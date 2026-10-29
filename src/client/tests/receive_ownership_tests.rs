use super::*;
use crate::client::protocol_core::ProtocolCore;
use crate::protocol::payload::Payload;

#[test]
fn compressed_response_keeps_one_decoded_body_through_pending_and_active_delivery() {
    for registered in [false, true] {
        let mut client = Client::new(dummy_cfg());
        let uid = 123;
        let receiver = registered.then(|| client.pending_api.api_pending.register(uid));
        let names = vec!["BTCUSDT".to_owned(), "ETHUSDT".to_owned()];
        let body = build_markets_indexes_response(&names);
        let mut wire = build_engine_response_payload(uid, EngineMethod::GetMarketsIndexes, &deflate_raw(&body));
        wire[27] = 1;
        let mut payload = DecodedPayload::from(wire);
        let consumed = Client::dispatch_api_pending_decoded(&client.pending_api.api_pending, Command::API.to_byte(), &mut payload);
        assert_eq!(consumed, registered);
        let mut payloads = Vec::new();
        client.client_new_data_decoded(Command::API.to_byte(), payload, consumed, false, &mut payloads);
        let (_, mut payload) = payloads.pop().unwrap();
        let response = payload.parsed_api.as_ref().unwrap().as_ref().unwrap();
        assert_eq!(response.data, body);
        let body_ptr = response.data.as_ptr();
        if let Some(receiver) = receiver {
            let pending = receiver.try_recv().unwrap();
            assert_eq!(pending.data, body);
            assert_ne!(pending.data.as_ptr(), body_ptr, "the waiting receiver owns its independent response");
        }
        let mut dispatcher = EventDispatcher::new();
        let mut out = Vec::new();
        let parsed_api = payload.parsed_api.take();
        dispatcher.dispatch_into_active_actions_with_response(Command::API, &payload, 123, &mut out,
            &crate::events::ActiveDispatchContext::from_client(&client), &mut Vec::new(), parsed_api);
        assert_eq!(dispatcher.markets.market_index_names(), names);
        let response = out.iter().find_map(|event| match event {
            crate::events::Event::EngineResponse(response) => Some(response),
            _ => None,
        }).unwrap();
        assert_eq!(response.data.as_ptr(), body_ptr, "active dispatch must consume the existing parse result");
    }
}

#[test]
fn malformed_compressed_response_keeps_waiter_and_cached_failure() {
    let mut client = Client::new(dummy_cfg());
    let receiver = client.pending_api.api_pending.register(123);
    let mut wire = build_engine_response_payload(123, EngineMethod::GetMarketsIndexes, &[0x06]);
    wire[27] = 1; // reserved DEFLATE block type
    let mut payload = DecodedPayload::from(wire);
    assert!(!Client::dispatch_api_pending_decoded(&client.pending_api.api_pending, Command::API.to_byte(), &mut payload));
    assert!(matches!(payload.parsed_api, Some(None)));
    let mut payloads = Vec::new();
    client.client_new_data_decoded(Command::API.to_byte(), payload, false, false, &mut payloads);
    assert!(matches!(payloads[0].1.parsed_api, Some(None)));
    assert!(matches!(receiver.try_recv(), Err(std::sync::mpsc::TryRecvError::Empty)));
}

#[test]
fn ordinary_udp_retains_storage_through_transport_crypto_and_delivery() {
    let mut client = Client::new(dummy_cfg());
    let key = install_server_decode_session(&mut client, 42);
    let wire = build_server_crypted_payload(&key, 1, Command::UI, &[0; 64]);
    let packet = pack_server_packet(&client.cfg.mac_key, Command::Crypted, &wire);
    let (header, payload) = crate::transport::transport_unpack_with_mac(&client.transport.mac_ctx, &packet, 0).unwrap();
    let expected_ptr = payload.as_ptr().wrapping_add(12 + 16 + crate::protocol::crypted::CRYPTO_HEADER_SIZE);
    let (cmd, payload) = Client::decode_command_payload_owned(&mut client.recv.data_read_state, header.cmd, payload).unwrap();
    assert_eq!(payload.as_ptr(), expected_ptr);
    let mut delivered = Vec::new();
    client.client_new_data_decoded(cmd, payload, false, false, &mut delivered);
    assert_eq!(delivered[0].1.as_ptr(), expected_ptr, "no compaction or second buffer on delivery");
    assert_eq!(&*delivered[0].1, &[0; 64]);
    // The same authenticated message must still be rejected by the replay slider.
    assert!(Client::decode_command_payload_owned(&mut client.recv.data_read_state, header.cmd, wire.clone()).is_none());
    let mut corrupt = wire;
    corrupt[12] ^= 1;
    assert!(Client::decode_command_payload_owned(&mut client.recv.data_read_state, header.cmd, corrupt).is_none());
}

#[test]
fn grouped_reordered_duplicates_and_truncated_tail_keep_delivery_order() {
    let mut client = Client::new(dummy_cfg());
    client.authorized = true;
    client.auth_status = AuthStatus::AuthDone;
    let key = install_server_decode_session(&mut client, 42);
    let response = |names: &[&str]| build_engine_response_payload(0, EngineMethod::GetMarketsIndexes,
        &build_markets_indexes_response(&names.iter().map(|name| name.to_string()).collect::<Vec<_>>()));
    let first = build_server_crypted_payload(&key, 1, Command::API, &response(&["A", "B"]));
    let second = build_server_crypted_payload(&key, 2, Command::API, &response(&["B", "A"]));
    let mut grouped = Vec::new();
    for part in [&second, &first, &second] {
        grouped.push(Command::Crypted.to_byte());
        grouped.extend_from_slice(&(part.len() as u16).to_le_bytes());
        grouped.extend_from_slice(part);
    }
    grouped.extend_from_slice(&[Command::Crypted.to_byte(), 255, 255, 0]);
    let packet = pack_server_packet(&client.cfg.mac_key, Command::Grouped, &grouped);
    let mut dispatcher = EventDispatcher::new();
    let mut mode = RunMode::new(&mut dispatcher);
    assert!((ProtocolCore { client: &mut client }).process_datagram(&packet, packet.len() as u64, &mut mode));
    assert_eq!(mode.dispatcher.markets.market_index_names(), ["A", "B"]);
}

#[test]
fn payload_offsets_compose_without_changing_visible_bytes() {
    let mut payload = Payload::from((0..128u8).collect::<Vec<_>>());
    let pointer = payload.as_ptr();
    payload.restrict(7..120);
    payload.restrict(40..100);
    assert_eq!(payload.as_ptr(), pointer.wrapping_add(47));
    assert_eq!(&*payload, &(47..107u8).collect::<Vec<_>>());
}
