//! Structural PDF integrity assessment (`FR-REP-001`..`002`), pure-Rust cross-reference
//! recovery, and Tier 2 `qpdf` linearization / repair orchestration (`MOD-RULE-04`, `MOD-RULE-05`).

use std::collections::BTreeMap;
use std::fmt::Write as _;

use file_core::{ErrorCode, FileFusionError, FileFusionResult, JobContext};
use lopdf::{Document, Object};
use serde::{Deserialize, Serialize};

#[cfg(feature = "native-process")]
use std::ffi::OsString;
#[cfg(feature = "native-process")]
use std::fs;
#[cfg(feature = "native-process")]
use std::path::Path;

#[cfg(feature = "native-process")]
use file_core::{IsolatedProcessRunner, ProcessInvocation, StagedPath};

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

/// Repairs a damaged or trailer-truncated PDF purely in memory (`FR-REP-001`), rebuilding missing
/// `xref`/`startxref`/`%%EOF` tables from intact indirect object definitions when possible.
///
/// # Errors
/// Returns [`FileFusionError`] (`FmtDrm` for encrypted PDFs, `FmtCorrupt` when unrecoverable, or
/// cancellation).
pub fn repair_pdf_bytes(
    raw_pdf: &[u8],
    ctx: &JobContext,
) -> FileFusionResult<(Vec<u8>, PdfHealthReport)> {
    ctx.check_cancelled()?;
    ensure_pdf_magic(raw_pdf)?;
    if raw_pdf.windows(8).any(|w| w == b"/Encrypt") {
        return Err(FileFusionError::new(
            ErrorCode::FmtDrm,
            "encrypted PDFs must be unlocked before structural repair",
        ));
    }

    let doc_res = Document::load_mem(raw_pdf);
    let mut doc = if let Ok(d) = doc_res {
        if d.is_encrypted() {
            return Err(FileFusionError::new(
                ErrorCode::FmtDrm,
                "encrypted PDFs must be unlocked before structural repair",
            ));
        }
        d
    } else {
        let reconstructed = synthesize_xref_trailer_for_truncated_pdf(raw_pdf).ok_or_else(|| {
            FileFusionError::new(
                ErrorCode::FmtCorrupt,
                "PDF cross-reference table and indirect object headers are unrecoverable natively",
            )
        })?;
        Document::load_mem(&reconstructed).map_err(|err| {
            FileFusionError::new(
                ErrorCode::FmtCorrupt,
                format!("PDF structure remained unparseable after native xref synthesis: {err}"),
            )
        })?
    };

    ctx.check_cancelled()?;
    doc.compress();
    doc.prune_objects();
    doc.renumber_objects();
    let mut out = Vec::new();
    doc.save_to(&mut out).map_err(|err| {
        FileFusionError::new(
            ErrorCode::IntInvariant,
            format!("failed to serialize repaired PDF: {err}"),
        )
    })?;
    let post_health = inspect_pdf_health(&out);
    Ok((out, post_health))
}

/// Repairs a damaged PDF into `staged_output`, preferring isolated Tier 2 `qpdf` structural
/// recovery when available and falling back cleanly to pure-Rust [`repair_pdf_bytes`]
/// (`FR-REP-001`, `MOD-RULE-04`).
///
/// Returns the post-repair [`PdfHealthReport`] paired with the engine identifier actually used
/// (`"tier2-qpdf-repair"` or `"tier1-pdf-repair"`).
///
/// # Errors
/// Returns [`FileFusionError`] on unrecoverable corruption, subprocess failure, or cancellation.
#[cfg(feature = "native-process")]
pub fn repair_pdf_to_staged_with_runner(
    input_path: &Path,
    staged_output: &StagedPath,
    qpdf_binary: Option<&Path>,
    runner: Option<&dyn IsolatedProcessRunner>,
    ctx: &JobContext,
) -> FileFusionResult<(PdfHealthReport, &'static str)> {
    ctx.check_cancelled()?;
    if let (Some(qpdf_bin), Some(exec)) = (qpdf_binary, runner) {
        let inv = build_qpdf_optimize_invocation(
            qpdf_bin,
            input_path,
            staged_output.staged_path(),
            QpdfMode::StructuralRepair,
            false,
            ctx.limits.subprocess_timeout_secs,
        );
        let out = exec.run_isolated(&inv, ctx)?;
        if !out.timed_out && (out.exit_code == 0 || out.exit_code == 3) {
            let repaired_raw = fs::read(staged_output.staged_path())?;
            return Ok((inspect_pdf_health(&repaired_raw), "tier2-qpdf-repair"));
        }
    }

    let raw = fs::read(input_path)?;
    let (repaired, report) = repair_pdf_bytes(&raw, ctx)?;
    fs::write(staged_output.staged_path(), &repaired)?;
    Ok((report, "tier1-pdf-repair"))
}

