//! Stage 4 Local OCR (`TesseractAdapter` via [`IsolatedProcessRunner`], TSV/`hOCR` parsing,
//! raster `XObject` extraction, and invisible `3 Tr` searchable PDF text overlay generation)
//! (`FR-PDF-003`, `MOD-RULE-04`).

use std::collections::HashMap;
use std::fmt::Write as _;
use std::io::Cursor;

use file_core::{ErrorCode, FileFusionError, FileFusionResult, JobContext};
use image::{DynamicImage, GenericImageView, ImageFormat, RgbImage};
use lopdf::{Dictionary, Document, Object, ObjectId, Stream, dictionary};
use serde::{Deserialize, Serialize};

#[cfg(feature = "native-process")]
use std::collections::BTreeMap;
#[cfg(feature = "native-process")]
use std::ffi::OsString;
#[cfg(feature = "native-process")]
use std::fs;
#[cfg(feature = "native-process")]
use std::path::{Path, PathBuf};

#[cfg(feature = "native-process")]
use file_core::{
    ExternalToolAdapter, IsolatedProcessRunner, ProcessInvocation, ProcessOutput,
    ProcessingOperation, ProcessingRequest, StagedPath, ValidatedSource,
};

use crate::studio::{
    append_content_stream_to_page, attach_page_resources, escape_pdf_literal, load_unencrypted_pdf,
    materialize_inherited_page_attrs, resolve_page_box, serialize_pdf,
};

/// Output artifact mode requested from `TesseractAdapter` (`FR-PDF-003`).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TesseractOutputMode {
    /// Tab-separated word bounding boxes and per-word confidences (default for PDF overlay).
    #[default]
    Tsv,
    /// Plain recognized UTF-8 text.
    PlainText,
    /// Standard `hOCR` HTML bounding-box markup.
    Hocr,
    /// Native Tesseract searchable PDF artifact.
    SearchablePdf,
}

/// OCR execution configuration passed to [`TesseractAdapter`] (`FR-PDF-003`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OcrOptions {
    /// Tesseract traineddata language specification (e.g. `"eng"` or `"eng+deu"`).
    pub language: String,
    /// Page segmentation mode (`0..=13`, default `3` = fully automatic).
    pub page_seg_mode: u8,
    /// Optional explicit raster DPI hint (`70..=1200`).
    pub dpi: Option<u32>,
    /// Output format produced by Tesseract (`Tsv` default).
    pub output_mode: TesseractOutputMode,
}

impl Default for OcrOptions {
    fn default() -> Self {
        Self {
            language: String::from("eng"),
            page_seg_mode: 3,
            dpi: None,
            output_mode: TesseractOutputMode::Tsv,
        }
    }
}

/// Single word bounding box in top-down raster pixel coordinates (`FR-PDF-003`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OcrWordBox {
    /// Recognized word text.
    pub text: String,
    /// Left pixel coordinate (`0` = image left edge).
    pub left_px: u32,
    /// Top pixel coordinate (`0` = image top edge).
    pub top_px: u32,
    /// Word bounding box width in pixels.
    pub width_px: u32,
    /// Word bounding box height in pixels.
    pub height_px: u32,
    /// Confidence percentage (`0.0..=100.0`).
    pub confidence: f32,
}

/// OCR result for one page/image aligned for invisible searchable PDF overlay injection.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OcrPageResult {
    /// 1-based page index.
    pub page_number: u32,
    /// Source raster image width in pixels.
    pub page_width_px: u32,
    /// Source raster image height in pixels.
    pub page_height_px: u32,
    /// Word-level bounding boxes ordered reading top-to-bottom.
    pub words: Vec<OcrWordBox>,
    /// Reconstructed full page text.
    pub plain_text: String,
    /// Arithmetic mean confidence across recognized words (`0.0..=100.0`).
    pub mean_confidence: f32,
}

/// Multi-page OCR document bundle (`FR-PDF-003`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OcrDocumentResult {
    /// Language profile used during recognition.
    pub language: String,
    /// Per-page recognition results.
    pub pages: Vec<OcrPageResult>,
    /// Concatenated full document text joined by form feeds / newlines.
    pub full_text: String,
}

