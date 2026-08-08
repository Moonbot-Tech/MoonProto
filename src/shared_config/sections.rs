//! Shared-config data structures — one struct per wire section.
//!
//! Every public field carries a doc-comment explaining what it does from the
//! user's perspective.

// ---------------------------------------------------------------------------
// Top-level aggregate
// ---------------------------------------------------------------------------

/// Full shared-config payload.  Six mandatory sections plus the catalog
/// version that was written into the wire header.
#[derive(Clone, Debug)]
pub struct SharedConfig {
    /// Core configuration catalog version carried by the source snapshot.
    ///
    /// This is not the safe-share format version. It is retained for exact
    /// roundtrips and is intentionally not editable by application code.
    pub(crate) config_version: u16,
    /// Signal sources, telegram parsing, detection autobuy, alert sounds.
    pub signals: SignalsSection,
    /// Order execution, risk management, auto-start, manual strategies,
    /// multi-orders, position control, commissions, leverage management.
    pub trading: TradingSection,
    /// Chart appearance, draw tools, orderbook display, candle style,
    /// volume indicators, heatmap, blink alerts, news form, font sizes.
    pub visual: VisualSection,
    /// Active color theme (light/dark) and per-theme INI color overrides.
    pub theme: ThemeSection,
    /// Chart indicator and arb-color INI key/value pairs.
    pub ini: IniSection,
    /// Hotkeys, markets table layout, main toolbar, strat editor state.
    pub ui: UiSection,
}

impl SharedConfig {
    /// Configuration catalog version reported by the core that produced this
    /// snapshot. The safe-share payload format is versioned separately.
    pub fn core_config_version(&self) -> u16 {
        self.config_version
    }
}

// ---------------------------------------------------------------------------
// Section 1: Signals (Kind = 1, internal version = 2)
// ---------------------------------------------------------------------------

/// Signal parsing configuration for Telegram channels, clipboard monitoring,
/// and autodetection-driven autobuy.
#[derive(Clone, Debug)]
pub struct SignalsSection {
    // --- Pump channels ---
    /// Primary Telegram channel name for signal monitoring.
    pub pump_channel: String,
    /// Additional Telegram channels (multi-channel mode).
    pub pump_channels: Vec<String>,

    /// Master toggle: enable signal monitoring pipeline.
    pub do_monitoring: bool,
    /// Parse full hyperlinks in Telegram messages for token names.
    pub look_full_link_tlg: bool,
    /// Allow signals from multiple channels simultaneously.
    pub multi_channels: bool,
    /// Auto-buy when a Telegram signal matches.
    pub telegram_auto_buy: bool,
    /// Load and analyze historical candles for all markets on startup
    /// (Properties: "Analize coins on startup").
    pub load_deep_history: bool,
    /// Internal MoonBot setting, no user-facing control. Keep the value
    /// received from the kernel.
    pub only_secret_signal: bool,
    /// Require the token to appear in more than one channel before buying.
    pub more_then_1_channel: bool,
    /// Auto-buy when a token name is detected on the clipboard.
    pub clipboard_auto_buy: bool,
    /// Auto-buy on autodetect-engine signals (pump/volume/delta detects).
    pub autodetect_auto_buy: bool,
    /// Sound preset index for incoming signal notifications (1-based).
    pub signal_sound: i32,
    /// Play an alert sound on network problems (disconnects / high latency),
    /// throttled to once per 5 s. Despite the historical name this is a
    /// connectivity alert, not a signal sound.
    pub play_signal_sound: bool,
    /// Convert detected tokens to lowercase before matching (Telegram source).
    pub lower_case_token_tlg: bool,
    /// Show crosshair cursor on the chart.
    pub show_cross: bool,
    /// Enable clipboard monitoring for token names.
    pub monitor_clipboard: bool,
    /// Convert detected tokens to lowercase (clipboard source).
    pub lower_case_token_cbd: bool,
    /// Parse full hyperlinks in clipboard text for token names.
    pub look_full_link_cbd: bool,

    // --- Signal keyword strings (comma-separated) ---
    /// Keywords that indicate a BUY/LONG signal in message text.
    pub msg_keywords_long: String,
    /// Keywords that indicate a SHORT signal in message text.
    pub msg_keywords_short: String,
    /// Tag prefixes used to find token tickers in messages (e.g. "#,$").
    pub msg_token_tags: String,
    /// Words whose presence cancels a signal (e.g. "called,dont").
    pub msg_black_words: String,

    /// Signal parser sub-config (telegram message analysis rules).
    pub signal_config: SignalConfig,

    /// Use advanced per-strategy signal filtering rules.
    pub advanced_filter: bool,
    /// Automatically show/switch to the chart on new signal.
    pub auto_show_on_signal: bool,
    /// Use advanced filtering also for clipboard-source signals.
    pub advanced_filter_clipboard: bool,
    /// Volume indicator type index for the base detection.
    pub base_vol_ind_type: i32,
    /// Legacy, no consumers in current MoonBot code (a config migration
    /// resets it to 0). Kept for wire compatibility only.
    pub max_too_long_msg_len: i32,
    /// Sound preset for the second alert tier.
    pub signal_sound_2: i32,
    /// Play the sell alert when the price is within this percentage of the
    /// sell-order price.
    pub sell_alert_level: i32,
    /// Play a sound when a sell condition triggers.
    pub play_sell_alert: bool,
    /// Last manually chosen order type (1-based UI combo index).
    pub last_used_order_type: i32,
    /// Suppress new chart windows from signals while working in full-screen
    /// chart mode or when the cursor is in the orderbook area.
    pub full_screen_prevent_signals: bool,
    /// Listen to the built-in MoonBot signal channel.
    pub listen_moon_channel: bool,
    /// Phrases that indicate the price might drop further — suppress buy
    /// (comma-separated, e.g. "wait dip,for dip").
    pub lower_price_words: String,
    /// Ignore signals that are replies to other messages.
    pub dont_buy_reply: bool,
    /// Minimum 24 h base volume (in BTC equivalent) for a token to qualify.
    pub base_vol_n: i32,
    /// Use the last detection's caption as the chart title.
    pub use_last_detect_caption: bool,
    /// Sound preset for buy-signal alert.
    pub buy_signal_sound: i32,
    /// Play a sound on buy alert.
    pub play_buy_alert: bool,
    /// Play the buy alert when the price is within this percentage of the
    /// buy-order price.
    pub buy_alert_level: i32,
    /// Comma-separated strategy name filter (only react to these strats).
    pub strats_filter: String,
    /// Comma-separated news-tag filter.
    pub news_tags_filter: String,
    /// Comma-separated news-token filter.
    pub news_tokens_filter: String,

    /// Opaque bytes from a newer MoonBot version (preserved for roundtrip).
    pub unknown_tail: Vec<u8>,
}

/// Telegram/clipboard message parser sub-config.
#[derive(Clone, Debug)]
pub struct SignalConfig {
    /// Internal version of this sub-record.
    pub ver: i32,
    /// Require keyword match in message text.
    pub use_keywords: bool,
    /// Require token-tag prefix match (e.g. `$BTC`).
    pub use_token_tags: bool,
    /// Max distance (in words) between keyword and token.
    pub buy_key_dist: i32,
    /// Only react if exactly one token is found in the message.
    pub only_1_token: bool,
    /// Buy if a price value is found in the message text.
    pub buy_if_price_found: bool,
    /// Price offset threshold for price-in-message filter.
    pub x_found_price: i32,
    /// Enable blackword filter (cancel signal if any blackword present).
    pub use_black_words: bool,
    /// Maximum total word count in a valid signal message.
    pub words_count: i32,
    /// Enable the word-count filter.
    pub use_words_count: bool,
    /// Parse and use embedded price data from messages.
    pub use_price: bool,
    /// Parse and use stop-loss levels from messages.
    pub use_stops: bool,
    /// Match token names without tag prefixes too.
    pub tokens_no_tags: bool,
    /// Follow links in messages to find token names.
    pub token_links: bool,
    /// Enable "lower price words" filter (defer buy).
    pub use_lower_price_words: bool,
    /// Price offset for lower-price-words filter.
    pub x_lower_price: i32,
    /// Enable special parsing formats for specific channel layouts.
    pub special_formats: bool,
}

// ---------------------------------------------------------------------------
// Section 2: Trading (Kind = 2, internal version = 3)
// ---------------------------------------------------------------------------

/// Order execution, risk management, and position control settings.
#[derive(Clone, Debug)]
pub struct TradingSection {
    /// Auto-delete old log files after this many days.
    pub auto_delete_logs: i32,
    /// Log verbosity level (0 = minimal, higher = more detail).
    pub log_level: i32,
    /// API connection variant index (exchange-specific, e.g. 0 = default).
    pub binance_connection: i32,

    /// Automatic start/stop and panic-sell rules.
    pub auto_start: AutoStartConfig,
    /// Extended auto-start rules (restart on BTC movement, session caps).
    pub auto_start_2: AutoStartConfig2,

