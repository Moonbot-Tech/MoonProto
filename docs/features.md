# Terminal Feature Guide

This guide maps common MoonBot terminal terms to the supported high-level
MoonProto API. Packet structs and command IDs are not application API.

## Markets And Charts

| Feature | Meaning | API |
|---|---|---|
| Market search | Find a market once and keep its stable handle for chart and action code. | `snapshot.markets().find(...)`; keep the returned `MarketHandle`. |
| Prices and funding | Current bid, ask, mark price, funding rate, and funding time. | `market.price()`; use typed time helpers instead of raw wire values. |
| Position overlay | Current position size/price, liquidation price, leverage, margin mode, and PnL for the selected market. | `market.balance_position()`. |
| Unprotected position | Whether the retained position is not fully covered by active non-emulator close orders. | `snapshot.position_protection_for(&market)`. |
| `Max.Order` | Maximum exchange order quantity converted to current quote value. | `market.with(|m| m.max_order_value())`. |
| `MaxPos` | Per-market position limit from leverage-management settings. | `market.max_pos_limit()`; the global fallback is `LevManage::default_max_pos_limit()`. |
| Signed market deltas | Core-compatible coin, BTC, and exchange deltas used by terminal indicators and panic/restart settings. | `market.delta_state()` and `snapshot.markets().global_deltas()`. |
| Orderbook | Current full book and best bid/ask; recovery after gaps is automatic. | `client.streams().subscribe_orderbook(...)`; `snapshot.order_book_for(...)` / `top_of_book_for(...)`. |
| Trades and live chart history | Tape rows, liquidations, Last/Mark price lines, mini-candles, 5m candles, and maintained analytics accumulated while connected. | Subscribe through `client.streams()`; read `snapshot.market_history_readers_for(&market)`. |
| Core chart archive | Older detailed trades, mini-candles, LastPrice points, and liquidations loaded when a chart opens. | `client.history().request_chart_for(&market)`; wait for `Event::MarketHistory`, then restart that chart's retained cursors. |
| History memory | How many tape/chart rows each market retains. `100%` is the production baseline; lower values trade history depth for memory. | `MarketHistorySizing::Auto` or `auto_with_budget_percent(75..=800)` in `ClientConfig`. |
| Derived volumes and deltas | Ready 1m/3m/5m trade volumes, candle volumes, short/long deltas, and the current 5m candle. | Read one `snapshot.market_history_derived_snapshot_now_for(&market)` per UI tick instead of rescanning rings. |
| MMOrders and wallets | Market-maker heat-map rows. HyperLiquid rows can include taker wallet addresses in slot-aligned companion data. | Use `TradesStreamMode::TradesAndMarketMakers` or `client.settings().set_mm_orders_subscription(true)`; read `mm_orders` with `mm_order_companion`; use `taker_hex()`. |
| Watcher fills | Core-decoded fills for watched HyperLiquid addresses. They are events, not ordinary tape rows. | `Event::WatcherFills`; use `user_hex()` and the decoded fill list. |
| CoinCard candles | Demand-loaded deep chart history for one market and timeframe. | `client.candles().request_coin_card_for(...)`; read `snapshot.coin_card_candles_for(...)`. |
| Live TF candles | Live updates for a selected chart timeframe after its base history is loaded. | `client.streams().subscribe_candles_for(...)`; read `snapshot.tf_candles_for(...)`. |
| New listings | Notification only after the refreshed market catalog has actually added markets. | `Event::Markets(MarketsEvent::NewMarketsAdded { names })`. |
| Arbitrage prices | Per-market external-platform prices and isolation flags. | `market.arb_slot(...)` / `market.arb_now(...)`; display preferences live in `ClientSettingsCommand::arb_config`. |

See [markets](markets.md), [trades](trades.md), [order books](order_books.md),
[candles](candles.md), and [arbitrage](arb.md).

## Orders And Trading

