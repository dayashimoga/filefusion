//! `conversion-core`: L4 universal conversion graph, pre-flight feature-loss warnings,
//! structural fidelity reporting, and cross-media conversion execution
//! (`FR-DOC-001`..`003`, `FR-AV-003`, `FR-FID-001`..`002`, `FF-MOD-001` §4).

#![forbid(unsafe_code)]

pub mod graph;
pub mod preflight;

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
    PreflightAssessment, QualityEvaluator, StagedPath, ValidatedSource,
};
#[cfg(feature = "native-process")]
use std::path::PathBuf;

#[cfg(feature = "native-process")]
use validation_engine::MagicByteOutputValidator;

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
        }
    }

    /// Exposes the underlying [`ConversionGraph`] for multi-hop route planning.
    #[must_use]
    pub const fn graph(&self) -> &ConversionGraph {
        &self.graph
    }

    /// Executes conversion using an optional Tier 2 [`IsolatedProcessRunner`] when the route
    /// traverses video containers, video-to-audio extraction, or Tier 2 lossy audio codecs.
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

        Err(FileFusionError::new(
            ErrorCode::FmtUnsupported,
            format!(
                "conversion route {source:?} -> {target:?} requires Tier 2 engine {:?} (not yet dispatched)",
                plan.required_tier2_tools
            ),
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
                let source_kind =
                    ArchiveContainerKind::detect_from_path_with_limits(&input.path, &ctx.limits)?;
                let entries =
                    read_archive_entries_checked(&input.path, source_kind, &ctx.limits, ctx)?;
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
                return Ok(build_fidelity_report(
                    source,
                    target,
                    assessment.warnings.clone(),
                    None,
                    None,
                    Some(quality),
                ));
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

        Err(FileFusionError::new(
            ErrorCode::FmtUnsupported,
            format!("no pure-Rust Tier 1 converter active yet for {source:?} -> {target:?}"),
        ))
    }
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
}
