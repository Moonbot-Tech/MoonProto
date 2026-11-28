use super::registry::CURRENT_PROTO_CMD_VER;
use super::strict_read::{read_f64, read_i64, read_str8, read_u32, read_u64, read_u8};
use super::trade::BaseCommandHeader;
use crate::{BaseCurrency, CancelledOrder, ExchangeCode, MoonTime, OrderType};
use std::sync::Arc;

pub(crate) fn build_request(uid: u64) -> Vec<u8> {
    let mut data = Vec::with_capacity(11);
    data.push(54);
    data.extend_from_slice(&CURRENT_PROTO_CMD_VER.to_le_bytes());
    data.extend_from_slice(&uid.to_le_bytes());
    data
}

pub(crate) fn read(input: &mut &[u8]) -> Option<(u64, Arc<CancelledOrder>)> {
    let header = BaseCommandHeader::read(input)?;
    let data = *input;
    let mut pos = 0;
    let sequence = read_u64(data, &mut pos)?;
    let market = read_str8(data, &mut pos)?;
    let flags = read_u8(data, &mut pos)?;
    let exchange = ExchangeCode::from_byte(read_u8(data, &mut pos)?);
    let base_currency = BaseCurrency::from_byte(read_u8(data, &mut pos)?);
    let strategy_id = read_u64(data, &mut pos)?;
    let order_type = OrderType::from_byte(read_u8(data, &mut pos)?);
    let created_at = MoonTime::from_unix_millis(read_i64(data, &mut pos)?);
    let cancelled_at = MoonTime::from_unix_millis(read_i64(data, &mut pos)?);
    let price = read_f64(data, &mut pos)?;
    let quantity = read_f64(data, &mut pos)?;
    let size = read_u32(data, &mut pos)? as usize;
    let traces = crate::state::parse_traces(data.get(pos..pos + size)?)?;
    *input = &data[pos + size..];
    Some((sequence, Arc::new(CancelledOrder { uid: header.uid, market, exchange, base_currency, strategy_id,
        is_emulator: flags & 1 != 0, is_short: flags & 2 != 0, order_type,
        created_at, cancelled_at, price, quantity, traces })))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands::trade::TradeCommand;

    #[test]
    fn delphi_fixture_decodes_without_touching_live_orders_and_duplicates_are_silent() {
        let packet = include_bytes!("../../tests/fixtures/cancelled-order.bin");
        assert_eq!(read(&mut packet.as_slice()).unwrap().0, 1234);
        let mut dispatcher = crate::events::EventDispatcher::new();
        let mut events = Vec::new();
        dispatcher.dispatch_into(crate::protocol::Command::Order, packet, 0, &mut events);
        let [crate::Event::CancelledOrderHistory(record)] = events.as_slice() else { panic!("{events:?}"); };
        assert_eq!(record.uid, 123);
        assert_eq!(record.market, "BTCUSDT");
        assert_eq!(record.strategy_id, 900);
        assert!(record.is_emulator && record.is_short);
        assert_eq!(record.exchange, ExchangeCode::FBinance);
        assert_eq!(record.base_currency, BaseCurrency::USDT);
        assert_eq!(record.created_at.unix_millis(), 1_800_000_000_123);
        assert_eq!(record.cancelled_at.unix_millis(), 1_800_000_001_123);
        assert_eq!(record.price, 123.45);
        assert_eq!(record.quantity, 1.25);
        assert!(record.traces.is_empty());
        let held = dispatcher.snapshot();
        assert!(held.orders().is_empty());
        assert_eq!(held.cancelled_orders().len(), 1);
        events.clear();
        dispatcher.dispatch_into(crate::protocol::Command::Order, packet, 0, &mut events);
        assert!(events.is_empty());
        assert_eq!(dispatcher.snapshot().cancelled_orders().len(), 1);
        assert_eq!(held.cancelled_orders().get(123).unwrap() as *const _,
            dispatcher.snapshot().cancelled_orders().get(123).unwrap() as *const _);
        for len in 0..packet.len() {
            assert!(TradeCommand::parse(&packet[..len]).is_none(), "truncated at {len}");
        }
    }

    #[test]
    fn history_preserves_archived_geometry_and_rejects_truncated_points() {
        let mut packet = include_bytes!("../../tests/fixtures/cancelled-order.bin").to_vec();
        packet.truncate(packet.len() - 4);
        let mut trace = vec![1, 1, 1];
        trace.extend_from_slice(&95.0_f64.to_le_bytes());
        trace.extend_from_slice(&1_800_000_000_999_i64.to_le_bytes());
        trace.extend_from_slice(&1_u32.to_le_bytes());
        trace.extend_from_slice(&1_800_000_000_123_i64.to_le_bytes());
        trace.extend_from_slice(&123.456789012345_f64.to_le_bytes());
        packet.extend_from_slice(&(trace.len() as u32).to_le_bytes());
        packet.extend_from_slice(&trace);
        let Some(TradeCommand::CancelledOrderHistory(_, order)) = TradeCommand::parse(&packet) else { panic!(); };
        assert_eq!(order.traces.len(), 1);
        assert_eq!(order.traces[0].points[0].price, 123.456789012345);
        assert_eq!(order.traces[0].points[0].time.unix_millis(), 1_800_000_000_123);
        assert_eq!(order.traces[0].stop_price, 95.0);
        packet.pop();
        assert!(TradeCommand::parse(&packet).is_none());
    }
}
