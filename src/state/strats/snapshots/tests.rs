//! Resolution payloads must survive later snapshots and edits of the same strategy ID.

use super::*;
use crate::commands::strategy_serializer::{StrategyFields, StrategyKind};

/// Return one small strategy with a revision and meaningful changed field.
fn strategy(date: u64, price: f64) -> StrategySnapshot {
    let mut fields = StrategyFields::new();
    fields.insert("BuyPrice", FieldValue::Double(price));
    StrategySnapshot::new(7, 1, date, false, StrategyKind::MANUAL, "", fields)
}

/// An adjusted echo followed immediately by another edit cannot change the earlier result's evidence.
#[test]
fn resolution_keeps_exact_desired_and_actual_revisions() {
    let mut state = StratsState::new();
    state.apply_server_snapshot(strategy(1, 10.0), &mut StrategySnapshotApplyOutcome::default());
    state.stage_local_snapshot_batch(vec![strategy(2, 12.0)], crate::MoonTime::now(), Instant::now());
    let mut outcome = StrategySnapshotApplyOutcome::default();
    state.apply_server_snapshot(strategy(2, 11.5), &mut outcome);
    state.stage_local_snapshot_batch(vec![strategy(3, 13.0)], crate::MoonTime::now(), Instant::now());
    state.apply_server_snapshot(strategy(3, 13.0), &mut StrategySnapshotApplyOutcome::default());
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
    state.apply_server_snapshot(strategy(3, 13.0), &mut StrategySnapshotApplyOutcome::default());
    let outcome = state.stage_local_snapshot_batch(vec![strategy(2, 12.0)], crate::MoonTime::now(), Instant::now());
    assert!(outcome.submitted.is_empty());
    assert_eq!(outcome.resolved[0].desired.last_date, 2);
    assert_eq!(outcome.resolved[0].actual.last_date, 3);
    assert_eq!(outcome.resolved[0].outcome, StrategyResolutionKind::Superseded);
}
