# Orders

MoonProto keeps a retained order read model inside `MoonClient`. UI code reads
orders from snapshots, reacts to lightweight order events, and sends user
intents back through `client.orders()`. The application does not build raw
order packets and does not mutate the live order state directly.

## Reading Orders

```rust
if let Some(snapshot) = client.snapshot() {
    for order in snapshot.orders().iter() {
        redraw_order(order);
    }
}
```

For incremental UI updates, drain events and refresh only the affected rows:

```rust
use moonproto::Event;
use moonproto::state::OrderEvent;

for event in client.drain_events() {
    if let Event::Order(order_event) = event {
        match &order_event {
            OrderEvent::Removed(order) => {
                show_final_order_state(order);
                remove_order_from_ui(order.uid);
            }
            OrderEvent::Snapshot => {
                if let Some(state) = client.snapshot() {
                    redraw_all_orders(state.orders().iter());
                }
            }
            _ => {
                if let Some(order) = order_event.order() {
                    redraw_order(order);
                }
            }
        }
    }
}
```

`OrderEvent::order()` carries an Arc-backed order row for `Created`, `Updated`,
and `Removed`, so an event-driven UI does not lose the final terminal status if
the latest snapshot has already removed the order from the live list. UI code
that already redraws at its own frame rate can still ignore individual events
and read the latest snapshot each frame.

## Orders History

Use `client.orders().request_history("BTCUSDT")` when the terminal needs the
core to run its orders-history flow for a market, or
`request_history_for_market(&market)` when the UI already keeps a
`MarketHandle`.

This is a fire-and-forget UI intent. It does not return a paired snapshot over
MoonProto; the core owns the history/export side effect. Regular order tables
and chart overlays continue to read live retained orders from
`snapshot().orders()`.

## Closed Sell Reports

`Event::ClosedSellOrderReport` is deprecated. It remains a compatibility path
for existing consumers that already execute the core's expanded report SQL.
New application-owned report databases should use `MoonClient::reports()` and
`Event::Report`; see `reports.md` for the replication and migration contract.

When the core closes a sell order or later updates that report row, it can emit
`Event::ClosedSellOrderReport`. The payload is:

```rust
pub struct ClosedSellOrderReportEvent {
    pub db_id: i64,
    pub sql: String,
}
```

`db_id` is the MoonBot Orders database row id, not an order worker UID and not
the exchange order id. Use it as the stable key when mirroring the report DB:
the same closed sell can receive more SQL after price changes, partial fills,
or final execution, and those commands must update the same DB row.

`sql` is the same expanded SQL text built by the core for its own Orders report
writer. It is the exact insert/update that MoonBot wrote or would write for
that Orders row. Active Lib does not parse this SQL back into `Order` fields and
does not update retained orders from it; regular trading UI should keep using
`snapshot().orders()`. The report event is for external report/DB sync tools
that need the exact row operation the core wrote.

Do not combine this stream with typed report replication in one database.
`db_id` is not `newRecID`, and the two paths can report the same closed sell.

## Order Fields

The `bulk_replace_buy` / `bulk_replace_sell` indicators reflect the core's
replacement state, including a MoonHook waiting to rearm. When the local
five-second wait for a target confirmation expires, only that wait ends: the
affected indicator returns to the latest core value. The other side is unchanged.

`Order` is the user-facing retained order object. The most common fields are:

```rust
pub struct Order {
    pub uid: u64,
    pub market_name: String,
    pub currency: BaseCurrency,
    pub platform: ExchangeCode,
    pub status: OrderWorkerStatus,
    pub buy_order: ExchangeOrder,
    pub sell_order: ExchangeOrder,
    pub buy_price: f64,
    pub buy_size: f64,
    pub sell_price: f64,
    pub stops: StopSettings,
    pub vstop_on: bool,
    pub vstop_fixed: bool,
    pub vstop_level: f64,
    pub vstop_vol: f64,
    pub panic_sell: bool,
    pub panic_sell_auto: bool,
    pub is_moon_shot: bool,
    pub corridor_price_down: f32,
    pub corridor_price_up: f32,
    pub immune_for_clicks: bool,
    pub is_short: bool,
    pub pending_buy_cond_price: Option<f64>,
    pub sell_reason: SellReason,
    pub planned_sell_price: f64,
    pub use_market_stop: bool,
    pub strat_id: u64,
    pub emulator_mode: bool,
    pub pending_cancel: bool,
    pub bulk_replace_buy: bool,
    pub bulk_replace_sell: bool,
    pub buy_trace_line: Option<OrderTraceLine>,
    pub sell_trace_line: Option<OrderTraceLine>,
    pub job_is_done: bool,
}
```

`currency` and `platform` are typed session route metadata copied from the
successful Init/BaseCheck context when the retained row is created. They are
useful for grouping rows in multi-core terminals. Canonical v4 order packets do
not carry route bytes, and actions addressed to an existing order use its
server `uid` rather than these fields.

