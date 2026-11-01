//! Core-owned watchdog configuration and service-setting acknowledgements.

use serde::{Deserialize, Serialize};

/// A complete observer snapshot. Credentials are never returned.
/// Field meanings and dependencies: [MoonDoc service settings](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/10-settings-apis.md#watchdog-control-bots-and-unaccounted-positions).
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct WatchDogState {
    /// Monotonic snapshot revision within one core session.
    pub revision: u64,
    /// The observer controller exists on the core.
    pub available: bool,
    /// Core license permits editing observer entries. Sender IP settings remain independent.
    pub can_edit: bool,
    /// Observer support for this core's platform/exchange, independent of its license.
    pub supported: bool,
    /// Observer-server IPv4 destination; empty or 0.0.0.0 disables sending.
    pub server_ip: String,
    /// Mark order ownership and permit observer keepalives when a destination is set.
    pub sign_orders: bool,
    /// Telegram notifications about confirmed unaccounted positions.
    pub report: bool,
    /// Close confirmed unaccounted positions automatically; requires position control.
    pub auto_sell: bool,
    /// Position-control exclusions, shared with `TradingSection::h_pos_black_list_text`.
    pub blacklist: String,
    pub udp_active: bool,
    pub udp_error: String,
    pub entries: Vec<WatchDogEntry>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct WatchDogEntry {
    /// Opaque identity valid for this core session; never reuse after reconnect.
    pub id: String,
    pub name: String,
    pub api_key_hint: String,
    pub key_set: bool,
    pub secret_set: bool,
    pub rsa: bool,
    pub orders: bool,
    pub positions: bool,
    pub ip: bool,
    pub price: f64,
    pub time: i32,
    /// Calendar date of addition on the core, not a timestamp.
    pub added: String,
    pub status: WatchDogStatus,
    pub error: String,
    pub online_count: i32,
    pub duplicate: bool,
}

#[derive(Debug, Clone, Copy, Default, Deserialize, PartialEq, Eq)]
#[serde(from = "u8")]
pub enum WatchDogStatus {
    #[default]
    Added,
    Error,
    Ready,
    Checking,
    Removing,
    Unknown(u8),
}

impl From<u8> for WatchDogStatus {
    fn from(value: u8) -> Self {
        match value {
            0 => Self::Added,
            1 => Self::Error,
            2 => Self::Ready,
            3 => Self::Checking,
            4 => Self::Removing,
            other => Self::Unknown(other),
        }
    }
}

/// Upserts plus observer-wide settings. Omitted entries are not deleted.
/// Build from a fresh snapshot with [`WatchDogState::edit`].
/// Field meanings and dependencies: [MoonDoc service settings](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/10-settings-apis.md#watchdog-control-bots-and-unaccounted-positions).
#[derive(Clone, Default, Serialize)]
pub struct WatchDogSettings {
    /// Observer-server IPv4 destination; empty or 0.0.0.0 disables sending.
    pub server_ip: String,
    /// Mark order ownership; also required for sending observer keepalives.
    pub sign_orders: bool,
    /// Notify through Telegram about confirmed unaccounted positions.
    pub report: bool,
    /// Close confirmed unaccounted positions; requires position control.
    pub auto_sell: bool,
    /// Position-control exclusions, as in `TradingSection::h_pos_black_list_text`.
    pub blacklist: String,
    /// Rows to add or update; deleting a row requires `remove(id)`.
    pub entries: Vec<WatchDogEdit>,
}

/// Empty `id` adds an observer. Missing credentials preserve the saved value.
/// Changing either credential reconnects only this observer, not the core.
#[derive(Clone, Default, Serialize)]
pub struct WatchDogEdit {
    /// Core-assigned session identity; empty adds a row.
    pub id: String,
    /// Observed-account label.
    pub name: String,
    /// None preserves the saved key. Supplied keys must contain more than five characters.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub api_key: Option<String>,
    /// None preserves the saved secret. Supplied secrets must contain more than five characters.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub api_secret: Option<String>,
    /// The observed account uses RSA keys.
    pub rsa: bool,
    /// Monitor buys using the price-distance and age checks below.
    pub orders: bool,
    /// Monitor unaccounted positions; requires core support and a paid license.
    pub positions: bool,
    /// Cancel related buys if the observed bots stop sending heartbeats.
    pub ip: bool,
    /// Nonnegative price distance in percent. At or below it, a buy is a cancellation candidate.
    /// Zero disables the distance check.
    pub price: f64,
    /// Nonnegative seconds since the tracked order's last update. Zero disables age checking.
    pub time: i32,
}

impl WatchDogState {
    pub fn edit(&self) -> WatchDogSettings {
        WatchDogSettings {
            server_ip: self.server_ip.clone(),
            sign_orders: self.sign_orders,
            report: self.report,
            auto_sell: self.auto_sell,
            blacklist: self.blacklist.clone(),
            entries: self
                .entries
                .iter()
                .map(|e| WatchDogEdit {
                    id: e.id.clone(),
                    name: e.name.clone(),
                    rsa: e.rsa,
                    orders: e.orders,
                    positions: e.positions,
                    ip: e.ip,
                    price: e.price,
                    time: e.time,
                    ..Default::default()
                })
                .collect(),
        }
    }
}

/// Actual core-side result, distinct from successful local enqueue.
/// A cancel result confirms the request; exchange outcomes arrive in core logs.
#[derive(Debug, Clone, Deserialize)]
pub struct ServiceSettingsResult {
    /// Compare with the string representation of the returned request ID.
    pub request_id: String,
    /// The operation took effect in memory, even if saving it failed.
    #[serde(default)]
    pub applied: bool,
    /// Empty on success. Persistence failures may leave an in-memory change applied.
    pub error: String,
    pub action: String,
    #[serde(flatten)]
    pub state: ServiceSettingsState,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "domain", content = "state", rename_all = "lowercase")]
pub enum ServiceSettingsState {
    Telegram(std::sync::Arc<super::TelegramState>),
    Watchdog(std::sync::Arc<WatchDogState>),
}
