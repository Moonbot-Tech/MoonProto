use super::*;
use crate::commands::registry::{find_descriptor, CommandPriority, UKeyRule};
use crate::commands::ui::UICommand;
use crate::state::{ServiceSettingsState, SettingsEvent, WatchDogStatus};
use serde_json::json;

fn packet(id: u8, value: serde_json::Value) -> Vec<u8> {
    let text = value.to_string();
    let mut bytes = vec![id];
    bytes.extend_from_slice(&CURRENT_PROTO_CMD_VER.to_le_bytes());
    bytes.extend_from_slice(&123u64.to_le_bytes());
    bytes.extend_from_slice(&(text.len() as u32).to_le_bytes());
    bytes.extend_from_slice(text.as_bytes());
    bytes
}

#[test]
fn service_registry_keeps_intents_independent_and_coalesces_only_state() {
    for id in 58..=61 {
        let desc = find_descriptor(Command::UI, id).unwrap();
        assert_eq!(desc.priority, CommandPriority::Sliced);
        if id == 58 {
            assert_eq!(desc.unique_kind, 34);
            assert!(matches!(desc.ukey, UKeyRule::Singleton(1)));
        } else {
            assert_eq!(desc.unique_kind, 0);
            assert!(matches!(desc.ukey, UKeyRule::None));
        }
    }
}

#[test]
fn observer_edit_preserves_secrets_and_uses_core_identity() {
    let state = json!({"available":true,"server_ip":"127.0.0.1","entries":[
        {"id":"core-entry","name":"тест","key_set":true,"secret_set":true,"rsa":true,
         "status":3,"online_count":2,"duplicate":true,"orders":true,"price":1.5,"time":30}
    ]});
    let UICommand::WatchDogState(state) = UICommand::parse(&packet(58, state)).unwrap() else {
        panic!()
    };
    assert_eq!(state.entries[0].status, WatchDogStatus::Checking);
    assert_eq!(state.entries[0].online_count, 2);
    let mut edit = state.edit();
    let saved = json!(&edit);
    assert_eq!(saved["entries"][0]["id"], "core-entry");
    assert_eq!(saved["entries"][0]["rsa"], true);
    assert!(saved["entries"][0].get("api_key").is_none());
    assert!(saved["entries"][0].get("api_secret").is_none());
    edit.entries[0].api_secret = Some("new-secret".into());
    assert_eq!(json!(&edit)["entries"][0]["api_secret"], "new-secret");
    assert_eq!(state.entries[0].name, "тест");
}

#[test]
fn applied_result_contains_actual_state_and_correlates_request() {
    let mut settings = crate::state::SettingsState::default();
    let cmd = UICommand::parse(&packet(
        61,
        json!({
            "domain":"telegram","action":"pin","request_id":"18446744073709551615","error":"",
            "state":{"settings_supported":true,"channel_pin":"PRIVATE_PIN","token_set":true}
        }),
    ))
    .unwrap();
    let Some(SettingsEvent::ServiceSettingsApplied(result)) = settings.apply(cmd) else {
        panic!()
    };
    assert_eq!(result.request_id, u64::MAX.to_string());
    assert!(matches!(result.state, ServiceSettingsState::Telegram(_)));
    assert_eq!(
        settings.telegram.as_ref().unwrap().channel_pin.as_deref(),
        Some("PRIVATE_PIN")
    );
    assert!(!format!("{result:?}").contains("PRIVATE_PIN"));
}

#[test]
fn malformed_service_packets_do_not_parse() {
    for id in [58, 61] {
        let bytes = packet(id, json!({"entries":[]}));
        for end in [0, 10, 14, bytes.len() - 1] {
            assert!(UICommand::parse(&bytes[..end]).is_none());
        }
        assert!(UICommand::parse(&packet(id, json!([]))).is_none());
    }
}

#[test]
fn legacy_telegram_snapshot_does_not_advertise_new_service_controls() {
    let mut settings = crate::state::SettingsState::default();
    let legacy = packet(36, json!({"enabled":true,"state_supported":true,"service_online":true}));
    assert!(matches!(settings.apply(UICommand::parse(&legacy).unwrap()), Some(SettingsEvent::TelegramUpdated)));
    let telegram = settings.telegram.unwrap();
    assert!(telegram.state_supported && telegram.enabled);
    assert!(!telegram.settings_supported && !telegram.token_set);
    assert_eq!(telegram.revision, 0);
    assert!(settings.watchdogs.is_none());
}

#[test]
fn observer_snapshot_is_replaced_and_cleared_on_new_core() {
    let client = crate::client::Client::new(dummy_client_cfg());
    let mut ctx = ActiveDispatchContext::from_client(&client);
    ctx.server_token = 10;
    ctx.peer_app_token = 20;
    let mut d = EventDispatcher::new();
    let mut events = Vec::new();
    let mut actions = Vec::new();
    for value in [json!({"entries":[{"id":"old"}]}), json!({"entries":[]})] {
        d.dispatch_into_active_actions(
            Command::UI,
            &packet(58, value),
            0,
            &mut events,
            &ctx,
            &mut actions,
        );
    }
    assert!(d.settings.watchdogs.as_ref().unwrap().entries.is_empty());
    assert!(actions.is_empty());
    ctx.peer_app_token += 1;
    d.dispatch_into_active_actions(Command::UI, &[0; 11], 1, &mut events, &ctx, &mut actions);
    assert!(d.settings.watchdogs.is_none());
}

#[test]
fn delayed_service_replies_and_pushes_do_not_rollback_newer_state() {
    for (id, domain) in [(36, "telegram"), (58, "watchdog")] {
        let mut settings = crate::state::SettingsState::default();
        let newer = json!({"revision": 3, "channel_name": "new", "server_ip": "new"});
        let older = json!({"revision": 2, "channel_name": "old", "server_ip": "old"});
        assert!(settings
            .apply(UICommand::parse(&packet(id, newer)).unwrap())
            .is_some());
        assert!(settings
            .apply(UICommand::parse(&packet(id, older.clone())).unwrap())
            .is_none());
        let reply =
            json!({"domain":domain,"action":"apply","request_id":"1","error":"","state":older});
        assert!(matches!(
            settings.apply(UICommand::parse(&packet(61, reply)).unwrap()),
            Some(SettingsEvent::ServiceSettingsApplied(_))
        ));
        if id == 36 {
            assert_eq!(
                settings.telegram.unwrap().channel_name.as_deref(),
                Some("new")
            );
        } else {
            assert_eq!(settings.watchdogs.unwrap().server_ip, "new");
        }
    }
}

#[test]
fn save_failure_keeps_assigned_observer_identity_for_retry() {
    let mut settings = crate::state::SettingsState::default();
    let reply = packet(61, json!({
        "domain":"watchdog", "action":"apply", "request_id":"42", "applied":true,
        "error":"disk write failed", "state":{"revision":5,"entries":[
            {"id":"assigned-by-core","key_set":true,"secret_set":true,"price":1,"time":30}
        ]}
    }));
    let Some(SettingsEvent::ServiceSettingsApplied(result)) = settings.apply(UICommand::parse(&reply).unwrap()) else {
        panic!()
    };
    assert!(result.applied);
    assert!(!result.error.is_empty());
    let edit = settings.watchdogs.unwrap().edit();
    assert_eq!(edit.entries[0].id, "assigned-by-core");
    assert!(edit.entries[0].api_key.is_none() && edit.entries[0].api_secret.is_none());
}
