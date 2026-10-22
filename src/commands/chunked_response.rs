//! Shared collector for chunked Engine API responses.

use crate::protocol::payload::Payload;
use std::io::{self, BufRead, Read};

/// Result of accepting one `[chunk_index:u16][chunk_total:u16][payload]`
/// fragment.
#[derive(Debug)]
#[cfg_attr(test, derive(PartialEq))]
pub(crate) enum ChunkedResponseResult {
    Ignored,
    Stored,
    Complete(ChunkedPayload),
}

#[derive(Debug)]
pub(crate) struct ChunkedResponseAggregator {
    chunks: Vec<Option<Payload>>,
    received: usize,
    total: usize,
    payload_bytes: usize,
    operation: &'static str,
}

impl ChunkedResponseAggregator {
    pub(crate) fn new(operation: &'static str) -> Self {
        Self {
            chunks: Vec::new(),
            received: 0,
            total: 0,
            payload_bytes: 0,
            operation,
        }
    }

    pub(crate) fn on_chunk(&mut self, mut response_data: Payload) -> ChunkedResponseResult {
        if response_data.len() < 4 {
            return ChunkedResponseResult::Ignored;
        }
        let chunk_index = u16::from_le_bytes([response_data[0], response_data[1]]) as usize;
        let chunk_total = u16::from_le_bytes([response_data[2], response_data[3]]) as usize;
        let payload_len = response_data.len() - 4;
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
        let Some(new_payload_bytes) = self.payload_bytes.checked_add(payload_len) else {
            log::warn!(
                target: "moonproto::chunked_response",
                "{} chunk payload size overflow",
                self.operation
            );
            self.reset();
            return ChunkedResponseResult::Ignored;
        };
        response_data.restrict(4..response_data.len());
        self.chunks[chunk_index] = Some(response_data);
        self.payload_bytes = new_payload_bytes;
        self.received += 1;

        if self.received != self.total {
            return ChunkedResponseResult::Stored;
        }

        let merged = ChunkedPayload {
            chunks: std::mem::take(&mut self.chunks),
            len: self.payload_bytes,
        };
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

/// Complete archive, kept in chunk-index order without concatenating its bytes.
#[derive(Debug)]
#[cfg_attr(test, derive(PartialEq))]
pub(crate) struct ChunkedPayload {
    chunks: Vec<Option<Payload>>,
    len: usize,
}

impl ChunkedPayload {
    pub(crate) fn len(&self) -> usize {
        self.len
    }

    pub(crate) fn reader(&self) -> impl BufRead + '_ {
        ChunkReader {
            chunks: self.chunks.iter(),
            current: &[],
        }
    }

    #[cfg(any(test, feature = "diagnostics"))]
    pub(crate) fn into_vec(self) -> Vec<u8> {
        let mut bytes = Vec::with_capacity(self.len);
        for chunk in self.chunks.into_iter().flatten() {
            bytes.extend_from_slice(&chunk);
        }
        bytes
    }
}

struct ChunkReader<'a> {
    chunks: std::slice::Iter<'a, Option<Payload>>,
    current: &'a [u8],
}

impl BufRead for ChunkReader<'_> {
    fn fill_buf(&mut self) -> io::Result<&[u8]> {
        while self.current.is_empty() {
            let Some(chunk) = self.chunks.next() else { break };
            self.current = chunk.as_deref().unwrap_or_default();
        }
        Ok(self.current)
    }

    fn consume(&mut self, amount: usize) {
        self.current = &self.current[amount..];
    }
}

impl Read for ChunkReader<'_> {
    fn read(&mut self, output: &mut [u8]) -> io::Result<usize> {
        let input = self.fill_buf()?;
        let len = input.len().min(output.len());
        output[..len].copy_from_slice(&input[..len]);
        self.consume(len);
        Ok(len)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn part(index: u16, total: u16, data: &[u8]) -> Payload {
        let mut bytes = Vec::new();
        bytes.extend_from_slice(&index.to_le_bytes());
        bytes.extend_from_slice(&total.to_le_bytes());
        bytes.extend_from_slice(data);
        bytes.into()
    }

    #[test]
    fn ordered_reader_retains_first_buffers_and_can_restart() {
        let mut collector = ChunkedResponseAggregator::new("test");
        let last = part(2, 3, b"last");
        let last_ptr = last.as_ptr().wrapping_add(4);
        assert_eq!(collector.on_chunk(last), ChunkedResponseResult::Stored);
        assert_eq!(collector.on_chunk(part(2, 3, b"wrong")), ChunkedResponseResult::Ignored);
        assert_eq!(collector.on_chunk(part(1, 3, b"")), ChunkedResponseResult::Stored);
        let first = part(0, 3, b"first");
        let first_ptr = first.as_ptr().wrapping_add(4);
        let ChunkedResponseResult::Complete(complete) = collector.on_chunk(first) else {
            panic!("complete")
        };
        assert_eq!(collector.progress(), (0, 0));
        let mut reader = complete.reader();
        assert_eq!(reader.fill_buf().unwrap().as_ptr(), first_ptr);
        reader.consume(5);
        assert_eq!(reader.fill_buf().unwrap().as_ptr(), last_ptr);
        reader.consume(4);
        assert!(reader.fill_buf().unwrap().is_empty());
        let mut bytes = Vec::new();
        complete.reader().read_to_end(&mut bytes).unwrap();
        assert_eq!(bytes, b"firstlast");
    }

    #[test]
    fn changed_total_resets_slots_and_invalid_or_duplicate_chunks_do_not_progress() {
        let mut collector = ChunkedResponseAggregator::new("test");
        assert_eq!(collector.on_chunk(part(0, 2, b"old")), ChunkedResponseResult::Stored);
        assert_eq!(
            collector.on_chunk(part(8, 3, b"bad index")),
            ChunkedResponseResult::Ignored
        );
        assert_eq!(collector.progress(), (0, 3));
        assert_eq!(collector.on_chunk(vec![0, 0, 0].into()), ChunkedResponseResult::Ignored);
        assert_eq!(
            collector.on_chunk(part(0, 0, b"zero total")),
            ChunkedResponseResult::Ignored
        );
        assert_eq!(collector.progress(), (0, 3));
        for index in [2, 0] {
            assert_eq!(
                collector.on_chunk(part(index, 3, &[index as u8])),
                ChunkedResponseResult::Stored
            );
        }
        let ChunkedResponseResult::Complete(complete) = collector.on_chunk(part(1, 3, &[1])) else {
            panic!("complete")
        };
        assert_eq!(complete.into_vec(), [0, 1, 2]);
        let ChunkedResponseResult::Complete(empty) = collector.on_chunk(part(0, 1, &[])) else {
            panic!("empty completion")
        };
        assert!(empty.reader().fill_buf().unwrap().is_empty());
    }
}
