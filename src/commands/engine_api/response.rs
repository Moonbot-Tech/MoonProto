//! `TEngineResponse` parser.

use super::{read_i32_zero_tail, read_u64_zero_tail, read_u8_zero_tail, EngineMethod};
use crate::commands::inflate::{read_inflate_to_vec, MAX_INFLATE_OUTPUT_SIZE};
use crate::commands::registry::read_string;
use crate::protocol::payload::Payload;
use flate2::bufread::DeflateDecoder;
use std::ops::Range;

/// Parsed Engine API response (`TEngineResponse`, server to client).
#[derive(Debug, Clone)]
pub struct EngineResponse {
    /// Delphi `TBaseCommand.ver` from the response header.
    pub ver: u16,
    /// UID of the original `TEngineRequest`.
    pub request_uid: u64,
    /// Engine method echoed by the server.
    pub method: EngineMethod,
    /// Server success flag.
    pub success: bool,
    /// Server error code when `success == false`.
    pub error_code: i32,
    /// Server error message when `success == false`.
    pub error_msg: String,
    /// Method-specific response payload, already DEFLATE-decompressed when the
    /// wire response was compressed.
    pub data: Vec<u8>,
}

/// Parse TEngineResponse from command payload.
///
/// **Wire-format** (after Crypted decrypt + CryptoHeader strip, payload **with** the Engine
/// TBaseCommand header):
/// ```text
/// [CmdId(1)=1][ver(2)][own_UID(8)][RequestUID(8)][Method(1)][Success(1)][ErrorCode(4)][ErrorMsg(string)][IsCompressed(1)][DataSize(4)][Data]
/// ```
///
/// The Engine TBaseCommand header (11 bytes: `CmdId + ver + own_UID`) is **skipped**
/// before reading `RequestUID` — matching Delphi `TEngineResponse.CreateFromStream`,
/// which via `inherited CreateFromStream` (TBaseCommand) first reads ver+UID,
/// then its own fields.
///
/// **Historical bug** (fixed): the parser used to start at `pos=0`, reading
/// `[ver][own_UID first 5 bytes]` as `request_uid` — it never matched the
/// registered uid -> all Engine API responses were lost (BaseCheck/AuthCheck/
/// GetMarketsList timeouts).
///
/// Matches `MoonProtoEngineStruct.pas:364-403`.
pub(crate) fn parse_engine_response(data: &[u8]) -> Option<EngineResponse> {
    let (mut response, range, compressed) = parse_response_header(data)?;
    response.data = decode_body(&data[range], compressed, response.request_uid)?;
    Some(response)
}

/// On success, transfer the body out of the receive buffer. Failed parsing
/// leaves the original bytes available to the ordinary delivery path.
pub(crate) fn parse_engine_response_owned(payload: &mut Payload) -> Option<(EngineResponse, Payload)> {
    let (response, range, compressed) = parse_response_header(payload)?;
    let body = if compressed {
        let body = decode_body(&payload[range], true, response.request_uid)?;
        *payload = Payload::default();
        body.into()
    } else {
        let mut body = std::mem::take(payload);
        body.restrict(range);
        body
    };
    Some((response, body))
}

fn parse_response_header(data: &[u8]) -> Option<(EngineResponse, Range<usize>, bool)> {
    if data.len() < 11 {
        return None;
    }
    let ver = u16::from_le_bytes(data[1..3].try_into().unwrap());
    // Skip Engine TBaseCommand header: CmdId(1) + ver(2) + own_UID(8) = 11 bytes.
    let mut pos = 11usize;

    let request_uid = read_u64_zero_tail(data, &mut pos);
    let method = EngineMethod::from_byte(read_u8_zero_tail(data, &mut pos));
    let success = read_u8_zero_tail(data, &mut pos) != 0;
    let error_code = read_i32_zero_tail(data, &mut pos);

    let error_msg = read_string(data, &mut pos)?;

    // Delphi uses TMemoryStream.Read for these scalar fields. Missing tail bytes
    // stay zero after the strict ErrorMsg string has already been read.
    let is_compressed = read_u8_zero_tail(data, &mut pos) != 0;
    let sz = read_i32_zero_tail(data, &mut pos);

    let end = pos + data.len().saturating_sub(pos).min(sz.max(0) as usize);
    Some((
        EngineResponse {
            ver,
            request_uid,
            method,
            success,
            error_code,
            error_msg,
            data: Vec::new(),
        },
        pos..end,
        is_compressed && sz > 0,
    ))
}

fn decode_body(raw: &[u8], compressed: bool, request_uid: u64) -> Option<Vec<u8>> {
    if !compressed {
        return Some(raw.to_vec());
    }
    let mut decoder = DeflateDecoder::new(raw);
    read_inflate_to_vec(&mut decoder, raw.len().saturating_mul(8), MAX_INFLATE_OUTPUT_SIZE)
        .map_err(|e| {
            log::warn!(target: "moonproto::engine_api",
                "DEFLATE decompress failed for EngineResponse uid={}: {}", request_uid, e);
        })
        .ok()
}
