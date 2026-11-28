//! Unified Stage 4 File Repair & Recoverability Diagnostics (`FR-REP-001`..`002`)
//! spanning `PDF`, `ZIP`, and `MP4`/`MOV`/`MKV`/`WebM` container indexes (`FF-MOD-001` §4).

use std::fs;
use std::path::Path;

use compression_archive::{ZipHealthReport, inspect_zip_health, repair_truncated_zip};
use compression_pdf::{PdfHealthReport, PdfRepairAssessment, inspect_pdf_health, repair_pdf_bytes};
use compression_video::diagnose_unfinalized_video_container;
use file_core::{
    ErrorCode, FileFusionError, FileFusionResult, FormatId, JobContext, ResourceLimits, StagedPath,
    inspect_path,
};
use serde::{Deserialize, Serialize};

#[cfg(feature = "native-process")]
use compression_pdf::repair_pdf_to_staged_with_runner;
#[cfg(feature = "native-process")]
use compression_video::build_ffmpeg_container_repair_invocation;
#[cfg(feature = "native-process")]
use file_core::IsolatedProcessRunner;

/// Normalized cross-domain recoverability classification (`FR-REP-002`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RecoverabilityClass {
    /// Structure and index tables verified healthy; no repair required.
    Healthy,
    /// Index/trailer/directory damaged, while intact payloads permit full structural recovery.
    FullyRepairable,
    /// Partial payload or frame/entry recovery possible with itemized loss summary.
    PartiallySalvageable,
    /// Signature missing or insufficient salvageable entropy to rebuild a valid file.
    Unrecoverable,
}

/// Cross-domain diagnostic snapshot returned by [`diagnose_file_recoverability`] (`FR-REP-002`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UnifiedRepairDiagnosis {
    /// Format resolved from magic bytes and container structure.
    pub detected_format: FormatId,
    /// Honest recoverability classification (`Healthy`..`Unrecoverable`).
    pub recoverability: RecoverabilityClass,
    /// Concise human-readable explanation of structural damage or health.
    pub issue_summary: String,
    /// Primary repair backend suited to recover this file.
    pub recommended_engine: String,
    /// Detailed PDF structural health snapshot when `detected_format == FormatId::Pdf`.
    pub pdf_health: Option<PdfHealthReport>,
    /// Detailed ZIP central-directory/LFH health snapshot when `detected_format == FormatId::Zip`.
    pub zip_health: Option<ZipHealthReport>,
    /// Media container `moov`/`EBML` diagnostic note for `MP4`/`MOV`/`MKV`/`WebM`.
    pub media_index_issue: Option<String>,
}

/// Outcome returned after executing container or cross-reference recovery into staged storage.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UnifiedRepairOutcome {
    /// Format repaired.
    pub detected_format: FormatId,
    /// Diagnostic assessment captured before repair ran.
    pub before_diagnosis: UnifiedRepairDiagnosis,
    /// Verified repaired byte length written to staged output.
    pub repaired_bytes: u64,
    /// Engine identifier executed (`tier1-pdf-repair`, `tier2-qpdf-repair`, `tier1-zip-repair`, etc.).
    pub engine_used: String,
    /// Count of logical entries/pages recovered.
    pub items_recovered: u32,
    /// Count of damaged entries skipped (`0` for clean full recoveries).
    pub items_skipped: u32,
    /// Explicit loss note when `items_skipped > 0` or `PartiallySalvageable`.
    pub partial_loss_summary: Option<String>,
}

