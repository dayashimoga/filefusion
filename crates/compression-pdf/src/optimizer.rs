//! Pure-Rust PDF structural, stream, embedded-image (`compression-image`), and metadata
//! privacy optimization (`FR-PDF-002`, `FR-MET-002`, `MOD-RULE-02`).

use std::io::Cursor;

use compression_core::CompressionProfileConfig;
use compression_image::{ImageResizeSpec, ResizeFilter, optimize_raster_in_memory};
use file_core::{
    CompressionProfile, CompressionRequest, ErrorCode, FileFusionError, FileFusionResult, FormatId,
    JobContext,
};
use flate2::Compression;
use flate2::write::ZlibEncoder;
use image::ColorType;
use image::codecs::jpeg::JpegEncoder;
use lopdf::{Dictionary, Document, Object, ObjectId};
use serde::{Deserialize, Serialize};
use std::io::Write;

/// Extended configuration for PDF compression (`FR-PDF-002`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[allow(clippy::struct_excessive_bools)]
pub struct PdfCompressionOptions {
    /// Shared `FileFusion` compression request contract.
    pub base: CompressionRequest,
    /// Compress uncompressed content/object streams via zlib/FlateDecode.
    pub compress_streams: bool,
    /// Remove unreferenced orphan objects and zero-length streams before renumbering.
    pub prune_unused_objects: bool,
    /// Recompress embedded raster `/Image` `XObjects` using `compression-image`.
    pub recompress_images: bool,
    /// Optional maximum pixel edge length (`width` or `height`) for oversized PDF images.
    pub max_image_dimension: Option<u32>,
    /// Allow Tier 2 `qpdf` post-pass linearization (`--linearize`) when configured natively.
    pub linearize_with_qpdf: bool,
}

impl PdfCompressionOptions {
    /// Derives default PDF optimization flags from a [`CompressionRequest`].
    #[must_use]
    pub fn from_request(base: CompressionRequest) -> Self {
        let max_image_dimension = match base.profile {
            CompressionProfile::ExtremeSizeTarget => Some(1280),
            CompressionProfile::WebSharingReady => Some(1920),
            _ => None,
        };
        Self {
            base,
            compress_streams: true,
            prune_unused_objects: true,
            recompress_images: true,
            max_image_dimension,
            linearize_with_qpdf: true,
        }
    }
}

/// Metrics returned by pure-Rust PDF optimization.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct PdfCompressionReport {
    /// Original input byte length.
    pub input_bytes: u64,
    /// Serialized output byte length.
    pub output_bytes: u64,
    /// Number of document pages detected.
    pub page_count: u32,
    /// Number of embedded `/Image` streams recompressed or downsampled.
    pub images_recompressed: u32,
    /// Number of orphaned/unreferenced objects pruned.
    pub objects_pruned: u32,
    /// Number of non-image streams compressed tighter via Flate/zlib.
    pub streams_recompressed: u32,
    /// Whether `/Info` PII keys or `/Metadata` `XMP` streams were scrubbed.
    pub metadata_scrubbed: bool,
}

