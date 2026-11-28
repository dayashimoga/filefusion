//! Pure-Rust Tier 1 native conversion engines for `OOXML` (`DOCX`, `XLSX`),
//! `OpenDocument` (`ODT`, `ODS`), and `EPUB` 2/3 containers (`FR-DOC-002`..`003`, `FR-SEC-001`).

use std::collections::HashMap;
use std::fmt::Write as _;

use compression_office::{
    extract_visible_xml_text, is_optimizable_xml_entry, read_validated_zip_entries,
    write_office_container_entries,
};
use file_core::{
    ErrorCode, FileFusionError, FileFusionResult, FormatId, JobContext, ResourceLimits,
};
use serde_json::{Map, Value};

use crate::markup::{
    BlockNode, InlineNode, MarkupDocument, escape_html_text, flatten_inlines_plain,
    parse_markup_document, render_markup_document,
};
use crate::tabular::{convert_structured_data, parse_structured_value};

/// Returns `true` for container-based document formats (`Docx`, `Xlsx`, `Odt`, `Ods`, `Epub`)
/// supported by pure-Rust Tier 1 container conversion.
#[must_use]
pub const fn is_native_office_or_epub(fmt: FormatId) -> bool {
    matches!(
        fmt,
        FormatId::Docx | FormatId::Xlsx | FormatId::Odt | FormatId::Ods | FormatId::Epub
    )
}

/// Returns `true` for spreadsheet/workbook formats (`Xlsx`, `Ods`).
#[must_use]
pub const fn is_spreadsheet_format(fmt: FormatId) -> bool {
    matches!(fmt, FormatId::Xlsx | FormatId::Ods)
}

/// Converts between `Xlsx`, `Ods`, and structured/tabular formats (`Csv`, `Json`, `Yaml`, `Xml`,
/// `Markdown`, `Html`, `PlainText`) without spawning external binaries.
///
/// # Errors
/// Returns [`FileFusionError`] on ZIP corruption, Zip-Slip/bomb violations, invalid XML, or
/// unsupported source/target pairs.
pub fn convert_spreadsheet_container(
    raw_bytes: &[u8],
    source: FormatId,
    target: FormatId,
    strip_metadata: bool,
    limits: &ResourceLimits,
    ctx: &JobContext,
) -> FileFusionResult<Vec<u8>> {
    ctx.check_cancelled()?;
    let rows = match source {
        FormatId::Xlsx => parse_xlsx_rows(raw_bytes, limits, ctx)?,
        FormatId::Ods => parse_ods_rows(raw_bytes, limits, ctx)?,
        FormatId::Csv | FormatId::Json | FormatId::Yaml | FormatId::Xml => {
            let text = std::str::from_utf8(raw_bytes).map_err(|err| {
                FileFusionError::new(
                    ErrorCode::FmtCorrupt,
                    format!("spreadsheet source text is invalid UTF-8: {err}"),
                )
            })?;
            let val = parse_structured_value(text, source)?;
            value_to_string_matrix(&val)
        }
        other => {
            return Err(FileFusionError::new(
                ErrorCode::FmtUnsupported,
                format!("unsupported spreadsheet conversion source {other:?}"),
            ));
        }
    };

    match target {
        FormatId::Xlsx => build_xlsx_container(&rows, strip_metadata, ctx),
        FormatId::Ods => build_ods_container(&rows, strip_metadata, ctx),
        FormatId::Csv => Ok(matrix_to_csv_string(&rows).into_bytes()),
        FormatId::Json
        | FormatId::Yaml
        | FormatId::Xml
        | FormatId::Markdown
        | FormatId::Html
        | FormatId::PlainText => {
            let csv = matrix_to_csv_string(&rows);
            let converted = convert_structured_data(&csv, FormatId::Csv, target, strip_metadata)?;
            Ok(converted.into_bytes())
        }
        other => Err(FileFusionError::new(
            ErrorCode::FmtUnsupported,
            format!("unsupported spreadsheet conversion target {other:?}"),
        )),
    }
}

/// Converts between prose document/ebook containers (`Docx`, `Odt`, `Epub`) and markup formats
/// (`Markdown`, `Html`, `PlainText`) natively in pure Rust (`FR-DOC-002`..`003`).
///
/// # Errors
/// Returns [`FileFusionError`] on container security failure, invalid ZIP/XML, or unsupported route.
pub fn convert_document_container(
    raw_bytes: &[u8],
    source: FormatId,
    target: FormatId,
    strip_metadata: bool,
    limits: &ResourceLimits,
    ctx: &JobContext,
) -> FileFusionResult<Vec<u8>> {
    ctx.check_cancelled()?;
    let doc = match source {
        FormatId::Docx => parse_docx_document(raw_bytes, limits, ctx)?,
        FormatId::Odt => parse_odt_document(raw_bytes, limits, ctx)?,
        FormatId::Epub => parse_epub_document(raw_bytes, limits, ctx)?,
        FormatId::Markdown | FormatId::Html | FormatId::PlainText => {
            let text = std::str::from_utf8(raw_bytes).map_err(|err| {
                FileFusionError::new(
                    ErrorCode::FmtCorrupt,
                    format!("document source text is invalid UTF-8: {err}"),
                )
            })?;
            parse_markup_document(text, source)?
        }
        other => {
            return Err(FileFusionError::new(
                ErrorCode::FmtUnsupported,
                format!("unsupported document container source {other:?}"),
            ));
        }
    };

    match target {
        FormatId::Docx => build_docx_container(&doc, strip_metadata, ctx),
        FormatId::Odt => build_odt_container(&doc, strip_metadata, ctx),
        FormatId::Epub => build_epub_container(&doc, strip_metadata, ctx),
        FormatId::Markdown | FormatId::Html | FormatId::PlainText => {
            render_markup_document(&doc, target, strip_metadata).map(String::into_bytes)
        }
        other => Err(FileFusionError::new(
            ErrorCode::FmtUnsupported,
            format!("unsupported document container target {other:?}"),
        )),
    }
}

