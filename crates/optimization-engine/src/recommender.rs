//! Content-aware adaptive codec and profile recommender (`FR-ADP-001`, `FR-ADP-002`).

use compression_video::VideoCodec;
use file_core::{
    CompressibilityTier, CompressionProfile, FileInspectionReport, FormatId, ProcessingDomain,
};
use hardware_acceleration::{HardwareProfile, HardwareVendorBackend, select_video_backend};
use serde::{Deserialize, Serialize};

/// Caller intent hints shaping codec and container selection.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[allow(clippy::struct_excessive_bools)]
pub struct RecommendationHints {
    /// Target [`CompressionProfile`] weighting size vs fidelity.
    pub profile: CompressionProfile,
    /// Prioritize universal browser/mobile playback (`JPEG`/`H264`/`MP4`/`Opus`).
    pub web_compatible: bool,
    /// Require mathematically lossless encoding (`PNG`/`FLAC`/`Zstd`/lossless remux).
    pub lossless_required: bool,
    /// Force deterministic CPU software encoding bypassing GPU encoders (`FR-ADP-002`).
    pub prefer_deterministic_cpu: bool,
    /// Strip identifying metadata (`EXIF`/`GPS`/`ID3`/`XMP`/`docProps`).
    pub strip_metadata: bool,
}

impl Default for RecommendationHints {
    fn default() -> Self {
        Self {
            profile: CompressionProfile::BalancedOptimal,
            web_compatible: false,
            lossless_required: false,
            prefer_deterministic_cpu: false,
            strip_metadata: true,
        }
    }
}

/// Actionable codec/engine recommendation emitted before Stage 2 execution (`FR-ADP-001`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CodecRecommendation {
    /// Source format identified during Stage 1 inspection.
    pub source_format: FormatId,
    /// Recommended output container [`FormatId`].
    pub recommended_format: FormatId,
    /// Selected elementary codec or algorithm label (`oxipng`, `ravif`, `libsvtav1`, `h264_nvenc`, etc.).
    pub codec_label: String,
    /// Recommended `CompressionProfile`.
    pub profile: CompressionProfile,
    /// Responsible L3 engine identifier.
    pub target_engine_id: &'static str,
    /// Optional GPU/hardware encoder selected (`None` for CPU / non-video domains).
    pub hardware_backend: Option<HardwareVendorBackend>,
    /// Bit-exact lossless guarantee when enabled.
    pub lossless: bool,
    /// True when input is already compact, encrypted, empty, or restricted and same-format work should skip (`FR-CMP-002`).
    pub should_skip_noop: bool,
    /// True when metadata privacy scrubbing is advised.
    pub strip_metadata: bool,
    /// Rough expected payload reduction percentage (`0..=90`), purely advisory.
    pub estimated_savings_pct: u8,
    /// Ordered human-readable engineering rationales.
    pub rationale: Vec<String>,
}

