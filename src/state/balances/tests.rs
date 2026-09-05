use super::*;
use crate::commands::balance::BalanceUpdate;

fn upd(cmd_id: u8, epoch: u16, global_changed: bool) -> BalanceUpdate {
    BalanceUpdate {
        cmd_id,
        epoch,
        global_changed,
        btc_balance_total: 1.0,
        btc_balance_locked: 0.5,
        btc_balance_full: 1.5,
        special_coin_balance: 42.0,
        items: Vec::new(),
    }
}

// Per-market balance apply (full snapshot, missing-reset, epoch gate, increment)
// is tested at dispatch level in `events::tests` against the live `MarketsState`
// (the single Delphi-parity store). These tests cover the account-level globals
// that `BalancesState` keeps.

#[test]
fn full_snapshot_sets_globals_and_total_pnl() {
    let mut s = BalancesState::new();
    s.apply_global(&upd(3, 1, false), 7.0);
    assert_eq!(s.global().btc_balance_total, 1.0);
    assert_eq!(s.global().btc_balance_locked, 0.5);
    assert_eq!(s.global().special_coin_balance, 42.0);
    assert_eq!(s.global().total_pnl, 7.0);
    assert_eq!(s.last_epoch, 1);
}

#[test]
fn incremental_sets_globals_only_when_changed_but_always_recalcs_pnl() {
    let mut s = BalancesState::new();
    s.apply_global(&upd(3, 1, false), 0.0); // seed globals (btc_total=1.0)

    // global_changed = false: BTC totals kept; total_pnl (recalc) still applied.
    let mut u = upd(4, 2, false);
    u.btc_balance_total = 999.0;
    s.apply_global(&u, 3.0);
    assert_eq!(s.global().btc_balance_total, 1.0); // unchanged
    assert_eq!(s.global().total_pnl, 3.0); // recalc always set
    assert_eq!(s.last_epoch, 2);

    // global_changed = true: BTC totals updated.
    let mut u2 = upd(4, 3, true);
    u2.btc_balance_total = 5.0;
    s.apply_global(&u2, 9.0);
    assert_eq!(s.global().btc_balance_total, 5.0);
    assert_eq!(s.global().total_pnl, 9.0);
}

#[test]
// parity: MoonBot MoonProtoEngine.pas:TMoonProtoEngine.ProcessBalanceCommand
fn exact_balance_command_cmd2_is_ignored() {
    let mut s = BalancesState::new();
    s.apply_global(&upd(3, 1, false), 7.0);
    s.apply_global(&upd(2, 2, true), 999.0); // cmd 2: not applied
    assert_eq!(s.global().total_pnl, 7.0);
    assert_eq!(s.global().btc_balance_total, 1.0);
    assert_eq!(s.last_epoch, 1); // unchanged
}

#[test]
fn clear_resets_globals() {
    let mut s = BalancesState::new();
    s.apply_global(&upd(3, 5, false), 7.0);
    s.clear();
    assert_eq!(s.global().total_pnl, 0.0);
    assert_eq!(s.global().btc_balance_total, 0.0);
    assert_eq!(s.last_epoch, 0);
}

#[test]
fn balance_hash_matches_wrapping_double_bit_vectors() {
    for (amount, expected) in [
        (0.0, 0),
        (1.0, 0x8b1b_c328_5493_7bd6),
        (4.0, 0xde8d_19eb_f675_6ec0),
        (100.0, 0x48d3_a0e2_cff8_33ea),
        (200.0, 0xeada_57da_504c_11c1),
        (375.0, 0x8214_49a1_5269_f5ba),
        (786.0, 0x0625_72a4_51ca_b7ec),
        (0.01, 0x3eb3_0d49_117d_216e),
        (-42.4, 0x977e_efc8_d31e_e67a),
    ] {
        let global = GlobalBalance {
            btc_balance_total: amount,
            btc_balance_full: amount,
            ..Default::default()
        };
        assert_eq!(global.balance_hash(), expected, "amount={amount}");
    }
}

#[test]
fn balance_hash_does_not_cancel_equal_wallet_fields() {
    let mut seen = std::collections::HashSet::new();
    for amount in 0..100_000 {
        let global = GlobalBalance {
            btc_balance_total: amount as f64,
            btc_balance_full: amount as f64,
            ..Default::default()
        };
        assert!(seen.insert(global.balance_hash()), "collision at {amount}");
    }
}

#[test]
fn balance_hash_uses_all_four_wire_totals_but_not_derived_pnl() {
    let global = GlobalBalance::default();
    let variants = [
        GlobalBalance {
            btc_balance_total: 2.0,
            ..global.clone()
        },
        GlobalBalance {
            btc_balance_locked: 2.0,
            ..global.clone()
        },
        GlobalBalance {
            btc_balance_full: 2.0,
            ..global.clone()
        },
        GlobalBalance {
            special_coin_balance: 2.0,
            ..global.clone()
        },
    ];
    let hashes: std::collections::HashSet<_> =
        variants.iter().map(GlobalBalance::balance_hash).collect();
    assert_eq!(hashes.len(), variants.len());
    assert!(!hashes.contains(&global.balance_hash()));
    assert_eq!(
        global.balance_hash(),
        GlobalBalance {
            total_pnl: 123.0,
            ..global
        }
        .balance_hash()
    );
}

#[test]
fn balance_digest_watermark_ignores_old_increments_and_full_resets_it() {
    let mut state = BalancesState::new();
    state.apply_global(&upd(3, 100, false), 0.0);
    state.apply_global(&upd(4, 99, true), 1.0);
    assert_eq!(state.last_epoch, 100);
    state.apply_global(&upd(3, u16::MAX, false), 0.0);
    state.apply_global(&upd(4, 0, false), 0.0);
    assert_eq!(state.last_epoch, 0);
    state.apply_global(&upd(3, 1, false), 0.0);
    assert_eq!(state.last_epoch, 1);
}
