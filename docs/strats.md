# Strategies

Active Lib keeps the terminal's strategy list in sync with the MoonBot core.
Applications read decoded strategies, edit strategy objects, change checked
state, and send start/stop intents. They do not parse compressed strategy blobs
or answer server snapshot requests by hand.

The active runtime maintains `StratsState` and emits `Event::Strat`. Snapshot
payloads are decoded automatically into both the lightweight `StrategyInfo`
state and full `StrategySnapshot` values. `last_server_epoch` records the last
successfully decoded snapshot's timestamp; it is not the accepted order version.
Use `last_modified()` for that. A malformed snapshot is logged and is not
reported as `SnapshotFull` / `SnapshotPartial`.

`strategy_snapshot(id)` and `strategy_snapshots()` are core-confirmed state.
Submitting an edited list does not overwrite them optimistically. While an edit
is in flight, `strategy_edit(id)` exposes the exact desired snapshot and its
`Pending` or `TimedOut` status. This lets a UI draw the desired value with a
pending marker without presenting it as core state.

Before init, user code gives the library its current local strategies through
`InitConfig::initial_strategies`. The runtime owns that list after that point:
Init sends it as the post-init strategy snapshot, answers later server snapshot
requests automatically, and applies strategy snapshots/deletes/checked updates
received from the server. If user code provides an explicit empty list, the
client has no local strategies; the current server snapshot is still available
through the same read API.
When the server asks for a client snapshot before Init is complete, the request
is remembered and answered during post-init resync after the strategy schema and
owned strategy state are ready.

Init also requests the live strategy schema and stores the decoded schema in
`StratsState`. Rust consumers read strategy field metadata from the server
instead of carrying a hardcoded copy of server strategy UI metadata. If the
schema response is missing, malformed, or cannot be decompressed, Init fails and
the domain gate does not open.

Strategy checked state is still synchronized with the core, but detect
calculation runs on the core. Rust receives ready detect facts through
`Event::Detect`; it does not run local strategy detect loops or rebuild
watcher/chart-alert text from strategy fields.

Low-level parser edge cases are intentionally kept out of the application model.
Normal terminal code observes decoded `StratsState`, `StrategySnapshot`, and
`StrategySchema`; malformed or future-version protocol payloads are handled by
the runtime without asking UI code to parse packet tails.

## Reading Strategy State

```rust
use moonproto::Event;
use moonproto::state::StratEvent;

for event in client.drain_events() {
    if let Event::Strat(strat_event) = event {
        match strat_event {
            StratEvent::SnapshotFull { .. } => {
                let Some(state) = client.snapshot() else { continue; };
                println!("strategies={}", state.strategy_snapshots().count());
                for strategy in state.strategy_snapshots() {
                    if let Some(name) = strategy.strategy_name() {
                        println!("{}: {}", strategy.strategy_id, name);
                    }
                }
            }
            StratEvent::Deleted {
                strategy_id,
                folder_path,
                strategy_deleted,
                folder_deleted,
            } => {
                if *strategy_deleted {
                    remove_strategy(*strategy_id);
                }
                if *folder_deleted {
                    remove_empty_folder(folder_path);
                }
            }
            StratEvent::CheckedSynced { changed, is_delta } => {
                println!("checked changed={changed} delta={is_delta}");
            }
            StratEvent::SchemaApplied { kind_count, field_count, .. } => {
                println!("strategy schema: kinds={kind_count} fields={field_count}");
            }
            _ => {}
        }
    }
}
```

Snapshot events are signals that the decoded state is ready. Normal
applications should read `state.strategy_snapshot(...)` or
`state.strategy_snapshots()`. For logging, use
`StratEvent::snapshot_server_epoch()`. Raw snapshot sizes/bytes are
diagnostics-only because terminal code should not depend on compressed protocol
payloads.

Server snapshot requests are answered by the runtime from the library-owned
local strategy list. Terminal code does not need to handle that packet or build
a snapshot reply; the normal `MoonClient` event sink suppresses the hidden
request event after latching/sending the reply.

## Strategy Order

