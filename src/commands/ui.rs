//! MoonProto UI/settings command channel.
//!
//! ## CmdId mapping
//! - 1  — `TClientSettingsCommand`  (Sliced, UK_BaseUISettings) — full UI/settings snapshot
//! - 2  — `TSettingsRequest`        (empty)
//! - 3  — `TStratStartStopCommand`  (boolean IsStart)
//! - 4  — `TStratStartStopCommandV2` (IsStart + Items[StratCheckedItem])
//! - 5  — `TMMOrdersSubscribeCommand` (boolean Subscribe, UK_TurnMMDetection)
//! - 6  — `TUpdateVersionCommand`   (VersionName + IsRelease)
//! - 7  — `TEmuTradesCommand`       (Sliced, mIndex + BaseTime + Points[EmuTradePoint])
//! - 8  — `TNewMarketNotifyCommand` (empty, Priority=High)
//! - 9  — `TLevManageCommand`       (Sliced, UK_LevManageSettings)
//! - 10 — `TTriggerManageCommand`   (Sliced, Markets/Keys[])
//! - 11 — `TResetProfitCommand`     (1 byte kind)
//! - 12 — `TArbActivateNotify`      (TDateTime ArbValid)
//! - 13 — `TSwitchDexCommand`       (ShortString\[15\] DexName, UK_DexSwitch, High)
//! - 14 — `TSwitchSpotCommand`      (byte SpotIndex, UK_SpotSwitch, High)
//! - 15 — `TAlertObjectCommand`     (Sliced, authoritative chart alert object)
//! - 16 — `TAlertSnapshotRequest`   (empty)
//! - 17 — `TChartTextStateCommand`  (High, UK_ChartTextState)
//! - 18 — `TChartTextSnapshotCommand` (Sliced, UK_ChartTextSnapshot)
//! - 19 — `TOrdersHistoryRequestCommand` (market name)
//! - 20 — `TRuntimeStateCommand`    (High, server runtime/passive state)
//! - 21 — `TRestartNowCommand`      (High, restart/start runtime now)
//! - 22 — `TKernelLicenseStateCommand` (High, license and MoonCredits state)
//! - 23 — `TKernelLicenseStateRequest` (High, request license/MoonCredits state)
//! - 24 — `TProfitStateCommand`     (High, current report/profit counters)
//! - 25 — `TAutoDetectCommand`       (High, set AutoDetect/passive-mode state)
//! - 26 — `TNewsRelayCommand`       (news frame/tags relay)
//! - 27 — `TNewsHistoryCommand`     (Sliced, UK_NewsHistory, startup news history)
//! - 28 — `TSharedConfigCommand`    (Sliced, UK_SharedConfig, gzip safe-share payload)
//! - 29 — `TSharedConfigRequest`    (empty, request the kernel's safe-share config)
//! - 30 — `THLRequestLimitStateCommand` (High, HyperLiquid action requests left)
//! - 31 — `TShutdownCommand`        (High, request graceful core shutdown)
//! - 32 - `TProblemsStateCommand`   (Sliced, complete confirmed-problem list)
//! - 33 - `TProblemNotifyCommand`   (High, newly confirmed problem)
//! - 34 - `TProblemsClearCommand`  (High, clear all core problems)
//! - 35 - `TProblemsTestCommand`   (High, publish a test problem)
//! - 36 - `TTelegramStateCommand`  (Sliced, complete Telegram service/auth state)
//! - 37..48 - Telegram state request and explicit account/service actions (High)
//! - 49 - `TLogSubscribeCommand` (High, per-client log subscription)
//!
//! ## ASCfg / ASCfg2 blobs
//! `TAutoStartConfig` (104 bytes) and `TAutoStartConfig2` (168 bytes) are
//! fixed packed settings blobs. On the wire they are encoded as
//! `Word size + bytes(size)` with soft-read semantics: extra tail bytes are
//! skipped and short payloads are partially copied. Active Lib preserves the
//! hidden blobs for exact roundtrip and exposes typed `AutoStartConfig` /
//! `AutoStartConfig2` views for terminal UI edits.
//!
//! ## ArbConfig compact format
//! This is a compact wire structure, not a raw settings record. The wire form
//! is `ver:byte + wantedSet:bytes(32) + flags:byte + colorCount:byte +
//! colorCount*5 bytes`. `wantedSet` is a 256-bit platform mask.

#![cfg_attr(feature = "diagnostics", allow(dead_code))]

use super::market::ArbPlatformCode;
use super::registry::{decode_utf8_delphi, read_string, write_string, CURRENT_PROTO_CMD_VER};
use super::strat::StratCheckedItem;
use std::time::Duration;
use zerocopy::byteorder::little_endian::{F32 as LeF32, F64 as LeF64, I32 as LeI32, U16 as LeU16};
use zerocopy::{FromBytes, Immutable, IntoBytes, KnownLayout, Unaligned};

mod builders;
mod parser;
mod telegram;
pub(crate) use telegram::TelegramAction;

use crate::time::MoonTime;
#[cfg(test)]
pub use builders::build_chart_text_snapshot_for_test;
#[cfg(test)]
pub(crate) use builders::build_new_market_notify;
pub(crate) use builders::{
    build_core_update, build_settings_snapshot_request,
    build_alert_object, build_alert_snapshot_request, build_arb_activate_notify, build_auto_detect,
    build_chart_text_state, build_client_settings, build_emu_trades,
    build_kernel_license_state_request, build_lev_manage, build_logs_subscribe, build_mm_orders_subscribe,
    build_orders_history_request, build_problems_clear, build_problems_test, build_reset_profit, build_reset_session,
    build_restart_now, build_settings_request, build_shared_config_blob,
    build_shared_config_request, build_shutdown, build_strat_start_stop, build_strat_start_stop_v2,
    build_switch_dex, build_switch_spot, build_trigger_manage, build_update_version,
};

// --- CmdId constants ---
const CMD_CLIENT_SETTINGS: u8 = 1;
const CMD_SETTINGS_REQUEST: u8 = 2;
const CMD_STRAT_START_STOP: u8 = 3;
const CMD_STRAT_START_STOP_V2: u8 = 4;
const CMD_MM_ORDERS_SUBSCRIBE: u8 = 5;
const CMD_UPDATE_VERSION: u8 = 6;
const CMD_EMU_TRADES: u8 = 7;
const CMD_NEW_MARKET_NOTIFY: u8 = 8;
const CMD_LEV_MANAGE: u8 = 9;
const CMD_TRIGGER_MANAGE: u8 = 10;
const CMD_RESET_PROFIT: u8 = 11;
const CMD_ARB_ACTIVATE_NOTIFY: u8 = 12;
const CMD_SWITCH_DEX: u8 = 13;
const CMD_SWITCH_SPOT: u8 = 14;
const CMD_ALERT_OBJECT: u8 = 15;
const CMD_ALERT_SNAPSHOT_REQUEST: u8 = 16;
const CMD_CHART_TEXT_STATE: u8 = 17;
const CMD_CHART_TEXT_SNAPSHOT: u8 = 18;
const CMD_ORDERS_HISTORY_REQUEST: u8 = 19;
const CMD_RUNTIME_STATE: u8 = 20;
const CMD_RESTART_NOW: u8 = 21;
const CMD_KERNEL_LICENSE_STATE: u8 = 22;
const CMD_KERNEL_LICENSE_STATE_REQUEST: u8 = 23;
const CMD_PROFIT_STATE: u8 = 24;
const CMD_AUTO_DETECT: u8 = 25;
const CMD_NEWS_RELAY: u8 = 26;
const CMD_NEWS_HISTORY: u8 = 27;
const CMD_SHARED_CONFIG: u8 = 28;
const CMD_SHARED_CONFIG_REQUEST: u8 = 29;
const CMD_HL_REQUEST_LIMIT_STATE: u8 = 30;
const CMD_SHUTDOWN: u8 = 31;
const CMD_PROBLEMS_STATE: u8 = 32;
const CMD_PROBLEM_NOTIFY: u8 = 33;
const CMD_PROBLEMS_CLEAR: u8 = 34;
const CMD_PROBLEMS_TEST: u8 = 35;
const CMD_TELEGRAM_STATE: u8 = 36;

pub(crate) const NEWS_RELAY_KIND_NEWS: u8 = 0;
pub(crate) const NEWS_RELAY_KIND_TAGS: u8 = 1;

/// State that can arrive from the core before the client's market initialization finishes.
/// Keep both receive gates on the same list; these payloads do not require market indexes.
#[inline]
pub(crate) fn is_startup_state_payload(payload: &[u8]) -> bool {
    matches!(payload.first(), Some(
        &CMD_RUNTIME_STATE | &CMD_KERNEL_LICENSE_STATE | &CMD_PROFIT_STATE
        | &CMD_NEWS_RELAY | &CMD_NEWS_HISTORY | &CMD_HL_REQUEST_LIMIT_STATE
        | &CMD_PROBLEMS_STATE | &CMD_PROBLEM_NOTIFY | &CMD_TELEGRAM_STATE | &58 | &61
    ))
}

const LEV_CMD_VER: u8 = 1;

/// `TAutoStartConfig` packed record size in bytes (Config.pas:344).
#[doc(hidden)]
pub(crate) const AS_CFG_SIZE: usize = 104;
/// `TAutoStartConfig2` packed record size in bytes (Config.pas:384).
#[doc(hidden)]
pub(crate) const AS_CFG2_SIZE: usize = 168;

