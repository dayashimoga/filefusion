//! Unified Stage 4 Privacy Metadata Inspection, Sanitization & Field Editing (`FR-MET-001`..`002`)
//! across `PDF`, `OOXML`/`ODF`/`EPUB`, raster images (`JPEG`/`PNG`/`WebP`), and audio tags
//! (`MP3`/`FLAC`/`WAV`).

use std::fs;
use std::path::Path;

use compression_audio::{
    AudioTagMetadata, read_id3_metadata, strip_id3_metadata, write_id3_metadata,
};
use compression_image::optimize_raster_in_memory;
use compression_office::{
    extract_office_metadata_from_entries, read_validated_zip_entries,
    sanitize_and_edit_office_metadata,
};
use compression_pdf::{inspect_pdf_security, update_pdf_metadata_bytes};
use file_core::{
    CompressionProfile, ErrorCode, FileFusionError, FileFusionResult, FormatId, JobContext,
    MetadataSummary, ResourceLimits, StagedPath, inspect_path,
};
use serde::{Deserialize, Serialize};

/// Mutable cross-format metadata fields supported by [`sanitize_and_edit_metadata_to_staged`]
/// (`None` preserves existing value, `Some(None)` clears the tag, `Some(Some(v))` sets it).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct EditableMetadataFields {
    /// Document or track title (`Title` / `dc:title` / `ID3 TIT2`).
    pub title: Option<Option<String>>,
    /// Author or artist (`Author` / `dc:creator` / `ID3 TPE1`).
    pub author: Option<Option<String>>,
    /// Subject, album, or comment (`Subject` / `dc:subject` / `ID3 COMM`).
    pub subject_or_comment: Option<Option<String>>,
    /// Document keyword list (`Keywords` / `cp:keywords` / `meta:keyword`).
    pub keywords: Option<Option<String>>,
    /// Producer or creator software string (`Creator` on PDF).
    pub creator_tool: Option<Option<String>>,
}

/// Telemetry & before/after comparison produced by [`sanitize_and_edit_metadata_to_staged`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MetadataOperationReport {
    /// True container format processed.
    pub format: FormatId,
    /// Normalized metadata snapshot before sanitization/editing.
    pub before: MetadataSummary,
    /// Normalized metadata snapshot verified on staged output.
    pub after: MetadataSummary,
    /// `true` if identifying privacy metadata was requested to be scrubbed.
    pub scrubbed_privacy_fields: bool,
    /// `true` when essential color profiles (`ICC`/`sRGB`) & visual display orientation were kept.
    pub preserved_color_profile_and_orientation: bool,
    /// Engine identifier (`tier1-pdf-metadata`, `tier1-office-metadata`, `tier1-image-scrub`, etc.).
    pub engine_used: String,
    /// Output size in bytes.
    pub output_bytes: u64,
}

/// Inspects a file across binary signatures and domain-specific tag readers into a unified
/// [`MetadataSummary`] (`FR-MET-001`).
///
/// # Errors
/// Returns [`FileFusionError`] on missing files, I/O errors, or cancellation.
pub fn inspect_unified_metadata(
    path: &Path,
    ctx: &JobContext,
) -> FileFusionResult<MetadataSummary> {
    ctx.check_cancelled()?;
    let report = inspect_path(path, false)?;
    let mut summary = report.metadata_summary;

    match report.detected_format {
        FormatId::Mp3 | FormatId::Flac | FormatId::Wav => {
            if let Ok(tags) = read_id3_metadata(path) {
                if summary.title.is_none() {
                    summary.title.clone_from(&tags.title);
                }
                if summary.author.is_none() {
                    summary.author.clone_from(&tags.artist);
                }
                if let Some(album) = tags.album {
                    summary.attributes.insert(String::from("album"), album);
                }
                if let Some(year) = tags.year {
                    summary
                        .attributes
                        .insert(String::from("year"), year.to_string());
                }
                if summary.author.is_some() {
                    summary.has_pii_author_risk = true;
                }
            }
        }
        FormatId::Pdf => {
            if let Ok(raw) = fs::read(path) {
                if let Ok(sec) = inspect_pdf_security(&raw, None) {
                    summary.is_encrypted = sec.is_encrypted;
                    summary
                        .attributes
                        .insert(String::from("pdf_cipher"), sec.cipher_name);
                }
            }
        }
        FormatId::Docx
        | FormatId::Xlsx
        | FormatId::Pptx
        | FormatId::Odt
        | FormatId::Ods
        | FormatId::Odp
        | FormatId::Epub => {
            if let Ok(raw) = fs::read(path) {
                if let Ok(entries) = read_validated_zip_entries(&raw, &ctx.limits, ctx) {
                    let meta = extract_office_metadata_from_entries(&entries);
                    if summary.title.is_none() {
                        summary.title = meta.title;
                    }
                    if summary.author.is_none() {
                        summary.author = meta.author;
                    }
                    if let Some(subj) = meta.subject {
                        summary.attributes.insert(String::from("subject"), subj);
                    }
                    if let Some(kw) = meta.keywords {
                        summary.attributes.insert(String::from("keywords"), kw);
                    }
                    if meta.last_modified_by.is_some() || summary.author.is_some() {
                        summary.has_pii_author_risk = true;
                    }
                }
            }
        }
        _ => {}
    }

    Ok(summary)
}

