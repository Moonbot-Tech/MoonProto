use super::*;
use crate::commands::registry::{find_descriptor, CommandPriority, UKeyRule};
use crate::commands::ui::UICommand;
use crate::state::{ProblemCategory, SettingsEvent};
use crate::MoonTime;

fn header(id: u8) -> Vec<u8> {
    let mut bytes = vec![id];
    bytes.extend_from_slice(&CURRENT_PROTO_CMD_VER.to_le_bytes());
    bytes.extend_from_slice(&123u64.to_le_bytes());
    bytes
}

fn item(kind: u8, count: i32) -> Vec<u8> {
    let mut bytes = vec![kind];
    write_string(&mut bytes, "test");
    bytes.push(3);
    // Intentional localized payload: display text is in the core's language.
    write_string(&mut bytes, "Test \u{0442}\u{0435}\u{0441}\u{0442}");
    write_string(&mut bytes, "Detector signal received");
    write_string(&mut bytes, "[Tech] evidence={signal}; thresholds={direct}");
    bytes.extend_from_slice(&1_788_600_000_123i64.to_le_bytes());
    bytes.extend_from_slice(&1_788_600_002_456i64.to_le_bytes());
    bytes.extend_from_slice(&count.to_le_bytes());
    bytes
}

fn snapshot(items: &[(u8, i32)]) -> Vec<u8> {
    let mut bytes = header(32);
    bytes.extend_from_slice(&(items.len() as u16).to_le_bytes());
    for &(kind, count) in items {
        bytes.extend_from_slice(&item(kind, count));
    }
    bytes
}

fn notification(kind: u8, count: i32) -> Vec<u8> {
    let mut bytes = header(33);
    bytes.extend_from_slice(&item(kind, count));
    bytes
}

fn receive(d: &mut EventDispatcher, client: &crate::client::Client, bytes: &[u8]) -> Vec<Event> {
    let mut events = Vec::new();
    let mut actions = Vec::new();
    dispatch_active_packet_for_test(d, Command::UI, bytes, 0, &mut events, client, &mut actions);
    assert!(
        actions.is_empty(),
        "diagnostic pushes must not request extra traffic"
    );
    events
}

#[test]
fn problems_registry_matches_etalon_without_new_version() {
    assert_eq!(CURRENT_PROTO_CMD_VER, 4);
    let state = find_descriptor(Command::UI, 32).unwrap();
    assert_eq!(state.priority, CommandPriority::Sliced);
    assert!(matches!(state.ukey, UKeyRule::Singleton(1)));
    for id in 33..=35 {
        assert_eq!(
            find_descriptor(Command::UI, id).unwrap().priority,
            CommandPriority::High
        );
    }
}

#[test]
fn problems_decode_fields_and_utc_milliseconds() {
    let UICommand::ProblemNotify(problem) = UICommand::parse(&notification(22, 7)).unwrap() else {
        panic!("expected problem notification");
    };
    assert_eq!(problem.kind, 22);
    assert_eq!(problem.kind_name, "test");
    assert_eq!(problem.category, ProblemCategory::Other);
    assert_eq!(problem.title, "Test \u{0442}\u{0435}\u{0441}\u{0442}");
    assert_eq!(problem.message, "Detector signal received");
    assert_eq!(
        problem.technical_details,
        "[Tech] evidence={signal}; thresholds={direct}"
    );
    assert_eq!(
        problem.first_seen,
        MoonTime::from_unix_millis(1_788_600_000_123)
    );
    assert_eq!(
        problem.confirmed,
        MoonTime::from_unix_millis(1_788_600_002_456)
    );
    assert_eq!(problem.confirmations, 7);

    let mut bytes = notification(254, 1);
    bytes[18] = 250; // category after kind + UTF-8 key
    let UICommand::ProblemNotify(problem) = UICommand::parse(&bytes).unwrap() else {
        panic!()
    };
    assert_eq!(problem.kind, 254);
    assert_eq!(problem.category, ProblemCategory::Unknown(250));
}