/// Inspects a possibly corrupted `PDF`, `ZIP`, or `MP4`/`MOV`/`MKV`/`WebM` file on disk and classifies
/// recoverability honestly without mutating the source file (`FR-REP-001`..`002`).
///
/// # Errors
/// Returns [`FileFusionError`] on missing files, security/bomb bounds on ZIP scanning, or cancellation.
pub fn diagnose_file_recoverability(
    path: &Path,
    ctx: &JobContext,
) -> FileFusionResult<UnifiedRepairDiagnosis> {
    ctx.check_cancelled()?;
    if !path.is_file() {
        return Err(FileFusionError::new(
            ErrorCode::IoNotFound,
            format!("repair source does not exist: `{}`", path.display()),
        ));
    }

    let report = inspect_path(path, false)?;
    let raw = fs::read(path)?;
    let fmt = resolve_repair_format(report.detected_format, &raw);

    match fmt {
        FormatId::Pdf => Ok(classify_pdf_diagnosis(&raw)),
        FormatId::Zip => classify_zip_diagnosis(&raw, &ctx.limits, ctx),
        FormatId::Mp4 | FormatId::Mov | FormatId::Mkv | FormatId::Webm => {
            classify_media_diagnosis(path, fmt)
        }
        other => Ok(UnifiedRepairDiagnosis {
            detected_format: other,
            recoverability: RecoverabilityClass::Unrecoverable,
            issue_summary: format!(
                "No container/index repair backend applies to detected format {other:?}."
            ),
            recommended_engine: String::from("none"),
            pdf_health: None,
            zip_health: None,
            media_index_issue: None,
        }),
    }
}

/// Executes pure-Rust Tier 1 recovery (`PDF` cross-reference rebuild or `ZIP` central-directory
/// reconstruction) into `staged_output` (`FR-REP-001`..`002`, `Native` + `wasm32`).
///
/// # Errors
/// Returns [`FileFusionError`] if the file is unrecoverable (`FmtCorrupt`), requires Tier 2 `FFmpeg`
/// media remuxing (`EngMissing`), or is cancelled.
pub fn repair_file_to_staged_tier1(
    input_path: &Path,
    staged_output: &StagedPath,
    limits: &ResourceLimits,
    ctx: &JobContext,
) -> FileFusionResult<UnifiedRepairOutcome> {
    ctx.check_cancelled()?;
    let diagnosis = diagnose_file_recoverability(input_path, ctx)?;
    ensure_repairable(&diagnosis)?;

    match diagnosis.detected_format {
        FormatId::Pdf => {
            let raw = fs::read(input_path)?;
            let (repaired, post) = repair_pdf_bytes(&raw, ctx)?;
            fs::write(staged_output.staged_path(), &repaired)?;
            Ok(build_pdf_outcome(
                diagnosis,
                u64::try_from(repaired.len()).unwrap_or(u64::MAX),
                &post,
                "tier1-pdf-repair",
            ))
        }
        FormatId::Zip => repair_zip_into_staged(input_path, staged_output, limits, diagnosis, ctx),
        FormatId::Mp4 | FormatId::Mov | FormatId::Mkv | FormatId::Webm => {
            Err(FileFusionError::new(
                ErrorCode::EngMissing,
                "Video container index repair requires Tier 2 FFmpeg via IsolatedProcessRunner",
            ))
        }
        other => Err(FileFusionError::new(
            ErrorCode::FmtUnsupported,
            format!("no repair backend available for {other:?}"),
        )),
    }
}

