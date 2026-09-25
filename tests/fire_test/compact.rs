use super::*;
use moonproto::{NewOrderParams, OrderSide, StopSettings};

fn has_history(session: &Session, name: &str) -> bool {
    let snapshot = session.state_snapshot();
    snapshot.markets().get(name).and_then(|market| snapshot.market_history_readers_for(&market)).is_some()
}

fn forget_all(cfg: &FireConfig, controller: &mut Session, station: &mut Session) {
    station.client.streams().unsubscribe_all_trades().unwrap();
    assert!(pump_pair_until_sessions(controller, station, cfg.connect_timeout, "release histories", |_, s| {
        s.client.active_subscriptions().all_trades.is_none()
            && firetest_retained_markets(cfg).iter().all(|name| !has_history(s, name))
    }));
}

fn select_and_capture(
    cfg: &FireConfig,
    controller: &mut Session,
    station: &mut Session,
    selected: &[&str],
    market: &str,
) {
    station.take_market_history_events();
    station.client.streams().subscribe_trades_for(TradesStreamMode::TradesOnly, selected.iter().copied()).unwrap();
    // Deliberately no pump, snapshot read, or readiness wait between these calls.
    let ticket = station.client.history().request_chart(market).unwrap();
    assert!(pump_pair_until_sessions(controller, station, cfg.candles_timeout, "compact chart", |_, s| {
        s.market_history_events.iter().any(|event| market_history_event_id(event) == ticket.id())
    }));
    let events = station.take_market_history_events();
    let event = events.iter().find(|event| market_history_event_id(event) == ticket.id()).unwrap();
    match event {
        MarketHistoryEvent::Ready { summary, .. } => {
            assert_market_history_layout(station, market, market_live_ask(station, market).unwrap(), *summary);
        }
        MarketHistoryEvent::Failed { error, .. } => panic!("compact chart {market}: {error}"),
    }
    let snapshot = station.state_snapshot();
    let handle = snapshot.markets().get(market).unwrap();
    let readers = snapshot.market_history_readers_for(&handle).unwrap();
    assert_eq!(readers.futures_trades.as_ref().unwrap().capacity(), 5_000);
    assert_eq!(readers.last_prices.as_ref().unwrap().capacity(), 1_000);
    assert!(readers.mm_orders.is_none());
    assert!(readers.candles_5m.is_none());
}

fn live_packets(session: &Session) -> u64 {
    session.client.protocol_metrics_snapshot().trades_stream_recv_count
}

fn wait_wire_quiet(cfg: &FireConfig, station: &mut Session) {
    let mut packets = live_packets(station);
    let mut quiet_since = Instant::now();
    assert!(pump_session_until(station, cfg.connect_timeout, "no live trades on wire for 2s", |s| {
        let current = live_packets(s);
        if current != packets {
            packets = current;
            quiet_since = Instant::now();
        }
        quiet_since.elapsed() >= Duration::from_secs(2)
    }));
    assert!(station.client.active_subscriptions().all_trades.is_none());
    assert!(firetest_retained_markets(cfg).iter().all(|name| !has_history(station, name)));
}

