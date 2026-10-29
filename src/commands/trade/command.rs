//! Parsed `MPC_Order` command envelope.

use super::*;
use crate::commands::registry::CURRENT_PROTO_CMD_VER;
use crate::commands::report::{
    CMD_ALIVE_MAP, CMD_ALIVE_MAP_REQUEST, CMD_CHECK_ROWS_REQUEST, CMD_ROW_DELETE, CMD_ROW_UPSERT,
    CMD_SCHEMA, CMD_SCHEMA_REQUEST, CMD_SET_ROWS_DELETED, CMD_SYNC_PAGE, CMD_SYNC_REQUEST,
    CMD_TRACE, CMD_TRACE_REQUEST,
};
use std::convert::TryInto;

/// Current `MPC_Order` payloads.
///
/// OrdersProto v1 command ids 2..22 and 27..30 are intentionally absent and
/// remain reserved forever. The report and visual subchannels keep their
/// existing ids; canonical order state lives in 41..47.
#[derive(Debug, Clone)]
pub enum TradeCommand {
    #[allow(dead_code)] // Parsed to retain the complete current command registry.
    Penalty(MarketCommandHeader),
    #[allow(dead_code)] // Parsed to retain the complete current command registry.
    TradeVisual(MarketCommandHeader),
    OrderTracePoint(OrderTracePoint),
    CorridorUpdate(CorridorUpdate),
    ClosedSellOrderReport(ClosedSellOrderReport),
    ReportRowUpsert(crate::commands::report::RepRowUpsert),
    ReportRowDelete(crate::commands::report::RepRowDelete),
    #[allow(dead_code)] // Request shape is parsed for registry parity; clients only send it.
    ReportSyncRequest(crate::commands::report::RepSyncRequest),
    #[allow(dead_code)] // Request shape is parsed for registry parity; clients only send it.
    ReportSchemaRequest(BaseCommandHeader),
    ReportSchema(crate::commands::report::RepSchema),
    ReportSyncPage(crate::commands::report::RepSyncPage),
    #[allow(dead_code)] // Request shape is parsed for registry parity; clients only send it.
    ReportCheckRowsRequest(crate::commands::report::RepCheckRowsRequest),
    ReportSetRowsDeleted(crate::commands::report::RepSetRowsDeleted),
    #[allow(dead_code)] // Request shape is parsed for registry parity; clients only send it.
    ReportAliveMapRequest(crate::commands::report::RepAliveMapRequest),
    ReportAliveMap(crate::commands::report::RepAliveMap),
    #[allow(dead_code)] // Request shape is parsed for registry parity; clients only send it.
    ReportTraceRequest(crate::commands::report::RepTraceRequest),
    ReportTrace(crate::commands::report::RepTrace),
    OrderImage(OrderImage),
    OrderPatch(OrderPatch),
    OrdersSnapshot(OrdersSnapshot),
    OrdersCatalog(OrdersCatalog),
    #[allow(dead_code)] // Request shape is parsed for registry parity; clients only send it.
    OrderStatusRequest(OrderStatusRequest),
    OrderNotFound(BaseCommandHeader),
    #[allow(dead_code)] // Inbound parsing is retained for registry parity and wire tests.
    OrderCommand(OrderCommand),
    #[allow(dead_code)] // Parsed to retain the inherited base command shape.
    BaseMarket(MarketCommandHeader),
    #[allow(dead_code)] // Unknown command metadata is intentionally retained for forward parsing.
    Unknown {
        cmd_id: u8,
        uid: u64,
    },
}

impl TradeCommand {
    pub(crate) fn parse_for_apply(payload: &[u8]) -> Option<Self> {
        let mut input = payload;
        Self::read_with_visual_names(&mut input, false)
    }

    #[allow(dead_code)] // Full wire parser retained for command inspection and regression tests.
    pub fn parse(payload: &[u8]) -> Option<Self> {
        let mut input = payload;
        Self::read(&mut input)
    }

    pub(super) fn read(input: &mut &[u8]) -> Option<Self> {
        Self::read_with_visual_names(input, true)
    }

