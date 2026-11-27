//! Multi-offset magic-byte, structural container probing, and extension spoofing
//! verification (`FR-DET-001`, `FR-DET-002`, `SECURITY.md`).
//!
//! Designed for zero-`unsafe`, bounded-memory inspection across both native and
//! `wasm32` targets.

use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::error::{ErrorCode, FileFusionError, FileFusionResult};
use crate::{FormatId, MediaKind};

const MAX_HEADER_PROBE_BYTES: usize = 65_536;
const MAX_ZIP_ENTRIES_SCANNED: usize = 256;
const ZIP_LOCAL_FILE_MAGIC: [u8; 4] = [0x50, 0x4B, 0x03, 0x04];
const ZIP_EMPTY_ARCHIVE_MAGIC: [u8; 4] = [0x50, 0x4B, 0x05, 0x06];
const ZIP_EOCD_SIG: [u8; 4] = [0x50, 0x4B, 0x05, 0x06];
const ZIP_CENTRAL_DIR_SIG: [u8; 4] = [0x50, 0x4B, 0x01, 0x02];

/// Confidence tier of a binary/structural signature match.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DetectionConfidence {
    /// Deterministic fixed-offset magic header match.
    ExactMagic,
    /// Structural container header or entry inspection match (`RIFF`, `ISO-BMFF`, `ZIP`, `EBML`).
    ContainerStructural,
    /// Text-encoding or markup structure heuristic.
    TextHeuristic,
    /// No signature recognized.
    Unknown,
}

/// Detailed result from magic-byte, container, and extension-mismatch probing.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SignatureMatch {
    /// True format resolved from binary/structural inspection.
    pub detected_format: FormatId,
    /// Resolved MIME media type.
    pub mime_type: String,
    /// High-level media bucket.
    pub media_kind: MediaKind,
    /// Canonical lowercase extensions without leading dots.
    pub canonical_extensions: Vec<String>,
    /// Confidence classification of the signature probe.
    pub confidence: DetectionConfidence,
    /// Machine identifier of the signature rule that matched.
    pub matched_rule: String,
    /// Lowercase extension extracted from the filename or extension hint, if present.
    pub claimed_extension: Option<String>,
    /// `true` when claimed extension contradicts the detected binary/container signature (`FR-DET-002`).
    pub spoofing_mismatch: bool,
    /// Actionable human explanation when `spoofing_mismatch` is `true`.
    pub spoofing_reason: Option<String>,
}

#[derive(Clone, Copy)]
struct ProbeCandidate {
    format: FormatId,
    confidence: DetectionConfidence,
    rule: &'static str,
}

impl ProbeCandidate {
    const fn exact(format: FormatId, rule: &'static str) -> Self {
        Self {
            format,
            confidence: DetectionConfidence::ExactMagic,
            rule,
        }
    }

    const fn container(format: FormatId, rule: &'static str) -> Self {
        Self {
            format,
            confidence: DetectionConfidence::ContainerStructural,
            rule,
        }
    }

