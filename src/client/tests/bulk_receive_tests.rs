use super::*;
use crate::commands::candles::parse_request_candles_data_response;
use crate::commands::engine_api::{parse_engine_response, parse_engine_response_owned};
use crate::protocol::payload::Payload;
use crate::protocol::slicing::{SliceHeader, SlicingReceiver};
use std::io::Write;

fn archive(name: &str) -> Vec<u8> {
    let mut raw = Vec::new();
    raw.extend_from_slice(&0i32.to_le_bytes());
    raw.push(2);
    raw.extend_from_slice(&1i32.to_le_bytes());
    raw.extend_from_slice(&180f64.to_le_bytes());
    raw.extend_from_slice(&(name.len() as u16).to_le_bytes());
    for c in name.encode_utf16() {
        raw.extend_from_slice(&c.to_le_bytes());
    }
    raw.extend_from_slice(&1i32.to_le_bytes());
    for value in [101f32, 99f32, 12.5f32] {
        raw.extend_from_slice(&value.to_le_bytes());
    }
    raw.extend_from_slice(&45_000.25f64.to_le_bytes());
    raw.extend_from_slice(&[0; 64]);
    let mut encoder = flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::default());
    encoder.write_all(&raw).unwrap();
    encoder.finish().unwrap()
}

fn chunk(index: usize, total: usize, bytes: &[u8]) -> Vec<u8> {
    let mut out = Vec::new();
    out.extend_from_slice(&(index as u16).to_le_bytes());
    out.extend_from_slice(&(total as u16).to_le_bytes());
    out.extend_from_slice(bytes);
    out
}

fn reassemble(encrypted: &[u8], reverse: bool) -> Payload {
    // Deliberately unequal fragment lengths, including an empty fragment.
    let first = [vec![Command::Crypted.to_byte()], encrypted[..1].to_vec()].concat();
    let parts = [&first[..], &encrypted[1..17], &[][..], &encrypted[17..]];
    let order = if reverse { [3, 2, 1, 0] } else { [0, 1, 2, 3] };
    let mut receiver = SlicingReceiver::new();
    receiver.set_last_online(10_000);
    let mut completed = None;
    for (position, index) in order.into_iter().enumerate() {
        let mut packet = Vec::new();
        SliceHeader {
            datagram_num: 9,
            block_num: index as u8,
            max_block_num: 3,
        }
        .write_to(&mut packet);
        packet.extend_from_slice(parts[index]);
        let (result, ack) = receiver.on_new_sliced(&packet);
        assert_ne!(ack[0] & (1 << index), 0);
        if position == 3 {
            completed = result;
        } else {
            assert!(result.is_none());
        }
        // Retransmission both before and after completion must not deliver twice.
        assert!(receiver.on_new_sliced(&packet).0.is_none());
    }
    let (_, cmd, payload, _, _) = completed.unwrap();
    assert_eq!(cmd, Command::Crypted.to_byte());
    assert_eq!(&*payload, encrypted);
    payload
}