    fn read_with_visual_names(input: &mut &[u8], retain_names: bool) -> Option<Self> {
        if input.len() < 11 {
            return None;
        }
        let cmd_id = input[0];
        let ver = u16::from_le_bytes([input[1], input[2]]);
        let uid = u64::from_le_bytes(input[3..11].try_into().unwrap());
        if ver > CURRENT_PROTO_CMD_VER {
            *input = &[];
            return Some(Self::Unknown { cmd_id, uid });
        }

        match cmd_id {
            1 => Some(Self::BaseMarket(MarketCommandHeader::read(input)?)),
            23 => Some(Self::Penalty(MarketCommandHeader::read(input)?)),
            24 => Some(Self::TradeVisual(MarketCommandHeader::read(input)?)),
            25 => Some(Self::OrderTracePoint(if retain_names {
                OrderTracePoint::read(input)?
            } else {
                OrderTracePoint::read_for_apply(input)?
            })),
            26 => Some(Self::CorridorUpdate(if retain_names {
                CorridorUpdate::read(input)?
            } else {
                CorridorUpdate::read_for_apply(input)?
            })),
            31 => Some(Self::ClosedSellOrderReport(ClosedSellOrderReport::read(
                input,
            )?)),
            CMD_ROW_UPSERT => Some(Self::ReportRowUpsert(
                crate::commands::report::RepRowUpsert::read(input)?,
            )),
            CMD_ROW_DELETE => Some(Self::ReportRowDelete(
                crate::commands::report::RepRowDelete::read(input)?,
            )),
            CMD_SYNC_REQUEST => Some(Self::ReportSyncRequest(
                crate::commands::report::RepSyncRequest::read(input)?,
            )),
            CMD_SCHEMA_REQUEST => Some(Self::ReportSchemaRequest(BaseCommandHeader::read(input)?)),
            CMD_SCHEMA => Some(Self::ReportSchema(
                crate::commands::report::RepSchema::read(input)?,
            )),
            CMD_SYNC_PAGE => Some(Self::ReportSyncPage(
                crate::commands::report::RepSyncPage::read(input)?,
            )),
            CMD_CHECK_ROWS_REQUEST => Some(Self::ReportCheckRowsRequest(
                crate::commands::report::RepCheckRowsRequest::read(input)?,
            )),
            CMD_SET_ROWS_DELETED => Some(Self::ReportSetRowsDeleted(
                crate::commands::report::RepSetRowsDeleted::read(input)?,
            )),
            CMD_ALIVE_MAP_REQUEST => Some(Self::ReportAliveMapRequest(
                crate::commands::report::RepAliveMapRequest::read(input)?,
            )),
            CMD_ALIVE_MAP => Some(Self::ReportAliveMap(
                crate::commands::report::RepAliveMap::read(input)?,
            )),
            CMD_TRACE_REQUEST => Some(Self::ReportTraceRequest(
                crate::commands::report::RepTraceRequest::read(input)?,
            )),
            CMD_TRACE => Some(Self::ReportTrace(crate::commands::report::RepTrace::read(input)?)),
            41 => Some(Self::OrderImage(OrderImage::read(input)?)),
            42 => Some(Self::OrderPatch(OrderPatch::read(input)?)),
            43 => Some(Self::OrdersSnapshot(OrdersSnapshot::read(input)?)),
            44 => Some(Self::OrdersCatalog(OrdersCatalog::read(input)?)),
            45 => Some(Self::OrderStatusRequest(OrderStatusRequest::read(input)?)),
            46 => Some(Self::OrderNotFound(BaseCommandHeader::read(input)?)),
            47 => Some(Self::OrderCommand(OrderCommand::read(input)?)),
            _ => {
                *input = &[];
                Some(Self::Unknown { cmd_id, uid })
            }
        }
    }