#[repr(C, packed)]
#[derive(Debug, Clone, Copy, FromBytes, IntoBytes, KnownLayout, Immutable, Unaligned)]
struct WireAutoStartConfig {
    auto_start: u8,
    auto_detect_on: u8,
    strategies_on: u8,
    work_time: u8,
    auto_stop_if_loss: u8,
    remember_state: u8,
    sell_if_loss: u8,
    dont_wait_sells: u8,
    auto_stop_loss: LeF64,
    panic_btc: u8,
    panic_market: u8,
    auto_stop_if_loss_hours: u8,
    auto_update: u8,
    restart_after_err: u8,
    restart_after_ping: u8,
    ignore_emulator: u8,
    _pad0: u8,
    stop_trades: LeI32,
    restart_err_time: LeI32,
    panic_btc_delta: LeF64,
    panic_market_delta: LeF64,
    auto_stop_on_errors: u8,
    auto_stop_on_ping: u8,
    sell_all_on_errors: u8,
    sell_all_on_ping: u8,
    errors_level: LeI32,
    ping_level: LeI32,
    restart_ping_time: LeI32,
    auto_stop_hours_val: LeF64,
    stop_hours: LeI32,
    stop_hours_trades: LeI32,
    panic_btc_delta_up: LeF64,
    work_time_from: LeF64,
    work_time_to: LeF64,
}

#[repr(C, packed)]
#[derive(Debug, Clone, Copy, FromBytes, IntoBytes, KnownLayout, Immutable, Unaligned)]
struct WireAutoStartConfig2 {
    restart_on_market: u8,
    _pad0: [u8; 7],
    btc_higher_then: LeF64,
    btc_lower_then: LeF64,
    market_higher_then: LeF64,
    show_old_listing: u8,
    _u1: [u8; 8],
    reset_session: u8,
    _pad1: [u8; 2],
    _u2: [LeI32; 8],
    max_session_cap: LeI32,
    rs_hours: LeI32,
    _pad2: [u8; 4],
    _u3: [LeF64; 10],
}

const _: () = assert!(core::mem::size_of::<WireAutoStartConfig>() == AS_CFG_SIZE);
const _: () = assert!(core::mem::size_of::<WireAutoStartConfig2>() == AS_CFG2_SIZE);

impl WireAutoStartConfig {
    fn from_blob(blob: &[u8]) -> Self {
        let mut bytes = [0u8; AS_CFG_SIZE];
        copy_blob_prefix(&mut bytes, blob);
        Self::read_from_bytes(&bytes).expect("fixed in-memory TAutoStartConfig")
    }

    fn as_blob(self) -> Vec<u8> {
        self.as_bytes().to_vec()
    }

    fn to_public(self) -> AutoStartConfig {
        AutoStartConfig {
            auto_start: self.auto_start != 0,
            auto_detect_on: self.auto_detect_on != 0,
            strategies_on: self.strategies_on != 0,
            work_time: self.work_time != 0,
            auto_stop_if_loss: self.auto_stop_if_loss != 0,
            remember_state: self.remember_state != 0,
            sell_if_loss: self.sell_if_loss != 0,
            dont_wait_sells: self.dont_wait_sells != 0,
            auto_stop_loss: self.auto_stop_loss.get(),
            panic_btc: self.panic_btc != 0,
            panic_market: self.panic_market != 0,
            auto_stop_if_loss_hours: self.auto_stop_if_loss_hours != 0,
            auto_update: self.auto_update != 0,
            restart_after_err: self.restart_after_err != 0,
            restart_after_ping: self.restart_after_ping != 0,
            ignore_emulator: self.ignore_emulator != 0,
            stop_trades: self.stop_trades.get(),
            restart_err_time: self.restart_err_time.get(),
            panic_btc_delta: self.panic_btc_delta.get(),
            panic_market_delta: self.panic_market_delta.get(),
            auto_stop_on_errors: self.auto_stop_on_errors != 0,
            auto_stop_on_ping: self.auto_stop_on_ping != 0,
            sell_all_on_errors: self.sell_all_on_errors != 0,
            sell_all_on_ping: self.sell_all_on_ping != 0,
            errors_level: self.errors_level.get(),
            ping_level: self.ping_level.get(),
            restart_ping_time: self.restart_ping_time.get(),
            auto_stop_hours_val: self.auto_stop_hours_val.get(),
            stop_hours: self.stop_hours.get(),
            stop_hours_trades: self.stop_hours_trades.get(),
            panic_btc_delta_up: self.panic_btc_delta_up.get(),
            work_time_from: self.work_time_from.get(),
            work_time_to: self.work_time_to.get(),
        }
    }

    fn write_public(&mut self, cfg: &AutoStartConfig) {
        self.auto_start = cfg.auto_start as u8;
        self.auto_detect_on = cfg.auto_detect_on as u8;
        self.strategies_on = cfg.strategies_on as u8;
        self.work_time = cfg.work_time as u8;
        self.auto_stop_if_loss = cfg.auto_stop_if_loss as u8;
        self.remember_state = cfg.remember_state as u8;
        self.sell_if_loss = cfg.sell_if_loss as u8;
        self.dont_wait_sells = cfg.dont_wait_sells as u8;
        self.auto_stop_loss = LeF64::new(cfg.auto_stop_loss);
        self.panic_btc = cfg.panic_btc as u8;
        self.panic_market = cfg.panic_market as u8;
        self.auto_stop_if_loss_hours = cfg.auto_stop_if_loss_hours as u8;
        self.auto_update = cfg.auto_update as u8;
        self.restart_after_err = cfg.restart_after_err as u8;
        self.restart_after_ping = cfg.restart_after_ping as u8;
        self.ignore_emulator = cfg.ignore_emulator as u8;
        self.stop_trades = LeI32::new(cfg.stop_trades);
        self.restart_err_time = LeI32::new(cfg.restart_err_time);
        self.panic_btc_delta = LeF64::new(cfg.panic_btc_delta);
        self.panic_market_delta = LeF64::new(cfg.panic_market_delta);
        self.auto_stop_on_errors = cfg.auto_stop_on_errors as u8;
        self.auto_stop_on_ping = cfg.auto_stop_on_ping as u8;
        self.sell_all_on_errors = cfg.sell_all_on_errors as u8;
        self.sell_all_on_ping = cfg.sell_all_on_ping as u8;
        self.errors_level = LeI32::new(cfg.errors_level);
        self.ping_level = LeI32::new(cfg.ping_level);
        self.restart_ping_time = LeI32::new(cfg.restart_ping_time);
        self.auto_stop_hours_val = LeF64::new(cfg.auto_stop_hours_val);
        self.stop_hours = LeI32::new(cfg.stop_hours);
        self.stop_hours_trades = LeI32::new(cfg.stop_hours_trades);
        self.panic_btc_delta_up = LeF64::new(cfg.panic_btc_delta_up);
        self.work_time_from = LeF64::new(cfg.work_time_from);
        self.work_time_to = LeF64::new(cfg.work_time_to);
    }
}

impl WireAutoStartConfig2 {
    fn from_blob(blob: &[u8]) -> Self {
        let mut bytes = [0u8; AS_CFG2_SIZE];
        copy_blob_prefix(&mut bytes, blob);
        Self::read_from_bytes(&bytes).expect("fixed in-memory TAutoStartConfig2")
    }

    fn as_blob(self) -> Vec<u8> {
        self.as_bytes().to_vec()
    }

    fn to_public(self) -> AutoStartConfig2 {
        AutoStartConfig2 {
            restart_on_market: self.restart_on_market != 0,
            btc_higher_than: self.btc_higher_then.get(),
            btc_lower_than: self.btc_lower_then.get(),
            market_higher_than: self.market_higher_then.get(),
            show_old_listing: self.show_old_listing != 0,
            reset_session: self.reset_session != 0,
            max_session_cap: self.max_session_cap.get(),
            rs_hours: self.rs_hours.get(),
        }
    }

    fn write_public(&mut self, cfg: &AutoStartConfig2) {
        self.restart_on_market = cfg.restart_on_market as u8;
        self.btc_higher_then = LeF64::new(cfg.btc_higher_than);
        self.btc_lower_then = LeF64::new(cfg.btc_lower_than);
        self.market_higher_then = LeF64::new(cfg.market_higher_than);
        self.show_old_listing = cfg.show_old_listing as u8;
        self.reset_session = cfg.reset_session as u8;
        self.max_session_cap = LeI32::new(cfg.max_session_cap);
        self.rs_hours = LeI32::new(cfg.rs_hours);
    }
}

/// ArbConfig wire version byte (ArbTypes.pas:25 `ARB_CONFIG_VER = 1`).
pub(crate) const ARB_CONFIG_VER: u8 = 1;

// =============================================================================
//  AutoStart typed views (TAutoStartConfig / TAutoStartConfig2)
// =============================================================================