    /// Comma-separated list of favorited market tickers.
    pub fav_markets: String,
    /// Buy price offset from ask, in price steps.
    pub x_ask: i32,
    /// Take-profit sell price offset from buy, in slider units.
    /// Mirrors [`ClientSettingsCommand::x_sell`].
    pub x_sell: i32,
    /// Scalp/quick sell offset in slider units.
    /// Mirrors [`ClientSettingsCommand::x_sell_scalp`].
    pub x_sell_scalp: i32,
    /// Order size as a share of the total trade balance, in thousandths:
    /// order = balance * play_with / 1000 (multiplied by leverage on futures).
    /// The main size slider in MoonBot. Ignored when a fixed manual balance is
    /// set.
    pub play_with: i32,
    /// Check orderbook depth before placing a buy order.
    pub check_glass: bool,
    /// Maximum concurrent open buy orders.
    pub max_orders: i32,
    /// Percentage of buy fill that triggers automatic sell (100 = wait for
    /// full fill).
    pub auto_sell_partial: i32,
    /// Acceptable price range for buys (in price steps from ask).
    pub price_range: i32,
    /// Auto-move sell orders to the best orderbook position.
    pub sell_auto_move: bool,
    /// Minimum PumpQ score a market must have for pump-signal autobuy; the
    /// signal is skipped below it. PumpQ is MoonBot's market-quality metric
    /// computed from recent volume/price history.
    pub pump_q_level: i32,
    /// Volume percentile threshold for doubling sell quantity.
    pub sell_x2_level: i32,
    /// Cancel the pending buy when a sell order fills.
    pub cancel_buy_on_sell_fill: bool,
    /// Auto-cancel an unfilled buy order after a timeout. Composite scale:
    /// values below 30 are seconds, values 30 and above mean
    /// `(value - 29)` minutes.
    pub auto_cancel_buy_order: i32,
    /// Volume-drop percentage that triggers panic sell (when enabled).
    /// Mirrors [`ClientSettingsCommand::vol_drop_level`].
    pub vol_drop_level: i32,
    /// Price-drop percentage that triggers panic sell.
    /// Mirrors [`ClientSettingsCommand::price_drop_level`].
    pub price_drop_level: f32,
    /// Panic-sell if volume drops below threshold.
    pub panic_if_vol_drop: bool,
    /// Panic-sell if price drops below threshold.
    /// Mirrors [`ClientSettingsCommand::panic_if_price_drop`].
    pub panic_if_price_drop: bool,
    /// Manual mode — all autobuy sources are disabled.
    pub manual_mode: bool,
    /// Draw all pending buy orders on the chart (not only the selected one).
    pub draw_all_buy: bool,
    /// Enable trailing-stop logic globally.
    /// Mirrors [`ClientSettingsCommand::trailing_stop`].
    pub trailing_stop: bool,
    /// Trailing-stop drop threshold (negative = below peak, e.g. -2.0 %).
    /// Mirrors [`ClientSettingsCommand::trailing_drop`].
    pub trailing_drop: f32,
    /// Engine double-checks balance before placing a buy.
    pub engine_check_buy: bool,
    /// Use a fixed trade amount instead of a percentage of balance.
    pub use_manual_balance: bool,
    /// Fixed trade amount in quote currency.
    pub fixed_trade_balance: f64,
    /// Use the current ask price for buys (vs. last trade price).
    pub use_current_ask: bool,
    /// Global take-profit percentage.
    /// Mirrors [`ClientSettingsCommand::g_take_profit`].
    pub g_take_profit: f64,
    /// Enable global take-profit.
    /// Mirrors [`ClientSettingsCommand::use_g_take_profit`].
    pub use_g_take_profit: bool,
    /// Show order comments (strategy name / signal source) on the chart.
    pub show_order_comment: bool,
    /// Clean up stale chart data after this many minutes of inactivity.
    pub chart_clean_up_time: i32,

    /// MoonBot service settings (silent mode, margin, group delay).
    pub moonbot_config: MoonBotConfig,

    /// Require double-click on the Panic Sell button to activate.
    pub dbl_click_panic_sell: bool,
    /// Permanent coin blacklist text (one ticker per line).
    /// Mirrors [`ClientSettingsCommand::coins_black_list_text`].
    pub coins_black_list_text: String,
    /// Enable permanent coin blacklist.
    /// Mirrors [`ClientSettingsCommand::use_coins_black_list`].
    pub use_coins_black_list: bool,
    /// Hanging-position blacklist tickers (position control).
    pub h_pos_black_list_text: String,
    /// Trailing-stop floating percentage.
    pub trailing_float: f64,
    /// Additional hanging-position blacklist entries.
    pub h_pos_black_list_add: String,
    /// Add small random noise to order price (avoid detection).
    pub random_price: bool,

    /// Report/history display settings.
    pub report_config: ReportConfig,

    /// Skip buying delisted coins.
    pub dont_buy_delisted: bool,
    /// Skip buying coins listed less than N minutes ago.
    pub dont_buy_new_coins: i32,
    /// Auto-cancel a buy order placed below the current price after this many
    /// minutes.
    pub auto_cancel_lower_buy: i32,
    /// Auto-buy by pressing Enter in the token search box.
    pub buy_on_enter: bool,
    /// Auto-purchase BNB when balance is low (for Binance commissions).
    pub auto_buy_bnb: bool,
    /// Minimum BNB balance that triggers auto-buy.
    pub auto_buy_bnb_level: f64,
    /// Amount of BNB to purchase when auto-buy triggers.
    pub auto_buy_bnb_volume: f64,
    /// Mouse click action that replaces a buy order's price.
    pub order_replace_click_buy: u8,
    /// Skip buying forward contracts / pre-market tokens.
    pub dont_buy_forward: bool,
    /// Show split-zone lines on chart (visual price zones).
    pub chart_split_zones: bool,
    /// Draw stop-loss line on the chart.
    pub draw_stop: bool,
    /// Maximum single order size in quote currency (0 = no limit).
    pub max_order: f64,
    /// Remove the max-orders limit entirely.
    pub unlimited_orders: bool,
    /// Use manual strategy for order parameters.
    pub use_manual_strategy: bool,
    /// Name of the manual strategy.
    pub manual_strategy: String,
    /// Use x9 (9-grid) mode for order splitting.
    pub x9_mode: bool,
    /// Shared scale flag — multiply offsets by 10 when true.
    /// Mirrors [`ClientSettingsCommand::x_tmode`].
    pub x_t_mode: bool,
    /// Runtime one-shot flag: the fixed-balance warning dialog has already
    /// been shown. Not a user setting.
    pub fixed_balance_warning: bool,
    /// Minimum gap between trades in milliseconds.
    pub trades_gap_time: i32,
    /// Use iceberg orders for buys.
    /// Mirrors [`ClientSettingsCommand::buy_iceberg`].
    pub buy_iceberg: bool,
    /// Use iceberg orders for sells.
    /// Mirrors [`ClientSettingsCommand::sell_iceberg`].
    pub sell_iceberg: bool,
    /// Iceberg slice size as a fraction of total.
    pub iceberg_step: f64,
    /// Mouse click action that places a new buy order at price.
    pub order_set_click: u8,
    /// Mouse click action that places a pending (limit) buy order.
    pub pending_order_set_click: u8,
    /// Spread percentage for pending orders above/below market.
    pub pending_orders_spread: f64,
    /// High-delta threshold for pending order spread adjustment.
    pub pending_orders_spread_h_delta: f64,
    /// Legacy, no consumers in current MoonBot code (default 1). Kept for wire
    /// compatibility only.
    pub buy_pending_spread: i32,
    /// Legacy, no consumers in current MoonBot code (default 1). Kept for wire
    /// compatibility only.
    pub buy_stop_pending_spread: i32,
    /// On thin-client: panic-sell positions when master closes.
    pub tm_panic_sell_closed: bool,
    /// Thin-client: only forward share commands, not full replays.
    pub tm_send_only_share: bool,
    /// Multi-orders (MoonFriend): when this master bot's sell order completes,
    /// send `cmd_CancelBuy` for the market to the linked user bots.
    pub tm_cancel_closed: bool,
    /// Multi-orders (MoonFriend): when this master bot's buy order fills, send
    /// `cmd_CancelBuy` to the linked user bots 5 seconds later.
    pub tm_cancel_buy_on_fill: bool,
    /// Allow multi-commands (batch order operations from thin-client).
    pub multi_commands: bool,
    /// Runtime counter of iceberg button clicks on the chart. Not a user
    /// setting.
    pub iceberg_click_count: i32,
    /// Multi-orders (MoonFriend): right after the master's buy completes, send
    /// `cmd_Sell` with the master's sell price to the linked bots — otherwise
    /// each linked bot places the sell at its own strategy price.
    pub tm_send_first_sell: bool,
    /// Thin-client: auto-sell if master buy is not filled.
    pub tm_sell_if_master_not_filled: bool,
    /// Thin-client: max order size override (0 = follow master).
    pub tm_max_order: f64,

    /// Multi-order mode settings (visual order grid behavior).
    pub multi_orders: MultiOrdersConfig,

    /// Mouse click action that replaces a sell order's price.
    pub order_replace_click_sell: u8,

    /// Chart price-alert configuration.
    pub alert_config: AlertConfig,

    /// Use the MoonBot-curated coin blacklist from the cloud.
    pub use_moon_bl: bool,
    /// Auto-correct order price to the nearest valid tick.
    pub correct_order_price: bool,
    /// Binance Futures commission rate override.
    pub f_binance_commission: f64,

    /// Spot chart panel display settings.
    pub spot_config: SpotConfig,

    /// Prefer WebSocket best-bid/ask stream over REST polling.
    pub use_book_ticker: bool,
    /// Exclude blacklisted coins from delta calculations.
    pub exclude_black_list_delta: bool,
    /// Check and auto-close orphaned (zero-size) positions.
    /// Mirrors [`ClientSettingsCommand::free_position_check`].
    pub free_position_check: bool,

    /// Order lifecycle watchdog settings.
    pub orders_control: OrdersControl,

