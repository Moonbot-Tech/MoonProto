# Balances

The Active Lib keeps live account and position state for the connected MoonBot
session. Application code normally does not parse balance packets directly.

For UI attached to a market chart, read position fields from `Market`, not from
a separate balance row. Balance packets mutate the live `Market` state, and
chart/order UI reads that same retained object.

## What The Library Maintains

`MarketsState` stores per-market balance/position fields directly on each live
`Market`:

- `initial_balance`, `locked_balance`;
- `pos_size`, `pos_price`, `liq_price`, `pos_dir` (`OrderType::Sell` /
  `OrderType::Buy`);
- long/short hedge position fields;
- `asset_balance`, `asset_balance_full`;
- `total_profit_b`, `total_profit_l`, `total_profit_s`;
- `max_value`, `leverage_x`, `position_type` (`PositionType::Cross` /
  `PositionType::Isolated`).

`BalancesState` remains the account-level balance view:

- global account totals in BTC equivalent.

The core also publishes per-market session profit: the value reset by the
market-table **Reset Session** action and shown as `Session` in a chart header.
It is retained on each `MarketHandle`, separately from position PnL and the
global report-profit counters.

Transferable wallet assets are a different state model. They are not chart
position fields. Use `client.balances().refresh_transfer_assets()` and
`snapshot().transfer_assets()` for the Spot/Futures/Quarterly asset lists used
by transfer UI.

Incoming balance rows are applied only for markets already known to
`MarketsState`. An unknown market name does not create an orphan balance row.

Full snapshots are authoritative for the current known market universe. If a
known market is missing from a full snapshot, the library resets the live market
balance/position fields to zero and `leverage_x = 1`, while preserving
protocol bookkeeping such as stale-check hashes/epochs.

Incremental updates merge only the changed rows and optionally update global
totals. Stale incremental rows are skipped per market.

## Reading Current State

```rust
use moonproto::{OrderType, PositionType};

let Some(state) = client.snapshot() else { return; };
let markets = state.markets();

if let Some(eth) = markets.get("ETHUSDT") {
    let pos = eth.balance_position();
    let protection = state.position_protection_for(&eth);
    let side = if pos.pos_dir == OrderType::Buy { "long" } else { "short" };
    if pos.position_type == PositionType::Isolated {
        println!("isolated liquidation line at {}", pos.liq_price);
    }
    println!(
        "pos={} entry={} liq={} lev={}x",
        pos.pos_size,
        pos.pos_price,
        pos.liq_price,
        pos.leverage_x
    );
    if protection.both.has_warning {
        println!("position is not fully covered by active close orders");
    }
    println!("direction={side}");
}

let global = state.balances().global();
println!(
    "btc_total={} locked={} full={}",
    global.btc_balance_total,
    global.btc_balance_locked,
    global.btc_balance_full
);
```

The snapshot is immutable and safe for UI code to keep. `MarketHandle` values
are stable across listing refreshes, so a chart can keep the handle for the
selected market and read fresh fields from it. Use `MarketHandle::with` for
zero-copy reads of many market fields, or `MarketHandle::balance_position` when
the UI only needs the live balance/position subset.
Use `snapshot.position_protection_for(&market)` for the chart warning that
compares the retained position with active non-emulator close orders.

## Per-Market Session Profit

```rust
let Some(state) = client.snapshot() else { return; };
let Some(market) = state.markets().find("ETH") else { return; };

// Raw value in the core's base currency.
if let Some(base) = market.session_profit() {
    println!("session profit in base currency: {base}");
}

// The chart-ready value using the retained base-currency/USD rate.
if let Some(profit) = state.session_profit_for(&market) {
    println!("Session: {:.2}$", profit.usd);
}
```

`MarketHandle::session_profit()` returns `None` until the connected core has
published this state. After the first snapshot, a real zero is `Some(0.0)`.
This distinction lets applications remain compatible with older cores without
presenting unavailable data as zero.
`snapshot.session_profit_for(&market)` additionally returns `None` until the
retained base-currency/USD conversion rate is available; the raw base-currency
value remains readable from the market handle.

The wire update is an authoritative sparse snapshot: it carries only non-zero
markets, and every omitted known market becomes zero. A newly listed market
also starts at known zero after session-profit support has been observed.
`BalanceEvent::SessionProfitsApplied` tells push-driven UI that the retained
values are ready; applications do not merge packet rows themselves.

