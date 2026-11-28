//! L5 control-plane orchestration for Stage 4 unified file repair, privacy metadata
//! inspection/sanitization/editing, `.ffenc` container encryption, and PDF AES protection
//! (`FR-REP-001`..`002`, `FR-MET-001`..`002`, `FR-SEC-002`, `FR-PDF-004`).

use std::path::Path;

use file_core::{
    CORE_SCHEMA_VERSION, CollisionPolicy, FileFusionResult, JobContext, MetadataSummary, StagedPath,
};
#[cfg(not(feature = "native-process"))]
use optimization_engine::repair_file_to_staged_tier1;
use optimization_engine::{
    EditableMetadataFields, FfencCipherSuite, FfencHeaderInfo, FfencKdfParams,
    MetadataOperationReport, PdfEncryptionAlgorithm, PdfProtectionOptions, UnifiedRepairDiagnosis,
    UnifiedRepairOutcome, decrypt_ffenc_file_to_staged, decrypt_pdf_file_to_staged,
    diagnose_file_recoverability, encrypt_file_to_ffenc_staged, encrypt_pdf_file_to_staged,
    inspect_ffenc_file, inspect_unified_metadata, sanitize_and_edit_metadata_to_staged,
};
use serde::{Deserialize, Serialize};

#[cfg(feature = "native-process")]
use codec_registry::collect_doctor_report;
#[cfg(feature = "native-process")]
use optimization_engine::repair_file_to_staged;

#[cfg(feature = "native-process")]
use crate::NativeIsolatedProcessRunner;
use crate::{ActiveJobGuard, JobManager};

/// Structured output emitted by [`JobManager::repair_file`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct JobRepairResult {
    /// Contract schema version.
    pub schema_version: u32,
    /// Damaged source file path.
    pub input_path: String,
    /// Promoted repaired destination path.
    pub output_path: String,
    /// Full pre-repair diagnosis and post-repair telemetry.
    pub outcome: UnifiedRepairOutcome,
}

/// Structured output emitted by [`JobManager::sanitize_and_edit_metadata`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct JobMetadataResult {
    /// Contract schema version.
    pub schema_version: u32,
    /// Source file path.
    pub input_path: String,
    /// Promoted sanitized/edited destination path.
    pub output_path: String,
    /// Normalized metadata before/after audit and engine details.
    pub report: MetadataOperationReport,
}

/// Structured output emitted by [`JobManager`] `.ffenc` / PDF cryptographic operations.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct JobEncryptResult {
    /// Contract schema version.
    pub schema_version: u32,
    /// Source file path.
    pub input_path: String,
    /// Promoted encrypted/decrypted destination path.
    pub output_path: String,
    /// Promoted output byte length.
    pub output_bytes: u64,
    /// Algorithm label (`ffenc-aes-256-gcm`, `ffenc-chacha20-poly1305`, `pdf-aes-256`, etc.).
    pub algorithm_label: String,
}

impl JobManager {
    /// Diagnoses structural corruption and classifies recoverability honestly (`FR-REP-001`..`002`).
    ///
    /// # Errors
    /// Returns [`file_core::FileFusionError`] on missing inputs, traversal/bomb limits, or
    /// cooperative job cancellation.
    pub fn diagnose_repair(
        &self,
        input_path: &Path,
        ctx: &JobContext,
    ) -> FileFusionResult<UnifiedRepairDiagnosis> {
        let _active = ActiveJobGuard::register(
            &self.active_jobs,
            &ctx.job_id,
            ctx.cancellation.clone(),
            None,
        );
        diagnose_file_recoverability(input_path, ctx)
    }

    /// Executes cross-domain repair (`PDF`, `ZIP`, or media index) via isolated staging and
    /// promotes the repaired artifact atomically (`FR-REP-001`..`002`, `MOD-RULE-04`).
    ///
    /// # Errors
    /// Returns [`file_core::FileFusionError`] when unrecoverable, missing Tier 2 `FFmpeg` for media
    /// indexes, or cancelled.
    pub fn repair_file(
        &self,
        input_path: &Path,
        output_path: &Path,
        collision_policy: CollisionPolicy,
        ctx: &JobContext,
    ) -> FileFusionResult<JobRepairResult> {
        ctx.check_cancelled()?;
        let _active = ActiveJobGuard::register(
            &self.active_jobs,
            &ctx.job_id,
            ctx.cancellation.clone(),
            None,
        );
        let staged = StagedPath::create_sibling(output_path)?;
        let outcome = self.dispatch_repair(input_path, &staged, ctx)?;
        let promoted = staged.promote(collision_policy)?;
        Ok(JobRepairResult {
            schema_version: CORE_SCHEMA_VERSION,
            input_path: input_path.to_string_lossy().into_owned(),
            output_path: promoted.to_string_lossy().into_owned(),
            outcome,
        })
    }