    /// Use volume-weighted moving average for market analysis.
    pub m_avg_use_vol_weight: bool,
    /// Use pending-buy price instead of current ask for sell calculations.
    pub pending_buy_price: bool,
    /// Fixed sell mode toggle.
    /// Mirrors [`ClientSettingsCommand::fixed_sell_mode`].
    pub fixed_sell_mode: bool,
    /// Fixed sell price value.
    /// Mirrors [`ClientSettingsCommand::fixed_sell_price`].
    pub fixed_sell_price: f64,
    /// Enable futures-specific safety rules (position mode checks etc.).
    pub futures_rules: bool,

    /// Cashback (BNB holder discount) tracking settings.
    pub cashback_settings: CashBackSettings,

    /// Automatically lower leverage if the exchange rejects the current level.
    pub auto_lower_lev: bool,

    /// Asset/funds transfer panel display settings.
    pub transfer_config: AssetTransferConfig,

    /// Minimum balance to keep in the account (negative = disabled).
    pub min_balance: f64,

    /// Automatic leverage management settings.
    pub auto_manage_lev: AutoManageLevConfig,

    /// Leverage control expression (e.g. "1k def 5k BTC*").
    pub auto_lev_control: String,
    /// Use stop-market orders instead of stop-limit.
    /// Mirrors [`ClientSettingsCommand::use_stop_market`].
    pub use_stop_market: bool,
    /// Auto-close zero-quantity ghost positions.
    pub auto_close_zero_pos: bool,

    /// Telegram screenshot sharing settings.
    pub send_shots_config: SendShotsConfig,

    /// Include leverage in take-profit calculations.
    pub use_lev_for_take: bool,
    /// ByBit commission rate multiplier (e.g. 1.001 = 0.1% fee).
    pub bybit_commission: f64,
    /// Auto-reduce oversized orders to match available balance.
    pub auto_reduce_order: bool,
    /// Gate.io commission rate multiplier.
    pub gate_commission: f64,

    /// User-defined manual strategy names (slots 1..10).
    pub manual_strats_names: [String; 10],

    /// Manual strategy buttons / hotkeys config.
    pub manual_strats_config: ManualStratsConfig,

    /// Trigger-clear expression (e.g. "1-200" = clear triggers for IDs 1..200).
    pub clear_triggers_string: String,
    /// Tickers that should not generate trade signals (one per line).
    pub no_trades_markets_text: String,
    /// Use WebSocket-based API for order placement (vs REST).
    pub use_websocket_api: bool,
    /// Order book depth levels for WebSocket stream.
    pub order_book_levels_ws: i32,

    /// Arbitrage panel display settings.
    pub arb_view_config: ArbViewConfig,

    // --- Tail fields (gated by blockEnd in Load, appended in Save) ---
    /// Compute deltas from trade stream instead of orderbook.
    pub deltas_by_trades: bool,
    /// Ignore strategy-defined sell price, use global settings instead.
    pub ignore_strat_sell_price: bool,
    /// Use HyperLiquid's fast IOC (Immediate-Or-Cancel) order mode.
    pub use_hl_fast_ioc: bool,

    /// Opaque bytes from a newer MoonBot version (preserved for roundtrip).
    pub unknown_tail: Vec<u8>,
}

/// Automatic start, stop, restart, and panic-sell rules.
#[derive(Clone, Debug)]
pub struct AutoStartConfig {
    /// Auto-start detection on application launch.
    pub auto_start: bool,
    /// Auto-enable detection engine on start.
    pub auto_detect_on: bool,
    /// Auto-enable strategies on start.
    pub strategies_on: bool,
    /// Restrict trading to a time-of-day window.
    pub work_time: bool,
    /// Stop the bot if cumulative loss exceeds threshold.
    pub auto_stop_if_loss: bool,
    /// Remember last running state across restarts.
    pub remember_state: bool,
    /// Sell all positions when loss threshold is hit.
    pub sell_if_loss: bool,
    /// Don't wait for pending sells before stopping.
    pub dont_wait_sells: bool,
    /// Loss threshold amount (in quote currency) that triggers auto-stop.
    pub auto_stop_loss: f64,
    /// Enable BTC price panic-sell trigger.
    pub panic_btc: bool,
    /// Enable market (alt) price panic-sell trigger.
    pub panic_market: bool,
    /// Use hourly session window for loss calculation.
    pub auto_stop_if_loss_hours: bool,
    /// Auto-update to new MoonBot releases.
    pub auto_update: bool,
    /// Restart on persistent API errors.
    pub restart_after_err: bool,
    /// Restart on high-latency / ping timeout.
    pub restart_after_ping: bool,
    /// Exclude emulator orders from loss calculation.
    pub ignore_emulator: bool,
    /// Number of trades in the loss-calculation window.
    pub stop_trades: i32,
    /// Seconds to wait before error-triggered restart.
    pub restart_err_time: i32,
    /// Hourly BTC rate drop (%) that triggers the global panic sell.
    pub panic_btc_delta: f64,
    /// Hourly average market rate drop (%) that triggers the global panic
    /// sell.
    pub panic_market_delta: f64,
    /// Stop detection on persistent errors.
    pub auto_stop_on_errors: bool,
    /// Stop detection on high-latency / ping timeout.
    pub auto_stop_on_ping: bool,
    /// Sell all positions on persistent errors.
    pub sell_all_on_errors: bool,
    /// Sell all positions on ping timeout.
    pub sell_all_on_ping: bool,
    /// Error count that qualifies as "persistent".
    pub errors_level: i32,
    /// Ping (ms) that qualifies as "high latency".
    pub ping_level: i32,
    /// Seconds to wait before ping-triggered restart.
    pub restart_ping_time: i32,
    /// Hourly loss cap for auto-stop (in quote currency).
    pub auto_stop_hours_val: f64,
    /// Hours to look back for hourly loss calculation.
    pub stop_hours: i32,
    /// Minimum trades in the hourly window.
    pub stop_hours_trades: i32,
    /// Hourly BTC rate rise (%) that triggers the global panic sell.
    pub panic_btc_delta_up: f64,
    /// Work-time window start (fraction of day, e.g. 0.0 = midnight).
    pub work_time_from: f64,
    /// Work-time window end (fraction of day, e.g. 0.9999 ≈ 23:59).
    pub work_time_to: f64,
}

/// Extended auto-start settings (BTC-threshold restart, session caps).
#[derive(Clone, Debug)]
pub struct AutoStartConfig2 {
    /// Restart trading when market changes by threshold.
    pub restart_on_market: bool,
    /// BTC must be higher than this % (from 24h baseline) to trade.
    pub btc_higher_then: f64,
    /// BTC must be lower than this % to trade.
    pub btc_lower_then: f64,
    /// Market must be higher than this % to trade.
    pub market_higher_then: f64,
    /// Show old listing notifications even for known coins.
    pub show_old_listing: bool,
    /// Auto-reset session profit counters periodically.
    pub reset_session: bool,
    /// Maximum session cap (in quote currency).
    pub max_session_cap: i32,
    /// Hours between session resets.
    pub rs_hours: i32,
}

/// MoonBot service sub-config (silent mode, margin auto-management).
#[derive(Clone, Debug)]
pub struct MoonBotConfig {
    /// Post only service messages to the user's own Telegram channel;
    /// trade and signal reports are suppressed.
    pub silent: bool,
    /// Auto-borrow margin when needed (margin trading).
    pub auto_margin_borrow: bool,
    /// Auto-transfer between spot and margin wallets.
    pub auto_margin_transfer: bool,
    /// Batching window for outgoing Telegram messages, in ms: messages queued
    /// during the window are merged and sent as one message (1000..5000; with
    /// the shared bot the effective minimum is 3500). Set at runtime with the
    /// `SetGroupDelay` Telegram command.
    pub group_delay: i32,
}

/// Report / trade history display configuration.
#[derive(Clone, Debug)]
pub struct ReportConfig {
    /// Column visibility flags (51 columns, index 0..50).
    pub fields_vis: [bool; 51],
    /// Column widths in pixels (51 columns).
    pub fields_width: [u16; 51],
    /// Show only active (open) positions.
    pub only_active: bool,
    /// Date range filter index.
    pub range: i32,
    /// Free-text filter 1 (ShortString, 26 raw bytes: 1 len + 25 chars).
    pub filter1: [u8; 26],
    /// Free-text filter 2 (ShortString, 26 raw bytes).
    pub filter2: [u8; 26],
    /// Free-text filter 3 (ShortString, 26 raw bytes).
    pub filter3: [u8; 26],
    /// Active-type filter (0=all, 1=active only, 2=closed only, 3=custom).
    pub active_type: u8,
    /// Include emulator orders in the report.
    pub rep_emulator_orders: bool,
    /// Stretch table columns to fill available width.
    pub stretch: bool,
    /// Aggregate multi-orders into single lines.
    pub only_2_orders: bool,
    /// Show leverage-adjusted profit numbers.
    pub use_leverage: bool,
    /// Max visible rows in the report table.
    pub max_lines: u16,
    /// Position direction filter (0=all, 1=long, 2=short).
    pub pos_direction: u8,
    /// Store liquidation price in the report.
    pub store_liq: bool,
    /// Store funding rate in the report.
    pub store_fund: bool,
    /// Sort by close date instead of open date.
    pub by_close_date: bool,
}

