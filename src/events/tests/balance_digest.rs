use super::*;
use crate::client::{Client, SendItem, SendPriority};
use crate::commands::balance::{build_balance_digest, parse_balance_digest};
use crate::state::balances::balance_hash_mix;

fn balance_rows(cmd: u8, epoch: u16, rows: &[(&str, u64, f64)]) -> Vec<u8> {
    let mut payload = balance_payload_with_items(cmd, epoch as u64, epoch, &[]);
    payload.truncate(payload.len() - 4);
    payload.extend_from_slice(&(rows.len() as i32).to_le_bytes());
    for (name, hash, profit) in rows {
        write_string(&mut payload, name);
        payload.extend_from_slice(&hash.to_le_bytes());
        payload.extend_from_slice(&(1u32 << 16).to_le_bytes());
        payload.extend_from_slice(&profit.to_le_bytes());
    }
    payload
}

fn receive(
    d: &mut EventDispatcher,
    client: &Client,
    payload: &[u8],
) -> (Vec<Event>, Vec<SendItem>) {
    let mut events = Vec::new();
    let mut actions = Vec::new();
    dispatch_active_packet_for_test(
        d,
        Command::Balance,
        payload,
        1000,
        &mut events,
        client,
        &mut actions,
    );
    apply_active_actions_for_test(client, &mut actions);
    (events, drain_client_send_items(client))
}

fn digest(d: &EventDispatcher) -> u64 {
    d.markets.balance_digest(d.balances.global.balance_hash())
}

#[test]
fn balance_digest_repairs_dropped_update_for_one_client_without_exchange_request() {
    let mut a = Client::new(dummy_client_cfg());
    let mut b = Client::new(dummy_client_cfg());
    a.testing_set_domain_ready(true);
    b.testing_set_domain_ready(true);
    let mut da = EventDispatcher::new();
    let mut db = EventDispatcher::new();
    for d in [&mut da, &mut db] {
        seed_event_markets(d, &["BTCUSDT", "ETHUSDT"]);
    }
    let initial = balance_rows(3, 10, &[("BTCUSDT", 77, -42.4)]);
    receive(&mut da, &a, &initial);
    receive(&mut db, &b, &initial);
    assert_eq!(
        da.markets
            .get("BTCUSDT")
            .unwrap()
            .balance_position()
            .total_profit(),
        -42.4
    );
    assert_eq!(
        da.markets
            .get("BTCUSDT")
            .unwrap()
            .balance_position()
            .pos_size,
        0.0
    );

    // B misses the ETH update. A has exactly the server's current rows.
    let update = balance_rows(4, 11, &[("ETHUSDT", 123, 12.5)]);
    receive(&mut da, &a, &update);
    let server_digest = digest(&da);
    let old_digest = digest(&db);
    assert_ne!(old_digest, server_digest);
    let packet = build_balance_digest(100, 12, server_digest);
    let (events, sent) = receive(&mut da, &a, &packet);
    assert!(events.is_empty());
    assert!(sent.is_empty(), "matching state must not request Full");

    let (events, sent) = receive(&mut db, &b, &packet);
    assert!(events.is_empty(), "repair is internal, not a UI event");
    assert_eq!(sent.len(), 1);
    let request = &sent[0];
    assert_eq!(request.cmd, Command::Balance.to_byte());
    assert_eq!(
        request.data[0], 8,
        "repair must not be manual refresh or Engine API"
    );
    assert_eq!(request.priority, SendPriority::High);
    assert!(request.encrypted);
    assert_eq!(request.max_retries, 3);
    assert!(request.u_key.is_none());
    assert_eq!(parse_balance_digest(&request.data[11..]), (0, old_digest));
    assert!(
        receive(&mut db, &b, &packet).1.is_empty(),
        "duplicate digest"
    );

    // A missing repair response is retried by the next server digest, no new timer.
    let next = build_balance_digest(101, 20, server_digest);
    assert_eq!(receive(&mut db, &b, &next).1.len(), 1);
    let repaired = balance_rows(3, 21, &[("BTCUSDT", 77, -42.4), ("ETHUSDT", 123, 12.5)]);
    let (events, sent) = receive(&mut db, &b, &repaired);
    assert!(matches!(
        events.as_slice(),
        [Event::Balance(BalanceEvent::SnapshotApplied {
            count: 2,
            ..
        })]
    ));
    assert!(sent.is_empty());
    assert_eq!(digest(&db), server_digest);
    assert_eq!(
        db.markets
            .get("ETHUSDT")
            .unwrap()
            .balance_position()
            .total_profit(),
        12.5
    );
    assert!(
        receive(&mut db, &b, &build_balance_digest(102, 22, server_digest))
            .1
            .is_empty()
    );
}

