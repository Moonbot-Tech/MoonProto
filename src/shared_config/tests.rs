//! Tests for the shared_config module.

use super::clipboard::{test_crc32_ieee, test_decode_base16384, test_encode_base16384};
use super::sections::*;
use super::wire::{parse_payload, serialize_payload};
use super::{from_mbsc_string, from_mbshare_bytes, to_mbsc_string, to_mbshare_bytes};

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

#[test]
fn magic_and_version() {
    let cfg = SharedConfig::default();
    let bytes = serialize_payload(&cfg).unwrap();
    assert_eq!(&bytes[..4], b"MBSP", "magic");
    assert_eq!(bytes[4], 7, "version");
    // config_version = 164 as u16 LE
    assert_eq!(u16::from_le_bytes([bytes[5], bytes[6]]), 164);
}

#[test]
fn section_kinds_present() {
    let cfg = SharedConfig::default();
    let bytes = serialize_payload(&cfg).unwrap();
    // Scan block headers after the 7-byte payload header.
    let mut pos = 7usize;
    let mut kinds = Vec::new();
    while pos + 5 <= bytes.len() {
        let kind = bytes[pos];
        let size = u32::from_le_bytes(bytes[pos + 1..pos + 5].try_into().unwrap());
        kinds.push(kind);
        pos += 5 + size as usize;
    }
    assert_eq!(kinds, vec![1, 2, 3, 4, 5, 6], "all 6 sections in order");
}

#[test]
fn section_internal_versions() {
    let cfg = SharedConfig::default();
    let bytes = serialize_payload(&cfg).unwrap();
    // Find each block and check the first byte of its body.
    let mut pos = 7usize;
    let expected_versions: [(u8, u8); 6] = [
        (1, 2), // Signals: kind=1, ver=2
        (2, 3), // Trading: kind=2, ver=3
        (3, 2), // Visual:  kind=3, ver=2
        (4, 1), // Theme:   kind=4, ver=1
        (5, 1), // Ini:     kind=5, ver=1
        (6, 3), // Ui:      kind=6, ver=3
    ];
    for (expected_kind, expected_ver) in &expected_versions {
        assert!(
            pos + 5 <= bytes.len(),
            "block header for kind {expected_kind}"
        );
        let kind = bytes[pos];
        let size = u32::from_le_bytes(bytes[pos + 1..pos + 5].try_into().unwrap());
        assert_eq!(kind, *expected_kind);
        let body_start = pos + 5;
        assert_eq!(
            bytes[body_start], *expected_ver,
            "internal version for kind {kind}"
        );
        pos = body_start + size as usize;
    }
}

// ---------------------------------------------------------------------------
// Roundtrip: default
// ---------------------------------------------------------------------------

#[test]
fn roundtrip_default() {
    let original = SharedConfig::default();
    let bytes = serialize_payload(&original).unwrap();
    let parsed = parse_payload(&bytes).expect("parse default payload");

    // Spot-check key fields across all sections.
    assert_eq!(parsed.config_version, 164);
    assert!(parsed.signals.full_screen_prevent_signals);
    assert_eq!(parsed.signals.msg_keywords_long, "buy");
    assert_eq!(parsed.trading.x_sell, 5);
    assert_eq!(parsed.trading.trailing_drop, -2.0);
    assert_eq!(parsed.trading.auto_start.stop_trades, 50);
    assert_eq!(parsed.trading.multi_orders.done_opacity, 0.5);
    assert_eq!(parsed.visual.chart_time_scale, 60);
    assert_eq!(parsed.visual.custom_draw_config.ver, 2);
    assert_eq!(parsed.theme.current_style, 0);
    assert_eq!(
        parsed.ui.hotkeys_config.s_price,
        [1.0, 3.0, 5.0, 10.0, 25.0, 100.0]
    );
    assert_eq!(parsed.ui.strat_expanded_state, [true; 11]);
}

#[test]
fn roundtrip_byte_exact() {
    let original = SharedConfig::default();
    let bytes1 = serialize_payload(&original).unwrap();
    let parsed = parse_payload(&bytes1).unwrap();
    let bytes2 = serialize_payload(&parsed).unwrap();
    assert_eq!(
        bytes1, bytes2,
        "serialize -> parse -> serialize must be identical"
    );
}

