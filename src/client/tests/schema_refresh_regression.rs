//! Exchange-switch contracts. No socket, service, exchange account or clock sleeps.
//! Assertions describe the required result; failing cases stay red until the fix.

use super::*;
use crate::commands::registry::CURRENT_PROTO_CMD_VER;
use crate::commands::strat::{build_snapshot, build_snapshot_request, StratCommand};
use crate::commands::strategy_schema::StrategySchema;
use crate::commands::strategy_serializer::{
    parse_strategy_batch, FieldValue, StrategyBatchBuilder, StrategyFields, StrategyKind, StrategySnapshot, TID_BOOL,
    TID_INT32, TID_STRING,
};
use crate::events::{ActiveAction, ActiveDispatchContext, Event, EventDispatcher};
use crate::state::StratEvent;
use flate2::{write::DeflateEncoder, Compression};
use std::io::Write;

const ID: u64 = 41;
const DATE: u64 = 100;

fn str8(out: &mut Vec<u8>, text: &str) {
    out.push(u8::try_from(text.len()).unwrap());
    out.extend_from_slice(text.as_bytes());
}

/// All fields are known on both exchanges; only their visibility differs.
fn schema_blob(futures: bool) -> Vec<u8> {
    let mut raw = vec![1, 1, StrategyKind::TELEGRAM.ordinal()];
    str8(&mut raw, "Telegram");
    raw.extend_from_slice(&3u16.to_le_bytes());
    str8(&mut raw, "Comment");
    raw.extend_from_slice(&[TID_STRING, 0, 1]);
    str8(&mut raw, "MinLeverage");
    raw.extend_from_slice(&[TID_INT32, 0x40]);
    raw.extend_from_slice(&1i32.to_le_bytes());
    raw.push(u8::from(futures));
    str8(&mut raw, "AutoSell");
    raw.extend_from_slice(&[TID_BOOL, 0x41, 1, u8::from(!futures)]);
    let mut zip = DeflateEncoder::new(Vec::new(), Compression::default());
    zip.write_all(&raw).unwrap();
    zip.finish().unwrap()
}

fn schema_packet(futures: bool) -> Vec<u8> {
    let blob = schema_blob(futures);
    let mut packet = vec![8];
    packet.extend_from_slice(&CURRENT_PROTO_CMD_VER.to_le_bytes());
    packet.extend_from_slice(&77u64.to_le_bytes());
    packet.extend_from_slice(&(blob.len() as u32).to_le_bytes());
    packet.extend_from_slice(&blob);
    packet
}

fn row(id: u64, date: u64, leverage: i32, comment: &str) -> StrategySnapshot {
    let mut fields = StrategyFields::new();
    fields.insert("Comment", FieldValue::String(comment.into()));
    fields.insert("MinLeverage", FieldValue::Int32(leverage));
    StrategySnapshot::new(id, 12, date, false, StrategyKind::TELEGRAM, "Group", fields)
}

fn full_packet(futures: bool, rows: &[StrategySnapshot]) -> Vec<u8> {
    let schema = StrategySchema::parse_compressed(&schema_blob(futures)).unwrap();
    let mut writer = StrategyBatchBuilder::new(&schema);
    for row in rows {
        writer.write_strategy(row);
    }
    build_snapshot(88, 100, DATE, true, &writer.finalize(), 0, 0)
}

fn client(ready: bool) -> Client {
    let mut client = Client::new(ClientConfig::new("127.0.0.1", 3000, [0; 16], [0; 16]));
    client.testing_set_domain_ready(ready);
    client.authorized = true;
    client.server_token = 1;
    client.peer_app_token = 10;
    client
}

fn dispatch(d: &mut EventDispatcher, client: &Client, packet: &[u8]) -> Vec<ActiveAction> {
    let mut events = Vec::new();
    let mut actions = Vec::new();
    let ctx = ActiveDispatchContext::from_client(client);
    d.dispatch_into_active_actions(Command::Strat, packet, 1000, &mut events, &ctx, &mut actions);
    assert!(
        !events
            .iter()
            .any(|event| matches!(event, Event::Strat(StratEvent::SchemaParseFailed { .. }))),
        "invalid schema fixture"
    );
    actions
}