#[test]
fn balance_digest_uses_server_order_and_keeps_unknown_slots() {
    let mut client = Client::new(dummy_client_cfg());
    client.testing_set_domain_ready(true);
    let mut d = EventDispatcher::new();
    seed_event_markets(&mut d, &["ETHUSDT", "BTCUSDT"]);
    receive(
        &mut d,
        &client,
        &balance_rows(3, 10, &[("ETHUSDT", 123, 0.5), ("BTCUSDT", 77, 1.0)]),
    );
    d.markets
        .apply_markets_indexes(vec!["BTCUSDT".into(), "UNKNOWN".into(), "ETHUSDT".into()]);
    let expected = [77, 0, 123]
        .into_iter()
        .fold(d.balances.global.balance_hash(), balance_hash_mix);
    assert_eq!(expected, 0x3ff1_3a94_7fa7_ca9d);
    assert_eq!(digest(&d), expected);
    assert_ne!(
        expected,
        [123, 77]
            .into_iter()
            .fold(d.balances.global.balance_hash(), balance_hash_mix)
    );
}

#[test]
fn balance_digest_default_transition_and_omitted_full_row_converge() {
    let mut client = Client::new(dummy_client_cfg());
    client.testing_set_domain_ready(true);
    let mut d = EventDispatcher::new();
    seed_event_markets(&mut d, &["BTCUSDT"]);
    receive(
        &mut d,
        &client,
        &balance_rows(3, 10, &[("BTCUSDT", 77, -42.4)]),
    );
    receive(
        &mut d,
        &client,
        &balance_rows(4, 11, &[("BTCUSDT", 0, 0.0)]),
    );
    let default_digest = digest(&d);
    assert_eq!(
        d.markets
            .get("BTCUSDT")
            .unwrap()
            .balance_position()
            .total_profit(),
        0.0
    );
    assert!(receive(
        &mut d,
        &client,
        &build_balance_digest(100, 12, default_digest)
    )
    .1
    .is_empty());

    receive(
        &mut d,
        &client,
        &balance_rows(4, 13, &[("BTCUSDT", 77, -42.4)]),
    );
    receive(&mut d, &client, &balance_rows(3, 14, &[]));
    assert_eq!(digest(&d), default_digest);
    assert_eq!(
        d.markets.get("BTCUSDT").unwrap().with(|m| m.balance_hash),
        0
    );
    assert!(receive(
        &mut d,
        &client,
        &build_balance_digest(101, 15, default_digest)
    )
    .1
    .is_empty());
}

#[test]
fn balance_digest_gates_stale_epochs_but_full_recovers_after_restart() {
    let mut client = Client::new(dummy_client_cfg());
    client.testing_set_domain_ready(true);
    let mut d = EventDispatcher::new();
    seed_event_markets(&mut d, &["BTCUSDT"]);
    receive(
        &mut d,
        &client,
        &balance_rows(3, 100, &[("BTCUSDT", 77, -42.4)]),
    );
    for epoch in [99, 100] {
        assert!(
            receive(&mut d, &client, &build_balance_digest(200, epoch, 999))
                .1
                .is_empty()
        );
        assert_eq!(d.balances.last_epoch, 100);
    }
    let matching = digest(&d);
    assert!(
        receive(&mut d, &client, &build_balance_digest(201, 102, matching))
            .1
            .is_empty()
    );
    receive(
        &mut d,
        &client,
        &balance_rows(4, 101, &[("BTCUSDT", 123, 5.0)]),
    );
    assert_eq!(
        d.balances.last_epoch, 102,
        "late increment must not rewind digest watermark"
    );
    assert!(
        receive(&mut d, &client, &build_balance_digest(202, 101, matching))
            .1
            .is_empty()
    );

    receive(&mut d, &client, &balance_rows(3, 1, &[]));
    assert_eq!(
        d.balances.last_epoch, 1,
        "Full resets the old process epoch"
    );
    let matching = digest(&d);
    assert!(
        receive(&mut d, &client, &build_balance_digest(203, 2, matching))
            .1
            .is_empty()
    );

    receive(&mut d, &client, &balance_rows(3, u16::MAX, &[]));
    assert!(
        receive(&mut d, &client, &build_balance_digest(204, 0, matching))
            .1
            .is_empty()
    );
    assert_eq!(d.balances.last_epoch, 0);
}

#[test]
fn balance_digest_respects_init_and_command_version_gates() {
    let mut client = Client::new(dummy_client_cfg());
    let mut d = EventDispatcher::new();
    let packet = build_balance_digest(1, 10, 999);
    assert!(receive(&mut d, &client, &packet).1.is_empty());
    assert_eq!(d.balances.last_epoch, 0);
    client.testing_set_domain_ready(true);
    let mut future = packet.clone();
    future[1..3].copy_from_slice(&(CURRENT_PROTO_CMD_VER + 1).to_le_bytes());
    assert!(receive(&mut d, &client, &future).1.is_empty());
    assert_eq!(d.balances.last_epoch, 0);
    assert_eq!(receive(&mut d, &client, &packet).1.len(), 1);
}
