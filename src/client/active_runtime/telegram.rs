use super::commands::{RuntimeCommand, UiRuntimeCommand};
use super::{MoonClient, MoonClientError};
use crate::commands::ui::TelegramAction;
use crate::state::{TelegramLoginMode, TelegramProxy, TelegramState};
use std::sync::Arc;

/// Controls the Telegram service on the core, not a local Telegram client.
///
/// Methods enqueue user intent; success does not confirm login or proxy setup.
/// Observe `SettingsEvent::TelegramUpdated` and the next full state instead.
/// Never replay login/reset/logout automatically on reconnect.
pub struct MoonTelegram<'a> {
    pub(super) client: &'a MoonClient,
}

impl MoonTelegram<'_> {
    /// Last received full snapshot, also in `snapshot().settings().telegram`.
    /// Treat it as stale while disconnected; hide QR until a new snapshot arrives.
    pub fn state(&self) -> Option<Arc<TelegramState>> {
        self.client.snapshot()?.settings().telegram.clone()
    }

    fn send(&self, action: TelegramAction) -> Result<(), MoonClientError> {
        action.validate().map_err(MoonClientError::InvalidTelegramInput)?;
        self.client
            .send_no_reply(RuntimeCommand::Ui(UiRuntimeCommand::Telegram(action)))
    }

    /// Request state without starting/resetting login or rotating a QR token.
    pub fn refresh(&self) -> Result<(), MoonClientError> {
        self.send(TelegramAction::Refresh)
    }

    /// Enable/disable this core's use of the shared service; disabling is not logout.
    pub fn set_enabled(&self, enabled: bool) -> Result<(), MoonClientError> {
        self.send(TelegramAction::SetEnabled(enabled))
    }

    /// Save all proxy fields on the core and apply them when the service is enabled.
    /// A password/secret must be supplied again; it is never returned by the core.
    pub fn set_proxy(&self, proxy: TelegramProxy) -> Result<(), MoonClientError> {
        self.send(TelegramAction::SetProxy(proxy))
    }

    /// QR starts only from `wait_phone`; Phone cancels an unfinished login.
    /// Neither choice logs out a `ready` account. Re-selecting QR does not reset it.
    pub fn set_login_mode(&self, mode: TelegramLoginMode) -> Result<(), MoonClientError> {
        self.send(TelegramAction::SetLoginMode(mode))
    }

    /// Submit an international phone number at `wait_phone`.
    pub fn set_phone(&self, phone: impl Into<String>) -> Result<(), MoonClientError> {
        self.send(TelegramAction::SetPhone(phone.into()))
    }

    /// Submit the login code at `wait_code`.
    pub fn set_code(&self, code: impl Into<String>) -> Result<(), MoonClientError> {
        self.send(TelegramAction::SetCode(code.into()))
    }

    /// Submit the two-factor password at `wait_password`.
    pub fn set_password(&self, password: impl Into<String>) -> Result<(), MoonClientError> {
        self.send(TelegramAction::SetPassword(password.into()))
    }

    /// Submit an email address at `wait_email_address`.
    pub fn set_email(&self, email: impl Into<String>) -> Result<(), MoonClientError> {
        self.send(TelegramAction::SetEmail(email.into()))
    }

    /// Submit the email verification code at `wait_email_code`.
    pub fn set_email_code(&self, code: impl Into<String>) -> Result<(), MoonClientError> {
        self.send(TelegramAction::SetEmailCode(code.into()))
    }

    /// Register at `wait_registration`, only after the user accepts the shown terms.
    pub fn register(&self, first_name: impl Into<String>, last_name: impl Into<String>) -> Result<(), MoonClientError> {
        self.send(TelegramAction::Register {
            first_name: first_name.into(),
            last_name: last_name.into(),
        })
    }

    /// Request another code at `wait_code` or `wait_email_code`.
    /// Respect phone-code `resend_at` and `next_code_type`; TDLib enforces its limits.
    pub fn resend_code(&self) -> Result<(), MoonClientError> {
        self.send(TelegramAction::ResendCode)
    }

    /// Explicit logout and reset of shared TDLib account data on the core's machine.
    /// Affects other MoonBot processes using the same service; ask the user first.
    pub fn logout(&self) -> Result<(), MoonClientError> {
        self.send(TelegramAction::Logout)
    }
}
