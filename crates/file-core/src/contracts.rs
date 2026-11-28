//! Canonical backend, engine, quality, validation, and isolated-subprocess trait
//! contracts (`FF-CTR-001` §3, `MOD-RULE-01`, `MOD-RULE-04`).
//!
//! All traits and DTOs live in `file-core` (`L0`) so `L1`–`L4` format crates and
//! `L5` (`job-manager`) share a unified ABI without violating the acyclic DAG.

use std::ffi::OsString;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::error::FileFusionResult;
use crate::metadata::FileInspectionReport;
use crate::pipeline::{
    CompressionProfile, CompressionStatus, JobContext, ProcessingRequest, StagedPath,
    ValidatedSource,
};
use crate::{FormatId, ResourceLimits};

/// Request parameters passed into a [`CompressionEngine`] implementation (`FF-CTR-001` §3.1).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CompressionRequest {
    /// Selected standard preset profile (`FR-CMP-001`).
    pub profile: CompressionProfile,
    /// Optional format transcode target (`None` preserves source format).
    pub target_format: Option<FormatId>,
    /// Optional output size cap in bytes (`FR-CMP-003`).
    pub target_max_bytes: Option<u64>,
    /// Optional minimum structural similarity quality floor (`0.0..=1.0`).
    pub min_ssim: Option<f64>,
    /// Strip non-essential EXIF, GPS, and author metadata (`FR-MET-002`).
    pub strip_metadata: bool,
    /// Enforce `FR-CMP-002`: retain original untouched if same-format output is larger.
    pub no_op_if_larger: bool,
}

/// Empirical perceptual and lossless verification metrics (`FF-CTR-001` §3.3).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct QualityMetrics {
    /// Structural similarity index (`0.0..=1.0`) for visual outputs.
    pub ssim: Option<f64>,
    /// Peak signal-to-noise ratio in dB.
    pub psnr_db: Option<f64>,
    /// Video Multimethod Assessment Fusion score (`0.0..=100.0`).
    pub vmaf: Option<f64>,
    /// Bit-exact decoded stream or byte-stream equivalence flag.
    pub bit_exact_lossless: Option<bool>,
}

/// Outcome returned by [`CompressionEngine::compress`].
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CompressionOutcome {
    /// Whether output shrank/transcoded or triggered `FR-CMP-002` no-op retention.
    pub status: CompressionStatus,
    /// Original input size in bytes.
    pub bytes_in: u64,
    /// Resulting staged output size in bytes.
    pub bytes_out: u64,
    /// Identifier of the codec/backend used.
    pub codec_used: String,
    /// Optional empirical quality evaluation results.
    pub quality: Option<QualityMetrics>,
}

/// Directed format conversion edge exposed by a [`ConversionEngine`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConversionRoute {
    /// Accepted source format.
    pub from: FormatId,
    /// Produced destination format.
    pub to: FormatId,
    /// `true` when lossless direct mapping exists without rasterization or lossy re-encode.
    pub is_lossless_direct: bool,
    /// Optional Tier 2 external capability ID required on this route.
    pub requires_tier2_tool: Option<String>,
}

/// Severity level for pre-flight and conversion fidelity warnings (`FR-FID-002`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FidelitySeverity {
    /// Informational conversion transformation notice.
    Info,
    /// Minor layout, styling, or metadata fidelity caveat.
    Warning,
    /// Irreversible structural/semantic loss (e.g., formula or animation flatten).
    BreakingLoss,
}

/// Individual fidelity finding emitted prior to or after conversion.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PreflightWarning {
    /// Stable machine diagnostic code (e.g. `FID-XLSX-FORMULAS`).
    pub code: String,
    /// Impact severity.
    pub severity: FidelitySeverity,
    /// Actionable user explanation.
    pub message: String,
}

/// Pre-flight compatibility assessment produced before launching conversion (`FR-FID-002`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PreflightAssessment {
    /// `true` when a valid conversion route exists.
    pub supported: bool,
    /// Resolved route if `supported == true`.
    pub route: Option<ConversionRoute>,
    /// Feature-loss warnings surfaced before execution.
    pub warnings: Vec<PreflightWarning>,
    /// Estimated peak memory working set in MB.
    pub estimated_memory_mb: u64,
}

