//! `compression-video`: L3 Tier 2 `FFmpeg` video transcoding, hardware/CPU routing,
//! lossless container fast-path remuxing, subtitle parsing/conversion, and `libvmaf`
//! quality scoring (`FR-AV-001`, `FR-AV-003`, `FR-ADP-002`, `FR-CMP-001`..`002`).

#![forbid(unsafe_code)]

pub mod pipeline;
pub mod probe;
pub mod subtitles;

use std::path::Path;

#[cfg(feature = "native-process")]
use compression_core::{finalize_staged_compression, prescreen_incompressible_noop};
use file_core::{
    CompressionEngine, CompressionOutcome, CompressionRequest, ErrorCode, FileFusionError,
    FileFusionResult, FileInspectionReport, FormatId, JobContext, QualityEvaluator, QualityMetrics,
    StagedPath, ValidatedSource,
};
#[cfg(feature = "native-process")]
use hardware_acceleration::probe_hardware;
use quality_engine::BitExactQualityEvaluator;
use validation_engine::MagicByteOutputValidator;

#[cfg(feature = "native-process")]
use std::path::PathBuf;

#[cfg(feature = "native-process")]
use file_core::IsolatedProcessRunner;

pub use pipeline::{
    ResolvedVideoRoute, VideoCodec, VideoCompressionOptions,
    build_ffmpeg_container_repair_invocation, build_ffmpeg_video_transcode_invocation,
    build_ffmpeg_vmaf_invocation, ffmpeg_muxer_for_video_container, parse_ffmpeg_vmaf_score,
    resolve_video_route,
};
pub use probe::{
    MediaAudioStreamDescriptor, SubtitleStreamDescriptor, VideoContainerInventory,
    VideoStreamDescriptor, can_fast_path_remux, diagnose_unfinalized_video_container,
    parse_ffprobe_json_probe,
};
#[cfg(feature = "native-process")]
pub use probe::{build_ffprobe_invocation, probe_video_with_ffprobe};
#[cfg(feature = "native-process")]
pub use subtitles::build_ffmpeg_extract_subtitle_invocation;
pub use subtitles::{
    SubtitleCue, TextSubtitleFormat, convert_subtitle_text, format_cues_as_srt,
    format_cues_as_webvtt, parse_srt_cues, parse_webvtt_cues,
};

/// Validates staged video outputs using magic-byte signature probing (`FR-VAL-001`).
pub type VideoOutputValidator = MagicByteOutputValidator;

/// Evaluates video quality using empirical `libvmaf` (if active `FFmpeg` runner is supplied)
/// or explicit fast-path lossless stream confirmation / [`BitExactQualityEvaluator`].
#[derive(Default)]
pub struct VideoQualityEvaluator<'a> {
    /// `true` when lossless `-c:v copy -c:a copy` fast-path remuxing ran.
    pub is_lossless_remux: bool,
    /// Optional `FFmpeg` binary + runner for empirical VMAF scoring (`0.0..=100.0`).
    #[cfg(feature = "native-process")]
    pub vmaf_runner: Option<(PathBuf, &'a dyn IsolatedProcessRunner)>,
    #[cfg(not(feature = "native-process"))]
    _phantom: core::marker::PhantomData<&'a ()>,
}

impl QualityEvaluator for VideoQualityEvaluator<'_> {
    fn evaluator_id(&self) -> &'static str {
        "compression-video-vmaf"
    }

    fn evaluate(
        &self,
        original: &ValidatedSource,
        candidate: &Path,
        ctx: &JobContext,
    ) -> FileFusionResult<QualityMetrics> {
        ctx.check_cancelled()?;
        if self.is_lossless_remux {
            return Ok(QualityMetrics {
                ssim: Some(1.0),
                psnr_db: Some(99.9),
                vmaf: Some(100.0),
                bit_exact_lossless: Some(true),
            });
        }

        #[cfg(feature = "native-process")]
        if let Some((ffmpeg_bin, runner)) = &self.vmaf_runner {
            let inv = build_ffmpeg_vmaf_invocation(
                ffmpeg_bin,
                candidate,
                &original.path,
                ctx.limits.subprocess_timeout_secs,
            );
            if let Ok(out) = runner.run_isolated(&inv, ctx) {
                let text = format!(
                    "{}\n{}",
                    String::from_utf8_lossy(&out.stdout),
                    String::from_utf8_lossy(&out.stderr)
                );
                if let Some(vmaf) = parse_ffmpeg_vmaf_score(&text) {
                    return Ok(QualityMetrics {
                        ssim: None,
                        psnr_db: None,
                        vmaf: Some(vmaf),
                        bit_exact_lossless: Some(false),
                    });
                }
            }
        }

        BitExactQualityEvaluator.evaluate(original, candidate, ctx)
    }
}