#[test]
#[ignore = "live MoonBot required; injects stale stream-control requests, no trading"]
fn fire_test_compact_subscription_recovery() {
    let _lock = firetest_live_test_lock();
    let cfg = FireConfig::load_required();
    let keys = parse_key_info(&cfg.key_b64).expect("invalid FireTest key").keys;
    let _loss = ErrEmuGuard::set(0);
    let mut station =
        Session::connect_with_market_history("Capture-races", &cfg, keys, None, MarketHistorySizing::Compact);
    let initial = live_packets(&station);
    assert!(pump_session_until(&mut station, cfg.connect_timeout, "initial live trades", |s| {
        live_packets(s) > initial && has_history(s, &cfg.market)
    }));
    let original_session = station.snapshot();

    let off = engine_response_count(&station, EngineMethod::UnsubscribeAllTrades);
    station.client.streams().unsubscribe_all_trades().unwrap();
    assert!(pump_session_until(&mut station, cfg.connect_timeout, "explicit Off applied by core", |s| {
        engine_response_count(s, EngineMethod::UnsubscribeAllTrades) > off
    }));
    wait_wire_quiet(&cfg, &mut station);

    // Deliver an old Subscribe AFTER the core acknowledged Off, without changing desired state.
    let before = live_packets(&station);
    let off = engine_response_count(&station, EngineMethod::UnsubscribeAllTrades);
    let on = engine_response_count(&station, EngineMethod::SubscribeAllTrades);
    station.client.debug_send_trades_subscription(true).unwrap();
    assert!(pump_session_until(&mut station, cfg.connect_timeout, "late Subscribe really enabled remote feed", |s| {
        live_packets(s) > before && engine_response_count(s, EngineMethod::SubscribeAllTrades) > on
    }));
    assert!(pump_session_until(&mut station, cfg.connect_timeout, "automatic Off repair acknowledged", |s| {
        engine_response_count(s, EngineMethod::UnsubscribeAllTrades) > off
    }));
    wait_wire_quiet(&cfg, &mut station);

    station.client.streams().subscribe_trades_for(TradesStreamMode::TradesOnly, [&cfg.market]).unwrap();
    let before = live_packets(&station);
    assert!(pump_session_until(&mut station, cfg.connect_timeout, "explicit On live", |s| {
        live_packets(s) > before && has_history(s, &cfg.market)
    }));
    let on = engine_response_count(&station, EngineMethod::SubscribeAllTrades);
    let off = engine_response_count(&station, EngineMethod::UnsubscribeAllTrades);
    station.client.debug_send_trades_subscription(false).unwrap();
    assert!(pump_session_until(&mut station, cfg.connect_timeout, "late Unsubscribe applied by core", |s| {
        engine_response_count(s, EngineMethod::UnsubscribeAllTrades) > off
    }));
    // Drain any datagrams already in flight, then prove this is a real outage.
    station.pump(Duration::from_secs(1));
    let stopped = live_packets(&station);
    station.pump(Duration::from_secs(2));
    assert_eq!(live_packets(&station), stopped, "stale Off did not actually stop the core");
    let readers = station.state_snapshot().market_history_readers(&cfg.market).unwrap();
    let ring = readers.futures_trades.unwrap();
    let seq = ring.cursor_from_now().next_seq();
    assert!(pump_session_until(
        &mut station,
        cfg.connect_timeout,
        "same-session silence repair and fresh retained trades",
        |s| {
            engine_response_count(s, EngineMethod::SubscribeAllTrades) > on
                && live_packets(s) > stopped
                && ring.cursor_from_now().next_seq() > seq
        }
    ));
    drop(ring);
    assert_eq!(station.snapshot().connected_again, original_session.connected_again);
    assert_eq!(station.snapshot().connected_fresh, original_session.connected_fresh);

    // Keep the old request off the wire until its capture has been forgotten and recreated.
    station.take_market_history_events();
    station.client.debug_set_outgoing_blackhole(true).unwrap();
    let old_chart = station.client.history().request_chart(&cfg.market).unwrap();
    station.client.streams().unsubscribe_all_trades().unwrap();
    station.client.streams().subscribe_trades_for(TradesStreamMode::TradesOnly, [&cfg.market]).unwrap();
    let new_chart = station.client.history().request_chart(&cfg.market).unwrap();
    station.client.debug_set_outgoing_blackhole(false).unwrap();
    assert!(pump_session_until(&mut station, cfg.candles_timeout, "forget/reselect while chart is in flight", |s| {
        s.market_history_events
            .iter()
            .any(|event| matches!(event, MarketHistoryEvent::Failed { ticket, .. } if *ticket == old_chart))
            && s.market_history_events
                .iter()
                .any(|event| matches!(event, MarketHistoryEvent::Ready { ticket, .. } if *ticket == new_chart))
    }));
    for event in station.take_market_history_events() {
        if let MarketHistoryEvent::Ready { ticket, summary } = event {
            assert_eq!(ticket, new_chart, "cancelled archive was applied after reselect");
            assert_market_history_layout(
                &station,
                &cfg.market,
                market_live_ask(&station, &cfg.market).unwrap(),
                summary,
            );
        }
    }

    // Rapid toggles end with Off. No sleeps between intents: exercise the real runtime queue.
    for _ in 0..12 {
        station.client.streams().unsubscribe_all_trades().unwrap();
        station.client.streams().subscribe_trades_for(TradesStreamMode::TradesOnly, [&cfg.market]).unwrap();
    }
    station.client.streams().unsubscribe_all_trades().unwrap();
    wait_wire_quiet(&cfg, &mut station);
    let packets = live_packets(&station);
    let off = engine_response_count(&station, EngineMethod::UnsubscribeAllTrades);
    station.pump(Duration::from_secs(6));
    assert_eq!(live_packets(&station), packets, "feed restarted after rapid-toggle drain");
    assert_eq!(engine_response_count(&station, EngineMethod::UnsubscribeAllTrades), off, "quiet Off must not poll");

    // Repeat with the opposite final intent and packet loss. Intermediate pairs must not linger.
    {
        let _loss = ErrEmuGuard::set(10);
        let markets = firetest_retained_markets(&cfg);
        for market in markets.iter().cycle().take(12) {
            station.client.streams().subscribe_trades_for(TradesStreamMode::TradesOnly, [market]).unwrap();
            station.client.streams().unsubscribe_all_trades().unwrap();
        }
        station.client.streams().subscribe_trades_for(TradesStreamMode::TradesOnly, [&cfg.market]).unwrap();
        let before = live_packets(&station);
        assert!(pump_session_until(&mut station, cfg.connect_timeout, "rapid toggles under 10% loss end On", |s| {
            live_packets(s) > before && has_history(s, &cfg.market)
        }));
        assert!(markets.iter().skip(1).all(|market| !has_history(&station, market)));
        station.pump(Duration::from_secs(3));
        let before = live_packets(&station);
        assert!(pump_session_until(&mut station, cfg.connect_timeout, "On remains live after retries", |s| {
            live_packets(s) > before
        }));
        station.client.streams().unsubscribe_all_trades().unwrap();
        wait_wire_quiet(&cfg, &mut station);
    }

    // A reconnect must preserve explicit Off, including its released retained rings.
    let before = station.snapshot();
    station.client.debug_set_outgoing_blackhole(true).unwrap();
    let disconnected = pump_session_until(&mut station, cfg.reconnect_timeout, "Off session disconnect", |s| {
        s.snapshot().reconnecting > before.reconnecting
    });
    station.client.debug_set_outgoing_blackhole(false).unwrap();
    assert!(disconnected);
    assert!(pump_session_until(&mut station, cfg.reconnect_timeout, "Off session reconnect", |s| {
        s.snapshot().connected_again > before.connected_again && s.snapshot().connected_now
    }));
    wait_wire_quiet(&cfg, &mut station);
    assert_eq!(station.snapshot().parse_failed, 0);
    assert_eq!(engine_response_count(&station, EngineMethod::RequestCandlesData), 0);
    println!("FIRETEST_COMPACT_RECOVERY_PASS: stale On/Off, same-session repair, cancelled chart/reselect, 24 rapid toggles (0/10% loss), quiet wire, ring release, Off reconnect; no trades created");
}

