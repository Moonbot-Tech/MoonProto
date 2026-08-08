//! Outer wrappers for shared-config payloads: gzip, base64 (.mbshare),
//! and clipboard format (MBSC7 + base16384 inside ```mbcfg fence).

use super::wire::{parse_payload, serialize_payload, SharedConfigError};
use super::SharedConfig;
use crate::commands::inflate::read_inflate_to_vec;
use flate2::read::{GzDecoder, GzEncoder};
use flate2::Compression;

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

const CLIPBOARD_PREFIX: &str = "MBSC7:";
const BASE16384_FIRST: u32 = 0x4E00;
const BASE16384_BITS: u32 = 14;
const BASE16384_MASK: u64 = (1u64 << BASE16384_BITS) - 1;
pub(crate) const MAX_COMPRESSED_SIZE: usize = 16 * 1024 * 1024;
const MAX_MBSHARE_SIZE: usize = MAX_COMPRESSED_SIZE.div_ceil(3) * 4;
const MAX_CLIPBOARD_TEXT_SIZE: usize =
    ((MAX_COMPRESSED_SIZE * 8).div_ceil(BASE16384_BITS as usize) * 3) + 4096;

// ---------------------------------------------------------------------------
// CRC-32 (IEEE / zlib polynomial 0xEDB88320)
// ---------------------------------------------------------------------------

fn crc32_ieee(data: &[u8]) -> u32 {
    let mut hasher = crc32fast::Hasher::new();
    hasher.update(data);
    hasher.finalize()
}

// ---------------------------------------------------------------------------
// Base16384 encode / decode
// ---------------------------------------------------------------------------

fn base16384_char_count(byte_count: usize) -> usize {
    (byte_count as u64 * 8 + BASE16384_BITS as u64 - 1) as usize / BASE16384_BITS as usize
}

fn encode_base16384(data: &[u8]) -> String {
    let char_count = base16384_char_count(data.len());
    let mut out = String::with_capacity(char_count * 3); // UTF-8 CJK = 3 bytes each
    let mut buf: u64 = 0;
    let mut bits: u32 = 0;

    for &b in data {
        buf |= (b as u64) << bits;
        bits += 8;
        while bits >= BASE16384_BITS {
            let ch = char::from_u32(BASE16384_FIRST + (buf & BASE16384_MASK) as u32).unwrap();
            out.push(ch);
            buf >>= BASE16384_BITS;
            bits -= BASE16384_BITS;
        }
    }
    if bits > 0 {
        let ch = char::from_u32(BASE16384_FIRST + buf as u32).unwrap();
        out.push(ch);
    }
    out
}

fn decode_base16384(encoded: &str, expected_size: usize) -> Result<Vec<u8>, SharedConfigError> {
    let mut result = Vec::with_capacity(expected_size);
    let mut buf: u64 = 0;
    let mut bits: u32 = 0;

    for ch in encoded.chars() {
        let code = ch as u32;
        if code < BASE16384_FIRST {
            return Err(SharedConfigError::from("wrong shared config text"));
        }
        let value = code - BASE16384_FIRST;
        if value as u64 > BASE16384_MASK {
            return Err(SharedConfigError::from("wrong shared config text"));
        }
        buf |= (value as u64) << bits;
        bits += BASE16384_BITS;

        while bits >= 8 && result.len() < expected_size {
            result.push((buf & 0xFF) as u8);
            buf >>= 8;
            bits -= 8;
        }
    }

    if result.len() != expected_size {
        return Err(SharedConfigError::from("wrong shared config text length"));
    }
    if bits > 0 && (buf & ((1u64 << bits) - 1)) != 0 {
        return Err(SharedConfigError::from("wrong shared config text padding"));
    }
    Ok(result)
}

// ---------------------------------------------------------------------------
// Gzip helpers
// ---------------------------------------------------------------------------

pub(crate) fn gzip_compress(data: &[u8]) -> Result<Vec<u8>, SharedConfigError> {
    if data.is_empty() || data.len() > super::wire::MAX_PAYLOAD_SIZE {
        return Err(SharedConfigError::from("shared config payload too large"));
    }
    let mut encoder = GzEncoder::new(data, Compression::default());
    read_inflate_to_vec(&mut encoder, data.len() / 4, MAX_COMPRESSED_SIZE)
        .map_err(|err| SharedConfigError::from(format!("gzip compress: {err}")))
}

pub(crate) fn gzip_decompress(data: &[u8]) -> Result<Vec<u8>, SharedConfigError> {
    if data.is_empty() || data.len() > MAX_COMPRESSED_SIZE {
        return Err(SharedConfigError::from("shared config gzip too large"));
    }
    let mut decoder = GzDecoder::new(data);
    read_inflate_to_vec(
        &mut decoder,
        data.len().saturating_mul(4),
        super::wire::MAX_PAYLOAD_SIZE,
    )
    .map_err(|err| SharedConfigError::from(format!("gzip decompress: {err}")))
}

// ---------------------------------------------------------------------------
// Hex8
// ---------------------------------------------------------------------------

fn hex8(v: u32) -> String {
    format!("{v:08X}")
}

fn parse_hex8(s: &str) -> Option<u32> {
    if s.len() != 8 {
        return None;
    }
    u32::from_str_radix(s, 16).ok()
}

// ---------------------------------------------------------------------------
// Clean clipboard text (strip all chars with code <= 32)
// ---------------------------------------------------------------------------

