//! Snapshot preconditions for applications that must not act on a different order phase.

use super::Order;
use crate::commands::trade::OrderWorkerStatus;

/// An opaque local-instance and phase guard captured from an observed order.
/// It is checked on the runtime thread before the existing action selects its exchange leg.
/// The core or exchange may still change after dispatch; this is not a server-side CAS.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct OrderGuard {
    uid: u64,
    instance: u64,
    status: OrderWorkerStatus,
    buy_exchange_id: i64,
    sell_exchange_id: i64,
    buy_remaining: u64,
    sell_remaining: u64,
    buy_actual_price: u64,
    sell_actual_price: u64,
    buy_quantity: u64,
    sell_quantity: u64,
    panic_sell: bool,
    buy_price: u64,
    sell_price: u64,
    buy_size: u64,
    pending_trigger: Option<u64>,
    replace_sent_ms: i64,
}

impl OrderGuard {
    /// The stable server UID of the observed order.
    pub fn uid(self) -> u64 { self.uid }

    /// Local lifetime identity; a removed and recreated UID receives a different incarnation.
    /// Compare only within the same client connection generation, never across clients.
    pub fn incarnation(self) -> u64 { self.instance }

    /// Compare inside the runtime owner, before any optimistic state or wire command changes.
    pub(crate) fn matches(self, order: &Order) -> bool {
        !order.job_is_done && !order.pending_cancel && !order.bulk_replace_buy && !order.bulk_replace_sell
            && self.replace_sent_ms == 0 && self == order.guard()
    }
}

impl Order {
    /// Capture the local incarnation, phase, exchange leg and existing target for a guarded action.
    pub fn guard(&self) -> OrderGuard {
        OrderGuard { uid:self.uid, instance:self.instance_id, status:self.status,
            buy_exchange_id:self.buy_order.int_id, sell_exchange_id:self.sell_order.int_id,
            buy_remaining:self.buy_order.quantity_remaining.to_bits(), sell_remaining:self.sell_order.quantity_remaining.to_bits(),
            buy_actual_price:self.buy_order.actual_price.to_bits(), sell_actual_price:self.sell_order.actual_price.to_bits(),
            buy_quantity:self.buy_order.quantity.to_bits(), sell_quantity:self.sell_order.quantity.to_bits(), panic_sell:self.panic_sell,
            buy_price:self.buy_price.to_bits(), sell_price:self.sell_price.to_bits(), buy_size:self.buy_size.to_bits(),
            pending_trigger:self.pending_buy_cond_price.map(f64::to_bits),
            replace_sent_ms:self.replace_sent_time_ms }
    }
}

#[cfg(test)]
mod tests;