/// Inspects a ZIP container's textual/XML markup to estimate [`crate::preflight::StructuralInventory`]
/// before or after OOXML/ODF/EPUB conversion (`FR-FID-001`).
#[must_use]
pub fn extract_container_markdown_preview(
    raw_bytes: &[u8],
    format: FormatId,
    limits: &ResourceLimits,
    ctx: &JobContext,
) -> Option<String> {
    let doc = match format {
        FormatId::Docx => parse_docx_document(raw_bytes, limits, ctx).ok()?,
        FormatId::Odt => parse_odt_document(raw_bytes, limits, ctx).ok()?,
        FormatId::Epub => parse_epub_document(raw_bytes, limits, ctx).ok()?,
        _ => return None,
    };
    render_markup_document(&doc, FormatId::Markdown, true).ok()
}

// =========================================================================
// XLSX & ODS Parsers / Builders
// =========================================================================

fn find_zip_entry_text<'a>(entries: &'a [(String, Vec<u8>)], name: &str) -> Option<&'a str> {
    entries
        .iter()
        .find(|(k, _)| k.eq_ignore_ascii_case(name))
        .and_then(|(_, v)| std::str::from_utf8(v).ok())
}

fn parse_xlsx_rows(
    raw_zip: &[u8],
    limits: &ResourceLimits,
    ctx: &JobContext,
) -> FileFusionResult<Vec<Vec<String>>> {
    let entries = read_validated_zip_entries(raw_zip, limits, ctx)?;
    reject_xml_entity_bombs(&entries)?;

    let shared_strings = find_zip_entry_text(&entries, "xl/sharedStrings.xml")
        .map(parse_xlsx_shared_strings)
        .unwrap_or_default();

    let Some(sheet_xml) = entries
        .iter()
        .find(|(k, _)| {
            k.to_ascii_lowercase().starts_with("xl/worksheets/sheet")
                && std::path::Path::new(k)
                    .extension()
                    .is_some_and(|ext| ext.eq_ignore_ascii_case("xml"))
        })
        .and_then(|(_, v)| std::str::from_utf8(v).ok())
    else {
        return Err(FileFusionError::new(
            ErrorCode::FmtCorrupt,
            "XLSX container is missing worksheet XML",
        ));
    };

    let mut rows = Vec::new();
    for row_inner in iter_xml_blocks(sheet_xml, "row") {
        let mut cells: Vec<String> = Vec::new();
        for (cell_attrs, cell_inner) in iter_xml_tags_with_attrs(row_inner, "c") {
            let cell_ref = extract_attr_value(cell_attrs, "r").unwrap_or_default();
            let cell_type = extract_attr_value(cell_attrs, "t").unwrap_or_default();
            let val = if cell_type == "inlineStr" {
                iter_xml_blocks(cell_inner, "t")
                    .map(decode_xml_entities_basic)
                    .collect::<String>()
            } else {
                let raw_v = iter_xml_blocks(cell_inner, "v")
                    .next()
                    .map(decode_xml_entities_basic)
                    .unwrap_or_default();
                if cell_type == "s" {
                    raw_v
                        .trim()
                        .parse::<usize>()
                        .ok()
                        .and_then(|idx| shared_strings.get(idx).cloned())
                        .unwrap_or(raw_v)
                } else if cell_type == "b" {
                    if raw_v.trim() == "1" {
                        String::from("true")
                    } else {
                        String::from("false")
                    }
                } else {
                    raw_v
                }
            };

            let col_idx = col_index_from_cell_ref(&cell_ref).unwrap_or(cells.len());
            if col_idx >= cells.len() && col_idx < 1024 {
                cells.resize(col_idx + 1, String::new());
            }
            if col_idx < cells.len() {
                cells[col_idx] = val;
            }
        }
        if !cells.is_empty() {
            rows.push(cells);
        }
    }
    Ok(rows)
}

fn parse_xlsx_shared_strings(xml: &str) -> Vec<String> {
    iter_xml_blocks(xml, "si")
        .map(|si_inner| {
            let parts: Vec<String> = iter_xml_blocks(si_inner, "t")
                .map(decode_xml_entities_basic)
                .collect();
            if parts.is_empty() {
                decode_xml_entities_basic(&extract_visible_xml_text(si_inner.as_bytes()))
            } else {
                parts.join("")
            }
        })
        .collect()
}

fn parse_ods_rows(
    raw_zip: &[u8],
    limits: &ResourceLimits,
    ctx: &JobContext,
) -> FileFusionResult<Vec<Vec<String>>> {
    let entries = read_validated_zip_entries(raw_zip, limits, ctx)?;
    reject_xml_entity_bombs(&entries)?;
    let Some(content_xml) = find_zip_entry_text(&entries, "content.xml") else {
        return Err(FileFusionError::new(
            ErrorCode::FmtCorrupt,
            "ODS container is missing content.xml",
        ));
    };

    let mut rows = Vec::new();
    for row_inner in iter_xml_blocks(content_xml, "table:table-row") {
        let mut cells = Vec::new();
        for (attrs, cell_inner) in iter_xml_tags_with_attrs(row_inner, "table:table-cell") {
            let repeat = extract_attr_value(attrs, "table:number-columns-repeated")
                .and_then(|s| s.parse::<usize>().ok())
                .unwrap_or(1)
                .min(256);
            let txt = iter_xml_blocks(cell_inner, "text:p")
                .map(decode_xml_entities_basic)
                .collect::<Vec<_>>()
                .join(" ");
            let val = if txt.is_empty() {
                extract_attr_value(attrs, "office:value").unwrap_or_default()
            } else {
                txt
            };
            for _ in 0..repeat {
                cells.push(val.clone());
            }
        }
        while cells.last().is_some_and(String::is_empty) {
            cells.pop();
        }
        if !cells.is_empty() {
            rows.push(cells);
        }
    }
    Ok(rows)
}

fn value_to_string_matrix(val: &Value) -> Vec<Vec<String>> {
    match val {
        Value::Array(items) => {
            if items.is_empty() {
                return Vec::new();
            }
            if items.iter().all(Value::is_object) {
                let mut cols: Vec<String> = Vec::new();
                for item in items {
                    if let Some(map) = item.as_object() {
                        for k in map.keys() {
                            if !cols.contains(k) {
                                cols.push(k.clone());
                            }
                        }
                    }
                }
                cols.sort();
                let mut rows = vec![cols.clone()];
                for item in items {
                    let row = cols
                        .iter()
                        .map(|c| {
                            item.as_object()
                                .and_then(|m| m.get(c))
                                .map(json_scalar_string)
                                .unwrap_or_default()
                        })
                        .collect();
                    rows.push(row);
                }
                return rows;
            }
            let mut rows = vec![vec![String::from("value")]];
            for item in items {
                rows.push(vec![json_scalar_string(item)]);
            }
            rows
        }
        Value::Object(map) => {
            let mut rows = vec![vec![String::from("key"), String::from("value")]];
            for (k, v) in map {
                rows.push(vec![k.clone(), json_scalar_string(v)]);
            }
            rows
        }
        other => vec![vec![String::from("value")], vec![json_scalar_string(other)]],
    }
}

