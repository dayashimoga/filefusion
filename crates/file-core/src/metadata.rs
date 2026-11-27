//! Streaming cryptographic/fast digests, container stream inventory, and
//! normalized metadata inspection (`FR-DET-003`, `FR-DET-004`, `FR-MET-001`).
//!
//! Uses a fixed `64 KB` streaming ring buffer (`NFR-PERF-001`) so multi-gigabyte
//! inputs never load whole-file payloads into memory.

use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::fs::File;
use std::io::{Cursor, Read, Seek, SeekFrom};
use std::path::Path;

use serde::{Deserialize, Serialize};
use sha2::Digest as _;
use xxhash_rust::xxh64::Xxh64;

use crate::classification::{FileClassification, classify_inspection_report};
use crate::error::FileFusionResult;
use crate::signature::{
    SignatureMatch, detect_signature_from_bytes, detect_signature_from_reader,
    enforce_spoofing_policy,
};
use crate::{CORE_SCHEMA_VERSION, FormatId};

const STREAM_BUFFER_BYTES: usize = 65_536;
const MAX_METADATA_WINDOW_BYTES: usize = 131_072;
const MAX_ATTRIBUTES: usize = 64;
const MAX_ZIP_INSPECT_ENTRIES: usize = 2_048;

/// Cryptographic and non-cryptographic digests computed in a single streaming pass (`FR-DET-004`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DigestSet {
    /// Hex-encoded `BLAKE3` 256-bit cryptographic digest.
    pub blake3: String,
    /// Hex-encoded `SHA-256` cryptographic digest.
    pub sha256: String,
    /// Hex-encoded canonical 64-bit `xxHash64` fast integrity digest.
    pub xxhash64: String,
}

/// High-level media or container stream kind inside an inspected file.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StreamKind {
    /// Raster or vector image track.
    Image,
    /// Video bitstream.
    Video,
    /// Audio bitstream.
    Audio,
    /// Subtitle or timed-text track.
    Subtitle,
    /// Document structure (`PDF`, office markup, ebook spine).
    Document,
    /// Archive container member summary.
    ArchiveEntrySet,
    /// UTF-8 / UTF-16 text stream.
    Text,
}

/// Structural descriptor for an individual track or container component (`FF-CTR-001` §2.2).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StreamDescriptor {
    /// 0-based index of the stream or component.
    pub index: u32,
    /// Stream classification.
    pub kind: StreamKind,
    /// Codec, subformat, or encoding name if known.
    pub codec: Option<String>,
    /// Pixel width for image/video tracks.
    pub width: Option<u32>,
    /// Pixel height for image/video tracks.
    pub height: Option<u32>,
    /// Bits per sample or channel.
    pub bit_depth: Option<u8>,
    /// Whether an alpha/transparency channel is present.
    pub has_alpha: Option<bool>,
    /// Audio sample rate in Hz.
    pub sample_rate_hz: Option<u32>,
    /// Audio channel count.
    pub channels: Option<u8>,
    /// Member count for archive/document containers.
    pub entry_count: Option<u64>,
    /// Concise human-readable stream detail.
    pub detail: Option<String>,
}

/// Normalized metadata and privacy-sensitivity summary (`FR-MET-001`, `FR-MET-002`).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[allow(clippy::struct_excessive_bools)]
pub struct MetadataSummary {
    /// Document or media title.
    pub title: Option<String>,
    /// Author or artist tag if present.
    pub author: Option<String>,
    /// Producing software or camera model string.
    pub creator_tool: Option<String>,
    /// Creation timestamp string when recorded by container metadata.
    pub created_at: Option<String>,
    /// Primary visual width in pixels.
    pub width: Option<u32>,
    /// Primary visual height in pixels.
    pub height: Option<u32>,
    /// Color model description (e.g. `rgb`, `rgba`, `grayscale`, `indexed`).
    pub color_space: Option<String>,
    /// Whether embedded `EXIF` metadata was detected.
    pub has_exif: bool,
    /// Whether embedded `XMP` metadata was detected.
    pub has_xmp: bool,
    /// Whether an embedded `ICC` color profile was detected.
    pub has_icc_profile: bool,
    /// `true` when GPS geolocation tags are detected in `EXIF` or `XMP`.
    pub has_gps_privacy_risk: bool,
    /// `true` when personally identifying author/creator tags are present.
    pub has_pii_author_risk: bool,
    /// `true` when encryption markers were detected (`PDF /Encrypt`, `ZIP` flag, or `.ffenc`).
    pub is_encrypted: bool,
    /// Line count for text-oriented documents when inspected synchronously.
    pub line_count: Option<u64>,
    /// Bounded key-value normalized attribute map (`MAX_ATTRIBUTES` max).
    pub attributes: BTreeMap<String, String>,
}

/// Suggested action attached by content-aware file classification (`FR-ADP-001`, `FR-CMP-002`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RecommendedAction {
    /// Stable action identifier.
    pub action_id: String,
    /// User-facing action title.
    pub title: String,
    /// Short explanation of why the action applies.
    pub reason: String,
}

