//! Shared-config data structures — one struct per wire section.
//!
//! Every public field carries a doc-comment explaining what it does from the
//! user's perspective.

// ---------------------------------------------------------------------------
// Top-level aggregate
// ---------------------------------------------------------------------------

/// Full shared-config payload.  Six mandatory sections plus the catalog
/// version that was written into the wire header.
///
/// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/09-config-structure.md#sharedconfig).
#[derive(Clone, Debug)]
pub struct SharedConfig {
    /// Core configuration catalog version carried by the source snapshot.
    ///
    /// This is not the safe-share format version. It is retained for exact
    /// roundtrips and is intentionally not editable by application code.
    pub(crate) config_version: u16,
    /// Signal parsing, sounds, filters and several saved preferences; see the signals chapter.
    ///
    /// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/09-config-structure.md#sharedconfig-signals).
    pub signals: SignalsSection,
    /// Shared trading policy, risk controls, manual execution and nested settings.
    ///
    /// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/09-config-structure.md#sharedconfig-trading).
    pub trading: TradingSection,
    /// Saved chart and GUI presentation settings on the kernel. A remote terminal must implement its own renderer.
    ///
    /// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/09-config-structure.md#sharedconfig-visual).
    pub visual: VisualSection,
    /// Main GUI style and allowlisted color overrides.
    ///
    /// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/09-config-structure.md#sharedconfig-theme).
    pub theme: ThemeSection,
    /// Allowlisted Charts and ArbColors INI sections. Not access to arbitrary core configuration files.
    ///
    /// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/09-config-structure.md#sharedconfig-ini).
    pub ini: IniSection,
    /// Hotkeys, chart ordering, table layout and strategy-editor expansion state.
    ///
    /// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/09-config-structure.md#sharedconfig-ui).
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
///
/// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/02-signals-and-alerts.md#signalssection).
#[derive(Clone, Debug)]
pub struct SignalsSection {
    // --- Pump channels ---
    /// Selected Telegram signal-channel name, or comma-separated selected names in multi-channel mode. Channel
    /// selection does not authenticate Telegram or start trading by itself.
    ///
    /// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/02-signals-and-alerts.md#signalssection-pump-channel).
    pub pump_channel: String,
    /// Saved list of channel choices shown by the channel selector. The active selection is `pump_channel`; listing
    /// a channel here alone does not subscribe a strategy to it.
    ///
    /// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/02-signals-and-alerts.md#signalssection-pump-channels).
    pub pump_channels: Vec<String>,

    /// Legacy stored flag without an active signal-processing consumer in the current kernel. Preserve it; do not
    /// expose it as a working master enable switch.
    ///
    /// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/02-signals-and-alerts.md#signalssection-do-monitoring).
    pub do_monitoring: bool,
    /// In the simple Telegram parser, require an exchange-market link. When false, ticker-text recognition is also
    /// attempted. Not used by the advanced parser.
    ///
    /// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/02-signals-and-alerts.md#signalssection-look-full-link-tlg).
    pub look_full_link_tlg: bool,
    /// Enable multiple selections in the channel picker. Together with `more_then_1_channel`, enables
    /// repeated-signal confirmation.
    ///
    /// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/02-signals-and-alerts.md#signalssection-multi-channels).
    pub multi_channels: bool,
    /// Allow automatic buys from ordinary Telegram signals. A matching Telegram strategy can supply its own
    /// auto-buy decision; this is not a master switch for all strategies.
    ///
    /// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/02-signals-and-alerts.md#signalssection-telegram-auto-buy).
    pub telegram_auto_buy: bool,
    /// Load and analyze market history on kernel startup. The normal auto-start path enables this if necessary;
    /// changing it is not a one-shot history-refresh command.
    ///
    /// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/02-signals-and-alerts.md#signalssection-load-deep-history).
    pub load_deep_history: bool,
    /// Legacy simple-parser option requiring the historical "X secret signal" marker in recognized links. No normal
    /// settings-page control; preserve the received value.
    ///
    /// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/02-signals-and-alerts.md#signalssection-only-secret-signal).
    pub only_secret_signal: bool,
    /// With `multi_channels`, wait for a second accepted signal for the same coin within 3 seconds. The
    /// implementation counts messages, not distinct channel identities; do not promise confirmation by two
    /// independent channels.
    ///
    /// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/02-signals-and-alerts.md#signalssection-more-then-1-channel).
    pub more_then_1_channel: bool,
    /// Allow automatic buys from text copied to the kernel machine's clipboard, unless `manual_mode` is active.
    /// Independent of the terminal application's clipboard; not a remote clipboard subscription.
    ///
    /// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/02-signals-and-alerts.md#signalssection-clipboard-auto-buy).
    pub clipboard_auto_buy: bool,
    /// Legacy flag: current detection and order-execution paths do not read it, and the GUI control is hidden.
    /// Configure auto-buy on the relevant strategies instead.
    ///
    /// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/02-signals-and-alerts.md#signalssection-autodetect-auto-buy).
    pub autodetect_auto_buy: bool,
    /// Connectivity-alert sound selected by `play_signal_sound`. 1-based sound number (GUI 1..=11); 0 means no
    /// sound. Not the strategy signal-sound selector.
    ///
    /// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/02-signals-and-alerts.md#signalssection-signal-sound).
    pub signal_sound: i32,
    /// Play `signal_sound` for connection problems or high API-load indication, at most once per 5 seconds. Does
    /// not enable sounds for every incoming trading signal.
    ///
    /// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/02-signals-and-alerts.md#signalssection-play-signal-sound).
    pub play_signal_sound: bool,
    /// Allow lowercase ticker text in the simple Telegram parser by normalizing text to uppercase. Has no effect
    /// when `advanced_filter` selects the advanced parser.
    ///
    /// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/02-signals-and-alerts.md#signalssection-lower-case-token-tlg).
    pub lower_case_token_tlg: bool,
    /// Show the chart crosshair in the Moonbot GUI. Does not change signal detection or order execution.
    ///
    /// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/02-signals-and-alerts.md#signalssection-show-cross).
    pub show_cross: bool,
    /// Process text from the kernel machine's clipboard for chart/signal selection. `clipboard_auto_buy` separately
    /// permits buying; `clipboard_auto_buy` also activates processing when this flag is false.
    ///
    /// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/02-signals-and-alerts.md#signalssection-monitor-clipboard).
    pub monitor_clipboard: bool,
    /// Allow lowercase ticker text in the simple clipboard parser. Not used when `advanced_filter_clipboard` is
    /// enabled.
    ///
    /// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/02-signals-and-alerts.md#signalssection-lower-case-token-cbd).
    pub lower_case_token_cbd: bool,
    /// Require an exchange-market link in the simple clipboard parser. When false, also recognize ticker text.
    /// Advanced clipboard parsing uses `signal_config` instead.
    ///
    /// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/02-signals-and-alerts.md#signalssection-look-full-link-cbd).
    pub look_full_link_cbd: bool,

    // --- Signal keyword strings (comma-separated) ---
    /// Comma-separated long/buy keywords for `signal_config.use_keywords`. Matching is case-insensitive; a keyword
    /// also identifies the signal type for strategy selection.
    ///
    /// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/02-signals-and-alerts.md#signalssection-msg-keywords-long).
    pub msg_keywords_long: String,
    /// Comma-separated short keywords for `signal_config.use_keywords`. Considered on futures; does not enable
    /// short trading on spot.
    ///
    /// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/02-signals-and-alerts.md#signalssection-msg-keywords-short).
    pub msg_keywords_short: String,
    /// Comma-separated ticker markers, for example #,$. Used by the configurable advanced token parser; markers may
    /// surround, precede or follow a ticker.
    ///
    /// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/02-signals-and-alerts.md#signalssection-msg-token-tags).
    pub msg_token_tags: String,
    /// Comma-separated forbidden words or phrases for `signal_config.use_black_words`. A match rejects the parsed
    /// signal.
    ///
    /// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/02-signals-and-alerts.md#signalssection-msg-black-words).
    pub msg_black_words: String,

    /// Shared advanced-parser rules used when `advanced_filter` or `advanced_filter_clipboard` is enabled. Both
    /// sources use the same rule record.
    ///
    /// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/02-signals-and-alerts.md#signalssection-signal-config).
    pub signal_config: SignalConfig,

    /// Select the advanced Telegram message parser with keyword, ticker, price and stop rules from `signal_config`.
    /// Does not itself enable auto-buy.
    ///
    /// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/02-signals-and-alerts.md#signalssection-advanced-filter).
    pub advanced_filter: bool,
    /// Bring the Moonbot GUI/chart forward for a signal or order alert. Display preference; does not grant
    /// permission to trade.
    ///
    /// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/02-signals-and-alerts.md#signalssection-auto-show-on-signal).
    pub auto_show_on_signal: bool,
    /// Use `signal_config` for clipboard text; otherwise use the simple link/ticker parser with
    /// `look_full_link_cbd` and `lower_case_token_cbd`.
    ///
    /// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/02-signals-and-alerts.md#signalssection-advanced-filter-clipboard).
    pub advanced_filter_clipboard: bool,
    /// Chart Bv/Sv display interval: -2 = last N minutes, -1 = last N trades, 0 = pre-pump view, or 1, 3, 5, 15, 30
    /// minutes. N comes from `base_vol_n`. This is not a minimum-volume trading filter.
    ///
    /// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/02-signals-and-alerts.md#signalssection-base-vol-ind-type).
    pub base_vol_ind_type: i32,
    /// Legacy character-limit field with no active consumer. Preserve it; the active advanced-parser limit is
    /// `signal_config.words_count` with `use_words_count`.
    ///
    /// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/02-signals-and-alerts.md#signalssection-max-too-long-msg-len).
    pub max_too_long_msg_len: i32,
    /// Sell-alert sound for `play_sell_alert`. 1-based sound number (GUI 1..=11); 0 means no sound.
    ///
    /// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/02-signals-and-alerts.md#signalssection-signal-sound-2).
    pub signal_sound_2: i32,
    /// Sell-price proximity in percent, GUI 0..=20. With `play_sell_alert`, 0 alerts on completion; positive values
    /// alert as price approaches a sell order after it has been open for 30 seconds, with a 5-second repeat
    /// interval.
    ///
    /// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/02-signals-and-alerts.md#signalssection-sell-alert-level).
    pub sell_alert_level: i32,
    /// Enable sell alerts using `signal_sound_2` and `sell_alert_level`. This only controls notification, not
    /// execution.
    ///
    /// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/02-signals-and-alerts.md#signalssection-play-sell-alert).
    pub play_sell_alert: bool,
    /// Remembered pending-order dialog choice: 0 = BUY, 1 = BUY STOP. Zero-based GUI state, not a protocol
    /// order-side enum.
    ///
    /// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/02-signals-and-alerts.md#signalssection-last-used-order-type).
    pub last_used_order_type: i32,
    /// Keep signal-created charts from interrupting work on an existing full-screen chart. The GUI also protects
    /// orderbook interaction independently. Does not disable trading signals.
    ///
    /// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/02-signals-and-alerts.md#signalssection-full-screen-prevent-signals).
    pub full_screen_prevent_signals: bool,
    /// Accept the built-in Moonbot signal channel in addition to the ordinary selected-channel checks. Telegram
    /// service/account and channel availability are still required.
    ///
    /// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/02-signals-and-alerts.md#signalssection-listen-moon-channel).
    pub listen_moon_channel: bool,
    /// Comma-separated dip-buy phrases, such as wait dip. With `signal_config.use_lower_price_words`, switch the
    /// non-strategy signal to `x_lower_price` and `auto_cancel_lower_buy`; do not interpret this as a blacklist.
    ///
    /// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/02-signals-and-alerts.md#signalssection-lower-price-words).
    pub lower_price_words: String,
    /// Reject Telegram trading signals that reply to another message. Does not prevent parsing ordinary clipboard
    /// text.
    ///
    /// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/02-signals-and-alerts.md#signalssection-dont-buy-reply).
    pub dont_buy_reply: bool,
    /// N for the chart's N-trades or N-minutes Bv/Sv display, selected by `base_vol_ind_type`. Market
    /// initialization enforces at least 2; the minutes view uses at most 1440. Not a 24-hour-volume threshold.
    ///
    /// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/02-signals-and-alerts.md#signalssection-base-vol-n).
    pub base_vol_n: i32,
    /// Replace the chart's stored signal caption with the latest detection caption. When false, retain an existing
    /// caption instead of replacing it on each detection.
    ///
    /// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/02-signals-and-alerts.md#signalssection-use-last-detect-caption).
    pub use_last_detect_caption: bool,
    /// Buy-alert sound for `play_buy_alert`. 1-based sound number (GUI 1..=11); 0 means no sound.
    ///
    /// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/02-signals-and-alerts.md#signalssection-buy-signal-sound).
    pub buy_signal_sound: i32,
    /// Enable buy-completion alerts. A positive `buy_alert_level` also enables price-proximity alerts while waiting
    /// for the buy.
    ///
    /// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/02-signals-and-alerts.md#signalssection-play-buy-alert).
    pub play_buy_alert: bool,
    /// Buy-price proximity in percent, GUI 0..=20. A positive value enables proximity alerts with a 5-second repeat
    /// interval; 0 leaves only the completion alert. Requires `play_buy_alert`.
    ///
    /// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/02-signals-and-alerts.md#signalssection-buy-alert-level).
    pub buy_alert_level: i32,
    /// Saved search text in the strategy list: matches a strategy's full name, or recognized strategy-type
    /// shortcuts. Filters displayed rows; does not enable/disable strategy execution.
    ///
    /// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/02-signals-and-alerts.md#signalssection-strats-filter).
    pub strats_filter: String,
    /// Selected MoonNews tags separated by newline (LF). Empty means all tags; feed mode bypasses the tag filter.
    /// Not comma-separated.
    ///
    /// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/02-signals-and-alerts.md#signalssection-news-tags-filter).
    pub news_tags_filter: String,
    /// MoonNews coin filter, separated by commas or spaces and matched case-insensitively. Empty means all coins;
    /// any matching coin passes. Display filter, not a trading blacklist.
    ///
    /// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/02-signals-and-alerts.md#signalssection-news-tokens-filter).
    pub news_tokens_filter: String,

    /// Opaque extension bytes from a newer kernel. Preserve exactly when editing or sharing a snapshot; never
    /// expose as an editable setting.
    ///
    /// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/02-signals-and-alerts.md#signalssection-unknown-tail).
    pub unknown_tail: Vec<u8>,
}

/// Telegram/clipboard message parser sub-config.
///
/// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/02-signals-and-alerts.md#signalconfig).
#[derive(Clone, Debug)]
pub struct SignalConfig {
    /// Record version. Preserve the received value; not a user setting.
    ///
    /// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/02-signals-and-alerts.md#signalconfig-ver).
    pub ver: i32,
    /// Require a configured long/short keyword in the advanced parser. A recognized exchange-market link can also
    /// satisfy this check. Short keywords are considered on futures.
    ///
    /// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/02-signals-and-alerts.md#signalconfig-use-keywords).
    pub use_keywords: bool,
    /// Enable configurable token tags from `msg_token_tags` in the advanced parser. Tags may precede or follow a
    /// ticker; `tokens_no_tags` separately permits untagged uppercase tickers.
    ///
    /// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/02-signals-and-alerts.md#signalconfig-use-token-tags).
    pub use_token_tags: bool,
    /// Maximum number of intervening words between a keyword and a ticker. Used with `use_keywords`; 0 means
    /// adjacent, values >=10 disable the distance limit. GUI range 0..=16.
    ///
    /// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/02-signals-and-alerts.md#signalconfig-buy-key-dist).
    pub buy_key_dist: i32,
    /// Reject advanced-parser messages containing more than one candidate coin. BTC and ETH are treated as
    /// quote/context symbols when another candidate exists; repeated mentions of the same coin do not count as
    /// different coins.
    ///
    /// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/02-signals-and-alerts.md#signalconfig-only-1-token).
    pub only_1_token: bool,
    /// Require a recognizable entry price in the message. This only filters messages; `use_price` separately
    /// selects that price for the order. The parser searches numeric values near the current market price, not
    /// arbitrary numbers.
    ///
    /// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/02-signals-and-alerts.md#signalconfig-buy-if-price-found).
    pub buy_if_price_found: bool,
    /// Entry-price adjustment in percent, applied to the price parsed from the message when `use_price` is enabled.
    /// Uses the normal direction-aware price adjustment; unlike `x_lower_price`, one unit here is one percent.
    ///
    /// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/02-signals-and-alerts.md#signalconfig-x-found-price).
    pub x_found_price: i32,
    /// Reject a parsed signal when a configured `msg_black_words` entry is found. Uses the advanced
    /// Telegram/clipboard parser.
    ///
    /// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/02-signals-and-alerts.md#signalconfig-use-black-words).
    pub use_black_words: bool,
    /// Maximum counted words in a message, used with `use_words_count`. GUI range 2..=50. Numeric values and
    /// one-character items are excluded from this counter.
    ///
    /// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/02-signals-and-alerts.md#signalconfig-words-count).
    pub words_count: i32,
    /// Reject messages whose parsed word count exceeds `words_count`. Does not limit raw character length.
    ///
    /// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/02-signals-and-alerts.md#signalconfig-use-words-count).
    pub use_words_count: bool,
    /// Use an entry price found in the message, adjusted by `x_found_price`. Without a recognized price, normal
    /// pricing is used unless `buy_if_price_found` rejects the signal.
    ///
    /// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/02-signals-and-alerts.md#signalconfig-use-price).
    pub use_price: bool,
    /// Parse stop-loss and take-profit candidates from message numbers for the signal. This is heuristic parsing
    /// near the current price, not a promise to interpret every channel's format.
    ///
    /// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/02-signals-and-alerts.md#signalconfig-use-stops).
    pub use_stops: bool,
    /// Also recognize untagged uppercase tickers in the advanced parser. Ambiguous ordinary words are excluded;
    /// this does not make arbitrary lowercase words into tickers.
    ///
    /// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/02-signals-and-alerts.md#signalconfig-tokens-no-tags).
    pub tokens_no_tags: bool,
    /// Recognize exchange-market URL patterns in the advanced token parser; links are parsed as text, not fetched
    /// over HTTP. Enable the configurable-token path with `use_token_tags` or `tokens_no_tags`.
    ///
    /// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/02-signals-and-alerts.md#signalconfig-token-links).
    pub token_links: bool,
    /// Recognize `lower_price_words` phrases and place the non-strategy signal buy with `x_lower_price`, using
    /// `auto_cancel_lower_buy` as its timeout. Does not simply discard the signal; strategies have their own dip
    /// settings.
    ///
    /// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/02-signals-and-alerts.md#signalconfig-use-lower-price-words).
    pub use_lower_price_words: bool,
    /// Dip-buy offset in half-percent units: -10 means -5%. GUI range -40..=40. Used when `use_lower_price_words`
    /// matches a phrase and no strategy supplies its own dip price.
    ///
    /// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/02-signals-and-alerts.md#signalconfig-x-lower-price).
    pub x_lower_price: i32,
    /// Also recognize tickers spelled as separated letters, such as (B)(T)(C), B-T-C, B=T=C or B T C, before normal
    /// token matching.
    ///
    /// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/02-signals-and-alerts.md#signalconfig-special-formats).
    pub special_formats: bool,
}

// ---------------------------------------------------------------------------
// Section 2: Trading (Kind = 2, internal version = 3)
// ---------------------------------------------------------------------------

/// Order execution, risk management, and position control settings.
///
/// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/03-trading-and-leverage.md#tradingsection).
#[derive(Clone, Debug)]
pub struct TradingSection {
    /// Retention for ordinary logs, binary chart records and screenshots, in days. GUI 0..=15; 15 disables routine
    /// age cleanup, while low-disk-space cleanup remains independent. 0 does not mean keep forever.
    ///
    /// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/03-trading-and-leverage.md#tradingsection-auto-delete-logs).
    pub auto_delete_logs: i32,
    /// Kernel diagnostic verbosity, 0..=5 in the GUI. Higher values enable more detailed logs; 0 does not suppress
    /// all essential messages. Independent of InitConfig.subscribe_logs, which controls delivery to one SDK client.
    ///
    /// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/03-trading-and-leverage.md#tradingsection-log-level).
    pub log_level: i32,
    /// REST connection-method selector: 0 = Original, 1 = Method 2, 2 = Method 3, 3 = Method 4. Historical name;
    /// some exchange engines force a method and startup fallback may change it. Does not choose an exchange or
    /// MoonProto transport.
    ///
    /// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/03-trading-and-leverage.md#tradingsection-binance-connection).
    pub binance_connection: i32,