fn old_client() -> (EventDispatcher, Client) {
    let mut d = EventDispatcher::new();
    let client = client(true);
    dispatch(&mut d, &client, &schema_packet(false));
    dispatch(&mut d, &client, &full_packet(false, &[row(ID, DATE, 5, "original")]));
    let retained = d.strategy_snapshot(ID).unwrap();
    assert!(
        retained.fields.get("MinLeverage").is_none(),
        "fixture must reproduce the old omission"
    );
    (d, client)
}

fn refresh(d: &mut EventDispatcher, client: &Client, schema_first: bool, value: i32, date: u64) {
    let schema = schema_packet(true);
    let full = full_packet(true, &[row(ID, date, value, "original")]);
    for packet in if schema_first {
        [&schema, &full]
    } else {
        [&full, &schema]
    } {
        dispatch(d, client, packet);
    }
}

fn effective_leverage(d: &EventDispatcher, id: u64) -> i32 {
    let row = d.strategy_snapshot(id).unwrap();
    let field = d.strats().strategy_schema().unwrap().field("MinLeverage").unwrap();
    match row.fields.get("MinLeverage").or(field.default_value.as_ref()).unwrap() {
        FieldValue::Int32(value) => *value,
        other => panic!("unexpected leverage type: {other:?}"),
    }
}

fn drain(client: &Client) -> Vec<SendItem> {
    let (mut sliced, high, low) = client.take_send_queues_for_test();
    sliced.extend(high);
    sliced.extend(low);
    sliced
}

fn export_wire(name: &str, data: &[u8]) {
    if let Some(dir) = std::env::var_os("MOONPROTO_SCHEMA_WIRE_DIR") {
        std::fs::write(std::path::Path::new(&dir).join(name), data).unwrap();
    }
}

#[test]
fn cached_schema_is_refreshed_on_ready_snapshot_request() {
    let (mut d, client) = old_client();
    let actions = dispatch(&mut d, &client, &build_snapshot_request(90));
    assert!(
        actions.iter().any(|a| matches!(a, ActiveAction::RequestStrategySchema)),
        "an already cached schema must not suppress refresh"
    );
}

#[test]
fn empty_strategy_list_still_refreshes_cached_schema() {
    let mut d = EventDispatcher::new();
    let client = client(true);
    dispatch(&mut d, &client, &schema_packet(false));
    let actions = dispatch(&mut d, &client, &build_snapshot_request(91));
    assert!(actions.iter().any(|a| matches!(a, ActiveAction::RequestStrategySchema)));
}

#[test]
fn changed_core_before_post_init_refreshes_the_previous_schema() {
    let mut d = EventDispatcher::new();
    let mut client = client(false);
    dispatch(&mut d, &client, &schema_packet(false));
    client.server_token = 2;
    client.peer_app_token = 20;
    dispatch(&mut d, &client, &build_snapshot_request(92));
    client.testing_set_domain_ready(true);
    super::super::init::send_post_init_resync(
        &mut client,
        &mut d,
        &InitConfig::default(),
        &mut super::super::init::InitResult::default(),
    );
    assert!(
        drain(&client).iter().any(|item| item.cmd == Command::Strat.to_byte()
            && matches!(
                StratCommand::parse(&item.data),
                Some(StratCommand::SchemaRequest { .. })
            )),
        "post-init must not retain the previous core's schema"
    );
}

#[test]
fn pre_init_request_does_not_open_the_domain_gate() {
    let mut d = EventDispatcher::new();
    let client = client(false);
    assert!(dispatch(&mut d, &client, &build_snapshot_request(93)).is_empty());
    assert!(!client.is_domain_ready());
}

#[test]
fn same_revision_values_arriving_after_schema_are_recovered() {
    let (mut d, client) = old_client();
    refresh(&mut d, &client, true, 5, DATE);
    assert_eq!(effective_leverage(&d, ID), 5);
}

