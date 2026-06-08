// SPDX-License-Identifier: MPL-2.0
//
// OpenBook Browser — VPN exit-IP verification native messaging host
// (`org.openbook.vpn_helper`). DEFERRED scaffold (Build Plan §6).
//
// This binary is a thin stdio loop over the framing/dispatch logic in
// `protocol.rs` (also compiled as the crate library `openbook_vpn_helper`). It:
//   - reads length-prefixed UTF-8 JSON frames from stdin,
//   - dispatches `status` / `verify` (and rejects anything else as
//     `invalid-request`),
//   - writes length-prefixed JSON responses to stdout.
//
// It NEVER panics on malformed input, NEVER creates a tunnel, and performs NO
// network I/O at rest (the only would-be networked step — the live exit-IP probe
// — is stubbed in `protocol.rs::handle_verify`). See the crate README for the
// supported model and the permissions invariant.

use std::io::{self, Read, Write};

use openbook_vpn_helper::{
    dispatch, error_response, parse_frame, read_frame, write_frame, ErrorCode, FrameRead,
};

fn main() {
    // Lock stdio once; native messaging is a single serial stream per launch.
    let stdin = io::stdin();
    let stdout = io::stdout();
    let mut reader = stdin.lock();
    let mut writer = stdout.lock();

    match run(&mut reader, &mut writer) {
        Ok(()) => {}
        Err(e) => {
            // A transport-level write failure (peer closed the pipe, etc.) is not
            // an application error worth a nonzero status in normal teardown, but
            // we surface unexpected I/O errors on stderr for debugging.
            eprintln!("org.openbook.vpn_helper: I/O error: {e}");
        }
    }
}

/// The serial request/response loop, generic over reader/writer so it is
/// integration-testable with in-memory streams.
fn run<R: Read, W: Write>(reader: &mut R, writer: &mut W) -> io::Result<()> {
    loop {
        match read_frame(reader)? {
            FrameRead::Eof => return Ok(()),
            FrameRead::Invalid(err) => {
                // We could not trust/collect the body; reply with a best-effort
                // invalid-request (id 0) and stop reading this stream, since the
                // framing is desynchronized and continuing is unsafe.
                let resp = error_response(None, err.code, err.message);
                write_frame(writer, &resp)?;
                return Ok(());
            }
            FrameRead::Message(payload) => {
                let resp = match parse_frame(&payload) {
                    Ok(request) => dispatch(&request),
                    Err(err) => error_response(None, ErrorCode::InvalidRequest, err.message),
                };
                write_frame(writer, &resp)?;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Build a single length-prefixed frame (native-endian length + JSON body).
    fn frame(json: &str) -> Vec<u8> {
        let body = json.as_bytes();
        let mut v = Vec::new();
        v.extend_from_slice(&(body.len() as u32).to_ne_bytes());
        v.extend_from_slice(body);
        v
    }

    /// Decode the FIRST length-prefixed frame from `out` into a JSON value.
    fn first_response(out: &[u8]) -> serde_json::Value {
        let len = u32::from_ne_bytes(out[..4].try_into().unwrap()) as usize;
        serde_json::from_slice(&out[4..4 + len]).unwrap()
    }

    #[test]
    fn status_request_yields_descriptor() {
        let input = frame(r#"{"type":"status","id":1}"#);
        let mut reader = std::io::Cursor::new(input);
        let mut out = Vec::new();
        run(&mut reader, &mut out).unwrap();
        let v = first_response(&out);
        assert_eq!(v["ok"], true);
        assert_eq!(v["id"], 1);
        assert_eq!(v["createsTunnels"], false);
    }

    #[test]
    fn verify_offline_match() {
        let input = frame(
            r#"{"type":"verify","id":2,"expectedExitIp":"203.0.113.9","observedExitIp":"203.0.113.9"}"#,
        );
        let mut reader = std::io::Cursor::new(input);
        let mut out = Vec::new();
        run(&mut reader, &mut out).unwrap();
        let v = first_response(&out);
        assert_eq!(v["outcome"], "match");
        assert_eq!(v["matches"], true);
        assert_eq!(v["performedNetworkProbe"], false);
    }

    #[test]
    fn unknown_type_yields_invalid_request() {
        let input = frame(r#"{"type":"nope","id":3}"#);
        let mut reader = std::io::Cursor::new(input);
        let mut out = Vec::new();
        run(&mut reader, &mut out).unwrap();
        let v = first_response(&out);
        assert_eq!(v["ok"], false);
        assert_eq!(v["error"], "invalid-request");
    }

    #[test]
    fn garbage_frame_is_invalid_request_then_stops() {
        // Bad JSON body in a well-formed frame: one invalid-request response, then
        // clean termination.
        let input = frame("{not json");
        let mut reader = std::io::Cursor::new(input);
        let mut out = Vec::new();
        run(&mut reader, &mut out).unwrap();
        let v = first_response(&out);
        assert_eq!(v["ok"], false);
        assert_eq!(v["error"], "invalid-request");
    }

    #[test]
    fn clean_eof_writes_nothing() {
        let mut reader = std::io::Cursor::new(Vec::<u8>::new());
        let mut out = Vec::new();
        run(&mut reader, &mut out).unwrap();
        assert!(out.is_empty());
    }

    #[test]
    fn two_sequential_requests_both_answered() {
        let mut input = frame(r#"{"type":"status","id":1}"#);
        input.extend(frame(r#"{"type":"status","id":2}"#));
        let mut reader = std::io::Cursor::new(input);
        let mut out = Vec::new();
        run(&mut reader, &mut out).unwrap();
        // First response id == 1.
        let len0 = u32::from_ne_bytes(out[..4].try_into().unwrap()) as usize;
        let v0: serde_json::Value = serde_json::from_slice(&out[4..4 + len0]).unwrap();
        assert_eq!(v0["id"], 1);
        // Second response id == 2.
        let rest = &out[4 + len0..];
        let len1 = u32::from_ne_bytes(rest[..4].try_into().unwrap()) as usize;
        let v1: serde_json::Value = serde_json::from_slice(&rest[4..4 + len1]).unwrap();
        assert_eq!(v1["id"], 2);
    }
}