    /// Kernel startup, working hours, loss stops and recovery settings. See the startup and protection reference.
    ///
    /// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/03-trading-and-leverage.md#tradingsection-auto-start).
    pub auto_start: AutoStartConfig,
    /// Recovery after delta panic and per-market session-profit options. These are separate from process restart
    /// and order-size limits.
    ///
    /// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/03-trading-and-leverage.md#tradingsection-auto-start-2).
    pub auto_start_2: AutoStartConfig2,

    /// Saved favorite coin symbols, comma-separated. Used by favorite-market buttons and lists; does not select the
    /// universe of tradable markets.
    ///
    /// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/03-trading-and-leverage.md#tradingsection-fav-markets).
    pub fav_markets: String,
    /// Global buy-price offset in half-percent units: 6 means +3%, -4 means -2%. Applied to the reference ask
    /// selected by `use_current_ask`; a strategy or explicit order price can override it. Not exchange tick steps.
    ///
    /// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/03-trading-and-leverage.md#tradingsection-x-ask).
    pub x_ask: i32,
    /// Main sell target in percent before leverage adjustment. Positive values use `x_t_mode`'s optional x10 scale,
    /// capped at 900%; 0 selects scalp mode using `x_sell_scalp` / 50. `fixed_sell_mode` selects its own target.
    /// Prefer the typed ClientSettingsCommand helpers for UI controls.
    ///
    /// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/03-trading-and-leverage.md#tradingsection-x-sell).
    pub x_sell: i32,
    /// Minimum scalp sell target in 1/50-percent units: 10 means 0.2%. Used when `x_sell` = 0 and `fixed_sell_mode`
    /// is off; `sell_auto_move` can place the sell above this minimum based on the order book.
    ///
    /// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/03-trading-and-leverage.md#tradingsection-x-sell-scalp).
    pub x_sell_scalp: i32,
    /// Order amount as thousandths of total trading balance: balance * `play_with` / 1000, then multiplied by
    /// leverage on futures. GUI 1..=1000. Ignored when `use_manual_balance` selects `fixed_trade_balance`.
    ///
    /// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/03-trading-and-leverage.md#tradingsection-play-with).
    pub play_with: i32,
    /// Legacy stored flag without an active order-book check consumer. Preserve it; do not expose it as a working
    /// pre-buy protection.
    ///
    /// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/03-trading-and-leverage.md#tradingsection-check-glass).
    pub check_glass: bool,
    /// Global concurrent active-order limit used by entry checks. Includes active trade workers, not only unfilled
    /// buys. GUI 1..=1000, additionally restricted by edition and `unlimited_orders`; strategies can impose lower
    /// limits.
    ///
    /// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/03-trading-and-leverage.md#tradingsection-max-orders).
    pub max_orders: i32,
    /// For ordinary buys, cancel the unfilled remainder and proceed to selling the filled portion once the fill
    /// percentage is strictly greater than this threshold and the filled amount is tradable. GUI 1..=100; 100
    /// disables this early transition.
    ///
    /// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/03-trading-and-leverage.md#tradingsection-auto-sell-partial).
    pub auto_sell_partial: i32,
    /// Chart price-scale preset, not an order-price filter. 1 = automatic; 2/3/4/5/6 select progressively different
    /// fixed spans. Normal spot spans are 20/50/100/10/5%; futures spans are 10/20/50/5/2%. `x9_mode` and
    /// `scale_plus_index` can replace these spans.
    ///
    /// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/03-trading-and-leverage.md#tradingsection-price-range).
    pub price_range: i32,
    /// Adjust the initial sell placement using order-book liquidity. In scalp mode, search above the minimum scalp
    /// target; this is not a periodic timer that continually moves every sell.
    ///
    /// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/03-trading-and-leverage.md#tradingsection-sell-auto-move).
    pub sell_auto_move: bool,
    /// Legacy threshold referenced by the old predictor filter, which has no current callers. Preserve it; do not
    /// use it as a general auto-buy quality control.
    ///
    /// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/03-trading-and-leverage.md#tradingsection-pump-q-level).
    pub pump_q_level: i32,
    /// Percent spacing of the order-book analysis bands used for Buy/Sell x2..x4 volumes and PumpQ calculations. It
    /// does not double an order's quantity.
    ///
    /// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/03-trading-and-leverage.md#tradingsection-sell-x2-level).
    pub sell_x2_level: i32,
    /// When a sell completes, cancel remaining buys on the same market through the kernel's normal cancellation
    /// path. Strategy-specific cancellation rules also exist independently.
    ///
    /// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/03-trading-and-leverage.md#tradingsection-cancel-buy-on-sell-fill).
    pub cancel_buy_on_sell_fill: bool,
    /// Timeout for ordinary automatically started buys without a strategy: values <30 are seconds; 30..59 mean
    /// (value - 29) minutes; 60 disables this timeout. Dip-word signals use `auto_cancel_lower_buy`. A manually
    /// moved order and strategy timers follow their own rules.
    ///
    /// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/03-trading-and-leverage.md#tradingsection-auto-cancel-buy-order).
    pub auto_cancel_buy_order: i32,
    /// V-Stop price offset in half-percent units: 7 means +3.5% from entry. GUI -20..=20. Defines the price at
    /// which available order-book volume is checked for an enabled per-order V-Stop; it is not the percentage by
    /// which volume must fall.
    ///
    /// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/03-trading-and-leverage.md#tradingsection-vol-drop-level).
    pub vol_drop_level: i32,
    /// Global stop-loss price adjustment in percent, normally negative. Used when an order follows the global stop;
    /// per-order/strategy settings can override it. Long price = entry * (1 + value/100); short price = entry / (1
    /// + value/100), so the same numeric value is not a symmetric market move.
    ///
    /// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/03-trading-and-leverage.md#tradingsection-price-drop-level).
    pub price_drop_level: f32,
    /// Legacy stored toggle without an active V-Stop consumer. Actual V-Stop enablement belongs to the order;
    /// `vol_drop_level` still supplies its global price offset.
    ///
    /// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/03-trading-and-leverage.md#tradingsection-panic-if-vol-drop).
    pub panic_if_vol_drop: bool,
    /// Default stop-loss enablement for orders using global settings. Does not override every strategy's or already
    /// edited order's individual stop state.
    ///
    /// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/03-trading-and-leverage.md#tradingsection-panic-if-price-drop).
    pub panic_if_price_drop: bool,
    /// Disable automatic entry through the normal signal/detection paths while retaining manual order controls and
    /// management of existing positions. Not an emulator switch.
    ///
    /// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/03-trading-and-leverage.md#tradingsection-manual-mode).
    pub manual_mode: bool,
    /// Show completed/partially filled buy markers on charts, including historical ones. This controls drawing, not
    /// the set of active pending orders.
    ///
    /// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/03-trading-and-leverage.md#tradingsection-draw-all-buy).
    pub draw_all_buy: bool,
    /// Default trailing-stop enablement for orders using global settings. Strategy and individual-order trailing
    /// settings remain separate.
    ///
    /// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/03-trading-and-leverage.md#tradingsection-trailing-stop).
    pub trailing_stop: bool,
    /// Base trailing distance in percent, normally negative, for orders following the global trailing level.
    /// `trailing_float` adjusts the distance as favorable price movement grows.
    ///
    /// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/03-trading-and-leverage.md#tradingsection-trailing-drop).
    pub trailing_drop: f32,
    /// Recheck purchased quantity through the balance when cancellation/order-status checks failed, after market
    /// data initialization. Not a routine balance check before every buy; preserve unless this recovery option is
    /// intentionally needed.
    ///
    /// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/03-trading-and-leverage.md#tradingsection-engine-check-buy).
    pub engine_check_buy: bool,
    /// Use `fixed_trade_balance` as the default order amount instead of the `play_with` share of balance. A
    /// strategy's explicit order size remains separate.
    ///
    /// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/03-trading-and-leverage.md#tradingsection-use-manual-balance).
    pub use_manual_balance: bool,
    /// Fixed default order amount in the trading currency, selected by `use_manual_balance`. The amount is not
    /// automatically multiplied by leverage on this path; initialization defaults depend on the trading currency.
    ///
    /// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/03-trading-and-leverage.md#tradingsection-fixed-trade-balance).
    pub fixed_trade_balance: f64,
    /// Use the current ask as the reference for ordinary buy pricing; otherwise use the ask from about 30 seconds
    /// earlier. Strategies can select their own price source. The alternative is not the last trade price.
    ///
    /// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/03-trading-and-leverage.md#tradingsection-use-current-ask).
    pub use_current_ask: bool,
    /// Profit threshold in percent for `use_g_take_profit`. Gates global trailing activation and constrains its
    /// closing price; it is separate from the initial sell target `x_sell`. GUI range 0.5..=10.
    ///
    /// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/03-trading-and-leverage.md#tradingsection-g-take-profit).
    pub g_take_profit: f64,
    /// Require the global `g_take_profit` threshold before global trailing closes a non-strategy order. Does not
    /// place a separate take-profit order by itself.
    ///
    /// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/03-trading-and-leverage.md#tradingsection-use-g-take-profit).
    pub use_g_take_profit: bool,
    /// Show the comment editor in the pending-order dialog. Does not control all strategy labels on charts.
    ///
    /// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/03-trading-and-leverage.md#tradingsection-show-order-comment).
    pub show_order_comment: bool,
    /// Inactivity timeout for closing chart windows when `visual.auto_close_charts` is enabled. Normally minutes; 0
    /// means about 10 seconds in a normal build, 60 disables routine closure. API pressure or the chart-count limit
    /// can shorten the effective timeout.
    ///
    /// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/03-trading-and-leverage.md#tradingsection-chart-clean-up-time).
    pub chart_clean_up_time: i32,

    /// Telegram reporting mode, batching and margin-management defaults. Telegram sender/account setup is exposed
    /// separately through service settings.
    ///
    /// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/03-trading-and-leverage.md#tradingsection-moonbot-config).
    pub moonbot_config: MoonBotConfig,

    /// Require a second click within 300 ms to activate chart Panic Sell. The explicit hotkey path bypasses this
    /// mouse confirmation.
    ///
    /// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/03-trading-and-leverage.md#tradingsection-dbl-click-panic-sell).
    pub dbl_click_panic_sell: bool,
    /// Comma-separated global coin blacklist, used with `use_coins_black_list`. Strategy-specific ignore/blacklist
    /// rules still apply. Use coin symbols, not one symbol per line.
    ///
    /// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/03-trading-and-leverage.md#tradingsection-coins-black-list-text).
    pub coins_black_list_text: String,
    /// Enable `coins_black_list_text` in ordinary entry checks. A strategy may explicitly bypass the global
    /// blacklist through its own settings.
    ///
    /// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/03-trading-and-leverage.md#tradingsection-use-coins-black-list).
    pub use_coins_black_list: bool,
    /// Comma-separated coins excluded from hanging-position control. Does not blacklist them from normal trading.
    /// The control-bot settings use the same list.
    ///
    /// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/03-trading-and-leverage.md#tradingsection-h-pos-black-list-text).
    pub h_pos_black_list_text: String,
    /// Coefficient widening the trailing distance as favorable price movement grows: subtract coefficient *
    /// favorable gain percent from the negative trailing level. 0 keeps a fixed distance. GUI coefficient range
    /// -0.5..=2, stored directly (not percent units).
    ///
    /// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/03-trading-and-leverage.md#tradingsection-trailing-float).
    pub trailing_float: f64,
    /// Derived remainder of `h_pos_black_list_text` for symbols absent from the current market list. Preserve it;
    /// edit the full blacklist rather than presenting this as an independent user list.
    ///
    /// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/03-trading-and-leverage.md#tradingsection-h-pos-black-list-add).
    pub h_pos_black_list_add: String,
    /// Randomize the initial global sell-target percentage by up to about 5% of that target, before leverage
    /// adjustment. Does not randomize every order or guarantee concealment from other traders.
    ///
    /// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/03-trading-and-leverage.md#tradingsection-random-price).
    pub random_price: bool,

    /// Saved report-window columns, filters and display preferences. Does not change the SDK's report record
    /// format.
    ///
    /// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/03-trading-and-leverage.md#tradingsection-report-config).
    pub report_config: ReportConfig,