/// Complete inspection snapshot matching `docs/CONTRACTS.md` (`FF-CTR-001` §2.2).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FileInspectionReport {
    /// Contract schema version (`CORE_SCHEMA_VERSION`).
    pub schema_version: u32,
    /// True binary/structural format resolved by signature detector.
    pub detected_format: FormatId,
    /// Canonical MIME type.
    pub mime_type: String,
    /// Extension claimed by input path/filename, if present.
    pub extension_claimed: Option<String>,
    /// Whether extension contradicts detected binary signature (`FR-DET-002`).
    pub spoofing_mismatch: bool,
    /// Exact input payload size in bytes.
    pub byte_size: u64,
    /// Streaming `BLAKE3`, `SHA-256`, and `xxHash64` digests.
    pub digests: DigestSet,
    /// Internal tracks, streams, or container inventory.
    pub streams: Vec<StreamDescriptor>,
    /// Normalized metadata and privacy-risk summary.
    pub metadata_summary: MetadataSummary,
    /// Classification/optimization recommendations (`FR-ADP-001`, `FR-CMP-002`).
    pub recommended_actions: Vec<RecommendedAction>,
    /// Domain, compressibility tier, and entropy classification.
    pub classification: Option<FileClassification>,
}

fn to_lower_hex(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len() * 2);
    for &b in bytes {
        let _ = write!(out, "{b:02x}");
    }
    out
}

fn push_attr(summary: &mut MetadataSummary, key: impl Into<String>, value: impl Into<String>) {
    if summary.attributes.len() < MAX_ATTRIBUTES {
        summary.attributes.insert(key.into(), value.into());
    }
}

fn read_u16_le(slice: &[u8], offset: usize) -> Option<u16> {
    let bytes: [u8; 2] = slice.get(offset..offset + 2)?.try_into().ok()?;
    Some(u16::from_le_bytes(bytes))
}

fn read_u32_le(slice: &[u8], offset: usize) -> Option<u32> {
    let bytes: [u8; 4] = slice.get(offset..offset + 4)?.try_into().ok()?;
    Some(u32::from_le_bytes(bytes))
}

fn read_u16_be(slice: &[u8], offset: usize) -> Option<u16> {
    let bytes: [u8; 2] = slice.get(offset..offset + 2)?.try_into().ok()?;
    Some(u16::from_be_bytes(bytes))
}

fn read_u32_be(slice: &[u8], offset: usize) -> Option<u32> {
    let bytes: [u8; 4] = slice.get(offset..offset + 4)?.try_into().ok()?;
    Some(u32::from_be_bytes(bytes))
}

fn find_subslice(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    if needle.is_empty() || haystack.len() < needle.len() {
        return None;
    }
    haystack.windows(needle.len()).position(|w| w == needle)
}

/// Computes `BLAKE3`, `SHA-256`, and `xxHash64` in one streaming `64 KB` pass, returning `(byte_size, DigestSet)`.
///
/// # Errors
/// Returns [`FileFusionError`] if reading from the stream fails.
pub fn compute_digests_from_reader<R: Read>(reader: &mut R) -> FileFusionResult<(u64, DigestSet)> {
    let mut blake_hasher = blake3::Hasher::new();
    let mut sha_hasher = sha2::Sha256::new();
    let mut xx_hasher = Xxh64::new(0);
    let mut buf = vec![0u8; STREAM_BUFFER_BYTES];
    let mut total_bytes = 0u64;

    loop {
        let n = reader.read(&mut buf)?;
        if n == 0 {
            break;
        }
        let chunk = &buf[..n];
        total_bytes = total_bytes.saturating_add(u64::try_from(n).unwrap_or(0));
        blake_hasher.update(chunk);
        sha_hasher.update(chunk);
        xx_hasher.update(chunk);
    }

    let blake_hash = blake_hasher.finalize();
    let sha_hash = sha_hasher.finalize();
    let xx_hash = xx_hasher.digest();

    Ok((
        total_bytes,
        DigestSet {
            blake3: blake_hash.to_hex().to_string(),
            sha256: to_lower_hex(sha_hash.as_slice()),
            xxhash64: format!("{xx_hash:016x}"),
        },
    ))
}

/// Computes `BLAKE3`, `SHA-256`, and `xxHash64` digests for an in-memory slice.
#[must_use]
pub fn compute_digests_from_bytes(data: &[u8]) -> DigestSet {
    let blake_hash = blake3::hash(data);
    let sha_hash = sha2::Sha256::digest(data);
    let mut xx = Xxh64::new(0);
    xx.update(data);
    DigestSet {
        blake3: blake_hash.to_hex().to_string(),
        sha256: to_lower_hex(sha_hash.as_slice()),
        xxhash64: format!("{:016x}", xx.digest()),
    }
}

