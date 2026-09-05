//! Strategy snapshot apply/cache helpers.

use super::{
    StrategyEdit, StrategyEditStageOutcome, StrategyEditStatus, StrategySnapshotApplyOutcome,
    StrategySnapshotPayloadCache, StratsState,
};
use crate::commands::strat::StratCheckedItem;
use crate::commands::strategy_serializer::{
    parse_strategy_batch_for_each_with_schema_field_types_skip_old,
    parse_strategy_batch_with_schema, parse_strategy_batch_with_schema_field_types, FieldValue,
    StrategyBatch, StrategySnapshot,
};
use std::cell::Cell;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Instant;

#[derive(Clone, Copy)]
enum EditResolution {
    Confirmed,
    Adjusted,
    Superseded,
}

impl StratsState {
    pub(crate) fn local_order_matches(&self, strategies: &[StrategySnapshot]) -> bool {
        let incoming = strategies.iter().map(|s| s.strategy_id);
        match &self.local_snapshots {
            Some(local) => local.iter().map(|s| s.strategy_id).eq(incoming),
            None => self.order.iter().copied().eq(incoming),
        }
    }

    pub(crate) fn apply_server_order(&mut self, order: &[u64], date: u64, update_local: bool) {
        if self.last_modified != 0 && date <= self.last_modified {
            return;
        }
        let ranks: HashMap<_, _> = order.iter().enumerate().map(|(i, &id)| (id, i)).collect();
        // Stable sorting keeps absent Own strategies without implicitly deleting them.
        self.order
            .sort_by_key(|id| ranks.get(id).copied().unwrap_or(usize::MAX));
        if update_local {
            if let Some(local) = &mut self.local_snapshots {
                local.sort_by_key(|s| ranks.get(&s.strategy_id).copied().unwrap_or(usize::MAX));
                for (i, s) in local.iter().enumerate() {
                    self.local_snapshot_index.insert(s.strategy_id, i);
                }
            }
        }
        self.last_modified = date;
        self.invalidate_snapshot_payload_cache();
    }

    pub(super) fn invalidate_snapshot_payload_cache(&mut self) {
        self.snapshot_payload_cache = None;
    }

    fn set_snapshot_payload_cache_from_wire(
        &mut self,
        client_max_last_date: u64,
        deflate_data: &[u8],
    ) {
        self.snapshot_payload_cache = Some(Arc::new(StrategySnapshotPayloadCache {
            client_max_last_date,
            data: deflate_data.to_vec(),
        }));
    }

    fn update_snapshot_payload_cache_after_apply(
        &mut self,
        decoded: (usize, &[Arc<str>]),
        client_max_last_date: u64,
        deflate_data: &[u8],
        changed: bool,
        skipped_old: bool,
    ) {
        if self.local_snapshots.is_some() || self.local_folders.is_some() {
            if changed {
                self.invalidate_snapshot_payload_cache();
            }
            return;
        }

        if skipped_old {
            if changed {
                self.invalidate_snapshot_payload_cache();
            }
            return;
        }

        if decoded.0 == self.snapshots_by_id.len() {
            let mut paths = HashMap::new();
            for path in decoded.1 {
                Self::add_folder_path(&mut paths, path);
            }
            if paths.len() == self.folders_by_key.len()
                && self
                    .folders_by_key
                    .keys()
                    .all(|key| paths.contains_key(key))
            {
                self.set_snapshot_payload_cache_from_wire(client_max_last_date, deflate_data);
            } else {
                self.invalidate_snapshot_payload_cache();
            }
        } else if changed {
            self.invalidate_snapshot_payload_cache();
        }
    }

    fn sell_price_from_snapshot(s: &StrategySnapshot) -> f64 {
        match s.fields.get("SellPrice") {
            Some(FieldValue::Double(v)) => *v,
            _ => 0.0,
        }
    }

    /// Update lightweight strategy state from decoded snapshot header fields.
    pub fn upsert(
        &mut self,
        strategy_id: u64,
        last_date: u64,
        folder_path: impl Into<std::sync::Arc<str>>,
    ) {
        let entry = self.get_or_insert(strategy_id);
        entry.last_date = last_date;
        entry.folder_path = folder_path.into();
        let path = entry.folder_path.clone();
        self.create_folders_for_path(&path);
    }

