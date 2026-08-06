//! Shared collector for chunked Engine API responses.

/// Result of accepting one `[chunk_index:u16][chunk_total:u16][payload]`
/// fragment.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum ChunkedResponseResult {
    Ignored,
    Stored,
    Complete(Vec<u8>),
}

#[derive(Debug)]
pub(crate) struct ChunkedResponseAggregator {
    chunks: Vec<Option<Vec<u8>>>,
    received: usize,
    total: usize,
    payload_bytes: usize,
    max_payload_bytes: usize,
    operation: &'static str,
}

impl ChunkedResponseAggregator {
    pub(crate) fn new(operation: &'static str, max_payload_bytes: usize) -> Self {
        Self {
            chunks: Vec::new(),
            received: 0,
            total: 0,
            payload_bytes: 0,
            max_payload_bytes,
            operation,
        }
    }

    pub(crate) fn on_chunk(&mut self, response_data: &[u8]) -> ChunkedResponseResult {
        if response_data.len() < 4 {
            return ChunkedResponseResult::Ignored;
        }
        let chunk_index = u16::from_le_bytes([response_data[0], response_data[1]]) as usize;
        let chunk_total = u16::from_le_bytes([response_data[2], response_data[3]]) as usize;
        let payload = &response_data[4..];
        if chunk_total == 0 {
            return ChunkedResponseResult::Ignored;
        }

        if self.total != chunk_total {
            let mut chunks = Vec::new();
            if chunks.try_reserve_exact(chunk_total).is_err() {
                log::warn!(
                    target: "moonproto::chunked_response",
                    "{} chunk table for {} chunks cannot be allocated",
                    self.operation,
                    chunk_total
                );
                self.reset();
                return ChunkedResponseResult::Ignored;
            }
            chunks.resize_with(chunk_total, || None);
            self.chunks = chunks;
            self.received = 0;
            self.total = chunk_total;
            self.payload_bytes = 0;
        }

        if chunk_index >= chunk_total || self.chunks[chunk_index].is_some() {
            return ChunkedResponseResult::Ignored;
        }
        let Some(new_payload_bytes) = self.payload_bytes.checked_add(payload.len()) else {
            log::warn!(
                target: "moonproto::chunked_response",
                "{} chunk payload size overflow",
                self.operation
            );
            self.reset();
            return ChunkedResponseResult::Ignored;
        };
        if new_payload_bytes > self.max_payload_bytes {
            log::warn!(
                target: "moonproto::chunked_response",
                "{} chunk payload {} exceeds cap {}",
                self.operation,
                new_payload_bytes,
                self.max_payload_bytes
            );
            self.reset();
            return ChunkedResponseResult::Ignored;
        }
        let mut owned = Vec::new();
        if owned.try_reserve_exact(payload.len()).is_err() {
            log::warn!(
                target: "moonproto::chunked_response",
                "{} chunk payload {} cannot be allocated",
                self.operation,
                payload.len()
            );
            self.reset();
            return ChunkedResponseResult::Ignored;
        }
        owned.extend_from_slice(payload);
        self.chunks[chunk_index] = Some(owned);
        self.payload_bytes = new_payload_bytes;
        self.received += 1;

        if self.received != self.total {
            return ChunkedResponseResult::Stored;
        }

        let mut merged = Vec::new();
        if merged.try_reserve_exact(self.payload_bytes).is_err() {
            log::warn!(
                target: "moonproto::chunked_response",
                "{} merged payload {} cannot be allocated",
                self.operation,
                self.payload_bytes
            );
            self.reset();
            return ChunkedResponseResult::Ignored;
        }
        for chunk in self.chunks.drain(..).flatten() {
            merged.extend_from_slice(&chunk);
        }
        self.reset_counters();
        ChunkedResponseResult::Complete(merged)
    }

    pub(crate) fn reset(&mut self) {
        self.chunks.clear();
        self.reset_counters();
    }

    #[cfg(any(test, feature = "diagnostics"))]
    pub(crate) fn progress(&self) -> (usize, usize) {
        (self.received, self.total)
    }

    fn reset_counters(&mut self) {
        self.received = 0;
        self.total = 0;
        self.payload_bytes = 0;
    }
}