    /// Block ordinary signal auto-buy for markets marked as being delisted. The flag relies on the kernel's market
    /// status; it does not query the exchange on each click.
    ///
    /// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/03-trading-and-leverage.md#tradingsection-dont-buy-delisted).
    pub dont_buy_delisted: bool,
    /// Block automatic entry for this many minutes after a market was added. 0 disables the restriction. NewListing
    /// strategies are exempt; the check adds a 10-second margin.
    ///
    /// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/03-trading-and-leverage.md#tradingsection-dont-buy-new-coins).
    pub dont_buy_new_coins: i32,
    /// Timeout in minutes for a non-strategy signal using dip-word pricing. 600 disables the timeout. Different
    /// from the strategy parameter AutoCancelLowerBuy, whose unit is seconds.
    ///
    /// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/03-trading-and-leverage.md#tradingsection-auto-cancel-lower-buy).
    pub auto_cancel_lower_buy: i32,
    /// Allow Enter in the coin-search workflow to buy on the last opened chart. Ctrl+Enter is excluded. This is a
    /// manual trading action, not automatic buying whenever search text changes.
    ///
    /// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/03-trading-and-leverage.md#tradingsection-buy-on-enter).
    pub buy_on_enter: bool,
    /// Automatically replenish the exchange's supported fee token when its balance is low; the historical name is
    /// not a guarantee of BNB on every exchange. Disabled in emulator mode. Binance Futures uses a separate BNB
    /// purchase/transfer flow requiring the appropriate account mode and API permissions; unsupported engines do
    /// not gain this feature from the flag.
    ///
    /// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/03-trading-and-leverage.md#tradingsection-auto-buy-bnb).
    pub auto_buy_bnb: bool,
    /// Low-balance threshold in units of the supported fee token, used with `auto_buy_bnb`. Some balance paths also
    /// reserve this quantity when the fee token itself is the asset being traded. Not a USD amount or percentage.
    ///
    /// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/03-trading-and-leverage.md#tradingsection-auto-buy-bnb-level).
    pub auto_buy_bnb_level: f64,
    /// Quantity of the supported fee token to buy per replenishment, not the target total balance. Must satisfy
    /// that exchange/flow's minimum order value and available funds; Binance Futures additionally checks a minimum
    /// purchase value of 10.
    ///
    /// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/03-trading-and-leverage.md#tradingsection-auto-buy-bnb-volume).
    pub auto_buy_bnb_volume: f64,
    /// Legacy single-order click binding without a current consumer. Current chart movement uses `multi_orders`
    /// buy-move bindings; preserve this old field.
    ///
    /// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/03-trading-and-leverage.md#tradingsection-order-replace-click-buy).
    pub order_replace_click_buy: u8,
    /// Reject trading signals from forwarded Telegram messages. Unrelated to forward contracts or pre-market
    /// listings.
    ///
    /// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/03-trading-and-leverage.md#tradingsection-dont-buy-forward).
    pub dont_buy_forward: bool,
    /// Separate chart mouse-interaction zones: drawing/order actions are restricted to the appropriate chart or
    /// orderbook side. This is interaction behavior, not merely drawing divider lines.
    ///
    /// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/03-trading-and-leverage.md#tradingsection-chart-split-zones).
    pub chart_split_zones: bool,
    /// Draw stop-loss lines and allow their direct mouse interaction on charts. Does not enable or disable the
    /// underlying stop itself.
    ///
    /// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/03-trading-and-leverage.md#tradingsection-draw-stop).
    pub draw_stop: bool,
    /// Historical runtime statistic of maximum observed order size, not an editable maximum-order risk limit.
    /// Preserve the kernel value; do not build an order-size limit control from it.
    ///
    /// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/03-trading-and-leverage.md#tradingsection-max-order).
    pub max_order: f64,
    /// Allow the PRO settings editor to set `max_orders` above its ordinary 30-order ceiling. Does not remove
    /// `max_orders` or exchange/edition/strategy limits.
    ///
    /// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/03-trading-and-leverage.md#tradingsection-unlimited-orders).
    pub unlimited_orders: bool,
    /// Use the selected Manual strategy for manual order parameters. The strategy must exist; selecting it does not
    /// start automatic strategy execution.
    ///
    /// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/03-trading-and-leverage.md#tradingsection-use-manual-strategy).
    pub use_manual_strategy: bool,
    /// Name of the selected Manual strategy, used with `use_manual_strategy`. The compact settings view identifies
    /// the selected strategy by manual_strategy_id instead.
    ///
    /// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/03-trading-and-leverage.md#tradingsection-manual-strategy).
    pub manual_strategy: String,
    /// Wide chart-price scale mode: standard buttons use 100%, 200%, 500%, 1000% and 2000% spans. Does not create a
    /// nine-order grid or split orders.
    ///
    /// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/03-trading-and-leverage.md#tradingsection-x9-mode).
    pub x9_mode: bool,
    /// Multiply the main/fixed sell percentage by 10. Does not multiply every numeric setting, and does not scale
    /// `x_sell_scalp` / 50. Shared with the compact settings x_tmode field.
    ///
    /// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/03-trading-and-leverage.md#tradingsection-x-t-mode).
    pub x_t_mode: bool,
    /// Remembered acknowledgement of the fixed-amount warning dialog. Preserve it; not a risk-control switch or
    /// editable trading parameter.
    ///
    /// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/03-trading-and-leverage.md#tradingsection-fixed-balance-warning).
    pub fixed_balance_warning: bool,
    /// Adaptive trade-stream silence threshold in milliseconds, used by exchange reconnect checks. The kernel
    /// adjusts it as streams go silent/recover; not a minimum interval between the user's trades. Preserve as
    /// runtime state.
    ///
    /// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/03-trading-and-leverage.md#tradingsection-trades-gap-time).
    pub trades_gap_time: i32,
    /// Allow iceberg buy orders where the exchange engine and market support them. Quantity/minimum-notional checks
    /// still apply; this flag does not emulate iceberg orders on unsupported exchanges. Mirrors the compact
    /// `buy_iceberg` setting.
    ///
    /// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/03-trading-and-leverage.md#tradingsection-buy-iceberg).
    pub buy_iceberg: bool,
    /// Allow iceberg sell orders where supported by the exchange engine and market. Does not guarantee an iceberg
    /// for every sell; order quantity/type and market constraints still apply. Mirrors compact `sell_iceberg`.
    ///
    /// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/03-trading-and-leverage.md#tradingsection-sell-iceberg).
    pub sell_iceberg: bool,
    /// In the standard iceberg check, require the market's tick size as a percentage of current ask to be strictly
    /// below this threshold. This is a price-step percentage, not a slice-size fraction. The engine calculates the
    /// slice quantity from exchange limits.
    ///
    /// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/03-trading-and-leverage.md#tradingsection-iceberg-step).
    pub iceberg_step: f64,
    /// Legacy mouse binding without a current order-placement consumer. Preserve it; current long-order placement
    /// uses `multi_orders.buy_set_click`.
    ///
    /// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/03-trading-and-leverage.md#tradingsection-order-set-click).
    pub order_set_click: u8,
    /// Mouse binding for a pending long order in the classic Moonbot chart; uses the [mouse-action
    /// codes](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/04-orders-reports-and-notifications.md#mouse-action-codes).
    /// A remote terminal sends a separate [`new_pending_order`
    /// action](https://github.com/Moonbot-Tech/MoonProto/blob/main/docs/trade_actions.md); saving this binding does
    /// not place an order.
    ///
    /// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/03-trading-and-leverage.md#tradingsection-pending-order-set-click).
    pub pending_order_set_click: u8,
    /// Execution-price offset from the trigger, in percent, for manual pending orders placed locally or through
    /// MoonProto. Effective spread = this value + one-hour market delta × `pending_orders_spread_h_delta`; long and
    /// short directions use the normal price adjustment. Strategy pending orders use their own
    /// `PendingOrderSpread`.
    ///
    /// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/03-trading-and-leverage.md#tradingsection-pending-orders-spread).
    pub pending_orders_spread: f64,
    /// Dimensionless coefficient multiplying the one-hour market delta in the pending-order spread formula. It is
    /// not a delta threshold; 0 removes the delta-dependent term.
    ///
    /// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/03-trading-and-leverage.md#tradingsection-pending-orders-spread-h-delta).
    pub pending_orders_spread_h_delta: f64,
    /// Legacy serialized value with no current consumer. Preserve; use `pending_orders_spread` and the strategy's
    /// own pending-order parameters where applicable.
    ///
    /// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/03-trading-and-leverage.md#tradingsection-buy-pending-spread).
    pub buy_pending_spread: i32,
    /// Legacy serialized value with no current consumer. Preserve it instead of exposing an ineffective editor
    /// control.
    ///
    /// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/03-trading-and-leverage.md#tradingsection-buy-stop-pending-spread).
    pub buy_stop_pending_spread: i32,
    /// Trust Management master: when its sell completes, send a panic-close command for this market to linked slave
    /// bots. Requires the order to participate in MoonFriend forwarding. Unrelated to a MoonProto terminal
    /// disconnect.
    ///
    /// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/03-trading-and-leverage.md#tradingsection-tm-panic-sell-closed).
    pub tm_panic_sell_closed: bool,
    /// For Telegram Trust Management forwarding, send only on explicit Share instead of automatically forwarding
    /// new orders. Requires auto-posting enabled. The separate UDP forwarding switch remains independent.
    ///
    /// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/03-trading-and-leverage.md#tradingsection-tm-send-only-share).
    pub tm_send_only_share: bool,
    /// Trust Management master: send a cancel-buy command for the market to linked slave bots when the master's
    /// sell completes. Requires MoonFriend forwarding for the order.
    ///
    /// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/03-trading-and-leverage.md#tradingsection-tm-cancel-closed).
    pub tm_cancel_closed: bool,
    /// Trust Management master: after its buy fills, send a cancel-buy command to linked slave bots with a
    /// five-second delay.
    ///
    /// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/03-trading-and-leverage.md#tradingsection-tm-cancel-buy-on-fill).
    pub tm_cancel_buy_on_fill: bool,
    /// Permit multiple Telegram commands in one message on the authorized own-channel command path. Does not change
    /// MoonProto API batching.
    ///
    /// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/03-trading-and-leverage.md#tradingsection-multi-commands).
    pub multi_commands: bool,
    /// Remembered count used to stop showing the iceberg button hint after repeated use. Preserve as GUI state; not
    /// a trade quantity or retry limit.
    ///
    /// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/03-trading-and-leverage.md#tradingsection-iceberg-click-count).
    pub iceberg_click_count: i32,
    /// Trust Management master: forward the initial sell price after a successful buy. Does not send every later
    /// sell-price change by itself.
    ///
    /// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/03-trading-and-leverage.md#tradingsection-tm-send-first-sell).
    pub tm_send_first_sell: bool,
    /// Trust Management master: ask slaves to panic-close when the master's buy is cancelled unfilled or placement
    /// fails. On placement failure with this flag off, the master instead requests a commission-based sell target
    /// after cancel-buy commands.
    ///
    /// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/03-trading-and-leverage.md#tradingsection-tm-sell-if-master-not-filled).
    pub tm_sell_if_master_not_filled: bool,
    /// Master reference order value for Trust Management sizing. The forwarded percentage is clamped to 1..=100
    /// from actual buy value / this amount × 100; a non-positive amount yields 100%. Does not cap the master's
    /// actual order.
    ///
    /// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/03-trading-and-leverage.md#tradingsection-tm-max-order).
    pub tm_max_order: f64,

    /// Current chart mouse bindings, order-movement selection and multi-order display options. See
    /// MultiOrdersConfig; the obsolete enable byte is not an execution switch.
    ///
    /// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/03-trading-and-leverage.md#tradingsection-multi-orders).
    pub multi_orders: MultiOrdersConfig,

    /// Legacy sell-movement mouse binding with no current consumer. Current bindings are in `multi_orders`;
    /// preserve this serialized field.
    ///
    /// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/03-trading-and-leverage.md#tradingsection-order-replace-click-sell).
    pub order_replace_click_sell: u8,

    /// Manual chart price-alert behavior and sounds. See AlertConfig; these alerts are separate from automatic
    /// trading strategies.
    ///
    /// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/03-trading-and-leverage.md#tradingsection-alert-config).
    pub alert_config: AlertConfig,

    /// Respect temporary Moonbot-system market penalties in strategy filtering. Separate from the user's own coin
    /// blacklist; does not erase or edit that list.
    ///
    /// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/03-trading-and-leverage.md#tradingsection-use-moon-bl).
    pub use_moon_bl: bool,
    /// Allow the exchange engine to correct prices against exchange percentage-price limits once sufficient price
    /// data is available. This is separate from mandatory tick-size rounding and may change the requested price.
    ///
    /// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/03-trading-and-leverage.md#tradingsection-correct-order-price).
    pub correct_order_price: bool,
    /// Cached Binance Futures fee multiplier: 1 + fractional fee, e.g. 1.0005 means 0.05%. Updated from observed
    /// fills; engine initialization limits it to its default ceiling. Preserve as learned state, not a universal
    /// commission percentage input.
    ///
    /// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/03-trading-and-leverage.md#tradingsection-f-binance-commission).
    pub f_binance_commission: f64,

    /// Spot/reference-market overlays and comparisons on the chart. Does not change the account from futures to
    /// spot.
    ///
    /// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/03-trading-and-leverage.md#tradingsection-spot-config).
    pub spot_config: SpotConfig,

    /// Enable the best-bid/ask WebSocket feed where the engine advertises support. Feed freshness and reconnect
    /// logic still apply; it is not a blanket replacement of all REST requests. Moonbot's GUI/startup clears it on
    /// single-processor systems.
    ///
    /// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/03-trading-and-leverage.md#tradingsection-use-book-ticker).
    pub use_book_ticker: bool,
    /// Exclude user-blacklisted markets from aggregate market/market-average calculations. Does not switch off an
    /// individual market's own delta calculation.
    ///
    /// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/03-trading-and-leverage.md#tradingsection-exclude-black-list-delta).
    pub exclude_black_list_delta: bool,
    /// Use full balance/position amounts in relevant futures sizing checks rather than only currently free funds;
    /// in one-way mode, position-limit checks use the order's direction. Does not detect or close orphaned
    /// positions. Exchange rejection remains possible.
    ///
    /// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/03-trading-and-leverage.md#tradingsection-free-position-check).
    pub free_position_check: bool,

    /// Control-bot coordination, unaccounted-position handling and recovery options. The first four fields are
    /// legacy mirrors; current WatchDog rows are managed separately.
    ///
    /// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/03-trading-and-leverage.md#tradingsection-orders-control).
    pub orders_control: OrdersControl,

    /// Weight markets by their volume in aggregate market-average calculations; false gives equal market weights.
    /// Does not select a moving-average indicator on an individual chart.
    ///
    /// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/03-trading-and-leverage.md#tradingsection-m-avg-use-vol-weight).
    pub m_avg_use_vol_weight: bool,
    /// Draw an additional dashed execution-price line for pending orders on the Moonbot chart. Does not choose the
    /// price used for sell calculations.
    ///
    /// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/03-trading-and-leverage.md#tradingsection-pending-buy-price).
    pub pending_buy_price: bool,
    /// Use `fixed_sell_price` as the global sell percentage instead of the `x_sell` slider/scalp mode. This is a
    /// fixed percentage target, not an absolute exchange price; strategy/per-order overrides remain separate.
    ///
    /// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/03-trading-and-leverage.md#tradingsection-fixed-sell-mode).
    pub fixed_sell_mode: bool,
    /// Sell-target percentage used in `fixed_sell_mode`, multiplied by 10 when `x_t_mode` is on. It is not an
    /// absolute quote price. Leverage-aware take-profit mode can further scale the resulting price distance.
    ///
    /// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/03-trading-and-leverage.md#tradingsection-fixed-sell-price).
    pub fixed_sell_price: f64,
    /// Enable the kernel's Binance Futures quantitative-trading checks before strategy orders, including recent
    /// order counts and a temporary market block. Applies to Binance Futures variants, not all futures exchanges;
    /// does not configure hedge/one-way position mode.
    ///
    /// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/03-trading-and-leverage.md#tradingsection-futures-rules).
    pub futures_rules: bool,

    /// Cashback history/panel preferences. Does not enable the exchange's BNB fee discount.
    ///
    /// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/03-trading-and-leverage.md#tradingsection-cashback-settings).
    pub cashback_settings: CashBackSettings,

    /// Automatically reduce futures leverage after an exchange position-limit condition asks for deleveraging.
    /// Independent of `auto_max_order`: chooses the next lower supported step from the kernel's ladder and respects
    /// temporary change restrictions.
    ///
    /// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/03-trading-and-leverage.md#tradingsection-auto-lower-lev).
    pub auto_lower_lev: bool,

    /// Asset-transfer panel display preferences; changing this record does not execute a transfer.
    ///
    /// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/03-trading-and-leverage.md#tradingsection-transfer-config).
    pub transfer_config: AssetTransferConfig,

    /// Observed minimum account balance shown in the transfer panel. Negative initial value means not initialized;
    /// the balance updater replaces it and tracks subsequent lows. Not a minimum reserve that trading must keep.
    ///
    /// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/03-trading-and-leverage.md#tradingsection-min-balance).
    pub min_balance: f64,

    /// Automatic leverage and margin-mode policy. The target-amount expression is stored separately in
    /// `auto_lev_control`; see AutoManageLevConfig for dependencies and exchange restrictions.
    ///
    /// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/03-trading-and-leverage.md#tradingsection-auto-manage-lev).
    pub auto_manage_lev: AutoManageLevConfig,

    /// Target position-limit amounts for automatic leverage selection, not an enable flag. Example: `1k def 5k
    /// BTC*` sets a default target of 1000 and 5000 for matching tickers; K/M suffixes and wildcard ticker masks
    /// are supported. Empty text clears targets. Used by `auto_manage_lev.auto_max_order`; `auto_lev_up` only
    /// permits upward adjustments within that mode.
    ///
    /// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/03-trading-and-leverage.md#tradingsection-auto-lev-control).
    pub auto_lev_control: String,
    /// Allow supported server-side market-stop handling. The kernel's platform gate admits Binance spot and main
    /// Binance Futures; compatible order/strategy stop conditions are also required. This is not a universal
    /// conversion of all stops on every exchange.
    ///
    /// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/03-trading-and-leverage.md#tradingsection-use-stop-market).
    pub use_stop_market: bool,
    /// For Binance Futures variants, allow correction of a zero-quantity position with negative isolated margin
    /// even when its absolute margin deficit is 5 or more. Smaller deficits already enter the correction path with
    /// this flag off. Does not close all zero-size records or control normal stop-losses.
    ///
    /// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/03-trading-and-leverage.md#tradingsection-auto-close-zero-pos).
    pub auto_close_zero_pos: bool,

    /// Automatic sharing of trade screenshots and their scale/recipient settings. See SendShotsConfig; Telegram
    /// destination configuration is separate.
    ///
    /// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/03-trading-and-leverage.md#tradingsection-send-shots-config).
    pub send_shots_config: SendShotsConfig,

    /// Interpret relevant futures profit/target percentages with leverage: the global sell price distance is
    /// divided by leverage, while displayed/calculated profit percentages include it. Strategy-specific leverage
    /// options still apply; this does not change exchange leverage itself.
    ///
    /// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/03-trading-and-leverage.md#tradingsection-use-lev-for-take).
    pub use_lev_for_take: bool,
    /// Cached Bybit fee multiplier, e.g. 1.001 = 0.1%. The engine accepts saved values strictly between 0.999 and
    /// 1.003 and updates them from exchange data/fills. Preserve as learned state.
    ///
    /// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/03-trading-and-leverage.md#tradingsection-bybit-commission).
    pub bybit_commission: f64,
    /// Reduce requested buy quantity to the market's exchange maximum order/position quantity where those limits
    /// are known. Not the free-balance sizing switch; inverse-base-currency mode is excluded, and some explicit
    /// split-order paths have their own sizing.
    ///
    /// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/03-trading-and-leverage.md#tradingsection-auto-reduce-order).
    pub auto_reduce_order: bool,
    /// Shared cached fee multiplier historically named for Gate; also used by Bitget, OKX and Hyperliquid engines.
    /// Values strictly between 0.999 and 1.003 are accepted at initialization; exchange data updates it. Not a
    /// plain percent or a Gate-only user setting.
    ///
    /// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/03-trading-and-leverage.md#tradingsection-gate-commission).
    pub gate_commission: f64,

    /// Ten saved Manual-strategy names for quick-selection slots. Each must refer to an existing Manual strategy;
    /// these names do not create strategies or start them automatically.
    ///
    /// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/03-trading-and-leverage.md#tradingsection-manual-strats-names).
    pub manual_strats_names: [String; 10],

    /// Visibility and shortcut bindings for Manual-strategy quick-selection buttons. The slot contents are in
    /// `manual_strats_names`.
    ///
    /// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/03-trading-and-leverage.md#tradingsection-manual-strats-config).
    pub manual_strats_config: ManualStratsConfig,

    /// Saved text for the explicit clear-triggers dialog: comma-separated IDs or inclusive ranges, e.g. `1,5-12`.
    /// Updating this text alone does not clear triggers; the separate action must run.
    ///
    /// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/03-trading-and-leverage.md#tradingsection-clear-triggers-string).
    pub clear_triggers_string: String,
    /// Comma-separated exact tickers excluded from trade-history collection/stream subscription where that path
    /// supports the exclusion. Comparison is case-insensitive. Separate from the blacklist for trading signals;
    /// this is not a newline-separated signal filter.
    ///
    /// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/03-trading-and-leverage.md#tradingsection-no-trades-markets-text).
    pub no_trades_markets_text: String,
    /// Prefer the authenticated WebSocket order API when the exchange engine supports it and its API connection is
    /// active. Does not control public market-data subscriptions, and does not remove all REST usage.
    ///
    /// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/03-trading-and-leverage.md#tradingsection-use-websocket-api).
    pub use_websocket_api: bool,
    /// Legacy GUI depth-selector index, not a number of levels. The current Bybit futures engine fixes its depth at
    /// 1000 and no longer consumes this choice. Preserve rather than offer an effective depth control.
    ///
    /// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/03-trading-and-leverage.md#tradingsection-order-book-levels-ws).
    pub order_book_levels_ws: i32,

    /// Arbitrage chart/table presentation preferences. Actual arbitrage subscriptions and slot configuration are
    /// separate from this shared display record.
    ///
    /// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/03-trading-and-leverage.md#tradingsection-arb-view-config).
    pub arb_view_config: ArbViewConfig,

    // --- Tail fields (gated by blockEnd in Load, appended in Save) ---
    /// Include raw trade-history extrema in the legacy short-window delta calculation. The current
    /// average-price/ticker path still runs, including when trades stop; this is not a global switch between trades
    /// and the orderbook for every delta.
    ///
    /// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/03-trading-and-leverage.md#tradingsection-deltas-by-trades).
    pub deltas_by_trades: bool,
    /// For Manual strategies, use the global sell target instead of the strategy's SellPrice. Does not override
    /// SellPrice for every automatic strategy.
    ///
    /// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/03-trading-and-leverage.md#tradingsection-ignore-strat-sell-price).
    pub ignore_strat_sell_price: bool,
    /// Enable Hyperliquid's paid-priority IOC path for eligible taker actions. An IOC remainder is cancelled rather
    /// than left resting in the orderbook; outcome/prediction assets are excluded. This does not make every order
    /// IOC and is not a free latency preference.
    ///
    /// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/03-trading-and-leverage.md#tradingsection-use-hl-fast-ioc).
    pub use_hl_fast_ioc: bool,