/// Multi-order mode settings (visual order grid placement and movement).
#[derive(Clone, Debug)]
pub struct MultiOrdersConfig {
    /// Internal version of this sub-record.
    pub ver: u8,
    /// Enable multi-order visual grid mode.
    pub use_multi_orders: bool,
    /// Click action to place a buy order at chart price.
    pub buy_set_click: u8,
    /// Click action to move buy orders.
    pub buy_move_click: u8,
    /// Click action to move sell orders.
    pub sell_move_click: u8,
    /// How to select which buy to replace (shift/topvol/all/...).
    pub replace_buy_kind: u8,
    /// How to select which sell to replace.
    pub replace_sell_kind: u8,
    /// Number of split-sell parts (0 = no split).
    pub split_sells: i32,
    /// Show order count badges on chart.
    pub show_orders_num: bool,
    /// Use Kir-style order visualization.
    pub kir_style: bool,
    /// Position side filter (0=both, 1=long, 2=short).
    pub fix_pos: u8,
    /// Join-sell mode (0=none, 1=fixed price, 2=fixed profit).
    pub join_sell_kind: u8,
    /// Click action to set short order at chart price.
    pub short_set_click: u8,
    /// Click action to set pending short.
    pub pending_short_set_click: u8,
    /// Opacity of completed/done orders on chart (0.0..1.0).
    pub done_opacity: f32,
    /// Secondary buy-move click action.
    pub buy_move_click_2: u8,
    /// Secondary sell-move click action.
    pub sell_move_click_2: u8,
    /// Secondary buy-replace logic.
    pub replace_buy_kind_2: u8,
    /// Secondary sell-replace logic.
    pub replace_sell_kind_2: u8,
    /// Use the same hotkeys for both primary and secondary move.
    pub same_hotkeys_for_move: bool,
    /// Short-side buy move click.
    pub short_buy_move_click: u8,
    /// Short-side sell move click.
    pub short_sell_move_click: u8,
    /// Short-side secondary buy move click.
    pub short_buy_move_click_2: u8,
    /// Short-side secondary sell move click.
    pub short_sell_move_click_2: u8,
}

/// Chart price-alert configuration.
#[derive(Clone, Debug)]
pub struct AlertConfig {
    /// Enable price alerts.
    pub enabled: bool,
    /// How long an alert stays visible on screen (seconds).
    pub keep_time: i32,
    /// Alert sound preset index.
    pub sound_kind: i32,
    /// Number of times to repeat the alert sound.
    pub repeat_count: i32,
}

/// Spot chart panel display settings.
#[derive(Clone, Debug)]
pub struct SpotConfig {
    /// Show trade ticker stream.
    pub show_trades: bool,
    /// Show orderbook visualization.
    pub show_book: bool,
    /// Orderbook visual depth (percentage of chart width).
    pub book_len: f32,
    /// Show volume-weighted market average line.
    pub show_market_avg: bool,
    /// Show 24h min/max price markers.
    pub show_min_max: bool,
    /// Show average entry price line.
    pub show_avg_price: bool,
    /// Show BTC price in the spot panel.
    pub spot_btc: bool,
    /// Shift spot chart to account for orderbook panel width.
    pub shift_spot: bool,
    /// Show mark price (futures).
    pub show_mark_price: bool,
    /// Show liquidation price line (futures).
    pub show_liq: bool,
    /// Highlight abnormally large liquidations.
    pub huge_liq: bool,
    /// Show open interest indicator.
    pub show_open_int: bool,
    /// Show average price as a chart line (not just label).
    pub show_avg_price_line: bool,
}

/// Order lifecycle watchdog settings.
#[derive(Clone, Debug)]
pub struct OrdersControl {
    /// Enable order lifecycle watchdog.
    pub active: bool,
    /// Minimum order price threshold.
    pub min_price: f64,
    /// Maximum time for an order to stay pending (seconds).
    pub max_time: i32,
    /// Enable hanging-position detection.
    pub h_pos_control: bool,
    /// Report hanging positions via Telegram.
    pub h_pos_report: bool,
    /// Auto-sell hanging positions.
    pub h_pos_auto_sell: bool,
    /// Ignore the "replacing" order state bug in some engines.
    pub ignore_replacing_bug: bool,
    /// Order protection bypass level (0 = enabled).
    pub ignore_protection: i32,
    /// Sign orders with HMAC for thin-client authentication.
    /// Mirrors [`ClientSettingsCommand::sign_orders`].
    pub sign_orders: bool,
    /// Enable liquidation-price proximity control.
    pub liq_control: bool,
}

/// Cashback / BNB-holder discount tracking.
#[derive(Clone, Debug, Default)]
pub struct CashBackSettings {
    /// Display cashback as a separate line item.
    pub sep: bool,
    /// Aggregate cashback across sub-accounts.
    pub agg_accounts: bool,
    /// Maximum days to look back for cashback history.
    pub max_days: i32,
    /// Use instant BNB conversion for cashback.
    pub instant_bnb: bool,
    /// Hide the info/explanation panel.
    pub hide_info: bool,
}

/// Asset transfer panel display settings.
#[derive(Clone, Debug)]
pub struct AssetTransferConfig {
    /// Show total balance line.
    pub show_total: bool,
    /// Hide zero-balance assets.
    pub hide_zero: bool,
    /// Show backup/snapshot balances.
    pub show_baks: bool,
}

/// Automatic leverage management.
#[derive(Clone, Debug)]
pub struct AutoManageLevConfig {
    /// Auto-set max order leverage to fit available balance.
    pub auto_max_order: bool,
    /// Auto-increase leverage when exchange allows.
    pub auto_lev_up: bool,
    /// Fixed leverage value (when auto is off).
    pub fix_lev: i32,
    /// Auto-switch to isolated margin mode.
    pub auto_isolated: bool,
    /// Report leverage changes via Telegram.
    pub tlg_report: bool,
    /// Auto-switch to cross margin mode.
    pub auto_cross: bool,
    /// Auto-apply the fixed leverage value.
    pub auto_fix_lev: bool,
}

/// Telegram screenshot sharing settings.
#[derive(Clone, Debug)]
pub struct SendShotsConfig {
    /// Enable automatic screenshot sharing.
    pub may_send: bool,
    /// Minimum absolute profit to trigger a screenshot.
    pub profit_abs: i32,
    /// Minimum profit percentage to trigger a screenshot.
    pub profit_pers: i32,
    /// Minimum session profit to trigger a screenshot.
    pub profit_session: i32,
    /// Also send screenshots for negative (loss) trades.
    pub send_negative: bool,
    /// Send to the currently open Telegram chat.
    pub send_to_open_chat: bool,
    /// Post screenshots to the public channel.
    pub send_public: bool,
    /// Time scale for the screenshot chart (seconds of history).
    pub time_scale: i32,
    /// Price scale for the screenshot chart.
    pub price_scale: i32,
}

/// Manual strategy buttons and hotkeys.
#[derive(Clone, Debug)]
pub struct ManualStratsConfig {
    /// Show manual strategy quick-buttons on the toolbar.
    pub use_buttons: bool,
    /// Hotkey assignments for manual strategy slots (10 slots, `TShortCut`).
    pub hot_keys: [u16; 10],
    /// Visibility of each strategy button slot.
    pub show_button: [bool; 10],
}

/// Arbitrage panel display settings.
#[derive(Clone, Debug)]
pub struct ArbViewConfig {
    /// Internal version byte.
    pub ver: u8,
    /// Show absolute spread values.
    pub show_absolute: bool,
    /// Show spread numbers on the arb chart.
    pub show_numbers: bool,
    /// Draw spread lines on the arb chart.
    pub show_lines: bool,
    /// Show spread as percentage.
    pub show_percent: bool,
    /// Place the arb panel on the right side.
    pub show_right: bool,
}

// ---------------------------------------------------------------------------
// Section 3: Visual (Kind = 3, internal version = 2)
// ---------------------------------------------------------------------------

/// Chart appearance, draw tools, visual indicators, candle style.
#[derive(Clone, Debug)]
pub struct VisualSection {
    /// Order color scheme (7 colors + price line width).
    pub colors: IntColors,

    /// Opacity of the panic-sell overlay (0..100).
    pub panic_sell_opacity: i32,
    /// Hide the forum/community label shown on the main form (Properties
    /// checkbox).
    pub hide_forum_label: bool,
    /// Show colored (red/green) price movement lines on chart.
    pub show_red_green_lines: bool,
    /// Enable chart auto-scrolling to follow the latest candle.
    pub scrolling_charts: bool,
    /// Load saved charts on application startup.
    pub startup_load_charts: bool,
    /// Auto-close chart windows when the coin is deselected.
    pub auto_close_charts: bool,
    /// Show chart info panel on the left side (vs. right).
    pub left_chart_info: bool,
    /// Show colored dots at trade execution points.
    pub show_red_green_dots: bool,
    /// Orderbook panel opacity (0..100).
    pub glass_opacity: i32,
    /// Default chart time scale in minutes.
    pub chart_time_scale: i32,
    /// Enable multi-timeframe chart watch windows.
    pub chart_watch_multi_frames: bool,
    /// Number of historical candles to load (0 = default).
    pub chart_history_len: i32,
    /// Hide the right info panel on chart windows.
    pub hide_right_chart_panel: bool,
    /// Show vertical volume indicator.
    pub show_vert_volume: bool,
    /// Vertical volume timeframe index.
    pub vert_vol_time_frame: i32,
    /// Vertical volume indicator opacity (0.0..1.0).
    pub vert_vol_opacity: f32,
    /// Vertical volume indicator height (fraction of chart, 0.0..1.0).
    pub vert_vol_height: f32,
    /// Vertical volume visualization mode (0=bars, 1=area, ...).
    pub vert_vol_kind: i32,
    /// Show Volume Tool indicator.
    pub v_tool_show: bool,
    /// Volume Tool time parameter.
    pub v_tool_time: f64,
    /// Volume Tool opacity (0.0..1.0).
    pub v_tool_opacity: f32,
    /// Volume Tool: show buyer-initiated volume.
    pub v_tool_show_buyers: bool,
    /// Volume Tool timeframe index.
    pub v_tool_time_index: i32,
    /// Show HeatVolume indicator.
    pub hv_show: bool,
    /// HeatVolume indicator width (fraction of chart).
    pub hv_width: f32,
    /// HeatVolume timeframe index.
    pub hv_time_frame_ind: i32,
    /// HeatVolume timeframe raw value (minutes).
    pub hv_time_frame: i32,
    /// HeatVolume price frame (fraction of price range).
    pub hv_price_frame: f32,
    /// HeatVolume opacity (0.0..1.0).
    pub hv_opacity: f32,