#[test]
fn encrypted_chunk_permutations_duplicates_and_parallel_uids() {
    let archives = [archive("BTCUSDT"), archive("ETHUSDT")];
    let expected = archives
        .each_ref()
        .map(|a| parse_request_candles_data_response(a).unwrap());
    for a in 0..4 {
        for b in 0..4 {
            for c in 0..4 {
                for d in 0..4 {
                    let order = [a, b, c, d];
                    if (0..4).any(|i| order[i + 1..].contains(&order[i])) {
                        continue;
                    }
                    let mut client = Client::new(dummy_cfg());
                    let key = install_server_decode_session(&mut client, 42);
                    client.authorized = true;
                    client.auth_status = AuthStatus::AuthDone;
                    let requests = [
                        client.api_request_candles_data_async_registered(),
                        client.api_request_candles_data_async_registered(),
                    ];
                    let mut dispatcher = EventDispatcher::new();
                    let mut mode = RunMode::new(&mut dispatcher);
                    let mut sequence = 0;
                    for (position, index) in order.into_iter().enumerate() {
                        for (request, (uid, _, progress)) in requests.iter().enumerate() {
                            let bytes = &archives[request];
                            let bounds = [0, 0, 1, bytes.len() / 2, bytes.len()];
                            let body = chunk(index, 4, &bytes[bounds[index]..bounds[index + 1]]);
                            let response = build_engine_response_payload(*uid, EngineMethod::RequestCandlesData, &body);
                            sequence += 1;
                            let encrypted = build_server_crypted_payload(&key, sequence, Command::API, &response);
                            let payload = reassemble(&encrypted, request == 1);
                            ProtocolCore { client: &mut client }.dispatch_command_owned(
                                Command::Crypted.to_byte(),
                                payload,
                                0,
                                10,
                                false,
                                None,
                                &mut mode,
                            );
                            assert_eq!(progress.generation(), position as u64 + 1);
                            assert!(
                                Client::decode_command_payload_owned(
                                    &mut client.recv.data_read_state,
                                    Command::Crypted.to_byte(),
                                    encrypted
                                )
                                .is_none(),
                                "same encrypted message is a replay"
                            );
                            if position < 3 {
                                // Fresh encryption of the same chunk index with different data:
                                // first arrival wins and duplicates do not refresh progress.
                                sequence += 1;
                                let repeated = build_engine_response_payload(
                                    *uid,
                                    EngineMethod::RequestCandlesData,
                                    &chunk(index, 4, b"wrong"),
                                );
                                ProtocolCore { client: &mut client }.dispatch_command_owned(
                                    Command::Crypted.to_byte(),
                                    build_server_crypted_payload(&key, sequence, Command::API, &repeated),
                                    0,
                                    20,
                                    false,
                                    None,
                                    &mut mode,
                                );
                                assert_eq!(progress.generation(), position as u64 + 1);
                            }
                        }
                    }
                    for ((uid, receiver, _), expected) in requests.into_iter().zip(&expected) {
                        let result = receiver.recv_timeout(Duration::from_secs(2)).unwrap();
                        assert_eq!(result.uid, uid);
                        assert_eq!(&result.markets, expected);
                        assert!(receiver.try_recv().is_err());
                        let late =
                            build_engine_response_payload(uid, EngineMethod::RequestCandlesData, &chunk(0, 1, b"late"));
                        let mut late = Payload::from(late);
                        let ptr = late.as_ptr();
                        assert!(!Client::dispatch_chunked_api_response(
                            &mut client.pending_api,
                            Command::API.to_byte(),
                            &mut late
                        ));
                        assert_eq!(
                            late.as_ptr(),
                            ptr,
                            "unregistered responses keep their ordinary delivery buffer"
                        );
                    }
                    assert!(client.pending_api.pending_candles.is_empty());
                }
            }
        }
    }
}

#[test]
fn owned_decrypt_and_response_keep_allocation_and_reject_corruption_before_replay() {
    let mut client = Client::new(dummy_cfg());
    let key = install_server_decode_session(&mut client, 42);
    let body = chunk(0, 1, &archive("BTCUSDT"));
    let response = build_engine_response_payload(123, EngineMethod::RequestCandlesData, &body);
    let encrypted = build_server_crypted_payload(&key, 7, Command::API, &response);
    let mut corrupt = encrypted.clone();
    corrupt[12] ^= 1;
    assert!(Client::decode_command_payload_owned(
        &mut client.recv.data_read_state,
        Command::Crypted.to_byte(),
        corrupt
    )
    .is_none());
    let ptr = encrypted.as_ptr();
    let (_, mut decoded) =
        Client::decode_command_payload_owned(&mut client.recv.data_read_state, Command::Crypted.to_byte(), encrypted)
            .unwrap();
    assert_eq!(decoded.as_ptr(), ptr.wrapping_add(28 + 12));
    assert_eq!(&*decoded, &response);
    let (header, owned_body) = parse_engine_response_owned(&mut decoded).unwrap();
    assert_eq!(header.request_uid, 123);
    assert_eq!(&*owned_body, &body);
    assert_eq!(owned_body.as_ptr(), ptr.wrapping_add(28 + 12 + 32));
    assert!(decoded.is_empty());
}

