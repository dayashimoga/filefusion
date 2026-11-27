//! Content-aware domain classification, Shannon entropy estimation, and
//! action recommendations (`FR-ADP-001`, `FR-CMP-002`, `FR-MET-002`).
//!
//! Operates strictly on inspected metadata and bounded sample windows without
//! allocating whole-file buffers or requiring external codecs (`wasm32`-ready).

use serde::{Deserialize, Serialize};

use crate::FormatId;
use crate::metadata::{FileInspectionReport, RecommendedAction};

/// High-level processing studio/engine domain for a classified input.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProcessingDomain {
    /// Raster bitmap image (`JPEG`, `PNG`, `WebP`, `AVIF`, `GIF`, `BMP`, `TIFF`, `ICO`, `HEIC`).
    RasterImage,
    /// Scalable vector graphics document (`SVG`).
    VectorImage,
    /// Video container (`MP4`, `MOV`, `MKV`, `WebM`, `AVI`).
    VideoMedia,
    /// Audio container or stream (`FLAC`, `WAV`, `MP3`, `Ogg`, `Opus`, `AAC`, `M4A`).
    AudioMedia,
    /// Rich document, spreadsheet, presentation, or ebook.
    DocumentOrEbook,
    /// Text or structured data format (`HTML`, `XML`, `JSON`, `YAML`, `CSV`, `Markdown`, `PlainText`).
    StructuredText,
    /// Archive or compressed stream container.
    ArchiveContainer,
    /// Encrypted container requiring key/password before inspection or transform.
    EncryptedVault,
    /// Native executable or spoofing-flagged payload requiring security review.
    RestrictedPayload,
    /// Unrecognized binary stream.
    UnclassifiedBinary,
}

/// Empirical compressibility classification guiding `FR-CMP-002` no-op guards.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CompressibilityTier {
    /// Zero-byte file; no transform needed.
    Empty,
    /// Uncompressed media/text or low-entropy payload (`< 5.5` bits/byte).
    HighlyCompressible,
    /// Moderate entropy or structured container (`5.5..7.75` bits/byte) with recompression headroom.
    ModeratelyCompressible,
    /// High-entropy, encrypted, or modern pre-compressed stream (`>= 7.75` bits/byte).
    AlreadyCompressedOrEncrypted,
}

/// Complete domain classification, entropy score, and recommended actions for an inspected file.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FileClassification {
    /// Target processing studio domain.
    pub domain: ProcessingDomain,
    /// Estimated compressibility bucket for `FR-CMP-002` planning.
    pub compressibility: CompressibilityTier,
    /// Shannon entropy scaled by `1000` (`0..=8000`, representing `0.000..=8.000` bits/byte).
    pub entropy_milli_bits: u16,
    /// `true` when GPS, author, or EXIF/XMP privacy tags are present (`FR-MET-002`).
    pub privacy_sensitive: bool,
    /// `true` when spoofing mismatch or executable signature requires security triage (`FR-DET-002`).
    pub requires_security_review: bool,
    /// Deterministic ordered action recommendations.
    pub recommended_actions: Vec<RecommendedAction>,
}

impl FileClassification {
    /// Converts integer `entropy_milli_bits` into floating-point bits per byte (`0.0..=8.0`).
    #[must_use]
    pub fn entropy_bits_per_byte(&self) -> f64 {
        f64::from(self.entropy_milli_bits) / 1000.0
    }
}

/// Computes Shannon entropy in milli-bits per byte (`0..=8000`) over a bounded byte sample.
#[must_use]
pub fn compute_entropy_milli_bits(sample: &[u8]) -> u16 {
    if sample.is_empty() {
        return 0;
    }
    let mut counts = [0u64; 256];
    for &byte in sample {
        counts[usize::from(byte)] += 1;
    }
    #[allow(clippy::cast_precision_loss)]
    let total = sample.len() as f64;
    let mut entropy = 0.0f64;
    for &count in &counts {
        if count > 0 {
            #[allow(clippy::cast_precision_loss)]
            let p = (count as f64) / total;
            entropy -= p * p.log2();
        }
    }
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let scaled = (entropy.clamp(0.0, 8.0) * 1000.0).round() as u16;
    scaled
}

