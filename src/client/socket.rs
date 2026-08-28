//! UDP socket options used by the MoonProto client.

use crate::protocol::slicing;
use polling::{Events as PollEvents, Poller};
use std::net::{SocketAddr, UdpSocket};

/// Transport state carved out of [`super::Client`]: the UDP socket, the
/// receive/send buffers, the slicing receiver, the bind-port cursor, the cached
/// server address, and the bind-failure tracking. These are the hot recv/send
/// path. Fields moved from `Client` keep their original meaning; passive
/// per-socket packet counters live here because this object owns socket
/// replacement.
///
/// Note: `last_socket_recreate` is **not** here — it is a reconnect-throttle
/// clock owned by the handshake/reconnect core and stays on `Client`.
pub(crate) struct ClientTransport {
    /// Main-thread UDP socket. `None` until the first successful `bind_socket`.
    pub(crate) socket: Option<UdpSocket>,
    /// Sliced-datagram reassembly receiver (matches Delphi `SlicingReceiver`).
    pub(crate) recv_slicer: slicing::SlicingReceiver,
    /// Readiness poller registered on `socket`; `None` when polling is unavailable
    /// (falls back to a 5 ms nonblocking recv probe).
    pub(crate) recv_poller: Option<Poller>,
    /// Reusable event buffer for `recv_poller.wait`.
    pub(crate) recv_events: PollEvents,
    /// Cached resolved server address; cleared on bind and on a resolve error.
    pub(crate) cached_server_addr: Option<SocketAddr>,
    /// Next UDP bind port to try (200-port walk in `bind_socket`).
    pub(crate) next_port: u16,
    /// Local UDP port of the currently bound socket.
    pub(crate) current_local_port: Option<u16>,
    /// Successfully sent physical UDP datagrams on the current socket.
    pub(crate) current_sent_packets: u64,
    /// Physical UDP datagrams returned by `recv_from` on the current socket.
    pub(crate) current_received_packets: u64,
    /// Local UDP port closed by the latest automatic reconnect.
    pub(crate) previous_local_port: Option<u16>,
    /// Successfully sent physical UDP datagrams before that socket was closed.
    pub(crate) previous_sent_packets: u64,
    /// Physical UDP datagrams received before that socket was closed.
    pub(crate) previous_received_packets: u64,
    /// Number of sockets closed for automatic reconnect/port rotation.
    pub(crate) rebind_count: u32,
    /// How many consecutive `bind_socket` 200-port walks failed (for `BindFailed`).
    pub(crate) bind_failure_streak: u32,
    /// Wall-clock ms of the first bind failure in the current streak.
    pub(crate) first_bind_failure_ms: i64,
    /// Wall-clock ms of the last emitted `BindFailed` event (event throttle).
    pub(crate) last_bind_failed_event_ms: i64,
    /// Cached SipHash-1-3 MAC context for `cfg.mac_key` — fixed for the whole
    /// life of the Client, reused by the recv/send phases.
    pub(crate) mac_ctx: crate::transport::MacContext,
    /// Delphi `SentCountDNS` equivalent for transport mode V2.
    pub(crate) transport_mode_state: crate::transport::ClientTransportModeState,
    /// Reusable client transport pack buffer — reused across outgoing packets to
    /// avoid an alloc/dealloc per send; capacity grows up to the peak packet size.
    pub(crate) send_buf: Vec<u8>,
}

impl ClientTransport {
    pub(crate) fn new(mac_ctx: crate::transport::MacContext, next_port: u16) -> Self {
        Self {
            socket: None,
            recv_slicer: slicing::SlicingReceiver::new(),
            recv_poller: None,
            recv_events: PollEvents::new(),
            cached_server_addr: None,
            next_port,
            current_local_port: None,
            current_sent_packets: 0,
            current_received_packets: 0,
            previous_local_port: None,
            previous_sent_packets: 0,
            previous_received_packets: 0,
            rebind_count: 0,
            bind_failure_streak: 0,
            first_bind_failure_ms: super::constants::NEVER_TIME_MS,
            last_bind_failed_event_ms: super::constants::NEVER_TIME_MS,
            mac_ctx,
            transport_mode_state: crate::transport::ClientTransportModeState::new(),
            send_buf: Vec::with_capacity(2048), // typical send packet ~500-1500 bytes
        }
    }

    pub(crate) fn install_socket(&mut self, socket: UdpSocket, local_port: u16) {
        self.socket = Some(socket);
        self.current_local_port = Some(local_port);
        self.current_sent_packets = 0;
        self.current_received_packets = 0;
    }

    pub(crate) fn close_for_rebind(&mut self) {
        if self.socket.take().is_none() {
            return;
        }
        self.previous_local_port = self.current_local_port.take();
        self.previous_sent_packets = self.current_sent_packets;
        self.previous_received_packets = self.current_received_packets;
        self.current_sent_packets = 0;
        self.current_received_packets = 0;
        self.rebind_count = self.rebind_count.wrapping_add(1);
    }
}

/// Set SO_RCVBUF + SO_SNDBUF to 8 MB via socket2 (cross-platform).
///
/// MoonProto can receive short UDP bursts during snapshots/recovery. A small
/// kernel buffer turns those bursts into silent packet loss, so the socket asks
/// for a larger buffer and logs when the OS clamps or rejects it.
pub(crate) fn set_socket_buffers(sock: &UdpSocket) {
    let sock2 = socket2::SockRef::from(sock);
    if let Err(e) = sock2.set_recv_buffer_size(8 * 1024 * 1024) {
        log::warn!("SO_RCVBUF=8MB rejected by OS (probably net.core.rmem_max too small): {e}");
    }
    if let Err(e) = sock2.set_send_buffer_size(8 * 1024 * 1024) {
        log::warn!("SO_SNDBUF=8MB rejected by OS: {e}");
    }
}

