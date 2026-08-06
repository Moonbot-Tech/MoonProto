//! Runtime-owned pending request state and the per-tick pollers that drain it.
//!
//! Each `Pending*` struct holds an in-flight async Engine API request whose
//! response is consumed by the matching `poll_*` helper inside the runtime
//! loop. The loop owns one [`RuntimePending`] for the whole session.

use super::*;

const CHUNK_IDLE_TIMEOUT: Duration = Duration::from_secs(15);

#[derive(Default)]
pub(super) struct RuntimePending {
    pub(super) auto_candles_scope: Option<std::sync::Arc<crate::state::TradeStorageScope>>,
    pub(super) auto_candles_requested: bool,
    pub(super) auto_candles: Vec<PendingAutoCandles>,
    pub(super) auto_candles_apply: Vec<PendingAutoCandlesApply>,
    pub(super) market_history: Vec<PendingMarketHistory>,
    pub(super) market_history_apply: Vec<PendingMarketHistoryApply>,
    pub(super) coin_card_candles: Vec<PendingCoinCardCandles>,
    pub(super) account_refreshes: Vec<PendingAccountRefresh>,
    pub(super) transfer_assets: Vec<PendingTransferAssets>,
    pub(super) transfer_assets_batches: Vec<PendingTransferAssetsBatch>,
    pub(super) next_transfer_assets_batch_id: u64,
    pub(super) next_transfer_assets_refresh_at: Option<Instant>,
    pub(super) engine_actions: Vec<PendingEngineAction>,
}

pub(super) struct PendingAutoCandles {
    pub(super) uid: u64,
    pub(super) deadline: Instant,
    pub(super) progress: crate::client::ChunkProgress,
    pub(super) seen_progress: u64,
    pub(super) rx: mpsc::Receiver<crate::client::MergedCandles>,
}

pub(super) struct PendingMarketHistory {
    pub(super) ticket: crate::state::MarketHistoryTicket,
    pub(super) uid: u64,
    pub(super) deadline: Instant,
    pub(super) progress: crate::client::ChunkProgress,
    pub(super) seen_progress: u64,
    pub(super) rx: mpsc::Receiver<Result<crate::client::MergedMarketHistory, String>>,
}

pub(super) struct PendingMarketHistoryApply {
    pub(super) ticket: crate::state::MarketHistoryTicket,
    pub(super) deadline: Instant,
    pub(super) rx: mpsc::Receiver<Result<crate::state::MarketHistoryApplySummary, String>>,
}

pub(super) struct PendingAutoCandlesApply {
    #[cfg(any(test, feature = "diagnostics"))]
    pub(super) uid: u64,
    pub(super) summary: crate::state::CandlesSnapshotApplySummary,
    pub(super) deadline: Instant,
    pub(super) rx: mpsc::Receiver<()>,
}

pub(super) struct PendingTransferAssets {
    pub(super) kind: crate::state::ExchangeKind,
    pub(super) batch_id: Option<u64>,
    pub(super) request_uid: Option<u64>,
    pub(super) deadline: Instant,
    pub(super) rx: mpsc::Receiver<crate::commands::engine_api::EngineResponse>,
}

pub(super) struct PendingTransferAssetsBatch {
    pub(super) id: u64,
    pub(super) remaining: usize,
    pub(super) updated: usize,
    pub(super) failed: usize,
}

pub(super) struct PendingCoinCardCandles {
    pub(super) ticket: super::super::CoinCardCandlesTicket,
    pub(super) deadline: Instant,
    pub(super) rx: mpsc::Receiver<crate::commands::engine_api::EngineResponse>,
}

pub(super) struct PendingAccountRefresh {
    pub(super) kind: PendingAccountRefreshKind,
    pub(super) request_uid: Option<u64>,
    pub(super) deadline: Instant,
    pub(super) rx: mpsc::Receiver<crate::commands::engine_api::EngineResponse>,
}

#[derive(Clone, Copy)]
pub(super) enum PendingAccountRefreshKind {
    HedgeMode,
    ApiExpiration,
}

fn remove_api_pending(api_pending: &ApiPending, request_uid: Option<u64>) {
    if let Some(uid) = request_uid {
        api_pending.remove(uid);
    }
}

fn engine_pending_deadline() -> Instant {
    Instant::now() + Duration::from_millis(crate::api_pending::DEFAULT_PENDING_TIMEOUT_MS as u64)
}

pub(super) fn chunk_idle_deadline() -> Instant {
    Instant::now() + CHUNK_IDLE_TIMEOUT
}

