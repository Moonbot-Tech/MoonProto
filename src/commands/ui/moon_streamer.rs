use crate::state::{MoonStreamerDistribution, MoonStreamerListenerStatus, MoonStreamerPacketStatus, MoonStreamerReceiver,
    MoonStreamerSource, MoonStreamerState};
use std::net::Ipv4Addr;
use zerocopy::byteorder::little_endian::{F64, I32, U16, U32};
use zerocopy::{FromBytes, Immutable, KnownLayout, Unaligned};

#[repr(C, packed)]
#[derive(FromBytes, Immutable, KnownLayout, Unaligned)]
struct WireMoonStreamerState {
    server_ip: U32,
    last_own_ip: U32,
    use_server: u8,
    can_configure: u8,
    reader_supported: u8,
    fast_charts: u8,
    online: u8,
    reader_status: I32,
    trades_per_second: I32,
    total_users: I32,
    last_block: u8,
    packet_loss: F64,
    packet_loss_max: F64,
    checksum_errors: I32,
    last_trades: I32,
    max_trades: I32,
    port_udp: U16,
    listener_status: u8,
    port_ping_seen: u8,
    users_count: I32,
    last_packet_size: I32,
    max_packet_size: I32,
    compression_rate: I32,
    sleep_time: I32,
}

const _: [(); 78] = [(); std::mem::size_of::<WireMoonStreamerState>()];

pub(super) fn parse(data: &[u8], receive_enabled: bool) -> Option<MoonStreamerState> {
    let wire = WireMoonStreamerState::read_from_prefix(data).ok()?.0;
    Some(MoonStreamerState {
        receive_enabled,
        source: match wire.server_ip.get() {
            0 => MoonStreamerSource::Common,
            ip => MoonStreamerSource::Custom(Ipv4Addr::from(ip)),
        },
        last_custom_server: (wire.last_own_ip.get() != 0).then(|| Ipv4Addr::from(wire.last_own_ip.get())),
        distribution_enabled: wire.use_server != 0,
        can_configure: wire.can_configure != 0,
        reception_supported: wire.reader_supported != 0,
        fast_charts: wire.fast_charts != 0,
        udp_port: wire.port_udp.get(),
        receiver: MoonStreamerReceiver {
            online: wire.online != 0,
            last_packet_status: match wire.reader_status.get() {
                0 => MoonStreamerPacketStatus::Ok,
                1 => MoonStreamerPacketStatus::ShortPacket,
                2 => MoonStreamerPacketStatus::ChecksumError,
                value => MoonStreamerPacketStatus::Unknown(value),
            },
            trades_per_second: wire.trades_per_second.get(),
            source_users: wire.total_users.get(),
            last_block: wire.last_block,
            packet_loss_percent: wire.packet_loss.get(),
            max_packet_loss_percent: wire.packet_loss_max.get(),
            checksum_errors: wire.checksum_errors.get(),
            last_trades: wire.last_trades.get(),
            max_trades: wire.max_trades.get(),
        },
        distribution: MoonStreamerDistribution {
            listener_status: match wire.listener_status {
                0 => MoonStreamerListenerStatus::NotStarted,
                1 => MoonStreamerListenerStatus::Listening,
                2 => MoonStreamerListenerStatus::BindFailed,
                value => MoonStreamerListenerStatus::Unknown(value),
            },
            port_ping_seen: wire.port_ping_seen != 0,
            subscribers: wire.users_count.get(),
            last_packet_bytes: wire.last_packet_size.get(),
            max_packet_bytes: wire.max_packet_size.get(),
            compression_percent: wire.compression_rate.get(),
            sleep_ms: wire.sleep_time.get(),
        },
    })
}

