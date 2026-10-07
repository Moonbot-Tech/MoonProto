//! Diagnostics-only per-thread CPU clock.
//!
//! `Instant` measures elapsed wall time inside a protocol segment, so it also
//! includes scheduler preemption. FireTest uses this helper beside wall timings
//! to tell "the code spent CPU" from "the OS paused this thread".

use std::time::{Duration, Instant};

/// OS-visible roles let an external process sampler attribute worker CPU.
pub(crate) fn set_diagnostic_thread_name(name: &str) {
    #[cfg(windows)]
    {
        use windows_sys::Win32::System::Threading::{GetCurrentThread, SetThreadDescription};
        let name: Vec<u16> = name.encode_utf16().chain(Some(0)).collect();
        unsafe {
            SetThreadDescription(GetCurrentThread(), name.as_ptr());
        }
    }
    #[cfg(target_os = "linux")]
    {
        let mut short_name = [0u8; 16];
        let len = name.len().min(15);
        short_name[..len].copy_from_slice(&name.as_bytes()[..len]);
        unsafe { libc::pthread_setname_np(libc::pthread_self(), short_name.as_ptr().cast()); }
    }
    #[cfg(not(any(windows, target_os = "linux")))]
    let _ = name;
}

/// Paired wall/CPU sample. Nested phases are inclusive and must not be summed.
pub(crate) struct ProfileTimer {
    wall: Instant,
    cpu: Option<ThreadCpuTimer>,
}

pub(crate) struct ProfileElapsed {
    pub(crate) wall: Duration,
    pub(crate) cpu: ThreadCpuElapsed,
}

impl ProfileElapsed {
    pub(crate) fn excluding_wait(mut self, wait: Duration) -> Self {
        self.wall = self.wall.saturating_sub(wait);
        self
    }
}

impl ProfileTimer {
    pub(crate) fn start() -> Self {
        // Randomized 1/64 sampling avoids a pair of OS calls for every tiny
        // phase on every idle tick. A fixed periodic sample would alias phases.
        // Distinct thread seeds keep identical clients from sampling the same
        // positions in a broadcast stream.
        static NEXT_SEED: std::sync::atomic::AtomicU64 =
            std::sync::atomic::AtomicU64::new(0x91a5_2f47_d638_ce0b);
        thread_local! {
            static SAMPLE: std::cell::Cell<u64> = std::cell::Cell::new(
                NEXT_SEED.fetch_add(0x9e37_79b9_7f4a_7c15, std::sync::atomic::Ordering::Relaxed));
        }
        let sample = SAMPLE.with(|state| {
            let next = state
                .get()
                .wrapping_mul(6364136223846793005)
                .wrapping_add(1);
            state.set(next);
            next >> 58 == 0
        });
        Self {
            wall: Instant::now(),
            cpu: sample.then(ThreadCpuTimer::start),
        }
    }

    pub(crate) fn elapsed(self) -> ProfileElapsed {
        let cpu = self
            .cpu
            .map(ThreadCpuTimer::elapsed)
            .unwrap_or(ThreadCpuElapsed {
                time: None,
                cycles: None,
            });
        ProfileElapsed {
            wall: self.wall.elapsed(),
            cpu,
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct ThreadCpuTimer {
    start_time: Option<Duration>,
    start_cycles: Option<u64>,
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct ThreadCpuElapsed {
    pub(crate) time: Option<Duration>,
    pub(crate) cycles: Option<u64>,
}

impl ThreadCpuTimer {
    pub(crate) fn start() -> Self {
        Self {
            start_time: thread_cpu_time(),
            start_cycles: thread_cpu_cycles(),
        }
    }

    pub(crate) fn elapsed(self) -> ThreadCpuElapsed {
        ThreadCpuElapsed {
            time: self
                .start_time
                .and_then(|start| Some(thread_cpu_time()?.saturating_sub(start))),
            cycles: self
                .start_cycles
                .and_then(|start| Some(thread_cpu_cycles()?.saturating_sub(start))),
        }
    }
}

#[cfg(windows)]
fn thread_cpu_time() -> Option<Duration> {
    // `GetThreadTimes` has coarse tick granularity on Windows (commonly
    // 15.625ms), which is worse than useless for sub-millisecond protocol
    // segments. Use QueryThreadCycleTime instead and leave duration empty.
    None
}

#[cfg(windows)]
fn thread_cpu_cycles() -> Option<u64> {
    use windows_sys::Win32::System::Threading::GetCurrentThread;
    use windows_sys::Win32::System::WindowsProgramming::QueryThreadCycleTime;

    let mut cycles = 0u64;
    let ok = unsafe { QueryThreadCycleTime(GetCurrentThread(), &mut cycles) };
    (ok != 0).then_some(cycles)
}

#[cfg(unix)]
fn thread_cpu_time() -> Option<Duration> {
    let mut ts = std::mem::MaybeUninit::<libc::timespec>::uninit();
    let ok = unsafe { libc::clock_gettime(libc::CLOCK_THREAD_CPUTIME_ID, ts.as_mut_ptr()) };
    if ok != 0 {
        return None;
    }
    let ts = unsafe { ts.assume_init() };
    let secs = u64::try_from(ts.tv_sec).ok()?;
    let nanos = u32::try_from(ts.tv_nsec).ok()?;
    Some(Duration::new(secs, nanos))
}

#[cfg(unix)]
fn thread_cpu_cycles() -> Option<u64> {
    None
}

#[cfg(not(any(windows, unix)))]
fn thread_cpu_time() -> Option<Duration> {
    None
}

#[cfg(not(any(windows, unix)))]
fn thread_cpu_cycles() -> Option<u64> {
    None
}
