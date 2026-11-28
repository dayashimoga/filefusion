//! `validation-engine`: L1 untrusted-input security and integrity gate.
//!
//! Enforces path-traversal rejection and decompression-bomb ceilings defined in
//! `docs/CONTRACTS.md` and `SECURITY.md`.

#![forbid(unsafe_code)]

use core::fmt;
use std::fs;
use std::path::Path;

use file_core::{
    ErrorCode, FileFusionError, FileFusionResult, FormatId, OutputValidator, ResourceLimits,
    ValidationOutcome, detect_signature_from_path,
};
use serde::{Deserialize, Serialize};

/// Reasons an archive member path is rejected by the security gate.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum PathSafetyError {
    /// Member path was empty or whitespace-only.
    EmptyPath,
    /// Member path contained an embedded null byte.
    NullByte,
    /// Member path started with `/` or `\` (absolute or root-relative).
    AbsoluteOrRooted,
    /// Member path contained a Windows drive or colon prefix (e.g. `C:`).
    DrivePrefix,
    /// Member path contained a `..` parent traversal segment.
    ParentTraversal,
}

impl fmt::Display for PathSafetyError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::EmptyPath => write!(f, "archive member path cannot be empty"),
            Self::NullByte => write!(f, "archive member path contains a null byte"),
            Self::AbsoluteOrRooted => write!(f, "archive member path cannot be absolute"),
            Self::DrivePrefix => write!(f, "archive member path cannot include a drive prefix"),
            Self::ParentTraversal => write!(f, "archive member path attempts parent traversal"),
        }
    }
}

impl std::error::Error for PathSafetyError {}

impl From<PathSafetyError> for FileFusionError {
    fn from(err: PathSafetyError) -> Self {
        let msg = err.to_string();
        Self::new(ErrorCode::SecTraversal, msg)
            .with_remediation(
                "Archive extraction aborted; inspect archive entries before retrying.",
            )
            .with_source(err)
    }
}

/// Reasons a decompression or nested container payload violates [`ResourceLimits`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum BombGuardError {
    /// Container nesting exceeded `max_archive_depth`.
    DepthExceeded {
        /// Observed nesting depth.
        depth: u8,
        /// Configured ceiling.
        max_depth: u8,
    },
    /// Uncompressed payload exceeded `max_uncompressed_bytes`.
    UncompressedSizeExceeded {
        /// Observed uncompressed bytes.
        bytes: u64,
        /// Configured ceiling.
        max_bytes: u64,
    },
    /// Compression ratio exceeded `max_expansion_ratio`.
    ExpansionRatioExceeded {
        /// Observed expansion ratio (`uncompressed / max(1, compressed)`).
        ratio: u64,
        /// Configured ceiling.
        max_ratio: u32,
    },
}

impl fmt::Display for BombGuardError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::DepthExceeded { depth, max_depth } => {
                write!(f, "archive nesting depth {depth} exceeds limit {max_depth}")
            }
            Self::UncompressedSizeExceeded { bytes, max_bytes } => {
                write!(
                    f,
                    "uncompressed size {bytes} bytes exceeds limit {max_bytes}"
                )
            }
            Self::ExpansionRatioExceeded { ratio, max_ratio } => {
                write!(f, "expansion ratio {ratio}:1 exceeds ceiling {max_ratio}:1")
            }
        }
    }
}

impl std::error::Error for BombGuardError {}

impl From<BombGuardError> for FileFusionError {
    fn from(err: BombGuardError) -> Self {
        let msg = err.to_string();
        Self::new(ErrorCode::SecBomb, msg)
            .with_remediation(
                "Reject untrusted bomb payload or explicitly increase ResourceLimits if trusted.",
            )
            .with_source(err)
    }
}

