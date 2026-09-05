//! Delphi `TStrategies` folder tree helpers.

use super::StratsState;
use std::collections::HashMap;
use std::sync::Arc;

impl StratsState {
    /// Complete core-confirmed folder paths, including empty folders and parents.
    /// Iteration order is unspecified; strategy order is exposed separately.
    pub fn folder_paths(&self) -> impl Iterator<Item = &str> {
        self.folders_by_key.values().map(String::as_str)
    }

    /// UTC milliseconds of the confirmed folder tree. Zero means that no
    /// versioned tree has arrived yet (including cores without folder sync).
    pub fn folders_last_modified(&self) -> i64 {
        self.folders_last_modified
    }

    pub(crate) fn local_folders_last_modified(&self) -> i64 {
        self.local_folders
            .as_ref()
            .map_or(self.folders_last_modified, |(date, _)| *date)
    }

    pub(super) fn outgoing_folder_paths(&self) -> Vec<String> {
        let folders = self
            .local_folders
            .as_ref()
            .map_or(&self.folders_by_key, |(_, paths)| paths);
        let mut paths: Vec<_> = folders.values().cloned().collect();
        paths.sort_unstable();
        paths
    }

    pub(super) fn add_folder_path(folders: &mut HashMap<String, String>, path: &str) -> bool {
        if path.is_empty() || folders.contains_key(&Self::folder_key(path)) {
            return false;
        }
        let mut current = String::new();
        for part in path.split('/') {
            if !current.is_empty() {
                current.push('/');
            }
            current.push_str(part);
            folders
                .entry(Self::folder_key(&current))
                .or_insert(current.clone());
        }
        true
    }

    pub(crate) fn stage_local_folders(&mut self, paths: Option<&[String]>, now_ms: i64) -> bool {
        let previous = self
            .local_folders
            .as_ref()
            .map_or(&self.folders_by_key, |(_, paths)| paths);
        let mut wanted = if paths.is_some() {
            HashMap::new()
        } else {
            previous.clone()
        };
        if let Some(paths) = paths {
            for path in paths {
                Self::add_folder_path(&mut wanted, path);
            }
        }
        // A folder containing a retained/local strategy cannot be deleted by absence.
        if let Some(local) = &self.local_snapshots {
            for strategy in local {
                Self::add_folder_path(&mut wanted, &strategy.path);
            }
        } else {
            for strategy in self.snapshots() {
                Self::add_folder_path(&mut wanted, &strategy.path);
            }
        }
        if wanted.len() == previous.len() && wanted.keys().all(|key| previous.contains_key(key)) {
            return false;
        }
        let date = now_ms.max(self.local_folders_last_modified().saturating_add(1));
        self.local_folders = Some((date, wanted));
        self.invalidate_snapshot_payload_cache();
        true
    }

    pub(crate) fn apply_server_folders(&mut self, paths: &[Arc<str>], date: i64) {
        if date <= self.folders_last_modified {
            return;
        }
        let mut wanted = HashMap::new();
        for path in paths {
            Self::add_folder_path(&mut wanted, path);
            self.create_folders_for_path(path);
        }
        let absent: Vec<_> = self
            .folders_by_key
            .iter()
            .filter(|(key, _)| !wanted.contains_key(*key))
            .map(|(_, path)| path.clone())
            .collect();
        for path in absent {
            self.delete_folder_by_path(&path);
        }
        self.folders_last_modified = date;
        if self
            .local_folders
            .as_ref()
            .is_some_and(|(pending, _)| date >= *pending)
        {
            self.local_folders = None;
        }
        self.invalidate_snapshot_payload_cache();
    }

    pub(super) fn folder_key(path: &str) -> String {
        path.to_lowercase()
    }

    fn is_same_or_child_folder(candidate_key: &str, folder_key: &str) -> bool {
        candidate_key == folder_key
            || candidate_key
                .strip_prefix(folder_key)
                .is_some_and(|rest| rest.starts_with('/'))
    }

    pub(super) fn create_folders_for_path(&mut self, path: &str) {
        if Self::add_folder_path(&mut self.folders_by_key, path) {
            self.invalidate_snapshot_payload_cache();
        }
    }

    pub(super) fn remove_strategy_by_id(&mut self, strategy_id: u64) -> bool {
        let removed = self.by_id.remove(&strategy_id).is_some();
        if removed {
            self.order.retain(|id| *id != strategy_id);
            self.snapshots_by_id.remove(&strategy_id);
            self.remove_local_snapshot(strategy_id);
            self.invalidate_snapshot_payload_cache();
        }
        removed
    }

    fn folder_has_strategies(&self, folder_key: &str) -> bool {
        self.by_id.values().any(|entry| {
            let entry_key = Self::folder_key(&entry.folder_path);
            Self::is_same_or_child_folder(&entry_key, folder_key)
        })
    }

    pub(super) fn delete_folder_by_path(&mut self, path: &str) -> bool {
        if path.is_empty() {
            return false;
        }

        let key = Self::folder_key(path);
        if !self.folders_by_key.contains_key(&key) {
            return false;
        }
        if self.folder_has_strategies(&key) {
            return false;
        }

        let deleted_keys: Vec<String> = self
            .folders_by_key
            .keys()
            .filter(|candidate_key| Self::is_same_or_child_folder(candidate_key, &key))
            .cloned()
            .collect();
        for deleted_key in deleted_keys {
            self.folders_by_key.remove(&deleted_key);
        }
        self.invalidate_snapshot_payload_cache();
        true
    }
}
