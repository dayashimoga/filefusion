//! Pure-Rust Stage 4 PDF Studio structural manipulation (`merge`, `split`, `reorder`,
//! `rotate`, `crop`, `extract_pages`), scanned-page detection (`FR-PDF-003`), and vector
//! text watermarking (`FR-PDF-001`, `FR-PDF-004`).

use std::collections::{BTreeMap, HashMap, HashSet};
use std::fmt::Write as _;

use file_core::{ErrorCode, FileFusionError, FileFusionResult, JobContext};
use lopdf::content::{Content, Operation};
use lopdf::{Dictionary, Document, Object, ObjectId, Stream, dictionary};
use serde::{Deserialize, Serialize};

/// Symmetric or per-edge inset margins (in PDF points, `1 pt = 1/72 in`) for [`crop_pdf_pages`].
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct PdfCropMargins {
    /// Points trimmed from the left edge (`x0`).
    pub left_pt: f32,
    /// Points trimmed from the bottom edge (`y0`).
    pub bottom_pt: f32,
    /// Points trimmed from the right edge (`x1`).
    pub right_pt: f32,
    /// Points trimmed from the top edge (`y1`).
    pub top_pt: f32,
}

/// Configuration for vector text overlay watermarking via [`apply_pdf_text_watermark`].
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PdfWatermarkOptions {
    /// Visible watermark string (e.g. `"CONFIDENTIAL"`).
    pub text: String,
    /// Font size in points (`8.0..=144.0`).
    pub font_size_pt: f32,
    /// Alpha fill opacity (`0.05..=1.0`).
    pub opacity: f32,
    /// Rotation angle in degrees (typically `45.0`).
    pub angle_deg: f32,
    /// Optional explicit 1-based pages (`None` applies to all pages).
    pub pages: Option<Vec<u32>>,
}

impl Default for PdfWatermarkOptions {
    fn default() -> Self {
        Self {
            text: String::from("CONFIDENTIAL"),
            font_size_pt: 42.0,
            opacity: 0.28,
            angle_deg: 45.0,
            pages: None,
        }
    }
}

/// Page-level text/image scan diagnostic summarizing OCR & signature readiness (`FR-PDF-003`..`004`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PdfScanInspection {
    /// Total pages detected in `/Pages`.
    pub total_pages: u32,
    /// 1-based page numbers containing extractable text glyphs.
    pub text_pages: Vec<u32>,
    /// 1-based page numbers containing `/Image` `XObjects` with zero extractable text (OCR candidates).
    pub image_only_pages: Vec<u32>,
    /// `true` when every page lacks extractable text and at least one image-only page exists.
    pub is_scanned_image_only: bool,
    /// `true` if `/Sig` or `/ByteRange` digital signature dictionaries were detected.
    pub has_digital_signatures: bool,
}

/// Parses a 1-based PDF page range expression (e.g. `"1-3,5,7"` or `"all"`) against `total_pages`.
///
/// # Errors
/// Returns [`FileFusionError`] on empty spec, inverted range (`end < start`), or out-of-bounds page.
pub fn parse_page_range_spec(spec: &str, total_pages: u32) -> FileFusionResult<Vec<u32>> {
    if total_pages == 0 {
        return Err(FileFusionError::new(
            ErrorCode::FmtCorrupt,
            "PDF contains zero pages",
        ));
    }
    let trimmed = spec.trim();
    if trimmed.is_empty() {
        return Err(FileFusionError::new(
            ErrorCode::FmtCorrupt,
            "page range specification cannot be empty",
        ));
    }
    if trimmed.eq_ignore_ascii_case("all") {
        return Ok((1..=total_pages).collect());
    }

    let mut pages = Vec::new();
    for part in trimmed.split(',') {
        let token = part.trim();
        if token.is_empty() {
            return Err(FileFusionError::new(
                ErrorCode::FmtCorrupt,
                format!("malformed page range segment in `{spec}`"),
            ));
        }
        if let Some((start_s, end_s)) = token.split_once('-') {
            let start = parse_one_page_num(start_s.trim(), total_pages)?;
            let end = parse_one_page_num(end_s.trim(), total_pages)?;
            if end < start {
                return Err(FileFusionError::new(
                    ErrorCode::FmtCorrupt,
                    format!("descending page range `{token}` is invalid"),
                ));
            }
            pages.extend(start..=end);
        } else {
            pages.push(parse_one_page_num(token, total_pages)?);
        }
    }
    Ok(pages)
}

