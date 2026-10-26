use super::*;

pub(super) fn run_settings_snapshot_check(cfg: &FireConfig, keys: ImportedKeys) {
    use std::io::Read;

    let client = MoonClient::connect(
        ClientConfig::new(&cfg.host, cfg.port, keys.master_key, keys.mac_key)
            .with_transport_mode(cfg.transport_mode)
            .with_client_id(rand::random()),
        ConnectConfig::new(InitConfig {
            subscribe_logs: false,
            ..InitConfig::default()
        })
        .with_connect_timeout(cfg.connect_timeout),
    )
    .expect("settings snapshot: connect");
    let deadline = Instant::now() + cfg.connect_timeout + cfg.wait;
    loop {
        if client
            .drain_lifecycle_events()
            .iter()
            .any(|e| matches!(e, LifecycleEvent::Ready))
        {
            break;
        }
        assert!(Instant::now() < deadline, "settings snapshot: startup timed out");
        std::thread::sleep(PUMP_SLICE);
    }
    let subscriptions = client.active_subscriptions();
    let directory = std::env::var_os("MOONPROTO_FIRETEST_SNAPSHOT_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(std::env::temp_dir);
    fs::create_dir_all(&directory).expect("settings snapshot: output directory");
    for attempt in 1..=2 {
        let path = directory.join(format!(
            "moonproto-firetest-snapshot-{:016x}.zip",
            rand::random::<u64>()
        ));
        client
            .settings()
            .download_settings_snapshot(&path)
            .expect("settings snapshot: queue");
        let deadline = Instant::now() + Duration::from_secs(310);
        let bytes = 'wait: loop {
            for event in client.drain_events() {
                match event {
                    Event::Settings(SettingsEvent::SettingsSnapshotDownloaded { path: actual, result })
                        if actual == path =>
                    {
                        break 'wait result.expect("settings snapshot: download and checksum");
                    }
                    Event::Settings(SettingsEvent::VersionUpdate { .. } | SettingsEvent::CoreUpdateResult { .. }) => {
                        panic!("settings snapshot unexpectedly triggered an update");
                    }
                    _ => {}
                }
            }
            assert!(Instant::now() < deadline, "settings snapshot: completion timed out");
            std::thread::sleep(PUMP_SLICE);
        };
        assert!(bytes > 4);
        assert_eq!(fs::metadata(&path).unwrap().len(), bytes);
        let mut signature = [0; 4];
        fs::File::open(&path).unwrap().read_exact(&mut signature).unwrap();
        assert_eq!(signature, *b"PK\x03\x04", "settings snapshot must be a ZIP");
        fs::remove_file(&path).expect("remove confidential FireTest snapshot");
        assert_eq!(client.active_subscriptions(), subscriptions);
        client
            .settings()
            .refresh()
            .expect("settings snapshot: source remains running");
        let deadline = Instant::now() + cfg.wait;
        loop {
            if client
                .drain_events()
                .iter()
                .any(|event| matches!(event, Event::Settings(SettingsEvent::ClientSettingsUpdated)))
            {
                break;
            }
            assert!(
                Instant::now() < deadline,
                "settings snapshot: source did not answer after download"
            );
            std::thread::sleep(PUMP_SLICE);
        }
        println!("OK: FIRETEST settings snapshot #{attempt}: {bytes} bytes, checksum verified, source alive");
    }
    client.disconnect().expect("settings snapshot: disconnect");
    client.wait_finished().expect("settings snapshot: stop runtime");
}

#[test]
#[ignore = "live Windows core with settings snapshot support required"]
fn fire_test_settings_snapshot() {
    let _guard = firetest_live_test_lock();
    let cfg = FireConfig::load_required();
    let keys = parse_key_info(&cfg.key_b64).expect("invalid FireTest key").keys;
    run_settings_snapshot_check(&cfg, keys);
}
