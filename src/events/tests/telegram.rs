use super::*;
use crate::commands::registry::{find_descriptor, CommandPriority, UKeyRule};
use crate::commands::ui::UICommand;
use crate::state::SettingsEvent;
use serde_json::json;

fn packet(json: &str) -> Vec<u8> {
    let mut bytes = vec![36];
    bytes.extend_from_slice(&CURRENT_PROTO_CMD_VER.to_le_bytes());
    bytes.extend_from_slice(&123u64.to_le_bytes());
    bytes.extend_from_slice(&(json.len() as u32).to_le_bytes());
    bytes.extend_from_slice(json.as_bytes());
    bytes
}

fn receive(d: &mut EventDispatcher, client: &crate::client::Client, bytes: &[u8]) -> Vec<Event> {
    let mut events = Vec::new();
    let mut actions = Vec::new();
    dispatch_active_packet_for_test(d, Command::UI, bytes, 0, &mut events, client, &mut actions);
    assert!(actions.is_empty(), "receiving state must not start or reset login");
    events
}

#[test]
fn telegram_registry_matches_etalon() {
    assert_eq!(CURRENT_PROTO_CMD_VER, 4);
    let state = find_descriptor(Command::UI, 36).unwrap();
    assert_eq!(state.priority, CommandPriority::Sliced);
    assert!(matches!(state.ukey, UKeyRule::Singleton(1)));
    assert_eq!(state.unique_kind, 31);
    for id in 37..=48 {
        let action = find_descriptor(Command::UI, id).unwrap();
        assert_eq!(action.priority, CommandPriority::High);
        assert_eq!(action.unique_kind, 0);
    }
}

#[test]
fn telegram_full_state_applies_before_ready_and_replaces_old_step() {
    let client = crate::client::Client::new(dummy_client_cfg());
    assert!(!client.is_domain_ready());
    let mut d = EventDispatcher::new();
    assert!(d.settings.telegram.is_none());
    let qr = packet(
        r#"{"enabled":true,"service_online":true,"state_supported":true,
        "proxy_type":1,"proxy_host":"proxy.test","proxy_port":1080,"proxy_user":"user",
        "proxy_password_set":true,"service_version":"7.67.0.11","client_state":"ready",
        "setup_error":"","client_error":"previous reset failed",
        "service":{"type":"auth.state","auth_state":"wait_qr_confirmation",
        "details":{"qr_link":"tg://login?token=TEST_TOKEN"},"connection":"connecting",
        "error":null,"proxy_error":{"code":400,"message":"proxy failed"},"proxy":{"mode":"none"}}}"#,
    );
    assert!(matches!(
        receive(&mut d, &client, &qr).as_slice(),
        [Event::Settings(SettingsEvent::TelegramUpdated)]
    ));
    let old = d.settings.telegram.clone().unwrap();
    assert_eq!(old.proxy_port, Some(1080));
    assert_eq!(old.client_error.as_deref(), Some("previous reset failed"));
    assert!(old.proxy_password_set);
    let service = old.service.as_ref().unwrap();
    assert_eq!(service.proxy.as_ref().unwrap().mode, "none");
    assert_eq!(service.proxy_error.as_ref().unwrap().code, 400);
    assert!(!format!("{old:?} {service:?} {:?}", service.details).contains("TEST_TOKEN"));

    receive(
        &mut d,
        &client,
        &packet(
            r#"{"enabled":true,"service":{"auth_state":"wait_password",
        "details":{"password_hint":"hint","recovery_email_pattern":"a***@test"},
        "error":{"code":400,"message":"PASSWORD_HASH_INVALID"}}}"#,
        ),
    );
    let current = d.settings.telegram.as_ref().unwrap();
    assert!(!current.proxy_password_set && !current.service_online);
    assert!(current.client_error.is_none() && current.proxy_host.is_none());
    let service = current.service.as_ref().unwrap();
    assert_eq!(service.auth_state, "wait_password");
    assert!(service.details.qr_link.is_none());
    assert_eq!(service.details.password_hint.as_deref(), Some("hint"));
    assert_eq!(service.error.as_ref().unwrap().message, "PASSWORD_HASH_INVALID");
    assert!(
        old.service.as_ref().unwrap().details.qr_link.is_some(),
        "published snapshots stay immutable"
    );
    receive(
        &mut d,
        &client,
        &packet(r#"{"enabled":false,"client_state":"disabled"}"#),
    );
    assert!(d.settings.telegram.as_ref().unwrap().service.is_none());
}

