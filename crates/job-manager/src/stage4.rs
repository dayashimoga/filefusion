//! L5 control-plane orchestration for Stage 4 unified file repair, privacy metadata
//! inspection/sanitization/editing, `.ffenc` container encryption, and PDF AES protection
//! (`FR-REP-001`..`002`, `FR-MET-001`..`002`, `FR-SEC-002`, `FR-PDF-004`).

use std::path::{Path, PathBuf};

use file_core::{
    CORE_SCHEMA_VERSION, CollisionPolicy, ErrorCode, FileFusionError, FileFusionResult, JobContext,
    MetadataSummary, StagedPath,
};
#[cfg(not(feature = "native-process"))]
use optimization_engine::repair_file_to_staged_tier1;
use optimization_engine::{
    EditableMetadataFields, FfencCipherSuite, FfencHeaderInfo, FfencKdfParams,
    MetadataOperationReport, OcrDocumentResult, PdfCropMargins, PdfEncryptionAlgorithm,
    PdfProtectionOptions, PdfScanInspection, PdfSecurityReport, PdfSignatureReport,
    PdfWatermarkOptions, UnifiedRepairDiagnosis, UnifiedRepairOutcome, apply_pdf_text_watermark,
    crop_pdf_pages, decrypt_ffenc_file_to_staged, decrypt_pdf_file_to_staged,
    diagnose_file_recoverability, encrypt_file_to_ffenc_staged, encrypt_pdf_file_to_staged,
    extract_or_reorder_pdf_pages, inspect_ffenc_file, inspect_pdf_health, inspect_pdf_scan_status,
    inspect_pdf_security, inspect_unified_metadata, merge_pdfs, parse_page_range_spec,
    rotate_pdf_pages, sanitize_and_edit_metadata_to_staged, split_pdf_by_ranges,
    split_pdf_every_n_pages, verify_pdf_signatures,
};
use serde::{Deserialize, Serialize};

#[cfg(feature = "native-process")]
use codec_registry::collect_doctor_report;
#[cfg(feature = "native-process")]
use optimization_engine::{OcrOptions, TesseractAdapter, repair_file_to_staged};

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

/// Structured output emitted by [`JobManager`] PDF Studio structural operations (`FR-PDF-001`, `FR-PDF-004`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct JobPdfResult {
    /// Contract schema version.
    pub schema_version: u32,
    /// Studio operation identifier (`merge`, `split`, `reorder`, `rotate`, `crop`, `watermark`).
    pub operation: String,
    /// Promoted destination file path(s).
    pub output_paths: Vec<String>,
    /// Total output pages across produced artifact(s).
    pub page_count: u32,
    /// Combined byte count written across produced PDF artifact(s).
    pub output_bytes: u64,
}

