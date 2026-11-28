//! Isolated out-of-process Tier 2 document and ebook tool adapters for `LibreOffice`
//! (`soffice`), `Pandoc` (`pandoc`), and `Calibre` (`ebook-convert`) (`FF-CTR-001` §3.3,
//! `FR-DOC-002`..`003`, `MOD-RULE-04`).

use std::ffi::OsString;
use std::fs;
use std::path::{Path, PathBuf};

use file_core::{
    ErrorCode, ExternalToolAdapter, FileFusionError, FileFusionResult, FormatId,
    IsolatedProcessRunner, JobContext, ProcessInvocation, ProcessOutput, ProcessingOperation,
    ProcessingRequest, StagedPath, ValidatedSource,
};

/// Tier 2 headless `LibreOffice` (`soffice`) adapter (`MPL-2.0` isolated subprocess).
#[derive(Debug, Clone)]
pub struct LibreOfficeAdapter {
    binary_path: PathBuf,
}

impl LibreOfficeAdapter {
    /// Creates a `LibreOffice` adapter bound to a verified `soffice`/`libreoffice` path.
    #[must_use]
    pub const fn new(binary_path: PathBuf) -> Self {
        Self { binary_path }
    }

    /// Builds a sanitized headless `soffice --convert-to` invocation inside `output_path`'s
    /// private temp directory (`FF-CTR-001` §3.3).
    ///
    /// # Errors
    /// Returns [`FileFusionError`] if `target_format` is unsupported by `LibreOffice`.
    pub fn build_invocation(
        &self,
        input_path: &Path,
        output_path: &Path,
        target_format: FormatId,
    ) -> FileFusionResult<ProcessInvocation> {
        let out_ext = libreoffice_target_arg(target_format)?;
        let out_dir = output_path
            .parent()
            .unwrap_or_else(|| Path::new("."))
            .to_path_buf();
        let profile_arg = OsString::from(format!(
            "-env:UserInstallation=file://{}",
            out_dir.join(".lo_profile").display()
        ));
        Ok(ProcessInvocation {
            executable: self.binary_path.clone(),
            args: vec![
                OsString::from("--headless"),
                OsString::from("--norestore"),
                OsString::from("--nodefault"),
                OsString::from("--nolockcheck"),
                OsString::from("--nofirststartwizard"),
                profile_arg,
                OsString::from("--convert-to"),
                OsString::from(out_ext),
                OsString::from("--outdir"),
                out_dir.into_os_string(),
                input_path.as_os_str().to_os_string(),
            ],
            working_dir: output_path.parent().map(Path::to_path_buf),
            timeout_secs: 180,
            allow_env_keys: vec![String::from("HOME"), String::from("TMPDIR")],
        })
    }
}

impl ExternalToolAdapter for LibreOfficeAdapter {
    fn adapter_id(&self) -> &'static str {
        "adapter-libreoffice"
    }

    fn capability_id(&self) -> &'static str {
        "tier2-libreoffice"
    }

    fn spdx_license(&self) -> &'static str {
        "MPL-2.0"
    }

    fn build_args(
        &self,
        input: &ValidatedSource,
        staged_output: &StagedPath,
        req: &ProcessingRequest,
    ) -> FileFusionResult<Vec<OsString>> {
        let target = extract_target_format(req)?;
        Ok(self
            .build_invocation(&input.path, staged_output.staged_path(), target)?
            .args)
    }

    fn execute(
        &self,
        input: &ValidatedSource,
        staged_output: &StagedPath,
        req: &ProcessingRequest,
        runner: &dyn IsolatedProcessRunner,
        ctx: &JobContext,
    ) -> FileFusionResult<ProcessOutput> {
        let target = extract_target_format(req)?;
        let staged_path = staged_output.staged_path();
        let out_dir = staged_path.parent().unwrap_or_else(|| Path::new("."));
        let stem = input
            .path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("converted");
        let expected = out_dir.join(format!("{stem}.{}", libreoffice_target_arg(target)?));
        let inv = self.build_invocation(&input.path, staged_path, target)?;
        execute_tier2_document_tool(runner, "LibreOffice", &inv, &expected, staged_path, ctx)
    }
}

/// Tier 2 `Pandoc` adapter (`GPL-2.0-or-later` isolated subprocess).
#[derive(Debug, Clone)]
pub struct PandocAdapter {
    binary_path: PathBuf,
}

impl PandocAdapter {
    /// Creates a `Pandoc` adapter bound to a verified `pandoc` binary path.
    #[must_use]
    pub const fn new(binary_path: PathBuf) -> Self {
        Self { binary_path }
    }