/// Embedded raster `/Image` `XObject` normalized as PNG bytes for local OCR ingestion.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExtractedPdfImage {
    /// 1-based page number where the `/Image` appeared.
    pub page_number: u32,
    /// Resource dictionary key name (e.g. `"Im0"`).
    pub xobject_name: String,
    /// Image pixel width.
    pub width_px: u32,
    /// Image pixel height.
    pub height_px: u32,
    /// Encoded `PNG` bytes ready for `TesseractAdapter` or raster inspection.
    pub png_bytes: Vec<u8>,
}

/// Parses Tesseract TSV output (`level page_num block_num par_num line_num word_num left top width
/// height conf text`) into an [`OcrPageResult`] (`FR-PDF-003`).
///
/// # Errors
/// Returns [`FileFusionError`] on zero page number or dimensions.
pub fn parse_tesseract_tsv(
    page_number: u32,
    page_width_px: u32,
    page_height_px: u32,
    tsv: &str,
) -> FileFusionResult<OcrPageResult> {
    validate_page_geometry(page_number, page_width_px, page_height_px)?;

    let mut words = Vec::new();
    let mut plain_text = String::new();
    let mut prev_line_key: Option<(u32, u32, u32)> = None;
    let mut conf_sum = 0.0_f64;
    let mut conf_count = 0_u32;

    for raw_line in tsv.lines() {
        let line = raw_line.trim_end_matches('\r');
        if line.is_empty() || line.starts_with("level\t") {
            continue;
        }
        let cols: Vec<&str> = line.splitn(12, '\t').collect();
        if cols.len() < 12 {
            continue;
        }
        let text = cols[11].trim();
        if text.is_empty() {
            continue;
        }

        let block = cols[2].trim().parse::<u32>().unwrap_or(0);
        let par = cols[3].trim().parse::<u32>().unwrap_or(0);
        let line_no = cols[4].trim().parse::<u32>().unwrap_or(0);
        let left_px = cols[6].trim().parse::<u32>().unwrap_or(0);
        let top_px = cols[7].trim().parse::<u32>().unwrap_or(0);
        let width_px = cols[8].trim().parse::<u32>().unwrap_or(1).max(1);
        let height_px = cols[9].trim().parse::<u32>().unwrap_or(12).max(1);
        let raw_conf = cols[10].trim().parse::<f32>().unwrap_or(-1.0);
        let confidence = if raw_conf.is_finite() && raw_conf >= 0.0 {
            raw_conf.clamp(0.0, 100.0)
        } else {
            0.0
        };
        if raw_conf.is_finite() && raw_conf >= 0.0 {
            conf_sum += f64::from(confidence);
            conf_count = conf_count.saturating_add(1);
        }

        let line_key = (block, par, line_no);
        if !plain_text.is_empty() {
            if prev_line_key == Some(line_key) {
                plain_text.push(' ');
            } else {
                plain_text.push('\n');
            }
        }
        plain_text.push_str(text);
        prev_line_key = Some(line_key);

        words.push(OcrWordBox {
            text: text.to_owned(),
            left_px,
            top_px,
            width_px,
            height_px,
            confidence,
        });
    }

    #[allow(clippy::cast_possible_truncation)]
    let mean_confidence = if conf_count > 0 {
        (conf_sum / f64::from(conf_count)) as f32
    } else {
        0.0
    };

    Ok(OcrPageResult {
        page_number,
        page_width_px,
        page_height_px,
        words,
        plain_text,
        mean_confidence,
    })
}

