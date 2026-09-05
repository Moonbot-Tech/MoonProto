//! Settings sync state — latest UI/settings snapshots received from the server.
//!
//! The state layer keeps the latest snapshot for each supported subcommand;
//! applying those settings to an application UI/engine is the consumer's
//! responsibility.
//!
//! ## Tracked State
//! - `ClientSettings`: full UI settings snapshot.
//! - `LevManage`: leverage-management settings snapshot.
//! - `RuntimeState`: started/passive-mode state of the MoonBot core.
//! - `KernelLicenseState`: license/module/MoonCredits state.
//! - `ProfitState`: report/profit counters shown by MoonBot settings UI.
//! - HyperLiquid request limit: remaining address-level action requests.
//! - `ArbActivateNotify`: arbitrage-valid-until timestamp.
//!
//! Client->server action commands (`SettingsRequest`, `StratStartStop`,
//! `MMOrdersSubscribe`, `EmuTrades`, `TriggerManage`, `ResetProfit`,
//! `SwitchDex`, `SwitchSpot`, `RestartNow`, `KernelLicenseStateRequest`) are
//! sent through high-level handles and ignored if they ever arrive inbound.
//! `NewMarketNotify` is an internal Active Lib trigger: the dispatcher uses it
//! to force listing refresh, and user code receives a market event only after
//! the refreshed list actually inserts new markets.

use crate::commands::ui::{
    ClientSettingsCommand, KernelLicenseStateCommand, LevManage, ProfitStateCommand,
    RuntimeStateCommand, UICommand,
};
use crate::time::MoonTime;

/// Synchronized UI/settings state updated from inbound UI settings packets.
///
/// Settings are snapshot state, not accumulated history. Every accepted
/// full settings snapshot fully replaces `client_settings`.
#[derive(Debug, Clone, Default)]
pub struct SettingsState {
    /// Last received client settings snapshot.
    pub client_settings: Option<ClientSettingsCommand>,
    /// Current settings fallback for append-only packet tails.
    ///
    /// Old packets may omit append-only tail fields; those fields are filled
    /// from the current retained settings. After every full settings snapshot
    /// this fallback is refreshed automatically; before the first snapshot,
    /// low-level dispatcher tests/tools may seed it through the hidden fallback
    /// helper.
    client_settings_fallback: ClientSettingsCommand,
    /// Current leverage-management settings, if received.
    pub lev_manage: Option<LevManage>,
    /// Kernel's last safe-share config snapshot, if received.
    pub shared_config: Option<crate::shared_config::SharedConfig>,
    update_revision: u64,
    client_settings_revision: u64,
    lev_manage_revision: u64,
    shared_config_revision: u64,
    /// Current market-runtime/passive-mode state, if received.
    pub runtime_state: Option<RuntimeStateCommand>,
    /// Current license/module/MoonCredits state, if received.
    pub kernel_license_state: Option<KernelLicenseStateCommand>,
    /// Current report/profit counters, if received.
    pub profit_state: Option<ProfitStateCommand>,
    /// Remaining address-level HyperLiquid action requests.
    ///
    /// `None` means the core has not published a value yet or the connected
    /// core is not HyperLiquid.
    pub hyperliquid_requests_left: Option<u64>,
    /// Raw `TDateTime` days for diagnostics/parity tests.
    ///
    /// Normal terminal code should use [`Self::arb_valid_until_time`] and
    /// [`Self::arb_is_active_now`] instead of carrying wire day doubles.
    #[cfg(any(test, feature = "diagnostics"))]
    #[doc(hidden)]
    pub arb_valid_until: Option<f64>,
    #[cfg(not(any(test, feature = "diagnostics")))]
    pub(crate) arb_valid_until: Option<f64>,
}

