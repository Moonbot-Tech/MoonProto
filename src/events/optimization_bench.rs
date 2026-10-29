use super::*;
use crate::bench_support::measure;
use crate::commands::trade::*;
use std::hint::black_box;

fn header(uid: u64) -> MarketCommandHeader {
    MarketCommandHeader {
        base: BaseCommandHeader {
            cmd_id: 26,
            ver: 4,
            uid,
        },
        currency: 0,
        platform: 0,
        market_name: String::new(),
    }
}

fn seeded_dispatcher() -> EventDispatcher {
    let mut d = EventDispatcher::new();
    d.last_known_server_token = 1;
    d.last_known_peer_app_token = 2;
    d.markets
        .apply_markets_list(crate::commands::market::MarketsListResponse {
            markets: vec![crate::state::markets::mk_market("BTCUSDT", 0)],
            corr_markets: vec![],
        });
    d.process_command_order(
        TradeCommand::OrderImage(OrderImage {
            header: BaseCommandHeader {
                cmd_id: 41,
                ver: 4,
                uid: 1,
            },
            state_rev: 1,
            desc: OrderDescription::for_test("BTCUSDT", false, false),
            section_mask: ORDER_SECTION_ALL_MASK,
            state: CanonicalOrderState::default(),
        }),
        1000,
        &mut Vec::new(),
    );
    assert!(d.orders.get(1).is_some());
    d
}

#[test]
fn visual_dispatch_preserves_snapshot_and_world_gates() {
    let mut d = seeded_dispatcher();
    let held = d.orders.snapshot();
    let mut payload = Vec::new();
    header(1).base.write(&mut payload);
    payload.extend_from_slice(&[0, 0]);
    crate::commands::registry::write_string(&mut payload, "BTCUSDT");
    payload.extend_from_slice(&90.0f32.to_le_bytes());
    payload.extend_from_slice(&110.0f32.to_le_bytes());
    let mut out = Vec::new();
    for _ in 0..2 {
        out.clear();
        d.client_new_data_order(&payload, 1000, &mut out);
        assert!(matches!(out.as_slice(), [Event::Order(OrderEvent::CorridorChanged(1))]));
    }
    assert_eq!(held.get(1).unwrap().corridor_price_down, 0.0);
    assert_eq!(d.orders.get(1).unwrap().corridor_price_down, 90.0);
    d.last_known_server_token = 0;
    d.last_known_peer_app_token = 3;
    out.clear();
    d.client_new_data_order(&payload, 1000, &mut out);
    assert!(out.is_empty());
    assert!(d.orders.get(1).is_some());
    d.last_known_server_token = 1;
    payload[1..3].copy_from_slice(&5u16.to_le_bytes());
    d.client_new_data_order(&payload, 1000, &mut out);
    assert!(
        d.orders.get(1).is_none(),
        "future commands still run world-reset handling"
    );
    assert!(!out.is_empty());
}

#[test]
#[ignore = "CPU/allocation benchmark; release, --nocapture --test-threads=1"]
fn followup_order_benchmark() {
    for held_snapshot in [false, true] {
        let mut d = seeded_dispatcher();
        let mut out = Vec::with_capacity(4);
        let name = if held_snapshot {
            "order_apply_cow"
        } else {
            "order_apply_unique"
        };
        measure(name, 8192, || {
            let held = held_snapshot.then(|| d.orders.snapshot());
            out.clear();
            d.process_command_order(
                TradeCommand::CorridorUpdate(CorridorUpdate {
                    market: header(1),
                    price_down: black_box(90.0),
                    price_up: black_box(110.0),
                }),
                1000,
                &mut out,
            );
            assert!(matches!(out.as_slice(), [Event::Order(OrderEvent::CorridorChanged(1))]));
            black_box(held);
        });
        assert_eq!(d.orders.get(1).unwrap().corridor_price_down, 90.0);
    }
    for (cmd, tail) in [(26, 8), (25, 22)] {
        let mut payload = Vec::new();
        header(1).base.write(&mut payload);
        payload[0] = cmd;
        payload.extend_from_slice(&[0, 0]);
        crate::commands::registry::write_string(&mut payload, "BTCUSDT");
        payload.resize(payload.len() + tail, 0);
        let name = if cmd == 26 { "corridor_parse" } else { "trace_parse" };
        measure(name, 32768, || {
            let command = TradeCommand::parse_for_apply(black_box(&payload)).unwrap();
            assert_eq!(black_box(&command).uid(), 1);
            drop(command);
        });
    }
}

