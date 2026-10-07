//! Offline replay of locally captured strategy blobs. No server connection.

use super::*;
use crate::client::thread_cpu::ThreadCpuTimer;
use crate::commands::inflate::{read_inflate_to_vec, MAX_INFLATE_OUTPUT_SIZE};
use crate::commands::strategy_serializer::parse_strategy_batch_plain_for_each_with_schema_field_types;
use flate2::read::DeflateDecoder;
use sha3::{Digest, Sha3_256};
use std::hint::black_box;
use std::path::Path;

fn inflate(data: &[u8]) -> Vec<u8> {
    read_inflate_to_vec(
        &mut DeflateDecoder::new(data),
        data.len().saturating_mul(40).clamp(4096, 8 * 1024 * 1024),
        MAX_INFLATE_OUTPUT_SIZE,
    ).unwrap()
}

fn measure<T>(name: &str, repeats: usize, mut f: impl FnMut() -> T) {
    let mut wall = Vec::new();
    let mut cpu = Vec::new();
    let mut cycles = Vec::new();
    for i in 0..repeats + 5 {
        let started = Instant::now();
        let clock = ThreadCpuTimer::start();
        let result = black_box(f());
        let elapsed = clock.elapsed();
        let elapsed_wall = started.elapsed().as_nanos() as u64;
        if i >= 5 {
            wall.push(elapsed_wall);
            cpu.extend(elapsed.time.map(|v| v.as_nanos() as u64));
            cycles.extend(elapsed.cycles);
        }
        drop(result); // Destruction is outside the measured receive/apply work.
    }
    wall.sort_unstable();
    cpu.sort_unstable();
    cycles.sort_unstable();
    println!("STRATEGY_PROFILE {}", serde_json::json!({
        "phase": name, "repeats": repeats,
        "wall_ns_median": wall[wall.len() / 2],
        "wall_ns_p95": wall[wall.len() * 95 / 100],
        "cpu_ns_median": cpu.get(cpu.len() / 2),
        "cycles_median": cycles.get(cycles.len() / 2),
    }));
}

fn hash_snapshot(hash: &mut Sha3_256, snapshot: &StrategySnapshot) {
    hash.update(snapshot.strategy_id.to_le_bytes());
    hash.update(snapshot.strategy_ver.to_le_bytes());
    hash.update(snapshot.last_date.to_le_bytes());
    hash.update([u8::from(snapshot.checked), snapshot.kind]);
    hash.update((snapshot.path.len() as u64).to_le_bytes());
    hash.update(snapshot.path.as_bytes());
    hash.update((snapshot.fields.len() as u64).to_le_bytes());
    for (name, value) in snapshot.fields.iter() {
        hash.update((name.len() as u64).to_le_bytes());
        hash.update(name.as_bytes());
        use crate::FieldValue::*;
        hash.update([value.type_id()]);
        match value {
            Bool(v) => hash.update([u8::from(*v)]),
            Byte(v) => hash.update([*v]),
            Word(v) => hash.update(v.to_le_bytes()),
            Int32(v) => hash.update(v.to_le_bytes()),
            UInt32(v) => hash.update(v.to_le_bytes()),
            Int64(v) => hash.update(v.to_le_bytes()),
            UInt64(v) => hash.update(v.to_le_bytes()),
            Single(v) => hash.update(v.to_bits().to_le_bytes()),
            Double(v) => hash.update(v.to_bits().to_le_bytes()),
            String(v) => {
                hash.update((v.len() as u64).to_le_bytes());
                hash.update(v.as_bytes());
            }
        }
    }
}

#[test]
#[ignore = "requires MOONPROTO_STRATEGY_PROFILE_DIR with private local captures"]
fn offline_strategy_profile() {
    let dir = std::env::var("MOONPROTO_STRATEGY_PROFILE_DIR").unwrap();
    let dir = Path::new(&dir);
    let field_types = Arc::new(std::fs::read_to_string(dir.join("field_types.txt")).unwrap()
        .lines().map(|line| {
            let (kind, name) = line.trim_start_matches('\u{feff}').split_once(' ').unwrap();
            (name.to_owned(), kind.parse::<u8>().unwrap())
        }).collect::<HashMap<_, _>>());
    let data = std::fs::read(dir.join("snapshot.bin")).unwrap();
    let plain = inflate(&data);
    let parse = || {
        let mut strategies = Vec::new();
        let (count, paths) = parse_strategy_batch_plain_for_each_with_schema_field_types(
            &plain, Some(&field_types), &mut |_, _, _| false,
            &mut |s| strategies.push(s),
        ).unwrap();
        assert_eq!(count, strategies.len());
        (paths, strategies)
    };
    let (_, strategies) = parse();
    let fields: usize = strategies.iter().map(|s| s.fields.len()).sum();
    let strings: usize = strategies.iter().flat_map(|s| s.fields.iter())
        .filter(|(_, v)| matches!(v, crate::FieldValue::String(s) if !s.is_empty())).count();
    let mut proof = Sha3_256::new();
    for strategy in &strategies { hash_snapshot(&mut proof, strategy); }
    println!("STRATEGY_INPUT {}", serde_json::json!({
        "compressed": data.len(), "plain": plain.len(), "strategies": strategies.len(),
        "fields": fields, "nonempty_strings": strings,
        "decoded_sha3_256": format!("{:x}", proof.finalize()),
    }));
    let fresh = || {
        let mut state = StratsState::new();
        state.schema_field_types = Some(Arc::clone(&field_types));
        state
    };
    let apply = |state: &mut StratsState| {
        let outcome = state.apply_snapshot_decoded_with_mode_in_place(&data, true).unwrap();
        state.apply_server_folders(&outcome.paths, 1);
        state.apply_server_order(&outcome.order, 1, true);
    };
    measure("inflate", 100, || inflate(&data));
    measure("parse_plain", 100, parse);
    measure("cold_apply", 100, || { let mut state = fresh(); apply(&mut state); state });
    let mut loaded = fresh();
    apply(&mut loaded);
    assert_eq!(loaded.snapshots_by_id.len(), strategies.len());
    for expected in &strategies {
        assert_eq!(loaded.snapshot(expected.strategy_id), Some(expected));
        assert_eq!(loaded.get(expected.strategy_id).unwrap().sell_price.to_bits(),
            expected.fields.get_double("SellPrice").unwrap_or(0.0).to_bits());
    }
    measure("unchanged_apply", 100, || apply(&mut loaded));
    let corpus = dir.join("corpus");
    if corpus.is_dir() {
        let mut paths: Vec<_> = std::fs::read_dir(corpus).unwrap()
            .map(|entry| entry.unwrap().path()).collect();
        paths.sort();
        let mut hash = Sha3_256::new();
        let mut state = fresh();
        for path in &paths {
            let data = std::fs::read(path).unwrap();
            let outcome = state.apply_snapshot_decoded_with_mode_in_place(&data, true).unwrap();
            hash.update((outcome.count as u64).to_le_bytes());
            for id in &outcome.order { hash.update(id.to_le_bytes()); }
            // Covers accepted/stale mixtures and retained data across dictionary changes.
            for id in &state.order {
                let snapshot = state.snapshot(*id).unwrap();
                let sell_price = state.get(*id).unwrap().sell_price.to_bits();
                assert_eq!(sell_price, snapshot.fields.get_double("SellPrice").unwrap_or(0.0).to_bits());
                hash.update(sell_price.to_le_bytes());
                hash_snapshot(&mut hash, snapshot);
            }
        }
        println!("STRATEGY_CORPUS files={} sha3={:x}", paths.len(), hash.finalize());
    }
}