    /// Replace the application-owned strategy list.
    ///
    /// User code normally calls this before init. After that the dispatcher owns
    /// the list, sends it during init, and maintains it through the protocol.
    pub fn replace_with_snapshots(&mut self, strategies: &[StrategySnapshot]) {
        self.clear_entries();
        for strategy in strategies {
            self.insert_snapshot_unchecked(strategy.clone());
        }
    }

    /// Stage the latest complete local list without overwriting core-confirmed state.
    pub(crate) fn stage_local_snapshot_batch(
        &mut self,
        strategies: Vec<StrategySnapshot>,
        submitted_at: crate::MoonTime,
        deadline: Instant,
    ) -> StrategyEditStageOutcome {
        let mut local = Vec::<Arc<StrategySnapshot>>::with_capacity(strategies.len());
        let mut index = HashMap::<u64, usize>::with_capacity(strategies.len());
        for strategy in strategies {
            let strategy = Arc::new(strategy);
            if let Some(&position) = index.get(&strategy.strategy_id) {
                local[position] = strategy;
            } else {
                index.insert(strategy.strategy_id, local.len());
                local.push(strategy);
            }
        }

        let mut outcome = StrategyEditStageOutcome::default();
        let mut edits = HashMap::with_capacity(local.len());
        for strategy in &mut local {
            let Some(confirmed) = self.snapshots_by_id.get(&strategy.strategy_id) else {
                edits.insert(
                    strategy.strategy_id,
                    StrategyEdit::new(Arc::clone(strategy), submitted_at, deadline),
                );
                outcome.submitted.push(strategy.strategy_id);
                continue;
            };
            if self.strategy_effectively_equal(strategy.as_ref(), confirmed.as_ref()) {
                continue;
            }
            if revision_strictly_dominates(confirmed.as_ref(), strategy.as_ref()) {
                *strategy = Arc::clone(confirmed);
                outcome.superseded.push(strategy.strategy_id);
                continue;
            }
            edits.insert(
                strategy.strategy_id,
                StrategyEdit::new(Arc::clone(strategy), submitted_at, deadline),
            );
            outcome.submitted.push(strategy.strategy_id);
        }

        self.local_snapshots = Some(local);
        self.local_snapshot_index = index;
        self.strategy_edits = edits;
        self.recompute_next_strategy_edit_deadline();
        self.invalidate_snapshot_payload_cache();
        outcome
    }

    /// Insert or replace one application-owned strategy without rollback guard.
    ///
    /// For the local strategy list the application is the source of truth, so an
    /// explicit snapshot replaces the previous value even with equal dates or
    /// versions.
    pub fn upsert_local_snapshot(&mut self, strategy: StrategySnapshot) {
        self.insert_snapshot_unchecked(strategy);
    }

    fn insert_snapshot_unchecked(&mut self, s: StrategySnapshot) {
        {
            let entry = self.get_or_insert(s.strategy_id);
            entry.strategy_ver = s.strategy_ver;
            entry.last_date = s.last_date;
            entry.folder_path = s.path.clone();
            entry.sell_price = Self::sell_price_from_snapshot(&s);
            entry.checked = s.checked;
            entry.prev_checked = s.checked;
        }
        self.create_folders_for_path(&s.path);
        self.snapshots_by_id.insert(s.strategy_id, Arc::new(s));
        self.invalidate_snapshot_payload_cache();
    }

    /// Apply one decoded strategy snapshot after `parse_strategy_batch`.
    ///
    /// Updates `last_date`, `folder_path`, and `checked` from the header and
    /// stores the full `StrategySnapshot` for API reads and
    /// `TStratSnapshotRequest` replies.
    pub fn upsert_from_snapshot(&mut self, s: &StrategySnapshot) -> bool {
        {
            let (existed, entry) = self.get_or_insert_with_existed(s.strategy_id);
            if existed && entry.last_date >= s.last_date && entry.strategy_ver >= s.strategy_ver {
                return false;
            }
            entry.strategy_ver = s.strategy_ver;
            entry.last_date = s.last_date;
            entry.folder_path = s.path.clone();
            entry.sell_price = Self::sell_price_from_snapshot(s);
            entry.checked = s.checked;
            entry.prev_checked = s.checked;
        }
        self.create_folders_for_path(&s.path);
        self.snapshots_by_id
            .insert(s.strategy_id, Arc::new(s.clone()));
        self.invalidate_snapshot_payload_cache();
        true
    }

