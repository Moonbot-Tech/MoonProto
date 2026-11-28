//! On-demand snapshots of the core's existing chart lines. No history cache or retention policy.

use crate::{BaseCurrency, ExchangeCode, MoonTime, OrderType};
use crate::commands::chunked_response::{ChunkedResponseAggregator, ChunkedResponseResult};
use crate::commands::order_lines::OrderLinesPage;
use std::collections::HashMap;
use std::io::{self, Read};
use std::sync::Arc;
use std::time::{Duration, Instant};

/// UTC coordinate in the core's original order-line geometry.
pub type OrderLinePoint = super::ReportTracePoint;

#[derive(Debug, Clone, PartialEq)]
pub struct OrderLine {
    /// Core chart-object ID. Imported figures can share an ID; replace snapshots instead of merging by this field.
    pub line_id: u64,
    /// Live MoonProto order UID when the core has linked this line to a protocol order.
    pub order_uid: Option<u64>,
    /// Core's native order-leg identity, separate from the live protocol UID.
    pub native_order_id: i64,
    pub market: String,
    pub exchange: ExchangeCode,
    pub base_currency: BaseCurrency,
    pub order_type: OrderType,
    pub stop_price: f64,
    pub stop_time: MoonTime,
    pub points: Arc<[OrderLinePoint]>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OrderLinesTicket {
    pub request_id: u64,
    /// `None` requests all markets currently held by the core.
    pub market: Option<String>,
}

#[derive(Debug, Clone)]
pub enum OrderLinesEvent {
    /// Complete requested snapshot. An empty list is a successful empty snapshot.
    Ready { ticket: OrderLinesTicket, lines: Arc<[OrderLine]> },
    /// No partial snapshot is published if a page is missing or malformed.
    Failed { ticket: OrderLinesTicket, error: String },
}

struct PendingLines {
    ticket: OrderLinesTicket,
    deadline: Instant,
    chunks: ChunkedResponseAggregator,
}

#[derive(Default)]
pub(crate) struct OrderLineRequests {
    pending: HashMap<u64, PendingLines>,
}

fn timeout() -> Duration {
    Duration::from_millis(crate::api_pending::DEFAULT_PENDING_TIMEOUT_MS as u64)
}

impl OrderLineRequests {
    pub(crate) fn begin(&mut self, ticket: OrderLinesTicket, now: Instant) {
        self.pending.insert(ticket.request_id, PendingLines {
            ticket, deadline: now + timeout(), chunks: ChunkedResponseAggregator::new("OrderLines"),
        });
    }

    pub(crate) fn fail(&mut self, uid: u64, error: &str) -> Option<crate::Event> {
        self.pending.remove(&uid).map(|pending| crate::Event::OrderLines(OrderLinesEvent::Failed {
            ticket: pending.ticket, error: error.to_owned(),
        }))
    }

    pub(crate) fn on_page(&mut self, page: OrderLinesPage, now: Instant, out: &mut Vec<crate::Event>) {
        let uid = page.header.uid;
        let Some(pending) = self.pending.get_mut(&uid) else { return };
        if page.data[2..4] == [0, 0] {
            let error = String::from_utf8_lossy(&page.data[4..]);
            out.extend(self.fail(uid, &error));
            return;
        }
        match pending.chunks.on_chunk(page.data.into()) {
            ChunkedResponseResult::Ignored => {},
            ChunkedResponseResult::Stored => pending.deadline = now + timeout(),
            ChunkedResponseResult::Complete(payload) => {
                let pending = self.pending.remove(&uid).unwrap();
                let event = match parse_snapshot(payload.reader(), payload.len()) {
                    Ok(lines) => OrderLinesEvent::Ready { ticket: pending.ticket, lines },
                    Err(_) => OrderLinesEvent::Failed { ticket: pending.ticket, error: "invalid order-line snapshot".into() },
                };
                out.push(crate::Event::OrderLines(event));
            }
        }
    }