/// Executes unified file repair across `PDF` (Tier 2 `qpdf` or pure-Rust fallback), `ZIP`
/// (pure-Rust Local File Header scanner), and `MP4`/`MOV`/`MKV`/`WebM` (`FFmpeg` copy/faststart
/// remux via [`IsolatedProcessRunner`]) into `staged_output` (`FR-REP-001`..`002`, `MOD-RULE-04`).
///
/// # Errors
/// Returns [`FileFusionError`] when unrecoverable (`FmtCorrupt`), missing Tier 2 `FFmpeg`
/// (`EngMissing`), subprocess fails, or job is cancelled.
#[cfg(feature = "native-process")]
pub fn repair_file_to_staged(
    input_path: &Path,
    staged_output: &StagedPath,
    limits: &ResourceLimits,
    qpdf_binary: Option<&Path>,
    ffmpeg_binary: Option<&Path>,
    runner: Option<&dyn IsolatedProcessRunner>,
    ctx: &JobContext,
) -> FileFusionResult<UnifiedRepairOutcome> {
    ctx.check_cancelled()?;
    let diagnosis = diagnose_file_recoverability(input_path, ctx)?;
    ensure_repairable(&diagnosis)?;

    match diagnosis.detected_format {
        FormatId::Pdf => {
            let (post, engine) = repair_pdf_to_staged_with_runner(
                input_path,
                staged_output,
                qpdf_binary,
                runner,
                ctx,
            )?;
            let repaired_bytes = fs::metadata(staged_output.staged_path())?.len();
            Ok(build_pdf_outcome(diagnosis, repaired_bytes, &post, engine))
        }
        FormatId::Zip => repair_zip_into_staged(input_path, staged_output, limits, diagnosis, ctx),
        fmt @ (FormatId::Mp4 | FormatId::Mov | FormatId::Mkv | FormatId::Webm) => {
            let Some(ffmpeg) = ffmpeg_binary else {
                return Err(FileFusionError::new(
                    ErrorCode::EngMissing,
                    "FFmpeg binary required to rebuild unfinalized video container indexes",
                ));
            };
            let Some(exec) = runner else {
                return Err(FileFusionError::new(
                    ErrorCode::EngMissing,
                    "IsolatedProcessRunner required to execute FFmpeg container repair",
                ));
            };
            let inv = build_ffmpeg_container_repair_invocation(
                ffmpeg,
                input_path,
                staged_output.staged_path(),
                fmt,
                ctx.limits.subprocess_timeout_secs,
            )?;
            let out = exec.run_isolated(&inv, ctx)?;
            if out.timed_out || out.exit_code != 0 {
                return Err(FileFusionError::new(
                    ErrorCode::FmtCorrupt,
                    format!(
                        "FFmpeg container repair failed (exit={}, timed_out={}): {}",
                        out.exit_code,
                        out.timed_out,
                        String::from_utf8_lossy(&out.stderr).trim()
                    ),
                ));
            }
            let repaired_bytes = fs::metadata(staged_output.staged_path())?.len();
            Ok(UnifiedRepairOutcome {
                detected_format: fmt,
                before_diagnosis: diagnosis,
                repaired_bytes,
                engine_used: String::from("tier2-ffmpeg-remux"),
                items_recovered: 1,
                items_skipped: 0,
                partial_loss_summary: None,
            })
        }
        other => Err(FileFusionError::new(
            ErrorCode::FmtUnsupported,
            format!("no repair backend available for {other:?}"),
        )),
    }
}

fn ensure_repairable(diagnosis: &UnifiedRepairDiagnosis) -> FileFusionResult<()> {
    if diagnosis.recoverability == RecoverabilityClass::Unrecoverable {
        return Err(FileFusionError::new(
            ErrorCode::FmtCorrupt,
            diagnosis.issue_summary.clone(),
        ));
    }
    Ok(())
}

fn repair_zip_into_staged(
    input_path: &Path,
    staged_output: &StagedPath,
    limits: &ResourceLimits,
    diagnosis: UnifiedRepairDiagnosis,
    ctx: &JobContext,
) -> FileFusionResult<UnifiedRepairOutcome> {
    let rep = repair_truncated_zip(input_path, staged_output.staged_path(), limits, ctx)?;
    let repaired_bytes = fs::metadata(staged_output.staged_path())?.len();
    let recovered = u32::try_from(rep.entries_recovered).unwrap_or(u32::MAX);
    let skipped = u32::try_from(rep.entries_skipped_corrupt).unwrap_or(u32::MAX);
    let partial_loss_summary = (skipped > 0).then(|| {
        format!(
            "Recovered {recovered} intact member(s); skipped {skipped} damaged/CRC-invalid record(s)."
        )
    });
    Ok(UnifiedRepairOutcome {
        detected_format: FormatId::Zip,
        before_diagnosis: diagnosis,
        repaired_bytes,
        engine_used: String::from("tier1-zip-repair"),
        items_recovered: recovered,
        items_skipped: skipped,
        partial_loss_summary,
    })
}

fn build_pdf_outcome(
    diagnosis: UnifiedRepairDiagnosis,
    repaired_bytes: u64,
    post: &PdfHealthReport,
    engine: &str,
) -> UnifiedRepairOutcome {
    UnifiedRepairOutcome {
        detected_format: FormatId::Pdf,
        before_diagnosis: diagnosis,
        repaired_bytes,
        engine_used: String::from(engine),
        items_recovered: post.page_count.max(1),
        items_skipped: 0,
        partial_loss_summary: None,
    }
}