fn parse_one_page_num(raw: &str, total_pages: u32) -> FileFusionResult<u32> {
    let n = raw.parse::<u32>().map_err(|_| {
        FileFusionError::new(
            ErrorCode::FmtCorrupt,
            format!("invalid page number `{raw}`"),
        )
    })?;
    if n == 0 || n > total_pages {
        return Err(FileFusionError::new(
            ErrorCode::FmtCorrupt,
            format!("page number {n} is out of bounds (1..={total_pages})"),
        ));
    }
    Ok(n)
}

/// Inspects raw PDF bytes to classify text vs scanned image-only pages and detect digital signatures.
///
/// # Errors
/// Returns [`FileFusionError`] on invalid or encrypted PDF payloads.
pub fn inspect_pdf_scan_status(raw_pdf: &[u8]) -> FileFusionResult<PdfScanInspection> {
    let doc = load_unencrypted_pdf(raw_pdf)?;
    let pages = doc.get_pages();
    let total_pages = u32::try_from(pages.len()).unwrap_or(u32::MAX);
    let mut text_pages = Vec::new();
    let mut image_only_pages = Vec::new();

    for (&page_num, &page_id) in &pages {
        let extracted = doc.extract_text(&[page_num]).unwrap_or_default();
        let has_text = !extracted.trim().is_empty();
        if has_text {
            text_pages.push(page_num);
        } else if page_references_image_xobject(&doc, page_id) {
            image_only_pages.push(page_num);
        }
    }

    let is_scanned_image_only =
        total_pages > 0 && text_pages.is_empty() && !image_only_pages.is_empty();
    let has_digital_signatures = doc.objects.values().any(|obj| {
        if let Object::Dictionary(dict) = obj {
            let is_sig_type = dict
                .get(b"Type")
                .and_then(Object::as_name)
                .is_ok_and(|n| n == b"Sig");
            let has_byterange = dict.has(b"ByteRange");
            is_sig_type || has_byterange
        } else {
            false
        }
    });

    Ok(PdfScanInspection {
        total_pages,
        text_pages,
        image_only_pages,
        is_scanned_image_only,
        has_digital_signatures,
    })
}

/// Merges two or more PDF documents sequentially in pure Rust (`FR-PDF-001`).
///
/// # Errors
/// Returns [`FileFusionError`] when `inputs` is empty, any input is corrupt/encrypted, or cancelled.
pub fn merge_pdfs(inputs: &[&[u8]], ctx: &JobContext) -> FileFusionResult<Vec<u8>> {
    ctx.check_cancelled()?;
    if inputs.is_empty() {
        return Err(FileFusionError::new(
            ErrorCode::IoNotFound,
            "at least one PDF payload is required to merge",
        ));
    }

    let mut out_doc = Document::with_version("1.5");
    let pages_root_id = out_doc.new_object_id();
    let mut all_page_ids: Vec<ObjectId> = Vec::new();

    for raw in inputs {
        ctx.check_cancelled()?;
        let mut src_doc = load_unencrypted_pdf(raw)?;
        let start_id = out_doc.max_id.saturating_add(1);
        src_doc.renumber_objects_with(start_id);
        out_doc.max_id = out_doc.max_id.max(src_doc.max_id);

        let src_pages = src_doc.get_pages();
        for &page_id in src_pages.values() {
            materialize_inherited_page_attrs(&mut src_doc, page_id);
            set_page_parent(&mut src_doc, page_id, pages_root_id);
            all_page_ids.push(page_id);
        }

        for (obj_id, obj) in src_doc.objects {
            out_doc.objects.insert(obj_id, obj);
        }
    }

    if all_page_ids.is_empty() {
        return Err(FileFusionError::new(
            ErrorCode::FmtCorrupt,
            "merged PDF sources contain zero pages",
        ));
    }

    finalize_document_pages(&mut out_doc, pages_root_id, &all_page_ids, ctx)
}