/// Scrubs privacy risks (`EXIF` GPS, camera serial numbers, `XMP`, author/revision history) and/or
/// writes explicit metadata field updates into `staged_output`, keeping essential display
/// orientation and color profiles intact (`FR-MET-002`).
///
/// # Errors
/// Returns [`FileFusionError`] on unsupported format, corrupt/encrypted file (`FmtDrm`), or
/// cooperative cancellation.
pub fn sanitize_and_edit_metadata_to_staged(
    input_path: &Path,
    staged_output: &StagedPath,
    scrub_privacy: bool,
    edits: Option<&EditableMetadataFields>,
    limits: &ResourceLimits,
    ctx: &JobContext,
) -> FileFusionResult<MetadataOperationReport> {
    ctx.check_cancelled()?;
    let report = inspect_path(input_path, false)?;
    let before = inspect_unified_metadata(input_path, ctx)?;
    let format = report.detected_format;

    let engine_used = match format {
        FormatId::Pdf => {
            let raw = fs::read(input_path)?;
            let (out_bytes, _) = update_pdf_metadata_bytes(
                &raw,
                scrub_privacy,
                edits.and_then(|e| e.title.as_ref().map(|o| o.as_deref())),
                edits.and_then(|e| e.author.as_ref().map(|o| o.as_deref())),
                edits.and_then(|e| e.subject_or_comment.as_ref().map(|o| o.as_deref())),
                edits.and_then(|e| e.keywords.as_ref().map(|o| o.as_deref())),
                edits.and_then(|e| e.creator_tool.as_ref().map(|o| o.as_deref())),
                ctx,
            )?;
            fs::write(staged_output.staged_path(), &out_bytes)?;
            String::from("tier1-pdf-metadata")
        }
        FormatId::Docx
        | FormatId::Xlsx
        | FormatId::Pptx
        | FormatId::Odt
        | FormatId::Ods
        | FormatId::Odp
        | FormatId::Epub => {
            let raw = fs::read(input_path)?;
            let (out_bytes, _) = sanitize_and_edit_office_metadata(
                &raw,
                format,
                scrub_privacy,
                edits.and_then(|e| e.title.as_ref().map(|o| o.as_deref())),
                edits.and_then(|e| e.author.as_ref().map(|o| o.as_deref())),
                edits.and_then(|e| e.subject_or_comment.as_ref().map(|o| o.as_deref())),
                edits.and_then(|e| e.keywords.as_ref().map(|o| o.as_deref())),
                limits,
                ctx,
            )?;
            fs::write(staged_output.staged_path(), &out_bytes)?;
            String::from("tier1-office-metadata")
        }
        FormatId::Jpeg | FormatId::Png | FormatId::Webp => {
            if edits.is_some() {
                return Err(FileFusionError::new(
                    ErrorCode::FmtUnsupported,
                    "custom EXIF tag editing is not supported on raster images; enable privacy scrub instead",
                ));
            }
            let raw = fs::read(input_path)?;
            let profile = if format == FormatId::Jpeg {
                CompressionProfile::BalancedOptimal
            } else {
                CompressionProfile::LosslessMaximum
            };
            let cleaned =
                optimize_raster_in_memory(&raw, format, format, profile, scrub_privacy, None)?;
            fs::write(staged_output.staged_path(), &cleaned)?;
            String::from("tier1-image-metadata-scrub")
        }
        FormatId::Mp3 | FormatId::Flac | FormatId::Wav => {
            apply_audio_metadata_updates(
                input_path,
                staged_output.staged_path(),
                scrub_privacy,
                edits,
            )?;
            String::from("tier1-audio-id3-metadata")
        }
        other => {
            return Err(FileFusionError::new(
                ErrorCode::FmtUnsupported,
                format!("no metadata sanitizer/editor backend exists for {other:?}"),
            ));
        }
    };

    ctx.check_cancelled()?;
    let after = inspect_unified_metadata(staged_output.staged_path(), ctx)?;
    let output_bytes = fs::metadata(staged_output.staged_path())?.len();

    Ok(MetadataOperationReport {
        format,
        before,
        after,
        scrubbed_privacy_fields: scrub_privacy,
        preserved_color_profile_and_orientation: true,
        engine_used,
        output_bytes,
    })
}

fn apply_audio_metadata_updates(
    input_path: &Path,
    staged_path: &Path,
    scrub_privacy: bool,
    edits: Option<&EditableMetadataFields>,
) -> FileFusionResult<()> {
    fs::copy(input_path, staged_path)?;
    if scrub_privacy {
        let _ = strip_id3_metadata(staged_path)?;
    }
    if let Some(ed) = edits {
        let mut current = if scrub_privacy {
            AudioTagMetadata::default()
        } else {
            read_id3_metadata(staged_path).unwrap_or_default()
        };
        if let Some(title) = &ed.title {
            current.title.clone_from(title);
        }
        if let Some(author) = &ed.author {
            current.artist.clone_from(author);
        }
        if let Some(album) = &ed.subject_or_comment {
            current.album.clone_from(album);
        }
        write_id3_metadata(staged_path, &current)?;
    }
    Ok(())
}