fn json_scalar_string(val: &Value) -> String {
    match val {
        Value::Null => String::new(),
        Value::Bool(b) => b.to_string(),
        Value::Number(n) => n.to_string(),
        Value::String(s) => s.clone(),
        Value::Array(_) | Value::Object(_) => serde_json::to_string(val).unwrap_or_default(),
    }
}

fn matrix_to_csv_string(rows: &[Vec<String>]) -> String {
    let mut out = String::new();
    for row in rows {
        let cells: Vec<String> = row
            .iter()
            .map(|cell| {
                if cell.contains([',', '"', '\n', '\r']) {
                    format!("\"{}\"", cell.replace('"', "\"\""))
                } else {
                    cell.clone()
                }
            })
            .collect();
        out.push_str(&cells.join(","));
        out.push('\n');
    }
    out
}

fn build_xlsx_container(
    rows: &[Vec<String>],
    strip_metadata: bool,
    ctx: &JobContext,
) -> FileFusionResult<Vec<u8>> {
    let content_types = "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\
<Types xmlns=\"http://schemas.openxmlformats.org/package/2006/content-types\">\
<Default Extension=\"rels\" ContentType=\"application/vnd.openxmlformats-package.relationships+xml\"/>\
<Default Extension=\"xml\" ContentType=\"application/xml\"/>\
<Override PartName=\"/xl/workbook.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.spreadsheetml.sheet.main+xml\"/>\
<Override PartName=\"/xl/worksheets/sheet1.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.spreadsheetml.worksheet+xml\"/>\
</Types>";
    let root_rels = "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\
<Relationships xmlns=\"http://schemas.openxmlformats.org/package/2006/relationships\">\
<Relationship Id=\"rId1\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument\" Target=\"xl/workbook.xml\"/>\
</Relationships>";
    let workbook = "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\
<workbook xmlns=\"http://schemas.openxmlformats.org/spreadsheetml/2006/main\" \
xmlns:r=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships\">\
<sheets><sheet name=\"Sheet1\" sheetId=\"1\" r:id=\"rId1\"/></sheets>\
</workbook>";
    let wb_rels = "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\
<Relationships xmlns=\"http://schemas.openxmlformats.org/package/2006/relationships\">\
<Relationship Id=\"rId1\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/worksheet\" Target=\"worksheets/sheet1.xml\"/>\
</Relationships>";

    let mut sheet = String::from(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\
<worksheet xmlns=\"http://schemas.openxmlformats.org/spreadsheetml/2006/main\"><sheetData>",
    );
    for (r_idx, row) in rows.iter().enumerate() {
        let r_num = r_idx + 1;
        let _ = write!(sheet, "<row r=\"{r_num}\">");
        for (c_idx, val) in row.iter().enumerate() {
            let col_ref = format!("{}{r_num}", excel_col_name(c_idx));
            if let Ok(num) = val.trim().parse::<f64>() {
                if num.is_finite() && !val.trim().starts_with('0') {
                    let _ = write!(sheet, "<c r=\"{col_ref}\"><v>{}</v></c>", val.trim());
                    continue;
                }
            }
            let _ = write!(
                sheet,
                "<c r=\"{col_ref}\" t=\"inlineStr\"><is><t>{}</t></is></c>",
                escape_html_text(val)
            );
        }
        sheet.push_str("</row>");
    }
    sheet.push_str("</sheetData></worksheet>");

    let mut entries = vec![
        (
            String::from("[Content_Types].xml"),
            content_types.as_bytes().to_vec(),
        ),
        (String::from("_rels/.rels"), root_rels.as_bytes().to_vec()),
        (
            String::from("xl/workbook.xml"),
            workbook.as_bytes().to_vec(),
        ),
        (
            String::from("xl/_rels/workbook.xml.rels"),
            wb_rels.as_bytes().to_vec(),
        ),
        (String::from("xl/worksheets/sheet1.xml"), sheet.into_bytes()),
    ];
    if !strip_metadata {
        entries.push((
            String::from("docProps/core.xml"),
            b"<?xml version=\"1.0\" encoding=\"UTF-8\"?><cp:coreProperties xmlns:cp=\"http://schemas.openxmlformats.org/package/2006/metadata/core-properties\"/>".to_vec(),
        ));
    }
    write_office_container_entries(&entries, FormatId::Xlsx, ctx)
}

fn build_ods_container(
    rows: &[Vec<String>],
    strip_metadata: bool,
    ctx: &JobContext,
) -> FileFusionResult<Vec<u8>> {
    let mut content = String::from(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\
<office:document-content xmlns:office=\"urn:oasis:names:tc:opendocument:xmlns:office:1.0\" \
xmlns:table=\"urn:oasis:names:tc:opendocument:xmlns:table:1.0\" \
xmlns:text=\"urn:oasis:names:tc:opendocument:xmlns:text:1.0\" office:version=\"1.2\">\
<office:body><office:spreadsheet><table:table table:name=\"Sheet1\">",
    );
    for row in rows {
        content.push_str("<table:table-row>");
        for val in row {
            if let Ok(num) = val.trim().parse::<f64>() {
                if num.is_finite() {
                    let _ = write!(
                        content,
                        "<table:table-cell office:value-type=\"float\" office:value=\"{}\"><text:p>{}</text:p></table:table-cell>",
                        val.trim(),
                        escape_html_text(val.trim())
                    );
                    continue;
                }
            }
            let _ = write!(
                content,
                "<table:table-cell office:value-type=\"string\"><text:p>{}</text:p></table:table-cell>",
                escape_html_text(val)
            );
        }
        content.push_str("</table:table-row>");
    }
    content.push_str("</table:table></office:spreadsheet></office:body></office:document-content>");

    let manifest = "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\
<manifest:manifest xmlns:manifest=\"urn:oasis:names:tc:opendocument:xmlns:manifest:1.0\">\
<manifest:file-entry manifest:full-path=\"/\" manifest:media-type=\"application/vnd.oasis.opendocument.spreadsheet\"/>\
<manifest:file-entry manifest:full-path=\"content.xml\" manifest:media-type=\"text/xml\"/>\
</manifest:manifest>";

    let mut entries = vec![
        (
            String::from("mimetype"),
            b"application/vnd.oasis.opendocument.spreadsheet".to_vec(),
        ),
        (
            String::from("META-INF/manifest.xml"),
            manifest.as_bytes().to_vec(),
        ),
        (String::from("content.xml"), content.into_bytes()),
    ];
    if !strip_metadata {
        entries.push((
            String::from("meta.xml"),
            b"<?xml version=\"1.0\" encoding=\"UTF-8\"?><office:document-meta xmlns:office=\"urn:oasis:names:tc:opendocument:xmlns:office:1.0\"/>".to_vec(),
        ));
    }
    write_office_container_entries(&entries, FormatId::Ods, ctx)
}

