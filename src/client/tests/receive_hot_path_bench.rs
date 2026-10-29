//! Deterministic receive replay, including transport, crypto, dispatch and state mutation.
use super::*;
use crate::client::thread_cpu::ThreadCpuTimer;
use crate::commands::market::MarketsListResponse;
use std::alloc::{GlobalAlloc, Layout, System};
use std::cell::Cell;
use std::hint::black_box;

struct CountingAllocator;
thread_local! {
    static COUNTS: Cell<Option<(u64, u64)>> = const { Cell::new(None) };
}

fn allocated(size: usize) {
    let _ = COUNTS.try_with(|counter| {
        if let Some((calls, bytes)) = counter.get() {
            counter.set(Some((calls + 1, bytes + size as u64)));
        }
    });
}

unsafe impl GlobalAlloc for CountingAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        allocated(layout.size());
        unsafe { System.alloc(layout) }
    }
    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        allocated(layout.size());
        unsafe { System.alloc_zeroed(layout) }
    }
    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        unsafe { System.dealloc(ptr, layout) }
    }
    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, size: usize) -> *mut u8 {
        allocated(size);
        unsafe { System.realloc(ptr, layout, size) }
    }
}

#[global_allocator]
static ALLOCATOR: CountingAllocator = CountingAllocator;

fn prices(count: usize) -> Vec<u8> {
    let mut data = vec![0]; // no funding
    data.extend_from_slice(&(count as i32).to_le_bytes());
    for index in 0..count {
        data.extend_from_slice(&(index as u16).to_le_bytes());
        for value in [index as f64 + 1.0, index as f64 + 2.0, index as f64 + 1.5] {
            data.extend_from_slice(&value.to_le_bytes());
        }
        data.push(1);
    }
    data.push(0); // no correlation markets
    data
}

fn percentile(samples: &mut [f64], fraction: f64) -> f64 {
    if samples.is_empty() { return f64::NAN; }
    samples.sort_by(f64::total_cmp);
    samples[((samples.len() - 1) as f64 * fraction).round() as usize]
}

#[test]
#[ignore = "deterministic receive benchmark; release mode, --nocapture --test-threads=1"]
fn receive_hot_path_benchmark() {
    for (name, count, encrypted, compressed) in [
        ("prices1000", 1000, false, false),
        ("prices1000_deflate", 1000, false, true),
        ("encrypted_prices1", 1, true, false),
        ("encrypted_prices1000", 1000, true, false),
    ] {
        // Keep even the small-packet batch long enough to reduce scheduler noise.
        let packets_per_batch = if count == 1 { 8192 } else { 256 };
        let mut client = Client::new(dummy_cfg());
        let key = install_server_decode_session(&mut client, 42);
        client.authorized = true;
        client.auth_status = AuthStatus::AuthDone;
        client.testing_set_domain_ready(true);
        let mut dispatcher = EventDispatcher::new();
        dispatcher.markets.apply_markets_list(MarketsListResponse {
            markets: (0..count).map(|i| crate::state::markets::mk_market(&format!("TEST{i}"), i as u16)).collect(),
            corr_markets: Vec::new(),
        });
        dispatcher.markets.apply_markets_indexes((0..count).rev().map(|i| format!("TEST{i}")).collect());
        let body = prices(count);
        let data = if compressed { deflate_raw(&body) } else { body };
        let mut response = build_engine_response_payload(0, EngineMethod::UpdateMarketsList, &data);
        response[27] = u8::from(compressed);
        let packets: Vec<_> = (1..=packets_per_batch).map(|seq| {
            if encrypted {
                pack_server_packet(&client.cfg.mac_key, Command::Crypted,
                    &build_server_crypted_payload(&key, seq as u64, Command::API, &response))
            } else {
                pack_server_packet(&client.cfg.mac_key, Command::API, &response)
            }
        }).collect();
        let mut mode = RunMode::new(&mut dispatcher);
        let mut times = Vec::new();
        let mut cycles = Vec::new();
        let mut allocation_totals = (0, 0);
        for iteration in 0..25 {
            client.recv.data_read_state.slider = crate::protocol::slider::Slider::new();
            COUNTS.with(|c| c.set(Some((0, 0))));
            let cpu = ThreadCpuTimer::start();
            let start = Instant::now();
            for packet in &packets {
                assert!((crate::client::protocol_core::ProtocolCore { client: &mut client })
                    .process_datagram(black_box(packet), packet.len() as u64, &mut mode));
            }
            let elapsed = start.elapsed().as_secs_f64() * 1000.0;
            let elapsed_cpu = cpu.elapsed();
            let alloc = COUNTS.with(|c| c.replace(None).unwrap());
            // All rows are mapped through a reversed server index, not local order.
            for index in 0..count {
                let price = mode.dispatcher.markets.price(&format!("TEST{}", count - 1 - index)).unwrap();
                assert_eq!(price.bid, index as f64 + 1.0);
                assert_eq!(price.ask, index as f64 + 2.0);
            }
            if iteration >= 5 {
                times.push(elapsed);
                if let Some(value) = elapsed_cpu.cycles { cycles.push(value as f64 / 1e6); }
                allocation_totals.0 += alloc.0;
                allocation_totals.1 += alloc.1;
            }
        }
        println!("RECEIVE case={name} packets={packets_per_batch} samples=20 ms_p50={:.3} ms_p95={:.3} Mcycles_p50={:.3} allocs_per_packet={:.2} bytes_per_packet={:.2}",
            percentile(&mut times, 0.5), percentile(&mut times, 0.95), percentile(&mut cycles, 0.5),
            allocation_totals.0 as f64 / (20 * packets_per_batch) as f64,
            allocation_totals.1 as f64 / (20 * packets_per_batch) as f64);
    }
}