/// Extracts or reorders exact 1-based pages from `raw_pdf` into a standalone PDF (`FR-PDF-001`).
///
/// # Errors
/// Returns [`FileFusionError`] on invalid page indices, empty list, encrypted PDF, or cancellation.
pub fn extract_or_reorder_pdf_pages(
    raw_pdf: &[u8],
    ordered_pages: &[u32],
    ctx: &JobContext,
) -> FileFusionResult<Vec<u8>> {
    ctx.check_cancelled()?;
    if ordered_pages.is_empty() {
        return Err(FileFusionError::new(
            ErrorCode::FmtCorrupt,
            "at least one target page index is required",
        ));
    }

    let mut src_doc = load_unencrypted_pdf(raw_pdf)?;
    let page_map = src_doc.get_pages();
    let total_pages = u32::try_from(page_map.len()).unwrap_or(0);

    let mut out_doc = Document::with_version("1.5");
    let pages_root_id = out_doc.new_object_id();
    let mut cloned_pages = Vec::with_capacity(ordered_pages.len());

    for &page_num in ordered_pages {
        ctx.check_cancelled()?;
        let Some(&orig_page_id) = page_map.get(&page_num) else {
            return Err(FileFusionError::new(
                ErrorCode::FmtCorrupt,
                format!("requested page {page_num} exceeds PDF page count ({total_pages})"),
            ));
        };
        materialize_inherited_page_attrs(&mut src_doc, orig_page_id);

        // Allocate fresh object IDs per requested occurrence (allows duplicating a page cleanly if desired).
        let mut local_map: HashMap<ObjectId, ObjectId> = HashMap::new();
        let new_page_id =
            copy_object_graph_without_parent(&src_doc, orig_page_id, &mut out_doc, &mut local_map);
        set_page_parent(&mut out_doc, new_page_id, pages_root_id);
        cloned_pages.push(new_page_id);
    }

    finalize_document_pages(&mut out_doc, pages_root_id, &cloned_pages, ctx)
}

/// Splits `raw_pdf` into separate PDFs matching each range specification (e.g. `["1-2", "3-4"]`).
///
/// # Errors
/// Returns [`FileFusionError`] when `ranges` is empty or any range is invalid.
pub fn split_pdf_by_ranges(
    raw_pdf: &[u8],
    ranges: &[&str],
    ctx: &JobContext,
) -> FileFusionResult<Vec<Vec<u8>>> {
    if ranges.is_empty() {
        return Err(FileFusionError::new(
            ErrorCode::FmtCorrupt,
            "at least one split page range is required",
        ));
    }
    let doc = load_unencrypted_pdf(raw_pdf)?;
    let total_pages = u32::try_from(doc.get_pages().len()).unwrap_or(0);
    let mut outputs = Vec::with_capacity(ranges.len());
    for spec in ranges {
        ctx.check_cancelled()?;
        let pages = parse_page_range_spec(spec, total_pages)?;
        outputs.push(extract_or_reorder_pdf_pages(raw_pdf, &pages, ctx)?);
    }
    Ok(outputs)
}

/// Splits `raw_pdf` into sequential chunks of `chunk_size` pages each.
///
/// # Errors
/// Returns [`FileFusionError`] when `chunk_size == 0` or the PDF is corrupt/encrypted.
pub fn split_pdf_every_n_pages(
    raw_pdf: &[u8],
    chunk_size: u32,
    ctx: &JobContext,
) -> FileFusionResult<Vec<Vec<u8>>> {
    if chunk_size == 0 {
        return Err(FileFusionError::new(
            ErrorCode::FmtCorrupt,
            "PDF split chunk size must be >= 1",
        ));
    }
    let doc = load_unencrypted_pdf(raw_pdf)?;
    let total_pages = u32::try_from(doc.get_pages().len()).unwrap_or(0);
    if total_pages == 0 {
        return Err(FileFusionError::new(
            ErrorCode::FmtCorrupt,
            "PDF contains zero pages",
        ));
    }
    let mut outputs = Vec::new();
    let mut start = 1u32;
    while start <= total_pages {
        ctx.check_cancelled()?;
        let end = start.saturating_add(chunk_size - 1).min(total_pages);
        let chunk: Vec<u32> = (start..=end).collect();
        outputs.push(extract_or_reorder_pdf_pages(raw_pdf, &chunk, ctx)?);
        start = end.saturating_add(1);
    }
    Ok(outputs)
}

