// SPDX-License-Identifier: MPL-2.0
//
// Native-messaging wire protocol for the OpenBook VPN exit-IP verification host
// (`org.openbook.vpn_helper`).
//
// DEFERRED scaffold (Build Plan §6). OpenBook's supported real-tunnel model is
// "OS-level VPN, browser verifies the exit IP" (§6 Option 1). This host does NOT
// create or manage tunnels and does NOT ship a userspace WireGuard stack (§6
// rejects that for v1). It speaks only the native-messaging protocol and exposes
// a single exit-IP VERIFICATION request.
//
// Transport (Firefox native messaging framing), identical to the vault host:
//   [4-byte message length in NATIVE byte order][that many bytes of UTF-8 JSON]
//
// A hard 1 MiB cap is enforced on a single message; a larger declared length is
// rejected as `invalid-request` WITHOUT allocating the claimed size (so a hostile
// length field cannot drive a multi-gigabyte allocation). On EOF the loop exits
// cleanly (handled in `main.rs`).
//
// PERMISSIONS / NETWORK INVARIANT: no outbound traffic happens at rest. The only
// path that could ever touch the network is an explicit, user-driven `verify`
// request, and even that is STUBBED in this scaffold — it performs an offline
// comparison or returns a structured `not-implemented-in-scaffold` result and
// never opens a socket. See `handle_verify`.
//
// `parse_frame` operates on an already-collected byte slice so the parser is
// directly unit- and fuzz-testable in isolation from any I/O.

use std::io::{self, Read, Write};

use serde::{Deserialize, Serialize};

/// Maximum accepted single-message size (header-declared length), in bytes.
/// Mirrors Firefox's 1 MiB native-messaging cap; anything larger is refused up
/// front without allocation.
pub const MAX_MESSAGE_LEN: usize = 1024 * 1024;

/// Stable error codes echoed to the extension. Kept as `&'static str` so the wire
/// contract is fixed and greppable.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErrorCode {
    /// Malformed framing, non-UTF-8, bad JSON, unknown/missing fields.
    InvalidRequest,
    /// An internal failure (e.g. response serialization). Not expected in normal
    /// operation; present so the dispatcher never has to panic.
    Internal,
}

impl ErrorCode {
    pub fn as_str(self) -> &'static str {
        match self {
            ErrorCode::InvalidRequest => "invalid-request",
            ErrorCode::Internal => "internal",
        }
    }
}

/// A protocol-level error carrying a stable code plus a human-readable message.
/// Never surfaces secrets; this host handles none.
#[derive(Debug, Clone)]
pub struct ProtocolError {
    pub code: ErrorCode,
    pub message: String,
}

impl ProtocolError {
    pub fn invalid_request(message: impl Into<String>) -> Self {
        ProtocolError {
            code: ErrorCode::InvalidRequest,
            message: message.into(),
        }
    }

    pub fn internal(message: impl Into<String>) -> Self {
        ProtocolError {
            code: ErrorCode::Internal,
            message: message.into(),
        }
    }
}

/// A successfully framed-and-parsed inbound request.
///
/// Modeled as a tagged enum keyed on the JSON `type` field. `id` is required on
/// the known request types and is echoed back on every response. Unknown `type`
/// values deserialize to `Unknown` (rather than failing) so the dispatcher can
/// answer them with a clean `invalid-request` that still echoes `id` where it can.
#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "type")]
pub enum Request {
    /// Liveness/capability probe. Returns the host's static descriptor: that it
    /// is a deferred verification-only scaffold that creates no tunnels.
    #[serde(rename = "status")]
    Status { id: i64 },

    /// Exit-IP verification. In the supported model the OS holds the tunnel and
    /// the browser merely checks that the observed exit IP matches expectation.
    ///
    /// `expected_exit_ip` is OPTIONAL:
    /// - If provided alongside `observed_exit_ip`, this scaffold performs a purely
    ///   offline string comparison and reports match/mismatch — no network.
    /// - If `expected_exit_ip` is provided WITHOUT an `observed_exit_ip`, the
    ///   scaffold returns a structured `not-implemented-in-scaffold` outcome
    ///   because fetching the live exit IP (the only networked step) is stubbed.
    /// - If neither is provided, the request is treated as a capability query and
    ///   also returns `not-implemented-in-scaffold`.
    ///
    /// No socket is ever opened by this scaffold regardless of the arguments.
    #[serde(rename = "verify")]
    Verify {
        id: i64,
        #[serde(default, rename = "expectedExitIp")]
        expected_exit_ip: Option<String>,
        /// Caller-supplied observed exit IP (e.g. obtained out-of-band). When
        /// present, enables a fully offline comparison in this scaffold.
        #[serde(default, rename = "observedExitIp")]
        observed_exit_ip: Option<String>,
    },

