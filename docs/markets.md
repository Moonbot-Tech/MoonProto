# Markets

`MoonClient` maintains the market universe and live market read model for the
application. UI code searches by symbol once, keeps a stable `MarketHandle`, and
reads prices, funding, tags, balances/positions, arbitrage slots, and retained
history from snapshots/handles.

The runtime owns the server refreshes that feed this state: full market list,
incremental price/funding updates, token tags, correlation prices, and
server-index refresh after reconnect/server restart. Applications should read
the maintained state and events from `MoonClient` snapshots/events, not parse
market payloads or server indexes themselves.

## Reading State

`MarketsState::find(input)` / `search(input, limit)` are the normal terminal
search-box helpers: users may type a full market name (`BTCUSDT`) or a coin
symbol (`BTC`, `SOL`). The result is a stable `MarketHandle`, not a temporary
borrow. `MarketsState::get(name)` remains the exact-name path for code that
already has the canonical name.

This follows the production core's stable-market-object model: listing refresh
may replace the surrounding list/dictionaries, but existing market objects stay
alive and are mutated in place. UI code may keep the handle after a search and
read it later without re-searching by name.

```rust
use moonproto::TokenTags;

let Some(state) = client.snapshot() else { return; };
let markets = state.markets();

if let Some(market) = markets.find("BTC") {
    let pos = market.balance_position();
    let price = market.price();
    let tail = market.trade_state();
    let deltas = market.delta_state();
    let max_pos = market.max_pos_limit();
    let protection = state.position_protection_for(&market);
    market.with(|market| {
        println!(
            "tick={} max_lev={} max_order={}",
            market.tick_size(),
            market.max_leverage,
            market.max_order_value()
        );
    });
    println!(
        "liq={} bid={} ask={} mark={} last_trade={} coin1h={} max_pos={} protected={}",
        pos.liq_price,
        price.bid,
        price.ask,
        price.mark_price,
        tail.last_trade_price,
        deltas.coin_1h_delta,
        max_pos,
        !protection.both.has_warning
    );
}

let global_deltas = markets.global_deltas();
println!("btc1h={} exchange1h={}", global_deltas.btc_1h_delta, global_deltas.exchange_1h_delta);

let tags = markets.tags("BTCUSDT");
if tags.contains(TokenTags::ALPHA) {
    println!("BTCUSDT has ALPHA tag");
}
```

Balance and position packets update these same live `Market` objects. For chart
UI this is the normal path: keep the selected `MarketHandle` and read fields
such as `pos_size`, `pos_price`, `liq_price`, `leverage_x`, `asset_balance`,
`total_profit_*`, and `max_value` from `balance_position()`. `BalancesState` is the account
totals view, not the primary per-market UI object.

For chart overlays that only need position fields, `MarketHandle::balance_position`
returns a small copy without cloning the whole market object.
For the "unprotected position" warning, use
`snapshot.position_protection_for(&market)`: the library counts active
non-emulator `SellSet` close orders by side, and the UI only decides how to
draw/blink that warning.
For price/funding/mark-price and live trade-tail overlays, use
`MarketHandle::price()` and `MarketHandle::trade_state()` on the same retained
handle instead of resolving the market name again.
For signed MoonBot signal deltas, use `MarketHandle::delta_state()` for the
selected market and `MarketsState::global_deltas()` for BTC/exchange signals.
These are separate from retained-history range/max-move analytics. If the UI
wants "Exclude blacklisted markets from the market delta calculation"
checkbox, call
`client.settings().set_exclude_blacklisted_markets_from_exchange_delta(true)`;
the runtime then applies `coins_black_list_text` to retained markets before
computing `Exchange1hDelta` / `Exchange24hDelta`.

Markets-table style values are also read from the retained market handle:

- `market.with(|m| m.max_order_value())` is the `Max.Order` column: exchange
  `max_qty` converted through the current ask price.
- `market.max_pos_limit()` is the per-market `MaxPos` value derived from the
  latest leverage-management config. `0` means there is no explicit/wildcard
  per-market rule; the global `def` fallback remains available through
  `snapshot.settings().lev_manage.as_ref().map(|l| l.default_max_pos_limit())`.

