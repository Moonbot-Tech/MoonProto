use super::*;

// Compare decoded values without requiring the production parser to allocate.
#[derive(Debug, PartialEq)]
enum ArbPayload {
    Price {
        version: u8,
        blocks: Vec<ArbPriceBlock>,
    },
    Isolation {
        version: u8,
        entries: Vec<ArbIsolationEntry>,
    },
}

#[derive(Debug, PartialEq)]
struct ArbPriceBlock {
    market_index: u16,
    prices: Vec<ArbPriceItem>,
}

#[derive(Debug, PartialEq)]
struct ArbIsolationEntry {
    market_index: u16,
    platform_code: u8,
    flags: u8,
}

fn parse_arb_payload_compact(bytes: &[u8]) -> Option<ArbPayload> {
    Some(match super::parse_arb_payload_compact(bytes)? {
        super::ArbPayload::Price { version, blocks } => ArbPayload::Price {
            version,
            blocks: blocks
                .map(|block| ArbPriceBlock {
                    market_index: block.market_index,
                    prices: block.items().collect(),
                })
                .collect(),
        },
        super::ArbPayload::Isolation { version, entries } => ArbPayload::Isolation {
            version,
            entries: entries
                .chunks_exact(4)
                .map(|row| ArbIsolationEntry {
                    market_index: u16::from_le_bytes([row[0], row[1]]),
                    platform_code: row[2],
                    flags: row[3],
                })
                .collect(),
        },
    })
}

#[test]
fn envelope_borrows_input_and_ignores_trailing_bytes() {
    let mut raw = build_arb_prices(42, &[3, 2, 0, 0]);
    raw.extend_from_slice(&[99; 16]);
    let parsed = parse_arb_prices(&raw).unwrap();
    assert_eq!(parsed.payload, &[3, 2, 0, 0]);
    assert_eq!(parsed.payload.as_ptr(), raw[15..].as_ptr());
}

#[test]
fn roundtrip() {
    let original = b"hello arb data";
    let raw = build_arb_prices(42, original);
    let parsed = parse_arb_prices(&raw).unwrap();
    assert_eq!(parsed.uid, 42);
    assert_eq!(parsed.payload, original);
}

#[test]
fn wrong_cmd_id_returns_none() {
    // CmdId=99 ≠ 6
    let mut payload = vec![99u8, 3, 0];
    payload.extend_from_slice(&1u64.to_le_bytes());
    payload.extend_from_slice(&0i32.to_le_bytes());
    assert!(parse_arb_prices(&payload).is_none());
}

#[test]
fn empty_payload() {
    let raw = build_arb_prices(7, &[]);
    let parsed = parse_arb_prices(&raw).unwrap();
    assert!(parsed.payload.is_empty());
}

#[test]
// parity: MoonBot MoonProtoBalanceStruct.pas:TArbPricesCommand.CreateFromStream
fn negative_len_is_empty() {
    let mut raw = Vec::new();
    raw.push(ARB_PRICES_CMD_ID);
    raw.extend_from_slice(&CURRENT_PROTO_CMD_VER.to_le_bytes());
    raw.extend_from_slice(&7u64.to_le_bytes());
    raw.extend_from_slice(&(-1i32).to_le_bytes());

    let parsed = parse_arb_prices(&raw).unwrap();
    assert!(parsed.payload.is_empty());
}

#[test]
fn compact_v2_price_payload() {
    let mut payload = vec![2u8];
    payload.extend_from_slice(&42u16.to_le_bytes());
    payload.push(2);
    payload.push(7);
    payload.extend_from_slice(&123.25f32.to_le_bytes());
    payload.push(8);
    payload.extend_from_slice(&99.5f32.to_le_bytes());
    payload.extend_from_slice(&9u16.to_le_bytes());
    payload.push(1);
    payload.push(100);
    payload.extend_from_slice(&1.5f32.to_le_bytes());

    let parsed = parse_arb_payload_compact(&payload).unwrap();
    assert_eq!(
        parsed,
        ArbPayload::Price {
            version: 2,
            blocks: vec![
                ArbPriceBlock {
                    market_index: 42,
                    prices: vec![
                        ArbPriceItem {
                            platform_code: 7,
                            price: 123.25,
                        },
                        ArbPriceItem {
                            platform_code: 8,
                            price: 99.5,
                        },
                    ],
                },
                ArbPriceBlock {
                    market_index: 9,
                    prices: vec![ArbPriceItem {
                        platform_code: 100,
                        price: 1.5,
                    }],
                },
            ],
        }
    );
}