fn extend_chunk_deadline(
    deadline: &mut Instant,
    seen_progress: &mut u64,
    progress: &crate::client::ChunkProgress,
    now: Instant,
) {
    let generation = progress.generation();
    if generation != *seen_progress {
        *seen_progress = generation;
        *deadline = now + CHUNK_IDLE_TIMEOUT;
    }
}

pub(super) fn clear_auto_candles_pending(client: &mut Client, pending: &mut RuntimePending) {
    for item in pending.auto_candles.drain(..) {
        client.pending_api.pending_candles.remove(&item.uid);
    }
    pending.auto_candles_apply.clear();
    pending.auto_candles_requested = false;
}

pub(super) struct PendingEngineAction {
    pub(super) kind: crate::events::EngineActionKind,
    pub(super) ticket: super::super::EngineActionTicket,
    pub(super) deadline: Instant,
    pub(super) rx: mpsc::Receiver<crate::commands::engine_api::EngineResponse>,
}

pub(super) fn poll_auto_candles(
    client: &mut Client,
    pending: &mut RuntimePending,
    dispatcher: &mut crate::events::EventDispatcher,
) -> bool {
    let mut changed = false;
    let mut i = 0;
    let now = Instant::now();
    while i < pending.auto_candles.len() {
        let item = &mut pending.auto_candles[i];
        extend_chunk_deadline(
            &mut item.deadline,
            &mut item.seen_progress,
            &item.progress,
            now,
        );
        match pending.auto_candles[i].rx.try_recv() {
            Ok(merged) => {
                #[cfg(any(test, feature = "diagnostics"))]
                let request_uid = merged.uid;
                #[cfg(any(test, feature = "diagnostics"))]
                let fallback_uid = pending.auto_candles[i].uid;
                let summary = dispatcher.apply_candles_snapshot(
                    &merged.markets,
                    client.now_ms(),
                    #[cfg(any(test, feature = "diagnostics"))]
                    Some(client.metrics.protocol_metrics.as_ref()),
                );
                pending.auto_candles.swap_remove(i);
                if let Some(summary) = summary {
                    if let Some(rx) = dispatcher.market_history_barrier_async() {
                        pending.auto_candles_apply.push(PendingAutoCandlesApply {
                            #[cfg(any(test, feature = "diagnostics"))]
                            uid: request_uid,
                            summary,
                            deadline: engine_pending_deadline(),
                            rx,
                        });
                    } else {
                        dispatcher.queue_candles_snapshot_event(
                            crate::state::CandlesSnapshotEvent::Failed {
                                #[cfg(any(test, feature = "diagnostics"))]
                                request_uid: Some(request_uid),
                                error: "market history worker unavailable after snapshot apply"
                                    .to_string(),
                            },
                        );
                        changed = true;
                    }
                } else {
                    dispatcher.queue_candles_snapshot_event(
                        crate::state::CandlesSnapshotEvent::Failed {
                            #[cfg(any(test, feature = "diagnostics"))]
                            request_uid: Some(if request_uid != 0 {
                                request_uid
                            } else {
                                fallback_uid
                            }),
                            error: "candles snapshot was not applied to retained history"
                                .to_string(),
                        },
                    );
                    changed = true;
                }
            }
            Err(mpsc::TryRecvError::Disconnected) => {
                let uid = pending.auto_candles.swap_remove(i).uid;
                client.pending_api.pending_candles.remove(&uid);
                pending.auto_candles_requested = false;
                dispatcher.queue_candles_snapshot_event(
                    crate::state::CandlesSnapshotEvent::Failed {
                        #[cfg(any(test, feature = "diagnostics"))]
                        request_uid: Some(uid),
                        error: "pending full candles receiver closed before response".to_string(),
                    },
                );
                changed = true;
            }
            Err(mpsc::TryRecvError::Empty) => {
                if pending.auto_candles[i].deadline <= now {
                    let uid = pending.auto_candles.swap_remove(i).uid;
                    client.pending_api.pending_candles.remove(&uid);
                    pending.auto_candles_requested = false;
                    dispatcher.queue_candles_snapshot_event(
                        crate::state::CandlesSnapshotEvent::Failed {
                            #[cfg(any(test, feature = "diagnostics"))]
                            request_uid: Some(uid),
                            error: "pending full candles request timed out".to_string(),
                        },
                    );
                    changed = true;
                } else {
                    i += 1;
                }
            }
        }
    }

    let mut i = 0;
    let now = Instant::now();
    while i < pending.auto_candles_apply.len() {
        match pending.auto_candles_apply[i].rx.try_recv() {
            Ok(()) => {
                let applied = pending.auto_candles_apply.swap_remove(i);
                dispatcher.queue_candles_snapshot_event(
                    crate::state::CandlesSnapshotEvent::Ready {
                        #[cfg(any(test, feature = "diagnostics"))]
                        request_uid: applied.uid,
                        summary: applied.summary,
                    },
                );
                changed = true;
            }
            Err(mpsc::TryRecvError::Disconnected) => {
                let applied = pending.auto_candles_apply.swap_remove(i);
                pending.auto_candles_requested = false;
                dispatcher.queue_candles_snapshot_event(
                    crate::state::CandlesSnapshotEvent::Failed {
                        #[cfg(any(test, feature = "diagnostics"))]
                        request_uid: Some(applied.uid),
                        error: "market history worker barrier closed before ack".to_string(),
                    },
                );
                #[cfg(not(any(test, feature = "diagnostics")))]
                let _ = applied;
                changed = true;
            }
            Err(mpsc::TryRecvError::Empty) => {
                if pending.auto_candles_apply[i].deadline <= now {
                    let applied = pending.auto_candles_apply.swap_remove(i);
                    pending.auto_candles_requested = false;
                    dispatcher.queue_candles_snapshot_event(
                        crate::state::CandlesSnapshotEvent::Failed {
                            #[cfg(any(test, feature = "diagnostics"))]
                            request_uid: Some(applied.uid),
                            error: "market history worker barrier timed out".to_string(),
                        },
                    );
                    #[cfg(not(any(test, feature = "diagnostics")))]
                    let _ = applied;
                    changed = true;
                } else {
                    i += 1;
                }
            }
        }
    }
    changed
}