/// Terminal-facing view of the first AutoStart settings page.
///
/// This is the settings-page meaning of the fixed 104-byte `as_cfg` blob. The
/// wire blob is still kept on [`ClientSettingsCommand`] for exact roundtrip and
/// append-only compatibility, but UI code should edit this typed view instead
/// of hand-parsing bytes.
/// The compact send path forces `auto_start`, `auto_detect_on`, `strategies_on`
/// to true and `remember_state` to false; use SharedConfig for those saved preferences.
/// [Startup and protection](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/01-startup-and-protection.md).
#[derive(Debug, Clone, PartialEq, Default)]
pub struct AutoStartConfig {
    /// Start market monitoring automatically after startup analysis completes. Startup preference; changing it does
    /// not start the running kernel immediately. The compact send path forces this to true; use SharedConfig to
    /// edit this saved preference.
    ///
    /// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/01-startup-and-protection.md#autostartconfig-auto-start).
    pub auto_start: bool,
    /// Enable detection when auto-start or an automatic recovery starts monitoring. With `remember_state`, this
    /// value is saved from the last running state. The compact send path forces this to true; use SharedConfig to
    /// edit this saved preference.
    ///
    /// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/01-startup-and-protection.md#autostartconfig-auto-detect-on).
    pub auto_detect_on: bool,
    /// Start checked strategies on auto-start or automatic recovery. With `remember_state`, the kernel remembers
    /// whether any strategy was active. The compact send path forces this to true; use SharedConfig to edit this
    /// saved preference.
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
    /// `auto_start`, `auto_detect_on` and `strategies_on` as the running state changes. The compact send path
    /// forces this to false; use SharedConfig to edit this saved preference.
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

/// Terminal-facing view of the second AutoStart settings page.
///
/// Reserved wire fields are intentionally not exposed, but `set_*` /
/// `update_*` helpers preserve the current reserved bytes in `as_cfg2`.
/// Units match SharedConfig; compact `*_than` fields correspond to its `*_then` names.
/// Recovery resumes trading, not the process.
/// [Recovery policy](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/01-startup-and-protection.md#autostartconfig2).
#[derive(Debug, Clone, PartialEq, Default)]
pub struct AutoStartConfig2 {
    /// Resume after the global delta panic when the one-hour BTC delta is between `btc_higher_than` and
    /// `btc_lower_than` and the one-hour market delta exceeds `market_higher_than`. Working-hour restrictions still
    /// apply.
    ///
    /// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/01-startup-and-protection.md#autostartconfig2-restart-on-market).
    pub restart_on_market: bool,
    /// Lower one-hour BTC delta bound for recovery, in percent. Requires `restart_on_market`; the comparison is
    /// strict. GUI stores a non-positive value.
    ///
    /// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/01-startup-and-protection.md#autostartconfig2-btc-higher-then).
    pub btc_higher_than: f64,
    /// Upper one-hour BTC delta bound for recovery, in percent. Requires `restart_on_market`; the comparison is
    /// strict. GUI stores a non-negative value.
    ///
    /// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/01-startup-and-protection.md#autostartconfig2-btc-lower-then).
    pub btc_lower_than: f64,
    /// Lower one-hour average market delta bound for recovery, in percent. Requires `restart_on_market`; the
    /// comparison is strict. GUI stores a non-positive value.
    ///
    /// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/01-startup-and-protection.md#autostartconfig2-market-higher-then).
    pub market_higher_than: f64,
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

// =============================================================================
//  ArbConfig — compact wire form (NOT raw record)
// =============================================================================

/// Compact arbitrage display config embedded into the settings snapshot.
///
/// The compact form keeps a 256-bit wanted-platform mask plus display flags.
/// [Display settings](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/05-charts-and-display.md#arbviewconfig).
#[derive(Debug, Clone)]
pub struct ArbConfigCompact {
    /// 256-bit wanted-platform mask.
    #[cfg(any(test, feature = "diagnostics"))]
    #[doc(hidden)]
    pub wanted: [bool; 256],
    #[cfg(not(any(test, feature = "diagnostics")))]
    pub(crate) wanted: [bool; 256],
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

impl Default for ArbConfigCompact {
    /// Defaults from `InitArbConfigDefaults`: show lines and percentages.
    fn default() -> Self {
        Self {
            wanted: [false; 256],
            show_absolute: false,
            show_numbers: false,
            show_lines: true,
            show_percent: true,
            show_right: false,
        }
    }
}

impl ArbConfigCompact {
    /// Whether arbitrage data for `platform` should be requested/shown.
    pub fn is_wanted(&self, platform: ArbPlatformCode) -> bool {
        self.wanted[platform.to_byte() as usize]
    }

    /// Set the wanted flag for one arbitrage platform.
    pub fn set_wanted(&mut self, platform: ArbPlatformCode, wanted: bool) {
        self.wanted[platform.to_byte() as usize] = wanted;
    }

    /// Iterate enabled platform codes.
    pub fn wanted_platforms(&self) -> impl Iterator<Item = ArbPlatformCode> + '_ {
        self.wanted
            .iter()
            .enumerate()
            .filter_map(|(code, wanted)| wanted.then_some(ArbPlatformCode::from_byte(code as u8)))
    }
}

// =============================================================================
//  EmuTradePoint (6-byte packed record)
// =============================================================================

/// One compact emulated chart tick: millisecond delta + f32 price.
///
/// The 6-byte row keeps replayed emulator paths small enough to send as a
/// normal chart-edit action, while UI code works with [`EmuPencilPoint`]
/// absolute time + price values.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct EmuTradePoint {
    time_delta_ms: u16,
    price: f32,
}

/// One drawn chart-pencil point for the MoonBot trade emulator.
///
/// This is not a wire record. It is the user-facing input matching
/// `TChartFrame.TryEmulatePrices`: the UI has absolute chart time + price, and
/// Active Lib converts that path into signed [`EmuTradePoint`] rows.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct EmuPencilPoint {
    /// Absolute chart time.
    pub time: crate::MoonTime,
    /// Drawn chart price.
    pub price: f32,
}

impl EmuPencilPoint {
    pub const fn new(time: crate::MoonTime, price: f32) -> Self {
        Self { time, price }
    }
}

#[repr(C, packed)]
#[derive(Debug, Clone, Copy, FromBytes, IntoBytes, KnownLayout, Immutable, Unaligned)]
struct WireEmuTradePoint {
    time_delta_ms: LeU16,
    price: LeF32,
}

const EMU_TRADE_POINT_SIZE: usize = std::mem::size_of::<WireEmuTradePoint>();
const _: [(); 6] = [(); EMU_TRADE_POINT_SIZE];

impl EmuTradePoint {
    /// Emulated buy tick at `time_delta_ms` after the command base time.
    pub fn buy(time_delta_ms: u16, price: f32) -> Self {
        Self {
            time_delta_ms,
            price: price.abs(),
        }
    }

    /// Emulated sell tick at `time_delta_ms` after the command base time.
    ///
    /// The wire format stores sell-side ticks as a negative `Price`. Keeping
    /// that sign convention inside the constructor lets UI drawing code build
    /// the exact payload without exposing raw bytes.
    pub fn sell(time_delta_ms: u16, price: f32) -> Self {
        Self {
            time_delta_ms,
            price: -price.abs(),
        }
    }

    /// Whether this point represents a sell-side tick.
    pub fn is_sell(self) -> bool {
        self.price.is_sign_negative()
    }

    /// Delta from the command base time in milliseconds.
    pub fn time_delta_ms(self) -> u16 {
        self.time_delta_ms
    }

    /// Absolute trade price independent of side.
    pub fn abs_price(self) -> f32 {
        self.price.abs()
    }

    #[cfg(test)]
    fn from_wire(wire: WireEmuTradePoint) -> Self {
        Self {
            time_delta_ms: wire.time_delta_ms.get(),
            price: wire.price.get(),
        }
    }

    fn to_wire(self) -> WireEmuTradePoint {
        WireEmuTradePoint {
            time_delta_ms: LeU16::new(self.time_delta_ms),
            price: LeF32::new(self.price),
        }
    }

    #[cfg(test)]
    fn from_bytes(data: &[u8]) -> Option<Self> {
        if data.len() < EMU_TRADE_POINT_SIZE {
            return None;
        }
        Some(Self::from_wire(
            WireEmuTradePoint::read_from_bytes(&data[..EMU_TRADE_POINT_SIZE]).ok()?,
        ))
    }

    fn write_to(self, out: &mut Vec<u8>) {
        out.extend_from_slice(self.to_wire().as_bytes());
    }
}

// =============================================================================
//  Subcommand payloads
// =============================================================================

/// User-facing join-sells mode returned by
/// [`ClientSettingsCommand::join_sell_mode`] and accepted by
/// [`ClientSettingsCommand::set_join_sell_mode`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum JoinSellKind {
    None,
    FixedPrice,
    FixedProfit,
    Unknown(u8),
}

impl JoinSellKind {
    #[cfg(any(test, feature = "diagnostics"))]
    #[doc(hidden)]
    pub fn from_byte(value: u8) -> Self {
        Self::from_byte_inner(value)
    }

    #[cfg(not(any(test, feature = "diagnostics")))]
    pub(crate) fn from_byte(value: u8) -> Self {
        Self::from_byte_inner(value)
    }

    fn from_byte_inner(value: u8) -> Self {
        match value {
            0 => Self::None,
            1 => Self::FixedPrice,
            2 => Self::FixedProfit,
            other => Self::Unknown(other),
        }
    }

    #[cfg(any(test, feature = "diagnostics"))]
    #[doc(hidden)]
    pub fn to_byte(self) -> u8 {
        self.to_byte_inner()
    }

    #[cfg(not(any(test, feature = "diagnostics")))]
    pub(crate) fn to_byte(self) -> u8 {
        self.to_byte_inner()
    }