/// Rotates target pages (or all pages when `target_pages == None`) by `delta_degrees` (multiple of 90).
///
/// # Errors
/// Returns [`FileFusionError`] if `delta_degrees % 90 != 0` or page indices exceed bounds.
pub fn rotate_pdf_pages(
    raw_pdf: &[u8],
    target_pages: Option<&[u32]>,
    delta_degrees: i32,
    ctx: &JobContext,
) -> FileFusionResult<Vec<u8>> {
    ctx.check_cancelled()?;
    if delta_degrees % 90 != 0 {
        return Err(FileFusionError::new(
            ErrorCode::FmtCorrupt,
            format!("PDF page rotation must be a multiple of 90 degrees (got {delta_degrees})"),
        ));
    }

    let mut doc = load_unencrypted_pdf(raw_pdf)?;
    let selected_ids = resolve_target_page_ids(&doc, target_pages)?;

    for page_id in selected_ids {
        ctx.check_cancelled()?;
        let existing_deg = resolve_inherited_integer(&doc, page_id, b"Rotate").unwrap_or(0);
        let updated = i64::from(delta_degrees)
            .saturating_add(existing_deg)
            .rem_euclid(360);
        if let Ok(dict) = doc.get_dictionary_mut(page_id) {
            dict.set("Rotate", Object::Integer(updated));
        }
    }

    serialize_pdf(&mut doc)
}

/// Crops target pages' `/CropBox` inward by `margins_pt` points (`FR-PDF-001`).
///
/// # Errors
/// Returns [`FileFusionError`] if margins are negative, inverted (`width <= 0` / `height <= 0`),
/// or target pages exceed bounds.
pub fn crop_pdf_pages(
    raw_pdf: &[u8],
    target_pages: Option<&[u32]>,
    margins: PdfCropMargins,
    ctx: &JobContext,
) -> FileFusionResult<Vec<u8>> {
    ctx.check_cancelled()?;
    if !margins.left_pt.is_finite()
        || !margins.bottom_pt.is_finite()
        || !margins.right_pt.is_finite()
        || !margins.top_pt.is_finite()
        || margins.left_pt < 0.0
        || margins.bottom_pt < 0.0
        || margins.right_pt < 0.0
        || margins.top_pt < 0.0
    {
        return Err(FileFusionError::new(
            ErrorCode::FmtCorrupt,
            "crop margins must be finite and non-negative",
        ));
    }

    let mut doc = load_unencrypted_pdf(raw_pdf)?;
    let selected_ids = resolve_target_page_ids(&doc, target_pages)?;

    for page_id in selected_ids {
        ctx.check_cancelled()?;
        let base_box = resolve_page_box(&doc, page_id);
        let nx0 = base_box[0] + margins.left_pt;
        let ny0 = base_box[1] + margins.bottom_pt;
        let nx1 = base_box[2] - margins.right_pt;
        let ny1 = base_box[3] - margins.top_pt;
        if nx1 <= nx0 + 1.0 || ny1 <= ny0 + 1.0 {
            return Err(FileFusionError::new(
                ErrorCode::FmtCorrupt,
                "crop margins collapse page dimensions below minimum 1 pt",
            ));
        }
        if let Ok(dict) = doc.get_dictionary_mut(page_id) {
            dict.set(
                "CropBox",
                Object::Array(vec![
                    Object::Real(nx0),
                    Object::Real(ny0),
                    Object::Real(nx1),
                    Object::Real(ny1),
                ]),
            );
        }
    }

    serialize_pdf(&mut doc)
}

