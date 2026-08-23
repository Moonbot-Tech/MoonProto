# Report Database Replication

MoonProto can maintain an application-owned replica of the core's historical
`Orders` report database. Active Lib owns transport, schema decoding, retries,
hard-reconnect recovery, and typed row parsing. The application owns its
SQLite connection, migrations, transactions, retention policy, and durable
data.

This domain is separate from `snapshot().orders()`. The snapshot is the live
trading model used for tables, charts, and order actions. Report replication is
the durable historical database model.

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