Pass the complete editor list to `sync_local_strategies` in the desired order.
For a reorder, the library sends one Full snapshot: its row sequence is the
order, including any parameter edits made in the same action. No separate
order command or application-assigned timestamp is needed. Keep the core's
folder grouping: strategies in one folder form a contiguous group.

When order is unchanged, only changed strategies are serialized and sent as a
Partial snapshot. An unchanged, already-confirmed list sends nothing. Initial
sync and automatic Full replies still carry the complete list.

The core and library accept a newer Full order independently of per-strategy
edit dates. An older Full cannot roll back order, but can still supply newer
parameters. A Full never implicitly deletes missing strategies; use `delete`.
Read the confirmed order through `strategy_snapshots()` after `SnapshotFull`.
Keep an editor draft while edits are pending instead of rebuilding each next
edit from the still-unconfirmed core snapshot.

Known limits: after all delivery retries fail, the next reorder, Full sync or
reconnect repairs order; there is no periodic order repair. Concurrent reorders
with equal dates are not separately arbitrated. Recovery of lost strategy-delete
commands is not part of order synchronization. Folder-path edits still require
the strategy's normal edit-date update.

## Folders, Including Empty Folders

Read `state.strats().folder_paths()` after `StratEvent::SnapshotFull`. The list
contains every confirmed folder and parent, including folders with no strategies.
Its iteration order is unspecified; it is not the strategy order.

Wait until `state.strats().folders_last_modified() > 0` before editing folders.
Zero means the first versioned folder tree has not arrived, or the core does not
support folder synchronization. The new folder APIs return `StateUnavailable`
until then. Do not submit an old cached folder list automatically on connect:
the runtime receives the current core tree and handles subsequent replies itself.

```rust
let state = client.snapshot().expect("state is ready");
let mut paths: Vec<String> = state.strats().folder_paths().map(str::to_owned).collect();
paths.push("Research/Empty".into());
client.strategies().sync_local_folders(paths)?;
```

This submits the **complete desired tree**, not additions. To delete an empty
folder, omit its path and all descendants. Parents are created automatically;
a folder containing a retained strategy cannot be removed by omission. The
library assigns the folder timestamp. Confirmation is a `SnapshotFull` with
the updated `folder_paths()`, not a per-folder `Deleted` event.

For a rename or move containing strategies, update their paths and edit dates,
replace the old folder paths in the complete tree, then submit both together:

```rust
client.strategies().sync_local_strategies_with_folders(edited_strategies, paths)?;
```

`edited_strategies` is the complete editor list in its desired order, just as
for `sync_local_strategies`. The core applies strategy changes first, then the
newer folder tree, removing the now-empty old paths. No separate folder-delete
command is needed. Real strategy deletion still uses `delete(strategy_id, "")`.

Parameter-only edits still use a Partial snapshot. Folder changes use one Full;
their timestamp is independent of strategy order and individual edit dates.
A late older Full cannot restore deleted empty folders. Full replies and
reconnect preserve the complete folder tree, including an empty tree.

Paths use `/`, are case-insensitive, and must fit 255 UTF-8 bytes. New folder
intents reject empty path segments, surrounding whitespace, quotes, line breaks,
NUL, or a tree exceeding the snapshot dictionary's capacity.

Known limits: concurrent complete folder edits use the newer timestamp; a lost
empty folder can be recreated. Equal timestamps are not separately arbitrated.
Exact sibling order of empty folders is not synchronized. A conflicting newer
strategy can keep its old folder occupied, so that folder is retained for safety.
After exhausted delivery retries, a later Full or reconnect repairs the tree;
there is no extra periodic folder-sync loop.

## Global Strategy Runtime State

The core also reports whether the global strategy engine is currently running.
This is retained state, not a log message. A terminal reads it from the snapshot
and updates its start/stop UI from `StratsState::strategies_running()`:

```rust
let Some(state) = client.snapshot() else { return; };
match state.strats().strategies_running() {
    Some(true) => show_strategies_running(),
    Some(false) => show_strategies_stopped(),
    None => show_strategy_runtime_unknown(),
}
```

