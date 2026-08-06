use crate::commands::candles::{CandlesAggregator, RequestCandlesMarket};
use crate::commands::chunked_response::ChunkedResponseAggregator;
use crate::commands::engine_api::EngineMethod;
use crate::commands::market_history::{
    parse_market_history_archive, MarketHistoryArchive, MAX_MARKET_HISTORY_CHUNKED_BYTES,
};
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{mpsc, Arc};
use std::thread;
// =============================================================================
//  Full candles snapshot collector
// =============================================================================

/// Parsed result returned by the internal full-candles snapshot collector.
///
/// The server answers `RequestCandlesData` with several `EngineResponse` chunks
/// sharing one `request_uid`. The library aggregates those chunks through
/// [`CandlesAggregator`] and hands parsed market entries to Active Lib.
#[derive(Debug, Clone)]
pub(crate) struct MergedCandles {
    /// Request UID used by diagnostics to correlate the chunked response.
    #[cfg(any(test, feature = "diagnostics"))]
    pub uid: u64,
    /// Parsed market entries from the zipped stream.
    pub markets: Vec<RequestCandlesMarket>,
}

/// Internal state for a partially assembled full-candles snapshot.
pub(crate) struct PartialCandles {
    pub(crate) aggregator: CandlesAggregator,
    pub(crate) progress: ChunkProgress,
    /// Completion sender used by the persistent candles parse worker after the
    /// aggregator returns the merged zipped stream.
    pub(crate) sender: mpsc::Sender<MergedCandles>,
}

pub(crate) struct PartialMarketHistory {
    pub(crate) aggregator: ChunkedResponseAggregator,
    pub(crate) progress: ChunkProgress,
    pub(crate) sender: mpsc::Sender<Result<MergedMarketHistory, String>>,
}

impl PartialMarketHistory {
    pub(crate) fn new(sender: mpsc::Sender<Result<MergedMarketHistory, String>>) -> Self {
        Self {
            aggregator: ChunkedResponseAggregator::new(
                "RequestMarketHistory",
                MAX_MARKET_HISTORY_CHUNKED_BYTES,
            ),
            progress: ChunkProgress::new(),
            sender,
        }
    }
}

#[derive(Debug, Clone)]
pub(crate) struct MergedMarketHistory {
    pub(crate) uid: u64,
    pub(crate) archive: MarketHistoryArchive,
}

#[derive(Debug, Clone)]
pub(crate) struct ChunkProgress(Arc<AtomicU64>);

impl ChunkProgress {
    pub(crate) fn new() -> Self {
        Self(Arc::new(AtomicU64::new(0)))
    }

    pub(crate) fn mark_stored(&self) {
        self.0.fetch_add(1, Ordering::Relaxed);
    }

    pub(crate) fn generation(&self) -> u64 {
        self.0.load(Ordering::Relaxed)
    }
}

pub(crate) struct ChunkedApiParseQueue {
    tx: Option<mpsc::Sender<ChunkedApiParseJob>>,
}

enum ChunkedApiParseJob {
    Candles {
        uid: u64,
        compressed: Vec<u8>,
        sender: mpsc::Sender<MergedCandles>,
    },
    MarketHistory {
        uid: u64,
        compressed: Vec<u8>,
        sender: mpsc::Sender<Result<MergedMarketHistory, String>>,
    },
}

impl ChunkedApiParseQueue {
    pub(crate) fn new() -> Self {
        let (tx, rx) = mpsc::channel::<ChunkedApiParseJob>();
        match thread::Builder::new()
            .name("moonproto-api-parse".to_string())
            .spawn(move || {
                while let Ok(job) = rx.recv() {
                    if let Err(payload) = catch_unwind(AssertUnwindSafe(|| process_parse_job(job)))
                    {
                        log::error!(
                            target: "moonproto::client",
                            "moonproto-api-parse panicked: {}",
                            panic_payload_message(payload.as_ref())
                        );
                    }
                }
            }) {
            Ok(_) => Self { tx: Some(tx) },
            Err(err) => {
                log::warn!(target: "moonproto::client",
                    "failed to spawn persistent API parse worker: {err}; completed chunked responses will parse inline");
                Self { tx: None }
            }
        }
    }

    pub(crate) fn submit_candles(
        &self,
        uid: u64,
        compressed: Vec<u8>,
        sender: mpsc::Sender<MergedCandles>,
    ) {
        self.submit(ChunkedApiParseJob::Candles {
            uid,
            compressed,
            sender,
        });
    }

    pub(crate) fn submit_market_history(
        &self,
        uid: u64,
        compressed: Vec<u8>,
        sender: mpsc::Sender<Result<MergedMarketHistory, String>>,
    ) {
        self.submit(ChunkedApiParseJob::MarketHistory {
            uid,
            compressed,
            sender,
        });
    }

    fn submit(&self, job: ChunkedApiParseJob) {
        if let Some(tx) = &self.tx {
            match tx.send(job) {
                Ok(()) => return,
                Err(err) => {
                    let job = err.0;
                    log::warn!(target: "moonproto::client",
                        "persistent API parse worker stopped; parsing chunked response inline");
                    process_parse_job(job);
                    return;
                }
            }
        }
        process_parse_job(job);
    }
}

fn process_parse_job(job: ChunkedApiParseJob) {
    match job {
        ChunkedApiParseJob::Candles {
            uid,
            compressed,
            sender,
        } => parse_and_send_candles(uid, compressed, sender),
        ChunkedApiParseJob::MarketHistory {
            uid,
            compressed,
            sender,
        } => {
            let parsed = parse_market_history_archive(&compressed)
                .map(|archive| MergedMarketHistory { uid, archive });
            let _ = sender.send(parsed);
        }
    }
}

fn panic_payload_message(payload: &(dyn std::any::Any + Send)) -> String {
    if let Some(value) = payload.downcast_ref::<&'static str>() {
        (*value).to_string()
    } else if let Some(value) = payload.downcast_ref::<String>() {
        value.clone()
    } else {
        "non-string panic payload".to_string()
    }
}

fn parse_and_send_candles(uid: u64, zipped_data: Vec<u8>, sender: mpsc::Sender<MergedCandles>) {
    let markets = crate::commands::candles::parse_request_candles_data_response(&zipped_data)
        .unwrap_or_else(|| {
            log::warn!(target: "moonproto::client",
                "candles aggregator merged but strict parse failed for uid={} ({} bytes); trying Delphi partial apply",
                uid,
                zipped_data.len()
            );
            crate::commands::candles::parse_request_candles_data_response_partial(&zipped_data)
                .unwrap_or_default()
        });
    let _ = sender.send(MergedCandles {
        #[cfg(any(test, feature = "diagnostics"))]
        uid,
        markets,
    });
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct EngineResponseMeta {
    pub(crate) request_uid: u64,
    pub(crate) method: EngineMethod,
    pub(crate) success: bool,
}
