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

pub use codec_registry::{
    CapabilityEntry, CapabilityRegistry, DoctorReport, EngineTier, LicenseClass, ToolAvailability,
    collect_baseline_doctor_report, collect_doctor_report,
};
pub use file_core::{
    BatchDefaults, CliConfigOverrides, CliExitCode, CollisionPolicy, CompressibilityTier,
    ConfigSource, DetectionConfidence, DigestSet, ErrorCategory, ErrorCode, ErrorDiagnostic,
    FileClassification, FileFusionConfig, FileFusionError, FileFusionResult, FileInspectionReport,
    FormatId, JobStage, LoadedConfig, LogFormat, LogLevel, LoggingConfig, MediaKind,
    MetadataSummary, PathConfig, PrivacyConfig, ProcessingDomain, ProgressEvent, RecommendedAction,
    ResourceLimits, SignatureMatch, StreamDescriptor, StreamKind, classify_inspection_report,
    classify_processing_domain, compute_digests_from_bytes, compute_entropy_milli_bits,
    detect_signature_from_bytes, detect_signature_from_path, enforce_spoofing_policy, init_logging,
    inspect_bytes, inspect_path, load_config_from_path, load_effective_config, redact_path_for_log,
    resolve_default_config_path, save_config_atomic, workspace_identity,
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

/// Resolves and validates runtime configuration with privacy-safe tracing.
///
/// # Errors
/// Returns [`FileFusionError`] if an explicit config file is missing/malformed or
/// resulting settings violate configuration invariants.
pub fn load_runtime_config(overrides: &CliConfigOverrides) -> FileFusionResult<LoadedConfig> {
    let loaded = load_effective_config(overrides)?;
    let source_label = match &loaded.source {
        ConfigSource::Default => String::from("default"),
        ConfigSource::UserConfigFile(path) | ConfigSource::ExplicitConfigFile(path) => {
            redact_path_for_log(path)
        }
    };
    tracing::debug!(
        source = %source_label,
        max_workers = loaded.config.limits.max_workers,
        max_memory_mb = loaded.config.limits.max_memory_mb,
        offline_only = loaded.config.privacy.offline_only,
        "runtime configuration resolved"
    );
    Ok(loaded)
}

/// Runs diagnostic capability probing using an active [`FileFusionConfig`].
#[must_use]
pub fn run_doctor_probe_with_config(config: &FileFusionConfig) -> DoctorReport {
    let span = tracing::info_span!("doctor_probe", workspace = workspace_identity());
    let _guard = span.enter();
    let report = collect_doctor_report(config.limits);
    tracing::info!(
        host_os = %report.host_os,
        host_arch = %report.host_arch,
        logical_cpus = report.logical_cpus,
        effective_workers = report.effective_limits.max_workers,
        effective_memory_mb = report.effective_limits.max_memory_mb,
        capabilities = report.capabilities.len(),
        "doctor diagnostics collected"
    );
    report
}

/// Runs diagnostic capability probing inside a structured `tracing` span.
#[must_use]
pub fn run_doctor_probe() -> DoctorReport {
    run_doctor_probe_with_config(&FileFusionConfig::default())
}

/// Probes a file's binary/container signature under privacy-safe `tracing` (`FR-DET-001..002`).
///
/// When `strict_spoofing` is `true`, extension mismatches return [`ErrorCode::SecSpoofed`].
///
/// # Errors
/// Returns [`FileFusionError`] if the file cannot be read or fails strict spoofing checks.
pub fn probe_file_signature(
    path: &std::path::Path,
    strict_spoofing: bool,
) -> FileFusionResult<SignatureMatch> {
    let redacted = redact_path_for_log(&path.to_string_lossy());
    let span = tracing::info_span!("signature_probe", file = %redacted);
    let _guard = span.enter();
    let result = detect_signature_from_path(path)?;
    tracing::info!(
        format = ?result.detected_format,
        mime = %result.mime_type,
        rule = %result.matched_rule,
        spoofed = result.spoofing_mismatch,
        "file signature probed"
    );
    enforce_spoofing_policy(&result, strict_spoofing)?;
    Ok(result)
}

/// Inspects a file's signature, streaming digests, internal streams, and normalized
/// metadata under privacy-safe `tracing` (`FR-DET-001..004`, `FR-MET-001`).
///
/// # Errors
/// Returns [`FileFusionError`] if reading fails or `strict_spoofing` rejects a mismatch.
pub fn inspect_file(
    path: &std::path::Path,
    strict_spoofing: bool,
) -> FileFusionResult<FileInspectionReport> {
    let redacted = redact_path_for_log(&path.to_string_lossy());
    let span = tracing::info_span!("inspect_file", file = %redacted);
    let _guard = span.enter();
    let report = inspect_path(path, strict_spoofing)?;
    tracing::info!(
        format = ?report.detected_format,
        byte_size = report.byte_size,
        streams = report.streams.len(),
        domain = ?report.classification.as_ref().map(|c| c.domain),
        compressibility = ?report.classification.as_ref().map(|c| c.compressibility),
        actions = report.recommended_actions.len(),
        encrypted = report.metadata_summary.is_encrypted,
        gps_risk = report.metadata_summary.has_gps_privacy_risk,
        pii_risk = report.metadata_summary.has_pii_author_risk,
        "file inspection completed"
    );
    Ok(report)
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
        assert_eq!(report.capabilities.len(), 10);
        assert_eq!(report.tool_availability.len(), 7);
    }

    #[test]
    fn doctor_probe_reflects_configured_resource_limits() {
        let mut cfg = FileFusionConfig::default();
        cfg.limits.max_workers = 3;
        cfg.limits.max_memory_mb = 768;
        let report = run_doctor_probe_with_config(&cfg);
        assert_eq!(report.effective_limits.max_workers, 3);
        assert_eq!(report.effective_limits.max_memory_mb, 768);
    }
}