/// Parses standard Tesseract `hOCR` (`ocrx_word` spans with `bbox x0 y0 x1 y1`) into an
/// [`OcrPageResult`] (`FR-PDF-003`).
///
/// # Errors
/// Returns [`FileFusionError`] on invalid page number or dimensions.
pub fn parse_tesseract_hocr(
    page_number: u32,
    page_width_px: u32,
    page_height_px: u32,
    hocr: &str,
) -> FileFusionResult<OcrPageResult> {
    validate_page_geometry(page_number, page_width_px, page_height_px)?;

    let mut words = Vec::new();
    let mut plain_words = Vec::new();
    let mut conf_sum = 0.0_f64;
    let mut conf_count = 0_u32;
    let mut rest = hocr;

    while let Some(span_start) = rest.find("ocrx_word") {
        let after_class = &rest[span_start..];
        let Some(tag_end) = after_class.find('>') else {
            break;
        };
        let attrs = &after_class[..tag_end];
        let after_open = &after_class[tag_end + 1..];
        let Some(close_idx) = after_open.find("</span>") else {
            break;
        };
        let raw_inner = strip_inline_tags(&after_open[..close_idx]);
        let text = unescape_html_entities(raw_inner.trim());
        rest = &after_open[close_idx + 7..];

        if text.is_empty() {
            continue;
        }

        let (left_px, top_px, width_px, height_px) =
            parse_hocr_bbox(attrs).unwrap_or((0, 0, 10, 12));
        let conf = parse_hocr_conf(attrs);
        if let Some(c) = conf {
            conf_sum += f64::from(c);
            conf_count = conf_count.saturating_add(1);
        }

        plain_words.push(text.clone());
        words.push(OcrWordBox {
            text,
            left_px,
            top_px,
            width_px,
            height_px,
            confidence: conf.unwrap_or(0.0),
        });
    }

    #[allow(clippy::cast_possible_truncation)]
    let mean_confidence = if conf_count > 0 {
        (conf_sum / f64::from(conf_count)) as f32
    } else {
        0.0
    };

    Ok(OcrPageResult {
        page_number,
        page_width_px,
        page_height_px,
        words,
        plain_text: plain_words.join(" "),
        mean_confidence,
    })
}

/// Extracts raster `/Image` `XObjects` per page from an unencrypted PDF normalized as PNG bytes so
/// local OCR engines (`TesseractAdapter`) can inspect scanned pages directly (`FR-PDF-003`).
///
/// # Errors
/// Returns [`FileFusionError`] on empty, corrupt, or encrypted PDFs, or job cancellation.
pub fn extract_pdf_page_images(
    raw_pdf: &[u8],
    ctx: &JobContext,
) -> FileFusionResult<Vec<ExtractedPdfImage>> {
    ctx.check_cancelled()?;
    let doc = load_unencrypted_pdf(raw_pdf)?;
    let pages = doc.get_pages();
    let mut extracted = Vec::new();

    for (page_number, page_id) in pages {
        ctx.check_cancelled()?;
        let xobjects = collect_page_xobjects(&doc, page_id);
        for (name, ref_id) in xobjects {
            let Ok(Object::Stream(st)) = doc.get_object(ref_id) else {
                continue;
            };
            if !st
                .dict
                .get(b"Subtype")
                .and_then(Object::as_name)
                .is_ok_and(|n| n == b"Image")
            {
                continue;
            }
            if let Some(img) = decode_pdf_image_to_png(page_number, &name, st) {
                extracted.push(img);
            }
        }
    }

    Ok(extracted)
}

/// Injects an invisible (`3 Tr`) searchable PDF text overlay onto each matching page in `raw_pdf`
/// using word bounding boxes from [`OcrPageResult`] (`FR-PDF-003`).
///
/// # Errors
/// Returns [`FileFusionError`] on corrupt/encrypted PDFs, out-of-range page numbers, invalid OCR
/// geometry, or job cancellation.
pub fn embed_searchable_ocr_text_layer(
    raw_pdf: &[u8],
    ocr_pages: &[OcrPageResult],
    ctx: &JobContext,
) -> FileFusionResult<Vec<u8>> {
    ctx.check_cancelled()?;
    let mut doc = load_unencrypted_pdf(raw_pdf)?;
    let page_map = doc.get_pages();

    let font_id = doc.add_object(dictionary! {
        "Type" => "Font",
        "Subtype" => "Type1",
        "BaseFont" => "Helvetica",
    });

    for ocr_page in ocr_pages {
        ctx.check_cancelled()?;
        validate_page_geometry(
            ocr_page.page_number,
            ocr_page.page_width_px,
            ocr_page.page_height_px,
        )?;
        let Some(&page_id) = page_map.get(&ocr_page.page_number) else {
            return Err(FileFusionError::new(
                ErrorCode::FmtCorrupt,
                format!(
                    "OCR page {} exceeds document page count ({})",
                    ocr_page.page_number,
                    page_map.len()
                ),
            ));
        };
        materialize_inherited_page_attrs(&mut doc, page_id);
        let page_box = resolve_page_box(&doc, page_id);
        attach_page_resources(&mut doc, page_id, "F_FF_OCR", font_id, None);
        let overlay = build_ocr_overlay_stream(page_box, ocr_page);
        let overlay_id = doc.add_object(overlay);
        append_content_stream_to_page(&mut doc, page_id, overlay_id);
    }

    serialize_pdf(&mut doc)
}

