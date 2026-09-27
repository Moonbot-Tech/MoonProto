# Runtime measurements

The Windows-only `runtime_bench` example measures several independent clients in
one process against one core. It does not place orders or edit shared settings.
Each client has its own identity and socket. Use a test core with enough capacity.

Build in Release, without `diagnostics` or `diagnostic-trace`:

```powershell
cargo build --release --example runtime_bench
target/release/examples/runtime_bench.exe bench.conf 30 idle 90
target/release/examples/runtime_bench.exe bench.conf 30 trades 90
```

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

Measurement starts after all clients are ready and a 15-second warmup. Events are
drained every 50 ms. Each run prints one JSON object; connection progress goes to
stderr. Clients disconnect when the run finishes.

`cpu_ms_per_sec` is process CPU time, not wall-clock latency. Divide by 10 for
percent of one logical CPU, then by the machine's logical CPU count for percent
of the whole machine. `cycles_per_sec` is a separate Windows process counter;
do not convert it to CPU time using the advertised processor clock.

The output includes private/working-set memory, physical socket traffic,
retained trade counts, and the core's reported process/system CPU. Retained
counts are current ring occupancy, not the number of trades received during
the run. `valid` rejects observed disconnects, port rotations and zero aggregate
traffic; it is not a proof of lossless delivery.

Append `alloc` for a separate allocation-count run. It counts allocation and
reallocation calls and their requested sizes, not net memory growth. Do not use
that run for CPU comparisons: counting itself adds contended atomics.

For before/after comparisons, preserve both executables and alternate repeated
runs on the same machine/core, with no concurrent builds or load generators.
Compare received traffic and core load too: a quieter market is not a library
optimization. Multiple clients of one core exercise runtime scaling but do not
replace testing heterogeneous exchanges. Keep raw output, not only averages.
