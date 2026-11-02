use super::*;
use crate::commands::engine_api::{EngineMethod, EngineResponse};
use crate::events::{EngineActionKind, Event, EventDispatcher};

fn schedule(client: &mut Client, dispatcher: &mut EventDispatcher, pending: &mut RuntimePending) -> EngineActionTicket {
    client.testing_set_domain_ready(true);
    let payload = crate::commands::engine_request::get_strategies_snapshot();
    let ticket = EngineActionTicket {
        kind: EngineActionKind::RequestStrategiesSnapshot,
        request_uid: engine_request_uid(&payload), method: EngineMethod::GetStrategiesSnapshot,
    };
    handle_command(client, dispatcher, RuntimeCommand::EngineAction {
        kind: ticket.kind.clone(), ticket: ticket.clone(), payload,
    }, pending);
    ticket
}

fn response(ticket: &EngineActionTicket) -> EngineResponse {
    EngineResponse {
        ver: crate::commands::registry::CURRENT_PROTO_CMD_VER,
        request_uid: ticket.request_uid.unwrap(), method: ticket.method,
        success: true, error_code: 0, error_msg: String::new(),
        data: crate::commands::strat::build_snapshot(1, 2, 0, true, &[], 0, 0),
    }
}

#[test]
fn newest_manual_request_wins_and_duplicate_or_late_responses_cannot_replace_state() {
    let mut client = Client::new(tests::dummy_cfg());
    client.peer_app_token = 1;
    let mut d = EventDispatcher::new();
    d.strats.upsert(99, 100, "Old");
    let mut pending = RuntimePending::default();
    let first = schedule(&mut client, &mut d, &mut pending);
    // Even a response already delivered to the old receiver is superseded.
    assert!(client.pending_api.api_pending.dispatch(response(&first)).is_none());
    let last = schedule(&mut client, &mut d, &mut pending);
    assert_eq!(pending.engine_actions.len(), 1);
    assert!(matches!(&d.take_queued_events()[..], [Event::EngineAction(e)] if first.matches(e) && !e.success));
    poll_engine_actions(&mut pending.engine_actions, &mut d, &client.pending_api.api_pending, 1);
    assert!(d.strats.get(99).is_some());
    assert!(client.pending_api.api_pending.dispatch(response(&last)).is_none());
    poll_engine_actions(&mut pending.engine_actions, &mut d, &client.pending_api.api_pending, 1);
    assert!(d.strats.get(99).is_none());
    assert!(pending.engine_actions.is_empty());
    let events = d.take_queued_events();
    assert!(events.iter().any(|e| matches!(e, Event::EngineAction(e) if last.matches(e) && e.success)));
    assert!(!events.iter().any(|e| matches!(e, Event::EngineAction(e) if first.matches(e))));
    for ticket in [&first, &last] {
        assert!(client.pending_api.api_pending.dispatch(response(ticket)).is_some());
    }
}

#[test]
fn manual_request_timeout_or_changed_core_discards_even_queued_response() {
    for change_core in [false, true] {
        let mut client = Client::new(tests::dummy_cfg());
        client.peer_app_token = 1;
        let mut d = EventDispatcher::new();
        d.strats.upsert(99, 100, "Keep");
        let mut pending = RuntimePending::default();
        let ticket = schedule(&mut client, &mut d, &mut pending);
        client.pending_api.api_pending.dispatch(response(&ticket));
        if !change_core { pending.engine_actions[0].deadline = Instant::now(); }
        poll_engine_actions(&mut pending.engine_actions, &mut d, &client.pending_api.api_pending,
            if change_core { 2 } else { 1 });
        assert!(pending.engine_actions.is_empty());
        assert!(d.strats.get(99).is_some());
        assert!(matches!(&d.take_queued_events()[..], [Event::EngineAction(e)] if ticket.matches(e) && !e.success));
    }
}