// =========================================================================
// DOCX, ODT & EPUB Parsers / Builders
// =========================================================================

fn parse_docx_document(
    raw_zip: &[u8],
    limits: &ResourceLimits,
    ctx: &JobContext,
) -> FileFusionResult<MarkupDocument> {
    let entries = read_validated_zip_entries(raw_zip, limits, ctx)?;
    reject_xml_entity_bombs(&entries)?;
    let Some(doc_xml) = find_zip_entry_text(&entries, "word/document.xml") else {
        return Err(FileFusionError::new(
            ErrorCode::FmtCorrupt,
            "DOCX container is missing word/document.xml",
        ));
    };

    let rels_map = find_zip_entry_text(&entries, "word/_rels/document.xml.rels")
        .map(parse_rels_targets)
        .unwrap_or_default();

    let body = iter_xml_blocks(doc_xml, "w:body").next().unwrap_or(doc_xml);
    let mut blocks = Vec::new();
    let mut title = None;

    for tbl_xml in iter_xml_blocks(body, "w:tbl") {
        if let Some(table) = parse_docx_table(tbl_xml) {
            blocks.push(table);
        }
    }
    for p_xml in iter_xml_blocks(body, "w:p") {
        if let Some(block) = parse_docx_paragraph(p_xml, &rels_map, &mut title) {
            blocks.push(block);
        }
    }

    Ok(MarkupDocument { title, blocks })
}

fn parse_docx_paragraph(
    p_xml: &str,
    rels_map: &HashMap<String, String>,
    title: &mut Option<String>,
) -> Option<BlockNode> {
    let heading_level = iter_xml_tags_with_attrs(p_xml, "w:pStyle")
        .find_map(|(attrs, _)| extract_attr_value(attrs, "w:val"))
        .and_then(|val| parse_docx_heading_level(&val));

    let is_list = p_xml.contains("<w:numPr");
    let mut inlines = Vec::new();

    for (attrs, hl_inner) in iter_xml_tags_with_attrs(p_xml, "w:hyperlink") {
        let rid = extract_attr_value(attrs, "r:id").unwrap_or_default();
        let href = rels_map
            .get(&rid)
            .cloned()
            .unwrap_or_else(|| String::from("#"));
        let txt = iter_xml_blocks(hl_inner, "w:t")
            .map(decode_xml_entities_basic)
            .collect::<String>();
        if !txt.is_empty() {
            inlines.push(InlineNode::Link { text: txt, href });
        }
    }

    let direct_text = strip_tag_blocks(p_xml, "w:hyperlink");
    for run_inner in iter_xml_blocks(&direct_text, "w:r") {
        let txt = iter_xml_blocks(run_inner, "w:t")
            .map(decode_xml_entities_basic)
            .collect::<String>();
        if txt.is_empty() {
            continue;
        }
        let bold = run_inner.contains("<w:b/>") || run_inner.contains("<w:b ");
        let italic = run_inner.contains("<w:i/>") || run_inner.contains("<w:i ");
        if bold {
            inlines.push(InlineNode::Strong(txt));
        } else if italic {
            inlines.push(InlineNode::Emphasis(txt));
        } else {
            inlines.push(InlineNode::Text(txt));
        }
    }

    if inlines.is_empty() {
        return None;
    }
    if let Some(level) = heading_level {
        if title.is_none() && level == 1 {
            *title = Some(flatten_inlines_plain(&inlines));
        }
        return Some(BlockNode::Heading { level, inlines });
    }
    if is_list {
        return Some(BlockNode::List {
            ordered: false,
            items: vec![inlines],
        });
    }
    Some(BlockNode::Paragraph(inlines))
}

fn parse_docx_table(tbl_xml: &str) -> Option<BlockNode> {
    let mut all_rows: Vec<Vec<Vec<InlineNode>>> = Vec::new();
    for tr_inner in iter_xml_blocks(tbl_xml, "w:tr") {
        let mut cells = Vec::new();
        for tc_inner in iter_xml_blocks(tr_inner, "w:tc") {
            let txt = iter_xml_blocks(tc_inner, "w:t")
                .map(decode_xml_entities_basic)
                .collect::<String>();
            cells.push(vec![InlineNode::Text(txt)]);
        }
        if !cells.is_empty() {
            all_rows.push(cells);
        }
    }
    if all_rows.is_empty() {
        None
    } else {
        let headers = all_rows.remove(0);
        Some(BlockNode::Table {
            headers,
            rows: all_rows,
        })
    }
}

