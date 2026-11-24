use super::*;

fn response(data: Vec<u8>) -> EngineResponse {
    EngineResponse {
        ver: CURRENT_PROTO_CMD_VER, request_uid: 42, method: EngineMethod::MoonStreamSettings,
        success: true, error_code: 0, error_msg: String::new(), data,
    }
}

#[test]
fn moon_streamer_actions_apply_core_state_before_the_matching_completion() {
    for kind in [EngineActionKind::RefreshMoonStreamer,
        EngineActionKind::SetMoonStreamerSource { source: crate::state::MoonStreamerSource::Common },
        EngineActionKind::SetMoonStreamerDistribution { enabled: true },
        EngineActionKind::SetMoonStreamerReception { enabled: false },
        EngineActionKind::ReconnectMoonStreamer]
    {
        let mut d = EventDispatcher::new();
        d.queue_engine_action_response(kind.clone(), response(crate::commands::ui::moon_streamer_runtime_fixture()));
        assert_eq!(d.snapshot().settings().runtime_state.unwrap().moon_streamer.unwrap().receiver.trades_per_second, 1234);
        assert!(matches!(&d.take_queued_events()[..], [Event::Settings(crate::state::SettingsEvent::RuntimeStateUpdated),
            Event::EngineAction(event)] if event.kind == kind && event.request_uid == Some(42) && event.success));
    }
}

#[test]
fn moon_streamer_failed_or_invalid_replies_preserve_confirmed_state_and_report_errors() {
    let raw = crate::commands::ui::moon_streamer_runtime_fixture();
    let mut rejected = response(Vec::new());
    rejected.success = false;
    rejected.error_msg = "Unable to save config".into();
    let mut wrong = response(raw.clone());
    wrong.method = EngineMethod::GetStartupState;
    for resp in [rejected, wrong, response(raw[..18].to_vec()), response(vec![255])] {
        let error = resp.error_msg.clone();
        let mut d = EventDispatcher::new();
        d.settings.apply(crate::commands::ui::UICommand::parse(&raw).unwrap());
        let confirmed = d.settings.runtime_state;
        d.queue_engine_action_response(EngineActionKind::RefreshMoonStreamer, resp);
        assert_eq!(d.settings.runtime_state, confirmed);
        let events = d.take_queued_events();
        let [Event::EngineAction(event)] = &events[..] else { panic!("failure completion expected") };
        assert!(!event.success);
        assert_eq!(event.request_uid, Some(42));
        assert_eq!(event.error_msg, if error.is_empty() { "Invalid Moon Streamer response" } else { &error });
    }
}

#[test]
fn requested_subscriber_reports_are_retained_before_completion_and_malformed_replies_do_not_erase_them() {
    let mut d = EventDispatcher::new();
    for report in ["Bot: 7 <203.0.113.1:2503> (R: 2)\r\n", ""] {
        let before = d.snapshot();
        let mut data = Vec::new();
        write_string(&mut data, report);
        d.queue_engine_action_response(EngineActionKind::RequestMoonStreamerSubscribers, response(data));
        assert_eq!(d.snapshot().settings().moon_streamer_subscribers.as_deref(), Some(report));
        assert!(matches!(&d.take_queued_events()[..],
            [Event::Settings(crate::state::SettingsEvent::MoonStreamerSubscribersUpdated),
            Event::EngineAction(event)] if event.success && event.request_uid == Some(42)));
        assert_ne!(before.settings().moon_streamer_subscribers.as_deref(), Some(report));
    }
    d.queue_engine_action_response(EngineActionKind::RequestMoonStreamerSubscribers, response(vec![3, 0, b'a']));
    assert_eq!(d.settings.moon_streamer_subscribers.as_deref(), Some(""));
    assert!(matches!(&d.take_queued_events()[..], [Event::EngineAction(event)] if !event.success));
}
