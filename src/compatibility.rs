/// Core requirements of this SDK build, independent of any connected client.
#[derive(Debug, Clone, Copy, serde::Serialize)]
pub struct CoreCompatibility {
    /// Minimum MoonBot version and build for the SDK's complete feature set.
    /// Individual features can still have platform or exchange requirements.
    pub minimum_core_version: &'static str,
    /// What needs that core version and how older compatible cores behave.
    pub comment: &'static str,
}

/// Return the core requirements compiled into this SDK.
///
/// This is descriptive metadata, not a connection gate or live feature detection.
/// Use [`crate::MoonClient::server_info`] to read the connected core's version.
///
/// ```
/// let compatibility = moonproto::core_compatibility();
/// println!("Minimum core: {}", compatibility.minimum_core_version);
/// println!("{}", compatibility.comment);
/// ```
pub const fn core_compatibility() -> CoreCompatibility {
    CoreCompatibility {
        minimum_core_version: "7.72 L6",
        comment: "7.72 L6 adds the prepared release version to core runtime state. \
                  L5 supports startup-state recovery but does not report prepared releases. \
                  Older cores using a compatible protocol still connect, but may reach Ready \
                  with initial runtime, license, profit, Hyperliquid allowance or \
                  strategy-running states missing after packet loss.",
    }
}