    /// Opaque bytes added by newer kernels. Preserve unchanged when editing and round-tripping a received
    /// configuration; never expose as a setting.
    ///
    /// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/03-trading-and-leverage.md#tradingsection-unknown-tail).
    pub unknown_tail: Vec<u8>,
}

/// Automatic startup, trading stops, recovery and panic-close rules.
///
/// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/01-startup-and-protection.md#autostartconfig).
#[derive(Clone, Debug)]
pub struct AutoStartConfig {
    /// Start market monitoring automatically after startup analysis completes. Startup preference; changing it does
    /// not start the running kernel immediately.
    ///
    /// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/01-startup-and-protection.md#autostartconfig-auto-start).
    pub auto_start: bool,
    /// Enable detection when auto-start or an automatic recovery starts monitoring. With `remember_state`, this
    /// value is saved from the last running state.
    ///
    /// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/01-startup-and-protection.md#autostartconfig-auto-detect-on).
    pub auto_detect_on: bool,
    /// Start checked strategies on auto-start or automatic recovery. With `remember_state`, the kernel remembers
    /// whether any strategy was active.
    ///
    /// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/01-startup-and-protection.md#autostartconfig-strategies-on).
    pub strategies_on: bool,
    /// Restrict monitoring and strategies to `work_time_from`..`work_time_to` in the kernel machine's local time.
    /// Outside the window, stop strategies and cancel buys; existing sell positions remain managed. A window
    /// crossing midnight is supported.
    ///
    /// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/01-startup-and-protection.md#autostartconfig-work-time).
    pub work_time: bool,
    /// Stop monitoring, detection and strategies and cancel buys when the trade-count loss check exceeds
    /// `auto_stop_loss`. Compare the recent completed-trade buffer (when it has at least `stop_trades` entries)
    /// with the latest report records after the profit-counter reset, using the worse result. At least
    /// `stop_trades` entries in either count are required; `sell_if_loss` also closes positions.
    ///
    /// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/01-startup-and-protection.md#autostartconfig-auto-stop-if-loss).
    pub auto_stop_if_loss: bool,
    /// Remember monitoring, detection and strategy running state for the next startup. Overrides manual choices of
    /// `auto_start`, `auto_detect_on` and `strategies_on` as the running state changes.
    ///
    /// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/01-startup-and-protection.md#autostartconfig-remember-state).
    pub remember_state: bool,
    /// Also close all positions when either loss stop is triggered. Has no effect by itself without
    /// `auto_stop_if_loss` or `auto_stop_if_loss_hours`.
    ///
    /// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/01-startup-and-protection.md#autostartconfig-sell-if-loss).
    pub sell_if_loss: bool,
    /// Allow automatic update without waiting for all sell positions to finish. Applies to `auto_update`; this is
    /// not a general rule for stopping trading.
    ///
    /// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/01-startup-and-protection.md#autostartconfig-dont-wait-sells).
    pub dont_wait_sells: bool,
    /// Positive loss threshold in the trading currency for `auto_stop_if_loss`. The trade-count window uses
    /// `stop_trades`; the GUI stores the absolute value.
    ///
    /// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/01-startup-and-protection.md#autostartconfig-auto-stop-loss).
    pub auto_stop_loss: f64,
    /// Stop monitoring, cancel buys and close positions when the one-hour BTC delta is below `panic_btc_delta` or
    /// above `panic_btc_delta_up`. Recovery is configured in `auto_start_2`.
    ///
    /// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/01-startup-and-protection.md#autostartconfig-panic-btc).
    pub panic_btc: bool,
    /// Stop monitoring, cancel buys and close positions when the one-hour average market delta is below
    /// `panic_market_delta`. Recovery is configured in `auto_start_2`.
    ///
    /// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/01-startup-and-protection.md#autostartconfig-panic-market).
    pub panic_market: bool,
    /// Enable the separate loss stop for trades opened within `stop_hours`, after the profit counter's reset point.
    /// Requires at least `stop_hours_trades` records; compares loss with `auto_stop_hours_val`.
    ///
    /// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/01-startup-and-protection.md#autostartconfig-auto-stop-if-loss-hours).
    pub auto_stop_if_loss_hours: bool,
    /// Automatically install an available kernel update. Normally waits for sell positions to finish;
    /// `dont_wait_sells` removes that wait.
    ///
    /// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/01-startup-and-protection.md#autostartconfig-auto-update).
    pub auto_update: bool,
    /// Resume trading after an error stop once the error level has stayed below `errors_level` for
    /// `restart_err_time` minutes. This resumes monitoring and configured detection/strategies; it does not restart
    /// the process.
    ///
    /// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/01-startup-and-protection.md#autostartconfig-restart-after-err).
    pub restart_after_err: bool,
    /// Resume trading after a latency stop once latency has stayed at or below `ping_level` for `restart_ping_time`
    /// minutes, within working hours. Does not restart the process.
    ///
    /// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/01-startup-and-protection.md#autostartconfig-restart-after-ping).
    pub restart_after_ping: bool,
    /// Exclude emulator trades from both loss-stop calculations. Does not switch the kernel out of emulator mode.
    ///
    /// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/01-startup-and-protection.md#autostartconfig-ignore-emulator).
    pub ignore_emulator: bool,
    /// Length of the trade-count loss window. GUI minimum 1. The kernel compares the recent completed-trade buffer
    /// with the latest report records after the profit-counter reset; either count can satisfy the minimum.
    ///
    /// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/01-startup-and-protection.md#autostartconfig-stop-trades).
    pub stop_trades: i32,
    /// Error-free waiting period before automatic recovery, in minutes. GUI minimum 3; only used with
    /// `restart_after_err`.
    ///
    /// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/01-startup-and-protection.md#autostartconfig-restart-err-time).
    pub restart_err_time: i32,
    /// Lower one-hour BTC delta threshold in percent. GUI stores a non-positive value; `panic_btc` triggers
    /// strictly below it.
    ///
    /// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/01-startup-and-protection.md#autostartconfig-panic-btc-delta).
    pub panic_btc_delta: f64,
    /// Lower one-hour average market delta threshold in percent. GUI stores a non-positive value; `panic_market`
    /// triggers strictly below it.
    ///
    /// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/01-startup-and-protection.md#autostartconfig-panic-market-delta).
    pub panic_market_delta: f64,
    /// Stop monitoring and strategies and cancel buys when the kernel's smoothed API-error score reaches
    /// `errors_level`. `sell_all_on_errors` additionally closes positions.
    ///
    /// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/01-startup-and-protection.md#autostartconfig-auto-stop-on-errors).
    pub auto_stop_on_errors: bool,
    /// Stop monitoring and strategies and cancel buys when the kernel's smoothed exchange-response timing exceeds
    /// `ping_level`. This does not measure MoonProto client-to-kernel RTT.
    ///
    /// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/01-startup-and-protection.md#autostartconfig-auto-stop-on-ping).
    pub auto_stop_on_ping: bool,
    /// Close positions as well as canceling buys when `auto_stop_on_errors` triggers. Depends on that stop being
    /// enabled.
    ///
    /// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/01-startup-and-protection.md#autostartconfig-sell-all-on-errors).
    pub sell_all_on_errors: bool,
    /// Close positions as well as canceling buys when `auto_stop_on_ping` triggers. Depends on that stop being
    /// enabled.
    ///
    /// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/01-startup-and-protection.md#autostartconfig-sell-all-on-ping).
    pub sell_all_on_ping: bool,
    /// Threshold for the smoothed API-error score, not an error count. GUI range 1..=5; the stop triggers at score
    /// >= threshold.
    ///
    /// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/01-startup-and-protection.md#autostartconfig-errors-level).
    pub errors_level: i32,
    /// Exchange-response timing threshold in milliseconds. GUI range 100..=2000; the stop triggers strictly above
    /// it. Not a 1..=5 level and not MoonProto RTT.
    ///
    /// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/01-startup-and-protection.md#autostartconfig-ping-level).
    pub ping_level: i32,
    /// Normal-latency waiting period before automatic recovery, in minutes. GUI minimum 1; only used with
    /// `restart_after_ping`.
    ///
    /// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/01-startup-and-protection.md#autostartconfig-restart-ping-time).
    pub restart_ping_time: i32,
    /// Positive loss amount in the trading currency for the hourly loss window. Used with
    /// `auto_stop_if_loss_hours`, `stop_hours` and `stop_hours_trades`.
    ///
    /// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/01-startup-and-protection.md#autostartconfig-auto-stop-hours-val).
    pub auto_stop_hours_val: f64,
    /// Length of the loss lookback window in hours. GUI minimum 1. Trades are selected by opening time and the last
    /// counter-reset boundary.
    ///
    /// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/01-startup-and-protection.md#autostartconfig-stop-hours).
    pub stop_hours: i32,
    /// Minimum number of trades before the hourly loss stop can trigger. GUI minimum 0; 0 adds no trade-count
    /// requirement.
    ///
    /// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/01-startup-and-protection.md#autostartconfig-stop-hours-trades).
    pub stop_hours_trades: i32,
    /// Upper one-hour BTC delta threshold in percent. GUI stores a non-negative value; `panic_btc` triggers
    /// strictly above it.
    ///
    /// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/01-startup-and-protection.md#autostartconfig-panic-btc-delta-up).
    pub panic_btc_delta_up: f64,
    /// Work-window start as a fraction of a day in the kernel's local time: 0 = midnight, 0.5 = noon. GUI precision
    /// is one minute. Used with `work_time`.
    ///
    /// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/01-startup-and-protection.md#autostartconfig-work-time-from).
    pub work_time_from: f64,
    /// Work-window end as a fraction of a day in the kernel's local time, in [0, 1). A start later than the end
    /// means an overnight window. Boundaries are strict; equal endpoints do not mean an empty window.
    ///
    /// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/01-startup-and-protection.md#autostartconfig-work-time-to).
    pub work_time_to: f64,
}

/// Trading recovery after delta panic and per-market session-profit limits.
///
/// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/01-startup-and-protection.md#autostartconfig2).
#[derive(Clone, Debug)]
pub struct AutoStartConfig2 {
    /// Resume after the global delta panic when the one-hour BTC delta is between `btc_higher_then` and
    /// `btc_lower_then` and the one-hour market delta exceeds `market_higher_then`. Working-hour restrictions still
    /// apply.
    ///
    /// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/01-startup-and-protection.md#autostartconfig2-restart-on-market).
    pub restart_on_market: bool,
    /// Lower one-hour BTC delta bound for recovery, in percent. Requires `restart_on_market`; the comparison is
    /// strict. GUI stores a non-positive value.
    ///
    /// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/01-startup-and-protection.md#autostartconfig2-btc-higher-then).
    pub btc_higher_then: f64,
    /// Upper one-hour BTC delta bound for recovery, in percent. Requires `restart_on_market`; the comparison is
    /// strict. GUI stores a non-negative value.
    ///
    /// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/01-startup-and-protection.md#autostartconfig2-btc-lower-then).
    pub btc_lower_then: f64,
    /// Lower one-hour average market delta bound for recovery, in percent. Requires `restart_on_market`; the
    /// comparison is strict. GUI stores a non-positive value.
    ///
    /// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/01-startup-and-protection.md#autostartconfig2-market-higher-then).
    pub market_higher_then: f64,
    /// Gate market-list option: allow markets that have price and volume data even if present in the
    /// unavailable/delisted-market list. Does not enable trading on a closed exchange market; this is not a generic
    /// notification toggle.
    ///
    /// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/01-startup-and-protection.md#autostartconfig2-show-old-listing).
    pub show_old_listing: bool,
    /// Reset each market's session profit after `rs_hours` without a completed trade on that market. This is an
    /// inactivity timeout, not a global periodic reset.
    ///
    /// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/01-startup-and-protection.md#autostartconfig2-reset-session).
    pub reset_session: bool,
    /// Upper cap on accumulated per-market session profit in USD equivalent. 0 disables the cap. Does not limit
    /// orders, account balance or loss; applied when a trade completes.
    ///
    /// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/01-startup-and-protection.md#autostartconfig2-max-session-cap).
    pub max_session_cap: i32,
    /// Inactivity timeout in hours for `reset_session`. GUI minimum 1; read the connected kernel's value rather
    /// than using the offline default as a UI preset.
    ///
    /// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/01-startup-and-protection.md#autostartconfig2-rs-hours).
    pub rs_hours: i32,
}

/// MoonBot service sub-config (silent mode, margin auto-management).
///
/// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/04-orders-reports-and-notifications.md#moonbotconfig).
#[derive(Clone, Debug)]
pub struct MoonBotConfig {
    /// Suppress ordinary trade/signal Telegram messages and image sending while allowing service-message paths.
    /// Does not mute Moonbot's local sounds or stop trading.
    ///
    /// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/04-orders-reports-and-notifications.md#moonbotconfig-silent).
    pub silent: bool,
    /// Allow automatic asset borrowing for eligible short orders on a margin-capable engine. The implemented
    /// Binance margin path requires the relevant account mode, API permissions and borrowing capacity. Does not
    /// borrow funds for ordinary futures leverage.
    ///
    /// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/04-orders-reports-and-notifications.md#moonbotconfig-auto-margin-borrow).
    pub auto_margin_borrow: bool,
    /// Within automatic margin borrowing, allow transfer of collateral from spot to margin; the borrowing flow can
    /// return the transfer on failure. Requires the borrowing path and API permissions. Not an independent
    /// wallet-balancing service.
    ///
    /// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/04-orders-reports-and-notifications.md#moonbotconfig-auto-margin-transfer).
    pub auto_margin_transfer: bool,
    /// Batch outgoing Telegram messages over this many milliseconds. Normal command setter clamps to 1000..=5000;
    /// the shared-bot path uses at least 3500. The worker polls separately, so this is not a precise delivery
    /// deadline. Service messages may bypass batching.
    ///
    /// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/04-orders-reports-and-notifications.md#moonbotconfig-group-delay).
    pub group_delay: i32,
}

/// Report / trade history display configuration.
///
/// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/04-orders-reports-and-notifications.md#reportconfig).
#[derive(Clone, Debug)]
pub struct ReportConfig {
    /// 51 saved report-column visibility slots. Current GUI consumes 43 columns, listed in MoonDoc; preserve unused
    /// slots. Not a projection for report API requests. Column 11 is synchronized with `trading.use_lev_for_take`.
    ///
    /// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/04-orders-reports-and-notifications.md#reportconfig-fields-vis).
    pub fields_vis: [bool; 51],
    /// Saved widths of the same 51 report-column slots in GUI pixels. Moonbot enforces a minimum of 10 when using
    /// them; stretch can redistribute displayed widths.
    ///
    /// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/04-orders-reports-and-notifications.md#reportconfig-fields-width).
    pub fields_width: [u16; 51],
    /// Legacy field without a current report-filter consumer. Use `active_type` for the Moonbot report UI; API
    /// request filters are separate.
    ///
    /// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/04-orders-reports-and-notifications.md#reportconfig-only-active).
    pub only_active: bool,
    /// Legacy saved value without a current consumer. The active report date controls use other core state; this is
    /// not the SDK report-query range.
    ///
    /// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/04-orders-reports-and-notifications.md#reportconfig-range).
    pub range: i32,
    /// First saved report filter, combined with filter2 by AND. Length-prefixed UTF-8: byte 0 is a byte count up to
    /// 25, bytes 1..=25 hold text; do not split a UTF-8 character. Accepts a coin, strategy/type text, `manual`,
    /// `+` for profit, `-` for loss and recognized type aliases; otherwise searches channel/signal text and, for
    /// longer nonnumeric text, sell reason. Empty means no first filter.
    ///
    /// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/04-orders-reports-and-notifications.md#reportconfig-filter1).
    pub filter1: [u8; 26],
    /// Second saved report filter; same encoding and matching rules as filter1. Both non-empty filters must match;
    /// this is not an alternative OR filter.
    ///
    /// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/04-orders-reports-and-notifications.md#reportconfig-filter2).
    pub filter2: [u8; 26],
    /// Unused third legacy filter slot. Preserve its 26 bytes unchanged; current report filtering consumes filter1
    /// and filter2 only.
    ///
    /// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/04-orders-reports-and-notifications.md#reportconfig-filter3).
    pub filter3: [u8; 26],
    /// Report UI selection: 0 all non-deleted records, 1 active non-deleted, 2 closed non-deleted, 3 also include
    /// deleted records. Does not select a custom query or delete records.
    ///
    /// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/04-orders-reports-and-notifications.md#reportconfig-active-type).
    pub active_type: u8,
    /// Choose emulator-only records when true, real-only records when false. It does not merge emulator trades with
    /// real ones.
    ///
    /// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/04-orders-reports-and-notifications.md#reportconfig-rep-emulator-orders).
    pub rep_emulator_orders: bool,
    /// Fit visible report columns to the available GUI width; false uses saved column widths.
    ///
    /// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/04-orders-reports-and-notifications.md#reportconfig-stretch).
    pub stretch: bool,
    /// In a static saved-trade chart, show the selected trade's buy/sell pair rather than unrelated order lines.
    /// Does not aggregate multi-orders into one report row.
    ///
    /// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/04-orders-reports-and-notifications.md#reportconfig-only-2-orders).
    pub only_2_orders: bool,
    /// Select the leverage-aware spent-amount basis for report summary percentages. Its effect depends on
    /// `trading.use_lev_for_take`; it does not change exchange leverage or stored execution prices.
    ///
    /// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/04-orders-reports-and-notifications.md#reportconfig-use-leverage).
    pub use_leverage: bool,
    /// Maximum rows loaded into the Moonbot report grid; the normal setter clamps to 50..=1000. Report totals use a
    /// separate query without this row limit. This is not an SDK request page size.
    ///
    /// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/04-orders-reports-and-notifications.md#reportconfig-max-lines).
    pub max_lines: u16,
    /// Report UI direction filter: 0 both, 1 long, 2 short. Does not change order direction.
    ///
    /// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/04-orders-reports-and-notifications.md#reportconfig-pos-direction).
    pub pos_direction: u8,
    /// Write detected liquidation events as report records. Not a switch for recording a liquidation-price column
    /// in ordinary trades.
    ///
    /// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/04-orders-reports-and-notifications.md#reportconfig-store-liq).
    pub store_liq: bool,
    /// Write received funding payments as separate report records. The current path excludes inverse-base-currency
    /// mode and global emulator mode. Does not merely store a funding-rate indicator.
    ///
    /// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/04-orders-reports-and-notifications.md#reportconfig-store-fund).
    pub store_fund: bool,
    /// Apply the report's date-range filter to closing time instead of buying/opening time. Sorting has separate
    /// state; this is not the sort-order switch.
    ///
    /// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/04-orders-reports-and-notifications.md#reportconfig-by-close-date).
    pub by_close_date: bool,
}

/// Multi-order mode settings (visual order grid placement and movement).
///
/// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/04-orders-reports-and-notifications.md#multiordersconfig).
#[derive(Clone, Debug)]
pub struct MultiOrdersConfig {
    /// Serialized sub-record version; preserve the received value. Not a user-selectable feature level.
    ///
    /// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/04-orders-reports-and-notifications.md#multiordersconfig-ver).
    pub ver: u8,
    /// Obsolete enable byte retained for compatibility: current kernel export writes true and import ignores it.
    /// Multi-orders remain available; this is not the current grid-pause control.
    ///
    /// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/04-orders-reports-and-notifications.md#multiordersconfig-use-multi-orders).
    pub use_multi_orders: bool,
    /// Mouse action for placing a long entry at the clicked chart price. Use the mouse-action codes in MoonDoc; 0
    /// disables this binding.
    ///
    /// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/04-orders-reports-and-notifications.md#multiordersconfig-buy-set-click).
    pub buy_set_click: u8,
    /// Primary mouse action for moving long entry orders, using `replace_buy_kind`. If equal to
    /// `short_buy_move_click`, the handler selects both directions.
    ///
    /// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/04-orders-reports-and-notifications.md#multiordersconfig-buy-move-click).
    pub buy_move_click: u8,
    /// Primary mouse action for moving long exit orders, using `replace_sell_kind`. If equal to
    /// `short_sell_move_click`, the handler selects both directions.
    ///
    /// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/04-orders-reports-and-notifications.md#multiordersconfig-sell-move-click).
    pub sell_move_click: u8,
    /// Primary entry-order selection/movement mode, codes 0..=7 described in MoonDoc. Mode 4 uses the largest
    /// quantity for entries because no completed-position profit is available.
    ///
    /// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/04-orders-reports-and-notifications.md#multiordersconfig-replace-buy-kind).
    pub replace_buy_kind: u8,
    /// Primary exit-order selection/movement mode, codes 0..=7 described in MoonDoc. Only eligible active orders on
    /// the chosen side are affected; click-immune orders are excluded.
    ///
    /// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/04-orders-reports-and-notifications.md#multiordersconfig-replace-sell-kind).
    pub replace_sell_kind: u8,
    /// Legacy unused field; does not split current orders. Preserve it. Explicit split actions and strategy split
    /// settings are separate.
    ///
    /// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/04-orders-reports-and-notifications.md#multiordersconfig-split-sells).
    pub split_sells: i32,
    /// Show the worker/order number alongside order labels on charts. Does not change order count or grouping.
    ///
    /// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/04-orders-reports-and-notifications.md#multiordersconfig-show-orders-num).
    pub show_orders_num: bool,
    /// Unused reserved byte in the current kernel, despite the historical SDK name. Preserve; no current
    /// visualization switch is connected to it.
    ///
    /// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/04-orders-reports-and-notifications.md#multiordersconfig-kir-style).
    pub kir_style: bool,
    /// Legacy direction selector replaced by separate long/short mouse bindings. Preserve; current movement
    /// direction is selected by matching those bindings.
    ///
    /// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/04-orders-reports-and-notifications.md#multiordersconfig-fix-pos).
    pub fix_pos: u8,
    /// Global exit-joining policy: 0 disabled, 1 retain a fixed target price, 2 retain a profit-percentage target.
    /// Applies to eligible same-market/same-direction orders; strategy join keys can independently request joining.
    ///
    /// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/04-orders-reports-and-notifications.md#multiordersconfig-join-sell-kind).
    pub join_sell_kind: u8,
    /// Mouse action for placing a short entry at the clicked price. Uses the same codes as `buy_set_click` and
    /// still requires a supported short-trading path.
    ///
    /// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/04-orders-reports-and-notifications.md#multiordersconfig-short-set-click).
    pub short_set_click: u8,
    /// Mouse action for a pending short in the Moonbot GUI; requires futures. GUI pending orders are not available
    /// in kernel mode.
    ///
    /// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/04-orders-reports-and-notifications.md#multiordersconfig-pending-short-set-click).
    pub pending_short_set_click: u8,
    /// Base opacity of completed order lines, 0..=1. Some closed/unfilled lines are faded further by the renderer;
    /// not a retention period.
    ///
    /// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/04-orders-reports-and-notifications.md#multiordersconfig-done-opacity).
    pub done_opacity: f32,
    /// Secondary long-entry movement binding, paired with `replace_buy_kind_2`; same mouse-action codes as the
    /// primary binding.
    ///
    /// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/04-orders-reports-and-notifications.md#multiordersconfig-buy-move-click-2).
    pub buy_move_click_2: u8,
    /// Secondary long-exit movement binding, paired with `replace_sell_kind_2`; same mouse-action codes as the
    /// primary binding.
    ///
    /// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/04-orders-reports-and-notifications.md#multiordersconfig-sell-move-click-2).
    pub sell_move_click_2: u8,
    /// Selection/movement mode 0..=7 for the secondary entry binding. Same semantics as `replace_buy_kind`.
    ///
    /// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/04-orders-reports-and-notifications.md#multiordersconfig-replace-buy-kind-2).
    pub replace_buy_kind_2: u8,
    /// Selection/movement mode 0..=7 for the secondary exit binding. Same semantics as `replace_sell_kind`.
    ///
    /// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/04-orders-reports-and-notifications.md#multiordersconfig-replace-sell-kind-2).
    pub replace_sell_kind_2: u8,
    /// GUI preference to copy each long movement binding into its corresponding short binding, for both primary and
    /// secondary actions. Does not make primary and secondary actions equal. The runtime compares the actual
    /// binding values, so an editor must update them consistently.
    ///
    /// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/04-orders-reports-and-notifications.md#multiordersconfig-same-hotkeys-for-move).
    pub same_hotkeys_for_move: bool,
    /// Primary short-entry movement binding. Shares `replace_buy_kind`; matching the long binding selects both
    /// directions.
    ///
    /// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/04-orders-reports-and-notifications.md#multiordersconfig-short-buy-move-click).
    pub short_buy_move_click: u8,
    /// Primary short-exit movement binding. Shares `replace_sell_kind`; matching the long binding selects both
    /// directions.
    ///
    /// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/04-orders-reports-and-notifications.md#multiordersconfig-short-sell-move-click).
    pub short_sell_move_click: u8,
    /// Secondary short-entry movement binding; shares `replace_buy_kind_2`. Matching `buy_move_click_2` selects
    /// both directions.
    ///
    /// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/04-orders-reports-and-notifications.md#multiordersconfig-short-buy-move-click-2).
    pub short_buy_move_click_2: u8,
    /// Secondary short-exit movement binding; shares `replace_sell_kind_2`. Matching `sell_move_click_2` selects
    /// both directions.
    ///
    /// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/04-orders-reports-and-notifications.md#multiordersconfig-short-sell-move-click-2).
    pub short_sell_move_click_2: u8,
}

/// Chart price-alert configuration.
///
/// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/04-orders-reports-and-notifications.md#alertconfig).
#[derive(Clone, Debug)]
pub struct AlertConfig {
    /// Legacy byte without a current global price-alert consumer. Alert enablement belongs to the individual chart
    /// object/strategy; do not expose this as a working master switch.
    ///
    /// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/04-orders-reports-and-notifications.md#alertconfig-enabled).
    pub enabled: bool,
    /// How long a figure/price-alert button remains active, in seconds; GUI 10..=3600. A strategy alert can supply
    /// its own KeepAlert instead.
    ///
    /// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/04-orders-reports-and-notifications.md#alertconfig-keep-time).
    pub keep_time: i32,
    /// Sound preset 1..=11; 0 plays nothing. Strategy figure alerts can use their own named sound. Preset names are
    /// in the [sound
    /// reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/02-signals-and-alerts.md).
    ///
    /// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/04-orders-reports-and-notifications.md#alertconfig-sound-kind).
    pub sound_kind: i32,
    /// Repeat counter for a figure alert, GUI 1..=10. Repeats are checked at five-second intervals while the alert
    /// remains active, in addition to the initial notification; `keep_time` can end them earlier.
    ///
    /// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/04-orders-reports-and-notifications.md#alertconfig-repeat-count).
    pub repeat_count: i32,
}

/// Spot chart panel display settings.
///
/// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/05-charts-and-display.md#spotconfig).
#[derive(Clone, Debug)]
pub struct SpotConfig {
    /// Draw the corresponding spot/reference market's trade dots when that stream is available. Separate from the
    /// current market's trade-dot switch.
    ///
    /// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/05-charts-and-display.md#spotconfig-show-trades).
    pub show_trades: bool,
    /// Draw the corresponding spot/second orderbook when supported and fresh. Does not hide the primary market's
    /// orderbook.
    ///
    /// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/05-charts-and-display.md#spotconfig-show-book).
    pub show_book: bool,
    /// Visual width multiplier for the second/spot orderbook, in percent. Does not request this percentage of
    /// exchange depth levels.
    ///
    /// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/05-charts-and-display.md#spotconfig-book-len).
    pub book_len: f32,
    /// Overlay the aggregate market-average history, normalized to the current chart. Weighting follows
    /// `trading.m_avg_use_vol_weight`; not necessarily volume-weighted.
    ///
    /// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/05-charts-and-display.md#spotconfig-show-market-avg).
    pub show_market_avg: bool,
    /// Draw historical minimum/maximum price-envelope curves from the market's stored extrema. Not simply two
    /// 24-hour high/low markers.
    ///
    /// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/05-charts-and-display.md#spotconfig-show-min-max).
    pub show_min_max: bool,
    /// Draw the market's smoothed price history from second/minute averages. Does not show the user's average
    /// position entry price.
    ///
    /// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/05-charts-and-display.md#spotconfig-show-avg-price).
    pub show_avg_price: bool,
    /// Select a BTC-based corresponding/reference-market feed where supported, instead of the usual matching spot
    /// comparison. Changing it can require the kernel's subscription-refresh path; not just a BTC text label.
    ///
    /// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/05-charts-and-display.md#spotconfig-spot-btc).
    pub spot_btc: bool,
    /// Shift spot/reference prices vertically by the current spot/futures price difference for comparison, except
    /// BTC-reference mode. Does not compensate for panel width or alter order prices.
    ///
    /// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/05-charts-and-display.md#spotconfig-shift-spot).
    pub shift_spot: bool,
    /// Draw mark-price history when the market has enough mark-price samples. Requires a feed supporting mark
    /// price; does not configure liquidation protection.
    ///
    /// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/05-charts-and-display.md#spotconfig-show-mark-price).
    pub show_mark_price: bool,
    /// Draw exchange liquidation-event dots on the chart. These are market events, not the liquidation-price line
    /// of the user's own position.
    ///
    /// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/05-charts-and-display.md#spotconfig-show-liq).
    pub show_liq: bool,
    /// Increase the drawn size of liquidation-event dots when `show_liq` is on. Does not filter events by
    /// liquidation amount.
    ///
    /// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/05-charts-and-display.md#spotconfig-huge-liq).
    pub huge_liq: bool,
    /// Draw futures open-interest history and cursor information when available. Does not enable a trading filter.
    ///
    /// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/05-charts-and-display.md#spotconfig-show-open-int).
    pub show_open_int: bool,
    /// Draw the market's recent averaged-price history line. Separate from `show_avg_price`'s second/minute
    /// smoothed curves and from position entry prices.
    ///
    /// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/05-charts-and-display.md#spotconfig-show-avg-price-line).
    pub show_avg_price_line: bool,
}

/// Control-bot coordination, unaccounted-position handling and recovery preferences.
/// The first four fields are legacy observer mirrors; use `client.watchdogs()` for active observers.
///
/// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/03-trading-and-leverage.md#orderscontrol).
#[derive(Clone, Debug)]
pub struct OrdersControl {
    /// Legacy mirror of a WatchDog row's order-control switch. Current workers read the separate observer list;
    /// changing this field does not enable an observer. Use `client.watchdogs()` to edit that list.
    ///
    /// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/03-trading-and-leverage.md#orderscontrol-active).
    pub active: bool,
    /// Legacy WatchDog row mirror: market-price distance in percent, at or below which a tracked buy is a
    /// cancellation candidate; 0 disables the distance check. Edit the active observer through
    /// `client.watchdogs()`, not this mirror.
    ///
    /// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/03-trading-and-leverage.md#orderscontrol-min-price).
    pub min_price: f64,
    /// Legacy WatchDog row mirror: maximum time since an order update, in seconds; 0 disables this age check.
    /// Current workers read the separate WatchDog row.
    ///
    /// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/03-trading-and-leverage.md#orderscontrol-max-time).
    pub max_time: i32,
    /// Legacy mirror of a WatchDog row's position-control switch. Current workers read the active observer's
    /// position-control setting and require the corresponding license. Edit it through `client.watchdogs()`.
    ///
    /// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/03-trading-and-leverage.md#orderscontrol-h-pos-control).
    pub h_pos_control: bool,
    /// Send Telegram reports when the position controller confirms an unaccounted position. Requires active
    /// position control; the repeat interval comes from the kernel system-report interval.
    ///
    /// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/03-trading-and-leverage.md#orderscontrol-h-pos-report).
    pub h_pos_report: bool,
    /// Automatically close positions confirmed as unaccounted by the position controller. Requires active position
    /// control; repeated closure attempts for the same market are limited to once per minute. This is a trading
    /// action, not merely an alert.
    ///
    /// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/03-trading-and-leverage.md#orderscontrol-h-pos-auto-sell).
    pub h_pos_auto_sell: bool,
    /// Skip some recovery checks after an uncertain placement/replacement result; a failed classic buy replacement
    /// can be marked closed locally. Affects recovery behavior and some exchange-native replacement paths. Do not
    /// present as a harmless performance toggle.
    ///
    /// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/03-trading-and-leverage.md#orderscontrol-ignore-replacing-bug).
    pub ignore_replacing_bug: bool,
    /// Protection bypass marker: 0 is normal, exactly 1666 activates bypasses in order/strategy protection. Not a
    /// numeric severity level. Moonbot requires explicit acknowledgement before setting it; preserve that meaning
    /// in an editor.
    ///
    /// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/03-trading-and-leverage.md#orderscontrol-ignore-protection).
    pub ignore_protection: i32,
    /// Mark buy orders as belonging to this Moonbot for control-bot coordination; also permits WatchDog keepalives
    /// when a destination is configured. Does not enable exchange API cryptographic signing: authentication remains
    /// independent.
    ///
    /// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/03-trading-and-leverage.md#orderscontrol-sign-orders).
    pub sign_orders: bool,
    /// After a detected futures liquidation, cancel that market's orders and attempt to close remaining positions.
    /// It reacts to liquidation detection; it is not a configurable pre-liquidation price-distance stop.
    ///
    /// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/03-trading-and-leverage.md#orderscontrol-liq-control).
    pub liq_control: bool,
}

/// Cashback history and display preferences; not the exchange fee-discount policy.
///
/// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/04-orders-reports-and-notifications.md#cashbacksettings).
#[derive(Clone, Debug, Default)]
pub struct CashBackSettings {
    /// Display spot and futures cashback estimates separately instead of their sum.
    ///
    /// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/04-orders-reports-and-notifications.md#cashbacksettings-sep).
    pub sep: bool,
    /// Legacy unused flag; current cashback display has no consumer for it. Preserve rather than promise subaccount
    /// aggregation.
    ///
    /// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/04-orders-reports-and-notifications.md#cashbacksettings-agg-accounts).
    pub agg_accounts: bool,
    /// Period selector, not a literal day count: 0 all stored history, 1 last 90 days, 2 last 30 days, 3 current
    /// half-month payout-period selection. Current-day data is included separately. GUI clamps the index to 0..=3.
    ///
    /// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/04-orders-reports-and-notifications.md#cashbacksettings-max-days).
    pub max_days: i32,
    /// Use the stored at-the-time BNB valuation for cashback estimates. When false, recent records with currency
    /// components revalue the BNB component at the current cached BNB price; old/incomplete records fall back to
    /// stored totals. Does not buy, sell or convert BNB.
    ///
    /// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/04-orders-reports-and-notifications.md#cashbacksettings-instant-bnb).
    pub instant_bnb: bool,
    /// Hide the cashback information UI. Its visibility is also limited to the supported Binance platforms; does
    /// not disable cashback accrual or payment.
    ///
    /// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/04-orders-reports-and-notifications.md#cashbacksettings-hide-info).
    pub hide_info: bool,
}

/// Asset transfer panel display settings.
///
/// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/04-orders-reports-and-notifications.md#assettransferconfig).
#[derive(Clone, Debug)]
pub struct AssetTransferConfig {
    /// Show free quantity, total quantity and estimated USD value in each asset row. False shows free quantity
    /// only; does not transfer or reserve funds.
    ///
    /// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/04-orders-reports-and-notifications.md#assettransferconfig-show-total).
    pub show_total: bool,
    /// Hide an asset only when neither its free nor total quantity is positive. Does not delete the asset or its
    /// balance.
    ///
    /// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/04-orders-reports-and-notifications.md#assettransferconfig-hide-zero).
    pub hide_zero: bool,
    /// Unused serialized display flag in the current kernel. Preserve; it does not select backup balances or
    /// currently toggle USD valuation.
    ///
    /// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/04-orders-reports-and-notifications.md#assettransferconfig-show-baks).
    pub show_baks: bool,
}

/// Automatic leverage management.
///
/// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/03-trading-and-leverage.md#automanagelevconfig).
#[derive(Clone, Debug)]
pub struct AutoManageLevConfig {
    /// Choose leverage from exchange risk brackets so their maximum position size accommodates the amount
    /// configured in `trading.auto_lev_control`. Needs loaded leverage/bracket data and a positive effective target
    /// for the market; unsupported for inverse-base-currency mode. Lowering is allowed; raising additionally
    /// requires `auto_lev_up`. This is not sizing an order from free balance.
    ///
    /// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/03-trading-and-leverage.md#automanagelevconfig-auto-max-order).
    pub auto_max_order: bool,
    /// Allow `auto_max_order` to increase leverage as well as decrease it. Has no independent effect when
    /// `auto_max_order` is off or the market has no positive target amount.
    ///
    /// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/03-trading-and-leverage.md#automanagelevconfig-auto-lev-up).
    pub auto_lev_up: bool,
    /// Requested fixed leverage multiplier, GUI integer 1..=500; each market is limited by its exchange maximum.
    /// Used by `auto_fix_lev`, including new-listing initialization. It may coexist with `auto_max_order` for
    /// markets without an effective target amount.
    ///
    /// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/03-trading-and-leverage.md#automanagelevconfig-fix-lev).
    pub fix_lev: i32,
    /// Automatically request isolated futures margin. Exchange support and account/position restrictions still
    /// apply. Mutually exclusive with `auto_cross` in the GUI; if both are received as true, isolated has priority.
    /// Uses account-wide mode on engines with a global margin-mode setting, otherwise per-market mode.
    ///
    /// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/03-trading-and-leverage.md#automanagelevconfig-auto-isolated).
    pub auto_isolated: bool,
    /// Report successful automatic leverage changes to Telegram. Does not control leverage selection itself or
    /// exchange-request logging.
    ///
    /// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/03-trading-and-leverage.md#automanagelevconfig-tlg-report).
    pub tlg_report: bool,
    /// Automatically request cross futures margin when `auto_isolated` is off. Subject to exchange support and
    /// account/position restrictions; mutually exclusive with `auto_isolated` in the GUI.
    ///
    /// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/03-trading-and-leverage.md#automanagelevconfig-auto-cross).
    pub auto_cross: bool,
    /// Apply `fix_lev` automatically where `auto_max_order` is disabled or has no effective target amount for the
    /// market. Exchange maximums and temporary change restrictions remain in force.
    ///
    /// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/03-trading-and-leverage.md#automanagelevconfig-auto-fix-lev).
    pub auto_fix_lev: bool,
}

/// Telegram screenshot sharing settings.
///
/// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/04-orders-reports-and-notifications.md#sendshotsconfig).
#[derive(Clone, Debug)]
pub struct SendShotsConfig {
    /// Send qualifying automatic screenshots to the configured personal Telegram destination. Requires an own
    /// Telegram bot and non-silent image sending. Public screenshot sending has a separate switch; false does not
    /// disable that path.
    ///
    /// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/04-orders-reports-and-notifications.md#sendshotsconfig-may-send).
    pub may_send: bool,
    /// Strict profit threshold in USDT for a single-trade screenshot. The absolute-amount and percentage tests are
    /// alternatives (OR), not both required. GUI minimum 1; enabled exchange promotion policy can adjust
    /// thresholds.
    ///
    /// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/04-orders-reports-and-notifications.md#sendshotsconfig-profit-abs).
    pub profit_abs: i32,
    /// Strict profit-percentage threshold alternative to `profit_abs`. GUI minimum 1. With `send_negative`, the
    /// loss magnitude is tested against the same threshold.
    ///
    /// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/04-orders-reports-and-notifications.md#sendshotsconfig-profit-pers).
    pub profit_pers: i32,
    /// Threshold for a market's accumulated hourly-session profit screenshot when the single-trade tests did not
    /// qualify. GUI uses at least 50 and at least twice `profit_abs`; server/promotion policy can also affect the
    /// session threshold. Not a global account loss limit.
    ///
    /// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/04-orders-reports-and-notifications.md#sendshotsconfig-profit-session).
    pub profit_session: i32,
    /// Allow single-trade loss screenshots when loss magnitude exceeds `profit_abs` or `profit_pers`. Does not
    /// itself enable either delivery destination.
    ///
    /// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/04-orders-reports-and-notifications.md#sendshotsconfig-send-negative).
    pub send_negative: bool,
    /// Legacy unused flag. Current automatic delivery uses the configured Telegram destination, not whichever chat
    /// is open in Telegram.
    ///
    /// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/04-orders-reports-and-notifications.md#sendshotsconfig-send-to-open-chat).
    pub send_to_open_chat: bool,
    /// Allow real-trade screenshots to be sent to the Moonbot public screenshot service. Independent of `may_send`;
    /// emulator screenshots are excluded from this automatic public path. Some enabled exchange promotion modes
    /// force public sending.
    ///
    /// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/04-orders-reports-and-notifications.md#sendshotsconfig-send-public).
    pub send_public: bool,
    /// Screenshot history expansion percentage, GUI 100..=400. Values above 120 activate the volatility-aware
    /// expansion logic; not a fixed number of seconds.
    ///
    /// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/04-orders-reports-and-notifications.md#sendshotsconfig-time-scale).
    pub time_scale: i32,
    /// Optional price-span percentage for automatic screenshots, GUI 0..=100; 0 leaves automatic scale. Positive
    /// values only widen the already selected range when the new bounds contain it.
    ///
    /// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/04-orders-reports-and-notifications.md#sendshotsconfig-price-scale).
    pub price_scale: i32,
}

/// Manual strategy buttons and hotkeys.
///
/// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/04-orders-reports-and-notifications.md#manualstratsconfig).
#[derive(Clone, Debug)]
pub struct ManualStratsConfig {
    /// Show enabled Manual-strategy quick buttons while `trading.use_manual_strategy` is active. Does not execute a
    /// strategy merely by showing it.
    ///
    /// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/04-orders-reports-and-notifications.md#manualstratsconfig-use-buttons).
    pub use_buttons: bool,
    /// Ten GUI shortcuts selecting the corresponding `manual_strats_names` slots. Each is a 16-bit shortcut
    /// encoding described in the keyboard chapter; 0 means unassigned. A hidden/disabled slot also disables its
    /// action in Moonbot.
    ///
    /// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/04-orders-reports-and-notifications.md#manualstratsconfig-hot-keys).
    pub hot_keys: [u16; 10],
    /// Ten per-slot enable/visibility flags. In Moonbot these also enable the slot's shortcut action; `use_buttons`
    /// controls the overall button strip.
    ///
    /// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/04-orders-reports-and-notifications.md#manualstratsconfig-show-button).
    pub show_button: [bool; 10],
}

/// Arbitrage panel display settings.
///
/// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/05-charts-and-display.md#arbviewconfig).
#[derive(Clone, Debug)]
pub struct ArbViewConfig {
    /// Serialized display-record version. Preserve received value.
    ///
    /// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/05-charts-and-display.md#arbviewconfig-ver).
    pub ver: u8,
    /// Show absolute arbitrage price differences in the numeric block; requires `show_numbers`. May be combined
    /// with `show_percent`.
    ///
    /// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/05-charts-and-display.md#arbviewconfig-show-absolute).
    pub show_absolute: bool,
    /// Show numeric arbitrage differences when arbitrage data is active. Does not create subscriptions or enable
    /// arbitrage trading.
    ///
    /// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/05-charts-and-display.md#arbviewconfig-show-numbers).
    pub show_numbers: bool,
    /// Draw corresponding arbitrage price lines when arbitrage data is active. Independent of numeric-block
    /// visibility.
    ///
    /// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/05-charts-and-display.md#arbviewconfig-show-lines).
    pub show_lines: bool,
    /// Show percentage differences in the numeric block. When both `show_percent` and `show_absolute` are false,
    /// the renderer still uses percent as its fallback.
    ///
    /// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/05-charts-and-display.md#arbviewconfig-show-percent).
    pub show_percent: bool,
    /// Place the arbitrage numeric block on the right instead of the left of the chart.
    ///
    /// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/05-charts-and-display.md#arbviewconfig-show-right).
    pub show_right: bool,
}

// ---------------------------------------------------------------------------
// Section 3: Visual (Kind = 3, internal version = 2)
// ---------------------------------------------------------------------------

/// Chart appearance, draw tools, visual indicators, candle style.
///
/// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/05-charts-and-display.md#visualsection).
#[derive(Clone, Debug)]
pub struct VisualSection {
    /// Legacy order-color record plus the active `price_line_width` setting. The seven color slots have no current
    /// rendering consumers; active theme colors are stored separately.
    ///
    /// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/05-charts-and-display.md#visualsection-colors).
    pub colors: IntColors,

