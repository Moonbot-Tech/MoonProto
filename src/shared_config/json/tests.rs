//! Regression checks for exact leaf edits, opaque metadata and catalog-driven schemas.

use super::*;
use serde_json::json;

/// A changed field must not reset an unrelated setting or future-version section bytes.
#[test]
fn patch_preserves_unmentioned_fields_and_metadata() {
    let mut original = SharedConfig::default();
    original.trading.fixed_trade_balance = 123.45678912345678;
    original.signals.unknown_tail = vec![21, 1, 255];
    original.ui.unknown_tail = vec![43, 0, 19];
    let mut expected = original.clone();
    expected.trading.log_level = 3;
    let actual = original.patch_json(&BTreeMap::from([("trading.log_level".into(), json!(3))])).unwrap();
    assert_eq!(super::super::serialize_payload(&actual).unwrap(), super::super::serialize_payload(&expected).unwrap());
    assert_eq!(original.trading.log_level, SharedConfig::default().trading.log_level);
    assert!(actual.json_values().unwrap()["signals"].get("unknown_tail").is_none());
}

/// Reject unknown names, object replacement, service fields and coerced numeric values atomically.
#[test]
fn rejects_invalid_patch_without_changing_source() {
    let original = SharedConfig::default();
    let before = super::super::serialize_payload(&original).unwrap();
    for (path, value) in [
        ("trading.typo", json!(1)), ("trading", json!({"log_level":3})),
        ("config_version", json!(1)), ("signals.unknown_tail", json!([1])),
        ("ui.hotkeys_config.ver", json!(2)), ("trading.log_level", json!("3")),
        ("trading.log_level", json!(1u64 << 40)), ("trading.log_level", json!(1.5)),
        ("trading.fixed_trade_balance", Value::Null),
    ] {
        assert!(original.patch_json(&BTreeMap::from([("signals.signal_sound".into(), json!(42)), (path.into(), value)])).is_err(), "{path}");
        assert_eq!(super::super::serialize_payload(&original).unwrap(), before);
    }
}

/// Oversized arrays are part of the real wire catalog and retain their fixed-length contract.
#[test]
fn fixed_arrays_validate_length_and_roundtrip() {
    let original = SharedConfig::default();
    let mut visibility = vec![true; 41];
    visibility[40] = false;
    let changed = original.patch_json(&BTreeMap::from([("ui.markets_table_config.col_vis".into(), json!(visibility))])).unwrap();
    assert!(!changed.ui.markets_table_config.col_vis[40]);
    assert!(original.patch_json(&BTreeMap::from([("ui.markets_table_config.col_vis".into(), json!([false]))])).is_err());
    let schema = SharedConfig::json_schema();
    let column = &schema["$defs"]["MarketsTableConfig"]["properties"]["col_vis"];
    assert_eq!(column["minItems"], 41);
    assert_eq!(column["maxItems"], 41);
    assert_eq!(schema["properties"]["config_version"]["readOnly"], true);
    assert!(schema["$defs"]["SignalsSection"]["properties"].get("unknown_tail").is_none());
}