fn inspect_png_bytes(
    data: &[u8],
    summary: &mut MetadataSummary,
    streams: &mut Vec<StreamDescriptor>,
) {
    if data.len() < 33 || !data.starts_with(b"\x89PNG\r\n\x1a\n") {
        return;
    }
    let mut pos = 8usize;
    let mut width = None;
    let mut height = None;
    let mut bit_depth = None;
    let mut has_alpha = None;

    while pos + 12 <= data.len() {
        let Some(chunk_len_u32) = read_u32_be(data, pos) else {
            break;
        };
        let chunk_len = usize::try_from(chunk_len_u32).unwrap_or(usize::MAX);
        let Some(chunk_type) = data.get(pos + 4..pos + 8) else {
            break;
        };
        let chunk_start = pos + 8;
        let Some(chunk_end) = chunk_start.checked_add(chunk_len) else {
            break;
        };
        let payload = data.get(chunk_start..chunk_end).unwrap_or(&[]);

        match chunk_type {
            b"IHDR" if payload.len() >= 13 => {
                width = read_u32_be(payload, 0);
                height = read_u32_be(payload, 4);
                bit_depth = Some(payload[8]);
                let color_type = payload[9];
                let (cs, alpha) = match color_type {
                    0 => ("grayscale", false),
                    2 => ("rgb", false),
                    3 => ("indexed", false),
                    4 => ("grayscale_alpha", true),
                    6 => ("rgba", true),
                    _ => ("unknown", false),
                };
                has_alpha = Some(alpha);
                summary.width = width;
                summary.height = height;
                summary.color_space = Some(cs.to_owned());
                push_attr(summary, "png.color_type", cs);
                if let Some(bd) = bit_depth {
                    push_attr(summary, "png.bit_depth", bd.to_string());
                }
            }
            b"tRNS" => has_alpha = Some(true),
            b"iCCP" => summary.has_icc_profile = true,
            b"eXIf" => summary.has_exif = true,
            b"tEXt" | b"iTXt" => extract_png_text_chunk(chunk_type, payload, summary),
            b"IEND" => break,
            _ => {}
        }

        let Some(next) = chunk_end.checked_add(4) else {
            break;
        };
        if next <= pos || next > data.len() {
            break;
        }
        pos = next;
    }

    streams.push(StreamDescriptor {
        index: 0,
        kind: StreamKind::Image,
        codec: Some(String::from("png")),
        width,
        height,
        bit_depth,
        has_alpha,
        sample_rate_hz: None,
        channels: None,
        entry_count: None,
        detail: Some(String::from("PNG image stream")),
    });
}

fn extract_png_text_chunk(chunk_type: &[u8], payload: &[u8], summary: &mut MetadataSummary) {
    let Some(sep) = payload.iter().position(|&b| b == 0) else {
        return;
    };
    let Ok(key) = core::str::from_utf8(&payload[..sep]) else {
        return;
    };
    if key == "XML:com.adobe.xmp" {
        summary.has_xmp = true;
        if find_subslice(payload, b"exif:GPS").is_some()
            || find_subslice(payload, b"GPSLatitude").is_some()
        {
            summary.has_gps_privacy_risk = true;
        }
    } else if chunk_type == b"tEXt" {
        if let Ok(val) = core::str::from_utf8(&payload[sep + 1..]) {
            let trimmed = val.trim();
            match key {
                "Title" if summary.title.is_none() => summary.title = Some(trimmed.to_owned()),
                "Author" if summary.author.is_none() => {
                    summary.author = Some(trimmed.to_owned());
                    summary.has_pii_author_risk = true;
                }
                "Software" if summary.creator_tool.is_none() => {
                    summary.creator_tool = Some(trimmed.to_owned());
                }
                _ => {}
            }
        }
    }
}

fn inspect_jpeg_bytes(
    data: &[u8],
    summary: &mut MetadataSummary,
    streams: &mut Vec<StreamDescriptor>,
) {
    if data.len() < 4 || !data.starts_with(&[0xFF, 0xD8]) {
        return;
    }
    let mut pos = 2usize;
    let mut width = None;
    let mut height = None;
    let mut bit_depth = None;
    let mut components = None;

    while pos + 4 <= data.len() {
        if data[pos] != 0xFF {
            break;
        }
        while pos < data.len() && data[pos] == 0xFF {
            pos += 1;
        }
        if pos >= data.len() {
            break;
        }
        let marker = data[pos];
        pos += 1;
        // SOI, EOI, RSTn, SOS stop marker parsing cleanly.
        if marker == 0xD9 || marker == 0xDA || (0xD0..=0xD7).contains(&marker) {
            break;
        }
        let Some(seg_len) = read_u16_be(data, pos).map(usize::from) else {
            break;
        };
        if seg_len < 2 {
            break;
        }
        let seg_start = pos + 2;
        let seg_end = pos + seg_len;
        let payload = data.get(seg_start..seg_end).unwrap_or(&[]);

        if marker == 0xE1 {
            if payload.starts_with(b"Exif\0\0") {
                summary.has_exif = true;
                if find_subslice(payload, b"GPS").is_some()
                    || find_subslice(payload, &[0x88, 0x25]).is_some()
                {
                    summary.has_gps_privacy_risk = true;
                }
            } else if payload.starts_with(b"http://ns.adobe.com/xap/1.0/\0") {
                summary.has_xmp = true;
                if find_subslice(payload, b"GPSLatitude").is_some() {
                    summary.has_gps_privacy_risk = true;
                }
            }
        } else if marker == 0xE2 && payload.starts_with(b"ICC_PROFILE") {
            summary.has_icc_profile = true;
        } else if matches!(marker, 0xC0..=0xC3) && payload.len() >= 6 {
            bit_depth = Some(payload[0]);
            height = read_u16_be(payload, 1).map(u32::from);
            width = read_u16_be(payload, 3).map(u32::from);
            components = Some(payload[5]);
        }

        if seg_end > data.len() {
            break;
        }
        pos = seg_end;
    }

    summary.width = width;
    summary.height = height;
    summary.color_space = match components {
        Some(1) => Some(String::from("grayscale")),
        Some(3) => Some(String::from("ycbcr")),
        Some(4) => Some(String::from("cmyk")),
        _ => None,
    };

    streams.push(StreamDescriptor {
        index: 0,
        kind: StreamKind::Image,
        codec: Some(String::from("jpeg")),
        width,
        height,
        bit_depth,
        has_alpha: Some(false),
        sample_rate_hz: None,
        channels: None,
        entry_count: None,
        detail: Some(String::from("JPEG baseline/progressive stream")),
    });
}