    /// Opacity of the panic-sell price-area overlay, percent 0..=100; 0 hides it. Does not alter panic selling.
    ///
    /// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/05-charts-and-display.md#visualsection-panic-sell-opacity).
    pub panic_sell_opacity: i32,
    /// Hide the community/forum label on Moonbot's main form.
    ///
    /// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/05-charts-and-display.md#visualsection-hide-forum-label).
    pub hide_forum_label: bool,
    /// Draw bid/ask price-history lines. The renderer uses the supported book-ticker path when enabled, otherwise
    /// its ordinary price-history path.
    ///
    /// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/05-charts-and-display.md#visualsection-show-red-green-lines).
    pub show_red_green_lines: bool,
    /// Place multiple charts in a scrollable container instead of fitting them into the fixed chart panel. Does not
    /// control following the latest time within a chart.
    ///
    /// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/05-charts-and-display.md#visualsection-scrolling-charts).
    pub scrolling_charts: bool,
    /// Restore saved chart selections on Moonbot startup. Does not itself enable automatic trading on those
    /// markets.
    ///
    /// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/05-charts-and-display.md#visualsection-startup-load-charts).
    pub startup_load_charts: bool,
    /// Enable automatic cleanup of eligible inactive charts using `trading.chart_clean_up_time` and the kernel's
    /// activity/load checks. Does not close a chart immediately just because another coin was selected.
    ///
    /// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/05-charts-and-display.md#visualsection-auto-close-charts).
    pub auto_close_charts: bool,
    /// Prefer chart information on the left. Individual chart modes/layout constraints can override its placement.
    ///
    /// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/05-charts-and-display.md#visualsection-left-chart-info).
    pub left_chart_info: bool,
    /// Draw market trade dots. Candle/combo modes can force a tick layer independently; this does not stop
    /// collecting trades.
    ///
    /// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/05-charts-and-display.md#visualsection-show-red-green-dots).
    pub show_red_green_dots: bool,
    /// Opacity of the orderbook background fill, percent 0..=100. Cumulative and individual-level strokes have
    /// separate opacity settings.
    ///
    /// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/05-charts-and-display.md#visualsection-glass-opacity).
    pub glass_opacity: i32,
    /// Saved chart time span in seconds, not minutes. Tick charts clamp to 60..=21600 seconds; candle mode has its
    /// own available-history limit and remembered tick scale.
    ///
    /// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/05-charts-and-display.md#visualsection-chart-time-scale).
    pub chart_time_scale: i32,
    /// Keep multiple saved-trade charts in the chart-watch viewer. False clears previous viewer frames when another
    /// saved chart is opened; not a multi-timeframe switch.
    ///
    /// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/05-charts-and-display.md#visualsection-chart-watch-multi-frames).
    pub chart_watch_multi_frames: bool,
    /// Extra wait in minutes after a trade before saving its chart history, GUI 0..=60. Normal saving waits at
    /// least about three seconds even at 0; chart-forwarding paths use their own 15..90-second bounds. Not a number
    /// of historical candles to request.
    ///
    /// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/05-charts-and-display.md#visualsection-chart-history-len).
    pub chart_history_len: i32,
    /// Collapse the right chart control panel by default. A chart can temporarily expand it; hiding controls does
    /// not disable their underlying trading functions.
    ///
    /// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/05-charts-and-display.md#visualsection-hide-right-chart-panel).
    pub hide_right_chart_panel: bool,
    /// Show the vertical trade-volume histogram. Uses the fast renderer; candle mode can supply candle-volume bins.
    /// Separate from whether the trade dots themselves are shown.
    ///
    /// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/05-charts-and-display.md#visualsection-show-vert-volume).
    pub show_vert_volume: bool,
    /// Volume bin index: 0 automatic by chart span, 1 five seconds, 2 fifteen seconds, 3 thirty seconds, 4 one
    /// minute, 5 three minutes, 6 five minutes. Candle-volume rendering can use the selected candle timeframe
    /// instead.
    ///
    /// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/05-charts-and-display.md#visualsection-vert-vol-time-frame).
    pub vert_vol_time_frame: i32,
    /// Vertical-volume opacity as a fraction 0..=1. GUI percentages are divided by 100 before storing.
    ///
    /// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/05-charts-and-display.md#visualsection-vert-vol-opacity).
    pub vert_vol_opacity: f32,
    /// Vertical-volume maximum height as a fraction of chart height, 0..=1. Not a number of pixels.
    ///
    /// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/05-charts-and-display.md#visualsection-vert-vol-height).
    pub vert_vol_height: f32,
    /// Volume display: 0 overlay buy/sell bars from the same baseline, 1 stack buy and sell volumes. Not bars
    /// versus area rendering.
    ///
    /// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/05-charts-and-display.md#visualsection-vert-vol-kind).
    pub vert_vol_kind: i32,
    /// Enable the cursor volume tool and, with horizontal volumes, its price-range measurements. Does not enable a
    /// strategy detector.
    ///
    /// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/05-charts-and-display.md#visualsection-v-tool-show).
    pub v_tool_show: bool,
    /// Cursor-tool time-window length in seconds; 0 uses the current vertical-volume interval. Keep consistent with
    /// `v_tool_time_index` when editing the preset.
    ///
    /// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/05-charts-and-display.md#visualsection-v-tool-time).
    pub v_tool_time: f64,
    /// Opacity fraction 0..=1 for the cursor-tool selection area; the time-selection rectangle uses half this
    /// value.
    ///
    /// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/05-charts-and-display.md#visualsection-v-tool-opacity).
    pub v_tool_opacity: f32,
    /// Legacy saved checkbox without a current renderer consumer. Preserve; it does not currently hide/show buyer
    /// volume.
    ///
    /// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/05-charts-and-display.md#visualsection-v-tool-show-buyers).
    pub v_tool_show_buyers: bool,
    /// GUI preset index 0..=10 mapping to `v_tool_time` seconds: 0, 2, 5, 15, 30, 60, 180, 300, 900, 1800, 3600.
    /// The GUI writes both fields; the renderer reads `v_tool_time`.
    ///
    /// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/05-charts-and-display.md#visualsection-v-tool-time-index).
    pub v_tool_time_index: i32,
    /// Show the horizontal volume profile by price. Uses the fast renderer and applies to the main chart rather
    /// than every secondary frame.
    ///
    /// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/05-charts-and-display.md#visualsection-hv-show).
    pub hv_show: bool,
    /// Horizontal-volume panel width as a fraction of available chart width, 0..=1.
    ///
    /// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/05-charts-and-display.md#visualsection-hv-width).
    pub hv_width: f32,
    /// Horizontal-volume history selector: 0 automatic; 1..=9 correspond to 1, 5, 15, 30, 60, 120, 360, 720, 1440
    /// minutes. Index 10 uses the available deep candle archive, not an exact guaranteed 48-hour window.
    ///
    /// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/05-charts-and-display.md#visualsection-hv-time-frame-ind).
    pub hv_time_frame_ind: i32,
    /// Unused serialized value. The renderer calculates its own window from `hv_time_frame_ind`; preserve this
    /// field.
    ///
    /// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/05-charts-and-display.md#visualsection-hv-time-frame).
    pub hv_time_frame: i32,
    /// Price smoothing half-width for horizontal-volume accumulation, in percent, GUI 0..=1. The renderer uses at
    /// least the market's percentage price tick. Not a fraction of the visible chart price range.
    ///
    /// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/05-charts-and-display.md#visualsection-hv-price-frame).
    pub hv_price_frame: f32,
    /// Horizontal-volume stroke opacity, fraction 0..=1.
    ///
    /// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/05-charts-and-display.md#visualsection-hv-opacity).
    pub hv_opacity: f32,