#[test]
fn compact_v3_price_payload() {
    let mut payload = vec![3u8, CMD_PRICE];
    payload.extend_from_slice(&11u16.to_le_bytes());
    payload.push(1);
    payload.push(102);
    payload.extend_from_slice(&77.0f32.to_le_bytes());

    let parsed = parse_arb_payload_compact(&payload).unwrap();
    assert_eq!(
        parsed,
        ArbPayload::Price {
            version: 3,
            blocks: vec![ArbPriceBlock {
                market_index: 11,
                prices: vec![ArbPriceItem {
                    platform_code: 102,
                    price: 77.0,
                }],
            }],
        }
    );
}

#[test]
fn compact_v3_isolation_payload() {
    let mut payload = vec![3u8, CMD_ISOL];
    payload.extend_from_slice(&2u16.to_le_bytes());
    payload.extend_from_slice(&42u16.to_le_bytes());
    payload.push(7);
    payload.push(0b01);
    payload.extend_from_slice(&43u16.to_le_bytes());
    payload.push(8);
    payload.push(0b10);

    let parsed = parse_arb_payload_compact(&payload).unwrap();
    assert_eq!(
        parsed,
        ArbPayload::Isolation {
            version: 3,
            entries: vec![
                ArbIsolationEntry {
                    market_index: 42,
                    platform_code: 7,
                    flags: 0b01,
                },
                ArbIsolationEntry {
                    market_index: 43,
                    platform_code: 8,
                    flags: 0b10,
                },
            ],
        }
    );
}

#[test]
fn compact_truncated_price_block_stops_before_partial_block() {
    let mut payload = vec![2u8];
    payload.extend_from_slice(&42u16.to_le_bytes());
    payload.push(1);
    payload.push(7);
    payload.extend_from_slice(&123.25f32.to_le_bytes());
    payload.extend_from_slice(&43u16.to_le_bytes());
    payload.push(2);
    payload.push(8);
    payload.extend_from_slice(&99.5f32.to_le_bytes());

    let parsed = parse_arb_payload_compact(&payload).unwrap();
    assert_eq!(
        parsed,
        ArbPayload::Price {
            version: 2,
            blocks: vec![ArbPriceBlock {
                market_index: 42,
                prices: vec![ArbPriceItem {
                    platform_code: 7,
                    price: 123.25,
                }],
            }],
        }
    );
}

#[test]
fn compact_truncated_isolation_keeps_complete_entries() {
    let mut payload = vec![3u8, CMD_ISOL];
    payload.extend_from_slice(&2u16.to_le_bytes());
    payload.extend_from_slice(&42u16.to_le_bytes());
    payload.push(7);
    payload.push(0b01);
    payload.extend_from_slice(&43u16.to_le_bytes());
    payload.push(8);

    let parsed = parse_arb_payload_compact(&payload).unwrap();
    assert_eq!(
        parsed,
        ArbPayload::Isolation {
            version: 3,
            entries: vec![ArbIsolationEntry {
                market_index: 42,
                platform_code: 7,
                flags: 0b01,
            }],
        }
    );
}

#[test]
fn compact_isolation_declared_count_is_bounded_by_payload() {
    let mut payload = vec![3u8, CMD_ISOL];
    payload.extend_from_slice(&u16::MAX.to_le_bytes());
    payload.extend_from_slice(&42u16.to_le_bytes());
    payload.push(7);
    payload.push(0b01);

    let parsed = parse_arb_payload_compact(&payload).unwrap();
    assert_eq!(
        parsed,
        ArbPayload::Isolation {
            entries: vec![ArbIsolationEntry {
                market_index: 42,
                platform_code: 7,
                flags: 0b01,
            }],
            version: 3,
        }
    );
}

#[test]
fn compact_rejects_invalid_header() {
    assert!(parse_arb_payload_compact(&[]).is_none());
    assert!(parse_arb_payload_compact(&[1]).is_none());
    assert!(parse_arb_payload_compact(&[0, 1]).is_none());
    assert!(parse_arb_payload_compact(&[3]).is_none());
    assert!(parse_arb_payload_compact(&[3, 99]).is_none());
    assert!(parse_arb_payload_compact(&[3, CMD_ISOL]).is_none());
    assert!(parse_arb_payload_compact(&[3, CMD_ISOL, 0]).is_none());
    assert_eq!(parse_arb_payload_compact(&[3, CMD_ISOL, 0, 0]),
        Some(ArbPayload::Isolation { version: 3, entries: vec![] }));
}