The value is `None` only before the server has reported the first runtime
state. After that, `Event::Strat(StratEvent::RuntimeState { .. })` is the
signal to repaint buttons or restore a previous start/stop mode after a
temporary test/editor operation:

```rust
let was_running = state.strats().strategies_running().unwrap_or(false);

// mutate checked state or synchronize a test/editor strategy...

if was_running {
    client.strategies().start()?;
} else {
    client.strategies().stop()?;
}
```

`start()` and `stop()` are asynchronous intents. They queue the command and the
next retained runtime-state update confirms what the core actually applied.

## Strategy Schema

The schema is built by the server from live strategy metadata and decoded by
Active Lib during Init. Terminal UI code reads the decoded `StrategySchema`;
it does not need a hardcoded Rust copy of server strategy fields.

Public read API:

```rust
let Some(state) = client.snapshot() else { return; };
let schema = state
    .strats()
    .strategy_schema()
    .expect("schema is available after LifecycleEvent::Ready");

for kind in &schema.kinds {
    println!("kind {} {}", kind.ordinal(), kind.name);
}

for field in &schema.fields {
    println!(
        "{} type={} ui={:?} visible_for={:?}",
        field.name,
        field.type_id.name(),
        field.ui_kind,
        field
            .visible_strategy_kinds()
            .map(|kind| schema.kind_name_for_strategy_kind(kind).unwrap_or("?"))
            .collect::<Vec<_>>()
    );
}
```

`StrategySchema` exposes:

- `format_version`;
- `kinds`: typed strategy kind and server UI name;
- `fields`: field name, typed field kind, UI kind, non-zero server default
  value, and typed strategy-kind visibility;
- `StrategyFieldLayout`: no layout marker, comment, filter class, or chapter
  class with its chapter name;
- `static_picklist`;
- `dynamic_picklist`: `UseHookStrategy` means local MoonHook strategies with an
  empty first item; `ComboStart` / `ComboEnd` mean all local strategies.

`ChannelName` is intentionally not a schema picklist. The server exports it as
a plain string because its suggestions come from runtime terminal
configuration, not from strategy schema data. A terminal may add its own UI
suggestions for that field, but Active Lib does not hardcode them.

Use `field.visible_for_strategy_kind(kind)` or
`field.visible_strategy_kinds()` for visibility checks. Raw wire ordinals and
the internal serializer bitmask are diagnostics-only; terminal code should keep
the typed `StrategyKind`. For strategy editors, prefer the ready-made
editor views:

```rust
let kind = strategy.kind();
for section in schema.editor_sections_for_strategy_kind(kind) {
    draw_section_header(&section.title);
    for field in section.fields {
        draw_strategy_field(field);
    }
}
```

`editor_sections_for_strategy_kind` preserves server-defined editor grouping.
Layout markers are carried over following fields until the next marker, so
terminal UI does not need to know that comment/filter/chapter markers are stored
only on the first field of a section.

Dynamic combo fields can also build their current values from the retained
strategy list:

```rust
if let Some(source) = &field.dynamic_picklist {
    let values = source.values_from_snapshots(state.strategy_snapshots());
    draw_combo_values(values);
}
```

This mirrors the editor data sources: `UseHookStrategy` gives an empty item plus
local MoonHook strategy names, while `ComboStart` / `ComboEnd` give all local
strategy names.

Schema TypeIDs use the same value model as strategy snapshots:

```rust
use moonproto::{
    StrategyDynamicPicklist, StrategyFieldLayout, StrategyFieldType,
    StrategyFieldUiKind, StrategySchema, StrategySchemaEditorSection,
};
```

Normal clients should read the active runtime state populated by Init. Schema
parsing helpers are kept out of the terminal model.

## State

```rust
pub struct StrategyInfo {
    pub strategy_id: u64,
    pub strategy_ver: i32,
    pub last_date: u64,
    pub sell_price: f64,
    pub checked: bool,
    pub prev_checked: bool,
    pub folder_path: Arc<str>,
}
```