/// Evaluates inspected metadata, entropy, alpha/resolution properties, and hardware capabilities
/// to recommend the optimal codec, container, and backend (`FR-ADP-001`, `FR-ADP-002`).
#[must_use]
pub fn recommend_codec(
    inspection: &FileInspectionReport,
    hints: &RecommendationHints,
    hw_profile: &HardwareProfile,
) -> CodecRecommendation {
    let source = inspection.detected_format;
    let effective_profile = if hints.lossless_required {
        CompressionProfile::LosslessMaximum
    } else {
        hints.profile
    };
    let strip = hints.strip_metadata
        || inspection
            .classification
            .as_ref()
            .is_some_and(|c| c.privacy_sensitive);

    if let Some(cls) = &inspection.classification {
        if matches!(
            cls.domain,
            ProcessingDomain::RestrictedPayload | ProcessingDomain::EncryptedVault
        ) || matches!(cls.compressibility, CompressibilityTier::Empty)
        {
            return CodecRecommendation {
                source_format: source,
                recommended_format: source,
                codec_label: String::from("noop-guard"),
                profile: effective_profile,
                target_engine_id: "compression-core",
                hardware_backend: None,
                lossless: true,
                should_skip_noop: true,
                strip_metadata: false,
                estimated_savings_pct: 0,
                rationale: vec![format!(
                    "Domain {:?} with compressibility {:?} should not be recompressed automatically.",
                    cls.domain, cls.compressibility
                )],
            };
        }
    }

    match source {
        FormatId::Jpeg
        | FormatId::Png
        | FormatId::Webp
        | FormatId::Avif
        | FormatId::Gif
        | FormatId::Bmp
        | FormatId::Tiff
        | FormatId::Ico => recommend_image_codec(inspection, hints, effective_profile, strip),
        FormatId::Mp4 | FormatId::Mov | FormatId::Mkv | FormatId::Webm | FormatId::Avi => {
            recommend_video_codec(inspection, hints, effective_profile, hw_profile, strip)
        }
        FormatId::Wav
        | FormatId::Flac
        | FormatId::Mp3
        | FormatId::Ogg
        | FormatId::Opus
        | FormatId::Aac
        | FormatId::M4a => recommend_audio_codec(source, hints, effective_profile, strip),
        FormatId::Pdf
        | FormatId::Docx
        | FormatId::Xlsx
        | FormatId::Pptx
        | FormatId::Odt
        | FormatId::Ods
        | FormatId::Odp
        | FormatId::Epub
        | FormatId::Markdown
        | FormatId::Html
        | FormatId::PlainText
        | FormatId::Csv
        | FormatId::Json
        | FormatId::Yaml
        | FormatId::Xml
        | FormatId::Svg => recommend_document_or_text_codec(inspection, effective_profile, strip),
        FormatId::Zip
        | FormatId::Tar
        | FormatId::Gzip
        | FormatId::Zstd
        | FormatId::Xz
        | FormatId::Bzip2
        | FormatId::Lz4
        | FormatId::SevenZ => recommend_archive_codec(inspection, effective_profile, strip),
        other => CodecRecommendation {
            source_format: other,
            recommended_format: other,
            codec_label: String::from("zstd-stream"),
            profile: effective_profile,
            target_engine_id: "tier1-compression-archive",
            hardware_backend: None,
            lossless: true,
            should_skip_noop: inspection.classification.as_ref().is_some_and(|c| {
                c.compressibility == CompressibilityTier::AlreadyCompressedOrEncrypted
            }),
            strip_metadata: strip,
            estimated_savings_pct: estimate_savings(inspection, 15),
            rationale: vec![format!(
                "Fallback lossless stream recommendation for {other:?}."
            )],
        },
    }
}

fn recommend_document_or_text_codec(
    inspection: &FileInspectionReport,
    profile: CompressionProfile,
    strip: bool,
) -> CodecRecommendation {
    let source = inspection.detected_format;
    match source {
        FormatId::Pdf => CodecRecommendation {
            source_format: source,
            recommended_format: FormatId::Pdf,
            codec_label: String::from("lopdf-flate-xobject"),
            profile,
            target_engine_id: "tier1-compression-pdf",
            hardware_backend: None,
            lossless: matches!(profile, CompressionProfile::LosslessMaximum),
            should_skip_noop: false,
            strip_metadata: strip,
            estimated_savings_pct: estimate_savings(inspection, 28),
            rationale: vec![String::from(
                "PDF stream compaction, embedded image XObject optimization, and orphan pruning selected.",
            )],
        },
        FormatId::Docx
        | FormatId::Xlsx
        | FormatId::Pptx
        | FormatId::Odt
        | FormatId::Ods
        | FormatId::Odp
        | FormatId::Epub => CodecRecommendation {
            source_format: source,
            recommended_format: source,
            codec_label: String::from("ooxml-odf-asset-repack"),
            profile,
            target_engine_id: "tier1-compression-office",
            hardware_backend: None,
            lossless: matches!(profile, CompressionProfile::LosslessMaximum),
            should_skip_noop: false,
            strip_metadata: strip,
            estimated_savings_pct: estimate_savings(inspection, 24),
            rationale: vec![String::from(
                "ZIP-based office/EPUB structural XML minification + embedded raster optimization selected.",
            )],
        },
        _ => CodecRecommendation {
            source_format: source,
            recommended_format: FormatId::Zstd,
            codec_label: String::from("zstd-stream"),
            profile,
            target_engine_id: "tier1-compression-archive",
            hardware_backend: None,
            lossless: true,
            should_skip_noop: false,
            strip_metadata: strip,
            estimated_savings_pct: estimate_savings(inspection, 65),
            rationale: vec![String::from(
                "Text/markup content achieves maximum lossless density with Zstandard (`tar.zst` / `.zst`).",
            )],
        },
    }
}

