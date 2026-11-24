use super::*;

fn ping(client: &mut crate::client::Client, offset_minutes: i64) {
    let raw_now = 45_000.0;
    let mut payload = vec![0; crate::protocol::control::PING_SIZE];
    let core_time = raw_now + offset_minutes as f64 / 1440.0;
    payload[8..16].copy_from_slice(&core_time.to_le_bytes());
    // A transport NTP correction must not leak into the timezone estimate.
    assert!(client.apply_ping_and_build_response(&payload, raw_now, raw_now + 1.0, 0, 0).is_some());
}

fn setup() -> (crate::client::Client, EventDispatcher, crate::state::MarketHistoryWorker) {
    let worker = crate::state::MarketHistoryWorker::spawn(crate::state::MarketHistoryConfig {
        futures_trades_capacity: 16, spot_trades_capacity: 8, liquidation_capacity: 8, mm_orders_capacity: 8,
        last_price_capacity: 0, mini_candles_capacity: 0, candles_5m_capacity: 0,
    });
    let mut d = EventDispatcher::new();
    d.set_market_history_handle(worker.handle());
    seed_event_markets(&mut d, &["BTCUSDT"]);
    d.markets.apply_markets_indexes(vec!["BTCUSDT".into()]);
    let mut client = crate::client::Client::new(dummy_client_cfg());
    client.testing_set_domain_ready(true);
    client.subscribe_all_trades(true);
    (client, d, worker)
}

fn packet(base: f64, number: u16) -> Vec<u8> {
    let mut payload = base.to_le_bytes().to_vec();
    payload.extend_from_slice(&number.to_le_bytes());
    // Unknown sections must not determine the shift or corrupt following rows.
    push_trades_section(&mut payload, 0, 99, &[(-32760, 99.0, 1.0)]);
    push_trades_section(&mut payload, 0, 0, &[(-32760, 100.0, 1.0), (0, 101.0, 2.0)]);
    push_trades_section(&mut payload, 2, 0, &[(0, 102.0, 1.0)]);
    push_liquidation_section(&mut payload, 0, &[(0, 103.0, 1.0)]);
    push_trades_section(&mut payload, 1, 0, &[(0, 104.0, 1.0)]);
    push_watcher_fills_section(&mut payload, 0, [7; 20], &[(0, 105.0, 1.0, 0.0, 0.0, 0, 0)]);
    payload.push(0);
    payload
}

#[test]
fn ping_timezone_keeps_live_resend_and_all_history_sections_on_utc() {
    let _clock_lock = server_time_delta_test_lock();
    for offset in [0, 330, -345] {
        for delay_ms in [0, 180_000, 600_000] {
            for command in [Command::TradesStream, Command::TradesResendResponse] {
                let (mut client, mut d, worker) = setup();
                ping(&mut client, offset);
                let mut other = crate::client::Client::new(dummy_client_cfg());
                ping(&mut other, offset + 180);
                let mut ctx = ActiveDispatchContext::from_client(&client);
                assert_eq!(ctx.server_utc_offset_minutes, Some(offset));
                ctx.now_time_days = 45_000.0 + delay_ms as f64 / DELPHI_MSECS_PER_DAY;
                let packet = packet(45_000.0 + offset as f64 / 1440.0, 10);
                let resend = trades_resend_response_payload(&packet);
                let bytes = if command == Command::TradesStream { &packet } else { &resend };
                let mut out = Vec::new();
                d.dispatch_into_active_actions(command, bytes, 7000, &mut out, &ctx, &mut Vec::new());
                // A later Ping cannot change the batch already queued for history.
                ping(&mut client, offset + 60);
                assert!(worker.flush(mt(ctx.now_time_days)));
                let readers = worker.readers("BTCUSDT").unwrap();
                let mut futures = Vec::new();
                readers.futures_trades.unwrap().copy_last(16, &mut futures);
                assert_eq!(futures.len(), 2);
                assert_eq!(futures[0].time, mt(45_000.0 - 32760.0 / DELPHI_MSECS_PER_DAY));
                assert_eq!(futures[1].time, mt(45_000.0));
                assert_eq!(readers.spot_trades.unwrap().read_at_seq(0).unwrap().time, mt(45_000.0));
                assert_eq!(readers.liquidations.unwrap().read_at_seq(0).unwrap().time, mt(45_000.0));
                assert_eq!(readers.mm_orders.unwrap().read_at_seq(0).unwrap().time, mt(45_000.0));
                let fill = out.iter().find_map(|event| match event {
                    Event::WatcherFills(event) => Some(&event.fills[0]), _ => None,
                }).unwrap();
                assert_eq!(mt(fill.time), mt(45_000.0));
                out.clear();
                let mut next_ctx = ActiveDispatchContext::from_client(&client);
                next_ctx.now_time_days = ctx.now_time_days;
                d.dispatch_into_active_actions(Command::TradesResendResponse, &resend, 7100,
                    &mut out, &next_ctx, &mut Vec::new());
                assert!(worker.flush(mt(ctx.now_time_days)));
                assert!(!out.iter().any(|event| matches!(event, Event::WatcherFills(_))));
                futures.clear();
                worker.readers("BTCUSDT").unwrap().futures_trades.unwrap().copy_last(16, &mut futures);
                assert_eq!(futures.len(), 2);
            }
        }
    }
}