    fn upsert_snapshot_arc_without_cache_invalidation(&mut self, s: Arc<StrategySnapshot>) -> bool {
        {
            let (existed, entry) = self.get_or_insert_with_existed(s.strategy_id);
            if existed && entry.last_date >= s.last_date && entry.strategy_ver >= s.strategy_ver {
                return false;
            }
            entry.strategy_ver = s.strategy_ver;
            entry.last_date = s.last_date;
            entry.folder_path = s.path.clone();
            entry.sell_price = Self::sell_price_from_snapshot(&s);
            entry.checked = s.checked;
            entry.prev_checked = s.checked;
        }
        self.create_folders_for_path(&s.path);
        self.snapshots_by_id.insert(s.strategy_id, s);
        true
    }

    fn snapshot_is_old_or_equal(&self, s: &StrategySnapshot) -> bool {
        self.by_id.get(&s.strategy_id).is_some_and(|entry| {
            entry.last_date >= s.last_date && entry.strategy_ver >= s.strategy_ver
        })
    }

    fn set_local_snapshot_from_server(&mut self, snapshot: Arc<StrategySnapshot>, force: bool) {
        let Some(local) = self.local_snapshots.as_mut() else {
            return;
        };
        if let Some(&position) = self.local_snapshot_index.get(&snapshot.strategy_id) {
            if force || !revision_dominates(local[position].as_ref(), snapshot.as_ref()) {
                local[position] = snapshot;
            }
            return;
        }
        self.local_snapshot_index
            .insert(snapshot.strategy_id, local.len());
        local.push(snapshot);
    }

    pub(super) fn set_local_snapshot_checked(&mut self, strategy_id: u64, checked: bool) -> bool {
        let Some(&position) = self.local_snapshot_index.get(&strategy_id) else {
            return false;
        };
        let Some(local) = self.local_snapshots.as_mut() else {
            return false;
        };
        let snapshot = Arc::make_mut(&mut local[position]);
        if snapshot.checked == checked {
            return false;
        }
        snapshot.checked = checked;
        true
    }

    pub(super) fn remove_local_snapshot(&mut self, strategy_id: u64) {
        self.strategy_edits.remove(&strategy_id);
        let Some(&position) = self.local_snapshot_index.get(&strategy_id) else {
            self.recompute_next_strategy_edit_deadline();
            return;
        };
        if let Some(local) = self.local_snapshots.as_mut() {
            local.remove(position);
            self.local_snapshot_index.clear();
            for (index, snapshot) in local.iter().enumerate() {
                self.local_snapshot_index
                    .insert(snapshot.strategy_id, index);
            }
        }
        self.recompute_next_strategy_edit_deadline();
    }

    fn recompute_next_strategy_edit_deadline(&mut self) {
        self.next_strategy_edit_deadline = self
            .strategy_edits
            .values()
            .filter(|edit| edit.status() == StrategyEditStatus::Pending)
            .map(|edit| edit.deadline)
            .min();
    }

    pub(crate) fn tick_strategy_edit_timeouts(&mut self, now: Instant) -> Vec<u64> {
        if self
            .next_strategy_edit_deadline
            .is_none_or(|deadline| now < deadline)
        {
            return Vec::new();
        }
        let mut timed_out = Vec::new();
        for (&strategy_id, edit) in &mut self.strategy_edits {
            if edit.status() == StrategyEditStatus::Pending && now >= edit.deadline {
                edit.mark_timed_out();
                timed_out.push(strategy_id);
            }
        }
        timed_out.sort_unstable();
        self.recompute_next_strategy_edit_deadline();
        timed_out
    }