fn recommend_image_codec(
    inspection: &FileInspectionReport,
    hints: &RecommendationHints,
    profile: CompressionProfile,
    strip: bool,
) -> CodecRecommendation {
    let source = inspection.detected_format;
    let has_alpha = inspection
        .metadata_summary
        .color_space
        .as_deref()
        .is_some_and(|cs| {
            let low = cs.to_ascii_lowercase();
            low.contains("rgba") || low.contains("alpha")
        });
    let mut rationale = Vec::new();

    if matches!(
        profile,
        CompressionProfile::LosslessMaximum | CompressionProfile::ArchivalColdStorage
    ) || hints.lossless_required
    {
        let target = if matches!(source, FormatId::Bmp | FormatId::Tiff | FormatId::Png) {
            FormatId::Png
        } else {
            source
        };
        let codec = if target == FormatId::Png {
            "oxipng"
        } else {
            "lossless-raster"
        };
        rationale.push(format!(
            "Lossless profile selected {codec} targeting {target:?} (alpha={has_alpha})."
        ));
        let already_compact = matches!(source, FormatId::Jpeg | FormatId::Webp | FormatId::Avif)
            && !strip
            && inspection.classification.as_ref().is_some_and(|c| {
                c.compressibility == CompressibilityTier::AlreadyCompressedOrEncrypted
            });
        return CodecRecommendation {
            source_format: source,
            recommended_format: target,
            codec_label: String::from(codec),
            profile,
            target_engine_id: "tier1-compression-image",
            hardware_backend: None,
            lossless: true,
            should_skip_noop: already_compact,
            strip_metadata: strip,
            estimated_savings_pct: estimate_savings(
                inspection,
                if target == FormatId::Png { 22 } else { 8 },
            ),
            rationale,
        };
    }

    let (target, codec) = if hints.web_compatible {
        if has_alpha {
            rationale.push(String::from(
                "Transparent web image routed to lossless WebP.",
            ));
            (FormatId::Webp, "webp-lossless")
        } else {
            rationale.push(String::from(
                "Opaque web image routed to WebP / JPEG compatibility target.",
            ));
            (FormatId::Webp, "webp")
        }
    } else if matches!(profile, CompressionProfile::ExtremeSizeTarget) {
        rationale.push(String::from(
            "ExtremeSizeTarget selected AVIF (`ravif`) for highest perceptual density.",
        ));
        (FormatId::Avif, "ravif")
    } else if source == FormatId::Png && has_alpha {
        rationale.push(String::from(
            "Opaque-preserving alpha PNG kept in lossless `oxipng`/`WebP`.",
        ));
        (FormatId::Webp, "webp-lossless")
    } else {
        rationale.push(String::from(
            "Balanced profile routed photographic/uncompressed raster to WebP.",
        ));
        (FormatId::Webp, "webp")
    };

    CodecRecommendation {
        source_format: source,
        recommended_format: target,
        codec_label: String::from(codec),
        profile,
        target_engine_id: "tier1-compression-image",
        hardware_backend: None,
        lossless: has_alpha && target == FormatId::Webp,
        should_skip_noop: false,
        strip_metadata: strip,
        estimated_savings_pct: estimate_savings(inspection, 45),
        rationale,
    }
}

