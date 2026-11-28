//! Safe ZIP container unpacking, bomb/Zip-Slip verification (`FR-SEC-001`), embedded
//! raster optimization via `compression-image` (`MOD-RULE-02`), and `ODF`/`EPUB`
//! `mimetype`-first re-packaging.

use std::io::{Cursor, Read, Write};
use std::path::Path;

use compression_core::CompressionProfileConfig;
use compression_image::{ImageResizeSpec, ResizeFilter, optimize_raster_in_memory};
use file_core::{
    CompressionRequest, ErrorCode, FileFusionError, FileFusionResult, FormatId, JobContext,
    ResourceLimits,
};
use serde::{Deserialize, Serialize};
use validation_engine::{validate_archive_member_path, validate_expansion_budget};
use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, ZipArchive, ZipWriter};

use crate::xml::{
    edit_metadata_xml, is_optimizable_xml_entry, minify_xml_bytes, scrub_metadata_xml,
};

/// Extended options governing `compression-office` structural and media optimization.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OfficeCompressionOptions {
    /// Core compression request (`profile`, `target_format`, `strip_metadata`, `no_op_if_larger`).
    pub base: CompressionRequest,
    /// Recompress embedded raster assets (`word/media/*`, `xl/media/*`, `ppt/media/*`,
    /// `Pictures/*`, `OEBPS/images/*`) via `compression-image`.
    pub recompress_images: bool,
    /// Optional maximum pixel dimension (`width` or `height`) for oversized embedded images.
    pub max_image_dimension: Option<u32>,
    /// Apply lossless `XML` / `XHTML` whitespace & comment minification.
    pub minify_xml: bool,
    /// Scrub privacy metadata fields when `base.strip_metadata` is enabled.
    pub strip_personal_metadata: bool,
}

impl OfficeCompressionOptions {
    /// Derives canonical office/EPUB compression options from a [`CompressionRequest`].
    #[must_use]
    pub fn from_request(base: CompressionRequest) -> Self {
        let max_image_dimension = match base.profile {
            file_core::CompressionProfile::ExtremeSizeTarget => Some(1280),
            file_core::CompressionProfile::WebSharingReady => Some(1920),
            _ => None,
        };
        Self {
            strip_personal_metadata: base.strip_metadata,
            base,
            recompress_images: true,
            max_image_dimension,
            minify_xml: true,
        }
    }
}

/// Summary telemetry produced by container optimization.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct OfficeOptimizationStats {
    /// Total ZIP entries repackaged.
    pub entries_processed: u32,
    /// Embedded raster assets optimized with size reduction.
    pub images_optimized: u32,
    /// `XML`/`XHTML`/`.rels`/`.opf` entries minified losslessly.
    pub xml_entries_minified: u32,
    /// Metadata documents scrubbed for privacy.
    pub metadata_entries_scrubbed: u32,
}

/// Scrubs identifying metadata XML and/or applies explicit metadata field edits inside an `OOXML`,
/// `ODF`, or `EPUB` container while enforcing traversal/bomb limits and preserving `mimetype` order
/// (`FR-MET-002`).
///
/// # Errors
/// Returns [`FileFusionError`] on invalid ZIP syntax, traversal/bomb violation, or cancellation.
#[allow(clippy::too_many_arguments)]
pub fn sanitize_and_edit_office_metadata(
    raw_zip: &[u8],
    format: FormatId,
    scrub_privacy: bool,
    title: Option<Option<&str>>,
    author: Option<Option<&str>>,
    subject: Option<Option<&str>>,
    keywords: Option<Option<&str>>,
    limits: &ResourceLimits,
    ctx: &JobContext,
) -> FileFusionResult<(Vec<u8>, bool)> {
    ctx.check_cancelled()?;
    let entries = read_validated_zip_entries(raw_zip, limits, ctx)?;
    let mut changed = false;
    let mut out_entries = Vec::with_capacity(entries.len());

    for (name, mut payload) in entries {
        ctx.check_cancelled()?;
        if scrub_privacy {
            if let Some(scrubbed) = scrub_metadata_xml(&name, &payload) {
                if scrubbed != payload {
                    payload = scrubbed;
                    changed = true;
                }
            }
        }
        if let Some(edited) = edit_metadata_xml(&name, &payload, title, author, subject, keywords) {
            if edited != payload {
                payload = edited;
                changed = true;
            }
        }
        out_entries.push((name, payload));
    }

    let repackaged = write_office_container_entries(&out_entries, format, ctx)?;
    Ok((repackaged, changed))
}

/// Checks whether a container format requires an uncompressed `mimetype` entry written first.
#[must_use]
pub const fn requires_uncompressed_mimetype_first(format: FormatId) -> bool {
    matches!(
        format,
        FormatId::Epub | FormatId::Odt | FormatId::Ods | FormatId::Odp
    )
}