/// Builds a single-page searchable PDF containing the raster image background plus an invisible
/// (`3 Tr`) OCR word overlay (`FR-PDF-003`).
///
/// # Errors
/// Returns [`FileFusionError`] on unreadable image bytes, invalid OCR geometry, or cancellation.
pub fn build_searchable_pdf_from_image_and_ocr(
    image_bytes: &[u8],
    ocr_page: &OcrPageResult,
    ctx: &JobContext,
) -> FileFusionResult<Vec<u8>> {
    ctx.check_cancelled()?;
    validate_page_geometry(
        ocr_page.page_number,
        ocr_page.page_width_px,
        ocr_page.page_height_px,
    )?;

    let dyn_img = image::load_from_memory(image_bytes).map_err(|err| {
        FileFusionError::new(
            ErrorCode::FmtCorrupt,
            format!("failed to decode OCR raster image: {err}"),
        )
    })?;
    let rgb = dyn_img.to_rgb8();
    let (w, h) = rgb.dimensions();
    let w_pt = f32::from(u16::try_from(w).unwrap_or(u16::MAX)) * (72.0 / 150.0);
    let h_pt = f32::from(u16::try_from(h).unwrap_or(u16::MAX)) * (72.0 / 150.0);
    let page_w = w_pt.clamp(72.0, 2400.0);
    let page_h = h_pt.clamp(72.0, 2400.0);

    let mut doc = Document::with_version("1.5");
    let root_pages_id = doc.new_object_id();
    let font_id = doc.add_object(dictionary! {
        "Type" => "Font",
        "Subtype" => "Type1",
        "BaseFont" => "Helvetica",
    });
    let img_id = doc.add_object(Stream::new(
        dictionary! {
            "Type" => "XObject",
            "Subtype" => "Image",
            "Width" => i64::from(w),
            "Height" => i64::from(h),
            "ColorSpace" => "DeviceRGB",
            "BitsPerComponent" => 8,
        },
        rgb.into_raw(),
    ));
    let draw_ops = format!("q\n{page_w:.2} 0 0 {page_h:.2} 0 0 cm\n/Im0 Do\nQ\n");
    let draw_id = doc.add_object(Stream::new(dictionary! {}, draw_ops.into_bytes()));
    let overlay_id = doc.add_object(build_ocr_overlay_stream(
        [0.0, 0.0, page_w, page_h],
        ocr_page,
    ));
    let res_id = doc.add_object(dictionary! {
        "XObject" => dictionary! { "Im0" => img_id },
        "Font" => dictionary! { "F_FF_OCR" => font_id },
    });
    let page_id = doc.add_object(dictionary! {
        "Type" => "Page",
        "Parent" => root_pages_id,
        "MediaBox" => vec![0.into(), 0.into(), Object::Real(page_w), Object::Real(page_h)],
        "Resources" => res_id,
        "Contents" => vec![Object::Reference(draw_id), Object::Reference(overlay_id)],
    });
    doc.objects.insert(
        root_pages_id,
        Object::Dictionary(dictionary! {
            "Type" => "Pages",
            "Kids" => vec![Object::Reference(page_id)],
            "Count" => 1,
        }),
    );
    let catalog_id = doc.add_object(dictionary! {
        "Type" => "Catalog",
        "Pages" => root_pages_id,
    });
    doc.trailer.set("Root", catalog_id);
    serialize_pdf(&mut doc)
}

/// Tier 2 local `Tesseract` OCR adapter (`Apache-2.0` isolated subprocess, `FR-PDF-003`).
#[cfg(feature = "native-process")]
#[derive(Debug, Clone)]
pub struct TesseractAdapter {
    binary_path: PathBuf,
}

#[cfg(feature = "native-process")]
impl TesseractAdapter {
    /// Creates a `TesseractAdapter` bound to a verified `tesseract` executable path.
    #[must_use]
    pub const fn new(binary_path: PathBuf) -> Self {
        Self { binary_path }
    }

