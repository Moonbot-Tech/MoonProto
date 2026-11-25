use super::*;

#[test]
fn entry_price_correction_reaches_the_ui_without_changing_order_state() {
    for (requested, min_price, max_price) in [(12.675, 13.0, 0.0), (102.0, 0.0, 100.0)] {
        let mut raw = vec![64];
        raw.extend_from_slice(&CURRENT_PROTO_CMD_VER.to_le_bytes());
        raw.extend_from_slice(&42u64.to_le_bytes());
        write_string(&mut raw, "BITO");
        for price in [requested, min_price, max_price] { raw.extend_from_slice(&f64::to_le_bytes(price)); }
        let mut d = EventDispatcher::new();
        let before = d.snapshot();
        let events = d.dispatch(Command::UI, &raw, 1000);
        assert!(matches!(events.as_slice(), [Event::EntryPriceCorrection(notice)]
            if notice.market_name == "BITO" && notice.requested_price == requested
                && notice.min_price == min_price && notice.max_price == max_price));
        assert_eq!(d.snapshot().orders().len(), before.orders().len());
        for size in 11..raw.len() {
            assert!(crate::commands::ui::UICommand::parse(&raw[..size]).is_none());
        }
        let descriptor = crate::commands::registry::find_descriptor(Command::UI, 64).unwrap();
        assert_eq!(descriptor.max_retries, 0);
    }
}

#[test]
fn chart_text_clears_old_session_rows_and_accepts_restored_subscription() {
    use crate::commands::ui::{ChartTextStateCommand, ChartTextSnapshotCommand, build_chart_text_snapshot_for_test};
    let mut client = crate::client::Client::new(dummy_client_cfg());
    client.testing_set_domain_ready(true);
    client.ui_chart_text_state(&ChartTextStateCommand::new("ETHUSDT", true, true));
    let raw = build_chart_text_snapshot_for_test(&ChartTextSnapshotCommand {
        uid: 42, market_name: "ETHUSDT".into(), filter_lines: vec!["filter".into()], debug_lines: vec!["debug".into()],
    });
    let mut ctx = ActiveDispatchContext::from_client(&client);
    ctx.server_token = 1;
    let mut d = EventDispatcher::new();
    let mut out = Vec::new();
    d.dispatch_into_active_actions(Command::UI, &raw, 1000, &mut out, &ctx, &mut Vec::new());
    assert!(matches!(out.as_slice(), [Event::ChartText(_)]));
    let held = d.snapshot();
    ctx.server_token = 2;
    d.dispatch_into_active_actions(Command::from_byte(99), &[], 1100, &mut Vec::new(), &ctx, &mut Vec::new());
    assert!(d.snapshot().chart_text().get("ETHUSDT").is_none());
    assert!(held.chart_text().get("ETHUSDT").is_some());
    out.clear();
    d.dispatch_into_active_actions(Command::UI, &raw, 1200, &mut out, &ctx, &mut Vec::new());
    assert!(matches!(out.as_slice(), [Event::ChartText(_)]));
    // A rebuilt dispatcher adopts the same registry intent without a new UI action.
    let mut rebuilt = EventDispatcher::new();
    out.clear();
    rebuilt.dispatch_into_active_actions(Command::UI, &raw, 1300, &mut out, &ctx, &mut Vec::new());
    assert!(matches!(out.as_slice(), [Event::ChartText(_)]));
}