    fn to_byte_inner(self) -> u8 {
        match self {
            Self::None => 0,
            Self::FixedPrice => 1,
            Self::FixedProfit => 2,
            Self::Unknown(value) => value,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::None => "None",
            Self::FixedPrice => "Fixed Price",
            Self::FixedProfit => "Fixed Profit",
            Self::Unknown(_) => "Unknown",
        }
    }
}

/// One `TempBL` row from [`ClientSettingsCommand`].
///
/// TempBL is the core-wide temporary "do not buy" list. While a row is active,
/// matching markets cannot produce new strategy entries; selling and closing
/// existing positions remain allowed.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TempBlacklistEntry<'a> {
    pub symbol: &'a str,
    remaining_days: f64,
}

impl TempBlacklistEntry<'_> {
    /// Remaining blacklist duration as normal Rust time.
    ///
    /// The wire value is a day fraction; terminal code should treat it as a
    /// duration row.
    pub fn remaining_duration(self) -> Duration {
        if !self.remaining_days.is_finite() || self.remaining_days <= 0.0 {
            return Duration::ZERO;
        }
        Duration::from_secs_f64(self.remaining_days * 86_400.0)
    }

    pub fn remaining_hours(self) -> f64 {
        self.remaining_days * 24.0
    }

    pub fn remaining_days(self) -> f64 {
        self.remaining_days
    }
}

/// Compact MoonBot live-settings snapshot retained by Active Lib.
///
/// Many fields are append-only soft-read fields: depending on server version,
/// part of the tail may be absent. Active Lib fills the missing tail from the
/// current retained settings through
/// `UICommand::parse_with_client_settings_fallback`.
///
/// Normal terminal code edits the retained settings snapshot and sends the
/// whole snapshot back through the high-level active client. `Default` is mainly
/// for tests/tools, not for a live configured terminal session.
/// ```ignore
/// if let Some(current) = &snapshot.settings().client_settings {
///     let mut settings = current.clone();
///     settings.set_main_take_profit_percent(3.0);
///     settings.use_g_take_profit = true;
///     settings.g_take_profit = 1.5;
///     client.settings().send(settings);
/// }
/// ```
/// High-level send helpers write a fresh wire UID and use the fixed settings
/// UKey slot for queue deduplication. Terminal UI edits the settings values
/// below; it does not choose the wire UID.
/// The current core forces three startup flags on and remember-state off when
/// applying this snapshot, and does not assign its `sign_orders` field. Use
/// SharedConfig for those saved preferences. See the
/// [settings API guide](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/10-settings-apis.md#compact-settings-and-leverage).
#[derive(Debug, Clone, Default)]
pub struct ClientSettingsCommand {
    /// Wire command UID. Leave it as `0` when using high-level send helpers.
    /// Set it manually only when using `build_client_settings` directly.
    #[cfg(any(test, feature = "diagnostics"))]
    #[doc(hidden)]
    pub uid: u64,
    #[cfg(not(any(test, feature = "diagnostics")))]
    pub(crate) uid: u64,
    // --- always present (v1+) ---
    /// Main sell target in percent before leverage adjustment. Positive values use `x_tmode`'s optional x10 scale,
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
    /// Multiply the main/fixed sell percentage by 10. Does not multiply every numeric setting, and does not scale
    /// `x_sell_scalp` / 50.
    ///
    /// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/03-trading-and-leverage.md#tradingsection-x-t-mode).
    pub x_tmode: bool,
    /// Use `fixed_sell_price` as the global sell percentage instead of the `x_sell` slider/scalp mode. This is a
    /// fixed percentage target, not an absolute exchange price; strategy/per-order overrides remain separate.
    ///
    /// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/03-trading-and-leverage.md#tradingsection-fixed-sell-mode).
    pub fixed_sell_mode: bool,
    /// Sell-target percentage used in `fixed_sell_mode`, multiplied by 10 when `x_tmode` is on. It is not an
    /// absolute quote price. Leverage-aware take-profit mode can further scale the resulting price distance.
    ///
    /// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/03-trading-and-leverage.md#tradingsection-fixed-sell-price).
    pub fixed_sell_price: f64,
    /// Global stop-loss price adjustment in percent, normally negative. Used when an order follows the global stop;
    /// per-order/strategy settings can override it. Long price = entry * (1 + value/100); short price = entry / (1
    /// + value/100), so the same numeric value is not a symmetric market move.
    ///
    /// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/03-trading-and-leverage.md#tradingsection-price-drop-level).
    pub price_drop_level: f32,
    /// Base trailing distance in percent, normally negative, for orders following the global trailing level.
    /// `trailing_float` adjusts the distance as favorable price movement grows.
    ///
    /// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/03-trading-and-leverage.md#tradingsection-trailing-drop).
    pub trailing_drop: f32,
    /// Default trailing-stop enablement for orders using global settings. Strategy and individual-order trailing
    /// settings remain separate.
    ///
    /// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/03-trading-and-leverage.md#tradingsection-trailing-stop).
    pub trailing_stop: bool,
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
    #[cfg(any(test, feature = "diagnostics"))]
    #[doc(hidden)]
    pub unused_spread: i32,
    #[cfg(not(any(test, feature = "diagnostics")))]
    pub(crate) unused_spread: i32,
    /// Default stop-loss enablement for orders using global settings. Does not override every strategy's or already
    /// edited order's individual stop state.
    ///
    /// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/03-trading-and-leverage.md#tradingsection-panic-if-price-drop).
    pub panic_if_price_drop: bool,
    /// Kernel trading-emulator mode; absent from portable SharedConfig.
    /// Changes take effect through the core's normal emulator toggle.
    pub emu_mode: bool,
    // --- v2+ ---
    /// Allow iceberg buy orders where the exchange engine and market support them. Quantity/minimum-notional checks
    /// still apply; this flag does not emulate iceberg orders on unsupported exchanges.
    ///
    /// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/03-trading-and-leverage.md#tradingsection-buy-iceberg).
    pub buy_iceberg: bool,
    /// Allow iceberg sell orders where supported by the exchange engine and market. Does not guarantee an iceberg
    /// for every sell; order quantity/type and market constraints still apply.
    ///
    /// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/03-trading-and-leverage.md#tradingsection-sell-iceberg).
    pub sell_iceberg: bool,
    /// Mark buy orders as belonging to this Moonbot for control-bot coordination; also permits WatchDog keepalives
    /// when a destination is configured. Does not enable exchange API cryptographic signing: authentication remains
    /// independent. The current core does not apply edits to this compact field; use
    /// `SharedConfig.trading.orders_control.sign_orders` or `client.watchdogs()`.
    ///
    /// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/03-trading-and-leverage.md#orderscontrol-sign-orders).
    pub sign_orders: bool,
    // --- always present (v1+) ---
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
    #[cfg(any(test, feature = "diagnostics"))]
    #[doc(hidden)]
    pub temp_bl_symbols: Vec<String>,
    #[cfg(not(any(test, feature = "diagnostics")))]
    pub(crate) temp_bl_symbols: Vec<String>,
    /// `TempBLTimes[i]: TDateTime` delta in days.
    #[cfg(any(test, feature = "diagnostics"))]
    #[doc(hidden)]
    pub temp_bl_times: Vec<f64>,
    #[cfg(not(any(test, feature = "diagnostics")))]
    pub(crate) temp_bl_times: Vec<f64>,
    // --- soft-read tail (optional in older packets) ---
    /// Use the selected Manual strategy for manual order parameters. The strategy must exist; selecting it does not
    /// start automatic strategy execution.
    ///
    /// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/03-trading-and-leverage.md#tradingsection-use-manual-strategy).
    pub use_manual_strategy: bool,
    /// Selected Manual strategy instance ID; SharedConfig stores its name instead.
    /// `build_shared_config()` does not resolve this ID into a name.
    /// [Manual strategy](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/03-trading-and-leverage.md#tradingsection-manual-strategy).
    pub manual_strategy_id: u64,
    /// Use full balance/position amounts in relevant futures sizing checks rather than only currently free funds;
    /// in one-way mode, position-limit checks use the order's direction. Does not detect or close orphaned
    /// positions. Exchange rejection remains possible.
    ///
    /// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/03-trading-and-leverage.md#tradingsection-free-position-check).
    pub free_position_check: bool,
    /// V-Stop price offset in half-percent units: 7 means +3.5% from entry. GUI -20..=20. Defines the price at
    /// which available order-book volume is checked for an enabled per-order V-Stop; it is not the percentage by
    /// which volume must fall.
    ///
    /// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/03-trading-and-leverage.md#tradingsection-vol-drop-level).
    pub vol_drop_level: i32,
    /// Allow supported server-side market-stop handling. The kernel's platform gate admits Binance spot and main
    /// Binance Futures; compatible order/strategy stop conditions are also required. This is not a universal
    /// conversion of all stops on every exchange.
    ///
    /// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/03-trading-and-leverage.md#tradingsection-use-stop-market).
    pub use_stop_market: bool,
    /// AutoStart settings-page blob (104 bytes in the current wire version).
    #[cfg(any(test, feature = "diagnostics"))]
    #[doc(hidden)]
    pub as_cfg: Vec<u8>,
    #[cfg(not(any(test, feature = "diagnostics")))]
    pub(crate) as_cfg: Vec<u8>,
    /// Second AutoStart settings-page blob (168 bytes in the current wire version).
    #[cfg(any(test, feature = "diagnostics"))]
    #[doc(hidden)]
    pub as_cfg2: Vec<u8>,
    #[cfg(not(any(test, feature = "diagnostics")))]
    pub(crate) as_cfg2: Vec<u8>,
    /// HotkeysConfig.SPrice[1..6].
    #[cfg(any(test, feature = "diagnostics"))]
    #[doc(hidden)]
    pub s_price: [f32; 6],
    #[cfg(not(any(test, feature = "diagnostics")))]
    pub(crate) s_price: [f32; 6],
    /// HotkeysConfig.sbNum.
    #[cfg(any(test, feature = "diagnostics"))]
    #[doc(hidden)]
    pub sb_num: u8,
    #[cfg(not(any(test, feature = "diagnostics")))]
    pub(crate) sb_num: u8,
    /// MultiOrders.JoinSellKind (TJoinSellKind: 0=None, 1=FixPrice, 2=FixProfit).
    #[cfg(any(test, feature = "diagnostics"))]
    #[doc(hidden)]
    pub join_sell_kind: u8,
    #[cfg(not(any(test, feature = "diagnostics")))]
    pub(crate) join_sell_kind: u8,
    /// Compact `ArbConfig` form, not a raw settings record.
    pub arb_config: ArbConfigCompact,
    /// Use the main sell controls for manual orders even when a manual strategy is selected.
    pub ignore_strat_sell_price: bool,
    /// Use paid IOC execution for crossing orders on HyperLiquid.
    pub use_hl_fast_ioc: bool,
    /// Pause new grid placements on the core.
    pub grids_paused: bool,
    /// Allow sending debug data to MoonBot servers.
    pub can_collect_user_data: bool,
    /// Allow sending trade results to MoonBot statistics servers.
    pub can_send_trades: bool,
    /// Send trade results anonymously.
    pub sent_trades_anon: bool,
    /// Omit the strategy type from trade reports.
    pub dont_send_strat_info: bool,
    /// Monitor core online status via MBOnlineBot. Requires can_send_trades.
    pub watch_online_status: bool,
}