// ---------------------------------------------------------------------------
// Roundtrip: modified fields in every section
// ---------------------------------------------------------------------------

#[test]
fn roundtrip_modified_signals() {
    let mut cfg = SharedConfig::default();
    cfg.signals.pump_channel = "test_channel".into();
    cfg.signals.pump_channels = vec!["ch1".into(), "ch2".into()];
    cfg.signals.telegram_auto_buy = true;
    cfg.signals.signal_config.x_found_price = 42;
    cfg.signals.news_tokens_filter = "BTC,ETH".into();

    let bytes = serialize_payload(&cfg).unwrap();
    let parsed = parse_payload(&bytes).unwrap();
    assert_eq!(parsed.signals.pump_channel, "test_channel");
    assert_eq!(parsed.signals.pump_channels.len(), 2);
    assert!(parsed.signals.telegram_auto_buy);
    assert_eq!(parsed.signals.signal_config.x_found_price, 42);
    assert_eq!(parsed.signals.news_tokens_filter, "BTC,ETH");
}

#[test]
fn roundtrip_modified_trading() {
    let mut cfg = SharedConfig::default();
    cfg.trading.trailing_stop = true;
    cfg.trading.g_take_profit = 7.77;
    cfg.trading.manual_strategy = "MyStrat".into();
    cfg.trading.manual_strats_names[0] = "Strat1".into();
    cfg.trading.deltas_by_trades = true;
    cfg.trading.use_hl_fast_ioc = true;

    let bytes = serialize_payload(&cfg).unwrap();
    let parsed = parse_payload(&bytes).unwrap();
    assert!(parsed.trading.trailing_stop);
    assert!((parsed.trading.g_take_profit - 7.77).abs() < 1e-10);
    assert_eq!(parsed.trading.manual_strategy, "MyStrat");
    assert_eq!(parsed.trading.manual_strats_names[0], "Strat1");
    assert!(parsed.trading.deltas_by_trades);
    assert!(parsed.trading.use_hl_fast_ioc);
}

#[test]
fn roundtrip_modified_visual() {
    let mut cfg = SharedConfig::default();
    cfg.visual.chart_candles_style = 1;
    cfg.visual.candle_colors[0].green = 0xFF00FF00;
    cfg.visual.ai_card_model = "gpt-4".into();
    cfg.visual.custom_draw_config.tools[0].color_w = 0xFFAABBCC;
    cfg.visual.custom_draw_config.tools[0].stroke = 2; // Dot

    let bytes = serialize_payload(&cfg).unwrap();
    let parsed = parse_payload(&bytes).unwrap();
    assert_eq!(parsed.visual.chart_candles_style, 1);
    assert_eq!(parsed.visual.candle_colors[0].green, 0xFF00FF00);
    assert_eq!(parsed.visual.ai_card_model, "gpt-4");
    assert_eq!(
        parsed.visual.custom_draw_config.tools[0].color_w,
        0xFFAABBCC
    );
    assert_eq!(parsed.visual.custom_draw_config.tools[0].stroke, 2);
}

#[test]
fn roundtrip_modified_theme_ini() {
    let mut cfg = SharedConfig::default();
    cfg.theme.current_style = 1;
    cfg.theme.ini_sections.push(IniSectionData {
        name: "ColorsLight".into(),
        entries: vec![
            ("key1".into(), "val1".into()),
            ("key2".into(), "val2".into()),
        ],
    });
    cfg.ini.ini_sections.push(IniSectionData {
        name: "Charts".into(),
        entries: vec![("ind1".into(), "cfg1".into())],
    });

    let bytes = serialize_payload(&cfg).unwrap();
    let parsed = parse_payload(&bytes).unwrap();
    assert_eq!(parsed.theme.current_style, 1);
    assert_eq!(parsed.theme.ini_sections.len(), 1);
    assert_eq!(parsed.theme.ini_sections[0].name, "ColorsLight");
    assert_eq!(parsed.theme.ini_sections[0].entries.len(), 2);
    assert_eq!(parsed.ini.ini_sections.len(), 1);
    assert_eq!(
        parsed.ini.ini_sections[0].entries[0],
        ("ind1".into(), "cfg1".into())
    );
}

