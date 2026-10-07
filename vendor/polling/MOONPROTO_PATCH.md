# Local polling patch

This is polling 3.11.0 from crates.io, under its original Apache-2.0 OR MIT license.
Source: https://github.com/smol-rs/polling/tree/v3.11.0

In `src/iocp/mod.rs`, after processing
IOCP completions, `wait_deadline` checks the current time against the deadline
instead of testing the timeout computed before the OS wait. An empty timeout
therefore returns without a second zero-timeout OS wait. Readiness and notify
completions are still processed before returning; infinite waits are unchanged.

In `src/epoll.rs`, Linux/Android waits use `ppoll` on the epoll descriptor,
then collect ready events with a nonblocking epoll wait. This removes timerfd
arming and registration from each wait while preserving sub-millisecond
timeouts without requiring Linux 5.11's `epoll_pwait2`. The epoll descriptor
becomes readable when its ready list is nonempty (see
[`epoll(7)`](https://man7.org/linux/man-pages/man7/epoll.7.html)). The internal
notifier uses level triggering and is read only when reported; it no longer
needs an `epoll_ctl` rearm after every wait. The outer wait lock and absolute
deadline retry after signals are unchanged. Redox retains its native epoll
timeout. Other platform backends are unchanged.

MoonProto uses a direct path dependency so this fix also applies when MoonProto
is a dependency of another application. A root-only `[patch.crates-io]` would not
propagate to those applications. Remove this copy when an upstream release
provides the equivalent fix and passes the UDP readiness tests.