`StrategyInfo` is a lightweight UI/index state. Full strategy fields are not
stored there; they are stored as `StrategySnapshot` values owned by the
runtime state. `last_date` is the exact Unix-millisecond value used by core
rollback guards; UI labels should use `info.last_edit_time()` /
`snapshot.last_edit_time()` and new local snapshots can be built with
`StrategySnapshot::new_at(..., MoonTime, ...)`. `checked` is the direct checked
state; `prev_checked` is the last server-acknowledged checked state. Checked deltas are
pending while these fields differ and become acknowledged only after the server
echoes or synchronizes checked state.
`sell_price` is copied from the decoded snapshot field `SellPrice` when that
field exists; incoming sell-price command echoes are not applied as state
updates because the core client model has no receive-side branch for that
command. A full incoming snapshot does not delete local strategies that are
absent from the payload. The runtime keeps those strategies as local "Own"
entries.

Strategy delete has two independent effects: delete `StrategyID` when it is
non-zero, then delete `FolderPath` when it names an existing empty non-root
folder. `StratEvent::Deleted` exposes both result flags. `strategy_deleted` and
`folder_deleted` tell which parts actually changed state. If both are false,
Active Lib does not publish a strategy event.

Future-version strat commands, unknown strat command ids, incoming
schema requests, and incoming sell-price updates do not emit active strategy
events. The active runtime treats those as skipped/base commands or as commands
with no receive-side state branch, so they have no strategy side effects. The
low-level parser/state APIs still expose `StratCommand::Skipped`,
`StratCommand::Unknown`, and the command parsers under the diagnostics feature
for explicit protocol diagnostics.

## Active Predicates

`StrategySnapshot` exposes core-compatible helpers for code that needs to
reason about active strategies without guessing that `checked == active`.
`is_active(mode)` follows MoonBot active-client rules: in `ActiveClient` mode a
checked strategy is local-active only when it cannot auto-buy and does not run
detection on the core; in `UsingMoonProto` mode the inverse side is active; in
`Standalone` mode active is just checked.

```rust
use moonproto::{StrategyActiveMode, StrategyKind};

let is_local = strategy.is_active(StrategyActiveMode::ActiveClient);
let kind = strategy.kind();

if kind == StrategyKind::NEW_LISTING && strategy.sell_from_asset() {
    println!("listing sell-from-asset strategy");
}
```

`StratsState` also exposes listing predicates:

```rust
let has_listing = state
    .strats()
    .has_listing_strategy(StrategyActiveMode::ActiveClient);

let needs_assets = state
    .strats()
    .has_listing_sell_strategy(StrategyActiveMode::ActiveClient, is_futures);
```

These are read helpers only. They do not make the active library send listing
automation requests by themselves.

```rust
use moonproto::StrategySnapshot;

let strategies: Vec<StrategySnapshot> = load_current_strategies();
let init = InitConfig {
    initial_strategies: Some(InitialStrategies::new(
        load_local_strategy_epoch(),
        strategies,
    )),
    ..Default::default()
};

let client = MoonClient::connect(cfg, ConnectConfig::new(init))?;
let owned_for_export: Vec<StrategySnapshot> = client
    .snapshot()
    .map(|state| state.strategy_snapshot_vec())
    .unwrap_or_default();
```

`strategy_snapshot_vec()` clones full snapshots and is meant for owned export,
persistence handoff, or offline editing. Rendering code should normally use
`strategy_snapshots()` / `strategy_snapshot(id)` and borrow the retained state.

The date passed to `InitialStrategies::new` belongs to that persisted list's
order. Persist `state.strats().last_modified()` together with its confirmed
strategy list; use `0` for an old cache without this metadata. Do not substitute
the cache load time. If the application reloads
its whole local strategy list after `MoonClient::connect`, use
`client.strategies().sync_local_strategies(strategies)`. The application still
owns the strategy editor/persistence; this call tells Active Lib that the local
list changed. Active Lib updates the order date only when the list sequence
changes and keeps the runtime-owned copy for future server snapshot requests. The call queues
intent and returns immediately; if startup is still running, the runtime defers
the intent until the Init/schema gate has opened. The core already echoes
accepted strategy revisions and returns its newer revision when the rollback
guard wins. Active Lib maps those existing snapshots to:

