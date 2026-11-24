use super::*;
use crate::state::MoonStreamerSource;
use std::net::Ipv4Addr;

#[test]
fn moon_streamer_public_actions_encode_only_the_users_choice_and_keep_the_legacy_switch() {
    let (tx, rx) = mpsc::channel();
    let client = MoonClient {
        tx, server_time_delta_ms: Arc::new(std::sync::atomic::AtomicI64::new(i64::MIN)),
        shutdown: Default::default(), event_queue: None, snapshot: Default::default(),
        startup_status: Default::default(), err_emu_diagnostics: Default::default(),
        protocol_metrics: Default::default(), subscription_registry: Default::default(),
        join: Default::default(), lifecycle_join: Default::default(),
    };
    let stream = client.moon_streamer();
    assert!(matches!(stream.use_last_custom_server(), Err(MoonClientError::StateUnavailable(_))));
    assert!(matches!(stream.set_source(MoonStreamerSource::Custom(Ipv4Addr::UNSPECIFIED)),
        Err(MoonClientError::InvalidServiceSettings(_))));
    assert!(rx.try_recv().is_err());
    let requests = [
        (stream.refresh(), vec![0]),
        (stream.set_source(MoonStreamerSource::Common), vec![1, 0, 0, 0, 0]),
        (stream.set_source(MoonStreamerSource::Custom(Ipv4Addr::new(203, 0, 113, 9))), vec![1, 9, 113, 0, 203]),
        (stream.set_distribution_enabled(true), vec![2, 1]),
        (stream.set_distribution_enabled(false), vec![2, 0]),
        (stream.reconnect(), vec![3]),
        (stream.request_subscribers(), vec![4]),
        (stream.set_receive_enabled(true), vec![5, 1]),
        (stream.set_receive_enabled(false), vec![5, 0]),
    ];
    for (ticket, params) in requests {
        let ticket = ticket.unwrap();
        let RuntimeCommand::EngineAction { kind, ticket: queued, payload } = rx.try_recv().unwrap() else {
            panic!("engine action expected");
        };
        assert_eq!(ticket, queued);
        assert_eq!(kind, ticket.kind);
        assert_eq!(&payload[..3], &[2, 4, 0]);
        assert_eq!(payload[11], 38);
        assert_eq!(&payload[12..18], &[0; 6]);
        assert_eq!(&payload[18..22], &(params.len() as i32).to_le_bytes());
        assert_eq!(&payload[22..], params);
    }

    let mut d = crate::events::EventDispatcher::new();
    d.settings.apply(crate::commands::ui::UICommand::parse(&crate::commands::ui::moon_streamer_runtime_fixture()).unwrap());
    *client.snapshot.write() = Some(MoonClientSnapshot::new(1, Arc::new(d.snapshot())));
    stream.use_last_custom_server().unwrap();
    let RuntimeCommand::EngineAction { payload, .. } = rx.try_recv().unwrap() else { panic!() };
    assert_eq!(&payload[22..], &[1, 7, 100, 51, 198]);
    client.settings().set_moon_stream_enabled(false).unwrap();
    assert!(matches!(rx.try_recv().unwrap(), RuntimeCommand::Ui(UiRuntimeCommand::MoonStream(false))));
    assert!(rx.try_recv().is_err());
}
