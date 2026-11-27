//! Canonical typed error taxonomy for `FileFusion` (`docs/CONTRACTS.md` §2.1).

use core::fmt;
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::CliExitCode;

/// High-level error classification across all `FileFusion` boundaries.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ErrorCategory {
    /// Filesystem or OS I/O error.
    Io,
    /// Unsupported, corrupt, or DRM-locked input payload.
    Input,
    /// Security violation (traversal, bomb, or spoofing).
    Security,
    /// Required Tier 2 external tool is missing or unavailable.
    MissingEngine,
    /// Memory, concurrency, or timeout limit reached.
    ResourceLimit,
    /// Operation cancelled cooperatively by caller.
    Cancelled,
    /// Unexpected internal invariant failure.
    Internal,
}

/// Stable machine-readable error codes (`FF-ERR-*`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ErrorCode {
    /// `FF-ERR-IO-NOT-FOUND`: Source path missing or inaccessible.
    #[serde(rename = "FF-ERR-IO-NOT-FOUND")]
    IoNotFound,
    /// `FF-ERR-FMT-UNSUPPORTED`: Format or conversion route unsupported.
    #[serde(rename = "FF-ERR-FMT-UNSUPPORTED")]
    FmtUnsupported,
    /// `FF-ERR-FMT-CORRUPT`: Container/header truncated or malformed.
    #[serde(rename = "FF-ERR-FMT-CORRUPT")]
    FmtCorrupt,
    /// `FF-ERR-FMT-DRM`: DRM-protected content rejected (no circumvention).
    #[serde(rename = "FF-ERR-FMT-DRM")]
    FmtDrm,
    /// `FF-ERR-SEC-TRAVERSAL`: Archive traversal or symlink escape rejected.
    #[serde(rename = "FF-ERR-SEC-TRAVERSAL")]
    SecTraversal,
    /// `FF-ERR-SEC-BOMB`: Expansion ratio, depth, or size ceiling exceeded.
    #[serde(rename = "FF-ERR-SEC-BOMB")]
    SecBomb,
    /// `FF-ERR-SEC-SPOOFED`: File extension contradicts binary signature.
    #[serde(rename = "FF-ERR-SEC-SPOOFED")]
    SecSpoofed,
    /// `FF-ERR-ENG-MISSING`: Required Tier 2 external engine missing.
    #[serde(rename = "FF-ERR-ENG-MISSING")]
    EngMissing,
    /// `FF-ERR-RES-MEMORY`: Host memory or worker budget exceeded.
    #[serde(rename = "FF-ERR-RES-MEMORY")]
    ResMemory,
    /// `FF-ERR-JOB-CANCELLED`: Active job aborted via `CancellationToken`.
    #[serde(rename = "FF-ERR-JOB-CANCELLED")]
    JobCancelled,
    /// `FF-ERR-INT-INVARIANT`: Unexpected internal invariant failure.
    #[serde(rename = "FF-ERR-INT-INVARIANT")]
    IntInvariant,
}

impl ErrorCode {
    /// Returns the canonical `FF-ERR-*` string representation.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::IoNotFound => "FF-ERR-IO-NOT-FOUND",
            Self::FmtUnsupported => "FF-ERR-FMT-UNSUPPORTED",
            Self::FmtCorrupt => "FF-ERR-FMT-CORRUPT",
            Self::FmtDrm => "FF-ERR-FMT-DRM",
            Self::SecTraversal => "FF-ERR-SEC-TRAVERSAL",
            Self::SecBomb => "FF-ERR-SEC-BOMB",
            Self::SecSpoofed => "FF-ERR-SEC-SPOOFED",
            Self::EngMissing => "FF-ERR-ENG-MISSING",
            Self::ResMemory => "FF-ERR-RES-MEMORY",
            Self::JobCancelled => "FF-ERR-JOB-CANCELLED",
            Self::IntInvariant => "FF-ERR-INT-INVARIANT",
        }
    }

    /// Returns the [`ErrorCategory`] associated with this code.
    #[must_use]
    pub const fn category(self) -> ErrorCategory {
        match self {
            Self::IoNotFound => ErrorCategory::Io,
            Self::FmtUnsupported | Self::FmtCorrupt | Self::FmtDrm => ErrorCategory::Input,
            Self::SecTraversal | Self::SecBomb | Self::SecSpoofed => ErrorCategory::Security,
            Self::EngMissing => ErrorCategory::MissingEngine,
            Self::ResMemory => ErrorCategory::ResourceLimit,
            Self::JobCancelled => ErrorCategory::Cancelled,
            Self::IntInvariant => ErrorCategory::Internal,
        }
    }

    /// Maps the error code deterministically to [`CliExitCode`].
    #[must_use]
    pub const fn exit_code(self) -> CliExitCode {
        match self {
            Self::SecTraversal | Self::SecBomb | Self::SecSpoofed => CliExitCode::SecurityRejection,
            Self::JobCancelled => CliExitCode::Cancelled,
            Self::EngMissing => CliExitCode::MissingEngine,
            Self::IoNotFound
            | Self::FmtUnsupported
            | Self::FmtCorrupt
            | Self::FmtDrm
            | Self::ResMemory
            | Self::IntInvariant => CliExitCode::OperationalFailure,
        }
    }
}

