# Time Values

MoonProto public API uses `MoonTime`: a compact Unix-milliseconds timestamp.
It is cheap to copy, works naturally with Rust/UI code, and can be converted to
`SystemTime` when a framework needs it.

The protocol still uses MoonBot wire time (`f64` days since `1899-12-30`) on
the wire. That is converted at packet boundaries. Application code should not
store or compare raw wire-day floats.

Report replication preserves the core database's local-clock date integers.
Convert them with `ServerClock` as described below before using them on UTC axes.

```rust
use moonproto::MoonTime;

let now = MoonTime::now();
let unix_ms = now.unix_millis();
let unix_seconds = now.unix_seconds();
let system_time = now.system_time();
```

Common retained rows expose helper methods:

```rust
let trade_ms = trade.time().unix_millis();
let candle_ms = candle.time().unix_millis();
let last_price_time = point.time().system_time();
let order_open_ms = order.buy_order.open_time().unix_millis();
let trace_ms = chart_point.time().unix_millis();
```

Diagnostic builds keep hidden wire-time helpers for byte-level protocol tests.
They are not the normal terminal API.

## Core Clock And Report Dates

`client.server_time_delta_ms()` returns the latest core-local-minus-client-UTC
clock difference, rounded to signed milliseconds. `None` means no usable Ping
has arrived; `Some(0)` is a valid zero offset. No log subscription is needed:
this also works with `InitConfig { subscribe_logs: false, ..Default::default() }`.

For conversion, capture `client.server_clock()` once per report page or batch:

```rust
use moonproto::{MoonClient, MoonTime};

fn report_date(client: &MoonClient, raw_millis: i64) -> Option<MoonTime> {
    client.server_clock()?.report_millis_to_utc(raw_millis)
}
```

`ServerClock::report_millis_to_utc` handles `BuyDateMs`, `SellSetDateMs`,
`CloseDateMs`, and `BuySetDateMs`. `report_seconds_to_utc` handles their legacy
whole-second counterparts. Both subtract the captured offset and return UTC
`MoonTime`; a zero input means no date and returns `None`. Integer overflow also
returns `None`. Keep missing/NULL fields absent. Prefer a present millisecond
field, falling back to seconds only if it is absent, never if it is zero.
See [report timestamps](reports.md#report-timestamps) for a row example.

Each client has an independent estimate. Ping normally arrives every 300 ms
with activity/pending data, otherwise every second; packet loss or a stalled
connection can delay updates. Clock changes are visible on the next received
Ping. During reconnect the getter retains the last sample. Rebuilding the
runtime owner or terminating it clears the sample. A captured `ServerClock`
is an immutable value: take a new one for the next batch.

The estimate uses the station's OS UTC clock, not its local timezone or the
library's transport NTP correction. It includes clock error and packet delivery
latency; it is not a timezone identifier. The current estimate does not recover
historical daylight-saving/timezone changes. Already normalized live order,
trade, and candle timestamps must not be converted again. Replicated raw report
values remain unchanged, so existing database storage and replication keep their
original semantics.
