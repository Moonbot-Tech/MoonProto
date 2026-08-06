//! `RequestMarketHistory` archive parser.

use flate2::read::DeflateDecoder;

use crate::state::{LastPricePoint, MiniCandle, TradeHistoryRow};
use crate::MoonTime;

use super::inflate::read_inflate_to_vec;

pub(crate) const MAX_MARKET_HISTORY_CHUNKED_BYTES: usize = 16 * 1024 * 1024;
pub(crate) const MAX_MARKET_HISTORY_DECOMPRESSED_BYTES: usize = 16 * 1024 * 1024;
const MARKET_HISTORY_VERSION: u8 = 1;
const TRADE_ROW_SIZE: usize = 16;
const MINI_CANDLE_ROW_SIZE: usize = 28;
const LAST_PRICE_ROW_SIZE: usize = 12;

#[derive(Debug, Clone, Default, PartialEq)]
pub(crate) struct MarketHistoryArchive {
    pub(crate) futures_trades: Vec<TradeHistoryRow>,
    pub(crate) mini_candles: Vec<MiniCandle>,
    pub(crate) last_prices: Vec<LastPricePoint>,
    pub(crate) liquidations: Vec<TradeHistoryRow>,
}

pub(crate) fn parse_market_history_archive(
    compressed: &[u8],
) -> Result<MarketHistoryArchive, String> {
    let mut decoder = DeflateDecoder::new(compressed);
    let raw = read_inflate_to_vec(
        &mut decoder,
        compressed.len().saturating_mul(4),
        MAX_MARKET_HISTORY_DECOMPRESSED_BYTES,
    )
    .map_err(|err| format!("market-history DEFLATE failed: {err}"))?;
    parse_market_history_archive_raw(&raw)
}

fn parse_market_history_archive_raw(raw: &[u8]) -> Result<MarketHistoryArchive, String> {
    let mut pos = 0usize;
    let version = read_u8(raw, &mut pos)?;
    if version != MARKET_HISTORY_VERSION {
        return Err(format!(
            "unsupported market-history archive version {version}"
        ));
    }

    let futures_trades = read_trade_rows(raw, &mut pos, "futures trades")?;
    let mini_candles = read_mini_candles(raw, &mut pos)?;
    let last_prices = read_last_prices(raw, &mut pos)?;
    let liquidations = read_trade_rows(raw, &mut pos, "liquidations")?;

    // Version 1 is append-only: future positional sections may be added to the
    // tail without making an older client reject the four sections it knows.
    Ok(MarketHistoryArchive {
        futures_trades,
        mini_candles,
        last_prices,
        liquidations,
    })
}

fn read_trade_rows(
    raw: &[u8],
    pos: &mut usize,
    section: &'static str,
) -> Result<Vec<TradeHistoryRow>, String> {
    let count = read_count(raw, pos, TRADE_ROW_SIZE, section)?;
    let mut rows = Vec::new();
    rows.try_reserve_exact(count)
        .map_err(|err| format!("{section} allocation failed: {err}"))?;
    for _ in 0..count {
        rows.push(TradeHistoryRow {
            time: read_time(raw, pos, section)?,
            price: read_f32(raw, pos, section)?,
            qty: read_f32(raw, pos, section)?,
        });
    }
    Ok(rows)
}

fn read_mini_candles(raw: &[u8], pos: &mut usize) -> Result<Vec<MiniCandle>, String> {
    let section = "mini candles";
    let count = read_count(raw, pos, MINI_CANDLE_ROW_SIZE, section)?;
    let mut rows = Vec::new();
    rows.try_reserve_exact(count)
        .map_err(|err| format!("{section} allocation failed: {err}"))?;
    for _ in 0..count {
        rows.push(MiniCandle {
            time: read_time(raw, pos, section)?,
            cnt: read_i32(raw, pos, section)?,
            min_price: read_f32(raw, pos, section)?,
            max_price: read_f32(raw, pos, section)?,
            buy_vol: read_f32(raw, pos, section)?,
            sell_vol: read_f32(raw, pos, section)?,
        });
    }
    Ok(rows)
}

fn read_last_prices(raw: &[u8], pos: &mut usize) -> Result<Vec<LastPricePoint>, String> {
    let section = "last prices";
    let count = read_count(raw, pos, LAST_PRICE_ROW_SIZE, section)?;
    let mut rows = Vec::new();
    rows.try_reserve_exact(count)
        .map_err(|err| format!("{section} allocation failed: {err}"))?;
    for _ in 0..count {
        let current = read_f32(raw, pos, section)?;
        let time = read_time(raw, pos, section)?;
        rows.push(LastPricePoint { current, time });
    }
    Ok(rows)
}