impl fmt::Display for ErrorCode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Serializable diagnostic payload surfaced across CLI, FFI, WASM, and REST bridges.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ErrorDiagnostic {
    /// Canonical `FF-ERR-*` code.
    pub code: ErrorCode,
    /// Error category.
    pub category: ErrorCategory,
    /// Human-readable description of the failure.
    pub message: String,
    /// Actionable next step for the user or operator.
    pub remediation: Option<String>,
    /// Whether retrying the operation may succeed without changing input.
    pub retryable: bool,
}

/// Canonical `FileFusion` error carrying structured diagnostic context and optional source.
#[derive(Debug, Error)]
#[error("{}: {}", .diagnostic.code, .diagnostic.message)]
pub struct FileFusionError {
    /// Actionable structured diagnostic payload.
    pub diagnostic: ErrorDiagnostic,
    #[source]
    source: Option<Box<dyn std::error::Error + Send + Sync>>,
}

/// Convenience result alias used across `FileFusion` crates.
pub type FileFusionResult<T> = std::result::Result<T, FileFusionError>;

impl FileFusionError {
    /// Creates a new [`FileFusionError`] from an [`ErrorCode`] and message.
    #[must_use]
    pub fn new(code: ErrorCode, message: impl Into<String>) -> Self {
        Self {
            diagnostic: ErrorDiagnostic {
                code,
                category: code.category(),
                message: message.into(),
                remediation: None,
                retryable: false,
            },
            source: None,
        }
    }

    /// Attaches an actionable remediation hint for UI/CLI display.
    #[must_use]
    pub fn with_remediation(mut self, remediation: impl Into<String>) -> Self {
        self.diagnostic.remediation = Some(remediation.into());
        self
    }

    /// Marks whether this failure is potentially transient/retryable.
    #[must_use]
    pub const fn retryable(mut self, retryable: bool) -> Self {
        self.diagnostic.retryable = retryable;
        self
    }

    /// Attaches an underlying cause error while keeping diagnostics serializable.
    #[must_use]
    pub fn with_source(mut self, source: impl std::error::Error + Send + Sync + 'static) -> Self {
        self.source = Some(Box::new(source));
        self
    }

    /// Returns the deterministic CLI exit code for this error.
    #[must_use]
    pub const fn exit_code(&self) -> CliExitCode {
        self.diagnostic.code.exit_code()
    }
}

impl From<std::io::Error> for FileFusionError {
    fn from(err: std::io::Error) -> Self {
        let code = match err.kind() {
            std::io::ErrorKind::NotFound | std::io::ErrorKind::PermissionDenied => {
                ErrorCode::IoNotFound
            }
            std::io::ErrorKind::InvalidData | std::io::ErrorKind::UnexpectedEof => {
                ErrorCode::FmtCorrupt
            }
            _ => ErrorCode::IntInvariant,
        };
        let remediation = match code {
            ErrorCode::IoNotFound => {
                "Verify that the file path exists and OS permissions allow access."
            }
            ErrorCode::FmtCorrupt => {
                "Check whether the file is truncated or run File Repair diagnostics."
            }
            _ => "Inspect system logs or retry the operation.",
        };
        let retryable = matches!(
            err.kind(),
            std::io::ErrorKind::TimedOut
                | std::io::ErrorKind::Interrupted
                | std::io::ErrorKind::WouldBlock
        );
        Self::new(code, err.to_string())
            .with_remediation(remediation)
            .retryable(retryable)
            .with_source(err)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn error_codes_serialize_with_canonical_ff_err_strings() -> Result<(), serde_json::Error> {
        let err = FileFusionError::new(ErrorCode::SecBomb, "ratio 250:1 exceeds ceiling 100:1")
            .with_remediation("Inspect archive or raise ResourceLimits::max_expansion_ratio");
        let json = serde_json::to_string(&err.diagnostic)?;
        assert!(json.contains("\"FF-ERR-SEC-BOMB\""));
        assert!(json.contains("\"security\""));
        assert_eq!(err.exit_code(), CliExitCode::SecurityRejection);
        Ok(())
    }

    #[test]
    fn io_errors_map_to_actionable_diagnostics() {
        let not_found = std::io::Error::new(std::io::ErrorKind::NotFound, "missing.pdf");
        let ff_err = FileFusionError::from(not_found);
        assert_eq!(ff_err.diagnostic.code, ErrorCode::IoNotFound);
        assert_eq!(ff_err.diagnostic.category, ErrorCategory::Io);
        assert!(ff_err.diagnostic.remediation.is_some());
        assert!(!ff_err.diagnostic.retryable);

        let timed_out = std::io::Error::new(std::io::ErrorKind::TimedOut, "io timeout");
        let retry_err = FileFusionError::from(timed_out);
        assert!(retry_err.diagnostic.retryable);
    }
}