    /// Chart draw tools configuration (per-tool colors, stroke, alerts).
    pub custom_draw_config: CustomDrawConfig,

    /// Currently selected draw tool kind (TChartObjectKind ordinal).
    pub global_current_draw_kind: u8,
    /// Placement of horizontal-volume labels on the chart: 0 = right with a
    /// cleared background, 1 = left with a guide line to the bar, 2 = right
    /// drawn over the chart, 3 = left over the chart with a guide line.
    pub hv_disp_vol: i32,
    /// HeatVolume chart kind.
    pub hv_kind: i32,
    /// Show market name captions on chart.
    pub show_market_captions: bool,
    /// Show order price captions on chart.
    pub show_orders_captions: bool,
    /// Place order captions below the line (vs. above).
    pub orders_captions_lower: bool,
    /// Show USD-converted prices on charts (for non-USDT pairs).
    pub show_usd_on_charts: bool,
    /// Auto-zoom new market charts to maximum scale.
    pub new_markets_max_scale: bool,
    /// Vertical volume indicator panel position index.
    pub vert_vol_ind_pos: i32,
    /// Cumulative orderbook visualization opacity (0..100).
    pub book_cumulative_opacity: i32,
    /// Individual order level opacity (0..100).
    pub book_orders_opacity: i32,
    /// Orderbook level bar width in pixels.
    pub book_orders_width: i32,

    /// BTC price blink-alert settings.
    pub blink_config: BlinkConfig,

    /// Auto-shift chart to keep the latest candle visible.
    pub charts_auto_shift: bool,
    /// Show iceberg order visualization on chart.
    pub show_iceberg: bool,
    /// Hide the PnL (profit and loss) display.
    pub hide_pnl: bool,
    /// Draw the current price horizontal line (0=off, 1=line, 2=dashed).
    pub draw_cur_price: i32,
    /// Auto-request chart data when opening a market.
    pub auto_request_charts: bool,
    /// Visual orders panel sort mode (0=buy first, 1=sell first, 2=creation).
    pub vo_sort_kind: u8,
    /// Visual orders: show older orders first.
    pub vo_older_first: bool,
    /// Visual orders: group by market.
    pub vo_sort_by_market: bool,
    /// Hide the main Buy button on the toolbar.
    pub hide_buy_button: bool,
    /// Show strategy index numbers on the chart.
    pub show_strat_numbers: bool,

    /// Chart filter-bar visibility settings.
    pub show_filters: ShowFilters,

    /// Hide the cashback button on the main form.
    pub hide_cashback_button: bool,
    /// Hide the seasonal New Year decoration and greetings (shown late
    /// December through early January).
    pub hide_ny_elka: bool,

    /// Chart window manager settings.
    pub charts_settings: ChartsSettings,

    /// Spy/stealth mode: hide sensitive info.
    pub spy_hide_mode: bool,

    /// Performance heatmap display settings.
    pub heatmap_config: HeatMapConfig,

    /// Application tray icon variant index.
    pub icon_selection: i32,
    /// Remember chart toolbar button states across sessions.
    pub remember_chart_buttons: bool,
    /// Show the detection-tool button on the chart toolbar.
    pub show_detects_tool: bool,
    /// Scale-plus default zoom step index.
    pub scale_plus_index: i32,
    /// Scale-minus default zoom step index.
    pub scale_minus_index: i32,

    /// News feed form display settings.
    pub news_form_config: NewsFormConfig,
    /// Font size overrides for 20 UI areas.
    pub font_sizes: [u8; 20],

    // --- Tail fields (gated by blockEnd) ---
    /// Candle rendering style (0=filled, 1=contours, 2=contours+ticks).
    pub chart_candles_style: u8,
    /// Tick-candle opacity (0..100, default 25).
    pub chart_candles_tick_opacity: u8,
    /// Use neutral (grey) color for doji/neutral candles.
    pub chart_candles_neutral_ticks: bool,
    /// Candle outline stroke width (1..3).
    pub chart_candles_outline_width: u8,
    /// Draw tick-style wicks on candles.
    pub chart_candles_tick_wicks: bool,
    /// Per-theme candle colors: `[light, dark]` each has green/red/neutral.
    pub candle_colors: [CandleColorSet; 2],
    /// Enable AI coin-card feature.
    pub use_ai_coin_card: bool,
    /// AI card provider index (0 = default/none).
    pub ai_card_provider: u8,
    /// AI card model name.
    pub ai_card_model: String,
    /// AI card custom prompt.
    pub ai_card_prompt: String,
    /// Open chart windows in full-screen for manual trading mode.
    pub manual_charts_full_screen: bool,

    /// Opaque bytes from a newer MoonBot version (preserved for roundtrip).
    pub unknown_tail: Vec<u8>,
}

/// Candle colors for one theme (light or dark).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct CandleColorSet {
    /// Bullish candle color (ARGB).
    pub green: u32,
    /// Bearish candle color (ARGB).
    pub red: u32,
    /// Neutral/doji candle color (ARGB).
    pub neutral: u32,
}

/// Order visualization color scheme.
#[derive(Clone, Debug)]
pub struct IntColors {
    /// Buy order color (active, ARGB).
    pub buy_order_color_bu: u32,
    /// Sell color (ARGB).
    pub sell_color_u: u32,
    /// Buy order color (waiting/pending, ARGB).
    pub buy_order_color_wu: u32,
    /// Sell order color (ARGB).
    pub sell_order_color_u: u32,
    /// Completed buy order color (ARGB).
    pub buy_order_done_color_u: u32,
    /// Completed sell order color (ARGB).
    pub sell_order_done_color_u: u32,
    /// Trailing-stop line color (ARGB).
    pub trailing_color_u: u32,
    /// Price line width in pixels.
    pub price_line_width: i32,
}

/// Chart draw tool configuration (colors, stroke, alerts) per object kind.
#[derive(Clone, Debug)]
pub struct CustomDrawConfig {
    /// Internal version.
    pub ver: i32,
    /// Global figure opacity (0.0..1.0).
    pub f_opacity: f32,
    /// Show chart drawing tools.
    pub f_show: bool,
    /// Per-tool settings, indexed by `TChartObjectKind` (16 entries).
    pub tools: [CustomDrawTool; 16],
}

/// Settings for a single chart draw tool.
#[derive(Clone, Debug)]
pub struct CustomDrawTool {
    /// Primary line color (white/light theme, ARGB).
    pub color_w: u32,
    /// Primary line color (black/dark theme, ARGB).
    pub color_b: u32,
    /// Fill color (light theme, ARGB).
    pub fill_color_w: u32,
    /// Fill color (dark theme, ARGB).
    pub fill_color_b: u32,
    /// Trigger a sound alert when price crosses this tool's line.
    pub sound_alert: bool,
    /// Alert sound preset index.
    pub sound_kind: i32,
    /// Primary line thickness.
    pub thickness: f32,
    /// Primary line stroke style (`TStrokeDash` ordinal, 4 bytes on wire).
    pub stroke: i32,
    /// Secondary line thickness.
    pub thickness_2: f32,
    /// Secondary line stroke style (4 bytes on wire).
    pub stroke_2: i32,
    /// Secondary line colors (6 per theme, light theme).
    pub color_2w: [u32; 6],
    /// Secondary line colors (dark theme).
    pub color_2b: [u32; 6],
    /// Secondary fill colors (light theme).
    pub fill_color_2w: [u32; 6],
    /// Secondary fill colors (dark theme).
    pub fill_color_2b: [u32; 6],
    /// Emulate trades when price crosses this tool's line.
    pub emulate_trades: bool,
    /// Prevent chart-switch when drawing this tool.
    pub prevent_switch: bool,
}

/// BTC price blink-alert settings.
#[derive(Clone, Debug)]
pub struct BlinkConfig {
    /// Enable BTC price blink notification.
    pub blink_btc: bool,
    /// BTC drop threshold (%) to trigger blink.
    pub blink_btc_delta: f64,
    /// BTC rise threshold (%) to trigger blink.
    pub blink_btc_delta_up: f64,
    /// Enable audible BTC alarm.
    pub alarm_btc: bool,
    /// Alarm type (sound variant, 1 byte).
    pub alarm_type: u8,
}

/// Chart filter-bar visibility flags.
#[derive(Clone, Debug)]
pub struct ShowFilters {
    /// Show the filter bar at all.
    pub need_show: bool,
    /// Show strategy filters.
    pub filters: bool,
    /// Show session filters.
    pub sessions: bool,
    /// Hide zero-profit entries.
    pub hide_zero: bool,
    /// Show MoonShot area indicator.
    pub m_shot_area: bool,
    /// Show scale tool controls.
    pub scale_tool: bool,
    /// Enable scroll-through filters.
    pub scroll_filters: bool,
    /// Show detection events.
    pub show_detects: bool,
}

