# Time Values

MoonProto public API uses `MoonTime`: a compact Unix-milliseconds timestamp.
It is cheap to copy, works naturally with Rust/UI code, and can be converted to
`SystemTime` when a framework needs it.

The protocol still uses MoonBot wire time (`f64` days since `1899-12-30`) on
the wire. That is converted at packet boundaries. Application code should not
store or compare raw wire-day floats.

Report replication preserves the core database's local-clock date integers.
Choose timezone-only conversion or full clock-difference conversion below before
using them on UTC axes.

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

## Core UTC Offset

`client.server_utc_offset_minutes()` estimates the core's current timezone offset
from UTC in signed minutes, rounded to the nearest 15 minutes. Positive means
east of UTC: `Some(120)` means UTC+02:00, `Some(-300)` means UTC-05:00, and
`Some(345)` means UTC+05:45. `None` means no usable Ping yet; `Some(0)` is UTC.

The getter rounds the existing core-local-minus-station-UTC clock difference:

```text
offset_minutes = round(server_time_delta_ms / 900000) * 15
UTC timestamp = core-local timestamp - offset_minutes * 60000  // milliseconds
UTC timestamp = core-local timestamp - offset_minutes * 60     // seconds
```

For example, core time 14:00:05 against station UTC 12:00:01 gives +120 minutes.
Converting core-local 14:00:05 gives 12:00:05 UTC: the four-second clock error is
deliberately not corrected. Preserve missing dates and zero date sentinels.

Read the offset once per batch. The latest Ping is sufficient, including repeated
equal values: do not wait for the number to change, count distinct Pings, or run
an agreement window. Polling may compare the rounded offset with the previously
applied offset to update stored conversion state only when the offset changes.
This works without server logs, candle requests, or a diagnostics feature.

Each client's existing clock source updates on every usable Ping; a timezone
change is reflected by the next received Ping. The last value survives ordinary
reconnect and clears when the runtime owner is rebuilt or stops. No additional
sampling state or protocol fields are involved.

This is an estimate, not an explicitly reported timezone ID. It assumes offsets
on a 15-minute grid and combined clock error/network delay below 7.5 minutes;
halfway values round away from zero. It does not correct clock accuracy or
reconstruct historical timezone/DST changes. The station's local timezone is
irrelevant: the source uses its OS UTC clock.

## Core Clock And Report Dates

Use `server_utc_offset_minutes()` above when only the timezone should change.
The existing APIs below retain their full clock-difference semantics, including
clock error and packet delivery latency; they do not apply 15-minute rounding.

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