/// Maps `(FormatId, spoofing_mismatch, is_encrypted)` to a [`ProcessingDomain`].
#[must_use]
pub const fn classify_processing_domain(
    format: FormatId,
    spoofing_mismatch: bool,
    is_encrypted: bool,
) -> ProcessingDomain {
    if spoofing_mismatch || matches!(format, FormatId::Executable) {
        return ProcessingDomain::RestrictedPayload;
    }
    if is_encrypted || matches!(format, FormatId::Ffenc) {
        return ProcessingDomain::EncryptedVault;
    }
    match format {
        FormatId::Jpeg
        | FormatId::Png
        | FormatId::Webp
        | FormatId::Avif
        | FormatId::Heic
        | FormatId::Gif
        | FormatId::Bmp
        | FormatId::Tiff
        | FormatId::Ico => ProcessingDomain::RasterImage,
        FormatId::Svg => ProcessingDomain::VectorImage,
        FormatId::Mp4 | FormatId::Mov | FormatId::Mkv | FormatId::Webm | FormatId::Avi => {
            ProcessingDomain::VideoMedia
        }
        FormatId::Flac
        | FormatId::Wav
        | FormatId::Mp3
        | FormatId::Ogg
        | FormatId::Opus
        | FormatId::Aac
        | FormatId::M4a => ProcessingDomain::AudioMedia,
        FormatId::Pdf
        | FormatId::Docx
        | FormatId::Xlsx
        | FormatId::Pptx
        | FormatId::Odt
        | FormatId::Ods
        | FormatId::Odp
        | FormatId::Epub
        | FormatId::Mobi
        | FormatId::Rtf => ProcessingDomain::DocumentOrEbook,
        FormatId::Html
        | FormatId::Xml
        | FormatId::Json
        | FormatId::Yaml
        | FormatId::Csv
        | FormatId::Markdown
        | FormatId::PlainText => ProcessingDomain::StructuredText,
        FormatId::Zip
        | FormatId::Tar
        | FormatId::Gzip
        | FormatId::Bzip2
        | FormatId::Xz
        | FormatId::Zstd
        | FormatId::Lz4
        | FormatId::SevenZ => ProcessingDomain::ArchiveContainer,
        FormatId::Ffenc => ProcessingDomain::EncryptedVault,
        FormatId::Executable => ProcessingDomain::RestrictedPayload,
        FormatId::Unknown => ProcessingDomain::UnclassifiedBinary,
    }
}

fn determine_compressibility(
    report: &FileInspectionReport,
    entropy_milli_bits: u16,
) -> CompressibilityTier {
    if report.byte_size == 0 {
        return CompressibilityTier::Empty;
    }
    if report.metadata_summary.is_encrypted || report.detected_format == FormatId::Ffenc {
        return CompressibilityTier::AlreadyCompressedOrEncrypted;
    }
    if matches!(
        report.detected_format,
        FormatId::Bmp
            | FormatId::Wav
            | FormatId::Tar
            | FormatId::Svg
            | FormatId::Html
            | FormatId::Xml
            | FormatId::Json
            | FormatId::Yaml
            | FormatId::Csv
            | FormatId::Markdown
            | FormatId::PlainText
    ) {
        return CompressibilityTier::HighlyCompressible;
    }
    if matches!(
        report.detected_format,
        FormatId::Avif
            | FormatId::Heic
            | FormatId::Zstd
            | FormatId::Xz
            | FormatId::SevenZ
            | FormatId::Opus
    ) || entropy_milli_bits >= 7_750
    {
        return CompressibilityTier::AlreadyCompressedOrEncrypted;
    }
    if entropy_milli_bits < 5_500 {
        CompressibilityTier::HighlyCompressible
    } else {
        CompressibilityTier::ModeratelyCompressible
    }
}

fn action(action_id: &str, title: &str, reason: &str) -> RecommendedAction {
    RecommendedAction {
        action_id: String::from(action_id),
        title: String::from(title),
        reason: String::from(reason),
    }
}