#[test]
fn pre_ping_fallback_and_next_session_use_their_own_clock() {
    let _clock_lock = server_time_delta_test_lock();
    let (mut client, mut d, worker) = setup();
    let mut other = crate::client::Client::new(dummy_client_cfg());
    ping(&mut other, 180);
    let mut ctx = ActiveDispatchContext::from_client(&client);
    assert_eq!(ctx.server_utc_offset_minutes, None);
    ctx.now_time_days = 45_000.0;
    ctx.server_token = 1;
    let mut out = Vec::new();
    d.dispatch_into_active_actions(Command::TradesStream, &packet(45_000.0, 10), 1000,
        &mut out, &ctx, &mut Vec::new());
    assert!(worker.flush(mt(ctx.now_time_days)));
    assert_eq!(worker.readers("BTCUSDT").unwrap().futures_trades.unwrap().read_at_seq(1).unwrap().time, mt(45_000.0));
    ping(&mut client, 345);
    ctx = ActiveDispatchContext::from_client(&client);
    ctx.now_time_days = 45_000.01;
    ctx.server_token = 2;
    out.clear();
    d.dispatch_into_active_actions(Command::TradesStream, &packet(45_000.01 + 345.0 / 1440.0, 10), 2000,
        &mut out, &ctx, &mut Vec::new());
    assert!(worker.flush(mt(ctx.now_time_days)));
    assert_eq!(worker.readers("BTCUSDT").unwrap().futures_trades.unwrap().read_at_seq(3).unwrap().time, mt(45_000.01));
    let fresh = crate::client::Client::new(dummy_client_cfg());
    assert_eq!(ActiveDispatchContext::from_client(&fresh).server_utc_offset_minutes, None);
}

#[test]
fn pre_ping_skipped_storage_and_watcher_fill_share_the_packet_shift() {
    let (_, mut d, worker) = setup();
    d.set_trade_storage_intent(Some(&crate::client::TradeStorageIntent {
        scope: Arc::new(TradeStorageScope::All), retain_mm_orders: false,
    }), 45_000.0);
    let mut payload = 45_000.0f64.to_le_bytes().to_vec();
    payload.extend_from_slice(&10u16.to_le_bytes());
    // This visible MM section is not retained, but supplies the fallback shift.
    push_trades_section(&mut payload, 1, 0, &[(0, 1.0, 1.0)]);
    push_trades_section(&mut payload, 0, 0, &[(30000, 100.0, 1.0)]);
    push_watcher_fills_section(&mut payload, 0, [7; 20], &[(30000, 100.0, 1.0, 0.0, 0.0, 0, 0)]);
    payload.push(0);
    let now = 45_000.0 + 160_000.0 / DELPHI_MSECS_PER_DAY;
    let mut out = Vec::new();
    d.client_new_data_trades_stream(&payload, 1000, Some(now), None, &mut out);
    assert!(worker.flush(mt(now)));
    let time = worker.readers("BTCUSDT").unwrap().futures_trades.unwrap().read_at_seq(0).unwrap().time;
    assert_eq!(time, mt(45_000.0 + 330_000.0 / DELPHI_MSECS_PER_DAY));
    let fill = out.iter().find_map(|event| match event {
        Event::WatcherFills(event) => Some(&event.fills[0]), _ => None,
    }).unwrap();
    assert_eq!(mt(fill.time), time);
}