/// Optimizes an unencrypted PDF payload purely in memory using `lopdf`, `flate2`, and
/// `compression-image` (`FR-PDF-002`, `FR-MET-002`).
///
/// # Errors
/// Returns [`FileFusionError`] on corrupt PDF structure, encrypted PDFs (`SecEncrypted`),
/// invalid dimensions, or job cancellation.
pub fn optimize_pdf_bytes(
    raw_pdf: &[u8],
    opts: &PdfCompressionOptions,
    ctx: &JobContext,
) -> FileFusionResult<(Vec<u8>, PdfCompressionReport)> {
    ctx.check_cancelled()?;
    if raw_pdf.is_empty() {
        return Err(FileFusionError::new(
            ErrorCode::FmtCorrupt,
            "PDF input payload is empty",
        ));
    }

    let mut doc = Document::load_mem(raw_pdf).map_err(|err| {
        if matches!(err, lopdf::Error::Decryption(_))
            || raw_pdf.windows(8).any(|w| w == b"/Encrypt")
        {
            FileFusionError::new(
                ErrorCode::FmtDrm,
                format!("PDF is encrypted or password-protected: {err}"),
            )
        } else {
            FileFusionError::new(
                ErrorCode::FmtCorrupt,
                format!("failed to parse PDF structure: {err}"),
            )
        }
    })?;

    if doc.is_encrypted() {
        return Err(FileFusionError::new(
            ErrorCode::FmtDrm,
            "PDF is encrypted with a password or security handler; decrypt before compression",
        ));
    }
    ctx.check_cancelled()?;

    let page_count = u32::try_from(doc.get_pages().len()).unwrap_or(u32::MAX);
    let cfg = CompressionProfileConfig::for_profile(opts.base.profile);

    let mut report = PdfCompressionReport {
        input_bytes: u64::try_from(raw_pdf.len()).unwrap_or(u64::MAX),
        page_count,
        ..PdfCompressionReport::default()
    };

    if opts.base.strip_metadata {
        report.metadata_scrubbed = scrub_pdf_privacy_metadata(&mut doc);
    }
    ctx.check_cancelled()?;

    if opts.recompress_images {
        report.images_recompressed = optimize_embedded_images(&mut doc, opts, cfg, ctx)?;
    }
    ctx.check_cancelled()?;

    if opts.compress_streams {
        report.streams_recompressed = recompress_flate_streams(&mut doc, cfg, ctx)?;
        doc.compress();
    }

    if opts.prune_unused_objects {
        let deleted_zero = doc.delete_zero_length_streams();
        let pruned = doc.prune_objects();
        doc.renumber_objects();
        report.objects_pruned =
            u32::try_from(deleted_zero.len().saturating_add(pruned.len())).unwrap_or(u32::MAX);
    }
    ctx.check_cancelled()?;

    let mut out_buf = Vec::new();
    doc.save_to(&mut out_buf).map_err(|err| {
        FileFusionError::new(
            ErrorCode::IntInvariant,
            format!("failed to serialize compressed PDF: {err}"),
        )
    })?;
    report.output_bytes = u64::try_from(out_buf.len()).unwrap_or(u64::MAX);

    Ok((out_buf, report))
}

/// Applies privacy scrubbing and/or explicit `/Info` metadata edits to an unencrypted PDF payload
/// cleanly (`FR-MET-001`..`002`).
///
/// # Errors
/// Returns [`FileFusionError`] on empty, corrupt, or encrypted PDFs, or job cancellation.
#[allow(clippy::too_many_arguments)]
pub fn update_pdf_metadata_bytes(
    raw_pdf: &[u8],
    scrub_privacy: bool,
    title: Option<Option<&str>>,
    author: Option<Option<&str>>,
    subject: Option<Option<&str>>,
    keywords: Option<Option<&str>>,
    creator_tool: Option<Option<&str>>,
    ctx: &JobContext,
) -> FileFusionResult<(Vec<u8>, bool)> {
    ctx.check_cancelled()?;
    let mut doc = crate::studio::load_unencrypted_pdf(raw_pdf)?;
    let mut changed = false;
    if scrub_privacy {
        changed |= scrub_pdf_privacy_metadata(&mut doc);
    }

    let updates = [
        ("Title", title),
        ("Author", author),
        ("Subject", subject),
        ("Keywords", keywords),
        ("Creator", creator_tool),
    ];
    let has_updates = updates.iter().any(|(_, v)| v.is_some());
    if has_updates {
        let info_id = if let Ok(id) = doc.trailer.get(b"Info").and_then(Object::as_reference) {
            id
        } else {
            let id = doc.add_object(Dictionary::new());
            doc.trailer.set("Info", Object::Reference(id));
            id
        };
        if let Ok(info_dict) = doc.get_dictionary_mut(info_id) {
            for (key, val_opt) in updates {
                match val_opt {
                    Some(Some(val)) => {
                        info_dict.set(key, Object::string_literal(val));
                        changed = true;
                    }
                    Some(None) if info_dict.remove(key.as_bytes()).is_some() => {
                        changed = true;
                    }
                    _ => {}
                }
            }
        }
    }

    let out = crate::studio::serialize_pdf(&mut doc)?;
    Ok((out, changed))
}

