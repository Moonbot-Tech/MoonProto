use super::*;
use moonproto::state::{SeqRingCursor, SeqRingReader, TradeHistoryRow};
use std::collections::HashMap;
use std::io::Write;

struct LiveCapture {
    market: String,
    reader: SeqRingReader<TradeHistoryRow>,
    cursor: SeqRingCursor,
    rows: Vec<TradeHistoryRow>,
}

impl LiveCapture {
    fn drain(&mut self, scratch: &mut Vec<TradeHistoryRow>) {
        let meta = self.reader.copy_new_since(&mut self.cursor, self.reader.capacity(), scratch);
        assert!(!meta.clipped && !meta.concurrent_miss, "{}: live capture lost retained rows", self.market);
        self.rows.extend_from_slice(scratch);
    }
}

fn volumes(rows: &[TradeHistoryRow]) -> [f64; 2] {
    let mut result = [0.0; 2];
    for row in rows {
        result[usize::from(!row.is_buy())] += f64::from(row.quantity());
    }
    result
}

fn volume_error_percent(actual: [f64; 2], expected: [f64; 2]) -> f64 {
    let a = actual[0] + actual[1];
    let b = expected[0] + expected[1];
    assert!(b > 0.0, "comparison needs nonzero traded volume");
    (a / b - 1.0) * 100.0
}

fn exact_overlap(a: &[TradeHistoryRow], b: &[TradeHistoryRow]) -> usize {
    let mut remaining = HashMap::<_, usize>::new();
    for row in a {
        *remaining.entry((row.unix_millis(), row.price.to_bits(), row.qty.to_bits())).or_default() += 1;
    }
    b.iter().filter(|row| {
        let count = remaining.entry((row.unix_millis(), row.price.to_bits(), row.qty.to_bits())).or_default();
        let matched = *count != 0;
        *count = count.saturating_sub(1);
        matched
    }).count()
}

fn in_window(rows: &[TradeHistoryRow], start_ms: i64, end_ms: i64) -> Vec<TradeHistoryRow> {
    rows.iter().copied().filter(|row| (start_ms..end_ms).contains(&row.unix_millis())).collect()
}

fn save_rows(path: &std::path::Path, rows: &[TradeHistoryRow]) {
    let mut file = std::io::BufWriter::new(std::fs::File::create(path).unwrap());
    writeln!(file, "unix_ms,price,signed_qty").unwrap();
    for row in rows {
        writeln!(file, "{},{},{}", row.unix_millis(), row.price, row.qty).unwrap();
    }
    file.flush().unwrap();
}

#[test]
fn comparison_preserves_multiplicity_and_does_not_call_aggregation_packet_loss() {
    let row = TradeHistoryRow { time: MoonTime::now(), price: 100.0, qty: 3.0 };
    let live = [row, TradeHistoryRow { qty: 7.0, ..row }];
    let archive = [TradeHistoryRow { qty: 10.0, ..row }];
    assert_eq!(exact_overlap(&live, &archive), 0);
    assert_eq!(volumes(&live), volumes(&archive));
    assert_eq!(exact_overlap(&[row], &[row, row]), 1);
    assert_eq!(exact_overlap(&[row, row], &[row, row]), 2);
}

