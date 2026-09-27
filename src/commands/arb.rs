//! MPC_Balance subcommand CmdId=6 — `TArbPricesCommand`.
//!
//! Delphi source: `MoonProtoBalanceStruct.pas:199-205, 607-633`.
//!
//! Wire-format:
//!   BaseCommand header (CmdId=6 + ver:u16 + UID:u64) + len:i32 LE + payload:bytes(len).
//!
//! `payload` is raw kernel data. The compact format is decoded by
//! [`parse_arb_payload_compact`], the Rust port of
//! `ArbClientU.pas:ParseArbPayloadCompact`.

use super::registry::CURRENT_PROTO_CMD_VER;

const ARB_PRICES_CMD_ID: u8 = 6;

#[derive(Debug, Clone)]
#[doc(hidden)]
pub(crate) struct ArbPricesCommand<'a> {
    pub uid: u64,
    pub payload: &'a [u8],
}

#[derive(Debug, Clone)]
#[doc(hidden)]
pub(crate) enum ArbPayload<'a> {
    Price {
        version: u8,
        blocks: ArbPriceBlocks<'a>,
    },
    Isolation {
        version: u8,
        entries: &'a [u8],
    },
}

#[derive(Debug, Clone)]
#[doc(hidden)]
pub(crate) struct ArbPriceBlock<'a> {
    pub market_index: u16,
    pub prices: &'a [u8],
}

#[derive(Debug, Clone, PartialEq)]
#[doc(hidden)]
pub(crate) struct ArbPriceItem {
    pub platform_code: u8,
    pub price: f32,
}

#[derive(Debug, Clone)]
pub(crate) struct ArbPriceBlocks<'a>(&'a [u8]);

impl<'a> Iterator for ArbPriceBlocks<'a> {
    type Item = ArbPriceBlock<'a>;

    fn next(&mut self) -> Option<Self::Item> {
        let data = self.0;
        self.0 = &[];
        let header = data.get(..3)?;
        let end = 3 + usize::from(header[2]) * 5;
        let prices = data.get(3..end)?;
        self.0 = &data[end..];
        Some(ArbPriceBlock {
            market_index: u16::from_le_bytes([header[0], header[1]]),
            prices,
        })
    }
}

impl ArbPriceBlock<'_> {
    pub(crate) fn items(&self) -> impl ExactSizeIterator<Item = ArbPriceItem> + '_ {
        self.prices.chunks_exact(5).map(|row| ArbPriceItem {
            platform_code: row[0],
            price: f32::from_le_bytes(row[1..5].try_into().unwrap()),
        })
    }
}

const ARB_VER_MIN: u8 = 1;
const CMD_PRICE: u8 = 1;
const CMD_ISOL: u8 = 2;

/// Parse `TArbPricesCommand`.
///
/// `payload` must already be routed from the MPC_Balance channel. Returns
/// `None` when `cmd_id != 6` or the command envelope is too short.
#[doc(hidden)]
pub(crate) fn parse_arb_prices(payload: &[u8]) -> Option<ArbPricesCommand<'_>> {
    if payload.len() < 11 {
        return None;
    }
    let cmd_id = payload[0];
    if cmd_id != ARB_PRICES_CMD_ID {
        return None;
    }
    let ver = u16::from_le_bytes([payload[1], payload[2]]);
    if ver > CURRENT_PROTO_CMD_VER {
        return None;
    }
    let uid = u64::from_le_bytes(payload[3..11].try_into().unwrap());

    let mut pos = 11;
    if pos + 4 > payload.len() {
        return Some(ArbPricesCommand { uid, payload: &[] });
    }
    let len = i32::from_le_bytes(payload[pos..pos + 4].try_into().unwrap());
    pos += 4;
    let blob = if len > 0 {
        let len = len as usize;
        if pos + len <= payload.len() {
            &payload[pos..pos + len]
        } else {
            &[]
        }
    } else {
        &[]
    };
    Some(ArbPricesCommand { uid, payload: blob })
}

/// Decode compact kernel→client arb payload.
///
/// Delphi source:
/// - `ArbClientU.pas:299-327` — dispatcher;
/// - `ArbClientU.pas:205-226` — compact price items;
/// - `ArbClientU.pas:232-259` — compact isolation snapshot.
#[doc(hidden)]
pub(crate) fn parse_arb_payload_compact(payload: &[u8]) -> Option<ArbPayload<'_>> {
    if payload.len() < 2 {
        return None;
    }

    let version = payload[0];
    if version < ARB_VER_MIN {
        return None;
    }

    let mut pos = 1usize;
    if version <= 2 {
        return Some(ArbPayload::Price {
            version,
            blocks: ArbPriceBlocks(&payload[pos..]),
        });
    }

    if pos >= payload.len() {
        return None;
    }
    let cmd = payload[pos];
    pos += 1;

    match cmd {
        CMD_PRICE => Some(ArbPayload::Price {
            version,
            blocks: ArbPriceBlocks(&payload[pos..]),
        }),
        CMD_ISOL => Some(ArbPayload::Isolation {
            version,
            entries: isolation_entries(&payload[pos..])?,
        }),
        _ => None,
    }
}

fn isolation_entries(data: &[u8]) -> Option<&[u8]> {
    let header = data.get(..2)?;
    let count = usize::from(u16::from_le_bytes([header[0], header[1]]));
    Some(&data[2..2 + count.min((data.len() - 2) / 4) * 4])
}

/// Build `TArbPricesCommand` for low-level protocol tools.
#[cfg(test)]
pub(crate) fn build_arb_prices(uid: u64, payload: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(11 + 4 + payload.len());
    out.push(ARB_PRICES_CMD_ID);
    out.extend_from_slice(&CURRENT_PROTO_CMD_VER.to_le_bytes());
    out.extend_from_slice(&uid.to_le_bytes());
    out.extend_from_slice(&(payload.len() as i32).to_le_bytes());
    out.extend_from_slice(payload);
    out
}

#[cfg(test)]
mod tests;