impl ClientSettingsCommand {
    /// Effective take-profit percentage shown by the main sell control.
    ///
    /// Mirrors terminal state after settings apply: fixed-sell mode uses the
    /// selected fixed-sell preset, normal mode uses `x_sell`, and `x_sell == 0`
    /// falls back to scalp mode (`x_sell_scalp / 50`).
    pub fn effective_take_profit_percent(&self) -> f64 {
        if self.fixed_sell_mode {
            self.selected_fixed_sell_percent()
        } else if self.x_sell > 0 {
            let mut value = f64::from(self.x_sell);
            if self.x_tmode {
                value *= 10.0;
            }
            value.min(900.0)
        } else {
            f64::from(self.x_sell_scalp) / 50.0
        }
    }

    /// Set the normal main take-profit slider and leave scalp/fixed-sell modes.
    ///
    /// The wire stores this as `xSell` plus the shared `xTMode` scale flag. The
    /// terminal-facing helper writes the exact visible percent in the regular
    /// scale (`xTMode=false`), so UI code does not have to remember the storage
    /// trick. Values are rounded and clamped to the visible `1..=900` percent
    /// range.
    pub fn set_main_take_profit_percent(&mut self, percent: f64) {
        let percent = if percent.is_finite() { percent } else { 1.0 };
        let percent = percent.round().clamp(1.0, 900.0) as i32;
        self.fixed_sell_mode = false;
        self.x_tmode = false;
        self.x_sell = percent;
    }

    /// Set the scalp/min-price sell mode percentage.
    ///
    /// The wire selects this mode by storing `xSell=0` and the visible percent
    /// as `xSellScalp / 50`. This helper writes the mode directly from a normal
    /// percent value and keeps the integer storage internal to the settings
    /// snapshot.
    pub fn set_scalp_take_profit_percent(&mut self, percent: f64) {
        let percent = if percent.is_finite() {
            percent.max(0.0)
        } else {
            0.0
        };
        let raw = (percent * 50.0).round().clamp(0.0, i32::MAX as f64) as i32;
        self.fixed_sell_mode = false;
        self.x_sell = 0;
        self.x_sell_scalp = raw;
    }

    /// Visible percentage for a 1-based fixed-sell preset button.
    pub fn fixed_sell_preset_percent(&self, slot_1_based: usize) -> Option<f64> {
        if !(1..=6).contains(&slot_1_based) {
            return None;
        }
        let value = f64::from(self.s_price[slot_1_based - 1]);
        Some(if self.x_tmode { value * 10.0 } else { value })
    }

    /// Fixed-sell presets shown as the six sell-price buttons.
    pub fn fixed_sell_presets(&self) -> &[f32; 6] {
        &self.s_price
    }

    /// Wire-compatible 1-based fixed-sell slot number, clamped to `1..=6`.
    pub fn selected_fixed_sell_slot(&self) -> usize {
        usize::from(self.sb_num.clamp(1, 6))
    }

    /// Select one of the six fixed-sell buttons.
    ///
    /// The slot is clamped to `1..=6` and `fixed_sell_price` is synchronized
    /// from the selected preset.
    pub fn set_selected_fixed_sell_slot(&mut self, slot_1_based: usize) {
        let slot = slot_1_based.clamp(1, 6);
        self.sb_num = slot as u8;
        self.fixed_sell_price = f64::from(self.s_price[slot - 1]);
    }

    /// Current fixed-sell preset value selected by [`Self::selected_fixed_sell_slot`].
    pub fn selected_fixed_sell_price(&self) -> f32 {
        self.s_price[self.selected_fixed_sell_slot() - 1]
    }

    /// Set a fixed-sell preset button value.
    ///
    /// The value is the raw preset value; display helpers apply `x_tmode` when
    /// drawing the visible percentage. If the edited slot is selected,
    /// `fixed_sell_price` is synchronized immediately.
    pub fn set_fixed_sell_preset_price(&mut self, slot_1_based: usize, price: f32) -> bool {
        if !(1..=6).contains(&slot_1_based) {
            return false;
        }
        self.s_price[slot_1_based - 1] = price;
        if self.selected_fixed_sell_slot() == slot_1_based {
            self.fixed_sell_price = f64::from(price);
        }
        true
    }

    /// Set the currently selected fixed-sell preset value.
    pub fn set_selected_fixed_sell_price(&mut self, price: f32) {
        let slot = self.selected_fixed_sell_slot();
        self.s_price[slot - 1] = price;
        self.fixed_sell_price = f64::from(price);
    }

    /// Visible percentage for the currently selected fixed-sell preset.
    pub fn selected_fixed_sell_percent(&self) -> f64 {
        self.fixed_sell_preset_percent(self.selected_fixed_sell_slot())
            .unwrap_or(0.0)
    }

    /// Typed join-sells mode for the multi-order "M" control.
    pub fn join_sell_mode(&self) -> JoinSellKind {
        JoinSellKind::from_byte(self.join_sell_kind)
    }

    /// Set the join-sells mode while preserving the exact wire byte.
    pub fn set_join_sell_mode(&mut self, mode: JoinSellKind) {
        self.join_sell_kind = mode.to_byte();
    }

    /// Temporary blacklist rows as UI entries instead of parallel wire arrays.
    pub fn temp_blacklist_entries(&self) -> impl Iterator<Item = TempBlacklistEntry<'_>> {
        self.temp_bl_symbols
            .iter()
            .zip(self.temp_bl_times.iter())
            .map(|(symbol, remaining_days)| TempBlacklistEntry {
                symbol,
                remaining_days: *remaining_days,
            })
    }

    /// Replace the complete temporary coin-blacklist as one typed UI list.
    ///
    /// The wire stores this as two parallel arrays (`TempBLSymbols` and
    /// `TempBLTimes`). Terminal code should edit rows as `(symbol, remaining
    /// duration)`; the wire arrays are rebuilt here for exact roundtrip.
    /// This is replacement, not append: include every row that must remain.
    pub fn set_temp_blacklist_entries<I, S>(&mut self, entries: I)
    where
        I: IntoIterator<Item = (S, Duration)>,
        S: Into<String>,
    {
        self.temp_bl_symbols.clear();
        self.temp_bl_times.clear();
        for (symbol, remaining) in entries {
            self.temp_bl_symbols.push(symbol.into());
            self.temp_bl_times.push(remaining.as_secs_f64() / 86_400.0);
        }
    }

    #[cfg(any(test, feature = "diagnostics"))]
    #[doc(hidden)]
    pub fn set_temp_blacklist_entries_days<I, S>(&mut self, entries: I)
    where
        I: IntoIterator<Item = (S, f64)>,
        S: Into<String>,
    {
        self.temp_bl_symbols.clear();
        self.temp_bl_times.clear();
        for (symbol, remaining_days) in entries {
            self.temp_bl_symbols.push(symbol.into());
            self.temp_bl_times.push(remaining_days);
        }
    }

    /// Decode the AutoStart settings page from the retained 104-byte blob.
    pub fn auto_start_config(&self) -> AutoStartConfig {
        WireAutoStartConfig::from_blob(&self.as_cfg).to_public()
    }

    /// Replace the AutoStart settings page while keeping the exact wire layout.
    pub fn set_auto_start_config(&mut self, cfg: AutoStartConfig) {
        let mut wire = WireAutoStartConfig::from_blob(&self.as_cfg);
        wire.write_public(&cfg);
        self.as_cfg = wire.as_blob();
    }

    /// Edit AutoStart settings in-place.
    pub fn update_auto_start_config(&mut self, f: impl FnOnce(&mut AutoStartConfig)) {
        let mut cfg = self.auto_start_config();
        f(&mut cfg);
        self.set_auto_start_config(cfg);
    }

    /// Decode the second AutoStart settings page from the retained 168-byte blob.
    pub fn auto_start_config2(&self) -> AutoStartConfig2 {
        WireAutoStartConfig2::from_blob(&self.as_cfg2).to_public()
    }

    /// Replace the second AutoStart settings page. Reserved bytes from the
    /// current blob are preserved.
    pub fn set_auto_start_config2(&mut self, cfg: AutoStartConfig2) {
        let mut wire = WireAutoStartConfig2::from_blob(&self.as_cfg2);
        wire.write_public(&cfg);
        self.as_cfg2 = wire.as_blob();
    }

    /// Edit the second AutoStart settings page in-place while preserving
    /// reserved wire bytes.
    pub fn update_auto_start_config2(&mut self, f: impl FnOnce(&mut AutoStartConfig2)) {
        let mut cfg = self.auto_start_config2();
        f(&mut cfg);
        self.set_auto_start_config2(cfg);
    }
}

