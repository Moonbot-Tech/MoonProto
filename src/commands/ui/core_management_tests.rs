use super::*;
use crate::commands::registry::{find_descriptor, CommandDirection, CommandPriority};
use crate::protocol::Command;
use crate::state::{SettingsEvent, SettingsState};

#[test]
fn management_ids_preserve_retired_slots_and_encryption() {
    assert!(find_descriptor(Command::UI, 51).is_none());
    assert!(find_descriptor(Command::UI, 52).is_none());
    for (id, priority, direction) in [
        (53, CommandPriority::High, CommandDirection::Outbound),
        (54, CommandPriority::Sliced, CommandDirection::Inbound),
        (55, CommandPriority::High, CommandDirection::Outbound),
    ] {
        let desc = find_descriptor(Command::UI, id).unwrap();
        assert_eq!(desc.priority, priority);
        assert_eq!(desc.direction, direction);
        assert!(desc.default_encrypted);
        assert_eq!(desc.unique_kind, 0);
    }
    let request = build_settings_snapshot_request(0x1234, 0x5678, -1);
    assert_eq!(request.len(), 27);
    assert_eq!(&request[..3], &[53, 4, 0]);
    assert_eq!(&request[3..11], &0x1234u64.to_le_bytes());
    assert_eq!(&request[11..19], &0x5678u64.to_le_bytes());
    assert_eq!(&request[19..27], &(-1i64).to_le_bytes());
}

#[test]
fn legacy_update_wire_and_terminal_event_are_unchanged() {
    for (name, release) in [("", true), ("MoonBot-7.72", false)] {
        let payload = build_update_version(0x1234, name, release);
        let mut expected = vec![6, 4, 0, 0x34, 0x12, 0, 0, 0, 0, 0, 0];
        expected.extend_from_slice(&(name.len() as u16).to_le_bytes());
        expected.extend_from_slice(name.as_bytes());
        expected.push(u8::from(release));
        assert_eq!(payload, expected);
        for ver in 1u16..=4 {
            let mut historical = payload.clone();
            historical[1..3].copy_from_slice(&ver.to_le_bytes());
            let command = UICommand::parse(&historical).unwrap();
            assert!(
                matches!(SettingsState::new().apply(command), Some(SettingsEvent::VersionUpdate {
                uid: 0x1234, version_name, is_release,
            }) if version_name == name && is_release == release)
            );
        }
    }
}

#[test]
fn targeted_update_result_never_becomes_a_terminal_update() {
    for error in ["", "HTTP 404"] {
        let mut payload = builders::build_core_update(10, "missing-build", false);
        assert_eq!(&payload[..3], &[6, 5, 0]);
        payload.truncate(payload.len() - 2);
        write_string(&mut payload, error);
        let command = UICommand::parse(&payload).unwrap();
        assert!(
            matches!(SettingsState::new().apply(command), Some(SettingsEvent::CoreUpdateResult {
            version_name, is_release: false, error: actual,
        }) if version_name == "missing-build" && actual == error)
        );
    }
    // Version 5 is local to this command; it must not raise the shared gate.
    let mut payload = builders::build_core_update(10, "", true);
    payload[1] = 6;
    assert!(matches!(
        UICommand::parse(&payload),
        Some(UICommand::Skipped { ver: 6, .. })
    ));
    payload[1] = 5;
    payload[0] = CMD_SETTINGS_REQUEST;
    assert!(matches!(
        UICommand::parse(&payload),
        Some(UICommand::Skipped { ver: 5, .. })
    ));
}

#[test]
fn targeted_update_requires_complete_new_fields() {
    let payload = builders::build_core_update(10, "", true);
    for end in 11..payload.len() {
        assert!(UICommand::parse(&payload[..end]).is_none(), "accepted cut at {end}");
    }
    // A v5 command with a false OnlyThisCore keeps the legacy broadcast meaning.
    let mut broadcast = payload;
    broadcast[14] = 0;
    assert!(matches!(
        SettingsState::new().apply(UICommand::parse(&broadcast).unwrap()),
        Some(SettingsEvent::VersionUpdate { .. })
    ));
}