#[derive(Debug, Clone)]
pub enum SettingsEvent {
    /// A fresh full settings snapshot was applied.
    ClientSettingsUpdated,
    /// Leverage-management snapshot changed.
    LevManageUpdated,
    /// Kernel safe-share config snapshot updated (see
    /// `snapshot().settings().shared_config`).
    SharedConfigUpdated,
    /// MoonBot core runtime/passive-mode state changed.
    RuntimeStateUpdated,
    /// License/module/MoonCredits state changed.
    KernelLicenseStateUpdated,
    /// Report/profit counters changed.
    ProfitStateUpdated,
    /// Remaining HyperLiquid address-level action requests changed.
    HyperliquidRequestLimitUpdated,
    /// Remote update command: version name + release/test flag.
    ///
    /// Terminal clients treat this as a request to run their local updater. The
    /// state layer only surfaces the wire command; application code decides
    /// whether/how to update itself.
    VersionUpdate {
        #[cfg(any(test, feature = "diagnostics"))]
        #[doc(hidden)]
        uid: u64,
        version_name: String,
        is_release: bool,
    },
    /// Arbitrage license was activated/refreshed.
    ArbActivated {
        #[cfg(any(test, feature = "diagnostics"))]
        #[doc(hidden)]
        uid: u64,
        arb_valid: MoonTime,
    },
    /// Command from a future protocol version. Low-level diagnostics can surface
    /// it, while `EventDispatcher` skips it without state changes.
    #[cfg(any(test, feature = "diagnostics"))]
    Skipped { cmd_id: u8, uid: u64, ver: u16 },
    /// Unknown subcommand for forward compatibility.
    #[cfg(any(test, feature = "diagnostics"))]
    Unknown { cmd_id: u8, uid: u64 },
}

impl SettingsState {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn arb_valid_until_time(&self) -> Option<MoonTime> {
        self.arb_valid_until.and_then(MoonTime::from_delphi_days)
    }

    /// Whether the retained arb-valid-until timestamp is still in the future.
    pub fn arb_is_active_now(&self) -> bool {
        self.arb_is_active_at(MoonTime::now())
    }

    /// Whether the retained arb-valid-until timestamp is later than `now`.
    pub fn arb_is_active_at(&self, now: MoonTime) -> bool {
        self.arb_valid_until_time()
            .is_some_and(|valid_until| valid_until > now)
    }

    /// Seed settings fallback used while parsing old settings packets with
    /// missing append-only tail fields.
    #[doc(hidden)]
    #[cfg(test)]
    pub(crate) fn set_client_settings_fallback(&mut self, fallback: ClientSettingsCommand) {
        self.client_settings_fallback = fallback;
    }

    pub(crate) fn client_settings_parse_fallback(&self) -> &ClientSettingsCommand {
        &self.client_settings_fallback
    }

    fn next_update_revision(&mut self) -> u64 {
        self.update_revision = self.update_revision.wrapping_add(1).max(1);
        self.update_revision
    }

    pub(crate) fn shared_config_revision(&self) -> u64 {
        self.shared_config_revision
    }

    pub(crate) fn build_shared_config(&self) -> Option<crate::shared_config::SharedConfig> {
        let mut cfg = self.shared_config.clone()?;
        if self.client_settings_revision > self.shared_config_revision {
            if let Some(settings) = &self.client_settings {
                cfg.absorb_client_settings_raw(
                    settings.x_sell,
                    settings.x_sell_scalp,
                    settings.x_tmode,
                    settings.fixed_sell_mode,
                    settings.fixed_sell_price,
                    settings.price_drop_level,
                    settings.trailing_drop,
                    settings.trailing_stop,
                    settings.g_take_profit,
                    settings.use_g_take_profit,
                    settings.panic_if_price_drop,
                    settings.buy_iceberg,
                    settings.sell_iceberg,
                    settings.sign_orders,
                    &settings.coins_black_list_text,
                    settings.use_coins_black_list,
                    settings.use_manual_strategy,
                    settings.free_position_check,
                    settings.vol_drop_level,
                    settings.use_stop_market,
                    &settings.s_price,
                    settings.sb_num,
                    settings.join_sell_kind,
                );
            }
        }
        if self.lev_manage_revision > self.shared_config_revision {
            if let Some(lev) = &self.lev_manage {
                cfg.absorb_lev_manage_raw(
                    lev.auto_max_order,
                    lev.auto_lev_up,
                    lev.auto_isolated,
                    lev.auto_cross,
                    lev.auto_fix_lev,
                    lev.fix_lev,
                    lev.tlg_report,
                    &lev.lev_control,
                );
            }
        }
        Some(cfg)
    }

