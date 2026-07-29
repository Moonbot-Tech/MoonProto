/// Runtime telemetry reported by the connected MoonBot core in protocol v4 Ping.
///
/// CPU and latency values are refreshed on every Ping. Memory is a lower-rate
/// tail and is therefore `None` until the first memory-bearing Ping arrives;
/// afterwards the last reported values remain available between memory samples.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct KernelHealth {
    /// MoonBot process CPU usage as a percentage of the whole machine.
    pub process_cpu_percent: u8,
    /// Total CPU usage of the machine running MoonBot.
    pub system_cpu_percent: u8,
    /// MoonBot process memory in decimal megabytes.
    pub used_memory_mb: Option<u16>,
    /// Available physical memory on the machine in decimal megabytes.
    pub free_physical_memory_mb: Option<u16>,
    /// Logical CPU count reported with the same periodic memory profile.
    pub logical_cpu_count: Option<u8>,
    /// Smoothed full UDP round-trip time between this client and the core.
    ///
    /// `None` until the core has measured at least one Ping response. A UI that
    /// wants the same one-way estimate shown by MoonBot can display half of this
    /// value.
    pub core_round_trip_ms: Option<u32>,
    /// Smoothed duration of order-related requests from the core to the exchange.
    ///
    /// This measures real order API request/response paths, not a standalone
    /// exchange REST ping. It reacts quickly to slower samples and decays more
    /// gradually when latency improves. `None` means the core has no sample yet.
    pub order_api_latency_ms: Option<u16>,
}
