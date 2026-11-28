//! Hardware-aware `FFmpeg` encoder routing (`FR-ADP-002`), fast-path container remuxing
//! (`FR-AV-001`), scaling/CRF option construction, and VMAF score extraction.

use std::ffi::OsString;
use std::path::Path;

use compression_core::CompressionProfileConfig;
use file_core::{
    CompressionProfile, CompressionRequest, ErrorCode, FileFusionError, FileFusionResult, FormatId,
    ProcessInvocation,
};
use hardware_acceleration::{HardwareProfile, HardwareVendorBackend, select_video_backend};
use serde::{Deserialize, Serialize};

/// Supported modern video codecs for CPU & hardware encoding (`FR-AV-001`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VideoCodec {
    /// H.264 / AVC (highest compatibility).
    H264,
    /// H.265 / HEVC (high efficiency).
    Hevc,
    /// Google VP9 (`WebM` / open web standard).
    Vp9,
    /// `AOMedia` Video 1 (`libsvtav1` / HW AV1).
    Av1,
}

impl VideoCodec {
    /// Default target codec when caller selects only a target container format.
    #[must_use]
    pub const fn default_for_container(format: FormatId, profile: CompressionProfile) -> Self {
        match format {
            FormatId::Webm => Self::Vp9,
            FormatId::Avi => Self::H264,
            FormatId::Mkv if matches!(profile, CompressionProfile::ArchivalColdStorage) => {
                Self::Hevc
            }
            _ => match profile {
                CompressionProfile::ExtremeSizeTarget => Self::Av1,
                CompressionProfile::ArchivalColdStorage => Self::Hevc,
                _ => Self::H264,
            },
        }
    }

    /// Canonical CRF / CQ rate-factor derived from `CompressionProfile` when `crf_override` is omitted.
    #[must_use]
    pub const fn default_crf(self, profile: CompressionProfile) -> u8 {
        match self {
            Self::H264 => match profile {
                CompressionProfile::LosslessMaximum => 16,
                CompressionProfile::ArchivalColdStorage => 18,
                CompressionProfile::BalancedOptimal => 22,
                CompressionProfile::WebSharingReady => 24,
                CompressionProfile::ExtremeSizeTarget => 28,
            },
            Self::Hevc => match profile {
                CompressionProfile::LosslessMaximum => 18,
                CompressionProfile::ArchivalColdStorage => 20,
                CompressionProfile::BalancedOptimal => 24,
                CompressionProfile::WebSharingReady => 26,
                CompressionProfile::ExtremeSizeTarget => 30,
            },
            Self::Vp9 | Self::Av1 => match profile {
                CompressionProfile::LosslessMaximum => 22,
                CompressionProfile::ArchivalColdStorage => 24,
                CompressionProfile::BalancedOptimal => 30,
                CompressionProfile::WebSharingReady => 32,
                CompressionProfile::ExtremeSizeTarget => 38,
            },
        }
    }
}

/// Extended options controlling `compression-video` transcode vs fast-path remuxing.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct VideoCompressionOptions {
    /// Standard `FileFusion` compression contract (`FF-CTR-001` §3.1).
    pub base: CompressionRequest,
    /// Optional explicit output video codec (`None` picks container/preset default).
    pub target_codec: Option<VideoCodec>,
    /// Optional CRF / CQ override (`0..=51`).
    pub crf_override: Option<u8>,
    /// Optional maximum output width in pixels (aspect ratio preserved, even dimensions).
    pub max_width: Option<u32>,
    /// Optional maximum output height in pixels (aspect ratio preserved, even dimensions).
    pub max_height: Option<u32>,
    /// Optional target frame rate cap (fps).
    pub target_fps: Option<u32>,
    /// Allow lossless zero-reencode remuxing when source streams fit target container.
    pub allow_fast_remux: bool,
    /// Force deterministic CPU encoding (`libx264` / `libx265` / `libsvtav1` / `libvpx-vp9`)
    /// bypassing vendor GPU acceleration (`FR-ADP-002`).
    pub prefer_deterministic_cpu: bool,
    /// Optional pre-probed hardware capability profile (`None` probes current host).
    pub hw_profile_override: Option<HardwareProfile>,
}