#[cfg(test)]
pub(crate) fn runtime_fixture() -> Vec<u8> {
    let mut data = Vec::new();
    super::write_header(&mut data, 20, 42);
    data.extend_from_slice(&[1, 0]);
    data.extend_from_slice(&773u32.to_le_bytes());
    data.push(1); // Reception enabled, despite the offline sample below.
    data.extend_from_slice(&0xcb007109u32.to_le_bytes()); // 203.0.113.9
    data.extend_from_slice(&0xc6336407u32.to_le_bytes()); // 198.51.100.7
    data.extend_from_slice(&[1, 1, 0, 1, 0]); // distribution, configuration, support, fast charts, online
    for value in [2i32, 1234, 56] { data.extend_from_slice(&value.to_le_bytes()); }
    data.push(250);
    for value in [1.25f64, 2.5] { data.extend_from_slice(&value.to_le_bytes()); }
    for value in [3i32, 456, 789] { data.extend_from_slice(&value.to_le_bytes()); }
    data.extend_from_slice(&2503u16.to_le_bytes());
    data.extend_from_slice(&[1, 0]);
    for value in [4i32, 1200, 1460, 42, 24] { data.extend_from_slice(&value.to_le_bytes()); }
    assert_eq!(data.len(), 18 + 78);
    data
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands::ui::UICommand;

    #[test]
    fn extended_runtime_decodes_configuration_and_diagnostics_without_conflating_switches() {
        let raw = runtime_fixture();
        let UICommand::RuntimeState(runtime) = UICommand::parse(&raw).unwrap() else { panic!() };
        let state = runtime.moon_streamer.unwrap();
        assert_eq!(state, MoonStreamerState {
            receive_enabled: true, source: MoonStreamerSource::Custom(Ipv4Addr::new(203, 0, 113, 9)),
            last_custom_server: Some(Ipv4Addr::new(198, 51, 100, 7)), distribution_enabled: true,
            can_configure: true, reception_supported: false, fast_charts: true, udp_port: 2503,
            receiver: MoonStreamerReceiver {
                online: false, last_packet_status: MoonStreamerPacketStatus::ChecksumError,
                trades_per_second: 1234, source_users: 56, last_block: 250,
                packet_loss_percent: 1.25, max_packet_loss_percent: 2.5, checksum_errors: 3,
                last_trades: 456, max_trades: 789,
            },
            distribution: MoonStreamerDistribution {
                listener_status: MoonStreamerListenerStatus::Listening,
                port_ping_seen: false, subscribers: 4, last_packet_bytes: 1200,
                max_packet_bytes: 1460, compression_percent: 42, sleep_ms: 24,
            },
        });
        assert_eq!(runtime.moon_stream_enabled, Some(true));
    }

    #[test]
    fn absent_or_partial_extension_stays_unknown_and_replaces_old_retained_state() {
        let raw = runtime_fixture();
        for size in 13..raw.len() {
            let UICommand::RuntimeState(state) = UICommand::parse(&raw[..size]).unwrap() else { panic!() };
            assert!(state.moon_streamer.is_none(), "partial tail size={size}");
            assert_eq!(state.moon_stream_enabled, (size >= 18).then_some(true));
        }
        let mut settings = crate::state::SettingsState::new();
        settings.apply(UICommand::parse(&raw).unwrap());
        let retained = settings.clone();
        settings.apply(UICommand::parse(&raw[..18]).unwrap());
        assert!(settings.runtime_state.unwrap().moon_streamer.is_none());
        assert!(retained.runtime_state.unwrap().moon_streamer.is_some());
        let mut future = raw.clone();
        future.extend_from_slice(b"future tail");
        let UICommand::RuntimeState(future) = UICommand::parse(&future).unwrap() else { panic!() };
        assert_eq!(future, retained.runtime_state.unwrap());
    }

    #[test]
    fn zero_addresses_mean_common_source_and_no_custom_history() {
        let mut raw = runtime_fixture();
        raw[18..26].fill(0);
        let UICommand::RuntimeState(runtime) = UICommand::parse(&raw).unwrap() else { panic!() };
        let state = runtime.moon_streamer.unwrap();
        assert_eq!(state.source, MoonStreamerSource::Common);
        assert_eq!(state.last_custom_server, None);
    }

    #[test]
    fn listener_waiting_listening_and_bind_failure_remain_distinct() {
        for (value, expected) in [(0, MoonStreamerListenerStatus::NotStarted),
            (1, MoonStreamerListenerStatus::Listening), (2, MoonStreamerListenerStatus::BindFailed),
            (99, MoonStreamerListenerStatus::Unknown(99))]
        {
            let mut raw = runtime_fixture();
            raw[18 + 56] = value;
            let UICommand::RuntimeState(runtime) = UICommand::parse(&raw).unwrap() else { panic!() };
            let state = runtime.moon_streamer.unwrap();
            assert!(state.distribution_enabled);
            assert_eq!(state.distribution.listener_status, expected);
        }
    }
}