    /// Catch-all for any unrecognized `type`. serde's `other` cannot bind fields,
    /// so `id()` falls back to `None` (the dispatcher then uses id 0).
    #[serde(other)]
    Unknown,
}

impl Request {
    /// Best-effort extraction of the request id for responses, including the
    /// `Unknown` arm (where serde's `other` cannot also bind `id`). Returns
    /// `None` only when no usable numeric id is present.
    pub fn id(&self) -> Option<i64> {
        match self {
            Request::Status { id } | Request::Verify { id, .. } => Some(*id),
            Request::Unknown => None,
        }
    }
}

/// Parse a single complete message payload (the JSON bytes, *without* the length
/// header) into a `Request`.
///
/// Errors with `invalid-request` if the bytes are not valid UTF-8, not valid
/// JSON, or do not match a known request object shape. This function never panics
/// on arbitrary input and never allocates more than the input itself — both
/// properties are exercised by the tests below and the fuzz-style robustness test.
pub fn parse_frame(payload: &[u8]) -> Result<Request, ProtocolError> {
    if payload.len() > MAX_MESSAGE_LEN {
        return Err(ProtocolError::invalid_request("message exceeds 1 MiB limit"));
    }
    let text = std::str::from_utf8(payload)
        .map_err(|_| ProtocolError::invalid_request("message is not valid UTF-8"))?;
    serde_json::from_str::<Request>(text)
        .map_err(|e| ProtocolError::invalid_request(format!("malformed request JSON: {e}")))
}

/// Outcome of attempting to read one frame from a stream.
pub enum FrameRead {
    /// A complete payload was read (length header consumed, JSON bytes returned).
    Message(Vec<u8>),
    /// Clean EOF before any byte of a new frame — caller should exit 0.
    Eof,
    /// The frame was structurally invalid at the transport level (declared length
    /// over the cap, or the stream ended mid-frame). The body could not be
    /// trusted/collected; caller emits an `invalid-request` (id 0) and stops.
    Invalid(ProtocolError),
}

