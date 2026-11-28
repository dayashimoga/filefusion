//! Lossless `XML` / `XHTML` structural whitespace & comment optimization, office/`EPUB`
//! privacy metadata scrubbing (`FR-MET-002`), and visible text extraction for fidelity checks.

use std::fmt::Write as _;
use std::path::Path;

/// Returns `true` if `entry_path` identifies an `XML`, `XHTML`, relationships (`.rels`), or
/// `OPF`/`NCX` markup file eligible for lossless structural optimization.
#[must_use]
pub fn is_optimizable_xml_entry(entry_path: &str) -> bool {
    Path::new(entry_path).extension().is_some_and(|ext| {
        ext.eq_ignore_ascii_case("xml")
            || ext.eq_ignore_ascii_case("rels")
            || ext.eq_ignore_ascii_case("xhtml")
            || ext.eq_ignore_ascii_case("opf")
            || ext.eq_ignore_ascii_case("ncx")
    })
}

/// Losslessly strips non-conditional `<!-- ... -->` XML comments and redundant inter-tag
/// formatting whitespace (`>\s+<`) while strictly preserving CDATA, `xml:space="preserve"` blocks,
/// and text elements (`<w:t>`, `<t>`, `<a:t>`, `<text:p>`, `<pre>`, `<code>`, `<script>`, `<style>`).
///
/// Falls back to returning `raw` unchanged if `raw` is not valid UTF-8.
#[must_use]
pub fn minify_xml_bytes(raw: &[u8]) -> Vec<u8> {
    let Ok(text) = std::str::from_utf8(raw) else {
        return raw.to_vec();
    };
    if text.contains("xml:space=\"preserve\"") || text.contains("xml:space='preserve'") {
        return strip_safe_xml_comments_only(text).into_bytes();
    }

    let without_comments = strip_safe_xml_comments_only(text);
    let mut out = String::with_capacity(without_comments.len());
    let chars: Vec<char> = without_comments.chars().collect();
    let mut i = 0usize;
    let mut in_preserve_tag = false;

    while i < chars.len() {
        if chars[i] == '<' {
            let start = i;
            while i < chars.len() && chars[i] != '>' {
                i += 1;
            }
            if i < chars.len() {
                i += 1;
            }
            let tag: String = chars[start..i].iter().collect();
            let lower_tag = tag.to_ascii_lowercase();
            if is_opening_preserve_tag(&lower_tag) && !lower_tag.ends_with("/>") {
                in_preserve_tag = true;
            } else if is_closing_preserve_tag(&lower_tag) {
                in_preserve_tag = false;
            }
            out.push_str(&tag);
        } else {
            let start = i;
            while i < chars.len() && chars[i] != '<' {
                i += 1;
            }
            let text_run: String = chars[start..i].iter().collect();
            if in_preserve_tag || !text_run.chars().all(char::is_whitespace) {
                out.push_str(&text_run);
            }
        }
    }

    let candidate = out.into_bytes();
    if candidate.len() <= raw.len() {
        candidate
    } else {
        raw.to_vec()
    }
}

/// Scrubs identifying author, editor, revision, company, and generator tags from `OOXML`
/// (`docProps/core.xml`, `app.xml`, `custom.xml`), `ODF` (`meta.xml`), and `EPUB` (`.opf`) metadata.
#[must_use]
pub fn scrub_metadata_xml(entry_path: &str, raw: &[u8]) -> Option<Vec<u8>> {
    let lower = entry_path.to_ascii_lowercase();
    if lower == "docprops/custom.xml" {
        return Some(
            br#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><Properties xmlns="http://schemas.openxmlformats.org/officeDocument/2006/custom-properties"></Properties>"#
                .to_vec(),
        );
    }

    let Ok(text) = std::str::from_utf8(raw) else {
        return None;
    };

    if lower == "docprops/core.xml" {
        let mut scrubbed = text.to_owned();
        for tag in [
            "dc:title",
            "dc:subject",
            "dc:creator",
            "dc:description",
            "cp:keywords",
            "cp:lastModifiedBy",
            "cp:category",
        ] {
            scrubbed = clear_simple_xml_element(&scrubbed, tag);
        }
        scrubbed = replace_simple_xml_element(&scrubbed, "cp:revision", "1");
        return Some(scrubbed.into_bytes());
    }

    if lower == "docprops/app.xml" {
        let mut scrubbed = text.to_owned();
        for tag in ["Company", "Manager", "Template"] {
            scrubbed = clear_simple_xml_element(&scrubbed, tag);
        }
        return Some(scrubbed.into_bytes());
    }

    if lower == "meta.xml" {
        let mut scrubbed = text.to_owned();
        for tag in [
            "meta:initial-creator",
            "dc:creator",
            "meta:printed-by",
            "dc:title",
            "dc:subject",
            "dc:description",
            "meta:keyword",
        ] {
            scrubbed = clear_simple_xml_element(&scrubbed, tag);
        }
        return Some(scrubbed.into_bytes());
    }

    if Path::new(entry_path)
        .extension()
        .is_some_and(|ext| ext.eq_ignore_ascii_case("opf"))
    {
        // Keep mandatory EPUB metadata (`dc:title`, `dc:identifier`, `dc:language`), scrub optional PII.
        let mut scrubbed = clear_simple_xml_element(text, "dc:contributor");
        scrubbed = clear_simple_xml_element(&scrubbed, "dc:creator");
        return Some(scrubbed.into_bytes());
    }

    None
}

