//! # Shared Configuration (Safe-Share)
//!
//! MoonBot can export and import a safe subset of its settings as a compact
//! binary payload — the **safe-share** format. Payloads travel as `.mbshare`
//! files (base64-wrapped gzip) or as clipboard strings (`MBSC7:` prefix with
//! base16384-encoded gzip inside a ` ```mbcfg ` fence block).
//!
//! ## Editing model
//!
//! The full live configuration belongs to the MoonBot core. After `Ready`, the
//! runtime requests that base in the background and retries every five seconds
//! until it arrives. A terminal edits a value returned by
//! [`crate::MoonSettings::build_shared_config`] and sends it through
//! [`crate::MoonSettings::send_shared_config`]. The core then broadcasts fresh
//! compact and full settings snapshots to every connected terminal.
//!
//! Typical cycle after connection:
//! 1. Wait for the retained core [`SharedConfig`] or its settings event.
//! 2. Edit the fields you need.
//! 3. Send the result back to the kernel, or serialize it for sharing.
//!
//! [`SharedConfig::default()`] exists for the offline scenario and matches the
//! defaults of the current format. Active sessions deliberately refuse to build
//! an editable config until a real core snapshot has arrived, preventing an
//! early UI action from replacing a configured core with defaults.
//!
//! ## Unknown-tail preservation
//!
//! Each section carries an opaque `unknown_tail: Vec<u8>`. When parsing a
//! payload produced by a *newer* MoonBot version, bytes beyond the last field
//! this library knows about are captured there. On re-serialization they are
//! appended verbatim, so round-tripping a foreign config does not silently
//! discard new settings.

mod absorb;
mod clipboard;
mod sections;
#[cfg(test)]
mod tests;
mod wire;

pub use clipboard::{from_mbsc_string, from_mbshare_bytes, to_mbsc_string, to_mbshare_bytes};
pub(crate) use clipboard::{gzip_compress, gzip_decompress, MAX_COMPRESSED_SIZE};
pub use sections::*;
pub use wire::{parse_payload, serialize_payload, SharedConfigError};