#[test]
fn same_revision_values_arriving_before_schema_are_recovered() {
    let (mut d, client) = old_client();
    refresh(&mut d, &client, false, 5, DATE);
    assert_eq!(effective_leverage(&d, ID), 5);
}

#[test]
fn newer_revision_already_recovers_values() {
    let (mut d, client) = old_client();
    refresh(&mut d, &client, true, 5, DATE + 1);
    assert_eq!(effective_leverage(&d, ID), 5);
}

#[test]
fn default_value_remains_default_when_core_omits_it() {
    let (mut d, client) = old_client();
    refresh(&mut d, &client, false, 1, DATE);
    assert_eq!(effective_leverage(&d, ID), 1);
}

#[test]
fn older_full_cannot_undo_a_newer_confirmed_value() {
    let (mut d, client) = old_client();
    refresh(&mut d, &client, true, 7, DATE + 10);
    dispatch(&mut d, &client, &full_packet(true, &[row(ID, DATE, 3, "stale")]));
    assert_eq!(effective_leverage(&d, ID), 7);
}

#[test]
fn identical_schema_and_full_do_not_change_values_or_edit_dates() {
    let (mut d, client) = old_client();
    refresh(&mut d, &client, true, 7, DATE + 10);
    dispatch(&mut d, &client, &schema_packet(true));
    dispatch(
        &mut d,
        &client,
        &full_packet(true, &[row(ID, DATE + 10, 7, "original")]),
    );
    assert_eq!(effective_leverage(&d, ID), 7);
    assert_eq!(d.strategy_snapshot(ID).unwrap().last_date, DATE + 10);
    assert!(d.strats().strategy_edit(ID).is_none());
}

#[test]
fn own_strategies_and_pending_edits_survive_schema_and_full() {
    let (mut d, client) = old_client();
    let mut desired = d.strategy_snapshot(ID).unwrap().clone();
    desired.last_date = DATE + 10;
    desired.fields.insert("Comment", FieldValue::String("pending".into()));
    d.stage_local_strategies_owned(
        vec![desired, row(99, DATE + 1, 1, "Own")],
        crate::MoonTime::now(),
        Instant::now() + Duration::from_secs(60),
    );
    refresh(&mut d, &client, true, 5, DATE);
    assert_eq!(
        d.strats().strategy_edit(ID).unwrap().desired().fields.get("Comment"),
        Some(&FieldValue::String("pending".into()))
    );
    assert!(
        d.strats().strategy_edit(99).is_some(),
        "automatic refresh must retain Own"
    );
}

#[test]
fn refreshing_unknown_field_values_does_not_erase_a_pending_comment() {
    let (mut d, client) = old_client();
    let mut desired = d.strategy_snapshot(ID).unwrap().clone();
    desired.last_date = DATE + 10;
    desired.fields.insert("Comment", FieldValue::String("pending".into()));
    d.stage_local_strategies_owned(
        vec![desired],
        crate::MoonTime::now(),
        Instant::now() + Duration::from_secs(60),
    );
    refresh(&mut d, &client, true, 5, DATE);
    assert_eq!(effective_leverage(&d, ID), 5, "confirmed value must be completed too");
    assert_eq!(
        d.strats().strategy_edit(ID).unwrap().desired().fields.get("Comment"),
        Some(&FieldValue::String("pending".into()))
    );
}

#[test]
fn editing_only_comment_does_not_reset_the_newly_visible_leverage() {
    let (mut d, client) = old_client();
    refresh(&mut d, &client, true, 5, DATE);
    let mut edited = d.strategy_snapshot(ID).unwrap().clone();
    edited.last_date += 1;
    edited
        .fields
        .insert("Comment", FieldValue::String("changed only comment".into()));
    d.stage_local_strategies_owned(
        vec![edited],
        crate::MoonTime::now(),
        Instant::now() + Duration::from_secs(60),
    );
    let reply = d.local_strategy_diff_reply(&[ID]).unwrap();
    export_wire("comment-edit.deflate", &reply.data);
    let batch = parse_strategy_batch(&reply.data).unwrap();
    assert_eq!(
        batch.strategies[0].fields.get("MinLeverage"),
        Some(&FieldValue::Int32(5)),
        "the core treats a declared but absent field as its default"
    );
}

