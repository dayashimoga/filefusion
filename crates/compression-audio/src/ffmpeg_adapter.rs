//! Sanitized Tier 2 `FFmpeg` adapter for lossy audio transcoding (`Opus`, `AAC`, `MP3`,
//! `Vorbis`), ALAC packing, EBU R128 `loudnorm`, and video audio-track extraction
//! (`FR-AV-002`, `FR-AV-003`, `MOD-RULE-05`).

use std::ffi::OsString;
use std::path::Path;

use compression_core::CompressionProfileConfig;
use file_core::{
    CompressionRequest, ErrorCode, FileFusionError, FileFusionResult, FormatId,
    IsolatedProcessRunner, JobContext, ProcessInvocation,
};

use crate::AudioCompressionOptions;

/// Builds a sanitized argument vector (`OsString` array, zero shell strings) for Tier 2 `FFmpeg`
/// audio transcoding and optional EBU R128 loudness normalization (`FR-AV-002`).
///
/// # Errors
/// Returns [`FileFusionError`] if `target_format` does not map to a supported `FFmpeg` audio codec.
pub fn build_ffmpeg_audio_transcode_invocation(
    ffmpeg_binary: &Path,
    input_path: &Path,
    output_path: &Path,
    target_format: FormatId,
    opts: &AudioCompressionOptions,
    timeout_secs: u64,
) -> FileFusionResult<ProcessInvocation> {
    let req = &opts.base;
    let (codec_arg, default_kbps) = ffmpeg_codec_for_target(target_format, req)?;
    let muxer = ffmpeg_muxer_for_target(target_format)?;
    let bitrate_kbps = opts.target_bitrate_kbps.or(default_kbps);

    let mut args = vec![
        OsString::from("-hide_banner"),
        OsString::from("-loglevel"),
        OsString::from("error"),
        OsString::from("-y"),
        OsString::from("-i"),
        input_path.as_os_str().to_os_string(),
        OsString::from("-vn"),
    ];

    if let Some(lufs) = opts.target_lufs {
        let clamped = lufs.clamp(-36.0, -5.0);
        args.push(OsString::from("-af"));
        args.push(OsString::from(format!(
            "loudnorm=I={clamped:.1}:TP=-1.0:LRA=11"
        )));
    }
    if let Some(rate) = opts.target_sample_rate {
        args.push(OsString::from("-ar"));
        args.push(OsString::from(rate.to_string()));
    }
    if let Some(ch) = opts.target_channels {
        args.push(OsString::from("-ac"));
        args.push(OsString::from(ch.to_string()));
    }

    args.push(OsString::from("-c:a"));
    args.push(OsString::from(codec_arg));
    if let Some(kbps) = bitrate_kbps {
        args.push(OsString::from("-b:a"));
        args.push(OsString::from(format!("{kbps}k")));
    }
    if req.strip_metadata {
        args.push(OsString::from("-map_metadata"));
        args.push(OsString::from("-1"));
    }
    args.push(OsString::from("-f"));
    args.push(OsString::from(muxer));
    args.push(output_path.as_os_str().to_os_string());

    Ok(ProcessInvocation {
        executable: ffmpeg_binary.to_path_buf(),
        args,
        working_dir: None,
        timeout_secs,
        allow_env_keys: Vec::new(),
    })
}