    /// Builds a sanitized `tesseract` invocation (`MOD-RULE-04`, zero shell interpolation).
    ///
    /// # Errors
    /// Returns [`FileFusionError`] if `opts.language` contains invalid characters or `page_seg_mode > 13`.
    pub fn build_ocr_invocation(
        &self,
        input_image_path: &Path,
        output_base_or_stdout: Option<&Path>,
        opts: &OcrOptions,
        timeout_secs: u64,
    ) -> FileFusionResult<ProcessInvocation> {
        validate_ocr_options(opts)?;
        let mut args = vec![
            input_image_path.as_os_str().to_os_string(),
            output_base_or_stdout.map_or_else(
                || OsString::from("stdout"),
                |p| p.as_os_str().to_os_string(),
            ),
            OsString::from("-l"),
            OsString::from(&opts.language),
            OsString::from("--psm"),
            OsString::from(opts.page_seg_mode.to_string()),
        ];
        if let Some(dpi) = opts.dpi {
            args.push(OsString::from("--dpi"));
            args.push(OsString::from(dpi.clamp(70, 1200).to_string()));
        }
        args.push(OsString::from(match opts.output_mode {
            TesseractOutputMode::Tsv => "tsv",
            TesseractOutputMode::PlainText => "txt",
            TesseractOutputMode::Hocr => "hocr",
            TesseractOutputMode::SearchablePdf => "pdf",
        }));

        Ok(ProcessInvocation {
            executable: self.binary_path.clone(),
            args,
            working_dir: input_image_path.parent().map(Path::to_path_buf),
            timeout_secs: timeout_secs.max(30),
            allow_env_keys: vec![String::from("TESSDATA_PREFIX"), String::from("TMPDIR")],
        })
    }

    /// Executes local OCR via [`IsolatedProcessRunner`] on an encoded raster image buffer
    /// (`PNG`/`JPEG`/`TIFF`/`BMP`), returning structured word boxes and plain text (`FR-PDF-003`).
    ///
    /// # Errors
    /// Returns [`FileFusionError`] on invalid image payload, subprocess error, or cancellation.
    pub fn ocr_image_bytes(
        &self,
        image_bytes: &[u8],
        page_number: u32,
        opts: &OcrOptions,
        runner: &dyn IsolatedProcessRunner,
        ctx: &JobContext,
    ) -> FileFusionResult<OcrPageResult> {
        ctx.check_cancelled()?;
        validate_ocr_options(opts)?;
        let img = image::load_from_memory(image_bytes).map_err(|err| {
            FileFusionError::new(
                ErrorCode::FmtCorrupt,
                format!("failed to decode raster image before OCR: {err}"),
            )
        })?;
        let (width_px, height_px) = img.dimensions();

        let staged_input = StagedPath::create_sibling(
            std::env::temp_dir().join(format!("ff_ocr_p{page_number}.png")),
        )?;
        let mut cursor = Cursor::new(Vec::new());
        img.write_to(&mut cursor, ImageFormat::Png).map_err(|err| {
            FileFusionError::new(
                ErrorCode::IntInvariant,
                format!("failed to stage PNG input for Tesseract: {err}"),
            )
        })?;
        fs::write(staged_input.staged_path(), cursor.into_inner())?;

        let inv = self.build_ocr_invocation(
            staged_input.staged_path(),
            None,
            opts,
            ctx.limits.subprocess_timeout_secs,
        )?;
        let out = runner.run_isolated(&inv, ctx)?;
        if out.timed_out || out.exit_code != 0 {
            return Err(FileFusionError::new(
                ErrorCode::IntInvariant,
                format!(
                    "Tesseract OCR failed (exit={}, timed_out={}): {}",
                    out.exit_code,
                    out.timed_out,
                    String::from_utf8_lossy(&out.stderr).trim()
                ),
            ));
        }

        let text = String::from_utf8_lossy(&out.stdout);
        match opts.output_mode {
            TesseractOutputMode::Tsv => {
                parse_tesseract_tsv(page_number, width_px, height_px, &text)
            }
            TesseractOutputMode::Hocr => {
                parse_tesseract_hocr(page_number, width_px, height_px, &text)
            }
            TesseractOutputMode::PlainText | TesseractOutputMode::SearchablePdf => {
                Ok(OcrPageResult {
                    page_number,
                    page_width_px: width_px,
                    page_height_px: height_px,
                    words: Vec::new(),
                    plain_text: text.trim().to_owned(),
                    mean_confidence: 0.0,
                })
            }
        }
    }

