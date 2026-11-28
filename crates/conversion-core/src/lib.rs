//! `conversion-core`: L4 universal conversion graph, pre-flight feature-loss warnings,
//! structural fidelity reporting, and cross-media conversion execution
//! (`FR-DOC-001`..`003`, `FR-AV-003`, `FR-FID-001`..`002`, `FF-MOD-001` §4).

#![forbid(unsafe_code)]

pub mod graph;
pub mod markup;
pub mod office_epub;
pub mod preflight;
pub mod tabular;
#[cfg(feature = "native-process")]
pub mod tier2_adapters;

use compression_archive::{
    ArchiveContainerKind, ArchiveOutputValidator, ArchiveQualityEvaluator,
    create_archive_from_entries, read_archive_entries_checked,
};
use compression_audio::AudioCompressionEngine;
use compression_image::{ImageCompressionOptions, RasterCompressionEngine, RasterQualityEvaluator};
use file_core::{
    CompressionEngine, CompressionProfile, CompressionRequest, ConversionEngine,
    ConversionFidelityReport, ConversionRequest, ConversionRoute, ErrorCode, FidelitySeverity,
    FileFusionError, FileFusionResult, FileInspectionReport, FormatId, JobContext, OutputValidator,
    PreflightAssessment, QualityEvaluator, StagedPath, ValidatedSource, detect_signature_from_path,
};
use validation_engine::MagicByteOutputValidator;

#[cfg(feature = "native-process")]
use std::path::PathBuf;

#[cfg(feature = "native-process")]
use compression_audio::{
    AudioCompressionOptions, build_ffmpeg_audio_transcode_invocation,
    build_ffmpeg_extract_audio_invocation, execute_ffmpeg_invocation,
};
#[cfg(feature = "native-process")]
use compression_video::{VideoCompressionEngine, VideoCompressionOptions};
#[cfg(feature = "native-process")]
use file_core::IsolatedProcessRunner;

pub use graph::{ConversionGraph, ResolvedConversionPlan};
pub use preflight::{
    StructuralInventory, analyze_markup_structure, build_fidelity_report, evaluate_preflight,
};

/// L4 Universal Format Conversion Engine coordinating pure-Rust Tier 1 raster/archive/audio
/// cross-conversions and optional Tier 2 external adapters (`FFmpeg`, `LibreOffice`, `Pandoc`,
/// `Calibre`) under [`ConversionEngine`].
#[derive(Debug, Clone)]
pub struct UniversalConversionEngine {
    graph: ConversionGraph,
    #[cfg(feature = "native-process")]
    ffmpeg: Option<PathBuf>,
    #[cfg(feature = "native-process")]
    ffprobe: Option<PathBuf>,
    #[cfg(feature = "native-process")]
    libreoffice: Option<PathBuf>,
    #[cfg(feature = "native-process")]
    pandoc: Option<PathBuf>,
    #[cfg(feature = "native-process")]
    calibre: Option<PathBuf>,
}

impl Default for UniversalConversionEngine {
    fn default() -> Self {
        Self::new_tier1()
    }
}

impl UniversalConversionEngine {
    /// Creates a Tier 1 conversion engine (`Native` + `wasm32`) with direct-first multi-hop routing.
    #[must_use]
    pub fn new_tier1() -> Self {
        Self {
            graph: ConversionGraph::new_baseline(),
            #[cfg(feature = "native-process")]
            ffmpeg: None,
            #[cfg(feature = "native-process")]
            ffprobe: None,
            #[cfg(feature = "native-process")]
            libreoffice: None,
            #[cfg(feature = "native-process")]
            pandoc: None,
            #[cfg(feature = "native-process")]
            calibre: None,
        }
    }

    /// Configures verified Tier 2 external binaries (`ffmpeg`, `ffprobe`) for native video/audio routes.
    #[cfg(feature = "native-process")]
    #[must_use]
    pub fn with_ffmpeg_binaries(ffmpeg: Option<PathBuf>, ffprobe: Option<PathBuf>) -> Self {
        Self {
            graph: ConversionGraph::new_baseline(),
            ffmpeg,
            ffprobe,
            libreoffice: None,
            pandoc: None,
            calibre: None,
        }
    }

    /// Configures verified Tier 2 document and ebook binaries (`libreoffice`, `pandoc`, `calibre`)
    /// alongside optional media binaries (`ffmpeg`, `ffprobe`).
    #[cfg(feature = "native-process")]
    #[must_use]
    pub fn with_tier2_binaries(
        ffmpeg: Option<PathBuf>,
        ffprobe: Option<PathBuf>,
        libreoffice: Option<PathBuf>,
        pandoc: Option<PathBuf>,
        calibre: Option<PathBuf>,
    ) -> Self {
        Self {
            graph: ConversionGraph::new_baseline(),
            ffmpeg,
            ffprobe,
            libreoffice,
            pandoc,
            calibre,
        }
    }

    /// Exposes the underlying [`ConversionGraph`] for multi-hop route planning.
    #[must_use]
    pub const fn graph(&self) -> &ConversionGraph {
        &self.graph
    }

    /// Executes conversion using an optional Tier 2 [`IsolatedProcessRunner`] when the route
    /// traverses video containers, audio lossy transcodes, `LibreOffice`, `Pandoc`, or `Calibre`.
    ///
    /// # Errors
    /// Returns [`FileFusionError`] on unsupported route, breaking-loss preflight abort,
    /// missing Tier 2 binary/runner, validation failure, or cancellation.
    #[cfg(feature = "native-process")]
    pub fn convert_with_optional_runner(
        &self,
        input: &ValidatedSource,
        staged_output: &StagedPath,
        req: &ConversionRequest,
        runner: Option<&dyn IsolatedProcessRunner>,
        ctx: &JobContext,
    ) -> FileFusionResult<ConversionFidelityReport> {
        ctx.check_cancelled()?;
        let assessment = self.preflight(&input.inspection, req.target_format)?;
        enforce_preflight_gate(&assessment, req)?;

        let source = input.inspection.detected_format;
        let target = req.target_format;
        let plan = self.graph.resolve_route_plan(source, target, true)?;
        if plan.required_tier2_tools.is_empty() {
            return Self::convert_tier1_internal(input, staged_output, req, &assessment, ctx);
        }

        if plan
            .required_tier2_tools
            .iter()
            .any(|t| t == "tier2-ffmpeg")
        {
            return self.convert_via_ffmpeg(input, staged_output, req, &assessment, runner, ctx);
        }

        if let Some(tool_id) = plan.required_tier2_tools.first() {
            if matches!(
                tool_id.as_str(),
                "tier2-libreoffice" | "tier2-pandoc" | "tier2-calibre"
            ) {
                return self.convert_via_tier2_document(
                    tool_id,
                    input,
                    staged_output,
                    req,
                    &assessment,
                    runner,
                    ctx,
                );
            }
        }

        Err(FileFusionError::new(
            ErrorCode::FmtUnsupported,
            format!(
                "conversion route {source:?} -> {target:?} requires Tier 2 engine {:?} (not yet dispatched)",
                plan.required_tier2_tools
            ),
        ))
    }

