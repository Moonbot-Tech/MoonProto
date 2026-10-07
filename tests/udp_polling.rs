//! Cross-platform UDP readiness contract for the runtime loop.
//!
//! The active runtime relies on "poll says readable -> drain UDP until
//! WouldBlock -> rearm". This test keeps that OS-level assumption separate from
//! live MoonProto behavior, so a platform polling regression does not get
//! misdiagnosed as a server/protocol failure.

use polling::{Event, Events, Poller};
use std::io;
use std::net::UdpSocket;
use std::time::{Duration, Instant};

#[test]
fn expired_deadline_still_delivers_readiness_and_notify_does_not_latch() -> io::Result<()> {
    let peer = UdpSocket::bind("127.0.0.1:0")?;
    let socket = UdpSocket::bind("127.0.0.1:0")?;
    socket.set_nonblocking(true)?;
    let poller = Poller::new()?;
    let mut events = Events::new();
    // Safety: the socket is removed before either object is dropped.
    unsafe { poller.add(&socket, Event::readable(1))?; }
    for byte in 0..20 {
        peer.send_to(&[byte], socket.local_addr()?)?;
        // A past deadline is a nonblocking poll, not permission to skip I/O.
        // IOCP may first process an internal completion, so allow another poll.
        let deadline = Instant::now() - Duration::from_secs(1);
        let limit = Instant::now() + Duration::from_secs(1);
        loop {
            events.clear();
            poller.wait_deadline(&mut events, deadline)?;
            if events.iter().any(|event| event.key == 1 && event.readable) {
                break;
            }
            assert!(Instant::now() < limit, "expired deadline lost queued UDP readiness");
            std::thread::yield_now();
        }
        assert_eq!(drain_udp(&socket)?, vec![vec![byte]]);
        poller.modify(&socket, Event::readable(1))?;
        poller.notify()?;
        events.clear();
        poller.wait_deadline(&mut events, deadline)?;
        assert!(events.is_empty());
        let start = Instant::now();
        poller.wait(&mut events, Some(Duration::from_millis(2)))?;
        assert!(events.is_empty());
        assert!(start.elapsed() >= Duration::from_millis(2), "consumed notify kept the poller awake");
    }
    poller.delete(&socket)?;
    Ok(())
}

#[test]
fn notifications_survive_udp_readiness_with_single_event_capacity() -> io::Result<()> {
    let peer = UdpSocket::bind("127.0.0.1:0")?;
    let socket = UdpSocket::bind("127.0.0.1:0")?;
    socket.set_nonblocking(true)?;
    let poller = Poller::new()?;
    let mut events = Events::with_capacity(std::num::NonZeroUsize::new(1).unwrap());
    // Safety: registration is removed before the socket is dropped.
    unsafe { poller.add(&socket, Event::readable(1))?; }

    for value in 0..100u8 {
        // Both readiness sources compete for a single event slot. Repeated
        // notifications may coalesce, but must not stick after being consumed.
        peer.send_to(&[value], socket.local_addr()?)?;
        poller.notify()?;
        poller.notify()?;
        wait_for_readable(&poller, &mut events, 1, Duration::from_secs(1))?;
        assert_eq!(drain_udp(&socket)?, vec![vec![value]]);
        poller.modify(&socket, Event::readable(1))?;
        // A notification may still occupy the ready list after the UDP event.
        events.clear();
        poller.wait(&mut events, Some(Duration::ZERO))?;
        assert!(events.is_empty());
        events.clear();
        let start = Instant::now();
        poller.wait(&mut events, Some(Duration::from_millis(1)))?;
        assert!(events.is_empty());
        assert!(start.elapsed() >= Duration::from_millis(1));
    }
    poller.delete(&socket)
}

