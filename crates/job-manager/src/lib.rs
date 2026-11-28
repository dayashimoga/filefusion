//! `job-manager`: L5 control plane crate for `FileFusion`.
//!
//! Coordinates cooperative cancellation, structured `tracing` spans, security
//! validation hooks, and runtime doctor diagnostics without UI coupling
//! (`MOD-RULE-01`, `MOD-RULE-03`).

#![forbid(unsafe_code)]

pub mod backends;
pub mod progress;
pub mod scheduler;

use std::collections::HashMap;
use std::path::Path;
use std::sync::{Arc, Mutex};

pub use backends::{BackendCatalog, NativeIsolatedProcessRunner};
pub use codec_registry::{
    CapabilityEntry, CapabilityRegistry, DoctorReport, EngineTier, LicenseClass, ToolAvailability,
    collect_baseline_doctor_report, collect_doctor_report,
};
pub use file_core::{
    BatchDefaults, CallbackProgressSink, CancelSubscription, CancellationToken, CliConfigOverrides,
    CliExitCode, CollisionPolicy, CompressibilityTier, CompressionEngine, CompressionOutcome,
    CompressionProfile, CompressionRequest, CompressionStatus, ConfigSource, ConversionEngine,
    ConversionFidelityReport, ConversionRequest, ConversionRoute, DetectionConfidence,
    DeviceProfile, DigestSet, ErrorCategory, ErrorCode, ErrorDiagnostic, ExternalToolAdapter,
    FidelitySeverity, FileClassification, FileFusionConfig, FileFusionError, FileFusionResult,
    FileInspectionReport, FormatId, IsolatedProcessRunner, ItemOutcome, ItemStatus, JobContext,
    JobReceipt, JobStage, LoadedConfig, LogFormat, LogLevel, LoggingConfig, MediaKind,
    MemoryBudgetSnapshot, MemoryGovernor, MemoryReservation, MetadataSummary,
    MonotonicProgressSink, NoOpProgressSink, OutputValidator, PathConfig, PreflightAssessment,
    PreflightWarning, PrivacyConfig, ProcessInvocation, ProcessOutput, ProcessingDomain,
    ProcessingHandler, ProcessingOperation, ProcessingRequest, ProgressEvent, ProgressSink,
    QualityEvaluator, QualityMetrics, RecommendedAction, ResourceLimits, SignatureMatch,
    StagedPath, StreamDescriptor, StreamKind, ThrottledProgressSink, ValidatedSource,
    ValidationOutcome, VecProgressSink, classify_inspection_report, classify_processing_domain,
    compute_digests_from_bytes, compute_entropy_milli_bits, copy_bounded_streaming,
    detect_signature_from_bytes, detect_signature_from_path, enforce_spoofing_policy, init_logging,
    inspect_bytes, inspect_path, load_config_from_path, load_effective_config,
    parse_proc_meminfo_mb, redact_path_for_log, resolve_default_config_path,
    resolve_output_destination, save_config_atomic, workspace_identity,
};
pub use hardware_acceleration::{
    HardwareEncoderCapability, HardwareProfile, HardwareRoutingDecision, HardwareVendorBackend,
    SimdFeatures, probe_hardware as probe_hardware_acceleration, select_video_backend,
};
pub use progress::{ProgressBus, ProgressBusStats, ProgressSubscription};
pub use scheduler::{
    JobPriority, JobScheduler, ScheduledItemSpec, SchedulerController, SchedulerStats,
};
pub use validation_engine::{
    BombGuardError, MagicByteOutputValidator, PathSafetyError, validate_archive_member_path,
    validate_expansion_budget,
};

#[derive(Debug, Clone)]
struct ActiveJobEntry {
    cancellation: CancellationToken,
    controller: Option<SchedulerController>,
}

struct ActiveJobGuard {
    registry: Arc<Mutex<HashMap<String, ActiveJobEntry>>>,
    job_id: String,
}