    #[cfg(feature = "native-process")]
    #[allow(clippy::too_many_arguments)]
    fn convert_via_tier2_document(
        &self,
        tool_id: &str,
        input: &ValidatedSource,
        staged_output: &StagedPath,
        req: &ConversionRequest,
        assessment: &PreflightAssessment,
        runner: Option<&dyn IsolatedProcessRunner>,
        ctx: &JobContext,
    ) -> FileFusionResult<ConversionFidelityReport> {
        let source = input.inspection.detected_format;
        let target = req.target_format;
        let Some(exec) = runner else {
            return Err(FileFusionError::new(
                ErrorCode::EngMissing,
                format!("IsolatedProcessRunner required for {tool_id} conversion"),
            ));
        };

        let staged_path = staged_output.staged_path();
        let temp_parent = staged_path
            .parent()
            .unwrap_or_else(|| std::path::Path::new("."));
        let stem = input
            .path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("converted");

        match tool_id {
            "tier2-libreoffice" => {
                let Some(bin) = &self.libreoffice else {
                    return Err(FileFusionError::new(
                        ErrorCode::EngMissing,
                        "Tier 2 LibreOffice (`soffice`) binary required for this route",
                    ));
                };
                let adapter = tier2_adapters::LibreOfficeAdapter::new(bin.clone());
                let ext = format_default_extension(target);
                let expected = temp_parent.join(format!("{stem}.{ext}"));
                let inv = adapter.build_invocation(&input.path, staged_path, target)?;
                let _out = tier2_adapters::execute_tier2_document_tool(
                    exec,
                    "LibreOffice",
                    &inv,
                    &expected,
                    staged_path,
                    ctx,
                )?;
            }
            "tier2-pandoc" => {
                let Some(bin) = &self.pandoc else {
                    return Err(FileFusionError::new(
                        ErrorCode::EngMissing,
                        "Tier 2 `pandoc` binary required for this route",
                    ));
                };
                let adapter = tier2_adapters::PandocAdapter::new(bin.clone());
                let inv = adapter.build_conversion_invocation(
                    &input.path,
                    staged_path,
                    source,
                    target,
                    ctx.limits.subprocess_timeout_secs,
                )?;
                let _out = tier2_adapters::execute_tier2_document_tool(
                    exec,
                    "Pandoc",
                    &inv,
                    staged_path,
                    staged_path,
                    ctx,
                )?;
            }
            "tier2-calibre" => {
                let Some(bin) = &self.calibre else {
                    return Err(FileFusionError::new(
                        ErrorCode::EngMissing,
                        "Tier 2 Calibre (`ebook-convert`) binary required for this route",
                    ));
                };
                let adapter = tier2_adapters::CalibreAdapter::new(bin.clone());
                let ext = tier2_adapters::calibre_target_extension(target)?;
                let expected = temp_parent.join(format!("{stem}.{ext}"));
                let inv = adapter.build_invocation(&input.path, &expected, target)?;
                let _out = tier2_adapters::execute_tier2_document_tool(
                    exec,
                    "Calibre",
                    &inv,
                    &expected,
                    staged_path,
                    ctx,
                )?;
            }
            _ => {}
        }

        validate_staged_any_document_output(staged_path, target, &ctx.limits)?;
        Ok(build_fidelity_report(
            source,
            target,
            assessment.warnings.clone(),
            None,
            None,
            None,
        ))
    }

    #[cfg(feature = "native-process")]
    fn convert_via_ffmpeg(
        &self,
        input: &ValidatedSource,
        staged_output: &StagedPath,
        req: &ConversionRequest,
        assessment: &PreflightAssessment,
        runner: Option<&dyn IsolatedProcessRunner>,
        ctx: &JobContext,
    ) -> FileFusionResult<ConversionFidelityReport> {
        let source = input.inspection.detected_format;
        let target = req.target_format;

        let Some(ffmpeg_bin) = &self.ffmpeg else {
            return Err(FileFusionError::new(
                ErrorCode::EngMissing,
                "Tier 2 `ffmpeg` binary required for this media conversion route",
            ));
        };
        let Some(exec) = runner else {
            return Err(FileFusionError::new(
                ErrorCode::EngMissing,
                "IsolatedProcessRunner required for Tier 2 media conversion",
            ));
        };

        if is_video_format(target) {
            let vid_engine = VideoCompressionEngine::with_ffmpeg_binaries(
                ffmpeg_bin.clone(),
                self.ffprobe.clone(),
            );
            let opts = VideoCompressionOptions {
                base: default_conversion_compression_req(target, req.strip_metadata),
                target_codec: None,
                crf_override: None,
                max_width: None,
                max_height: None,
                target_fps: None,
                allow_fast_remux: false,
                prefer_deterministic_cpu: false,
                hw_profile_override: None,
            };
            let outcome =
                vid_engine.compress_with_runner(input, staged_output, &opts, exec, ctx)?;
            return Ok(build_fidelity_report(
                source,
                target,
                assessment.warnings.clone(),
                None,
                None,
                outcome.quality,
            ));
        }

        if is_video_format(source) && is_audio_format(target) {
            let inv = build_ffmpeg_extract_audio_invocation(
                ffmpeg_bin,
                &input.path,
                staged_output.staged_path(),
                target,
                false,
                ctx.limits.subprocess_timeout_secs,
            )?;
            execute_ffmpeg_invocation(exec, &inv, ctx)?;
            MagicByteOutputValidator.validate_staged(
                staged_output.staged_path(),
                target,
                &ctx.limits,
            )?;
            return Ok(build_fidelity_report(
                source,
                target,
                assessment.warnings.clone(),
                None,
                None,
                None,
            ));
        }

        if is_audio_format(target) {
            let opts = AudioCompressionOptions {
                base: default_conversion_compression_req(target, req.strip_metadata),
                target_sample_rate: None,
                target_channels: None,
                target_lufs: None,
                target_bitrate_kbps: None,
            };
            let inv = build_ffmpeg_audio_transcode_invocation(
                ffmpeg_bin,
                &input.path,
                staged_output.staged_path(),
                target,
                &opts,
                ctx.limits.subprocess_timeout_secs,
            )?;
            execute_ffmpeg_invocation(exec, &inv, ctx)?;
            MagicByteOutputValidator.validate_staged(
                staged_output.staged_path(),
                target,
                &ctx.limits,
            )?;
            return Ok(build_fidelity_report(
                source,
                target,
                assessment.warnings.clone(),
                None,
                None,
                None,
            ));
        }

        Err(FileFusionError::new(
            ErrorCode::FmtUnsupported,
            format!("FFmpeg conversion route {source:?} -> {target:?} is not supported"),
        ))
    }

