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
        minimum_core_version: "7.72 L7",
        comment: "7.72 L7 adds manual core-authoritative strategy snapshots without reconnecting. \
                  Update older cores before using that request; older Linux builds may crash on unknown Engine requests. \
                  7.72 L6 with service-settings support adds outgoing Telegram settings and watchdog control, \
                  as well as the prepared release version in core runtime state. \
                  Earlier builds omit settings_supported and watchdog state; hide mutation controls there. \
                  L5 supports startup-state recovery but does not report prepared releases. \
                  Older cores using a compatible protocol still connect, but may reach Ready \
                  with initial runtime, license, profit, Hyperliquid allowance or \
                  strategy-running states missing after packet loss.",
    }
}