    #[allow(dead_code)] // Complete-envelope helper retained with the registry even when no consumer needs it.
    pub fn uid(&self) -> u64 {
        match self {
            Self::Penalty(h) | Self::TradeVisual(h) | Self::BaseMarket(h) => h.base.uid,
            Self::OrderTracePoint(c) => c.market.base.uid,
            Self::CorridorUpdate(c) => c.market.base.uid,
            Self::ClosedSellOrderReport(c) => c.header.uid,
            Self::ReportRowUpsert(c) => c.header.uid,
            Self::ReportRowDelete(c) => c.header.uid,
            Self::ReportSyncRequest(c) => c.header.uid,
            Self::ReportSchemaRequest(h) => h.uid,
            Self::ReportSchema(c) => c.header.uid,
            Self::ReportSyncPage(c) => c.header.uid,
            Self::ReportCheckRowsRequest(c) => c.header.uid,
            Self::ReportSetRowsDeleted(c) => c.header.uid,
            Self::ReportAliveMapRequest(c) => c.header.uid,
            Self::ReportAliveMap(c) => c.header.uid,
            Self::ReportTraceRequest(c) => c.header.uid,
            Self::ReportTrace(c) => c.header.uid,
            Self::OrderImage(c) => c.header.uid,
            Self::OrderPatch(c) => c.header.uid,
            Self::OrdersSnapshot(c) => c.header.uid,
            Self::OrdersCatalog(c) => c.header.uid,
            Self::OrderStatusRequest(c) => c.header.uid,
            Self::OrderNotFound(h) => h.uid,
            Self::OrderCommand(c) => c.header.uid,
            Self::Unknown { uid, .. } => *uid,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn visual_apply_parser_preserves_values_and_truncation_rules() {
        for cmd_id in [25, 26] {
            for name in [b"BTCUSDT".as_slice(), &[0xff, 0xc0, b'X'], &[]] {
                let mut payload = vec![cmd_id];
                payload.extend_from_slice(&CURRENT_PROTO_CMD_VER.to_le_bytes());
                payload.extend_from_slice(&123_u64.to_le_bytes());
                payload.extend_from_slice(&[2, 3]);
                payload.extend_from_slice(&(name.len() as u16).to_le_bytes());
                payload.extend_from_slice(name);
                let header_len = payload.len();
                payload.extend_from_slice(&45000.0_f64.to_le_bytes());
                payload.extend_from_slice(&100.0_f32.to_le_bytes());
                payload.extend_from_slice(&99.0_f32.to_le_bytes());
                payload.extend_from_slice(&98.0_f32.to_le_bytes());
                payload.extend_from_slice(&[1, 7]);
                for len in 0..=payload.len() {
                    let full = TradeCommand::parse(&payload[..len]);
                    let compact = TradeCommand::parse_for_apply(&payload[..len]);
                    assert_eq!(full.is_some(), len >= header_len);
                    assert_eq!(compact.is_some(), full.is_some());
                    if let (Some(mut full), Some(compact)) = (full, compact) {
                        let market = match &mut full {
                            TradeCommand::OrderTracePoint(c) => &mut c.market,
                            TradeCommand::CorridorUpdate(c) => &mut c.market,
                            _ => unreachable!(),
                        };
                        assert_eq!(market.market_name, if name == [0xff, 0xc0, b'X'] { "??X" } else {
                            std::str::from_utf8(name).unwrap()
                        });
                        market.market_name.clear();
                        assert_eq!(format!("{full:?}"), format!("{compact:?}"));
                    }
                }
                payload[1..3].copy_from_slice(&(CURRENT_PROTO_CMD_VER + 1).to_le_bytes());
                for parse in [TradeCommand::parse, TradeCommand::parse_for_apply] {
                    assert!(matches!(parse(&payload[..11]), Some(TradeCommand::Unknown { uid: 123, .. })));
                }
            }
        }
    }

    #[test]
    fn retired_order_command_ids_stay_unknown() {
        for cmd_id in (2_u8..=22).chain(27..=30) {
            let mut payload = vec![cmd_id];
            payload.extend_from_slice(&CURRENT_PROTO_CMD_VER.to_le_bytes());
            payload.extend_from_slice(&123_u64.to_le_bytes());
            assert!(matches!(
                TradeCommand::parse(&payload),
                Some(TradeCommand::Unknown {
                    cmd_id: parsed,
                    uid: 123
                }) if parsed == cmd_id
            ));
        }
    }
}
