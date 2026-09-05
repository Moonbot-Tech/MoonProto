# Core Diagnostic Problems

MoonBot's detectors report confirmed problems such as paging, API restrictions,
and network failures. These are **core diagnostics**, not terminal errors or
connection status. Hypotheses that have not reached confirmation are not sent.

The core sends its current list automatically on connection. No subscription or
request is needed, and this delivery does not block `Ready`.

```rust
if let Some(snapshot) = client.snapshot() {
    let problems = &snapshot.settings().problems;
    if problems.snapshot_received() {
        for problem in problems.items() {
            println!("{}: {}", problem.title, problem.message);
        }
    }
}
```

Before `snapshot_received()` becomes true, the list may be incomplete, even if
a live notification has already arrived. Older cores do not send this extension;
an absent initial list is not proof that the core is healthy.

## Fields and Events

`KernelProblem` is exported from `moonproto::state`. `kind` identifies a row;
`kind_name` is its stable textual key (`paging`, `region-blocked`, `test`, etc.).
There is one row per kind, not per market. Preserve unknown kinds and categories.

`title` and `message` are display text in the core's language.
`technical_details` contains detector evidence and thresholds, not structured data
for parsing. `first_seen` and `confirmed` are UTC `MoonTime` values.
`confirmations` is the last received confirmation count.

- `Event::Settings(SettingsEvent::ProblemsUpdated)`: a full list replaced the
  previous list. Missing rows are removed; an empty list means no confirmed facts.
- `Event::Settings(SettingsEvent::ProblemConfirmed { problem })`: a newly confirmed
  fact was inserted or updated by kind. Use this event for a notification.

State is applied before the event is published. Repeated confirmations of an
existing fact are deliberately **not broadcast**; text, time, and count can be
older than the core's current row until the next full list.

## Clear and Test

`client.settings().clear_problems()?` clears **all** facts and pending hypotheses
on the core, for all terminals. It does not fix their causes. A cause that remains
can produce a new fact later. Do not clear local state optimistically: wait for
the resulting full-list event. New facts can appear while the reply is in flight.

`client.settings().test_problem("Terminal check")?` publishes a test signal
through the normal detector worker, usually processed within about two seconds.
Use short ASCII text: the core's signal buffer retains at most 200 characters
in its legacy encoding. The first `test` fact produces a notification; repeated
tests update the existing core row without another notification until it is cleared.
Neither method adds a library retry timer or promises a request-specific reply.

## Known Limitation

Full lists and notifications have no shared revision and are applied in arrival
order. A rare delayed list can erase a newer notification, or a delayed notification
can restore a cleared row. This limitation is accepted to keep the implementation
simple. The next successfully received current full list repairs the state;
a fresh connection receives that list automatically.
The old retained list is reset on a hard reconnect or core restart, not a soft
network rebind. There is no periodic diagnostic-list refresh.

## FireTest

The full FireTest includes this extension; `fire_test_core_problems` runs just the
diagnostic scenario. It checks the initial lists, a test notification to two
clients, and the existing fact in a late client's initial list.

`MOONPROTO_FIRETEST_CLEAR_PROBLEMS=1` explicitly enables clearing all core diagnostics
before and after the test. Use it only on an isolated test core: cleared real
diagnostics cannot be restored. Without this opt-in, the test leaves its fact on
the core; if a `test` fact already exists, mutation is skipped with an explicit log.
