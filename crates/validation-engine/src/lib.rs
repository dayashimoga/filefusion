//! `validation-engine`: L1 untrusted-input security and integrity gate.
//!
//! Enforces path-traversal rejection and decompression-bomb ceilings defined in
//! `docs/CONTRACTS.md` and `SECURITY.md`.

#![forbid(unsafe_code)]

use core::fmt;
use file_core::ResourceLimits;
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

#[cfg(test)]
mod tests {
    use super::*;

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
}