/// Overlays a semi-transparent centered rotated vector text watermark onto PDF pages (`FR-PDF-004`).
///
/// # Errors
/// Returns [`FileFusionError`] on empty watermark text, invalid opacity/font size, corrupt/encrypted
/// PDF, or invalid page indices.
pub fn apply_pdf_text_watermark(
    raw_pdf: &[u8],
    opts: &PdfWatermarkOptions,
    ctx: &JobContext,
) -> FileFusionResult<Vec<u8>> {
    ctx.check_cancelled()?;
    if opts.text.trim().is_empty() {
        return Err(FileFusionError::new(
            ErrorCode::FmtCorrupt,
            "watermark text cannot be empty",
        ));
    }
    if !opts.font_size_pt.is_finite() || !opts.opacity.is_finite() || !opts.angle_deg.is_finite() {
        return Err(FileFusionError::new(
            ErrorCode::FmtCorrupt,
            "watermark font size, opacity, and angle must be finite",
        ));
    }

    let mut doc = load_unencrypted_pdf(raw_pdf)?;
    let selected_ids = resolve_target_page_ids(&doc, opts.pages.as_deref())?;

    let font_id = doc.add_object(dictionary! {
        "Type" => "Font",
        "Subtype" => "Type1",
        "BaseFont" => "Helvetica-Bold",
    });
    let alpha = opts.opacity.clamp(0.05, 1.0);
    let gs_id = doc.add_object(dictionary! {
        "Type" => "ExtGState",
        "CA" => Object::Real(alpha),
        "ca" => Object::Real(alpha),
    });

    for page_id in selected_ids {
        ctx.check_cancelled()?;
        materialize_inherited_page_attrs(&mut doc, page_id);
        let page_box = resolve_page_box(&doc, page_id);
        attach_page_resources(
            &mut doc,
            page_id,
            "F_FF_WM",
            font_id,
            Some(("GS_FF_WM", gs_id)),
        );
        let overlay = build_watermark_stream(page_box, opts);
        let overlay_id = doc.add_object(overlay);
        append_content_stream_to_page(&mut doc, page_id, overlay_id);
    }

    serialize_pdf(&mut doc)
}

// =========================================================================
// Internal PDF Graph & Box Helpers
// =========================================================================

pub(crate) fn load_unencrypted_pdf(raw_pdf: &[u8]) -> FileFusionResult<Document> {
    if raw_pdf.is_empty() {
        return Err(FileFusionError::new(
            ErrorCode::FmtCorrupt,
            "PDF input payload is empty",
        ));
    }
    let doc = Document::load_mem(raw_pdf).map_err(|err| {
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
            "PDF is encrypted; unlock with user/owner password before editing",
        ));
    }
    Ok(doc)
}

fn resolve_target_page_ids(
    doc: &Document,
    target_pages: Option<&[u32]>,
) -> FileFusionResult<Vec<ObjectId>> {
    let page_map: BTreeMap<u32, ObjectId> = doc.get_pages();
    let total_pages = u32::try_from(page_map.len()).unwrap_or(0);
    let Some(req_pages) = target_pages else {
        return Ok(page_map.into_values().collect());
    };
    let mut ids = Vec::with_capacity(req_pages.len());
    for &p in req_pages {
        let Some(&pid) = page_map.get(&p) else {
            return Err(FileFusionError::new(
                ErrorCode::FmtCorrupt,
                format!("requested page {p} exceeds PDF page count ({total_pages})"),
            ));
        };
        ids.push(pid);
    }
    Ok(ids)
}

fn resolve_inherited_integer(doc: &Document, mut obj_id: ObjectId, key: &[u8]) -> Option<i64> {
    let mut visited = HashSet::new();
    while visited.insert(obj_id) {
        let dict = doc.get_dictionary(obj_id).ok()?;
        if let Ok(Object::Integer(v)) = dict.get(key) {
            return Some(*v);
        }
        obj_id = dict.get(b"Parent").and_then(Object::as_reference).ok()?;
    }
    None
}

pub(crate) fn resolve_page_box(doc: &Document, page_id: ObjectId) -> [f32; 4] {
    for key in [b"CropBox".as_slice(), b"MediaBox".as_slice()] {
        if let Some(arr) = resolve_inherited_array(doc, page_id, key) {
            if let Some(rect) = parse_pdf_rect(&arr) {
                return rect;
            }
        }
    }
    [0.0, 0.0, 612.0, 792.0]
}

fn resolve_inherited_array(
    doc: &Document,
    mut obj_id: ObjectId,
    key: &[u8],
) -> Option<Vec<Object>> {
    let mut visited = HashSet::new();
    while visited.insert(obj_id) {
        let dict = doc.get_dictionary(obj_id).ok()?;
        if let Ok(Object::Array(arr)) = dict.get(key) {
            return Some(arr.clone());
        }
        obj_id = dict.get(b"Parent").and_then(Object::as_reference).ok()?;
    }
    None
}