    /// Builds a sanitized `pandoc` conversion invocation given explicit source & target formats.
    ///
    /// # Errors
    /// Returns [`FileFusionError`] if `source_format` or `target_format` is unsupported by Pandoc.
    pub fn build_conversion_invocation(
        &self,
        input_path: &Path,
        output_path: &Path,
        source_format: FormatId,
        target_format: FormatId,
        timeout_secs: u64,
    ) -> FileFusionResult<ProcessInvocation> {
        let from_fmt = pandoc_format_arg(source_format)?;
        let to_fmt = pandoc_format_arg(target_format)?;
        Ok(ProcessInvocation {
            executable: self.binary_path.clone(),
            args: vec![
                input_path.as_os_str().to_os_string(),
                OsString::from("--from"),
                OsString::from(from_fmt),
                OsString::from("--to"),
                OsString::from(to_fmt),
                OsString::from("--standalone"),
                OsString::from("-o"),
                output_path.as_os_str().to_os_string(),
            ],
            working_dir: output_path.parent().map(Path::to_path_buf),
            timeout_secs: timeout_secs.max(60),
            allow_env_keys: vec![String::from("TMPDIR")],
        })
    }
}

impl ExternalToolAdapter for PandocAdapter {
    fn adapter_id(&self) -> &'static str {
        "adapter-pandoc"
    }

    fn capability_id(&self) -> &'static str {
        "tier2-pandoc"
    }

    fn spdx_license(&self) -> &'static str {
        "GPL-2.0-or-later"
    }

    fn build_args(
        &self,
        input: &ValidatedSource,
        staged_output: &StagedPath,
        req: &ProcessingRequest,
    ) -> FileFusionResult<Vec<OsString>> {
        let target = extract_target_format(req)?;
        Ok(self
            .build_conversion_invocation(
                &input.path,
                staged_output.staged_path(),
                input.inspection.detected_format,
                target,
                120,
            )?
            .args)
    }

    fn execute(
        &self,
        input: &ValidatedSource,
        staged_output: &StagedPath,
        req: &ProcessingRequest,
        runner: &dyn IsolatedProcessRunner,
        ctx: &JobContext,
    ) -> FileFusionResult<ProcessOutput> {
        let target = extract_target_format(req)?;
        let staged_path = staged_output.staged_path();
        let inv = self.build_conversion_invocation(
            &input.path,
            staged_path,
            input.inspection.detected_format,
            target,
            ctx.limits.subprocess_timeout_secs,
        )?;
        execute_tier2_document_tool(runner, "Pandoc", &inv, staged_path, staged_path, ctx)
    }
}

/// Tier 2 `Calibre` (`ebook-convert`) adapter (`GPL-3.0-only` isolated subprocess, `FR-DOC-003`).
#[derive(Debug, Clone)]
pub struct CalibreAdapter {
    binary_path: PathBuf,
}

impl CalibreAdapter {
    /// Creates a `Calibre` adapter bound to a verified `ebook-convert` binary path.
    #[must_use]
    pub const fn new(binary_path: PathBuf) -> Self {
        Self { binary_path }
    }

    /// Builds a sanitized `ebook-convert` invocation targeting a format-suffixed output path.
    ///
    /// # Errors
    /// Returns [`FileFusionError`] if `target_format` is unsupported by Calibre.
    pub fn build_invocation(
        &self,
        input_path: &Path,
        output_path: &Path,
        target_format: FormatId,
    ) -> FileFusionResult<ProcessInvocation> {
        let _ext = calibre_target_extension(target_format)?;
        Ok(ProcessInvocation {
            executable: self.binary_path.clone(),
            args: vec![
                input_path.as_os_str().to_os_string(),
                output_path.as_os_str().to_os_string(),
            ],
            working_dir: output_path.parent().map(Path::to_path_buf),
            timeout_secs: 180,
            allow_env_keys: vec![String::from("HOME"), String::from("TMPDIR")],
        })
    }
}

impl ExternalToolAdapter for CalibreAdapter {
    fn adapter_id(&self) -> &'static str {
        "adapter-calibre"
    }

    fn capability_id(&self) -> &'static str {
        "tier2-calibre"
    }

    fn spdx_license(&self) -> &'static str {
        "GPL-3.0-only"
    }

    fn build_args(
        &self,
        input: &ValidatedSource,
        staged_output: &StagedPath,
        req: &ProcessingRequest,
    ) -> FileFusionResult<Vec<OsString>> {
        let target = extract_target_format(req)?;
        let ext = calibre_target_extension(target)?;
        let temp_out = staged_output.staged_path().with_extension(ext);
        Ok(self.build_invocation(&input.path, &temp_out, target)?.args)
    }

    fn execute(
        &self,
        input: &ValidatedSource,
        staged_output: &StagedPath,
        req: &ProcessingRequest,
        runner: &dyn IsolatedProcessRunner,
        ctx: &JobContext,
    ) -> FileFusionResult<ProcessOutput> {
        let target = extract_target_format(req)?;
        let staged_path = staged_output.staged_path();
        let ext = calibre_target_extension(target)?;
        let expected = staged_path.with_extension(ext);
        let inv = self.build_invocation(&input.path, &expected, target)?;
        execute_tier2_document_tool(runner, "Calibre", &inv, &expected, staged_path, ctx)
    }
}

