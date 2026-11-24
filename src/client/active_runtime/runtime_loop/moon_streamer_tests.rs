use super::*;
use crate::commands::engine_api::{EngineMethod, EngineResponse};
use crate::events::{EngineActionKind, Event, EventDispatcher};

#[test]
fn streamer_request_uses_encrypted_transport_and_publishes_state_before_completion() {
    let mut client = Client::new(tests::dummy_cfg());
    client.testing_set_domain_ready(true);
    let mut d = EventDispatcher::new();
    let mut pending = RuntimePending::default();
    let payload = crate::commands::engine_request::moon_stream_state();
    let ticket = EngineActionTicket { kind: EngineActionKind::RefreshMoonStreamer,
        request_uid: engine_request_uid(&payload), method: EngineMethod::MoonStreamSettings };
    handle_command(&mut client, &mut d, RuntimeCommand::EngineAction {
        kind: ticket.kind.clone(), ticket: ticket.clone(), payload,
    }, &mut pending);
    let (sliced, high, low) = client.take_send_queues_for_test();
    assert!(high.is_empty() && low.is_empty());
    assert_eq!(sliced.len(), 1);
    assert!(sliced[0].encrypted);
    assert_eq!(sliced[0].cmd, Command::API.to_byte());
    let response = EngineResponse {
        ver: crate::commands::registry::CURRENT_PROTO_CMD_VER,
        request_uid: ticket.request_uid.unwrap(), method: ticket.method,
        success: true, error_code: 0, error_msg: String::new(),
        data: crate::commands::ui::moon_streamer_runtime_fixture(),
    };
    assert!(client.pending_api.api_pending.dispatch(response.clone()).is_none());
    poll_engine_actions(&mut pending.engine_actions, &mut d, &client.pending_api.api_pending, 0);
    let published = RwLock::new(None);
    let events = take_queued_events_and_publish_snapshot(&client, &mut d, &published, false);
    assert!(published.read().as_ref().unwrap().state_arc().settings().runtime_state.unwrap().moon_streamer.is_some());
    assert!(matches!(&events[..], [Event::Settings(crate::state::SettingsEvent::RuntimeStateUpdated),
        Event::EngineAction(event)] if ticket.matches(event) && event.success));
    assert!(client.pending_api.api_pending.dispatch(response).is_some(), "completed requests must not consume duplicates");
}

#[test]
fn expired_or_old_core_streamer_response_cannot_replace_retained_state() {
    for change_core in [false, true] {
        let mut client = Client::new(tests::dummy_cfg());
        client.testing_set_domain_ready(true);
        client.peer_app_token = 1;
        let mut d = EventDispatcher::new();
        let mut pending = RuntimePending::default();
        let payload = crate::commands::engine_request::moon_stream_state();
        let ticket = EngineActionTicket { kind: EngineActionKind::RefreshMoonStreamer,
            request_uid: engine_request_uid(&payload), method: EngineMethod::MoonStreamSettings };
        handle_command(&mut client, &mut d, RuntimeCommand::EngineAction {
            kind: ticket.kind.clone(), ticket: ticket.clone(), payload,
        }, &mut pending);
        assert_eq!(pending.engine_actions.len(), 1);
        client.pending_api.api_pending.dispatch(EngineResponse {
            ver: crate::commands::registry::CURRENT_PROTO_CMD_VER,
            request_uid: ticket.request_uid.unwrap(), method: ticket.method,
            success: true, error_code: 0, error_msg: String::new(),
            data: crate::commands::ui::moon_streamer_runtime_fixture(),
        });
        if !change_core { pending.engine_actions[0].deadline = Instant::now(); }
        poll_engine_actions(&mut pending.engine_actions, &mut d, &client.pending_api.api_pending,
            if change_core { 2 } else { 1 });
        assert!(pending.engine_actions.is_empty());
        assert!(d.settings.runtime_state.is_none());
        assert!(matches!(&d.take_queued_events()[..], [Event::EngineAction(event)] if ticket.matches(event) && !event.success));
    }
}
