//! Video container structure probing, `ffprobe` stream inventory parsing,
//! fast-path remux compatibility checks, and unfinalized container diagnostics
//! (`FR-DET-003`, `FR-AV-001`, `FR-REP-001`).

use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::Path;

use file_core::{ErrorCode, FileFusionError, FileFusionResult, FormatId};
use serde::{Deserialize, Serialize};

#[cfg(feature = "native-process")]
use std::ffi::OsString;

#[cfg(feature = "native-process")]
use file_core::{IsolatedProcessRunner, JobContext, ProcessInvocation};

/// Video elementary stream descriptor parsed from `ffprobe` JSON.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VideoStreamDescriptor {
    /// Zero-based container stream index.
    pub index: u32,
    /// Normalized codec identifier (`h264`, `hevc`, `vp9`, `av1`, etc.).
    pub codec_name: String,
    /// Video frame width in pixels.
    pub width: u32,
    /// Video frame height in pixels.
    pub height: u32,
    /// Pixel format string if reported (e.g., `yuv420p`).
    pub pix_fmt: Option<String>,
    /// Bitrate in bits/sec if reported.
    pub bit_rate_bps: Option<u64>,
}

/// Audio elementary stream descriptor inside a video container.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MediaAudioStreamDescriptor {
    /// Zero-based container stream index.
    pub index: u32,
    /// Normalized codec identifier (`aac`, `opus`, `vorbis`, `mp3`, `ac3`, `flac`).
    pub codec_name: String,
    /// Sample rate in Hz if reported.
    pub sample_rate_hz: Option<u32>,
    /// Channel count if reported.
    pub channels: Option<u16>,
}

/// Subtitle track descriptor inside a video container.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SubtitleStreamDescriptor {
    /// Zero-based container stream index.
    pub index: u32,
    /// Codec identifier (`subrip`/`srt`, `webvtt`, `ass`, `ssa`, `mov_text`).
    pub codec_name: String,
    /// ISO-639 language tag if present.
    pub language: Option<String>,
}

/// Complete stream inventory parsed from `ffprobe` output (`FR-DET-003`).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct VideoContainerInventory {
    /// `FFprobe` format short name (`mov,mp4,m4a,3gp,3g2,mj2`, `matroska,webm`, `avi`, etc.).
    pub format_name: String,
    /// Media duration in seconds if reported.
    pub duration_secs: Option<f64>,
    /// Aggregate container bit rate in bits/sec if reported.
    pub bit_rate_bps: Option<u64>,
    /// Video tracks discovered.
    pub video_streams: Vec<VideoStreamDescriptor>,
    /// Audio tracks discovered.
    pub audio_streams: Vec<MediaAudioStreamDescriptor>,
    /// Subtitle tracks discovered.
    pub subtitle_streams: Vec<SubtitleStreamDescriptor>,
}

#[derive(Deserialize)]
struct RawFfprobeOutput {
    format: Option<RawFfprobeFormat>,
    #[serde(default)]
    streams: Vec<RawFfprobeStream>,
}

#[derive(Deserialize)]
struct RawFfprobeFormat {
    format_name: Option<String>,
    duration: Option<String>,
    bit_rate: Option<String>,
}

#[derive(Deserialize)]
struct RawFfprobeStream {
    index: Option<u32>,
    codec_type: Option<String>,
    codec_name: Option<String>,
    width: Option<u32>,
    height: Option<u32>,
    pix_fmt: Option<String>,
    sample_rate: Option<String>,
    channels: Option<u16>,
    bit_rate: Option<String>,
    tags: Option<RawStreamTags>,
}

#[derive(Deserialize)]
struct RawStreamTags {
    language: Option<String>,
}