fn build_recommended_actions(
    report: &FileInspectionReport,
    domain: ProcessingDomain,
    compressibility: CompressibilityTier,
    privacy_sensitive: bool,
) -> Vec<RecommendedAction> {
    let mut actions = Vec::new();

    if domain == ProcessingDomain::RestrictedPayload {
        actions.push(action(
            "security.inspect_spoofing",
            "Verify binary signature before processing",
            "Extension mismatch or executable payload detected by magic-byte inspection.",
        ));
        return actions;
    }

    if report.metadata_summary.is_encrypted {
        actions.push(action(
            "security.encrypted_input",
            "Decrypt or unlock before optimization",
            "Encryption markers were detected; compressed re-encoding requires unlocked access.",
        ));
    }

    if privacy_sensitive {
        actions.push(action(
            "privacy.scrub_metadata",
            "Strip sensitive EXIF/GPS/author metadata",
            "GPS coordinates or author/EXIF privacy tags were found during inspection.",
        ));
    }

    match compressibility {
        CompressibilityTier::Empty => {
            actions.push(action(
                "validate.empty_input",
                "Skip empty zero-byte file",
                "Zero-byte inputs have no payload to compress or convert.",
            ));
            return actions;
        }
        CompressibilityTier::AlreadyCompressedOrEncrypted => {
            actions.push(action(
                "noop.guard_check",
                "Keep original unless transcoding",
                "High payload entropy or pre-compressed container; FR-CMP-002 prevents inflation.",
            ));
        }
        CompressibilityTier::HighlyCompressible | CompressibilityTier::ModeratelyCompressible => {}
    }

    push_domain_actions(report, domain, compressibility, &mut actions);
    actions
}

fn push_domain_actions(
    report: &FileInspectionReport,
    domain: ProcessingDomain,
    compressibility: CompressibilityTier,
    actions: &mut Vec<RecommendedAction>,
) {
    match domain {
        ProcessingDomain::RasterImage => {
            if report.detected_format == FormatId::Bmp {
                actions.push(action(
                    "compress.image_lossless",
                    "Convert uncompressed BMP to PNG or WebP",
                    "Uncompressed bitmap payload yields major lossless size reduction.",
                ));
            } else {
                actions.push(action(
                    "optimize.image_balanced",
                    "Optimize image with SSIM/no-op guard",
                    "Lossless chunk/entropy tuning or perceptual WebP/AVIF transcoding available.",
                ));
            }
        }
        ProcessingDomain::AudioMedia if report.detected_format == FormatId::Wav => {
            actions.push(action(
                "compress.audio_lossless",
                "Encode PCM WAV to lossless FLAC",
                "Uncompressed PCM audio typically shrinks 40-60% losslessly in FLAC.",
            ));
        }
        ProcessingDomain::DocumentOrEbook if !report.metadata_summary.is_encrypted => {
            actions.push(action(
                "optimize.document_assets",
                "Inspect/compress embedded document assets",
                "Office, EPUB, and PDF containers often hold compressible embedded images/streams.",
            ));
        }
        ProcessingDomain::StructuredText | ProcessingDomain::VectorImage => {
            actions.push(action(
                "compress.archive_zstd",
                "Compress structured text or archive with Zstd",
                "Low-entropy UTF-8 text or SVG compresses efficiently under Zstandard.",
            ));
        }
        ProcessingDomain::ArchiveContainer if report.detected_format == FormatId::Tar => {
            actions.push(action(
                "compress.archive_zstd",
                "Pack TAR archive with Zstandard (.tar.zst)",
                "Uncompressed TAR container gains substantial space savings with Zstd.",
            ));
        }
        ProcessingDomain::VideoMedia
        | ProcessingDomain::AudioMedia
        | ProcessingDomain::ArchiveContainer => {
            if compressibility != CompressibilityTier::AlreadyCompressedOrEncrypted {
                actions.push(action(
                    "optimize.media_remux_or_repack",
                    "Evaluate adaptive transcoding or container repack",
                    "Run Stage 2 codec recommender to verify bitrate or container savings.",
                ));
            }
        }
        ProcessingDomain::DocumentOrEbook
        | ProcessingDomain::EncryptedVault
        | ProcessingDomain::RestrictedPayload
        | ProcessingDomain::UnclassifiedBinary => {}
    }
}

