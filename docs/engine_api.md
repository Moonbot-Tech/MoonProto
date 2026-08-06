# Engine API

Engine API actions are long-running server operations exposed through the
non-blocking Active Lib runtime. Applications queue intents on `MoonClient`; the
runtime keeps the session alive, parses responses, updates retained state, and
emits typed events.

## Public Shape

Use these non-blocking calls from UI/application code:

| Need | Call | Read result from |
|---|---|---|
| Balances/positions | `balances().refresh()` | `Event::Balance`, `snapshot().balances()`, `snapshot().markets()` |
| Hedge mode | `account().refresh_hedge_mode()` | `Event::Account`, `snapshot().account().hedge_mode()` |
| API-key expiration | `account().refresh_api_expiration_time()` | `Event::Account`, `snapshot().account().api_expiration()` |
| Transferable assets | `balances().refresh_transfer_assets()` / `balances().refresh_transfer_assets_kind(kind)` | `Event::TransferAssets`, `snapshot().transfer_assets()` |
| Orders snapshot | `orders().request_snapshot()` | order events, `snapshot().orders()` |
| UI settings | `settings().refresh()` | `Event::Settings`, `snapshot().settings()` |
| CoinCard candles | `candles().request_coin_card_for(&market, kind)` | `Event::CoinCardCandles`, `snapshot().coin_card_candles_for(&market, kind)` |
| Account/exchange mutations | `account()`, `balances()`, and `streams()` typed action helpers | `Event::EngineAction` and normal retained state updates |

Example:

```rust
client.account().refresh_hedge_mode()?;
client.account().refresh_api_expiration_time()?;
client.balances().refresh_transfer_assets()?;
client.balances().refresh()?;

for event in client.drain_events() {
    println!("event={event:?}");
}

if let Some(snapshot) = client.snapshot() {
    println!("hedge={:?}", snapshot.account().hedge_mode());
    println!("total_pnl={}", snapshot.balances().global().total_pnl);
}
```

`snapshot().account().api_expiration()` returns `ApiExpirationTime`: use
`time()`, `system_time()`, or `days_until(now)` for UI labels instead of
carrying legacy wire-time doubles through the UI. Current cores report the
remaining duration, so the normalized expiration is not shifted when the core
and terminal use different time zones. `reported_days_left()` exposes the
core's rounded value for the refresh; `is_known() == false` after a successful
refresh means that the key has no reported expiration.

## Account And Exchange Actions

| User action | API |
|---|---|
| Set leverage for the selected market | `account().set_leverage_for(&market, leverage)` |
| Switch account hedge mode | `account().set_hedge_mode(enabled)` |
| Switch one market between cross and isolated mode | `account().change_position_type_for(&market, position_type)` |
| Confirm a pending exchange risk-limit/MMR change | `account().confirm_risk_limit_for(&market)` |
| Toggle multi-assets/union margin mode where supported | `account().set_ma_mode(enabled)` |
| Cancel every exchange order through the connected core | `account().cancel_all_orders()` |
| Transfer an asset between Spot/Futures/Quarterly wallets | `balances().transfer_asset(...)` |
| Convert supported dust balances to BNB | `balances().convert_dust_bnb()` |
| Force an Engine API orderbook reload | `streams().reload_order_book()` |

These methods queue work and return an `EngineActionTicket`; they do not mean
the exchange accepted the action. Use the matching `Event::EngineAction` and
retained state update as the result.

## Candles

There are two user-facing candle paths:

- retained 5m candles: after trades storage is enabled, Active Lib requests the
  initial full 5m snapshot for that storage scope, retries lost/stuck chunked
  requests by event+timeout, applies the successful snapshot to market history,
  emits `Event::CandlesSnapshot`, then keeps candles current from trades;
- CoinCard/deep-history candles: call
  `candles().request_coin_card_for(&market, kind)` for the selected retained
  market, wait for `Event::CoinCardCandles`, then read
  `snapshot().coin_card_candles_for(&market, kind)`. The string-keyed request
  remains available for scripts/tools.

Normal chart UI does not receive raw zipped chunk payloads.

## Transfer Assets

`balances().refresh_transfer_assets()` queues one Spot/Futures/Quarterly wallet refresh batch
without blocking other runtime work. Repeated full-refresh calls within five seconds are
coalesced and do not multiply Engine API traffic. Each wallet response updates
`snapshot().transfer_assets()` and emits `TransferAssetsEvent::Updated`; after
all requested wallet kinds answer, Active Lib emits
`TransferAssetsEvent::RefreshCompleted`.

```rust
use moonproto::ExchangeKind;

client.balances().refresh_transfer_assets()?;

if let Some(snapshot) = client.snapshot() {
    for asset in snapshot.transfer_assets().get(ExchangeKind::Futures) {
        println!("{} transferable={} total={}", asset.currency, asset.amount, asset.total);
    }
}
```

## Low-Level Internals

Raw request builders, request UIDs, Engine API methods, and protocol receivers
are internal Active Lib/diagnostic machinery. Public code should not wait on raw
`Receiver<EngineResponse>` or pick a protocol-loop duration; the application API
is the owned `MoonClient` runtime plus events/snapshots.
