//! Pure-Rust subtitle parsing and conversion (`SRT` <-> `WebVTT`) plus Tier 2 `FFmpeg`
//! embedded subtitle track extraction (`FR-AV-003`).

use std::fmt::Write as _;

use file_core::{ErrorCode, FileFusionError, FileFusionResult};
use serde::{Deserialize, Serialize};

#[cfg(feature = "native-process")]
use std::ffi::OsString;
#[cfg(feature = "native-process")]
use std::path::Path;

#[cfg(feature = "native-process")]
use file_core::ProcessInvocation;

/// Normalized plain-text subtitle formats supported cleanly in Tier 1 Rust (`FR-AV-003`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TextSubtitleFormat {
    /// `SubRip` (`00:00:01,500 --> 00:00:03,000`).
    Srt,
    /// W3C `WebVTT` (`WEBVTT`, `00:00:01.500 --> 00:00:03.000`).
    WebVtt,
}

/// Timed subtitle cue entry.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SubtitleCue {
    /// One-based cue index.
    pub index: usize,
    /// Cue start timestamp in milliseconds.
    pub start_ms: u64,
    /// Cue end timestamp in milliseconds.
    pub end_ms: u64,
    /// Plain/formatted cue body lines joined by `\n`.
    pub text: String,
}

/// Parses an SRT (`SubRip`) subtitle document into typed [`SubtitleCue`] records.
///
/// # Errors
/// Returns [`FileFusionError`] (`FmtCorrupt`) if a cue timing arrow or timestamp is invalid.
pub fn parse_srt_cues(input: &str) -> FileFusionResult<Vec<SubtitleCue>> {
    parse_cue_blocks(input, false)
}

/// Parses a `WebVTT` subtitle document into typed [`SubtitleCue`] records.
///
/// # Errors
/// Returns [`FileFusionError`] (`FmtCorrupt`) if the `WEBVTT` header or cue timing is invalid.
pub fn parse_webvtt_cues(input: &str) -> FileFusionResult<Vec<SubtitleCue>> {
    let trimmed = input.trim_start_matches('\u{feff}').trim_start();
    if !trimmed.starts_with("WEBVTT") {
        return Err(FileFusionError::new(
            ErrorCode::FmtCorrupt,
            "WebVTT stream missing mandatory WEBVTT header",
        ));
    }
    let after_header = trimmed
        .split_once('\n')
        .map_or("", |(_, rest)| rest.trim_start());
    parse_cue_blocks(after_header, true)
}

/// Serializes [`SubtitleCue`] items into canonical `SubRip` (`SRT`) text.
#[must_use]
pub fn format_cues_as_srt(cues: &[SubtitleCue]) -> String {
    let mut out = String::new();
    for (idx, cue) in cues.iter().enumerate() {
        if idx > 0 {
            out.push('\n');
        }
        let num = idx + 1;
        let start = format_timestamp_ms(cue.start_ms, ',');
        let end = format_timestamp_ms(cue.end_ms, ',');
        let _ = write!(out, "{num}\n{start} --> {end}\n{}\n", cue.text);
    }
    out
}

/// Serializes [`SubtitleCue`] items into canonical `WebVTT` text.
#[must_use]
pub fn format_cues_as_webvtt(cues: &[SubtitleCue]) -> String {
    let mut out = String::from("WEBVTT\n\n");
    for (idx, cue) in cues.iter().enumerate() {
        if idx > 0 {
            out.push('\n');
        }
        let start = format_timestamp_ms(cue.start_ms, '.');
        let end = format_timestamp_ms(cue.end_ms, '.');
        let _ = write!(out, "{start} --> {end}\n{}\n", cue.text);
    }
    out
}

/// Converts subtitle text losslessly between [`TextSubtitleFormat::Srt`] and
/// [`TextSubtitleFormat::WebVtt`] (`FR-AV-003`).
///
/// # Errors
/// Returns [`FileFusionError`] if source subtitle syntax cannot be parsed.
pub fn convert_subtitle_text(
    input: &str,
    from: TextSubtitleFormat,
    to: TextSubtitleFormat,
) -> FileFusionResult<String> {
    let cues = match from {
        TextSubtitleFormat::Srt => parse_srt_cues(input)?,
        TextSubtitleFormat::WebVtt => parse_webvtt_cues(input)?,
    };
    Ok(match to {
        TextSubtitleFormat::Srt => format_cues_as_srt(&cues),
        TextSubtitleFormat::WebVtt => format_cues_as_webvtt(&cues),
    })
}