    /// Drawing-tool appearance and behavior. Individual drawing-object alerts are separate state; see the
    /// drawing-tools chapter.
    ///
    /// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/05-charts-and-display.md#visualsection-custom-draw-config).
    pub custom_draw_config: CustomDrawConfig,

    /// Selected chart drawing tool, using the [tool
    /// codes](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/06-drawing-and-news.md#customdrawconfig-tools).
    /// Preserve reserved/internal kinds; ordinary drawing excludes the none, order-line and news-marker kinds.
    ///
    /// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/05-charts-and-display.md#visualsection-global-current-draw-kind).
    pub global_current_draw_kind: u8,
    /// Horizontal-volume label placement: 0 right with cleared background, 1 left with guide line, 2 right over the
    /// chart, 3 left over the chart with guide line.
    ///
    /// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/05-charts-and-display.md#visualsection-hv-disp-vol).
    pub hv_disp_vol: i32,
    /// Horizontal-volume composition: 0 overlay buyer/seller volumes, 1 stack them. Deep-candle archive mode has
    /// combined volume rather than a true buy/sell split.
    ///
    /// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/05-charts-and-display.md#visualsection-hv-kind).
    pub hv_kind: i32,
    /// Show transient market/order-status messages on the chart. Not simply the ticker/title label; the VDS mode
    /// suppresses these messages.
    ///
    /// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/05-charts-and-display.md#visualsection-show-market-captions).
    pub show_market_captions: bool,
    /// Show detailed order captions unless `spy_hide_mode` suppresses them. Does not hide the underlying order
    /// lines.
    ///
    /// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/05-charts-and-display.md#visualsection-show-orders-captions).
    pub show_orders_captions: bool,
    /// Place eligible captions below their line; short-order captions use the lower placement even when this flag
    /// is false.
    ///
    /// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/05-charts-and-display.md#visualsection-orders-captions-lower).
    pub orders_captions_lower: bool,
    /// Use USD valuation for chart profit/session/position amounts and related totals instead of the account's
    /// trading-currency amount. Does not convert the chart's price axis into USD.
    ///
    /// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/05-charts-and-display.md#visualsection-show-usd-on-charts).
    pub show_usd_on_charts: bool,
    /// Open a new chart at its maximum available time span: normally six hours for ticks, candle-history capacity
    /// in candle mode. Not maximum price zoom and not limited to newly listed coins.
    ///
    /// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/05-charts-and-display.md#visualsection-new-markets-max-scale).
    pub new_markets_max_scale: bool,
    /// Volume-scale placement: 0 left, 1 right. Changes label/scale layout, not the histogram time direction.
    ///
    /// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/05-charts-and-display.md#visualsection-vert-vol-ind-pos).
    pub vert_vol_ind_pos: i32,
    /// Cumulative orderbook stroke opacity in percent 0..=100. The renderer skips the strokes at values 0 or 1.
    ///
    /// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/05-charts-and-display.md#visualsection-book-cumulative-opacity).
    pub book_cumulative_opacity: i32,
    /// Individual orderbook-level stroke opacity in percent 0..=100. The renderer skips these strokes at 0 or 1.
    ///
    /// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/05-charts-and-display.md#visualsection-book-orders-opacity).
    pub book_orders_opacity: i32,
    /// Stroke thickness of individual orderbook levels, GUI 1..=6 pixels. Not the horizontal length of the volume
    /// bar.
    ///
    /// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/05-charts-and-display.md#visualsection-book-orders-width).
    pub book_orders_width: i32,

    /// BTC one-hour-delta visual/audible notification thresholds. Separate from automatic panic/stop settings.
    ///
    /// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/05-charts-and-display.md#visualsection-blink-config).
    pub blink_config: BlinkConfig,

    /// Keep eligible charts following the latest time automatically. Panned/history views and chart modes still
    /// influence recentering.
    ///
    /// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/05-charts-and-display.md#visualsection-charts-auto-shift).
    pub charts_auto_shift: bool,
    /// Use the iceberg-specific appearance for an order identified as iceberg. Does not enable iceberg order
    /// placement.
    ///
    /// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/05-charts-and-display.md#visualsection-show-iceberg).
    pub show_iceberg: bool,
    /// Hide the futures PnL label on charts. Does not stop profit calculation or remove profit from API snapshots.
    ///
    /// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/05-charts-and-display.md#visualsection-hide-pnl).
    pub hide_pnl: bool,
    /// Current-price label placement: 0 in the right panel, 1 in the orderbook spread, 2 hidden. Not a solid/dashed
    /// line selector.
    ///
    /// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/05-charts-and-display.md#visualsection-draw-cur-price).
    pub draw_cur_price: i32,
    /// Request missing market history when opening a Moonbot chart. Separate from restoring the saved chart set on
    /// startup and from the SDK's own candle refresh settings.
    ///
    /// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/05-charts-and-display.md#visualsection-auto-request-charts).
    pub auto_request_charts: bool,
    /// Order-panel sort: 0 entries first, 1 exits first, 2 creation order. Applies to GUI order controls, not API
    /// delivery order.
    ///
    /// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/05-charts-and-display.md#visualsection-vo-sort-kind).
    pub vo_sort_kind: u8,
    /// Show older order controls first; false reverses the normal order when not grouping by market.
    ///
    /// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/05-charts-and-display.md#visualsection-vo-older-first).
    pub vo_older_first: bool,
    /// Group GUI order controls by market. The VDS layout suppresses this grouping; it can override the older/newer
    /// ordering preference.
    ///
    /// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/05-charts-and-display.md#visualsection-vo-sort-by-market).
    pub vo_sort_by_market: bool,
    /// Hide the chart Buy button and related global buy-offset controls. Does not prohibit buy orders from
    /// shortcuts, strategies or API commands.
    ///
    /// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/05-charts-and-display.md#visualsection-hide-buy-button).
    pub hide_buy_button: bool,
    /// Prefix strategy names in the strategy tree with their numeric identifier. Does not change strategy IDs.
    ///
    /// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/05-charts-and-display.md#visualsection-show-strat-numbers).
    pub show_strat_numbers: bool,