/// Concrete `FFmpeg` encoder & quality routing plan resolved from hardware capabilities and policy.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResolvedVideoRoute {
    /// `true` when lossless `-c:v copy -c:a copy` fast-path remuxing is selected.
    pub fast_path_remux: bool,
    /// `FFmpeg` `-c:v` encoder identifier (`copy`, `libx264`, `h264_nvenc`, `libsvtav1`, etc.).
    pub ffmpeg_encoder: String,
    /// Selected vendor hardware backend if GPU acceleration is active.
    pub hw_backend: Option<HardwareVendorBackend>,
    /// Quality parameter flag (`-crf` or `-cq` or `-qp`) if transcoding.
    pub quality_flag: Option<String>,
    /// Numeric quality value paired with `quality_flag`.
    pub quality_value: Option<u8>,
    /// `FFmpeg` output muxer (`-f mp4`, `-f matroska`, `-f webm`, `-f mov`, `-f avi`).
    pub muxer: String,
    /// Human-readable routing rationale for telemetry and Fidelity/Optimization reports.
    pub rationale: String,
}

/// Maps a target [`FormatId`] to its canonical `FFmpeg` muxer name for staged `.filefusion.tmp.*` files.
///
/// # Errors
/// Returns [`FileFusionError`] if `format` is not a supported video container.
pub fn ffmpeg_muxer_for_video_container(format: FormatId) -> FileFusionResult<&'static str> {
    match format {
        FormatId::Mp4 => Ok("mp4"),
        FormatId::Mov => Ok("mov"),
        FormatId::Mkv => Ok("matroska"),
        FormatId::Webm => Ok("webm"),
        FormatId::Avi => Ok("avi"),
        other => Err(FileFusionError::new(
            ErrorCode::FmtUnsupported,
            format!("unsupported video container target {other:?}"),
        )),
    }
}

/// Resolves deterministic CPU vs hardware `FFmpeg` encoder routing (`FR-ADP-002`, `FR-AV-001`).
///
/// # Errors
/// Returns [`FileFusionError`] if the container or `(codec, hw_backend)` combination is invalid.
pub fn resolve_video_route(
    target_format: FormatId,
    opts: &VideoCompressionOptions,
    hw_profile: &HardwareProfile,
    fast_remux_eligible: bool,
) -> FileFusionResult<ResolvedVideoRoute> {
    let muxer = ffmpeg_muxer_for_video_container(target_format)?;
    let no_resize_or_fps =
        opts.max_width.is_none() && opts.max_height.is_none() && opts.target_fps.is_none();

    if opts.allow_fast_remux
        && fast_remux_eligible
        && no_resize_or_fps
        && opts.crf_override.is_none()
    {
        return Ok(ResolvedVideoRoute {
            fast_path_remux: true,
            ffmpeg_encoder: String::from("copy"),
            hw_backend: None,
            quality_flag: None,
            quality_value: None,
            muxer: String::from(muxer),
            rationale: String::from(
                "Source elementary streams natively fit target container; zero-reencode fast-path remux selected.",
            ),
        });
    }

    let req = &opts.base;
    let codec = opts
        .target_codec
        .unwrap_or_else(|| VideoCodec::default_for_container(target_format, req.profile));
    if target_format == FormatId::Webm && matches!(codec, VideoCodec::H264 | VideoCodec::Hevc) {
        return Err(FileFusionError::new(
            ErrorCode::FmtUnsupported,
            format!("WebM container does not permit {codec:?}; select VP9 or AV1"),
        ));
    }

    let decision = select_video_backend(hw_profile, target_format, opts.prefer_deterministic_cpu);
    let crf = opts
        .crf_override
        .unwrap_or_else(|| codec.default_crf(req.profile))
        .clamp(0, 51);

    let (encoder, hw_used, q_flag) = map_codec_and_backend(codec, decision.selected_backend);
    Ok(ResolvedVideoRoute {
        fast_path_remux: false,
        ffmpeg_encoder: String::from(encoder),
        hw_backend: hw_used,
        quality_flag: Some(String::from(q_flag)),
        quality_value: Some(crf),
        muxer: String::from(muxer),
        rationale: decision.reason,
    })
}

