use super::*;

#[test]
fn force_cancel_uses_the_existing_side_group_and_preserves_retained_state() {
    for (status, opcode, ordinary) in [
        (OrderWorkerStatus::BuySet, 20, OrderCommandPayload::CancelBuy { order_id: 7 }),
        (OrderWorkerStatus::SellSet, 21, OrderCommandPayload::CancelSell { order_id: 7 }),
    ] {
        let group = ordinary.group().unwrap();
        let mut client = ready_client();
        let mut d = crate::events::EventDispatcher::new();
        seed_runtime_order(&mut d, 7, status, 1);
        let held = d.snapshot();
        let before = held.orders().get(7).unwrap().guard();
        let (tx, rx) = mpsc::channel();
        let orders = crate::client::MoonOrders { tx };
        let mut pending = RuntimePending::default();
        // The force request replaces an unsent ordinary cancel in the same side's slot.
        orders.cancel(7u64).unwrap();
        assert!(handle_command(&mut client, &mut d, rx.try_recv().unwrap(), &mut pending));
        let retained_before_force = d.snapshot();
        orders.force_cancel(held.orders().get(7).unwrap()).unwrap();
        assert!(handle_command(&mut client, &mut d, rx.try_recv().unwrap(), &mut pending));
        let (sliced, high, low) = client.take_send_queues_for_test();
        assert!(sliced.is_empty() && low.is_empty());
        assert_eq!(high.len(), 1);
        let item = &high[0];
        assert!(item.encrypted);
        assert_eq!(item.max_retries, 4);
        assert_eq!(item.cmd, Command::Order.to_byte());
        assert_eq!(item.u_key, crate::client::UniqueKey::order_command(group.unique_kind(), 7));
        assert_eq!(&item.data[11..], &[opcode, 7, 0, 0, 0, 0, 0, 0, 0]);
        let TradeCommand::OrderCommand(command) = TradeCommand::parse(&item.data).unwrap() else { panic!() };
        assert_ne!(command.header.uid, 0);
        assert_eq!(command.payload.opcode(), opcode);
        assert_eq!(command.payload.order_id(), Some(7));
        assert_eq!(command.payload.group(), Some(group));
        assert_eq!(command.to_bytes(), item.data);
        assert_eq!(d.orders().get(7).unwrap().guard(), before);
        assert!(std::ptr::eq(retained_before_force.orders().get(7).unwrap(), d.orders().get(7).unwrap()),
            "force cancellation must not clone or change the retained order");
    }
}

#[test]
fn force_cancel_does_not_send_for_pending_filled_missing_or_disconnected_orders() {
    for (status, ready, exists) in [
        (OrderWorkerStatus::None, true, true),
        (OrderWorkerStatus::BuyDone, true, true),
        (OrderWorkerStatus::SellSet, true, false),
        (OrderWorkerStatus::BuySet, false, true),
    ] {
        let mut client = ready_client();
        client.testing_set_domain_ready(ready);
        let mut d = crate::events::EventDispatcher::new();
        if exists { seed_runtime_order(&mut d, 7, status, 1); }
        let (tx, rx) = mpsc::channel();
        crate::client::MoonOrders { tx }.force_cancel(7u64).unwrap();
        assert!(!handle_command(&mut client, &mut d, rx.try_recv().unwrap(), &mut RuntimePending::default()));
        let (sliced, high, low) = client.take_send_queues_for_test();
        assert!(sliced.is_empty() && high.is_empty() && low.is_empty());
        if exists { assert!(!d.orders().get(7).unwrap().pending_cancel); }
    }
}

#[test]
fn ordinary_cancel_keeps_buy_sell_and_pending_commands() {
    for (status, opcode) in [(OrderWorkerStatus::None, 9), (OrderWorkerStatus::BuySet, 7), (OrderWorkerStatus::SellSet, 8)] {
        let mut client = ready_client();
        let mut d = crate::events::EventDispatcher::new();
        seed_runtime_order(&mut d, 7, status, 1);
        let (tx, rx) = mpsc::channel();
        crate::client::MoonOrders { tx }.cancel(7u64).unwrap();
        assert!(handle_command(&mut client, &mut d, rx.try_recv().unwrap(), &mut RuntimePending::default()));
        let (_, high, _) = client.take_send_queues_for_test();
        assert_eq!(high.len(), 1);
        assert_eq!(high[0].data[11], opcode);
        assert_eq!(d.orders().get(7).unwrap().pending_cancel, status == OrderWorkerStatus::None);
    }
}