/// Validates and normalizes an untrusted archive member path (`FR-SEC-001`).
///
/// # Errors
/// Returns [`PathSafetyError`] if the path is empty, contains null bytes,
/// is rooted/absolute, carries a Windows drive prefix, or uses `..` segments.
pub fn validate_archive_member_path(raw: &str) -> Result<String, PathSafetyError> {
    if raw.trim().is_empty() {
        return Err(PathSafetyError::EmptyPath);
    }
    if raw.contains('\0') {
        return Err(PathSafetyError::NullByte);
    }
    if raw.starts_with('/') || raw.starts_with('\\') {
        return Err(PathSafetyError::AbsoluteOrRooted);
    }

    let normalized = raw.replace('\\', "/");
    let mut parts = Vec::new();
    for (idx, segment) in normalized.split('/').enumerate() {
        if segment.is_empty() || segment == "." {
            continue;
        }
        if idx == 0 && segment.contains(':') {
            return Err(PathSafetyError::DrivePrefix);
        }
        if segment == ".." {
            return Err(PathSafetyError::ParentTraversal);
        }
        parts.push(segment);
    }

    if parts.is_empty() {
        return Err(PathSafetyError::EmptyPath);
    }

    Ok(parts.join("/"))
}

/// Validates container depth, uncompressed size, and expansion ratio against [`ResourceLimits`].
///
/// # Errors
/// Returns [`BombGuardError`] if any resource ceiling is exceeded.
pub fn validate_expansion_budget(
    compressed_bytes: u64,
    uncompressed_bytes: u64,
    depth: u8,
    limits: &ResourceLimits,
) -> Result<(), BombGuardError> {
    if depth > limits.max_archive_depth {
        return Err(BombGuardError::DepthExceeded {
            depth,
            max_depth: limits.max_archive_depth,
        });
    }
    if uncompressed_bytes > limits.max_uncompressed_bytes {
        return Err(BombGuardError::UncompressedSizeExceeded {
            bytes: uncompressed_bytes,
            max_bytes: limits.max_uncompressed_bytes,
        });
    }
    if uncompressed_bytes > 0 {
        let denom = compressed_bytes.max(1);
        let ratio = uncompressed_bytes.div_ceil(denom);
        if ratio > u64::from(limits.max_expansion_ratio) {
            return Err(BombGuardError::ExpansionRatioExceeded {
                ratio,
                max_ratio: limits.max_expansion_ratio,
            });
        }
    }
    Ok(())
}

/// Default L1 [`OutputValidator`] verifying file non-emptiness, size ceiling, and
/// magic-byte signature conformance before atomic destination promote (`FR-VAL-001`).
#[derive(Debug, Clone, Copy, Default)]
pub struct MagicByteOutputValidator;

