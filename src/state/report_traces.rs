//! On-demand historical order geometry. The application owns persistent storage.

use super::ReportEvent;
use crate::commands::strict_read::{read_f64, read_i64, read_u32, read_u8};
use crate::commands::trade::OrderType;
use crate::MoonTime;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Instant;

/// One on-demand trace request, correlated with `TraceReady` or `TraceFailed`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ReportTraceTicket {
    pub request_id: u64,
    /// Report-row identity, not `newRecID` or a live order ID. Negative values are valid.
    pub report_uid: i64,
}

/// Archived chart coordinate. Prices retain the core's full precision.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ReportTracePoint {
    /// Unix UTC milliseconds; no core-timezone or ping-offset correction is needed.
    pub time: MoonTime,
    pub price: f64,
}

/// One own or inherited buy/sell trace from a closed trade's archive.
#[derive(Debug, Clone, PartialEq)]
pub struct ReportTrace {
    /// False for geometry inherited from another order through join/split.
    pub own: bool,
    pub order_type: OrderType,
    /// Zero means no stop marker.
    pub stop_price: f64,
    /// Unix UTC milliseconds; zero means no stop time.
    pub stop_time: MoonTime,
    /// Original chart points: one anchor followed by groups of three per segment.
    /// See `docs/reports.md` for the core's rendering convention.
    pub points: Arc<[ReportTracePoint]>,
}

fn parse_traces(data: &[u8]) -> Option<Arc<[ReportTrace]>> {
    if data.is_empty() {
        return Some(Arc::from([]));
    }
    let mut pos = 0;
    if read_u8(data, &mut pos)? != 1 {
        return None;
    }
    let mut traces = Vec::new();
    while pos < data.len() {
        let own = read_u8(data, &mut pos)? & 1 != 0;
        let order_type = OrderType::from_byte(read_u8(data, &mut pos)?);
        let stop_price = read_f64(data, &mut pos)?;
        let stop_time = MoonTime::from_unix_millis(read_i64(data, &mut pos)?);
        let count = usize::try_from(read_u32(data, &mut pos)?).ok()?;
        if count > (data.len() - pos) / 16 {
            return None;
        }
        let mut points = Vec::new();
        points.try_reserve_exact(count).ok()?;
        for _ in 0..count {
            points.push(ReportTracePoint {
                time: MoonTime::from_unix_millis(read_i64(data, &mut pos)?),
                price: read_f64(data, &mut pos)?,
            });
        }
        traces.push(ReportTrace { own, order_type, stop_price, stop_time, points: points.into() });
    }
    Some(traces.into())
}

struct PendingTrace {
    tickets: Vec<ReportTraceTicket>,
    deadline: Instant,
}

#[derive(Default)]
pub(crate) struct ReportTraceRequests {
    pending: HashMap<u64, PendingTrace>,
}

impl ReportTraceRequests {
    /// The core replaces queued requests for the same ReportUID. Share their answer locally.
    pub(crate) fn begin(&mut self, ticket: ReportTraceTicket, deadline: Instant) -> bool {
        if let Some(pending) = self.pending.values_mut().find(|p| p.tickets[0].report_uid == ticket.report_uid) {
            pending.tickets.push(ticket);
            return false;
        }
        self.pending.insert(ticket.request_id, PendingTrace { tickets: vec![ticket], deadline });
        true
    }

    pub(crate) fn complete(
        &mut self,
        request_uid: u64,
        result: Result<&[u8], &str>,
        out: &mut Vec<ReportEvent>,
    ) {
        let Some(pending) = self.pending.remove(&request_uid) else { return };
        let result = result.and_then(|blob| parse_traces(blob).ok_or("invalid or unsupported report trace archive"));
        for ticket in pending.tickets {
            out.push(match &result {
                Ok(traces) => ReportEvent::TraceReady { ticket, traces: Arc::clone(traces) },
                Err(error) => ReportEvent::TraceFailed { ticket, error: (*error).to_owned() },
            });
        }
    }