#[test]
#[ignore = "CPU/allocation benchmark; release, --nocapture --test-threads=1"]
fn followup_scope_benchmark() {
    for count in [0, 1, 1000] {
        let mut d = EventDispatcher::new();
        d.market_history_auto_enabled = false;
        let scope = if count == 0 {
            TradeStorageScope::All
        } else {
            TradeStorageScope::from_markets((0..count).map(|i| format!("COIN{i}USDT")))
        };
        let intent = crate::client::TradeStorageIntent {
            scope: Arc::new(scope),
            retain_mm_orders: false,
        };
        d.set_trade_storage_intent(Some(&intent), 45000.0);
        measure(&format!("scope_{count}"), 8192, || {
            d.set_trade_storage_intent(black_box(Some(&intent)), 45000.0);
            d.sync_market_history_storage();
        });
        assert!(d.trade_storage_scope.is_some());
    }
}

#[test]
#[ignore = "CPU/allocation benchmark; release, --nocapture --test-threads=1"]
fn followup_unretained_rows_benchmark() {
    for kind in [1u16, 3] {
        for rows in [1u8, 250] {
            let mut d = seeded_dispatcher();
            let mut payload = 45000.0f64.to_le_bytes().to_vec();
            payload.extend_from_slice(&0u16.to_le_bytes());
            payload.extend_from_slice(&(kind << 14).to_le_bytes());
            if kind == 3 {
                payload.push(0);
            }
            payload.push(rows);
            for row in 0..rows {
                payload.extend_from_slice(&i16::from(row).to_le_bytes());
                payload.extend_from_slice(&100.0f32.to_le_bytes());
                payload.extend_from_slice(&1.0f32.to_le_bytes());
            }
            payload.push(0);
            let mut out = Vec::with_capacity(4);
            let mut packet = 0u16;
            measure(&format!("unretained_{kind}_{rows}"), 8192, || {
                packet = packet.wrapping_add(1);
                payload[8..10].copy_from_slice(&packet.to_le_bytes());
                out.clear();
                d.client_new_data_trades_stream(black_box(&payload), 1000, Some(45000.0), &mut out);
                assert!(!out.is_empty());
                black_box(&out);
            });
            assert!(d.market_history.is_none());
        }
    }
}

#[test]
#[ignore = "CPU/allocation benchmark; release, --nocapture --test-threads=1"]
fn followup_queue_benchmark() {
    println!(
        "FOLLOWUP_SIZE Event={} OrderEvent={}",
        std::mem::size_of::<Event>(),
        std::mem::size_of::<OrderEvent>()
    );
    for count in [1, 64, 4096] {
        let mut d = EventDispatcher::new();
        measure(&format!("queue_{count}"), 256, || {
            d.queue_events((0..count).map(|uid| Event::Order(OrderEvent::CorridorChanged(uid))));
            for (uid, event) in d.take_queued_events().into_iter().enumerate() {
                assert!(matches!(black_box(event), Event::Order(OrderEvent::CorridorChanged(n)) if n == uid as u64));
            }
        });
    }
}

#[test]
#[ignore = "CPU/allocation benchmark; release, --nocapture --test-threads=1"]
fn followup_retained_rows_benchmark() {
    for kind in [1u16, 3] {
        let worker = MarketHistoryWorker::spawn(crate::state::MarketHistoryConfig {
            futures_trades_capacity: 0,
            spot_trades_capacity: 0,
            liquidation_capacity: 256,
            mm_orders_capacity: 256,
            last_price_capacity: 0,
            mini_candles_capacity: 0,
            candles_5m_capacity: 0,
        });
        let mut d = seeded_dispatcher();
        d.set_market_history_handle(worker.handle());
        d.set_trade_storage_intent(
            Some(&crate::client::TradeStorageIntent {
                scope: Arc::new(TradeStorageScope::All),
                retain_mm_orders: true,
            }),
            45000.0,
        );
        let mut payload = 45000.0f64.to_le_bytes().to_vec();
        payload.extend_from_slice(&0u16.to_le_bytes());
        payload.extend_from_slice(&(kind << 14).to_le_bytes());
        if kind == 3 {
            payload.push(0);
        }
        payload.push(250);
        for row in 0..250i16 {
            payload.extend_from_slice(&row.to_le_bytes());
            payload.extend_from_slice(&100.0f32.to_le_bytes());
            payload.extend_from_slice(&1.0f32.to_le_bytes());
        }
        payload.push(0);
        let mut out = Vec::with_capacity(4);
        let mut packet = 0u16;
        measure(&format!("retained_{kind}_250"), 256, || {
            packet = packet.wrapping_add(1);
            payload[8..10].copy_from_slice(&packet.to_le_bytes());
            out.clear();
            d.client_new_data_trades_stream(black_box(&payload), 1000, Some(45000.0), &mut out);
            black_box(&out);
        });
        assert!(worker.flush(crate::MoonTime::from_delphi_days(45000.0).unwrap()));
        let readers = worker.readers("BTCUSDT").unwrap();
        let count = if kind == 1 {
            readers.mm_orders.unwrap().bounds().len
        } else {
            readers.liquidations.unwrap().bounds().len
        };
        assert_eq!(count, 256);
    }
}