/// Read exactly one length-prefixed frame from `reader`.
///
/// - EOF with zero bytes consumed -> `FrameRead::Eof`.
/// - EOF after a partial read -> `FrameRead::Invalid` (truncated frame).
/// - Declared length > `MAX_MESSAGE_LEN` -> `FrameRead::Invalid` and the body is
///   NOT read/allocated.
pub fn read_frame<R: Read>(reader: &mut R) -> io::Result<FrameRead> {
    let mut len_buf = [0u8; 4];
    match read_exact_or_eof(reader, &mut len_buf)? {
        ReadExact::Eof => return Ok(FrameRead::Eof),
        ReadExact::Partial => {
            return Ok(FrameRead::Invalid(ProtocolError::invalid_request(
                "truncated length header",
            )))
        }
        ReadExact::Full => {}
    }

    // NATIVE byte order per the native-messaging spec.
    let declared = u32::from_ne_bytes(len_buf) as usize;
    if declared > MAX_MESSAGE_LEN {
        return Ok(FrameRead::Invalid(ProtocolError::invalid_request(format!(
            "declared message length {declared} exceeds 1 MiB limit"
        ))));
    }

    let mut payload = vec![0u8; declared];
    match read_exact_or_eof(reader, &mut payload)? {
        ReadExact::Full => Ok(FrameRead::Message(payload)),
        ReadExact::Eof | ReadExact::Partial => Ok(FrameRead::Invalid(
            ProtocolError::invalid_request("truncated message body"),
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
/// Capped at `MAX_MESSAGE_LEN`; an over-large response returns an error rather
/// than emitting a frame the peer would reject.
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

/// Result of an exit-IP verification, used only to shape the JSON response.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum VerifyOutcome {
    /// Offline comparison succeeded: observed == expected.
    Match,
    /// Offline comparison failed: observed != expected. This is the FAIL-CLOSED
    /// signal the extension must treat as "exit IP is wrong, do not trust tunnel".
    Mismatch,
    /// The networked live-IP probe is intentionally not built in this scaffold.
    NotImplementedInScaffold,
}

impl VerifyOutcome {
    pub fn as_str(self) -> &'static str {
        match self {
            VerifyOutcome::Match => "match",
            VerifyOutcome::Mismatch => "mismatch",
            VerifyOutcome::NotImplementedInScaffold => "not-implemented-in-scaffold",
        }
    }
}

/// Build the `status` response: a static, side-effect-free capability descriptor.
pub fn handle_status(id: i64) -> serde_json::Value {
    serde_json::json!({
        "id": id,
        "ok": true,
        "type": "status",
        "host": "org.openbook.vpn_helper",
        "version": env!("CARGO_PKG_VERSION"),
        // Honest capability advertisement so the extension never assumes tunnels.
        "role": "exit-ip-verification",
        "deferred": true,
        "createsTunnels": false,
        "shipsWireguard": false,
        "performsNetworkAtRest": false,
        "model": "os-level-vpn-browser-verifies",
        "message": "Deferred scaffold: verifies exit IP only; never creates or manages tunnels."
    })
}

/// Handle a `verify` request WITHOUT touching the network.
///
/// Decision table (see `Request::Verify` docs):
/// - expected + observed present -> offline string compare -> match/mismatch.
/// - expected present, observed absent -> not-implemented-in-scaffold (the live
///   probe is the stubbed networked step).
/// - neither present -> not-implemented-in-scaffold (capability query).
///
/// IMPORTANT: a fail-closed consumer must treat anything other than an explicit
/// `match` (including `mismatch` and `not-implemented-in-scaffold`) as "exit IP
/// not verified" and refuse to assume the tunnel is up.
pub fn handle_verify(
    id: i64,
    expected_exit_ip: Option<&str>,
    observed_exit_ip: Option<&str>,
) -> serde_json::Value {
    match (expected_exit_ip, observed_exit_ip) {
        (Some(expected), Some(observed)) => {
            let outcome = if expected == observed {
                VerifyOutcome::Match
            } else {
                VerifyOutcome::Mismatch
            };
            serde_json::json!({
                "id": id,
                "ok": true,
                "type": "verify",
                "outcome": outcome.as_str(),
                "matches": outcome == VerifyOutcome::Match,
                "expectedExitIp": expected,
                "observedExitIp": observed,
                "performedNetworkProbe": false,
                "message": "Offline comparison only; the live exit-IP probe is not built in this scaffold."
            })
        }
        _ => serde_json::json!({
            "id": id,
            "ok": true,
            "type": "verify",
            "outcome": VerifyOutcome::NotImplementedInScaffold.as_str(),
            "matches": false,
            "performedNetworkProbe": false,
            "message": "Live exit-IP probe is deferred (Build Plan §6). Supply observedExitIp to run an offline comparison, or implement the probe on a real build host."
        }),
    }
}

/// Build an error response, echoing `id` (0 when unknown — documented sentinel).
pub fn error_response(id: Option<i64>, code: ErrorCode, message: impl Into<String>) -> serde_json::Value {
    serde_json::json!({
        "id": id.unwrap_or(0),
        "ok": false,
        "error": code.as_str(),
        "message": message.into(),
    })
}

/// Dispatch a parsed request to a response value. Pure and side-effect-free in
/// this scaffold (no network, no filesystem), which is what keeps the "no traffic
/// at rest" invariant trivially true and testable.
pub fn dispatch(request: &Request) -> serde_json::Value {
    match request {
        Request::Status { id } => handle_status(*id),
        Request::Verify {
            id,
            expected_exit_ip,
            observed_exit_ip,
        } => handle_verify(*id, expected_exit_ip.as_deref(), observed_exit_ip.as_deref()),
        Request::Unknown => error_response(
            request.id(),
            ErrorCode::InvalidRequest,
            "unknown request type",
        ),
    }
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
    fn parses_verify_with_both_ips() {
        let r = parse_frame(
            br#"{"type":"verify","id":2,"expectedExitIp":"203.0.113.7","observedExitIp":"203.0.113.7"}"#,
        )
        .unwrap();
        match r {
            Request::Verify {
                id,
                expected_exit_ip,
                observed_exit_ip,
            } => {
                assert_eq!(id, 2);
                assert_eq!(expected_exit_ip.as_deref(), Some("203.0.113.7"));
                assert_eq!(observed_exit_ip.as_deref(), Some("203.0.113.7"));
            }
            _ => panic!("expected verify"),
        }
    }

    #[test]
    fn parses_verify_with_no_ips() {
        let r = parse_frame(br#"{"type":"verify","id":3}"#).unwrap();
        match r {
            Request::Verify {
                id,
                expected_exit_ip,
                observed_exit_ip,
            } => {
                assert_eq!(id, 3);
                assert!(expected_exit_ip.is_none());
                assert!(observed_exit_ip.is_none());
            }
            _ => panic!("expected verify"),
        }
    }

    #[test]
    fn unknown_type_is_unknown_not_error() {
        let r = parse_frame(br#"{"type":"frobnicate","id":3}"#).unwrap();
        assert!(matches!(r, Request::Unknown));
        assert_eq!(r.id(), None);
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
        let big = vec![b' '; MAX_MESSAGE_LEN + 1];
        let e = parse_frame(&big).unwrap_err();
        assert_eq!(e.code, ErrorCode::InvalidRequest);
    }

    #[test]
    fn status_dispatch_is_offline_descriptor() {
        let v = dispatch(&Request::Status { id: 11 });
        assert_eq!(v["ok"], true);
        assert_eq!(v["createsTunnels"], false);
        assert_eq!(v["performsNetworkAtRest"], false);
        assert_eq!(v["deferred"], true);
        assert_eq!(v["id"], 11);
    }

    #[test]
    fn verify_match_offline() {
        let v = handle_verify(1, Some("198.51.100.4"), Some("198.51.100.4"));
        assert_eq!(v["outcome"], "match");
        assert_eq!(v["matches"], true);
        assert_eq!(v["performedNetworkProbe"], false);
    }

    #[test]
    fn verify_mismatch_offline_fails_closed() {
        let v = handle_verify(1, Some("198.51.100.4"), Some("10.0.0.1"));
        assert_eq!(v["outcome"], "mismatch");
        // Fail-closed: a mismatch must never read as a verified tunnel.
        assert_eq!(v["matches"], false);
        assert_eq!(v["performedNetworkProbe"], false);
    }

    #[test]
    fn verify_without_observed_is_not_implemented() {
        let v = handle_verify(5, Some("198.51.100.4"), None);
        assert_eq!(v["outcome"], "not-implemented-in-scaffold");
        assert_eq!(v["matches"], false);
        assert_eq!(v["performedNetworkProbe"], false);
    }

    #[test]
    fn verify_capability_query_is_not_implemented() {
        let v = handle_verify(6, None, None);
        assert_eq!(v["outcome"], "not-implemented-in-scaffold");
        assert_eq!(v["matches"], false);
    }

    #[test]
    fn unknown_dispatch_is_invalid_request() {
        let v = dispatch(&Request::Unknown);
        assert_eq!(v["ok"], false);
        assert_eq!(v["error"], "invalid-request");
        assert_eq!(v["id"], 0);
    }

    #[test]
    fn read_frame_roundtrip() {
        let buf = frame_bytes(r#"{"type":"status","id":9}"#);
        let mut cur = std::io::Cursor::new(buf);
        match read_frame(&mut cur).unwrap() {
            FrameRead::Message(p) => {
                let r = parse_frame(&p).unwrap();
                assert!(matches!(r, Request::Status { id: 9 }));
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
        let mut cur = std::io::Cursor::new(vec![0x01, 0x02]);
        assert!(matches!(read_frame(&mut cur).unwrap(), FrameRead::Invalid(_)));
    }

    #[test]
    fn read_frame_truncated_body() {
        let mut v = Vec::new();
        v.extend_from_slice(&(10u32).to_ne_bytes());
        v.extend_from_slice(b"abc");
        let mut cur = std::io::Cursor::new(v);
        assert!(matches!(read_frame(&mut cur).unwrap(), FrameRead::Invalid(_)));
    }

    #[test]
    fn read_frame_oversized_length_not_allocated() {
        let mut v = Vec::new();
        v.extend_from_slice(&((MAX_MESSAGE_LEN as u32) + 1).to_ne_bytes());
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
        let len = u32::from_ne_bytes(out[..4].try_into().unwrap()) as usize;
        assert_eq!(len, out.len() - 4);
        let parsed: serde_json::Value = serde_json::from_slice(&out[4..]).unwrap();
        assert_eq!(parsed["ok"], true);
    }

    #[test]
    fn fuzz_like_garbage_never_panics() {
        // A spread of hostile inputs: parse_frame must always return, never panic.
        let cases: &[&[u8]] = &[
            b"",
            b"{",
            b"[]",
            b"null",
            b"123",
            b"\"string\"",
            &[0x00, 0x01, 0x02, 0xff],
            br#"{"type":"verify"}"#,            // missing id
            br#"{"type":"verify","id":"x"}"#,   // wrong id type
            br#"{"type":42}"#,                  // wrong type field type
            br#"{"id":1}"#,                     // no type
        ];
        for c in cases {
            // Either Ok(parsed) or Err(invalid-request); both are fine, no panic.
            let _ = parse_frame(c);
        }
    }
}