    /// Apply an inbound UI command to retained state.
    ///
    /// Returns `None` for internal commands that have no public settings event.
    pub(crate) fn apply(&mut self, cmd: UICommand) -> Option<SettingsEvent> {
        match cmd {
            UICommand::ClientSettings(c) => {
                let settings = *c;
                self.client_settings_fallback = settings.clone();
                self.client_settings = Some(settings);
                self.client_settings_revision = self.next_update_revision();
                Some(SettingsEvent::ClientSettingsUpdated)
            }
            UICommand::SharedConfig(c) => {
                match crate::shared_config::gzip_decompress(&c.data)
                    .and_then(|payload| crate::shared_config::parse_payload(&payload))
                {
                    Ok(cfg) => {
                        self.shared_config = Some(cfg);
                        self.shared_config_revision = self.next_update_revision();
                        Some(SettingsEvent::SharedConfigUpdated)
                    }
                    Err(err) => {
                        log::warn!(
                            target: "moonproto::shared_config",
                            "rejected invalid shared-config snapshot: {err}"
                        );
                        None
                    }
                }
            }

            UICommand::SettingsRequest { .. }
            | UICommand::StratStartStop(_)
            | UICommand::StratStartStopV2(_)
            | UICommand::MMOrdersSubscribe(_)
            | UICommand::EmuTrades(_)
            | UICommand::TriggerManage(_)
            | UICommand::ResetProfit(_)
            | UICommand::SwitchDex(_)
            | UICommand::SwitchSpot(_)
            | UICommand::AlertObject(_)
            | UICommand::AlertSnapshotRequest { .. }
            | UICommand::ChartTextState(_)
            | UICommand::ChartTextSnapshot(_)
            | UICommand::OrdersHistoryRequest(_)
            | UICommand::RestartNow { .. }
            | UICommand::KernelLicenseStateRequest { .. }
            | UICommand::AutoDetect(_)
            | UICommand::NewsRelay(_)
            | UICommand::NewsHistory(_)
            | UICommand::Shutdown { .. } => None,

            UICommand::UpdateVersion(u) => Some(SettingsEvent::VersionUpdate {
                #[cfg(any(test, feature = "diagnostics"))]
                uid: u.uid,
                version_name: u.version_name,
                is_release: u.is_release,
            }),

            UICommand::NewMarketNotify(_) => None,

            UICommand::LevManage(l) => {
                self.lev_manage = Some(l);
                self.lev_manage_revision = self.next_update_revision();
                Some(SettingsEvent::LevManageUpdated)
            }

            UICommand::RuntimeState(s) => {
                self.runtime_state = Some(s);
                Some(SettingsEvent::RuntimeStateUpdated)
            }

            UICommand::KernelLicenseState(s) => {
                self.kernel_license_state = Some(s);
                Some(SettingsEvent::KernelLicenseStateUpdated)
            }

            UICommand::ProfitState(s) => {
                self.profit_state = Some(s);
                Some(SettingsEvent::ProfitStateUpdated)
            }

            UICommand::HyperliquidRequestLimitState(s) => {
                self.hyperliquid_requests_left = s.requests_left;
                Some(SettingsEvent::HyperliquidRequestLimitUpdated)
            }

            UICommand::ArbActivateNotify(a) => {
                self.arb_valid_until = Some(a.arb_valid);
                Some(SettingsEvent::ArbActivated {
                    #[cfg(any(test, feature = "diagnostics"))]
                    uid: a.uid,
                    arb_valid: MoonTime::from_delphi_days(a.arb_valid).unwrap_or(MoonTime::ZERO),
                })
            }

            UICommand::Skipped { cmd_id, uid, ver } => {
                #[cfg(any(test, feature = "diagnostics"))]
                {
                    Some(SettingsEvent::Skipped { cmd_id, uid, ver })
                }
                #[cfg(not(any(test, feature = "diagnostics")))]
                {
                    let _ = (cmd_id, uid, ver);
                    None
                }
            }

            UICommand::Unknown { cmd_id, uid } => {
                #[cfg(any(test, feature = "diagnostics"))]
                {
                    Some(SettingsEvent::Unknown { cmd_id, uid })
                }
                #[cfg(not(any(test, feature = "diagnostics")))]
                {
                    let _ = (cmd_id, uid);
                    None
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands::ui::*;

    fn apply_shared_config(st: &mut SettingsState, cfg: &crate::shared_config::SharedConfig) {
        let payload = crate::shared_config::serialize_payload(cfg).unwrap();
        let data = crate::shared_config::gzip_compress(&payload).unwrap();
        let event = st.apply(UICommand::SharedConfig(SharedConfigCommand { data }));
        assert!(matches!(event, Some(SettingsEvent::SharedConfigUpdated)));
    }

    #[test]
    fn client_settings_stores_snapshot() {
        let mut st = SettingsState::new();
        let cmd = ClientSettingsCommand {
            uid: 1,
            x_sell: 50,
            x_sell_scalp: 10,
            x_tmode: false,
            fixed_sell_mode: false,
            fixed_sell_price: 0.0,
            price_drop_level: 0.0,
            trailing_drop: 0.0,
            trailing_stop: false,
            g_take_profit: 0.0,
            use_g_take_profit: false,
            unused_spread: 0,
            panic_if_price_drop: false,
            emu_mode: false,
            buy_iceberg: false,
            sell_iceberg: false,
            sign_orders: true,
            coins_black_list_text: String::new(),
            use_coins_black_list: false,
            temp_bl_symbols: vec![],
            temp_bl_times: vec![],
            use_manual_strategy: false,
            manual_strategy_id: 0,
            free_position_check: false,
            vol_drop_level: 0,
            use_stop_market: false,
            as_cfg: vec![0; AS_CFG_SIZE],
            as_cfg2: vec![0; AS_CFG2_SIZE],
            s_price: [0.0; 6],
            sb_num: 0,
            join_sell_kind: 0,
            arb_config: ArbConfigCompact::default(),
        };
        let ev = st.apply(UICommand::ClientSettings(Box::new(cmd)));
        assert!(matches!(ev, Some(SettingsEvent::ClientSettingsUpdated)));
        assert_eq!(st.client_settings.as_ref().unwrap().x_sell, 50);
    }

    #[test]
    fn inbound_mm_orders_subscribe_is_ignored_like_delphi_client() {
        let mut st = SettingsState::new();
        let ev = st.apply(UICommand::MMOrdersSubscribe(MMOrdersSubscribe {
            uid: 1,
            subscribe: true,
        }));
        assert!(ev.is_none());

        assert!(st.client_settings.is_none());
    }

    #[test]
    fn inbound_dex_switch_is_ignored_like_delphi_client() {
        let mut st = SettingsState::new();
        let ev = st.apply(UICommand::SwitchDex(SwitchDex {
            uid: 1,
            dex_name: "Uni".to_string(),
        }));
        assert!(ev.is_none());
    }

    #[test]
    fn inbound_spot_switch_is_ignored_like_delphi_client() {
        let mut st = SettingsState::new();
        let ev = st.apply(UICommand::SwitchSpot(SwitchSpot {
            uid: 1,
            spot_index: SpotMarketKind::Predict,
        }));
        assert!(ev.is_none());
    }

    #[test]
    fn arb_activate_stores_valid_until() {
        let mut st = SettingsState::new();
        let ev = st.apply(UICommand::ArbActivateNotify(ArbActivateNotify {
            uid: 1,
            arb_valid: 45000.5,
        }));
        assert_eq!(st.arb_valid_until, Some(45000.5));
        assert!(matches!(
            ev,
            Some(SettingsEvent::ArbActivated { arb_valid, .. })
                if arb_valid == MoonTime::from_delphi_days(45000.5).unwrap()
        ));
    }

    #[test]
    fn lev_manage_stores_snapshot() {
        let mut st = SettingsState::new();
        let lm = LevManage {
            uid: 1,
            cmd_ver: 1,
            auto_max_order: true,
            auto_lev_up: false,
            auto_isolated: true,
            auto_cross: false,
            auto_fix_lev: true,
            fix_lev: 10,
            tlg_report: false,
            lev_control: "BTC".to_string(),
        };
        let _ = st.apply(UICommand::LevManage(lm));
        assert!(st.lev_manage.is_some());
        assert_eq!(st.lev_manage.as_ref().unwrap().fix_lev, 10);
    }

    #[test]
    fn shared_config_builder_requires_a_real_full_snapshot() {
        assert!(SettingsState::new().build_shared_config().is_none());
    }

    #[test]
    fn full_snapshot_supersedes_older_compact_settings() {
        let mut st = SettingsState::new();
        let compact = ClientSettingsCommand {
            x_sell: 25,
            ..ClientSettingsCommand::default()
        };
        st.apply(UICommand::ClientSettings(Box::new(compact)));

        let mut full = crate::shared_config::SharedConfig::default();
        full.trading.x_sell = 75;
        apply_shared_config(&mut st, &full);

        assert_eq!(st.build_shared_config().unwrap().trading.x_sell, 75);
    }

    #[test]
    fn newer_compact_settings_are_overlaid_on_the_full_snapshot() {
        let mut st = SettingsState::new();
        let mut full = crate::shared_config::SharedConfig::default();
        full.trading.x_sell = 75;
        apply_shared_config(&mut st, &full);

        let compact = ClientSettingsCommand {
            x_sell: 25,
            ..ClientSettingsCommand::default()
        };
        st.apply(UICommand::ClientSettings(Box::new(compact)));

        assert_eq!(st.build_shared_config().unwrap().trading.x_sell, 25);
    }

    #[test]
    fn leverage_overlay_uses_the_same_receive_order_rule() {
        let mut st = SettingsState::new();
        st.apply(UICommand::LevManage(LevManage {
            uid: 1,
            cmd_ver: 1,
            auto_max_order: false,
            auto_lev_up: false,
            auto_isolated: false,
            auto_cross: false,
            fix_lev: 12,
            auto_fix_lev: true,
            tlg_report: false,
            lev_control: String::new(),
        }));
        let mut full = crate::shared_config::SharedConfig::default();
        full.trading.auto_manage_lev.fix_lev = 20;
        apply_shared_config(&mut st, &full);
        assert_eq!(
            st.build_shared_config()
                .unwrap()
                .trading
                .auto_manage_lev
                .fix_lev,
            20
        );

        st.apply(UICommand::LevManage(LevManage {
            uid: 2,
            cmd_ver: 1,
            auto_max_order: false,
            auto_lev_up: false,
            auto_isolated: false,
            auto_cross: false,
            fix_lev: 15,
            auto_fix_lev: true,
            tlg_report: false,
            lev_control: String::new(),
        }));
        assert_eq!(
            st.build_shared_config()
                .unwrap()
                .trading
                .auto_manage_lev
                .fix_lev,
            15
        );
    }

    #[test]
    fn runtime_state_stores_snapshot() {
        let mut st = SettingsState::new();
        let ev = st.apply(UICommand::RuntimeState(RuntimeStateCommand {
            uid: 1,
            is_started: true,
            auto_detect_active: false,
        }));
        assert!(matches!(ev, Some(SettingsEvent::RuntimeStateUpdated)));
        let runtime = st.runtime_state.unwrap();
        assert!(runtime.is_started);
        assert!(!runtime.auto_detect_active);
    }

    #[test]
    fn kernel_license_state_stores_snapshot() {
        let mut st = SettingsState::new();
        let ev = st.apply(UICommand::KernelLicenseState(KernelLicenseStateCommand {
            uid: 1,
            paid_version: true,
            reg_id: 42,
            order_count: 3,
            use_moon_strike: true,
            use_load_charts: false,
            use_web_hook: true,
            use_moon_streamer: false,
            use_algo_mod: true,
            use_ref_mod: false,
            use_back_mod: true,
            news_valid_until: Some(MoonTime::from_unix_millis(1_000)),
            news_trial_used: true,
            arb_active: false,
            arb_valid_until: Some(MoonTime::from_unix_millis(2_000)),
            moon_credits: 100,
            moon_credits_hold: 20,
            moon_credits_auction: 7,
            can_use_watcher: true,
        }));
        assert!(matches!(ev, Some(SettingsEvent::KernelLicenseStateUpdated)));
        let state = st.kernel_license_state.unwrap();
        assert!(state.paid_version);
        assert_eq!(state.reg_id, 42);
        assert_eq!(state.moon_credits, 100);
        assert!(state.can_use_watcher);
    }

    #[test]
    fn hyperliquid_request_limit_updates_and_clears_state() {
        let mut st = SettingsState::new();
        let ev = st.apply(UICommand::HyperliquidRequestLimitState(
            crate::commands::ui::HyperliquidRequestLimitStateCommand {
                uid: 1,
                requests_left: Some(12_345),
            },
        ));
        assert!(matches!(
            ev,
            Some(SettingsEvent::HyperliquidRequestLimitUpdated)
        ));
        assert_eq!(st.hyperliquid_requests_left, Some(12_345));

        st.apply(UICommand::HyperliquidRequestLimitState(
            crate::commands::ui::HyperliquidRequestLimitStateCommand {
                uid: 2,
                requests_left: None,
            },
        ));
        assert_eq!(st.hyperliquid_requests_left, None);
    }

    #[test]
    fn action_commands_pass_through_without_state() {
        let mut st = SettingsState::new();
        let ev = st.apply(UICommand::StratStartStop(StratStartStop {
            uid: 1,
            is_start: true,
        }));
        assert!(ev.is_none());
        // No retained state changes.
        assert!(st.client_settings.is_none());

        let ev = st.apply(UICommand::RestartNow { uid: 2 });
        assert!(ev.is_none());
        assert!(st.runtime_state.is_none());

        let ev = st.apply(UICommand::KernelLicenseStateRequest {
            uid: 3,
            activate_feature: 0,
        });
        assert!(ev.is_none());
        assert!(st.kernel_license_state.is_none());
    }
}