- `StratEvent::EditSubmitted` when changed rows are sent;
- `StratEvent::EditConfirmed` when the core echoes the same `StrategyID`,
  revision and the same canonical fields;
- `StratEvent::EditAdjusted` when the core accepts the revision but returns
  different canonical fields;
- `StratEvent::EditSuperseded` when the core returns a newer revision;
- `StratEvent::EditTimedOut` when no resolving snapshot arrives within 45
  seconds.

Timeout is deliberately not treated as rejection. The core may have applied an
edit whose echo was lost, so Active Lib keeps the desired edit and accepts a
late confirmation. The latest complete local list also remains the source for
automatic snapshot replies after reconnect. No extra polling or wire command is
required.

## Strategy Fields

```rust
use moonproto::{field_names, FieldValue, StrategyFields};

let Some(state) = client.snapshot() else { return; };
for strategy in state.strategy_snapshots() {
    if let Some(name) = strategy.fields.get_string(field_names::STRATEGY_NAME) {
        println!("{}: {}", strategy.strategy_id, name);
    }
}
```

`StrategySnapshot.fields` is a `StrategyFields` container, not a standard
`HashMap`. It stores the decoded fields densely in received order, which avoids
hash work while parsing large snapshots. The reader path appends fields in the
schema serializer order; `insert` keeps replacement semantics for user-built
snapshots. Prefer typed getters and `field_names::*` constants for common fields
so UI code does not depend on unreviewed string literals. The public operations
are intentionally small and familiar:

```rust
let mut fields = StrategyFields::new();
fields.insert(field_names::STRATEGY_NAME, FieldValue::String("Local".to_string()));

if let Some(name) = fields.get_string(field_names::STRATEGY_NAME) {
    println!("{name}");
}

for (name, value) in fields.iter() {
    println!("{name} = {value:?}");
}
```

`FieldValue` variants:

```rust
Bool(bool)
Int32(i32)
Int64(i64)
Double(f64)
String(String)
Byte(u8)
Word(u16)
UInt32(u32)
UInt64(u64)
Single(f32)
```

Outgoing snapshots keep default-valued fields out of each compact strategy
record, but the batch dictionary still declares every schema field known for
the included strategy kinds. The core therefore resets a known-but-absent
field to its schema default. A field absent from the dictionary is unknown to
the sender and remains untouched, which preserves compatibility with newer
core fields.

Raw serializer parsers remain available for diagnostics and custom protocol
tools, but they are hidden from the normal API surface. Applications should use
decoded `StratsState` from `MoonClient::snapshot()`.

## Editing Strategy Objects

`StrategySnapshot` is the retained/core snapshot shape. It preserves every field
needed for round-trip synchronization, including fields the current UI may not
understand yet. Terminal editors should not build the `fields` container by
hand for common strategy types. Use the live schema and an editor object:

```rust
use moonproto::MoonShotStrategy;

let Some(state) = client.snapshot() else { return; };
let schema = state
    .strats()
    .strategy_schema()
    .expect("schema is available after Ready");

let existing = state.strategy_snapshot(strategy_id);
let mut shot = match existing {
    Some(snapshot) => MoonShotStrategy::from_snapshot(schema, snapshot)?,
    None => MoonShotStrategy::new(strategy_id),
};

shot.name = "MoonProto FireTest Shot".to_string();
shot.path = "FireTest".to_string();
shot.checked = true;
shot.auto_buy = true;
shot.emulator_mode = true;
shot.ignore_filters = false;
shot.mshot_price_min = 3.0;
shot.mshot_price = 5.0;
shot.order_size = 250.0;
shot.coins_white_list = "ETH".to_string();
shot.coins_black_list.clear();

let edited = shot.into_snapshot(schema)?;
```

Typed wrappers such as `MoonShotStrategy` keep the public editing surface close
to what terminal code actually changes, while `into_snapshot` still validates
field names, visibility, and TypeIDs against the live schema. Unknown/future
fields from an existing snapshot are preserved.