impl OutputValidator for MagicByteOutputValidator {
    fn validator_id(&self) -> &'static str {
        "validation-engine-magic-byte"
    }

    fn validate_staged(
        &self,
        staged_path: &Path,
        expected_format: FormatId,
        limits: &ResourceLimits,
    ) -> FileFusionResult<ValidationOutcome> {
        let meta = fs::metadata(staged_path)?;
        if !meta.is_file() {
            return Err(FileFusionError::new(
                ErrorCode::FmtCorrupt,
                "staged output path is not a regular file",
            )
            .with_remediation("Aborted atomic commit to preserve destination integrity."));
        }
        if meta.len() == 0 {
            return Err(FileFusionError::new(
                ErrorCode::FmtCorrupt,
                "staged output is empty (0 bytes)",
            )
            .with_remediation("Aborted commit; source file remains untouched."));
        }
        if meta.len() > limits.max_uncompressed_bytes {
            return Err(FileFusionError::new(
                ErrorCode::SecBomb,
                format!(
                    "staged output ({} bytes) exceeds limit ({} bytes)",
                    meta.len(),
                    limits.max_uncompressed_bytes
                ),
            ));
        }

        let sig = detect_signature_from_path(staged_path)?;
        let compatible = sig.detected_format == expected_format
            || (expected_format == FormatId::Unknown
                && sig.detected_format != FormatId::Executable);
        if !compatible {
            return Err(FileFusionError::new(
                ErrorCode::FmtCorrupt,
                format!(
                    "staged output signature {:?} does not match expected format {:?}",
                    sig.detected_format, expected_format
                ),
            )
            .with_remediation("OutputValidator rejected corrupt or misencoded staged artifact."));
        }

        Ok(ValidationOutcome {
            valid: true,
            validator_id: String::from(self.validator_id()),
            detail: format!(
                "Verified {:?} ({}) staged output ({} bytes)",
                sig.detected_format,
                sig.mime_type,
                meta.len()
            ),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use file_core::CliExitCode;

    #[test]
    fn rejects_zip_slip_and_unsafe_paths() {
        assert_eq!(
            validate_archive_member_path(""),
            Err(PathSafetyError::EmptyPath)
        );
        assert_eq!(
            validate_archive_member_path("foo\0bar"),
            Err(PathSafetyError::NullByte)
        );
        assert_eq!(
            validate_archive_member_path("/etc/passwd"),
            Err(PathSafetyError::AbsoluteOrRooted)
        );
        assert_eq!(
            validate_archive_member_path("\\\\server\\share\\file"),
            Err(PathSafetyError::AbsoluteOrRooted)
        );
        assert_eq!(
            validate_archive_member_path("C:\\Windows\\System32\\cmd.exe"),
            Err(PathSafetyError::DrivePrefix)
        );
        assert_eq!(
            validate_archive_member_path("../../evil.sh"),
            Err(PathSafetyError::ParentTraversal)
        );
        assert_eq!(
            validate_archive_member_path("sub\\..\\..\\evil.sh"),
            Err(PathSafetyError::ParentTraversal)
        );
    }

    #[test]
    fn normalizes_safe_relative_paths() {
        assert_eq!(
            validate_archive_member_path("./docs\\reports/q1.pdf"),
            Ok(String::from("docs/reports/q1.pdf"))
        );
    }

    #[test]
    fn enforces_bomb_ratio_size_and_depth_limits() {
        let limits = ResourceLimits::default();
        assert_eq!(validate_expansion_budget(1024, 10_240, 1, &limits), Ok(()));
        assert!(matches!(
            validate_expansion_budget(1024, 10_240, 4, &limits),
            Err(BombGuardError::DepthExceeded {
                depth: 4,
                max_depth: 3
            })
        ));
        assert!(matches!(
            validate_expansion_budget(10, 50_000, 1, &limits),
            Err(BombGuardError::ExpansionRatioExceeded { .. })
        ));
        assert!(matches!(
            validate_expansion_budget(
                limits.max_uncompressed_bytes,
                limits.max_uncompressed_bytes + 1,
                1,
                &limits
            ),
            Err(BombGuardError::UncompressedSizeExceeded { .. })
        ));
    }

    #[test]
    fn security_errors_convert_to_canonical_file_fusion_errors() {
        let trav = FileFusionError::from(PathSafetyError::ParentTraversal);
        assert_eq!(trav.diagnostic.code, ErrorCode::SecTraversal);
        assert_eq!(trav.exit_code(), CliExitCode::SecurityRejection);

        let bomb = FileFusionError::from(BombGuardError::DepthExceeded {
            depth: 5,
            max_depth: 3,
        });
        assert_eq!(bomb.diagnostic.code, ErrorCode::SecBomb);
        assert_eq!(bomb.exit_code(), CliExitCode::SecurityRejection);
    }

    #[test]
    fn magic_byte_output_validator_accepts_valid_png_and_rejects_corrupt_or_empty()
    -> Result<(), Box<dyn std::error::Error>> {
        let dir =
            std::env::temp_dir().join(format!("filefusion_val_staged_{}", std::process::id()));
        if dir.exists() {
            fs::remove_dir_all(&dir)?;
        }
        fs::create_dir_all(&dir)?;

        let validator = MagicByteOutputValidator;
        let limits = ResourceLimits::default();

        let valid_png = dir.join("ok.tmp");
        fs::write(&valid_png, b"\x89PNG\r\n\x1a\n\x00\x00\x00\rIHDR")?;
        let ok = validator.validate_staged(&valid_png, FormatId::Png, &limits)?;
        assert!(ok.valid);

        let mismatch = validator.validate_staged(&valid_png, FormatId::Pdf, &limits);
        assert!(mismatch.is_err());
        if let Err(err) = mismatch {
            assert_eq!(err.diagnostic.code, ErrorCode::FmtCorrupt);
        }

        let empty = dir.join("empty.tmp");
        fs::write(&empty, b"")?;
        assert!(
            validator
                .validate_staged(&empty, FormatId::Png, &limits)
                .is_err()
        );

        let _ = fs::remove_dir_all(&dir);
        Ok(())
    }
}
