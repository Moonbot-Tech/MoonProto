//! Account-level balance totals maintained by the active `MoonClient` runtime.
//!
//! Per-market balance, position, liquidation, leverage, and PnL live directly
//! on each retained `Market`. This module keeps only account-level totals
//! (BTC totals + total PnL), so chart/UI code reads one authoritative market
//! object instead of stitching together a separate balance table.

use crate::commands::balance::BalanceUpdate;
use crate::state::epoch::epoch_is_ok;

// parity: HelpClasses.HashMix. Mix high double bits back into the low half.
pub(crate) fn balance_hash_mix(hash: u64, value: u64) -> u64 {
    let hash = (hash ^ value).wrapping_mul(0x9E37_79B9_7F4A_7C15);
    hash ^ (hash >> 32)
}

/// Global account balance totals in BTC-equivalent units.
#[derive(Debug, Clone, Default)]
pub struct GlobalBalance {
    /// Available BTC-equivalent balance.
    pub btc_balance_total: f64,
    /// Locked BTC-equivalent balance.
    pub btc_balance_locked: f64,
    /// Full BTC-equivalent balance including unrealized PnL.
    pub btc_balance_full: f64,
    /// Special-coin balance (USDT for futures, BUSD/USDC in MA mode, etc.).
    pub special_coin_balance: f64,
    /// Sum of per-market `total_profit` for BTC-quoted markets only,
    /// recomputed from the retained live market objects.
    pub total_pnl: f64,
}

impl GlobalBalance {
    pub(crate) fn balance_hash(&self) -> u64 {
        [
            self.btc_balance_total,
            self.btc_balance_locked,
            self.btc_balance_full,
            self.special_coin_balance,
        ]
        .into_iter()
        .fold(0, |hash, value| balance_hash_mix(hash, value.to_bits()))
    }
}

/// Account-level balance state published through active-session snapshots.
///
/// Per-market rows are read from the live markets
/// ([`crate::events::MoonStateSnapshot::markets`] /
/// [`crate::state::markets::MarketHandle::balance_position`]); this state holds
/// only the account-level totals.
#[derive(Debug, Clone, Default)]
pub struct BalancesState {
    /// Account totals (BTC, special coin, total PnL).
    pub global: GlobalBalance,
    /// Latest balance/digest epoch, used to ignore stale digests. Full snapshots
    /// reset it unconditionally; individual rows retain their own epoch gates.
    pub(crate) last_epoch: u16,
}

#[derive(Debug, Clone)]
pub enum BalanceEvent {
    /// Full snapshot applied: N markets received rows, missing rows were reset.
    SnapshotApplied {
        count: usize,
        #[cfg(any(test, feature = "diagnostics"))]
        #[doc(hidden)]
        epoch: u16,
    },
    /// Incremental update applied.
    IncrementalApplied {
        count: usize,
        #[cfg(any(test, feature = "diagnostics"))]
        #[doc(hidden)]
        epoch: u16,
        global_changed: bool,
    },
    /// Authoritative per-market session-profit snapshot applied.
    ///
    /// The payload contains only non-zero rows; every known market omitted from
    /// it was reset to zero.
    SessionProfitsApplied {
        /// Non-zero rows that resolved to a known market and were applied.
        nonzero_count: usize,
        #[cfg(any(test, feature = "diagnostics"))]
        #[doc(hidden)]
        epoch: u16,
    },
    /// Command was recognized, but this command kind has no balance-state effect.
    #[cfg(any(test, feature = "diagnostics"))]
    #[doc(hidden)]
    Ignored { cmd_id: u8, epoch: u16 },
    /// Epoch check rejected the packet as stale.
    #[cfg(any(test, feature = "diagnostics"))]
    #[doc(hidden)]
    EpochStale { incoming: u16, last: u16 },
    /// Session-profit snapshot rejected as duplicate or stale.
    #[cfg(any(test, feature = "diagnostics"))]
    #[doc(hidden)]
    SessionProfitEpochStale { incoming: u16, last: u16 },
}

impl BalancesState {
    pub fn new() -> Self {
        Self::default()
    }

    /// Apply the account-level globals after the per-market balance apply ran on
    /// markets. `total_pnl` is
    /// [`crate::state::markets::MarketsState::sum_btc_total_profit`]
    /// over the just-updated live markets.
    ///
    /// - cmd 3 (full snapshot): always carries globals.
    /// - cmd 4 (incremental): updates globals only when `global_changed`.
    /// - other (e.g. cmd 2): not applied.
    // parity: MoonBot MarketsU.pas:TMarkets (FTotalPNL/BTC globals) + RecalcTotalPnl
    pub(crate) fn apply_global(&mut self, upd: &BalanceUpdate, total_pnl: f64) {
        let set_btc = match upd.cmd_id {
            3 => true,
            4 => upd.global_changed,
            _ => return,
        };
        if set_btc {
            self.global.btc_balance_total = upd.btc_balance_total;
            self.global.btc_balance_locked = upd.btc_balance_locked;
            self.global.btc_balance_full = upd.btc_balance_full;
            self.global.special_coin_balance = upd.special_coin_balance;
        }
        self.global.total_pnl = total_pnl;
        if upd.cmd_id == 3 || epoch_is_ok(self.last_epoch, upd.epoch) {
            self.last_epoch = upd.epoch;
        }
    }

    pub fn global(&self) -> &GlobalBalance {
        &self.global
    }

    pub fn clear(&mut self) {
        self.global = GlobalBalance::default();
        self.last_epoch = 0;
    }
}

#[cfg(test)]
mod tests;