/// Parses `ffprobe -v quiet -print_format json -show_format -show_streams` stdout into a typed
/// [`VideoContainerInventory`].
///
/// # Errors
/// Returns [`FileFusionError`] (`FmtCorrupt`) if JSON payload is malformed.
pub fn parse_ffprobe_json_probe(json_text: &str) -> FileFusionResult<VideoContainerInventory> {
    let raw: RawFfprobeOutput = serde_json::from_str(json_text).map_err(|err| {
        FileFusionError::new(
            ErrorCode::FmtCorrupt,
            format!("invalid ffprobe JSON inventory: {err}"),
        )
    })?;

    let mut inv = VideoContainerInventory::default();
    if let Some(fmt) = raw.format {
        inv.format_name = fmt.format_name.unwrap_or_default();
        inv.duration_secs = fmt.duration.and_then(|d| d.parse::<f64>().ok());
        inv.bit_rate_bps = fmt.bit_rate.and_then(|b| b.parse::<u64>().ok());
    }

    for stream in raw.streams {
        let idx = stream.index.unwrap_or(0);
        let codec = stream.codec_name.unwrap_or_default().to_ascii_lowercase();
        match stream.codec_type.as_deref() {
            Some("video") => inv.video_streams.push(VideoStreamDescriptor {
                index: idx,
                codec_name: codec,
                width: stream.width.unwrap_or(0),
                height: stream.height.unwrap_or(0),
                pix_fmt: stream.pix_fmt,
                bit_rate_bps: stream.bit_rate.and_then(|b| b.parse::<u64>().ok()),
            }),
            Some("audio") => inv.audio_streams.push(MediaAudioStreamDescriptor {
                index: idx,
                codec_name: codec,
                sample_rate_hz: stream.sample_rate.and_then(|r| r.parse::<u32>().ok()),
                channels: stream.channels,
            }),
            Some("subtitle") => inv.subtitle_streams.push(SubtitleStreamDescriptor {
                index: idx,
                codec_name: codec,
                language: stream.tags.and_then(|t| t.language),
            }),
            _ => {}
        }
    }

    Ok(inv)
}

/// Builds a sanitized `ffprobe` invocation for stream inventory (`MOD-RULE-05`).
#[cfg(feature = "native-process")]
#[must_use]
pub fn build_ffprobe_invocation(
    ffprobe_binary: &Path,
    media_path: &Path,
    timeout_secs: u64,
) -> ProcessInvocation {
    ProcessInvocation {
        executable: ffprobe_binary.to_path_buf(),
        args: vec![
            OsString::from("-v"),
            OsString::from("quiet"),
            OsString::from("-print_format"),
            OsString::from("json"),
            OsString::from("-show_format"),
            OsString::from("-show_streams"),
            media_path.as_os_str().to_os_string(),
        ],
        working_dir: None,
        timeout_secs,
        allow_env_keys: Vec::new(),
    }
}

/// Runs `ffprobe` via [`IsolatedProcessRunner`] and returns parsed [`VideoContainerInventory`].
///
/// # Errors
/// Returns [`FileFusionError`] on process error, cancellation, or malformed JSON output.
#[cfg(feature = "native-process")]
pub fn probe_video_with_ffprobe(
    runner: &dyn IsolatedProcessRunner,
    ffprobe_binary: &Path,
    media_path: &Path,
    ctx: &JobContext,
) -> FileFusionResult<VideoContainerInventory> {
    ctx.check_cancelled()?;
    let inv = build_ffprobe_invocation(
        ffprobe_binary,
        media_path,
        ctx.limits.subprocess_timeout_secs,
    );
    let out = runner.run_isolated(&inv, ctx)?;
    if out.exit_code != 0 {
        return Err(FileFusionError::new(
            ErrorCode::FmtCorrupt,
            format!(
                "ffprobe exited with code {}: {}",
                out.exit_code,
                String::from_utf8_lossy(&out.stderr).trim()
            ),
        ));
    }
    let json = String::from_utf8_lossy(&out.stdout);
    parse_ffprobe_json_probe(&json)
}

