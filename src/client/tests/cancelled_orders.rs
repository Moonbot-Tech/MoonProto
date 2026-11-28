use super::*;
use crate::events::{ActiveDispatchContext, EventDispatcher};

fn client(ready: bool) -> Client {
    let mut c = Client::new(ClientConfig::new("127.0.0.1", 3000, [0; 16], [0; 16]).without_ntp());
    c.testing_set_domain_ready(ready);
    c.authorized = true;
    c.server_token = 1;
    c.peer_app_token = 10;
    c
}

fn history_requests(c: &Client) -> usize {
    let (sliced, high, low) = c.take_send_queues_for_test();
    sliced.into_iter().chain(high).chain(low).filter(|item| {
        item.cmd == Command::Order.to_byte() && item.data.first() == Some(&54)
    }).count()
}

#[test]
fn cancelled_history_is_requested_after_init_and_on_ready_reconnect() {
    let mut c = client(false);
    c.restore_domain_after_reconnect();
    assert_eq!(history_requests(&c), 0);
    c.testing_set_domain_ready(true);
    let mut d = EventDispatcher::new();
    super::super::init::send_post_init_resync(&mut c, &mut d, &InitConfig::default(),
        &mut super::super::init::InitResult::default());
    assert_eq!(history_requests(&c), 1);
    c.restore_domain_after_reconnect();
    assert_eq!(history_requests(&c), 1);
}

#[test]
fn cancelled_history_survives_rebind_but_not_a_new_core_world() {
    let mut c = client(true);
    let mut d = EventDispatcher::new();
    let mut packet = include_bytes!("../../../tests/fixtures/cancelled-order.bin").to_vec();
    for (server, peer, uid) in [(1, 10, 123_u64), (2, 10, 123), (3, 20, 456)] {
        c.server_token = server;
        c.peer_app_token = peer;
        packet[3..11].copy_from_slice(&uid.to_le_bytes());
        d.dispatch_into_active_actions(Command::Order, &packet, 1000, &mut Vec::new(),
            &ActiveDispatchContext::from_client(&c), &mut Vec::new());
        assert_eq!(d.snapshot().cancelled_orders().len(), 1);
        assert!(d.snapshot().cancelled_orders().get(uid).is_some());
    }
    assert!(d.snapshot().cancelled_orders().get(123).is_none());
}