/// Structured output emitted by [`JobManager::pdf_ocr`] (`FR-PDF-003`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct JobPdfOcrResult {
    /// Contract schema version.
    pub schema_version: u32,
    /// Input PDF path.
    pub input_path: String,
    /// Promoted searchable PDF destination path.
    pub output_path: String,
    /// Output byte count.
    pub output_bytes: u64,
    /// Multi-page OCR recognition telemetry and extracted text.
    pub ocr: OcrDocumentResult,
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

    /// Merges multiple unencrypted PDF documents cleanly in input order (`FR-PDF-001`).
    ///
    /// # Errors
    /// Returns [`file_core::FileFusionError`] on empty input list, corrupt/encrypted PDF, or cancellation.
    pub fn pdf_merge(
        &self,
        inputs: &[PathBuf],
        output_path: &Path,
        collision_policy: CollisionPolicy,
        ctx: &JobContext,
    ) -> FileFusionResult<JobPdfResult> {
        ctx.check_cancelled()?;
        let _active = ActiveJobGuard::register(
            &self.active_jobs,
            &ctx.job_id,
            ctx.cancellation.clone(),
            None,
        );
        if inputs.is_empty() {
            return Err(FileFusionError::new(
                ErrorCode::FmtUnsupported,
                "at least one input PDF is required for merge",
            ));
        }
        let mut buffers = Vec::with_capacity(inputs.len());
        for p in inputs {
            ctx.check_cancelled()?;
            buffers.push(std::fs::read(p)?);
        }
        let refs: Vec<&[u8]> = buffers.iter().map(Vec::as_slice).collect();
        let merged = merge_pdfs(&refs, ctx)?;
        write_staged_pdf_result("merge", &merged, output_path, collision_policy, ctx)
    }

    /// Splits a PDF by explicit page range groups (e.g. `"1-2;3-4"`) or fixed `every_n` chunk size
    /// into clean sibling PDFs inside `output_dir` (`FR-PDF-001`).
    ///
    /// # Errors
    /// Returns [`file_core::FileFusionError`] on invalid range spec, corrupt/encrypted PDF, or cancellation.
    #[allow(clippy::too_many_arguments)]
    pub fn pdf_split(
        &self,
        input_path: &Path,
        output_dir: &Path,
        every_n: Option<u32>,
        ranges: Option<&str>,
        collision_policy: CollisionPolicy,
        ctx: &JobContext,
    ) -> FileFusionResult<JobPdfResult> {
        ctx.check_cancelled()?;
        let _active = ActiveJobGuard::register(
            &self.active_jobs,
            &ctx.job_id,
            ctx.cancellation.clone(),
            None,
        );
        let raw = std::fs::read(input_path)?;
        let chunks: Vec<Vec<u8>> = if let Some(spec) = ranges {
            let groups: Vec<&str> = spec
                .split(';')
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .collect();
            split_pdf_by_ranges(&raw, &groups, ctx)?
        } else {
            split_pdf_every_n_pages(&raw, every_n.unwrap_or(1), ctx)?
        };
        if !output_dir.exists() {
            std::fs::create_dir_all(output_dir)?;
        }
        let stem = input_path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("split");

        let mut output_paths = Vec::with_capacity(chunks.len());
        let mut page_count = 0u32;
        let mut output_bytes = 0u64;
        for (idx, chunk) in chunks.iter().enumerate() {
            ctx.check_cancelled()?;
            let dest = output_dir.join(format!("{stem}_part{}.pdf", idx + 1));
            let staged = StagedPath::create_sibling(&dest)?;
            std::fs::write(staged.staged_path(), chunk)?;
            let promoted = staged.promote(collision_policy)?;
            page_count = page_count.saturating_add(inspect_pdf_health(chunk).page_count);
            output_bytes = output_bytes.saturating_add(u64::try_from(chunk.len()).unwrap_or(0));
            output_paths.push(promoted.to_string_lossy().into_owned());
        }
        Ok(JobPdfResult {
            schema_version: CORE_SCHEMA_VERSION,
            operation: String::from("split"),
            output_paths,
            page_count,
            output_bytes,
        })
    }

    /// Extracts and/or reorders explicit 1-based pages (e.g. `"3,1-2"`) into a new PDF (`FR-PDF-001`).
    ///
    /// # Errors
    /// Returns [`file_core::FileFusionError`] on invalid range spec, corrupt/encrypted PDF, or cancellation.
    pub fn pdf_reorder(
        &self,
        input_path: &Path,
        pages_spec: &str,
        output_path: &Path,
        collision_policy: CollisionPolicy,
        ctx: &JobContext,
    ) -> FileFusionResult<JobPdfResult> {
        ctx.check_cancelled()?;
        let _active = ActiveJobGuard::register(
            &self.active_jobs,
            &ctx.job_id,
            ctx.cancellation.clone(),
            None,
        );
        let raw = std::fs::read(input_path)?;
        let order = resolve_pdf_page_selection(&raw, Some(pages_spec))?.unwrap_or_default();
        let reordered = extract_or_reorder_pdf_pages(&raw, &order, ctx)?;
        write_staged_pdf_result("reorder", &reordered, output_path, collision_policy, ctx)
    }

    /// Rotates all or selected PDF pages by `degrees` (`90`° increments) atomically (`FR-PDF-001`).
    ///
    /// # Errors
    /// Returns [`file_core::FileFusionError`] on non-90° rotation, corrupt/encrypted PDF, or cancellation.
    pub fn pdf_rotate(
        &self,
        input_path: &Path,
        pages_spec: Option<&str>,
        degrees: i32,
        output_path: &Path,
        collision_policy: CollisionPolicy,
        ctx: &JobContext,
    ) -> FileFusionResult<JobPdfResult> {
        ctx.check_cancelled()?;
        let _active = ActiveJobGuard::register(
            &self.active_jobs,
            &ctx.job_id,
            ctx.cancellation.clone(),
            None,
        );
        let raw = std::fs::read(input_path)?;
        let pages = resolve_pdf_page_selection(&raw, pages_spec)?;
        let rotated = rotate_pdf_pages(&raw, pages.as_deref(), degrees, ctx)?;
        write_staged_pdf_result("rotate", &rotated, output_path, collision_policy, ctx)
    }

    /// Trims PDF `/CropBox` boundaries on all or selected pages cleanly (`FR-PDF-001`).
    ///
    /// # Errors
    /// Returns [`file_core::FileFusionError`] on invalid margins, corrupt/encrypted PDF, or cancellation.
    pub fn pdf_crop(
        &self,
        input_path: &Path,
        pages_spec: Option<&str>,
        margins: PdfCropMargins,
        output_path: &Path,
        collision_policy: CollisionPolicy,
        ctx: &JobContext,
    ) -> FileFusionResult<JobPdfResult> {
        ctx.check_cancelled()?;
        let _active = ActiveJobGuard::register(
            &self.active_jobs,
            &ctx.job_id,
            ctx.cancellation.clone(),
            None,
        );
        let raw = std::fs::read(input_path)?;
        let pages = resolve_pdf_page_selection(&raw, pages_spec)?;
        let cropped = crop_pdf_pages(&raw, pages.as_deref(), margins, ctx)?;
        write_staged_pdf_result("crop", &cropped, output_path, collision_policy, ctx)
    }

    /// Applies a transparent vector text watermark across PDF pages (`FR-PDF-004`).
    ///
    /// # Errors
    /// Returns [`file_core::FileFusionError`] on invalid options, invalid page spec,
    /// corrupt/encrypted PDF, or cancellation.
    #[allow(clippy::too_many_arguments)]
    pub fn pdf_watermark(
        &self,
        input_path: &Path,
        pages_spec: Option<&str>,
        opts: &PdfWatermarkOptions,
        output_path: &Path,
        collision_policy: CollisionPolicy,
        ctx: &JobContext,
    ) -> FileFusionResult<JobPdfResult> {
        ctx.check_cancelled()?;
        let _active = ActiveJobGuard::register(
            &self.active_jobs,
            &ctx.job_id,
            ctx.cancellation.clone(),
            None,
        );
        let raw = std::fs::read(input_path)?;
        let mut resolved_opts = opts.clone();
        if let Some(pages) = resolve_pdf_page_selection(&raw, pages_spec)? {
            resolved_opts.pages = Some(pages);
        }
        let watermarked = apply_pdf_text_watermark(&raw, &resolved_opts, ctx)?;
        write_staged_pdf_result(
            "watermark",
            &watermarked,
            output_path,
            collision_policy,
            ctx,
        )
    }

    /// Inspects a PDF to detect image-only scanned pages requiring local OCR (`FR-PDF-003`).
    ///
    /// # Errors
    /// Returns [`file_core::FileFusionError`] on unreadable/corrupt PDF or cancellation.
    pub fn pdf_scan_check(
        &self,
        input_path: &Path,
        ctx: &JobContext,
    ) -> FileFusionResult<PdfScanInspection> {
        ctx.check_cancelled()?;
        let _active = ActiveJobGuard::register(
            &self.active_jobs,
            &ctx.job_id,
            ctx.cancellation.clone(),
            None,
        );
        let raw = std::fs::read(input_path)?;
        inspect_pdf_scan_status(&raw)
    }

    /// Verifies `/Sig` `/ByteRange` integrity and ASN.1 DER `/Contents` framing (`FR-PDF-004`).
    ///
    /// # Errors
    /// Returns [`file_core::FileFusionError`] on corrupt PDF or cancellation.
    pub fn pdf_verify_signatures(
        &self,
        input_path: &Path,
        ctx: &JobContext,
    ) -> FileFusionResult<PdfSignatureReport> {
        let _active = ActiveJobGuard::register(
            &self.active_jobs,
            &ctx.job_id,
            ctx.cancellation.clone(),
            None,
        );
        let raw = std::fs::read(input_path)?;
        verify_pdf_signatures(&raw, ctx)
    }

    /// Inspects `/Encrypt` cipher strength, `/P` permission bits, and optional password (`FR-PDF-004`).
    ///
    /// # Errors
    /// Returns [`file_core::FileFusionError`] on corrupt PDF or cancellation.
    pub fn pdf_inspect_security(
        &self,
        input_path: &Path,
        password: Option<&str>,
        ctx: &JobContext,
    ) -> FileFusionResult<PdfSecurityReport> {
        ctx.check_cancelled()?;
        let _active = ActiveJobGuard::register(
            &self.active_jobs,
            &ctx.job_id,
            ctx.cancellation.clone(),
            None,
        );
        let raw = std::fs::read(input_path)?;
        inspect_pdf_security(&raw, password)
    }

    /// Runs local `Tesseract` OCR via [`NativeIsolatedProcessRunner`] to embed invisible searchable
    /// text (`3 Tr`) onto scanned PDF pages (`FR-PDF-003`, `MOD-RULE-04`).
    ///
    /// # Errors
    /// Returns [`file_core::FileFusionError`] (`EngMissing` when `tesseract` binary is absent) or
    /// subprocess/corrupt-input failure.
    #[cfg(feature = "native-process")]
    pub fn pdf_ocr(
        &self,
        input_path: &Path,
        output_path: &Path,
        opts: &OcrOptions,
        collision_policy: CollisionPolicy,
        ctx: &JobContext,
    ) -> FileFusionResult<JobPdfOcrResult> {
        ctx.check_cancelled()?;
        let _active = ActiveJobGuard::register(
            &self.active_jobs,
            &ctx.job_id,
            ctx.cancellation.clone(),
            None,
        );
        let doctor = collect_doctor_report(self.limits);
        let tesseract_bin = doctor
            .tool_availability
            .iter()
            .find(|t| t.tool_id == "tier2-tesseract" && t.available)
            .and_then(|t| t.binary_path.as_ref().map(PathBuf::from))
            .ok_or_else(|| {
                FileFusionError::new(
                    ErrorCode::EngMissing,
                    "Tier 2 Tesseract binary (`tesseract`) was not found on PATH",
                )
                .with_remediation(
                    "Install Tesseract OCR and ensure `tesseract` is executable on PATH.",
                )
            })?;

        let raw = std::fs::read(input_path)?;
        let adapter = TesseractAdapter::new(tesseract_bin);
        let runner = NativeIsolatedProcessRunner::new();
        let (searchable_pdf, ocr) =
            adapter.make_pdf_searchable_with_runner(&raw, opts, &runner, ctx)?;
        let staged = StagedPath::create_sibling(output_path)?;
        std::fs::write(staged.staged_path(), &searchable_pdf)?;
        let promoted = staged.promote(collision_policy)?;

        Ok(JobPdfOcrResult {
            schema_version: CORE_SCHEMA_VERSION,
            input_path: input_path.to_string_lossy().into_owned(),
            output_path: promoted.to_string_lossy().into_owned(),
            output_bytes: u64::try_from(searchable_pdf.len()).unwrap_or(u64::MAX),
            ocr,
        })
    }
}

fn write_staged_pdf_result(
    operation: &str,
    bytes: &[u8],
    output_path: &Path,
    collision_policy: CollisionPolicy,
    ctx: &JobContext,
) -> FileFusionResult<JobPdfResult> {
    ctx.check_cancelled()?;
    let staged = StagedPath::create_sibling(output_path)?;
    std::fs::write(staged.staged_path(), bytes)?;
    let promoted = staged.promote(collision_policy)?;
    let page_count = inspect_pdf_health(bytes).page_count;
    Ok(JobPdfResult {
        schema_version: CORE_SCHEMA_VERSION,
        operation: String::from(operation),
        output_paths: vec![promoted.to_string_lossy().into_owned()],
        page_count,
        output_bytes: u64::try_from(bytes.len()).unwrap_or(u64::MAX),
    })
}

fn resolve_pdf_page_selection(
    raw: &[u8],
    pages_spec: Option<&str>,
) -> FileFusionResult<Option<Vec<u32>>> {
    let Some(spec) = pages_spec else {
        return Ok(None);
    };
    let total = inspect_pdf_health(raw).page_count.max(1);
    parse_page_range_spec(spec, total).map(Some)
}