    fn convert_tier1_internal(
        input: &ValidatedSource,
        staged_output: &StagedPath,
        req: &ConversionRequest,
        assessment: &PreflightAssessment,
        ctx: &JobContext,
    ) -> FileFusionResult<ConversionFidelityReport> {
        let source = input.inspection.detected_format;
        let target = req.target_format;

        if is_raster_format(source) && is_raster_format(target) {
            let cmp_req = default_conversion_compression_req(target, req.strip_metadata);
            let opts = ImageCompressionOptions {
                base: cmp_req,
                resize: None,
                auto_orient: true,
            };
            let outcome =
                RasterCompressionEngine.compress_with_options(input, staged_output, &opts, ctx)?;
            let eval_quality = match outcome.quality {
                Some(q) => Some(q),
                None => RasterQualityEvaluator
                    .evaluate(input, staged_output.staged_path(), ctx)
                    .ok(),
            };
            return Ok(build_fidelity_report(
                source,
                target,
                assessment.warnings.clone(),
                None,
                None,
                eval_quality,
            ));
        }

        if let Some(target_kind) = format_to_archive_kind(target) {
            if is_archive_format(source) {
                return convert_tier1_archive(input, staged_output, target_kind, assessment, ctx);
            }
        }

        if is_audio_format(source) && matches!(target, FormatId::Wav | FormatId::Flac) {
            let cmp_req = default_conversion_compression_req(target, req.strip_metadata);
            let outcome = AudioCompressionEngine::new_tier1().compress(
                input,
                staged_output,
                &cmp_req,
                ctx,
            )?;
            return Ok(build_fidelity_report(
                source,
                target,
                assessment.warnings.clone(),
                None,
                None,
                outcome.quality,
            ));
        }

        if (office_epub::is_spreadsheet_format(source) || is_structured_format(source))
            && (office_epub::is_spreadsheet_format(target)
                || is_structured_format(target)
                || is_markup_format(target))
            && (office_epub::is_spreadsheet_format(source)
                || office_epub::is_spreadsheet_format(target))
        {
            return convert_tier1_spreadsheet(input, staged_output, req, assessment, ctx);
        }

        if (office_epub::is_native_office_or_epub(source) || is_markup_format(source))
            && (office_epub::is_native_office_or_epub(target) || is_markup_format(target))
            && !office_epub::is_spreadsheet_format(source)
            && !office_epub::is_spreadsheet_format(target)
            && (office_epub::is_native_office_or_epub(source)
                || office_epub::is_native_office_or_epub(target))
        {
            return convert_tier1_document_container(input, staged_output, req, assessment, ctx);
        }

        if is_markup_format(source) && is_markup_format(target) {
            return convert_tier1_markup(input, staged_output, req, assessment, ctx);
        }

        if is_structured_format(source)
            && (is_structured_format(target) || is_markup_format(target))
        {
            return convert_tier1_structured(input, staged_output, req, assessment, ctx);
        }

        Err(FileFusionError::new(
            ErrorCode::FmtUnsupported,
            format!("no pure-Rust Tier 1 converter active yet for {source:?} -> {target:?}"),
        ))
    }
}

fn convert_tier1_spreadsheet(
    input: &ValidatedSource,
    staged_output: &StagedPath,
    req: &ConversionRequest,
    assessment: &PreflightAssessment,
    ctx: &JobContext,
) -> FileFusionResult<ConversionFidelityReport> {
    let source = input.inspection.detected_format;
    let target = req.target_format;
    let raw_bytes = read_bytes_bounded(&input.path, ctx.limits.max_uncompressed_bytes)?;
    let out_bytes = office_epub::convert_spreadsheet_container(
        &raw_bytes,
        source,
        target,
        req.strip_metadata,
        &ctx.limits,
        ctx,
    )?;
    if office_epub::is_spreadsheet_format(target) {
        write_and_validate_binary_staged(
            staged_output.staged_path(),
            target,
            &out_bytes,
            &ctx.limits,
        )?;
    } else {
        let out_text = std::str::from_utf8(&out_bytes).map_err(|err| {
            FileFusionError::new(ErrorCode::FmtCorrupt, format!("invalid UTF-8: {err}"))
        })?;
        write_and_validate_text_staged(staged_output.staged_path(), target, out_text, &ctx.limits)?;
    }
    Ok(build_fidelity_report(
        source,
        target,
        assessment.warnings.clone(),
        None,
        None,
        None,
    ))
}

fn convert_tier1_document_container(
    input: &ValidatedSource,
    staged_output: &StagedPath,
    req: &ConversionRequest,
    assessment: &PreflightAssessment,
    ctx: &JobContext,
) -> FileFusionResult<ConversionFidelityReport> {
    let source = input.inspection.detected_format;
    let target = req.target_format;
    let raw_bytes = read_bytes_bounded(&input.path, ctx.limits.max_uncompressed_bytes)?;

    let before_md = if is_markup_format(source) {
        std::str::from_utf8(&raw_bytes).ok().map(String::from)
    } else {
        office_epub::extract_container_markdown_preview(&raw_bytes, source, &ctx.limits, ctx)
    };
    let before = before_md.as_deref().map(|txt| {
        let fmt = if is_markup_format(source) {
            source
        } else {
            FormatId::Markdown
        };
        analyze_markup_structure(txt, fmt)
    });

    let out_bytes = office_epub::convert_document_container(
        &raw_bytes,
        source,
        target,
        req.strip_metadata,
        &ctx.limits,
        ctx,
    )?;

    if office_epub::is_native_office_or_epub(target) {
        write_and_validate_binary_staged(
            staged_output.staged_path(),
            target,
            &out_bytes,
            &ctx.limits,
        )?;
    } else {
        let out_text = std::str::from_utf8(&out_bytes).map_err(|err| {
            FileFusionError::new(ErrorCode::FmtCorrupt, format!("invalid UTF-8: {err}"))
        })?;
        write_and_validate_text_staged(staged_output.staged_path(), target, out_text, &ctx.limits)?;
    }

    let after_md = if is_markup_format(target) {
        std::str::from_utf8(&out_bytes).ok().map(String::from)
    } else {
        office_epub::extract_container_markdown_preview(&out_bytes, target, &ctx.limits, ctx)
    };
    let after = after_md.as_deref().map(|txt| {
        let fmt = if is_markup_format(target) {
            target
        } else {
            FormatId::Markdown
        };
        analyze_markup_structure(txt, fmt)
    });

    Ok(build_fidelity_report(
        source,
        target,
        assessment.warnings.clone(),
        before,
        after,
        None,
    ))
}

fn read_bytes_bounded(path: &std::path::Path, max_bytes: u64) -> FileFusionResult<Vec<u8>> {
    let meta = std::fs::metadata(path)?;
    if meta.len() > max_bytes {
        return Err(FileFusionError::new(
            ErrorCode::SecBomb,
            format!(
                "conversion input ({} bytes) exceeds limit ({max_bytes} bytes)",
                meta.len()
            ),
        ));
    }
    Ok(std::fs::read(path)?)
}

fn write_and_validate_binary_staged(
    staged_path: &std::path::Path,
    target: FormatId,
    bytes: &[u8],
    limits: &file_core::ResourceLimits,
) -> FileFusionResult<()> {
    if bytes.is_empty() {
        return Err(FileFusionError::new(
            ErrorCode::FmtCorrupt,
            "converted binary output is empty",
        ));
    }
    std::fs::write(staged_path, bytes)?;
    MagicByteOutputValidator.validate_staged(staged_path, target, limits)?;
    Ok(())
}

#[cfg(feature = "native-process")]
fn validate_staged_any_document_output(
    staged_path: &std::path::Path,
    target: FormatId,
    limits: &file_core::ResourceLimits,
) -> FileFusionResult<()> {
    let sig = detect_signature_from_path(staged_path)?;
    if sig.detected_format == FormatId::Executable {
        return Err(FileFusionError::new(
            ErrorCode::SecBomb,
            "Tier 2 output rejected by executable-signature gate",
        ));
    }
    if matches!(
        target,
        FormatId::Pdf
            | FormatId::Docx
            | FormatId::Xlsx
            | FormatId::Pptx
            | FormatId::Odt
            | FormatId::Ods
            | FormatId::Odp
            | FormatId::Epub
            | FormatId::Html
            | FormatId::Xml
            | FormatId::Rtf
    ) {
        MagicByteOutputValidator.validate_staged(staged_path, target, limits)?;
    }
    Ok(())
}

#[cfg(feature = "native-process")]
const fn format_default_extension(fmt: FormatId) -> &'static str {
    match fmt {
        FormatId::Pdf => "pdf",
        FormatId::Docx => "docx",
        FormatId::Xlsx => "xlsx",
        FormatId::Pptx => "pptx",
        FormatId::Odt => "odt",
        FormatId::Ods => "ods",
        FormatId::Odp => "odp",
        FormatId::Epub => "epub",
        FormatId::Mobi => "mobi",
        FormatId::Html => "html",
        FormatId::PlainText => "txt",
        _ => "out",
    }
}

