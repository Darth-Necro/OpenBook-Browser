// SPDX-License-Identifier: MPL-2.0
//
// Native-messaging wire protocol for the OpenBook vault host.
//
// Transport (Firefox native messaging framing):
//   [4-byte message length in NATIVE byte order][that many bytes of UTF-8 JSON]
//
// We enforce a hard 1 MiB cap on a single message; larger frames are rejected as
// `invalid-request` WITHOUT allocating the claimed size (so a hostile/garbage
// length field cannot drive us to allocate gigabytes). On EOF the loop exits
// cleanly (handled in `main.rs`).
//
// `parse_frame` operates on an already-collected byte slice so the parser is
// directly unit- and fuzz-testable in isolation from any I/O.

use std::io::{self, Read, Write};

use serde::{Deserialize, Serialize};

use crate::error::{ErrorCode, VaultError};

/// Maximum accepted single-message size (header-declared length), in bytes.
/// Firefox itself caps host->browser messages at 1 MiB; we mirror that for the
/// inbound direction and refuse anything larger up front.
pub const MAX_MESSAGE_LEN: usize = 1024 * 1024;

/// A successfully framed-and-parsed inbound request.
///
/// We model the request as a tagged enum keyed on the JSON `type` field. `id` is
/// required on every well-formed request and echoed back on every response.
/// Unknown `type` values are turned into `Unknown { id }` by `parse_frame`
/// (which recovers the id from the raw JSON first), so the dispatcher can answer
/// them with a clean `invalid-request` that still echoes `id` when present.
#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "type")]
pub enum Request {
    #[serde(rename = "status")]
    Status { id: i64 },

    #[serde(rename = "setup")]
    Setup {
        id: i64,
        secret: String,
        /// Wire key is `maxAttempts`. Defaults to 6 when omitted.
        #[serde(rename = "maxAttempts", default = "default_max_attempts")]
        max_attempts: u32,
        /// Wire key is `acknowledgeNoRecovery`. Must be exactly `true`.
        #[serde(rename = "acknowledgeNoRecovery", default)]
        acknowledge_no_recovery: bool,
    },

    #[serde(rename = "unlock")]
    Unlock { id: i64, secret: String },

    #[serde(rename = "lock")]
    Lock { id: i64 },

    #[serde(rename = "erase")]
    Erase {
        id: i64,
        #[serde(default)]
        confirm: bool,
    },

    /// Catch-all for any unrecognized `type`. Carries the id recovered from the
    /// raw JSON by `parse_frame` so the error response can still echo it. This
    /// variant is never produced by serde directly (it would require an exact
    /// `"type":"__unknown__"`); `parse_frame` constructs it.
    #[serde(rename = "__unknown__")]
    Unknown { id: Option<i64> },
}

fn default_max_attempts() -> u32 {
    6
}

impl Request {
    /// The request id for this request, including the recovered id on `Unknown`.
    /// Returns `None` only when no usable numeric id was present in the JSON.
    pub fn id(&self) -> Option<i64> {
        match self {
            Request::Status { id }
            | Request::Setup { id, .. }
            | Request::Unlock { id, .. }
            | Request::Lock { id }
            | Request::Erase { id, .. } => Some(*id),
            Request::Unknown { id } => *id,
        }
    }
}

/// Outbound response. We build responses as `serde_json::Value` in the engine to
/// keep the per-request shapes flexible (different success/error fields), but
/// provide typed constructors here for the common cases so call sites stay
/// honest about the wire contract.
#[derive(Debug, Clone, Serialize)]
pub struct ErrorResponse {
    pub id: i64,
    pub ok: bool,
    pub error: &'static str,
    pub message: String,
}

impl ErrorResponse {
    /// Build an error response from a `VaultError`, echoing the given id. When
    /// the request id could not be parsed we use `0` (documented sentinel).
    pub fn from_error(id: Option<i64>, err: &VaultError) -> Self {
        ErrorResponse {
            id: id.unwrap_or(0),
            ok: false,
            error: err.code.as_str(),
            message: err.message.clone(),
        }
    }