/// Classifies an inspected file and computes domain, compressibility tier, entropy, and actions.
#[must_use]
pub fn classify_inspection_report(
    report: &FileInspectionReport,
    sample: &[u8],
) -> FileClassification {
    let domain = classify_processing_domain(
        report.detected_format,
        report.spoofing_mismatch,
        report.metadata_summary.is_encrypted,
    );
    let entropy_milli_bits = compute_entropy_milli_bits(sample);
    let compressibility = determine_compressibility(report, entropy_milli_bits);
    let privacy_sensitive = report.metadata_summary.has_gps_privacy_risk
        || report.metadata_summary.has_pii_author_risk
        || report.metadata_summary.has_exif
        || report.metadata_summary.has_xmp;
    let requires_security_review =
        report.spoofing_mismatch || report.detected_format == FormatId::Executable;
    let recommended_actions =
        build_recommended_actions(report, domain, compressibility, privacy_sensitive);

    FileClassification {
        domain,
        compressibility,
        entropy_milli_bits,
        privacy_sensitive,
        requires_security_review,
        recommended_actions,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::metadata::{DigestSet, MetadataSummary, compute_digests_from_bytes};
    use crate::{CORE_SCHEMA_VERSION, FormatId};

    fn sample_report(format: FormatId, size: u64, spoofed: bool) -> FileInspectionReport {
        FileInspectionReport {
            schema_version: CORE_SCHEMA_VERSION,
            detected_format: format,
            mime_type: format.mime_type().to_owned(),
            extension_claimed: None,
            spoofing_mismatch: spoofed,
            byte_size: size,
            digests: DigestSet {
                blake3: compute_digests_from_bytes(b"sample").blake3,
                sha256: compute_digests_from_bytes(b"sample").sha256,
                xxhash64: compute_digests_from_bytes(b"sample").xxhash64,
            },
            streams: Vec::new(),
            metadata_summary: MetadataSummary::default(),
            recommended_actions: Vec::new(),
            classification: None,
        }
    }

    #[test]
    fn entropy_distinguishes_constant_text_and_uniform_random_payloads() {
        let zero = vec![b'A'; 4096];
        assert_eq!(compute_entropy_milli_bits(&zero), 0);

        let mut uniform = Vec::with_capacity(2048);
        for _ in 0..8 {
            uniform.extend(0u8..=255u8);
        }
        assert_eq!(compute_entropy_milli_bits(&uniform), 8000);
    }

    #[test]
    fn classifies_uncompressed_media_high_entropy_noop_and_spoofing() {
        let wav_rep = sample_report(FormatId::Wav, 44_100, false);
        let wav_cls = classify_inspection_report(&wav_rep, b"RIFFWAVE");
        assert_eq!(wav_cls.domain, ProcessingDomain::AudioMedia);
        assert_eq!(
            wav_cls.compressibility,
            CompressibilityTier::HighlyCompressible
        );
        assert!(
            wav_cls
                .recommended_actions
                .iter()
                .any(|a| a.action_id == "compress.audio_lossless")
        );

        let zst_rep = sample_report(FormatId::Zstd, 8192, false);
        let high_entropy: Vec<u8> = (0..8).flat_map(|_| 0u8..=255u8).collect();
        let zst_cls = classify_inspection_report(&zst_rep, &high_entropy);
        assert_eq!(
            zst_cls.compressibility,
            CompressibilityTier::AlreadyCompressedOrEncrypted
        );
        assert!(
            zst_cls
                .recommended_actions
                .iter()
                .any(|a| a.action_id == "noop.guard_check")
        );

        let spoof_rep = sample_report(FormatId::Html, 128, true);
        let spoof_cls = classify_inspection_report(&spoof_rep, b"<html></html>");
        assert_eq!(spoof_cls.domain, ProcessingDomain::RestrictedPayload);
        assert!(spoof_cls.requires_security_review);
        assert_eq!(
            spoof_cls.recommended_actions[0].action_id,
            "security.inspect_spoofing"
        );
    }
}
