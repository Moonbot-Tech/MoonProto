# Shared Configuration

MoonProto exposes the core's safe-share configuration as a typed
`SharedConfig`. It contains the full portable settings set used for sharing or
editing configuration, while `ClientSettingsCommand` remains the compact live
state used by frequently updated terminal controls.

## Availability

The runtime requests the full configuration in the background immediately
after initialization. `LifecycleEvent::Ready` does not wait for this large
snapshot. If no valid response has arrived, the runtime retries every five
seconds without blocking market data, orders, or other initialization work.

Read the retained value from `snapshot().settings().shared_config` or react to
`SettingsEvent::SharedConfigUpdated`. `build_shared_config()` returns
`MoonClientError::StateUnavailable` until a real snapshot has been received;
the active API never substitutes a default configuration for a live core.

## Editing Live Settings

Start every edit from the latest retained full snapshot:

```rust
let mut config = client.settings().build_shared_config()?;
config.trading.x_sell = 6;
config.visual.chart_time_scale = 90;
client.settings().send_shared_config(&config)?;
```

The send call transfers one complete safe-share snapshot. After the core
applies it, all connected clients receive fresh full and compact settings
snapshots. Wait for `SettingsEvent::SharedConfigUpdated` before treating the
edit as accepted.

If a compact settings or leverage update arrived after the latest full
snapshot, `build_shared_config()` overlays those newer values first. This keeps
an edit based on the newest state without allowing an older compact packet to
overwrite a newer full snapshot.

`refresh_shared_config()` is available for an explicit refresh; normal
applications do not need to call it during startup because the runtime already
maintains the background request.

## Files And Clipboard

Use the same typed model for portable safe-share data:

```rust
let file_bytes = moonproto::shared_config::to_mbshare_bytes(&config)?;
let from_file = moonproto::shared_config::from_mbshare_bytes(&file_bytes)?;

let clipboard = moonproto::shared_config::to_mbsc_string(&config)?;
let from_clipboard = moonproto::shared_config::from_mbsc_string(&clipboard)?;
```

The format contains portable settings only. API keys, credentials, local
runtime state, temporary blacklists, and emulator state are not included.
Parsing and serialization enforce the same section and compressed-payload
bounds as the live protocol path.