impl ActiveJobGuard {
    fn register(
        registry: &Arc<Mutex<HashMap<String, ActiveJobEntry>>>,
        job_id: &str,
        cancellation: CancellationToken,
        controller: Option<SchedulerController>,
    ) -> Self {
        let mut guard = match registry.lock() {
            Ok(g) => g,
            Err(p) => p.into_inner(),
        };
        guard.insert(
            job_id.to_owned(),
            ActiveJobEntry {
                cancellation,
                controller,
            },
        );
        Self {
            registry: Arc::clone(registry),
            job_id: job_id.to_owned(),
        }
    }
}

impl Drop for ActiveJobGuard {
    fn drop(&mut self) {
        let mut guard = match self.registry.lock() {
            Ok(g) => g,
            Err(p) => p.into_inner(),
        };
        guard.remove(&self.job_id);
    }
}

/// Top-level L5 processing coordinator enforcing cancellation, validation, and receipts.
#[derive(Debug, Clone)]
pub struct JobManager {
    limits: ResourceLimits,
    defaults: BatchDefaults,
    active_jobs: Arc<Mutex<HashMap<String, ActiveJobEntry>>>,
    governor: Arc<MemoryGovernor>,
}

impl Default for JobManager {
    fn default() -> Self {
        Self::new(ResourceLimits::default(), BatchDefaults::default())
    }
}

impl JobManager {
    /// Creates a [`JobManager`] bound to explicit runtime limits and batch defaults.
    #[must_use]
    pub fn new(limits: ResourceLimits, defaults: BatchDefaults) -> Self {
        Self {
            limits,
            defaults,
            active_jobs: Arc::new(Mutex::new(HashMap::new())),
            governor: Arc::new(MemoryGovernor::new(limits)),
        }
    }

    /// Creates a [`JobManager`] from a validated [`FileFusionConfig`].
    #[must_use]
    pub fn from_config(config: &FileFusionConfig) -> Self {
        Self::new(config.limits, config.batch.clone())
    }

    /// Returns the default [`BatchDefaults`] configured on this coordinator.
    #[must_use]
    pub const fn defaults(&self) -> &BatchDefaults {
        &self.defaults
    }

    /// Returns the shared [`MemoryGovernor`] enforcing working-set permits across jobs.
    #[must_use]
    pub const fn memory_governor(&self) -> &Arc<MemoryGovernor> {
        &self.governor
    }

    /// Returns a live [`MemoryBudgetSnapshot`] across jobs managed by this coordinator.
    #[must_use]
    pub fn memory_stats(&self) -> MemoryBudgetSnapshot {
        self.governor.snapshot()
    }

    /// Creates a [`JobContext`] pre-wired with an isolated [`NativeIsolatedProcessRunner`]
    /// and the coordinator's shared [`MemoryGovernor`].
    #[must_use]
    pub fn create_context(&self, job_id: impl Into<String>) -> JobContext {
        JobContext::new(job_id, self.limits)
            .with_process_runner(Arc::new(NativeIsolatedProcessRunner::new()))
            .with_memory_governor(Arc::clone(&self.governor))
    }

    /// Probes CPU SIMD instructions and optional GPU hardware encoders (`FR-ADP-002`).
    #[must_use]
    pub fn probe_hardware(&self) -> HardwareProfile {
        probe_hardware_acceleration()
    }

    /// Returns a bounded [`JobScheduler`] configured with this coordinator's [`ResourceLimits`].
    #[must_use]
    pub const fn scheduler(&self) -> JobScheduler {
        JobScheduler::new(self.limits)
    }

    /// Creates a thread-safe [`SchedulerController`] for pause/resume, per-item cancel, and stats.
    #[must_use]
    pub fn create_scheduler_controller(&self) -> SchedulerController {
        SchedulerController::new(self.limits)
    }

    /// Cancels an active running job by `job_id`, triggering immediate `< 500 ms` abort
    /// (`NFR-REL-002`). Returns `true` if `job_id` was registered as running.
    #[must_use]
    pub fn cancel_job(&self, job_id: &str) -> bool {
        let entry = {
            let guard = match self.active_jobs.lock() {
                Ok(g) => g,
                Err(p) => p.into_inner(),
            };
            guard.get(job_id).cloned()
        };
        let Some(active) = entry else {
            return false;
        };
        active.cancellation.cancel();
        if let Some(ctrl) = active.controller {
            ctrl.cancel_all();
        }
        true
    }

