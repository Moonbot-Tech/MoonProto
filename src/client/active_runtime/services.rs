use super::commands::{RuntimeCommand, UiRuntimeCommand};
use super::{MoonClient, MoonClientError, MoonTelegram};
use crate::state::{WatchDogSettings, WatchDogState};
use serde_json::json;
use std::sync::Arc;

/// Controls observers running on the connected core.
pub struct MoonWatchDogs<'a> {
    pub(super) client: &'a MoonClient,
}

impl MoonClient {
    /// Edit only a fresh snapshot while connected. Old cores leave state unavailable.
    pub fn watchdogs(&self) -> MoonWatchDogs<'_> {
        MoonWatchDogs { client: self }
    }

    fn service_settings(&self, id: u8, value: serde_json::Value) -> Result<u64, MoonClientError> {
        let uid = rand::random();
        self.send_no_reply(RuntimeCommand::Ui(UiRuntimeCommand::ServiceSettings {
            id,
            uid,
            json: value.to_string(),
        }))?;
        Ok(uid)
    }
}

impl MoonWatchDogs<'_> {
    pub fn state(&self) -> Option<Arc<WatchDogState>> {
        self.client.snapshot()?.settings().watchdogs.clone()
    }

    /// Request current configuration and status without starting observers.
    /// The matching `ServiceSettingsApplied` event carries the snapshot; a
    /// separate `WatchDogsUpdated` event is not required.
    pub fn refresh(&self) -> Result<u64, MoonClientError> {
        self.client.service_settings(59, json!({"action": "get"}))
    }

    /// Upsert entries and save settings. Observe ServiceSettingsApplied for the result.
    /// Never replay mutations automatically on reconnect.
    pub fn apply(&self, settings: &WatchDogSettings) -> Result<u64, MoonClientError> {
        if settings
            .entries
            .iter()
            .any(|e| !e.price.is_finite() || e.price < 0.0 || e.time < 0)
        {
            return Err(MoonClientError::InvalidServiceSettings(
                "invalid observer price or time",
            ));
        }
        let mut value = json!(settings);
        value["action"] = json!("apply");
        self.client.service_settings(59, value)
    }

    pub fn remove(&self, id: &str) -> Result<u64, MoonClientError> {
        self.client
            .service_settings(59, json!({"action": "remove", "id": id}))
    }

    /// Request cancellation of tracked buys. Execution results are in core logs.
    pub fn cancel_buys(&self) -> Result<u64, MoonClientError> {
        self.client
            .service_settings(59, json!({"action": "cancel"}))
    }
}

impl MoonTelegram<'_> {
    /// Save outgoing bot mode/token. None preserves the token; Some("") clears it.
    /// The channel is bound by sending the core's channel PIN to Telegram.
    pub fn set_outgoing_bot(
        &self,
        own_bot: bool,
        token: Option<&str>,
    ) -> Result<u64, MoonClientError> {
        let mut value = json!({"action": "apply", "own_bot": own_bot});
        if let Some(token) = token {
            value["token"] = json!(token);
        }
        self.client.service_settings(60, value)
    }

    /// Generate a PIN on the core. Read it from the acknowledged Telegram state.
    pub fn generate_channel_pin(&self) -> Result<u64, MoonClientError> {
        self.client.service_settings(60, json!({"action": "pin"}))
    }

    pub fn reset_channel(&self) -> Result<u64, MoonClientError> {
        self.client
            .service_settings(60, json!({"action": "reset_channel"}))
    }
}