#[test]
fn roundtrip_modified_ui() {
    let mut cfg = SharedConfig::default();
    cfg.ui.coins_sort_order = 3;
    cfg.ui.hotkeys_config.s_price = [2.0, 4.0, 6.0, 8.0, 10.0, 50.0];
    cfg.ui.strat_expanded_state[5] = false;
    cfg.ui.main_button_index_1 = 3; // MBT_Alerts

    let bytes = serialize_payload(&cfg).unwrap();
    let parsed = parse_payload(&bytes).unwrap();
    assert_eq!(parsed.ui.coins_sort_order, 3);
    assert_eq!(parsed.ui.hotkeys_config.s_price[0], 2.0);
    assert!(!parsed.ui.strat_expanded_state[5]);
    assert_eq!(parsed.ui.main_button_index_1, 3);
}

// ---------------------------------------------------------------------------
// Unknown tail preservation
// ---------------------------------------------------------------------------

#[test]
fn unknown_tail_signals() {
    let cfg = SharedConfig::default();
    let mut bytes = serialize_payload(&cfg).unwrap();

    // Append extra bytes to the signals block body.
    // The signals block starts at offset 7 (after header).
    // Block header: kind(1) + size(4), then body.
    let block_start = 7;
    let block_size_pos = block_start + 1;
    let old_size = u32::from_le_bytes(
        bytes[block_size_pos..block_size_pos + 4]
            .try_into()
            .unwrap(),
    );
    let body_end = block_start + 5 + old_size as usize;

    // Insert 3 extra bytes at the end of the signals block body.
    let extra = [0xAA, 0xBB, 0xCC];
    bytes.splice(body_end..body_end, extra.iter().copied());
    // Patch the block size.
    let new_size = old_size + 3;
    bytes[block_size_pos..block_size_pos + 4].copy_from_slice(&new_size.to_le_bytes());

    let parsed = parse_payload(&bytes).expect("parse with extra tail bytes");
    assert_eq!(parsed.signals.unknown_tail, vec![0xAA, 0xBB, 0xCC]);

    // Re-serialize and verify the extra bytes are preserved.
    let bytes2 = serialize_payload(&parsed).unwrap();
    let parsed2 = parse_payload(&bytes2).unwrap();
    assert_eq!(parsed2.signals.unknown_tail, vec![0xAA, 0xBB, 0xCC]);
}

#[test]
fn unknown_tail_trading() {
    let cfg = SharedConfig::default();
    let mut bytes = serialize_payload(&cfg).unwrap();

    // Find the trading block (second block).
    let mut pos = 7;
    // Skip signals block.
    let sig_size = u32::from_le_bytes(bytes[pos + 1..pos + 5].try_into().unwrap());
    pos += 5 + sig_size as usize;

    // Now at trading block.
    let block_size_pos = pos + 1;
    let old_size = u32::from_le_bytes(
        bytes[block_size_pos..block_size_pos + 4]
            .try_into()
            .unwrap(),
    );
    let body_end = pos + 5 + old_size as usize;

    let extra = [0xDE, 0xAD];
    bytes.splice(body_end..body_end, extra.iter().copied());
    let new_size = old_size + 2;
    bytes[block_size_pos..block_size_pos + 4].copy_from_slice(&new_size.to_le_bytes());

    let parsed = parse_payload(&bytes).expect("parse with trading tail");
    assert_eq!(parsed.trading.unknown_tail, vec![0xDE, 0xAD]);

    let bytes2 = serialize_payload(&parsed).unwrap();
    let parsed2 = parse_payload(&bytes2).unwrap();
    assert_eq!(parsed2.trading.unknown_tail, vec![0xDE, 0xAD]);
}

// ---------------------------------------------------------------------------
// Base16384
// ---------------------------------------------------------------------------

#[test]
fn base16384_empty() {
    let encoded = test_encode_base16384(&[]);
    assert!(encoded.is_empty());
    let decoded = test_decode_base16384(&encoded, 0).unwrap();
    assert!(decoded.is_empty());
}

#[test]
fn base16384_single_byte() {
    let encoded = test_encode_base16384(&[0x42]);
    let decoded = test_decode_base16384(&encoded, 1).unwrap();
    assert_eq!(decoded, vec![0x42]);
}