fn scrub_pdf_privacy_metadata(doc: &mut Document) -> bool {
    let mut scrubbed = false;
    if let Ok(Object::Reference(info_id)) = doc.trailer.get(b"Info").cloned() {
        if let Ok(info_dict) = doc.get_dictionary_mut(info_id) {
            for key in [
                b"Title".as_slice(),
                b"Author".as_slice(),
                b"Subject".as_slice(),
                b"Keywords".as_slice(),
                b"Creator".as_slice(),
                b"Producer".as_slice(),
                b"CreationDate".as_slice(),
                b"ModDate".as_slice(),
            ] {
                if info_dict.remove(key).is_some() {
                    scrubbed = true;
                }
            }
        }
    } else if doc.trailer.remove(b"Info").is_some() {
        scrubbed = true;
    }

    if let Ok(catalog) = doc.catalog_mut() {
        if catalog.remove(b"Metadata").is_some() {
            scrubbed = true;
        }
    }
    scrubbed
}

fn optimize_embedded_images(
    doc: &mut Document,
    opts: &PdfCompressionOptions,
    cfg: CompressionProfileConfig,
    ctx: &JobContext,
) -> FileFusionResult<u32> {
    let obj_ids: Vec<ObjectId> = doc.objects.keys().copied().collect();
    let mut count = 0u32;

    for id in obj_ids {
        ctx.check_cancelled()?;
        let Some(Object::Stream(stream)) = doc.objects.get_mut(&id) else {
            continue;
        };
        if !dict_has_name(&stream.dict, b"Subtype", b"Image") {
            continue;
        }

        let resize_spec = opts.max_image_dimension.map(|dim| ImageResizeSpec {
            max_width: Some(dim),
            max_height: Some(dim),
            filter: ResizeFilter::Lanczos3,
            preserve_aspect_ratio: true,
        });

        let filter_name = extract_primary_filter(&stream.dict);
        if filter_name.as_deref() == Some(b"DCTDecode") {
            let Ok(recompressed) = optimize_raster_in_memory(
                &stream.content,
                FormatId::Jpeg,
                FormatId::Jpeg,
                opts.base.profile,
                opts.base.strip_metadata,
                resize_spec.as_ref(),
            ) else {
                continue;
            };
            if recompressed.len() < stream.content.len() {
                if let Ok((w, h)) = probe_jpeg_dimensions(&recompressed) {
                    stream.dict.set("Width", i64::from(w));
                    stream.dict.set("Height", i64::from(h));
                }
                stream.content = recompressed;
                stream
                    .dict
                    .set("Length", i64::try_from(stream.content.len()).unwrap_or(0));
                count = count.saturating_add(1);
            }
            continue;
        }

        // Handle raw uncompressed RGB/Gray pixel streams (`/Filter` absent, `/BitsPerComponent 8`).
        if filter_name.is_none()
            && !cfg.lossless_preferred
            && !stream.dict.has(b"SMask")
            && !stream.dict.has(b"Mask")
        {
            if let Some(jpeg_bytes) = try_encode_raw_pixel_image_as_jpeg(stream, cfg) {
                if jpeg_bytes.len() < stream.content.len() {
                    stream.content = jpeg_bytes;
                    stream
                        .dict
                        .set("Filter", Object::Name(b"DCTDecode".to_vec()));
                    stream
                        .dict
                        .set("ColorSpace", Object::Name(b"DeviceRGB".to_vec()));
                    stream.dict.set("BitsPerComponent", 8);
                    stream
                        .dict
                        .set("Length", i64::try_from(stream.content.len()).unwrap_or(0));
                    count = count.saturating_add(1);
                }
            }
        }
    }

    Ok(count)
}

