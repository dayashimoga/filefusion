//! `compression-audio`: L3 pure-Rust lossless audio compression, DSP sample-rate &
//! ITU-R BS.1770 / EBU R128 loudness normalization, ID3/Vorbis metadata handling,
//! and optional Tier 2 `FFmpeg` audio transcoding / track extraction
//! (`FR-AV-002`, `FR-AV-003`, `FR-CMP-001`..`002`).

#![forbid(unsafe_code)]

pub mod dsp;
#[cfg(feature = "native-process")]
pub mod ffmpeg_adapter;
pub mod tags;

use std::path::Path;

use compression_core::{finalize_staged_compression, prescreen_incompressible_noop};
use file_core::{
    CompressionEngine, CompressionOutcome, CompressionRequest, ErrorCode, FileFusionError,
    FileFusionResult, FileInspectionReport, FormatId, JobContext, QualityEvaluator, QualityMetrics,
    StagedPath, ValidatedSource,
};
use quality_engine::BitExactQualityEvaluator;
use serde::{Deserialize, Serialize};
use validation_engine::MagicByteOutputValidator;

#[cfg(feature = "native-process")]
use std::path::PathBuf;

#[cfg(feature = "native-process")]
use file_core::IsolatedProcessRunner;

pub use dsp::{
    LoudnessMeasurement, PcmAudioBuffer, decode_any_audio_to_pcm, decode_flac_path,
    decode_via_symphonia, decode_wav_path, encode_flac_path, encode_wav_path,
    evaluate_pcm_equivalence, measure_integrated_lufs, normalize_loudness_lufs, remap_channels,
    resample_pcm,
};
#[cfg(feature = "native-process")]
pub use ffmpeg_adapter::{
    build_ffmpeg_audio_transcode_invocation, build_ffmpeg_extract_audio_invocation,
    execute_ffmpeg_invocation,
};
pub use tags::{
    AudioTagMetadata, has_non_essential_metadata, read_id3_metadata, strip_id3_metadata,
    write_id3_metadata,
};

/// Extended parameters for audio transcoding, resampling, channel downmix, and LUFS normalization.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AudioCompressionOptions {
    /// Standard `FileFusion` compression contract (`FF-CTR-001` §3.1).
    pub base: CompressionRequest,
    /// Optional target sample rate in Hz (e.g., `44_100` or `48_000`).
    pub target_sample_rate: Option<u32>,
    /// Optional target channel count (`1` = mono, `2` = stereo).
    pub target_channels: Option<u16>,
    /// Optional EBU R128 integrated loudness target in LUFS (e.g., `-16.0`).
    pub target_lufs: Option<f64>,
    /// Optional target CBR/ABR bitrate in kbps for Tier 2 lossy encoders.
    pub target_bitrate_kbps: Option<u32>,
}

/// Decodes original & candidate audio streams into PCM to verify bit-exact lossless preservation
/// (`WAV <-> FLAC`) and compute waveform PSNR (`FF-CTR-001` §3.3).
#[derive(Debug, Clone, Copy, Default)]
pub struct AudioQualityEvaluator {
    /// Source format override when pre-resolved.
    pub source_format: Option<FormatId>,
    /// Candidate format override (useful before `.filefusion.tmp.*` commit).
    pub target_format: Option<FormatId>,
}

impl QualityEvaluator for AudioQualityEvaluator {
    fn evaluator_id(&self) -> &'static str {
        "compression-audio-pcm-evaluator"
    }

    fn evaluate(
        &self,
        original: &ValidatedSource,
        candidate: &Path,
        ctx: &JobContext,
    ) -> FileFusionResult<QualityMetrics> {
        ctx.check_cancelled()?;
        let orig_fmt = self
            .source_format
            .unwrap_or(original.inspection.detected_format);
        let cand_fmt = self.target_format.unwrap_or(orig_fmt);

        if let (Ok(orig_pcm), Ok(cand_pcm)) = (
            decode_any_audio_to_pcm(&original.path, orig_fmt),
            decode_any_audio_to_pcm(candidate, cand_fmt),
        ) {
            let (exact, psnr_db) = evaluate_pcm_equivalence(&orig_pcm, &cand_pcm);
            return Ok(QualityMetrics {
                ssim: None,
                psnr_db: Some(psnr_db),
                vmaf: None,
                bit_exact_lossless: Some(exact),
            });
        }

        BitExactQualityEvaluator.evaluate(original, candidate, ctx)
    }
}