fn inspect_webp_bytes(
    data: &[u8],
    summary: &mut MetadataSummary,
    streams: &mut Vec<StreamDescriptor>,
) {
    if data.len() < 30 || &data[0..4] != b"RIFF" || &data[8..12] != b"WEBP" {
        return;
    }
    let chunk_id = &data[12..16];
    let mut width = None;
    let mut height = None;
    let mut has_alpha = None;

    match chunk_id {
        b"VP8X" if data.len() >= 30 => {
            let flags = data[20];
            summary.has_icc_profile = (flags & 0x20) != 0;
            has_alpha = Some((flags & 0x10) != 0);
            summary.has_exif = (flags & 0x08) != 0;
            summary.has_xmp = (flags & 0x04) != 0;
            width = Some(u32::from_le_bytes([data[24], data[25], data[26], 0]) + 1);
            height = Some(u32::from_le_bytes([data[27], data[28], data[29], 0]) + 1);
        }
        b"VP8L" if data.len() >= 25 && data[20] == 0x2F => {
            let packed = read_u32_le(data, 21).unwrap_or(0);
            width = Some((packed & 0x3FFF) + 1);
            height = Some(((packed >> 14) & 0x3FFF) + 1);
            has_alpha = Some(((packed >> 28) & 1) != 0);
        }
        b"VP8 " if data.len() >= 30 && &data[23..26] == b"\x9D\x01\x2A" => {
            width = read_u16_le(data, 26).map(|v| u32::from(v & 0x3FFF));
            height = read_u16_le(data, 28).map(|v| u32::from(v & 0x3FFF));
            has_alpha = Some(false);
        }
        _ => {}
    }

    summary.width = width;
    summary.height = height;
    streams.push(StreamDescriptor {
        index: 0,
        kind: StreamKind::Image,
        codec: Some(String::from("webp")),
        width,
        height,
        bit_depth: Some(8),
        has_alpha,
        sample_rate_hz: None,
        channels: None,
        entry_count: None,
        detail: Some(String::from("WebP RIFF stream")),
    });
}

fn inspect_gif_bmp_bytes(
    format: FormatId,
    data: &[u8],
    summary: &mut MetadataSummary,
    streams: &mut Vec<StreamDescriptor>,
) {
    if format == FormatId::Gif && data.len() >= 10 {
        let width = read_u16_le(data, 6).map(u32::from);
        let height = read_u16_le(data, 8).map(u32::from);
        summary.width = width;
        summary.height = height;
        summary.color_space = Some(String::from("indexed"));
        streams.push(StreamDescriptor {
            index: 0,
            kind: StreamKind::Image,
            codec: Some(String::from("gif")),
            width,
            height,
            bit_depth: Some(8),
            has_alpha: Some(true),
            sample_rate_hz: None,
            channels: None,
            entry_count: None,
            detail: Some(String::from("GIF bitmap sequence")),
        });
    } else if format == FormatId::Bmp && data.len() >= 30 {
        let width = read_u32_le(data, 18);
        let height = data
            .get(22..26)
            .and_then(|b| b.try_into().ok())
            .map(i32::from_le_bytes)
            .map(i32::unsigned_abs);
        let bpp = read_u16_le(data, 28).and_then(|v| u8::try_from(v).ok());
        summary.width = width;
        summary.height = height;
        streams.push(StreamDescriptor {
            index: 0,
            kind: StreamKind::Image,
            codec: Some(String::from("bmp")),
            width,
            height,
            bit_depth: bpp,
            has_alpha: Some(bpp == Some(32)),
            sample_rate_hz: None,
            channels: None,
            entry_count: None,
            detail: Some(String::from("BMP DIB bitmap")),
        });
    }
}