| Feature | Meaning | API |
|---|---|---|
| Live orders | Current active order rows; a removal event still carries the final row. | `snapshot.orders()` and `Event::Order`; `OrderEvent::order()` carries the event-time row. |
| New order | Create a manual or strategy-owned order for a selected market. | `client.trade().new_order(NewOrderParams::for_market(...))`. |
| Pending order | Place a bare trigger or retain an explicit strategy candidate until the trigger fires. It appears in the normal order snapshot immediately and can be moved or cancelled through `client.orders()`. | `client.trade().new_pending_order(PendingOrderParams::for_market(...).with_strategy_id(...))`; omit the strategy id for a bare pending. |
| Move or cancel | Apply the trader's intent to the current live order state; the runtime handles phase and in-flight gates. | `client.orders().move_order(...)` / `cancel(...)`. |
| Stops and VStop | Change stop-loss, trailing stop, take-profit, or volume stop only when values actually changed. | `client.orders().update_stops(...)` / `update_vstop(...)`. |
| Panic sell | Toggle panic mode for one sell, apply the market panic-button behavior, or trigger the one-shot global action. | `turn_panic_sell(...)`, `switch_panic_sell_for_market(...)`, `client.trade().panic_sell_all()`. |
| Click immunity | Exclude selected active orders from replace-kind and price-zone bulk moves. Percent-based move-all intentionally includes them. | `client.orders().set_immune_for_orders(...)`. |
| Join, split, and close | Join orders, split an order/position, or close by normal limit or explicit market semantics. | `client.trade()` with `SplitOrderParams`, `ClosePositionParams`, and named `*_for_market` helpers. |
| Move all buys/sells | Reprice matching orders for one market with replace-kind, price-zone, or percent semantics. | `move_all_buys_for_market(...)` / `move_all_sells_for_market(...)`. |
| Order traces and corridors | Chart-ready order path, stop-line endpoint, and MoonShot corridor state. | Read `buy_trace_line`, `sell_trace_line`, `stop_time`, and corridor fields from `Order`. |
| Historical reports | Durable typed replica of the core's Orders report database, including offline catch-up and soft delete/restore. | `client.reports()` and `Event::Report`; this is separate from live `snapshot.orders()`. |

`NewOrderTicket::client_order_id` is only an outbound local label. The server's
order identity is `Order::uid`; do not join optimistic UI rows to live orders by
`client_order_id`.

See [orders](orders.md), [trade actions](trade_actions.md), and
[report replication](reports.md).

## Strategies And Automation

