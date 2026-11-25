use super::*;

fn wait_exit_policy(cfg: &FireConfig, a: &mut Session, b: &mut Session, value: bool, before: [u64; 2]) {
    assert!(pump_pair_until_sessions(a, b, cfg.connect_timeout, "exit policy broadcast", |a, b| {
        [a, b].into_iter().zip(before).all(|(session, count)| {
            let state = session.state_snapshot();
            session.stats.lock().unwrap().settings_events > count
                && state.settings().client_settings.as_ref().is_some_and(|settings|
                    settings.auto_start_config2().cancel_buys_on_exit == value)
        })
    }), "both clients must receive the exit policy and retain it for the next full edit");
    for session in [a, b] {
        assert_eq!(session.client.settings().build_shared_config().unwrap().trading.auto_start_2.cancel_buys_on_exit,
            Some(value), "the next full edit must retain the echoed policy");
    }
}

pub(super) fn run_exit_policy_gate(cfg: &FireConfig, a: &mut Session, b: &mut Session) {
    assert!(pump_pair_until_sessions(a, b, cfg.connect_timeout, "N2 full settings", |a, b|
        retained_shared_config(a).is_some() && retained_shared_config(b).is_some()));
    let original = retained_shared_config(a).unwrap().trading.auto_start_2.cancel_buys_on_exit
        .expect("updated N2 core must report cancel_buys_on_exit");
    let mutation = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        for full in [false, true] {
            for value in [!original, original] {
                let before = [a.stats.lock().unwrap().settings_events, b.stats.lock().unwrap().settings_events];
                if full {
                    let mut config = a.client.settings().build_shared_config().unwrap();
                    config.trading.auto_start_2.cancel_buys_on_exit = Some(value);
                    a.client.settings().send_shared_config(&config).unwrap();
                } else {
                    let mut settings = request_settings_until(a, cfg.wait);
                    settings.update_auto_start_config2(|config| config.cancel_buys_on_exit = value);
                    a.client.settings().send(settings).unwrap();
                }
                wait_exit_policy(cfg, a, b, value, before);
                println!("OK: N2 exit policy full={full} value={value}, echoed to two clients");
            }
        }

        // An old full snapshot must not turn an enabled core policy off.
        let mut settings = request_settings_until(a, cfg.wait);
        settings.update_auto_start_config2(|config| config.cancel_buys_on_exit = true);
        let before = [a.stats.lock().unwrap().settings_events, b.stats.lock().unwrap().settings_events];
        a.client.settings().send(settings).unwrap();
        wait_exit_policy(cfg, a, b, true, before);
        let mut legacy = a.client.settings().build_shared_config().unwrap();
        legacy.trading.auto_start_2.cancel_buys_on_exit = None;
        let before = [a.stats.lock().unwrap().settings_events, b.stats.lock().unwrap().settings_events];
        let full_before = [a.stats.lock().unwrap().shared_config_events, b.stats.lock().unwrap().shared_config_events];
        a.client.settings().send_shared_config(&legacy).unwrap();
        wait_exit_policy(cfg, a, b, true, before);
        assert!(pump_pair_until_sessions(a, b, cfg.connect_timeout, "legacy import full echo", |a, b|
            [a, b].into_iter().zip(full_before).all(|(session, count)|
                session.stats.lock().unwrap().shared_config_events > count
                    && session.state_snapshot().settings().shared_config.as_ref().is_some_and(|config|
                        config.trading.auto_start_2.cancel_buys_on_exit == Some(true)))),
            "wait for a new full core snapshot, not just the already-enabled compact state");
        println!("OK: importing a legacy full snapshot preserves enabled exit policy");
    }));

    let mut settings = request_settings_until(a, cfg.wait);
    settings.update_auto_start_config2(|config| config.cancel_buys_on_exit = original);
    let before = [a.stats.lock().unwrap().settings_events, b.stats.lock().unwrap().settings_events];
    a.client.settings().send(settings).unwrap();
    wait_exit_policy(cfg, a, b, original, before);
    println!("OK: N2 exit policy restored to {original}; core shutdown was not requested");
    if let Err(error) = mutation { std::panic::resume_unwind(error); }
}

fn run_chart_text_reconnect_gate(cfg: &FireConfig, a: &mut Session, b: &mut Session, err_emu: &mut ErrEmuGuard) {
    a.client.chart_text().set_visible_market(&cfg.market, true, true).unwrap();
    assert!(pump_pair_until_sessions(a, b, cfg.connect_timeout, "initial chart text", |a, _|
        a.state_snapshot().chart_text().get(&cfg.market).is_some()));
    run_high_loss_simple_ops_gate(a, b, err_emu, cfg.high_loss_timeout);
    err_emu.reset("N2 chart-text reconnect");
    let received = a.stats.lock().unwrap().chart_text_events;
    assert!(pump_pair_until_sessions(a, b, cfg.connect_timeout, "fresh chart text after reconnect", |a, _|
        a.stats.lock().unwrap().chart_text_events > received && a.state_snapshot().chart_text().get(&cfg.market).is_some()),
        "fresh text must arrive without another set_visible_market call");
    a.client.chart_text().clear_visible_market().unwrap();
    assert!(pump_pair_until_sessions(a, b, cfg.wait, "clear chart text", |a, _|
        a.state_snapshot().chart_text().get(&cfg.market).is_none()));
    println!("OK: fresh chart text after network reconnect without reselecting the chart; clear applied");
}

#[test]
#[ignore = "live updated N2 core required; temporarily changes and restores the core-exit policy"]
fn fire_test_n2_controls() {
    let _live_test_lock = firetest_live_test_lock();
    let cfg = FireConfig::load_required();
    assert!(cfg.allow_mutation, "N2 controls test temporarily changes the exit policy");
    let keys = parse_key_info(&cfg.key_b64).expect("invalid FireTest key").keys;
    let mut err_emu = ErrEmuGuard::set(0);
    let mut a = Session::connect("N2-A", &cfg, keys, None);
    let mut b = Session::connect("N2-B", &cfg, keys, None);
    let before = wait_ui_runtime_state(&cfg, &mut a, &mut b);
    let emu_mode = request_settings_until(&mut a, cfg.wait).emu_mode;
    let running = wait_strategy_runtime_state(&cfg, &mut a, &mut b);
    run_exit_policy_gate(&cfg, &mut a, &mut b);
    run_chart_text_reconnect_gate(&cfg, &mut a, &mut b, &mut err_emu);
    let after = wait_ui_runtime_state(&cfg, &mut a, &mut b);
    assert_eq!((after.is_started, after.auto_detect_active), (before.is_started, before.auto_detect_active));
    assert_eq!(request_settings_until(&mut a, cfg.wait).emu_mode, emu_mode);
    assert_eq!(wait_strategy_runtime_state(&cfg, &mut a, &mut b), running);
    assert_eq!(a.snapshot().parse_failed + b.snapshot().parse_failed, 0);
    println!("FIRETEST_N2_CONTROLS_PASS: policy restored, chart text recovered, no order commands sent");
}