fn parse_odt_document(
    raw_zip: &[u8],
    limits: &ResourceLimits,
    ctx: &JobContext,
) -> FileFusionResult<MarkupDocument> {
    let entries = read_validated_zip_entries(raw_zip, limits, ctx)?;
    reject_xml_entity_bombs(&entries)?;
    let Some(content_xml) = find_zip_entry_text(&entries, "content.xml") else {
        return Err(FileFusionError::new(
            ErrorCode::FmtCorrupt,
            "ODT container is missing content.xml",
        ));
    };

    let mut blocks = Vec::new();
    let mut title = None;

    for (attrs, h_inner) in iter_xml_tags_with_attrs(content_xml, "text:h") {
        let level = extract_attr_value(attrs, "text:outline-level")
            .and_then(|s| s.parse::<u8>().ok())
            .unwrap_or(1)
            .clamp(1, 6);
        let txt = decode_xml_entities_basic(&extract_visible_xml_text(h_inner.as_bytes()));
        if !txt.is_empty() {
            if title.is_none() && level == 1 {
                title = Some(txt.clone());
            }
            blocks.push(BlockNode::Heading {
                level,
                inlines: vec![InlineNode::Text(txt)],
            });
        }
    }
    for tbl_inner in iter_xml_blocks(content_xml, "table:table") {
        let mut all_rows: Vec<Vec<Vec<InlineNode>>> = Vec::new();
        for tr_inner in iter_xml_blocks(tbl_inner, "table:table-row") {
            let cells: Vec<Vec<InlineNode>> = iter_xml_blocks(tr_inner, "table:table-cell")
                .map(|c| {
                    vec![InlineNode::Text(decode_xml_entities_basic(
                        &extract_visible_xml_text(c.as_bytes()),
                    ))]
                })
                .collect();
            if !cells.is_empty() {
                all_rows.push(cells);
            }
        }
        if !all_rows.is_empty() {
            let headers = all_rows.remove(0);
            blocks.push(BlockNode::Table {
                headers,
                rows: all_rows,
            });
        }
    }
    for p_inner in iter_xml_blocks(content_xml, "text:p") {
        let mut inlines = Vec::new();
        for (attrs, a_inner) in iter_xml_tags_with_attrs(p_inner, "text:a") {
            let href = extract_attr_value(attrs, "xlink:href").unwrap_or_default();
            let txt = decode_xml_entities_basic(&extract_visible_xml_text(a_inner.as_bytes()));
            if !txt.is_empty() {
                inlines.push(InlineNode::Link { text: txt, href });
            }
        }
        let plain = decode_xml_entities_basic(&extract_visible_xml_text(
            strip_tag_blocks(p_inner, "text:a").as_bytes(),
        ));
        if !plain.trim().is_empty() {
            inlines.insert(0, InlineNode::Text(plain.trim().to_owned()));
        }
        if !inlines.is_empty() {
            blocks.push(BlockNode::Paragraph(inlines));
        }
    }

    Ok(MarkupDocument { title, blocks })
}

fn parse_epub_document(
    raw_zip: &[u8],
    limits: &ResourceLimits,
    ctx: &JobContext,
) -> FileFusionResult<MarkupDocument> {
    let entries = read_validated_zip_entries(raw_zip, limits, ctx)?;
    reject_xml_entity_bombs(&entries)?;
    let opf_path = find_zip_entry_text(&entries, "META-INF/container.xml")
        .and_then(|c| {
            iter_xml_tags_with_attrs(c, "rootfile")
                .find_map(|(attrs, _)| extract_attr_value(attrs, "full-path"))
        })
        .unwrap_or_else(|| String::from("OEBPS/content.opf"));

    let opf_dir = opf_path.rsplit_once('/').map_or("", |(dir, _)| dir);

    let mut manifest = HashMap::new();
    let mut spine_ids = Vec::new();
    let mut title = None;

    if let Some(opf_xml) = find_zip_entry_text(&entries, &opf_path) {
        if let Some(dc_title) = iter_xml_blocks(opf_xml, "dc:title").next() {
            let t = decode_xml_entities_basic(dc_title).trim().to_owned();
            if !t.is_empty() {
                title = Some(t);
            }
        }
        for (attrs, _) in iter_xml_tags_with_attrs(opf_xml, "item") {
            if let (Some(id), Some(href)) = (
                extract_attr_value(attrs, "id"),
                extract_attr_value(attrs, "href"),
            ) {
                let resolved = if opf_dir.is_empty() {
                    href
                } else {
                    format!("{opf_dir}/{href}")
                };
                manifest.insert(id, resolved);
            }
        }
        for (attrs, _) in iter_xml_tags_with_attrs(opf_xml, "itemref") {
            if let Some(idref) = extract_attr_value(attrs, "idref") {
                spine_ids.push(idref);
            }
        }
    }

    let mut blocks = Vec::new();
    for idref in spine_ids {
        let Some(path) = manifest.get(&idref) else {
            continue;
        };
        if path.ends_with("nav.xhtml") {
            continue;
        }
        if let Some(xhtml) = find_zip_entry_text(&entries, path) {
            let parsed = parse_markup_document(xhtml, FormatId::Html)?;
            if title.is_none() {
                title = parsed.title;
            }
            blocks.extend(parsed.blocks);
        }
    }

    if blocks.is_empty() {
        for (name, bytes) in &entries {
            let is_html_entry = std::path::Path::new(name).extension().is_some_and(|ext| {
                ext.eq_ignore_ascii_case("xhtml") || ext.eq_ignore_ascii_case("html")
            });
            let is_nav = name.to_ascii_lowercase().ends_with("nav.xhtml");
            if is_html_entry && !is_nav {
                if let Ok(xhtml) = std::str::from_utf8(bytes) {
                    blocks.extend(parse_markup_document(xhtml, FormatId::Html)?.blocks);
                }
            }
        }
    }

    Ok(MarkupDocument { title, blocks })
}