/// Extracts visible plain text across `OOXML`/`ODF`/`EPUB` XML/XHTML entries for fidelity audits.
#[must_use]
pub fn extract_visible_xml_text(raw: &[u8]) -> String {
    let Ok(text) = std::str::from_utf8(raw) else {
        return String::new();
    };
    let mut out = String::new();
    let mut in_tag = false;
    for ch in text.chars() {
        match ch {
            '<' => in_tag = true,
            '>' => {
                in_tag = false;
                if !out.ends_with(' ') && !out.is_empty() {
                    out.push(' ');
                }
            }
            c if !in_tag => {
                if !c.is_whitespace() {
                    out.push(c);
                } else if !out.ends_with(' ') && !out.is_empty() {
                    out.push(' ');
                }
            }
            _ => {}
        }
    }
    let mut normalized = String::new();
    for (idx, word) in out.split_whitespace().enumerate() {
        if idx > 0 {
            normalized.push(' ');
        }
        let _ = write!(normalized, "{word}");
    }
    normalized
}

fn strip_safe_xml_comments_only(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(pos) = rest.find("<!--") {
        let (head, tail) = rest.split_at(pos);
        out.push_str(head);
        // Preserve IE/Office conditional comments (`<!--[if ...]>`) cleanly.
        if tail.starts_with("<!--[if") {
            out.push_str("<!--");
            rest = &tail[4..];
            continue;
        }
        if let Some(end_rel) = tail.find("-->") {
            rest = &tail[end_rel + 3..];
        } else {
            out.push_str(tail);
            return out;
        }
    }
    out.push_str(rest);
    out
}

fn is_opening_preserve_tag(lower_tag: &str) -> bool {
    [
        "<w:t ",
        "<w:t>",
        "<t ",
        "<t>",
        "<a:t ",
        "<a:t>",
        "<text:p",
        "<text:h",
        "<text:span",
        "<pre",
        "<code",
        "<textarea",
        "<script",
        "<style",
    ]
    .iter()
    .any(|prefix| lower_tag.starts_with(prefix))
}

fn is_closing_preserve_tag(lower_tag: &str) -> bool {
    [
        "</w:t>",
        "</t>",
        "</a:t>",
        "</text:p>",
        "</text:h>",
        "</text:span>",
        "</pre>",
        "</code>",
        "</textarea>",
        "</script>",
        "</style>",
    ]
    .iter()
    .any(|prefix| lower_tag.starts_with(prefix))
}

fn clear_simple_xml_element(xml: &str, tag: &str) -> String {
    replace_simple_xml_element(xml, tag, "")
}

fn replace_simple_xml_element(xml: &str, tag: &str, new_inner: &str) -> String {
    let open_prefix = format!("<{tag}");
    let close_tag = format!("</{tag}>");
    let mut result = String::with_capacity(xml.len());
    let mut cursor = xml;

    while let Some(start_idx) = cursor.find(&open_prefix) {
        let (before, after_prefix) = cursor.split_at(start_idx);
        result.push_str(before);
        let Some(open_end_rel) = after_prefix.find('>') else {
            result.push_str(after_prefix);
            return result;
        };
        let after_open = &after_prefix[open_end_rel + 1..];
        let open_slice = &after_prefix[..=open_end_rel];
        // Self-closing tag (`<dc:creator/>`) -> emit unchanged.
        if open_slice.ends_with("/>") {
            result.push_str(open_slice);
            cursor = after_open;
            continue;
        }
        // Only match exact tag boundary (`<tag>` or `<tag ...>`), not `<tagExtra>`.
        let next_char = after_prefix[open_prefix.len()..].chars().next();
        if !matches!(next_char, Some('>' | ' ' | '\t' | '\r' | '\n')) {
            result.push_str(&open_prefix);
            cursor = &after_prefix[open_prefix.len()..];
            continue;
        }
        if let Some(close_rel) = after_open.find(&close_tag) {
            result.push_str(open_slice);
            result.push_str(new_inner);
            result.push_str(&close_tag);
            cursor = &after_open[close_rel + close_tag.len()..];
        } else {
            result.push_str(after_prefix);
            return result;
        }
    }
    result.push_str(cursor);
    result
}
