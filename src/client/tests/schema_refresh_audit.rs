use super::*;

fn pending_comment(d: &mut EventDispatcher) -> StrategySnapshot {
    let mut desired = d.strategy_snapshot(ID).unwrap().clone();
    desired.last_date += 10;
    desired.fields.insert("Comment", FieldValue::String("changed only comment".into()));
    d.stage_local_strategies_owned(vec![desired.clone()], crate::MoonTime::now(), Instant::now() + Duration::from_secs(60));
    desired
}

#[test]
fn comment_between_schema_and_full_keeps_unknown_value() {
    let (mut d, client) = old_client();
    dispatch(&mut d, &client, &schema_packet(true));
    let desired = pending_comment(&mut d);
    let reply = d.local_strategy_diff_reply(&[ID]).unwrap();
    export_wire("schema-first-edit.deflate", &reply.data);
    let batch = parse_strategy_batch(&reply.data).unwrap();
    assert!(batch.names.iter().any(|n| n == "MinLeverage"));
    assert!(!batch.strategies[0].fields.knows_field("MinLeverage"));
    assert_eq!(batch.strategies[0].fields.get("Comment"), desired.fields.get("Comment"));
    let mut events = Vec::new();
    d.dispatch_into(Command::Strat, &full_packet(true, &[row(ID, DATE + 10, 5, "changed only comment")]), 0, &mut events);
    assert!(events.iter().any(|e| matches!(e, Event::Strat(StratEvent::EditConfirmed { strategy_ids }) if strategy_ids == &[ID])));
    assert!(d.strats().strategy_edit(ID).is_none());
    assert_eq!(effective_leverage(&d, ID), 5);
}

#[test]
fn mixed_batch_keeps_unknown_and_applies_explicit_value_or_default() {
    for leverage in [7, 1] {
        let (mut d, client) = old_client();
        let desired = pending_comment(&mut d);
        dispatch(&mut d, &client, &schema_packet(true));
        d.stage_local_strategies_owned(vec![desired, row(42, DATE + 10, leverage, "second")],
            crate::MoonTime::now(), Instant::now() + Duration::from_secs(60));
        let reply = d.local_strategy_diff_reply(&[ID, 42]).unwrap();
        export_wire(&format!("mixed-{leverage}.deflate"), &reply.data);
        let batch = parse_strategy_batch(&reply.data).unwrap();
        assert!(!batch.strategies[0].fields.knows_field("MinLeverage"));
        assert!(batch.strategies[1].fields.knows_field("MinLeverage"));
        assert_eq!(batch.strategies[1].fields.get("MinLeverage"),
            (leverage != 1).then_some(FieldValue::Int32(leverage)).as_ref());
    }
}

#[test]
fn pending_projection_survives_restage_after_full_and_automatic_reply() {
    let (mut d, client) = old_client();
    let desired = pending_comment(&mut d);
    refresh(&mut d, &client, true, 5, DATE);
    // Terminal overlay_pending_edits uses the exact desired row again.
    let pending = d.strats().strategy_edit(ID).unwrap().desired().clone();
    assert_eq!(pending, desired);
    d.stage_local_strategies_owned(vec![pending, row(42, DATE + 10, 7, "second")],
        crate::MoonTime::now(), Instant::now() + Duration::from_secs(60));
    let diff = d.local_strategy_diff_reply(&[ID, 42]).unwrap();
    export_wire("pending-restaged.deflate", &diff.data);
    let full = d.local_strategy_snapshot_reply().unwrap();
    export_wire("pending-full.deflate", &full.data);
    for data in [&diff.data, &full.data] {
        let batch = parse_strategy_batch(data).unwrap();
        assert!(!batch.strategies[0].fields.knows_field("MinLeverage"));
        assert!(batch.strategies[0].fields.get("MinLeverage").is_none());
        assert_eq!(batch.strategies[1].fields.get("MinLeverage"), Some(&FieldValue::Int32(7)));
    }
}

#[test]
fn known_omitted_default_still_resets_receiver() {
    let (mut d, client) = old_client();
    refresh(&mut d, &client, true, 1, DATE);
    assert!(d.strategy_snapshot(ID).unwrap().fields.knows_field("MinLeverage"));
    pending_comment(&mut d);
    let reply = d.local_strategy_diff_reply(&[ID]).unwrap();
    export_wire("known-default.deflate", &reply.data);
    let batch = parse_strategy_batch(&reply.data).unwrap();
    assert!(batch.strategies[0].fields.knows_field("MinLeverage"));
    assert!(batch.strategies[0].fields.get("MinLeverage").is_none());
}