#[test]
fn base16384_7_bytes() {
    let data = [1, 2, 3, 4, 5, 6, 7];
    let encoded = test_encode_base16384(&data);
    let decoded = test_decode_base16384(&encoded, 7).unwrap();
    assert_eq!(decoded, data.to_vec());
}

#[test]
fn base16384_175_bytes() {
    let data: Vec<u8> = (0..175).map(|i| (i * 37) as u8).collect();
    let encoded = test_encode_base16384(&data);
    let decoded = test_decode_base16384(&encoded, 175).unwrap();
    assert_eq!(decoded, data);
}

#[test]
fn base16384_roundtrip_various_lengths() {
    for len in [0, 1, 2, 7, 14, 15, 100, 175, 256, 1000] {
        let data: Vec<u8> = (0..len).map(|i| (i * 13 + 7) as u8).collect();
        let encoded = test_encode_base16384(&data);
        let decoded = test_decode_base16384(&encoded, len).unwrap();
        assert_eq!(decoded, data, "roundtrip failed for len={len}");
    }
}

// ---------------------------------------------------------------------------
// CRC32
// ---------------------------------------------------------------------------

#[test]
fn crc32_known_vector() {
    // CRC-32 of "123456789" with polynomial 0xEDB88320 = 0xCBF43926
    let crc = test_crc32_ieee(b"123456789");
    assert_eq!(crc, 0xCBF43926, "CRC32 known vector");
}

#[test]
fn crc32_empty() {
    let crc = test_crc32_ieee(&[]);
    assert_eq!(crc, 0x00000000, "CRC32 of empty input");
}

// ---------------------------------------------------------------------------
// Clipboard: MBSC string roundtrip
// ---------------------------------------------------------------------------

#[test]
fn mbsc_roundtrip() {
    let original = SharedConfig::default();
    let mbsc = to_mbsc_string(&original).unwrap();

    // Verify fence format.
    assert!(mbsc.starts_with("```mbcfg\n"));
    assert!(mbsc.ends_with("\n```"));
    assert!(mbsc.contains("MBSC7:"));

    let parsed = from_mbsc_string(&mbsc).unwrap();
    assert_eq!(parsed.trading.x_sell, original.trading.x_sell);
    assert!(parsed.signals.full_screen_prevent_signals);
}

#[test]
fn mbsc_parse_with_surrounding_text() {
    let cfg = SharedConfig::default();
    let mbsc = to_mbsc_string(&cfg).unwrap();

    // Wrap in chat-like text.
    let messy = format!("Hey check this config:\n{mbsc}\nLet me know!");
    let parsed = from_mbsc_string(&messy).unwrap();
    assert_eq!(parsed.trading.x_sell, cfg.trading.x_sell);
}

#[test]
fn mbsc_parse_with_linebreaks_inside() {
    let cfg = SharedConfig::default();
    let mbsc = to_mbsc_string(&cfg).unwrap();

    // Insert newlines inside the MBSC data (CleanSharedConfigClipboardText
    // strips chars <= 32).
    let parts: Vec<&str> = mbsc.split("MBSC7:").collect();
    assert_eq!(parts.len(), 2);
    let data_part = parts[1];
    // Insert a newline every 50 chars.
    let with_breaks: String = data_part
        .chars()
        .enumerate()
        .flat_map(|(i, c)| {
            if i > 0 && i % 50 == 0 {
                vec!['\n', c]
            } else {
                vec![c]
            }
        })
        .collect();
    let rebuilt = format!("{}MBSC7:{}", parts[0], with_breaks);
    let parsed = from_mbsc_string(&rebuilt).unwrap();
    assert_eq!(parsed.trading.x_sell, cfg.trading.x_sell);
}

// ---------------------------------------------------------------------------
// mbshare roundtrip
// ---------------------------------------------------------------------------

#[test]
fn mbshare_roundtrip() {
    let original = SharedConfig::default();
    let bytes = to_mbshare_bytes(&original).unwrap();
    let parsed = from_mbshare_bytes(&bytes).unwrap();
    assert_eq!(parsed.trading.x_sell, original.trading.x_sell);
    assert_eq!(parsed.config_version, 164);
}