#[test]
fn telegram_step_details_and_open_ended_states_survive_decoding() {
    for (step, details) in [
        ("wait_phone", json!({})),
        (
            "wait_code",
            json!({"phone":"+10000000000","code_type":{"kind":"sms","length":5},
            "next_code_type":{"kind":"call","length":5},"resend_at":1789000000}),
        ),
        ("wait_email_address", json!({})),
        ("wait_email_code", json!({"email_pattern":"t***@test","code_length":6})),
        (
            "wait_registration",
            json!({"terms":"terms\nsecond line","min_user_age":18,"show_popup":true}),
        ),
        (
            "wait_premium_purchase",
            json!({"support_email":"support@test","support_subject":"subject"}),
        ),
        ("ready", json!({})),
        ("logging_out", json!({})),
        ("closing", json!({})),
        ("closed", json!({})),
        ("wait_tdlib_parameters", json!({})),
        ("starting", json!({})),
        ("future_step", json!({"future_field":"future"})),
    ] {
        let raw = json!({"enabled":true,"service":{"auth_state":step,"details":details,
            "connection":"future_connection","phone":"+10000000000","future_field":true}});
        let UICommand::TelegramState(state) = UICommand::parse(&packet(&raw.to_string())).unwrap() else {
            panic!()
        };
        let service = state.service.as_ref().unwrap();
        assert_eq!(service.auth_state, step);
        assert_eq!(service.connection, "future_connection");
        assert_eq!(service.phone.as_deref(), Some("+10000000000"));
        match step {
            "wait_code" => {
                assert_eq!(service.details.resend_at, Some(1789000000));
                assert_eq!(service.details.code_type.as_ref().unwrap().kind, "sms");
                assert_eq!(service.details.next_code_type.as_ref().unwrap().length, Some(5));
            }
            "wait_email_code" => assert_eq!(service.details.code_length, Some(6)),
            "wait_registration" => {
                assert_eq!(service.details.terms.as_deref(), Some("terms\nsecond line"));
                assert_eq!(service.details.min_user_age, Some(18));
                assert!(service.details.show_popup);
            }
            "wait_premium_purchase" => assert_eq!(service.details.support_subject.as_deref(), Some("subject")),
            _ => {}
        }
    }
}

#[test]
fn telegram_length_is_u32_and_malformed_state_does_not_erase_state() {
    let client = crate::client::Client::new(dummy_client_cfg());
    let mut d = EventDispatcher::new();
    let terms = "x".repeat(70_000);
    let raw = json!({"service":{"auth_state":"wait_registration","details":{"terms":terms}}});
    let bytes = packet(&raw.to_string());
    receive(&mut d, &client, &bytes);
    let old = d.settings.telegram.clone().unwrap();
    assert_eq!(
        old.service.as_ref().unwrap().details.terms.as_ref().unwrap().len(),
        70_000
    );
    for end in [0, 1, 10, 11, 14, 15, bytes.len() - 1] {
        assert!(UICommand::parse(&bytes[..end]).is_none(), "truncated at {end}");
        receive(&mut d, &client, &bytes[..end]);
    }
    for invalid in [
        "",
        "null",
        "[]",
        "{",
        r#"{"enabled":"yes"}"#,
        r#"{"service":{"details":false}}"#,
    ] {
        assert!(UICommand::parse(&packet(invalid)).is_none(), "{invalid}");
        receive(&mut d, &client, &packet(invalid));
    }
    let mut future = packet("{}");
    future[1..3].copy_from_slice(&5u16.to_le_bytes());
    assert!(receive(&mut d, &client, &future).is_empty());
    assert!(Arc::ptr_eq(d.settings.telegram.as_ref().unwrap(), &old));
}

#[test]
fn telegram_hard_session_reset_clears_auth_but_soft_rebind_preserves_snapshot() {
    let client = crate::client::Client::new(dummy_client_cfg());
    let mut ctx = ActiveDispatchContext::from_client(&client);
    ctx.server_token = 10;
    ctx.peer_app_token = 20;
    let mut d = EventDispatcher::new();
    for (server, app, cleared) in [(10, 20, false), (11, 20, true), (11, 21, true)] {
        let mut events = Vec::new();
        let mut actions = Vec::new();
        d.dispatch_into_active_actions(
            Command::UI,
            &packet(r#"{"enabled":true}"#),
            0,
            &mut events,
            &ctx,
            &mut actions,
        );
        ctx.server_token = server;
        ctx.peer_app_token = app;
        d.dispatch_into_active_actions(Command::UI, &[0; 11], 1, &mut events, &ctx, &mut actions);
        assert_eq!(d.settings.telegram.is_none(), cleared);
    }
}
