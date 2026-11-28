//! Unified Stage 2 L3 compression engine orchestration & routing (`FF-MOD-001` §4).

use compression_archive::ArchiveCompressionEngine;
use compression_audio::AudioCompressionEngine;
use compression_core::effective_target_format;
use compression_image::RasterCompressionEngine;
use compression_office::OfficeCompressionEngine;
use compression_pdf::PdfCompressionEngine;
use compression_video::VideoCompressionEngine;
use file_core::{
    CompressionEngine, CompressionOutcome, CompressionRequest, ErrorCode, FileFusionError,
    FileFusionResult, FormatId, JobContext, StagedPath, ValidatedSource,
};

#[cfg(feature = "native-process")]
use std::path::PathBuf;

#[cfg(feature = "native-process")]
use compression_video::VideoCompressionOptions;
#[cfg(feature = "native-process")]
use file_core::IsolatedProcessRunner;

/// Unified L4 compression dispatcher coordinating all six L3 format engines cleanly.
#[derive(Debug, Clone, Default)]
pub struct OptimizationDispatcher {
    #[cfg(feature = "native-process")]
    ffmpeg: Option<PathBuf>,
    #[cfg(feature = "native-process")]
    ffprobe: Option<PathBuf>,
    #[cfg(feature = "native-process")]
    qpdf: Option<PathBuf>,
}

impl OptimizationDispatcher {
    /// Creates a pure-Rust Tier 1 dispatcher supporting `Image`, `Archive`, `Audio` (`WAV`/`FLAC`),
    /// `PDF`, and `Office`/`EPUB` natively across Desktop, Mobile, CLI, and WASM targets.
    #[must_use]
    pub const fn new_tier1() -> Self {
        Self {
            #[cfg(feature = "native-process")]
            ffmpeg: None,
            #[cfg(feature = "native-process")]
            ffprobe: None,
            #[cfg(feature = "native-process")]
            qpdf: None,
        }
    }

    /// Configures verified Tier 2 external binaries (`ffmpeg`, `ffprobe`, `qpdf`) for native runs.
    #[cfg(feature = "native-process")]
    #[must_use]
    pub fn with_tier2_binaries(
        ffmpeg: Option<PathBuf>,
        ffprobe: Option<PathBuf>,
        qpdf: Option<PathBuf>,
    ) -> Self {
        Self {
            ffmpeg,
            ffprobe,
            qpdf,
        }
    }

    /// Executes compression using the appropriate L3 format engine for `input` / `req`, using
    /// an optional Tier 2 [`IsolatedProcessRunner`] for external video/PDF accelerators.
    ///
    /// # Errors
    /// Returns [`FileFusionError`] if format is unsupported, required Tier 2 runner is absent,
    /// validation fails, or job is cancelled.
    #[cfg(feature = "native-process")]
    pub fn compress_with_optional_runner(
        &self,
        input: &ValidatedSource,
        staged_output: &StagedPath,
        req: &CompressionRequest,
        runner: Option<&dyn IsolatedProcessRunner>,
        ctx: &JobContext,
    ) -> FileFusionResult<CompressionOutcome> {
        let target = effective_target_format(input, req);
        if matches!(
            target,
            FormatId::Mp4 | FormatId::Mov | FormatId::Mkv | FormatId::Webm | FormatId::Avi
        ) {
            let Some(ffmpeg_bin) = &self.ffmpeg else {
                return Err(FileFusionError::new(
                    ErrorCode::EngMissing,
                    "FFmpeg binary not configured for video compression",
                ));
            };
            let Some(exec) = runner else {
                return Err(FileFusionError::new(
                    ErrorCode::EngMissing,
                    "IsolatedProcessRunner required for Tier 2 video compression",
                ));
            };
            let vid_engine = VideoCompressionEngine::with_ffmpeg_binaries(
                ffmpeg_bin.clone(),
                self.ffprobe.clone(),
            );
            let opts = VideoCompressionOptions {
                base: req.clone(),
                target_codec: None,
                crf_override: None,
                max_width: None,
                max_height: None,
                target_fps: None,
                allow_fast_remux: true,
                prefer_deterministic_cpu: false,
                hw_profile_override: None,
            };
            return vid_engine.compress_with_runner(input, staged_output, &opts, exec, ctx);
        }

        if target == FormatId::Pdf {
            if let (Some(qpdf_bin), Some(exec)) = (&self.qpdf, runner) {
                let pdf_engine = PdfCompressionEngine::with_qpdf_binary(qpdf_bin.clone());
                let opts = compression_pdf::PdfCompressionOptions::from_request(req.clone());
                return pdf_engine.compress_with_runner(
                    input,
                    staged_output,
                    &opts,
                    Some(exec),
                    ctx,
                );
            }
        }

        self.compress(input, staged_output, req, ctx)
    }
}