fn recommend_video_codec(
    inspection: &FileInspectionReport,
    hints: &RecommendationHints,
    profile: CompressionProfile,
    hw_profile: &HardwareProfile,
    strip: bool,
) -> CodecRecommendation {
    let source = inspection.detected_format;
    let target = if hints.web_compatible {
        FormatId::Mp4
    } else if matches!(profile, CompressionProfile::ArchivalColdStorage) {
        FormatId::Mkv
    } else {
        source
    };
    let codec = if hints.web_compatible {
        VideoCodec::H264
    } else {
        VideoCodec::default_for_container(target, profile)
    };
    let hw_decision = select_video_backend(hw_profile, target, hints.prefer_deterministic_cpu);
    let codec_label = match (codec, hw_decision.selected_backend) {
        (VideoCodec::H264, Some(HardwareVendorBackend::Nvenc)) => "h264_nvenc",
        (VideoCodec::Hevc, Some(HardwareVendorBackend::Nvenc)) => "hevc_nvenc",
        (VideoCodec::Av1, Some(HardwareVendorBackend::Nvenc)) => "av1_nvenc",
        (VideoCodec::H264, _) => "libx264",
        (VideoCodec::Hevc, _) => "libx265",
        (VideoCodec::Vp9, _) => "libvpx-vp9",
        (VideoCodec::Av1, _) => "libsvtav1",
    };

    CodecRecommendation {
        source_format: source,
        recommended_format: target,
        codec_label: String::from(codec_label),
        profile,
        target_engine_id: "compression-video-tier2-ffmpeg",
        hardware_backend: hw_decision.selected_backend,
        lossless: false,
        should_skip_noop: false,
        strip_metadata: strip,
        estimated_savings_pct: estimate_savings(inspection, 35),
        rationale: vec![hw_decision.reason],
    }
}

fn recommend_audio_codec(
    source: FormatId,
    hints: &RecommendationHints,
    profile: CompressionProfile,
    strip: bool,
) -> CodecRecommendation {
    if source == FormatId::Wav
        || hints.lossless_required
        || matches!(
            profile,
            CompressionProfile::LosslessMaximum | CompressionProfile::ArchivalColdStorage
        )
    {
        return CodecRecommendation {
            source_format: source,
            recommended_format: FormatId::Flac,
            codec_label: String::from("flacenc"),
            profile,
            target_engine_id: "tier1-compression-audio",
            hardware_backend: None,
            lossless: true,
            should_skip_noop: source == FormatId::Flac && !strip,
            strip_metadata: strip,
            estimated_savings_pct: if source == FormatId::Wav { 42 } else { 5 },
            rationale: vec![String::from(
                "Lossless PCM/audio routed to pure-Rust FLAC (`flacenc`).",
            )],
        };
    }

    CodecRecommendation {
        source_format: source,
        recommended_format: if hints.web_compatible {
            FormatId::Opus
        } else {
            source
        },
        codec_label: String::from(if hints.web_compatible {
            "libopus"
        } else {
            "tier1-audio"
        }),
        profile,
        target_engine_id: "tier1-compression-audio",
        hardware_backend: None,
        lossless: false,
        should_skip_noop: matches!(source, FormatId::Opus | FormatId::Mp3 | FormatId::Aac)
            && !hints.web_compatible
            && !strip,
        strip_metadata: strip,
        estimated_savings_pct: if hints.web_compatible { 20 } else { 5 },
        rationale: vec![String::from(
            "Already perceptual audio avoids unnecessary generation loss unless transcoded.",
        )],
    }
}

fn recommend_archive_codec(
    inspection: &FileInspectionReport,
    profile: CompressionProfile,
    strip: bool,
) -> CodecRecommendation {
    let source = inspection.detected_format;
    let target = match profile {
        CompressionProfile::ExtremeSizeTarget => FormatId::Xz,
        CompressionProfile::ArchivalColdStorage | CompressionProfile::BalancedOptimal => {
            FormatId::Zstd
        }
        _ => source,
    };
    let already_modern = matches!(source, FormatId::Zstd | FormatId::Xz | FormatId::SevenZ);
    CodecRecommendation {
        source_format: source,
        recommended_format: target,
        codec_label: format!("{target:?}").to_ascii_lowercase(),
        profile,
        target_engine_id: "tier1-compression-archive",
        hardware_backend: None,
        lossless: true,
        should_skip_noop: already_modern && target == source,
        strip_metadata: strip,
        estimated_savings_pct: if source == FormatId::Tar {
            65
        } else if already_modern {
            2
        } else {
            15
        },
        rationale: vec![format!(
            "Archive optimizer selected {target:?} for profile {profile:?}."
        )],
    }
}

fn estimate_savings(inspection: &FileInspectionReport, base_pct: u8) -> u8 {
    match inspection
        .classification
        .as_ref()
        .map(|c| c.compressibility)
    {
        Some(CompressibilityTier::Empty) => 0,
        Some(CompressibilityTier::AlreadyCompressedOrEncrypted) => base_pct.min(12),
        Some(CompressibilityTier::HighlyCompressible) => base_pct.clamp(55, 90),
        _ => base_pct,
    }
}
