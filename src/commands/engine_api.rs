/// `MPC_API` Engine RPC helpers.
///
/// Request: client → server (CmdId=002)
/// Response: server → client (CmdId=001)
use std::time::{Duration, SystemTime};

use super::registry::read_string;
#[cfg(any(test, feature = "diagnostics"))]
use crate::time::DelphiTime;
use crate::MoonTime;
mod auth_check;
mod base_check;
mod method;
mod response;

pub(crate) use self::auth_check::parse_auth_check_response;
pub use self::auth_check::{AuthCheckResponse, DexInfo, HyperDexIndex};
#[allow(unused_imports)]
pub(crate) use self::base_check::{exchange_type_flags, parse_base_check_response};
pub use self::base_check::{ExchangeTypeMask, ServerInfo};
pub use self::method::EngineMethod;
pub(crate) use self::response::parse_engine_response;
pub use self::response::EngineResponse;

#[cfg(test)]
const DELPHI_UNIX_EPOCH_DAYS: f64 = 25_569.0;
const SECONDS_PER_DAY: f64 = 86_400.0;
const MAX_TRANSFER_ASSET_ROWS: usize = 16_384;
const TRANSFER_ASSET_MIN_WIRE_ROW_SIZE: usize = 2;

fn read_zero_tail<const N: usize>(data: &[u8], pos: &mut usize) -> [u8; N] {
    let mut out = [0u8; N];
    if *pos < data.len() {
        let n = (data.len() - *pos).min(N);
        out[..n].copy_from_slice(&data[*pos..*pos + n]);
        *pos += n;
    }
    out
}

fn read_u8_zero_tail(data: &[u8], pos: &mut usize) -> u8 {
    read_zero_tail::<1>(data, pos)[0]
}

fn read_i32_zero_tail(data: &[u8], pos: &mut usize) -> i32 {
    i32::from_le_bytes(read_zero_tail::<4>(data, pos))
}

fn read_u64_zero_tail(data: &[u8], pos: &mut usize) -> u64 {
    u64::from_le_bytes(read_zero_tail::<8>(data, pos))
}

/// Parse `EngineResponse.data` for `emk_QueryHedgeMode`
/// (`EngineMethod::QueryHedgeMode`).
///
/// The server writes one boolean byte on success. Extra trailing bytes are
/// ignored for forward compatibility.
pub(crate) fn parse_query_hedge_mode_response(data: &[u8]) -> Option<bool> {
    let mut pos = 0usize;
    Some(read_u8_zero_tail(data, &mut pos) != 0)
}

/// API-key expiration time returned by `emk_CheckAPIExpirationTime`.
///
/// Current cores append a timezone-safe remaining duration to the legacy
/// server-local timestamp. MoonProto normalizes that duration to the client's
/// clock at the wire boundary. A value with no known time means that the check
/// succeeded and the API key has no reported expiration.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ApiExpirationTime {
    delphi_time: f64,
    reported_days_left: Option<i32>,
}

impl ApiExpirationTime {
    pub fn from_time(time: MoonTime) -> Self {
        Self {
            delphi_time: time.to_delphi_days(),
            reported_days_left: None,
        }
    }

    /// Build from the raw legacy day-count timestamp.
    pub(crate) fn from_delphi_time(delphi_time: f64) -> Self {
        Self {
            delphi_time,
            reported_days_left: None,
        }
    }

    fn from_delphi_time_with_reported_days(delphi_time: f64, days_left: i32) -> Self {
        Self {
            delphi_time,
            reported_days_left: Some(days_left),
        }
    }

    /// API expiration time when the server reported a known value.
    pub fn time(&self) -> Option<MoonTime> {
        self.is_known()
            .then(|| MoonTime::from_delphi_days(self.delphi_time))
            .flatten()
    }

    /// API expiration as a raw day-count helper for diagnostics.
    #[cfg(any(test, feature = "diagnostics"))]
    #[doc(hidden)]
    pub fn time_delphi(&self) -> DelphiTime {
        DelphiTime::from_days(self.delphi_time)
    }

    /// Normalized day-count value retained for exact diagnostics.
    #[cfg(any(test, feature = "diagnostics"))]
    #[doc(hidden)]
    pub fn delphi_time(&self) -> f64 {
        self.delphi_time
    }

    /// Returns false when the server reported no known expiration time.
    pub fn is_known(&self) -> bool {
        self.delphi_time.is_finite() && self.delphi_time > 0.0
    }

    /// Rounded day count calculated by the core for this refresh.
    ///
    /// Returns `None` only for a legacy response that predates this field.
    /// A successful no-expiration response reports `Some(0)` and is
    /// distinguished by [`Self::is_known`].
    pub fn reported_days_left(&self) -> Option<i32> {
        self.reported_days_left
    }

    /// Convert to whole Unix seconds when the value is known and representable
    /// by `SystemTime` on the Unix side of the epoch.
    pub fn unix_seconds(&self) -> Option<i64> {
        let seconds = self.time()?.unix_seconds().round();
        (seconds.is_finite() && seconds >= 0.0 && seconds <= i64::MAX as f64)
            .then_some(seconds as i64)
    }