fn parse_pdf_rect(arr: &[Object]) -> Option<[f32; 4]> {
    if arr.len() < 4 {
        return None;
    }
    let to_f32 = |o: &Object| -> Option<f32> {
        match o {
            Object::Real(v) => Some(*v),
            #[allow(clippy::cast_precision_loss)]
            Object::Integer(v) => Some(*v as f32),
            _ => None,
        }
    };
    Some([
        to_f32(&arr[0])?,
        to_f32(&arr[1])?,
        to_f32(&arr[2])?,
        to_f32(&arr[3])?,
    ])
}

pub(crate) fn materialize_inherited_page_attrs(doc: &mut Document, page_id: ObjectId) {
    let media_box = resolve_inherited_array(doc, page_id, b"MediaBox");
    let crop_box = resolve_inherited_array(doc, page_id, b"CropBox");
    let resources = doc.get_page_resources(page_id).ok().and_then(|(d, ids)| {
        if let Some(dict) = d {
            Some(Object::Dictionary(dict.clone()))
        } else {
            ids.first().map(|&id| Object::Reference(id))
        }
    });
    let rotate = resolve_inherited_integer(doc, page_id, b"Rotate");

    if let Ok(dict) = doc.get_dictionary_mut(page_id) {
        if !dict.has(b"MediaBox") {
            if let Some(mb) = media_box {
                dict.set("MediaBox", Object::Array(mb));
            }
        }
        if !dict.has(b"CropBox") {
            if let Some(cb) = crop_box {
                dict.set("CropBox", Object::Array(cb));
            }
        }
        if !dict.has(b"Resources") {
            if let Some(res) = resources {
                dict.set("Resources", res);
            }
        }
        if !dict.has(b"Rotate") {
            if let Some(rot) = rotate {
                dict.set("Rotate", Object::Integer(rot));
            }
        }
    }
}

fn set_page_parent(doc: &mut Document, page_id: ObjectId, parent_id: ObjectId) {
    if let Ok(dict) = doc.get_dictionary_mut(page_id) {
        dict.set("Parent", Object::Reference(parent_id));
    }
}

fn copy_object_graph_without_parent(
    src: &Document,
    old_id: ObjectId,
    dst: &mut Document,
    id_map: &mut HashMap<ObjectId, ObjectId>,
) -> ObjectId {
    if let Some(&mapped) = id_map.get(&old_id) {
        return mapped;
    }
    let new_id = dst.new_object_id();
    id_map.insert(old_id, new_id);

    let cloned_obj = src.objects.get(&old_id).cloned().unwrap_or(Object::Null);
    let rewritten = rewrite_object_refs(src, cloned_obj, dst, id_map, true);
    dst.objects.insert(new_id, rewritten);
    new_id
}

fn rewrite_object_refs(
    src: &Document,
    obj: Object,
    dst: &mut Document,
    id_map: &mut HashMap<ObjectId, ObjectId>,
    skip_parent_key: bool,
) -> Object {
    match obj {
        Object::Reference(ref_id) => {
            let mapped = copy_object_graph_without_parent(src, ref_id, dst, id_map);
            Object::Reference(mapped)
        }
        Object::Array(items) => Object::Array(
            items
                .into_iter()
                .map(|it| rewrite_object_refs(src, it, dst, id_map, false))
                .collect(),
        ),
        Object::Dictionary(dict) => {
            Object::Dictionary(rewrite_dict_refs(src, dict, dst, id_map, skip_parent_key))
        }
        Object::Stream(mut stream) => {
            stream.dict = rewrite_dict_refs(src, stream.dict, dst, id_map, false);
            Object::Stream(stream)
        }
        other => other,
    }
}

fn rewrite_dict_refs(
    src: &Document,
    dict: Dictionary,
    dst: &mut Document,
    id_map: &mut HashMap<ObjectId, ObjectId>,
    skip_parent_key: bool,
) -> Dictionary {
    let mut new_dict = Dictionary::new();
    for (k, v) in dict {
        if skip_parent_key && k == b"Parent" {
            continue;
        }
        let rewritten = rewrite_object_refs(src, v, dst, id_map, false);
        new_dict.set(k, rewritten);
    }
    new_dict
}