/// Builds a sanitized `FFmpeg` `ProcessInvocation` for video compression or fast-path remuxing
/// writing to `.filefusion.tmp.*` `staged_output` (`FR-AV-001`, `MOD-RULE-05`).
///
/// # Errors
/// Returns [`FileFusionError`] on invalid scaling constraints.
pub fn build_ffmpeg_video_transcode_invocation(
    ffmpeg_binary: &Path,
    input_path: &Path,
    staged_output_path: &Path,
    target_format: FormatId,
    opts: &VideoCompressionOptions,
    route: &ResolvedVideoRoute,
    timeout_secs: u64,
) -> FileFusionResult<ProcessInvocation> {
    let req = &opts.base;
    let mut args = vec![
        OsString::from("-hide_banner"),
        OsString::from("-loglevel"),
        OsString::from("error"),
        OsString::from("-y"),
        OsString::from("-i"),
        input_path.as_os_str().to_os_string(),
    ];

    if route.fast_path_remux {
        args.push(OsString::from("-c:v"));
        args.push(OsString::from("copy"));
        args.push(OsString::from("-c:a"));
        args.push(OsString::from("copy"));
        args.push(OsString::from("-c:s"));
        args.push(OsString::from("copy"));
    } else {
        if let Some(vf) = build_scale_filter(opts.max_width, opts.max_height)? {
            args.push(OsString::from("-vf"));
            args.push(OsString::from(vf));
        }
        if let Some(fps) = opts.target_fps {
            args.push(OsString::from("-r"));
            args.push(OsString::from(fps.max(1).to_string()));
        }

        args.push(OsString::from("-c:v"));
        args.push(OsString::from(&route.ffmpeg_encoder));
        if let (Some(flag), Some(val)) = (&route.quality_flag, route.quality_value) {
            args.push(OsString::from(flag));
            args.push(OsString::from(val.to_string()));
        }

        let cfg = CompressionProfileConfig::for_profile(req.profile);
        if route.ffmpeg_encoder == "libx264" || route.ffmpeg_encoder == "libx265" {
            let preset = match cfg.effort_level {
                0..=4 => "faster",
                5..=7 => "medium",
                _ => "slow",
            };
            args.push(OsString::from("-preset"));
            args.push(OsString::from(preset));
        } else if route.ffmpeg_encoder == "libsvtav1" {
            let svt_preset = 13u8.saturating_sub(cfg.effort_level).clamp(2, 10);
            args.push(OsString::from("-preset"));
            args.push(OsString::from(svt_preset.to_string()));
        }

        args.push(OsString::from("-c:a"));
        if target_format == FormatId::Webm {
            args.push(OsString::from("libopus"));
            args.push(OsString::from("-b:a"));
            args.push(OsString::from("128k"));
        } else {
            args.push(OsString::from("aac"));
            args.push(OsString::from("-b:a"));
            args.push(OsString::from("160k"));
        }
    }

    if matches!(target_format, FormatId::Mp4 | FormatId::Mov) {
        args.push(OsString::from("-movflags"));
        args.push(OsString::from("+faststart"));
    }
    if req.strip_metadata {
        args.push(OsString::from("-map_metadata"));
        args.push(OsString::from("-1"));
    }

    args.push(OsString::from("-f"));
    args.push(OsString::from(&route.muxer));
    args.push(staged_output_path.as_os_str().to_os_string());

    Ok(ProcessInvocation {
        executable: ffmpeg_binary.to_path_buf(),
        args,
        working_dir: None,
        timeout_secs,
        allow_env_keys: Vec::new(),
    })
}

/// Builds a sanitized `FFmpeg` `libvmaf` evaluation command (`0.0..=100.0`).
#[must_use]
pub fn build_ffmpeg_vmaf_invocation(
    ffmpeg_binary: &Path,
    distorted_path: &Path,
    reference_path: &Path,
    timeout_secs: u64,
) -> ProcessInvocation {
    ProcessInvocation {
        executable: ffmpeg_binary.to_path_buf(),
        args: vec![
            OsString::from("-hide_banner"),
            OsString::from("-i"),
            distorted_path.as_os_str().to_os_string(),
            OsString::from("-i"),
            reference_path.as_os_str().to_os_string(),
            OsString::from("-lavfi"),
            OsString::from("libvmaf"),
            OsString::from("-f"),
            OsString::from("null"),
            OsString::from("-"),
        ],
        working_dir: None,
        timeout_secs,
        allow_env_keys: Vec::new(),
    }
}

/// Extracts mean VMAF score (`0.0..=100.0`) from `FFmpeg` `libvmaf` stderr/JSON output.
#[must_use]
pub fn parse_ffmpeg_vmaf_score(output: &str) -> Option<f64> {
    for line in output.lines() {
        if let Some((_, rhs)) = line.split_once("VMAF score:") {
            let token = rhs.split_whitespace().next()?;
            if let Ok(score) = token.trim().parse::<f64>() {
                return Some(score.clamp(0.0, 100.0));
            }
        }
    }
    if let Ok(json) = serde_json::from_str::<serde_json::Value>(output) {
        if let Some(mean) = json
            .get("pooled_metrics")
            .and_then(|m| m.get("vmaf"))
            .and_then(|v| v.get("mean"))
            .and_then(serde_json::Value::as_f64)
        {
            return Some(mean.clamp(0.0, 100.0));
        }
    }
    None
}