pub(super) fn poll_market_history(
    client: &mut Client,
    pending: &mut RuntimePending,
    dispatcher: &mut crate::events::EventDispatcher,
) -> bool {
    let mut changed = false;
    let now = Instant::now();
    let mut i = 0;
    while i < pending.market_history.len() {
        let item = &mut pending.market_history[i];
        extend_chunk_deadline(
            &mut item.deadline,
            &mut item.seen_progress,
            &item.progress,
            now,
        );
        match pending.market_history[i].rx.try_recv() {
            Ok(Ok(merged)) => {
                let item = pending.market_history.swap_remove(i);
                if merged.uid != item.uid {
                    dispatcher.queue_market_history_event(
                        crate::state::MarketHistoryEvent::Failed {
                            ticket: item.ticket,
                            error: "market-history parser returned a mismatched request UID"
                                .to_string(),
                        },
                    );
                    changed = true;
                    continue;
                }
                if let Some(rx) = dispatcher
                    .apply_market_history_archive_async(item.ticket.market.clone(), merged.archive)
                {
                    pending
                        .market_history_apply
                        .push(PendingMarketHistoryApply {
                            ticket: item.ticket,
                            deadline: engine_pending_deadline(),
                            rx,
                        });
                } else {
                    dispatcher.queue_market_history_event(
                        crate::state::MarketHistoryEvent::Failed {
                            ticket: item.ticket,
                            error: "retained history is not configured for this market".to_string(),
                        },
                    );
                    changed = true;
                }
            }
            Ok(Err(error)) => {
                let item = pending.market_history.swap_remove(i);
                dispatcher.queue_market_history_event(crate::state::MarketHistoryEvent::Failed {
                    ticket: item.ticket,
                    error,
                });
                changed = true;
            }
            Err(mpsc::TryRecvError::Disconnected) => {
                let item = pending.market_history.swap_remove(i);
                client.pending_api.pending_market_history.remove(&item.uid);
                dispatcher.queue_market_history_event(crate::state::MarketHistoryEvent::Failed {
                    ticket: item.ticket,
                    error: "market-history parser stopped before completion".to_string(),
                });
                changed = true;
            }
            Err(mpsc::TryRecvError::Empty) if pending.market_history[i].deadline <= now => {
                let market = pending.market_history[i].ticket.market.clone();
                let old_uid = pending.market_history[i].uid;
                client.pending_api.pending_market_history.remove(&old_uid);
                let (uid, rx, progress) =
                    client.api_request_market_history_async_registered(&market);
                let item = &mut pending.market_history[i];
                item.uid = uid;
                item.rx = rx;
                item.seen_progress = progress.generation();
                item.progress = progress;
                item.deadline = chunk_idle_deadline();
                log::warn!(
                    target: "moonproto::market_history",
                    "market-history request for {} was idle for 15s; retrying the whole archive",
                    market
                );
                i += 1;
            }
            Err(mpsc::TryRecvError::Empty) => i += 1,
        }
    }

    let now = Instant::now();
    let mut i = 0;
    while i < pending.market_history_apply.len() {
        match pending.market_history_apply[i].rx.try_recv() {
            Ok(Ok(summary)) => {
                let item = pending.market_history_apply.swap_remove(i);
                dispatcher.queue_market_history_event(crate::state::MarketHistoryEvent::Ready {
                    ticket: item.ticket,
                    summary,
                });
                changed = true;
            }
            Ok(Err(error)) => {
                let item = pending.market_history_apply.swap_remove(i);
                dispatcher.queue_market_history_event(crate::state::MarketHistoryEvent::Failed {
                    ticket: item.ticket,
                    error,
                });
                changed = true;
            }
            Err(mpsc::TryRecvError::Disconnected) => {
                let item = pending.market_history_apply.swap_remove(i);
                dispatcher.queue_market_history_event(crate::state::MarketHistoryEvent::Failed {
                    ticket: item.ticket,
                    error: "market-history worker stopped before apply acknowledgement".to_string(),
                });
                changed = true;
            }
            Err(mpsc::TryRecvError::Empty) if pending.market_history_apply[i].deadline <= now => {
                let item = pending.market_history_apply.swap_remove(i);
                dispatcher.queue_market_history_event(crate::state::MarketHistoryEvent::Failed {
                    ticket: item.ticket,
                    error: "market-history apply acknowledgement timed out".to_string(),
                });
                changed = true;
            }
            Err(mpsc::TryRecvError::Empty) => i += 1,
        }
    }
    changed
}