| Feature | Meaning | API |
|---|---|---|
| Strategy list and editor | Read, create, edit, delete, and synchronize full strategy objects using the live server schema. | `snapshot.strategy_snapshots()`, `MoonShotStrategy` / `StrategyEditor`, and `client.strategies().sync_local_strategies(...)`. |
| Strategy order | The complete editor list defines the global linear order; parameter-only edits still send only changed rows. | `sync_local_strategies(...)`; read confirmed `snapshot.strategy_snapshots()` after `SnapshotFull`. |
| Empty folders and folder rename | Synchronize a complete folder tree, including empty parents; rename a populated subtree together with its strategy paths. | `snapshot.strats().folder_paths()`, `sync_local_folders(...)`, `sync_local_strategies_with_folders(...)`; see [folders](strats.md#folders-including-empty-folders). |
| Checked strategies | Change which strategies are selected without guessing the core's state. | `set_checked(...)` and `send_checked_delta()`; server confirmation updates retained state. |
| Start/stop strategies | Start checked strategies or stop all, with the actual core state retained separately from checkbox state. | `client.strategies().start()` / `stop()`; read `snapshot.strats().strategies_running()`. |
| AutoDetect / passive mode | Enable or disable core detection. This does not replace the separate strategy start/stop state. | `client.settings().set_auto_detect_active(...)`; read `runtime_state.auto_detect_active`. |
| Per-strategy whitelist/blacklist | Limit new entries for one strategy. Existing position management and sells remain separate. | Edit `coins_white_list` / `coins_black_list` on a typed strategy object, then synchronize the local list. |
| Global coin blacklist | Persistent core-wide "do not buy" list. It blocks new entries for matching coins but does not block selling or closing existing positions. | Clone retained `ClientSettingsCommand`, edit `use_coins_black_list` and `coins_black_list_text`, then `client.settings().send(...)`. |
| `TempBL` | Temporary core-wide blacklist with an expiry per symbol. It blocks new entries while active but does not block selling or closing existing positions. | Read `temp_blacklist_entries()`; replace the complete list with `set_temp_blacklist_entries(...)`; send the edited settings snapshot. |
| Manual strategy mode | Attach a selected strategy to a manual order. A regular new order with `strategy_id = 0` follows the core's configured Manual-strategy fallback; an explicit id makes ownership deterministic. | Read `use_manual_strategy` / `manual_strategy_id`, then call `NewOrderParams::with_strategy_id(...)`. |
| AutoStart | Core startup, work-time, loss/ping/error stop, restart, state-memory, and session-reset policy. | Edit typed `auto_start_config()` / `auto_start_config2()` views on retained settings. |
| Global sell defaults | Main/scalp/fixed-sell target, global stop/trailing settings, and fixed-sell button presets. | Use `ClientSettingsCommand` semantic helpers and fields, then send the full retained settings snapshot. |
| Full portable settings | The complete safe-share settings set for configuration editors, import/export, and settings not present in the compact live snapshot. It loads in the background without delaying `Ready`. | Start with `client.settings().build_shared_config()`, edit typed fields, then `send_shared_config(...)`; see [shared configuration](shared_config.md). |
| Trading emulator mode | Run core order handling in emulator mode. | Read/edit `ClientSettingsCommand::emu_mode`. |
| Chart pencil emulator | Inject drawn price points as synthetic trades for emulator testing; this is separate from `emu_mode`. | `client.emulator().send_pencil_prices_for_market(...)`. |
| Trigger keys | Arm or clear core trigger keys for selected or all markets. | `set_triggers_for_markets(...)`, `clear_triggers_for_markets(...)`, and all-market variants. |
| Chart alerts | Store authoritative armed chart objects in the core and receive accepted state. | `client.chart_alerts()`; read `snapshot.chart_alerts()`. |
| Chart filter/debug text | Ask the core to build the ready filter/debug rows for the visible chart. | `client.chart_text().set_visible_market_for_market(...)`; read `snapshot.chart_text()`. |
| Detect facts | Receive completed strategy detect, watcher, marker, and chart-alert facts without rerunning detect logic in the terminal. | `Event::Detect`. |

See [strategies](strats.md), [UI and settings](ui.md), and [events](events.md).

## Account And Core Services

| Feature | Meaning | API |
|---|---|---|
| Balances and positions | Account totals plus per-market live position state. | `client.balances().refresh()`; read `snapshot.balances()` and `market.balance_position()`. |
| Exchange PnL | Exchange-reported cumulative profit for a market, also when its position is closed. Not the resettable Session counter. | `market.balance_position().total_profit()`; delivery repair is automatic, no polling needed. |
| Asset transfer | List transferable Spot/Futures/Quarterly assets, move assets between wallets, or convert dust to BNB where supported. | `client.balances().refresh_transfer_assets()`, `transfer_asset(...)`, `convert_dust_bnb()`. |
| Hedge and margin mode | Read hedge mode; change hedge mode or one market's cross/isolated position type through the core. | `client.account().refresh_hedge_mode()`, `set_hedge_mode(...)`, `change_position_type_for(...)`. |
| API-key expiration | Current exchange API-key expiration time, when the connected engine reports it. | `client.account().refresh_api_expiration_time()`; read `snapshot.account().api_expiration()`. |
| HyperLiquid request quota | Remaining address-level HyperLiquid action requests reported by the core. | Read `snapshot.settings().hyperliquid_requests_left` after `SettingsEvent::HyperliquidRequestLimitUpdated`; the HyperLiquid user address is `snapshot.auth_info().btc_address`. |
| Leverage and risk limit | Set leverage or confirm a pending exchange risk-limit change for one market. | `client.account().set_leverage_for(...)` / `confirm_risk_limit_for(...)`. |
| Multi-assets mode | Toggle the exchange account's multi-assets/union margin mode where the connected engine supports it. | `client.account().set_ma_mode(...)`. |
| Profit counters | Core report-database profit totals for configured windows, not account balance or live order PnL. | Read `snapshot.settings().profit_state`; reset through `client.settings().reset_profit(...)`. |
| Runtime state | Whether the market runtime is started and whether AutoDetect is active. | Read `snapshot.settings().runtime_state`; `restart_now()` requests the normal start/restart flow. |
| License and MoonCredits | Current core license/module permissions and MoonCredits balances. | Read `snapshot.settings().kernel_license_state`; refresh with `client.settings().request_kernel_license_state()`. |
| Core health | Core process CPU/memory, host CPU/free memory, logical CPU count, client/core RTT, and core/exchange order API request latency. | `Event::KernelHealth` and `snapshot.kernel_health()`. |
| Core diagnostic problems | Confirmed detector findings, including memory, network, and exchange restrictions. | `snapshot.settings().problems`, `SettingsEvent::ProblemConfirmed` / `ProblemsUpdated`, `client.settings().clear_problems()` / `test_problem(...)`; see [problems](problems.md). |
| News and tags | Retained/live news JSON with same-ID translation updates plus the latest complete tags catalog. | `Event::News` and `snapshot.news()`. |
| Server logs | Authenticated core log lines for terminal logs. | `Event::ServerLog`. |
| Remote administration | Ask the core to run its release/named update flow, switch DEX/Spot selection, or shut down when no active take/sell order exists. | Use the typed methods on `client.settings()`; core shutdown is a one-shot request without an acknowledgement. |

See [balances](balances.md), [Engine API](engine_api.md), [news](news.md), and
[UI and settings](ui.md).