fn convert_tier1_archive(
    input: &ValidatedSource,
    staged_output: &StagedPath,
    target_kind: ArchiveContainerKind,
    assessment: &PreflightAssessment,
    ctx: &JobContext,
) -> FileFusionResult<ConversionFidelityReport> {
    let source = input.inspection.detected_format;
    let source_kind = ArchiveContainerKind::detect_from_path_with_limits(&input.path, &ctx.limits)?;
    let entries = read_archive_entries_checked(&input.path, source_kind, &ctx.limits, ctx)?;
    let _summary = create_archive_from_entries(
        &entries,
        staged_output.staged_path(),
        target_kind,
        CompressionProfile::BalancedOptimal,
        ctx,
    )?;
    let validator = ArchiveOutputValidator {
        allow_brotli: target_kind == ArchiveContainerKind::TarBrotli,
    };
    validator.validate_staged(
        staged_output.staged_path(),
        target_kind.outer_format_id(),
        &ctx.limits,
    )?;
    let quality = ArchiveQualityEvaluator {
        source_kind: Some(source_kind),
        target_kind: Some(target_kind),
    }
    .evaluate(input, staged_output.staged_path(), ctx)?;
    Ok(build_fidelity_report(
        source,
        target_kind.outer_format_id(),
        assessment.warnings.clone(),
        None,
        None,
        Some(quality),
    ))
}

fn convert_tier1_markup(
    input: &ValidatedSource,
    staged_output: &StagedPath,
    req: &ConversionRequest,
    assessment: &PreflightAssessment,
    ctx: &JobContext,
) -> FileFusionResult<ConversionFidelityReport> {
    let source = input.inspection.detected_format;
    let target = req.target_format;
    let raw_in = read_utf8_source_bounded(&input.path, ctx.limits.max_uncompressed_bytes)?;
    ctx.check_cancelled()?;
    let before = Some(analyze_markup_structure(&raw_in, source));
    let rendered = markup::convert_markup_text(&raw_in, source, target, req.strip_metadata)?;
    write_and_validate_text_staged(staged_output.staged_path(), target, &rendered, &ctx.limits)?;
    let after = Some(analyze_markup_structure(&rendered, target));
    Ok(build_fidelity_report(
        source,
        target,
        assessment.warnings.clone(),
        before,
        after,
        None,
    ))
}

fn convert_tier1_structured(
    input: &ValidatedSource,
    staged_output: &StagedPath,
    req: &ConversionRequest,
    assessment: &PreflightAssessment,
    ctx: &JobContext,
) -> FileFusionResult<ConversionFidelityReport> {
    let source = input.inspection.detected_format;
    let target = req.target_format;
    let raw_in = read_utf8_source_bounded(&input.path, ctx.limits.max_uncompressed_bytes)?;
    ctx.check_cancelled()?;
    let rendered = tabular::convert_structured_data(&raw_in, source, target, req.strip_metadata)?;
    write_and_validate_text_staged(staged_output.staged_path(), target, &rendered, &ctx.limits)?;
    let after = is_markup_format(target).then(|| analyze_markup_structure(&rendered, target));
    let before = is_markup_format(target).then(|| StructuralInventory {
        tables: 1,
        ..StructuralInventory::default()
    });
    Ok(build_fidelity_report(
        source,
        target,
        assessment.warnings.clone(),
        before,
        after,
        None,
    ))
}

fn read_utf8_source_bounded(path: &std::path::Path, max_bytes: u64) -> FileFusionResult<String> {
    let meta = std::fs::metadata(path)?;
    if meta.len() > max_bytes {
        return Err(FileFusionError::new(
            ErrorCode::SecBomb,
            format!(
                "text conversion input ({} bytes) exceeds limit ({max_bytes} bytes)",
                meta.len()
            ),
        ));
    }
    let bytes = std::fs::read(path)?;
    String::from_utf8(bytes).map_err(|err| {
        FileFusionError::new(
            ErrorCode::FmtCorrupt,
            format!("input file is not valid UTF-8 text: {err}"),
        )
    })
}

fn write_and_validate_text_staged(
    staged_path: &std::path::Path,
    target: FormatId,
    content: &str,
    limits: &file_core::ResourceLimits,
) -> FileFusionResult<()> {
    let bytes = content.as_bytes();
    if bytes.is_empty() {
        return Err(FileFusionError::new(
            ErrorCode::FmtCorrupt,
            "converted text output is unexpectedly empty",
        ));
    }
    let byte_len = u64::try_from(bytes.len()).unwrap_or(u64::MAX);
    if byte_len > limits.max_uncompressed_bytes {
        return Err(FileFusionError::new(
            ErrorCode::SecBomb,
            format!(
                "converted text output ({byte_len} bytes) exceeds limit ({} bytes)",
                limits.max_uncompressed_bytes
            ),
        ));
    }
    std::fs::write(staged_path, bytes)?;
    let sig = detect_signature_from_path(staged_path)?;
    if sig.detected_format == FormatId::Executable {
        return Err(FileFusionError::new(
            ErrorCode::SecBomb,
            "converted output was rejected by executable-signature gate",
        ));
    }
    if matches!(target, FormatId::Html | FormatId::Xml) {
        MagicByteOutputValidator.validate_staged(staged_path, target, limits)?;
    }
    if is_structured_format(target) {
        let _reparsed = tabular::parse_structured_value(content, target)?;
    } else if is_markup_format(target) {
        let _reparsed = markup::parse_markup_document(content, target)?;
    }
    Ok(())
}

impl ConversionEngine for UniversalConversionEngine {
    fn engine_id(&self) -> &'static str {
        "l4-conversion-core"
    }

    fn routes(&self) -> &[ConversionRoute] {
        self.graph.routes()
    }

    fn preflight(
        &self,
        inspection: &FileInspectionReport,
        target: FormatId,
    ) -> FileFusionResult<PreflightAssessment> {
        let allow_tier2 = cfg!(feature = "native-process");
        let plan = self
            .graph
            .resolve_route_plan(inspection.detected_format, target, allow_tier2)
            .ok();
        let assessment = evaluate_preflight(inspection, target, plan.as_ref());
        Ok(assessment)
    }

    fn convert(
        &self,
        input: &ValidatedSource,
        staged_output: &StagedPath,
        req: &ConversionRequest,
        ctx: &JobContext,
    ) -> FileFusionResult<ConversionFidelityReport> {
        ctx.check_cancelled()?;
        let assessment = self.preflight(&input.inspection, req.target_format)?;
        enforce_preflight_gate(&assessment, req)?;
        Self::convert_tier1_internal(input, staged_output, req, &assessment, ctx)
    }
}

fn enforce_preflight_gate(
    assessment: &PreflightAssessment,
    req: &ConversionRequest,
) -> FileFusionResult<()> {
    if !assessment.supported {
        return Err(FileFusionError::new(
            ErrorCode::FmtUnsupported,
            format!(
                "conversion route to {:?} is unsupported or blocked by encryption",
                req.target_format
            ),
        ));
    }
    if req.fail_on_breaking_loss {
        if let Some(breaking) = assessment
            .warnings
            .iter()
            .find(|w| w.severity == FidelitySeverity::BreakingLoss)
        {
            return Err(FileFusionError::new(
                ErrorCode::IntInvariant,
                format!(
                    "aborted conversion due to fail_on_breaking_loss [{}]: {}",
                    breaking.code, breaking.message
                ),
            ));
        }
    }
    Ok(())
}

fn default_conversion_compression_req(
    target: FormatId,
    strip_metadata: bool,
) -> CompressionRequest {
    CompressionRequest {
        profile: CompressionProfile::BalancedOptimal,
        target_format: Some(target),
        target_max_bytes: None,
        min_ssim: None,
        strip_metadata,
        no_op_if_larger: false,
    }
}