#[test]
#[ignore = "live MoonBot required; opens and closes one emulator-only trade"]
fn fire_test_compact_trade_capture() {
    let _lock = firetest_live_test_lock();
    let cfg = FireConfig::load_required();
    assert!(cfg.allow_mutation);
    let keys = parse_key_info(&cfg.key_b64).expect("invalid FireTest key").keys;
    let _loss = ErrEmuGuard::set(0);
    let mut controller =
        Session::connect_with_market_history("Capture-controller", &cfg, keys, None, MarketHistorySizing::Compact);
    let mut station =
        Session::connect_with_market_history("Capture-station", &cfg, keys, None, MarketHistorySizing::Compact);
    station.begin_order_state_capture();
    let original_settings = ensure_server_emulator_mode(&cfg, &mut controller, &mut station);
    let before: Vec<_> = controller.state_snapshot().orders().iter().map(|order| order.uid).collect();
    let mut owned_uid = None;
    let mut requested_price = 0.0;
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        forget_all(&cfg, &mut controller, &mut station);
        let ask = market_live_ask(&controller, &cfg.market).expect("current ask");
        requested_price = ask * 0.98;
        controller
            .client
            .trade()
            .new_order(
                NewOrderParams::new(&cfg.market, OrderSide::Long, requested_price, FIRETEST_ORDER_SIZE_USD)
                    .with_planned_sell_price(ask * 1.5)
                    .with_stops(StopSettings::disabled()),
            )
            .unwrap();
        assert!(pump_pair_until_sessions(
            &mut controller,
            &mut station,
            cfg.connect_timeout,
            "emulator buy order",
            |a, _| {
                !matching_new_order_uids(a, &before, &cfg.market, false, requested_price, FIRETEST_ORDER_SIZE_USD, true)
                    .is_empty()
            }
        ));
        let candidates = matching_new_order_uids(
            &controller,
            &before,
            &cfg.market,
            false,
            requested_price,
            FIRETEST_ORDER_SIZE_USD,
            true,
        );
        assert_eq!(candidates.len(), 1, "ambiguous test order: {candidates:?}");
        let uid = candidates[0];
        owned_uid = Some(uid);
        assert!(pump_pair_until_sessions(
            &mut controller,
            &mut station,
            cfg.connect_timeout,
            "emulator BuySet",
            |a, _| {
                a.state_snapshot().orders().get(uid).is_some_and(|order| order.status == OrderWorkerStatus::BuySet)
            }
        ));
        assert!(controller.replace_order(uid, market_live_ask(&station, &cfg.market).unwrap() * 1.01));
        assert!(pump_pair_until_sessions(
            &mut controller,
            &mut station,
            cfg.connect_timeout,
            "emulator buy filled",
            |a, b| {
                a.state_snapshot().orders().get(uid).is_some_and(|order| order.status == OrderWorkerStatus::SellSet)
                    && captured_order_state_since(b, 0, uid, |event| event.status == OrderWorkerStatus::BuyDone)
            }
        ));
        select_and_capture(&cfg, &mut controller, &mut station, &[&cfg.market], &cfg.market);
        let live_ring = station.state_snapshot().market_history_readers(&cfg.market).unwrap().futures_trades.unwrap();
        let before_live = live_ring.cursor_from_now().next_seq();
        assert!(pump_pair_until_sessions(
            &mut controller,
            &mut station,
            cfg.connect_timeout,
            "live trades after archive",
            |_, _| live_ring.cursor_from_now().next_seq() > before_live
        ));
        drop(live_ring);

        // A station-owned cap of two makes eviction observable with only three markets.
        let markets = firetest_retained_markets(&cfg);
        let second = &markets[1];
        let third = &markets[2];
        select_and_capture(&cfg, &mut controller, &mut station, &[&cfg.market, second], second);
        assert!(has_history(&station, &cfg.market));
        select_and_capture(&cfg, &mut controller, &mut station, &[second, third], third);
        assert!(!has_history(&station, &cfg.market));
        assert!(has_history(&station, second) && has_history(&station, third));
        select_and_capture(&cfg, &mut controller, &mut station, &[&cfg.market], &cfg.market);
        assert!(!has_history(&station, second) && !has_history(&station, third));

        // Exercise the expiry action without waiting ten real minutes. Timers belong to the station.
        println!("FIRETEST compact: simulate 10-minute open-trade expiry; discard without saving");
        forget_all(&cfg, &mut controller, &mut station);
        assert!(controller
            .state_snapshot()
            .orders()
            .get(uid)
            .is_some_and(|order| order.status == OrderWorkerStatus::SellSet));
        assert!(controller.panic_sell_order(uid, true));
        assert!(pump_pair_until_sessions(
            &mut controller,
            &mut station,
            cfg.connect_timeout,
            "sale while station has no histories",
            |_, b| { captured_order_state_since(b, 0, uid, |event| event.status == OrderWorkerStatus::SellDone) }
        ));
        select_and_capture(&cfg, &mut controller, &mut station, &[&cfg.market], &cfg.market);
        station.pump(Duration::from_secs(2));
        let snapshot = station.state_snapshot();
        let market = snapshot.markets().get(&cfg.market).unwrap();
        let readers = snapshot.market_history_readers_for(&market).unwrap();
        let mut rows = Vec::new();
        readers.futures_trades.as_ref().unwrap().copy_last(5_000, &mut rows);
        assert!(!rows.is_empty() && rows.len() <= 5_000);
        let mut csv = String::from("unix_millis,price,quantity\n");
        for row in &rows {
            writeln!(csv, "{},{},{}", row.unix_millis(), row.price, row.qty).unwrap();
        }
        let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("target/firetest_compact_capture.csv");
        fs::write(&path, csv).unwrap();
        println!("FIRETEST compact: saved {} rows after 2s post-sale tail to {}", rows.len(), path.display());
        drop(readers);
        drop(snapshot);
        drop(rows);
        forget_all(&cfg, &mut controller, &mut station);
        for session in [&controller, &station] {
            let stats = session.snapshot();
            assert_eq!(stats.parse_failed, 0);
            assert_eq!(
                stats.engine_method_counts.get(&EngineMethod::RequestCandlesData.to_byte()).copied().unwrap_or(0),
                0
            );
        }
    }));

    // Cleanup also covers a failure after creation but before correlation completed.
    if owned_uid.is_none() && requested_price > 0.0 {
        controller.pump(Duration::from_secs(1));
        let candidates = matching_new_order_uids(
            &controller,
            &before,
            &cfg.market,
            false,
            requested_price,
            FIRETEST_ORDER_SIZE_USD,
            true,
        );
        if candidates.len() == 1 {
            owned_uid = candidates.first().copied();
        }
    }
    if let Some(uid) = owned_uid {
        if let Some(order) = controller.state_snapshot().orders().get(uid) {
            assert!(order.emulator_mode, "refuse to clean up a real order");
            if order.status == OrderWorkerStatus::SellSet || order.status == OrderWorkerStatus::BuyDone {
                controller.panic_sell_order(uid, true);
            } else if !order.job_is_done {
                controller.cancel_order(uid);
            }
        }
        assert!(
            pump_pair_until_sessions(
                &mut controller,
                &mut station,
                cfg.connect_timeout,
                "compact emulator cleanup",
                |a, _| { a.state_snapshot().orders().get(uid).is_none() }
            ),
            "test emulator order {uid} is still active"
        );
    }
    restore_server_emulator_mode(&cfg, &mut controller, &mut station, original_settings);
    if let Err(error) = result {
        std::panic::resume_unwind(error);
    }
    println!("FIRETEST_COMPACT_PASS: immediate charts, 3 markets, eviction, expiry, emulator sale, recapture, release; no automatic candles");
}