#[test]
fn problems_strings_are_strict_but_numeric_tail_matches_delphi_soft_reads() {
    let bytes = notification(22, 7);
    let numeric_start = bytes.len() - 20;
    for end in 11..numeric_start {
        assert!(
            UICommand::parse(&bytes[..end]).is_none(),
            "truncated string at {end}"
        );
    }
    let UICommand::ProblemNotify(problem) = UICommand::parse(&bytes[..numeric_start]).unwrap()
    else {
        panic!();
    };
    assert_eq!(problem.first_seen, MoonTime::ZERO);
    assert_eq!(problem.confirmed, MoonTime::ZERO);
    assert_eq!(problem.confirmations, 0);
    assert!(
        matches!(UICommand::parse(&header(32)), Some(UICommand::ProblemsState(items)) if items.is_empty())
    );
}

#[test]
fn problems_initial_state_and_live_broadcast_apply_before_ready() {
    let client = crate::client::Client::new(dummy_client_cfg());
    assert!(!client.is_domain_ready());
    for mut d in [EventDispatcher::new(), EventDispatcher::new()] {
        assert!(!d.settings.problems.snapshot_received());
        assert!(matches!(
            receive(&mut d, &client, &snapshot(&[(1, 3)])).as_slice(),
            [Event::Settings(SettingsEvent::ProblemsUpdated)]
        ));
        let initial = d.settings.clone();
        assert!(
            matches!(receive(&mut d, &client, &notification(22, 1)).as_slice(),
            [Event::Settings(SettingsEvent::ProblemConfirmed { problem })] if problem.kind == 22)
        );
        assert_eq!(d.settings.problems.items().len(), 2);
        assert_eq!(
            initial.problems.items().len(),
            1,
            "published snapshots remain immutable"
        );
        receive(&mut d, &client, &notification(22, 2));
        assert_eq!(d.settings.problems.items().len(), 2);
        assert_eq!(d.settings.problems.items()[1].confirmations, 2);
        receive(&mut d, &client, &snapshot(&[]));
        assert!(d.settings.problems.snapshot_received());
        assert!(d.settings.problems.items().is_empty());
    }
}

#[test]
fn problems_malformed_snapshot_is_not_partially_applied() {
    let client = crate::client::Client::new(dummy_client_cfg());
    let mut d = EventDispatcher::new();
    receive(&mut d, &client, &snapshot(&[(1, 2)]));
    let mut broken = snapshot(&[(22, 1)]);
    broken[11..13].copy_from_slice(&2u16.to_le_bytes());
    receive(&mut d, &client, &broken);
    assert_eq!(d.settings.problems.items()[0].kind, 1);
    assert_eq!(d.settings.problems.items().len(), 1);
    let mut future = snapshot(&[]);
    future[1..3].copy_from_slice(&(CURRENT_PROTO_CMD_VER + 1).to_le_bytes());
    assert!(receive(&mut d, &client, &future).is_empty());
    assert_eq!(d.settings.problems.items().len(), 1);
}

#[test]
fn problems_known_issue_arrival_order_wins_then_full_repairs() {
    let client = crate::client::Client::new(dummy_client_cfg());
    let mut d = EventDispatcher::new();
    receive(&mut d, &client, &notification(22, 1));
    assert!(!d.settings.problems.snapshot_received());
    // Accepted Ad finem limitation: no invented receiver ordering/version scheme.
    receive(&mut d, &client, &snapshot(&[]));
    assert!(d.settings.problems.items().is_empty());
    receive(&mut d, &client, &snapshot(&[(22, 1)]));
    assert_eq!(d.settings.problems.items().len(), 1);
}

#[test]
fn problems_reset_on_hard_session_or_kernel_restart_not_soft_rebind() {
    let client = crate::client::Client::new(dummy_client_cfg());
    let mut ctx = ActiveDispatchContext::from_client(&client);
    ctx.server_token = 10;
    ctx.peer_app_token = 20;
    let mut d = EventDispatcher::new();
    let mut events = Vec::new();
    let mut actions = Vec::new();
    for (server, app, empty) in [(10, 20, false), (11, 20, true), (11, 21, true)] {
        d.dispatch_into_active_actions(
            Command::UI,
            &snapshot(&[(22, 1)]),
            0,
            &mut events,
            &ctx,
            &mut actions,
        );
        ctx.server_token = server;
        ctx.peer_app_token = app;
        d.dispatch_into_active_actions(
            Command::UI,
            &header(34),
            1,
            &mut events,
            &ctx,
            &mut actions,
        );
        assert_eq!(d.settings.problems.items().is_empty(), empty);
        assert_eq!(d.settings.problems.snapshot_received(), !empty);
    }
}
