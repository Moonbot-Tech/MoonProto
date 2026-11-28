//! Recent cancelled orders, independent of the live order replica.

use crate::{BaseCurrency, ExchangeCode, MoonTime, OrderType, ReportTrace};
use std::sync::Arc;

/// Maximum number of cancelled orders retained per client.
pub const CANCELLED_ORDERS_HISTORY_LIMIT: usize = 100;

/// Final chart data for one cancelled order. Times are UTC milliseconds.
#[derive(Debug, Clone, PartialEq)]
pub struct CancelledOrder {
    /// Original live order UID. History and live delivery keep the same identity.
    pub uid: u64,
    pub market: String,
    pub exchange: ExchangeCode,
    pub base_currency: BaseCurrency,
    pub strategy_id: u64,
    pub is_emulator: bool,
    pub is_short: bool,
    pub order_type: OrderType,
    pub created_at: MoonTime,
    pub cancelled_at: MoonTime,
    pub price: f64,
    pub quantity: f64,
    /// Original chart geometry, in the same format as archived report traces.
    /// When empty, draw the fixed price between creation and cancellation.
    pub traces: Arc<[ReportTrace]>,
}

/// Immutable-snapshot-friendly history. Inserts copy tree paths, not the full list.
#[derive(Debug, Clone, Default)]
pub struct CancelledOrders {
    records: im::OrdMap<u64, Arc<CancelledOrder>>,
    by_id: im::HashMap<u64, u64>,
}

impl CancelledOrders {
    pub fn len(&self) -> usize { self.records.len() }
    pub fn is_empty(&self) -> bool { self.records.is_empty() }

    pub fn get(&self, uid: u64) -> Option<&CancelledOrder> {
        self.records.get(self.by_id.get(&uid)?).map(Arc::as_ref)
    }

    /// Most recently retained by the core first, independent of packet arrival and clock time.
    pub fn iter(&self) -> impl Iterator<Item = &CancelledOrder> {
        self.records.iter().rev().map(|(_, order)| order.as_ref())
    }

    pub(crate) fn apply(&mut self, sequence: u64, order: Arc<CancelledOrder>) -> bool {
        if self.by_id.contains_key(&order.uid) { return false; }
        if self.len() == CANCELLED_ORDERS_HISTORY_LIMIT {
            let oldest = *self.records.iter().next().unwrap().0;
            if sequence <= oldest { return false; }
            let removed = self.records.remove(&oldest).unwrap();
            self.by_id.remove(&removed.uid);
        }
        self.by_id.insert(order.uid, sequence);
        self.records.insert(sequence, order);
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn order(uid: u64, cancelled_at: i64) -> Arc<CancelledOrder> {
        Arc::new(CancelledOrder { uid, market: "BTCUSDT".into(), exchange: ExchangeCode::FBinance,
            base_currency: BaseCurrency::USDT, strategy_id: 0, is_emulator: true, is_short: false,
            order_type: OrderType::Buy, created_at: MoonTime::from_unix_millis(1),
            cancelled_at: MoonTime::from_unix_millis(cancelled_at), price: 100.0, quantity: 1.0,
            traces: Arc::from([]) })
    }

    #[test]
    fn repeats_and_reordered_history_keep_the_last_hundred_once() {
        let mut history = CancelledOrders::default();
        for uid in (1..=150).rev() {
            history.apply(uid, order(uid, uid as i64));
        }
        for uid in 1..=150 {
            assert!(!history.apply(uid, order(uid, uid as i64)));
        }
        assert_eq!(history.len(), 100);
        assert_eq!(history.iter().map(|v| v.uid).collect::<Vec<_>>(), (51..=150).rev().collect::<Vec<_>>());
        assert!(history.apply(151, order(151, 151)));
        assert!(history.get(51).is_none());
        assert!(history.get(151).is_some());
    }

    #[test]
    fn delayed_finalization_and_equal_timestamps_follow_core_retention_order() {
        let mut history = CancelledOrders::default();
        for uid in 1..=100 { history.apply(uid, order(uid, 1000)); }
        // A sell worker finishes after newer buy cancellations; its older timestamp
        // must not make the connected client reject what a fresh client receives.
        assert!(history.apply(101, order(999, 1)));
        assert!(history.get(1).is_none());
        assert_eq!(history.iter().next().unwrap().uid, 999);
        assert!(history.apply(102, order(500, 1000)));
        assert!(history.get(2).is_none());
        assert!(!history.apply(1, order(1, 2000)));
        let mut fresh = CancelledOrders::default();
        for uid in (3..=100).rev() { fresh.apply(uid, order(uid, 1000)); }
        fresh.apply(102, order(500, 1000));
        fresh.apply(101, order(999, 1));
        assert_eq!(history.iter().collect::<Vec<_>>(), fresh.iter().collect::<Vec<_>>());
    }

    #[test]
    fn held_snapshots_and_clients_are_independent_without_copying_records() {
        let mut history = CancelledOrders::default();
        for uid in 1..=100 { history.apply(uid, order(uid, uid as i64)); }
        let held = history.clone();
        let before = held.get(50).unwrap() as *const CancelledOrder;
        assert_eq!(history.get(50).unwrap() as *const CancelledOrder, before);
        history.apply(101, order(101, 101));
        assert!(held.get(1).is_some());
        assert!(history.get(1).is_none());
        assert_eq!(history.get(50).unwrap() as *const CancelledOrder, before);
        let mut other = CancelledOrders::default();
        assert!(other.apply(1, order(101, 1)));
        assert_eq!(history.get(101).unwrap().cancelled_at.unix_millis(), 101);
    }
}
