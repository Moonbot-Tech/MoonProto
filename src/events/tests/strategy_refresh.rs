use super::*;

fn response(data: Vec<u8>) -> EngineResponse {
    EngineResponse {
        ver: CURRENT_PROTO_CMD_VER, request_uid: 42, method: EngineMethod::GetStrategiesSnapshot,
        success: true, error_code: 0, error_msg: String::new(), data,
    }
}

#[test]
fn manual_snapshot_completion_follows_replacement_and_resets_reply_epoch() {
    let mut d = EventDispatcher::new();
    d.strats.upsert(99, 100, "Old");
    d.local_strategy_epoch = 100;
    let data = crate::commands::strat::build_snapshot(1, 2, 0, true, &[], 0, 0);
    d.queue_engine_action_response(EngineActionKind::RequestStrategiesSnapshot, response(data));
    assert!(d.strats.get(99).is_none());
    assert_eq!(d.local_strategy_epoch, 2);
    let events = d.take_queued_events();
    assert!(matches!(&events[..], [Event::Strat(StratEvent::SnapshotFull { server_epoch: 2, .. }),
        Event::EngineAction(EngineActionEvent { success: true, request_uid: Some(42), .. })]));
}

#[test]
fn rejected_wrong_method_partial_and_malformed_manual_responses_leave_state_unchanged() {
    let mut rejected = response(Vec::new());
    rejected.success = false;
    rejected.error_code = 400;
    rejected.error_msg = "Unknown method".into();
    let full = crate::commands::strat::build_snapshot(1, 2, 0, true, &[], 0, 0);
    let mut wrong = response(full);
    wrong.method = EngineMethod::GetStartupState;
    let partial = crate::commands::strat::build_snapshot(1, 2, 0, false, &[], 0, 0);
    for resp in [rejected, wrong, response(partial), response(vec![255])] {
        let mut d = EventDispatcher::new();
        d.strats.upsert(99, 100, "Old");
        d.local_strategy_epoch = 100;
        d.queue_engine_action_response(EngineActionKind::RequestStrategiesSnapshot, resp);
        assert!(d.strats.get(99).is_some());
        assert_eq!(d.local_strategy_epoch, 100);
        assert!(matches!(&d.take_queued_events()[..], [Event::EngineAction(EngineActionEvent { success: false, .. })]));
    }
}