/// Executes a sanitized Tier 2 document invocation via [`IsolatedProcessRunner`], staging into a
/// tool-expected extension when tools require extension-based muxing/output naming and copying into
/// `staged_output_path`.
///
/// # Errors
/// Returns [`FileFusionError`] on non-zero exit code, timeout, cancellation, or missing produced output.
pub fn execute_tier2_document_tool(
    runner: &dyn IsolatedProcessRunner,
    tool_label: &str,
    inv: &ProcessInvocation,
    expected_produced_path: &Path,
    staged_output_path: &Path,
    ctx: &JobContext,
) -> FileFusionResult<ProcessOutput> {
    ctx.check_cancelled()?;
    let output = runner.run_isolated(inv, ctx)?;
    if output.timed_out || output.exit_code != 0 {
        let stderr_preview = String::from_utf8_lossy(&output.stderr);
        return Err(FileFusionError::new(
            ErrorCode::IntInvariant,
            format!(
                "{tool_label} conversion failed (exit={}, timed_out={}): {}",
                output.exit_code,
                output.timed_out,
                stderr_preview.trim()
            ),
        ));
    }
    if expected_produced_path != staged_output_path {
        if !expected_produced_path.is_file() {
            return Err(FileFusionError::new(
                ErrorCode::FmtCorrupt,
                format!("{tool_label} completed without producing expected output artifact"),
            ));
        }
        fs::copy(expected_produced_path, staged_output_path)?;
        let _ = fs::remove_file(expected_produced_path);
    }
    Ok(output)
}

fn extract_target_format(req: &ProcessingRequest) -> FileFusionResult<FormatId> {
    match req.operation {
        ProcessingOperation::Convert { target_format, .. } => Ok(target_format),
        ProcessingOperation::Compress {
            target_format: Some(fmt),
            ..
        } => Ok(fmt),
        _ => Err(FileFusionError::new(
            ErrorCode::FmtUnsupported,
            "Tier 2 document adapter requires a Convert operation target format",
        )),
    }
}

fn libreoffice_target_arg(fmt: FormatId) -> FileFusionResult<&'static str> {
    match fmt {
        FormatId::Pdf => Ok("pdf"),
        FormatId::Docx => Ok("docx"),
        FormatId::Xlsx => Ok("xlsx"),
        FormatId::Pptx => Ok("pptx"),
        FormatId::Odt => Ok("odt"),
        FormatId::Ods => Ok("ods"),
        FormatId::Odp => Ok("odp"),
        FormatId::Html => Ok("html"),
        FormatId::PlainText => Ok("txt"),
        other => Err(FileFusionError::new(
            ErrorCode::FmtUnsupported,
            format!("LibreOffice target format {other:?} is unsupported"),
        )),
    }
}

fn pandoc_format_arg(fmt: FormatId) -> FileFusionResult<&'static str> {
    match fmt {
        FormatId::Markdown => Ok("markdown"),
        FormatId::Html => Ok("html"),
        FormatId::Docx => Ok("docx"),
        FormatId::Odt => Ok("odt"),
        FormatId::Epub => Ok("epub"),
        FormatId::Rtf => Ok("rtf"),
        FormatId::PlainText => Ok("plain"),
        other => Err(FileFusionError::new(
            ErrorCode::FmtUnsupported,
            format!("Pandoc format {other:?} is unsupported"),
        )),
    }
}

/// Canonical filename extension expected by `Calibre` / `LibreOffice` / `Pandoc` output files.
///
/// # Errors
/// Returns [`FileFusionError`] if `fmt` has no ebook conversion extension.
pub fn calibre_target_extension(fmt: FormatId) -> FileFusionResult<&'static str> {
    match fmt {
        FormatId::Epub => Ok("epub"),
        FormatId::Mobi => Ok("mobi"),
        FormatId::Pdf => Ok("pdf"),
        FormatId::Html => Ok("html"),
        FormatId::PlainText => Ok("txt"),
        other => Err(FileFusionError::new(
            ErrorCode::FmtUnsupported,
            format!("Calibre ebook-convert target {other:?} is unsupported"),
        )),
    }
}
