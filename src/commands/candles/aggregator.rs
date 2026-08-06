/// Aggregator for the chunked candles response. Each chunk has the header
/// `ChunkIndex:u16 + ChunkTotal:u16`, followed by the data payload.
///
/// **Caller requirements:**
/// 1. `response_data` is `EngineResponse.data` **already after DEFLATE-decompression**
///    (if `is_compressed=true`, `parse_engine_response` decompressed it automatically).
/// 2. Keep a separate aggregator per `request_uid` when several requests are in
///    flight.
/// 3. The aggregator does not parse the payload, but it still enforces the
///    candles-domain aggregate size cap before copying chunks.
///
/// Used like this:
/// ```ignore
/// let mut agg = CandlesAggregator::new();
/// // For each response with emk_RequestCandlesData:
/// if let Some(merged) = agg.on_chunk(&response.data) {
///     // All chunks received — merged holds the zlib stream from StoreCandlesToZip.
///     let markets = parse_request_candles_data_response(&merged)?;
/// }
/// ```
use crate::commands::chunked_response::ChunkedResponseAggregator;

#[derive(Debug)]
pub struct CandlesAggregator(ChunkedResponseAggregator);

impl Default for CandlesAggregator {
    fn default() -> Self {
        Self(ChunkedResponseAggregator::new(
            "RequestCandlesData",
            super::MAX_REQUEST_CANDLES_CHUNKED_PAYLOAD_BYTES,
        ))
    }
}

pub(crate) use crate::commands::chunked_response::ChunkedResponseResult as CandlesChunkResult;

impl CandlesAggregator {
    pub fn new() -> Self {
        Self::default()
    }

    #[cfg(test)]
    pub(crate) fn with_max_payload_bytes(max_payload_bytes: usize) -> Self {
        Self(ChunkedResponseAggregator::new(
            "RequestCandlesData",
            max_payload_bytes,
        ))
    }

    /// Add a chunk. If all chunks are assembled, return the concatenated buffer and reset state.
    /// Wire: `ChunkIndex:u16 + ChunkTotal:u16 + chunk_payload`.
    #[cfg(any(test, feature = "diagnostics"))]
    pub fn on_chunk(&mut self, response_data: &[u8]) -> Option<Vec<u8>> {
        match self.on_chunk_result(response_data) {
            CandlesChunkResult::Complete(merged) => Some(merged),
            CandlesChunkResult::Ignored | CandlesChunkResult::Stored => None,
        }
    }

    /// Add a chunk and return the exact processing status.
    ///
    /// The caller uses `Stored`/`Complete` to extend the idle deadline only
    /// after a new chunk fills an empty slot. Duplicates and invalid headers do
    /// not keep an incomplete request alive.
    pub(crate) fn on_chunk_result(&mut self, response_data: &[u8]) -> CandlesChunkResult {
        self.0.on_chunk(response_data)
    }

    /// Reset state (on a new candles request).
    #[cfg(any(test, feature = "diagnostics"))]
    pub fn reset(&mut self) {
        self.0.reset();
    }

    #[cfg(any(test, feature = "diagnostics"))]
    pub fn progress(&self) -> (usize, usize) {
        self.0.progress()
    }
}