    /// Extracts embedded scanned raster page images from `raw_pdf`, runs [`TesseractAdapter`] per
    /// page via `runner`, and embeds invisible (`3 Tr`) searchable text layers back into the PDF.
    ///
    /// # Errors
    /// Returns [`FileFusionError`] when no raster images exist (`FmtUnsupported`), on subprocess
    /// failure, or on cancellation.
    pub fn make_pdf_searchable_with_runner(
        &self,
        raw_pdf: &[u8],
        opts: &OcrOptions,
        runner: &dyn IsolatedProcessRunner,
        ctx: &JobContext,
    ) -> FileFusionResult<(Vec<u8>, OcrDocumentResult)> {
        ctx.check_cancelled()?;
        let extracted = extract_pdf_page_images(raw_pdf, ctx)?;
        if extracted.is_empty() {
            return Err(FileFusionError::new(
                ErrorCode::FmtUnsupported,
                "PDF contains no extractable raster /Image XObjects for local OCR",
            ));
        }

        let mut per_page: BTreeMap<u32, OcrPageResult> = BTreeMap::new();
        for img in extracted {
            ctx.check_cancelled()?;
            let page_res =
                self.ocr_image_bytes(&img.png_bytes, img.page_number, opts, runner, ctx)?;
            per_page
                .entry(img.page_number)
                .and_modify(|existing| {
                    if !existing.plain_text.is_empty() && !page_res.plain_text.is_empty() {
                        existing.plain_text.push('\n');
                    }
                    existing.plain_text.push_str(&page_res.plain_text);
                    existing.words.extend(page_res.words.iter().cloned());
                })
                .or_insert(page_res);
        }

        let pages: Vec<OcrPageResult> = per_page.into_values().collect();
        let searchable_pdf = embed_searchable_ocr_text_layer(raw_pdf, &pages, ctx)?;
        let full_text = pages
            .iter()
            .map(|p| p.plain_text.as_str())
            .filter(|s| !s.is_empty())
            .collect::<Vec<_>>()
            .join("\n\n");

        Ok((
            searchable_pdf,
            OcrDocumentResult {
                language: opts.language.clone(),
                pages,
                full_text,
            },
        ))
    }
}

#[cfg(feature = "native-process")]
impl ExternalToolAdapter for TesseractAdapter {
    fn adapter_id(&self) -> &'static str {
        "adapter-tesseract"
    }

    fn capability_id(&self) -> &'static str {
        "tier2-tesseract"
    }

    fn spdx_license(&self) -> &'static str {
        "Apache-2.0"
    }

    fn build_args(
        &self,
        input: &ValidatedSource,
        staged_output: &StagedPath,
        req: &ProcessingRequest,
    ) -> FileFusionResult<Vec<OsString>> {
        let mode = match req.operation {
            ProcessingOperation::Convert {
                target_format: file_core::FormatId::Pdf,
                ..
            } => TesseractOutputMode::SearchablePdf,
            _ => TesseractOutputMode::Tsv,
        };
        let opts = OcrOptions {
            output_mode: mode,
            ..OcrOptions::default()
        };
        let base = if mode == TesseractOutputMode::SearchablePdf {
            Some(staged_output.staged_path())
        } else {
            None
        };
        Ok(self
            .build_ocr_invocation(&input.path, base, &opts, 180)?
            .args)
    }

    fn execute(
        &self,
        input: &ValidatedSource,
        staged_output: &StagedPath,
        req: &ProcessingRequest,
        runner: &dyn IsolatedProcessRunner,
        ctx: &JobContext,
    ) -> FileFusionResult<ProcessOutput> {
        ctx.check_cancelled()?;
        let staged_path = staged_output.staged_path();
        let out_base = staged_path.with_extension("");
        let (mode, expected_path) = match req.operation {
            ProcessingOperation::Convert {
                target_format: file_core::FormatId::Pdf,
                ..
            } => (
                TesseractOutputMode::SearchablePdf,
                out_base.with_extension("pdf"),
            ),
            _ => (
                TesseractOutputMode::PlainText,
                out_base.with_extension("txt"),
            ),
        };
        let opts = OcrOptions {
            output_mode: mode,
            ..OcrOptions::default()
        };
        let inv = self.build_ocr_invocation(
            &input.path,
            Some(&out_base),
            &opts,
            ctx.limits.subprocess_timeout_secs,
        )?;
        let out = runner.run_isolated(&inv, ctx)?;
        if out.timed_out || out.exit_code != 0 {
            return Err(FileFusionError::new(
                ErrorCode::IntInvariant,
                format!(
                    "Tesseract exited with code {}: {}",
                    out.exit_code,
                    String::from_utf8_lossy(&out.stderr).trim()
                ),
            ));
        }
        if expected_path.is_file() {
            fs::copy(&expected_path, staged_path)?;
            let _ = fs::remove_file(&expected_path);
        } else if !out.stdout.is_empty() {
            fs::write(staged_path, &out.stdout)?;
        }
        Ok(out)
    }
}

