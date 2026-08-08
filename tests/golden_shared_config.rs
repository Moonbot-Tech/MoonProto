//! Golden acceptance for `shared_config` against a real MoonBot export.
//!
//! Requires local files that never enter the repo (they contain a real user
//! config). Run explicitly:
//!
//! ```text
//! set MOONBOT_GOLDEN_PAYLOAD=<path to decompressed payload.bin>
//! set MOONBOT_GOLDEN_MBSC=<path to the clipboard export log>
//! cargo test --test golden_shared_config -- --ignored
//! ```

use moonproto::shared_config::{from_mbsc_string, parse_payload, serialize_payload};

fn env_file(var: &str) -> Option<Vec<u8>> {
    let path = std::env::var(var).ok()?;
    Some(std::fs::read(path).expect("golden file must be readable"))
}

/// The decompressed payload from a live bot must parse without remainder and
/// re-serialize bit-for-bit.
#[test]
#[ignore = "needs a local golden export, see module docs"]
fn golden_payload_roundtrip_bit_exact() {
    let Some(payload) = env_file("MOONBOT_GOLDEN_PAYLOAD") else {
        panic!("MOONBOT_GOLDEN_PAYLOAD not set");
    };
    let cfg = parse_payload(&payload).expect("golden payload must parse");
    let out = serialize_payload(&cfg).expect("serialize golden shared config");
    assert_eq!(out.len(), payload.len(), "re-serialized size differs");
    // Bit-exact comparison with one documented tolerance: legacy data can carry
    // a non-0/1 byte in a bool slot (seen in the wild: CustomDrawTool.SoundAlert
    // = 0x8B). Parsing normalizes it to true and re-serializing emits 1, which
    // is semantically identical for every reader. Any structural error
    // (shift, wrong width) still fails on the very next bytes.
    for (i, (ours, golden)) in out.iter().zip(payload.iter()).enumerate() {
        if ours == golden {
            continue;
        }
        if *ours == 1 && *golden > 1 {
            continue; // legacy true byte normalized to 1
        }
        let lo = i.saturating_sub(16);
        let hi = (i + 16).min(payload.len());
        panic!(
            "payload differs at byte {i}\n  golden: {}\n  ours:   {}",
            hex_dump(&payload[lo..hi]),
            hex_dump(&out[lo..hi])
        );
    }
}

fn hex_dump(bytes: &[u8]) -> String {
    bytes
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect::<Vec<_>>()
        .join(" ")
}

/// The full clipboard string (fence, MBSC7 header, base16384, CRC) must decode
/// to the same config as the raw payload.
#[test]
#[ignore = "needs a local golden export, see module docs"]
fn golden_mbsc_string_matches_payload() {
    let Some(payload) = env_file("MOONBOT_GOLDEN_PAYLOAD") else {
        panic!("MOONBOT_GOLDEN_PAYLOAD not set");
    };
    let Some(raw) = env_file("MOONBOT_GOLDEN_MBSC") else {
        panic!("MOONBOT_GOLDEN_MBSC not set");
    };
    let text = String::from_utf8(raw).expect("mbsc log must be UTF-8");
    let from_string = from_mbsc_string(&text).expect("mbsc string must parse");
    let from_payload = parse_payload(&payload).expect("golden payload must parse");
    assert_eq!(
        serialize_payload(&from_string).expect("serialize clipboard config"),
        serialize_payload(&from_payload).expect("serialize payload config"),
        "clipboard decode disagrees with raw payload"
    );
}
