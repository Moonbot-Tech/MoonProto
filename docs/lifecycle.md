# Lifecycle Events

`LifecycleEvent` reports connection state and critical transport conditions.
Init is run once per `MoonClient` session. Before that Init, transport handshakes do
not emit Engine API. After Init, reconnect restore refreshes market indexes only
when `PeerAppToken` changed, then sends the needed market refresh/subscription
replay, and replays only the registry subscriptions the application requested.

## Enum

```rust
pub enum LifecycleEvent {
    Connecting,
    Connected { fresh: bool },
    Ready,
    InitStepCompleted { step: &'static str, elapsed_ms: u64 },
    ConnectFailed { error: ConnectError },
    Disconnected,
    Reconnecting,
    BindFailed { consecutive_failures: u32 },
    ServerRestart,
}
```

## Handling

```rust
use moonproto::LifecycleEvent;

for event in client.drain_lifecycle_events() {
    match event {
        LifecycleEvent::Connecting => ui_status("connecting"),
        LifecycleEvent::Connected { fresh: true } => ui_status("connected"),
        LifecycleEvent::Connected { fresh: false } => ui_status("reconnected"),
        LifecycleEvent::Ready => ui_status("ready"),
        LifecycleEvent::InitStepCompleted { step, .. } => show_init_step(step),
        LifecycleEvent::ConnectFailed { error } => show_connect_error(error),
        LifecycleEvent::Reconnecting => ui_status("reconnecting"),
        LifecycleEvent::ServerRestart => ui_status("server restarted"),
        LifecycleEvent::Disconnected => ui_status("disconnected"),
        LifecycleEvent::BindFailed { consecutive_failures } => {
            show_network_alert(consecutive_failures);
        }
    }
}
```

## Semantics

| Event | Meaning | Application action |
|---|---|---|
| `Connecting` | A handshake attempt has started. | Update connection indicator. |
| `Connected { fresh: true }` | First successful authorization for this `MoonClient` runtime. | UI status; wait for `Ready` before treating Active Lib state as initialized. |
| `Connected { fresh: false }` | Re-handshake after reconnect. | UI only; the library refreshes stale indexes after a changed `PeerAppToken`, refreshes markets, restores saved subscriptions, and requests a fresh canonical order snapshot. |
| `Ready` | `MoonClient` finished its one-time connect/init sequence and published the initial snapshot. | UI can treat the Active Lib state as initialized. |
| `InitStepCompleted { step, elapsed_ms }` | One mandatory startup step finished; `elapsed_ms` is cumulative wall-clock time, not the duration of that single step. For the init-spine steps it counts from transport authorization; for the final `StartupSnapshot`/`StartupEvents` steps it counts from runtime startup, so it also includes connect/handshake time. Current cold-init steps: `BaseCheck`, `AuthCheck`, `GetMarketsList`, `UpdateMarketsList`, `StrategySchema`, `PostInitFlush`, `StartupSnapshot`, or `StartupEvents`. | Optional progress display/diagnostics only. |
| `ConnectFailed { error }` | Background `MoonClient` startup failed. | Show the error and create a new client when the user retries. |
| `Reconnecting` | Traffic was silent long enough to trigger soft reconnect. | UI only. |
| `ServerRestart` | Server app token changed. | UI only; after reconnect the library refetches indexes before indexed streams/price refresh and replays saved subscriptions. |
| `Disconnected` | Explicit shutdown through `client.disconnect()`. | Treat the client as finished. |
| `BindFailed` | UDP bind failed across the full port-rotation range for at least 15 seconds; repeat events are throttled to about 50 seconds. | Show OS/network permission or port exhaustion alert. The library keeps retrying. |

## State Flow

```text
Base
  -> Connecting
  -> Connected { fresh: true }
  -> Ready
  -> [running]
  -> Reconnecting
  -> Connecting
  -> Connected { fresh: false }
```

## Live Startup Status

`InitStepCompleted` is the low-frequency lifecycle signal for completed gates.
While a large Sliced response is still arriving, applications can poll
`client.startup_status()` from their existing UI/status update loop:

```rust
let status = client.startup_status();

show_startup_status(
    status.state,
    status.current_step,
    status.elapsed_ms,
    status.received_sliced_bytes,
    status.receive_rate_bytes_per_sec,
    status.idle_for_ms,
);
```

The snapshot also contains completed steps, active Sliced transfer/block
counts, duplicate blocks, whole-step retries, reconnect count, RTT, PMTU, and
the core's server-to-client delivery estimate. These transport-wide counters
describe the connection, not necessarily the response awaited by
`current_step`, and never extend that step's deadline. The snapshot is passive
and available in regular builds; packet tracing and the `diagnostics` feature
are not required.

There is deliberately no single percentage for the whole Init sequence. The
strategy schema may overlap the sequential market requests, and the combined
response size is not known before the Sliced datagrams arrive. Current step,
received bytes/rate, active block counts, and time since useful progress are
truthful signals; a fabricated global percentage is not.

The protocol owner publishes this snapshot at a bounded rate. Polling it does
not install a packet callback or add logging/locking to each received datagram.
After `Ready`, startup transfer totals are frozen. The state can still move to
`Reconnecting` and back to `Ready` if the established transport drops.

`ServerRestart` is emitted during a successful handshake when the peer app token
changes. If the one-time Init has already completed, the following successful
reconnect restores required Engine API state automatically.

If an internal parser/dispatch bug panics while applying one incoming payload,
`MoonClient` logs the error, drops that payload, clears unpublished event/action
buffers, and keeps the runtime alive. This keeps an unexpected bad packet from
turning into a full terminal reconnect. A broader runtime-loop panic outside the
per-payload boundary is still guarded by a last-resort rebuild/reconnect path,
but normal domain dispatch is isolated at payload scope.

`Ready` is not a "all background data is fully loaded" barrier. It waits for
the mandatory init spine: authorization, BaseCheck/AuthCheck, markets list with
the initial server-index map, price refresh, strategy schema, and the post-init
send flush. The schema request can overlap the market/price requests, but
`Ready` is still emitted only after the schema is applied. Replies to the queued
order/settings/balance/local-strategy resync, retained 5m candles, CoinCard
candles, transfer assets, stream packets, and later refreshes report their own
domain events and may arrive after `Ready`. Startup-safe news and runtime/license
state may instead arrive before `Ready`.

## Callback Cost

Regular applications receive lifecycle events from `MoonClient` through the
configured `MoonEventSink`. The default queue adapter exposes
`drain_lifecycle_events` / `try_recv_lifecycle_event`; callback integrations can
post lifecycle events directly into the host UI loop. Low-level protocol
diagnostics have their own hidden hooks; they are not the normal desktop/UI
integration path.
