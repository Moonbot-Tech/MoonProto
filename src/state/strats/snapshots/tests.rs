//! Resolution payloads must survive later snapshots and edits of the same strategy ID.

use super::*;
use crate::commands::strategy_serializer::{StrategyFields, StrategyKind};

/// Return one small strategy with a revision and meaningful changed field.
fn strategy(date: u64, price: f64) -> StrategySnapshot {
    let mut fields = StrategyFields::new();
    fields.insert("BuyPrice", FieldValue::Double(price));
    StrategySnapshot::new(7, 1, date, false, StrategyKind::MANUAL, "", fields)
}

fn projected_strategy(date: u64, price: f64) -> StrategySnapshot {
    let mut raw = vec![1, 0, 8];
    raw.extend_from_slice(b"BuyPrice");
    raw.extend_from_slice(&[1, 0, 0, 1, 0]); // one empty path, one row
    raw.extend_from_slice(&7u64.to_le_bytes());
    raw.extend_from_slice(&1i32.to_le_bytes());
    raw.extend_from_slice(&date.to_le_bytes());
    raw.extend_from_slice(&[0, StrategyKind::MANUAL.ordinal(), 0, 0, 1, 0, 0, 0, 4]);
    raw.extend_from_slice(&price.to_le_bytes());
    crate::commands::strategy_serializer::parse_strategy_batch_plain(&raw).unwrap().strategies.remove(0)
}

/// An adjusted echo followed immediately by another edit cannot change the earlier result's evidence.
#[test]
fn resolution_keeps_exact_desired_and_actual_revisions() {
    let mut state = StratsState::new();
    state.apply_server_snapshot(strategy(1, 10.0), true, &mut StrategySnapshotApplyOutcome::default());
    state.stage_local_snapshot_batch(vec![strategy(2, 12.0)], crate::MoonTime::now(), Instant::now());
    let mut outcome = StrategySnapshotApplyOutcome::default();
    state.apply_server_snapshot(strategy(2, 11.5), true, &mut outcome);
    state.stage_local_snapshot_batch(vec![strategy(3, 13.0)], crate::MoonTime::now(), Instant::now());
    state.apply_server_snapshot(strategy(3, 13.0), true, &mut StrategySnapshotApplyOutcome::default());
    assert_eq!(outcome.resolved.len(), 1);
    let resolution = &outcome.resolved[0];
    assert_eq!(resolution.outcome, StrategyResolutionKind::Adjusted);
    assert_eq!(resolution.desired.last_date, 2);
    assert_eq!(resolution.actual.last_date, 2);
    assert_eq!(resolution.desired.fields.get("BuyPrice"), Some(&FieldValue::Double(12.0)));
    assert_eq!(resolution.actual.fields.get("BuyPrice"), Some(&FieldValue::Double(11.5)));
}

/// Rejection by a newer retained core revision must identify the attempted revision, not the new one.
#[test]
fn staging_stale_edit_preserves_its_identity() {
    let mut state = StratsState::new();
    state.apply_server_snapshot(strategy(3, 13.0), true, &mut StrategySnapshotApplyOutcome::default());
    let outcome = state.stage_local_snapshot_batch(vec![strategy(2, 12.0)], crate::MoonTime::now(), Instant::now());
    assert!(outcome.submitted.is_empty());
    assert_eq!(outcome.resolved[0].desired.last_date, 2);
    assert_eq!(outcome.resolved[0].actual.last_date, 3);
    assert_eq!(outcome.resolved[0].outcome, StrategyResolutionKind::Superseded);
}

#[test]
fn equal_full_completes_fields_without_replacing_known_values() {
    let mut state = StratsState::new();
    let mut outcome = StrategySnapshotApplyOutcome::default();
    state.apply_server_snapshot(projected_strategy(1, 10.0), true, &mut outcome);
    let mut expanded = strategy(1, 99.0);
    expanded.fields.insert("MinLeverage", FieldValue::Int32(5));
    assert!(state.apply_server_snapshot(expanded.clone(), true, &mut outcome));
    let retained = state.snapshot(7).unwrap();
    assert_eq!(retained.fields.get("BuyPrice"), Some(&FieldValue::Double(10.0)));
    assert_eq!(retained.fields.get("MinLeverage"), Some(&FieldValue::Int32(5)));
    assert_eq!(retained.last_date, 1);
    assert!(!state.apply_server_snapshot(expanded, true, &mut outcome));
}

#[test]
fn completing_projection_preserves_exact_submitted_edit() {
    for local_leverage in [None, Some(1), Some(9)] {
        let mut state = StratsState::new();
        let mut outcome = StrategySnapshotApplyOutcome::default();
        state.apply_server_snapshot(projected_strategy(1, 10.0), true, &mut outcome);
        let mut desired = projected_strategy(2, 12.0);
        if let Some(leverage) = local_leverage {
            desired.fields.insert("MinLeverage", FieldValue::Int32(leverage));
        }
        state.stage_local_snapshot_batch(vec![desired.clone()], crate::MoonTime::now(), Instant::now());
        let mut expanded = strategy(1, 10.0);
        expanded.fields.insert("MinLeverage", FieldValue::Int32(5));
        state.apply_server_snapshot(expanded, true, &mut outcome);
        assert_eq!(state.strategy_edit(7).unwrap().desired(), &desired);
        let outgoing = &state.local_snapshots.as_ref().unwrap()[0];
        assert_eq!(outgoing.fields.get("MinLeverage"), local_leverage.map(FieldValue::Int32).as_ref());
        assert_eq!(outgoing.fields.get("BuyPrice"), Some(&FieldValue::Double(12.0)));
        assert_eq!(outgoing.last_date, 2);
        assert!(outcome.resolved.is_empty());
    }
}

#[test]
fn equal_partial_keeps_the_revision_guard() {
    let mut state = StratsState::new();
    let mut outcome = StrategySnapshotApplyOutcome::default();
    state.apply_server_snapshot(strategy(1, 10.0), true, &mut outcome);
    let mut expanded = strategy(1, 99.0);
    expanded.fields.insert("MinLeverage", FieldValue::Int32(5));
    assert!(!state.apply_server_snapshot(expanded, false, &mut outcome));
    assert!(state.snapshot(7).unwrap().fields.get("MinLeverage").is_none());
    assert_eq!(state.snapshot(7).unwrap().fields.get("BuyPrice"), Some(&FieldValue::Double(10.0)));
}