#[test]
#[ignore = "live MoonBot required; captures 120s on three markets, no trading or settings changes"]
fn fire_test_live_trades_vs_raw_archive() {
    let _lock = firetest_live_test_lock();
    let cfg = FireConfig::load_required();
    let keys = parse_key_info(&cfg.key_b64).expect("invalid FireTest key").keys;
    let _loss = ErrEmuGuard::set(0);
    let mut session = Session::connect_with_market_history(
        "Archive-compare", &cfg, keys, None, MarketHistorySizing::compact_with_budget_percent(200),
    );
    let markets = firetest_retained_markets(&cfg);
    assert_eq!(markets.len(), 3, "comparison needs three distinct markets");
    assert!(pump_session_until(&mut session, cfg.connect_timeout, "three live histories", |s| {
        markets.iter().all(|name| s.state_snapshot().market_history_readers(name)
            .and_then(|r| r.futures_trades).is_some_and(|r| r.bounds().len > 0))
    }));
    let initial = session.snapshot();
    let traffic_before = session.client.startup_status();
    let mut captures: Vec<_> = markets.into_iter().map(|market| {
        let reader = session.state_snapshot().market_history_readers(&market).unwrap().futures_trades.unwrap();
        LiveCapture { cursor: reader.cursor_from_now(), market, reader, rows: Vec::new() }
    }).collect();
    let start_ms = MoonTime::now().unix_millis() + 2_000;
    let mut scratch = Vec::new();
    let started = Instant::now();
    println!("FIRETEST_ARCHIVE_COMPARE: collecting live-only histories for 120s; no archive requested yet");
    while started.elapsed() < Duration::from_secs(120) {
        session.pump(Duration::from_millis(20));
        for capture in &mut captures {
            capture.drain(&mut scratch);
        }
    }
    let end_ms = MoonTime::now().unix_millis() - 2_000;
    // Let late live packets settle, but keep the comparison window fixed.
    let settle = Instant::now();
    while settle.elapsed() < Duration::from_secs(15) {
        session.pump(Duration::from_millis(20));
        for capture in &mut captures {
            capture.drain(&mut scratch);
        }
    }
    // Never read these cursors again: applying an archive rewrites the ring.
    let tickets: Vec<_> = captures.iter().map(|c| session.client.history().request_chart(&c.market).unwrap()).collect();
    assert!(pump_session_until(&mut session, cfg.candles_timeout, "raw archives and applied histories", |s| {
        tickets.iter().all(|ticket| s.market_history_events.iter().any(|e| market_history_event_id(e) == ticket.id()))
    }));
    for event in &session.market_history_events {
        assert!(matches!(event, MarketHistoryEvent::Ready { .. }), "{event:?}");
    }
    let output = std::path::PathBuf::from("target").join(format!("history-compare-{}", MoonTime::now().unix_millis()));
    std::fs::create_dir_all(&output).unwrap();
    let mut windows = Vec::new();
    for (capture, ticket) in captures.iter().zip(&tickets) {
        let archive = &session.market_history_archives.iter().find(|(t, _)| t == ticket)
            .expect("diagnostic raw archive was not captured").1;
        assert!(!archive.is_empty(), "{}: core has no trade archive", capture.market);
        let received_count = session.market_history_events.iter().find_map(|event| match event {
            MarketHistoryEvent::Ready { ticket: t, summary } if t == ticket => Some(summary.received.futures_trades),
            _ => None,
        }).unwrap();
        assert_eq!(archive.len(), received_count);
        let mut merged = Vec::new();
        capture.reader.copy_last(capture.reader.capacity(), &mut merged);
        save_rows(&output.join(format!("{}-live.csv", capture.market)), &capture.rows);
        save_rows(&output.join(format!("{}-archive.csv", capture.market)), archive);
        save_rows(&output.join(format!("{}-merged.csv", capture.market)), &merged);
        let low = start_ms.max(archive.iter().map(|r| r.unix_millis()).min().unwrap() + 1_000)
            .max(merged.iter().map(|r| r.unix_millis()).min().unwrap() + 1_000);
        let high = end_ms.min(archive.iter().map(|r| r.unix_millis()).max().unwrap() - 1_000);
        assert!(high - low >= 30_000, "{}: insufficient common history window", capture.market);
        let live = in_window(&capture.rows, low, high);
        let archived = in_window(archive, low, high);
        let merged_window = in_window(&merged, low, high);
        assert!(!live.is_empty() && !archived.is_empty());
        assert!(live.iter().chain(&archived).all(|r| r.price.is_finite() && r.price > 0.0 && r.qty.is_finite()));
        let overlap = exact_overlap(&live, &archived);
        println!("FIRETEST_ARCHIVE_COMPARE {} window_ms={}..{} span_s={:.3} live={} raw_archive={} merged={} exact_overlap={} live_only={} archive_only={} buy_sell_qty_live={:?} archive={:?} merged={:?}",
            capture.market, low, high, (high - low) as f64 / 1000.0, live.len(), archived.len(), merged_window.len(),
            overlap, live.len() - overlap, archived.len() - overlap, volumes(&live), volumes(&archived), volumes(&merged_window));
        assert_eq!(merged_window.len(), archived.len(), "{}: overlap row count changed", capture.market);
        assert_eq!(exact_overlap(&merged_window, &archived), archived.len(), "{}: overlap is not archive-owned", capture.market);
        let live_error = volume_error_percent(volumes(&live), volumes(&archived));
        assert!(live_error.abs() <= 3.0, "{}: live/archive volume differs by {live_error:.3}%", capture.market);
        // Extend to the independently captured live tail, including the join when available.
        let join_high = capture.rows.iter().map(|r| r.unix_millis()).max().unwrap() + 1;
        let live_join = in_window(&capture.rows, low, join_high);
        let merged_join = in_window(&merged, low, join_high);
        let join_error = volume_error_percent(volumes(&merged_join), volumes(&live_join));
        assert!(join_error.abs() <= 3.0, "{}: merged/live volume differs by {join_error:.3}%", capture.market);
        println!("FIRETEST_ARCHIVE_SPLICE {} live_vs_archive={live_error:.6}% merged_vs_live_through_join={join_error:.6}% apply_us={}",
            capture.market, session.market_history_events.iter().find_map(|event| match event {
                MarketHistoryEvent::Ready { ticket: t, summary } if t == ticket => Some(summary.apply_wall_micros),
                _ => None,
            }).unwrap());
        // Totals reveal aggregation differences; unequal rows alone do NOT prove packet loss.
        // Five-second buckets also expose localized gaps hidden by whole-window totals.
        let mut buckets = std::io::BufWriter::new(std::fs::File::create(
            output.join(format!("{}-buckets.csv", capture.market))).unwrap());
        writeln!(buckets, "start_ms,live_count,archive_count,live_buy,archive_buy,live_sell,archive_sell").unwrap();
        for from in (low..high).step_by(5_000) {
            let a = in_window(&live, from, (from + 5_000).min(high));
            let b = in_window(&archived, from, (from + 5_000).min(high));
            let av = volumes(&a);
            let bv = volumes(&b);
            writeln!(buckets, "{from},{},{},{},{},{},{}", a.len(), b.len(), av[0], bv[0], av[1], bv[1]).unwrap();
        }
        buckets.flush().unwrap();
        windows.push((low, high, archived));
    }
    // A later archive must neither accumulate the overlap nor let delayed live packets reinsert it.
    session.pump(Duration::from_secs(2));
    let repeated: Vec<_> = captures.iter().map(|c| session.client.history().request_chart(&c.market).unwrap()).collect();
    assert!(pump_session_until(&mut session, cfg.candles_timeout, "repeated archives", |s| {
        repeated.iter().all(|ticket| s.market_history_events.iter().any(|e| market_history_event_id(e) == ticket.id()))
    }));
    session.pump(Duration::from_secs(2));
    for ((capture, ticket), (low, high, original)) in captures.iter().zip(&repeated).zip(&windows) {
        assert!(session.market_history_events.iter().any(|event| matches!(event,
            MarketHistoryEvent::Ready { ticket: t, .. } if t == ticket)));
        let mut rows = Vec::new();
        capture.reader.copy_last(capture.reader.capacity(), &mut rows);
        let rows = in_window(&rows, *low, *high);
        assert_eq!(rows.len(), original.len(), "{}: repeated archive changed old row count", capture.market);
        assert_eq!(exact_overlap(&rows, original), original.len(), "{}: repeated archive accumulated overlap", capture.market);
        println!("FIRETEST_ARCHIVE_REPEAT {} unchanged_rows={} buy_sell_qty={:?}", capture.market, rows.len(), volumes(&rows));
    }
    let after = session.client.startup_status();
    assert_eq!(after.local_port_change_count, traffic_before.local_port_change_count);
    assert!(after.current_port_received_bytes > traffic_before.current_port_received_bytes);
    assert!(after.current_port_sent_bytes > traffic_before.current_port_sent_bytes);
    assert_eq!(session.snapshot().connected_again, initial.connected_again);
    assert_eq!(session.snapshot().parse_failed, 0);
    assert_eq!(engine_response_count(&session, EngineMethod::RequestCandlesData), 0);
    println!("FIRETEST_ARCHIVE_COMPARE traffic payload_rx={} payload_tx={} packets_rx={} packets_tx={} elapsed_s={:.2} files={}",
        after.current_port_received_bytes - traffic_before.current_port_received_bytes,
        after.current_port_sent_bytes - traffic_before.current_port_sent_bytes,
        after.current_port_received_packets - traffic_before.current_port_received_packets,
        after.current_port_sent_packets - traffic_before.current_port_sent_packets,
        started.elapsed().as_secs_f64(), output.display());
    session.client.streams().unsubscribe_all_trades().unwrap();
    assert!(pump_session_until(&mut session, cfg.connect_timeout, "release comparison histories", |s| {
        s.client.active_subscriptions().all_trades.is_none()
            && captures.iter().all(|c| s.state_snapshot().market_history_readers(&c.market).is_none())
    }));
    println!("FIRETEST_ARCHIVE_COMPARISON_COMPLETE: volume tolerance <=3%, exact archive-owned overlap, repeated requests stable");
}