fn build_docx_container(
    doc: &MarkupDocument,
    strip_metadata: bool,
    ctx: &JobContext,
) -> FileFusionResult<Vec<u8>> {
    let mut body = String::new();
    let mut links: Vec<(String, String)> = Vec::new();

    for block in &doc.blocks {
        match block {
            BlockNode::Heading { level, inlines } => {
                let lvl = (*level).clamp(1, 6);
                let _ = write!(
                    body,
                    "<w:p><w:pPr><w:pStyle w:val=\"Heading{lvl}\"/></w:pPr>{}</w:p>",
                    render_docx_inlines(inlines, &mut links)
                );
            }
            BlockNode::Paragraph(inlines) | BlockNode::BlockQuote(inlines) => {
                let _ = write!(
                    body,
                    "<w:p>{}</w:p>",
                    render_docx_inlines(inlines, &mut links)
                );
            }
            BlockNode::CodeBlock { code, .. } => {
                let _ = write!(
                    body,
                    "<w:p><w:r><w:t xml:space=\"preserve\">{}</w:t></w:r></w:p>",
                    escape_html_text(code)
                );
            }
            BlockNode::List { items, .. } => {
                for item in items {
                    let _ = write!(
                        body,
                        "<w:p><w:pPr><w:numPr><w:ilvl w:val=\"0\"/><w:numId w:val=\"1\"/></w:numPr></w:pPr>{}</w:p>",
                        render_docx_inlines(item, &mut links)
                    );
                }
            }
            BlockNode::Table { headers, rows } => {
                body.push_str("<w:tbl>");
                append_docx_table_row(&mut body, headers, &mut links);
                for row in rows {
                    append_docx_table_row(&mut body, row, &mut links);
                }
                body.push_str("</w:tbl>");
            }
            BlockNode::ThematicBreak => {
                body.push_str("<w:p><w:r><w:t>---</w:t></w:r></w:p>");
            }
        }
    }

    let document_xml = format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\
<w:document xmlns:w=\"http://schemas.openxmlformats.org/wordprocessingml/2006/main\" \
xmlns:r=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships\">\
<w:body>{body}<w:sectPr/></w:body></w:document>"
    );

    let mut rels_xml = String::from(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\
<Relationships xmlns=\"http://schemas.openxmlformats.org/package/2006/relationships\">",
    );
    for (rid, href) in &links {
        let _ = write!(
            rels_xml,
            "<Relationship Id=\"{rid}\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/hyperlink\" Target=\"{}\" TargetMode=\"External\"/>",
            escape_html_text(href)
        );
    }
    rels_xml.push_str("</Relationships>");

    let content_types = "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\
<Types xmlns=\"http://schemas.openxmlformats.org/package/2006/content-types\">\
<Default Extension=\"rels\" ContentType=\"application/vnd.openxmlformats-package.relationships+xml\"/>\
<Default Extension=\"xml\" ContentType=\"application/xml\"/>\
<Override PartName=\"/word/document.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml\"/>\
</Types>";
    let root_rels = "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\
<Relationships xmlns=\"http://schemas.openxmlformats.org/package/2006/relationships\">\
<Relationship Id=\"rId1\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument\" Target=\"word/document.xml\"/>\
</Relationships>";

    let mut entries = vec![
        (
            String::from("[Content_Types].xml"),
            content_types.as_bytes().to_vec(),
        ),
        (String::from("_rels/.rels"), root_rels.as_bytes().to_vec()),
        (String::from("word/document.xml"), document_xml.into_bytes()),
        (
            String::from("word/_rels/document.xml.rels"),
            rels_xml.into_bytes(),
        ),
    ];
    if !strip_metadata {
        entries.push((
            String::from("docProps/core.xml"),
            b"<?xml version=\"1.0\" encoding=\"UTF-8\"?><cp:coreProperties xmlns:cp=\"http://schemas.openxmlformats.org/package/2006/metadata/core-properties\"/>".to_vec(),
        ));
    }
    write_office_container_entries(&entries, FormatId::Docx, ctx)
}

fn append_docx_table_row(
    body: &mut String,
    cells: &[Vec<InlineNode>],
    links: &mut Vec<(String, String)>,
) {
    body.push_str("<w:tr>");
    for cell in cells {
        let _ = write!(
            body,
            "<w:tc><w:p>{}</w:p></w:tc>",
            render_docx_inlines(cell, links)
        );
    }
    body.push_str("</w:tr>");
}

fn render_docx_inlines(inlines: &[InlineNode], links: &mut Vec<(String, String)>) -> String {
    let mut out = String::new();
    for node in inlines {
        match node {
            InlineNode::Text(t) | InlineNode::Code(t) => {
                let _ = write!(
                    out,
                    "<w:r><w:t xml:space=\"preserve\">{}</w:t></w:r>",
                    escape_html_text(t)
                );
            }
            InlineNode::Strong(t) => {
                let _ = write!(
                    out,
                    "<w:r><w:rPr><w:b/></w:rPr><w:t xml:space=\"preserve\">{}</w:t></w:r>",
                    escape_html_text(t)
                );
            }
            InlineNode::Emphasis(t) => {
                let _ = write!(
                    out,
                    "<w:r><w:rPr><w:i/></w:rPr><w:t xml:space=\"preserve\">{}</w:t></w:r>",
                    escape_html_text(t)
                );
            }
            InlineNode::Link { text, href } => {
                let rid = format!("rIdLink{}", links.len() + 1);
                links.push((rid.clone(), href.clone()));
                let _ = write!(
                    out,
                    "<w:hyperlink r:id=\"{rid}\"><w:r><w:t xml:space=\"preserve\">{}</w:t></w:r></w:hyperlink>",
                    escape_html_text(text)
                );
            }
            InlineNode::Image { alt, .. } => {
                let _ = write!(
                    out,
                    "<w:r><w:t>[image: {}]</w:t></w:r>",
                    escape_html_text(alt)
                );
            }
            InlineNode::LineBreak => out.push_str("<w:r><w:br/></w:r>"),
        }
    }
    out
}

fn build_odt_container(
    doc: &MarkupDocument,
    strip_metadata: bool,
    ctx: &JobContext,
) -> FileFusionResult<Vec<u8>> {
    let mut body = String::new();
    for block in &doc.blocks {
        match block {
            BlockNode::Heading { level, inlines } => {
                let lvl = (*level).clamp(1, 6);
                let _ = write!(
                    body,
                    "<text:h text:outline-level=\"{lvl}\">{}</text:h>",
                    render_odt_inlines(inlines)
                );
            }
            BlockNode::Paragraph(inlines) | BlockNode::BlockQuote(inlines) => {
                let _ = write!(body, "<text:p>{}</text:p>", render_odt_inlines(inlines));
            }
            BlockNode::CodeBlock { code, .. } => {
                let _ = write!(body, "<text:p>{}</text:p>", escape_html_text(code));
            }
            BlockNode::List { items, .. } => {
                body.push_str("<text:list>");
                for item in items {
                    let _ = write!(
                        body,
                        "<text:list-item><text:p>{}</text:p></text:list-item>",
                        render_odt_inlines(item)
                    );
                }
                body.push_str("</text:list>");
            }
            BlockNode::Table { headers, rows } => {
                body.push_str("<table:table>");
                append_odt_row(&mut body, headers);
                for row in rows {
                    append_odt_row(&mut body, row);
                }
                body.push_str("</table:table>");
            }
            BlockNode::ThematicBreak => body.push_str("<text:p>---</text:p>"),
        }
    }

    let content_xml = format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\
<office:document-content xmlns:office=\"urn:oasis:names:tc:opendocument:xmlns:office:1.0\" \
xmlns:text=\"urn:oasis:names:tc:opendocument:xmlns:text:1.0\" \
xmlns:table=\"urn:oasis:names:tc:opendocument:xmlns:table:1.0\" \
xmlns:xlink=\"http://www.w3.org/1999/xlink\" office:version=\"1.2\">\
<office:body><office:text>{body}</office:text></office:body></office:document-content>"
    );
    let manifest = "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\
<manifest:manifest xmlns:manifest=\"urn:oasis:names:tc:opendocument:xmlns:manifest:1.0\">\
<manifest:file-entry manifest:full-path=\"/\" manifest:media-type=\"application/vnd.oasis.opendocument.text\"/>\
<manifest:file-entry manifest:full-path=\"content.xml\" manifest:media-type=\"text/xml\"/>\
</manifest:manifest>";

    let mut entries = vec![
        (
            String::from("mimetype"),
            b"application/vnd.oasis.opendocument.text".to_vec(),
        ),
        (
            String::from("META-INF/manifest.xml"),
            manifest.as_bytes().to_vec(),
        ),
        (String::from("content.xml"), content_xml.into_bytes()),
    ];
    if !strip_metadata {
        entries.push((
            String::from("meta.xml"),
            b"<?xml version=\"1.0\" encoding=\"UTF-8\"?><office:document-meta xmlns:office=\"urn:oasis:names:tc:opendocument:xmlns:office:1.0\"/>".to_vec(),
        ));
    }
    write_office_container_entries(&entries, FormatId::Odt, ctx)
}