    /// Visibility and interaction preferences for filter/session/detection annotations on charts. Does not enable
    /// or disable actual strategy filters.
    ///
    /// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/05-charts-and-display.md#visualsection-show-filters).
    pub show_filters: ShowFilters,

    /// Unused legacy flag in the current kernel. Preserve it; current cashback visibility uses other settings such
    /// as `cashback_settings.hide_info`.
    ///
    /// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/05-charts-and-display.md#visualsection-hide-cashback-button).
    pub hide_cashback_button: bool,
    /// Hide the seasonal New Year decoration/greetings when that seasonal UI would otherwise be shown.
    ///
    /// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/05-charts-and-display.md#visualsection-hide-ny-elka).
    pub hide_ny_elka: bool,

    /// Separate chart-monitor window layout and recentering preferences. Not the main chart's render rate.
    ///
    /// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/05-charts-and-display.md#visualsection-charts-settings).
    pub charts_settings: ChartsSettings,

    /// Visually blur or suppress selected balance/profit/order information in Moonbot's GUI. Not a data-access or
    /// secrecy control: the underlying values remain available to the program/API.
    ///
    /// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/05-charts-and-display.md#visualsection-spy-hide-mode).
    pub spy_hide_mode: bool,

    /// Trade-size heatmap and optional system/timing overlays. See HeatMapConfig; this is a display preference, not
    /// the SDK CPU profiler.
    ///
    /// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/05-charts-and-display.md#visualsection-heatmap-config).
    pub heatmap_config: HeatMapConfig,

    /// Moonbot icon-set choice: 1 Original, 2 Mark black-and-white. Stored as one-based value, unlike many other
    /// GUI selectors.
    ///
    /// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/05-charts-and-display.md#visualsection-icon-selection).
    pub icon_selection: i32,
    /// Remember each market's selected fixed buy-amount and fixed sell-percentage buttons and restore them on
    /// selection in the corresponding modes. Not a blanket persistence switch for every chart button.
    ///
    /// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/05-charts-and-display.md#visualsection-remember-chart-buttons).
    pub remember_chart_buttons: bool,
    /// Place detection alert buttons in a separate detection-tool window instead of the main toolbar.
    ///
    /// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/05-charts-and-display.md#visualsection-show-detects-tool).
    pub show_detects_tool: bool,
    /// Extra price-span preset: 0 normal scale; 1..=9 select 100%, 200%, 500%, 1000%, 2000%, 5000%, 10000%, 20000%,
    /// 50000%. Used in the expanded price-scale mode, not as a generic zoom-step size.
    ///
    /// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/05-charts-and-display.md#visualsection-scale-plus-index).
    pub scale_plus_index: i32,
    /// Unused serialized companion to `scale_plus_index`. Preserve; current zoom-out controls do not consume it.
    ///
    /// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/05-charts-and-display.md#visualsection-scale-minus-index).
    pub scale_minus_index: i32,

    /// News-window filtering and presentation settings; see NewsFormConfig. Visibility itself is not a field in
    /// this public record.
    ///
    /// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/05-charts-and-display.md#visualsection-news-form-config).
    pub news_form_config: NewsFormConfig,
    /// Twenty saved GUI font slots. Current explicit consumer uses slot 0 for the takers viewer: 0 leaves its
    /// default, positive values are clamped to 8..=20. Preserve unrecognized slots; not twenty interchangeable
    /// global font sizes.
    ///
    /// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/05-charts-and-display.md#visualsection-font-sizes).
    pub font_sizes: [u8; 20],

    // --- Tail fields (gated by blockEnd) ---
    /// Candle bodies: 0 filled, 1 outlines, 2 outlines where raw ticks are available and filled outside that
    /// region. Does not select a candle interval.
    ///
    /// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/05-charts-and-display.md#visualsection-chart-candles-style).
    pub chart_candles_style: u8,
    /// Candle opacity in the region with raw ticks, integer percent 0..=100. Does not set opacity of the trade dots
    /// themselves.
    ///
    /// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/05-charts-and-display.md#visualsection-chart-candles-tick-opacity).
    pub chart_candles_tick_opacity: u8,
    /// Use the neutral candle color in the region with raw ticks regardless of bullish/bearish direction. Not
    /// specifically a doji-color rule.
    ///
    /// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/05-charts-and-display.md#visualsection-chart-candles-neutral-ticks).
    pub chart_candles_neutral_ticks: bool,
    /// Candle outline thickness setting 1..=3; renderer scales it for DPI and limits it to the actual candle width.
    ///
    /// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/05-charts-and-display.md#visualsection-chart-candles-outline-width).
    pub chart_candles_outline_width: u8,
    /// Also draw candle wicks in the raw-tick region. Outside that region wicks are drawn independently of this
    /// flag.
    ///
    /// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/05-charts-and-display.md#visualsection-chart-candles-tick-wicks).
    pub chart_candles_tick_wicks: bool,
    /// Two ARGB candle palettes in order [light, dark]. Each contains rising, falling and tick-region neutral
    /// colors.
    ///
    /// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/05-charts-and-display.md#visualsection-candle-colors).
    pub candle_colors: [CandleColorSet; 2],
    /// Legacy format flag; current AI-card runtime uses the configured own-provider path independently. Not an
    /// effective enable switch.
    ///
    /// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/05-charts-and-display.md#visualsection-use-ai-coin-card).
    pub use_ai_coin_card: bool,
    /// Provider index in this core: 0 OpenAI, 1 Anthropic, 2 Google Gemini, 3 xAI Grok, 4 DeepSeek, 5 Perplexity, 6
    /// OpenRouter. Out-of-range selects 0. API keys are separate private settings and are not included in
    /// SharedConfig.
    ///
    /// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/05-charts-and-display.md#visualsection-ai-card-provider).
    pub ai_card_provider: u8,
    /// Model ID for the selected provider. The core uses it only if present in that provider's current
    /// cached/fallback list; otherwise it chooses the first available model. Do not hardcode a universal model list
    /// in a settings editor.
    ///
    /// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/05-charts-and-display.md#visualsection-ai-card-model).
    pub ai_card_model: String,
    /// AI-card prompt template; blank uses the built-in prompt for the GUI language. Supported placeholders:
    /// {symbol}, {exchange}, {price}, {deltas}, {volume}, {mcap}, {news}. Saving the template does not send an AI
    /// request.
    ///
    /// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/05-charts-and-display.md#visualsection-ai-card-prompt).
    pub ai_card_prompt: String,
    /// Open manually added charts full-screen in the applicable GUI path; automatic signal/startup additions have
    /// separate rules. Does not enable manual trading mode.
    ///
    /// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/05-charts-and-display.md#visualsection-manual-charts-full-screen).
    pub manual_charts_full_screen: bool,

    /// Preserve opaque bytes from newer kernels unchanged when round-tripping the visual section.
    ///
    /// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/05-charts-and-display.md#visualsection-unknown-tail).
    pub unknown_tail: Vec<u8>,
}

/// Candle colors for one theme (light or dark).
///
/// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/05-charts-and-display.md#candlecolorset).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct CandleColorSet {
    /// ARGB color (0xAARRGGBB) for rising candles, including equal open/close.
    ///
    /// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/05-charts-and-display.md#candlecolorset-green).
    pub green: u32,
    /// ARGB color (0xAARRGGBB) for falling candles.
    ///
    /// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/05-charts-and-display.md#candlecolorset-red).
    pub red: u32,
    /// ARGB color for the tick region when `chart_candles_neutral_ticks` is enabled; not specifically a doji color.
    ///
    /// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/05-charts-and-display.md#candlecolorset-neutral).
    pub neutral: u32,
}

/// Legacy color slots and the active price-line width; theme colors are configured separately.
///
/// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/05-charts-and-display.md#intcolors).
#[derive(Clone, Debug)]
pub struct IntColors {
    /// Legacy stored color without an active renderer consumer. Preserve it; do not offer it as an effective color
    /// control.
    ///
    /// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/05-charts-and-display.md#intcolors-buy-order-color-bu).
    pub buy_order_color_bu: u32,
    /// Legacy stored color without an active renderer consumer. Preserve it; do not offer it as an effective color
    /// control.
    ///
    /// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/05-charts-and-display.md#intcolors-sell-color-u).
    pub sell_color_u: u32,
    /// Legacy stored color without an active renderer consumer. Preserve it; do not offer it as an effective color
    /// control.
    ///
    /// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/05-charts-and-display.md#intcolors-buy-order-color-wu).
    pub buy_order_color_wu: u32,
    /// Legacy stored color without an active renderer consumer. Preserve it; do not offer it as an effective color
    /// control.
    ///
    /// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/05-charts-and-display.md#intcolors-sell-order-color-u).
    pub sell_order_color_u: u32,
    /// Legacy stored color without an active renderer consumer. Preserve it; do not offer it as an effective color
    /// control.
    ///
    /// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/05-charts-and-display.md#intcolors-buy-order-done-color-u).
    pub buy_order_done_color_u: u32,
    /// Legacy stored color without an active renderer consumer. Preserve it; do not offer it as an effective color
    /// control.
    ///
    /// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/05-charts-and-display.md#intcolors-sell-order-done-color-u).
    pub sell_order_done_color_u: u32,
    /// Legacy stored color without an active renderer consumer. Preserve it; do not offer it as an effective color
    /// control.
    ///
    /// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/05-charts-and-display.md#intcolors-trailing-color-u).
    pub trailing_color_u: u32,
    /// Base price/order-line width in pixels; GUI 1..=5. Individual line renderers may add their own width
    /// adjustment.
    ///
    /// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/05-charts-and-display.md#intcolors-price-line-width).
    pub price_line_width: i32,
}

/// Chart drawing-tool appearance and interaction preferences; per-object alerts are separate.
///
/// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/06-drawing-and-news.md#customdrawconfig).
#[derive(Clone, Debug)]
pub struct CustomDrawConfig {
    /// Stored drawing-config version. Preserve the value received from the kernel.
    ///
    /// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/06-drawing-and-news.md#customdrawconfig-ver).
    pub ver: i32,
    /// Global drawing-stroke opacity, 0..=1. Order-line opacity has its own tool field.
    ///
    /// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/06-drawing-and-news.md#customdrawconfig-f-opacity).
    pub f_opacity: f32,
    /// Show/enable interaction with chart drawing objects. Hiding them does not delete saved objects.
    ///
    /// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/06-drawing-and-news.md#customdrawconfig-f-show).
    pub f_show: bool,
    /// 16 tool slots: 0=none, 1=price line, 2=channel, 3=Fibonacci, 4=triangle, 5=rectangle, 6=pencil, 7=order
    /// line, 8=system rectangle, 9=news marker; 10..15 reserved. Preserve reserved slots.
    ///
    /// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/06-drawing-and-news.md#customdrawconfig-tools).
    pub tools: [CustomDrawTool; 16],
}

/// Settings for a single chart draw tool.
///
/// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/06-drawing-and-news.md#customdrawtool).
#[derive(Clone, Debug)]
pub struct CustomDrawTool {
    /// Light-theme stroke color, ARGB (0xAARRGGBB).
    ///
    /// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/06-drawing-and-news.md#customdrawtool-color-w).
    pub color_w: u32,
    /// Dark-theme stroke color, ARGB (0xAARRGGBB).
    ///
    /// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/06-drawing-and-news.md#customdrawtool-color-b).
    pub color_b: u32,
    /// Stored legacy/reserved field without a current tool-config consumer; preserve it.
    ///
    /// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/06-drawing-and-news.md#customdrawtool-fill-color-w).
    pub fill_color_w: u32,
    /// Stored legacy/reserved field without a current tool-config consumer; preserve it.
    ///
    /// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/06-drawing-and-news.md#customdrawtool-fill-color-b).
    pub fill_color_b: u32,
    /// Stored legacy/reserved field without a current tool-config consumer; preserve it. Individual drawing-object
    /// alerts are separate state.
    ///
    /// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/06-drawing-and-news.md#customdrawtool-sound-alert).
    pub sound_alert: bool,
    /// Stored legacy/reserved field without a current tool-config consumer; preserve it. Individual drawing-object
    /// alerts are separate state.
    ///
    /// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/06-drawing-and-news.md#customdrawtool-sound-kind).
    pub sound_kind: i32,
    /// Stroke width in pixels; GUI 0.5..=6. For news markers, values below 0.6 hide the marker and its hit target.
    ///
    /// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/06-drawing-and-news.md#customdrawtool-thickness).
    pub thickness: f32,
    /// Line dash style: 0=solid, 1=dash, 2=dot, 3=dash-dot, 4=dash-dot-dot.
    ///
    /// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/06-drawing-and-news.md#customdrawtool-stroke).
    pub stroke: i32,
    /// For tool 7 (order line): opacity 0..=1, not width; below 0.1 hides its drawing. Preserve for other tools.
    ///
    /// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/06-drawing-and-news.md#customdrawtool-thickness-2).
    pub thickness_2: f32,
    /// Stored legacy/reserved field without a current tool-config consumer; preserve it.
    ///
    /// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/06-drawing-and-news.md#customdrawtool-stroke-2).
    pub stroke_2: i32,
    /// Stored legacy/reserved field without a current tool-config consumer; preserve it.
    ///
    /// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/06-drawing-and-news.md#customdrawtool-color-2w).
    pub color_2w: [u32; 6],
    /// Stored legacy/reserved field without a current tool-config consumer; preserve it.
    ///
    /// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/06-drawing-and-news.md#customdrawtool-color-2b).
    pub color_2b: [u32; 6],
    /// Six ARGB Fibonacci-zone fills; rectangle uses element 0. These are used in both themes despite the w suffix.
    ///
    /// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/06-drawing-and-news.md#customdrawtool-fill-color-2w).
    pub fill_color_2w: [u32; 6],
    /// Stored legacy/reserved field without a current tool-config consumer; preserve it.
    ///
    /// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/06-drawing-and-news.md#customdrawtool-fill-color-2b).
    pub fill_color_2b: [u32; 6],
    /// Pencil-only simulation: use drawn future points as synthetic market ticks in the supported emulator mode.
    /// This is not an order-on-line-crossing switch. GUI forces it off for other tools.
    ///
    /// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/06-drawing-and-news.md#customdrawtool-emulate-trades).
    pub emulate_trades: bool,
    /// Exclude this tool from cycling through drawing tools with a hotkey. News-marker tool is excluded by default.
    /// Does not prevent chart switching.
    ///
    /// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/06-drawing-and-news.md#customdrawtool-prevent-switch).
    pub prevent_switch: bool,
}

/// Visual and audible notifications for the one-hour BTC delta.
///
/// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/05-charts-and-display.md#blinkconfig).
#[derive(Clone, Debug)]
pub struct BlinkConfig {
    /// Blink when the one-hour BTC delta leaves the lower/upper bounds. Blinking clears after the delta returns
    /// inside 99% of each threshold: for -5%/+5%, inside approximately -4.95%/+4.95%. This notification does not
    /// stop trading.
    ///
    /// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/05-charts-and-display.md#blinkconfig-blink-btc).
    pub blink_btc: bool,
    /// Lower BTC one-hour delta bound, percent. The GUI stores a non-positive value (-abs(input)).
    ///
    /// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/05-charts-and-display.md#blinkconfig-blink-btc-delta).
    pub blink_btc_delta: f64,
    /// Upper BTC one-hour delta bound, percent. The GUI stores abs(input).
    ///
    /// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/05-charts-and-display.md#blinkconfig-blink-btc-delta-up).
    pub blink_btc_delta_up: f64,
    /// Repeat a sound every five seconds while BTC blinking is active; requires `blink_btc`.
    ///
    /// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/05-charts-and-display.md#blinkconfig-alarm-btc).
    pub alarm_btc: bool,
    /// Normal sound preset 1..=11; 0 is silent. Used when `alarm_btc` is enabled.
    ///
    /// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/05-charts-and-display.md#blinkconfig-alarm-type).
    pub alarm_type: u8,
}

/// Chart filter/session/detection annotation and cursor-tool preferences.
///
/// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/05-charts-and-display.md#showfilters).
#[derive(Clone, Debug)]
pub struct ShowFilters {
    /// Show the strategy-filter/session annotation area and eligible MoonShot bounds on charts. Does not enable the
    /// underlying strategy conditions.
    ///
    /// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/05-charts-and-display.md#showfilters-need-show).
    pub need_show: bool,
    /// Show strategy-filter annotations when `need_show` is enabled.
    ///
    /// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/05-charts-and-display.md#showfilters-filters).
    pub filters: bool,
    /// Show session annotations when `need_show` is enabled.
    ///
    /// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/05-charts-and-display.md#showfilters-sessions).
    pub sessions: bool,
    /// Hide inactive or near-zero session annotations, using the session limits, counters and penalty state. This
    /// is not a report filter for exactly zero profit.
    ///
    /// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/05-charts-and-display.md#showfilters-hide-zero).
    pub hide_zero: bool,
    /// Show eligible MoonShot price boundaries in the annotation area; requires `need_show`.
    ///
    /// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/05-charts-and-display.md#showfilters-m-shot-area).
    pub m_shot_area: bool,
    /// Show the percentage-distance tool between the cursor price and spread. Independent of `need_show`; does not
    /// add zoom controls.
    ///
    /// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/05-charts-and-display.md#showfilters-scale-tool).
    pub scale_tool: bool,
    /// Allow Alt+mouse-wheel scrolling inside the visible filter area.
    ///
    /// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/05-charts-and-display.md#showfilters-scroll-filters).
    pub scroll_filters: bool,
    /// Show detection annotations on charts. Separate from the `need_show` filter-area switch and from detection
    /// execution.
    ///
    /// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/05-charts-and-display.md#showfilters-show-detects).
    pub show_detects: bool,
}

/// Chart windows manager settings.
///
/// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/05-charts-and-display.md#chartssettings).
#[derive(Clone, Debug)]
pub struct ChartsSettings {
    /// Automatically arrange charts in monitoring windows.
    ///
    /// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/05-charts-and-display.md#chartssettings-auto-arrange).
    pub auto_arrange: bool,
    /// Saved visibility of monitoring windows, not all main-window charts.
    ///
    /// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/05-charts-and-display.md#chartssettings-visible).
    pub visible: bool,
    /// Use the wide layout of charts in monitoring windows.
    ///
    /// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/05-charts-and-display.md#chartssettings-wide).
    pub wide: bool,
    /// Keep monitoring windows above other windows.
    ///
    /// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/05-charts-and-display.md#chartssettings-stay-on-top).
    pub stay_on_top: bool,
    /// Maximum chart frames per monitoring window. GUI minimum: 10. Not the number of monitoring windows; use the
    /// received value rather than SDK Default (0).
    ///
    /// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/05-charts-and-display.md#chartssettings-max-charts).
    pub max_charts: i32,
    /// Recentring interval selector: 0=5 s, 1=15 s, 2=30 s, 3=60 s, 4=300 s, 5=900 s. Recentres charts around the
    /// latest price; not FPS or a data-download interval.
    ///
    /// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/05-charts-and-display.md#chartssettings-refresh).
    pub refresh: i32,
    /// Reserved legacy value without a current consumer; preserve it.
    ///
    /// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/05-charts-and-display.md#chartssettings-x1).
    pub x1: f64,
    /// Reserved legacy value without a current consumer; preserve it.
    ///
    /// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/05-charts-and-display.md#chartssettings-x2).
    pub x2: f64,
}