Arbitrage relay packets also apply to the live market. Use
`MarketHandle::arb_slot(ArbPlatformCode::...)` or
`arb_now(ArbPlatformCode::...)` from the
selected handle; raw arb `market_index` blocks are diagnostic protocol details.
Arb price entries expose `time()` / `unix_millis()` helpers; the fixed ring
cursor is diagnostics/test-only.

## Init and Refresh

Initial fetch:

```rust
use moonproto::{ConnectConfig, InitConfig, MoonClient};

let init = InitConfig {
    ..Default::default()
};
let client = MoonClient::connect(cfg, ConnectConfig::new(init))?;
```

Long-running price refresh is controlled by `ClientConfig.refresh`. The default
uses the MoonBot core worker cadence, but ticks are gated by Init: transport `Fine`
does not start background Engine API. Set `update_markets_every` /
`check_tags_every` to `None` if the application owns those requests manually.

See `examples/market_refresh.rs` for a compact consumer-side loop that reads
prices and tags from `MoonClient`.

## Events

```rust
pub enum MarketsEvent {
    // Historical name: emitted when a GetMarketsList response was applied.
    MarketsListReplaced { count: usize, corr_count: usize },
    NewMarketsAdded { names: Vec<String> },
    PricesUpdated { count: usize, included_funding: bool, included_corr: bool },
    IndexesUpdated { count: usize },
    TokenTagsUpdated { count: usize },
}
```

## Public State

`MarketsState` is a read API over the live market catalog. Its internal COW
maps/lists and server-index helpers are not the terminal surface. Normal UI code
uses `iter()`, `get() -> MarketHandle`, `market_snapshot(name)`, `price(name)`,
`tags(name)`, `trade_state(name)`, `delta_state(name)`, `global_deltas()`,
`exclude_blacklisted_markets_from_exchange_delta()`, and the count helpers.
Selected-market UI should keep the `MarketHandle` returned by `get()` and read
through that handle.

```rust
pub struct MarketPrice {
    pub bid: f64,
    pub ask: f64,
    pub last_bid: f64,
    pub last_ask: f64,
    pub p_last: f64,
    pub min_lot_size: f64,
    pub chart_price_step: f64,
    pub funding_rate: f64,
    pub mark_price: f64,
    pub mark_price_found: bool,
}

impl MarketPrice {
    pub fn funding_time(self) -> MoonTime;
}
```

```rust
pub struct BaseCurrencyPrice {
    pub base_currency: String,
    pub last_price: f64,
    pub usdt_market: Option<String>,
    pub usdt_rev_market: Option<String>,
    pub usdt_corr_market: Option<String>,
    pub usdt_rev_corr_market: Option<String>,
}
```

```rust
pub struct MarketTradeState {
    pub last_got_all_trades_ms: i64,
    pub last_got_spot_trades_ms: i64,
    pub last_trade_price: f64,
    pub last_buy_price: f64,
    pub last_sell_price: f64,
    pub last_trade_price_ema15: f64,
    pub last_trade_price_ema5: f64,
    pub last_trade_was_sell: bool,
}
```

```rust
pub struct MarketDeltaState {
    pub last_price_ema: f64,
    pub coin_1h_avg: f64,
    pub coin_24h_avg: f64,
    pub coin_1h_delta: f64,
    pub coin_1h_delta_ema: f64,
    pub coin_24h_delta: f64,
    pub coin_24h_delta_ema: f64,
}

pub struct MarketGlobalDeltas {
    pub btc_1h_avg: f64,
    pub btc_24h_avg: f64,
    pub btc_72h_avg: f64,
    pub btc_1h_delta: f64,
    pub btc_24h_delta: f64,
    pub btc_72h_delta: f64,
    pub exchange_1h_delta: f64,
    pub exchange_24h_delta: f64,
    pub exchange_market_count: usize,
}
```

The retained LastPrice line row is:

```rust
let price = point.price();
let unix_ms = point.unix_millis();
let time = point.time();
```