/// L3 pure-Rust & optional Tier 2 `FFmpeg` audio compression engine (`FR-AV-002`, `FR-CMP-002`).
#[derive(Debug, Clone, Default)]
pub struct AudioCompressionEngine {
    #[cfg(feature = "native-process")]
    ffmpeg_binary: Option<PathBuf>,
}

impl AudioCompressionEngine {
    /// Creates an engine without external Tier 2 `FFmpeg` configured (Tier 1 pure Rust only).
    #[must_use]
    pub const fn new_tier1() -> Self {
        Self {
            #[cfg(feature = "native-process")]
            ffmpeg_binary: None,
        }
    }

    /// Configures a verified Tier 2 `ffmpeg` binary path for lossy codecs (`Opus`, `AAC`, `MP3`,
    /// `Ogg Vorbis`) and ALAC packing (`MOD-RULE-05`).
    #[cfg(feature = "native-process")]
    #[must_use]
    pub fn with_ffmpeg_binary(ffmpeg_binary: PathBuf) -> Self {
        Self {
            ffmpeg_binary: Some(ffmpeg_binary),
        }
    }

    /// Compresses or transcodes `input` using pure-Rust Tier 1 codecs (`WAV`, `FLAC`) + DSP
    /// normalization, enforcing `FR-CMP-002` No-Op Guard.
    ///
    /// # Errors
    /// Returns [`FileFusionError`] on unsupported target codec, corrupt stream, cancellation, or
    /// resource ceiling.
    pub fn compress_with_options(
        &self,
        input: &ValidatedSource,
        staged_output: &StagedPath,
        opts: &AudioCompressionOptions,
        ctx: &JobContext,
    ) -> FileFusionResult<CompressionOutcome> {
        ctx.check_cancelled()?;
        let req = &opts.base;
        let target_format = req
            .target_format
            .unwrap_or(input.inspection.detected_format);

        if opts.target_sample_rate.is_none()
            && opts.target_channels.is_none()
            && opts.target_lufs.is_none()
        {
            if let Some(noop) = prescreen_incompressible_noop(input, req, self.engine_id()) {
                return Ok(noop);
            }
        }

        let est_mb = self.estimate_memory_mb(&input.inspection, req);
        let _mem = ctx.reserve_working_set_mb(est_mb, "audio_compression")?;

        if !matches!(target_format, FormatId::Flac | FormatId::Wav) {
            return Err(FileFusionError::new(
                ErrorCode::FmtUnsupported,
                format!(
                    "Tier 1 audio encoder supports FLAC/WAV; use compress_with_ffmpeg for {target_format:?}"
                ),
            ));
        }

        let mut pcm = decode_any_audio_to_pcm(&input.path, input.inspection.detected_format)?;
        ctx.check_cancelled()?;

        if let Some(rate) = opts.target_sample_rate {
            pcm = resample_pcm(&pcm, rate)?;
        }
        if let Some(ch) = opts.target_channels {
            pcm = remap_channels(&pcm, ch)?;
        }
        if let Some(lufs) = opts.target_lufs {
            let _ = normalize_loudness_lufs(&mut pcm, lufs, -1.0)?;
        }
        ctx.check_cancelled()?;

        match target_format {
            FormatId::Flac => encode_flac_path(&pcm, staged_output.staged_path(), req.profile)?,
            FormatId::Wav => encode_wav_path(&pcm, staged_output.staged_path())?,
            _ => {}
        }

        let validator = MagicByteOutputValidator;
        let evaluator = AudioQualityEvaluator {
            source_format: Some(input.inspection.detected_format),
            target_format: Some(target_format),
        };
        let codec_name = if target_format == FormatId::Flac {
            "tier1-audio-flacenc"
        } else {
            "tier1-audio-wav-pcm"
        };

        finalize_staged_compression(
            input,
            staged_output,
            req,
            &validator,
            Some(&evaluator),
            codec_name,
            ctx,
        )
    }