    pub(crate) fn tick(&mut self, now: Instant, out: &mut Vec<ReportEvent>) {
        self.pending.retain(|_, pending| {
            if now < pending.deadline {
                return true;
            }
            for &ticket in &pending.tickets {
                out.push(ReportEvent::TraceFailed { ticket, error: "report trace request timed out".to_owned() });
            }
            false
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    fn line(own: bool, order_type: OrderType, points: &[(i64, f64)]) -> Vec<u8> {
        let mut raw = vec![u8::from(own), order_type.to_byte()];
        raw.extend_from_slice(&0.123456789012345_f64.to_le_bytes());
        raw.extend_from_slice(&1_800_000_000_123_i64.to_le_bytes());
        raw.extend_from_slice(&(points.len() as u32).to_le_bytes());
        for (time, price) in points {
            raw.extend_from_slice(&time.to_le_bytes());
            raw.extend_from_slice(&price.to_le_bytes());
        }
        raw
    }

    fn ticket(request_id: u64, report_uid: i64) -> ReportTraceTicket {
        ReportTraceTicket { request_id, report_uid }
    }

    #[test]
    fn archive_preserves_own_inherited_geometry_precision_and_utc_time() {
        let points = [(1_800_000_000_123, 0.000000123456789012345), (0, 0.0), (1_799_999_999_987, 9.9)];
        let mut raw = vec![1];
        raw.extend(line(true, OrderType::Buy, &points));
        raw.extend(line(true, OrderType::Sell, &points[..1]));
        raw.extend(line(false, OrderType::BuyStop, &[]));
        raw.extend(line(false, OrderType::from_byte(255), &points));
        let traces = parse_traces(&raw).unwrap();
        assert_eq!(traces.len(), 4);
        assert!(traces[0].own && traces[1].own);
        assert!(!traces[2].own && !traces[3].own);
        assert_eq!(traces[0].order_type, OrderType::Buy);
        assert_eq!(traces[1].order_type, OrderType::Sell);
        assert_eq!(traces[2].order_type, OrderType::BuyStop);
        assert_eq!(traces[3].order_type.to_byte(), 255);
        assert!(traces[2].points.is_empty());
        assert_eq!(traces[0].stop_time.unix_millis(), 1_800_000_000_123);
        assert_eq!(traces[0].stop_price.to_bits(), 0.123456789012345_f64.to_bits());
        for (actual, (time, price)) in traces[0].points.iter().zip(points) {
            assert_eq!(actual.time.unix_millis(), time);
            assert_eq!(actual.price.to_bits(), price.to_bits());
        }
    }

    #[test]
    fn archive_accepts_large_lines_without_a_client_side_point_cap() {
        let points = vec![(1_800_000_000_123, 0.000012345678901234); 20_000];
        let mut raw = vec![1];
        raw.extend(line(true, OrderType::Buy, &points));
        let traces = parse_traces(&raw).unwrap();
        assert_eq!(traces[0].points.len(), points.len());
        let last = traces[0].points.last().unwrap();
        assert_eq!(last.time.unix_millis(), points[19_999].0);
        assert_eq!(last.price, points[19_999].1);
    }

    #[test]
    fn archive_rejects_partial_records_bad_counts_and_unknown_versions() {
        let mut raw = vec![1];
        raw.extend(line(true, OrderType::Sell, &[(123, 4.5)]));
        for end in 2..raw.len() {
            assert!(parse_traces(&raw[..end]).is_none(), "accepted prefix {end}");
        }
        raw[19..23].copy_from_slice(&u32::MAX.to_le_bytes());
        assert!(parse_traces(&raw).is_none());
        assert!(parse_traces(&[2]).is_none());
        assert!(parse_traces(&[]).unwrap().is_empty());
        assert!(parse_traces(&[1]).unwrap().is_empty());
    }

    #[test]
    fn requests_match_out_of_order_and_share_duplicate_report_answers() {
        let mut state = ReportTraceRequests::default();
        let deadline = Instant::now() + Duration::from_secs(12);
        let a = ticket(10, i64::MIN);
        let b = ticket(20, 22);
        let repeated_a = ticket(30, a.report_uid);
        assert!(state.begin(a, deadline));
        assert!(state.begin(b, deadline));
        assert!(!state.begin(repeated_a, deadline));
        let mut out = Vec::new();
        state.complete(99, Ok(&[2]), &mut out);
        assert!(out.is_empty());
        state.complete(b.request_id, Ok(&[]), &mut out);
        assert!(matches!(&out[0], ReportEvent::TraceReady { ticket, traces } if *ticket == b && traces.is_empty()));
        out.clear();
        let mut raw = vec![1];
        raw.extend(line(true, OrderType::Buy, &[(123, 4.5)]));
        state.complete(a.request_id, Ok(&raw), &mut out);
        let [ReportEvent::TraceReady { ticket: ta, traces: first },
             ReportEvent::TraceReady { ticket: tr, traces: second }] = out.as_slice() else { panic!("{out:?}") };
        assert_eq!((*ta, *tr), (a, repeated_a));
        assert!(Arc::ptr_eq(first, second));
        assert_eq!(first[0].points.len(), 1);
        assert!(state.pending.is_empty());
        out.clear();
        state.complete(a.request_id, Ok(&raw), &mut out);
        assert!(out.is_empty());
    }

    #[test]
    fn malformed_and_timed_out_requests_fail_without_caching_absence() {
        let mut state = ReportTraceRequests::default();
        let deadline = Instant::now() + Duration::from_secs(12);
        let a = ticket(1, -1);
        let b = ticket(2, 99);
        let repeated_b = ticket(3, b.report_uid);
        state.begin(a, deadline);
        state.begin(b, deadline);
        state.begin(repeated_b, deadline + Duration::from_secs(60));
        let mut out = Vec::new();
        let mut invalid = vec![1];
        invalid.extend(line(true, OrderType::Buy, &[(123, 4.5)]));
        invalid.push(0); // No partial-success event for the valid first line.
        state.complete(a.request_id, Ok(&invalid), &mut out);
        assert!(matches!(&out[0], ReportEvent::TraceFailed { ticket, .. } if *ticket == a));
        out.clear();
        state.tick(deadline - Duration::from_millis(1), &mut out);
        assert!(out.is_empty());
        state.tick(deadline, &mut out);
        assert_eq!(out.len(), 2);
        assert!(out.iter().all(|event| matches!(event, ReportEvent::TraceFailed { .. })));
        assert!(state.pending.is_empty());
        out.clear();
        state.complete(b.request_id, Ok(&[]), &mut out);
        assert!(out.is_empty());
        assert!(state.begin(ticket(4, b.report_uid), deadline));
    }
}