For generic editors or less common strategy kinds, use `StrategyEditor`:

```rust
use moonproto::{field_names, StrategyEditor};

let mut editor = StrategyEditor::from_snapshot(schema, snapshot)?;
editor.set_string(field_names::STRATEGY_NAME, "Local strategy")?;
editor.set_number("OrderSize", 250.0)?;
editor.set_checked(true);
editor.touch_now();
let edited = editor.into_snapshot();
```

After editing, synchronize the current local strategy list. This is list
synchronization, not a single-field patch. Start from the list retained in
Active Lib, replace the edited strategy, then send the whole current list:

```rust
let mut strategies = state.strategy_snapshot_vec();
if let Some(existing) = strategies.iter_mut().find(|s| s.strategy_id == edited.strategy_id) {
    *existing = edited;
} else {
    strategies.push(edited);
}

client.strategies().sync_local_strategies(strategies)?;
```

Every generic editor change must call `touch` or `touch_now` once after its
field changes. The core's rollback guard compares `last_date` and
`strategy_ver`; reusing an old revision cannot provide a meaningful
confirmation. Typed editors such as `MoonShotStrategy::into_snapshot` perform
this touch automatically.

## Sending Strategy Commands

Regular applications use `client.strategies()`:

```rust
client.strategies().sell_price_update(strategy_id, sell_price)?;
client.strategies().delete(strategy_id, folder_path)?;
```

Do not send raw strategy-snapshot requests from client code. They are
server-to-client requests, and the server ignores them when received from a
client. The real flows are: post-init sends the current local strategy list, and
later the server may request that list again; the runtime answers
automatically.

`strat_sell_price_update` is the client-to-server sell-price command. The
server applies it to its local strategy if the strategy exists; the active
client does not treat the same command as a server-to-client state update.

Use the same handle for regular UI integration:

```rust
client.strategies().sell_price_update(strategy_id, sell_price)?;
client.strategies().set_checked(strategy_id, true)?;
client.strategies().send_checked_delta()?;
```

For normal active-library flow, pass the local list before init and let the
runtime answer server snapshot requests:

```rust
use moonproto::{InitConfig, InitialStrategies};

let init = InitConfig {
    initial_strategies: Some(InitialStrategies::new(
        load_local_strategy_epoch(),
        load_current_strategies(),
    )),
    ..Default::default()
};
```

Checked-state sends should also go through the active-library state. This
matches the MoonBot checked-delta model: local UI changes update `checked`,
leave `prev_checked` untouched, and the outgoing delta contains only items where
`checked != prev_checked`.

```rust
client.strategies().set_checked(strategy_id, true)?;
client.strategies().send_checked_delta()?;
client.strategies().start()?;
```

`send_checked_delta` sends a checked-state delta only when the delta is
non-empty. `strategies().start()` always sends the start command after the
client's Init gate is open; the checked delta may be empty because the same
command also carries the start/stop action. Both helpers keep `prev_checked`
unchanged until the server confirms the checked-state change.

Low-level compatibility tools may still use raw checked-sync/start-stop and
snapshot helpers, but those helpers are hidden diagnostics. Regular
applications should prefer `MoonClient` helpers so the library-owned strategy
state stays authoritative. Checked-state echo messages are inbound only; client
code must not send them.

When the terminal's local strategy list changes after startup, synchronize the
current list through the same active-library strategy handle:

```rust
client
    .strategies()
    .sync_local_strategies(load_current_strategies())?;
```

This is still an Active Lib intent, not a raw protocol call: "local strategies
changed; synchronize them". The vector defines the global order, not an order
within each folder. The runtime keeps the list for automatic snapshot replies
and assigns a fresh UTC date when its sequence changes. If the call is made
while Init is still in progress, the command is held in the runtime FIFO and
serialized only after the live server schema is available.

Active Lib owns schema-order serialization, field visibility/type checks,
default elision, and automatic replies to later core snapshot requests.
Application code edits typed strategy objects and calls
`client.strategies().sync_local_strategies(...)`; it does not reproduce the
snapshot serializer or intercept the request path.