const fn is_raster_format(fmt: FormatId) -> bool {
    matches!(
        fmt,
        FormatId::Png
            | FormatId::Jpeg
            | FormatId::Webp
            | FormatId::Avif
            | FormatId::Gif
            | FormatId::Bmp
            | FormatId::Tiff
            | FormatId::Ico
    )
}

const fn is_archive_format(fmt: FormatId) -> bool {
    matches!(
        fmt,
        FormatId::Zip
            | FormatId::Tar
            | FormatId::Gzip
            | FormatId::Zstd
            | FormatId::Xz
            | FormatId::Bzip2
            | FormatId::Lz4
            | FormatId::SevenZ
    )
}

const fn format_to_archive_kind(fmt: FormatId) -> Option<ArchiveContainerKind> {
    match fmt {
        FormatId::Zip => Some(ArchiveContainerKind::Zip),
        FormatId::Tar => Some(ArchiveContainerKind::Tar),
        FormatId::Gzip => Some(ArchiveContainerKind::TarGz),
        FormatId::Zstd => Some(ArchiveContainerKind::TarZst),
        FormatId::Xz => Some(ArchiveContainerKind::TarXz),
        FormatId::Bzip2 => Some(ArchiveContainerKind::TarBz2),
        FormatId::Lz4 => Some(ArchiveContainerKind::TarLz4),
        FormatId::SevenZ => Some(ArchiveContainerKind::SevenZ),
        _ => None,
    }
}

const fn is_audio_format(fmt: FormatId) -> bool {
    matches!(
        fmt,
        FormatId::Wav
            | FormatId::Flac
            | FormatId::Mp3
            | FormatId::Ogg
            | FormatId::Opus
            | FormatId::Aac
            | FormatId::M4a
    )
}

const fn is_markup_format(fmt: FormatId) -> bool {
    matches!(
        fmt,
        FormatId::Markdown | FormatId::Html | FormatId::PlainText
    )
}

const fn is_structured_format(fmt: FormatId) -> bool {
    matches!(
        fmt,
        FormatId::Csv | FormatId::Json | FormatId::Yaml | FormatId::Xml
    )
}

