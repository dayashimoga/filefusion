//! Structural PDF integrity assessment (`FR-REP-001`..`002`) and Tier 2 `qpdf`
//! linearization / recovery command construction (`MOD-RULE-04`, `MOD-RULE-05`).

use file_core::{ErrorCode, FileFusionError, FileFusionResult};
use lopdf::Document;
use serde::{Deserialize, Serialize};

#[cfg(feature = "native-process")]
use std::ffi::OsString;
#[cfg(feature = "native-process")]
use std::path::Path;

#[cfg(feature = "native-process")]
use file_core::ProcessInvocation;

/// Honest recoverability classification aligned with `FR-REP-002`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PdfRepairAssessment {
    /// Document parsed cleanly; no repair required or straightforward structural clean-up.
    Healthy,
    /// Header and/or object streams intact; `xref`/`%%EOF` recovery via `qpdf`/re-save succeeds.
    FullyRepairable,
    /// Core objects readable with minor corruption; partial page/stream salvage expected.
    PartiallySalvageable,
    /// Missing `%PDF-` signature or insufficient recognizable PDF structure.
    Unrecoverable,
}

/// Structural health snapshot produced by [`inspect_pdf_health`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[allow(clippy::struct_excessive_bools)]
pub struct PdfHealthReport {
    /// True if `%PDF-` header signature was located within the first 1024 bytes.
    pub has_pdf_header: bool,
    /// True if `%%EOF` trailer terminator was detected in trailing bytes.
    pub has_eof_marker: bool,
    /// True if `startxref` table pointer was detected.
    pub has_startxref: bool,
    /// True if `/Encrypt` trailer handler was found.
    pub is_encrypted: bool,
    /// Page count discovered by pure-Rust `lopdf` parser (`0` if unparseable).
    pub page_count: u32,
    /// Total indirect object count parsed.
    pub object_count: u32,
    /// Honest recoverability verdict (`FR-REP-002`).
    pub assessment: PdfRepairAssessment,
    /// Actionable human-readable diagnostic summary.
    pub diagnostic: String,
}

/// Operating mode for Tier 2 `qpdf` subprocess invocations.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum QpdfMode {
    /// Compress object/content streams and linearize for Fast Web View (`FR-PDF-001`).
    OptimizeLinearize,
    /// Structural `xref`/stream recovery without dropping salvageable pages (`FR-REP-001`).
    StructuralRepair,
}

/// Inspects raw PDF bytes for structural integrity, `xref`/`%%EOF` health, encryption status,
/// and recoverability (`FR-REP-001`, `FR-REP-002`).
#[must_use]
pub fn inspect_pdf_health(raw: &[u8]) -> PdfHealthReport {
    let head = &raw[..raw.len().min(1024)];
    let tail_start = raw.len().saturating_sub(2048);
    let tail = &raw[tail_start..];

    let has_pdf_header = head.windows(5).any(|w| w == b"%PDF-");
    let has_eof_marker = tail.windows(5).any(|w| w == b"%%EOF");
    let has_startxref = tail.windows(9).any(|w| w == b"startxref");

    if !has_pdf_header {
        return PdfHealthReport {
            has_pdf_header: false,
            has_eof_marker,
            has_startxref,
            is_encrypted: false,
            page_count: 0,
            object_count: 0,
            assessment: PdfRepairAssessment::Unrecoverable,
            diagnostic: String::from(
                "Missing mandatory `%PDF-` header signature within leading 1024 bytes.",
            ),
        };
    }

    match Document::load_mem(raw) {
        Ok(doc) => {
            let is_encrypted = doc.is_encrypted();
            let page_count = u32::try_from(doc.get_pages().len()).unwrap_or(u32::MAX);
            let object_count = u32::try_from(doc.objects.len()).unwrap_or(u32::MAX);
            if has_eof_marker && has_startxref {
                PdfHealthReport {
                    has_pdf_header,
                    has_eof_marker,
                    has_startxref,
                    is_encrypted,
                    page_count,
                    object_count,
                    assessment: PdfRepairAssessment::Healthy,
                    diagnostic: format!(
                        "Healthy PDF structure ({page_count} page(s), {object_count} object(s))."
                    ),
                }
            } else {
                PdfHealthReport {
                    has_pdf_header,
                    has_eof_marker,
                    has_startxref,
                    is_encrypted,
                    page_count,
                    object_count,
                    assessment: PdfRepairAssessment::FullyRepairable,
                    diagnostic: String::from(
                        "PDF trailer markers (`startxref`/`%%EOF`) incomplete; object graph is intact and rebuildable.",
                    ),
                }
            }
        }
        Err(err) => {
            let has_any_obj = raw.windows(4).any(|w| w == b" obj");
            let has_endobj = raw.windows(6).any(|w| w == b"endobj");
            let has_root_ref = raw.windows(5).any(|w| w == b"/Root");
            let assessment = if has_any_obj && has_endobj && has_root_ref && !has_eof_marker {
                PdfRepairAssessment::FullyRepairable
            } else if has_any_obj {
                PdfRepairAssessment::PartiallySalvageable
            } else {
                PdfRepairAssessment::Unrecoverable
            };
            PdfHealthReport {
                has_pdf_header,
                has_eof_marker,
                has_startxref,
                is_encrypted: raw.windows(8).any(|w| w == b"/Encrypt"),
                page_count: 0,
                object_count: 0,
                assessment,
                diagnostic: format!("PDF xref/parser error: {err}"),
            }
        }
    }
}

/// Validates and maps an IO-read failure on missing/empty files before inspection.
///
/// # Errors
/// Returns [`FileFusionError`] when `raw` cannot represent a valid PDF header.
pub fn ensure_pdf_magic(raw: &[u8]) -> FileFusionResult<()> {
    let head = &raw[..raw.len().min(1024)];
    if head.windows(5).any(|w| w == b"%PDF-") {
        Ok(())
    } else {
        Err(FileFusionError::new(
            ErrorCode::FmtCorrupt,
            "input payload does not begin with a `%PDF-` signature",
        ))
    }
}

/// Builds a sanitized Tier 2 `qpdf` `ProcessInvocation` (`MOD-RULE-04`, zero shell invocation)
/// writing to `.filefusion.tmp.*` `staged_output`.
#[cfg(feature = "native-process")]
#[must_use]
pub fn build_qpdf_optimize_invocation(
    qpdf_binary: &Path,
    input_path: &Path,
    staged_output_path: &Path,
    mode: QpdfMode,
    linearize: bool,
    timeout_secs: u64,
) -> ProcessInvocation {
    let mut args = vec![OsString::from("--object-streams=generate")];
    match mode {
        QpdfMode::OptimizeLinearize => {
            args.push(OsString::from("--stream-data=compress"));
            args.push(OsString::from("--compression-level=9"));
            args.push(OsString::from("--recompress-flate"));
            if linearize {
                args.push(OsString::from("--linearize"));
            }
        }
        QpdfMode::StructuralRepair => {
            args.push(OsString::from("--suppress-recovery"));
            args.push(OsString::from("--stream-data=compress"));
        }
    }
    args.push(input_path.as_os_str().to_os_string());
    args.push(staged_output_path.as_os_str().to_os_string());

    ProcessInvocation {
        executable: qpdf_binary.to_path_buf(),
        args,
        working_dir: None,
        timeout_secs,
        allow_env_keys: Vec::new(),
    }
}
