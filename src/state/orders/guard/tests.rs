//! A queued entry action must never silently become an exit action or target a reused UID.

use super::*;
use crate::commands::{market::{BaseCurrency, ExchangeCode}, trade::OrderDescription};

/// Capture an entry then change the facts the runtime must recheck before dispatch.
#[test]
fn rejects_changed_phase_instance_target_fill_and_pending_replace() {
    let mut entry = Order::new(7, &OrderDescription::for_test("BTCUSDT", false, false), 1, BaseCurrency::USDT, ExchangeCode::Binance);
    entry.status = OrderWorkerStatus::BuySet;
    entry.buy_price = 100.0;
    entry.buy_order.int_id = 101;
    entry.buy_order.quantity_remaining = 2.0;
    let guard = entry.guard();
    assert!(guard.matches(&entry));
    let mut changed = entry.clone();
    changed.status = OrderWorkerStatus::SellSet;
    assert!(!guard.matches(&changed));
    changed = entry.clone(); changed.instance_id = 2;
    assert!(!guard.matches(&changed));
    changed = entry.clone(); changed.buy_price = 101.0;
    assert!(!guard.matches(&changed));
    changed = entry.clone(); changed.pending_buy_cond_price = Some(102.0);
    assert!(!guard.matches(&changed));
    changed = entry.clone(); changed.buy_order.quantity_remaining = 1.0;
    assert!(!guard.matches(&changed));
    changed = entry.clone(); changed.buy_order.actual_price = 99.0;
    assert!(!guard.matches(&changed));
    changed = entry.clone(); changed.sell_order.actual_price = 102.0;
    assert!(!guard.matches(&changed));
    changed = entry.clone(); changed.buy_order.quantity = 3.0;
    assert!(!guard.matches(&changed));
    changed = entry.clone(); changed.sell_order.quantity = 3.0;
    assert!(!guard.matches(&changed));
    changed = entry.clone(); changed.panic_sell = true;
    assert!(!guard.matches(&changed));
    changed = entry.clone(); changed.job_is_done = true;
    assert!(!guard.matches(&changed));
    changed = entry.clone(); changed.bulk_replace_buy = true;
    assert!(!guard.matches(&changed));
    changed = entry.clone(); changed.bulk_replace_sell = true;
    assert!(!guard.matches(&changed));
    changed = entry.clone(); changed.replace_sent_time_ms = 100;
    assert!(!changed.guard().matches(&changed));
}