UI code should use `price()`, `time()`, or `unix_millis()` instead of carrying
raw protocol time.

This row is the retained LastPrice chart line, not the last trade price. It is
filled from `UpdateMarketsList`: the server sends `Bid/Ask`, the client
computes `pLast = (Bid + Ask) / 2`, and the chart line is drawn from that
retained price history.

The retained-history worker appends a `LastPricePoint` only when the production
core would add a price-history row: `pLast > 0`, bid or ask is present, and the
market is a BTC market or a base-USDT market.

The retained MarkPrice line row has the same shape:

```rust
let mark_price = point.price();
let mark_time = point.time();
```

It is filled from `UpdateMarketsList -> MarketPrice.mark_price` when the server
marks the value as present. UI code can compare the MarkPrice line with the
LastPrice line for the same market; both are retained in the same per-market
history model.

When trades retained storage is active, `MoonClient` appends these rows
immediately after applying market prices. Retained history is created lazily
from the active trades subscription scope, so markets outside
`subscribe_trades_for` do not allocate price-line rings.

`Market::futures_type` uses `BaseCurrency`, a small public wrapper that
preserves unknown future server values:

```rust
pub struct BaseCurrency;

BaseCurrency::BTC;
BaseCurrency::USDT;
BaseCurrency::USDC;
BaseCurrency::EMPTY;
BaseCurrency::UNKNOWN;

let label = market.futures_type.name();
```

Known constants cover the currently named server values. Unknown future values
are preserved as their original byte instead of being collapsed to
`BaseCurrency::UNKNOWN`. For older servers that do not provide this field,
`Market::futures_type` is `BaseCurrency::EMPTY`.
Use `BaseCurrency::name()` for UI labels.

`Market::listed_type()` returns the core post-processing result for
`GetMarketsList`: `BaseCurrency::EMPTY` means
`ListedType::SPOT`; any other `futures_type` means `ListedType::BOTH`.
`ListedType` is a public ordinal wrapper for the derived listing kind.

Convenience methods:

```rust
let Some(state) = client.snapshot() else { return; };
let markets = state.markets();

for handle in markets.iter() {
    handle.with(|market| {
        println!("{} {}", market.symbol(), market.status_trading);
    });
}

let btc = markets.get("BTCUSDT"); // Option<MarketHandle>
let btc_snapshot = markets.market_snapshot("BTCUSDT");
markets.price("BTCUSDT");
markets.ref_btc_corr_market("DOGEUSDT");
markets.base_currency_price("BTC");
markets.trade_state("BTCUSDT");
markets.delta_state("BTCUSDT");
markets.global_deltas();
markets.tags("BTCUSDT");
markets.market_count();
markets.corr_count();
```

Server-index mapping is runtime/diagnostic protocol state. Normal UI code keeps
a `MarketHandle` or reads by market name. In the normal `MoonClient` path,
trades and orderbook events are gated until fresh indexes are rebuilt by
cold-init `GetMarketsList` or refreshed through `GetMarketsIndexes` after
reconnect/server restart.

## TokenTags

```rust
pub struct TokenTags;

TokenTags::MONITORING;
TokenTags::FAN;
TokenTags::SEED;
TokenTags::LAUNCH;
TokenTags::GAMING;
TokenTags::NEW;
TokenTags::OLD;
TokenTags::BNB;
TokenTags::ALPHA;
TokenTags::OI_CAPPED;
TokenTags::TRAD_FI;
```

Use `contains`, `is_empty`, `bits`, and `from_bits` for bitset work.

## Runtime Ownership

MoonClient refreshes market metadata, prices, funding, tags, and server-index
mappings. It also handles listing refresh and reconnect recovery. Application
code does not poll these endpoints or use server market indexes.

- Existing `MarketHandle` values stay valid while refreshed fields change.
- Indexed streams are not applied against a stale server mapping.
- `MarketsEvent::NewMarketsAdded` is emitted only after new markets are present
  in retained state.
- Funding and other timestamps should be read through their `MoonTime` helpers.
- Raw merge rules, correlation-index rebuilding, and listing throttles are
  runtime implementation details rather than terminal control flow.