    /// Executes an audio compression/transcode via Tier 2 `ffmpeg` + [`IsolatedProcessRunner`]
    /// (`Opus`, `AAC`, `M4A/ALAC`, `MP3`, `Ogg Vorbis`) with staged validation and `FR-CMP-002`
    /// No-Op Guard.
    ///
    /// # Errors
    /// Returns [`FileFusionError`] if `ffmpeg_binary` was not configured (`EngMissing`), runner
    /// fails, validation fails, or cancellation triggers.
    #[cfg(feature = "native-process")]
    pub fn compress_with_ffmpeg(
        &self,
        input: &ValidatedSource,
        staged_output: &StagedPath,
        opts: &AudioCompressionOptions,
        runner: &dyn IsolatedProcessRunner,
        ctx: &JobContext,
    ) -> FileFusionResult<CompressionOutcome> {
        ctx.check_cancelled()?;
        let Some(ffmpeg_bin) = &self.ffmpeg_binary else {
            return Err(FileFusionError::new(
                ErrorCode::EngMissing,
                "Tier 2 ffmpeg binary path is required for lossy/ALAC audio targets",
            ));
        };
        let req = &opts.base;
        if opts.target_sample_rate.is_none()
            && opts.target_channels.is_none()
            && opts.target_lufs.is_none()
        {
            if let Some(noop) = prescreen_incompressible_noop(input, req, self.engine_id()) {
                return Ok(noop);
            }
        }
        let est_mb = self.estimate_memory_mb(&input.inspection, req);
        let _mem = ctx.reserve_working_set_mb(est_mb, "audio_ffmpeg_compression")?;

        let target_format = req
            .target_format
            .unwrap_or(input.inspection.detected_format);
        let invocation = build_ffmpeg_audio_transcode_invocation(
            ffmpeg_bin,
            &input.path,
            staged_output.staged_path(),
            target_format,
            opts,
            ctx.limits.subprocess_timeout_secs,
        )?;
        execute_ffmpeg_invocation(runner, &invocation, ctx)?;

        let validator = MagicByteOutputValidator;
        let evaluator = AudioQualityEvaluator {
            source_format: Some(input.inspection.detected_format),
            target_format: Some(target_format),
        };
        finalize_staged_compression(
            input,
            staged_output,
            req,
            &validator,
            Some(&evaluator),
            format!("tier2-ffmpeg-{}", target_format.mime_type()),
            ctx,
        )
    }
}

impl CompressionEngine for AudioCompressionEngine {
    fn engine_id(&self) -> &'static str {
        "compression-audio-tier1"
    }

    fn supported_formats(&self) -> &[FormatId] {
        &[
            FormatId::Wav,
            FormatId::Flac,
            FormatId::Mp3,
            FormatId::Ogg,
            FormatId::Opus,
            FormatId::Aac,
            FormatId::M4a,
        ]
    }

    fn estimate_memory_mb(&self, input: &FileInspectionReport, _req: &CompressionRequest) -> u64 {
        // Lossless PCM expansion ratio (~10x max for compressed audio) capped safely.
        let mb = (input.byte_size / (1024 * 1024)).saturating_mul(10);
        mb.clamp(16, 512)
    }

    fn compress(
        &self,
        input: &ValidatedSource,
        staged_output: &StagedPath,
        req: &CompressionRequest,
        ctx: &JobContext,
    ) -> FileFusionResult<CompressionOutcome> {
        let opts = AudioCompressionOptions {
            base: req.clone(),
            target_sample_rate: None,
            target_channels: None,
            target_lufs: None,
            target_bitrate_kbps: None,
        };
        self.compress_with_options(input, staged_output, &opts, ctx)
    }
}

