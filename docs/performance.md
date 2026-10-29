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

## Ordinary receive buffers and indexed market updates

Ordinary Engine API responses are parsed and, when necessary, inflated once through active
delivery. Client bookkeeping and state application reuse that result. A registered
receiver that also needs an independent response gets one body copy; a pending
market-list/price owner receives the response directly and applies it later as before.

Ordinary, non-grouped datagrams retain their owned receive buffer through AES-GCM
and active delivery. Header removal advances a slice boundary instead of moving
the payload. Grouped subcommands and pre-authentication buffering still materialize
their independently owned data. Socket input and decompressed output still allocate.

Indexed updates resolve the current server index directly to a local market
position. Both server-map changes and local-list changes rebuild the table;
stale-session gates still apply. A published snapshot retains its mapping.
This adds about 16 KB per 1000 markets per client on 64-bit targets, plus a linear
rebuild when the mapping changes. Normal price/trade updates do not rebuild it.
The public API and Compact's 30 ms idle wait are unchanged.

Run the synchronous receive benchmark without a server or credentials:

```sh
cargo test --release --locked --lib receive_hot_path_benchmark -- --ignored --nocapture --test-threads=1
```

The benchmark replays generated datagrams through transport validation, optional
AES-GCM, API parsing, bookkeeping and active market-state application. Each batch
contains 256 updates of 1000 markets, or 8192 encrypted updates of one market.
The server index order is reversed and every final bid/ask is checked. Each case
has five warmups and 20 measured batches. Encryption, fixture construction, slider
reset and result comparisons are outside the timer in both versions. The decoded
workload is identical; encryption IVs are generated during fixture construction.

This is a synthetic receive-path comparison, including test-build diagnostics
and thread-local allocation counting. It excludes sockets, OS fragmentation,
Sliced reassembly, waiting, outgoing sends, and retained-history workers. The large
direct datagrams isolate receive/parse/apply costs; they do not model the core's
normal packetization. Allocation bytes mean requested allocation/reallocation
sizes, not retained memory. Use normal `runtime_bench` builds and the station's
own profiler to measure whole-process CPU; these numbers are not a predicted
station-wide saving or Linux allocator result.

