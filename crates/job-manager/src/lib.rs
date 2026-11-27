//! `job-manager`: L5 control plane crate for `FileFusion`.
//!
//! Coordinates cooperative cancellation, structured `tracing` spans, security
//! validation hooks, and runtime doctor diagnostics without UI coupling
//! (`MOD-RULE-01`, `MOD-RULE-03`).

#![forbid(unsafe_code)]

use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};

pub use codec_registry::{DoctorReport, collect_baseline_doctor_report};
pub use file_core::{
    CliExitCode, ErrorCategory, ErrorCode, ErrorDiagnostic, FileFusionError, FileFusionResult,
    JobStage, LogFormat, LogLevel, LoggingConfig, ProgressEvent, ResourceLimits, init_logging,
    redact_path_for_log, workspace_identity,
};
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
        tracing::info!(event = "job_cancelled", "cancellation signal raised");
    }

    /// Returns `true` once [`CancellationToken::cancel`] has been called.
    #[must_use]
    pub fn is_cancelled(&self) -> bool {
        self.cancelled.load(Ordering::SeqCst)
    }

    /// Returns `Err(FileFusionError)` (`FF-ERR-JOB-CANCELLED`) if cancelled.
    ///
    /// # Errors
    /// Returns [`FileFusionError`] with [`ErrorCode::JobCancelled`] when aborted.
    pub fn check(&self) -> FileFusionResult<()> {
        if self.is_cancelled() {
            return Err(
                FileFusionError::new(ErrorCode::JobCancelled, "job cancelled by user")
                    .with_remediation("Temporary staging files have been removed."),
            );
        }
        Ok(())
    }
}

/// Runs diagnostic capability probing inside a structured `tracing` span.
#[must_use]
pub fn run_doctor_probe() -> DoctorReport {
    let span = tracing::info_span!("doctor_probe", workspace = workspace_identity());
    let _guard = span.enter();
    let report = collect_baseline_doctor_report();
    tracing::info!(
        host_os = %report.host_os,
        host_arch = %report.host_arch,
        logical_cpus = report.logical_cpus,
        capabilities = report.capabilities.len(),
        "doctor diagnostics collected"
    );
    report
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cancellation_token_propagates_and_returns_typed_error() {
        let token = CancellationToken::new();
        let worker_handle = token.clone();
        assert!(!token.is_cancelled());
        assert!(token.check().is_ok());

        token.cancel();
        assert!(worker_handle.is_cancelled());
        let err = worker_handle.check().err();
        assert!(err.is_some());
        if let Some(e) = err {
            assert_eq!(e.diagnostic.code, ErrorCode::JobCancelled);
            assert_eq!(e.exit_code(), CliExitCode::Cancelled);
        }
    }

    #[test]
    fn job_manager_exposes_traced_doctor_report() {
        let report = run_doctor_probe();
        assert_eq!(report.workspace_version, file_core::WORKSPACE_VERSION);
        assert_eq!(report.capabilities.len(), 1);
    }
}