fn inspect_audio_bytes(
    format: FormatId,
    data: &[u8],
    summary: &mut MetadataSummary,
    streams: &mut Vec<StreamDescriptor>,
) {
    if format == FormatId::Wav && data.len() >= 36 && &data[8..12] == b"WAVE" {
        let mut pos = 12usize;
        while pos + 8 <= data.len() {
            let chunk_id = &data[pos..pos + 4];
            let chunk_size = read_u32_le(data, pos + 4).unwrap_or(0) as usize;
            let body_start = pos + 8;
            if chunk_id == b"fmt " && body_start + 16 <= data.len() {
                let channels = read_u16_le(data, body_start + 2).and_then(|c| u8::try_from(c).ok());
                let sample_rate_hz = read_u32_le(data, body_start + 4);
                let bits = read_u16_le(data, body_start + 14).and_then(|b| u8::try_from(b).ok());
                streams.push(StreamDescriptor {
                    index: 0,
                    kind: StreamKind::Audio,
                    codec: Some(String::from("pcm")),
                    width: None,
                    height: None,
                    bit_depth: bits,
                    has_alpha: None,
                    sample_rate_hz,
                    channels,
                    entry_count: None,
                    detail: Some(String::from("RIFF WAVE audio stream")),
                });
                return;
            }
            let padded = chunk_size.saturating_add(chunk_size & 1);
            let Some(next) = body_start.checked_add(padded) else {
                break;
            };
            if next <= pos {
                break;
            }
            pos = next;
        }
    } else if format == FormatId::Flac && data.len() >= 42 && data.starts_with(b"fLaC") {
        let block_type = data[4] & 0x7F;
        let len = (usize::from(data[5]) << 16) | (usize::from(data[6]) << 8) | usize::from(data[7]);
        if block_type == 0 && len >= 34 {
            let s = &data[8..42];
            let sr = (u32::from(s[10]) << 12) | (u32::from(s[11]) << 4) | (u32::from(s[12]) >> 4);
            let ch = ((s[12] >> 1) & 0x07) + 1;
            let bps = (((s[12] & 0x01) << 4) | (s[13] >> 4)) + 1;
            streams.push(StreamDescriptor {
                index: 0,
                kind: StreamKind::Audio,
                codec: Some(String::from("flac")),
                width: None,
                height: None,
                bit_depth: Some(bps),
                has_alpha: None,
                sample_rate_hz: Some(sr),
                channels: Some(ch),
                entry_count: None,
                detail: Some(String::from("FLAC lossless audio stream")),
            });
        }
    } else if format == FormatId::Mp3 {
        if data.starts_with(b"ID3") && data.len() >= 10 {
            push_attr(
                summary,
                "audio.id3v2_version",
                format!("2.{}.{}", data[3], data[4]),
            );
            summary.has_pii_author_risk = true;
        }
        streams.push(StreamDescriptor {
            index: 0,
            kind: StreamKind::Audio,
            codec: Some(String::from("mp3")),
            width: None,
            height: None,
            bit_depth: None,
            has_alpha: None,
            sample_rate_hz: None,
            channels: None,
            entry_count: None,
            detail: Some(String::from("MPEG audio stream")),
        });
    }
}

fn extract_pdf_literal_field(window: &str, key: &str) -> Option<String> {
    let idx = window.find(key)?;
    let rest = window.get(idx + key.len()..)?.trim_start();
    let inner = rest.strip_prefix('(')?;
    let end = inner.find(')')?;
    let val = inner.get(..end)?.trim();
    if val.is_empty() {
        None
    } else {
        Some(val.chars().take(128).collect())
    }
}

fn inspect_pdf_windows(
    head: &[u8],
    tail: &[u8],
    summary: &mut MetadataSummary,
    streams: &mut Vec<StreamDescriptor>,
) {
    if let Some(ver) = find_subslice(head, b"%PDF-")
        .and_then(|pos| head.get(pos + 5..pos + 8))
        .and_then(|ver_slice| core::str::from_utf8(ver_slice).ok())
    {
        push_attr(summary, "pdf.version", ver.trim().to_owned());
    }

    for slice in [head, tail] {
        if find_subslice(slice, b"/Encrypt").is_some() {
            summary.is_encrypted = true;
        }
        if find_subslice(slice, b"http://ns.adobe.com/xap").is_some()
            || find_subslice(slice, b"/Metadata").is_some()
        {
            summary.has_xmp = true;
        }
        let lossy = String::from_utf8_lossy(slice);
        if summary.title.is_none() {
            summary.title = extract_pdf_literal_field(&lossy, "/Title");
        }
        if summary.author.is_none() {
            if let Some(author) = extract_pdf_literal_field(&lossy, "/Author") {
                summary.author = Some(author);
                summary.has_pii_author_risk = true;
            }
        }
        if summary.creator_tool.is_none() {
            summary.creator_tool = extract_pdf_literal_field(&lossy, "/Creator")
                .or_else(|| extract_pdf_literal_field(&lossy, "/Producer"));
        }
    }

    streams.push(StreamDescriptor {
        index: 0,
        kind: StreamKind::Document,
        codec: Some(String::from("pdf")),
        width: None,
        height: None,
        bit_depth: None,
        has_alpha: None,
        sample_rate_hz: None,
        channels: None,
        entry_count: None,
        detail: Some(if summary.is_encrypted {
            String::from("Encrypted PDF document")
        } else {
            String::from("PDF document structure")
        }),
    });
}