// ---------------------------------------------------------------------------
// Golden test helper entry point
// ---------------------------------------------------------------------------

/// Helper for future golden-test integration: parse raw payload bytes and
/// return Ok on success.  The caller supplies a byte slice from a real
/// `.mbshare` file or a captured binary payload.
///
/// Usage (in a future test with real data):
/// ```ignore
/// let real_bytes = include_bytes!("../../testdata/real_config.mbshare");
/// golden_parse_mbshare(real_bytes).expect("golden parse");
/// ```
#[cfg(test)]
#[allow(dead_code)]
pub(super) fn golden_parse_mbshare(
    mbshare_bytes: &[u8],
) -> Result<SharedConfig, super::wire::SharedConfigError> {
    from_mbshare_bytes(mbshare_bytes)
}

/// Helper: parse raw (uncompressed) payload bytes.
#[cfg(test)]
#[allow(dead_code)]
pub(super) fn golden_parse_payload(
    raw: &[u8],
) -> Result<SharedConfig, super::wire::SharedConfigError> {
    parse_payload(raw)
}

// ---------------------------------------------------------------------------
// Absorb
// ---------------------------------------------------------------------------

#[test]
fn absorb_client_settings() {
    let mut cfg = SharedConfig::default();
    cfg.absorb_client_settings_raw(
        10,                                // x_sell
        20,                                // x_sell_scalp
        true,                              // x_tmode
        true,                              // fixed_sell_mode
        5.5,                               // fixed_sell_price
        -3.0,                              // price_drop_level
        -1.5,                              // trailing_drop
        true,                              // trailing_stop
        8.0,                               // g_take_profit
        true,                              // use_g_take_profit
        true,                              // panic_if_price_drop
        true,                              // buy_iceberg
        false,                             // sell_iceberg
        false,                             // sign_orders
        "DOGE",                            // coins_black_list_text
        true,                              // use_coins_black_list
        true,                              // use_manual_strategy
        true,                              // free_position_check
        15,                                // vol_drop_level
        true,                              // use_stop_market
        &[2.0, 4.0, 6.0, 8.0, 10.0, 50.0], // s_price
        3,                                 // sb_num
        1,                                 // join_sell_kind
    );

    assert_eq!(cfg.trading.x_sell, 10);
    assert!(cfg.trading.trailing_stop);
    assert!((cfg.trading.g_take_profit - 8.0).abs() < 1e-10);
    assert_eq!(cfg.trading.coins_black_list_text, "DOGE");
    assert_eq!(cfg.ui.hotkeys_config.s_price[0], 2.0);
    assert_eq!(cfg.ui.hotkeys_config.sb_num, 3);
    // Not part of ClientSettingsCommand: absorb must leave the base value.
    assert_eq!(
        cfg.signals.full_screen_prevent_signals,
        SharedConfig::default().signals.full_screen_prevent_signals
    );

    // Verify roundtrip after absorb.
    let bytes = serialize_payload(&cfg).unwrap();
    let parsed = parse_payload(&bytes).unwrap();
    assert_eq!(parsed.trading.x_sell, 10);
    assert!(parsed.trading.trailing_stop);
}

#[test]
fn absorb_lev_manage() {
    let mut cfg = SharedConfig::default();
    cfg.absorb_lev_manage_raw(
        true,             // auto_max_order
        false,            // auto_lev_up
        true,             // auto_isolated
        true,             // auto_cross
        true,             // auto_fix_lev
        20,               // fix_lev
        false,            // tlg_report
        "1k def 5k BTC*", // lev_control
    );

    assert!(cfg.trading.auto_manage_lev.auto_max_order);
    assert_eq!(cfg.trading.auto_manage_lev.fix_lev, 20);
    assert_eq!(cfg.trading.auto_lev_control, "1k def 5k BTC*");
}

#[test]
fn serializer_rejects_a_section_over_the_protocol_bound() {
    let mut cfg = SharedConfig::default();
    cfg.signals.unknown_tail = vec![0; super::wire::MAX_BLOCK_SIZE as usize];
    let err = serialize_payload(&cfg).expect_err("oversized section must be rejected");
    assert!(err.to_string().contains("block too large"));
}