`buy_order` and `sell_order` are `ExchangeOrder` values. They contain
exchange-side order values such as
`actual_price`, `quantity`, `quantity_remaining`, `mean_price`, `leverage`, and
open/close/create times. `order_type` and `sub_type` are typed values; use
their `name()` helpers for labels. The local `buy_price` and `sell_price`
fields are UI replace drafts for already placed exchange legs, not exchange
execution prices. While an order is still pending, its canonical trigger is
`pending_buy_cond_price`.

Order timestamps are wire time values, but raw time fields are not the normal
terminal API. Use `open_time()`, `close_time()`, and `create_time()` on
`ExchangeOrder`.
For exchange-order flags, use `is_opened()`, `is_closed()`, `canceled()`, and
`is_short()` on `ExchangeOrder`; the underlying packed boolean bytes and
packed-record byte IO are wire details kept inside Active Lib/tests.

`sell_reason` is a typed `SellReason` value. Use
`order.sell_reason.description()` for a MoonBot-compatible UI label.

Report-database identity is intentionally absent from the live `Order` row. It
comes from typed report replication, whose row ids and lifecycle are separate
from the canonical live-order UID.

Field groups for terminal UI:

| UI area | Read from |
|---|---|
| order table identity/routing | `uid`, `market_name`, `currency`, `platform`, `strat_id`, `emulator_mode`, `is_short` |
| lifecycle/status columns | `status.name()`, `status.is_terminal()`, `job_is_done` |
| exchange-side buy/sell details | `buy_order`, `sell_order` (`actual_price`, `mean_price`, `quantity`, `quantity_remaining`, `leverage`, times) |
| local user intents | `buy_price`, `sell_price`, `pending_buy_cond_price`, `pending_cancel`, `bulk_replace_buy`, `bulk_replace_sell`, `immune_for_clicks`, `panic_sell` |
| stops/VStop editor | `stops`, `vstop_on`, `vstop_fixed`, `vstop_level`, `vstop_vol` |
| chart overlays | `is_moon_shot`, `corridor_price_down/up`, `buy_trace_line`, `sell_trace_line` |

## Status

```rust
pub struct OrderWorkerStatus;

impl OrderWorkerStatus {
    pub const None: Self;
    pub const BuyFail: Self;
    pub const BuySet: Self;
    pub const BuyCancel: Self;
    pub const BuyDone: Self;
    pub const SellFail: Self;
    pub const SellSet: Self;
    pub const SellCancel: Self;
    pub const SellDone: Self;
    pub const SellAlmostDone: Self;

    pub const fn name(self) -> &'static str;
    pub const fn is_known(self) -> bool;
    pub const fn is_terminal(self) -> bool;
}
```

`OrderWorkerStatus::is_terminal()` returns true for final states. Unknown future
status bytes are preserved instead of being rejected; normal terminal UI uses
the typed constants, `name()`, and `is_terminal()`.

## Actions