fn finalize_document_pages(
    doc: &mut Document,
    pages_root_id: ObjectId,
    page_ids: &[ObjectId],
    ctx: &JobContext,
) -> FileFusionResult<Vec<u8>> {
    ctx.check_cancelled()?;
    let kids: Vec<Object> = page_ids.iter().copied().map(Object::Reference).collect();
    doc.objects.insert(
        pages_root_id,
        Object::Dictionary(dictionary! {
            "Type" => "Pages",
            "Kids" => kids,
            "Count" => i64::try_from(page_ids.len()).unwrap_or(0),
        }),
    );
    let catalog_id = doc.add_object(dictionary! {
        "Type" => "Catalog",
        "Pages" => pages_root_id,
    });
    doc.trailer.set("Root", catalog_id);
    doc.prune_objects();
    doc.renumber_objects();
    serialize_pdf(doc)
}

pub(crate) fn serialize_pdf(doc: &mut Document) -> FileFusionResult<Vec<u8>> {
    doc.compress();
    let mut out = Vec::new();
    doc.save_to(&mut out).map_err(|err| {
        FileFusionError::new(
            ErrorCode::IntInvariant,
            format!("failed to serialize PDF output: {err}"),
        )
    })?;
    Ok(out)
}

fn page_references_image_xobject(doc: &Document, page_id: ObjectId) -> bool {
    let Ok((res_opt, res_ids)) = doc.get_page_resources(page_id) else {
        return false;
    };
    let check_dict = |dict: &Dictionary| -> bool {
        let Ok(xobj_obj) = dict.get(b"XObject") else {
            return false;
        };
        let xobj_dict = match xobj_obj {
            Object::Dictionary(d) => Some(d),
            Object::Reference(id) => doc.get_dictionary(*id).ok(),
            _ => None,
        };
        let Some(xd) = xobj_dict else {
            return false;
        };
        xd.iter().any(|(_, val)| {
            if let Ok(ref_id) = val.as_reference() {
                if let Ok(Object::Stream(st)) = doc.get_object(ref_id) {
                    return st
                        .dict
                        .get(b"Subtype")
                        .and_then(Object::as_name)
                        .is_ok_and(|n| n == b"Image");
                }
            }
            false
        })
    };

    if res_opt.is_some_and(check_dict) {
        return true;
    }
    res_ids
        .into_iter()
        .any(|id| doc.get_dictionary(id).is_ok_and(check_dict))
}

pub(crate) fn attach_page_resources(
    doc: &mut Document,
    page_id: ObjectId,
    font_key: &str,
    font_id: ObjectId,
    gs: Option<(&str, ObjectId)>,
) {
    let existing_res_ref = doc
        .get_dictionary(page_id)
        .ok()
        .and_then(|d| d.get(b"Resources").and_then(Object::as_reference).ok());

    let mut res_dict = if let Some(rid) = existing_res_ref {
        doc.get_dictionary(rid).cloned().unwrap_or_default()
    } else if let Ok(Object::Dictionary(d)) = doc
        .get_dictionary(page_id)
        .and_then(|p| p.get(b"Resources"))
        .cloned()
    {
        d
    } else {
        Dictionary::new()
    };

    let mut font_dict = match res_dict.get(b"Font").cloned() {
        Ok(Object::Dictionary(d)) => d,
        Ok(Object::Reference(id)) => doc.get_dictionary(id).cloned().unwrap_or_default(),
        _ => Dictionary::new(),
    };
    font_dict.set(font_key, Object::Reference(font_id));
    res_dict.set("Font", Object::Dictionary(font_dict));

    if let Some((gs_key, gs_id)) = gs {
        let mut gs_dict = match res_dict.get(b"ExtGState").cloned() {
            Ok(Object::Dictionary(d)) => d,
            Ok(Object::Reference(id)) => doc.get_dictionary(id).cloned().unwrap_or_default(),
            _ => Dictionary::new(),
        };
        gs_dict.set(gs_key, Object::Reference(gs_id));
        res_dict.set("ExtGState", Object::Dictionary(gs_dict));
    }

    if let Ok(page_dict) = doc.get_dictionary_mut(page_id) {
        page_dict.set("Resources", Object::Dictionary(res_dict));
    }
}

pub(crate) fn escape_pdf_literal(text: &str) -> String {
    let mut escaped = String::with_capacity(text.len());
    for ch in text.chars() {
        match ch {
            '\\' | '(' | ')' => {
                escaped.push('\\');
                escaped.push(ch);
            }
            '\r' | '\n' => escaped.push(' '),
            _ => escaped.push(ch),
        }
    }
    escaped
}

