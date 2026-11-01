# Watchdogs On The Core

`client.watchdogs()` manages the core's observers. Show the core name/address in
the existing settings panel, disable edits while disconnected, and call
`refresh()` when opening it or reconnecting. An absent snapshot means unavailable
on an older core. `available` reports a running controller; `can_edit` reports the
core's license for observer entries. The sender IP remains independently editable.

```rust,ignore
let state = client.watchdogs().state().ok_or("waiting for core state")?;
let mut settings = state.edit();
let entry = settings.entries.iter_mut().find(|entry| entry.id == selected_id)
    .ok_or("observer is no longer present; refresh the selection")?;
entry.api_secret = Some(new_secret);
let request_id = client.watchdogs().apply(&settings)?;
```

Missing credentials preserve saved values. Empty credentials fail validation.
An empty entry ID adds an observer; existing IDs are opaque and belong to that
core session. Omitted entries are retained; deletion is an explicit `remove(id)`.
Never automatically replay configuration changes or cancellations after reconnect.

Changing a key reconnects only that observer. The table exposes checking, ready,
error, duplicate and online-count feedback through `WatchDogsUpdated`; pending
SafeGuard orders keep the account credentials with which they started.
`ServiceSettingsApplied` identifies the request and carries the actual resulting
state. For configuration edits, `applied=true` and an empty `error` confirm that
the configuration was applied and saved. If `applied`
is true but `error` is nonempty, saving failed: rebuild the edit from the returned
state before retrying, so newly added entries keep their core-assigned IDs.
Replies and pushes may arrive out of order: the SDK retains the newest revision;
a result still describes the state at that particular action.

`refresh()` also returns its state through `ServiceSettingsApplied` (action `get`);
it does not need a separate `WatchDogsUpdated` event. Both events update the retained
snapshot. The returned request ID confirms local enqueue only. On older cores,
keep these controls unavailable until a supported snapshot arrives.

Field meanings, units and dependencies: [MoonDoc service settings](https://github.com/Moonbot-Tech/MoonDoc/blob/main/reference/settings/10-settings-apis.md#watchdog-control-bots-and-unaccounted-positions).

`cancel_buys()` requests cancellation of tracked buys. Its acknowledgement means
the controller accepted the request. Exchange execution and SafeGuard sale results
arrive through the existing core logs; enable log reception in the client. They
are not orders on the terminal's primary trading account.