    /// Convenience for a bare code with a fixed id.
    pub fn code(id: Option<i64>, code: ErrorCode, message: impl Into<String>) -> Self {
        ErrorResponse {
            id: id.unwrap_or(0),
            ok: false,
            error: code.as_str(),
            message: message.into(),
        }
    }
}

/// Parse a single complete message payload (the JSON bytes, *without* the length
/// header) into a `Request`.
///
/// Errors:
/// - `invalid-request` if the bytes are not valid UTF-8, not valid JSON, or do
///   not match a known request object shape (missing `id`/`secret`, etc).
///
/// This function never panics on arbitrary input and never allocates more than
/// the input itself — both properties are exercised by the robustness tests and
/// the cargo-fuzz target.
pub fn parse_frame(payload: &[u8]) -> Result<Request, VaultError> {
    // Enforce the cap defensively here too, in case a caller hands us an
    // over-large buffer assembled by some other path.
    if payload.len() > MAX_MESSAGE_LEN {
        return Err(VaultError::invalid_request("message exceeds 1 MiB limit"));
    }
    // Validate UTF-8 explicitly so we return a clean code rather than relying on
    // serde's error text.
    let text = std::str::from_utf8(payload)
        .map_err(|_| VaultError::invalid_request("message is not valid UTF-8"))?;

    // Stage 1: a permissive probe to (a) confirm it's a JSON object, (b) recover
    // the id even when other fields are wrong/missing, and (c) detect an unknown
    // `type` so we can map it to `Unknown { id }` rather than a hard parse error.
    let probe: Probe = serde_json::from_str(text)
        .map_err(|e| VaultError::invalid_request(format!("malformed request JSON: {e}")))?;

    // An object with no `type` at all is not a valid request.
    let ty = match probe.ty.as_deref() {
        Some(t) => t,
        None => return Err(VaultError::invalid_request("request is missing `type`")),
    };

    if !is_known_type(ty) {
        // Unknown type: recover id (if numeric) and return the catch-all.
        return Ok(Request::Unknown { id: probe.id });
    }

    // Stage 2: strict typed parse for known types (enforces required fields like
    // `secret` and field types). Errors here are invalid-request.
    serde_json::from_str::<Request>(text)
        .map_err(|e| VaultError::invalid_request(format!("malformed request JSON: {e}")))
}

/// Permissive probe used by `parse_frame` to recover `id`/`type` before strict
/// typed deserialization. `id` is optional and only captured when it is a JSON
/// number that fits in i64 (serde will error the whole probe on a non-number id,
/// which we treat as invalid-request — acceptable, since a non-numeric id is
/// already a protocol violation).
#[derive(Deserialize)]
struct Probe {
    #[serde(rename = "type")]
    ty: Option<String>,
    #[serde(default)]
    id: Option<i64>,
}

fn is_known_type(ty: &str) -> bool {
    matches!(ty, "status" | "setup" | "unlock" | "lock" | "erase")
}

/// Outcome of attempting to read one frame from a stream.
pub enum FrameRead {
    /// A complete payload was read (length header consumed, JSON bytes returned).
    Message(Vec<u8>),
    /// Clean EOF before any byte of a new frame — caller should exit 0.
    Eof,
    /// The frame was structurally invalid at the transport level (e.g. declared
    /// length exceeds the cap, or the stream ended mid-frame). The payload could
    /// not be trusted/collected; caller should emit an `invalid-request` (with
    /// id 0, since we never saw the body) and then decide whether to continue.
    /// We return the offending declared length for diagnostics/tests.
    Invalid(VaultError),
}