## Getting A Fresh Snapshot

Regular UI code calls `client.balances().refresh()` and reads
`snapshot().balances()` after
`Event::Balance`:

```rust
client.balances().refresh()?;

for event in client.drain_events() {
    if matches!(event, moonproto::Event::Balance(_)) {
        if let Some(snapshot) = client.snapshot() {
            // Account totals; per-market balance/position is read from markets.
            println!("total_pnl={}", snapshot.balances().global().total_pnl);
        }
    }
}
```

The next snapshot arrives through the normal `MoonClient` event/snapshot path.

## Events

```rust
use moonproto::{Event, state::BalanceEvent};

for event in client.drain_events() {
    match event {
        Event::Balance(BalanceEvent::SnapshotApplied { count, .. }) => {
            println!("full balance snapshot: rows={count}");
        }
        Event::Balance(BalanceEvent::IncrementalApplied {
            count,
            global_changed,
            ..
        }) => {
            println!("balance increment: rows={count} global={global_changed}");
        }
        Event::Balance(BalanceEvent::SessionProfitsApplied {
            nonzero_count,
            ..
        }) => {
            println!("session profit updated: non-zero markets={nonzero_count}");
        }
        _ => {}
    }
}
```

`SnapshotApplied.count` and `IncrementalApplied.count` are counts of rows that
actually changed the read model after market filtering and stale-epoch checks.
Stale/ignored packet notifications are hidden diagnostics; the read model
remains valid and UI code does not drive recovery from them.

## Transferable Assets

MoonBot refreshes Spot, Futures, and Quarterly transferable wallet assets as
three independent wallet requests. Active Lib exposes the same user effect
without blocking the caller:

```rust
use moonproto::{Event, ExchangeKind};

client.balances().refresh_transfer_assets()?;

for event in client.drain_events() {
    if let Event::TransferAssets(ev) = event {
        println!("transfer assets event: {ev:?}");
    }
}

if let Some(snapshot) = client.snapshot() {
    for asset in snapshot.transfer_assets().get(ExchangeKind::Futures) {
        println!(
            "{} transferable={} total={}",
            asset.currency,
            asset.amount,
            asset.total
        );
    }
}
```

`balances().refresh_transfer_assets()` queues one Spot/Futures/Quarterly refresh batch and
returns immediately. Repeated full-refresh calls within five seconds are coalesced into that
requested state and do not create more Engine API calls. Each completed response updates the
library-owned state and emits a
per-wallet `Event::TransferAssets::Updated` or `UpdateFailed`. After all three
requests have answered, Active Lib emits `TransferAssetsEvent::RefreshCompleted`;
that is the UI-safe point for a complete transfer-assets refresh. Use
`balances().refresh_transfer_assets_kind(kind)` if the UI only needs one wallet.

## Diagnostic Rows

Per-market `BalanceItem` rows are a protocol parser input, not the public
terminal state. Active Lib applies them to `Market` immediately, the same way
the MoonBot core mutates live market state. For selected-market UI, keep a
`MarketHandle` and read `balance_position()`.

When applying `max_value`, a zero or near-zero incoming value does not erase a
previously known non-zero value on the live market.

## GlobalBalance

```rust
pub struct GlobalBalance {
    pub btc_balance_total:    f64,
    pub btc_balance_locked:   f64,
    pub btc_balance_full:     f64,
    pub special_coin_balance: f64,
    pub total_pnl:            f64,
}
```

All amounts are BTC equivalents. `special_coin_balance` is the server-selected
special coin balance, for example USDT on futures servers. `total_pnl` is the
sum of per-market profit over BTC markets after the balance packet has already
updated live `Market` objects.

## API Shape

Normal UI state is:

- `snapshot().markets().get("ETHUSDT") -> MarketHandle`;
- `MarketHandle::balance_position()` for position, liquidation, leverage, PnL;
- `MarketHandle::session_profit()` or `snapshot().session_profit_for(&market)`
  for the per-market session counter;
- `snapshot().balances().global()` for account totals;
- `snapshot().transfer_assets()` for transferable wallet assets.

Raw balance packet rows stay internal to the parser/apply path and should stay
out of chart/order hot paths.