/// Optimizes an `OOXML` (`DOCX`/`XLSX`/`PPTX`), `ODF` (`ODT`/`ODS`/`ODP`), or `EPUB` container
/// in memory while enforcing `FR-SEC-001` path-traversal & decompression-bomb limits.
///
/// # Errors
/// Returns [`FileFusionError`] on invalid ZIP syntax, Zip-Slip paths (`SecTraversal`),
/// decompression-bomb ceilings (`SecBomb`), or job cancellation.
pub fn optimize_office_container(
    raw_zip: &[u8],
    format: FormatId,
    opts: &OfficeCompressionOptions,
    limits: &ResourceLimits,
    ctx: &JobContext,
) -> FileFusionResult<(Vec<u8>, OfficeOptimizationStats)> {
    let mut entries = read_validated_zip_entries(raw_zip, limits, ctx)?;

    if requires_uncompressed_mimetype_first(format) {
        if let Some(pos) = entries.iter().position(|(name, _)| name == "mimetype") {
            let mime_entry = entries.remove(pos);
            entries.insert(0, mime_entry);
        }
    }

    write_optimized_office_zip(entries, format, opts, ctx)
}

/// Unpacks and validates ZIP container entries (`OOXML`/`ODF`/`EPUB`) while enforcing
/// Zip-Slip path sanitization and decompression-bomb ceilings (`FR-SEC-001`).
///
/// # Errors
/// Returns [`FileFusionError`] on invalid ZIP syntax, Zip-Slip paths (`SecTraversal`),
/// decompression-bomb ceilings (`SecBomb`), or cancellation.
pub fn read_validated_zip_entries(
    raw_zip: &[u8],
    limits: &ResourceLimits,
    ctx: &JobContext,
) -> FileFusionResult<Vec<(String, Vec<u8>)>> {
    ctx.check_cancelled()?;
    let cursor = Cursor::new(raw_zip);
    let mut archive = ZipArchive::new(cursor).map_err(|err| {
        FileFusionError::new(
            ErrorCode::FmtCorrupt,
            format!("invalid Office/EPUB ZIP container: {err}"),
        )
    })?;

    let compressed_input_len = u64::try_from(raw_zip.len()).unwrap_or(u64::MAX);
    let mut declared_total = 0u64;
    let mut entries: Vec<(String, Vec<u8>)> = Vec::with_capacity(archive.len());
    let mut total_read = 0u64;

    for idx in 0..archive.len() {
        ctx.check_cancelled()?;
        let mut entry = archive.by_index(idx).map_err(|err| {
            FileFusionError::new(
                ErrorCode::FmtCorrupt,
                format!("corrupt container entry {idx}: {err}"),
            )
        })?;
        let raw_name = entry.name().to_owned();
        if entry.is_dir() {
            let _ = validate_archive_member_path(&raw_name)?;
            continue;
        }
        let safe_name = validate_archive_member_path(&raw_name)?;

        declared_total = declared_total.saturating_add(entry.size());
        validate_expansion_budget(compressed_input_len, declared_total, 1, limits)?;

        let remaining_cap = limits.max_uncompressed_bytes.saturating_sub(total_read);
        let mut buf = Vec::new();
        let mut limited = (&mut entry).take(remaining_cap.saturating_add(1));
        limited.read_to_end(&mut buf)?;
        total_read = total_read.saturating_add(u64::try_from(buf.len()).unwrap_or(u64::MAX));
        validate_expansion_budget(compressed_input_len, total_read, 1, limits)?;

        entries.push((safe_name, buf));
    }

    Ok(entries)
}

/// Packages validated `(entry_path, bytes)` entries into an `OOXML`/`ODF`/`EPUB` ZIP container,
/// enforcing `FR-SEC-001` member-path safety and placing uncompressed `mimetype` first when required.
///
/// # Errors
/// Returns [`FileFusionError`] on invalid member paths (`SecTraversal`), ZIP serialization errors,
/// or cancellation.
pub fn write_office_container_entries(
    entries: &[(String, Vec<u8>)],
    format: FormatId,
    ctx: &JobContext,
) -> FileFusionResult<Vec<u8>> {
    ctx.check_cancelled()?;
    let mut ordered: Vec<(String, &[u8])> = Vec::with_capacity(entries.len());
    for (name, payload) in entries {
        let safe = validate_archive_member_path(name)?;
        ordered.push((safe, payload.as_slice()));
    }
    if requires_uncompressed_mimetype_first(format) {
        if let Some(pos) = ordered.iter().position(|(name, _)| name == "mimetype") {
            let mime = ordered.remove(pos);
            ordered.insert(0, mime);
        }
    }

    let mut out_cursor = Cursor::new(Vec::new());
    {
        let mut writer = ZipWriter::new(&mut out_cursor);
        for (name, payload) in ordered {
            ctx.check_cancelled()?;
            let file_opts = if name == "mimetype" && requires_uncompressed_mimetype_first(format) {
                SimpleFileOptions::default().compression_method(CompressionMethod::Stored)
            } else {
                SimpleFileOptions::default()
                    .compression_method(CompressionMethod::Deflated)
                    .compression_level(Some(6))
            };
            writer.start_file(&name, file_opts).map_err(|err| {
                FileFusionError::new(
                    ErrorCode::IntInvariant,
                    format!("failed to start ZIP entry `{name}`: {err}"),
                )
            })?;
            writer.write_all(payload)?;
        }
        writer.finish().map_err(|err| {
            FileFusionError::new(
                ErrorCode::IntInvariant,
                format!("failed to finalize Office/EPUB container: {err}"),
            )
        })?;
    }
    Ok(out_cursor.into_inner())
}