/// Read exactly one length-prefixed frame from `reader`.
///
/// Behavior:
/// - EOF with zero bytes consumed -> `FrameRead::Eof`.
/// - EOF after a partial read -> `FrameRead::Invalid` (truncated frame).
/// - Declared length > `MAX_MESSAGE_LEN` -> `FrameRead::Invalid` and we do NOT
///   read/allocate the body (we cannot safely skip an unknown-but-huge amount,
///   so the caller will typically stop reading this stream).
pub fn read_frame<R: Read>(reader: &mut R) -> io::Result<FrameRead> {
    let mut len_buf = [0u8; 4];
    // Distinguish "clean EOF at frame boundary" from "EOF mid-header".
    match read_exact_or_eof(reader, &mut len_buf)? {
        ReadExact::Eof => return Ok(FrameRead::Eof),
        ReadExact::Partial => {
            return Ok(FrameRead::Invalid(VaultError::invalid_request(
                "truncated length header",
            )))
        }
        ReadExact::Full => {}
    }

    // NATIVE byte order per the native-messaging spec.
    let declared = u32::from_ne_bytes(len_buf) as usize;
    if declared > MAX_MESSAGE_LEN {
        return Ok(FrameRead::Invalid(VaultError::invalid_request(format!(
            "declared message length {declared} exceeds 1 MiB limit"
        ))));
    }

    let mut payload = vec![0u8; declared];
    match read_exact_or_eof(reader, &mut payload)? {
        ReadExact::Full => Ok(FrameRead::Message(payload)),
        ReadExact::Eof | ReadExact::Partial => Ok(FrameRead::Invalid(
            VaultError::invalid_request("truncated message body"),
        )),
    }
}

enum ReadExact {
    Full,
    /// Zero bytes were read (clean EOF).
    Eof,
    /// Some but not all bytes were read before EOF.
    Partial,
}

/// Like `Read::read_exact` but reports clean EOF (zero bytes) distinctly from a
/// partial read, and treats `Interrupted` as retryable.
fn read_exact_or_eof<R: Read>(reader: &mut R, buf: &mut [u8]) -> io::Result<ReadExact> {
    if buf.is_empty() {
        return Ok(ReadExact::Full);
    }
    let mut filled = 0usize;
    while filled < buf.len() {
        match reader.read(&mut buf[filled..]) {
            Ok(0) => {
                return Ok(if filled == 0 {
                    ReadExact::Eof
                } else {
                    ReadExact::Partial
                });
            }
            Ok(n) => filled += n,
            Err(ref e) if e.kind() == io::ErrorKind::Interrupted => continue,
            Err(e) => return Err(e),
        }
    }
    Ok(ReadExact::Full)
}