    pub(crate) fn tick(&mut self, now: Instant, mut emit: impl FnMut(crate::Event)) {
        self.pending.retain(|_, pending| {
            if now < pending.deadline { return true; }
            emit(crate::Event::OrderLines(OrderLinesEvent::Failed {
                ticket: pending.ticket.clone(), error: "order-line snapshot timed out".into(),
            }));
            false
        });
    }
}

fn bytes<const N: usize>(r: &mut impl Read) -> io::Result<[u8; N]> {
    let mut value = [0; N];
    r.read_exact(&mut value)?;
    Ok(value)
}

fn invalid() -> io::Error { io::ErrorKind::InvalidData.into() }

fn parse_snapshot(reader: impl Read, size: usize) -> io::Result<Arc<[OrderLine]>> {
    let mut r = reader.take(size as u64);
    if bytes::<1>(&mut r)?[0] != 1 { return Err(invalid()); }
    let exchange = ExchangeCode::from_byte(bytes::<1>(&mut r)?[0]);
    let base_currency = BaseCurrency::from_byte(bytes::<1>(&mut r)?[0]);
    let count = u32::from_le_bytes(bytes(&mut r)?) as usize;
    if count as u64 > r.limit() / 46 { return Err(invalid()); }
    let mut lines = Vec::with_capacity(count);
    for _ in 0..count {
        let len = bytes::<1>(&mut r)?[0] as usize;
        let mut name = vec![0; len];
        r.read_exact(&mut name)?;
        let market = String::from_utf8(name).map_err(|_| invalid())?;
        let line_id = u64::from_le_bytes(bytes(&mut r)?);
        let uid = u64::from_le_bytes(bytes(&mut r)?);
        let native_order_id = i64::from_le_bytes(bytes(&mut r)?);
        let order_type = OrderType::from_byte(bytes::<1>(&mut r)?[0]);
        let stop_price = f64::from_le_bytes(bytes(&mut r)?);
        let stop_time = MoonTime::from_unix_millis(i64::from_le_bytes(bytes(&mut r)?));
        let point_count = u32::from_le_bytes(bytes(&mut r)?) as usize;
        if point_count as u64 > r.limit() / 16 { return Err(invalid()); }
        let mut points = Vec::with_capacity(point_count);
        for _ in 0..point_count {
            let raw = bytes::<16>(&mut r)?;
            points.push(OrderLinePoint {
                time: MoonTime::from_unix_millis(i64::from_le_bytes(raw[..8].try_into().unwrap())),
                price: f64::from_le_bytes(raw[8..].try_into().unwrap()),
            });
        }
        lines.push(OrderLine { line_id, order_uid: (uid != 0).then_some(uid), native_order_id,
            market, exchange, base_currency, order_type, stop_price, stop_time, points: points.into() });
    }
    if r.limit() != 0 { return Err(invalid()); }
    Ok(lines.into())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands::trade::TradeCommand;

    fn ticket(uid: u64, market: Option<&str>) -> OrderLinesTicket {
        OrderLinesTicket { request_id: uid, market: market.map(str::to_owned) }
    }

    fn page(uid: u64, index: usize, total: usize, data: &[u8]) -> OrderLinesPage {
        let mut wire = vec![58, 4, 0];
        wire.extend_from_slice(&uid.to_le_bytes());
        wire.extend_from_slice(&(index as u16).to_le_bytes());
        wire.extend_from_slice(&(total as u16).to_le_bytes());
        wire.extend_from_slice(data);
        let Some(TradeCommand::OrderLinesPage(page)) = TradeCommand::parse(&wire) else { panic!("page parse"); };
        page
    }

    #[test]
    fn core_geometry_survives_reordered_duplicate_pages_and_preserves_identity() {
        let raw = include_bytes!("../../tests/fixtures/order-lines-snapshot.bin");
        let parts: Vec<_> = raw.chunks(13).collect();
        let now = Instant::now();
        let mut requests = OrderLineRequests::default();
        requests.begin(ticket(11, Some("BTCUSDT")), now);
        let mut out = Vec::new();
        for index in (0..parts.len()).rev() {
            requests.on_page(page(11, index, parts.len(), parts[index]), now, &mut out);
            requests.on_page(page(11, index, parts.len(), parts[index]), now, &mut out);
        }
        let [crate::Event::OrderLines(OrderLinesEvent::Ready { ticket: received, lines })] = out.as_slice()
            else { panic!("{out:?}"); };
        assert_eq!(received, &ticket(11, Some("BTCUSDT")));
        assert_eq!(lines.len(), 1);
        let line = &lines[0];
        assert_eq!((line.line_id, line.order_uid, line.native_order_id), (100, Some(200), 300));
        assert_eq!(line.market, "BTCUSDT");
        assert_eq!(line.points.len(), 7);
        assert_eq!(line.points[0].time.unix_millis(), 1_800_000_000_123);
        assert_eq!(line.points[0].price, 123.456789012345);
        assert_eq!(line.points[6].price, 125.25);
        assert!(requests.pending.is_empty());
    }

    #[test]
    fn one_and_all_requests_are_independent_and_empty_is_successful() {
        let now = Instant::now();
        let mut requests = OrderLineRequests::default();
        requests.begin(ticket(1, None), now);
        requests.begin(ticket(2, Some("BTCUSDT")), now);
        let mut out = Vec::new();
        requests.on_page(page(2, 0, 1, include_bytes!("../../tests/fixtures/order-lines-snapshot.bin")), now, &mut out);
        requests.on_page(page(1, 0, 1, &[1, 4, 1, 0, 0, 0, 0]), now, &mut out);
        assert!(matches!(&out[0], crate::Event::OrderLines(OrderLinesEvent::Ready { ticket, lines })
            if ticket.request_id == 2 && lines.len() == 1));
        assert!(matches!(&out[1], crate::Event::OrderLines(OrderLinesEvent::Ready { ticket, lines })
            if ticket.market.is_none() && lines.is_empty()));
    }

    #[test]
    fn imported_lines_with_the_same_id_keep_both_geometries() {
        let fixture = include_bytes!("../../tests/fixtures/order-lines-snapshot.bin");
        let mut raw = fixture.to_vec();
        raw[3..7].copy_from_slice(&2u32.to_le_bytes());
        raw.extend_from_slice(&fixture[7..]);
        let end = raw.len();
        raw[end - 8..].copy_from_slice(&126.0f64.to_le_bytes());
        let lines = parse_snapshot(raw.as_slice(), raw.len()).unwrap();
        assert_eq!(lines.len(), 2);
        assert_eq!(lines[0].line_id, lines[1].line_id);
        assert_eq!(lines[0].points.last().unwrap().price, 125.25);
        assert_eq!(lines[1].points.last().unwrap().price, 126.0);
    }

    #[test]
    fn missing_page_times_out_and_duplicates_do_not_extend_the_deadline() {
        let now = Instant::now();
        let mut requests = OrderLineRequests::default();
        requests.begin(ticket(1, None), now);
        let mut out = Vec::new();
        requests.on_page(page(1, 1, 2, &[0, 0]), now, &mut out);
        requests.on_page(page(1, 1, 2, &[0, 0]), now + timeout() - Duration::from_millis(1), &mut out);
        assert!(out.is_empty());
        requests.tick(now + timeout(), |event| out.push(event));
        requests.on_page(page(1, 0, 2, &[1, 4, 1, 0, 0]), now + timeout(), &mut out);
        assert_eq!(out.len(), 1);
        assert!(matches!(out[0], crate::Event::OrderLines(OrderLinesEvent::Failed { .. })));
    }

    #[test]
    fn malformed_snapshot_or_server_error_never_publishes_partial_lines() {
        let raw = include_bytes!("../../tests/fixtures/order-lines-snapshot.bin");
        for len in 0..raw.len() { assert!(parse_snapshot(&raw[..len], len).is_err(), "length={len}"); }
        let mut requests = OrderLineRequests::default();
        let now = Instant::now();
        requests.begin(ticket(1, None), now);
        let mut out = Vec::new();
        requests.on_page(page(1, 0, 0, b"core error"), now, &mut out);
        assert!(matches!(&out[0], crate::Event::OrderLines(OrderLinesEvent::Failed { error, .. }) if error == "core error"));
    }
}