/// L3 Tier 2 `FFmpeg` video compression engine with hardware/CPU routing and fast-path remuxing
/// (`FR-AV-001`, `FR-ADP-002`, `FR-CMP-002`).
#[derive(Debug, Clone, Default)]
pub struct VideoCompressionEngine {
    #[cfg(feature = "native-process")]
    ffmpeg_binary: Option<PathBuf>,
    #[cfg(feature = "native-process")]
    ffprobe_binary: Option<PathBuf>,
}

impl VideoCompressionEngine {
    /// Creates an engine descriptor without external `FFmpeg` binaries configured.
    #[must_use]
    pub const fn new_unconfigured() -> Self {
        Self {
            #[cfg(feature = "native-process")]
            ffmpeg_binary: None,
            #[cfg(feature = "native-process")]
            ffprobe_binary: None,
        }
    }

    /// Configures verified Tier 2 `ffmpeg` and optional `ffprobe` executable paths (`MOD-RULE-05`).
    #[cfg(feature = "native-process")]
    #[must_use]
    pub fn with_ffmpeg_binaries(ffmpeg_binary: PathBuf, ffprobe_binary: Option<PathBuf>) -> Self {
        Self {
            ffmpeg_binary: Some(ffmpeg_binary),
            ffprobe_binary,
        }
    }

    /// Compresses or fast-path remuxes `input` to `staged_output` via [`IsolatedProcessRunner`]
    /// enforcing `JobContext` memory reservations and `FR-CMP-002` No-Op Guard.
    ///
    /// # Errors
    /// Returns [`FileFusionError`] if `ffmpeg` is missing (`EngMissing`), subprocess fails,
    /// container validation fails, or cancellation triggers.
    #[cfg(feature = "native-process")]
    pub fn compress_with_runner(
        &self,
        input: &ValidatedSource,
        staged_output: &StagedPath,
        opts: &VideoCompressionOptions,
        runner: &dyn IsolatedProcessRunner,
        ctx: &JobContext,
    ) -> FileFusionResult<CompressionOutcome> {
        ctx.check_cancelled()?;
        let Some(ffmpeg_bin) = &self.ffmpeg_binary else {
            return Err(FileFusionError::new(
                ErrorCode::EngMissing,
                "Tier 2 ffmpeg executable must be configured for video compression",
            ));
        };

        let req = &opts.base;
        let target_format = req
            .target_format
            .unwrap_or(input.inspection.detected_format);
        let no_transforms =
            opts.max_width.is_none() && opts.max_height.is_none() && opts.target_fps.is_none();

        if no_transforms && opts.crf_override.is_none() {
            if let Some(noop) = prescreen_incompressible_noop(input, req, self.engine_id()) {
                return Ok(noop);
            }
        }

        let est_mb = self.estimate_memory_mb(&input.inspection, req);
        let _mem = ctx.reserve_working_set_mb(est_mb, "video_compression")?;

        let remux_eligible = if opts.allow_fast_remux {
            if let Some(ffprobe_bin) = &self.ffprobe_binary {
                probe_video_with_ffprobe(runner, ffprobe_bin, &input.path, ctx)
                    .is_ok_and(|inv| can_fast_path_remux(&inv, target_format))
            } else {
                false
            }
        } else {
            false
        };

        let hw_profile = opts
            .hw_profile_override
            .clone()
            .unwrap_or_else(probe_hardware);
        let route = resolve_video_route(target_format, opts, &hw_profile, remux_eligible)?;
        let invocation = build_ffmpeg_video_transcode_invocation(
            ffmpeg_bin,
            &input.path,
            staged_output.staged_path(),
            target_format,
            opts,
            &route,
            ctx.limits.subprocess_timeout_secs,
        )?;
        let output = runner.run_isolated(&invocation, ctx)?;
        if output.exit_code != 0 {
            return Err(FileFusionError::new(
                ErrorCode::IntInvariant,
                format!(
                    "tier2 ffmpeg video job exited with code {}: {}",
                    output.exit_code,
                    String::from_utf8_lossy(&output.stderr).trim()
                ),
            ));
        }

        let validator = MagicByteOutputValidator;
        let evaluator = VideoQualityEvaluator {
            is_lossless_remux: route.fast_path_remux,
            vmaf_runner: None,
        };
        let codec_used = format!("tier2-ffmpeg-{}", route.ffmpeg_encoder);
        finalize_staged_compression(
            input,
            staged_output,
            req,
            &validator,
            Some(&evaluator),
            codec_used,
            ctx,
        )
    }
}