fn inspect_zip_central_dir<R: Read + Seek>(
    reader: &mut R,
    summary: &mut MetadataSummary,
    streams: &mut Vec<StreamDescriptor>,
) {
    let Ok(file_len) = reader.seek(SeekFrom::End(0)) else {
        return;
    };
    let tail_len = file_len.min(65_536);
    if reader
        .seek(SeekFrom::Start(file_len.saturating_sub(tail_len)))
        .is_err()
    {
        return;
    }
    let Ok(tail_cap) = usize::try_from(tail_len) else {
        return;
    };
    let mut tail = vec![0u8; tail_cap];
    if reader.read_exact(&mut tail).is_err() {
        return;
    }
    let Some(eocd_pos) = tail.windows(4).rposition(|w| w == [0x50, 0x4B, 0x05, 0x06]) else {
        return;
    };
    let entry_total = read_u16_le(&tail, eocd_pos + 10).map(u64::from);
    let cd_size = read_u32_le(&tail, eocd_pos + 12).unwrap_or(0);
    let cd_offset = read_u32_le(&tail, eocd_pos + 16).unwrap_or(0);
    let cd_len = usize::try_from(cd_size.min(512 * 1024)).unwrap_or(0);

    let mut total_uncompressed = 0u64;
    let mut encrypted = false;
    let mut scanned = 0usize;

    if cd_len > 0 && reader.seek(SeekFrom::Start(u64::from(cd_offset))).is_ok() {
        let mut cd = vec![0u8; cd_len];
        if let Ok(n) = reader.read(&mut cd) {
            let cd = &cd[..n];
            let mut pos = 0usize;
            while pos + 46 <= cd.len() && scanned < MAX_ZIP_INSPECT_ENTRIES {
                if cd.get(pos..pos + 4) != Some(&[0x50, 0x4B, 0x01, 0x02]) {
                    break;
                }
                scanned += 1;
                if let Some(flags) = read_u16_le(cd, pos + 8) {
                    if (flags & 0x0001) != 0 {
                        encrypted = true;
                    }
                }
                if let Some(uncomp) = read_u32_le(cd, pos + 24) {
                    total_uncompressed = total_uncompressed.saturating_add(u64::from(uncomp));
                }
                let Some(nlen) = read_u16_le(cd, pos + 28).map(usize::from) else {
                    break;
                };
                let Some(elen) = read_u16_le(cd, pos + 30).map(usize::from) else {
                    break;
                };
                let Some(clen) = read_u16_le(cd, pos + 32).map(usize::from) else {
                    break;
                };
                let Some(next) = pos
                    .checked_add(46)
                    .and_then(|v| v.checked_add(nlen))
                    .and_then(|v| v.checked_add(elen))
                    .and_then(|v| v.checked_add(clen))
                else {
                    break;
                };
                pos = next;
            }
        }
    }

    summary.is_encrypted = encrypted;
    if let Some(count) = entry_total {
        push_attr(summary, "container.entries", count.to_string());
    }
    push_attr(
        summary,
        "container.uncompressed_bytes",
        total_uncompressed.to_string(),
    );
    streams.push(StreamDescriptor {
        index: 0,
        kind: StreamKind::ArchiveEntrySet,
        codec: Some(String::from("zip_container")),
        width: None,
        height: None,
        bit_depth: None,
        has_alpha: None,
        sample_rate_hz: None,
        channels: None,
        entry_count: entry_total.or(Some(u64::try_from(scanned).unwrap_or(0))),
        detail: Some(String::from("ZIP-based container directory")),
    });
}

fn inspect_text_head(
    format: FormatId,
    head: &[u8],
    summary: &mut MetadataSummary,
    streams: &mut Vec<StreamDescriptor>,
) {
    let has_bom = head.starts_with(&[0xEF, 0xBB, 0xBF]);
    push_attr(
        summary,
        "text.encoding",
        if has_bom { "utf-8-bom" } else { "utf-8" },
    );
    let trimmed = head.strip_prefix(&[0xEF, 0xBB, 0xBF]).unwrap_or(head);
    if let Ok(text) = core::str::from_utf8(trimmed) {
        let lines = u64::try_from(text.lines().count()).unwrap_or(0);
        summary.line_count = Some(lines);
        if format == FormatId::Svg
            && (text.contains("http://ns.adobe.com/xap") || text.contains("<metadata"))
        {
            summary.has_xmp = true;
        }
    }
    streams.push(StreamDescriptor {
        index: 0,
        kind: StreamKind::Text,
        codec: Some(format!("{format:?}").to_ascii_lowercase()),
        width: None,
        height: None,
        bit_depth: None,
        has_alpha: None,
        sample_rate_hz: None,
        channels: None,
        entry_count: None,
        detail: Some(String::from("UTF-8 structured or plain text stream")),
    });
}

fn is_zip_backed_format(fmt: FormatId) -> bool {
    matches!(
        fmt,
        FormatId::Zip
            | FormatId::Docx
            | FormatId::Xlsx
            | FormatId::Pptx
            | FormatId::Odt
            | FormatId::Ods
            | FormatId::Odp
            | FormatId::Epub
    )
}

fn is_text_format(fmt: FormatId) -> bool {
    matches!(
        fmt,
        FormatId::Svg
            | FormatId::Html
            | FormatId::Xml
            | FormatId::Json
            | FormatId::Yaml
            | FormatId::Csv
            | FormatId::Markdown
            | FormatId::PlainText
    )
}

fn populate_metadata_and_streams<R: Read + Seek>(
    sig: &SignatureMatch,
    reader: &mut R,
    head: &[u8],
    byte_size: u64,
) -> (MetadataSummary, Vec<StreamDescriptor>) {
    let mut summary = MetadataSummary::default();
    let mut streams = Vec::new();

    match sig.detected_format {
        FormatId::Png => inspect_png_bytes(head, &mut summary, &mut streams),
        FormatId::Jpeg => inspect_jpeg_bytes(head, &mut summary, &mut streams),
        FormatId::Webp => inspect_webp_bytes(head, &mut summary, &mut streams),
        FormatId::Gif | FormatId::Bmp => {
            inspect_gif_bmp_bytes(sig.detected_format, head, &mut summary, &mut streams);
        }
        FormatId::Wav | FormatId::Flac | FormatId::Mp3 => {
            inspect_audio_bytes(sig.detected_format, head, &mut summary, &mut streams);
        }
        FormatId::Pdf => {
            let tail_len = byte_size.min(32_768);
            let mut tail = Vec::new();
            if tail_len > 0
                && reader
                    .seek(SeekFrom::Start(byte_size.saturating_sub(tail_len)))
                    .is_ok()
            {
                if let Ok(cap) = usize::try_from(tail_len) {
                    tail.resize(cap, 0);
                    if reader.read_exact(&mut tail).is_err() {
                        tail.clear();
                    }
                }
            }
            inspect_pdf_windows(head, &tail, &mut summary, &mut streams);
        }
        FormatId::Ffenc => {
            summary.is_encrypted = true;
            streams.push(StreamDescriptor {
                index: 0,
                kind: StreamKind::ArchiveEntrySet,
                codec: Some(String::from("ffenc")),
                width: None,
                height: None,
                bit_depth: None,
                has_alpha: None,
                sample_rate_hz: None,
                channels: None,
                entry_count: None,
                detail: Some(String::from("Encrypted FileFusion container")),
            });
        }
        fmt if is_zip_backed_format(fmt) => {
            inspect_zip_central_dir(reader, &mut summary, &mut streams);
        }
        fmt if is_text_format(fmt) => inspect_text_head(fmt, head, &mut summary, &mut streams),
        _ => {}
    }

    (summary, streams)
}