fn try_encode_raw_pixel_image_as_jpeg(
    stream: &lopdf::Stream,
    cfg: CompressionProfileConfig,
) -> Option<Vec<u8>> {
    let w = stream
        .dict
        .get(b"Width")
        .ok()
        .and_then(|o| o.as_i64().ok())
        .and_then(|v| u32::try_from(v).ok())?;
    let h = stream
        .dict
        .get(b"Height")
        .ok()
        .and_then(|o| o.as_i64().ok())
        .and_then(|v| u32::try_from(v).ok())?;
    let bpc = stream
        .dict
        .get(b"BitsPerComponent")
        .ok()
        .and_then(|o| o.as_i64().ok())
        .unwrap_or(8);
    if w < 16 || h < 16 || bpc != 8 {
        return None;
    }

    let is_rgb = dict_has_name(&stream.dict, b"ColorSpace", b"DeviceRGB");
    let is_gray = dict_has_name(&stream.dict, b"ColorSpace", b"DeviceGray");
    let expected_rgb = usize::try_from(w)
        .ok()?
        .saturating_mul(usize::try_from(h).ok()?)
        .saturating_mul(3);
    let expected_gray = usize::try_from(w)
        .ok()?
        .saturating_mul(usize::try_from(h).ok()?);

    let quality = cfg.lossy_quality_q.clamp(45, 92);
    let mut out = Vec::new();
    if is_rgb && stream.content.len() == expected_rgb {
        let mut enc = JpegEncoder::new_with_quality(&mut out, quality);
        enc.encode(&stream.content, w, h, ColorType::Rgb8.into())
            .ok()?;
        return Some(out);
    }
    if is_gray && stream.content.len() == expected_gray {
        let mut rgb = Vec::with_capacity(expected_rgb);
        for &g in &stream.content {
            rgb.extend_from_slice(&[g, g, g]);
        }
        let mut enc = JpegEncoder::new_with_quality(&mut out, quality);
        enc.encode(&rgb, w, h, ColorType::Rgb8.into()).ok()?;
        return Some(out);
    }
    None
}

fn recompress_flate_streams(
    doc: &mut Document,
    cfg: CompressionProfileConfig,
    ctx: &JobContext,
) -> FileFusionResult<u32> {
    let zlib_level = match cfg.effort_level {
        0..=3 => Compression::fast(),
        4..=7 => Compression::default(),
        _ => Compression::best(),
    };
    let obj_ids: Vec<ObjectId> = doc.objects.keys().copied().collect();
    let mut updated = 0u32;

    for id in obj_ids {
        ctx.check_cancelled()?;
        let Some(Object::Stream(stream)) = doc.objects.get_mut(&id) else {
            continue;
        };
        if dict_has_name(&stream.dict, b"Subtype", b"Image") {
            continue;
        }
        let filter = extract_primary_filter(&stream.dict);
        let raw_plain = match filter.as_deref() {
            None => stream.content.clone(),
            Some(b"FlateDecode") => match stream.decompressed_content() {
                Ok(plain) => plain,
                Err(_) => continue,
            },
            Some(_) => continue,
        };
        if raw_plain.len() < 32 {
            continue;
        }

        let mut enc = ZlibEncoder::new(Vec::new(), zlib_level);
        if enc.write_all(&raw_plain).is_err() {
            continue;
        }
        let Ok(compressed) = enc.finish() else {
            continue;
        };
        if compressed.len() < stream.content.len() {
            stream.content = compressed;
            stream
                .dict
                .set("Filter", Object::Name(b"FlateDecode".to_vec()));
            stream
                .dict
                .set("Length", i64::try_from(stream.content.len()).unwrap_or(0));
            updated = updated.saturating_add(1);
        }
    }
    Ok(updated)
}

fn probe_jpeg_dimensions(bytes: &[u8]) -> FileFusionResult<(u32, u32)> {
    let reader = image::ImageReader::with_format(Cursor::new(bytes), image::ImageFormat::Jpeg);
    reader.into_dimensions().map_err(|err| {
        FileFusionError::new(
            ErrorCode::FmtCorrupt,
            format!("invalid JPEG image dimensions: {err}"),
        )
    })
}

fn dict_has_name(dict: &Dictionary, key: &[u8], expected: &[u8]) -> bool {
    dict.get(key)
        .ok()
        .and_then(|obj| obj.as_name().ok())
        .is_some_and(|name| name == expected)
}

fn extract_primary_filter(dict: &Dictionary) -> Option<Vec<u8>> {
    match dict.get(b"Filter").ok()? {
        Object::Name(n) => Some(n.clone()),
        Object::Array(arr) if arr.len() == 1 => arr.first()?.as_name().ok().map(<[u8]>::to_vec),
        _ => None,
    }
}