    /// Convert to `SystemTime` when the value is known and not before the Unix epoch.
    pub fn system_time(&self) -> Option<SystemTime> {
        let seconds = self.unix_seconds()?;
        SystemTime::UNIX_EPOCH.checked_add(Duration::from_secs(seconds as u64))
    }

    /// Rounded signed number of days until expiration relative to `now`.
    pub fn days_until(&self, now: SystemTime) -> Option<i64> {
        let expiration = self.system_time()?;
        match expiration.duration_since(now) {
            Ok(duration) => Some((duration.as_secs_f64() / SECONDS_PER_DAY).round() as i64),
            Err(err) => {
                let days = (err.duration().as_secs_f64() / SECONDS_PER_DAY).round() as i64;
                Some(-days)
            }
        }
    }
}

/// Parse `EngineResponse.data` for `emk_CheckAPIExpirationTime`
/// (`EngineMethod::CheckAPIExpirationTime`).
///
/// Current payload: `[server_local_time:f64][days_left:i32][remaining_days:f64]`.
/// The first field is retained for old clients; current clients reconstruct the
/// absolute time from `remaining_days`, so different server/client time zones
/// cannot shift the result. An exact 8-byte payload remains supported for old
/// cores.
pub(crate) fn parse_api_expiration_time_response(data: &[u8]) -> Option<ApiExpirationTime> {
    parse_api_expiration_time_response_at(data, MoonTime::now())
}

fn parse_api_expiration_time_response_at(
    data: &[u8],
    client_now: MoonTime,
) -> Option<ApiExpirationTime> {
    const LEGACY_SIZE: usize = 8;
    const CURRENT_SIZE: usize = 8 + 4 + 8;

    if data.len() < LEGACY_SIZE {
        return None;
    }

    let server_local_time = f64::from_le_bytes(data[..8].try_into().ok()?);
    if data.len() == LEGACY_SIZE {
        return Some(ApiExpirationTime::from_delphi_time(server_local_time));
    }
    if data.len() < CURRENT_SIZE {
        return None;
    }

    let reported_days_left = i32::from_le_bytes(data[8..12].try_into().ok()?);
    let remaining_days = f64::from_le_bytes(data[12..20].try_into().ok()?);
    let normalized_time = if server_local_time.is_finite() && server_local_time > 0.0 {
        let local_days = client_now.to_delphi_days() + remaining_days;
        if !remaining_days.is_finite() || !local_days.is_finite() {
            return None;
        }
        local_days
    } else {
        0.0
    };

    Some(ApiExpirationTime::from_delphi_time_with_reported_days(
        normalized_time,
        reported_days_left,
    ))
}

/// One transferable asset row returned by `emk_UpdateTransferAssets`.
///
/// The wire row carries `currency`, transferable `amount`, and total balance
/// for one asset in one transfer bucket.
#[derive(Debug, Clone, PartialEq)]
pub struct TransferAsset {
    /// Asset symbol, for example `"USDT"` or `"BTC"`.
    pub currency: String,
    /// Transferable amount reported by the exchange.
    ///
    /// The wire spelling is historical; the SDK exposes the corrected field
    /// name while preserving the protocol meaning.
    pub amount: f64,
    /// Total balance reported for this transfer asset.
    pub total: f64,
}

/// Parse `EngineResponse.data` for `emk_UpdateTransferAssets`
/// (`EngineMethod::UpdateTransferAssets`).
///
/// Wire format:
///
/// ```text
/// count: i32 LE
/// items[count]:
///   currency: string (u16 length + UTF-8)
///   amount:   f64 LE
///   total:    f64 LE
/// ```
pub(crate) fn parse_update_transfer_assets_response(data: &[u8]) -> Option<Vec<TransferAsset>> {
    let mut pos = 0usize;
    let count_raw = read_i32_zero_tail(data, &mut pos);
    if count_raw <= 0 {
        return Some(Vec::new());
    }

    let count = count_raw as usize;
    if count > MAX_TRANSFER_ASSET_ROWS {
        log::warn!(
            target: "moonproto::engine_api",
            "UpdateTransferAssets row count {count} exceeds cap {MAX_TRANSFER_ASSET_ROWS}"
        );
        return None;
    }
    let min_wire = count.checked_mul(TRANSFER_ASSET_MIN_WIRE_ROW_SIZE)?;
    if data.len().saturating_sub(pos) < min_wire {
        log::warn!(
            target: "moonproto::engine_api",
            "UpdateTransferAssets row count {count} exceeds payload envelope"
        );
        return None;
    }

    let mut assets = Vec::new();
    assets.try_reserve_exact(count).ok()?;
    for _ in 0..count {
        let currency = read_string(data, &mut pos)?;
        let amount = f64::from_le_bytes(read_zero_tail::<8>(data, &mut pos));
        let total = f64::from_le_bytes(read_zero_tail::<8>(data, &mut pos));
        assets.push(TransferAsset {
            currency,
            amount,
            total,
        });
    }
    Some(assets)
}

#[cfg(test)]
mod tests;
