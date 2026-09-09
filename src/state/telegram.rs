//! Remote control of the core's built-in Telegram service.

use serde::Deserialize;
use std::fmt;

/// Full core snapshot. Missing optional fields are unavailable, not unchanged.
///
/// The account and active proxy are shared by MoonBot processes on the core's
/// machine. `enabled` and saved proxy settings belong to this particular core.
/// Debug output intentionally omits account, QR and error details.
#[derive(Clone, Default, Deserialize)]
#[serde(default)]
pub struct TelegramState {
    pub enabled: bool,
    /// Saved proxy kind: 0 = none, 1 = SOCKS5, 2 = MTProto.
    pub proxy_type: Option<u8>,
    pub proxy_host: Option<String>,
    pub proxy_port: Option<u16>,
    pub proxy_user: Option<String>,
    /// The core never returns the saved password or MTProto secret.
    pub proxy_password_set: bool,
    /// Core-to-service status, such as starting, connecting, disabled or offline.
    pub client_state: Option<String>,
    /// Pipe connection to the service, not a connection to Telegram itself.
    pub service_online: bool,
    /// The service supports recoverable remote login state.
    pub state_supported: bool,
    pub service_version: Option<String>,
    pub setup_error: Option<String>,
    /// Failure to reset an unfinished login or the local account state.
    pub client_error: Option<String>,
    /// Absent when disabled or disconnected from the service.
    pub service: Option<TelegramServiceState>,
}

impl fmt::Debug for TelegramState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("TelegramState")
            .field("enabled", &self.enabled)
            .field("service_online", &self.service_online)
            .field("state_supported", &self.state_supported)
            .finish_non_exhaustive()
    }
}

/// Current TDLib state. State names remain strings so newer service states survive.
#[derive(Clone, Default, Deserialize)]
#[serde(default)]
pub struct TelegramServiceState {
    /// Only `ready` means authenticated. See the Telegram guide for all steps.
    pub auth_state: String,
    /// Replaced with each snapshot; belongs only to the current auth step.
    pub details: TelegramAuthDetails,
    /// Telegram network status, independent of authentication.
    pub connection: String,
    /// Actual account phone after login, including QR login.
    pub phone: Option<String>,
    pub error: Option<TelegramError>,
    /// Actually enabled service proxy; may differ from this core's saved settings.
    pub proxy: Option<TelegramActiveProxy>,
    pub proxy_error: Option<TelegramError>,
}

impl fmt::Debug for TelegramServiceState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("TelegramServiceState").finish_non_exhaustive()
    }
}

/// Step-specific input hints. Absent fields must not be carried over from old steps.
#[derive(Clone, Default, Deserialize)]
#[serde(default)]
pub struct TelegramAuthDetails {
    /// Encode the entire `tg://login?token=...` string as QR; never log it.
    pub qr_link: Option<String>,
    pub phone: Option<String>,
    pub code_type: Option<TelegramCodeType>,
    pub next_code_type: Option<TelegramCodeType>,
    /// Unix seconds UTC; phone-code resend also needs `next_code_type`.
    pub resend_at: Option<i64>,
    pub password_hint: Option<String>,
    pub recovery_email_pattern: Option<String>,
    pub email_pattern: Option<String>,
    pub code_length: Option<i32>,
    pub terms: Option<String>,
    pub min_user_age: Option<i32>,
    pub show_popup: bool,
    pub support_email: Option<String>,
    pub support_subject: Option<String>,
}

impl fmt::Debug for TelegramAuthDetails {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("TelegramAuthDetails").finish_non_exhaustive()
    }
}

/// Delivery method for the current or next login code.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct TelegramCodeType {
    /// telegram, sms, sms_word, sms_phrase, call, flash_call, missed_call,
    /// fragment or unsupported. Preserve unknown kinds in UI fallback handling.
    pub kind: String,
    pub length: Option<i32>,
    pub first_letter: Option<String>,
    pub first_word: Option<String>,
    pub pattern: Option<String>,
    pub prefix: Option<String>,
    pub url: Option<String>,
}

/// A service error to display next to the current step or proxy settings.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct TelegramError {
    pub code: i32,
    pub message: String,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct TelegramActiveProxy {
    /// none, socks5, mtproto or other.
    pub mode: String,
    pub host: Option<String>,
    pub port: Option<u16>,
}

/// Explicit choice of login method. Choosing Phone cancels an unfinished login.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TelegramLoginMode {
    Phone,
    Qr,
}

/// Complete saved proxy configuration, not a partial update.
pub enum TelegramProxy {
    None,
    Socks5 {
        host: String,
        port: u16,
        user: String,
        password: String,
    },
    MtProto {
        host: String,
        port: u16,
        secret: String,
    },
}

impl fmt::Debug for TelegramProxy {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::None => "None",
            Self::Socks5 { .. } => "Socks5 { .. }",
            Self::MtProto { .. } => "MtProto { .. }",
        })
    }
}
