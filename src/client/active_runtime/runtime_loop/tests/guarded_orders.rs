//! The guard must run after dequeueing, before an old entry intent can select an exit leg.

use super::*;

/// A BUY snapshot captured before an advancing order image cannot cancel the resulting SELL.
#[test]
fn guarded_cancel_rechecks_phase_at_runtime_dispatch() {
    let mut client = ready_client();
    let mut dispatcher = crate::events::EventDispatcher::new();
    seed_runtime_order(&mut dispatcher, 7, OrderWorkerStatus::BuySet, 1);
    let guard = dispatcher.orders().get(7).unwrap().guard();
    seed_runtime_order(&mut dispatcher, 7, OrderWorkerStatus::SellSet, 2);
    let (reply, receipt) = mpsc::channel();
    assert!(!handle_command(&mut client, &mut dispatcher,
        RuntimeCommand::GuardedOrder { guard, action:GuardedOrderAction::Cancel, reply }, &mut RuntimePending::default()));
    assert!(!receipt.try_recv().unwrap());
    let (normal, high, low) = client.take_send_queues_for_test();
    assert!(normal.is_empty() && high.is_empty() && low.is_empty());
}

/// Guarded actions arriving during startup are rejected instead of firing after a reconnect.
#[test]
fn guarded_cancel_never_joins_the_startup_deferred_queue() {
    let mut dispatcher = crate::events::EventDispatcher::new();
    seed_runtime_order(&mut dispatcher, 7, OrderWorkerStatus::BuySet, 1);
    let guard = dispatcher.orders().get(7).unwrap().guard();
    let (reply, receipt) = mpsc::channel();
    let (tx, rx) = mpsc::channel();
    tx.send(RuntimeCommand::GuardedOrder { guard, action:GuardedOrderAction::Cancel, reply }).unwrap();
    let mut deferred = VecDeque::new();
    assert_eq!(drain_commands_during_startup(&rx, &mut deferred), (false, false));
    assert!(deferred.is_empty());
    assert!(!receipt.try_recv().unwrap());
}

/// Setting Panic Sell twice must never use the market button's toggle or clear stop settings.
#[test]
fn guarded_panic_sets_the_order_instead_of_toggling_it() {
    let mut client = ready_client();
    let mut dispatcher = crate::events::EventDispatcher::new();
    seed_runtime_order(&mut dispatcher, 7, OrderWorkerStatus::SellSet, 1);
    let guard = dispatcher.orders().get(7).unwrap().guard();
    let (reply, receipt) = mpsc::channel();
    assert!(handle_command(&mut client, &mut dispatcher,
        RuntimeCommand::GuardedOrder { guard, action:GuardedOrderAction::Panic(true), reply }, &mut RuntimePending::default()));
    assert!(receipt.try_recv().unwrap());
    assert!(dispatcher.orders().get(7).unwrap().panic_sell);
    let _ = client.take_send_queues_for_test();
    let (reply, receipt) = mpsc::channel();
    let guard = dispatcher.orders().get(7).unwrap().guard();
    assert!(!handle_command(&mut client, &mut dispatcher,
        RuntimeCommand::GuardedOrder { guard, action:GuardedOrderAction::Panic(true), reply }, &mut RuntimePending::default()));
    assert!(!receipt.try_recv().unwrap());
    assert!(dispatcher.orders().get(7).unwrap().panic_sell);
    let (normal, high, low) = client.take_send_queues_for_test();
    assert!(normal.is_empty() && high.is_empty() && low.is_empty());
}
