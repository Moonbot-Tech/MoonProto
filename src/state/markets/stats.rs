//! Manual core-calculated market statistics, applied to the existing handles.

use super::{MarketStats, MarketsState};

const ROW_SIZE: usize = 74;

impl MarketsState {
    pub(crate) fn apply_market_stats_payload(&mut self, data: &[u8]) -> Result<usize, &'static str> {
        let (count, rows) = data.split_at_checked(4).ok_or("Missing market statistics count")?;
        let count = i32::from_le_bytes(count.try_into().unwrap());
        if count < 0 || rows.len() % ROW_SIZE != 0 || rows.len() / ROW_SIZE != count as usize {
            return Err("Invalid market statistics length");
        }
        if !self.indexes_synchronized {
            return Err("Market indexes are not synchronized; retry after initialization");
        }
        // Validate the entire index map before changing any retained market. No
        // temporary table or per-row allocation is needed for this fixed layout.
        for row in rows.chunks_exact(ROW_SIZE) {
            if !self.has_server_market_index(u16::from_le_bytes(row[..2].try_into().unwrap())) {
                self.markets_list_refresh_needed = true;
                return Err("Unknown market index; retry after the market list refresh");
            }
        }
        for row in rows.chunks_exact(ROW_SIZE) {
            let index = u16::from_le_bytes(row[..2].try_into().unwrap());
            let pos = self.local_pos_for_server_index(index).unwrap();
            let stats = MarketStats {
                volume_24h: f64::from_le_bytes(row[2..10].try_into().unwrap()),
                volume_1h: f64::from_le_bytes(row[10..18].try_into().unwrap()),
                volume_1m: f32::from_le_bytes(row[18..22].try_into().unwrap()),
                volume_3m: f32::from_le_bytes(row[22..26].try_into().unwrap()),
                volume_5m: f32::from_le_bytes(row[26..30].try_into().unwrap()),
                delta_1m: f64::from_le_bytes(row[30..38].try_into().unwrap()),
                delta_15m: f64::from_le_bytes(row[38..46].try_into().unwrap()),
                delta_1h: f64::from_le_bytes(row[46..54].try_into().unwrap()),
                delta_3h: f64::from_le_bytes(row[54..62].try_into().unwrap()),
                delta_24h: f64::from_le_bytes(row[62..70].try_into().unwrap()),
                delta_72h: i32::from_le_bytes(row[70..74].try_into().unwrap()),
            };
            self.markets[pos].with_mut(|market| {
                market.volume = stats.volume_24h;
                market.stats = Some(stats);
            });
        }
        Ok(count as usize)
    }

    pub(crate) fn clear_market_stats(&self) {
        for market in self.markets.iter() {
            market.with_mut(|market| market.stats = None);
        }
    }
}