fn append_odt_row(body: &mut String, cells: &[Vec<InlineNode>]) {
    body.push_str("<table:table-row>");
    for cell in cells {
        let _ = write!(
            body,
            "<table:table-cell><text:p>{}</text:p></table:table-cell>",
            render_odt_inlines(cell)
        );
    }
    body.push_str("</table:table-row>");
}

fn render_odt_inlines(inlines: &[InlineNode]) -> String {
    let mut out = String::new();
    for node in inlines {
        match node {
            InlineNode::Text(t)
            | InlineNode::Strong(t)
            | InlineNode::Emphasis(t)
            | InlineNode::Code(t) => out.push_str(&escape_html_text(t)),
            InlineNode::Link { text, href } => {
                let _ = write!(
                    out,
                    "<text:a xlink:type=\"simple\" xlink:href=\"{}\">{}</text:a>",
                    escape_html_text(href),
                    escape_html_text(text)
                );
            }
            InlineNode::Image { alt, .. } => {
                let _ = write!(out, "[image: {}]", escape_html_text(alt));
            }
            InlineNode::LineBreak => out.push_str("<text:line-break/>"),
        }
    }
    out
}

fn build_epub_container(
    doc: &MarkupDocument,
    strip_metadata: bool,
    ctx: &JobContext,
) -> FileFusionResult<Vec<u8>> {
    let title = doc.title.as_deref().unwrap_or("FileFusion Document");
    let body_html = render_markup_document(doc, FormatId::Html, strip_metadata)?;
    let chapter_xhtml = format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n\
<!DOCTYPE html>\n\
<html xmlns=\"http://www.w3.org/1999/xhtml\" lang=\"en\">\n\
<head><meta charset=\"utf-8\"/><title>{}</title></head>\n{}\n</html>\n",
        escape_html_text(title),
        extract_body_fragment(&body_html)
    );

    let mut toc_items = String::new();
    let mut heading_idx = 0usize;
    for block in &doc.blocks {
        if let BlockNode::Heading { inlines, .. } = block {
            heading_idx += 1;
            let label = flatten_inlines_plain(inlines);
            let _ = writeln!(
                toc_items,
                "      <li><a href=\"chapter1.xhtml\">{}. {}</a></li>",
                heading_idx,
                escape_html_text(label.trim())
            );
        }
    }
    if toc_items.is_empty() {
        let _ = writeln!(
            toc_items,
            "      <li><a href=\"chapter1.xhtml\">{}</a></li>",
            escape_html_text(title)
        );
    }

    let nav_xhtml = format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n\
<!DOCTYPE html>\n\
<html xmlns=\"http://www.w3.org/1999/xhtml\" xmlns:epub=\"http://www.idpf.org/2007/ops\">\n\
<head><meta charset=\"utf-8\"/><title>Table of Contents</title></head>\n\
<body>\n  <nav epub:type=\"toc\" id=\"toc\">\n    <h1>Table of Contents</h1>\n    <ol>\n{toc_items}    </ol>\n  </nav>\n</body>\n</html>\n"
    );

    let creator_meta = if strip_metadata {
        String::new()
    } else {
        String::from("\n    <dc:creator>FileFusion</dc:creator>")
    };

    let opf = format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n\
<package xmlns=\"http://www.idpf.org/2007/opf\" version=\"3.0\" unique-identifier=\"bookid\">\n\
  <metadata xmlns:dc=\"http://purl.org/dc/elements/1.1/\">\n\
    <dc:identifier id=\"bookid\">urn:uuid:filefusion-epub3</dc:identifier>\n\
    <dc:title>{}</dc:title>\n\
    <dc:language>en</dc:language>{creator_meta}\n\
  </metadata>\n\
  <manifest>\n\
    <item id=\"nav\" href=\"nav.xhtml\" media-type=\"application/xhtml+xml\" properties=\"nav\"/>\n\
    <item id=\"chapter1\" href=\"chapter1.xhtml\" media-type=\"application/xhtml+xml\"/>\n\
  </manifest>\n\
  <spine>\n\
    <itemref idref=\"chapter1\"/>\n\
  </spine>\n\
</package>\n",
        escape_html_text(title)
    );

    let container_xml = "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n\
<container version=\"1.0\" xmlns=\"urn:oasis:names:tc:opendocument:xmlns:container\">\n\
  <rootfiles>\n\
    <rootfile full-path=\"OEBPS/content.opf\" media-type=\"application/oebps-package+xml\"/>\n\
  </rootfiles>\n\
</container>\n";

    let entries = vec![
        (String::from("mimetype"), b"application/epub+zip".to_vec()),
        (
            String::from("META-INF/container.xml"),
            container_xml.as_bytes().to_vec(),
        ),
        (String::from("OEBPS/content.opf"), opf.into_bytes()),
        (String::from("OEBPS/nav.xhtml"), nav_xhtml.into_bytes()),
        (
            String::from("OEBPS/chapter1.xhtml"),
            chapter_xhtml.into_bytes(),
        ),
    ];
    write_office_container_entries(&entries, FormatId::Epub, ctx)
}