/// Chart windows manager settings.
#[derive(Clone, Debug)]
pub struct ChartsSettings {
    /// Auto-arrange chart windows on screen.
    pub auto_arrange: bool,
    /// Chart windows visible by default.
    pub visible: bool,
    /// Use wide (landscape) chart layout.
    pub wide: bool,
    /// Keep chart windows on top.
    pub stay_on_top: bool,
    /// Maximum number of simultaneous chart windows.
    pub max_charts: i32,
    /// Chart refresh interval (seconds).
    pub refresh: i32,
    /// Reserved slot, unused by current MoonBot code. Kept for wire
    /// compatibility.
    pub x1: f64,
    /// Reserved slot, unused by current MoonBot code. Kept for wire
    /// compatibility.
    pub x2: f64,
}

/// Performance heatmap display settings.
#[derive(Clone, Debug)]
pub struct HeatMapConfig {
    /// Show the heatmap bar.
    pub show: bool,
    /// Use quantile-based coloring.
    pub use_q: bool,
    /// Heatmap bar height (fraction of window, 0.0..1.0).
    pub height: f32,
    /// Show CPU load segment.
    pub cpu: bool,
    /// Show trade throughput segment.
    pub trades: bool,
    /// Show application latency segment.
    pub app_latency: bool,
    /// Show draw/render latency segment.
    pub draw_latency: bool,
}

/// News feed form display settings.
#[derive(Clone, Debug)]
pub struct NewsFormConfig {
    /// Keep the news window on top of other windows.
    pub stay_on_top: bool,
    /// Show exact timestamps (vs. relative "2m ago").
    pub exact_time: bool,
    /// News feed font size.
    pub font_size: u8,
    /// Minimum news strength filter.
    pub strength: u8,
    /// Show news translated into the current UI language instead of the
    /// original text (news form "Translate" checkbox).
    pub update_orig: bool,
    /// Enable sound for news alerts.
    pub sound: u8,
    /// News window theme (0=auto, 1=light, 2=dark).
    pub theme: i32,
    /// CoinCard font size.
    pub coin_card_font_size: i32,
    /// Show full tag list (vs. abbreviated).
    pub full_tags: bool,
    /// Use feed mode (continuous scroll) vs. card mode.
    pub feed_mode: bool,
}

// ---------------------------------------------------------------------------
// Section 4: Theme (Kind = 4, internal version = 1)
// ---------------------------------------------------------------------------

/// Active color theme and per-theme INI color overrides.
#[derive(Clone, Debug, Default)]
pub struct ThemeSection {
    /// Active theme (0 = light, 1 = dark).
    pub current_style: i32,
    /// INI sections for theme colors (`ColorsLight`, `ColorsDark`).
    /// Each entry is `(section_name, Vec<(key, value)>)`.
    pub ini_sections: Vec<IniSectionData>,
    /// Opaque bytes from a newer MoonBot version (preserved for roundtrip).
    pub unknown_tail: Vec<u8>,
}

// ---------------------------------------------------------------------------
// Section 5: Ini (Kind = 5, internal version = 1)
// ---------------------------------------------------------------------------

/// Chart indicator and arbitrage color INI settings.
#[derive(Clone, Debug, Default)]
pub struct IniSection {
    /// INI sections (`Charts`, `ArbColors`).
    /// Each entry is `(section_name, Vec<(key, value)>)`.
    pub ini_sections: Vec<IniSectionData>,
    /// Opaque bytes from a newer MoonBot version (preserved for roundtrip).
    pub unknown_tail: Vec<u8>,
}

/// A single INI section: a name and an ordered list of key/value pairs.
/// Order is preserved for byte-exact roundtrip (NOT a HashMap).
#[derive(Clone, Debug, PartialEq)]
pub struct IniSectionData {
    /// Section name (e.g. "ColorsLight", "Charts").
    pub name: String,
    /// Key/value pairs in the order they appeared on the wire.
    pub entries: Vec<(String, String)>,
}

// ---------------------------------------------------------------------------
// Section 6: UI (Kind = 6, internal version = 3)
// ---------------------------------------------------------------------------

/// Hotkeys, markets table layout, toolbar, strategy editor state.
#[derive(Clone, Debug)]
pub struct UiSection {
    /// Hide the demo-mode button.
    pub hide_demo_button: bool,
    /// Confirm before closing the application.
    pub confirm_close: bool,
    /// Show newly listed markets at the top of the list.
    pub new_markets_on_top: bool,
    /// Coin list sort order index.
    pub coins_sort_order: i32,

    /// Keyboard shortcuts for order operations.
    pub hotkeys_config: HotkeysConfig,

    /// Strategy editor chapter expansion state (comma-separated indices).
    pub strat_editor_chapters: String,

    /// Markets table column layout.
    pub markets_table_config: MarketsTableConfig,

    /// Index of the main toolbar button in slot 1
    /// (MBT_Help=1, MBT_Reports=2, MBT_Alerts=3, MBT_Transfer=4, MBT_BackTest=6).
    pub main_button_index_1: u8,
    /// Per-strategy expand/collapse state in the editor (11 slots).
    pub strat_expanded_state: [bool; 11],

    /// Opaque bytes from a newer MoonBot version (preserved for roundtrip).
    pub unknown_tail: Vec<u8>,
}

/// Keyboard shortcuts for order and chart operations.
#[derive(Clone, Debug)]
pub struct HotkeysConfig {
    /// Whether this config has been initialized by the user.
    pub filled: bool,
    /// Internal version.
    pub ver: u8,
    /// Buy order size presets (6 slots, in quote currency).
    pub o_size: [f64; 6],
    /// Selected buy-size preset slot (1-based).
    pub b_num: i32,
    /// Buy order hotkeys (6 slots, `TShortCut` = platform key code).
    pub o_keys: [u16; 6],
    /// Number of iceberg split parts.
    pub split_parts: u8,
    /// Selected sell-price preset slot (1-based).
    /// Mirrors [`ClientSettingsCommand::sb_num`].
    pub sb_num: u8,
    /// Sell order hotkeys (6 slots).
    pub s_keys: [u16; 6],
    /// Sell price presets as percentages (6 slots, e.g. [1,3,5,10,25,100]).
    /// Mirrors [`ClientSettingsCommand::s_price`].
    pub s_price: [f32; 6],
    /// Cancel buy hotkey.
    pub cancel_buy: u16,
    /// Panic sell hotkey.
    pub panic_sell: u16,
    /// Join sells hotkey.
    pub join_sells: u16,
    /// Switch active chart hotkey.
    pub switch_charts: u16,
    /// Reload orderbook hotkey.
    pub reload_book: u16,
    /// Open new long position hotkey.
    pub new_long: u16,
    /// Open new short position hotkey.
    pub new_short: u16,
    /// Split selected order hotkey.
    pub split_order: u16,
    /// Shift buy up hotkey.
    pub shift_buy_up: u16,
    /// Shift buy down hotkey.
    pub shift_buy_down: u16,
    /// Shift sell up hotkey.
    pub shift_sell_up: u16,
    /// Shift sell down hotkey.
    pub shift_sell_down: u16,
    /// Take screenshot hotkey.
    pub make_shot: u16,
    /// Take bot screenshot (with overlay) hotkey.
    pub make_shot_bot: u16,
    /// Reload chart data hotkey.
    pub reload_chart: u16,
    /// Zoom in hotkey.
    pub scale_plus: u16,
    /// Zoom out hotkey.
    pub scale_minus: u16,
    /// Increase sell price hotkey.
    pub sell_plus: u16,
    /// Decrease sell price hotkey.
    pub sell_minus: u16,
    /// Toggle spy/stealth mode hotkey.
    pub spy_mode: u16,
    /// Toggle chart visibility hotkey.
    pub show_charts: u16,
    /// Extended split-order hotkey.
    pub split_order_x: u16,
    /// Cycle draw tool hotkey.
    pub switch_figure: u16,
    /// Fit sells to orderbook hotkey.
    pub fit_sells: u16,
    /// Panic sell single position hotkey.
    pub panic_sell_one: u16,
    /// Cancel all buy orders hotkey.
    pub cancel_all_buys: u16,
    /// Broadcast command to thin-clients hotkey.
    pub broadcast: u16,
}

/// Markets table column layout.
#[derive(Clone, Debug)]
pub struct MarketsTableConfig {
    /// Active sort column index (-1 = no sort).
    pub sort_col: i32,
    /// Column visibility flags (41 columns).
    pub col_vis: [bool; 41],
    /// Column position indices (41 columns).
    pub col_pos: [u8; 41],
}

// =========================================================================
// Default implementations
// =========================================================================

impl Default for SharedConfig {
    fn default() -> Self {
        Self {
            config_version: 164,
            signals: SignalsSection::default(),
            trading: TradingSection::default(),
            visual: VisualSection::default(),
            theme: ThemeSection::default(),
            ini: IniSection::default(),
            ui: UiSection::default(),
        }
    }
}

impl Default for SignalConfig {
    fn default() -> Self {
        Self {
            ver: 1,
            use_keywords: true,
            use_token_tags: true,
            buy_key_dist: 1,
            only_1_token: true,
            buy_if_price_found: false,
            x_found_price: -2,
            use_black_words: true,
            words_count: 10,
            use_words_count: false,
            use_price: true,
            use_stops: false,
            tokens_no_tags: false,
            token_links: true,
            use_lower_price_words: false,
            x_lower_price: 0,
            special_formats: false,
        }
    }
}