fn clean_clipboard_text(s: &str) -> String {
    s.chars().filter(|&c| c as u32 > 32).collect()
}

// ---------------------------------------------------------------------------
// Public API: .mbshare (base64 + gzip)
// ---------------------------------------------------------------------------

/// Serialize a [`SharedConfig`] into `.mbshare` file bytes (base64-encoded
/// gzip of the binary payload).
pub fn to_mbshare_bytes(cfg: &SharedConfig) -> Result<Vec<u8>, SharedConfigError> {
    use base64::Engine;
    let payload = serialize_payload(cfg)?;
    let zipped = gzip_compress(&payload)?;
    let b64 = base64::engine::general_purpose::STANDARD.encode(&zipped);
    Ok(b64.into_bytes())
}

/// Parse `.mbshare` file bytes into a [`SharedConfig`].
pub fn from_mbshare_bytes(data: &[u8]) -> Result<SharedConfig, SharedConfigError> {
    use base64::Engine;
    if data.len() > MAX_MBSHARE_SIZE {
        return Err(SharedConfigError::from("mbshare data too large"));
    }
    let b64_str = std::str::from_utf8(data)
        .map_err(|_| SharedConfigError::from("invalid mbshare encoding"))?;
    let zipped = base64::engine::general_purpose::STANDARD
        .decode(b64_str)
        .map_err(|_| SharedConfigError::from("invalid mbshare base64"))?;
    let payload = gzip_decompress(&zipped)?;
    parse_payload(&payload)
}

// ---------------------------------------------------------------------------
// Public API: clipboard (MBSC7 + base16384 in ```mbcfg fence)
// ---------------------------------------------------------------------------

/// Serialize a [`SharedConfig`] into the clipboard string format:
/// ` ```mbcfg\nMBSC7:...\n``` `.
pub fn to_mbsc_string(cfg: &SharedConfig) -> Result<String, SharedConfigError> {
    let payload = serialize_payload(cfg)?;
    let zipped = gzip_compress(&payload)?;
    let crc = crc32_ieee(&zipped);
    let encoded = encode_base16384(&zipped);
    let inner = format!(
        "{}{}:{}:{}",
        CLIPBOARD_PREFIX,
        hex8(zipped.len() as u32),
        hex8(crc),
        encoded
    );
    Ok(format!("```mbcfg\n{inner}\n```"))
}

/// Parse a clipboard string (possibly with surrounding text/fences) into a
/// [`SharedConfig`]. Whitespace and surrounding text/fences are accepted.
pub fn from_mbsc_string(s: &str) -> Result<SharedConfig, SharedConfigError> {
    if s.len() > MAX_CLIPBOARD_TEXT_SIZE {
        return Err(SharedConfigError::from("shared config text too large"));
    }
    let clean = clean_clipboard_text(s);

    // Search for the MBSC7: prefix.
    let mut search_from = 0usize;
    loop {
        let found = match clean[search_from..].find(CLIPBOARD_PREFIX) {
            Some(rel) => search_from + rel,
            None => return Err(SharedConfigError::from("MBSC7 prefix not found")),
        };

        let after_prefix = found + CLIPBOARD_PREFIX.len();
        let candidate = &clean[after_prefix..];

        // Need at least 8 + 1 + 8 + 1 chars for size:crc: header.
        if candidate.len() < 18 {
            search_from = after_prefix;
            continue;
        }

        let header = &candidate.as_bytes()[..18];
        if !header[..8].is_ascii()
            || header[8] != b':'
            || !header[9..17].is_ascii()
            || header[17] != b':'
        {
            search_from = after_prefix;
            continue;
        }

        let size_hex = std::str::from_utf8(&header[..8]).expect("ASCII header was checked");
        let zipped_size = match parse_hex8(size_hex) {
            Some(v) if v > 0 && v as usize <= MAX_COMPRESSED_SIZE => v,
            _ => {
                search_from = after_prefix;
                continue;
            }
        };

        let crc_hex = std::str::from_utf8(&header[9..17]).expect("ASCII header was checked");
        let expected_crc = match parse_hex8(crc_hex) {
            Some(v) => v,
            None => {
                search_from = after_prefix;
                continue;
            }
        };

        let data_start = 18;
        let encoded_chars = base16384_char_count(zipped_size as usize);
        let data_str: String = candidate[data_start..]
            .chars()
            .take(encoded_chars)
            .collect();
        if data_str.chars().count() < encoded_chars {
            search_from = after_prefix;
            continue;
        }

        let zipped = decode_base16384(&data_str, zipped_size as usize)?;
        let actual_crc = crc32_ieee(&zipped);
        if actual_crc != expected_crc {
            return Err(SharedConfigError::from("CRC32 mismatch"));
        }
        let payload = gzip_decompress(&zipped)?;
        return parse_payload(&payload);
    }
}

// ---------------------------------------------------------------------------
// Module-internal test helpers
// ---------------------------------------------------------------------------

#[cfg(test)]
pub(super) fn test_encode_base16384(data: &[u8]) -> String {
    encode_base16384(data)
}

#[cfg(test)]
pub(super) fn test_decode_base16384(
    encoded: &str,
    expected_size: usize,
) -> Result<Vec<u8>, SharedConfigError> {
    decode_base16384(encoded, expected_size)
}

#[cfg(test)]
pub(super) fn test_crc32_ieee(data: &[u8]) -> u32 {
    crc32_ieee(data)
}