/// Builds a sanitized `FFmpeg` invocation to extract an audio stream from a media container,
/// using lossless `-c:a copy` fast-path remuxing when `stream_copy` is `true` (`FR-AV-003`).
///
/// # Errors
/// Returns [`FileFusionError`] if `target_format` is unsupported when re-encoding.
pub fn build_ffmpeg_extract_audio_invocation(
    ffmpeg_binary: &Path,
    media_path: &Path,
    output_path: &Path,
    target_format: FormatId,
    stream_copy: bool,
    timeout_secs: u64,
) -> FileFusionResult<ProcessInvocation> {
    let muxer = ffmpeg_muxer_for_target(target_format)?;
    let mut args = vec![
        OsString::from("-hide_banner"),
        OsString::from("-loglevel"),
        OsString::from("error"),
        OsString::from("-y"),
        OsString::from("-i"),
        media_path.as_os_str().to_os_string(),
        OsString::from("-vn"),
        OsString::from("-c:a"),
    ];

    if stream_copy {
        args.push(OsString::from("copy"));
    } else {
        let codec_name = match target_format {
            FormatId::Opus => "libopus",
            FormatId::Aac | FormatId::M4a => "aac",
            FormatId::Mp3 => "libmp3lame",
            FormatId::Ogg => "libvorbis",
            FormatId::Flac => "flac",
            FormatId::Wav => "pcm_s16le",
            other => {
                return Err(FileFusionError::new(
                    ErrorCode::FmtUnsupported,
                    format!("unsupported FFmpeg audio extraction target {other:?}"),
                ));
            }
        };
        args.push(OsString::from(codec_name));
    }

    args.push(OsString::from("-f"));
    args.push(OsString::from(muxer));
    args.push(output_path.as_os_str().to_os_string());

    Ok(ProcessInvocation {
        executable: ffmpeg_binary.to_path_buf(),
        args,
        working_dir: None,
        timeout_secs,
        allow_env_keys: Vec::new(),
    })
}

/// Executes a pre-built audio transcode or extraction [`ProcessInvocation`] via
/// [`IsolatedProcessRunner`] (`MOD-RULE-05`).
///
/// # Errors
/// Returns [`FileFusionError`] on process spawn error, non-zero exit code, or cancellation.
pub fn execute_ffmpeg_invocation(
    runner: &dyn IsolatedProcessRunner,
    invocation: &ProcessInvocation,
    ctx: &JobContext,
) -> FileFusionResult<()> {
    ctx.check_cancelled()?;
    let out = runner.run_isolated(invocation, ctx)?;
    if out.exit_code != 0 {
        return Err(FileFusionError::new(
            ErrorCode::IntInvariant,
            format!(
                "tier2 ffmpeg audio job exited with code {}: {}",
                out.exit_code,
                String::from_utf8_lossy(&out.stderr).trim()
            ),
        ));
    }
    Ok(())
}

fn ffmpeg_codec_for_target(
    target_format: FormatId,
    req: &CompressionRequest,
) -> FileFusionResult<(&'static str, Option<u32>)> {
    let cfg = CompressionProfileConfig::for_profile(req.profile);
    let opus_kbps = match cfg.lossy_quality_q {
        90..=100 => 192,
        80..=89 => 128,
        70..=79 => 96,
        _ => 64,
    };
    let aac_kbps = match cfg.lossy_quality_q {
        90..=100 => 256,
        80..=89 => 192,
        70..=79 => 128,
        _ => 96,
    };

    match target_format {
        FormatId::Opus => Ok(("libopus", Some(opus_kbps))),
        FormatId::Aac => Ok(("aac", Some(aac_kbps))),
        FormatId::M4a => {
            if cfg.lossless_preferred {
                Ok(("alac", None))
            } else {
                Ok(("aac", Some(aac_kbps)))
            }
        }
        FormatId::Mp3 => Ok(("libmp3lame", Some(aac_kbps))),
        FormatId::Ogg => Ok(("libvorbis", Some(opus_kbps))),
        FormatId::Flac => Ok(("flac", None)),
        FormatId::Wav => Ok(("pcm_s16le", None)),
        other => Err(FileFusionError::new(
            ErrorCode::FmtUnsupported,
            format!("unsupported Tier 2 FFmpeg audio format {other:?}"),
        )),
    }
}

fn ffmpeg_muxer_for_target(target_format: FormatId) -> FileFusionResult<&'static str> {
    match target_format {
        FormatId::Opus => Ok("opus"),
        FormatId::Ogg => Ok("ogg"),
        FormatId::Aac => Ok("adts"),
        FormatId::M4a => Ok("ipod"),
        FormatId::Mp3 => Ok("mp3"),
        FormatId::Flac => Ok("flac"),
        FormatId::Wav => Ok("wav"),
        other => Err(FileFusionError::new(
            ErrorCode::FmtUnsupported,
            format!("unsupported Tier 2 FFmpeg muxer for {other:?}"),
        )),
    }
}