#[test]
fn empty_timeouts_preserve_later_readiness_and_notifications() -> io::Result<()> {
    let peer = UdpSocket::bind("127.0.0.1:0")?;
    let socket = UdpSocket::bind("127.0.0.1:0")?;
    socket.set_nonblocking(true)?;
    let poller = std::sync::Arc::new(Poller::new()?);
    let mut events = Events::new();
    // Safety: registration is removed before the socket is dropped.
    unsafe { poller.add(&socket, Event::readable(1))?; }
    for value in 0..10u8 {
        let timeout = Duration::from_millis(if value % 2 == 0 { 5 } else { 15 });
        events.clear();
        let start = Instant::now();
        poller.wait(&mut events, Some(timeout))?;
        assert!(events.is_empty());
        assert!(start.elapsed() >= timeout);

        peer.send_to(&[value], socket.local_addr()?)?;
        wait_for_readable(&poller, &mut events, 1, Duration::from_secs(1))?;
        assert_eq!(drain_udp(&socket)?, vec![vec![value]]);
        poller.modify(&socket, Event::readable(1))?;

        let notifier = std::sync::Arc::clone(&poller);
        let wake = std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(10));
            notifier.notify().unwrap();
        });
        events.clear();
        let start = Instant::now();
        poller.wait(&mut events, Some(Duration::from_secs(2)))?;
        assert!(start.elapsed() < Duration::from_secs(1));
        wake.join().unwrap();
        assert!(events.is_empty());
    }
    // An already posted notification must also wake the following wait.
    poller.notify()?;
    poller.wait(&mut events, Some(Duration::from_secs(2)))?;
    assert!(events.is_empty());
    poller.delete(&socket)?;
    Ok(())
}

fn wait_for_readable(
    poller: &Poller,
    events: &mut Events,
    key: usize,
    timeout: Duration,
) -> io::Result<()> {
    let deadline = Instant::now() + timeout;
    loop {
        events.clear();
        let now = Instant::now();
        if now >= deadline {
            return Err(io::Error::new(
                io::ErrorKind::TimedOut,
                "poller did not report UDP readable event",
            ));
        }
        poller.wait(events, Some(deadline.saturating_duration_since(now)))?;
        if events.iter().any(|ev| ev.key == key && ev.readable) {
            return Ok(());
        }
    }
}

fn drain_udp(sock: &UdpSocket) -> io::Result<Vec<Vec<u8>>> {
    let mut out = Vec::new();
    let mut buf = [0u8; 2048];
    loop {
        match sock.recv_from(&mut buf) {
            Ok((n, _)) => out.push(buf[..n].to_vec()),
            Err(e) if e.kind() == io::ErrorKind::WouldBlock => return Ok(out),
            Err(e) => return Err(e),
        }
    }
}

#[test]
fn cross_platform_udp_poller_drains_to_wouldblock_and_survives_rebind() -> io::Result<()> {
    let peer = UdpSocket::bind("127.0.0.1:0")?;
    let sock = UdpSocket::bind("127.0.0.1:0")?;
    sock.set_nonblocking(true)?;
    let addr = sock.local_addr()?;

    let poller = Poller::new()?;
    let mut events = Events::new();

    // Safety: the socket is deleted from this poller before it is dropped.
    unsafe {
        poller.add(&sock, Event::readable(1))?;
    }

    events.clear();
    poller.wait(&mut events, Some(Duration::from_millis(5)))?;
    assert!(
        events.iter().all(|ev| ev.key != 1 || !ev.readable),
        "fresh UDP socket must not be reported as readable before packets arrive",
    );

    for i in 0..3u8 {
        peer.send_to(&[i], addr)?;
    }
    wait_for_readable(&poller, &mut events, 1, Duration::from_secs(1))?;

    let drained = drain_udp(&sock)?;
    assert_eq!(
        drained,
        vec![vec![0], vec![1], vec![2]],
        "readable event must allow recv drain until WouldBlock without toggling socket options",
    );

    poller.modify(&sock, Event::readable(1))?;
    events.clear();
    poller.wait(&mut events, Some(Duration::from_millis(5)))?;
    assert!(
        events.iter().all(|ev| ev.key != 1 || !ev.readable),
        "after drain-to-WouldBlock and rearm, socket must go quiet again",
    );

    poller.delete(&sock)?;
    drop(sock);

    let rebound = UdpSocket::bind("127.0.0.1:0")?;
    rebound.set_nonblocking(true)?;
    let rebound_addr = rebound.local_addr()?;
    // Safety: the rebound socket is deleted from this poller before it is dropped.
    unsafe {
        poller.add(&rebound, Event::readable(2))?;
    }

    peer.send_to(&[9], rebound_addr)?;
    wait_for_readable(&poller, &mut events, 2, Duration::from_secs(1))?;
    assert_eq!(drain_udp(&rebound)?, vec![vec![9]]);

    poller.delete(&rebound)?;
    Ok(())
}