/// Evaluates whether an existing video + audio stream set can be **losslessly remuxed**
/// (`-c:v copy -c:a copy`) into `target_container` without re-encoding (`FR-AV-001`).
#[must_use]
pub fn can_fast_path_remux(
    inventory: &VideoContainerInventory,
    target_container: FormatId,
) -> bool {
    let Some(primary_v) = inventory.video_streams.first() else {
        return false;
    };
    let video_ok = match target_container {
        FormatId::Mp4 | FormatId::Mov => {
            matches!(
                primary_v.codec_name.as_str(),
                "h264" | "hevc" | "h265" | "av1"
            )
        }
        FormatId::Mkv => matches!(
            primary_v.codec_name.as_str(),
            "h264" | "hevc" | "h265" | "vp8" | "vp9" | "av1" | "mpeg4"
        ),
        FormatId::Webm => matches!(primary_v.codec_name.as_str(), "vp8" | "vp9" | "av1"),
        FormatId::Avi => matches!(primary_v.codec_name.as_str(), "h264" | "mpeg4" | "mjpeg"),
        _ => false,
    };
    if !video_ok {
        return false;
    }

    inventory
        .audio_streams
        .iter()
        .all(|a| match target_container {
            FormatId::Mp4 | FormatId::Mov => {
                matches!(
                    a.codec_name.as_str(),
                    "aac" | "mp3" | "alac" | "ac3" | "flac"
                )
            }
            FormatId::Mkv => true,
            FormatId::Webm => matches!(a.codec_name.as_str(), "opus" | "vorbis"),
            FormatId::Avi => matches!(a.codec_name.as_str(), "mp3" | "pcm_s16le" | "aac" | "ac3"),
            _ => false,
        })
}

/// Inspects bounded leading header bytes and top-level ISO-BMFF container boxes for MP4 `moov`
/// or EBML root health, returning a human-readable integrity issue if unfinalized/truncated
/// (`FR-REP-001`).
///
/// # Errors
/// Returns [`FileFusionError`] on I/O errors while opening or seeking the media container.
pub fn diagnose_unfinalized_video_container(
    path: &Path,
    format: FormatId,
) -> FileFusionResult<Option<String>> {
    let mut file = File::open(path)?;
    let meta = file.metadata()?;
    let file_len = meta.len();
    if file_len < 16 {
        return Ok(Some(String::from(
            "Video container header is truncated (< 16 bytes).",
        )));
    }

    let probe_len = usize::try_from(file_len.min(128 * 1024)).unwrap_or(128 * 1024);
    let mut head = vec![0u8; probe_len];
    let n = file.read(&mut head)?;
    let slice = &head[..n];

    match format {
        FormatId::Mp4 | FormatId::Mov => {
            let has_ftyp = slice.windows(4).any(|w| w == b"ftyp");
            if !has_ftyp {
                return Ok(Some(String::from(
                    "ISO-BMFF container missing mandatory `ftyp` header box.",
                )));
            }
            if !iso_bmff_has_moov_box(&mut file, file_len)? {
                return Ok(Some(String::from(
                    "ISO-BMFF container missing `moov` index atom (unfinalized capture).",
                )));
            }
            Ok(None)
        }
        FormatId::Mkv | FormatId::Webm => {
            if !slice.starts_with(&[0x1A, 0x45, 0xDF, 0xA3]) {
                return Ok(Some(String::from(
                    "Matroska/WebM file missing EBML root element magic bytes.",
                )));
            }
            Ok(None)
        }
        _ => Ok(None),
    }
}

fn iso_bmff_has_moov_box(file: &mut File, file_len: u64) -> FileFusionResult<bool> {
    const MAX_BOX_STEPS: usize = 256;
    file.seek(SeekFrom::Start(0))?;
    let mut offset = 0u64;
    let mut hdr = [0u8; 8];
    for _ in 0..MAX_BOX_STEPS {
        if offset.saturating_add(8) > file_len {
            return Ok(false);
        }
        if file.read_exact(&mut hdr).is_err() {
            return Ok(false);
        }
        let size32 = u32::from_be_bytes([hdr[0], hdr[1], hdr[2], hdr[3]]);
        let box_type = &hdr[4..8];
        if box_type == b"moov" {
            return Ok(true);
        }
        let box_size = if size32 == 0 {
            return Ok(false);
        } else if size32 == 1 {
            if offset.saturating_add(16) > file_len {
                return Ok(false);
            }
            let mut ext = [0u8; 8];
            if file.read_exact(&mut ext).is_err() {
                return Ok(false);
            }
            let s64 = u64::from_be_bytes(ext);
            if s64 < 16 {
                return Ok(false);
            }
            s64
        } else {
            let s = u64::from(size32);
            if s < 8 {
                return Ok(false);
            }
            s
        };
        offset = offset.saturating_add(box_size);
        if offset >= file_len {
            return Ok(false);
        }
        file.seek(SeekFrom::Start(offset))?;
    }
    Ok(false)
}