/// Trade-size heatmap and optional CPU, feed-latency and rendering overlays.
///
/// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/05-charts-and-display.md#heatmapconfig).
#[derive(Clone, Debug)]
pub struct HeatMapConfig {
    /// Show the heatmap area. Trade-size heatmap requires fast charts and market-maker detection; selecting any
    /// CPU/trade/app/draw diagnostic overlay replaces it with system data. Not a new market subscription.
    ///
    /// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/05-charts-and-display.md#heatmapconfig-show).
    pub show: bool,
    /// Calculate relative heatmap intensity from trade quantity (normalized by a common average price), instead of
    /// each trade's notional at its own price.
    ///
    /// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/05-charts-and-display.md#heatmapconfig-use-q).
    pub use_q: bool,
    /// Heatmap height as a fraction of the chart; GUI 0.05..=0.8.
    ///
    /// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/05-charts-and-display.md#heatmapconfig-height).
    pub height: f32,
    /// Show the CPU history overlay. A display switch, not a detailed profiler.
    ///
    /// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/05-charts-and-display.md#heatmapconfig-cpu).
    pub cpu: bool,
    /// Show trade-feed latency history, not trade count or throughput.
    ///
    /// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/05-charts-and-display.md#heatmapconfig-trades).
    pub trades: bool,
    /// Show application-latency history.
    ///
    /// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/05-charts-and-display.md#heatmapconfig-app-latency).
    pub app_latency: bool,
    /// Show chart-rendering latency history.
    ///
    /// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/05-charts-and-display.md#heatmapconfig-draw-latency).
    pub draw_latency: bool,
}

/// News feed form display settings.
///
/// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/06-drawing-and-news.md#newsformconfig).
#[derive(Clone, Debug)]
pub struct NewsFormConfig {
    /// Keep the news window above other windows.
    ///
    /// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/06-drawing-and-news.md#newsformconfig-stay-on-top).
    pub stay_on_top: bool,
    /// Show exact news time instead of relative age.
    ///
    /// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/06-drawing-and-news.md#newsformconfig-exact-time).
    pub exact_time: bool,
    /// News-card font size; GUI 12..=16.
    ///
    /// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/06-drawing-and-news.md#newsformconfig-font-size).
    pub font_size: u8,
    /// Inactive saved filter: opening the current news form resets it to 0 and hides its control. Do not expose as
    /// a working strength filter.
    ///
    /// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/06-drawing-and-news.md#newsformconfig-strength).
    pub strength: u8,
    /// Use text in the current GUI language when available; false uses the original text. Not a server-side
    /// news-edit permission.
    ///
    /// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/06-drawing-and-news.md#newsformconfig-update-orig).
    pub update_orig: bool,
    /// News sound-list index: 0=off; positive indices select the kernel's named-sound list. This is not a boolean
    /// or the ordinary 1..11 alert-preset list.
    ///
    /// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/06-drawing-and-news.md#newsformconfig-sound).
    pub sound: u8,
    /// News theme: 0=Light Paper, 1=Light Default, 2=Light Classic, 3=Light Quartz, 4=Light Mist, 5=Light Sand,
    /// 6=Dark Carbon, 7=Dark Navy, 8=Dark Slate. Separate from the main GUI style.
    ///
    /// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/06-drawing-and-news.md#newsformconfig-theme).
    pub theme: i32,
    /// Coin-card font size: non-positive means default 13; positive values are clamped to 8..=30.
    ///
    /// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/06-drawing-and-news.md#newsformconfig-coin-card-font-size).
    pub coin_card_font_size: i32,
    /// Show all tags on a news card. When false and tag selections exist, show only selected matching tags; does
    /// not alter the news itself.
    ///
    /// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/06-drawing-and-news.md#newsformconfig-full-tags).
    pub full_tags: bool,
    /// Show the news feed without the selected-tag visibility filter. Token filters still apply. Not a
    /// scrolling/layout mode.
    ///
    /// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/06-drawing-and-news.md#newsformconfig-feed-mode).
    pub feed_mode: bool,
}

// ---------------------------------------------------------------------------
// Section 4: Theme (Kind = 4, internal version = 1)
// ---------------------------------------------------------------------------

/// Active color theme and per-theme INI color overrides.
///
/// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/08-theme-and-ini.md#themesection).
#[derive(Clone, Debug, Default)]
pub struct ThemeSection {
    /// Main GUI style: 0=Native, 1=Light, 2=Diamond, 3=Dark, 4=Dark OldSchool. Styles 3/4 use the dark color set;
    /// 0/1/2 use the light one.
    ///
    /// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/08-theme-and-ini.md#themesection-current-style).
    pub current_style: i32,
    /// ColorsLight and ColorsDark sections. Import replaces every supplied section completely: retain untouched
    /// keys. See the INI section notes in MoonDoc.
    ///
    /// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/08-theme-and-ini.md#themesection-ini-sections).
    pub ini_sections: Vec<IniSectionData>,
    /// Preserve opaque bytes from newer kernels unchanged.
    ///
    /// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/08-theme-and-ini.md#themesection-unknown-tail).
    pub unknown_tail: Vec<u8>,
}

// ---------------------------------------------------------------------------
// Section 5: Ini (Kind = 5, internal version = 1)
// ---------------------------------------------------------------------------

/// Chart indicator and arbitrage color INI settings.
///
/// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/08-theme-and-ini.md#inisection).
#[derive(Clone, Debug, Default)]
pub struct IniSection {
    /// Charts and ArbColors sections. Import replaces every supplied section completely; unknown section names are
    /// rejected. See the INI key reference in MoonDoc.
    ///
    /// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/08-theme-and-ini.md#inisection-ini-sections).
    pub ini_sections: Vec<IniSectionData>,
    /// Preserve opaque bytes from newer kernels unchanged.
    ///
    /// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/08-theme-and-ini.md#inisection-unknown-tail).
    pub unknown_tail: Vec<u8>,
}

/// A single INI section: a name and an ordered list of key/value pairs.
/// Order is preserved for byte-exact roundtrip (NOT a HashMap).
///
/// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/08-theme-and-ini.md#inisectiondata).
#[derive(Clone, Debug, PartialEq)]
pub struct IniSectionData {
    /// Section name from the corresponding allowlist: ColorsLight/ColorsDark in theme, Charts/ArbColors in ini.
    ///
    /// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/08-theme-and-ini.md#inisectiondata-name).
    pub name: String,
    /// Ordered text key/value pairs. Preserve keys you do not edit. Import erases the supplied section before
    /// writing its entries; an empty list clears it.
    ///
    /// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/08-theme-and-ini.md#inisectiondata-entries).
    pub entries: Vec<(String, String)>,
}

// ---------------------------------------------------------------------------
// Section 6: UI (Kind = 6, internal version = 3)
// ---------------------------------------------------------------------------

/// Hotkeys, markets table layout, toolbar, strategy editor state.
///
/// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/07-interface-and-hotkeys.md#uisection).
#[derive(Clone, Debug)]
pub struct UiSection {
    /// Hide the demo-mode GUI button; does not switch emulation on/off.
    ///
    /// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/07-interface-and-hotkeys.md#uisection-hide-demo-button).
    pub hide_demo_button: bool,
    /// Ask for confirmation when closing the GUI normally. Update and OS-session shutdown paths can bypass this
    /// prompt.
    ///
    /// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/07-interface-and-hotkeys.md#uisection-confirm-close).
    pub confirm_close: bool,
    /// Place newly opened/reopened market charts at the top of the chart list. Does not sort exchange listings by
    /// listing date.
    ///
    /// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/07-interface-and-hotkeys.md#uisection-new-markets-on-top).
    pub new_markets_on_top: bool,
    /// Market-table sort direction multiplier: +1 or -1. Selected column is `markets_table_config.sort_col`. Not a
    /// column selector.
    ///
    /// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/07-interface-and-hotkeys.md#uisection-coins-sort-order).
    pub coins_sort_order: i32,

    /// Saved Moonbot GUI keyboard shortcuts and manual-trading presets. Saving them does not register shortcuts in
    /// a remote terminal.
    ///
    /// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/07-interface-and-hotkeys.md#uisection-hotkeys-config).
    pub hotkeys_config: HotkeysConfig,

    /// Comma-separated displayed chapter names that were expanded in the strategy editor, not numeric indices.
    /// Preserve as GUI state.
    ///
    /// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/07-interface-and-hotkeys.md#uisection-strat-editor-chapters).
    pub strat_editor_chapters: String,

    /// Saved column visibility and sort-column selection of the kernel GUI market table.
    ///
    /// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/07-interface-and-hotkeys.md#uisection-markets-table-config).
    pub markets_table_config: MarketsTableConfig,

    /// First configurable toolbar button: 1=Help, 2=Reports, 3=Alerts, 4=Assets/Transfer, 6=BackTest. Other values
    /// normalize to Help.
    ///
    /// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/07-interface-and-hotkeys.md#uisection-main-button-index-1).
    pub main_button_index_1: u8,
    /// 11 saved expansion flags for strategy-editor subchapters in construction order. Not one flag per strategy.
    ///
    /// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/07-interface-and-hotkeys.md#uisection-strat-expanded-state).
    pub strat_expanded_state: [bool; 11],

    /// Preserve opaque bytes from newer kernels unchanged.
    ///
    /// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/07-interface-and-hotkeys.md#uisection-unknown-tail).
    pub unknown_tail: Vec<u8>,
}

/// Keyboard shortcuts for order and chart operations.
///
/// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/07-interface-and-hotkeys.md#hotkeysconfig).
#[derive(Clone, Debug)]
pub struct HotkeysConfig {
    /// Legacy initialized marker without an active consumer; preserve.
    ///
    /// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/07-interface-and-hotkeys.md#hotkeysconfig-filled).
    pub filled: bool,
    /// Stored shortcut-record version; preserve the received value.
    ///
    /// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/07-interface-and-hotkeys.md#hotkeysconfig-ver).
    pub ver: u8,
    /// Six manual order-amount presets in the trading currency. Used by `use_manual_balance`; amounts are not
    /// percentages. Array index 0 corresponds to button 1.
    ///
    /// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/07-interface-and-hotkeys.md#hotkeysconfig-o-size).
    pub o_size: [f64; 6],
    /// Selected amount button, 1..=6 (one-based). Selecting it copies `o_size[b_num - 1]` to `fixed_trade_balance`.
    ///
    /// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/07-interface-and-hotkeys.md#hotkeysconfig-b-num).
    pub b_num: i32,
    /// Six shortcuts selecting amount presets; require `use_manual_balance`. Each is an encoded shortcut,
    /// 0=unassigned.
    ///
    /// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/07-interface-and-hotkeys.md#hotkeysconfig-o-keys).
    pub o_keys: [u16; 6],
    /// Number of parts for `split_order_x`; GUI accepts 1..=199. Not an iceberg setting. Actual splitting still
    /// obeys exchange lot-size rules.
    ///
    /// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/07-interface-and-hotkeys.md#hotkeysconfig-split-parts).
    pub split_parts: u8,
    /// Selected sell-price button, 1..=6. The GUI clamps the selection and copies `s_price[sb_num - 1]` to
    /// `fixed_sell_price`.
    ///
    /// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/07-interface-and-hotkeys.md#hotkeysconfig-sb-num).
    pub sb_num: u8,
    /// Six shortcuts selecting sell-percentage presets; require `fixed_sell_mode`. 0=unassigned.
    ///
    /// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/07-interface-and-hotkeys.md#hotkeysconfig-s-keys).
    pub s_keys: [u16; 6],
    /// Six sell-offset presets in percent, not absolute prices. The xT mode can multiply the effective value by 10;
    /// selecting a button updates `fixed_sell_price`.
    ///
    /// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/07-interface-and-hotkeys.md#hotkeysconfig-s-price).
    pub s_price: [f32; 6],
    /// Shortcut: Cancel buys on the active full-screen market. 0=unassigned.
    ///
    /// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/07-interface-and-hotkeys.md#hotkeysconfig-cancel-buy).
    pub cancel_buy: u16,
    /// Shortcut: Panic Sell on the active full-screen market. 0=unassigned.
    ///
    /// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/07-interface-and-hotkeys.md#hotkeysconfig-panic-sell).
    pub panic_sell: u16,
    /// Shortcut: Join closing orders on the active full-screen market; the kernel selects the applicable position
    /// direction. 0=unassigned.
    ///
    /// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/07-interface-and-hotkeys.md#hotkeysconfig-join-sells).
    pub join_sells: u16,
    /// Shortcut: Cycle to the next full-screen chart. 0=unassigned.
    ///
    /// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/07-interface-and-hotkeys.md#hotkeysconfig-switch-charts).
    pub switch_charts: u16,
    /// Shortcut: Request order-book reload and refresh active chart TF candles. 0=unassigned.
    ///
    /// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/07-interface-and-hotkeys.md#hotkeysconfig-reload-book).
    pub reload_book: u16,
    /// Shortcut: Trigger a manual long entry for the selected market through the normal signal/order flow.
    /// 0=unassigned.
    ///
    /// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/07-interface-and-hotkeys.md#hotkeysconfig-new-long).
    pub new_long: u16,
    /// Shortcut: Trigger a manual short entry for the selected market through the normal signal/order flow.
    /// 0=unassigned.
    ///
    /// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/07-interface-and-hotkeys.md#hotkeysconfig-new-short).
    pub new_short: u16,
    /// Shortcut: Split the largest eligible active closing order on the selected market into three parts. Not the
    /// hovered order; unavailable in emulator mode. 0=unassigned.
    ///
    /// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/07-interface-and-hotkeys.md#hotkeysconfig-split-order).
    pub split_order: u16,
    /// Shortcut: Move buys up on the selected market using the normal group-move step. 0=unassigned.
    ///
    /// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/07-interface-and-hotkeys.md#hotkeysconfig-shift-buy-up).
    pub shift_buy_up: u16,
    /// Shortcut: Move buys down on the selected market using the normal group-move step. 0=unassigned.
    ///
    /// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/07-interface-and-hotkeys.md#hotkeysconfig-shift-buy-down).
    pub shift_buy_down: u16,
    /// Shortcut: Move closing orders up on the selected market. 0=unassigned.
    ///
    /// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/07-interface-and-hotkeys.md#hotkeysconfig-shift-sell-up).
    pub shift_sell_up: u16,
    /// Shortcut: Move closing orders down on the selected market. 0=unassigned.
    ///
    /// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/07-interface-and-hotkeys.md#hotkeysconfig-shift-sell-down).
    pub shift_sell_down: u16,
    /// Shortcut: Capture the active report/history chart, otherwise the last full-screen market chart.
    /// 0=unassigned.
    ///
    /// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/07-interface-and-hotkeys.md#hotkeysconfig-make-shot).
    pub make_shot: u16,
    /// Shortcut: Capture the bot window. 0=unassigned.
    ///
    /// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/07-interface-and-hotkeys.md#hotkeysconfig-make-shot-bot).
    pub make_shot_bot: u16,
    /// Shortcut: Reload trade history for the selected market. 0=unassigned.
    ///
    /// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/07-interface-and-hotkeys.md#hotkeysconfig-reload-chart).
    pub reload_chart: u16,
    /// Shortcut: Increase the displayed price range (zoom out vertically). 0=unassigned.
    ///
    /// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/07-interface-and-hotkeys.md#hotkeysconfig-scale-plus).
    pub scale_plus: u16,
    /// Shortcut: Decrease the displayed price range (zoom in vertically). 0=unassigned.
    ///
    /// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/07-interface-and-hotkeys.md#hotkeysconfig-scale-minus).
    pub scale_minus: u16,
    /// Shortcut: Increase the sell-offset setting; may update the selected Manual strategy unless
    /// `ignore_strat_sell_price` is set. 0=unassigned.
    ///
    /// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/07-interface-and-hotkeys.md#hotkeysconfig-sell-plus).
    pub sell_plus: u16,
    /// Shortcut: Decrease the sell-offset setting; same Manual-strategy dependency as `sell_plus`. 0=unassigned.
    ///
    /// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/07-interface-and-hotkeys.md#hotkeysconfig-sell-minus).
    pub sell_minus: u16,
    /// Shortcut: Toggle the GUI privacy display mode. 0=unassigned.
    ///
    /// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/07-interface-and-hotkeys.md#hotkeysconfig-spy-mode).
    pub spy_mode: u16,
    /// Shortcut: Toggle chart visibility. 0=unassigned.
    ///
    /// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/07-interface-and-hotkeys.md#hotkeysconfig-show-charts).
    pub show_charts: u16,
    /// Shortcut: Same largest-order selection as `split_order`, using `split_parts` instead of three. 0=unassigned.
    ///
    /// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/07-interface-and-hotkeys.md#hotkeysconfig-split-order-x).
    pub split_order_x: u16,
    /// Shortcut: Cycle drawing tools, skipping tools with `prevent_switch`. 0=unassigned.
    ///
    /// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/07-interface-and-hotkeys.md#hotkeysconfig-switch-figure).
    pub switch_figure: u16,
    /// Shortcut: Toggle rectangle-drawing mode for redistributing closing orders within the drawn price zone; not
    /// an order-book fit. 0=unassigned.
    ///
    /// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/07-interface-and-hotkeys.md#hotkeysconfig-fit-sells).
    pub fit_sells: u16,
    /// Shortcut: Panic-close one order selected by the kernel on the active market. 0=unassigned.
    ///
    /// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/07-interface-and-hotkeys.md#hotkeysconfig-panic-sell-one).
    pub panic_sell_one: u16,
    /// Shortcut: Cancel all buys and pending orders in this bot. Distinct from broadcasting cancellation to other
    /// bots. 0=unassigned.
    ///
    /// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/07-interface-and-hotkeys.md#hotkeysconfig-cancel-all-buys).
    pub cancel_all_buys: u16,
    /// Shortcut: Broadcast the selected ticker through the local bot-window mechanism. Not a MoonProto thin-client
    /// broadcast. 0=unassigned.
    ///
    /// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/07-interface-and-hotkeys.md#hotkeysconfig-broadcast).
    pub broadcast: u16,
}

/// Saved market-table column visibility and sort-column selection.
///
/// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/07-interface-and-hotkeys.md#marketstableconfig).
#[derive(Clone, Debug)]
pub struct MarketsTableConfig {
    /// Zero-based displayed column index; -1 restores market-index order. The current GUI has 28 displayed columns,
    /// listed in MoonDoc; not the internal metric ID.
    ///
    /// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/07-interface-and-hotkeys.md#marketstableconfig-sort-col).
    pub sort_col: i32,
    /// 41 stored visibility flags, indexed by displayed column; only the current 28 columns are consumed. Preserve
    /// the remaining slots.
    ///
    /// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/07-interface-and-hotkeys.md#marketstableconfig-col-vis).
    pub col_vis: [bool; 41],
    /// Legacy stored positions without a current layout consumer. Preserve them; editing does not reorder current
    /// columns.
    ///
    /// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/07-interface-and-hotkeys.md#marketstableconfig-col-pos).
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
