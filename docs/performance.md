# Runtime measurements

The Windows and Linux `runtime_bench` example measures several independent clients in
one process against one core. It does not place orders or edit shared settings.
Each client has its own identity and socket. Use a test core with enough capacity.

Build in Release, without `diagnostics` or `diagnostic-trace`:

```powershell
cargo build --release --example runtime_bench
target/release/examples/runtime_bench.exe bench.conf 30 idle 90
target/release/examples/runtime_bench.exe bench.conf 30 trades 90
target/release/examples/runtime_bench.exe bench.conf 100 compare 90
```

On Linux, run `target/release/examples/runtime_bench` with the same arguments.

Keep credentials in a local, untracked config file:

```ini
key=<connection key>
server=HOST:PORT
# Optional transport override, matching the core:
mask_ver=0
```

Modes:

- `idle`: normal connected runtime and default periodic refreshes, no trades subscription.
- `quiet`: also disables periodic market/tag refreshes; unsolicited state and protocol maintenance remain active.
- `trades`: normal refreshes plus trades, retaining Compact history for BTC, ETH,
  SOL, XRP, DOGE, BNB, ADA, LINK, AVAX and LTC against USDT. Use a core listing these pairs.
- `full`: normal refreshes plus the complete trade stream, retaining Compact
  history for every market. Market-maker orders are a separate subscription.
- `compare`: keep the same clients alive for four measured windows:
  idle, full, idle, full. The seconds argument applies to each window.

Clients start concurrently. Startup has its own CPU/time result and a 180-second
connection timeout; a run fails if any client fails or the group is still not
ready after 185 seconds. Measurement starts after all clients are ready and a
30-second warmup. Each comparison window also has this warmup after changing
subscriptions. Events are drained every 50 ms. Output is JSON Lines: startup,
measurements, per-second samples, and optional diagnostic snapshots. Progress
goes to stderr. Clients disconnect when the run finishes.

`cpu_ms_per_sec` is process CPU time, not wall-clock latency. Divide by 10 for
percent of one logical CPU, then by the machine's logical CPU count for percent
of the whole machine. `cycles_per_sec` is a separate Windows process counter;
do not convert it to CPU time using the advertised processor clock. Cycle counters
are unavailable (`null`) on Linux, which uses process CPU time instead.

The output includes private/working-set memory, physical socket traffic,
retained trade counts, and the core's reported process/system CPU. Retained
counts are current ring occupancy, not the number of trades received during
the run. `valid` rejects observed disconnects, port rotations, and any client
with zero received bytes. Diagnostic builds also reject observed parse failures.
It is not a proof of lossless delivery. Parser-error counts are unavailable
(`null`) in normal builds. On Windows the harness thread has a separate cycle
counter; Linux reports its CPU time as `harness_cpu_ms_per_sec`.
`private_memory_kind` identifies committed private bytes on Windows and resident
private bytes on Linux; these are different memory measurements. Linux collects
memory only at window boundaries to avoid repeated page-table scans. Per-second
Linux samples therefore have `private_mb: null`.
Comparison continues after an invalid window to observe recovery; the process
exits unsuccessfully after disconnecting all clients if any window was invalid.

Append `alloc` for a separate allocation-count run. It counts allocation and
reallocation calls and their requested sizes, not net memory growth. Do not use
that run for CPU comparisons: counting itself adds contended atomics.

For before/after comparisons, preserve both executables and alternate repeated
runs on the same machine/core, with no concurrent builds or load generators.
Compare received traffic and core load too: a quieter market is not a library
optimization. Multiple clients of one core exercise runtime scaling but do not
replace testing heterogeneous exchanges. Keep raw output, not only averages.

For attribution, build a separate executable with `--features diagnostics`.
It emits per-client protocol and retained-history profile snapshots before and
after each measurement window, plus dispatch counts grouped by command.
Subtract the counters before interpreting them. Wall time is collected
on every phase call; thread CPU time/cycles are randomly sampled on approximately
1/64 of calls. Estimate a phase total as `sample_sum * calls / sample_count`;
zero samples means unavailable. Nested phases are inclusive: do not add them
together. Windows exposes cycles, while Unix exposes thread CPU nanoseconds.
These clocks exclude preemption, unlike wall time. The `clock_calibration` record
contains empty-timer cycle or CPU-time samples to estimate the clock's own cost. Small phases
include timer overhead; subtracting its median only gives an estimate. Use normal
builds for final CPU numbers and compare repeated runs. History phases distinguish
stream application, price updates, queue waits, compaction, analytics, and memory
warmup. Socket wait CPU includes poller rearming and processing completions,
excluding time spent asleep. Nested runtime and dispatch phases remain inclusive.
Diagnostic Windows and Linux builds also name the runtime, lifecycle, and history threads
for external per-thread CPU accounting. Normal builds contain no phase timers.