#[cfg(feature = "native-process")]
const fn is_video_format(fmt: FormatId) -> bool {
    matches!(
        fmt,
        FormatId::Mp4 | FormatId::Mov | FormatId::Mkv | FormatId::Webm | FormatId::Avi
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::error::Error;
    use std::fs;
    use std::io::Write;
    use std::time::{SystemTime, UNIX_EPOCH};

    use compression_archive::ArchiveInputEntry;
    use file_core::{CollisionPolicy, ResourceLimits, inspect_bytes, inspect_path};

    fn unique_temp_dir(label: &str) -> std::path::PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |d| d.as_nanos());
        let dir = std::env::temp_dir().join(format!("ff_cnv_{label}_{nanos}"));
        let _ = fs::create_dir_all(&dir);
        dir
    }

    fn sample_png_bytes() -> Vec<u8> {
        let mut raw = Vec::with_capacity(16 * (1 + 16 * 4));
        for y in 0..16u8 {
            raw.push(0);
            for x in 0..16u8 {
                raw.extend_from_slice(&[x.wrapping_mul(12), y.wrapping_mul(12), 200, 255]);
            }
        }
        // Minimal zlib stream wrapping via compression-archive Gzip? No: write valid stored zlib!
        let mut zlib = Vec::new();
        let len_u16 = u16::try_from(raw.len()).unwrap_or(0);
        zlib.extend_from_slice(&[0x78, 0x01, 0x01]);
        zlib.extend_from_slice(&len_u16.to_le_bytes());
        zlib.extend_from_slice(&(!len_u16).to_le_bytes());
        zlib.extend_from_slice(&raw);
        let (mut s1, mut s2) = (1u32, 0u32);
        for &b in &raw {
            s1 = (s1 + u32::from(b)) % 65521;
            s2 = (s2 + s1) % 65521;
        }
        zlib.extend_from_slice(&((s2 << 16) | s1).to_be_bytes());

        let mut png = Vec::from(b"\x89PNG\r\n\x1a\n".as_slice());
        let crc32 = |data: &[u8]| -> u32 {
            let mut crc = 0xFFFF_FFFFu32;
            for &byte in data {
                crc ^= u32::from(byte);
                for _ in 0..8 {
                    let mask = (crc & 1).wrapping_neg();
                    crc = (crc >> 1) ^ (0xEDB8_8320 & mask);
                }
            }
            !crc
        };
        let mut push = |tag: [u8; 4], body: &[u8]| {
            png.extend_from_slice(&(u32::try_from(body.len()).unwrap_or(0)).to_be_bytes());
            png.extend_from_slice(&tag);
            png.extend_from_slice(body);
            let mut c = Vec::with_capacity(4 + body.len());
            c.extend_from_slice(&tag);
            c.extend_from_slice(body);
            png.extend_from_slice(&crc32(&c).to_be_bytes());
        };
        let mut ihdr = Vec::with_capacity(13);
        ihdr.extend_from_slice(&16u32.to_be_bytes());
        ihdr.extend_from_slice(&16u32.to_be_bytes());
        ihdr.extend_from_slice(&[8, 6, 0, 0, 0]);
        push(*b"IHDR", &ihdr);
        push(*b"IDAT", &zlib);
        push(*b"IEND", &[]);
        png
    }

    #[test]
    fn graph_prefers_direct_lossless_and_detects_preflight_warnings() -> Result<(), Box<dyn Error>>
    {
        let graph = ConversionGraph::new_baseline();
        let wav_to_flac = graph.resolve_route_plan(FormatId::Wav, FormatId::Flac, false)?;
        assert_eq!(wav_to_flac.hops.len(), 1);
        assert!(wav_to_flac.is_lossless_chain);

        // Tier 2 disabled must cleanly block Video -> Opus multi-hop routes.
        assert!(
            graph
                .resolve_route_plan(FormatId::Mp4, FormatId::Opus, false)
                .is_err()
        );
        // Tier 2 enabled resolves Video -> Opus directly.
        let mp4_opus = graph.resolve_route_plan(FormatId::Mp4, FormatId::Opus, true)?;
        assert_eq!(
            mp4_opus.required_tier2_tools,
            vec![String::from("tier2-ffmpeg")]
        );

        // Pre-flight surfaces FID-ALPHA-DROP on PNG -> JPEG & FID-XLSX-FORMULAS on XLSX -> CSV.
        let engine = UniversalConversionEngine::new_tier1();
        let png_rep = inspect_bytes(&sample_png_bytes(), Some("ui.png"), false)?;
        let alpha_warn = engine.preflight(&png_rep, FormatId::Jpeg)?;
        assert!(alpha_warn.supported);
        assert!(
            alpha_warn
                .warnings
                .iter()
                .any(|w| w.code == "FID-ALPHA-DROP")
        );

        let mut xlsx_rep = png_rep;
        xlsx_rep.detected_format = FormatId::Xlsx;
        let xlsx_pre = engine.preflight(&xlsx_rep, FormatId::Csv)?;
        assert!(
            xlsx_pre
                .warnings
                .iter()
                .any(|w| w.code == "FID-XLSX-FORMULAS"
                    && w.severity == FidelitySeverity::BreakingLoss)
        );
        Ok(())
    }

    #[test]
    fn analyzes_markdown_and_html_structural_fidelity() {
        let md =
            "# Title\n\n| A | B |\n| --- | --- |\n\n[Link](https://example.org) ![Pic](a.png)\n";
        let html = "<h1>Title</h1><table><tr><th>A</th></tr></table><a href=\"x\">L</a><img src=\"a.png\">";
        let md_inv = analyze_markup_structure(md, FormatId::Markdown);
        let html_inv = analyze_markup_structure(html, FormatId::Html);
        assert_eq!(md_inv.headings, 1);
        assert_eq!(md_inv.tables, 1);
        assert_eq!(md_inv.links, 1);
        assert_eq!(md_inv.images, 1);
        assert_eq!(md_inv, html_inv);

        let report = build_fidelity_report(
            FormatId::Markdown,
            FormatId::Html,
            Vec::new(),
            Some(md_inv),
            Some(html_inv),
            None,
        );
        assert_eq!(report.headings_preserved, Some(1));
        assert_eq!(report.tables_preserved, Some(1));
    }

    #[test]
    fn converts_raster_and_archive_cross_formats_with_fidelity_reports()
    -> Result<(), Box<dyn Error>> {
        let dir = unique_temp_dir("media");
        let src_png = dir.join("src.png");
        let dst_webp = dir.join("out.webp");
        fs::write(&src_png, sample_png_bytes())?;

        let source = ValidatedSource {
            path: src_png.clone(),
            inspection: inspect_path(&src_png, false)?,
        };
        let ctx = JobContext::new("cnv-img", ResourceLimits::default());
        let engine = UniversalConversionEngine::new_tier1();
        let staged = StagedPath::create_sibling(&dst_webp)?;
        let req = ConversionRequest {
            target_format: FormatId::Webp,
            strip_metadata: true,
            preserve_structure: true,
            fail_on_breaking_loss: false,
        };

        let fidelity = engine.convert(&source, &staged, &req, &ctx)?;
        assert_eq!(fidelity.source_format, FormatId::Png);
        assert_eq!(fidelity.target_format, FormatId::Webp);
        assert!(
            fidelity
                .quality
                .and_then(|q| q.ssim)
                .is_some_and(|s| s > 0.99)
        );
        let promoted = staged.promote(CollisionPolicy::OverwriteWithBackup)?;
        assert_eq!(
            inspect_path(&promoted, false)?.detected_format,
            FormatId::Webp
        );

        // Archive cross-conversion: ZIP -> TAR.ZST with bit-exact payload preservation.
        let src_zip = dir.join("pack.zip");
        let dst_zst = dir.join("pack.tar.zst");
        let ctx_arc = JobContext::new("cnv-arc", ResourceLimits::default());
        create_archive_from_entries(
            &[ArchiveInputEntry {
                path: String::from("hello.txt"),
                data: b"FileFusion universal archive conversion!".to_vec(),
            }],
            &src_zip,
            ArchiveContainerKind::Zip,
            CompressionProfile::BalancedOptimal,
            &ctx_arc,
        )?;
        let arc_src = ValidatedSource {
            path: src_zip.clone(),
            inspection: inspect_path(&src_zip, false)?,
        };
        let staged_zst = StagedPath::create_sibling(&dst_zst)?;
        let arc_req = ConversionRequest {
            target_format: FormatId::Zstd,
            strip_metadata: false,
            preserve_structure: true,
            fail_on_breaking_loss: true,
        };
        let arc_fid = engine.convert(&arc_src, &staged_zst, &arc_req, &ctx_arc)?;
        assert_eq!(
            arc_fid.quality.and_then(|q| q.bit_exact_lossless),
            Some(true)
        );
        let _ = fs::remove_dir_all(&dir);
        Ok(())
    }

    #[test]
    fn converts_wav_to_flac_cross_format_losslessly() -> Result<(), Box<dyn Error>> {
        let dir = unique_temp_dir("audio");
        let src_wav = dir.join("tone.wav");
        let dst_flac = dir.join("tone.flac");

        // Write valid 16-bit mono 8kHz RIFF WAVE PCM container.
        let samples: Vec<i16> = (0..1600)
            .map(|i| i16::try_from(((i * 97) % 10_000) - 5_000).unwrap_or(0))
            .collect();
        let data_bytes = u32::try_from(samples.len() * 2).unwrap_or(0);
        let mut wav = Vec::new();
        wav.extend_from_slice(b"RIFF");
        wav.extend_from_slice(&(36 + data_bytes).to_le_bytes());
        wav.extend_from_slice(b"WAVEfmt ");
        wav.extend_from_slice(&16u32.to_le_bytes());
        wav.extend_from_slice(&1u16.to_le_bytes()); // PCM
        wav.extend_from_slice(&1u16.to_le_bytes()); // mono
        wav.extend_from_slice(&8000u32.to_le_bytes());
        wav.extend_from_slice(&16000u32.to_le_bytes());
        wav.extend_from_slice(&2u16.to_le_bytes());
        wav.extend_from_slice(&16u16.to_le_bytes());
        wav.extend_from_slice(b"data");
        wav.extend_from_slice(&data_bytes.to_le_bytes());
        for s in samples {
            wav.extend_from_slice(&s.to_le_bytes());
        }
        let mut wav_file = fs::File::create(&src_wav)?;
        wav_file.write_all(&wav)?;

        let source = ValidatedSource {
            path: src_wav.clone(),
            inspection: inspect_path(&src_wav, false)?,
        };
        let ctx = JobContext::new("cnv-aud", ResourceLimits::default());
        let engine = UniversalConversionEngine::new_tier1();
        let staged = StagedPath::create_sibling(&dst_flac)?;
        let req = ConversionRequest {
            target_format: FormatId::Flac,
            strip_metadata: true,
            preserve_structure: true,
            fail_on_breaking_loss: true,
        };
        let fid = engine.convert(&source, &staged, &req, &ctx)?;
        assert_eq!(fid.source_format, FormatId::Wav);
        assert_eq!(fid.target_format, FormatId::Flac);
        assert_eq!(fid.quality.and_then(|q| q.bit_exact_lossless), Some(true));

        let _ = fs::remove_dir_all(&dir);
        Ok(())
    }

    #[test]
    fn converts_markdown_html_plaintext_with_xss_sanitization_and_fidelity()
    -> Result<(), Box<dyn Error>> {
        let dir = unique_temp_dir("markup");
        let md_path = dir.join("guide.md");
        let html_path = dir.join("guide.html");
        let md_roundtrip_path = dir.join("guide_rt.md");
        let txt_path = dir.join("guide.txt");

        let md_sample = "# Project Spec\n\nIntro paragraph with **bold** and *italic*.\n\n| Key | Value |\n| --- | --- |\n| Engine | FileFusion |\n\n- Alpha item\n- Beta item\n\n[Docs](https://example.org) ![Logo](logo.png)\n";
        fs::write(&md_path, md_sample)?;

        let engine = UniversalConversionEngine::new_tier1();
        let ctx = JobContext::new("cnv-markup", ResourceLimits::default());
        let src_md = ValidatedSource {
            path: md_path.clone(),
            inspection: inspect_path(&md_path, false)?,
        };

        // Markdown -> HTML preserves headings, tables, links, images.
        let staged_html = StagedPath::create_sibling(&html_path)?;
        let req_html = ConversionRequest {
            target_format: FormatId::Html,
            strip_metadata: true,
            preserve_structure: true,
            fail_on_breaking_loss: true,
        };
        let fid_html = engine.convert(&src_md, &staged_html, &req_html, &ctx)?;
        assert_eq!(fid_html.headings_preserved, Some(1));
        assert_eq!(fid_html.tables_preserved, Some(1));
        assert_eq!(fid_html.links_preserved, Some(1));
        assert_eq!(fid_html.images_preserved, Some(1));
        let promoted_html = staged_html.promote(CollisionPolicy::OverwriteWithBackup)?;
        let html_body = fs::read_to_string(&promoted_html)?;
        assert!(html_body.starts_with("<!DOCTYPE html>"));
        assert!(!html_body.contains("name=\"generator\""));

        // Inject XSS payloads into HTML and verify HTML -> Markdown strips script/handlers/javascript URLs.
        let dirty_html = "<!DOCTYPE html><html><body><h1>Safe Title</h1><script>alert(1)</script><p onclick=\"evil()\">Hello</p><a href=\"javascript:alert(2)\">Bad</a><a href=\"https://ok.test\">Good</a></body></html>";
        let dirty_html_path = dir.join("dirty.html");
        fs::write(&dirty_html_path, dirty_html)?;
        let src_dirty = ValidatedSource {
            path: dirty_html_path.clone(),
            inspection: inspect_path(&dirty_html_path, false)?,
        };
        let staged_rt = StagedPath::create_sibling(&md_roundtrip_path)?;
        let req_md = ConversionRequest {
            target_format: FormatId::Markdown,
            strip_metadata: false,
            preserve_structure: true,
            fail_on_breaking_loss: true,
        };
        let fid_rt = engine.convert(&src_dirty, &staged_rt, &req_md, &ctx)?;
        assert_eq!(fid_rt.headings_preserved, Some(1));
        let promoted_rt = staged_rt.promote(CollisionPolicy::OverwriteWithBackup)?;
        let rt_text = fs::read_to_string(&promoted_rt)?;
        assert!(!rt_text.contains("alert("));
        assert!(!rt_text.contains("javascript:"));
        assert!(rt_text.contains("# Safe Title"));
        assert!(rt_text.contains("[Good](https://ok.test)"));

        // Markdown -> PlainText surfaces FID-MARKUP-PLAINTEXT warning.
        let staged_txt = StagedPath::create_sibling(&txt_path)?;
        let req_txt = ConversionRequest {
            target_format: FormatId::PlainText,
            strip_metadata: true,
            preserve_structure: false,
            fail_on_breaking_loss: true,
        };
        let fid_txt = engine.convert(&src_md, &staged_txt, &req_txt, &ctx)?;
        assert!(
            fid_txt
                .warnings
                .iter()
                .any(|w| w.code == "FID-MARKUP-PLAINTEXT")
        );
        let promoted_txt = staged_txt.promote(CollisionPolicy::OverwriteWithBackup)?;
        let txt_out = fs::read_to_string(&promoted_txt)?;
        assert!(txt_out.contains("Project Spec"));
        assert!(txt_out.contains("Engine\tFileFusion"));

        let _ = fs::remove_dir_all(&dir);
        Ok(())
    }

    #[test]
    fn converts_csv_json_yaml_xml_roundtrip_and_blocks_xxe() -> Result<(), Box<dyn Error>> {
        let dir = unique_temp_dir("tabular");
        let csv_path = dir.join("users.csv");
        let json_path = dir.join("users.json");
        let yaml_path = dir.join("users.yaml");
        let xml_path = dir.join("users.xml");
        let md_path = dir.join("users.md");
        let csv_rt_path = dir.join("users_rt.csv");

        fs::write(
            &csv_path,
            "id,name,active\n1,\"Alice, A.\",true\n2,Bob,false\n",
        )?;

        let engine = UniversalConversionEngine::new_tier1();
        let ctx = JobContext::new("cnv-tab", ResourceLimits::default());
        let run_step =
            |src_path: &std::path::Path,
             dst_path: &std::path::Path,
             target: FormatId|
             -> Result<(std::path::PathBuf, ConversionFidelityReport), Box<dyn Error>> {
                let src = ValidatedSource {
                    path: src_path.to_path_buf(),
                    inspection: inspect_path(src_path, false)?,
                };
                let staged = StagedPath::create_sibling(dst_path)?;
                let req = ConversionRequest {
                    target_format: target,
                    strip_metadata: false,
                    preserve_structure: true,
                    fail_on_breaking_loss: true,
                };
                let fid = engine.convert(&src, &staged, &req, &ctx)?;
                Ok((staged.promote(CollisionPolicy::OverwriteWithBackup)?, fid))
            };

        let (promoted_json, _) = run_step(&csv_path, &json_path, FormatId::Json)?;
        assert!(fs::read_to_string(&promoted_json)?.contains("\"Alice, A.\""));

        let (promoted_yaml, _) = run_step(&promoted_json, &yaml_path, FormatId::Yaml)?;
        let (promoted_xml, _) = run_step(&promoted_yaml, &xml_path, FormatId::Xml)?;
        let (promoted_csv_rt, fid_csv) = run_step(&promoted_xml, &csv_rt_path, FormatId::Csv)?;
        assert!(fid_csv.warnings.iter().any(|w| w.code == "FID-CSV-FLATTEN"));
        let rt_csv_text = fs::read_to_string(&promoted_csv_rt)?;
        assert!(rt_csv_text.contains("active,id,name"));
        assert!(rt_csv_text.contains("\"Alice, A.\""));
        assert!(rt_csv_text.contains("Bob"));

        let (_promoted_md, fid_md) = run_step(&csv_path, &md_path, FormatId::Markdown)?;
        assert_eq!(fid_md.tables_preserved, Some(1));

        // XXE / Billion Laughs ENTITY bomb must be blocked cleanly (`SecBomb`).
        let xxe =
            "<?xml version=\"1.0\"?><!DOCTYPE lolz [<!ENTITY lol \"lol\">]><root>&lol;</root>";
        let xxe_err = tabular::parse_structured_value(xxe, FormatId::Xml)
            .err()
            .map(|e| e.diagnostic.code);
        assert_eq!(xxe_err, Some(ErrorCode::SecBomb));

        let _ = fs::remove_dir_all(&dir);
        Ok(())
    }

    #[test]
    fn converts_xlsx_ods_csv_json_and_docx_odt_epub_natively() -> Result<(), Box<dyn Error>> {
        let dir = unique_temp_dir("office_epub");
        let csv_path = dir.join("sales.csv");
        let xlsx_path = dir.join("sales.xlsx");
        let ods_path = dir.join("sales.ods");
        let json_path = dir.join("sales.json");
        fs::write(&csv_path, "item,qty\nWidget,42\nGadget,15\n")?;

        let engine = UniversalConversionEngine::new_tier1();
        let ctx = JobContext::new("cnv-off", ResourceLimits::default());
        let convert_file =
            |src_p: &std::path::Path,
             dst_p: &std::path::Path,
             target: FormatId,
             fail_loss: bool|
             -> Result<(std::path::PathBuf, ConversionFidelityReport), Box<dyn Error>> {
                let src = ValidatedSource {
                    path: src_p.to_path_buf(),
                    inspection: inspect_path(src_p, false)?,
                };
                let staged = StagedPath::create_sibling(dst_p)?;
                let req = ConversionRequest {
                    target_format: target,
                    strip_metadata: true,
                    preserve_structure: true,
                    fail_on_breaking_loss: fail_loss,
                };
                let fid = engine.convert(&src, &staged, &req, &ctx)?;
                Ok((staged.promote(CollisionPolicy::OverwriteWithBackup)?, fid))
            };

        // CSV -> XLSX -> ODS -> JSON
        let (xlsx_out, _) = convert_file(&csv_path, &xlsx_path, FormatId::Xlsx, true)?;
        assert_eq!(
            inspect_path(&xlsx_out, false)?.detected_format,
            FormatId::Xlsx
        );
        let (ods_out, _) = convert_file(&xlsx_out, &ods_path, FormatId::Ods, true)?;
        assert_eq!(
            inspect_path(&ods_out, false)?.detected_format,
            FormatId::Ods
        );
        let (json_out, fid_ods_json) = convert_file(&ods_out, &json_path, FormatId::Json, false)?;
        assert!(
            fid_ods_json
                .warnings
                .iter()
                .any(|w| w.code == "FID-XLSX-FORMULAS")
        );
        let json_txt = fs::read_to_string(&json_out)?;
        assert!(json_txt.contains("Widget"));
        assert!(json_txt.contains("42"));

        // Markdown -> DOCX -> EPUB -> Markdown -> ODT
        let md_path = dir.join("book.md");
        let docx_path = dir.join("book.docx");
        let epub_path = dir.join("book.epub");
        let rt_md_path = dir.join("book_rt.md");
        let odt_path = dir.join("book.odt");
        fs::write(
            &md_path,
            "# Handbook\n\nBody paragraph.\n\n| Col | Val |\n| --- | --- |\n| A | 1 |\n\n[Home](https://example.org)\n",
        )?;

        let (docx_out, fid_docx) = convert_file(&md_path, &docx_path, FormatId::Docx, true)?;
        assert_eq!(
            inspect_path(&docx_out, false)?.detected_format,
            FormatId::Docx
        );
        assert_eq!(fid_docx.headings_preserved, Some(1));
        assert_eq!(fid_docx.tables_preserved, Some(1));

        let (epub_out, fid_epub) = convert_file(&docx_out, &epub_path, FormatId::Epub, true)?;
        assert_eq!(
            inspect_path(&epub_out, false)?.detected_format,
            FormatId::Epub
        );
        assert_eq!(fid_epub.headings_preserved, Some(1));

        let (rt_md, _) = convert_file(&epub_out, &rt_md_path, FormatId::Markdown, true)?;
        let rt_text = fs::read_to_string(&rt_md)?;
        assert!(rt_text.contains("# Handbook"));
        assert!(rt_text.contains("Body paragraph."));

        let (writer_odt_out, _) = convert_file(&rt_md, &odt_path, FormatId::Odt, true)?;
        assert_eq!(
            inspect_path(&writer_odt_out, false)?.detected_format,
            FormatId::Odt
        );

        let _ = fs::remove_dir_all(&dir);
        Ok(())
    }

    #[cfg(feature = "native-process")]
    struct StubDocRunner {
        recorded: std::sync::Mutex<Vec<Vec<String>>>,
    }

    #[cfg(feature = "native-process")]
    impl IsolatedProcessRunner for StubDocRunner {
        fn run_isolated(
            &self,
            invocation: &file_core::ProcessInvocation,
            _ctx: &JobContext,
        ) -> FileFusionResult<file_core::ProcessOutput> {
            let strs: Vec<String> = invocation
                .args
                .iter()
                .map(|a| a.to_string_lossy().into_owned())
                .collect();
            if strs.iter().any(|s| s == "--convert-to") {
                if let Some(out_idx) = strs.iter().position(|s| s == "--outdir") {
                    let out_dir = std::path::PathBuf::from(&strs[out_idx + 1]);
                    let _ = fs::write(
                        out_dir.join("sample.pdf"),
                        b"%PDF-1.7\n1 0 obj<<>>endobj\ntrailer<<>>\n%%EOF\n",
                    );
                }
            } else if let Some(last) = strs.last() {
                if std::path::Path::new(last)
                    .extension()
                    .is_some_and(|e| e.eq_ignore_ascii_case("mobi"))
                {
                    let _ = fs::write(last, b"BOOKMOBI_STUB_PAYLOAD");
                } else if let Some(pos) = strs.iter().position(|s| s == "-o") {
                    let _ = fs::write(&strs[pos + 1], b"# From Pandoc\n\nParagraph.\n");
                }
            }
            if let Ok(mut guard) = self.recorded.lock() {
                guard.push(strs);
            }
            Ok(file_core::ProcessOutput {
                exit_code: 0,
                stdout: Vec::new(),
                stderr: Vec::new(),
                duration_ms: 5,
                timed_out: false,
            })
        }
    }

    #[cfg(feature = "native-process")]
    #[test]
    fn tier2_adapters_build_sanitized_invocations_and_dispatch_via_mock_runner()
    -> Result<(), Box<dyn Error>> {
        let dir = unique_temp_dir("tier2_docs");
        let docx_path = dir.join("sample.docx");
        let epub_path = dir.join("sample.epub");
        let pdf_path = dir.join("sample.pdf");
        let ctx = JobContext::new("cnv-t2", ResourceLimits::default());
        let minimal_md = b"# Sample\n\nHello\n";
        fs::write(
            &docx_path,
            office_epub::convert_document_container(
                minimal_md,
                FormatId::Markdown,
                FormatId::Docx,
                true,
                &ctx.limits,
                &ctx,
            )?,
        )?;
        fs::write(
            &epub_path,
            office_epub::convert_document_container(
                minimal_md,
                FormatId::Markdown,
                FormatId::Epub,
                true,
                &ctx.limits,
                &ctx,
            )?,
        )?;

        let runner = StubDocRunner {
            recorded: std::sync::Mutex::new(Vec::new()),
        };
        let src = ValidatedSource {
            path: docx_path.clone(),
            inspection: inspect_path(&docx_path, false)?,
        };
        let staged_pdf = StagedPath::create_sibling(&pdf_path)?;
        let req_pdf = ConversionRequest {
            target_format: FormatId::Pdf,
            strip_metadata: true,
            preserve_structure: true,
            fail_on_breaking_loss: false,
        };
        assert_eq!(
            UniversalConversionEngine::new_tier1()
                .convert_with_optional_runner(&src, &staged_pdf, &req_pdf, Some(&runner), &ctx)
                .err()
                .map(|e| e.diagnostic.code),
            Some(ErrorCode::EngMissing)
        );

        let with_tools = UniversalConversionEngine::with_tier2_binaries(
            None,
            None,
            Some(PathBuf::from("soffice")),
            Some(PathBuf::from("pandoc")),
            Some(PathBuf::from("ebook-convert")),
        );
        let run_t2 = |in_path: &std::path::Path,
                      out_name: &str,
                      target: FormatId|
         -> Result<std::path::PathBuf, Box<dyn Error>> {
            let v_src = ValidatedSource {
                path: in_path.to_path_buf(),
                inspection: inspect_path(in_path, false)?,
            };
            let staged = StagedPath::create_sibling(dir.join(out_name))?;
            let req = ConversionRequest {
                target_format: target,
                strip_metadata: true,
                preserve_structure: true,
                fail_on_breaking_loss: false,
            };
            let _ = with_tools.convert_with_optional_runner(
                &v_src,
                &staged,
                &req,
                Some(&runner),
                &ctx,
            )?;
            Ok(staged.promote(CollisionPolicy::OverwriteWithBackup)?)
        };

        let promoted_pdf = run_t2(&docx_path, "sample.pdf", FormatId::Pdf)?;
        assert_eq!(
            inspect_path(&promoted_pdf, false)?.detected_format,
            FormatId::Pdf
        );
        let rtf_path = dir.join("note.rtf");
        fs::write(&rtf_path, b"{\\rtf1\\ansi Hello}")?;
        let _ = run_t2(&rtf_path, "note.md", FormatId::Markdown)?;
        let _ = run_t2(&epub_path, "book.mobi", FormatId::Mobi)?;

        let recorded_len = runner.recorded.lock().map_or(0, |g| g.len());
        assert_eq!(recorded_len, 3);

        let _ = fs::remove_dir_all(&dir);
        Ok(())
    }
}