    fn apply_server_snapshot(
        &mut self,
        snapshot: StrategySnapshot,
        outcome: &mut StrategySnapshotApplyOutcome,
    ) -> bool {
        let snapshot = Arc::new(snapshot);
        let strategy_id = snapshot.strategy_id;
        let edit_result = self.strategy_edits.get(&strategy_id).and_then(|edit| {
            let desired = edit.desired();
            if same_revision(snapshot.as_ref(), desired) {
                Some(
                    if self.strategy_effectively_equal(snapshot.as_ref(), desired) {
                        EditResolution::Confirmed
                    } else {
                        EditResolution::Adjusted
                    },
                )
            } else if revision_strictly_dominates(snapshot.as_ref(), desired) {
                Some(EditResolution::Superseded)
            } else {
                None
            }
        });

        let canonical_changed =
            self.upsert_snapshot_arc_without_cache_invalidation(Arc::clone(&snapshot));
        match edit_result {
            Some(EditResolution::Confirmed) => {
                self.strategy_edits.remove(&strategy_id);
                self.set_local_snapshot_from_server(snapshot, true);
                outcome.confirmed.push(strategy_id);
            }
            Some(EditResolution::Adjusted) => {
                self.strategy_edits.remove(&strategy_id);
                self.set_local_snapshot_from_server(snapshot, true);
                outcome.adjusted.push(strategy_id);
            }
            Some(EditResolution::Superseded) => {
                self.strategy_edits.remove(&strategy_id);
                self.set_local_snapshot_from_server(snapshot, true);
                outcome.superseded.push(strategy_id);
            }
            None if !self.strategy_edits.contains_key(&strategy_id) => {
                self.set_local_snapshot_from_server(snapshot, false);
            }
            None => {}
        }
        self.recompute_next_strategy_edit_deadline();
        canonical_changed || edit_result.is_some()
    }

    fn strategy_effectively_equal(
        &self,
        left: &StrategySnapshot,
        right: &StrategySnapshot,
    ) -> bool {
        same_revision(left, right)
            && left.strategy_id == right.strategy_id
            && left.checked == right.checked
            && left.kind() == right.kind()
            && left.path == right.path
            && left
                .fields
                .iter()
                .all(|(name, value)| self.field_matches(name, value, right))
            && right
                .fields
                .iter()
                .all(|(name, value)| self.field_matches(name, value, left))
    }

    fn field_matches(&self, name: &str, value: &FieldValue, other: &StrategySnapshot) -> bool {
        if let Some(other_value) = other.fields.get(name) {
            return value == other_value;
        }
        let Some(field) = self.schema.as_deref().and_then(|schema| schema.field(name)) else {
            return false;
        };
        if !field.visible_for_strategy_kind(other.kind()) {
            return false;
        }
        field
            .default_value
            .clone()
            .or_else(|| FieldValue::zero_for_type_id(field.raw_type_id))
            .as_ref()
            .is_some_and(|default| value == default)
    }

    /// Apply the full strategy batch from `TStratSnapshot.data`.
    ///
    /// Returns the decoded `StrategyBatch` so callers can inspect the
    /// `StrategyFields`. Returns `None` when the compressed payload is malformed.
    pub fn apply_snapshot_decoded_with_mode(
        &mut self,
        deflate_data: &[u8],
        full: bool,
    ) -> Option<StrategyBatch> {
        let batch = match self.schema_field_types.as_deref() {
            Some(field_types) => {
                parse_strategy_batch_with_schema_field_types(deflate_data, Some(field_types))?
            }
            None => parse_strategy_batch_with_schema(deflate_data, None)?,
        };
        let _ = full;
        // Delphi `ApplyStratSnapshot(IsFull=true)` does not clear strategies
        // absent from the incoming payload. They remain local "Own" strategies.
        let count = batch.strategies.len();
        let mut changed = false;
        let mut skipped_old = false;
        let mut client_max_last_date = 0u64;
        let mut outcome = StrategySnapshotApplyOutcome::default();
        for s in &batch.strategies {
            client_max_last_date = client_max_last_date.max(s.last_date);
            skipped_old |= self.snapshot_is_old_or_equal(s);
            changed |= self.apply_server_snapshot(s.clone(), &mut outcome);
        }
        self.update_snapshot_payload_cache_after_apply(
            (count, &batch.paths),
            client_max_last_date,
            deflate_data,
            changed,
            skipped_old,
        );
        Some(batch)
    }