#[test]
fn owned_response_matches_borrowed_for_compression_truncation_and_errors() {
    let body = b"response body".repeat(32);
    for compressed in [false, true] {
        let bytes = if compressed { deflate_raw(&body) } else { body.clone() };
        let mut wire = build_engine_response_payload(123, EngineMethod::RequestCandlesData, &bytes);
        wire[27] = u8::from(compressed);
        for len in 0..=wire.len() {
            let input = wire[..len].to_vec();
            let reference = parse_engine_response(&input);
            let mut owned = Payload::from(input.clone());
            let result = parse_engine_response_owned(&mut owned);
            assert_eq!(
                reference.is_some(),
                result.is_some(),
                "length={len} compressed={compressed}"
            );
            if let Some((header, data)) = result {
                let reference = reference.unwrap();
                assert_eq!(&*data, &reference.data);
                assert_eq!(header.request_uid, reference.request_uid);
                assert_eq!(header.success, reference.success);
            } else {
                assert_eq!(&*owned, &input);
            }
        }
    }
}

#[test]
fn owned_decode_preserves_outer_compression_and_inner_response_compression() {
    let mut client = Client::new(dummy_cfg());
    let key = install_server_decode_session(&mut client, 42);
    let body = b"compressed body".repeat(8192);
    let mut encoder = flate2::write::DeflateEncoder::new(Vec::new(), flate2::Compression::none());
    encoder.write_all(&body).unwrap();
    let inner = encoder.finish().unwrap();
    let mut response = build_engine_response_payload(123, EngineMethod::RequestCandlesData, &inner);
    response[27] = 1;
    let compressed = crate::compression::mp_compress(&response).unwrap();
    let mut plaintext = Vec::new();
    plaintext.extend_from_slice(&0u16.to_le_bytes());
    plaintext.extend_from_slice(&8u64.to_le_bytes());
    plaintext.push(Command::API.to_byte() | COMPRESSED_FLAG);
    plaintext.push(0);
    plaintext.extend_from_slice(&compressed);
    let encrypted = crypto::encrypt_with_cipher(&crypto::cipher_from_key(&key), &plaintext, &[]);
    let (cmd, mut decoded) =
        Client::decode_command_payload_owned(&mut client.recv.data_read_state, Command::Crypted.to_byte(), encrypted)
            .unwrap();
    assert_eq!(cmd, Command::API.to_byte());
    assert_eq!(&*decoded, &response);
    let (_, parsed) = parse_engine_response_owned(&mut decoded).unwrap();
    assert_eq!(&*parsed, &body);
}

#[test]
fn failed_chunked_responses_release_pending_requests() {
    let mut client = Client::new(dummy_cfg());
    let (candles_uid, candles_rx, candles_progress) = client.api_request_candles_data_async_registered();
    let (history_uid, history_rx, history_progress) = client.api_request_market_history_async_registered("BTCUSDT");
    for (uid, method) in [
        (candles_uid, EngineMethod::RequestCandlesData),
        (history_uid, EngineMethod::RequestMarketHistory),
    ] {
        let mut first = build_engine_response_payload(uid, method, &chunk(0, 2, b"unfinished")).into();
        assert!(Client::dispatch_chunked_api_response(
            &mut client.pending_api,
            Command::API.to_byte(),
            &mut first
        ));
        let mut failed = build_engine_response_payload(uid, method, &[]);
        failed[20] = 0;
        failed[21..25].copy_from_slice(&499i32.to_le_bytes());
        assert!(Client::dispatch_chunked_api_response(
            &mut client.pending_api,
            Command::API.to_byte(),
            &mut failed.into()
        ));
    }
    assert!(client.pending_api.pending_candles.is_empty());
    assert!(client.pending_api.pending_market_history.is_empty());
    assert_eq!(candles_progress.generation(), 1);
    assert_eq!(history_progress.generation(), 1);
    assert!(candles_rx.recv_timeout(Duration::from_secs(1)).is_err());
    assert!(history_rx
        .recv_timeout(Duration::from_secs(1))
        .unwrap()
        .unwrap_err()
        .contains("499"));
}

