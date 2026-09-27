//! Read-only live runtime benchmark. No orders or shared settings are changed.
//! Usage: runtime_bench <config> <clients> <idle|quiet|trades> <seconds> [alloc]
//! Config uses the FireTest key=value format; credentials never enter the log.
mod common;

#[cfg(windows)]
mod bench {
    use moonproto::state::MarketHistorySizing;
    use moonproto::{Event, MoonClient, RefreshConfig, TradesStreamMode};
    use std::alloc::{GlobalAlloc, Layout, System};
    use std::collections::HashMap;
    use std::sync::atomic::{AtomicBool, AtomicU64, Ordering::Relaxed};
    use std::time::{Duration, Instant};
    use windows_sys::Win32::Foundation::FILETIME;
    use windows_sys::Win32::System::ProcessStatus::{
        GetProcessMemoryInfo, PROCESS_MEMORY_COUNTERS_EX,
    };
    use windows_sys::Win32::System::Threading::{GetCurrentProcess, GetProcessTimes};
    use windows_sys::Win32::System::WindowsProgramming::QueryProcessCycleTime;

    static COUNT: AtomicBool = AtomicBool::new(false);
    static ALLOCS: AtomicU64 = AtomicU64::new(0);
    static BYTES: AtomicU64 = AtomicU64::new(0);
    pub struct Allocator;
    // The disabled path is identical in before/after builds. Allocation counting
    // is a separate run: contended atomics are not a CPU benchmark.
    unsafe impl GlobalAlloc for Allocator {
        unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
            if COUNT.load(Relaxed) {
                ALLOCS.fetch_add(1, Relaxed);
                BYTES.fetch_add(layout.size() as u64, Relaxed);
            }
            System.alloc(layout)
        }
        unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
            if COUNT.load(Relaxed) {
                ALLOCS.fetch_add(1, Relaxed);
                BYTES.fetch_add(layout.size() as u64, Relaxed);
            }
            System.alloc_zeroed(layout)
        }
        unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
            System.dealloc(ptr, layout)
        }
        unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, size: usize) -> *mut u8 {
            if COUNT.load(Relaxed) {
                ALLOCS.fetch_add(1, Relaxed);
                BYTES.fetch_add(size as u64, Relaxed);
            }
            System.realloc(ptr, layout, size)
        }
    }

    fn process_usage() -> (f64, u64, usize, usize) {
        unsafe {
            let process = GetCurrentProcess();
            let mut creation: FILETIME = std::mem::zeroed();
            let mut exit: FILETIME = std::mem::zeroed();
            let mut kernel: FILETIME = std::mem::zeroed();
            let mut user: FILETIME = std::mem::zeroed();
            assert_ne!(
                GetProcessTimes(process, &mut creation, &mut exit, &mut kernel, &mut user),
                0
            );
            let ticks =
                |t: FILETIME| (u64::from(t.dwHighDateTime) << 32) | u64::from(t.dwLowDateTime);
            let mut cycles = 0;
            assert_ne!(QueryProcessCycleTime(process, &mut cycles), 0);
            let mut memory: PROCESS_MEMORY_COUNTERS_EX = std::mem::zeroed();
            let size = std::mem::size_of_val(&memory) as u32;
            assert_ne!(
                GetProcessMemoryInfo(
                    process,
                    (&mut memory as *mut PROCESS_MEMORY_COUNTERS_EX).cast(),
                    size
                ),
                0
            );
            (
                (ticks(kernel) + ticks(user)) as f64 / 10_000.0,
                cycles,
                memory.PrivateUsage,
                memory.WorkingSetSize,
            )
        }
    }

    fn drain(clients: &[MoonClient], events: &mut Vec<Event>) -> u64 {
        let mut count = 0;
        for client in clients {
            events.clear();
            client.drain_events_into(events);
            count += events.len() as u64;
            // Lifecycle events are sparse, but must not accumulate either.
            client.drain_lifecycle_events();
        }
        count
    }

    fn counters(clients: &[MoonClient], markets: &[String]) -> (u64, u64, u64, u64, u64) {
        let (mut rx, mut tx, mut packets, mut trades, mut rotations) = (0, 0, 0, 0, 0);
        for client in clients {
            let s = client.startup_status();
            rx += s.current_port_received_bytes;
            tx += s.current_port_sent_bytes;
            packets += s.current_port_received_packets;
            rotations += u64::from(s.local_port_change_count);
            if let Some(snapshot) = client.snapshot() {
                for name in markets {
                    if let Some(readers) = snapshot.market_history_readers(name) {
                        for reader in [readers.futures_trades, readers.spot_trades]
                            .into_iter()
                            .flatten()
                        {
                            trades += reader.bounds().len as u64;
                        }
                    }
                }
            }
        }
        (rx, tx, packets, trades, rotations)
    }

    pub fn run() {
        let args: Vec<_> = std::env::args().collect();
        assert!(
            args.len() >= 5,
            "runtime_bench <config> <clients> <idle|quiet|trades> <seconds> [alloc]"
        );
        let count: usize = args[2].parse().unwrap();
        let mode = args[3].as_str();
        assert!(matches!(mode, "idle" | "quiet" | "trades"));
        let seconds: u64 = args[4].parse().unwrap();
        assert!(count > 0 && seconds > 0);
        let text = std::fs::read_to_string(&args[1]).expect("cannot read config");
        let config: HashMap<_, _> = text
            .lines()
            .filter_map(|line| {
                let line = line.trim_start_matches('\u{feff}').trim();
                if line.starts_with('#') {
                    return None;
                }
                line.split_once('=')
                    .map(|(k, v)| (k.trim(), v.trim().trim_matches(['\'', '"'])))
            })
            .collect();
        let key = config
            .get("key")
            .or_else(|| config.get("moonproto_key"))
            .expect("missing key");
        let endpoint = config.get("server").map(|s| s.to_string());
        let mut clients = Vec::with_capacity(count);
        let mut events = Vec::new();
        let markets: Vec<String> = [
            "BTCUSDT", "ETHUSDT", "SOLUSDT", "XRPUSDT", "DOGEUSDT", "BNBUSDT", "ADAUSDT",
            "LINKUSDT", "AVAXUSDT", "LTCUSDT",
        ]
        .into_iter()
        .map(str::to_owned)
        .collect();
        for index in 0..count {
            let (mut cfg, _) = super::common::client_config(key, endpoint.as_ref()).unwrap();
            cfg = cfg
                .with_client_id(rand::random())
                .with_market_history(MarketHistorySizing::Compact);
            if let Some(mode) = config
                .get("transport_mode")
                .or_else(|| config.get("mask_ver"))
            {
                cfg = cfg.with_transport_mode(match *mode {
                    "0" => moonproto::TransportMode::V0,
                    "1" => moonproto::TransportMode::V1,
                    "2" => moonproto::TransportMode::V2,
                    _ => panic!("unsupported transport_mode"),
                });
            }
            if mode == "quiet" {
                cfg = cfg.with_refresh(RefreshConfig {
                    update_markets_every: None,
                    check_tags_every: None,
                });
            }
            let timeout = Duration::from_secs(90);
            let client = MoonClient::connect_blocking(
                cfg,
                moonproto::ConnectConfig::new(super::common::init_config())
                    .with_connect_timeout(timeout),
                timeout,
            )
            .expect("connect failed");
            if mode == "trades" {
                client
                    .streams()
                    .subscribe_trades_for(TradesStreamMode::TradesOnly, markets.iter())
                    .unwrap();
            }
            clients.push(client);
            drain(&clients, &mut events);
            eprintln!("BENCH ready {}/{}", index + 1, count);
        }
        let warmup = Instant::now();
        while warmup.elapsed() < Duration::from_secs(15) {
            drain(&clients, &mut events);
            std::thread::sleep(Duration::from_millis(50));
        }
        let before = counters(&clients, &markets);
        let usage_before = process_usage();
        let start = Instant::now();
        COUNT.store(args.get(5).is_some_and(|s| s == "alloc"), Relaxed);
        let mut event_count = 0;
        let (mut server_cpu, mut system_cpu, mut health_samples) = (0u64, 0u64, 0u64);
        let mut server_cpu_max = 0;
        let mut next_health = Instant::now();
        let mut disconnected = false;
        while start.elapsed() < Duration::from_secs(seconds) {
            event_count += drain(&clients, &mut events);
            if Instant::now() >= next_health {
                if let Some(s) = clients[0].snapshot() {
                    let h = s.kernel_health();
                    server_cpu += u64::from(h.process_cpu_percent);
                    system_cpu += u64::from(h.system_cpu_percent);
                    server_cpu_max = server_cpu_max.max(h.process_cpu_percent);
                    health_samples += 1;
                }
                disconnected |= clients
                    .iter()
                    .any(|c| c.startup_status().state != moonproto::StartupState::Ready);
                next_health = Instant::now() + Duration::from_secs(1);
            }
            std::thread::sleep(Duration::from_millis(50));
        }
        COUNT.store(false, Relaxed);
        let elapsed = start.elapsed().as_secs_f64();
        let usage_after = process_usage();
        let after = counters(&clients, &markets);
        let valid = !disconnected && after.4 == before.4 && after.0 > before.0;
        println!(
            "{}",
            serde_json::json!({
                "clients": count, "mode": mode, "seconds": elapsed, "valid": valid,
                "cpu_ms_per_sec": (usage_after.0 - usage_before.0) / elapsed,
                "cycles_per_sec": (usage_after.1 - usage_before.1) as f64 / elapsed,
                "private_mb_start": usage_before.2 as f64 / 1e6, "private_mb_end": usage_after.2 as f64 / 1e6,
                "working_set_mb": usage_after.3 as f64 / 1e6,
                "rx_bytes_per_sec": after.0.saturating_sub(before.0) as f64 / elapsed,
                "tx_bytes_per_sec": after.1.saturating_sub(before.1) as f64 / elapsed,
                "rx_packets_per_sec": after.2.saturating_sub(before.2) as f64 / elapsed,
                "retained_trades_start": before.3, "retained_trades_end": after.3,
                "events_per_sec": event_count as f64 / elapsed,
                "allocs_per_sec": ALLOCS.load(Relaxed) as f64 / elapsed,
                "allocated_bytes_per_sec": BYTES.load(Relaxed) as f64 / elapsed,
                "server_cpu_mean": server_cpu as f64 / health_samples.max(1) as f64,
                "server_cpu_max": server_cpu_max,
                "server_system_cpu_mean": system_cpu as f64 / health_samples.max(1) as f64,
            })
        );
        for client in &clients {
            client.disconnect().unwrap();
        }
        for client in &clients {
            client.wait_finished().unwrap();
        }
        drop(clients);
        assert!(valid, "connection changed or no traffic during measurement");
    }
}

#[cfg(windows)]
#[global_allocator]
static ALLOCATOR: bench::Allocator = bench::Allocator;

fn main() {
    #[cfg(windows)]
    bench::run();
    #[cfg(not(windows))]
    panic!("This benchmark currently uses Windows process counters");
}
