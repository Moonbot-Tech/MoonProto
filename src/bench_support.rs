//! Test-only allocation counting and synchronous CPU benchmarks.
use crate::client::thread_cpu::ThreadCpuTimer;
use std::alloc::{GlobalAlloc, Layout, System};
use std::cell::Cell;
use std::time::Instant;

struct CountingAllocator;
thread_local! {
    pub(crate) static COUNTS: Cell<Option<(u64, u64)>> = const { Cell::new(None) };
}

fn allocated(size: usize) {
    let _ = COUNTS.try_with(|counter| {
        if let Some((calls, bytes)) = counter.get() {
            counter.set(Some((calls + 1, bytes + size as u64)));
        }
    });
}

unsafe impl GlobalAlloc for CountingAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        allocated(layout.size());
        unsafe { System.alloc(layout) }
    }
    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        allocated(layout.size());
        unsafe { System.alloc_zeroed(layout) }
    }
    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        unsafe { System.dealloc(ptr, layout) }
    }
    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, size: usize) -> *mut u8 {
        allocated(size);
        unsafe { System.realloc(ptr, layout, size) }
    }
}

#[global_allocator]
static ALLOCATOR: CountingAllocator = CountingAllocator;

pub(crate) fn percentile(samples: &mut [f64], fraction: f64) -> f64 {
    if samples.is_empty() {
        return f64::NAN;
    }
    samples.sort_by(f64::total_cmp);
    samples[((samples.len() - 1) as f64 * fraction).round() as usize]
}

pub(crate) fn measure(name: &str, calls: usize, mut run: impl FnMut()) {
    let mut wall = Vec::new();
    let mut cycles = Vec::new();
    let mut allocations = (0, 0);
    for iteration in 0..25 {
        COUNTS.with(|counter| counter.set(Some((0, 0))));
        let cpu = ThreadCpuTimer::start();
        let start = Instant::now();
        for _ in 0..calls {
            run();
        }
        let elapsed = start.elapsed().as_secs_f64() * 1000.0;
        let elapsed_cpu = cpu.elapsed();
        let counts = COUNTS.with(|counter| counter.replace(None).unwrap());
        if iteration >= 5 {
            wall.push(elapsed);
            if let Some(value) = elapsed_cpu.cycles {
                cycles.push(value as f64 / 1e6);
            }
            allocations.0 += counts.0;
            allocations.1 += counts.1;
        }
    }
    println!("FOLLOWUP case={name} calls={calls} samples=20 ms_p50={:.6} ms_p95={:.6} Mcycles_p50={:.6} allocs_per_call={:.4} bytes_per_call={:.2}",
        percentile(&mut wall, 0.5), percentile(&mut wall, 0.95), percentile(&mut cycles, 0.5),
        allocations.0 as f64 / (20 * calls) as f64, allocations.1 as f64 / (20 * calls) as f64);
}