/// Builds a sanitized Tier 2 `FFmpeg` invocation to extract an embedded subtitle track from
/// a video container (`FR-AV-003`, `MOD-RULE-05`).
#[cfg(feature = "native-process")]
#[must_use]
pub fn build_ffmpeg_extract_subtitle_invocation(
    ffmpeg_binary: &Path,
    media_path: &Path,
    output_path: &Path,
    stream_index: u32,
    target_format: Option<TextSubtitleFormat>,
    timeout_secs: u64,
) -> ProcessInvocation {
    let map_spec = format!("0:s:{stream_index}");
    let mut args = vec![
        OsString::from("-hide_banner"),
        OsString::from("-loglevel"),
        OsString::from("error"),
        OsString::from("-y"),
        OsString::from("-i"),
        media_path.as_os_str().to_os_string(),
        OsString::from("-map"),
        OsString::from(map_spec),
        OsString::from("-c:s"),
    ];
    match target_format {
        Some(TextSubtitleFormat::Srt) => {
            args.push(OsString::from("srt"));
            args.push(OsString::from("-f"));
            args.push(OsString::from("srt"));
        }
        Some(TextSubtitleFormat::WebVtt) => {
            args.push(OsString::from("webvtt"));
            args.push(OsString::from("-f"));
            args.push(OsString::from("webvtt"));
        }
        None => args.push(OsString::from("copy")),
    }
    args.push(output_path.as_os_str().to_os_string());

    ProcessInvocation {
        executable: ffmpeg_binary.to_path_buf(),
        args,
        working_dir: None,
        timeout_secs,
        allow_env_keys: Vec::new(),
    }
}

fn parse_cue_blocks(body: &str, is_vtt: bool) -> FileFusionResult<Vec<SubtitleCue>> {
    let normalized = body.replace("\r\n", "\n");
    let mut cues = Vec::new();
    for raw_block in normalized.split("\n\n") {
        let lines: Vec<&str> = raw_block
            .lines()
            .map(str::trim_end)
            .filter(|l| !l.trim().is_empty())
            .collect();
        if lines.is_empty() {
            continue;
        }
        if is_vtt && lines[0].starts_with("NOTE") {
            continue;
        }

        let (timing_line, text_lines) = if lines[0].contains("-->") {
            (lines[0], &lines[1..])
        } else if lines.len() >= 2 && lines[1].contains("-->") {
            (lines[1], &lines[2..])
        } else {
            return Err(FileFusionError::new(
                ErrorCode::FmtCorrupt,
                format!(
                    "subtitle block missing `-->` timing separator: `{}`",
                    lines[0]
                ),
            ));
        };

        let (start_str, rest_end) = timing_line.split_once("-->").ok_or_else(|| {
            FileFusionError::new(ErrorCode::FmtCorrupt, "invalid subtitle timing separator")
        })?;
        let end_token = rest_end.split_whitespace().next().unwrap_or("");
        let start_ms = parse_timestamp_ms(start_str.trim())?;
        let end_ms = parse_timestamp_ms(end_token)?;
        cues.push(SubtitleCue {
            index: cues.len() + 1,
            start_ms,
            end_ms,
            text: text_lines.join("\n"),
        });
    }
    Ok(cues)
}

fn parse_timestamp_ms(ts: &str) -> FileFusionResult<u64> {
    let clean = ts.replace(',', ".");
    let (hms, frac) = clean.split_once('.').unwrap_or((clean.as_str(), "000"));
    let parts: Vec<&str> = hms.split(':').collect();
    let (h, m, s): (u64, u64, u64) = match parts.as_slice() {
        [hh, mm, ss] => (parse_u64(hh)?, parse_u64(mm)?, parse_u64(ss)?),
        [mm, ss] => (0, parse_u64(mm)?, parse_u64(ss)?),
        _ => {
            return Err(FileFusionError::new(
                ErrorCode::FmtCorrupt,
                format!("invalid subtitle timestamp `{ts}`"),
            ));
        }
    };
    let frac_trimmed = frac.trim();
    if !frac_trimmed.bytes().all(|b| b.is_ascii_digit()) {
        return Err(FileFusionError::new(
            ErrorCode::FmtCorrupt,
            format!("invalid fractional subtitle timestamp `{ts}`"),
        ));
    }
    let padded = format!("{frac_trimmed:0<3}");
    let ms_str: String = padded.chars().take(3).collect();
    let ms = parse_u64(&ms_str)?;
    Ok(((h * 3600) + (m * 60) + s) * 1000 + ms)
}

fn parse_u64(s: &str) -> FileFusionResult<u64> {
    s.trim().parse::<u64>().map_err(|_| {
        FileFusionError::new(
            ErrorCode::FmtCorrupt,
            format!("invalid numeric subtitle timestamp token `{s}`"),
        )
    })
}

fn format_timestamp_ms(total_ms: u64, sep: char) -> String {
    let ms = total_ms % 1000;
    let total_secs = total_ms / 1000;
    let secs = total_secs % 60;
    let total_mins = total_secs / 60;
    let mins = total_mins % 60;
    let hours = total_mins / 60;
    format!("{hours:02}:{mins:02}:{secs:02}{sep}{ms:03}")
}