    pub(crate) fn apply_snapshot_decoded_with_mode_in_place(
        &mut self,
        deflate_data: &[u8],
        full: bool,
    ) -> Option<StrategySnapshotApplyOutcome> {
        let _ = full;
        let field_types = self.schema_field_types.clone();
        let existing_versions: HashMap<u64, (u64, i32)> = self
            .by_id
            .iter()
            .map(|(&strategy_id, info)| (strategy_id, (info.last_date, info.strategy_ver)))
            .collect();
        let pending_versions: HashMap<u64, (u64, i32)> = self
            .strategy_edits
            .iter()
            .map(|(&strategy_id, edit)| {
                let desired = edit.desired();
                (strategy_id, (desired.last_date, desired.strategy_ver))
            })
            .collect();
        let mut changed = false;
        let client_max_last_date = Cell::new(0u64);
        let skipped_old = Cell::new(false);
        let mut order = Vec::new();
        let mut outcome = StrategySnapshotApplyOutcome::default();
        let (count, paths) = parse_strategy_batch_for_each_with_schema_field_types_skip_old(
            deflate_data,
            field_types.as_deref(),
            |strategy_id, strategy_ver, last_date| {
                order.push(strategy_id);
                client_max_last_date.set(client_max_last_date.get().max(last_date));
                let old_or_equal = existing_versions.get(&strategy_id).is_some_and(
                    |(existing_last_date, existing_strategy_ver)| {
                        *existing_last_date >= last_date && *existing_strategy_ver >= strategy_ver
                    },
                );
                let resolves_pending = pending_versions.get(&strategy_id).is_some_and(
                    |(pending_last_date, pending_strategy_ver)| {
                        (last_date == *pending_last_date && strategy_ver == *pending_strategy_ver)
                            || (last_date >= *pending_last_date
                                && strategy_ver >= *pending_strategy_ver
                                && (last_date > *pending_last_date
                                    || strategy_ver > *pending_strategy_ver))
                    },
                );
                let skip = old_or_equal && !resolves_pending;
                if skip {
                    skipped_old.set(true);
                }
                skip
            },
            |s| {
                changed |= self.apply_server_snapshot(s, &mut outcome);
            },
        )?;
        outcome.count = count;
        outcome.paths = paths;
        outcome.order = order;
        self.update_snapshot_payload_cache_after_apply(
            (outcome.count, &outcome.paths),
            client_max_last_date.get(),
            deflate_data,
            changed,
            skipped_old.get(),
        );
        if outcome.order != self.order {
            self.invalidate_snapshot_payload_cache();
        }
        Some(outcome)
    }

    pub fn apply_snapshot_decoded(&mut self, deflate_data: &[u8]) -> Option<StrategyBatch> {
        self.apply_snapshot_decoded_with_mode(deflate_data, false)
    }

    pub fn upsert_checked_items(&mut self, items: &[StratCheckedItem]) {
        for it in items {
            let entry = self.get_or_insert(it.strategy_id);
            entry.checked = it.checked;
        }
    }

    pub(crate) fn snapshot_payload_cache(&mut self) -> Option<Arc<StrategySnapshotPayloadCache>> {
        if let Some(cache) = &self.snapshot_payload_cache {
            return Some(Arc::clone(cache));
        }

        let local_len = self
            .local_snapshots
            .as_ref()
            .map_or(self.snapshots_by_id.len(), Vec::len);
        if local_len == 0 {
            let cache = Arc::new(StrategySnapshotPayloadCache {
                client_max_last_date: 0,
                data: crate::commands::strategy_serializer::StrategyBatchBuilder::folder_payload(
                    self.outgoing_folder_paths(),
                ),
            });
            self.snapshot_payload_cache = Some(Arc::clone(&cache));
            return Some(cache);
        }

        let schema = Arc::clone(self.schema.as_ref()?);
        let mut builder = crate::commands::strategy_serializer::StrategyBatchBuilder::new(&schema);
        let mut client_max_last_date = 0u64;
        let mut write_strategy = |strategy: &StrategySnapshot| {
            client_max_last_date = client_max_last_date.max(strategy.last_date);
            builder.write_strategy(strategy);
        };
        if let Some(local) = &self.local_snapshots {
            for strategy in local {
                write_strategy(strategy);
            }
        } else {
            for strategy in self.snapshots() {
                write_strategy(strategy);
            }
        }
        for path in self.outgoing_folder_paths() {
            builder.path_index(&path);
        }
        let cache = Arc::new(StrategySnapshotPayloadCache {
            client_max_last_date,
            data: builder.finalize(),
        });
        self.snapshot_payload_cache = Some(Arc::clone(&cache));
        Some(cache)
    }
}

fn same_revision(left: &StrategySnapshot, right: &StrategySnapshot) -> bool {
    left.last_date == right.last_date && left.strategy_ver == right.strategy_ver
}

fn revision_dominates(left: &StrategySnapshot, right: &StrategySnapshot) -> bool {
    left.last_date >= right.last_date && left.strategy_ver >= right.strategy_ver
}

fn revision_strictly_dominates(left: &StrategySnapshot, right: &StrategySnapshot) -> bool {
    revision_dominates(left, right) && !same_revision(left, right)
}