/// Start or stop all checked strategies.
#[derive(Debug, Clone, Copy)]
pub struct StratStartStop {
    #[cfg(any(test, feature = "diagnostics"))]
    #[doc(hidden)]
    pub uid: u64,
    #[cfg(not(any(test, feature = "diagnostics")))]
    pub(crate) uid: u64,
    pub is_start: bool,
}

/// Start or stop only the explicit checked-strategy delta set.
#[derive(Debug, Clone)]
pub struct StratStartStopV2 {
    #[cfg(any(test, feature = "diagnostics"))]
    #[doc(hidden)]
    pub uid: u64,
    #[cfg(not(any(test, feature = "diagnostics")))]
    pub(crate) uid: u64,
    pub is_start: bool,
    pub items: Vec<StratCheckedItem>,
}

/// Toggle market-maker/order heatmap subscription.
#[derive(Debug, Clone, Copy)]
pub struct MMOrdersSubscribe {
    #[cfg(any(test, feature = "diagnostics"))]
    #[doc(hidden)]
    pub uid: u64,
    #[cfg(not(any(test, feature = "diagnostics")))]
    pub(crate) uid: u64,
    pub subscribe: bool,
}

/// Request a MoonBot version update.
///
/// This is the MoonBot remote-update command, not a passive client-version
/// notification. `VersionName=""` with `is_release=true` is the normal release
/// update button; a non-empty name targets a test/beta build name.
#[derive(Debug, Clone)]
pub struct UpdateVersion {
    #[cfg(any(test, feature = "diagnostics"))]
    #[doc(hidden)]
    pub uid: u64,
    #[cfg(not(any(test, feature = "diagnostics")))]
    pub(crate) uid: u64,
    pub version_name: String,
    pub is_release: bool,
    /// True for a targeted core download result; never a terminal-update instruction.
    pub only_this_core: bool,
    /// Download/preparation error. Empty means the package is ready, not installed.
    pub error: String,
}

/// Emulated chart ticks for one market.
#[derive(Debug, Clone)]
pub struct EmuTrades {
    #[cfg(any(test, feature = "diagnostics"))]
    #[doc(hidden)]
    pub uid: u64,
    #[cfg(not(any(test, feature = "diagnostics")))]
    pub(crate) uid: u64,
    #[cfg(any(test, feature = "diagnostics"))]
    #[doc(hidden)]
    pub m_index: u16,
    #[cfg(not(any(test, feature = "diagnostics")))]
    pub(crate) m_index: u16,
    /// Wire base time as day-fraction timestamp.
    pub base_time: f64,
    pub points: Vec<EmuTradePoint>,
}

/// Internal listing-refresh trigger after a new market notification.
#[derive(Debug, Clone, Copy)]
pub struct NewMarketNotify {
    #[cfg(any(test, feature = "diagnostics"))]
    #[doc(hidden)]
    pub uid: u64,
    #[cfg(not(any(test, feature = "diagnostics")))]
    pub(crate) uid: u64,
}

/// Leverage-management settings snapshot.
/// Fixed leverage and target-based automatic leverage may coexist on different
/// markets. A one-time market leverage change uses `client.account().set_leverage()`.
/// [Leverage policy](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/03-trading-and-leverage.md#how-automatic-leverage-uses-a-target).
#[derive(Debug, Clone)]
pub struct LevManage {
    #[cfg(any(test, feature = "diagnostics"))]
    #[doc(hidden)]
    pub uid: u64,
    #[cfg(not(any(test, feature = "diagnostics")))]
    pub(crate) uid: u64,
    /// Version byte read from the incoming command. Outgoing builder always
    /// writes the current `LevCmdVer = 1`, regardless of this read-model field.
    #[cfg(any(test, feature = "diagnostics"))]
    #[doc(hidden)]
    pub cmd_ver: u8,
    #[cfg(not(any(test, feature = "diagnostics")))]
    pub(crate) cmd_ver: u8,
    /// Choose leverage from exchange risk brackets so their maximum position size accommodates the amount
    /// configured in `lev_control`. Needs loaded leverage/bracket data and a positive effective target for the
    /// market; unsupported for inverse-base-currency mode. Lowering is allowed; raising additionally requires
    /// `auto_lev_up`. This is not sizing an order from free balance.
    ///
    /// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/03-trading-and-leverage.md#automanagelevconfig-auto-max-order).
    pub auto_max_order: bool,
    /// Allow `auto_max_order` to increase leverage as well as decrease it. Has no independent effect when
    /// `auto_max_order` is off or the market has no positive target amount.
    ///
    /// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/03-trading-and-leverage.md#automanagelevconfig-auto-lev-up).
    pub auto_lev_up: bool,
    /// Automatically request isolated futures margin. Exchange support and account/position restrictions still
    /// apply. Mutually exclusive with `auto_cross` in the GUI; if both are received as true, isolated has priority.
    /// Uses account-wide mode on engines with a global margin-mode setting, otherwise per-market mode.
    ///
    /// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/03-trading-and-leverage.md#automanagelevconfig-auto-isolated).
    pub auto_isolated: bool,
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
    /// Requested fixed leverage multiplier, GUI integer 1..=500; each market is limited by its exchange maximum.
    /// Used by `auto_fix_lev`, including new-listing initialization. It may coexist with `auto_max_order` for
    /// markets without an effective target amount.
    ///
    /// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/03-trading-and-leverage.md#automanagelevconfig-fix-lev).
    pub fix_lev: i32,
    /// Report successful automatic leverage changes to Telegram. Does not control leverage selection itself or
    /// exchange-request logging.
    ///
    /// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/03-trading-and-leverage.md#automanagelevconfig-tlg-report).
    pub tlg_report: bool,
    /// Target position-limit amounts for automatic leverage selection, not an enable flag. Example: `1k def 5k
    /// BTC*` sets a default target of 1000 and 5000 for matching tickers; K/M suffixes and wildcard ticker masks
    /// are supported. Empty text clears targets. Used by `auto_max_order`; `auto_lev_up` only permits upward
    /// adjustments within that mode.
    ///
    /// [MoonDoc reference](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/03-trading-and-leverage.md#tradingsection-auto-lev-control).
    pub lev_control: String,
}