/// Serialize `value` to JSON and write it as a length-prefixed frame.
///
/// The serialized form is capped at `MAX_MESSAGE_LEN`; in the (not expected)
/// event a response exceeds that, we return an error rather than emit a frame
/// the peer would reject — the caller maps this to an `internal` error.
pub fn write_frame<W: Write>(writer: &mut W, value: &serde_json::Value) -> io::Result<()> {
    let bytes = serde_json::to_vec(value).map_err(io::Error::other)?;
    if bytes.len() > MAX_MESSAGE_LEN {
        return Err(io::Error::other("response exceeds 1 MiB framing limit"));
    }
    let len = bytes.len() as u32;
    writer.write_all(&len.to_ne_bytes())?;
    writer.write_all(&bytes)?;
    writer.flush()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn frame_bytes(json: &str) -> Vec<u8> {
        let body = json.as_bytes();
        let mut v = Vec::new();
        v.extend_from_slice(&(body.len() as u32).to_ne_bytes());
        v.extend_from_slice(body);
        v
    }

    #[test]
    fn parses_status() {
        let r = parse_frame(br#"{"type":"status","id":7}"#).unwrap();
        assert!(matches!(r, Request::Status { id: 7 }));
        assert_eq!(r.id(), Some(7));
    }

    #[test]
    fn parses_setup_with_defaults() {
        let r = parse_frame(
            br#"{"type":"setup","id":1,"secret":"correct horse battery","acknowledgeNoRecovery":true}"#,
        )
        .unwrap();
        match r {
            Request::Setup {
                id,
                max_attempts,
                acknowledge_no_recovery,
                ..
            } => {
                assert_eq!(id, 1);
                assert_eq!(max_attempts, 6); // default applied
                assert!(acknowledge_no_recovery);
            }
            _ => panic!("expected setup"),
        }
    }

    #[test]
    fn unknown_type_is_unknown_not_error() {
        let r = parse_frame(br#"{"type":"frobnicate","id":3}"#).unwrap();
        assert!(matches!(r, Request::Unknown { id: Some(3) }));
        // Unknown now recovers the id from the raw JSON so the error can echo it.
        assert_eq!(r.id(), Some(3));
    }

    #[test]
    fn unknown_type_without_id_recovers_none() {
        let r = parse_frame(br#"{"type":"frobnicate"}"#).unwrap();
        assert!(matches!(r, Request::Unknown { id: None }));
        assert_eq!(r.id(), None);
    }

    #[test]
    fn missing_type_is_invalid() {
        let e = parse_frame(br#"{"id":5}"#).unwrap_err();
        assert_eq!(e.code, ErrorCode::InvalidRequest);
    }

    #[test]
    fn missing_required_field_is_invalid() {
        // unlock without secret
        let e = parse_frame(br#"{"type":"unlock","id":2}"#).unwrap_err();
        assert_eq!(e.code, ErrorCode::InvalidRequest);
    }

    #[test]
    fn non_utf8_is_invalid_request() {
        let e = parse_frame(&[0xff, 0xfe, 0x00]).unwrap_err();
        assert_eq!(e.code, ErrorCode::InvalidRequest);
    }

    #[test]
    fn bad_json_is_invalid_request() {
        let e = parse_frame(b"{not json").unwrap_err();
        assert_eq!(e.code, ErrorCode::InvalidRequest);
    }

    #[test]
    fn oversized_payload_rejected_without_panic() {
        // We don't actually allocate a >1MiB slice of real data here; we just
        // assert the guard triggers on a slice reported as too large via a
        // crafted length in read_frame (see read_frame tests). For parse_frame,
        // feed exactly over the cap.
        let big = vec![b' '; MAX_MESSAGE_LEN + 1];
        let e = parse_frame(&big).unwrap_err();
        assert_eq!(e.code, ErrorCode::InvalidRequest);
    }

    #[test]
    fn read_frame_roundtrip() {
        let buf = frame_bytes(r#"{"type":"lock","id":9}"#);
        let mut cur = std::io::Cursor::new(buf);
        match read_frame(&mut cur).unwrap() {
            FrameRead::Message(p) => {
                let r = parse_frame(&p).unwrap();
                assert!(matches!(r, Request::Lock { id: 9 }));
            }
            _ => panic!("expected message"),
        }
    }

    #[test]
    fn read_frame_clean_eof() {
        let mut cur = std::io::Cursor::new(Vec::<u8>::new());
        assert!(matches!(read_frame(&mut cur).unwrap(), FrameRead::Eof));
    }

    #[test]
    fn read_frame_truncated_header() {
        let mut cur = std::io::Cursor::new(vec![0x01, 0x02]); // only 2 of 4 len bytes
        assert!(matches!(read_frame(&mut cur).unwrap(), FrameRead::Invalid(_)));
    }

    #[test]
    fn read_frame_truncated_body() {
        // Declare 10 bytes but provide 3.
        let mut v = Vec::new();
        v.extend_from_slice(&(10u32).to_ne_bytes());
        v.extend_from_slice(b"abc");
        let mut cur = std::io::Cursor::new(v);
        assert!(matches!(read_frame(&mut cur).unwrap(), FrameRead::Invalid(_)));
    }

    #[test]
    fn read_frame_oversized_length_not_allocated() {
        // Declared length is huge; read_frame must reject WITHOUT reading a body.
        let mut v = Vec::new();
        v.extend_from_slice(&((MAX_MESSAGE_LEN as u32) + 1).to_ne_bytes());
        // No body bytes follow on purpose.
        let mut cur = std::io::Cursor::new(v);
        match read_frame(&mut cur).unwrap() {
            FrameRead::Invalid(e) => assert_eq!(e.code, ErrorCode::InvalidRequest),
            _ => panic!("expected invalid"),
        }
    }

    #[test]
    fn write_frame_roundtrip() {
        let val = serde_json::json!({"id":1,"ok":true});
        let mut out = Vec::new();
        write_frame(&mut out, &val).unwrap();
        // First 4 bytes are native-endian length.
        let len = u32::from_ne_bytes(out[..4].try_into().unwrap()) as usize;
        assert_eq!(len, out.len() - 4);
        let parsed: serde_json::Value = serde_json::from_slice(&out[4..]).unwrap();
        assert_eq!(parsed["ok"], true);
    }
}