/// Parameters controlling document or media conversion (`FF-CTR-001` §3.2).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConversionRequest {
    /// Target format to emit.
    pub target_format: FormatId,
    /// Strip private metadata fields when writing target.
    pub strip_metadata: bool,
    /// Preserve headings, tables, links, and embedded assets whenever target supports them.
    pub preserve_structure: bool,
    /// Abort before conversion if any [`FidelitySeverity::BreakingLoss`] warning arises.
    pub fail_on_breaking_loss: bool,
}

/// Quantitative post-conversion structural and visual preservation report (`FR-FID-001`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ConversionFidelityReport {
    /// Contract schema version (`CORE_SCHEMA_VERSION`).
    pub schema_version: u32,
    /// Source format converted from.
    pub source_format: FormatId,
    /// Destination format produced.
    pub target_format: FormatId,
    /// Pre-flight or post-conversion fidelity warnings.
    pub warnings: Vec<PreflightWarning>,
    /// Structural heading count preserved (for markup/document routes).
    pub headings_preserved: Option<u32>,
    /// Table count preserved.
    pub tables_preserved: Option<u32>,
    /// Hyperlink count preserved.
    pub links_preserved: Option<u32>,
    /// Embedded image count preserved.
    pub images_preserved: Option<u32>,
    /// Optional perceptual/lossless quality score.
    pub quality: Option<QualityMetrics>,
}

/// Verification verdict produced by an [`OutputValidator`] before atomic rename (`FR-VAL-001`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ValidationOutcome {
    /// `true` when staged output passed syntactic and signature checks.
    pub valid: bool,
    /// Identifier of the validator that inspected the staged artifact.
    pub validator_id: String,
    /// Human-readable verification summary.
    pub detail: String,
}

/// Sanitized argument-array invocation specification for [`IsolatedProcessRunner`] (`MOD-RULE-04`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProcessInvocation {
    /// Resolved executable path (never passed through a shell interpreter).
    pub executable: PathBuf,
    /// Explicit OS argument vector (`OsString`, never shell-concatenated command strings).
    pub args: Vec<OsString>,
    /// Optional explicit working directory (`None` uses an isolated private temp dir).
    pub working_dir: Option<PathBuf>,
    /// Wall-clock timeout ceiling in seconds (clamped against `JobContext.limits.subprocess_timeout_secs`).
    pub timeout_secs: u64,
    /// Specific host environment variable names allowed through env sanitization.
    pub allow_env_keys: Vec<String>,
}

/// Output captured from a bounded external binary invocation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProcessOutput {
    /// Process exit status code.
    pub exit_code: i32,
    /// Captured standard output bytes (bounded).
    pub stdout: Vec<u8>,
    /// Captured standard error bytes (bounded).
    pub stderr: Vec<u8>,
    /// Elapsed wall-clock runtime in milliseconds.
    pub duration_ms: u64,
    /// `true` if terminated for exceeding `timeout_secs`.
    pub timed_out: bool,
}

/// Trait implemented by `job-manager` and injected via `JobContext` for Tier 2 tool isolation (`FF-CTR-001` §3.3).
pub trait IsolatedProcessRunner: Send + Sync {
    /// Executes an external binary cleanly with sanitized env, private temp dir, timeout, and cancellation kill.
    ///
    /// # Errors
    /// Returns [`crate::FileFusionError`] on spawn failure, timeout, or cooperative cancellation.
    fn run_isolated(
        &self,
        invocation: &ProcessInvocation,
        ctx: &JobContext,
    ) -> FileFusionResult<ProcessOutput>;
}

/// Trait implemented by `L2`/`L3` compression backends (`FF-CTR-001` §3.1).
pub trait CompressionEngine: Send + Sync {
    /// Stable machine identifier (e.g. `"tier1-image-oxipng"`).
    fn engine_id(&self) -> &'static str;

    /// Source formats handled by this engine.
    fn supported_formats(&self) -> &[FormatId];

    /// Estimates peak working-set RAM usage in megabytes before scheduler permit acquisition.
    fn estimate_memory_mb(
        &self,
        inspection: &FileInspectionReport,
        opts: &CompressionRequest,
    ) -> u64;