impl LevManage {
    /// Global `def` fallback from the leverage-control text.
    ///
    /// MoonBot keeps this as `cfg.AutoLevControlOther`. Per-market values are
    /// applied to retained markets by Active Lib; this helper exposes the
    /// fallback separately because the markets-table `MaxPos` column itself
    /// keeps `0` for markets that only use `def`.
    pub fn default_max_pos_limit(&self) -> i32 {
        parse_lev_control(&self.lev_control).default_max_pos_limit
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct LevControlRule {
    pub(crate) limit: i32,
    pub(crate) token: String,
    pub(crate) wildcard: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct LevControlParsed {
    pub(crate) default_max_pos_limit: i32,
    pub(crate) rules: Vec<LevControlRule>,
}

pub(crate) fn parse_lev_control(text: &str) -> LevControlParsed {
    let normalized = text.replace([',', '.'], " ");
    let mut parsed = LevControlParsed::default();
    let mut scan_tokens = false;
    let mut current_limit = 0;

    for token in normalized.split_whitespace() {
        if let Some(limit) = parse_int_km(token) {
            current_limit = limit;
            scan_tokens = true;
            continue;
        }
        if !scan_tokens {
            continue;
        }
        if token.eq_ignore_ascii_case("def") {
            parsed.default_max_pos_limit = current_limit;
            continue;
        }
        parsed.rules.push(LevControlRule {
            limit: current_limit,
            token: token.trim().to_string(),
            wildcard: token.contains('*') || token.contains('?'),
        });
    }

    parsed
}

fn parse_int_km(token: &str) -> Option<i32> {
    let token = token.trim();
    if token.is_empty() || !token.as_bytes()[0].is_ascii_digit() {
        return None;
    }
    let (number, mult) =
        if let Some(number) = token.strip_suffix('k').or_else(|| token.strip_suffix('K')) {
            (number, 1_000_i32)
        } else if let Some(number) = token.strip_suffix('m').or_else(|| token.strip_suffix('M')) {
            (number, 1_000_000_i32)
        } else {
            (token, 1_i32)
        };
    let value = number.parse::<i32>().ok()?;
    value.checked_mul(mult)
}

pub(crate) fn lev_control_wildcard_match(subject: &str, pattern: &str) -> bool {
    wildcard_match_ascii_ci(subject.as_bytes(), pattern.as_bytes())
}

fn wildcard_match_ascii_ci(subject: &[u8], pattern: &[u8]) -> bool {
    let mut si = 0usize;
    let mut pi = 0usize;
    let mut star_pi: Option<usize> = None;
    let mut star_si = 0usize;

    while si < subject.len() {
        if pi < pattern.len()
            && (pattern[pi] == b'?' || pattern[pi].eq_ignore_ascii_case(&subject[si]))
        {
            si += 1;
            pi += 1;
        } else if pi < pattern.len() && pattern[pi] == b'*' {
            star_pi = Some(pi);
            pi += 1;
            star_si = si;
        } else if let Some(star) = star_pi {
            pi = star + 1;
            star_si += 1;
            si = star_si;
        } else {
            return false;
        }
    }

    while pi < pattern.len() && pattern[pi] == b'*' {
        pi += 1;
    }
    pi == pattern.len()
}

/// CmdId=10 `TTriggerManageCommand`.
#[derive(Debug, Clone)]
pub struct TriggerManage {
    #[cfg(any(test, feature = "diagnostics"))]
    #[doc(hidden)]
    pub uid: u64,
    #[cfg(not(any(test, feature = "diagnostics")))]
    pub(crate) uid: u64,
    /// 0 = Clear, 1 = Set.
    #[cfg(any(test, feature = "diagnostics"))]
    #[doc(hidden)]
    pub action: u8,
    #[cfg(not(any(test, feature = "diagnostics")))]
    pub(crate) action: u8,
    pub all_markets: bool,
    #[cfg(any(test, feature = "diagnostics"))]
    #[doc(hidden)]
    pub markets: Vec<u16>,
    #[cfg(not(any(test, feature = "diagnostics")))]
    pub(crate) markets: Vec<u16>,
    pub keys: Vec<u16>,
}

/// CmdId=11 `TResetProfitCommand`.
#[derive(Debug, Clone, Copy)]
pub struct ResetProfit {
    #[cfg(any(test, feature = "diagnostics"))]
    #[doc(hidden)]
    pub uid: u64,
    pub kind: ResetProfitKind,
}

/// CmdId=24 `TProfitStateCommand`.
///
/// This mirrors the current report/profit counters shown by MoonBot settings
/// UI. It is state from the report DB layer, not account balance and not an
/// order/PnL stream.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct ProfitStateCommand {
    #[cfg(any(test, feature = "diagnostics"))]
    #[doc(hidden)]
    pub uid: u64,
    pub rep_total_profit: f64,
    pub rep_total_trades: i32,
    pub rep_trades_total: f64,
    pub rep_count_trades: i32,
}

/// CmdId=30 `THLRequestLimitStateCommand`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct HyperliquidRequestLimitStateCommand {
    #[cfg(any(test, feature = "diagnostics"))]
    #[doc(hidden)]
    pub uid: u64,
    /// Remaining address-level HyperLiquid action requests. `None` is the
    /// wire sentinel used before the core has a value and on non-HyperLiquid cores.
    pub requests_left: Option<u64>,
}

/// CmdId=25 `TAutoDetectCommand`.
///
/// This is the explicit terminal action behind the AutoDetect/passive-mode
/// button. The server either toggles passive mode or, if it is already in the
/// requested state, sends a fresh `TRuntimeStateCommand`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AutoDetectCommand {
    #[cfg(any(test, feature = "diagnostics"))]
    #[doc(hidden)]
    pub uid: u64,
    #[cfg(not(any(test, feature = "diagnostics")))]
    pub(crate) uid: u64,
    pub active: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct NewsRelayCommand {
    pub(crate) kind: u8,
    pub(crate) data: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct NewsHistoryCommand {
    pub(crate) frames: Vec<Vec<u8>>,
    pub(crate) tags: Vec<u8>,
}

/// CmdId=28 `TSharedConfigCommand` — gzip-compressed safe-share payload
/// (see [`crate::shared_config`]). Kernel -> terminal carries the kernel's
/// current config (request reply / post-apply re-broadcast); terminal ->
/// kernel means "apply this config".
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct SharedConfigCommand {
    pub(crate) data: Vec<u8>,
}

/// Trigger-management action used by
/// [`crate::MoonSettings::set_triggers_for_markets`],
/// [`crate::MoonSettings::clear_triggers_for_markets`], and the all-market
/// trigger helpers.
///
/// Maps the trigger-management action byte.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TriggerAction {
    /// Clear the listed triggers (`Action = 0`).
    Clear,
    /// Set/arm the listed triggers (`Action = 1`).
    Set,
    /// Future/unknown action byte preserved for diagnostics/roundtrip.
    Unknown(u8),
}

impl TriggerAction {
    #[cfg(any(test, feature = "diagnostics"))]
    #[doc(hidden)]
    pub const fn from_byte(value: u8) -> Self {
        Self::from_byte_inner(value)
    }

    #[cfg(not(any(test, feature = "diagnostics")))]
    pub(crate) const fn from_byte(value: u8) -> Self {
        Self::from_byte_inner(value)
    }

    const fn from_byte_inner(value: u8) -> Self {
        match value {
            0 => Self::Clear,
            1 => Self::Set,
            other => Self::Unknown(other),
        }
    }

    /// Wire ordinal: `Clear = 0`, `Set = 1`.
    #[cfg(any(test, feature = "diagnostics"))]
    #[doc(hidden)]
    pub const fn to_byte(self) -> u8 {
        self.to_byte_inner()
    }

    #[cfg(not(any(test, feature = "diagnostics")))]
    pub(crate) const fn to_byte(self) -> u8 {
        self.to_byte_inner()
    }

    const fn to_byte_inner(self) -> u8 {
        match self {
            Self::Clear => 0,
            Self::Set => 1,
            Self::Unknown(value) => value,
        }
    }
}

/// Which profit counter to reset, for [`crate::MoonSettings::reset_profit`].
///
/// Maps the reset-profit kind byte.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResetProfitKind {
    /// Reset only the current-session profit (`ResetKind = 0`).
    CurrentProfit,
    /// Reset the all-time accumulated profit (`ResetKind = 1`).
    AllProfit,
    /// Future/unknown reset kind preserved from the wire byte.
    Unknown(u8),
}

impl ResetProfitKind {
    #[cfg(any(test, feature = "diagnostics"))]
    #[doc(hidden)]
    pub const fn from_byte(value: u8) -> Self {
        Self::from_byte_inner(value)
    }

    #[cfg(not(any(test, feature = "diagnostics")))]
    pub(crate) const fn from_byte(value: u8) -> Self {
        Self::from_byte_inner(value)
    }

    const fn from_byte_inner(value: u8) -> Self {
        match value {
            0 => Self::CurrentProfit,
            1 => Self::AllProfit,
            other => Self::Unknown(other),
        }
    }

    /// Wire ordinal: `CurrentProfit = 0`, `AllProfit = 1`.
    #[cfg(any(test, feature = "diagnostics"))]
    #[doc(hidden)]
    pub const fn to_byte(self) -> u8 {
        self.to_byte_inner()
    }

    #[cfg(not(any(test, feature = "diagnostics")))]
    pub(crate) const fn to_byte(self) -> u8 {
        self.to_byte_inner()
    }

    const fn to_byte_inner(self) -> u8 {
        match self {
            Self::CurrentProfit => 0,
            Self::AllProfit => 1,
            Self::Unknown(value) => value,
        }
    }
}

/// CmdId=12 `TArbActivateNotify`.
#[derive(Debug, Clone, Copy)]
pub struct ArbActivateNotify {
    #[cfg(any(test, feature = "diagnostics"))]
    #[doc(hidden)]
    pub uid: u64,
    #[cfg(not(any(test, feature = "diagnostics")))]
    pub(crate) uid: u64,
    /// `ArbValid: TDateTime`.
    pub arb_valid: f64,
}

/// CmdId=13 `TSwitchDexCommand` (High, UK_DexSwitch).
/// `DexName: ShortString[15]` = 16 wire bytes: one length byte plus up to
/// 15 ASCII bytes.
#[derive(Debug, Clone)]
pub struct SwitchDex {
    #[cfg(any(test, feature = "diagnostics"))]
    #[doc(hidden)]
    pub uid: u64,
    #[cfg(not(any(test, feature = "diagnostics")))]
    pub(crate) uid: u64,
    pub dex_name: String,
}

/// Spot-market switch target.
///
/// Server accepts `0=Crypto` and `1=Predict`; raw future values are preserved
/// for forward compatibility, but normal application code should use the named
/// constants.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct SpotMarketKind(u8);

#[allow(non_upper_case_globals)]
impl SpotMarketKind {
    pub const Crypto: Self = Self(0);
    pub const Predict: Self = Self(1);

    #[cfg(any(test, feature = "diagnostics"))]
    #[doc(hidden)]
    pub const fn from_byte(value: u8) -> Self {
        Self(value)
    }

    #[cfg(not(any(test, feature = "diagnostics")))]
    pub(crate) const fn from_byte(value: u8) -> Self {
        Self(value)
    }