pub(super) fn poll_coin_card_candles(
    pending: &mut Vec<PendingCoinCardCandles>,
    dispatcher: &mut crate::events::EventDispatcher,
    api_pending: &ApiPending,
) -> bool {
    let mut changed = false;
    let mut i = 0;
    let now = Instant::now();
    while i < pending.len() {
        match pending[i].rx.try_recv() {
            Ok(resp) => {
                let ticket = pending.swap_remove(i).ticket;
                changed |=
                    dispatcher.apply_coin_card_candles_response(ticket.market, ticket.kind, resp);
            }
            Err(mpsc::TryRecvError::Disconnected) => {
                let ticket = pending.swap_remove(i).ticket;
                dispatcher.coin_card_candles_request_failed(
                    ticket.market,
                    ticket.kind,
                    ticket.request_uid,
                    "pending CoinCard candles receiver closed before response",
                );
            }
            Err(mpsc::TryRecvError::Empty) => {
                if pending[i].deadline <= now {
                    let ticket = pending.swap_remove(i).ticket;
                    remove_api_pending(api_pending, ticket.request_uid);
                    dispatcher.coin_card_candles_request_failed(
                        ticket.market,
                        ticket.kind,
                        ticket.request_uid,
                        "pending CoinCard candles request timed out",
                    );
                    changed = true;
                } else {
                    i += 1;
                }
            }
        }
    }
    changed
}

pub(super) fn poll_transfer_assets(
    pending: &mut RuntimePending,
    dispatcher: &mut crate::events::EventDispatcher,
    api_pending: &ApiPending,
) -> bool {
    let mut changed = false;
    let mut i = 0;
    let now = Instant::now();
    while i < pending.transfer_assets.len() {
        match pending.transfer_assets[i].rx.try_recv() {
            Ok(resp) => {
                let item = pending.transfer_assets.swap_remove(i);
                let success = dispatcher.apply_transfer_assets_response(item.kind, resp);
                changed |= success;
                finish_transfer_assets_batch_item(pending, dispatcher, item.batch_id, success);
            }
            Err(mpsc::TryRecvError::Disconnected) => {
                let item = pending.transfer_assets.swap_remove(i);
                dispatcher.transfer_assets_request_failed(
                    item.kind,
                    "pending transfer-assets receiver closed before response",
                );
                changed = true;
                finish_transfer_assets_batch_item(pending, dispatcher, item.batch_id, false);
            }
            Err(mpsc::TryRecvError::Empty) => {
                if pending.transfer_assets[i].deadline <= now {
                    let item = pending.transfer_assets.swap_remove(i);
                    remove_api_pending(api_pending, item.request_uid);
                    dispatcher.transfer_assets_request_failed(
                        item.kind,
                        "pending transfer-assets request timed out",
                    );
                    changed = true;
                    finish_transfer_assets_batch_item(pending, dispatcher, item.batch_id, false);
                } else {
                    i += 1;
                }
            }
        }
    }
    changed
}