fn build_watermark_stream(page_box: [f32; 4], opts: &PdfWatermarkOptions) -> Stream {
    let cx = f64::midpoint(f64::from(page_box[0]), f64::from(page_box[2]));
    let cy = f64::midpoint(f64::from(page_box[1]), f64::from(page_box[3]));
    let rad = f64::from(opts.angle_deg).to_radians();
    let cos_a = rad.cos();
    let sin_a = rad.sin();
    let size = f64::from(opts.font_size_pt.clamp(8.0, 144.0));
    #[allow(clippy::cast_precision_loss)]
    let est_half_w = (opts.text.len() as f64) * size * 0.28;
    let tx = cx - est_half_w * cos_a;
    let ty = cy - est_half_w * sin_a;
    let escaped = escape_pdf_literal(&opts.text);

    let mut stream_ops = String::new();
    let _ = write!(
        stream_ops,
        "q\n/GS_FF_WM gs\n0.55 0.12 0.12 rg\nBT\n/F_FF_WM {size:.2} Tf\n{cos_a:.4} {sin_a:.4} {:.4} {cos_a:.4} {tx:.2} {ty:.2} Tm\n({escaped}) Tj\nET\nQ\n",
        -sin_a
    );
    Stream::new(dictionary! {}, stream_ops.into_bytes())
}

pub(crate) fn append_content_stream_to_page(
    doc: &mut Document,
    page_id: ObjectId,
    overlay_id: ObjectId,
) {
    let existing = doc
        .get_dictionary(page_id)
        .ok()
        .and_then(|d| d.get(b"Contents").ok())
        .cloned();

    let new_contents = match existing {
        Some(Object::Array(mut arr)) => {
            arr.push(Object::Reference(overlay_id));
            Object::Array(arr)
        }
        Some(Object::Reference(id)) => {
            Object::Array(vec![Object::Reference(id), Object::Reference(overlay_id)])
        }
        Some(Object::Stream(st)) => {
            let old_id = doc.add_object(Object::Stream(st));
            Object::Array(vec![
                Object::Reference(old_id),
                Object::Reference(overlay_id),
            ])
        }
        _ => Object::Reference(overlay_id),
    };

    if let Ok(dict) = doc.get_dictionary_mut(page_id) {
        dict.set("Contents", new_contents);
    }
}

/// Helper used by tests and downstream OCR builder to emit simple single-page text PDFs cleanly.
#[must_use]
pub fn build_simple_text_pdf_bytes(pages_text: &[&str]) -> Vec<u8> {
    let mut doc = Document::with_version("1.5");
    let root_pages_id = doc.new_object_id();
    let font_id = doc.add_object(dictionary! {
        "Type" => "Font",
        "Subtype" => "Type1",
        "BaseFont" => "Helvetica",
    });
    let mut kids = Vec::with_capacity(pages_text.len());
    for text in pages_text {
        let content = Content {
            operations: vec![
                Operation::new("BT", vec![]),
                Operation::new("Tf", vec!["F1".into(), 12.into()]),
                Operation::new("Td", vec![72.into(), 700.into()]),
                Operation::new("Tj", vec![Object::string_literal(*text)]),
                Operation::new("ET", vec![]),
            ],
        };
        let encoded = content.encode().unwrap_or_default();
        let content_id = doc.add_object(Stream::new(dictionary! {}, encoded));
        let res_id = doc.add_object(dictionary! {
            "Font" => dictionary! { "F1" => font_id },
        });
        let single_page_id = doc.add_object(dictionary! {
            "Type" => "Page",
            "Parent" => root_pages_id,
            "MediaBox" => vec![0.into(), 0.into(), 612.into(), 792.into()],
            "Resources" => res_id,
            "Contents" => content_id,
        });
        kids.push(Object::Reference(single_page_id));
    }
    doc.objects.insert(
        root_pages_id,
        Object::Dictionary(dictionary! {
            "Type" => "Pages",
            "Kids" => kids,
            "Count" => i64::try_from(pages_text.len()).unwrap_or(0),
        }),
    );
    let catalog_id = doc.add_object(dictionary! { "Type" => "Catalog", "Pages" => root_pages_id });
    doc.trailer.set("Root", catalog_id);
    let mut out = Vec::new();
    let _ = doc.save_to(&mut out);
    out
}
