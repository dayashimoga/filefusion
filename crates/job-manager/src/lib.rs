//! `job-manager`: L5 control plane crate for `FileFusion`.
//!
//! Coordinates cooperative cancellation, security validation hooks, and runtime
//! doctor diagnostics without UI coupling (`MOD-RULE-01`, `MOD-RULE-03`).

#![forbid(unsafe_code)]

use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};

pub use codec_registry::{DoctorReport, collect_baseline_doctor_report};
pub use file_core::{CliExitCode, JobStage, ProgressEvent, ResourceLimits, workspace_identity};
pub use validation_engine::{
    BombGuardError, PathSafetyError, validate_archive_member_path, validate_expansion_budget,
};

/// Cooperative thread-safe cancellation token checked at chunk/step boundaries (`NFR-REL-002`).
#[derive(Debug, Clone, Default)]
pub struct CancellationToken {
    cancelled: Arc<AtomicBool>,
}

impl CancellationToken {
    /// Creates a new active (non-cancelled) token.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Signals cancellation across all cloned handles immediately.
    pub fn cancel(&self) {
        self.cancelled.store(true, Ordering::SeqCst);
    }

    /// Returns `true` once [`CancellationToken::cancel`] has been called.
    #[must_use]
    pub fn is_cancelled(&self) -> bool {
        self.cancelled.load(Ordering::SeqCst)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cancellation_token_propagates_across_clones() {
        let token = CancellationToken::new();
        let worker_handle = token.clone();
        assert!(!token.is_cancelled());
        assert!(!worker_handle.is_cancelled());

        token.cancel();
        assert!(token.is_cancelled());
        assert!(worker_handle.is_cancelled());
    }

    #[test]
    fn job_manager_exposes_doctor_report() {
        let report = collect_baseline_doctor_report();
        assert_eq!(report.workspace_version, file_core::WORKSPACE_VERSION);
        assert_eq!(report.capabilities.len(), 1);
    }
}