    #[cfg(feature = "native-process")]
    fn dispatch_repair(
        &self,
        input_path: &Path,
        staged: &StagedPath,
        ctx: &JobContext,
    ) -> FileFusionResult<UnifiedRepairOutcome> {
        let report = collect_doctor_report(self.limits);
        let find_tool = |id: &str| {
            report
                .tool_availability
                .iter()
                .find(|t| t.tool_id == id && t.available)
                .and_then(|t| t.binary_path.as_ref().map(std::path::PathBuf::from))
        };
        let qpdf = find_tool("tier2-qpdf");
        let ffmpeg = find_tool("tier2-ffmpeg");
        let runner = NativeIsolatedProcessRunner::new();
        repair_file_to_staged(
            input_path,
            staged,
            &self.limits,
            qpdf.as_deref(),
            ffmpeg.as_deref(),
            Some(&runner),
            ctx,
        )
    }

    #[cfg(not(feature = "native-process"))]
    fn dispatch_repair(
        &self,
        input_path: &Path,
        staged: &StagedPath,
        ctx: &JobContext,
    ) -> FileFusionResult<UnifiedRepairOutcome> {
        repair_file_to_staged_tier1(input_path, staged, &self.limits, ctx)
    }

    /// Inspects unified normalized metadata and privacy risks for a local file (`FR-MET-001`).
    ///
    /// # Errors
    /// Returns [`file_core::FileFusionError`] on missing file, I/O error, or cancellation.
    pub fn inspect_metadata(
        &self,
        input_path: &Path,
        ctx: &JobContext,
    ) -> FileFusionResult<MetadataSummary> {
        let _active = ActiveJobGuard::register(
            &self.active_jobs,
            &ctx.job_id,
            ctx.cancellation.clone(),
            None,
        );
        inspect_unified_metadata(input_path, ctx)
    }

    /// Scrubs privacy tags and/or edits mutable metadata fields cleanly into a staged file
    /// promoted atomically (`FR-MET-002`).
    ///
    /// # Errors
    /// Returns [`file_core::FileFusionError`] on unsupported format, encrypted input, or cancellation.
    #[allow(clippy::too_many_arguments)]
    pub fn sanitize_and_edit_metadata(
        &self,
        input_path: &Path,
        output_path: &Path,
        scrub_privacy: bool,
        edits: Option<&EditableMetadataFields>,
        collision_policy: CollisionPolicy,
        ctx: &JobContext,
    ) -> FileFusionResult<JobMetadataResult> {
        ctx.check_cancelled()?;
        let _active = ActiveJobGuard::register(
            &self.active_jobs,
            &ctx.job_id,
            ctx.cancellation.clone(),
            None,
        );
        let staged = StagedPath::create_sibling(output_path)?;
        let report = sanitize_and_edit_metadata_to_staged(
            input_path,
            &staged,
            scrub_privacy,
            edits,
            &self.limits,
            ctx,
        )?;
        let promoted = staged.promote(collision_policy)?;
        Ok(JobMetadataResult {
            schema_version: CORE_SCHEMA_VERSION,
            input_path: input_path.to_string_lossy().into_owned(),
            output_path: promoted.to_string_lossy().into_owned(),
            report,
        })
    }

    /// Reads an `.ffenc` container header without decrypting plaintext (`FR-SEC-002`).
    ///
    /// # Errors
    /// Returns [`file_core::FileFusionError`] on invalid magic or malformed header.
    pub fn inspect_ffenc(
        &self,
        input_path: &Path,
        ctx: &JobContext,
    ) -> FileFusionResult<FfencHeaderInfo> {
        let _active = ActiveJobGuard::register(
            &self.active_jobs,
            &ctx.job_id,
            ctx.cancellation.clone(),
            None,
        );
        inspect_ffenc_file(input_path, ctx)
    }

    /// Encrypts a file into an authenticated `.ffenc` container promoted atomically (`FR-SEC-002`).
    ///
    /// # Errors
    /// Returns [`file_core::FileFusionError`] on empty passphrase, memory budget error, or cancellation.
    #[allow(clippy::too_many_arguments)]
    pub fn encrypt_ffenc(
        &self,
        input_path: &Path,
        output_path: &Path,
        passphrase: &[u8],
        suite: FfencCipherSuite,
        kdf: FfencKdfParams,
        collision_policy: CollisionPolicy,
        ctx: &JobContext,
    ) -> FileFusionResult<JobEncryptResult> {
        ctx.check_cancelled()?;
        let _active = ActiveJobGuard::register(
            &self.active_jobs,
            &ctx.job_id,
            ctx.cancellation.clone(),
            None,
        );
        let staged = StagedPath::create_sibling(output_path)?;
        let output_bytes =
            encrypt_file_to_ffenc_staged(input_path, &staged, passphrase, suite, kdf, ctx)?;
        let promoted = staged.promote(collision_policy)?;
        let algorithm_label = match suite {
            FfencCipherSuite::Aes256Gcm => String::from("ffenc-aes-256-gcm"),
            FfencCipherSuite::ChaCha20Poly1305 => String::from("ffenc-chacha20-poly1305"),
        };
        Ok(JobEncryptResult {
            schema_version: CORE_SCHEMA_VERSION,
            input_path: input_path.to_string_lossy().into_owned(),
            output_path: promoted.to_string_lossy().into_owned(),
            output_bytes,
            algorithm_label,
        })
    }