fn read_count(
    raw: &[u8],
    pos: &mut usize,
    row_size: usize,
    section: &'static str,
) -> Result<usize, String> {
    let count = read_i32(raw, pos, section)?;
    if count < 0 {
        return Err(format!("negative {section} count {count}"));
    }
    let count = count as usize;
    let bytes = count
        .checked_mul(row_size)
        .ok_or_else(|| format!("{section} byte count overflow"))?;
    if bytes > raw.len().saturating_sub(*pos) {
        return Err(format!(
            "{section} count {count} exceeds the remaining archive"
        ));
    }
    Ok(count)
}

fn read_u8(raw: &[u8], pos: &mut usize) -> Result<u8, String> {
    let value = *raw
        .get(*pos)
        .ok_or_else(|| "market-history archive is empty".to_string())?;
    *pos += 1;
    Ok(value)
}

fn read_i32(raw: &[u8], pos: &mut usize, field: &str) -> Result<i32, String> {
    let bytes = take::<4>(raw, pos, field)?;
    Ok(i32::from_le_bytes(bytes))
}

fn read_f32(raw: &[u8], pos: &mut usize, field: &str) -> Result<f32, String> {
    let bytes = take::<4>(raw, pos, field)?;
    Ok(f32::from_le_bytes(bytes))
}

fn read_time(raw: &[u8], pos: &mut usize, field: &str) -> Result<MoonTime, String> {
    let days = f64::from_le_bytes(take::<8>(raw, pos, field)?);
    MoonTime::from_delphi_days(days).ok_or_else(|| format!("invalid {field} timestamp {days}"))
}

fn take<const N: usize>(raw: &[u8], pos: &mut usize, field: &str) -> Result<[u8; N], String> {
    let end = pos
        .checked_add(N)
        .ok_or_else(|| format!("{field} offset overflow"))?;
    let bytes = raw
        .get(*pos..end)
        .ok_or_else(|| format!("truncated {field} row"))?;
    *pos = end;
    Ok(bytes.try_into().expect("slice length is checked"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use flate2::{write::DeflateEncoder, Compression};
    use std::io::Write;

    fn deflate(raw: &[u8]) -> Vec<u8> {
        let mut encoder = DeflateEncoder::new(Vec::new(), Compression::fast());
        encoder.write_all(raw).unwrap();
        encoder.finish().unwrap()
    }

    #[test]
    fn parses_all_v1_sections_and_ignores_future_tail() {
        let time = 45_000.25f64;
        let mut raw = vec![MARKET_HISTORY_VERSION];
        raw.extend_from_slice(&1i32.to_le_bytes());
        raw.extend_from_slice(&time.to_le_bytes());
        raw.extend_from_slice(&12.5f32.to_le_bytes());
        raw.extend_from_slice(&(-3.0f32).to_le_bytes());
        raw.extend_from_slice(&1i32.to_le_bytes());
        raw.extend_from_slice(&(time + 0.1).to_le_bytes());
        raw.extend_from_slice(&7i32.to_le_bytes());
        for value in [10.0f32, 15.0, 20.0, 30.0] {
            raw.extend_from_slice(&value.to_le_bytes());
        }
        raw.extend_from_slice(&1i32.to_le_bytes());
        raw.extend_from_slice(&9.5f32.to_le_bytes());
        raw.extend_from_slice(&(time + 0.2).to_le_bytes());
        raw.extend_from_slice(&1i32.to_le_bytes());
        raw.extend_from_slice(&(time + 0.3).to_le_bytes());
        raw.extend_from_slice(&8.0f32.to_le_bytes());
        raw.extend_from_slice(&2.0f32.to_le_bytes());
        raw.extend_from_slice(b"future section");

        let parsed = parse_market_history_archive(&deflate(&raw)).unwrap();

        assert_eq!(parsed.futures_trades.len(), 1);
        assert_eq!(parsed.futures_trades[0].price, 12.5);
        assert_eq!(parsed.futures_trades[0].qty, -3.0);
        assert_eq!(parsed.mini_candles[0].cnt, 7);
        assert_eq!(parsed.mini_candles[0].sell_vol, 30.0);
        assert_eq!(parsed.last_prices[0].price(), 9.5);
        assert_eq!(parsed.liquidations[0].qty, 2.0);
    }

    #[test]
    fn rejects_unknown_version_and_impossible_count() {
        assert!(parse_market_history_archive(&deflate(&[2])).is_err());

        let mut raw = vec![MARKET_HISTORY_VERSION];
        raw.extend_from_slice(&i32::MAX.to_le_bytes());
        let err = parse_market_history_archive(&deflate(&raw)).unwrap_err();
        assert!(err.contains("exceeds the remaining archive"));
    }
}
