//! Read-only live runtime benchmark. No orders or shared settings are changed.
//! Usage: runtime_bench <config> <clients> <idle|quiet|trades|full|compare> <seconds> [alloc]
//! Config uses the FireTest key=value format; credentials never enter the log.
mod common;

#[cfg(any(windows, target_os = "linux"))]
mod bench {
    use moonproto::state::MarketHistorySizing;
    use moonproto::{Event, MoonClient, RefreshConfig, TradesStreamMode};
    use std::alloc::{GlobalAlloc, Layout, System};
    use std::collections::HashMap;
    use std::sync::atomic::{AtomicBool, AtomicU64, Ordering::Relaxed};
    use std::time::{Duration, Instant};

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

    #[cfg(windows)]
    fn process_usage(_memory: bool) -> (f64, u64, usize, usize) {
        use windows_sys::Win32::Foundation::FILETIME;
        use windows_sys::Win32::System::ProcessStatus::{GetProcessMemoryInfo, PROCESS_MEMORY_COUNTERS_EX};
        use windows_sys::Win32::System::Threading::{GetCurrentProcess, GetProcessTimes};
        use windows_sys::Win32::System::WindowsProgramming::QueryProcessCycleTime;
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

    #[cfg(target_os = "linux")]
    fn process_usage(memory: bool) -> (f64, u64, usize, usize) {
        let mut time: libc::timespec = unsafe { std::mem::zeroed() };
        assert_eq!(unsafe { libc::clock_gettime(libc::CLOCK_PROCESS_CPUTIME_ID, &mut time) }, 0);
        // smaps walks the page tables. Only collect it at window boundaries.
        let memory = if memory {
            std::fs::read_to_string("/proc/self/smaps_rollup").expect("process memory counters")
        } else {
            String::new()
        };
        let (mut private, mut resident) = (0, 0);
        for line in memory.lines() {
            let mut fields = line.split_whitespace();
            let field = fields.next().unwrap_or_default();
            if matches!(field, "Rss:" | "Private_Clean:" | "Private_Dirty:" | "Private_Hugetlb:") {
                let bytes = fields.next().unwrap().parse::<usize>().unwrap() * 1024;
                if field == "Rss:" { resident = bytes; } else { private += bytes; }
            }
        }
        (time.tv_sec as f64 * 1_000.0 + time.tv_nsec as f64 / 1e6, 0, private, resident)
    }

    fn thread_cycles() -> u64 {
        #[cfg(windows)]
        unsafe {
            let mut cycles = 0;
            assert_ne!(windows_sys::Win32::System::WindowsProgramming::QueryThreadCycleTime(
                windows_sys::Win32::System::Threading::GetCurrentThread(), &mut cycles), 0);
            cycles
        }
        #[cfg(not(windows))]
        { 0 }
    }

    fn thread_cpu_ms() -> Option<f64> {
        #[cfg(target_os = "linux")]
        {
            let mut time: libc::timespec = unsafe { std::mem::zeroed() };
            assert_eq!(unsafe { libc::clock_gettime(libc::CLOCK_THREAD_CPUTIME_ID, &mut time) }, 0);
            Some(time.tv_sec as f64 * 1_000.0 + time.tv_nsec as f64 / 1e6)
        }
        #[cfg(not(target_os = "linux"))]
        { None }
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
            "runtime_bench <config> <clients> <idle|quiet|trades|full|compare> <seconds> [alloc]"
        );
        let count: usize = args[2].parse().unwrap();
        let mode = args[3].as_str();
        assert!(matches!(
            mode,
            "idle" | "quiet" | "trades" | "full" | "compare"
        ));
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
        #[cfg(feature = "diagnostics")]
        println!(
            "{}",
            serde_json::json!({"kind":"clock_calibration", "empty_timer_cycles":
            moonproto::client::ProtocolMetricsSnapshot::calibrate_profile_timer_cycles(),
            "empty_timer_cpu_ns":moonproto::client::ProtocolMetricsSnapshot::calibrate_profile_timer_cpu_ns()})
        );
        let startup_usage = process_usage(true);
        let startup_started = Instant::now();
        eprintln!(
            "BENCH pid={} clients={} mode={} diagnostics={}",
            std::process::id(),
            count,
            mode,
            cfg!(feature = "diagnostics")
        );
        let mut clients = Vec::with_capacity(count);
        let mut events = Vec::new();
        let mut markets: Vec<String> = [
            "BTCUSDT", "ETHUSDT", "SOLUSDT", "XRPUSDT", "DOGEUSDT", "BNBUSDT", "ADAUSDT",
            "LINKUSDT", "AVAXUSDT", "LTCUSDT",
        ]
        .into_iter()
        .map(str::to_owned)
        .collect();
        for _ in 0..count {
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
            let timeout = Duration::from_secs(180);
            let client = MoonClient::connect(
                cfg,
                moonproto::ConnectConfig::new(super::common::init_config())
                    .with_connect_timeout(timeout),
            )
            .expect("connect failed");
            clients.push(client);
        }
        let mut last_ready = usize::MAX;
        let mut next_startup_report = Instant::now();
        loop {
            drain(&clients, &mut events);
            let states: Vec<_> = clients.iter().map(MoonClient::startup_status).collect();
            let ready = states
                .iter()
                .filter(|s| s.state == moonproto::StartupState::Ready)
                .count();
            if ready != last_ready {
                eprintln!(
                    "BENCH ready {ready}/{count} after {:.1}s",
                    startup_started.elapsed().as_secs_f64()
                );
                last_ready = ready;
            }
            if Instant::now() >= next_startup_report {
                let mut steps = std::collections::BTreeMap::new();
                for status in &states {
                    *steps
                        .entry(format!("{:?}/{:?}", status.state, status.current_step))
                        .or_insert(0usize) += 1;
                }
                eprintln!(
                    "BENCH startup {:.1}s steps={steps:?} retries={} duplicates={}",
                    startup_started.elapsed().as_secs_f64(),
                    states
                        .iter()
                        .map(|s| s.total_init_retries as u64)
                        .sum::<u64>(),
                    states
                        .iter()
                        .map(|s| s.duplicate_sliced_blocks)
                        .sum::<u64>()
                );
                next_startup_report = Instant::now() + Duration::from_secs(10);
            }
            assert!(
                !states
                    .iter()
                    .any(|s| s.state == moonproto::StartupState::Failed),
                "startup failed"
            );
            if ready == count {
                break;
            }
            if startup_started.elapsed() > Duration::from_secs(185) {
                for (index, s) in states
                    .iter()
                    .enumerate()
                    .filter(|(_, s)| s.state != moonproto::StartupState::Ready)
                {
                    eprintln!("BENCH incomplete client={index} state={:?} step={:?} rx={} retries={} rotations={}",
                        s.state, s.current_step, s.current_port_received_bytes, s.total_init_retries, s.local_port_change_count);
                }
                panic!("not all clients reached Ready");
            }
            std::thread::sleep(Duration::from_millis(50));
        }
        let startup_after = process_usage(true);
        println!(
            "{}",
            serde_json::json!({"kind":"startup", "clients":count,
            "seconds":startup_started.elapsed().as_secs_f64(),
            "cpu_ms":startup_after.0-startup_usage.0, "cycles":cfg!(windows).then_some(startup_after.1-startup_usage.1),
            "private_mb":startup_after.2 as f64 / 1e6,
            "per_client":clients.iter().map(|c| {let s=c.startup_status(); serde_json::json!({
                "init_ms":s.elapsed_ms, "retries":s.total_init_retries, "rotations":s.local_port_change_count,
                "duplicate_sliced_blocks":s.duplicate_sliced_blocks, "received_sliced_blocks":s.received_sliced_blocks,
                "rtt_ms":s.round_trip_ms, "delivery_percent":s.downlink_delivery_percent})}).collect::<Vec<_>>() })
        );
        let modes: &[&str] = if mode == "compare" {
            &["idle", "full", "idle", "full"]
        } else {
            &[mode]
        };
        let mut all_valid = true;
        for &mode in modes {
            if mode == "full" {
                markets = clients[0]
                    .snapshot()
                    .unwrap()
                    .markets()
                    .iter()
                    .map(|m| m.name().to_owned())
                    .collect();
                for client in &clients {
                    client
                        .streams()
                        .subscribe_all_trades(TradesStreamMode::TradesOnly)
                        .unwrap();
                }
            } else if mode == "trades" {
                for client in &clients {
                    client
                        .streams()
                        .subscribe_trades_for(TradesStreamMode::TradesOnly, markets.iter())
                        .unwrap();
                }
            }
            if mode == "idle" {
                for client in &clients {
                    client.streams().unsubscribe_all_trades().unwrap();
                }
            }
            let warmup = Instant::now();
            while warmup.elapsed() < Duration::from_secs(30) {
                drain(&clients, &mut events);
                std::thread::sleep(Duration::from_millis(50));
            }
            let before = counters(&clients, &markets);
            let per_client_before: Vec<_> =
                clients.iter().map(MoonClient::startup_status).collect();
            #[cfg(feature = "diagnostics")]
            let history_before: Vec<_> = clients
                .iter()
                .map(|c| {
                    c.snapshot()
                        .and_then(|s| s.history_profile_snapshot())
                        .unwrap_or_default()
                })
                .collect();
            #[cfg(feature = "diagnostics")]
            let profiles_before: Vec<_> = clients
                .iter()
                .map(MoonClient::protocol_metrics_snapshot)
                .collect();
            eprintln!(
                "BENCH measuring mode={mode} clients={count} seconds={seconds} retained_markets={}",
                if mode == "full" || mode == "trades" {
                    markets.len()
                } else {
                    0
                }
            );
            let started_unix_ms = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_millis();
            let usage_before = process_usage(true);
            let start = Instant::now();
            ALLOCS.store(0, Relaxed);
            BYTES.store(0, Relaxed);
            COUNT.store(args.get(5).is_some_and(|s| s == "alloc"), Relaxed);
            let mut event_count = 0;
            let mut server_logs = 0u64;
            #[allow(unused_mut)]
            let mut parse_errors = 0u64;
            let mut samples = Vec::new();
            let mut previous_usage = usage_before;
            let mut previous_sample_time = 0.0;
            let main_cycles_before = thread_cycles();
            let main_cpu_before = thread_cpu_ms();
            let (mut server_cpu, mut system_cpu, mut health_samples) = (0u64, 0u64, 0u64);
            let mut server_cpu_max = 0;
            let mut next_health = Instant::now();
            let mut disconnected = false;
            while start.elapsed() < Duration::from_secs(seconds) {
                for client in &clients {
                    events.clear();
                    client.drain_events_into(&mut events);
                    event_count += events.len() as u64;
                    for event in &events {
                        server_logs += u64::from(matches!(event, Event::ServerLog(_)));
                        #[cfg(feature = "diagnostics")]
                        {
                            parse_errors += u64::from(matches!(event, Event::ParseFailed { .. }));
                        }
                    }
                    client.drain_lifecycle_events();
                }
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
                    let usage = process_usage(cfg!(windows));
                    let sample_time = start.elapsed().as_secs_f64();
                    samples.push(serde_json::json!({"at_seconds":sample_time,
                    "cpu_ms":usage.0-previous_usage.0, "cycles":cfg!(windows).then_some(usage.1-previous_usage.1),
                    "seconds":sample_time-previous_sample_time, "private_mb":cfg!(windows).then_some(usage.2 as f64/1e6),
                    "ready":clients.iter().filter(|c| c.startup_status().state == moonproto::StartupState::Ready).count()}));
                    previous_usage = usage;
                    previous_sample_time = sample_time;
                    next_health = Instant::now() + Duration::from_secs(1);
                }
                std::thread::sleep(Duration::from_millis(50));
            }
            COUNT.store(false, Relaxed);
            let elapsed = start.elapsed().as_secs_f64();
            let main_cpu_after = thread_cpu_ms();
            let usage_after = process_usage(true);
            let main_cycles_after = thread_cycles();
            #[cfg(feature = "diagnostics")]
            let history_after: Vec<_> = clients
                .iter()
                .map(|c| {
                    c.snapshot()
                        .and_then(|s| s.history_profile_snapshot())
                        .unwrap_or_default()
                })
                .collect();
            #[cfg(feature = "diagnostics")]
            let profiles_after: Vec<_> = clients
                .iter()
                .map(MoonClient::protocol_metrics_snapshot)
                .collect();
            let per_client_after: Vec<_> = clients.iter().map(MoonClient::startup_status).collect();
            let after = counters(&clients, &markets);
            let receiving_clients = per_client_before
                .iter()
                .zip(&per_client_after)
                .filter(|(a, b)| b.current_port_received_bytes > a.current_port_received_bytes)
                .count();
            let valid = !disconnected
                && after.4 == before.4
                && receiving_clients == count
                && parse_errors == 0;
            println!(
                "{}",
                serde_json::json!({
                    "kind":"measurement", "clients": count, "mode": mode, "started_unix_ms":started_unix_ms,
                    "diagnostics":cfg!(feature = "diagnostics"), "allocation_counting":args.get(5).is_some_and(|s| s == "alloc"),
                    "logical_cpus":std::thread::available_parallelism().unwrap().get(),
                    "harness_cycles_per_sec":cfg!(windows).then_some(main_cycles_after.saturating_sub(main_cycles_before) as f64/elapsed),
                    "harness_cpu_ms_per_sec":main_cpu_before.zip(main_cpu_after).map(|(a,b)| (b-a)/elapsed),
                    "os":std::env::consts::OS, "private_memory_kind":if cfg!(windows) { "committed" } else { "resident" },
                    "receiving_clients":receiving_clients,
                    "observed_not_ready":disconnected, "port_rotations":after.4.saturating_sub(before.4),
                    "per_client_wire":per_client_before.iter().zip(&per_client_after).map(|(a,b)| serde_json::json!({
                        "state":format!("{:?}", b.state), "rotations":b.local_port_change_count.saturating_sub(a.local_port_change_count),
                        "rx_bytes":b.current_port_received_bytes.saturating_sub(a.current_port_received_bytes),
                        "rtt_ms":b.round_trip_ms, "delivery_percent":b.downlink_delivery_percent})).collect::<Vec<_>>(), "parse_errors":cfg!(feature = "diagnostics").then_some(parse_errors),
                    "server_logs_per_sec":server_logs as f64/elapsed,
                    "retained_markets_per_client":if mode == "full" || mode == "trades" { markets.len() } else { 0 }, "seconds": elapsed, "valid": valid,
                    "cpu_ms_per_sec": (usage_after.0 - usage_before.0) / elapsed,
                    "cycles_per_sec": cfg!(windows).then_some((usage_after.1 - usage_before.1) as f64 / elapsed),
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
            println!(
                "{}",
                serde_json::json!({"kind":"samples", "samples":samples})
            );
            #[cfg(feature = "diagnostics")]
            println!(
                "{}",
                serde_json::json!({"kind":"profiles", "before":profiles_before, "after":profiles_after, "history_before":history_before, "history_after":history_after})
            );
            all_valid &= valid;
        }
        for client in &clients {
            client.disconnect().unwrap();
        }
        for client in &clients {
            client.wait_finished().unwrap();
        }
        drop(clients);
        assert!(
            all_valid,
            "one or more measurement windows were invalid; inspect per-window results"
        );
    }
}

#[cfg(any(windows, target_os = "linux"))]
#[global_allocator]
static ALLOCATOR: bench::Allocator = bench::Allocator;

fn main() {
    #[cfg(any(windows, target_os = "linux"))]
    bench::run();
    #[cfg(not(any(windows, target_os = "linux")))]
    panic!("This benchmark supports Windows and Linux process counters");
}