fn build_scale_filter(
    max_width: Option<u32>,
    max_height: Option<u32>,
) -> FileFusionResult<Option<String>> {
    match (max_width, max_height) {
        (None, None) => Ok(None),
        (Some(0), _) | (_, Some(0)) => Err(FileFusionError::new(
            ErrorCode::FmtUnsupported,
            "video target dimensions must be positive",
        )),
        (Some(w), None) => Ok(Some(format!("scale='min({w},iw)':-2:flags=lanczos"))),
        (None, Some(h)) => Ok(Some(format!("scale=-2:'min({h},ih)':flags=lanczos"))),
        (Some(w), Some(h)) => Ok(Some(format!(
            "scale='min({w},iw)':'min({h},ih)':force_original_aspect_ratio=decrease,pad=ceil(iw/2)*2:ceil(ih/2)*2"
        ))),
    }
}

const fn map_codec_and_backend(
    codec: VideoCodec,
    backend: Option<HardwareVendorBackend>,
) -> (&'static str, Option<HardwareVendorBackend>, &'static str) {
    match (codec, backend) {
        (VideoCodec::H264, Some(HardwareVendorBackend::Nvenc)) => {
            ("h264_nvenc", Some(HardwareVendorBackend::Nvenc), "-cq")
        }
        (VideoCodec::H264, Some(HardwareVendorBackend::Qsv)) => (
            "h264_qsv",
            Some(HardwareVendorBackend::Qsv),
            "-global_quality",
        ),
        (VideoCodec::H264, Some(HardwareVendorBackend::Amf)) => {
            ("h264_amf", Some(HardwareVendorBackend::Amf), "-qp_i")
        }
        (VideoCodec::H264, Some(HardwareVendorBackend::VideoToolbox)) => (
            "h264_videotoolbox",
            Some(HardwareVendorBackend::VideoToolbox),
            "-q:v",
        ),
        (VideoCodec::H264, Some(HardwareVendorBackend::Vaapi)) => {
            ("h264_vaapi", Some(HardwareVendorBackend::Vaapi), "-qp")
        }
        (VideoCodec::Hevc, Some(HardwareVendorBackend::Nvenc)) => {
            ("hevc_nvenc", Some(HardwareVendorBackend::Nvenc), "-cq")
        }
        (VideoCodec::Hevc, Some(HardwareVendorBackend::Qsv)) => (
            "hevc_qsv",
            Some(HardwareVendorBackend::Qsv),
            "-global_quality",
        ),
        (VideoCodec::Hevc, Some(HardwareVendorBackend::Amf)) => {
            ("hevc_amf", Some(HardwareVendorBackend::Amf), "-qp_i")
        }
        (VideoCodec::Hevc, Some(HardwareVendorBackend::VideoToolbox)) => (
            "hevc_videotoolbox",
            Some(HardwareVendorBackend::VideoToolbox),
            "-q:v",
        ),
        (VideoCodec::Hevc, Some(HardwareVendorBackend::Vaapi)) => {
            ("hevc_vaapi", Some(HardwareVendorBackend::Vaapi), "-qp")
        }
        (VideoCodec::Av1, Some(HardwareVendorBackend::Nvenc)) => {
            ("av1_nvenc", Some(HardwareVendorBackend::Nvenc), "-cq")
        }
        (VideoCodec::Av1, Some(HardwareVendorBackend::Qsv)) => (
            "av1_qsv",
            Some(HardwareVendorBackend::Qsv),
            "-global_quality",
        ),
        (VideoCodec::Av1, Some(HardwareVendorBackend::Amf)) => {
            ("av1_amf", Some(HardwareVendorBackend::Amf), "-qp_i")
        }
        (VideoCodec::Vp9, Some(HardwareVendorBackend::Vaapi)) => {
            ("vp9_vaapi", Some(HardwareVendorBackend::Vaapi), "-qp")
        }
        (VideoCodec::Vp9, Some(HardwareVendorBackend::Qsv)) => (
            "vp9_qsv",
            Some(HardwareVendorBackend::Qsv),
            "-global_quality",
        ),
        (VideoCodec::H264, _) => ("libx264", None, "-crf"),
        (VideoCodec::Hevc, _) => ("libx265", None, "-crf"),
        (VideoCodec::Vp9, _) => ("libvpx-vp9", None, "-crf"),
        (VideoCodec::Av1, _) => ("libsvtav1", None, "-crf"),
    }
}
