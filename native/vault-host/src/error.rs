// SPDX-License-Identifier: MPL-2.0
//
// Error types for the OpenBook vault host. Every error maps to a stable wire
// error code (see `code()`), which is what the `vault-ui` extension matches on.
// Human-readable `message` strings are advisory and may change; codes are API.

use std::fmt;

/// Stable wire error codes. The string form is the `error` field on the JSON
/// error response. These MUST stay in sync with the protocol contract consumed
/// by `extensions/vault-ui`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErrorCode {
    /// Malformed frame, bad JSON, missing required fields, unknown type, or a
    /// request field that is present but invalid (e.g. `confirm` != true).
    InvalidRequest,
    /// An operation that needs an initialized vault was issued while
    /// uninitialized (e.g. unlock before setup).
    NotInitialized,
    /// `setup` issued against an already-initialized vault.
    AlreadyInitialized,
    /// Unlock secret did not decrypt the master key.
    BadSecret,
    /// The vault has been cryptographically erased; the data is gone.
    Erased,
    /// Software-fallback `setup` rejected because the secret is too weak.
    WeakSecret,
    /// `setup` issued without `acknowledgeNoRecovery == true`.
    NoRecoveryNotAcknowledged,
    /// A hardware provider was requested/required but is unavailable.
    HardwareUnavailable,
    /// Unexpected internal failure (I/O, serialization, provider fault). The
    /// host stays alive and reports this rather than crashing.
    Internal,
}

impl ErrorCode {
    /// The exact on-the-wire string for this code.
    pub fn as_str(self) -> &'static str {
        match self {
            ErrorCode::InvalidRequest => "invalid-request",
            ErrorCode::NotInitialized => "not-initialized",
            ErrorCode::AlreadyInitialized => "already-initialized",
            ErrorCode::BadSecret => "bad-secret",
            ErrorCode::Erased => "erased",
            ErrorCode::WeakSecret => "weak-secret",
            ErrorCode::NoRecoveryNotAcknowledged => "no-recovery-not-acknowledged",
            ErrorCode::HardwareUnavailable => "hardware-unavailable",
            ErrorCode::Internal => "internal",
        }
    }
}

impl fmt::Display for ErrorCode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// A vault error: a stable code plus a human-readable message.
#[derive(Debug, Clone, thiserror::Error)]
#[error("{code}: {message}")]
pub struct VaultError {
    pub code: ErrorCode,
    pub message: String,
}

impl VaultError {
    pub fn new(code: ErrorCode, message: impl Into<String>) -> Self {
        VaultError {
            code,
            message: message.into(),
        }
    }

    pub fn invalid_request(message: impl Into<String>) -> Self {
        Self::new(ErrorCode::InvalidRequest, message)
    }

    pub fn not_initialized() -> Self {
        Self::new(
            ErrorCode::NotInitialized,
            "vault is not initialized; run setup first",
        )
    }

    pub fn already_initialized() -> Self {
        Self::new(
            ErrorCode::AlreadyInitialized,
            "vault is already initialized",
        )
    }

    pub fn erased() -> Self {
        Self::new(
            ErrorCode::Erased,
            "vault has been cryptographically erased; data is unrecoverable",
        )
    }

    pub fn weak_secret(message: impl Into<String>) -> Self {
        Self::new(ErrorCode::WeakSecret, message)
    }

    pub fn no_recovery_not_acknowledged() -> Self {
        Self::new(
            ErrorCode::NoRecoveryNotAcknowledged,
            "setup requires acknowledgeNoRecovery == true; erasure is irreversible",
        )
    }

    pub fn hardware_unavailable(message: impl Into<String>) -> Self {
        Self::new(ErrorCode::HardwareUnavailable, message)
    }

    pub fn internal(message: impl Into<String>) -> Self {
        Self::new(ErrorCode::Internal, message)
    }
}

/// Internal result alias.
pub type Result<T> = std::result::Result<T, VaultError>;

// Convert the common foreign error types into an `internal` VaultError. We
// deliberately avoid leaking secret material; messages describe the operation,
// not the data.
impl From<std::io::Error> for VaultError {
    fn from(e: std::io::Error) -> Self {
        VaultError::internal(format!("io error: {e}"))
    }
}

impl From<serde_json::Error> for VaultError {
    fn from(e: serde_json::Error) -> Self {
        // Serde failures during *response* serialization are internal; failures
        // while parsing an inbound frame are mapped to invalid-request at the
        // call site before reaching here.
        VaultError::internal(format!("serialization error: {e}"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn codes_have_stable_strings() {
        // Lock the wire contract. If any of these change, vault-ui breaks.
        assert_eq!(ErrorCode::InvalidRequest.as_str(), "invalid-request");
        assert_eq!(ErrorCode::NotInitialized.as_str(), "not-initialized");
        assert_eq!(ErrorCode::AlreadyInitialized.as_str(), "already-initialized");
        assert_eq!(ErrorCode::BadSecret.as_str(), "bad-secret");
        assert_eq!(ErrorCode::Erased.as_str(), "erased");
        assert_eq!(ErrorCode::WeakSecret.as_str(), "weak-secret");
        assert_eq!(
            ErrorCode::NoRecoveryNotAcknowledged.as_str(),
            "no-recovery-not-acknowledged"
        );
        assert_eq!(ErrorCode::HardwareUnavailable.as_str(), "hardware-unavailable");
        assert_eq!(ErrorCode::Internal.as_str(), "internal");
    }

    #[test]
    fn display_uses_code_string() {
        let e = VaultError::bad_secret_placeholder();
        assert!(e.to_string().starts_with("bad-secret:"));
    }

    impl VaultError {
        // Test helper only.
        fn bad_secret_placeholder() -> Self {
            VaultError::new(ErrorCode::BadSecret, "wrong secret")
        }
    }
}