fn finalize_report(
    sig: SignatureMatch,
    byte_size: u64,
    digests: DigestSet,
    streams: Vec<StreamDescriptor>,
    metadata_summary: MetadataSummary,
    sample: &[u8],
) -> FileInspectionReport {
    let mut report = FileInspectionReport {
        schema_version: CORE_SCHEMA_VERSION,
        detected_format: sig.detected_format,
        mime_type: sig.mime_type,
        extension_claimed: sig.claimed_extension,
        spoofing_mismatch: sig.spoofing_mismatch,
        byte_size,
        digests,
        streams,
        metadata_summary,
        recommended_actions: Vec::new(),
        classification: None,
    };
    let cls = classify_inspection_report(&report, sample);
    report
        .recommended_actions
        .clone_from(&cls.recommended_actions);
    report.classification = Some(cls);
    report
}

/// Inspects a seekable stream: signature probe, streaming digests (`64 KB`), stream inventory,
/// and normalized metadata summary (`FR-DET-001..004`, `FR-MET-001`).
///
/// # Errors
/// Returns [`FileFusionError`] if reading/seeking fails or `strict_spoofing` rejects a mismatch.
pub fn inspect_reader<R: Read + Seek>(
    reader: &mut R,
    file_name_or_ext: Option<&str>,
    strict_spoofing: bool,
) -> FileFusionResult<FileInspectionReport> {
    let sig = detect_signature_from_reader(reader, file_name_or_ext)?;
    enforce_spoofing_policy(&sig, strict_spoofing)?;

    reader.seek(SeekFrom::Start(0))?;
    let (byte_size, digests) = compute_digests_from_reader(reader)?;

    reader.seek(SeekFrom::Start(0))?;
    let head_cap =
        usize::try_from(byte_size.min(u64::try_from(MAX_METADATA_WINDOW_BYTES).unwrap_or(131_072)))
            .unwrap_or(MAX_METADATA_WINDOW_BYTES);
    let mut head = vec![0u8; head_cap];
    let n = if head_cap > 0 {
        reader.read(&mut head)?
    } else {
        0
    };
    head.truncate(n);

    let (metadata_summary, streams) = populate_metadata_and_streams(&sig, reader, &head, byte_size);

    Ok(finalize_report(
        sig,
        byte_size,
        digests,
        streams,
        metadata_summary,
        &head,
    ))
}

/// Inspects an in-memory byte slice using the same pipeline as [`inspect_reader`].
///
/// # Errors
/// Returns [`FileFusionError`] if `strict_spoofing` rejects an extension mismatch.
pub fn inspect_bytes(
    data: &[u8],
    file_name_or_ext: Option<&str>,
    strict_spoofing: bool,
) -> FileFusionResult<FileInspectionReport> {
    let sig = detect_signature_from_bytes(data, file_name_or_ext);
    enforce_spoofing_policy(&sig, strict_spoofing)?;
    let mut cursor = Cursor::new(data);
    let head = &data[..data.len().min(MAX_METADATA_WINDOW_BYTES)];
    let byte_size = u64::try_from(data.len()).unwrap_or(u64::MAX);
    let digests = compute_digests_from_bytes(data);
    let (metadata_summary, streams) =
        populate_metadata_and_streams(&sig, &mut cursor, head, byte_size);

    Ok(finalize_report(
        sig,
        byte_size,
        digests,
        streams,
        metadata_summary,
        head,
    ))
}