To set SL, trailing and TP at creation, use `NewOrderParams::with_stops(...)`
or `PendingOrderParams::with_stops(...)`. Unlike a later update, the initial
settings travel together with the order. See
[initial stops and core compatibility](trade_actions.md#strategy-selection-and-initial-stops).

Order actions go through `client.orders()`:

```rust
use moonproto::{StopSettings, VStopParams};

let Some(snapshot) = client.snapshot() else { return; };
let Some(order) = snapshot.orders().get(ui_state.selected_order_uid()) else { return; };
let stops = StopSettings::disabled()
    .with_stop_loss_percent(2.5, 0.1)
    .with_take_profit_price(50_500.0);

client.orders().move_order(order, new_price)?;
client.orders().cancel(order)?;
client.orders().update_stops(order, stops)?;
client.orders().update_vstop(
    order,
    VStopParams::percent(50_000.0, 12.0),
)?;
client.orders().turn_panic_sell(order, true)?;
client.orders().set_immune_for_orders([order], true)?;
client.orders().request_status(order)?;
```

Desktop UI should pass the visible `&Order` from its current snapshot. The
handle also accepts a raw UID for CLI tools and scripts, but in both cases the
runtime resolves the current live order state before sending. That live check is
important: pending-cancel, replace-in-flight, stop/VStop changes, panic-sell
flags, and click immunity are all stateful.

## Events

```rust
pub enum OrderEvent {
    Created(Arc<Order>),
    Updated(Arc<Order>),
    Removed(Arc<Order>),
    TracePoint { uid: u64 },
    CorridorChanged(u64),
    Snapshot,
}
```

`order()` returns the order row captured at the moment of `Created`, `Updated`,
or `Removed`. Use it in event-driven UI code: terminal cleanup and a real
application-world reset can move an order out of the live snapshot before the
application drains the async event queue. `changed_uid()` and `removed_uid()`
are still available for code that only needs identities. `TracePoint` and
`CorridorChanged` are lightweight chart signals; read the updated
trace/corridor fields from the current retained order. `Snapshot` means a
canonical snapshot or catalog page was applied and the UI should reconcile the
whole list.

Low-level ignored/not-applicable telemetry is available only in
`test`/`diagnostics` builds. Normal terminal code should redraw from retained
state instead of branching on internal apply-result reasons.

## Trace Lines

Server trace points are applied into `buy_trace_line` and `sell_trace_line`.
These fields are the chart-ready read model; the public order state does not
expose a raw inbound packet history. Long trace lines are shrunk with the same
800-line chart policy used by the MoonBot core.

For chart timestamps, use `OrderTraceChartPoint::time()` or `unix_millis()`.
When a sell trace carries a stop line, `OrderTraceLine::stop_price` and
`stop_time` give the price and time endpoint for that dotted stop segment.

These are live-order overlays, not a durable historical archive. To display a
closed report trade, use `client.reports().request_traces(report_uid)` and store
the result in the terminal. Report rows also carry optional
[entry placement time and saved corridor prices](reports.md#report-chart-fields).
See [archived order traces](reports.md#archived-order-traces)
for the request timing, inherited lines, rendering, and local-cache contract.

## Existing Core Order Lines

MoonBot 7.73 N4 can return the order lines that it currently retains on its
charts, including the original price moves. Request one market or all markets:

```rust
let one = client.orders().request_lines(Some("BTCUSDT"))?;
let all = client.orders().request_lines(None)?;
```

Both calls return immediately. Match the returned `OrderLinesTicket` with
`Event::OrderLines(OrderLinesEvent::Ready { ticket, lines })` or `Failed`.
The SDK handles page reordering and repeats and publishes only a complete
snapshot. An empty `Ready` is a valid empty result. Missing pages or a failed
request produce `Failed`, not a partial set of lines. Requests are independent;
there is no automatic subscription or background history download.

Each `OrderLine` has the core's `line_id`, its market/exchange/base
currency, buy/sell type, stop marker and full `points` in UTC milliseconds with
`f64` prices. `order_uid`, when present, links it to the live MoonProto order;
`native_order_id` is the core's separate order-leg ID. Match by `(order_uid, order_type)` when
combining snapshots with live order legs. Existing figures without a protocol
link still have their `line_id`. Points use the core's original
[order-line geometry](reports.md#geometry), including price moves.

Apply each complete snapshot as a replacement for the requested market, or for
all markets when requesting all lines. Do not append repeated snapshots or merge
by `line_id`: importing chart figures on the core can produce different geometries
with the same ID, and the snapshot preserves both.

The core collects the snapshot entirely on its main thread and passes only
immutable bytes to transport. Pages are bounded by the Sliced payload size;
a long line can cross page boundaries. The SDK reads the assembled pages without
first concatenating them into another full byte buffer.

This API reads existing chart objects. It does not create an additional history,
retain cancelled orders separately, or write an archive to disk. Availability is
whatever the core still has when it handles the request. Its normal chart cleanup
continues unchanged: in release builds it trims unprotected lines to 50 buys and
50 sells per market after the respective count exceeds 70. Working-order lines
are protected. There is no three-day or 500-order retention guarantee.

The application owns the returned immutable snapshot and decides when to refresh
or release it. Opening a chart is a suitable request point. These are chart data,
not trading notifications.

## Lifecycle Notes

The v4 order channel is a canonical replica. A complete order image and a
partial patch carry one monotonic state revision plus independent revision
clocks for 13 state sections: phase, flags, targets, exchange execution and
placement, stops, VStop, and planned sell state. MoonProto applies only
sections newer than the section already retained. A delayed patch can therefore
fill a missing section but cannot roll a newer stop, execution, or lifecycle
section back.

After Init and after every successful re-authorization/rebind, Active Lib
requests a full canonical order snapshot. Later snapshot/catalog pages provide
membership reconciliation.
A row whose revision/hash is incomplete is repaired with an addressed status
request; an explicit not-found response removes it. This repair is owned by the
runtime and requires no application polling. `PeerAppToken`, rather than a
transient transport token, defines the order world, so an ordinary soft rebind
keeps the same mirror while a real core-world change removes every old retained
row and starts a new world.

Local replace targets remain UI-owned while an action is in flight. A
same-phase server update does not overwrite that draft; a real phase change
materializes the new canonical target. The in-flight window closes when the
canonical state confirms the sent target or after a 5-second timeout; on
timeout the `bulk_replace_buy` / `bulk_replace_sell` markers clear and an
`Updated` event repaints the row.

Terminal order updates are removed after the current receive batch. Sell-done
orders keep a short grace window so immediately following visual trace packets
can still attach to the order. Recently completed UIDs are retained for the
current application world so a delayed snapshot/catalog page cannot resurrect
a removed terminal order.

## Protocol Data

The crate-internal order wire model and canonical reducer exist for tests and
packet replay. Regular applications should use `MoonClient`, snapshots, events,
and the `client.orders()` / `client.trade()` handles.
