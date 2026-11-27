//! `file-core`: L0 foundation crate for `FileFusion`.
//!
//! Established in Subphase `0.3.2` as the initial verifiable Cargo workspace anchor.
//! Domain contracts, error diagnostics, magic-byte signatures, digests, and resource
//! bounds will be implemented incrementally starting in Stage 1 (`1.1.1`–`1.2.4`).

#![forbid(unsafe_code)]

/// Canonical package name of the L0 foundation crate.
pub const CRATE_NAME: &str = env!("CARGO_PKG_NAME");

/// Semantic version of the `FileFusion` workspace baseline.
pub const WORKSPACE_VERSION: &str = env!("CARGO_PKG_VERSION");

/// Returns a deterministic identifier string for workspace verification.
#[must_use]
pub fn workspace_identity() -> &'static str {
    concat!(env!("CARGO_PKG_NAME"), "/", env!("CARGO_PKG_VERSION"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn workspace_identity_is_consistent() {
        assert_eq!(CRATE_NAME, "file-core");
        assert_ne!(WORKSPACE_VERSION, "");
        assert_eq!(
            workspace_identity(),
            format!("{CRATE_NAME}/{WORKSPACE_VERSION}")
        );
    }
}
