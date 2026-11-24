use std::net::Ipv4Addr;

/// The selected stream source. Reception can be disabled independently.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MoonStreamerSource {
    Common,
    Custom(Ipv4Addr),
}

/// Status of the receiver's last inspected packet; this is not an online flag.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MoonStreamerPacketStatus {
    Ok,
    ShortPacket,
    ChecksumError,
    Unknown(i32),
}

/// Receiver diagnostics sampled by the core, refreshed on demand.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MoonStreamerReceiver {
    /// The core has recently received traffic from the selected source.
    pub online: bool,
    pub last_packet_status: MoonStreamerPacketStatus,
    pub trades_per_second: i32,
    /// Subscriber count advertised by the source, not by this core's distributor.
    pub source_users: i32,
    pub last_block: u8,
    pub packet_loss_percent: f64,
    /// Highest smoothed packet-loss percentage observed by the receiver.
    pub max_packet_loss_percent: f64,
    pub checksum_errors: i32,
    pub last_trades: i32,
    pub max_trades: i32,
}

/// Local distributor socket state reported by the core.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MoonStreamerListenerStatus {
    /// No listener yet. With distribution enabled, the core may still be loading market data.
    NotStarted,
    /// The local UDP socket is active. This does not prove remote reachability.
    Listening,
    BindFailed,
    Unknown(u8),
}

/// Diagnostics for this core's distribution of trades to other bots.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MoonStreamerDistribution {
    pub listener_status: MoonStreamerListenerStatus,
    /// A port-check ping has been observed; not a current firewall test.
    pub port_ping_seen: bool,
    pub subscribers: i32,
    pub last_packet_bytes: i32,
    pub max_packet_bytes: i32,
    pub compression_percent: i32,
    pub sleep_ms: i32,
}

/// Core-confirmed Moon Streamer configuration and a diagnostic sample.
///
/// Available on MoonBot 7.73 N2+. Read it through
/// `snapshot().settings().runtime_state.moon_streamer` and request fresh
/// diagnostics with `client.moon_streamer().refresh()` while its UI is visible.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MoonStreamerState {
    /// Configured reception switch. `receiver.online` reports actual traffic.
    pub receive_enabled: bool,
    pub source: MoonStreamerSource,
    pub last_custom_server: Option<Ipv4Addr>,
    /// Configured distribution switch. `distribution.listener_status` reports the socket.
    pub distribution_enabled: bool,
    /// Whether the core permits extended actions other than a diagnostic refresh.
    pub can_configure: bool,
    pub reception_supported: bool,
    pub fast_charts: bool,
    /// Core-reported port; this number alone does not imply support or an open socket.
    pub udp_port: u16,
    pub receiver: MoonStreamerReceiver,
    pub distribution: MoonStreamerDistribution,
}