// =========================================================================
// Internal OCR Geometry, PDF XObject & Parser Helpers
// =========================================================================

fn validate_page_geometry(
    page_number: u32,
    page_width_px: u32,
    page_height_px: u32,
) -> FileFusionResult<()> {
    if page_number == 0 || page_width_px == 0 || page_height_px == 0 {
        return Err(FileFusionError::new(
            ErrorCode::FmtCorrupt,
            "OCR page_number, page_width_px, and page_height_px must be non-zero",
        ));
    }
    Ok(())
}

#[cfg(feature = "native-process")]
fn validate_ocr_options(opts: &OcrOptions) -> FileFusionResult<()> {
    let valid_lang = !opts.language.is_empty()
        && opts
            .language
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '+' | '-'));
    if !valid_lang || opts.language.starts_with('-') {
        return Err(FileFusionError::new(
            ErrorCode::FmtUnsupported,
            format!(
                "invalid Tesseract language specification: '{}'",
                opts.language
            ),
        ));
    }
    if opts.page_seg_mode > 13 {
        return Err(FileFusionError::new(
            ErrorCode::FmtUnsupported,
            format!(
                "Tesseract PSM {} out of valid range 0..=13",
                opts.page_seg_mode
            ),
        ));
    }
    Ok(())
}

fn build_ocr_overlay_stream(page_box: [f32; 4], ocr_page: &OcrPageResult) -> Stream {
    let origin_x = f64::from(page_box[0]);
    let origin_y = f64::from(page_box[1]);
    let box_w = (f64::from(page_box[2]) - origin_x).max(1.0);
    let box_h = (f64::from(page_box[3]) - origin_y).max(1.0);
    let img_w = f64::from(ocr_page.page_width_px.max(1));
    let img_h = f64::from(ocr_page.page_height_px.max(1));

    let mut ops = String::from("q\nBT\n3 Tr\n");
    if ocr_page.words.is_empty() && !ocr_page.plain_text.is_empty() {
        let escaped = escape_pdf_literal(&ocr_page.plain_text);
        let _ = writeln!(
            ops,
            "/F_FF_OCR 10.00 Tf\n1 0 0 1 {:.2} {:.2} Tm\n({escaped}) Tj",
            origin_x + 18.0,
            origin_y + (box_h - 36.0).max(0.0)
        );
    } else {
        for word in &ocr_page.words {
            if word.text.is_empty() {
                continue;
            }
            let x_pt = origin_x + (f64::from(word.left_px) / img_w) * box_w;
            let baseline_y_px = f64::from(word.top_px.saturating_add(word.height_px));
            let y_pt = (origin_y + box_h - (baseline_y_px / img_h) * box_h).max(origin_y);
            let font_pt =
                ((f64::from(word.height_px.max(1)) / img_h) * box_h * 0.85).clamp(4.0, 96.0);
            let escaped = escape_pdf_literal(&word.text);
            let _ = writeln!(
                ops,
                "/F_FF_OCR {font_pt:.2} Tf\n1 0 0 1 {x_pt:.2} {y_pt:.2} Tm\n({escaped}) Tj"
            );
        }
    }
    ops.push_str("ET\nQ\n");
    Stream::new(dictionary! {}, ops.into_bytes())
}