#[test]
fn chunk_reader_restarts_for_partial_candles_fallback() {
    use std::io::Read;
    let mut raw = Vec::new();
    flate2::read::ZlibDecoder::new(archive("BTCUSDT").as_slice())
        .read_to_end(&mut raw)
        .unwrap();
    raw.truncate(raw.len() - 8); // Complete candle, truncated wall tail.
    let mut encoder = flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::default());
    encoder.write_all(&raw).unwrap();
    let bytes = encoder.finish().unwrap();
    assert!(parse_request_candles_data_response(&bytes).is_none());
    let mut client = Client::new(dummy_cfg());
    let (uid, rx, _) = client.api_request_candles_data_async_registered();
    let split = bytes.len() / 2;
    for (index, part) in [(1, &bytes[split..]), (0, &bytes[..split])] {
        let response = build_engine_response_payload(uid, EngineMethod::RequestCandlesData, &chunk(index, 2, part));
        assert!(Client::dispatch_chunked_api_response(
            &mut client.pending_api,
            Command::API.to_byte(),
            &mut response.into()
        ));
    }
    let result = rx.recv_timeout(Duration::from_secs(1)).unwrap();
    assert_eq!(result.markets.len(), 1);
    assert_eq!(result.markets[0].market_name, "BTCUSDT");
    assert_eq!(result.markets[0].candles_5m.len(), 1);
}


#[test]
fn manual_stats_response_survives_compression_sliced_reordering_and_duplicates() {
    let mut body = 64i32.to_le_bytes().to_vec();
    for index in 0..64u16 {
        body.extend_from_slice(&index.to_le_bytes());
        for value in [24.5f64, 1.25] {
            body.extend_from_slice(&(value * f64::from(index)).to_le_bytes());
        }
        for value in [1.5f32, 3.25, 5.75] {
            body.extend_from_slice(&(value * f32::from(index)).to_le_bytes());
        }
        for value in [0.125f64, 1.25, 2.5, 3.75, 24.5] {
            body.extend_from_slice(&(value * f64::from(index)).to_le_bytes());
        }
        body.extend_from_slice(&i32::from(index).to_le_bytes());
    }
    for compressed in [false, true] {
        let mut client = Client::new(dummy_cfg());
        let key = install_server_decode_session(&mut client, 42);
        let data = if compressed { deflate_raw(&body) } else { body.clone() };
        let mut response = build_engine_response_payload(123, EngineMethod::GetMarketsStats, &data);
        response[27] = u8::from(compressed);
        let wire = build_server_crypted_payload(&key, 1, Command::API, &response);
        let assembled = reassemble(&wire, true);
        let (_, decoded) = Client::decode_command_payload_owned(
            &mut client.recv.data_read_state, Command::Crypted.to_byte(), assembled.into_vec(),
        ).unwrap();
        let parsed = parse_engine_response(&decoded).unwrap();
        assert_eq!(parsed.method, EngineMethod::GetMarketsStats);
        assert_eq!(parsed.data, body);
        assert!(Client::decode_command_payload_owned(
            &mut client.recv.data_read_state, Command::Crypted.to_byte(), wire,
        ).is_none(), "replayed encrypted stats must not be delivered again");
    }
}

#[test]
fn startup_snapshot_survives_sliced_reordering_and_duplicate_blocks() {
    let mut client = Client::new(dummy_cfg());
    let key = install_server_decode_session(&mut client, 42);
    let body = crate::commands::engine_api::startup_state_fixture();
    let response = build_engine_response_payload(456, EngineMethod::GetStartupState, &body);
    let wire = build_server_crypted_payload(&key, 1, Command::API, &response);
    let assembled = reassemble(&wire, true);
    let (_, decoded) = Client::decode_command_payload_owned(
        &mut client.recv.data_read_state, Command::Crypted.to_byte(), assembled.into_vec(),
    ).unwrap();
    let parsed = parse_engine_response(&decoded).unwrap();
    assert_eq!(parsed.method, EngineMethod::GetStartupState);
    let snapshot = crate::commands::engine_api::parse_startup_state(&parsed.data).unwrap();
    assert_eq!(snapshot.profit.rep_total_profit, 123.5);
    assert_eq!(snapshot.hyperliquid.requests_left, Some(321));
    assert!(Client::decode_command_payload_owned(
        &mut client.recv.data_read_state, Command::Crypted.to_byte(), wire,
    ).is_none(), "replayed encrypted snapshot must not be delivered again");
}