/// Validates staged audio bytes cleanly using [`MagicByteOutputValidator`].
pub type AudioOutputValidator = MagicByteOutputValidator;

#[cfg(test)]
mod tests {
    use super::*;
    use std::error::Error;
    use std::fs;
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};

    use file_core::{
        CollisionPolicy, CompressionProfile, CompressionStatus, ResourceLimits, inspect_path,
    };

    fn unique_temp_dir(label: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |d| d.as_nanos());
        let dir = std::env::temp_dir().join(format!("ff_aud_{label}_{nanos}"));
        let _ = fs::create_dir_all(&dir);
        dir
    }

    fn sine_wave_pcm(
        sample_rate: u32,
        channels: u16,
        duration_secs: f64,
        amplitude_ratio: f64,
    ) -> PcmAudioBuffer {
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let frames = (f64::from(sample_rate) * duration_secs) as usize;
        let ch = usize::from(channels);
        let mut samples_i32 = Vec::with_capacity(frames * ch);
        let amp = f64::from(i16::MAX) * amplitude_ratio;

        for f in 0..frames {
            #[allow(clippy::cast_precision_loss)]
            let t = (f as f64) / f64::from(sample_rate);
            let s_l = (2.0 * std::f64::consts::PI * 440.0 * t).sin() * amp;
            let s_r = (2.0 * std::f64::consts::PI * 880.0 * t).sin() * amp;
            for c in 0..ch {
                let val = if c == 0 { s_l } else { s_r };
                #[allow(clippy::cast_possible_truncation)]
                samples_i32.push(val.round() as i32);
            }
        }
        PcmAudioBuffer {
            sample_rate,
            channels,
            bits_per_sample: 16,
            samples_i32,
        }
    }

    #[test]
    fn wav_to_flac_lossless_roundtrip_and_noop_guard() -> Result<(), Box<dyn Error>> {
        let dir = unique_temp_dir("flac_roundtrip");
        let wav_path = dir.join("tone.wav");
        let flac_dest = dir.join("tone.flac");
        let pcm = sine_wave_pcm(44_100, 2, 0.8, 0.45);
        encode_wav_path(&pcm, &wav_path)?;

        let wav_src = ValidatedSource {
            path: wav_path.clone(),
            inspection: inspect_path(&wav_path, false)?,
        };
        let ctx = JobContext::new("aud-flac-01", ResourceLimits::default());
        let engine = AudioCompressionEngine::new_tier1();

        let staged_flac = StagedPath::create_sibling(&flac_dest)?;
        let req = CompressionRequest {
            profile: CompressionProfile::LosslessMaximum,
            target_format: Some(FormatId::Flac),
            target_max_bytes: None,
            min_ssim: None,
            strip_metadata: true,
            no_op_if_larger: true,
        };
        let flac_out = engine.compress(&wav_src, &staged_flac, &req, &ctx)?;
        assert_eq!(flac_out.status, CompressionStatus::Optimized);
        assert!(flac_out.bytes_out < flac_out.bytes_in);
        assert_eq!(
            flac_out.quality.as_ref().and_then(|q| q.bit_exact_lossless),
            Some(true)
        );
        let committed_flac = staged_flac.promote(CollisionPolicy::RenameWithSuffix)?;

        // Decode FLAC back losslessly via `claxon`, verify clean FLAC metadata, and exercise `symphonia` decoding.
        let decoded_flac = decode_flac_path(&committed_flac)?;
        assert_eq!(decoded_flac, pcm);
        let decoded_symph = decode_via_symphonia(&wav_path)?;
        assert_eq!(decoded_symph, pcm);
        assert!(!has_non_essential_metadata(&committed_flac, FormatId::Flac));

        // Re-compressing already optimal FLAC at same format should trigger No-Op guard cleanly
        // when output size matches or increases (`BalancedOptimal` uses smaller block size -> larger).
        let flac_src = ValidatedSource {
            path: committed_flac,
            inspection: inspect_path(&flac_dest, false)?,
        };
        let staged_reopt = StagedPath::create_sibling(dir.join("tone_reopt.flac"))?;
        let reopt_req = CompressionRequest {
            profile: CompressionProfile::BalancedOptimal,
            target_format: Some(FormatId::Flac),
            target_max_bytes: None,
            min_ssim: None,
            strip_metadata: false,
            no_op_if_larger: true,
        };
        let reopt_out = engine.compress(&flac_src, &staged_reopt, &reopt_req, &ctx)?;
        assert!(matches!(
            reopt_out.status,
            CompressionStatus::NoOpRetainedOriginal | CompressionStatus::Optimized
        ));

        let _ = fs::remove_dir_all(&dir);
        Ok(())
    }

    #[test]
    fn resamples_remaps_and_normalizes_ebu_r128_loudness() -> Result<(), Box<dyn Error>> {
        let quiet_stereo = sine_wave_pcm(48_000, 2, 1.0, 0.04);
        let before = measure_integrated_lufs(&quiet_stereo);
        assert!(
            before.integrated_lufs < -25.0,
            "expected quiet input LUFS < -25, got {}",
            before.integrated_lufs
        );

        let mut normalized = quiet_stereo.clone();
        let after = normalize_loudness_lufs(&mut normalized, -16.0, -1.0)?;
        assert!(
            (after.integrated_lufs - (-16.0)).abs() < 1.2,
            "expected normalized LUFS near -16.0, got {}",
            after.integrated_lufs
        );
        assert!(after.sample_peak_dbfs <= -0.99);

        let resampled = resample_pcm(&quiet_stereo, 24_000)?;
        assert_eq!(resampled.sample_rate, 24_000);
        assert_eq!(resampled.channels, 2);
        assert_eq!(resampled.frame_count(), quiet_stereo.frame_count() / 2);

        let mono = remap_channels(&resampled, 1)?;
        assert_eq!(mono.channels, 1);
        assert_eq!(mono.frame_count(), resampled.frame_count());
        Ok(())
    }

    #[test]
    fn id3_metadata_roundtrip_and_privacy_strip() -> Result<(), Box<dyn Error>> {
        let dir = unique_temp_dir("id3_tags");
        let mp3_path = dir.join("sample.mp3");
        // Minimal valid MPEG-1 Layer 3 128 kbps 44.1 kHz frame header + zero padding.
        let mut fake_mp3 = vec![0u8; 417];
        fake_mp3[0] = 0xFF;
        fake_mp3[1] = 0xFB;
        fake_mp3[2] = 0x90;
        fake_mp3[3] = 0x00;
        fs::write(&mp3_path, fake_mp3)?;

        let input_meta = AudioTagMetadata {
            title: Some(String::from("FileFusion Mastering")),
            artist: Some(String::from("Studio Test")),
            album: Some(String::from("Stage 2 Corpus")),
            year: Some(2026),
            track: Some(1),
            has_embedded_artwork: false,
        };
        write_id3_metadata(&mp3_path, &input_meta)?;
        let read_back = read_id3_metadata(&mp3_path)?;
        assert_eq!(read_back, input_meta);
        assert!(has_non_essential_metadata(&mp3_path, FormatId::Mp3));

        let removed = strip_id3_metadata(&mp3_path)?;
        assert!(removed);
        assert!(!has_non_essential_metadata(&mp3_path, FormatId::Mp3));

        // Verify FLAC metadata block parser distinguishes clean STREAMINFO vs VORBIS_COMMENT (type 4).
        let clean_flac = dir.join("clean.flac");
        let mut clean_bytes = Vec::from(*b"fLaC");
        clean_bytes.push(0x80); // last-metadata-block = 1, type = 0 (STREAMINFO)
        clean_bytes.extend_from_slice(&[0x00, 0x00, 0x22]); // 34 bytes
        clean_bytes.extend_from_slice(&[0u8; 34]);
        fs::write(&clean_flac, &clean_bytes)?;
        assert!(!has_non_essential_metadata(&clean_flac, FormatId::Flac));

        let tagged_flac = dir.join("tagged.flac");
        let mut tagged_bytes = Vec::from(*b"fLaC");
        tagged_bytes.push(0x00); // not last, STREAMINFO
        tagged_bytes.extend_from_slice(&[0x00, 0x00, 0x22]);
        tagged_bytes.extend_from_slice(&[0u8; 34]);
        tagged_bytes.push(0x84); // last-metadata-block = 1, type = 4 (VORBIS_COMMENT)
        tagged_bytes.extend_from_slice(&[0x00, 0x00, 0x08]);
        tagged_bytes.extend_from_slice(&[0u8; 8]);
        fs::write(&tagged_flac, &tagged_bytes)?;
        assert!(has_non_essential_metadata(&tagged_flac, FormatId::Flac));

        let _ = fs::remove_dir_all(&dir);
        Ok(())
    }

    #[cfg(feature = "native-process")]
    #[test]
    fn builds_sanitized_ffmpeg_transcode_and_extraction_args() -> Result<(), Box<dyn Error>> {
        let req = CompressionRequest {
            profile: CompressionProfile::WebSharingReady,
            target_format: Some(FormatId::Opus),
            target_max_bytes: None,
            min_ssim: None,
            strip_metadata: true,
            no_op_if_larger: true,
        };
        let opts = AudioCompressionOptions {
            base: req,
            target_sample_rate: Some(48_000),
            target_channels: Some(1),
            target_lufs: Some(-16.0),
            target_bitrate_kbps: None,
        };
        let inv = build_ffmpeg_audio_transcode_invocation(
            Path::new("ffmpeg"),
            Path::new("lecture.mp4"),
            Path::new(".filefusion.tmp.01"),
            FormatId::Opus,
            &opts,
            60,
        )?;
        let arg_strs: Vec<String> = inv
            .args
            .iter()
            .map(|a| a.to_string_lossy().into_owned())
            .collect();
        assert!(arg_strs.contains(&String::from("libopus")));
        assert!(arg_strs.contains(&String::from("-af")));
        assert!(arg_strs.iter().any(|s| s.contains("loudnorm=I=-16.0")));
        assert!(arg_strs.contains(&String::from("-map_metadata")));
        assert!(arg_strs.windows(2).any(|w| w == ["-f", "opus"]));

        let inv_m4a = build_ffmpeg_audio_transcode_invocation(
            Path::new("ffmpeg"),
            Path::new("song.wav"),
            Path::new(".filefusion.tmp.02"),
            FormatId::M4a,
            &opts,
            60,
        )?;
        let m4a_args: Vec<String> = inv_m4a
            .args
            .iter()
            .map(|a| a.to_string_lossy().into_owned())
            .collect();
        assert!(m4a_args.windows(2).any(|w| w == ["-f", "ipod"]));

        let remux = build_ffmpeg_extract_audio_invocation(
            Path::new("ffmpeg"),
            Path::new("clip.mp4"),
            Path::new(".filefusion.tmp.03"),
            FormatId::Aac,
            true,
            60,
        )?;
        let remux_args: Vec<String> = remux
            .args
            .iter()
            .map(|a| a.to_string_lossy().into_owned())
            .collect();
        assert!(remux_args.contains(&String::from("copy")));
        assert!(remux_args.windows(2).any(|w| w == ["-f", "adts"]));
        Ok(())
    }
}