/// Cross-platform IP_DONTFRAGMENT / IP_MTU_DISCOVER / IP_DONTFRAG.
///
/// SizeAck/ProbeMTUAck are the protocol's PMTU probes, so they must exercise
/// the real no-fragment path instead of letting the OS fragment them. IPv4 and
/// IPv6 use different socket options; setting both paths explicitly keeps PMTU
/// discovery honest on desktop and dual-stack clients. Failures are warn-logged
/// because a rejected option changes recovery behaviour under packet loss.
pub(crate) fn set_dont_fragment_for_socket(sock: &UdpSocket, enable: bool) {
    // Determine IPv6 vs IPv4 from the local address. If local_addr returned an error — fall back
    // to IPv4 semantics (most systems are IPv4 by default).
    let is_v6 = sock.local_addr().map(|a| a.is_ipv6()).unwrap_or(false);

    #[cfg(target_os = "windows")]
    {
        use std::os::windows::io::AsRawSocket;
        let raw = sock.as_raw_socket();
        let val: i32 = if enable { 1 } else { 0 };
        // IPPROTO_IP=0, IP_DONTFRAGMENT=14; IPPROTO_IPV6=41, IPV6_DONTFRAG=14 (Win 10+ same value).
        let (level, optname) = if is_v6 { (41, 14) } else { (0, 14) };
        let rc = unsafe {
            extern "system" {
                fn setsockopt(
                    s: usize,
                    level: i32,
                    optname: i32,
                    optval: *const i8,
                    optlen: i32,
                ) -> i32;
            }
            setsockopt(
                raw as usize,
                level,
                optname,
                &val as *const i32 as *const i8,
                4,
            )
        };
        if rc != 0 {
            log::warn!(target: "moonproto::client",
                "set_dont_fragment_for_socket: setsockopt(level={level}, optname={optname}, v6={is_v6}) failed rc={rc} (Windows); PMTU discovery may be inaccurate");
        }
    }
    #[cfg(any(target_os = "linux", target_os = "android"))]
    {
        use std::os::fd::AsRawFd;
        let fd = sock.as_raw_fd();
        // IPv4: IPPROTO_IP=0, IP_MTU_DISCOVER=10.
        // IPv6: IPPROTO_IPV6=41, IPV6_MTU_DISCOVER=23.
        //
        // Linux `IP_PMTUDISC_DO` (2) sets DF, but also rejects datagrams above
        // the already cached route PMTU before they leave the host. MoonProto's
        // SizeAck/ProbeMTUAck packets are the PMTU probe itself: they must be
        // sent with DF while bypassing the cached path-MTU estimate, otherwise
        // the server never sees candidate sizes above Linux's stale/fallback
        // cache. `IP_PMTUDISC_PROBE` (3) is the Linux mode for that exact
        // packetization model; disabling returns to `IP_PMTUDISC_DONT` (0),
        // matching Delphi's "DF only around probe ack" behavior.
        let val: i32 = if enable { 3 } else { 0 };
        let (level, optname) = if is_v6 { (41, 23) } else { (0, 10) };
        let rc = unsafe {
            extern "C" {
                fn setsockopt(
                    s: i32,
                    level: i32,
                    optname: i32,
                    optval: *const i8,
                    optlen: u32,
                ) -> i32;
            }
            setsockopt(fd, level, optname, &val as *const i32 as *const i8, 4)
        };
        if rc != 0 {
            log::warn!(target: "moonproto::client",
                "set_dont_fragment_for_socket: setsockopt(level={level}, optname={optname}, v6={is_v6}) failed rc={rc} (Linux/Android); PMTU discovery may be inaccurate");
        }
    }
    #[cfg(any(target_os = "macos", target_os = "ios"))]
    {
        use std::os::fd::AsRawFd;
        let fd = sock.as_raw_fd();
        // IPv4: IPPROTO_IP=0, IP_DONTFRAG=28
        // IPv6: IPPROTO_IPV6=41, IPV6_DONTFRAG=62
        let val: i32 = if enable { 1 } else { 0 };
        let (level, optname) = if is_v6 { (41, 62) } else { (0, 28) };
        let rc = unsafe {
            extern "C" {
                fn setsockopt(
                    s: i32,
                    level: i32,
                    optname: i32,
                    optval: *const i8,
                    optlen: u32,
                ) -> i32;
            }
            setsockopt(fd, level, optname, &val as *const i32 as *const i8, 4)
        };
        if rc != 0 {
            log::warn!(target: "moonproto::client",
                "set_dont_fragment_for_socket: setsockopt(level={level}, optname={optname}, v6={is_v6}) failed rc={rc} (macOS/iOS); PMTU discovery may be inaccurate");
        }
    }
    #[cfg(not(any(
        target_os = "windows",
        target_os = "linux",
        target_os = "android",
        target_os = "macos",
        target_os = "ios"
    )))]
    {
        // Other platforms (BSD, etc.) — no-op for safety, PMTU discovery does not work.
        let _ = (sock, enable, is_v6);
    }
}