    /// Compresses `input` into `staged_output`, enforcing `NoOpRetainedOriginal` when appropriate.
    ///
    /// # Errors
    /// Returns [`crate::FileFusionError`] on codec failure, memory ceiling, or cancellation.
    fn compress(
        &self,
        input: &ValidatedSource,
        staged_output: &StagedPath,
        req: &CompressionRequest,
        ctx: &JobContext,
    ) -> FileFusionResult<CompressionOutcome>;
}

/// Trait implemented by `L4` universal format conversion engines (`FF-CTR-001` §3.2).
pub trait ConversionEngine: Send + Sync {
    /// Stable machine identifier.
    fn engine_id(&self) -> &'static str;

    /// Supported directed conversion routes.
    fn routes(&self) -> &[ConversionRoute];

    /// Inspects compatibility and returns pre-flight feature-loss warnings (`FR-FID-002`).
    ///
    /// # Errors
    /// Returns [`crate::FileFusionError`] if inspection or route resolution fails.
    fn preflight(
        &self,
        inspection: &FileInspectionReport,
        target: FormatId,
    ) -> FileFusionResult<PreflightAssessment>;

    /// Executes conversion from `input` into `staged_output` and emits a [`ConversionFidelityReport`].
    ///
    /// # Errors
    /// Returns [`crate::FileFusionError`] if conversion, fidelity gate, or cancellation triggers.
    fn convert(
        &self,
        input: &ValidatedSource,
        staged_output: &StagedPath,
        req: &ConversionRequest,
        ctx: &JobContext,
    ) -> FileFusionResult<ConversionFidelityReport>;
}

/// Adapter trait for out-of-process Tier 2 external binaries (`FF-CTR-001` §3.3, `ADR-002`).
pub trait ExternalToolAdapter: Send + Sync {
    /// Stable adapter identifier (e.g. `"adapter-ffmpeg"`).
    fn adapter_id(&self) -> &'static str;

    /// Matching capability identifier in `codec-registry` (e.g. `"tier2-ffmpeg"`).
    fn capability_id(&self) -> &'static str;

    /// Canonical SPDX license expression for compliance auditing.
    fn spdx_license(&self) -> &'static str;

    /// Builds sanitized OS argument vectors (`Vec<OsString>`); never builds shell command strings.
    ///
    /// # Errors
    /// Returns [`crate::FileFusionError`] if requested options cannot map safely to binary flags.
    fn build_args(
        &self,
        input: &ValidatedSource,
        staged_output: &StagedPath,
        req: &ProcessingRequest,
    ) -> FileFusionResult<Vec<OsString>>;

    /// Runs the external binary strictly through the injected [`IsolatedProcessRunner`].
    ///
    /// # Errors
    /// Returns [`crate::FileFusionError`] on missing binary, cancellation, timeout, or non-zero exit.
    fn execute(
        &self,
        input: &ValidatedSource,
        staged_output: &StagedPath,
        req: &ProcessingRequest,
        runner: &dyn IsolatedProcessRunner,
        ctx: &JobContext,
    ) -> FileFusionResult<ProcessOutput>;
}

/// Quality metrics evaluator (`SSIM`, `PSNR`, `VMAF`, lossless equivalence) (`FF-CTR-001` §3.3).
pub trait QualityEvaluator: Send + Sync {
    /// Evaluator identifier.
    fn evaluator_id(&self) -> &'static str;

    /// Compares `processed_path` against `original` under `ctx` cancellation checks.
    ///
    /// # Errors
    /// Returns [`crate::FileFusionError`] if decoding or comparison fails.
    fn evaluate(
        &self,
        original: &ValidatedSource,
        processed_path: &Path,
        ctx: &JobContext,
    ) -> FileFusionResult<QualityMetrics>;
}

/// Post-processing staged artifact gate before atomic promote (`FR-VAL-001`, `FF-CTR-001` §3.3).
pub trait OutputValidator: Send + Sync {
    /// Validator identifier.
    fn validator_id(&self) -> &'static str;

    /// Re-opens `staged_path` and checks syntactic and binary-signature integrity.
    ///
    /// # Errors
    /// Returns [`crate::FileFusionError`] (`ErrorCode::FmtCorrupt` / security code) if invalid.
    fn validate_staged(
        &self,
        staged_path: &Path,
        expected_format: FormatId,
        limits: &ResourceLimits,
    ) -> FileFusionResult<ValidationOutcome>;
}
