//! Absorb live SDK state into a [`SharedConfig`].
//!
//! The SDK's compact client-settings and leverage-management snapshots carry a subset of the
//! fields that also appear in safe-share sections.  The `absorb_*` methods
//! overwrite matching fields from these commands, leaving the rest untouched.
//!
//! ## ManualStrategy name resolution
//!
//! `ClientSettingsCommand` carries `manual_strategy_id: u64` (a strategy
//! instance ID), while the share format stores the strategy *name* as a
//! string.  Resolution requires access to the strategy snapshot from the
//! SDK state.  If the strategy is not found, the base value of
//! `manual_strategy` is left unchanged.
//!
//! ## Fields intentionally NOT absorbed
//!
//! - `temp_bl_symbols` / `temp_bl_times` — runtime-only temporary blacklist,
//!   not a persistent setting.
//! - `emu_mode` — local emulator toggle, not shared.
//!
//! ## Reverse direction (SharedConfig -> SDK state)
//!
//! There is no `apply` method.  After the kernel imports a shared config, it
//! re-broadcasts `ClientSettingsCommand` through the normal protocol path.
//! The SDK state updates from that broadcast, not from a local overlay.

use super::SharedConfig;

impl SharedConfig {
    /// Overlay fields from a `ClientSettingsCommand` snapshot onto this
    /// config.  Only the intersection is touched; fields that have no
    /// counterpart in the command are left as-is.
    ///
    /// Mirrors [`ClientSettingsCommand`] fields (from `commands::ui`):
    ///
    /// | Command field | SharedConfig path |
    /// |---|---|
    /// | `x_sell` | `trading.x_sell` |
    /// | `x_sell_scalp` | `trading.x_sell_scalp` |
    /// | `x_tmode` | `trading.x_t_mode` |
    /// | `fixed_sell_mode` | `trading.fixed_sell_mode` |
    /// | `fixed_sell_price` | `trading.fixed_sell_price` |
    /// | `price_drop_level` | `trading.price_drop_level` |
    /// | `trailing_drop` | `trading.trailing_drop` |
    /// | `trailing_stop` | `trading.trailing_stop` |
    /// | `g_take_profit` | `trading.g_take_profit` |
    /// | `use_g_take_profit` | `trading.use_g_take_profit` |
    /// | `panic_if_price_drop` | `trading.panic_if_price_drop` |
    /// | `buy_iceberg` | `trading.buy_iceberg` |
    /// | `sell_iceberg` | `trading.sell_iceberg` |
    /// | `sign_orders` | `trading.orders_control.sign_orders` |
    /// | `coins_black_list_text` | `trading.coins_black_list_text` |
    /// | `use_coins_black_list` | `trading.use_coins_black_list` |
    /// | `use_manual_strategy` | `trading.use_manual_strategy` |
    /// | `free_position_check` | `trading.free_position_check` |
    /// | `vol_drop_level` | `trading.vol_drop_level` |
    /// | `use_stop_market` | `trading.use_stop_market` |
    /// | `s_price` | `ui.hotkeys_config.s_price` |
    /// | `sb_num` | `ui.hotkeys_config.sb_num` |
    /// | `join_sell_kind` | `trading.multi_orders.join_sell_kind` |
    ///
    /// Not absorbed: `emu_mode`, `temp_bl_symbols`, `temp_bl_times`,
    /// `manual_strategy_id` (needs name resolution — see module doc).
    pub(crate) fn absorb_client_settings_raw(
        &mut self,
        x_sell: i32,
        x_sell_scalp: i32,
        x_tmode: bool,
        fixed_sell_mode: bool,
        fixed_sell_price: f64,
        price_drop_level: f32,
        trailing_drop: f32,
        trailing_stop: bool,
        g_take_profit: f64,
        use_g_take_profit: bool,
        panic_if_price_drop: bool,
        buy_iceberg: bool,
        sell_iceberg: bool,
        sign_orders: bool,
        coins_black_list_text: &str,
        use_coins_black_list: bool,
        use_manual_strategy: bool,
        free_position_check: bool,
        vol_drop_level: i32,
        use_stop_market: bool,
        s_price: &[f32; 6],
        sb_num: u8,
        join_sell_kind: u8,
    ) {
        self.trading.x_sell = x_sell;
        self.trading.x_sell_scalp = x_sell_scalp;
        self.trading.x_t_mode = x_tmode;
        self.trading.fixed_sell_mode = fixed_sell_mode;
        self.trading.fixed_sell_price = fixed_sell_price;
        self.trading.price_drop_level = price_drop_level;
        self.trading.trailing_drop = trailing_drop;
        self.trading.trailing_stop = trailing_stop;
        self.trading.g_take_profit = g_take_profit;
        self.trading.use_g_take_profit = use_g_take_profit;
        self.trading.panic_if_price_drop = panic_if_price_drop;
        self.trading.buy_iceberg = buy_iceberg;
        self.trading.sell_iceberg = sell_iceberg;
        self.trading.orders_control.sign_orders = sign_orders;
        self.trading.coins_black_list_text = coins_black_list_text.to_string();
        self.trading.use_coins_black_list = use_coins_black_list;
        self.trading.use_manual_strategy = use_manual_strategy;
        self.trading.free_position_check = free_position_check;
        self.trading.vol_drop_level = vol_drop_level;
        self.trading.use_stop_market = use_stop_market;
        self.ui.hotkeys_config.s_price = *s_price;
        self.ui.hotkeys_config.sb_num = sb_num;
        self.trading.multi_orders.join_sell_kind = join_sell_kind;
    }

    /// Overlay leverage-management fields from a `LevManage` command.
    ///
    /// | Command field | SharedConfig path |
    /// |---|---|
    /// | `auto_max_order` | `trading.auto_manage_lev.auto_max_order` |
    /// | `auto_lev_up` | `trading.auto_manage_lev.auto_lev_up` |
    /// | `auto_isolated` | `trading.auto_manage_lev.auto_isolated` |
    /// | `auto_cross` | `trading.auto_manage_lev.auto_cross` |
    /// | `auto_fix_lev` | `trading.auto_manage_lev.auto_fix_lev` |
    /// | `fix_lev` | `trading.auto_manage_lev.fix_lev` |
    /// | `tlg_report` | `trading.auto_manage_lev.tlg_report` |
    /// | `lev_control` | `trading.auto_lev_control` |
    pub(crate) fn absorb_lev_manage_raw(
        &mut self,
        auto_max_order: bool,
        auto_lev_up: bool,
        auto_isolated: bool,
        auto_cross: bool,
        auto_fix_lev: bool,
        fix_lev: i32,
        tlg_report: bool,
        lev_control: &str,
    ) {
        self.trading.auto_manage_lev.auto_max_order = auto_max_order;
        self.trading.auto_manage_lev.auto_lev_up = auto_lev_up;
        self.trading.auto_manage_lev.auto_isolated = auto_isolated;
        self.trading.auto_manage_lev.auto_cross = auto_cross;
        self.trading.auto_manage_lev.auto_fix_lev = auto_fix_lev;
        self.trading.auto_manage_lev.fix_lev = fix_lev;
        self.trading.auto_manage_lev.tlg_report = tlg_report;
        self.trading.auto_lev_control = lev_control.to_string();
    }
}