    const fn heuristic(format: FormatId, rule: &'static str) -> Self {
        Self {
            format,
            confidence: DetectionConfidence::TextHeuristic,
            rule,
        }
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

fn find_subslice(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    if needle.is_empty() || haystack.len() < needle.len() {
        return None;
    }
    haystack
        .windows(needle.len())
        .position(|window| window == needle)
}

fn classify_zip_entry_name(name: &str) -> Option<(FormatId, &'static str)> {
    if name == "word/document.xml" {
        return Some((FormatId::Docx, "zip_entry:word/document.xml"));
    }
    if name == "xl/workbook.xml" {
        return Some((FormatId::Xlsx, "zip_entry:xl/workbook.xml"));
    }
    if name == "ppt/presentation.xml" {
        return Some((FormatId::Pptx, "zip_entry:ppt/presentation.xml"));
    }
    if name == "META-INF/container.xml" {
        return Some((FormatId::Epub, "zip_entry:META-INF/container.xml"));
    }
    None
}

fn classify_zip_mimetype_payload(payload: &[u8]) -> Option<(FormatId, &'static str)> {
    let text = core::str::from_utf8(payload).ok()?.trim();
    match text {
        "application/epub+zip" => Some((FormatId::Epub, "zip_mimetype:epub")),
        "application/vnd.oasis.opendocument.text" => Some((FormatId::Odt, "zip_mimetype:odt")),
        "application/vnd.oasis.opendocument.spreadsheet" => {
            Some((FormatId::Ods, "zip_mimetype:ods"))
        }
        "application/vnd.oasis.opendocument.presentation" => {
            Some((FormatId::Odp, "zip_mimetype:odp"))
        }
        _ => None,
    }
}

fn probe_zip_bytes(data: &[u8]) -> ProbeCandidate {
    if data.starts_with(&ZIP_EMPTY_ARCHIVE_MAGIC) {
        return ProbeCandidate::container(FormatId::Zip, "zip:empty_eocd");
    }
    let mut offset = 0usize;
    let mut scanned = 0usize;
    let mut epub_candidate = false;

    while offset + 30 <= data.len() && scanned < MAX_ZIP_ENTRIES_SCANNED {
        if data.get(offset..offset + 4) != Some(&ZIP_LOCAL_FILE_MAGIC) {
            break;
        }
        scanned += 1;
        let Some(method) = read_u16_le(data, offset + 8) else {
            break;
        };
        let Some(comp_size) = read_u32_le(data, offset + 18).map(|v| v as usize) else {
            break;
        };
        let Some(name_len) = read_u16_le(data, offset + 26).map(usize::from) else {
            break;
        };
        let Some(extra_len) = read_u16_le(data, offset + 28).map(usize::from) else {
            break;
        };
        let name_start = offset + 30;
        let Some(name_end) = name_start.checked_add(name_len) else {
            break;
        };
        let Some(name_bytes) = data.get(name_start..name_end) else {
            break;
        };
        if let Ok(name) = core::str::from_utf8(name_bytes) {
            if name == "mimetype" && method == 0 && comp_size <= 128 {
                let Some(payload_start) = name_end.checked_add(extra_len) else {
                    break;
                };
                if let Some(payload) = payload_start
                    .checked_add(comp_size)
                    .and_then(|payload_end| data.get(payload_start..payload_end))
                {
                    if let Some((fmt, rule)) = classify_zip_mimetype_payload(payload) {
                        return ProbeCandidate::container(fmt, rule);
                    }
                }
            }
            if let Some((fmt, rule)) = classify_zip_entry_name(name) {
                if fmt == FormatId::Epub {
                    epub_candidate = true;
                } else {
                    return ProbeCandidate::container(fmt, rule);
                }
            }
        }

        let Some(next) = name_end
            .checked_add(extra_len)
            .and_then(|v| v.checked_add(comp_size))
        else {
            break;
        };
        if next <= offset || next > data.len() {
            break;
        }
        offset = next;
    }

    if epub_candidate {
        return ProbeCandidate::container(FormatId::Epub, "zip_entry:META-INF/container.xml");
    }
    ProbeCandidate::container(FormatId::Zip, "magic:zip_pk")
}

fn probe_zip_central_directory<R: Read + Seek>(reader: &mut R) -> Option<ProbeCandidate> {
    let file_len = reader.seek(SeekFrom::End(0)).ok()?;
    let tail_len = file_len.min(65_536);
    let tail_start = file_len.saturating_sub(tail_len);
    reader.seek(SeekFrom::Start(tail_start)).ok()?;
    let tail_cap = usize::try_from(tail_len).ok()?;
    let mut tail = vec![0u8; tail_cap];
    reader.read_exact(&mut tail).ok()?;

    let eocd_rel = tail.windows(4).rposition(|w| w == ZIP_EOCD_SIG)?;
    let cd_size = read_u32_le(&tail, eocd_rel + 12)?;
    let cd_offset = read_u32_le(&tail, eocd_rel + 16)?;
    let cd_len = usize::try_from(cd_size.min(512 * 1024)).ok()?;

    reader.seek(SeekFrom::Start(u64::from(cd_offset))).ok()?;
    let mut cd = vec![0u8; cd_len];
    let n = reader.read(&mut cd).ok()?;
    let cd = &cd[..n];

    let mut pos = 0usize;
    let mut count = 0usize;
    while pos + 46 <= cd.len() && count < MAX_ZIP_ENTRIES_SCANNED {
        if cd.get(pos..pos + 4) != Some(&ZIP_CENTRAL_DIR_SIG) {
            break;
        }
        count += 1;
        let name_len = read_u16_le(cd, pos + 28).map(usize::from)?;
        let extra_len = read_u16_le(cd, pos + 30).map(usize::from)?;
        let comment_len = read_u16_le(cd, pos + 32).map(usize::from)?;
        let name_start = pos + 46;
        let name_end = name_start.checked_add(name_len)?;
        let name_bytes = cd.get(name_start..name_end)?;
        if let Ok(name) = core::str::from_utf8(name_bytes) {
            if let Some((fmt, rule)) = classify_zip_entry_name(name) {
                return Some(ProbeCandidate::container(fmt, rule));
            }
        }
        pos = name_end.checked_add(extra_len)?.checked_add(comment_len)?;
    }
    None
}

fn probe_riff(data: &[u8]) -> Option<ProbeCandidate> {
    if data.len() < 12 || &data[0..4] != b"RIFF" {
        return None;
    }
    match &data[8..12] {
        b"WEBP" => Some(ProbeCandidate::container(FormatId::Webp, "riff:WEBP")),
        b"WAVE" => Some(ProbeCandidate::container(FormatId::Wav, "riff:WAVE")),
        b"AVI " => Some(ProbeCandidate::container(FormatId::Avi, "riff:AVI")),
        _ => None,
    }
}

fn probe_iso_bmff(data: &[u8]) -> Option<ProbeCandidate> {
    if data.len() < 12 || &data[4..8] != b"ftyp" {
        return None;
    }
    let box_len = u32::from_be_bytes([data[0], data[1], data[2], data[3]]) as usize;
    let brand_end = box_len.min(data.len()).min(64);
    let major = &data[8..12];

    if major == b"avif" || major == b"avis" {
        return Some(ProbeCandidate::container(FormatId::Avif, "iso_bmff:avif"));
    }
    if matches!(major, b"heic" | b"heix" | b"mif1" | b"msf1" | b"hevc") {
        // `mif1` may carry `avif` in compatible brands list at offsets 16, 20, ...
        if data
            .get(16..brand_end)
            .is_some_and(|compat| compat.chunks_exact(4).any(|b| b == b"avif" || b == b"avis"))
        {
            return Some(ProbeCandidate::container(
                FormatId::Avif,
                "iso_bmff:compat_avif",
            ));
        }
        return Some(ProbeCandidate::container(FormatId::Heic, "iso_bmff:heic"));
    }
    if major == b"qt  " {
        return Some(ProbeCandidate::container(FormatId::Mov, "iso_bmff:qt"));
    }
    if matches!(major, b"M4A " | b"M4B ") {
        return Some(ProbeCandidate::container(FormatId::M4a, "iso_bmff:m4a"));
    }
    if matches!(
        major,
        b"isom" | b"iso2" | b"mp41" | b"mp42" | b"avc1" | b"dash"
    ) {
        return Some(ProbeCandidate::container(FormatId::Mp4, "iso_bmff:mp4"));
    }
    None
}

fn probe_images_and_docs(data: &[u8]) -> Option<ProbeCandidate> {
    if data.starts_with(b"FFENC\x01\x00") {
        return Some(ProbeCandidate::exact(FormatId::Ffenc, "magic:ffenc"));
    }
    // Allow optional UTF-8 BOM or pre-header offset for PDF (%PDF- within first 1024 bytes).
    let pdf_window = &data[..data.len().min(1024)];
    if find_subslice(pdf_window, b"%PDF-").is_some() {
        return Some(ProbeCandidate::exact(FormatId::Pdf, "magic:pdf"));
    }
    if data.starts_with(b"\x89PNG\r\n\x1a\n") {
        return Some(ProbeCandidate::exact(FormatId::Png, "magic:png"));
    }
    if data.starts_with(&[0xFF, 0xD8, 0xFF]) {
        return Some(ProbeCandidate::exact(FormatId::Jpeg, "magic:jpeg"));
    }
    if data.starts_with(b"GIF87a") || data.starts_with(b"GIF89a") {
        return Some(ProbeCandidate::exact(FormatId::Gif, "magic:gif"));
    }
    if data.len() >= 14 && data.starts_with(b"BM") {
        return Some(ProbeCandidate::exact(FormatId::Bmp, "magic:bmp"));
    }
    if data.len() >= 6
        && (data.starts_with(&[0x00, 0x00, 0x01, 0x00])
            || data.starts_with(&[0x00, 0x00, 0x02, 0x00]))
        && read_u16_le(data, 4).unwrap_or(0) > 0
    {
        return Some(ProbeCandidate::exact(FormatId::Ico, "magic:ico"));
    }
    if data.starts_with(b"II*\x00") || data.starts_with(b"MM\x00*") {
        return Some(ProbeCandidate::exact(FormatId::Tiff, "magic:tiff"));
    }
    if data.get(60..68) == Some(b"BOOKMOBI") {
        return Some(ProbeCandidate::exact(FormatId::Mobi, "offset60:BOOKMOBI"));
    }
    if data.starts_with(b"{\\rtf") {
        return Some(ProbeCandidate::exact(FormatId::Rtf, "magic:rtf"));
    }
    None
}

fn probe_audio_and_ebml(data: &[u8]) -> Option<ProbeCandidate> {
    if data.starts_with(b"fLaC") {
        return Some(ProbeCandidate::exact(FormatId::Flac, "magic:flac"));
    }
    if data.starts_with(b"OggS") {
        let head = &data[..data.len().min(256)];
        if find_subslice(head, b"OpusHead").is_some() {
            return Some(ProbeCandidate::container(FormatId::Opus, "ogg:OpusHead"));
        }
        return Some(ProbeCandidate::container(FormatId::Ogg, "magic:ogg"));
    }
    if data.starts_with(b"ID3") {
        return Some(ProbeCandidate::exact(FormatId::Mp3, "magic:id3"));
    }
    if data.len() >= 2 && data[0] == 0xFF {
        if data[1] == 0xF1 || data[1] == 0xF9 {
            return Some(ProbeCandidate::exact(FormatId::Aac, "magic:aac_adts"));
        }
        if matches!(data[1], 0xFB | 0xF3 | 0xF2 | 0xFA) {
            return Some(ProbeCandidate::exact(FormatId::Mp3, "magic:mp3_sync"));
        }
    }
    if data.starts_with(&[0x1A, 0x45, 0xDF, 0xA3]) {
        let head = &data[..data.len().min(4096)];
        if find_subslice(head, b"webm").is_some() {
            return Some(ProbeCandidate::container(FormatId::Webm, "ebml:webm"));
        }
        if find_subslice(head, b"matroska").is_some() {
            return Some(ProbeCandidate::container(FormatId::Mkv, "ebml:matroska"));
        }
    }
    None
}

fn probe_archives_and_exec(data: &[u8]) -> Option<ProbeCandidate> {
    if data.starts_with(&ZIP_LOCAL_FILE_MAGIC) || data.starts_with(&ZIP_EMPTY_ARCHIVE_MAGIC) {
        return Some(probe_zip_bytes(data));
    }
    if data.starts_with(&[0x37, 0x7A, 0xBC, 0xAF, 0x27, 0x1C]) {
        return Some(ProbeCandidate::exact(FormatId::SevenZ, "magic:7z"));
    }
    if data.starts_with(&[0x28, 0xB5, 0x2F, 0xFD]) {
        return Some(ProbeCandidate::exact(FormatId::Zstd, "magic:zstd"));
    }
    if data.starts_with(&[0x04, 0x22, 0x4D, 0x18]) {
        return Some(ProbeCandidate::exact(FormatId::Lz4, "magic:lz4"));
    }
    if data.starts_with(&[0xFD, 0x37, 0x7A, 0x58, 0x5A, 0x00]) {
        return Some(ProbeCandidate::exact(FormatId::Xz, "magic:xz"));
    }
    if data.starts_with(&[0x1F, 0x8B, 0x08]) {
        return Some(ProbeCandidate::exact(FormatId::Gzip, "magic:gzip"));
    }
    if data.starts_with(b"BZh") {
        return Some(ProbeCandidate::exact(FormatId::Bzip2, "magic:bzip2"));
    }
    if data.get(257..262) == Some(b"ustar") {
        return Some(ProbeCandidate::exact(FormatId::Tar, "offset257:ustar"));
    }
    if data.starts_with(b"\x7FELF")
        || (data.len() >= 64 && data.starts_with(b"MZ"))
        || data.starts_with(&[0xFE, 0xED, 0xFA, 0xCE])
        || data.starts_with(&[0xFE, 0xED, 0xFA, 0xCF])
        || data.starts_with(&[0xCE, 0xFA, 0xED, 0xFE])
        || data.starts_with(&[0xCF, 0xFA, 0xED, 0xFE])
        || (data.starts_with(b"#!") && data.get(2).is_some_and(u8::is_ascii_graphic))
    {
        return Some(ProbeCandidate::exact(
            FormatId::Executable,
            "magic:executable",
        ));
    }
    None
}

fn probe_text_formats(data: &[u8], claimed_ext: Option<&str>) -> Option<ProbeCandidate> {
    let trimmed_bytes = data.strip_prefix(&[0xEF, 0xBB, 0xBF]).unwrap_or(data);
    if trimmed_bytes.is_empty() || trimmed_bytes.contains(&0) {
        return None;
    }
    let text = core::str::from_utf8(trimmed_bytes).ok()?;
    let trimmed = text.trim_start();
    if trimmed.is_empty() {
        return Some(ProbeCandidate::heuristic(
            FormatId::PlainText,
            "text:whitespace",
        ));
    }
    let lower_prefix: String = trimmed
        .chars()
        .take(512)
        .collect::<String>()
        .to_ascii_lowercase();

    if lower_prefix.starts_with("<!doctype html") || lower_prefix.starts_with("<html") {
        return Some(ProbeCandidate::heuristic(FormatId::Html, "text:html_root"));
    }
    if lower_prefix.starts_with("<svg")
        || (lower_prefix.starts_with("<?xml") && lower_prefix.contains("<svg"))
    {
        return Some(ProbeCandidate::heuristic(FormatId::Svg, "text:svg_root"));
    }
    if lower_prefix.starts_with("<?xml") {
        return Some(ProbeCandidate::heuristic(FormatId::Xml, "text:xml_decl"));
    }
    if looks_like_json_start(trimmed) {
        return Some(ProbeCandidate::heuristic(
            FormatId::Json,
            "text:json_structure",
        ));
    }
    match claimed_ext {
        Some("md" | "markdown") => {
            return Some(ProbeCandidate::heuristic(
                FormatId::Markdown,
                "text:utf8_markdown",
            ));
        }
        Some("csv" | "tsv") => {
            return Some(ProbeCandidate::heuristic(FormatId::Csv, "text:utf8_csv"));
        }
        Some("yaml" | "yml") => {
            return Some(ProbeCandidate::heuristic(FormatId::Yaml, "text:utf8_yaml"));
        }
        _ => {}
    }
    if trimmed.is_ascii()
        || trimmed
            .chars()
            .all(|c| !c.is_control() || matches!(c, '\n' | '\r' | '\t'))
    {
        return Some(ProbeCandidate::heuristic(
            FormatId::PlainText,
            "text:utf8_plain",
        ));
    }
    None
}

fn looks_like_json_start(trimmed: &str) -> bool {
    if let Some(rest) = trimmed.strip_prefix('{') {
        let after = rest.trim_start();
        return after.starts_with('"') || after.starts_with('}');
    }
    if let Some(rest) = trimmed.strip_prefix('[') {
        let after = rest.trim_start();
        return after.starts_with(']')
            || after.starts_with('{')
            || after.starts_with('[')
            || after.starts_with('"')
            || after.starts_with('-')
            || after.as_bytes().first().is_some_and(u8::is_ascii_digit)
            || after.starts_with("true")
            || after.starts_with("false")
            || after.starts_with("null");
    }
    false
}

fn extract_claimed_extension(file_name_or_ext: Option<&str>) -> Option<String> {
    let raw = file_name_or_ext?.trim();
    if raw.is_empty() {
        return None;
    }
    let file_part = raw.rsplit(['/', '\\']).next().unwrap_or(raw);
    let ext = if let Some((_, after_dot)) = file_part.rsplit_once('.') {
        after_dot
    } else if !file_part.contains('.') && file_part.len() <= 10 && raw.starts_with('.') {
        file_part
    } else {
        return None;
    };
    let normalized = ext.trim_start_matches('.').to_ascii_lowercase();
    if normalized.is_empty() {
        None
    } else {
        Some(normalized)
    }
}

fn evaluate_extension_match(
    detected: FormatId,
    claimed_ext: Option<&str>,
) -> (bool, Option<String>) {
    let Some(ext) = claimed_ext else {
        return (false, None);
    };
    if detected == FormatId::Unknown {
        return (false, None);
    }
    if detected.canonical_extensions().contains(&ext) {
        return (false, None);
    }
    // Text-on-text variations (e.g., `.txt` holding JSON/YAML/SVG XML or `.md` holding plain text)
    // should not raise a binary spoofing alarm unless HTML/executable is masked as a rich document/image.
    let text_safe = matches!(
        detected,
        FormatId::PlainText | FormatId::Json | FormatId::Yaml | FormatId::Csv | FormatId::Markdown
    ) && matches!(
        ext,
        "txt"
            | "log"
            | "md"
            | "markdown"
            | "json"
            | "yaml"
            | "yml"
            | "csv"
            | "tsv"
            | "ini"
            | "toml"
            | "cfg"
            | "conf"
    );
    if text_safe {
        return (false, None);
    }

    let expected = detected.canonical_extensions().join("/");
    (
        true,
        Some(format!(
            "claimed extension '.{ext}' contradicts detected format '{detected:?}' (expected '.{expected}')"
        )),
    )
}

fn build_signature_match(candidate: ProbeCandidate, claimed_ext: Option<String>) -> SignatureMatch {
    let (spoofing_mismatch, spoofing_reason) =
        evaluate_extension_match(candidate.format, claimed_ext.as_deref());
    SignatureMatch {
        detected_format: candidate.format,
        mime_type: candidate.format.mime_type().to_owned(),
        media_kind: candidate.format.media_kind(),
        canonical_extensions: candidate
            .format
            .canonical_extensions()
            .iter()
            .map(|s| (*s).to_owned())
            .collect(),
        confidence: candidate.confidence,
        matched_rule: candidate.rule.to_owned(),
        claimed_extension: claimed_ext,
        spoofing_mismatch,
        spoofing_reason,
    }
}

/// Detects binary/structural file signature and extension spoofing mismatches from bytes.
#[must_use]
pub fn detect_signature_from_bytes(data: &[u8], file_name_or_ext: Option<&str>) -> SignatureMatch {
    let claimed_ext = extract_claimed_extension(file_name_or_ext);
    let candidate = probe_images_and_docs(data)
        .or_else(|| probe_riff(data))
        .or_else(|| probe_iso_bmff(data))
        .or_else(|| probe_audio_and_ebml(data))
        .or_else(|| probe_archives_and_exec(data))
        .or_else(|| probe_text_formats(data, claimed_ext.as_deref()))
        .unwrap_or(ProbeCandidate {
            format: FormatId::Unknown,
            confidence: DetectionConfidence::Unknown,
            rule: "unknown:unmatched",
        });

    build_signature_match(candidate, claimed_ext)
}

/// Probes signature from a seekable byte stream using bounded I/O (`MAX_HEADER_PROBE_BYTES`).
///
/// If initial local ZIP headers did not isolate OOXML/EPUB entries, also inspects the ZIP
/// Central Directory near EOF without buffering the entire archive into memory.
///
/// # Errors
/// Returns [`FileFusionError`] if reading or seeking the input stream fails.
pub fn detect_signature_from_reader<R: Read + Seek>(
    reader: &mut R,
    file_name_or_ext: Option<&str>,
) -> FileFusionResult<SignatureMatch> {
    reader.seek(SeekFrom::Start(0))?;
    let mut header = vec![0u8; MAX_HEADER_PROBE_BYTES];
    let mut total_read = 0usize;
    while total_read < header.len() {
        let n = reader.read(&mut header[total_read..])?;
        if n == 0 {
            break;
        }
        total_read += n;
    }
    header.truncate(total_read);

    let claimed_ext = extract_claimed_extension(file_name_or_ext);
    let mut initial = detect_signature_from_bytes(&header, file_name_or_ext);
    if initial.detected_format == FormatId::Zip && initial.matched_rule == "magic:zip_pk" {
        if let Some(refined) = probe_zip_central_directory(reader) {
            initial = build_signature_match(refined, claimed_ext);
        }
    }
    Ok(initial)
}

/// Opens `path` read-only and detects its true binary signature and extension compatibility.
///
/// # Errors
/// Returns [`FileFusionError`] (`FF-ERR-IO-NOT-FOUND`, etc.) if the file cannot be opened or read.
pub fn detect_signature_from_path(path: &Path) -> FileFusionResult<SignatureMatch> {
    let mut file = File::open(path)?;
    let hint = path.file_name().and_then(|name| name.to_str());
    detect_signature_from_reader(&mut file, hint)
}

/// Enforces strict spoofing checks against a [`SignatureMatch`] (`FR-DET-002`).
///
/// # Errors
/// Returns [`FileFusionError`] (`FF-ERR-SEC-SPOOFED`, exit code `3`) when `strict` is
/// `true` and [`SignatureMatch::spoofing_mismatch`] is flagged.
pub fn enforce_spoofing_policy(probe: &SignatureMatch, strict: bool) -> FileFusionResult<()> {
    if strict && probe.spoofing_mismatch {
        let reason = probe
            .spoofing_reason
            .clone()
            .unwrap_or_else(|| String::from("extension contradicts true binary signature"));
        return Err(FileFusionError::new(ErrorCode::SecSpoofed, reason)
            .with_remediation("Inspect true MIME type (`filefusion inspect`) before processing."));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::io::Cursor;

    use crate::CliExitCode;

    use super::*;

    #[test]
    fn detects_magic_and_container_subformats_accurately() {
        let png =
            detect_signature_from_bytes(b"\x89PNG\r\n\x1a\n\x00\x00\x00\rIHDR", Some("photo.png"));
        assert_eq!(png.detected_format, FormatId::Png);
        assert_eq!(png.mime_type, "image/png");
        assert!(!png.spoofing_mismatch);

        let webp = detect_signature_from_bytes(b"RIFF\x24\x00\x00\x00WEBPVP8 ", Some("img.webp"));
        assert_eq!(webp.detected_format, FormatId::Webp);
        assert!(!webp.spoofing_mismatch);

        let avif = detect_signature_from_bytes(
            b"\x00\x00\x00\x1cftypavif\x00\x00\x00\x00avifmif1",
            Some("hero.avif"),
        );
        assert_eq!(avif.detected_format, FormatId::Avif);
        assert!(!avif.spoofing_mismatch);

        let mut tar = vec![0u8; 512];
        tar[257..262].copy_from_slice(b"ustar");
        let tar_match = detect_signature_from_bytes(&tar, Some("bundle.tar"));
        assert_eq!(tar_match.detected_format, FormatId::Tar);
        assert!(!tar_match.spoofing_mismatch);
    }

    #[test]
    fn differentiates_zip_subformats_via_local_header_probing() {
        fn build_zip_entry(name: &[u8], payload: &[u8]) -> Vec<u8> {
            let mut buf = Vec::new();
            buf.extend_from_slice(&ZIP_LOCAL_FILE_MAGIC);
            buf.extend_from_slice(&[20, 0, 0, 0, 0, 0]); // ver, flags, method=stored
            buf.extend_from_slice(&[0; 8]); // time, date, crc
            buf.extend_from_slice(&(u32::try_from(payload.len()).unwrap_or(0)).to_le_bytes());
            buf.extend_from_slice(&(u32::try_from(payload.len()).unwrap_or(0)).to_le_bytes());
            buf.extend_from_slice(&(u16::try_from(name.len()).unwrap_or(0)).to_le_bytes());
            buf.extend_from_slice(&[0, 0]); // extra len
            buf.extend_from_slice(name);
            buf.extend_from_slice(payload);
            buf
        }

        let docx_bytes = build_zip_entry(b"word/document.xml", b"<w:document/>");
        let docx = detect_signature_from_bytes(&docx_bytes, Some("report.docx"));
        assert_eq!(docx.detected_format, FormatId::Docx);
        assert!(!docx.spoofing_mismatch);

        let epub_bytes = build_zip_entry(b"mimetype", b"application/epub+zip");
        let epub = detect_signature_from_bytes(&epub_bytes, Some("book.epub"));
        assert_eq!(epub.detected_format, FormatId::Epub);
        assert!(!epub.spoofing_mismatch);

        let mut cursor = Cursor::new(docx_bytes);
        let streamed = detect_signature_from_reader(&mut cursor, Some("report.docx"));
        assert!(streamed.is_ok());
        if let Ok(found) = streamed {
            assert_eq!(found.detected_format, FormatId::Docx);
        }
    }

    #[test]
    fn flags_extension_spoofing_and_enforces_strict_security_error() {
        let html_as_pdf = detect_signature_from_bytes(
            b"<!DOCTYPE html><html><script>alert(1)</script></html>",
            Some("invoice.pdf"),
        );
        assert_eq!(html_as_pdf.detected_format, FormatId::Html);
        assert!(html_as_pdf.spoofing_mismatch);

        let mut pe_header = vec![0u8; 128];
        pe_header[0..2].copy_from_slice(b"MZ");
        let exe_as_jpg = detect_signature_from_bytes(&pe_header, Some("vacation.jpg"));
        assert_eq!(exe_as_jpg.detected_format, FormatId::Executable);
        assert_eq!(exe_as_jpg.media_kind, MediaKind::Executable);
        assert!(exe_as_jpg.spoofing_mismatch);

        let enforced = enforce_spoofing_policy(&exe_as_jpg, true);
        assert!(enforced.is_err());
        if let Err(err) = enforced {
            assert_eq!(err.diagnostic.code, ErrorCode::SecSpoofed);
            assert_eq!(err.exit_code(), CliExitCode::SecurityRejection);
        }

        // TOML `[section]` headers must not misclassify as JSON arrays.
        let toml_cfg = detect_signature_from_bytes(
            b"[workspace]\nmembers = [\"crates/file-core\"]\n",
            Some("Cargo.toml"),
        );
        assert_eq!(toml_cfg.detected_format, FormatId::PlainText);
        assert!(!toml_cfg.spoofing_mismatch);

        let valid_json = detect_signature_from_bytes(b"[{\"ok\":true}]", Some("data.json"));
        assert_eq!(valid_json.detected_format, FormatId::Json);
    }
}