pub(super) fn poll_account_refreshes(
    pending: &mut Vec<PendingAccountRefresh>,
    dispatcher: &mut crate::events::EventDispatcher,
    api_pending: &ApiPending,
) -> bool {
    let mut changed = false;
    let mut i = 0;
    let now = Instant::now();
    while i < pending.len() {
        match pending[i].rx.try_recv() {
            Ok(resp) => {
                let item = pending.swap_remove(i);
                changed |= match item.kind {
                    PendingAccountRefreshKind::HedgeMode => {
                        dispatcher.apply_hedge_mode_response(resp)
                    }
                    PendingAccountRefreshKind::ApiExpiration => {
                        dispatcher.apply_api_expiration_response(resp)
                    }
                };
            }
            Err(mpsc::TryRecvError::Disconnected) => {
                let item = pending.swap_remove(i);
                match item.kind {
                    PendingAccountRefreshKind::HedgeMode => dispatcher.hedge_mode_request_failed(
                        item.request_uid,
                        "pending hedge-mode receiver closed before response",
                    ),
                    PendingAccountRefreshKind::ApiExpiration => dispatcher
                        .api_expiration_request_failed(
                            item.request_uid,
                            "pending API-expiration receiver closed before response",
                        ),
                }
            }
            Err(mpsc::TryRecvError::Empty) => {
                if pending[i].deadline <= now {
                    let item = pending.swap_remove(i);
                    remove_api_pending(api_pending, item.request_uid);
                    match item.kind {
                        PendingAccountRefreshKind::HedgeMode => dispatcher
                            .hedge_mode_request_failed(
                                item.request_uid,
                                "pending hedge-mode request timed out",
                            ),
                        PendingAccountRefreshKind::ApiExpiration => dispatcher
                            .api_expiration_request_failed(
                                item.request_uid,
                                "pending API-expiration request timed out",
                            ),
                    }
                } else {
                    i += 1;
                }
            }
        }
    }
    changed
}

pub(super) fn finish_transfer_assets_batch_item(
    pending: &mut RuntimePending,
    dispatcher: &mut crate::events::EventDispatcher,
    batch_id: Option<u64>,
    success: bool,
) {
    let Some(batch_id) = batch_id else {
        return;
    };
    let Some(pos) = pending
        .transfer_assets_batches
        .iter()
        .position(|batch| batch.id == batch_id)
    else {
        return;
    };
    let batch = &mut pending.transfer_assets_batches[pos];
    batch.remaining = batch.remaining.saturating_sub(1);
    if success {
        batch.updated += 1;
    } else {
        batch.failed += 1;
    }
    if batch.remaining != 0 {
        return;
    }
    let batch = pending.transfer_assets_batches.swap_remove(pos);
    dispatcher.queue_events([crate::events::Event::TransferAssets(
        crate::state::TransferAssetsEvent::RefreshCompleted {
            #[cfg(any(test, feature = "diagnostics"))]
            request_id: batch.id,
            requested: batch.updated + batch.failed,
            updated: batch.updated,
            failed: batch.failed,
            revision: dispatcher.transfer_assets().revision(),
        },
    )]);
}

pub(super) fn poll_engine_actions(
    pending: &mut Vec<PendingEngineAction>,
    dispatcher: &mut crate::events::EventDispatcher,
    api_pending: &ApiPending,
) {
    let mut i = 0;
    let now = Instant::now();
    while i < pending.len() {
        match pending[i].rx.try_recv() {
            Ok(resp) => {
                let kind = pending[i].kind.clone();
                dispatcher.queue_engine_action_response(kind, resp);
                pending.swap_remove(i);
            }
            Err(mpsc::TryRecvError::Disconnected) => {
                let action = pending.swap_remove(i);
                dispatcher.queue_engine_action_disconnected(
                    action.kind,
                    action.ticket.request_uid,
                    action.ticket.method,
                    "pending Engine API action receiver closed before response",
                );
            }
            Err(mpsc::TryRecvError::Empty) => {
                if pending[i].deadline <= now {
                    let action = pending.swap_remove(i);
                    remove_api_pending(api_pending, action.ticket.request_uid);
                    dispatcher.queue_engine_action_disconnected(
                        action.kind,
                        action.ticket.request_uid,
                        action.ticket.method,
                        "pending Engine API action timed out",
                    );
                } else {
                    i += 1;
                }
            }
        }
    }
}