    /// Returns a sorted snapshot of currently active running `job_id` identifiers.
    #[must_use]
    pub fn active_job_ids(&self) -> Vec<String> {
        let guard = match self.active_jobs.lock() {
            Ok(g) => g,
            Err(p) => p.into_inner(),
        };
        let mut ids: Vec<String> = guard.keys().cloned().collect();
        ids.sort();
        ids
    }

    /// Executes a scheduled batch of [`ScheduledItemSpec`] work items through the bounded scheduler.
    ///
    /// # Errors
    /// Returns [`FileFusionError`] on invalid inputs, scheduler initialization errors, or cancellation.
    pub fn execute_scheduled_batch(
        &self,
        req: &ProcessingRequest,
        items: &[ScheduledItemSpec],
        handler: Option<&dyn ProcessingHandler>,
        controller: Option<&SchedulerController>,
        ctx: &JobContext,
    ) -> FileFusionResult<JobReceipt> {
        let _active_guard = ActiveJobGuard::register(
            &self.active_jobs,
            &req.job_id,
            ctx.cancellation.clone(),
            controller.cloned(),
        );
        self.scheduler()
            .execute_scheduled_items(req, items, handler, controller, ctx)
    }

    /// Executes a batch request through the bounded [`JobScheduler`] via `handler`
    /// (or performs validated inspection/dry-run).
    ///
    /// Checks [`CancellationToken`] at each item boundary, validates inputs via [`ValidatedSource`],
    /// resolves destination paths via [`resolve_output_destination`], enforces bounded concurrency
    /// and RAM permits, and emits deterministic [`ItemOutcome`] receipts sorted by item index.
    ///
    /// # Errors
    /// Returns [`FileFusionError`] (`ErrorCode::JobCancelled`) immediately on cancellation or if `req.inputs` is empty.
    pub fn execute_batch(
        &self,
        req: &ProcessingRequest,
        handler: Option<&dyn ProcessingHandler>,
        ctx: &JobContext,
    ) -> FileFusionResult<JobReceipt> {
        let ctrl = self.create_scheduler_controller();
        let _active_guard = ActiveJobGuard::register(
            &self.active_jobs,
            &req.job_id,
            ctx.cancellation.clone(),
            Some(ctrl.clone()),
        );
        let specs: Vec<ScheduledItemSpec> = req
            .inputs
            .iter()
            .enumerate()
            .map(|(idx, path)| {
                let item_index = u32::try_from(idx + 1).unwrap_or(u32::MAX);
                ScheduledItemSpec::new(item_index, idx + 1, path.clone())
                    .with_cancellation(ctx.cancellation.child())
            })
            .collect();
        if specs.is_empty() {
            return self.scheduler().execute_batch(req, handler, ctx);
        }
        self.scheduler()
            .execute_scheduled_items(req, &specs, handler, Some(&ctrl), ctx)
    }
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn run_validated_item(
    limits: &ResourceLimits,
    input_path: &Path,
    item_index: u32,
    seq_1based: usize,
    total: u32,
    req: &ProcessingRequest,
    handler: Option<&dyn ProcessingHandler>,
    ctx: &JobContext,
) -> FileFusionResult<ItemOutcome> {
    ctx.check_cancelled()?;
    let start_percent = 0.0_f32;
    ctx.emit_progress(
        item_index,
        total,
        JobStage::Probing,
        start_percent,
        0,
        format!(
            "Inspecting {}",
            redact_path_for_log(&input_path.to_string_lossy())
        ),
    );

    let done_percent = 100.0_f32;
    let source = match ValidatedSource::open(input_path, req.strict_spoofing, limits) {
        Ok(src) => src,
        Err(err) => {
            if err.diagnostic.code == ErrorCode::JobCancelled {
                return Err(err);
            }
            return Ok(emit_failed_outcome(
                ctx,
                item_index,
                total,
                done_percent,
                input_path,
                0,
                String::new(),
                err,
            ));
        }
    };

    let bytes_in = source.inspection.byte_size;
    let hash8: String = source.inspection.digests.blake3.chars().take(8).collect();
    let (profile_slug, target_ext) = operation_tokens(&req.operation);

    let resolved_dest = match resolve_output_destination(
        input_path,
        req.output_dir.as_deref(),
        &hash8,
        profile_slug,
        seq_1based,
        target_ext,
        &req.batch,
    ) {
        Ok(Some(dest)) => Some(dest),
        Ok(None) => {
            return Ok(skipped_collision_outcome(
                ctx,
                item_index,
                total,
                done_percent,
                input_path,
                bytes_in,
                source.inspection.digests.blake3,
            ));
        }
        Err(err) => {
            return Ok(emit_failed_outcome(
                ctx,
                item_index,
                total,
                done_percent,
                input_path,
                bytes_in,
                source.inspection.digests.blake3,
                err,
            ));
        }
    };

    let outcome = execute_handler_or_default(
        &source,
        resolved_dest,
        req,
        handler,
        item_index,
        total,
        done_percent,
        input_path,
        ctx,
    )?;

    ctx.emit_progress(
        item_index,
        total,
        JobStage::Completed,
        done_percent,
        bytes_in,
        "Item completed",
    );
    Ok(outcome)
}

#[allow(clippy::too_many_arguments)]
fn execute_handler_or_default(
    source: &ValidatedSource,
    resolved_dest: Option<std::path::PathBuf>,
    req: &ProcessingRequest,
    handler: Option<&dyn ProcessingHandler>,
    item_index: u32,
    total: u32,
    done_percent: f32,
    input_path: &Path,
    ctx: &JobContext,
) -> FileFusionResult<ItemOutcome> {
    let bytes_in = source.inspection.byte_size;
    let Some(active_handler) = handler
        .filter(|_| !req.dry_run && !matches!(req.operation, ProcessingOperation::InspectOnly))
    else {
        return Ok(ItemOutcome {
            item_index,
            source_path: input_path.to_path_buf(),
            output_path: if matches!(req.operation, ProcessingOperation::InspectOnly) {
                None
            } else {
                resolved_dest
            },
            status: ItemStatus::Succeeded,
            compression_status: None,
            bytes_in,
            bytes_out: bytes_in,
            blake3_in: source.inspection.digests.blake3.clone(),
            blake3_out: None,
            error: None,
        });
    };

    ctx.emit_progress(
        item_index,
        total,
        JobStage::Processing,
        40.0,
        bytes_in,
        "Executing processing handler",
    );
    match active_handler.process_item(source, resolved_dest.as_deref(), req, item_index, ctx) {
        Ok(out) => {
            ctx.emit_progress_estimate(
                item_index,
                total,
                JobStage::Validating,
                90.0,
                bytes_in,
                Some(out.bytes_out),
                None,
                "Verifying processed output",
            );
            Ok(out)
        }
        Err(err) => {
            if err.diagnostic.code == ErrorCode::JobCancelled {
                return Err(err);
            }
            Ok(emit_failed_outcome(
                ctx,
                item_index,
                total,
                done_percent,
                input_path,
                bytes_in,
                source.inspection.digests.blake3.clone(),
                err,
            ))
        }
    }
}

fn skipped_collision_outcome(
    ctx: &JobContext,
    item_index: u32,
    total: u32,
    done_percent: f32,
    input_path: &Path,
    bytes_in: u64,
    blake3_in: String,
) -> ItemOutcome {
    ctx.emit_progress(
        item_index,
        total,
        JobStage::Completed,
        done_percent,
        bytes_in,
        "Skipped existing destination (collision policy)",
    );
    ItemOutcome {
        item_index,
        source_path: input_path.to_path_buf(),
        output_path: None,
        status: ItemStatus::SkippedCollision,
        compression_status: None,
        bytes_in,
        bytes_out: bytes_in,
        blake3_in,
        blake3_out: None,
        error: None,
    }
}

fn operation_tokens(operation: &ProcessingOperation) -> (&'static str, Option<&'static str>) {
    match operation {
        ProcessingOperation::InspectOnly => ("inspect", None),
        ProcessingOperation::Compress {
            profile,
            target_format,
            ..
        } => (
            profile.slug(),
            target_format.and_then(|f| f.canonical_extensions().first().copied()),
        ),
        ProcessingOperation::Convert { target_format, .. } => (
            "convert",
            target_format.canonical_extensions().first().copied(),
        ),
    }
}

#[allow(clippy::too_many_arguments)]
fn emit_failed_outcome(
    ctx: &JobContext,
    item_index: u32,
    total: u32,
    done_percent: f32,
    input_path: &Path,
    bytes_in: u64,
    blake3_in: String,
    err: FileFusionError,
) -> ItemOutcome {
    ctx.emit_progress(
        item_index,
        total,
        JobStage::Failed,
        done_percent,
        bytes_in,
        err.diagnostic.message.clone(),
    );
    ItemOutcome {
        item_index,
        source_path: input_path.to_path_buf(),
        output_path: None,
        status: ItemStatus::Failed,
        compression_status: None,
        bytes_in,
        bytes_out: 0,
        blake3_in,
        blake3_out: None,
        error: Some(err.diagnostic),
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

    #[test]
    fn execute_batch_emits_progress_receipts_and_honors_cancellation()
    -> Result<(), Box<dyn std::error::Error>> {
        let dir = std::env::temp_dir().join(format!("filefusion_jm_batch_{}", std::process::id()));
        if dir.exists() {
            std::fs::remove_dir_all(&dir)?;
        }
        std::fs::create_dir_all(&dir)?;
        let f1 = dir.join("note.txt");
        std::fs::write(&f1, b"hello FileFusion")?;

        let manager = JobManager::from_config(&FileFusionConfig::default());
        let sink = std::sync::Arc::new(VecProgressSink::new());
        let ctx = JobContext::new("job_01", manager.limits).with_progress(sink.clone());

        let req = ProcessingRequest {
            schema_version: file_core::CORE_SCHEMA_VERSION,
            job_id: String::from("job_01"),
            inputs: vec![f1],
            output_dir: Some(dir.clone()),
            operation: ProcessingOperation::InspectOnly,
            batch: manager.defaults().clone(),
            strict_spoofing: true,
            dry_run: true,
        };

        let receipt = manager.execute_batch(&req, None, &ctx)?;
        assert_eq!(receipt.status, JobStage::Completed);
        assert_eq!(receipt.succeeded, 1);
        assert_eq!(receipt.bytes_in_total, 16);
        assert_eq!(sink.snapshot().len(), 2);

        let cancel_ctx = JobContext::new("job_02", manager.limits);
        cancel_ctx.cancellation.cancel();
        let aborted = manager.execute_batch(&req, None, &cancel_ctx);
        assert!(aborted.is_err());
        if let Err(err) = aborted {
            assert_eq!(err.diagnostic.code, ErrorCode::JobCancelled);
        }

        let _ = std::fs::remove_dir_all(&dir);
        Ok(())
    }

    #[test]
    fn job_manager_context_shares_memory_governor_and_tracks_reservations() -> FileFusionResult<()>
    {
        let limits = ResourceLimits {
            max_memory_mb: 256,
            ..ResourceLimits::default()
        };
        let manager = JobManager::new(limits, BatchDefaults::default());
        let ctx = manager.create_context("job_mem_gov");

        assert_eq!(manager.memory_stats().limit_mb, 256);
        assert_eq!(manager.memory_stats().used_mb, 0);

        let guard = ctx.reserve_working_set_mb(180, "stage_decode")?;
        assert!(guard.is_some());
        assert_eq!(manager.memory_stats().used_mb, 180);

        let overflow = ctx.reserve_working_set_mb(100, "stage_overflow");
        assert!(overflow.is_err());
        if let Err(err) = overflow {
            assert_eq!(err.diagnostic.code, ErrorCode::ResMemory);
        }

        drop(guard);
        assert_eq!(manager.memory_stats().used_mb, 0);
        assert_eq!(manager.memory_stats().peak_used_mb, 180);
        assert_eq!(manager.memory_stats().rejected_requests, 1);
        Ok(())
    }
}