fn write_optimized_office_zip(
    entries: Vec<(String, Vec<u8>)>,
    format: FormatId,
    opts: &OfficeCompressionOptions,
    ctx: &JobContext,
) -> FileFusionResult<(Vec<u8>, OfficeOptimizationStats)> {
    let cfg = CompressionProfileConfig::for_profile(opts.base.profile);
    let deflate_level = match cfg.effort_level {
        0..=3 => 4,
        4..=6 => 7,
        _ => 9,
    };

    let mut stats = OfficeOptimizationStats::default();
    let mut out_cursor = Cursor::new(Vec::new());
    {
        let mut writer = ZipWriter::new(&mut out_cursor);
        for (name, mut payload) in entries {
            ctx.check_cancelled()?;
            stats.entries_processed = stats.entries_processed.saturating_add(1);

            if opts.strip_personal_metadata && opts.base.strip_metadata {
                if let Some(scrubbed) = scrub_metadata_xml(&name, &payload) {
                    payload = scrubbed;
                    stats.metadata_entries_scrubbed =
                        stats.metadata_entries_scrubbed.saturating_add(1);
                }
            }

            if opts.recompress_images {
                if let Some(img_fmt) = detect_embedded_image_format(&name, &payload) {
                    let resize = opts.max_image_dimension.map(|d| ImageResizeSpec {
                        max_width: Some(d),
                        max_height: Some(d),
                        filter: ResizeFilter::Lanczos3,
                        preserve_aspect_ratio: true,
                    });
                    if let Ok(opt_img) = optimize_raster_in_memory(
                        &payload,
                        img_fmt,
                        img_fmt,
                        opts.base.profile,
                        opts.base.strip_metadata,
                        resize.as_ref(),
                    ) {
                        if opt_img.len() < payload.len() {
                            payload = opt_img;
                            stats.images_optimized = stats.images_optimized.saturating_add(1);
                        }
                    }
                }
            }

            if opts.minify_xml && is_optimizable_xml_entry(&name) {
                let minified = minify_xml_bytes(&payload);
                if minified.len() < payload.len() {
                    payload = minified;
                    stats.xml_entries_minified = stats.xml_entries_minified.saturating_add(1);
                }
            }

            let file_opts = if name == "mimetype" && requires_uncompressed_mimetype_first(format) {
                SimpleFileOptions::default().compression_method(CompressionMethod::Stored)
            } else {
                SimpleFileOptions::default()
                    .compression_method(CompressionMethod::Deflated)
                    .compression_level(Some(deflate_level))
            };
            writer.start_file(&name, file_opts).map_err(|err| {
                FileFusionError::new(
                    ErrorCode::IntInvariant,
                    format!("failed to start ZIP entry `{name}`: {err}"),
                )
            })?;
            writer.write_all(&payload)?;
        }
        writer.finish().map_err(|err| {
            FileFusionError::new(
                ErrorCode::IntInvariant,
                format!("failed to finalize Office/EPUB container: {err}"),
            )
        })?;
    }

    Ok((out_cursor.into_inner(), stats))
}

fn detect_embedded_image_format(name: &str, bytes: &[u8]) -> Option<FormatId> {
    let ext = Path::new(name).extension()?;
    if ext.eq_ignore_ascii_case("png") && bytes.starts_with(b"\x89PNG\r\n\x1A\n") {
        return Some(FormatId::Png);
    }
    if (ext.eq_ignore_ascii_case("jpg") || ext.eq_ignore_ascii_case("jpeg"))
        && bytes.starts_with(&[0xFF, 0xD8])
    {
        return Some(FormatId::Jpeg);
    }
    if ext.eq_ignore_ascii_case("webp") && bytes.len() >= 12 && &bytes[8..12] == b"WEBP" {
        return Some(FormatId::Webp);
    }
    None
}