fn resolve_repair_format(detected: FormatId, raw: &[u8]) -> FormatId {
    if detected != FormatId::Unknown {
        return detected;
    }
    if raw[..raw.len().min(1024)].windows(5).any(|w| w == b"%PDF-") {
        return FormatId::Pdf;
    }
    if raw.starts_with(b"PK\x03\x04") {
        return FormatId::Zip;
    }
    if raw.len() >= 8 && &raw[4..8] == b"ftyp" {
        return FormatId::Mp4;
    }
    if raw.starts_with(&[0x1A, 0x45, 0xDF, 0xA3]) {
        return FormatId::Mkv;
    }
    FormatId::Unknown
}

fn classify_pdf_diagnosis(raw: &[u8]) -> UnifiedRepairDiagnosis {
    let report = inspect_pdf_health(raw);
    let recoverability = match report.assessment {
        PdfRepairAssessment::Healthy => RecoverabilityClass::Healthy,
        PdfRepairAssessment::FullyRepairable => RecoverabilityClass::FullyRepairable,
        PdfRepairAssessment::PartiallySalvageable => RecoverabilityClass::PartiallySalvageable,
        PdfRepairAssessment::Unrecoverable => RecoverabilityClass::Unrecoverable,
    };
    UnifiedRepairDiagnosis {
        detected_format: FormatId::Pdf,
        recoverability,
        issue_summary: report.diagnostic.clone(),
        recommended_engine: String::from("tier1-pdf-repair"),
        pdf_health: Some(report),
        zip_health: None,
        media_index_issue: None,
    }
}

fn classify_zip_diagnosis(
    raw: &[u8],
    limits: &ResourceLimits,
    ctx: &JobContext,
) -> FileFusionResult<UnifiedRepairDiagnosis> {
    let zip = inspect_zip_health(raw, limits, ctx)?;
    let (recoverability, issue_summary) = if !zip.has_lfh_signature || zip.recoverable_entries == 0
    {
        (
            RecoverabilityClass::Unrecoverable,
            String::from("ZIP archive contains zero valid Local File Header records."),
        )
    } else if !zip.has_eocd && zip.corrupt_entries == 0 {
        (
            RecoverabilityClass::FullyRepairable,
            format!(
                "Missing ZIP Central Directory / EOCD; all {} local file entry(s) intact and recoverable.",
                zip.recoverable_entries
            ),
        )
    } else if zip.corrupt_entries > 0 {
        (
            RecoverabilityClass::PartiallySalvageable,
            format!(
                "{} intact entry(s) recoverable; {} damaged record(s) will be skipped.",
                zip.recoverable_entries, zip.corrupt_entries
            ),
        )
    } else {
        (
            RecoverabilityClass::Healthy,
            format!(
                "Healthy ZIP directory structure ({} recoverable entry(s)).",
                zip.recoverable_entries
            ),
        )
    };
    Ok(UnifiedRepairDiagnosis {
        detected_format: FormatId::Zip,
        recoverability,
        issue_summary,
        recommended_engine: String::from("tier1-zip-repair"),
        pdf_health: None,
        zip_health: Some(zip),
        media_index_issue: None,
    })
}

fn classify_media_diagnosis(
    path: &Path,
    fmt: FormatId,
) -> FileFusionResult<UnifiedRepairDiagnosis> {
    let issue = diagnose_unfinalized_video_container(path, fmt)?;
    match issue {
        None => Ok(UnifiedRepairDiagnosis {
            detected_format: fmt,
            recoverability: RecoverabilityClass::Healthy,
            issue_summary: format!("Healthy {fmt:?} container headers and seek index."),
            recommended_engine: String::from("tier2-ffmpeg-remux"),
            pdf_health: None,
            zip_health: None,
            media_index_issue: None,
        }),
        Some(msg) => {
            let unrecoverable = msg.contains("< 16 bytes")
                || msg.contains("missing mandatory")
                || msg.contains("missing EBML");
            Ok(UnifiedRepairDiagnosis {
                detected_format: fmt,
                recoverability: if unrecoverable {
                    RecoverabilityClass::Unrecoverable
                } else {
                    RecoverabilityClass::FullyRepairable
                },
                issue_summary: msg.clone(),
                recommended_engine: String::from("tier2-ffmpeg-remux"),
                pdf_health: None,
                zip_health: None,
                media_index_issue: Some(msg),
            })
        }
    }
}