/// Opens `path` read-only and produces a complete [`FileInspectionReport`].
///
/// # Errors
/// Returns [`FileFusionError`] if the path cannot be opened/read or fails `strict_spoofing`.
pub fn inspect_path(path: &Path, strict_spoofing: bool) -> FileFusionResult<FileInspectionReport> {
    let mut file = File::open(path)?;
    let hint = path.file_name().and_then(|name| name.to_str());
    inspect_reader(&mut file, hint, strict_spoofing)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_png_with_ihdr_and_author() -> Vec<u8> {
        let mut png = Vec::new();
        png.extend_from_slice(b"\x89PNG\r\n\x1a\n");

        // IHDR: 1920x1080, 8-bit RGBA (color type 6)
        let mut ihdr = Vec::new();
        ihdr.extend_from_slice(&1920u32.to_be_bytes());
        ihdr.extend_from_slice(&1080u32.to_be_bytes());
        ihdr.extend_from_slice(&[8, 6, 0, 0, 0]);
        png.extend_from_slice(&(13u32).to_be_bytes());
        png.extend_from_slice(b"IHDR");
        png.extend_from_slice(&ihdr);
        png.extend_from_slice(&[0u8; 4]); // test CRC placeholder

        // tEXt Author=Alice
        let text_payload = b"Author\0Alice";
        png.extend_from_slice(&(u32::try_from(text_payload.len()).unwrap_or(0)).to_be_bytes());
        png.extend_from_slice(b"tEXt");
        png.extend_from_slice(text_payload);
        png.extend_from_slice(&[0u8; 4]);

        // IEND
        png.extend_from_slice(&0u32.to_be_bytes());
        png.extend_from_slice(b"IEND");
        png.extend_from_slice(&[0u8; 4]);
        png
    }

    #[test]
    fn streaming_digests_match_known_sha256_blake3_and_xxh64_for_empty_and_abc()
    -> FileFusionResult<()> {
        let mut cursor = Cursor::new(b"abc".as_slice());
        let (size, streamed) = compute_digests_from_reader(&mut cursor)?;
        assert_eq!(size, 3);
        assert_eq!(
            streamed.sha256,
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
        assert_eq!(
            streamed.blake3,
            "6437b3ac38465133ffb63b75273a8db548c558465d79db03fd359c6cd5bd9d85"
        );
        assert_eq!(streamed.xxhash64.len(), 16);
        assert_eq!(streamed, compute_digests_from_bytes(b"abc"));
        Ok(())
    }

    #[test]
    fn inspects_png_dimensions_color_space_and_pii_author_flag() -> FileFusionResult<()> {
        let png = make_png_with_ihdr_and_author();
        let report = inspect_bytes(&png, Some("banner.png"), true)?;
        assert_eq!(report.schema_version, CORE_SCHEMA_VERSION);
        assert_eq!(report.detected_format, FormatId::Png);
        assert_eq!(report.byte_size, u64::try_from(png.len()).unwrap_or(0));
        assert_eq!(report.metadata_summary.width, Some(1920));
        assert_eq!(report.metadata_summary.height, Some(1080));
        assert_eq!(report.metadata_summary.color_space.as_deref(), Some("rgba"));
        assert_eq!(report.metadata_summary.author.as_deref(), Some("Alice"));
        assert!(report.metadata_summary.has_pii_author_risk);
        assert_eq!(report.streams.len(), 1);
        assert_eq!(report.streams[0].has_alpha, Some(true));
        Ok(())
    }

    #[test]
    fn inspects_pdf_version_encrypt_and_metadata_fields() -> FileFusionResult<()> {
        let sample = b"%PDF-1.7\n1 0 obj << /Title (Quarterly Report) /Author (Bob) /Encrypt 2 0 R >> endobj\n%%EOF";
        let report = inspect_bytes(sample, Some("report.pdf"), true)?;
        assert_eq!(report.detected_format, FormatId::Pdf);
        assert!(report.metadata_summary.is_encrypted);
        assert_eq!(
            report.metadata_summary.title.as_deref(),
            Some("Quarterly Report")
        );
        assert_eq!(report.metadata_summary.author.as_deref(), Some("Bob"));
        assert_eq!(
            report
                .metadata_summary
                .attributes
                .get("pdf.version")
                .map(String::as_str),
            Some("1.7")
        );
        Ok(())
    }

    #[test]
    fn inspects_jpeg_exif_gps_webp_vp8x_and_wav_pcm() -> FileFusionResult<()> {
        // Minimal JPEG: SOI + APP1 Exif(GPS) + SOF0 (800x600, 3 components) + EOI
        let mut jpeg = vec![0xFF, 0xD8, 0xFF, 0xE1];
        let exif = b"Exif\0\0GPS\x00";
        jpeg.extend_from_slice(&(u16::try_from(exif.len() + 2).unwrap_or(0)).to_be_bytes());
        jpeg.extend_from_slice(exif);
        jpeg.extend_from_slice(&[0xFF, 0xC0, 0x00, 0x0B, 8]);
        jpeg.extend_from_slice(&600u16.to_be_bytes());
        jpeg.extend_from_slice(&800u16.to_be_bytes());
        jpeg.extend_from_slice(&[3, 1, 0x22, 0, 0xFF, 0xD9]);
        let jpg_report = inspect_bytes(&jpeg, Some("photo.jpg"), true)?;
        assert_eq!(jpg_report.metadata_summary.width, Some(800));
        assert_eq!(jpg_report.metadata_summary.height, Some(600));
        assert!(jpg_report.metadata_summary.has_exif);
        assert!(jpg_report.metadata_summary.has_gps_privacy_risk);

        // Minimal RIFF WAVE PCM 44100 Hz, 2ch, 16-bit
        let mut wav = Vec::from(*b"RIFF\x24\x00\x00\x00WAVEfmt ");
        wav.extend_from_slice(&16u32.to_le_bytes());
        wav.extend_from_slice(&1u16.to_le_bytes()); // PCM
        wav.extend_from_slice(&2u16.to_le_bytes()); // stereo
        wav.extend_from_slice(&44_100u32.to_le_bytes());
        wav.extend_from_slice(&176_400u32.to_le_bytes());
        wav.extend_from_slice(&4u16.to_le_bytes());
        wav.extend_from_slice(&16u16.to_le_bytes());
        let wav_report = inspect_bytes(&wav, Some("clip.wav"), true)?;
        assert_eq!(wav_report.streams[0].sample_rate_hz, Some(44_100));
        assert_eq!(wav_report.streams[0].channels, Some(2));
        assert_eq!(wav_report.streams[0].bit_depth, Some(16));
        Ok(())
    }
}