    #[cfg(any(test, feature = "diagnostics"))]
    #[doc(hidden)]
    pub const fn to_byte(self) -> u8 {
        self.0
    }

    #[cfg(not(any(test, feature = "diagnostics")))]
    pub(crate) const fn to_byte(self) -> u8 {
        self.0
    }

    pub const fn is_known(self) -> bool {
        self.0 <= Self::Predict.0
    }

    pub const fn name(self) -> &'static str {
        match self {
            Self::Crypto => "Crypto",
            Self::Predict => "Predict",
            _ => "Unknown",
        }
    }
}

impl std::fmt::Debug for SpotMarketKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if self.is_known() {
            f.write_str(self.name())
        } else {
            write!(f, "Unknown({})", self.0)
        }
    }
}

/// CmdId=14 `TSwitchSpotCommand` (High, UK_SpotSwitch).
#[derive(Debug, Clone, Copy)]
pub struct SwitchSpot {
    #[cfg(any(test, feature = "diagnostics"))]
    #[doc(hidden)]
    pub uid: u64,
    #[cfg(not(any(test, feature = "diagnostics")))]
    pub(crate) uid: u64,
    pub spot_index: SpotMarketKind,
}

/// `TAlertObjectCommand` (UI CmdId=15).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AlertObjectCommand {
    #[cfg(any(test, feature = "diagnostics"))]
    #[doc(hidden)]
    pub uid: u64,
    #[cfg(not(any(test, feature = "diagnostics")))]
    pub(crate) uid: u64,
    pub market_name: String,
    pub obj_uid: u64,
    pub upsert: bool,
    /// `TChartObject.Save` blob. Empty for delete.
    pub blob: Vec<u8>,
    pub(crate) skipped: bool,
}

impl AlertObjectCommand {
    pub fn new_upsert(market_name: impl Into<String>, obj_uid: u64, blob: Vec<u8>) -> Self {
        Self {
            uid: rand::random(),
            market_name: market_name.into(),
            obj_uid,
            upsert: true,
            blob,
            skipped: false,
        }
    }

    pub fn new_delete(market_name: impl Into<String>, obj_uid: u64) -> Self {
        Self {
            uid: rand::random(),
            market_name: market_name.into(),
            obj_uid,
            upsert: false,
            blob: Vec::new(),
            skipped: false,
        }
    }

    pub(crate) fn skipped(&self) -> bool {
        self.skipped
    }
}

/// `TChartTextStateCommand` (UI CmdId=17).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChartTextStateCommand {
    #[cfg(any(test, feature = "diagnostics"))]
    #[doc(hidden)]
    pub uid: u64,
    #[cfg(not(any(test, feature = "diagnostics")))]
    pub(crate) uid: u64,
    pub market_name: String,
    pub need_filters: bool,
    pub need_debug_lines: bool,
}

impl ChartTextStateCommand {
    pub fn new(market_name: impl Into<String>, need_filters: bool, need_debug_lines: bool) -> Self {
        Self {
            uid: rand::random(),
            market_name: market_name.into(),
            need_filters,
            need_debug_lines,
        }
    }
}

/// `TChartTextSnapshotCommand` (UI CmdId=18).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChartTextSnapshotCommand {
    #[cfg(any(test, feature = "diagnostics"))]
    #[doc(hidden)]
    pub uid: u64,
    #[cfg(not(any(test, feature = "diagnostics")))]
    pub(crate) uid: u64,
    pub market_name: String,
    pub filter_lines: Vec<String>,
    pub debug_lines: Vec<String>,
}

/// `TOrdersHistoryRequestCommand` (UI CmdId=19).
///
/// Client asks the server-side MoonBot terminal to execute its existing orders
/// history export/update flow for one market. The response is not a paired
/// MoonProto payload; this is a fire-and-forget UI command like the terminal
/// button path.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OrdersHistoryRequest {
    #[cfg(any(test, feature = "diagnostics"))]
    #[doc(hidden)]
    pub uid: u64,
    #[cfg(not(any(test, feature = "diagnostics")))]
    pub(crate) uid: u64,
    pub market_name: String,
}

/// `TRuntimeStateCommand` (UI CmdId=20).
///
/// This is the server's current terminal runtime state: whether the market
/// runtime is started and whether automatic detection is active. It is broader
/// than `TStratRuntimeState`: strategies may be running/stopped independently
/// from the core market/passive-mode state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RuntimeStateCommand {
    #[cfg(any(test, feature = "diagnostics"))]
    #[doc(hidden)]
    pub uid: u64,
    #[cfg(not(any(test, feature = "diagnostics")))]
    pub(crate) uid: u64,
    /// MoonBot `MarketActive`: the market runtime is started.
    pub is_started: bool,
    /// MoonBot `not PassiveMode`: automatic market detection is active.
    pub auto_detect_active: bool,
    /// Release package prepared on the core, encoded as version * 100 (773 = 7.73).
    /// `Some(0)` means no prepared package; `None` means an older core did not
    /// report this field. This does not check the release server or install updates.
    pub prepared_release_version: Option<u32>,
}

/// `TKernelLicenseStateCommand` (UI CmdId=22).
///
/// Latest license/module/MoonCredits state sent by the MoonBot core after
/// connect and on explicit refresh. Wire `TDateTime` values are converted at
/// the protocol boundary; UI code receives normal [`MoonTime`] values.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct KernelLicenseStateCommand {
    #[cfg(any(test, feature = "diagnostics"))]
    #[doc(hidden)]
    pub uid: u64,
    #[cfg(not(any(test, feature = "diagnostics")))]
    pub(crate) uid: u64,
    pub paid_version: bool,
    pub reg_id: i32,
    pub order_count: i32,
    pub use_moon_strike: bool,
    pub use_load_charts: bool,
    pub use_web_hook: bool,
    pub use_moon_streamer: bool,
    pub use_algo_mod: bool,
    pub use_ref_mod: bool,
    pub use_back_mod: bool,
    pub news_valid_until: Option<MoonTime>,
    pub news_trial_used: bool,
    pub arb_active: bool,
    pub arb_valid_until: Option<MoonTime>,
    pub moon_credits: i32,
    pub moon_credits_hold: i32,
    pub moon_credits_auction: i32,
    pub can_use_watcher: bool,
}

// =============================================================================
//  UICommand enum
// =============================================================================

#[derive(Debug, Clone)]
pub enum UICommand {
    TelegramState(std::sync::Arc<crate::state::TelegramState>),
    WatchDogState(std::sync::Arc<crate::state::WatchDogState>),
    ServiceSettingsResult(std::sync::Arc<crate::state::ServiceSettingsResult>),
    /// Full client settings snapshot. Boxed to keep the common `UICommand`
    /// envelope small when it is moved through event queues.
    ClientSettings(Box<ClientSettingsCommand>),
    SettingsRequest {
        uid: u64,
    },
    StratStartStop(StratStartStop),
    StratStartStopV2(StratStartStopV2),
    MMOrdersSubscribe(MMOrdersSubscribe),
    UpdateVersion(UpdateVersion),
    EmuTrades(EmuTrades),
    NewMarketNotify(NewMarketNotify),
    LevManage(LevManage),
    TriggerManage(TriggerManage),
    ResetProfit(ResetProfit),
    ArbActivateNotify(ArbActivateNotify),
    SwitchDex(SwitchDex),
    SwitchSpot(SwitchSpot),
    AlertObject(AlertObjectCommand),
    AlertSnapshotRequest {
        uid: u64,
    },
    ChartTextState(ChartTextStateCommand),
    ChartTextSnapshot(ChartTextSnapshotCommand),
    OrdersHistoryRequest(OrdersHistoryRequest),
    RuntimeState(RuntimeStateCommand),
    RestartNow {
        uid: u64,
    },
    KernelLicenseState(KernelLicenseStateCommand),
    KernelLicenseStateRequest {
        uid: u64,
        activate_feature: i32,
    },
    ProfitState(ProfitStateCommand),
    AutoDetect(AutoDetectCommand),
    NewsRelay(NewsRelayCommand),
    NewsHistory(NewsHistoryCommand),
    SharedConfig(SharedConfigCommand),
    HyperliquidRequestLimitState(HyperliquidRequestLimitStateCommand),
    ProblemsState(Vec<std::sync::Arc<crate::state::KernelProblem>>),
    ProblemNotify(std::sync::Arc<crate::state::KernelProblem>),
    ProblemsClear,
    ProblemsTest(String),
    Shutdown {
        uid: u64,
    },
    /// Command header is well-formed, but the command version is newer than
    /// this library can parse. The command is skipped without state changes.
    Skipped {
        cmd_id: u8,
        uid: u64,
        ver: u16,
    },
    Unknown {
        cmd_id: u8,
        uid: u64,
    },
}

// =============================================================================
//  Builders (C → S)
// =============================================================================

pub(crate) fn write_header(out: &mut Vec<u8>, cmd_id: u8, uid: u64) {
    out.push(cmd_id);
    out.extend_from_slice(&CURRENT_PROTO_CMD_VER.to_le_bytes());
    out.extend_from_slice(&uid.to_le_bytes());
}

fn copy_blob_prefix<const N: usize>(dst: &mut [u8; N], blob: &[u8]) {
    let copy = blob.len().min(N);
    dst[..copy].copy_from_slice(&blob[..copy]);
}

// =============================================================================
//  Tests
// =============================================================================

#[cfg(test)]
mod tests;
#[cfg(test)]
mod core_management_tests;