impl CompressionEngine for OptimizationDispatcher {
    fn engine_id(&self) -> &'static str {
        "l4-optimization-dispatcher"
    }

    fn supported_formats(&self) -> &[FormatId] {
        &[
            FormatId::Jpeg,
            FormatId::Png,
            FormatId::Webp,
            FormatId::Avif,
            FormatId::Gif,
            FormatId::Bmp,
            FormatId::Tiff,
            FormatId::Ico,
            FormatId::Zip,
            FormatId::Tar,
            FormatId::Gzip,
            FormatId::Zstd,
            FormatId::Xz,
            FormatId::Bzip2,
            FormatId::Lz4,
            FormatId::SevenZ,
            FormatId::Wav,
            FormatId::Flac,
            FormatId::Pdf,
            FormatId::Docx,
            FormatId::Xlsx,
            FormatId::Pptx,
            FormatId::Odt,
            FormatId::Ods,
            FormatId::Odp,
            FormatId::Epub,
            FormatId::Mp4,
            FormatId::Mov,
            FormatId::Mkv,
            FormatId::Webm,
            FormatId::Avi,
        ]
    }

    fn estimate_memory_mb(
        &self,
        input: &file_core::FileInspectionReport,
        req: &CompressionRequest,
    ) -> u64 {
        let target = req.target_format.unwrap_or(input.detected_format);
        match target {
            FormatId::Jpeg
            | FormatId::Png
            | FormatId::Webp
            | FormatId::Avif
            | FormatId::Gif
            | FormatId::Bmp
            | FormatId::Tiff
            | FormatId::Ico => RasterCompressionEngine.estimate_memory_mb(input, req),
            FormatId::Wav
            | FormatId::Flac
            | FormatId::Mp3
            | FormatId::Ogg
            | FormatId::Opus
            | FormatId::Aac
            | FormatId::M4a => AudioCompressionEngine::new_tier1().estimate_memory_mb(input, req),
            FormatId::Pdf => PdfCompressionEngine::new_tier1().estimate_memory_mb(input, req),
            FormatId::Docx
            | FormatId::Xlsx
            | FormatId::Pptx
            | FormatId::Odt
            | FormatId::Ods
            | FormatId::Odp
            | FormatId::Epub => OfficeCompressionEngine.estimate_memory_mb(input, req),
            FormatId::Mp4 | FormatId::Mov | FormatId::Mkv | FormatId::Webm | FormatId::Avi => {
                VideoCompressionEngine::new_unconfigured().estimate_memory_mb(input, req)
            }
            _ => ArchiveCompressionEngine.estimate_memory_mb(input, req),
        }
    }

    fn compress(
        &self,
        input: &ValidatedSource,
        staged_output: &StagedPath,
        req: &CompressionRequest,
        ctx: &JobContext,
    ) -> FileFusionResult<CompressionOutcome> {
        let target = effective_target_format(input, req);
        match target {
            FormatId::Jpeg
            | FormatId::Png
            | FormatId::Webp
            | FormatId::Avif
            | FormatId::Gif
            | FormatId::Bmp
            | FormatId::Tiff
            | FormatId::Ico => RasterCompressionEngine.compress(input, staged_output, req, ctx),
            FormatId::Zip
            | FormatId::Tar
            | FormatId::Gzip
            | FormatId::Zstd
            | FormatId::Xz
            | FormatId::Bzip2
            | FormatId::Lz4
            | FormatId::SevenZ => ArchiveCompressionEngine.compress(input, staged_output, req, ctx),
            FormatId::Wav | FormatId::Flac => {
                AudioCompressionEngine::new_tier1().compress(input, staged_output, req, ctx)
            }
            FormatId::Pdf => {
                PdfCompressionEngine::new_tier1().compress(input, staged_output, req, ctx)
            }
            FormatId::Docx
            | FormatId::Xlsx
            | FormatId::Pptx
            | FormatId::Odt
            | FormatId::Ods
            | FormatId::Odp
            | FormatId::Epub => OfficeCompressionEngine.compress(input, staged_output, req, ctx),
            FormatId::Mp4 | FormatId::Mov | FormatId::Mkv | FormatId::Webm | FormatId::Avi => {
                VideoCompressionEngine::new_unconfigured().compress(input, staged_output, req, ctx)
            }
            other => Err(FileFusionError::new(
                ErrorCode::FmtUnsupported,
                format!("no L3 compression engine registered for target format {other:?}"),
            )),
        }
    }
}
