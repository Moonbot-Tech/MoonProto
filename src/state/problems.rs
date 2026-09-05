//! Confirmed diagnostic problems reported by the MoonBot core.

use crate::time::MoonTime;
use std::sync::Arc;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProblemCategory {
    Machine,
    Exchange,
    Network,
    Other,
    Unknown(u8),
}

impl ProblemCategory {
    pub(crate) fn from_byte(value: u8) -> Self {
        match value {
            0 => Self::Machine,
            1 => Self::Exchange,
            2 => Self::Network,
            3 => Self::Other,
            value => Self::Unknown(value),
        }
    }
}

/// One confirmed core problem, not a local terminal or connection error.
#[derive(Debug, Clone, PartialEq)]
pub struct KernelProblem {
    /// Core problem identity. There is at most one retained row per kind.
    pub kind: u8,
    /// Stable textual key, for example `paging`, `region-blocked`, or `test`.
    pub kind_name: String,
    pub category: ProblemCategory,
    /// Display text is in the core's selected language.
    pub title: String,
    pub message: String,
    /// Detector evidence and thresholds; display as details, do not parse as a schema.
    pub technical_details: String,
    pub first_seen: MoonTime,
    pub confirmed: MoonTime,
    /// Last received confirmation count. Repeat confirmations are not streamed.
    pub confirmations: i32,
}

/// Latest received diagnostic list. Initial delivery does not block client readiness.
/// Full lists and notifications are applied in arrival order; a rare reordered
/// packet can temporarily stale the list until the next current full snapshot.
#[derive(Debug, Clone, Default)]
pub struct ProblemsState {
    items: Vec<Arc<KernelProblem>>,
    snapshot_received: bool,
}

impl ProblemsState {
    /// Whether a complete list has arrived in this hard connection session.
    /// An empty list before this point does not mean the core has no problems.
    pub fn snapshot_received(&self) -> bool {
        self.snapshot_received
    }

    pub fn items(&self) -> &[Arc<KernelProblem>] {
        &self.items
    }

    pub(crate) fn apply_snapshot(&mut self, items: Vec<Arc<KernelProblem>>) {
        // Known limitation: the core does not version these snapshots. Arrival
        // order wins, even if an older snapshot arrives after a live notification.
        self.items = items;
        self.snapshot_received = true;
    }

    pub(crate) fn apply_notification(&mut self, problem: Arc<KernelProblem>) {
        if let Some(item) = self.items.iter_mut().find(|item| item.kind == problem.kind) {
            *item = problem;
        } else {
            self.items.push(problem);
        }
    }
}
