use super::{bytes_to_wire_mb, percent_to_wire, PingMemoryInfo, PingTelemetry};
use windows_sys::Win32::Foundation::FILETIME;
use windows_sys::Win32::System::ProcessStatus::{GetProcessMemoryInfo, PROCESS_MEMORY_COUNTERS};
use windows_sys::Win32::System::SystemInformation::{GlobalMemoryStatusEx, MEMORYSTATUSEX};
use windows_sys::Win32::System::Threading::{
    GetActiveProcessorCount, GetActiveProcessorGroupCount, GetCurrentProcess, GetProcessTimes,
    GetSystemTimes,
};

#[derive(Clone, Copy)]
struct CpuTimes {
    process: u64,
    system: u64,
    idle: u64,
}

impl CpuTimes {
    fn read() -> Option<Self> {
        unsafe {
            let mut creation: FILETIME = std::mem::zeroed();
            let mut exit: FILETIME = std::mem::zeroed();
            let mut kernel: FILETIME = std::mem::zeroed();
            let mut user: FILETIME = std::mem::zeroed();
            if GetProcessTimes(GetCurrentProcess(), &mut creation, &mut exit, &mut kernel, &mut user) == 0 {
                return None;
            }
            let ticks = |t: FILETIME| (u64::from(t.dwHighDateTime) << 32) | u64::from(t.dwLowDateTime);
            let process = ticks(kernel) + ticks(user);
            let mut idle: FILETIME = std::mem::zeroed();
            if GetSystemTimes(&mut idle, &mut kernel, &mut user) == 0 {
                return None;
            }
            Some(Self { process, system: ticks(kernel) + ticks(user), idle: ticks(idle) })
        }
    }

    fn usage_since(self, previous: Self) -> Option<(u8, u8)> {
        let system = self.system.checked_sub(previous.system)?;
        let process = self.process.checked_sub(previous.process)?;
        let idle = self.idle.checked_sub(previous.idle)?;
        if system == 0 {
            return None;
        }
        // GetSystemTimes includes idle in kernel time and sums logical CPUs.
        Some((
            percent_to_wire(100.0 * process as f32 / system as f32),
            percent_to_wire(100.0 * system.saturating_sub(idle) as f32 / system as f32),
        ))
    }
}

pub(super) struct Sampler {
    previous: Option<CpuTimes>,
    sample: PingTelemetry,
    cores: u8,
}

impl Sampler {
    pub(super) fn new() -> Option<Self> {
        // GetSystemTimes covers only the calling processor group. Leave multi-
        // group hosts on sysinfo rather than reporting one group as the machine.
        if unsafe { GetActiveProcessorGroupCount() } != 1 {
            return None;
        }
        Some(Self {
            previous: None,
            sample: PingTelemetry { moment_cpu_percent: 0, total_cpu_percent: 0, memory: None },
            cores: unsafe { GetActiveProcessorCount(0) }.min(u32::from(u8::MAX)) as u8,
        })
    }

    pub(super) fn sample(&mut self, include_memory: bool) -> PingTelemetry {
        if let Some(now) = CpuTimes::read() {
            if let Some(usage) = self.previous.and_then(|previous| now.usage_since(previous)) {
                self.sample.moment_cpu_percent = usage.0;
                self.sample.total_cpu_percent = usage.1;
            }
            self.previous = Some(now);
        }
        self.sample.memory = None;
        if include_memory {
            unsafe {
                let mut process: PROCESS_MEMORY_COUNTERS = std::mem::zeroed();
                let mut system: MEMORYSTATUSEX = std::mem::zeroed();
                system.dwLength = std::mem::size_of_val(&system) as u32;
                if GetProcessMemoryInfo(GetCurrentProcess(), &mut process, std::mem::size_of_val(&process) as u32) != 0
                    && GlobalMemoryStatusEx(&mut system) != 0
                {
                    self.sample.memory = Some(PingMemoryInfo {
                        used_memory_mb: bytes_to_wire_mb(process.WorkingSetSize as u64),
                        free_physical_memory_mb: bytes_to_wire_mb(system.ullAvailPhys),
                        cores: self.cores,
                    });
                }
            }
        }
        self.sample
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cpu_percent_is_normalized_to_the_machine() {
        let previous = CpuTimes { process: 100, system: 1_000, idle: 500 };
        let now = CpuTimes { process: 150, system: 1_200, idle: 600 };
        assert_eq!(now.usage_since(previous), Some((25, 50)));
        assert_eq!(previous.usage_since(previous), None);
        assert_eq!(previous.usage_since(now), None);
    }

    #[test]
    fn native_memory_and_optional_tail() {
        let Some(mut sampler) = Sampler::new() else { return };
        let memory = sampler.sample(true).memory.unwrap();
        assert!(memory.used_memory_mb > 0);
        assert!(memory.cores > 0);
        assert!(sampler.sample(false).memory.is_none());
    }

    #[test]
    #[ignore = "manual Windows telemetry cost measurement"]
    fn compare_sampler_cost() {
        use windows_sys::Win32::System::Threading::GetCurrentThread;
        use windows_sys::Win32::System::WindowsProgramming::QueryThreadCycleTime;
        let cycles = || {
            let mut value = 0;
            assert_ne!(unsafe { QueryThreadCycleTime(GetCurrentThread(), &mut value) }, 0);
            value
        };
        let Some(mut native) = Sampler::new() else { return };
        let mut system = sysinfo::System::new();
        let pid = sysinfo::get_current_pid().ok();
        let (mut old_cost, mut new_cost) = (0, 0);
        for i in 0..21 {
            let start = cycles();
            let old = super::super::refresh_system(&mut system, pid, i % 5 == 0);
            let middle = cycles();
            let new = native.sample(i % 5 == 0);
            let end = cycles();
            if i > 0 {
                old_cost += middle - start;
                new_cost += end - middle;
                eprintln!("sample={i} sysinfo={old:?} native={new:?}");
            }
            std::thread::sleep(std::time::Duration::from_secs(1));
        }
        eprintln!("cycles_per_sample sysinfo={} native={}", old_cost / 20, new_cost / 20);
    }
}