impl Default for SignalsSection {
    fn default() -> Self {
        Self {
            pump_channel: String::new(),
            pump_channels: Vec::new(),
            do_monitoring: false,
            look_full_link_tlg: true,
            multi_channels: false,
            telegram_auto_buy: false,
            load_deep_history: true,
            only_secret_signal: false,
            more_then_1_channel: false,
            clipboard_auto_buy: false,
            autodetect_auto_buy: false,
            signal_sound: 1,
            play_signal_sound: false,
            lower_case_token_tlg: false,
            show_cross: true,
            monitor_clipboard: false,
            lower_case_token_cbd: false,
            look_full_link_cbd: true,
            msg_keywords_long: "buy".into(),
            msg_keywords_short: "short".into(),
            msg_token_tags: "#,$".into(),
            msg_black_words: "called,gave,told,dont,don't,reached".into(),
            signal_config: SignalConfig::default(),
            advanced_filter: true,
            auto_show_on_signal: false,
            advanced_filter_clipboard: true,
            base_vol_ind_type: 0,
            max_too_long_msg_len: 0,
            signal_sound_2: 2,
            sell_alert_level: 1,
            play_sell_alert: false,
            last_used_order_type: 1,
            full_screen_prevent_signals: true,
            listen_moon_channel: false,
            lower_price_words: "wait dip,for dip,when dump,when dumped".into(),
            dont_buy_reply: true,
            base_vol_n: 100,
            use_last_detect_caption: true,
            buy_signal_sound: 2,
            play_buy_alert: false,
            buy_alert_level: 0,
            strats_filter: String::new(),
            news_tags_filter: String::new(),
            news_tokens_filter: String::new(),
            unknown_tail: Vec::new(),
        }
    }
}

impl Default for AutoStartConfig {
    fn default() -> Self {
        Self {
            auto_start: false,
            auto_detect_on: false,
            strategies_on: false,
            work_time: false,
            auto_stop_if_loss: true,
            remember_state: false,
            sell_if_loss: false,
            dont_wait_sells: false,
            auto_stop_loss: 0.003,
            panic_btc: false,
            panic_market: false,
            auto_stop_if_loss_hours: true,
            auto_update: false,
            restart_after_err: false,
            restart_after_ping: false,
            ignore_emulator: false,
            stop_trades: 50,
            restart_err_time: 60,
            panic_btc_delta: -5.0,
            panic_market_delta: 0.0,
            auto_stop_on_errors: true,
            auto_stop_on_ping: false,
            sell_all_on_errors: false,
            sell_all_on_ping: false,
            errors_level: 3,
            ping_level: 1000,
            restart_ping_time: 30,
            auto_stop_hours_val: 0.003,
            stop_hours: 12,
            stop_hours_trades: 0,
            panic_btc_delta_up: 5.0,
            work_time_from: 0.0,
            work_time_to: 0.9999,
        }
    }
}

impl Default for AutoStartConfig2 {
    fn default() -> Self {
        Self {
            restart_on_market: false,
            btc_higher_then: -0.5,
            btc_lower_then: 0.5,
            market_higher_then: -0.1,
            show_old_listing: false,
            reset_session: false,
            max_session_cap: 0,
            rs_hours: 0,
        }
    }
}

impl Default for MoonBotConfig {
    fn default() -> Self {
        Self {
            silent: false,
            auto_margin_borrow: false,
            auto_margin_transfer: false,
            group_delay: 1500,
        }
    }
}

impl Default for ReportConfig {
    fn default() -> Self {
        Self {
            fields_vis: [true; 51],
            fields_width: [0u16; 51],
            only_active: false,
            range: 0,
            filter1: [0u8; 26],
            filter2: [0u8; 26],
            filter3: [0u8; 26],
            active_type: 3,
            rep_emulator_orders: false,
            stretch: false,
            only_2_orders: false,
            use_leverage: true,
            max_lines: 250,
            pos_direction: 0,
            store_liq: false,
            store_fund: false,
            by_close_date: false,
        }
    }
}

impl Default for MultiOrdersConfig {
    fn default() -> Self {
        Self {
            ver: 1,
            use_multi_orders: true,
            buy_set_click: 1,     // Dbl_Click
            buy_move_click: 3,    // Shift_Click
            sell_move_click: 2,   // CTRL_Click
            replace_buy_kind: 3,  // RM_LowVol
            replace_sell_kind: 2, // RM_TopVol
            split_sells: 0,
            show_orders_num: true,
            kir_style: false,
            fix_pos: 0,        // FP_Both
            join_sell_kind: 0, // JS_None
            short_set_click: 0,
            pending_short_set_click: 0,
            done_opacity: 0.5,
            buy_move_click_2: 0,
            sell_move_click_2: 0,
            replace_buy_kind_2: 0,
            replace_sell_kind_2: 0,
            same_hotkeys_for_move: true,
            short_buy_move_click: 3,  // =BuyMoveClick default
            short_sell_move_click: 2, // =SellMoveClick default
            short_buy_move_click_2: 0,
            short_sell_move_click_2: 0,
        }
    }
}

impl Default for AlertConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            keep_time: 30,
            sound_kind: 2,
            repeat_count: 2,
        }
    }
}

impl Default for SpotConfig {
    fn default() -> Self {
        Self {
            show_trades: true,
            show_book: true,
            book_len: 70.0,
            show_market_avg: false,
            show_min_max: false,
            show_avg_price: false,
            spot_btc: false,
            shift_spot: false,
            show_mark_price: true,
            show_liq: true,
            huge_liq: false,
            show_open_int: true,
            show_avg_price_line: false,
        }
    }
}

impl Default for OrdersControl {
    fn default() -> Self {
        Self {
            active: false,
            min_price: 1.0,
            max_time: 300,
            h_pos_control: false,
            h_pos_report: false,
            h_pos_auto_sell: false,
            ignore_replacing_bug: false,
            ignore_protection: 0,
            sign_orders: true,
            liq_control: false,
        }
    }
}

impl Default for AssetTransferConfig {
    fn default() -> Self {
        Self {
            show_total: true,
            hide_zero: true,
            show_baks: false,
        }
    }
}

impl Default for AutoManageLevConfig {
    fn default() -> Self {
        Self {
            auto_max_order: false,
            auto_lev_up: true,
            fix_lev: 0,
            auto_isolated: false,
            tlg_report: true,
            auto_cross: false,
            auto_fix_lev: false,
        }
    }
}

impl Default for SendShotsConfig {
    fn default() -> Self {
        Self {
            may_send: false,
            profit_abs: 100,
            profit_pers: 25,
            profit_session: 500,
            send_negative: false,
            send_to_open_chat: true,
            send_public: true,
            time_scale: 200,
            price_scale: 0,
        }
    }
}

impl Default for ManualStratsConfig {
    fn default() -> Self {
        Self {
            use_buttons: true,
            hot_keys: [0u16; 10],
            show_button: [false; 10],
        }
    }
}

impl Default for ArbViewConfig {
    fn default() -> Self {
        Self {
            ver: 1,
            show_absolute: false,
            show_numbers: false,
            show_lines: true,
            show_percent: true,
            show_right: false,
        }
    }
}

impl Default for TradingSection {
    fn default() -> Self {
        Self {
            auto_delete_logs: 7,
            log_level: 3,
            binance_connection: 0,
            auto_start: AutoStartConfig::default(),
            auto_start_2: AutoStartConfig2::default(),
            fav_markets: String::new(),
            x_ask: 0,
            x_sell: 5,
            x_sell_scalp: 10,
            play_with: 20,
            check_glass: false,
            max_orders: 30,
            auto_sell_partial: 100,
            price_range: 1,
            sell_auto_move: false,
            pump_q_level: 30,
            sell_x2_level: 80,
            cancel_buy_on_sell_fill: false,
            auto_cancel_buy_order: 20,
            vol_drop_level: 7,
            price_drop_level: -1.0,
            panic_if_vol_drop: false,
            panic_if_price_drop: false,
            manual_mode: false,
            draw_all_buy: false,
            trailing_stop: false,
            trailing_drop: -2.0,
            engine_check_buy: false,
            use_manual_balance: true,
            fixed_trade_balance: 100.0,
            use_current_ask: false,
            g_take_profit: 2.0,
            use_g_take_profit: true,
            show_order_comment: false,
            chart_clean_up_time: 30,
            moonbot_config: MoonBotConfig::default(),
            dbl_click_panic_sell: false,
            coins_black_list_text: String::new(),
            use_coins_black_list: false,
            h_pos_black_list_text: String::new(),
            trailing_float: 0.0,
            h_pos_black_list_add: String::new(),
            random_price: false,
            report_config: ReportConfig::default(),
            dont_buy_delisted: true,
            dont_buy_new_coins: 60,
            auto_cancel_lower_buy: 180,
            buy_on_enter: true,
            auto_buy_bnb: false,
            auto_buy_bnb_level: 1.0,
            auto_buy_bnb_volume: 2.0,
            order_replace_click_buy: 0,
            dont_buy_forward: true,
            chart_split_zones: false,
            draw_stop: true,
            max_order: 0.0,
            unlimited_orders: false,
            use_manual_strategy: false,
            manual_strategy: String::new(),
            x9_mode: false,
            x_t_mode: false,
            fixed_balance_warning: false,
            trades_gap_time: 7000,
            buy_iceberg: false,
            sell_iceberg: true,
            iceberg_step: 0.1,
            order_set_click: 1, // Dbl_Click
            pending_order_set_click: 0,
            pending_orders_spread: 0.5,
            pending_orders_spread_h_delta: 0.1,
            buy_pending_spread: 0,
            buy_stop_pending_spread: 0,
            tm_panic_sell_closed: true,
            tm_send_only_share: false,
            tm_cancel_closed: false,
            tm_cancel_buy_on_fill: false,
            multi_commands: false,
            iceberg_click_count: 0,
            tm_send_first_sell: true,
            tm_sell_if_master_not_filled: false,
            tm_max_order: 0.0,
            multi_orders: MultiOrdersConfig::default(),
            order_replace_click_sell: 2, // CTRL_Click
            alert_config: AlertConfig::default(),
            use_moon_bl: true,
            correct_order_price: false,
            f_binance_commission: 0.0,
            spot_config: SpotConfig::default(),
            use_book_ticker: false,
            exclude_black_list_delta: false,
            free_position_check: false,
            orders_control: OrdersControl::default(),
            m_avg_use_vol_weight: true,
            pending_buy_price: true,
            fixed_sell_mode: true,
            fixed_sell_price: 1.0,
            futures_rules: true,
            cashback_settings: CashBackSettings::default(),
            auto_lower_lev: false,
            transfer_config: AssetTransferConfig::default(),
            min_balance: -1.0,
            auto_manage_lev: AutoManageLevConfig::default(),
            auto_lev_control: String::new(),
            use_stop_market: false,
            auto_close_zero_pos: false,
            send_shots_config: SendShotsConfig::default(),
            use_lev_for_take: false,
            bybit_commission: 1.001,
            auto_reduce_order: false,
            gate_commission: 1.001,
            manual_strats_names: Default::default(),
            manual_strats_config: ManualStratsConfig::default(),
            clear_triggers_string: "1-200".into(),
            no_trades_markets_text: String::new(),
            use_websocket_api: false,
            order_book_levels_ws: 2,
            arb_view_config: ArbViewConfig::default(),
            deltas_by_trades: false,
            ignore_strat_sell_price: false,
            use_hl_fast_ioc: false,
            unknown_tail: Vec::new(),
        }
    }
}