#[test]
fn visibility_changes_both_directions_and_clients_remain_independent() {
    let (mut a, client_a) = old_client();
    let (b, _) = old_client();
    for futures in [true, false, true] {
        dispatch(&mut a, &client_a, &schema_packet(futures));
        let schema = a.strats().strategy_schema().unwrap();
        assert_eq!(
            schema
                .field("MinLeverage")
                .unwrap()
                .visible_for_strategy_kind(StrategyKind::TELEGRAM),
            futures
        );
        assert_eq!(
            schema
                .field("AutoSell")
                .unwrap()
                .visible_for_strategy_kind(StrategyKind::TELEGRAM),
            !futures
        );
        assert!(!b
            .strats()
            .strategy_schema()
            .unwrap()
            .field("MinLeverage")
            .unwrap()
            .visible_for_strategy_kind(StrategyKind::TELEGRAM));
    }
}

#[test]
fn malformed_full_does_not_discard_retained_values() {
    let (mut d, client) = old_client();
    refresh(&mut d, &client, true, 7, DATE + 1);
    dispatch(&mut d, &client, &build_snapshot(99, 999, 999, true, &[255], 0, 0));
    assert_eq!(effective_leverage(&d, ID), 7);
}

#[test]
fn malformed_schema_keeps_the_previous_schema_and_values() {
    let (mut d, client) = old_client();
    refresh(&mut d, &client, true, 7, DATE + 1);
    let revision = d.strats().strategy_schema_revision();
    let mut packet = schema_packet(false);
    packet.truncate(15);
    packet[11..15].copy_from_slice(&1u32.to_le_bytes());
    packet.push(255);
    let mut events = Vec::new();
    d.dispatch_into(Command::Strat, &packet, 1000, &mut events);
    assert!(events
        .iter()
        .any(|event| matches!(event, Event::Strat(StratEvent::SchemaParseFailed { .. }))));
    assert_eq!(d.strats().strategy_schema_revision(), revision);
    assert_eq!(effective_leverage(&d, ID), 7);
}

#[test]
fn newer_local_leverage_edit_is_not_overwritten_by_the_core_refresh() {
    let (mut d, client) = old_client();
    dispatch(&mut d, &client, &schema_packet(true));
    d.stage_local_strategies_owned(
        vec![row(ID, DATE + 10, 9, "pending")],
        crate::MoonTime::now(),
        Instant::now() + Duration::from_secs(60),
    );
    dispatch(&mut d, &client, &full_packet(true, &[row(ID, DATE, 5, "original")]));
    let pending = d.strats().strategy_edit(ID).unwrap().desired();
    assert_eq!(pending.fields.get("MinLeverage"), Some(&FieldValue::Int32(9)));
    assert_eq!(pending.last_date, DATE + 10);
}

#[test]
fn ordinary_outgoing_edit_carries_nondefault_leverage() {
    let schema = StrategySchema::parse_compressed(&schema_blob(true)).unwrap();
    let mut writer = StrategyBatchBuilder::new(&schema);
    writer.write_strategy(&row(ID, DATE + 1, 5, "changed only comment"));
    let data = writer.finalize();
    export_wire("ordinary-edit.deflate", &data);
    assert_eq!(
        parse_strategy_batch(&data).unwrap().strategies[0]
            .fields
            .get("MinLeverage"),
        Some(&FieldValue::Int32(5))
    );
}

#[test]
fn old_projection_leaves_hidden_fields_out_of_the_dictionary() {
    let schema = StrategySchema::parse_compressed(&schema_blob(false)).unwrap();
    let mut writer = StrategyBatchBuilder::new(&schema);
    writer.write_strategy(&row(ID, DATE + 1, 5, "changed only comment"));
    let data = writer.finalize();
    export_wire("old-projection-edit.deflate", &data);
    assert!(parse_strategy_batch(&data).unwrap().strategies[0]
        .fields
        .get("MinLeverage")
        .is_none());
}

#[path = "schema_refresh_audit.rs"]
mod audit;