    /// Decrypts and authenticates an `.ffenc` container into a promoted output file (`FR-SEC-002`).
    ///
    /// # Errors
    /// Returns [`file_core::FileFusionError`] (`FmtDrm` on bad passphrase/tampering) or cancellation.
    pub fn decrypt_ffenc(
        &self,
        input_path: &Path,
        output_path: &Path,
        passphrase: &[u8],
        collision_policy: CollisionPolicy,
        ctx: &JobContext,
    ) -> FileFusionResult<JobEncryptResult> {
        ctx.check_cancelled()?;
        let _active = ActiveJobGuard::register(
            &self.active_jobs,
            &ctx.job_id,
            ctx.cancellation.clone(),
            None,
        );
        let staged = StagedPath::create_sibling(output_path)?;
        let output_bytes = decrypt_ffenc_file_to_staged(input_path, &staged, passphrase, ctx)?;
        let promoted = staged.promote(collision_policy)?;
        Ok(JobEncryptResult {
            schema_version: CORE_SCHEMA_VERSION,
            input_path: input_path.to_string_lossy().into_owned(),
            output_path: promoted.to_string_lossy().into_owned(),
            output_bytes,
            algorithm_label: String::from("ffenc-decrypt"),
        })
    }

    /// Applies PDF password encryption (`AES-256`/`AES-128`) and permissions atomically (`FR-PDF-004`).
    ///
    /// # Errors
    /// Returns [`file_core::FileFusionError`] on invalid options, already-encrypted PDF, or cancellation.
    pub fn encrypt_pdf(
        &self,
        input_path: &Path,
        output_path: &Path,
        opts: &PdfProtectionOptions,
        collision_policy: CollisionPolicy,
        ctx: &JobContext,
    ) -> FileFusionResult<JobEncryptResult> {
        ctx.check_cancelled()?;
        let _active = ActiveJobGuard::register(
            &self.active_jobs,
            &ctx.job_id,
            ctx.cancellation.clone(),
            None,
        );
        let staged = StagedPath::create_sibling(output_path)?;
        let output_bytes = encrypt_pdf_file_to_staged(input_path, &staged, opts, ctx)?;
        let promoted = staged.promote(collision_policy)?;
        let algorithm_label = match opts.algorithm {
            PdfEncryptionAlgorithm::Aes256 => String::from("pdf-aes-256"),
            PdfEncryptionAlgorithm::Aes128 => String::from("pdf-aes-128"),
        };
        Ok(JobEncryptResult {
            schema_version: CORE_SCHEMA_VERSION,
            input_path: input_path.to_string_lossy().into_owned(),
            output_path: promoted.to_string_lossy().into_owned(),
            output_bytes,
            algorithm_label,
        })
    }

    /// Unlocks an encrypted PDF using its user or owner password atomically (`FR-PDF-004`).
    ///
    /// # Errors
    /// Returns [`file_core::FileFusionError`] (`FmtDrm` on wrong password) or cancellation.
    pub fn decrypt_pdf(
        &self,
        input_path: &Path,
        output_path: &Path,
        password: &str,
        collision_policy: CollisionPolicy,
        ctx: &JobContext,
    ) -> FileFusionResult<JobEncryptResult> {
        ctx.check_cancelled()?;
        let _active = ActiveJobGuard::register(
            &self.active_jobs,
            &ctx.job_id,
            ctx.cancellation.clone(),
            None,
        );
        let staged = StagedPath::create_sibling(output_path)?;
        let output_bytes = decrypt_pdf_file_to_staged(input_path, &staged, password, ctx)?;
        let promoted = staged.promote(collision_policy)?;
        Ok(JobEncryptResult {
            schema_version: CORE_SCHEMA_VERSION,
            input_path: input_path.to_string_lossy().into_owned(),
            output_path: promoted.to_string_lossy().into_owned(),
            output_bytes,
            algorithm_label: String::from("pdf-decrypt"),
        })
    }
}