impl CompressionEngine for VideoCompressionEngine {
    fn engine_id(&self) -> &'static str {
        "compression-video-tier2-ffmpeg"
    }

    fn supported_formats(&self) -> &[FormatId] {
        &[
            FormatId::Mp4,
            FormatId::Mov,
            FormatId::Mkv,
            FormatId::Webm,
            FormatId::Avi,
        ]
    }

    fn estimate_memory_mb(&self, input: &FileInspectionReport, _req: &CompressionRequest) -> u64 {
        let w = u64::from(input.metadata_summary.width.unwrap_or(1920));
        let h = u64::from(input.metadata_summary.height.unwrap_or(1080));
        // Frame decode/reference DPB working set + encoder lookahead (~256..1024 MB bounded).
        let frame_mb = (w.saturating_mul(h).saturating_mul(16)) / 1_048_576;
        frame_mb.saturating_mul(8).clamp(128, 1024)
    }

    fn compress(
        &self,
        _input: &ValidatedSource,
        _staged_output: &StagedPath,
        _req: &CompressionRequest,
        ctx: &JobContext,
    ) -> FileFusionResult<CompressionOutcome> {
        ctx.check_cancelled()?;
        Err(FileFusionError::new(
            ErrorCode::EngMissing,
            "Video compression requires Tier 2 FFmpeg IsolatedProcessRunner; call compress_with_runner",
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::error::Error;
    use std::fs;
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};

    use file_core::{CORE_SCHEMA_VERSION, CompressionProfile};
    use hardware_acceleration::{
        HardwareEncoderCapability, HardwareProfile, HardwareVendorBackend, SimdFeatures,
    };

    fn unique_temp_dir(label: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |d| d.as_nanos());
        let dir = std::env::temp_dir().join(format!("ff_vid_{label}_{nanos}"));
        let _ = fs::create_dir_all(&dir);
        dir
    }

    fn sample_hw_profile(with_nvenc: bool) -> HardwareProfile {
        HardwareProfile {
            schema_version: CORE_SCHEMA_VERSION,
            arch: String::from("x86_64"),
            os: String::from("windows"),
            simd: SimdFeatures::detect(),
            encoders: if with_nvenc {
                vec![HardwareEncoderCapability {
                    backend: HardwareVendorBackend::Nvenc,
                    id: String::from("hw-nvenc"),
                    available: true,
                    supported_formats: vec![FormatId::Mp4, FormatId::Mkv, FormatId::Mov],
                    evidence: String::from("mocked nvcuda.dll"),
                }]
            } else {
                Vec::new()
            },
            wasm_stub: false,
            fallback_to_cpu: !with_nvenc,
            note: String::from("test profile"),
        }
    }

    #[test]
    fn parses_ffprobe_json_and_evaluates_fast_path_remux_and_moov_health()
    -> Result<(), Box<dyn Error>> {
        let json = r#"{
            "format": { "format_name": "matroska,webm", "duration": "12.50", "bit_rate": "4200000" },
            "streams": [
                { "index": 0, "codec_type": "video", "codec_name": "h264", "width": 1920, "height": 1080, "pix_fmt": "yuv420p" },
                { "index": 1, "codec_type": "audio", "codec_name": "aac", "sample_rate": "48000", "channels": 2 },
                { "index": 2, "codec_type": "subtitle", "codec_name": "subrip", "tags": { "language": "eng" } }
            ]
        }"#;
        let inv = parse_ffprobe_json_probe(json)?;
        assert_eq!(inv.video_streams.len(), 1);
        assert_eq!(inv.video_streams[0].codec_name, "h264");
        assert_eq!(inv.audio_streams[0].codec_name, "aac");
        assert_eq!(inv.subtitle_streams[0].language.as_deref(), Some("eng"));

        assert!(can_fast_path_remux(&inv, FormatId::Mp4));
        assert!(!can_fast_path_remux(&inv, FormatId::Webm));

        let dir = unique_temp_dir("moov_diag");
        let broken_mp4 = dir.join("unfinalized.mp4");
        let mut fake = Vec::new();
        fake.extend_from_slice(&20u32.to_be_bytes());
        fake.extend_from_slice(b"ftypisom\x00\x00\x02\x00isom");
        fs::write(&broken_mp4, &fake)?;
        let diag = diagnose_unfinalized_video_container(&broken_mp4, FormatId::Mp4)?;
        assert!(diag.is_some());

        let finalized_mp4 = dir.join("finalized.mp4");
        let mut valid = fake;
        valid.extend_from_slice(&16u32.to_be_bytes());
        valid.extend_from_slice(b"mdat\x00\x00\x00\x00\x00\x00\x00\x00");
        valid.extend_from_slice(&8u32.to_be_bytes());
        valid.extend_from_slice(b"moov");
        fs::write(&finalized_mp4, valid)?;
        assert_eq!(
            diagnose_unfinalized_video_container(&finalized_mp4, FormatId::Mp4)?,
            None
        );
        let _ = fs::remove_dir_all(&dir);
        Ok(())
    }

    #[test]
    fn resolves_hw_vs_cpu_routing_and_builds_staged_muxer_invocations() -> Result<(), Box<dyn Error>>
    {
        let base_req = CompressionRequest {
            profile: CompressionProfile::BalancedOptimal,
            target_format: Some(FormatId::Mp4),
            target_max_bytes: None,
            min_ssim: None,
            strip_metadata: true,
            no_op_if_larger: true,
        };
        let opts = VideoCompressionOptions {
            base: base_req.clone(),
            target_codec: Some(VideoCodec::H264),
            crf_override: None,
            max_width: Some(1280),
            max_height: None,
            target_fps: Some(30),
            allow_fast_remux: true,
            prefer_deterministic_cpu: false,
            hw_profile_override: None,
        };

        let nvenc_route =
            resolve_video_route(FormatId::Mp4, &opts, &sample_hw_profile(true), false)?;
        assert_eq!(nvenc_route.ffmpeg_encoder, "h264_nvenc");
        assert_eq!(nvenc_route.hw_backend, Some(HardwareVendorBackend::Nvenc));

        let mut det_opts = opts.clone();
        det_opts.prefer_deterministic_cpu = true;
        det_opts.target_codec = Some(VideoCodec::Av1);
        let cpu_route =
            resolve_video_route(FormatId::Mkv, &det_opts, &sample_hw_profile(true), false)?;
        assert_eq!(cpu_route.ffmpeg_encoder, "libsvtav1");
        assert_eq!(cpu_route.muxer, "matroska");

        let inv = build_ffmpeg_video_transcode_invocation(
            Path::new("ffmpeg"),
            Path::new("clip.mov"),
            Path::new(".filefusion.tmp.vid01"),
            FormatId::Mkv,
            &det_opts,
            &cpu_route,
            120,
        )?;
        let args: Vec<String> = inv
            .args
            .iter()
            .map(|a| a.to_string_lossy().into_owned())
            .collect();
        assert!(args.windows(2).any(|w| w == ["-f", "matroska"]));
        assert!(args.windows(2).any(|w| w == ["-c:v", "libsvtav1"]));
        assert!(args.windows(2).any(|w| w == ["-map_metadata", "-1"]));

        // Fast-path remux skips re-encode filters and copies streams cleanly.
        let mut remux_opts = opts;
        remux_opts.max_width = None;
        remux_opts.target_fps = None;
        remux_opts.base = base_req;
        let remux_route =
            resolve_video_route(FormatId::Mp4, &remux_opts, &sample_hw_profile(false), true)?;
        assert!(remux_route.fast_path_remux);
        let remux_inv = build_ffmpeg_video_transcode_invocation(
            Path::new("ffmpeg"),
            Path::new("input.mkv"),
            Path::new(".filefusion.tmp.vid02"),
            FormatId::Mp4,
            &remux_opts,
            &remux_route,
            60,
        )?;
        let remux_args: Vec<String> = remux_inv
            .args
            .iter()
            .map(|a| a.to_string_lossy().into_owned())
            .collect();
        assert!(remux_args.windows(2).any(|w| w == ["-c:v", "copy"]));
        assert!(
            remux_args
                .windows(2)
                .any(|w| w == ["-movflags", "+faststart"])
        );
        Ok(())
    }

    #[test]
    fn converts_srt_and_webvtt_subtitles_and_parses_vmaf_scores() -> Result<(), Box<dyn Error>> {
        let srt = "1\n00:00:01,250 --> 00:00:04,500\nWelcome to FileFusion\n\n2\n00:00:05,000 --> 00:00:07,000\nSecond line\n";
        let vtt = convert_subtitle_text(srt, TextSubtitleFormat::Srt, TextSubtitleFormat::WebVtt)?;
        assert!(vtt.starts_with("WEBVTT"));
        assert!(vtt.contains("00:00:01.250 --> 00:00:04.500"));

        let roundtrip =
            convert_subtitle_text(&vtt, TextSubtitleFormat::WebVtt, TextSubtitleFormat::Srt)?;
        assert_eq!(roundtrip, srt);

        // Multi-byte UTF-8 or non-digit fractional tokens must yield FmtCorrupt cleanly, never panic.
        assert!(
            parse_srt_cues("1\n00:00:01,🦀 --> 00:00:02,000\nbad\n").is_err(),
            "non-ASCII fractional timestamp must return error"
        );
        assert!(
            parse_srt_cues("1\n00:00:01,a🦀 --> 00:00:02,000\nbad\n").is_err(),
            "mixed non-ASCII fractional timestamp must return error"
        );

        let log = "frame=120 fps=30\n[Parsed_libvmaf_0] VMAF score: 94.812345\n";
        assert_eq!(parse_ffmpeg_vmaf_score(log), Some(94.812_345));
        assert_eq!(parse_ffmpeg_vmaf_score("unrelated"), None);
        Ok(())
    }

    #[cfg(feature = "native-process")]
    #[test]
    fn video_engine_enforces_noop_guard_and_promotes_smaller_staged_output()
    -> Result<(), Box<dyn Error>> {
        use file_core::{
            CompressionStatus, ProcessInvocation, ProcessOutput, ResourceLimits, inspect_path,
        };

        struct StubFfmpegRunner {
            output_bytes: Vec<u8>,
        }
        impl IsolatedProcessRunner for StubFfmpegRunner {
            fn run_isolated(
                &self,
                invocation: &ProcessInvocation,
                _ctx: &JobContext,
            ) -> FileFusionResult<ProcessOutput> {
                if let Some(out_path) = invocation.args.last() {
                    fs::write(Path::new(out_path), &self.output_bytes)?;
                }
                Ok(ProcessOutput {
                    exit_code: 0,
                    stdout: Vec::new(),
                    stderr: Vec::new(),
                    duration_ms: 5,
                    timed_out: false,
                })
            }
        }

        let dir = unique_temp_dir("vid_engine");
        let src_mp4 = dir.join("input.mp4");
        let mut big_mp4 = Vec::from(&20u32.to_be_bytes()[..]);
        big_mp4.extend_from_slice(b"ftypisom\x00\x00\x02\x00isom");
        big_mp4.resize(256, 0xAB);
        fs::write(&src_mp4, &big_mp4)?;

        let mut small_mp4 = Vec::from(&20u32.to_be_bytes()[..]);
        small_mp4.extend_from_slice(b"ftypisom\x00\x00\x02\x00isom");
        small_mp4.resize(48, 0xCD);

        let source = ValidatedSource {
            path: src_mp4.clone(),
            inspection: inspect_path(&src_mp4, false)?,
        };
        let ctx = JobContext::new("vid-01", ResourceLimits::default());
        let engine = VideoCompressionEngine::with_ffmpeg_binaries(PathBuf::from("ffmpeg"), None);
        let opts = VideoCompressionOptions {
            base: CompressionRequest {
                profile: CompressionProfile::BalancedOptimal,
                target_format: Some(FormatId::Mp4),
                target_max_bytes: None,
                min_ssim: None,
                strip_metadata: true,
                no_op_if_larger: true,
            },
            target_codec: Some(VideoCodec::H264),
            crf_override: Some(22),
            max_width: None,
            max_height: None,
            target_fps: None,
            allow_fast_remux: false,
            prefer_deterministic_cpu: true,
            hw_profile_override: Some(sample_hw_profile(false)),
        };

        // 1. Shrinking output promotes normally.
        let staged_small = StagedPath::create_sibling(dir.join("out_small.mp4"))?;
        let runner_small = StubFfmpegRunner {
            output_bytes: small_mp4,
        };
        let outcome_ok =
            engine.compress_with_runner(&source, &staged_small, &opts, &runner_small, &ctx)?;
        assert_eq!(outcome_ok.status, CompressionStatus::Optimized);
        assert!(outcome_ok.bytes_out < outcome_ok.bytes_in);

        // 2. Bloated same-format output triggers FR-CMP-002 NoOpRetainedOriginal.
        let mut bloated_mp4 = big_mp4;
        bloated_mp4.extend_from_slice(&[0u8; 128]);
        let staged_big = StagedPath::create_sibling(dir.join("out_big.mp4"))?;
        let runner_big = StubFfmpegRunner {
            output_bytes: bloated_mp4,
        };
        let outcome_noop =
            engine.compress_with_runner(&source, &staged_big, &opts, &runner_big, &ctx)?;
        assert_eq!(outcome_noop.status, CompressionStatus::NoOpRetainedOriginal);
        assert_eq!(outcome_noop.bytes_out, source.inspection.byte_size);

        let _ = fs::remove_dir_all(&dir);
        Ok(())
    }
}