fn collect_page_xobjects(doc: &Document, page_id: ObjectId) -> Vec<(String, ObjectId)> {
    let mut found: HashMap<String, ObjectId> = HashMap::new();
    let Ok((res_opt, res_ids)) = doc.get_page_resources(page_id) else {
        return Vec::new();
    };
    let mut collect_from = |res_dict: &Dictionary| {
        let Ok(xobj) = res_dict.get(b"XObject") else {
            return;
        };
        let xd = match xobj {
            Object::Dictionary(d) => Some(d),
            Object::Reference(id) => doc.get_dictionary(*id).ok(),
            _ => None,
        };
        if let Some(d) = xd {
            for (k, v) in d {
                if let Ok(rid) = v.as_reference() {
                    found.insert(String::from_utf8_lossy(k).into_owned(), rid);
                }
            }
        }
    };
    if let Some(d) = res_opt {
        collect_from(d);
    }
    for rid in res_ids {
        if let Ok(d) = doc.get_dictionary(rid) {
            collect_from(d);
        }
    }
    let mut ordered: Vec<_> = found.into_iter().collect();
    ordered.sort_by(|a, b| a.0.cmp(&b.0));
    ordered
}

fn decode_pdf_image_to_png(page_number: u32, name: &str, st: &Stream) -> Option<ExtractedPdfImage> {
    let filter_name = st.dict.get(b"Filter").and_then(Object::as_name).ok();
    if filter_name == Some(b"DCTDecode") {
        let dyn_img = image::load_from_memory(&st.content).ok()?;
        let (w, h) = dyn_img.dimensions();
        let mut cursor = Cursor::new(Vec::new());
        dyn_img.write_to(&mut cursor, ImageFormat::Png).ok()?;
        return Some(ExtractedPdfImage {
            page_number,
            xobject_name: name.to_owned(),
            width_px: w,
            height_px: h,
            png_bytes: cursor.into_inner(),
        });
    }

    let w = u32::try_from(st.dict.get(b"Width").and_then(Object::as_i64).ok()?).ok()?;
    let h = u32::try_from(st.dict.get(b"Height").and_then(Object::as_i64).ok()?).ok()?;
    if w == 0 || h == 0 {
        return None;
    }

    let samples = st
        .decompressed_content()
        .unwrap_or_else(|_| st.content.clone());
    let cs_name = st.dict.get(b"ColorSpace").and_then(Object::as_name).ok();
    let expected_rgb = (w as usize).saturating_mul(h as usize).saturating_mul(3);
    let expected_gray = (w as usize).saturating_mul(h as usize);

    let dyn_img = if cs_name == Some(b"DeviceGray") && samples.len() >= expected_gray {
        let mut rgb = Vec::with_capacity(expected_rgb);
        for &g in &samples[..expected_gray] {
            rgb.extend_from_slice(&[g, g, g]);
        }
        RgbImage::from_raw(w, h, rgb).map(DynamicImage::ImageRgb8)?
    } else if samples.len() >= expected_rgb {
        RgbImage::from_raw(w, h, samples[..expected_rgb].to_vec()).map(DynamicImage::ImageRgb8)?
    } else {
        return None;
    };

    let mut cursor = Cursor::new(Vec::new());
    dyn_img.write_to(&mut cursor, ImageFormat::Png).ok()?;
    Some(ExtractedPdfImage {
        page_number,
        xobject_name: name.to_owned(),
        width_px: w,
        height_px: h,
        png_bytes: cursor.into_inner(),
    })
}

fn parse_hocr_bbox(attrs: &str) -> Option<(u32, u32, u32, u32)> {
    let idx = attrs.find("bbox ")?;
    let tail = &attrs[idx + 5..];
    let end = tail.find([';', '"', '\'']).unwrap_or(tail.len());
    let mut nums = tail[..end]
        .split_whitespace()
        .filter_map(|t| t.parse::<u32>().ok());
    let x0 = nums.next()?;
    let y0 = nums.next()?;
    let x1 = nums.next()?;
    let y1 = nums.next()?;
    Some((
        x0,
        y0,
        x1.saturating_sub(x0).max(1),
        y1.saturating_sub(y0).max(1),
    ))
}

fn parse_hocr_conf(attrs: &str) -> Option<f32> {
    let idx = attrs.find("x_wconf ")?;
    let tail = &attrs[idx + 8..];
    let token = tail.split([';', '"', '\'', ' ']).next()?;
    let val = token.trim().parse::<f32>().ok()?;
    val.is_finite().then(|| val.clamp(0.0, 100.0))
}

fn strip_inline_tags(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut in_tag = false;
    for ch in s.chars() {
        match ch {
            '<' => in_tag = true,
            '>' => in_tag = false,
            _ if !in_tag => out.push(ch),
            _ => {}
        }
    }
    out
}

fn unescape_html_entities(s: &str) -> String {
    s.replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
}
