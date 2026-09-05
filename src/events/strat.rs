//! Active `MPC_Strat` dispatch.
//!
//! Keeps strategy protocol effects together: parse `TStratCommand`, apply
//! snapshot/update state, and auto-decode serializer payloads into `StratsState`.

use super::{Event, EventDispatcher};
use crate::commands::strat::StratCommand;
use crate::protocol::Command;
use crate::state::StratEvent;

impl EventDispatcher {
    pub(super) fn client_new_data_strat(&mut self, payload: &[u8], out: &mut Vec<Event>) {
        match StratCommand::parse(payload) {
            Some(cmd_v) => self.process_strat_command(cmd_v, out),
            None => Self::push_parse_failed(out, Command::Strat, payload),
        }
    }

    /// Delphi equivalent: `TMoonProtoNetClient.ProcessStratCommand`.
    fn process_strat_command(&mut self, cmd_v: StratCommand, out: &mut Vec<Event>) {
        match &cmd_v {
            StratCommand::SnapshotRequest { uid } => {
                self.strategy_snapshot_request_uids.push(*uid);
                return;
            }
            StratCommand::SellPriceUpdate(_)
            | StratCommand::SchemaRequest { .. }
            | StratCommand::Skipped { .. }
            | StratCommand::Unknown { .. } => return,
            _ => {}
        }
        match cmd_v {
            StratCommand::DetectSignal(cmd) => {
                if self.markets.get(&cmd.market_name).is_none() {
                    log::warn!(
                        target: "moonproto::events",
                        "detect dropped: market not found {}",
                        cmd.market_name
                    );
                    return;
                }
                if !cmd.has_alert() && self.strats.get(cmd.strategy_id).is_none() {
                    log::warn!(target: "moonproto::events", "detect dropped: strat not found");
                    return;
                }
                out.push(Event::Detect(crate::events::DetectEvent::from_command(cmd)));
            }
            StratCommand::Snapshot(snap) => {
                let raw_len = snap.data.len();
                let Some(apply_outcome) = self
                    .strats
                    .apply_snapshot_decoded_with_mode_in_place(&snap.data, snap.full)
                else {
                    log::warn!(
                        target: "moonproto::events",
                        "failed to decode {} strategy snapshot payload ({} bytes)",
                        if snap.full { "full" } else { "partial" },
                        raw_len
                    );
                    return;
                };
                self.strats.last_server_epoch = snap.server_epoch;
                if snap.full {
                    self.strats
                        .apply_server_folders(&apply_outcome.paths, snap.folders_last_modified);
                    self.strats.apply_server_order(
                        &apply_outcome.order,
                        snap.server_epoch,
                        snap.server_epoch >= self.local_strategy_epoch,
                    );
                    self.local_strategy_epoch = self.local_strategy_epoch.max(snap.server_epoch);
                }
                let ev = if snap.full {
                    StratEvent::SnapshotFull {
                        server_epoch: snap.server_epoch,
                        #[cfg(any(test, feature = "diagnostics"))]
                        raw_len,
                        #[cfg(feature = "diagnostics")]
                        raw_data: snap.data,
                    }
                } else {
                    StratEvent::SnapshotPartial {
                        server_epoch: snap.server_epoch,
                        #[cfg(any(test, feature = "diagnostics"))]
                        raw_len,
                        #[cfg(feature = "diagnostics")]
                        raw_data: snap.data,
                    }
                };
                out.push(Event::Strat(ev));
                if !apply_outcome.confirmed.is_empty() {
                    out.push(Event::Strat(StratEvent::EditConfirmed {
                        strategy_ids: apply_outcome.confirmed,
                    }));
                }
                if !apply_outcome.adjusted.is_empty() {
                    out.push(Event::Strat(StratEvent::EditAdjusted {
                        strategy_ids: apply_outcome.adjusted,
                    }));
                }
                if !apply_outcome.superseded.is_empty() {
                    out.push(Event::Strat(StratEvent::EditSuperseded {
                        strategy_ids: apply_outcome.superseded,
                    }));
                }
            }
            other => {
                if let Some(ev) = self.strats.apply(other) {
                    out.push(Event::Strat(ev));
                }
            }
        }
    }
}