impl Default for IntColors {
    fn default() -> Self {
        Self {
            buy_order_color_bu: 0xFF000000,
            sell_color_u: 0xFF0000FF,
            buy_order_color_wu: 0xFFD2691E,
            sell_order_color_u: 0xFF0000FF,
            buy_order_done_color_u: 0,
            sell_order_done_color_u: 0,
            trailing_color_u: 0,
            price_line_width: 1,
        }
    }
}

impl Default for CustomDrawTool {
    fn default() -> Self {
        Self {
            color_w: 0,
            color_b: 0,
            fill_color_w: 0,
            fill_color_b: 0,
            sound_alert: false,
            sound_kind: 0,
            thickness: 1.0,
            stroke: 0, // Solid
            thickness_2: 1.0,
            stroke_2: 0,
            color_2w: [0; 6],
            color_2b: [0; 6],
            fill_color_2w: [0; 6],
            fill_color_2b: [0; 6],
            emulate_trades: false,
            prevent_switch: false,
        }
    }
}

impl Default for CustomDrawConfig {
    fn default() -> Self {
        Self {
            ver: 2,
            f_opacity: 1.0,
            f_show: false,
            tools: std::array::from_fn(|_| CustomDrawTool::default()),
        }
    }
}

impl Default for BlinkConfig {
    fn default() -> Self {
        Self {
            blink_btc: false,
            blink_btc_delta: 0.0,
            blink_btc_delta_up: 0.0,
            alarm_btc: false,
            alarm_type: 0,
        }
    }
}

impl Default for ShowFilters {
    fn default() -> Self {
        Self {
            need_show: true,
            filters: true,
            sessions: true,
            hide_zero: true,
            m_shot_area: true,
            scale_tool: true,
            scroll_filters: false,
            show_detects: true,
        }
    }
}

impl Default for ChartsSettings {
    fn default() -> Self {
        Self {
            auto_arrange: true,
            visible: true,
            wide: true,
            stay_on_top: true,
            max_charts: 0,
            refresh: 3,
            x1: 0.0,
            x2: 0.0,
        }
    }
}

impl Default for HeatMapConfig {
    fn default() -> Self {
        Self {
            show: false,
            use_q: false,
            height: 0.15,
            cpu: false,
            trades: false,
            app_latency: false,
            draw_latency: false,
        }
    }
}

impl Default for NewsFormConfig {
    fn default() -> Self {
        Self {
            stay_on_top: true,
            exact_time: false,
            font_size: 0,
            strength: 0,
            update_orig: false,
            sound: 0,
            theme: 0,
            coin_card_font_size: 0,
            full_tags: true,
            feed_mode: false,
        }
    }
}

impl Default for VisualSection {
    fn default() -> Self {
        Self {
            colors: IntColors::default(),
            panic_sell_opacity: 25,
            hide_forum_label: false,
            show_red_green_lines: true,
            scrolling_charts: false,
            startup_load_charts: false,
            auto_close_charts: true,
            left_chart_info: false,
            show_red_green_dots: true,
            glass_opacity: 5,
            chart_time_scale: 60,
            chart_watch_multi_frames: true,
            chart_history_len: 0,
            hide_right_chart_panel: false,
            show_vert_volume: false,
            vert_vol_time_frame: 0,
            vert_vol_opacity: 0.3,
            vert_vol_height: 0.3,
            vert_vol_kind: 0,
            v_tool_show: false,
            v_tool_time: 0.0,
            v_tool_opacity: 0.2,
            v_tool_show_buyers: true,
            v_tool_time_index: 0,
            hv_show: false,
            hv_width: 0.2,
            hv_time_frame_ind: 0,
            hv_time_frame: 0,
            hv_price_frame: 0.1,
            hv_opacity: 0.3,
            custom_draw_config: CustomDrawConfig::default(),
            global_current_draw_kind: 6, // CO_Pencil
            hv_disp_vol: 0,
            hv_kind: 0,
            show_market_captions: false,
            show_orders_captions: true,
            orders_captions_lower: false,
            show_usd_on_charts: true,
            new_markets_max_scale: false,
            vert_vol_ind_pos: 0,
            book_cumulative_opacity: 100,
            book_orders_opacity: 0,
            book_orders_width: 3,
            blink_config: BlinkConfig::default(),
            charts_auto_shift: true,
            show_iceberg: true,
            hide_pnl: false,
            draw_cur_price: 0,
            auto_request_charts: true,
            vo_sort_kind: 2, // SK_Creation
            vo_older_first: true,
            vo_sort_by_market: false,
            hide_buy_button: false,
            show_strat_numbers: true,
            show_filters: ShowFilters::default(),
            hide_cashback_button: false,
            hide_ny_elka: false,
            charts_settings: ChartsSettings::default(),
            spy_hide_mode: false,
            heatmap_config: HeatMapConfig::default(),
            icon_selection: 1,
            remember_chart_buttons: false,
            show_detects_tool: false,
            scale_plus_index: 0,
            scale_minus_index: 0,
            news_form_config: NewsFormConfig::default(),
            font_sizes: [0u8; 20],
            chart_candles_style: 2, // cdsContoursOverTicks
            chart_candles_tick_opacity: 25,
            chart_candles_neutral_ticks: false,
            chart_candles_outline_width: 2,
            chart_candles_tick_wicks: false,
            candle_colors: [CandleColorSet::default(), CandleColorSet::default()],
            use_ai_coin_card: false,
            ai_card_provider: 0,
            ai_card_model: String::new(),
            ai_card_prompt: String::new(),
            manual_charts_full_screen: false,
            unknown_tail: Vec::new(),
        }
    }
}

impl Default for HotkeysConfig {
    fn default() -> Self {
        Self {
            filled: false,
            ver: 1,
            o_size: [0.0; 6],
            b_num: 1,
            o_keys: [0u16; 6],
            split_parts: 0,
            sb_num: 1,
            s_keys: [0u16; 6],
            s_price: [1.0, 3.0, 5.0, 10.0, 25.0, 100.0],
            cancel_buy: 0,
            panic_sell: 0,
            join_sells: 0,
            switch_charts: 0,
            reload_book: 0,
            new_long: 0,
            new_short: 0,
            split_order: 0,
            shift_buy_up: 0,
            shift_buy_down: 0,
            shift_sell_up: 0,
            shift_sell_down: 0,
            make_shot: 0,
            make_shot_bot: 0,
            reload_chart: 0,
            scale_plus: 0,
            scale_minus: 0,
            sell_plus: 0,
            sell_minus: 0,
            spy_mode: 0,
            show_charts: 0,
            split_order_x: 0,
            switch_figure: 0,
            fit_sells: 0,
            panic_sell_one: 0,
            cancel_all_buys: 0,
            broadcast: 0,
        }
    }
}

impl Default for MarketsTableConfig {
    fn default() -> Self {
        Self {
            sort_col: -1,
            col_vis: [true; 41],
            col_pos: [0u8; 41],
        }
    }
}

impl Default for UiSection {
    fn default() -> Self {
        Self {
            hide_demo_button: true,
            confirm_close: true,
            new_markets_on_top: false,
            coins_sort_order: 1,
            hotkeys_config: HotkeysConfig::default(),
            strat_editor_chapters: String::new(),
            markets_table_config: MarketsTableConfig::default(),
            main_button_index_1: 1, // MBT_Help
            strat_expanded_state: [true; 11],
            unknown_tail: Vec::new(),
        }
    }
}
