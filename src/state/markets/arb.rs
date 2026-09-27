//! Arbitrage compact relay apply path for live `Market` objects.
//!
//! Delphi `ParseArbPayloadCompact` resolves server `mIndex` to `TMarket` and
//! writes `TMarket.ArbSlots` / `TMarket.ArbNow`. The compact packet shape
//! remains available for diagnostics, but the Active Lib state lives on market
//! handles.

use crate::commands::arb::{ArbPayload, ArbPriceItem};
use crate::commands::market::{
    ArbIsolationFlags, ArbPlatformCode, Market, MarketArbPricePoint, ARB_PRICE_RING_LEN,
};

use super::MarketsState;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct ArbApplySummary {
    pub market_blocks: usize,
    pub price_items: usize,
    pub isolation_entries: usize,
    pub applied_prices: usize,
    pub applied_isolation_entries: usize,
}

impl MarketsState {
    // parity: MoonBot MoonProtoEngine.pas:ParseArbPayloadCompact
    pub(crate) fn apply_arb_payload(
        &self,
        payload: &ArbPayload<'_>,
        wanted_platforms: Option<&[bool; 256]>,
        now_time_days: f64,
    ) -> ArbApplySummary {
        let mut summary = ArbApplySummary::default();
        match payload {
            ArbPayload::Price { blocks, .. } => {
                for block in blocks.clone() {
                    if !self.indexes_synchronized {
                        continue;
                    }
                    let Some(local_idx) = self.local_pos_for_server_index(block.market_index)
                    else {
                        continue;
                    };
                    let Some(handle) = self.markets.get(local_idx) else {
                        continue;
                    };
                    summary.market_blocks += 1;
                    summary.price_items += block.prices.len() / 5;
                    let Some(wanted_platforms) = wanted_platforms else {
                        continue;
                    };
                    handle.with_mut(|market| {
                        let my_price = market.price.p_last as f32;
                        for item in block.items() {
                            if apply_arb_price(
                                market,
                                &item,
                                wanted_platforms,
                                now_time_days,
                                my_price,
                            ) {
                                summary.applied_prices += 1;
                            }
                        }
                    });
                }
            }
            ArbPayload::Isolation { entries, .. } => {
                for entry in entries.chunks_exact(4) {
                    if !self.indexes_synchronized {
                        continue;
                    }
                    let index = u16::from_le_bytes([entry[0], entry[1]]);
                    let Some(handle) = self.market_by_index(index) else {
                        continue;
                    };
                    summary.isolation_entries += 1;
                    if wanted_platforms.is_none() {
                        continue;
                    }
                    handle.with_mut(|market| {
                        if market.arb_slots.is_empty() {
                            return;
                        }
                        market
                            .arb_slots
                            .entry(ArbPlatformCode::from_byte(entry[2]))
                            .or_default()
                            .isolated_flags_tmp = ArbIsolationFlags::from_byte(entry[3]);
                        summary.applied_isolation_entries += 1;
                    });
                }
                if wanted_platforms.is_some() {
                    self.arb_isol_commit();
                }
            }
        }
        summary
    }

    // parity: MoonBot MoonProtoEngine.pas:ParseArbPayloadCompact (isolation commit pass)
    fn arb_isol_commit(&self) {
        for handle in self.markets.iter() {
            handle.with_mut(|market| {
                for slot in market.arb_slots.values_mut() {
                    slot.isolated_flags = slot.isolated_flags_tmp;
                    slot.isolated_flags_tmp = ArbIsolationFlags::None;
                }
            });
        }
    }
}

// parity: MoonBot MoonProtoEngine.pas:ParseArbPayloadCompact (per-platform price slot)
fn apply_arb_price(
    market: &mut Market,
    item: &ArbPriceItem,
    wanted_platforms: &[bool; 256],
    now_time_days: f64,
    my_price: f32,
) -> bool {
    if !wanted_platforms[item.platform_code as usize] {
        return false;
    }

    let slot = market
        .arb_slots
        .entry(ArbPlatformCode::from_byte(item.platform_code))
        .or_default();
    slot.enabled = true;
    let head = ((slot.head as usize + 1) % ARB_PRICE_RING_LEN) as u8;
    slot.ring[head as usize] = MarketArbPricePoint {
        price: item.price,
        time: now_time_days,
        my_price,
    };
    slot.head = head;
    slot.now.price = item.price;
    slot.now.time = now_time_days;
    true
}