#[test]
fn keep_marker_survives_decode_and_reencode() {
    let (mut d, client) = old_client();
    dispatch(&mut d, &client, &schema_packet(true));
    pending_comment(&mut d);
    let reply = d.local_strategy_diff_reply(&[ID]).unwrap();
    let batch = parse_strategy_batch(&reply.data).unwrap();
    let mut writer = StrategyBatchBuilder::new(d.strats().strategy_schema().unwrap());
    writer.write_strategies(batch.strategies.iter(), |_| None);
    let data = writer.finalize();
    export_wire("keep-roundtrip.deflate", &data);
    assert!(!parse_strategy_batch(&data).unwrap().strategies[0].fields.knows_field("MinLeverage"));
}

#[test]
fn later_kind_cannot_default_an_earlier_unknown_field() {
    let (d, _) = old_client();
    let mut schema = StrategySchema::parse_compressed(&schema_blob(true)).unwrap();
    let leverage = schema.fields.iter_mut().find(|f| f.name == "MinLeverage").unwrap();
    leverage.visible_kind_ordinals = vec![StrategyKind::MANUAL.ordinal()];
    leverage.visible_kind_mask = crate::commands::strategy_schema::visible_kind_mask(&leverage.visible_kind_ordinals);
    let mut first = d.strategy_snapshot(ID).unwrap().clone();
    first.last_date += 10;
    first.fields.insert("Comment", FieldValue::String("changed only comment".into()));
    let mut second = row(42, DATE + 10, 7, "second");
    second.kind = StrategyKind::MANUAL.ordinal();
    let rows = [first, second];
    let mut writer = StrategyBatchBuilder::new(&schema);
    writer.write_strategies(rows.iter(), |_| None);
    let data = writer.finalize();
    export_wire("mixed-kinds.deflate", &data);
    let batch = parse_strategy_batch(&data).unwrap();
    assert!(!batch.strategies[0].fields.knows_field("MinLeverage"));
    assert_eq!(batch.strategies[1].fields.get("MinLeverage"), Some(&FieldValue::Int32(7)));
}

#[test]
fn keep_does_not_hide_an_explicit_leverage_rejection() {
    let (mut d, client) = old_client();
    dispatch(&mut d, &client, &schema_packet(true));
    d.stage_local_strategies_owned(vec![row(ID, DATE + 10, 9, "changed only comment")],
        crate::MoonTime::now(), Instant::now() + Duration::from_secs(60));
    let mut events = Vec::new();
    d.dispatch_into(Command::Strat, &full_packet(true, &[row(ID, DATE + 10, 5, "changed only comment")]), 0, &mut events);
    assert!(events.iter().any(|e| matches!(e, Event::Strat(StratEvent::EditAdjusted { strategy_ids }) if strategy_ids == &[ID])));
}

#[test]
fn automatic_full_restores_values_learned_after_an_acknowledged_edit() {
    let (mut d, client) = old_client();
    pending_comment(&mut d);
    dispatch(&mut d, &client, &full_packet(false, &[row(ID, DATE + 10, 5, "changed only comment")]));
    assert!(d.strats().strategy_edit(ID).is_none());
    refresh(&mut d, &client, true, 5, DATE + 10);
    let reply = d.local_strategy_snapshot_reply().unwrap();
    let batch = parse_strategy_batch(&reply.data).unwrap();
    assert_eq!(batch.strategies[0].fields.get("MinLeverage"), Some(&FieldValue::Int32(5)));
}

#[test]
fn full_before_new_field_schema_requests_and_applies_fresh_values() {
    let (mut d, client) = old_client();
    let mut raw = vec![1, 1, StrategyKind::TELEGRAM.ordinal()];
    str8(&mut raw, "Telegram");
    raw.extend_from_slice(&1u16.to_le_bytes());
    str8(&mut raw, "Comment");
    raw.extend_from_slice(&[TID_STRING, 0, 1]);
    let mut zip = DeflateEncoder::new(Vec::new(), Compression::default());
    zip.write_all(&raw).unwrap();
    let blob = zip.finish().unwrap();
    let mut packet = schema_packet(false);
    packet.truncate(15);
    packet[11..15].copy_from_slice(&(blob.len() as u32).to_le_bytes());
    packet.extend(blob);
    dispatch(&mut d, &client, &packet);
    dispatch(&mut d, &client, &full_packet(true, &[row(ID, DATE, 5, "original")]));
    assert!(!d.strategy_snapshot(ID).unwrap().fields.knows_field("MinLeverage"));
    let actions = dispatch(&mut d, &client, &schema_packet(true));
    assert!(actions.iter().any(|a| matches!(a, ActiveAction::SendStrategySnapshot { .. })));
    dispatch(&mut d, &client, &full_packet(true, &[row(ID, DATE, 5, "original")]));
    assert_eq!(effective_leverage(&d, ID), 5);
}

#[test]
fn visibility_only_schema_does_not_request_an_extra_full() {
    let (mut d, client) = old_client();
    let actions = dispatch(&mut d, &client, &schema_packet(true));
    assert!(!actions.iter().any(|a| matches!(a, ActiveAction::SendStrategySnapshot { .. })));
}
