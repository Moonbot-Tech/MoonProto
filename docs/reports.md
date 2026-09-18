# Report Database Replication

MoonProto can maintain an application-owned replica of the core's historical
`Orders` report database. Active Lib owns transport, schema decoding, retries,
hard-reconnect recovery, and typed row parsing. The application owns its
SQLite connection, migrations, transactions, retention policy, and durable
data.

This domain is separate from `snapshot().orders()`. The snapshot is the live
trading model used for tables, charts, and order actions. Report replication is
the durable historical database model. Archived order traces can also be
[requested on demand](#archived-order-traces); they are not part of bulk replication.
Report rows also expose [entry placement time and saved corridor prices](#report-chart-fields).

## Recommended Flow

Start from the checkpoint committed with the local replica:

```rust
use moonproto::{ReportHistoryDepth, ReportSyncRequest};

let ticket = if local_db_is_empty() {
    client.reports().sync(ReportSyncRequest::fresh(
        ReportHistoryDepth::ServerDefault,
    ))?
} else {
    client.reports().sync_from(load_report_checkpoint()?)?
};
```

The call returns immediately. If the schema is not known yet, Active Lib asks
for it first. Catch-up then advances one page at a time:

```rust
match event {
    moonproto::Event::Report(moonproto::ReportEvent::Schema(schema)) => {
        migrate_local_table(&schema)?;
    }
    moonproto::Event::Report(moonproto::ReportEvent::SyncPage(page)) => {
        let tx = db.transaction()?;
        upsert_page_with_one_prepared_statement(&tx, &page.rows)?;
        tx.commit()?;

        // This is the flow-control boundary. No next page is requested before it.
        client.reports().page_applied(&page)?;
    }
    moonproto::Event::Report(moonproto::ReportEvent::RowUpsert(row)) => {
        upsert_live_row(row)?;
    }
    moonproto::Event::Report(moonproto::ReportEvent::RowDelete { rec_id }) => {
        delete_local_row(rec_id)?;
    }
    moonproto::Event::Report(moonproto::ReportEvent::RowsDeleted(change)) => {
        set_local_deleted_flag(&change)?;
    }
    moonproto::Event::Report(moonproto::ReportEvent::SyncComplete(done)) => {
        // Catch-up is durable, but offline delete/retention state is reconciled next.
        pending_sync_complete = Some(done.clone());
        client.reports().reconcile_alive(&done)?;
    }
    moonproto::Event::Report(moonproto::ReportEvent::AliveMapComplete(map)) => {
        match map.outcome {
            moonproto::ReportAliveMapOutcome::Snapshot => {
                let done = pending_sync_complete.take().unwrap();
                let tx = db.transaction()?;
                apply_alive_map_as_visibility(&tx, &map)?;
                store_report_checkpoint(&tx, done.checkpoint())?;
                tx.commit()?;
            }
            moonproto::ReportAliveMapOutcome::DatabaseRecreated => {
                clear_local_replica_and_start_fresh_sync()?;
            }
        }
    }
    _ => {}
}
```

One request produces one page. Active Lib never requests the next page until
the application acknowledges the current one after its database transaction.
This keeps at most one catch-up page in flight per core and makes the database
writer the natural backpressure boundary.

`SyncComplete` is emitted only after the final page has been acknowledged. It
therefore describes durably applied catch-up, not merely parsed network data.
After it, reconcile older row visibility as described below and only then
advance the durable checkpoint.

`sync(...)` loads/revalidates the schema automatically. Use
`refresh_schema()` only for an explicit manual schema refresh.

## Page Contract

`ReportSyncPage` contains:

- `rows`: the complete typed page;
- `epoch`: stable identity of the core report database;
- `from_rec_id`: the cursor used for this page;
- `last_rec_id`: the last row in this page, or zero for an empty page;
- `max_rec_id`: the core database's persistent high-water, which does not move
  backwards after physical tail retention;
- `database_recreated`: the core is serving another report database, detected
  by its epoch or by the legacy high-water fallback;
- `is_complete()`: no further page is needed for this catch-up pass.

Pages are idempotent by `newRecID`. If the application cannot commit a page,
it must not call `page_applied`; the next page will not be requested.

A live upsert/delete can overtake a sliced page on UDP. Active Lib tracks live
IDs only for the current in-flight page and removes their older page copies, so
the application always applies the live value last without retaining a
whole-sync reconciliation set.

When `database_recreated` is true, discard the stale local replica and then
call `page_applied`. Active Lib restarts the same operation from a fresh cursor.
This is detected by the persisted database epoch even when the replacement
database has already grown beyond the old numeric cursor.

Missing page responses are retried automatically. A retry repeats only the
current page, not the complete history, and keeps that page request's wire UID.
Therefore a delayed response to an earlier transmission of the same page remains
valid instead of being invalidated by the retry itself.

## Soft-Delete And Restore

Report rows are hidden by setting their `deleted` column; this does not
physically remove them. Address rows by inclusive `newRecID` ranges and/or
individual IDs:

```rust
use moonproto::ReportRecIdRange;

let batches = client.reports().delete_rows(
    &[ReportRecIdRange::new(first_rec_id, last_rec_id)],
    &selected_rec_ids,
)?;
```

`restore_rows` performs the same operation with `deleted=0`. Active Lib splits
large selections into Sliced commands near 1 KiB and returns the number
of non-empty batches. An empty selection returns zero and sends nothing.
Reversed ranges are preserved and select no rows, matching the core's SQL
`BETWEEN` semantics.

After committing a batch to its report database, the core broadcasts
`ReportEvent::RowsDeleted` to every report subscriber, including the sender.
Apply that event as the equivalent local SQLite `UPDATE`; rows absent from the
local replica are a no-op. One echo is expected per batch and may lag by about
three seconds plus transport time. If an echo does not arrive, the same
idempotent operation can be sent again. An older core without this operation
does not echo it.

`set_rows_deleted(...)` is the shared primitive behind `delete_rows(...)` and
`restore_rows(...)`; normal UI code should prefer the named operations.

Feed all `ReportEvent` values through one serialized database writer in delivery
order. During catch-up, Active Lib also overlays committed soft-delete echoes on
older rows from later sync pages. Rows already delivered to the application are
kept correct by applying the event before subsequent queued report work.

The application can hide `deleted=1` rows by default and offer an explicit
"show deleted" view. Per-row physical removals reported live arrive as
`RowDelete`; bulk retention cleanup may be visible only through the alive map.
Physical deletion cannot be requested through this API.

## Offline Visibility Reconciliation

Normal catch-up advances by `newRecID`, so it cannot discover a soft-delete,
restore, or physical retention delete of an older row that happened while the
terminal was offline. After each `SyncComplete`, request the core's compact
alive map:

```rust
client.reports().reconcile_alive(&sync_complete)?;

if let moonproto::ReportEvent::AliveMapComplete(map) = event {
    match map.outcome {
        moonproto::ReportAliveMapOutcome::Snapshot => {
            let tx = db.transaction()?;
            for rec_id in local_report_ids_up_to(&tx, map.covered_up_to)? {
                // A clear bit combines soft-delete and physical absence.
                // Preserve the local row but hide it; a later restore/upsert can revive it.
                set_local_deleted(&tx, rec_id, !map.is_alive(rec_id).unwrap())?;
            }
            store_report_checkpoint(&tx, sync_complete.checkpoint())?;
            tx.commit()?;
        }
        moonproto::ReportAliveMapOutcome::DatabaseRecreated => {
            clear_local_replica_and_start_fresh_sync()?;
        }
    }
}
```

`Snapshot` is authoritative for `newRecID=1..=covered_up_to`. A set bit means
the row exists on the core and has `deleted=0`; a clear bit means the row is
soft-deleted or physically absent. `is_alive(rec_id)` reads one bit in O(1).
Rows outside the covered range return `None`.

Persist `ReportSyncComplete::checkpoint()` in the same transaction that applies
the map. It contains both the database epoch and the next numeric cursor. If the
transaction fails, retain the previous checkpoint and repeat catch-up. Starting
with `sync_from(checkpoint)` makes database replacement detectable even when the
new database has already reused or exceeded old numeric IDs.

Active Lib retries a lost response with the same request UID and repeats the
request after a hard reconnect. Live upserts, physical deletes, and
`RowsDeleted` echoes received while the Sliced map is in flight are overlaid on
the map before `AliveMapComplete`, so one serialized report writer can apply
events in delivery order without another race-recovery layer.

## Open Rows After Reconnect

Report rows are not fully append-only. An open deal can close, change, or be
physically removed while the client is offline, even though its `newRecID` is
below the committed cursor. Keep the current open-row IDs registered with
Active Lib:

```rust
client.reports().check_open_rows(&open_rec_ids)?;
```

The library sorts and deduplicates the IDs, keeps the newest 100, sends an
addressed check, and retains that set for hard-reconnect recovery. Results use
the normal `RowUpsert` and `RowDelete` events. `OpenRowsCheckComplete` means one
authoritative result was received for every retained ID.

Call `check_open_rows` again when the local set changes. Passing an empty slice
clears the retained check intent. Closed rows are not rechecked: they are
stable apart from accepted cosmetic edits.

## Schema And SQLite

`ReportSchema` is append-only: existing field indices, names, kinds, and SQLite
declarations are stable; new fields extend the tail. Create missing columns,
never infer wire indices from a locally guessed column order.

```rust
let create = schema.sqlite_create_table_sql("Orders");
let add = schema.sqlite_add_column_sql("Orders", field);
let index = schema.sqlite_unique_index_sql("Orders");
```

`newRecID` is the immutable row address inside one core report database. Use it
for replication cursors, upserts, soft-delete/restore commands, and physical
delete events. It is different from an active order UID, exchange order id,
and the legacy report `db_id`.

`ReportUID` is the immutable 64-bit identity of the report row itself. It is
preserved when a MoonBot database is copied, so an application aggregating
several cores can recognize the shared historical rows without confusing their
per-database `newRecID` values. Rows created independently after the copy have
independent `ReportUID` values. The value is carried as an `i64`; negative
values are valid and must not be rejected or truncated.

Resolve optional fields once for each received schema revision and cache their
indices. Do not call `field_by_name` for every row:

```rust
use moonproto::{ReportFieldKind, ReportValue};

let report_uid_index = schema
    .field_by_name("ReportUID")
    .filter(|field| field.kind == ReportFieldKind::Integer)
    .map(|field| field.index);

let report_uid = report_uid_index.and_then(|index| match row.value(index) {
    Some(ReportValue::Integer(value)) => Some(*value),
    _ => None,
});
```

The schema is append-only, so a discovered index remains stable; refresh the
cache when a new `ReportEvent::Schema` revision arrives. A missing field means
that the connected core does not provide this identity. Never substitute
`ReportUID` for `newRecID` in replication or mutation APIs.

MoonProto does not own or rewrite the application's SQLite database. If
`ReportUID` is added to an existing local replica, previously stored rows keep
their local default until the application receives those rows again. An
application that needs historical cross-core deduplication can perform a
one-time fresh sync; until then, missing or placeholder values are not usable
as shared identity.

The current schema is also available from `snapshot().report_schema()`.

For each page, use one SQLite transaction and reuse one prepared upsert
statement. Preparing SQL for every row can turn the local writer into the
bottleneck that page-level flow control is designed to avoid.

## Report Chart Fields

Newer cores append three optional columns to `ReportSchema`. They arrive through
the existing `RowUpsert` and `SyncPage` events; no extra subscription or request
is needed. Migrate the application's table on `ReportEvent::Schema` before
writing rows.

| Field | Value | Meaning |
| --- | --- | --- |
| `BuySetDateMs` | `Integer(i64)` | Entry order creation time in milliseconds, not the entry fill time (`BuyDateMs`). Uses the [core's report clock](#report-timestamps), not necessarily UTC. |
| `BuyCorridorDown` | `Float(f64)` | Saved absolute price for the entry corridor's DOWN replacement condition. |
| `BuyCorridorUp` | `Float(f64)` | Saved absolute price for the entry corridor's UP replacement condition. |

The corridor is the last saved entry-side state, not a time series and not
percent offsets from `BuyPrice`. It covers MoonShot and managed MoonHook
corridors when available; joined sells have no single corridor and carry zeros.
Do not assume `Down <= Up`: the names describe replacement conditions, not
numeric sorting. To shade a band, use the minimum and maximum of two positive
prices while preserving their original names in storage.

Resolve and cache the indices once per schema revision, then read typed values:

```rust
// On Schema; None means the core does not expose this field.
let corridor_down_index = schema
    .field_by_name("BuyCorridorDown")
    .filter(|field| field.kind == moonproto::ReportFieldKind::Float)
    .map(|field| field.index);

// On RowUpsert or for each SyncPage row.
let corridor_down = corridor_down_index.and_then(|index| match row.value(index) {
    Some(moonproto::ReportValue::Float(price)) if *price > 0.0 => Some(*price),
    _ => None,
});
```

Use the same pattern for `BuyCorridorUp` and `Integer` for `BuySetDateMs`.
Zero or an absent value means unavailable, not a zero-price line or an epoch
date. These three columns default to zero, including pre-upgrade history and
synthetic rows; old cores lack the columns entirely. History is not backfilled.
The creation time does not reconstruct earlier limit prices or moves: use
[archived traces](#archived-order-traces) for the actual saved order path.

## Archived Order Traces

Use `client.reports().request_traces(report_uid)` to fetch the saved buy/sell
geometry of a **closed report trade**, including available traces inherited
through join/split. This works even when the terminal was offline during the
trade. It does not require the order to remain in `snapshot().orders()`.

**Recommended terminal behavior:** plan local persistent storage for these
traces, keyed by the report row's `ReportUID`. When the user opens a trade chart,
show saved traces immediately; request them only if they have not been saved.
Do not fetch traces for every report row during initialization or catch-up.
`ReportUID` is an `i64`, including negative values; neither `newRecID` nor a live
order ID is a substitute. See [Schema And SQLite](#schema-and-sqlite) for reading it.

### When To Request

Wait until the **report replica** contains a closed row (`CloseDate != 0`). The
core writes the final trace archive before broadcasting that row's `RowUpsert`.
The live order mirror can show completion several seconds earlier, while the
report database is still waiting for its normal write drain. An open report row
already has a `ReportUID`, but its final archive may not exist yet.

If an early request returns empty, do not retry after an arbitrary delay. Wait
for `ReportEvent::RowUpsert` with the same `ReportUID` and nonzero `CloseDate`,
discard the early "unavailable" result, and request again if that chart is still
open and has no saved traces. If the closed row arrived before the early empty
reply, retry after that reply instead: the required row is already available.
Track whether the original request used an open or closed row. No polling timer
is needed.

### Request And Result

```rust
// Called when the user opens a closed report trade without locally saved traces.
let ticket = client.reports().request_traces(report_uid)?;

match event {
    moonproto::Event::Report(moonproto::ReportEvent::TraceReady { ticket, traces }) => {
        if traces.is_empty() {
            show_traces_unavailable(ticket.report_uid);
        } else {
            // App-owned storage: preserve line order, own/type, stop marker,
            // and every point's Unix-millisecond time and f64 price.
            store_report_traces(ticket.report_uid, &traces)?;
            display_report_traces(ticket.report_uid, &traces);
        }
    }
    moonproto::Event::Report(moonproto::ReportEvent::TraceFailed { ticket, error }) => {
        show_trace_request_error(ticket.report_uid, &error);
        // A failed request is not evidence of an empty archive.
    }
    _ => {}
}
```

The call returns immediately; match completion with `ticket.request_id`.
Different trades can be requested concurrently and can complete out of order.
Repeated requests for the same trade share one in-flight network request;
each ticket gets its own result, sharing the same immutable trace allocation.
Requests made during initial connection wait for initialization. Once submitted
to the transport, the normal request timeout is 12 seconds; transport retries
are automatic, but the library does not poll or retain a persistent trace cache.

An empty `TraceReady` means the core returned an empty archive. After the closed
report update, no timed retry is needed for normal archive writing: it has
already happened. Old trades, unavailable chart figures, archive retention,
or trades recorded without archive support may have no traces. The core also
uses the same empty answer for archive-storage failures and can backfill older
charts during startup, so do not interpret it as an irrevocable "never existed"
fact. Allow a user-requested refresh rather than continuous retries.
An older core without this feature does not answer; that ends in `TraceFailed`,
not an empty `TraceReady`.

### Geometry

Each `ReportTrace` contains `own`, `order_type`, `stop_price`, `stop_time`, and
`points`. `own=false` identifies inherited geometry; there can be multiple
lines of the same type. A bounded core archive may omit some inherited lines,
so the result is the saved geometry, not a complete ancestry graph.

Point and stop times are already **Unix UTC milliseconds** (`MoonTime`). Unlike
the report date columns below, they need no core-timezone or ping-offset
correction. Prices are `f64`; do not downcast them when saving the archive.
Zero point times are unset coordinates, not dates to plot at the Unix epoch.

Points are the core's chart geometry, not successive live trace messages. Do
not feed them through a live-point append/simplification algorithm. For each
group `p0=points[k]`, `p1=points[k+1]`, `p2=points[k+2]`, `p3=points[k+3]`, draw
the order path `p0 -> p1 -> p3` and the auxiliary vertical segment from
`(p2.time, p1.price)` to `p2`; then advance `k` by three. Skip unset coordinates.
With a positive `stop_price` and nonzero `stop_time`, the dotted stop segment
runs from the first point's time to `stop_time`, at `stop_price`.

## Report Timestamps

The optional integer columns `BuyDateMs`, `SellSetDateMs`, and `CloseDateMs`
preserve milliseconds from the same source times as `BuyDate`, `SellSetDate`,
and `CloseDate`. The existing columns remain whole seconds.

For ordinary trade rows:

| Field | Meaning |
| --- | --- |
| `BuySetDateMs` | Creation time of the entry order; zero means unavailable. No seconds counterpart. |
| `BuyDateMs` | Time MoonBot records the entry as completed. |
| `SellSetDateMs` | Creation time of the exit order, not its execution time. |
| `CloseDateMs` | Exit order close time; zero means the report row is still open. |

Entry/exit meanings also apply to short positions. Funding and synthetic report
rows use their report-creation times for `BuyDateMs`, `SellSetDateMs`, and
`CloseDateMs`; `BuySetDateMs` remains zero. These fields retain MoonBot's report
semantics; they do not describe individual partial fills.

**Clock:** all report date columns, including `BuySetDateMs`, use the core's report clock, with no timezone
normalization during replication. They encode the core's local date/time
relative to the Unix epoch, not necessarily UTC. Reuse the same core-timezone
conversion as for the existing report dates, exactly once. With a UTC-configured
core, the millisecond values can be used directly as Unix UTC milliseconds.
Otherwise, convert using the core's timezone before constructing a `MoonTime`
or placing a marker on a UTC chart; do not use the terminal's timezone instead.

Resolve each optional column when the schema arrives and cache its index:

```rust
let buy_date_ms_index = schema
    .field_by_name("BuyDateMs")
    .filter(|field| field.kind == moonproto::ReportFieldKind::Integer)
    .map(|field| field.index);

// Per row: the value is still in the core's report clock.
let buy_date_ms = buy_date_ms_index.and_then(|index| match row.value(index) {
    Some(moonproto::ReportValue::Integer(value)) => Some(*value),
    _ => None,
});
```

Old cores lack these columns. For `BuyDateMs`, `SellSetDateMs`, and
`CloseDateMs`, existing historical rows are not backfilled:
their new columns are SQL `NULL`, omitted from the received row, and read as
`None`. Preserve that absence in the local replica. Prefer a present
millisecond value; otherwise use the corresponding seconds value as a
second-resolution fallback, not as a millisecond-accurate execution time.
Handle each column independently, and never interpret `CloseDateMs=0` as an
epoch-date marker.

## Reconnect And Checkpoint

Report subscription belongs to the hard server session. Active Lib tracks the
server session token. After a hard reconnect it resumes from the last page that
the application acknowledged and repeats the retained open-row check. A soft
network rebind keeps the server session and does not cause a false resync.
The append-only schema is revalidated once per new hard session before page or
check traffic resumes, so newly appended fields are migrated before their rows
are applied.

The durable checkpoint is `{ epoch, next_from_rec_id }`, where the numeric
cursor is the core's persistent high-water plus one. Never advance it merely
because a page arrived. Commit pages first, finish the alive-map reconciliation,
then store the checkpoint in the same local transaction as the visibility state.

For an empty replica, `ReportHistoryDepth::ServerDefault` uses the core's
default retained depth, `Days(n)` requests an explicit depth, and `All`
requests all retained history. History depth applies only to a fresh cursor.

## Legacy SQL Event

`Event::ClosedSellOrderReport` remains only for compatibility with existing
consumers of the expanded SQL stream. It has no schema negotiation, initial
history, offline catch-up, or reconnect recovery. New report databases should
use `Event::Report` only, and the two streams must not write into the same
replica.