fn synthesize_xref_trailer_for_truncated_pdf(raw: &[u8]) -> Option<Vec<u8>> {
    let mut offsets: BTreeMap<u32, (u16, usize)> = BTreeMap::new();
    let mut root_ref: Option<(u32, u16)> = parse_root_ref_hint(raw);

    let mut idx = 0usize;
    while idx < raw.len() {
        let line_start = idx;
        let line_end = raw[idx..]
            .iter()
            .position(|&b| b == b'\n' || b == b'\r')
            .map_or(raw.len(), |rel| idx + rel);
        let line = &raw[line_start..line_end];
        if let Some((obj_num, gen_num)) = parse_obj_header_line(line) {
            offsets.entry(obj_num).or_insert((gen_num, line_start));
            if root_ref.is_none() {
                let lookahead_end = (line_end + 512).min(raw.len());
                let slice = &raw[line_start..lookahead_end];
                if slice.windows(8).any(|w| w == b"/Catalog") {
                    root_ref = Some((obj_num, gen_num));
                }
            }
        }
        idx = line_end + 1;
        while idx < raw.len() && (raw[idx] == b'\n' || raw[idx] == b'\r') {
            idx += 1;
        }
    }

    let (root_id, root_gen) = root_ref?;
    if offsets.is_empty() {
        return None;
    }

    let body_end = raw
        .windows(4)
        .position(|w| w == b"xref")
        .or_else(|| raw.windows(9).position(|w| w == b"startxref"))
        .unwrap_or(raw.len());

    let mut out = raw[..body_end].to_vec();
    if !out.ends_with(b"\n") {
        out.push(b'\n');
    }
    let xref_offset = out.len();
    let max_id = offsets.keys().next_back().copied().unwrap_or(0);
    let size = max_id.saturating_add(1);

    let mut trailer_buf = String::new();
    let _ = writeln!(trailer_buf, "xref");
    let _ = writeln!(trailer_buf, "0 {size}");
    let _ = writeln!(trailer_buf, "0000000000 65535 f ");
    for id in 1..size {
        if let Some((gen_num, off)) = offsets.get(&id) {
            let _ = writeln!(trailer_buf, "{off:010} {gen_num:05} n ");
        } else {
            let _ = writeln!(trailer_buf, "0000000000 65535 f ");
        }
    }
    let _ = writeln!(trailer_buf, "trailer");
    let _ = writeln!(
        trailer_buf,
        "<< /Size {size} /Root {root_id} {root_gen} R >>"
    );
    let _ = writeln!(trailer_buf, "startxref");
    let _ = writeln!(trailer_buf, "{xref_offset}");
    let _ = writeln!(trailer_buf, "%%EOF");
    out.extend_from_slice(trailer_buf.as_bytes());
    let _ = Object::Null;
    Some(out)
}

fn parse_obj_header_line(line: &[u8]) -> Option<(u32, u16)> {
    let s = std::str::from_utf8(line).ok()?.trim();
    let mut parts = s.split_whitespace();
    let num = parts.next()?.parse::<u32>().ok()?;
    let gen_num = parts.next()?.parse::<u16>().ok()?;
    let kw = parts.next()?;
    if kw == "obj" && num > 0 {
        Some((num, gen_num))
    } else {
        None
    }
}

fn parse_root_ref_hint(raw: &[u8]) -> Option<(u32, u16)> {
    let pos = raw.windows(5).position(|w| w == b"/Root")?;
    let tail = std::str::from_utf8(&raw[pos + 5..(pos + 40).min(raw.len())]).ok()?;
    let mut parts = tail.split_whitespace();
    let num = parts.next()?.parse::<u32>().ok()?;
    let gen_num = parts.next()?.parse::<u16>().ok()?;
    let r = parts.next()?;
    if r.starts_with('R') {
        Some((num, gen_num))
    } else {
        None
    }
}
