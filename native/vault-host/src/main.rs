// SPDX-License-Identifier: MPL-2.0
//
// OpenBook vault host — stdio native-messaging entry point (Build Plan §5).
//
// Firefox launches this binary and speaks length-prefixed JSON over stdio:
//   [4-byte length, NATIVE byte order][UTF-8 JSON message]
//
// The loop:
//   * reads one frame at a time (1 MiB cap, no unbounded allocation),
//   * parses + dispatches to the engine,
//   * writes one framed JSON response per request,
//   * exits 0 cleanly on EOF.
//
// A malformed frame / bad JSON never crashes the host: it produces an
// `invalid-request` response (with id 0 when the id could not be recovered). A
// transport-level framing error (e.g. an over-large declared length we refuse to
// read) is reported once and then we stop reading, since the stream can no
// longer be resynchronized safely.
//
// Vault directory: by default `$OPENBOOK_VAULT_DIR` if set, else a per-user data
// dir (`$XDG_DATA_HOME/openbook/vault` or `~/.local/share/openbook/vault` on
// Unix; `%LOCALAPPDATA%\OpenBook\vault` on Windows). NEVER a real Firefox
// profile path by default — the vault is its own directory.

use std::io::{self, Read, Write};
use std::path::PathBuf;
use std::process::ExitCode;

use openbook_vault_host::engine::Engine;
use openbook_vault_host::error::VaultError;
use openbook_vault_host::protocol::{self, ErrorResponse, FrameRead};

fn main() -> ExitCode {
    let vault_dir = resolve_vault_dir();
    let mut engine = Engine::new(vault_dir);

    let stdin = io::stdin();
    let stdout = io::stdout();
    let mut reader = stdin.lock();
    let mut writer = stdout.lock();

    match run_loop(&mut reader, &mut writer, &mut engine) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            // I/O failure on the pipe (peer died, etc). Nothing useful to send;
            // exit non-zero. Do not panic (would skip orderly drop/zeroize).
            let _ = writeln!(io::stderr(), "openbook-vault-host: io error: {e}");
            ExitCode::FAILURE
        }
    }
}

/// The framed read/dispatch/write loop, factored out so it can be driven in
/// tests with in-memory readers/writers.
pub fn run_loop<R: Read, W: Write>(
    reader: &mut R,
    writer: &mut W,
    engine: &mut Engine,
) -> io::Result<()> {
    loop {
        match protocol::read_frame(reader)? {
            FrameRead::Eof => return Ok(()), // clean shutdown
            FrameRead::Message(payload) => {
                let response = match protocol::parse_frame(&payload) {
                    Ok(req) => engine.handle(req),
                    Err(e) => err_value(None, &e),
                };
                protocol::write_frame(writer, &response)?;
            }
            FrameRead::Invalid(e) => {
                // We could not trust the frame boundary. Emit one invalid-request
                // (id 0, since we never read a body) and stop: the stream is
                // desynchronized and continuing would misinterpret bytes.
                let response = err_value(None, &e);
                let _ = protocol::write_frame(writer, &response);
                return Ok(());
            }
        }
    }
}

fn err_value(id: Option<i64>, e: &VaultError) -> serde_json::Value {
    serde_json::to_value(ErrorResponse::from_error(id, e)).unwrap_or_else(|_| {
        serde_json::json!({"id": id.unwrap_or(0), "ok": false, "error": "internal", "message": "serialization failed"})
    })
}

/// Resolve the vault directory. Order:
///   1. `$OPENBOOK_VAULT_DIR` (explicit override; used by the installer/UI).
///   2. Platform per-user data dir + `openbook/vault`.
///
/// This is intentionally NOT a Firefox profile path: the vault is a separate
/// directory the host owns. (In the full product the container mechanism links
/// the decrypted profile to Gecko; that integration is out of scope for the v1
/// file-based container.)
fn resolve_vault_dir() -> PathBuf {
    if let Some(dir) = std::env::var_os("OPENBOOK_VAULT_DIR") {
        return PathBuf::from(dir);
    }
    #[cfg(unix)]
    {
        if let Some(xdg) = std::env::var_os("XDG_DATA_HOME") {
            return PathBuf::from(xdg).join("openbook").join("vault");
        }
        if let Some(home) = std::env::var_os("HOME") {
            return PathBuf::from(home)
                .join(".local")
                .join("share")
                .join("openbook")
                .join("vault");
        }
    }
    #[cfg(windows)]
    {
        if let Some(local) = std::env::var_os("LOCALAPPDATA") {
            return PathBuf::from(local).join("OpenBook").join("vault");
        }
    }
    // Last resort: a vault dir under the current directory. Never a profile.
    PathBuf::from("openbook-vault")
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Frame a JSON string for feeding to the loop.
    fn frame(json: &str) -> Vec<u8> {
        let body = json.as_bytes();
        let mut v = Vec::new();
        v.extend_from_slice(&(body.len() as u32).to_ne_bytes());
        v.extend_from_slice(body);
        v
    }

    /// Decode all framed responses from a buffer into JSON values.
    fn decode_all(mut buf: &[u8]) -> Vec<serde_json::Value> {
        let mut out = Vec::new();
        while buf.len() >= 4 {
            let len = u32::from_ne_bytes(buf[..4].try_into().unwrap()) as usize;
            let body = &buf[4..4 + len];
            out.push(serde_json::from_slice(body).unwrap());
            buf = &buf[4 + len..];
        }
        out
    }

    #[test]
    fn loop_handles_status_then_eof() {
        let tmp = tempfile::tempdir().unwrap();
        let mut engine = Engine::new(tmp.path().to_path_buf());

        let mut input = Vec::new();
        input.extend_from_slice(&frame(r#"{"type":"status","id":1}"#));
        // EOF naturally after the single frame.

        let mut reader = std::io::Cursor::new(input);
        let mut writer: Vec<u8> = Vec::new();
        run_loop(&mut reader, &mut writer, &mut engine).unwrap();

        let responses = decode_all(&writer);
        assert_eq!(responses.len(), 1);
        assert_eq!(responses[0]["state"], "uninitialized");
        assert_eq!(responses[0]["id"], 1);
    }

    #[test]
    fn loop_emits_invalid_request_on_bad_json_then_continues() {
        let tmp = tempfile::tempdir().unwrap();
        let mut engine = Engine::new(tmp.path().to_path_buf());

        let mut input = Vec::new();
        input.extend_from_slice(&frame("{not valid json"));
        input.extend_from_slice(&frame(r#"{"type":"status","id":2}"#));

        let mut reader = std::io::Cursor::new(input);
        let mut writer: Vec<u8> = Vec::new();
        run_loop(&mut reader, &mut writer, &mut engine).unwrap();

        let responses = decode_all(&writer);
        assert_eq!(responses.len(), 2);
        assert_eq!(responses[0]["error"], "invalid-request");
        assert_eq!(responses[1]["state"], "uninitialized");
    }

    #[test]
    fn resolve_vault_dir_respects_override() {
        // Safety: set then unset around the call; tests run single-threaded per
        // process by default here, and this var is not used elsewhere
        // concurrently.
        std::env::set_var("OPENBOOK_VAULT_DIR", "/tmp/openbook-vault-test-xyz");
        let d = resolve_vault_dir();
        assert_eq!(d, PathBuf::from("/tmp/openbook-vault-test-xyz"));
        std::env::remove_var("OPENBOOK_VAULT_DIR");
    }
}