#[test]
#[ignore = "local market search benchmark; release mode, --nocapture --test-threads=1"]
fn market_find_benchmark() {
    let mut markets = crate::state::MarketsState::new();
    markets.apply_markets_list(MarketsListResponse {
        markets: (0..1000).map(|index| {
            crate::state::markets::mk_market(&format!("COIN{index:04}USDT"), index)
        }).collect(),
        corr_markets: Vec::new(),
    });
    const CALLS: usize = 128;
    for (case, query) in [
        ("exact_first", "coin0000usdt"),
        ("exact_middle", " COIN0500USDT "),
        ("exact_last", "COIN0999USDT"),
        ("prefix_many", "COIN"),
        ("contains_many", "USDT"),
        ("missing", "NO_SUCH_COIN"),
        ("blank", " \t "),
    ] {
        let expected = markets.search(query, 1).pop();
        let mut wall = Vec::new();
        let mut cycles = Vec::new();
        let mut allocations = (0, 0);
        for iteration in 0..25 {
            COUNTS.with(|counter| counter.set(Some((0, 0))));
            let cpu = ThreadCpuTimer::start();
            let start = Instant::now();
            for _ in 0..CALLS {
                let result = black_box(markets.find(black_box(query)));
                match (&result, &expected) {
                    (Some(found), Some(expected)) => assert!(found.ptr_eq(expected)),
                    (None, None) => {},
                    _ => panic!("search result changed"),
                }
                drop(result);
            }
            let elapsed = start.elapsed().as_secs_f64() * 1000.0;
            let elapsed_cpu = cpu.elapsed();
            let counts = COUNTS.with(|counter| counter.replace(None).unwrap());
            if iteration >= 5 {
                wall.push(elapsed);
                if let Some(value) = elapsed_cpu.cycles { cycles.push(value as f64 / 1e6); }
                allocations.0 += counts.0;
                allocations.1 += counts.1;
            }
        }
        println!("FIND case={case} markets=1000 calls={CALLS} samples=20 ms_p50={:.6} ms_p95={:.6} Mcycles_p50={:.6} allocs_per_call={:.2} bytes_per_call={:.2}",
            percentile(&mut wall, 0.5), percentile(&mut wall, 0.95), percentile(&mut cycles, 0.5),
            allocations.0 as f64 / (20 * CALLS) as f64, allocations.1 as f64 / (20 * CALLS) as f64);
    }
}