Measured on Windows x86-64, Rust 1.96.0 MSVC, Release, on 2026-10-29. The baseline
is public `98420c6d`. Both executables used the same harness; values are medians
of three run medians (and medians of each run's p95), with alternating order and
no concurrent builds or load generators. CPU values are measured thread cycles,
not wall time converted through a nominal clock frequency.

| Update per message | Messages/batch | CPU Mcycles, before → after | Wall ms p50, before → after | Wall ms p95, before → after | Allocations/message, before → after |
| --- | ---: | ---: | ---: | ---: | ---: |
| 1000 prices | 256 | 131.064 → 94.738 | 34.958 → 25.681 | 36.705 → 26.510 | 4 → 2 |
| 1000 prices, DEFLATE | 256 | 213.934 → 139.271 | 57.805 → 37.556 | 65.264 → 39.404 | 6 → 3 |
| 1000 prices, AES-GCM | 256 | 152.224 → 112.537 | 40.270 → 30.010 | 42.040 → 32.064 | 4 → 2 |
| One price, AES-GCM | 8192 | 129.466 → 117.500 | 35.945 → 33.277 | 40.408 → 35.814 | 4 → 2 |

Requested allocation bytes per message fell from approximately 108633 to 54589,
211293 to 105919, 108697 to 54635, and 799 to 686 respectively. The extra persistent
index table is created before timing and is accounted for separately above.
The combined changes reduced measured receive CPU by about 26–35% for the large
fixtures and 9% for the small encrypted fixture. These are combined results,
not independent savings that can be added together.

## Single-market search

`MarketsState::find` scans without allocating match lists. It returns immediately
on the first exact match; otherwise it keeps the first prefix and substring
candidates and returns the better one. Matching fields, ASCII case handling,
whitespace trimming, ranking, market-list tie order and stable handles are unchanged.
Only the returned handle is cloned. No cache or additional persistent state is added.
`search` continues to return a ranked list for autocomplete.

```sh
cargo test --release --locked --lib market_find_benchmark -- --ignored --nocapture --test-threads=1
```

Measured on Windows x86-64, Rust 1.96.0 MSVC, Release, on 2026-10-30. Both
executables used the same harness and the same 1000-market catalog. The baseline
used `search(input, 1)` as in public `98420c6d`. Five alternating before/after pairs
were pinned to the same logical processor, without concurrent builds or load
generators. Each case used five warmups and 20 measured batches of 128 lookups.
The table gives medians of run medians and run p95s. The timer includes lookup,
stable-handle identity checks and result release in both versions. Catalog creation
and reference lookup are outside timing. Allocation counting is test-only;
requested bytes include reallocations, not retained memory or peak usage.

| Query | CPU Mcycles/batch, before → after | Wall ms p50, before → after | Wall ms p95, before → after | Allocations/lookup, before → after |
| --- | ---: | ---: | ---: | ---: |
| Exact, first market | 60.837 → 0.016 | 16.153 → 0.004 | 20.280 → 0.004 | 2 → 0 |
| Exact, middle market | 61.609 → 27.820 | 16.376 → 7.361 | 17.079 → 8.009 | 2 → 0 |
| Exact, last market | 60.857 → 60.064 | 16.120 → 15.964 | 17.652 → 17.536 | 2 → 0 |
| Prefix, many matches | 15.463 → 8.135 | 4.112 → 2.152 | 5.009 → 2.595 | 10 → 0 |
| Substring, many matches | 26.526 → 20.159 | 7.036 → 5.380 | 7.656 → 6.031 | 10 → 0 |
| No match | 25.836 → 25.374 | 6.820 → 6.708 | 7.070 → 7.474 | 0 → 0 |

Requested bytes per successful lookup fell from 120 for exact matches and 49080
for the many-match cases to zero. Empty input remained allocation-free and took
about 0.002 ms per batch in both versions. The first-match result is near the
timer/harness floor; do not extrapolate its ratio to typical queries. Last-match
and missing queries still scan the catalog and show no material speedup. In this
fixture, middle exact matches used about 55% fewer CPU cycles, prefix matches 47%
fewer and substring matches 24% fewer. These are lookup costs, not an estimate of
station-wide CPU savings.

## Bulk receive buffers

Full candle snapshots and market-history archives retain their receive buffers
through decryption, response parsing, and chunk collection. Consumed headers are
excluded by slice boundaries. The existing archive worker reads the retained
parts in chunk-index order directly; it no longer concatenates the archive.
No application API changes are required.

In-order UDP fragments reuse their assembled allocation. Reordered fragments
still require one ordered materialization before authentication. Archive
decompression and candle construction still allocate their output. The explicit
flat-buffer helper available under `diagnostics` also still concatenates when
called. These are not claims of zero-copy socket I/O.

Duplicates keep the first accepted part and do not refresh request progress.
Request UIDs remain independent, including interleaved transfers. The chunk
collector no longer rejects an archive solely because its compressed parts
exceed the former aggregate byte limit. Archive decoding retains its existing
format and decompression checks. Diagnostic response metadata is read from the
header without copying or decompressing the body; the diagnostic payload hash
remains available and has a measurable cost.

Run the local synthetic benchmark without a server or credentials:

```sh
cargo test --release --locked --lib bulk_receive_benchmark -- --ignored --nocapture
```

It uses the production assembly, decrypt, response, collection, and archive
parser functions synchronously. The fixture is a 4,307,641-byte zlib archive,
220 markets and 330,000 candles, delivered in two encrypted responses with 132
UDP fragments each. The receive timer includes fragment storage, assembly,
authentication/decryption, response parsing, and collection. The total also
includes archive decompression, candle construction, and release of the archive
buffers. Fixture generation, encryption, and result comparisons are outside the
timers. Every result is compared with the reference rows. Socket I/O, scheduling
between workers, and applying the parsed rows to retained state are outside this
benchmark; use FireTest and `runtime_bench` for those paths.

Measured on Windows x86-64 with Rust 1.96.0, GNU target, Release, on 2026-10-19.
The baseline is public commit `c856037`. Values below are medians of three run
medians, with five warmups and 60 measured iterations per case per run. Two
additional before/after pairs used the preserved executables alternately, with
no concurrent builds. Milliseconds are wall time. Mcycles are Windows thread
CPU cycles, not milliseconds inferred from a clock frequency.

| Fragment order | Diagnostic recorder | Receive ms, before → after | Receive Mcycles, before → after | Total ms, before → after | Total Mcycles, before → after |
| --- | --- | ---: | ---: | ---: | ---: |
| In order | Off | 9.204 → 4.543 | 34.580 → 17.173 | 32.337 → 27.654 | 120.587 → 103.524 |
| Reversed | Off | 8.476 → 4.835 | 31.868 → 18.281 | 28.572 → 24.678 | 107.810 → 92.856 |
| In order | On | 13.352 → 8.277 | 49.932 → 30.905 | 34.218 → 29.237 | 128.041 → 109.394 |
| Reversed | On | 13.020 → 8.558 | 48.975 → 32.269 | 33.428 → 28.697 | 125.429 → 107.746 |

The recorder switch isolates completion diagnostics inside the test harness;
it is not a comparison of whole normal and diagnostic runtimes. With recording
off, receive work fell by approximately 43–51%, while total processing fell by
approximately 14%. These figures describe this fixture and machine, not network
latency or a guaranteed improvement for every response size.
