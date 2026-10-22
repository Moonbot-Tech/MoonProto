//! Reproducible local benchmark; no socket, credentials, or live server required.
use super::*;
use crate::client::diagnostics::ErrEmuDiagnosticsState;
use crate::client::thread_cpu::ThreadCpuTimer;
use crate::commands::candles::{parse_request_candles_data_response, CandlesAggregator};
use crate::commands::candles::{parse_request_candles_reader, CandlesChunkResult};
use crate::commands::engine_api::parse_engine_response_owned;
use crate::protocol::slicing::SlicedData;
use std::io::Write;
use std::time::Instant;

fn fixture() -> Vec<u8> {
    let mut plain = Vec::new();
    plain.extend_from_slice(&0i32.to_le_bytes());
    plain.push(2);
    plain.extend_from_slice(&220i32.to_le_bytes());
    plain.extend_from_slice(&0f64.to_le_bytes());
    let mut random = 0x1234_5678u32;
    for market in 0..220 {
        let name = format!("TEST{market}");
        plain.extend_from_slice(&(name.len() as u16).to_le_bytes());
        for c in name.encode_utf16() {
            plain.extend_from_slice(&c.to_le_bytes());
        }
        plain.extend_from_slice(&1500i32.to_le_bytes());
        for row in 0..1500 {
            for _ in 0..3 {
                random ^= random << 13;
                random ^= random >> 17;
                random ^= random << 5;
                plain.extend_from_slice(&(0x3f80_0000 | (random & 0x007f_ffff)).to_le_bytes());
            }
            plain.extend_from_slice(&(45_000.0 + row as f64 / 288.0).to_le_bytes());
        }
        plain.extend_from_slice(&[0; 64]);
    }
    let mut zip = flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::default());
    zip.write_all(&plain).unwrap();
    zip.finish().unwrap()
}

fn percentile(samples: &mut [f64], fraction: f64) -> f64 {
    if samples.is_empty() {
        return f64::NAN; // Thread cycle counters are unavailable on this platform.
    }
    samples.sort_by(f64::total_cmp);
    samples[((samples.len() - 1) as f64 * fraction).round() as usize]
}

#[test]
#[ignore = "local bulk receive benchmark; run in release mode with --nocapture"]
fn bulk_receive_benchmark() {
    let archive = fixture();
    let expected = parse_request_candles_data_response(&archive).unwrap();
    assert_eq!(expected.len(), 220);
    let mut client = Client::new(dummy_cfg());
    let key = install_server_decode_session(&mut client, 42);
    let fragments: Vec<Vec<Vec<u8>>> = archive
        .chunks(archive.len().div_ceil(2))
        .enumerate()
        .map(|(index, part)| {
            let mut chunk = Vec::new();
            chunk.extend_from_slice(&(index as u16).to_le_bytes());
            chunk.extend_from_slice(&2u16.to_le_bytes());
            chunk.extend_from_slice(part);
            let response = build_engine_response_payload(123, EngineMethod::RequestCandlesData, &chunk);
            let encrypted = build_server_crypted_payload(&key, index as u64 + 1, Command::API, &response);
            let mut sliced = vec![Command::Crypted.to_byte()];
            sliced.extend_from_slice(&encrypted);
            sliced.chunks(16_384).map(<[u8]>::to_vec).collect()
        })
        .collect();
    println!(
        "BULK fixture: archive_bytes={} markets=220 rows=330000 fragments={:?}",
        archive.len(),
        fragments.iter().map(Vec::len).collect::<Vec<_>>()
    );
    for reordered in [false, true] {
        for diagnostic in [false, true] {
            let mut receive_ms = Vec::new();
            let mut total_ms = Vec::new();
            let mut receive_cycles = Vec::new();
            let mut total_cycles = Vec::new();
            for iteration in 0..65 {
                client.recv.data_read_state.slider = crate::protocol::slider::Slider::new();
                let mut collector = CandlesAggregator::new();
                let mut diagnostics = ErrEmuDiagnosticsState::default();
                let start = Instant::now();
                let cpu = ThreadCpuTimer::start();
                let mut completed = None;
                for (index, pieces) in fragments.iter().enumerate() {
                    let mut sliced = SlicedData::new(index as u16, (pieces.len() - 1) as u8);
                    for ordinal in 0..pieces.len() {
                        let block = if reordered { pieces.len() - 1 - ordinal } else { ordinal };
                        sliced.receive_piece(block as u8, &pieces[block]);
                    }
                    let (cmd, owned) = sliced.assemble().unwrap();
                    let (cmd, mut decoded) =
                        Client::decode_command_payload_owned(&mut client.recv.data_read_state, cmd, owned).unwrap();
                    if diagnostic {
                        diagnostics.record_sliced_complete(index as u16, pieces.len(), cmd, &decoded);
                    }
                    let (_, body) = parse_engine_response_owned(&mut decoded).unwrap();
                    if let CandlesChunkResult::Complete(bytes) = collector.on_chunk_result(body) {
                        completed = Some(bytes);
                    }
                }
                let receive = start.elapsed().as_secs_f64() * 1000.0;
                let receive_cpu = cpu.elapsed();
                let completed = completed.unwrap();
                let rows = parse_request_candles_reader(completed.reader(), completed.len(), 0.0).unwrap();
                drop(completed); // Include releasing the retained archive in the total, as in the baseline.
                let total = start.elapsed().as_secs_f64() * 1000.0;
                let total_cpu = cpu.elapsed();
                assert_eq!(rows.len(), expected.len());
                for (actual, expected) in rows.iter().zip(&expected) {
                    assert_eq!(actual.market_name, expected.market_name);
                    assert_eq!(actual.candles_5m, expected.candles_5m);
                }
                if iteration >= 5 {
                    receive_ms.push(receive);
                    total_ms.push(total);
                    if let Some(cycles) = receive_cpu.cycles {
                        receive_cycles.push(cycles as f64 / 1e6);
                    }
                    if let Some(cycles) = total_cpu.cycles {
                        total_cycles.push(cycles as f64 / 1e6);
                    }
                }
            }
            println!("BULK reordered={reordered} diagnostics={diagnostic} samples=60 receive_ms_p50={:.3} receive_ms_p95={:.3} total_ms_p50={:.3} total_ms_p95={:.3} receive_Mcycles_p50={:.3} total_Mcycles_p50={:.3}",
                percentile(&mut receive_ms, 0.5), percentile(&mut receive_ms, 0.95),
                percentile(&mut total_ms, 0.5), percentile(&mut total_ms, 0.95),
                percentile(&mut receive_cycles, 0.5), percentile(&mut total_cycles, 0.5));
        }
    }
}
