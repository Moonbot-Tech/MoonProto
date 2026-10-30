//! Isolated OS receive/rearm benchmark, without per-packet protocol diagnostics.
//! Run in release mode with --ignored --nocapture --test-threads=1.
use polling::{Event, Events, Poller};
use std::net::UdpSocket;
use std::time::{Duration, Instant};

#[allow(dead_code)]
#[path = "../src/client/thread_cpu.rs"]
mod thread_cpu;

#[derive(Clone, Copy, Debug)]
enum Rearm {
    AfterPacket,
    BeforeWait,
}

fn wait(poller: &Poller, socket: &UdpSocket, events: &mut Events, mode: Rearm, calls: &mut u64) {
    if matches!(mode, Rearm::BeforeWait) && !events.is_empty() {
        poller.modify(socket, Event::readable(1)).unwrap();
        *calls += 1;
    }
    events.clear();
    poller.wait(events, Some(Duration::from_millis(30))).unwrap();
}

fn after_packet(poller: &Poller, socket: &UdpSocket, mode: Rearm, calls: &mut u64) {
    if matches!(mode, Rearm::AfterPacket) {
        poller.modify(socket, Event::readable(1)).unwrap();
        *calls += 1;
    }
}

fn sockets() -> (UdpSocket, UdpSocket, Poller) {
    let peer = UdpSocket::bind("127.0.0.1:0").unwrap();
    let socket = UdpSocket::bind("127.0.0.1:0").unwrap();
    socket.set_nonblocking(true).unwrap();
    socket2::SockRef::from(&socket).set_recv_buffer_size(1 << 20).unwrap();
    let poller = Poller::new().unwrap();
    // Safety: each benchmark deletes the registration before dropping the socket.
    unsafe { poller.add(&socket, Event::readable(1)).unwrap(); }
    (peer, socket, poller)
}

fn percentile(values: &mut [f64], fraction: f64) -> f64 {
    values.sort_by(f64::total_cmp);
    values[((values.len() - 1) as f64 * fraction).round() as usize]
}

#[test]
#[ignore = "paired OS receive benchmark; release mode, --nocapture --test-threads=1"]
fn udp_rearm_benchmark() {
    for burst in [1, 32, 128] {
        for sample in 0..12 {
            // Reverse order every pair to avoid a systematic warmup/frequency bias.
            let modes = if sample % 2 == 0 {
                [Rearm::AfterPacket, Rearm::BeforeWait]
            } else {
                [Rearm::BeforeWait, Rearm::AfterPacket]
            };
            for mode in modes {
                let (peer, socket, poller) = sockets();
                let addr = socket.local_addr().unwrap();
                let mut events = Events::new();
                let mut calls = 0;
                let (mut wall, mut cpu, mut cycles) = (0.0, 0.0, 0u64);
                let mut packet = [0u8; 128];
                let mut received = [0u8; 128];
                for first in (0..8192u64).step_by(burst) {
                    // Sending and constructing the workload are outside receiver CPU/wall time.
                    for seq in first..first + burst as u64 {
                        packet[..8].copy_from_slice(&seq.to_le_bytes());
                        peer.send_to(&packet, addr).unwrap();
                    }
                    let timer = thread_cpu::ThreadCpuTimer::start();
                    let start = Instant::now();
                    wait(&poller, &socket, &mut events, mode, &mut calls);
                    assert!(!events.is_empty(), "queued burst must be readable");
                    for seq in first..first + burst as u64 {
                        assert_eq!(socket.recv_from(&mut received).unwrap().0, packet.len());
                        assert_eq!(u64::from_le_bytes(received[..8].try_into().unwrap()), seq);
                        after_packet(&poller, &socket, mode, &mut calls);
                    }
                    assert_eq!(socket.recv_from(&mut received).unwrap_err().kind(), std::io::ErrorKind::WouldBlock);
                    wall += start.elapsed().as_secs_f64() * 1000.0;
                    let elapsed = timer.elapsed();
                    cpu += elapsed.time.map_or(0.0, |t| t.as_secs_f64() * 1000.0);
                    cycles += elapsed.cycles.unwrap_or(0);
                }
                // The final consumed notification is rearmed on the next wait, just as in runtime.
                assert_eq!(calls, match mode {
                    Rearm::AfterPacket => 8192,
                    Rearm::BeforeWait => 8192 / burst as u64 - 1,
                });
                poller.delete(&socket).unwrap();
                println!("REARM burst={burst} sample={sample} mode={mode:?} packets=8192 rearm={calls} wall_ms={wall:.6} cpu_ms={cpu:.6} Mcycles={:.6}", cycles as f64 / 1e6);
            }
        }
    }
}

#[test]
#[ignore = "paired UDP wake latency benchmark; release mode, --nocapture --test-threads=1"]
fn udp_rearm_wake_latency() {
    for sample in 0..4 {
        let modes = if sample % 2 == 0 {
            [Rearm::AfterPacket, Rearm::BeforeWait]
        } else {
            [Rearm::BeforeWait, Rearm::AfterPacket]
        };
        for mode in modes {
            let (peer, socket, poller) = sockets();
            let addr = socket.local_addr().unwrap();
            let origin = Instant::now();
            let (ack, rx) = std::sync::mpsc::channel();
            let sender = std::thread::spawn(move || {
                for seq in 0..200u64 {
                    std::thread::sleep(Duration::from_millis(2));
                    let mut packet = [0u8; 16];
                    packet[..8].copy_from_slice(&seq.to_le_bytes());
                    packet[8..].copy_from_slice(&(origin.elapsed().as_nanos() as u64).to_le_bytes());
                    peer.send_to(&packet, addr).unwrap();
                    rx.recv_timeout(Duration::from_secs(1)).unwrap();
                }
            });
            let mut events = Events::new();
            let mut calls = 0;
            let mut latencies = Vec::new();
            let mut packet = [0u8; 16];
            for seq in 0..200u64 {
                let deadline = Instant::now() + Duration::from_secs(1);
                loop {
                    wait(&poller, &socket, &mut events, mode, &mut calls);
                    if !events.is_empty() { break; }
                    assert!(Instant::now() < deadline, "readiness lost");
                }
                assert_eq!(socket.recv_from(&mut packet).unwrap().0, packet.len());
                assert_eq!(u64::from_le_bytes(packet[..8].try_into().unwrap()), seq);
                let sent = u64::from_le_bytes(packet[8..].try_into().unwrap());
                latencies.push((origin.elapsed().as_nanos() as u64 - sent) as f64 / 1000.0);
                after_packet(&poller, &socket, mode, &mut calls);
                ack.send(()).unwrap();
            }
            sender.join().unwrap();
            poller.delete(&socket).unwrap();
            println!("WAKE sample={sample} mode={mode:?} packets=200 us_p50={:.3} us_p95={:.3} us_p99={:.3} us_max={:.3}",
                percentile(&mut latencies, 0.50), percentile(&mut latencies, 0.95),
                percentile(&mut latencies, 0.99), percentile(&mut latencies, 1.0));
        }
    }
}