fn extract_body_fragment(full_html: &str) -> &str {
    let lower = full_html.to_ascii_lowercase();
    if let (Some(start), Some(end)) = (lower.find("<body>"), lower.rfind("</body>")) {
        &full_html[start..end + 7]
    } else {
        full_html
    }
}

// =========================================================================
// Lightweight Security-Hardened XML Scanning Helpers
// =========================================================================

fn reject_xml_entity_bombs(entries: &[(String, Vec<u8>)]) -> FileFusionResult<()> {
    for (name, bytes) in entries {
        if is_optimizable_xml_entry(name) {
            if let Ok(text) = std::str::from_utf8(bytes) {
                let low = text.to_ascii_lowercase();
                if low.contains("<!entity") {
                    return Err(FileFusionError::new(
                        ErrorCode::SecBomb,
                        format!(
                            "XML DTD ENTITY declaration blocked in entry `{name}` (NFR-SEC-001)"
                        ),
                    ));
                }
            }
        }
    }
    Ok(())
}

fn iter_xml_blocks<'a>(xml: &'a str, tag: &str) -> impl Iterator<Item = &'a str> {
    let open_prefix = format!("<{tag}");
    let close_tag = format!("</{tag}>");
    let mut pos = 0usize;
    std::iter::from_fn(move || {
        while pos < xml.len() {
            let rel = xml[pos..].find(&open_prefix)?;
            let start = pos + rel;
            let after_name = xml[start + open_prefix.len()..].chars().next()?;
            if after_name != '>' && !after_name.is_whitespace() && after_name != '/' {
                pos = start + open_prefix.len();
                continue;
            }
            let gt_rel = xml[start..].find('>')?;
            if xml[start..start + gt_rel].trim_end().ends_with('/') {
                pos = start + gt_rel + 1;
                return Some("");
            }
            let inner_start = start + gt_rel + 1;
            let close_rel = xml[inner_start..].find(&close_tag)?;
            pos = inner_start + close_rel + close_tag.len();
            return Some(&xml[inner_start..inner_start + close_rel]);
        }
        None
    })
}

fn iter_xml_tags_with_attrs<'a>(
    xml: &'a str,
    tag: &str,
) -> impl Iterator<Item = (&'a str, &'a str)> {
    let open_prefix = format!("<{tag}");
    let close_tag = format!("</{tag}>");
    let mut pos = 0usize;
    std::iter::from_fn(move || {
        while pos < xml.len() {
            let rel = xml[pos..].find(&open_prefix)?;
            let start = pos + rel;
            let after_name = xml[start + open_prefix.len()..].chars().next()?;
            if after_name != '>' && !after_name.is_whitespace() && after_name != '/' {
                pos = start + open_prefix.len();
                continue;
            }
            let gt_rel = xml[start..].find('>')?;
            let attrs = xml[start + open_prefix.len()..start + gt_rel]
                .trim()
                .trim_end_matches('/');
            if xml[start..start + gt_rel].trim_end().ends_with('/') {
                pos = start + gt_rel + 1;
                return Some((attrs, ""));
            }
            let inner_start = start + gt_rel + 1;
            let close_rel = xml[inner_start..].find(&close_tag)?;
            pos = inner_start + close_rel + close_tag.len();
            return Some((attrs, &xml[inner_start..inner_start + close_rel]));
        }
        None
    })
}

fn extract_attr_value(attrs: &str, key: &str) -> Option<String> {
    for quote in ['"', '\''] {
        let pat = format!("{key}={quote}");
        if let Some(pos) = attrs.find(&pat) {
            let rest = &attrs[pos + pat.len()..];
            if let Some(end) = rest.find(quote) {
                return Some(decode_xml_entities_basic(&rest[..end]));
            }
        }
    }
    None
}

fn strip_tag_blocks(xml: &str, tag: &str) -> String {
    let mut out = xml.to_owned();
    let open_prefix = format!("<{tag}");
    let close_tag = format!("</{tag}>");
    while let Some(start) = out.find(&open_prefix) {
        if let Some(close_rel) = out[start..].find(&close_tag) {
            out.replace_range(start..start + close_rel + close_tag.len(), "");
        } else {
            break;
        }
    }
    out
}

fn parse_docx_heading_level(style_val: &str) -> Option<u8> {
    let lower = style_val.to_ascii_lowercase();
    lower
        .strip_prefix("heading")
        .and_then(|s| s.trim().parse::<u8>().ok())
        .map(|lvl| lvl.clamp(1, 6))
}

fn parse_rels_targets(rels_xml: &str) -> HashMap<String, String> {
    let mut map = HashMap::new();
    for (attrs, _) in iter_xml_tags_with_attrs(rels_xml, "Relationship") {
        if let (Some(id), Some(target)) = (
            extract_attr_value(attrs, "Id"),
            extract_attr_value(attrs, "Target"),
        ) {
            map.insert(id, target);
        }
    }
    map
}

fn col_index_from_cell_ref(cell_ref: &str) -> Option<usize> {
    let letters: String = cell_ref
        .chars()
        .take_while(char::is_ascii_alphabetic)
        .collect();
    if letters.is_empty() {
        return None;
    }
    let mut col = 0usize;
    for b in letters.bytes() {
        let upper = b.to_ascii_uppercase();
        if !upper.is_ascii_uppercase() {
            return None;
        }
        col = col * 26 + usize::from(upper - b'A' + 1);
    }
    col.checked_sub(1)
}

fn excel_col_name(mut col_idx: usize) -> String {
    let mut chars = Vec::new();
    loop {
        let rem = col_idx % 26;
        chars.push((b'A' + u8::try_from(rem).unwrap_or(0)) as char);
        if col_idx < 26 {
            break;
        }
        col_idx = (col_idx / 26) - 1;
    }
    chars.into_iter().rev().collect()
}

fn decode_xml_entities_basic(input: &str) -> String {
    input
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&apos;", "'")
        .replace("&amp;", "&")
}

/// Keeps `serde_json` helpers active cleanly across all build modes.
#[allow(dead_code)]
fn _unused_map_guard(_m: Map<String, Value>) {}
