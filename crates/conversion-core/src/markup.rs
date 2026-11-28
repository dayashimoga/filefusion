//! Pure-Rust bidirectional structured markup and document conversion engine covering
//! `Markdown` (CommonMark/GFM), `HTML`, and `PlainText` (`FR-DOC-001`, `FR-FID-001`, `NFR-SEC-001`).

use std::fmt::Write as _;

use file_core::{ErrorCode, FileFusionError, FileFusionResult, FormatId};

/// Inline text run within a parsed [`BlockNode`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InlineNode {
    /// Unformatted text span.
    Text(String),
    /// Strong emphasis (`**bold**` / `<strong>`).
    Strong(String),
    /// Emphasis (`*italic*` / `<em>`).
    Emphasis(String),
    /// Inline monospace code (`` `code` `` / `<code>`).
    Code(String),
    /// Hyperlink (`[label](href)` / `<a href="...">label</a>`).
    Link {
        /// Visible anchor text.
        text: String,
        /// Sanitized link destination URL.
        href: String,
    },
    /// Embedded image reference (`![alt](src)` / `<img src="..." alt="...">`).
    Image {
        /// Alternative text description.
        alt: String,
        /// Sanitized image source URI or relative path.
        src: String,
    },
    /// Explicit line break (`<br>`).
    LineBreak,
}

/// Block-level semantic node in a structured markup document.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BlockNode {
    /// Section heading (`1..=6`).
    Heading {
        /// Heading level (`1..=6`).
        level: u8,
        /// Inline heading runs.
        inlines: Vec<InlineNode>,
    },
    /// Body paragraph.
    Paragraph(Vec<InlineNode>),
    /// Block quotation (`>` / `<blockquote>`).
    BlockQuote(Vec<InlineNode>),
    /// Fenced or preformatted code block.
    CodeBlock {
        /// Optional language hint (e.g. `rust`, `json`).
        language: Option<String>,
        /// Raw verbatim code content.
        code: String,
    },
    /// Ordered or unordered list of inline-run items.
    List {
        /// `true` for numbered `<ol>` lists, `false` for bulleted `<ul>` lists.
        ordered: bool,
        /// Item contents.
        items: Vec<Vec<InlineNode>>,
    },
    /// Data table with header cells and body rows.
    Table {
        /// Column headers.
        headers: Vec<Vec<InlineNode>>,
        /// Data rows matching column count where possible.
        rows: Vec<Vec<Vec<InlineNode>>>,
    },
    /// Horizontal thematic divider (`---` / `<hr>`).
    ThematicBreak,
}

/// Canonical intermediate representation for structured markup conversions.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct MarkupDocument {
    /// Optional document title inferred from `<title>` or first top-level heading.
    pub title: Option<String>,
    /// Ordered top-level semantic blocks.
    pub blocks: Vec<BlockNode>,
}

/// Converts text between `Markdown`, `Html`, and `PlainText`, enforcing active XSS sanitization
/// when parsing untrusted HTML (`NFR-SEC-001`).
///
/// # Errors
/// Returns [`FileFusionError`] if either `source` or `target` is outside `{Markdown, Html, PlainText}`.
pub fn convert_markup_text(
    input: &str,
    source: FormatId,
    target: FormatId,
    strip_metadata: bool,
) -> FileFusionResult<String> {
    let doc = parse_markup_document(input, source)?;
    render_markup_document(&doc, target, strip_metadata)
}

/// Parses `Markdown`, `Html`, or `PlainText` into a canonical [`MarkupDocument`].
///
/// # Errors
/// Returns [`FileFusionError`] if `format` is unsupported by the markup parser.
pub fn parse_markup_document(input: &str, format: FormatId) -> FileFusionResult<MarkupDocument> {
    match format {
        FormatId::Markdown => Ok(parse_markdown(input)),
        FormatId::Html => Ok(parse_html(input)),
        FormatId::PlainText => Ok(parse_plain_text(input)),
        other => Err(FileFusionError::new(
            ErrorCode::FmtUnsupported,
            format!("markup parser does not handle format {other:?}"),
        )),
    }
}

/// Renders a [`MarkupDocument`] into `Markdown`, `Html`, or `PlainText`.
///
/// # Errors
/// Returns [`FileFusionError`] if `target` is outside `{Markdown, Html, PlainText}`.
pub fn render_markup_document(
    doc: &MarkupDocument,
    target: FormatId,
    strip_metadata: bool,
) -> FileFusionResult<String> {
    match target {
        FormatId::Markdown => Ok(render_markdown(doc)),
        FormatId::Html => Ok(render_html(doc, strip_metadata)),
        FormatId::PlainText => Ok(render_plain_text(doc)),
        other => Err(FileFusionError::new(
            ErrorCode::FmtUnsupported,
            format!("markup renderer does not support target {other:?}"),
        )),
    }
}

/// Wraps a structured tabular header + row matrix cleanly into a [`BlockNode::Table`] so tabular
/// formats (`CSV`/`JSON`/`YAML`/`XML`) can export directly to `Markdown`, `HTML`, or `PlainText`.
#[must_use]
pub fn document_from_table(
    title: Option<String>,
    headers: Vec<String>,
    rows: Vec<Vec<String>>,
) -> MarkupDocument {
    let hdr_nodes = headers
        .into_iter()
        .map(|h| vec![InlineNode::Text(h)])
        .collect();
    let row_nodes = rows
        .into_iter()
        .map(|r| r.into_iter().map(|c| vec![InlineNode::Text(c)]).collect())
        .collect();
    let mut blocks = Vec::new();
    if let Some(ref t) = title {
        blocks.push(BlockNode::Heading {
            level: 1,
            inlines: vec![InlineNode::Text(t.clone())],
        });
    }
    blocks.push(BlockNode::Table {
        headers: hdr_nodes,
        rows: row_nodes,
    });
    MarkupDocument { title, blocks }
}

fn parse_plain_text(input: &str) -> MarkupDocument {
    let normalized = input.replace("\r\n", "\n").replace('\r', "\n");
    let mut blocks = Vec::new();
    let mut para_lines: Vec<String> = Vec::new();

    for line in normalized.lines() {
        if line.trim().is_empty() {
            if !para_lines.is_empty() {
                blocks.push(BlockNode::Paragraph(vec![InlineNode::Text(
                    para_lines.join(" "),
                )]));
                para_lines.clear();
            }
        } else {
            para_lines.push(line.trim().to_owned());
        }
    }
    if !para_lines.is_empty() {
        blocks.push(BlockNode::Paragraph(vec![InlineNode::Text(
            para_lines.join(" "),
        )]));
    }

    MarkupDocument {
        title: None,
        blocks,
    }
}

fn parse_markdown(input: &str) -> MarkupDocument {
    let normalized = input.replace("\r\n", "\n").replace('\r', "\n");
    let lines: Vec<&str> = normalized.lines().collect();
    let mut blocks = Vec::new();
    let mut title = None;
    let mut i = 0usize;

    while i < lines.len() {
        let trimmed = lines[i].trim();
        if trimmed.is_empty() {
            i += 1;
            continue;
        }
        if let Some((block, next_i)) = try_parse_md_fenced_or_hr(trimmed, &lines, i) {
            blocks.push(block);
            i = next_i;
            continue;
        }
        if let Some((block, inferred_title, next_i)) = try_parse_md_heading(trimmed, &lines, i) {
            if title.is_none() {
                title = inferred_title;
            }
            blocks.push(block);
            i = next_i;
            continue;
        }
        if let Some((block, next_i)) = try_parse_md_structured_block(trimmed, &lines, i) {
            blocks.push(block);
            i = next_i;
            continue;
        }

        let mut para = vec![trimmed];
        i += 1;
        while i < lines.len() {
            let nt = lines[i].trim();
            if nt.is_empty()
                || nt.starts_with("```")
                || parse_atx_heading(nt).is_some()
                || is_md_hr(nt)
                || nt.starts_with('>')
                || parse_md_list_line(nt).is_some()
                || (nt.starts_with('|')
                    && i + 1 < lines.len()
                    && is_md_table_separator(lines[i + 1]))
            {
                break;
            }
            para.push(nt);
            i += 1;
        }
        blocks.push(BlockNode::Paragraph(parse_md_inlines(&para.join(" "))));
    }

    MarkupDocument { title, blocks }
}

fn try_parse_md_fenced_or_hr(
    trimmed: &str,
    lines: &[&str],
    mut i: usize,
) -> Option<(BlockNode, usize)> {
    if let Some(lang_raw) = trimmed.strip_prefix("```") {
        let language = {
            let s = lang_raw.trim();
            if s.is_empty() {
                None
            } else {
                Some(s.to_owned())
            }
        };
        i += 1;
        let mut code_lines = Vec::new();
        while i < lines.len() && !lines[i].trim_start().starts_with("```") {
            code_lines.push(lines[i]);
            i += 1;
        }
        if i < lines.len() {
            i += 1;
        }
        return Some((
            BlockNode::CodeBlock {
                language,
                code: code_lines.join("\n"),
            },
            i,
        ));
    }
    if is_md_hr(trimmed) {
        return Some((BlockNode::ThematicBreak, i + 1));
    }
    None
}

fn try_parse_md_heading(
    trimmed: &str,
    lines: &[&str],
    i: usize,
) -> Option<(BlockNode, Option<String>, usize)> {
    if let Some((level, content)) = parse_atx_heading(trimmed) {
        let inlines = parse_md_inlines(content);
        let title = (level == 1).then(|| flatten_inlines_plain(&inlines));
        return Some((BlockNode::Heading { level, inlines }, title, i + 1));
    }
    if i + 1 < lines.len() {
        let next_trimmed = lines[i + 1].trim();
        if is_setext_underline(next_trimmed, '=') {
            let inlines = parse_md_inlines(trimmed);
            let title = Some(flatten_inlines_plain(&inlines));
            return Some((BlockNode::Heading { level: 1, inlines }, title, i + 2));
        }
        if is_setext_underline(next_trimmed, '-') {
            return Some((
                BlockNode::Heading {
                    level: 2,
                    inlines: parse_md_inlines(trimmed),
                },
                None,
                i + 2,
            ));
        }
    }
    None
}

fn try_parse_md_structured_block(
    trimmed: &str,
    lines: &[&str],
    mut i: usize,
) -> Option<(BlockNode, usize)> {
    if trimmed.starts_with('|') && i + 1 < lines.len() && is_md_table_separator(lines[i + 1]) {
        let headers = split_md_table_row(trimmed)
            .into_iter()
            .map(|c| parse_md_inlines(&c))
            .collect();
        i += 2;
        let mut rows = Vec::new();
        while i < lines.len() && lines[i].trim().starts_with('|') {
            let cells = split_md_table_row(lines[i].trim())
                .into_iter()
                .map(|c| parse_md_inlines(&c))
                .collect();
            rows.push(cells);
            i += 1;
        }
        return Some((BlockNode::Table { headers, rows }, i));
    }
    if trimmed.starts_with('>') {
        let mut quote_parts = Vec::new();
        while i < lines.len() && lines[i].trim_start().starts_with('>') {
            let stripped = lines[i].trim_start().trim_start_matches('>').trim_start();
            if !stripped.is_empty() {
                quote_parts.push(stripped);
            }
            i += 1;
        }
        return Some((
            BlockNode::BlockQuote(parse_md_inlines(&quote_parts.join(" "))),
            i,
        ));
    }
    if let Some((ordered, first_item)) = parse_md_list_line(trimmed) {
        let mut items = vec![parse_md_inlines(first_item)];
        i += 1;
        while i < lines.len() {
            let next_trim = lines[i].trim();
            if let Some((next_ord, item_txt)) = parse_md_list_line(next_trim) {
                if next_ord == ordered {
                    items.push(parse_md_inlines(item_txt));
                    i += 1;
                    continue;
                }
            }
            break;
        }
        return Some((BlockNode::List { ordered, items }, i));
    }
    None
}

fn is_md_hr(trimmed: &str) -> bool {
    let compact: String = trimmed.chars().filter(|c| !c.is_whitespace()).collect();
    if compact.len() < 3 {
        return false;
    }
    compact.chars().all(|c| c == '-')
        || compact.chars().all(|c| c == '*')
        || compact.chars().all(|c| c == '_')
}

fn is_setext_underline(trimmed: &str, ch: char) -> bool {
    !trimmed.is_empty() && trimmed.chars().all(|c| c == ch)
}

fn parse_atx_heading(trimmed: &str) -> Option<(u8, &str)> {
    let hashes = trimmed.bytes().take_while(|&b| b == b'#').count();
    if hashes == 0 || hashes > 6 {
        return None;
    }
    let rest = trimmed[hashes..].strip_prefix(' ')?;
    let level = u8::try_from(hashes).ok()?;
    Some((level, rest.trim()))
}

fn is_md_table_separator(line: &str) -> bool {
    let trimmed = line.trim();
    if !trimmed.starts_with('|') || !trimmed.contains("---") {
        return false;
    }
    trimmed
        .chars()
        .all(|c| matches!(c, '|' | '-' | ':' | ' ' | '\t'))
}

fn split_md_table_row(line: &str) -> Vec<String> {
    let inner = line
        .trim()
        .strip_prefix('|')
        .unwrap_or(line.trim())
        .strip_suffix('|')
        .unwrap_or(line.trim().strip_prefix('|').unwrap_or(line.trim()));
    inner.split('|').map(|c| c.trim().to_owned()).collect()
}

fn parse_md_list_line(trimmed: &str) -> Option<(bool, &str)> {
    for prefix in ["- ", "* ", "+ "] {
        if let Some(rest) = trimmed.strip_prefix(prefix) {
            return Some((false, rest.trim()));
        }
    }
    let digits = trimmed.bytes().take_while(u8::is_ascii_digit).count();
    if digits > 0 {
        let after = &trimmed[digits..];
        if let Some(rest) = after.strip_prefix(". ") {
            return Some((true, rest.trim()));
        }
    }
    None
}

fn parse_md_inlines(text: &str) -> Vec<InlineNode> {
    let mut nodes = Vec::new();
    let mut buf = String::new();
    let bytes = text.as_bytes();
    let mut i = 0usize;

    let flush_text = |buf: &mut String, nodes: &mut Vec<InlineNode>| {
        if !buf.is_empty() {
            nodes.push(InlineNode::Text(std::mem::take(buf)));
        }
    };

    while i < bytes.len() {
        let slice = &text[i..];

        if let Some(after_img) = slice.strip_prefix("![") {
            if let Some(alt_end) = after_img.find("](") {
                let url_slice = &after_img[alt_end + 2..];
                if let Some(url_end) = url_slice.find(')') {
                    flush_text(&mut buf, &mut nodes);
                    let alt = after_img[..alt_end].to_owned();
                    let src = sanitize_url(&url_slice[..url_end]);
                    nodes.push(InlineNode::Image { alt, src });
                    i += 2 + alt_end + 2 + url_end + 1;
                    continue;
                }
            }
        }

        if let Some(after_brk) = slice.strip_prefix('[') {
            if let Some(txt_end) = after_brk.find("](") {
                let url_slice = &after_brk[txt_end + 2..];
                if let Some(url_end) = url_slice.find(')') {
                    flush_text(&mut buf, &mut nodes);
                    let link_text = after_brk[..txt_end].to_owned();
                    let href = sanitize_url(&url_slice[..url_end]);
                    nodes.push(InlineNode::Link {
                        text: link_text,
                        href,
                    });
                    i += 1 + txt_end + 2 + url_end + 1;
                    continue;
                }
            }
        }

        if let Some(after) = slice.strip_prefix('`') {
            if let Some(end) = after.find('`') {
                flush_text(&mut buf, &mut nodes);
                nodes.push(InlineNode::Code(after[..end].to_owned()));
                i += 1 + end + 1;
                continue;
            }
        }

        if let Some(after) = slice.strip_prefix("**") {
            if let Some(end) = after.find("**") {
                flush_text(&mut buf, &mut nodes);
                nodes.push(InlineNode::Strong(after[..end].to_owned()));
                i += 2 + end + 2;
                continue;
            }
        }

        if let Some(after) = slice.strip_prefix('*') {
            if let Some(end) = after.find('*') {
                if !after[..end].is_empty() {
                    flush_text(&mut buf, &mut nodes);
                    nodes.push(InlineNode::Emphasis(after[..end].to_owned()));
                    i += 1 + end + 1;
                    continue;
                }
            }
        }

        if let Some(ch) = slice.chars().next() {
            buf.push(ch);
            i += ch.len_utf8();
        } else {
            break;
        }
    }

    flush_text(&mut buf, &mut nodes);
    if nodes.is_empty() {
        nodes.push(InlineNode::Text(String::new()));
    }
    nodes
}

fn sanitize_url(raw: &str) -> String {
    let trimmed = raw.trim();
    let lower = trimmed.to_ascii_lowercase();
    if lower.starts_with("javascript:")
        || lower.starts_with("vbscript:")
        || lower.starts_with("data:text/html")
    {
        String::from("#blocked")
    } else {
        trimmed.to_owned()
    }
}

fn parse_html(input: &str) -> MarkupDocument {
    let cleaned = strip_html_danger_subtrees(input);
    let mut title = extract_html_tag_inner(&cleaned, "title")
        .map(|s| decode_html_entities(s.trim()))
        .filter(|s| !s.is_empty());

    let body = extract_html_tag_inner(&cleaned, "body").unwrap_or(&cleaned);
    let tokens = tokenize_html(body);
    let blocks = parse_html_tokens(&tokens);

    if title.is_none() {
        for b in &blocks {
            if let BlockNode::Heading { level: 1, inlines } = b {
                title = Some(flatten_inlines_plain(inlines));
                break;
            }
        }
    }

    MarkupDocument { title, blocks }
}

fn strip_html_danger_subtrees(input: &str) -> String {
    let mut cur = input.to_owned();
    for tag in ["script", "style", "iframe", "object", "embed"] {
        loop {
            let lower = cur.to_ascii_lowercase();
            let open_pat = format!("<{tag}");
            let close_pat = format!("</{tag}>");
            let Some(start) = lower.find(&open_pat) else {
                break;
            };
            let rest_lower = &lower[start..];
            let end = if let Some(close_rel) = rest_lower.find(&close_pat) {
                start + close_rel + close_pat.len()
            } else if let Some(tag_end) = rest_lower.find('>') {
                start + tag_end + 1
            } else {
                cur.len()
            };
            cur.replace_range(start..end, "");
        }
    }
    cur
}

fn extract_html_tag_inner<'a>(html: &'a str, tag: &str) -> Option<&'a str> {
    let lower = html.to_ascii_lowercase();
    let open = format!("<{tag}");
    let close = format!("</{tag}>");
    let start_pos = lower.find(&open)?;
    let gt_rel = lower[start_pos..].find('>')?;
    let content_start = start_pos + gt_rel + 1;
    let close_rel = lower[content_start..].find(&close)?;
    Some(&html[content_start..content_start + close_rel])
}

#[derive(Debug, Clone)]
enum HtmlTok {
    Open {
        name: String,
        attrs: Vec<(String, String)>,
        self_closing: bool,
    },
    Close(String),
    Text(String),
}

fn tokenize_html(html: &str) -> Vec<HtmlTok> {
    let mut tokens = Vec::new();
    let mut pos = 0usize;

    while pos < html.len() {
        if let Some(lt_rel) = html[pos..].find('<') {
            if lt_rel > 0 {
                let raw_txt = &html[pos..pos + lt_rel];
                let decoded = decode_html_entities(raw_txt);
                if !decoded.is_empty() {
                    tokens.push(HtmlTok::Text(decoded));
                }
            }
            let tag_start = pos + lt_rel + 1;
            let Some(gt_rel) = html[tag_start..].find('>') else {
                break;
            };
            let raw_tag = html[tag_start..tag_start + gt_rel].trim();
            pos = tag_start + gt_rel + 1;

            if raw_tag.starts_with('!') || raw_tag.starts_with('?') {
                continue;
            }
            if let Some(closing) = raw_tag.strip_prefix('/') {
                let name = closing
                    .split_whitespace()
                    .next()
                    .unwrap_or("")
                    .to_ascii_lowercase();
                if !name.is_empty() {
                    tokens.push(HtmlTok::Close(name));
                }
                continue;
            }

            let self_closing = raw_tag.ends_with('/');
            let inner = raw_tag.trim_end_matches('/').trim();
            let (name, attr_slice) = split_tag_name_and_attrs(inner);
            if !name.is_empty() {
                tokens.push(HtmlTok::Open {
                    name,
                    attrs: parse_html_attrs(attr_slice),
                    self_closing,
                });
            }
        } else {
            let decoded = decode_html_entities(&html[pos..]);
            if !decoded.is_empty() {
                tokens.push(HtmlTok::Text(decoded));
            }
            break;
        }
    }

    tokens
}

fn split_tag_name_and_attrs(inner: &str) -> (String, &str) {
    for (idx, ch) in inner.char_indices() {
        if ch.is_whitespace() {
            return (inner[..idx].to_ascii_lowercase(), &inner[idx..]);
        }
    }
    (inner.to_ascii_lowercase(), "")
}

fn parse_html_attrs(raw: &str) -> Vec<(String, String)> {
    let mut attrs = Vec::new();
    let chars: Vec<char> = raw.chars().collect();
    let mut i = 0usize;

    while i < chars.len() {
        while i < chars.len() && chars[i].is_whitespace() {
            i += 1;
        }
        let k_start = i;
        while i < chars.len() && !chars[i].is_whitespace() && chars[i] != '=' {
            i += 1;
        }
        let key: String = chars[k_start..i].iter().collect();
        let key_lower = key.to_ascii_lowercase();
        while i < chars.len() && chars[i].is_whitespace() {
            i += 1;
        }
        if i >= chars.len() || chars[i] != '=' {
            if !key_lower.is_empty() && !key_lower.starts_with("on") {
                attrs.push((key_lower, String::new()));
            }
            continue;
        }
        i += 1; // skip '='
        while i < chars.len() && chars[i].is_whitespace() {
            i += 1;
        }
        let val = if i < chars.len() && (chars[i] == '"' || chars[i] == '\'') {
            let quote = chars[i];
            i += 1;
            let v_start = i;
            while i < chars.len() && chars[i] != quote {
                i += 1;
            }
            let s: String = chars[v_start..i].iter().collect();
            if i < chars.len() {
                i += 1;
            }
            s
        } else {
            let v_start = i;
            while i < chars.len() && !chars[i].is_whitespace() {
                i += 1;
            }
            chars[v_start..i].iter().collect()
        };
        if !key_lower.is_empty() && !key_lower.starts_with("on") {
            attrs.push((key_lower, decode_html_entities(&val)));
        }
    }
    attrs
}

fn find_attr(attrs: &[(String, String)], key: &str) -> Option<String> {
    attrs.iter().find(|(k, _)| k == key).map(|(_, v)| v.clone())
}

fn parse_html_tokens(tokens: &[HtmlTok]) -> Vec<BlockNode> {
    let mut blocks = Vec::new();
    let mut i = 0usize;

    while i < tokens.len() {
        match &tokens[i] {
            HtmlTok::Open { name, .. }
                if matches!(name.as_str(), "h1" | "h2" | "h3" | "h4" | "h5" | "h6") =>
            {
                let level = name[1..].parse::<u8>().unwrap_or(1);
                let (inner, next_i) = collect_until_close(tokens, i + 1, name);
                blocks.push(BlockNode::Heading {
                    level,
                    inlines: parse_html_inlines(inner),
                });
                i = next_i;
            }
            HtmlTok::Open { name, .. } if name == "p" => {
                let (inner, next_i) = collect_until_close(tokens, i + 1, "p");
                let inlines = parse_html_inlines(inner);
                if !inlines.is_empty() {
                    blocks.push(BlockNode::Paragraph(inlines));
                }
                i = next_i;
            }
            HtmlTok::Open { name, .. } if name == "blockquote" => {
                let (inner, next_i) = collect_until_close(tokens, i + 1, "blockquote");
                let inlines = parse_html_inlines(inner);
                if !inlines.is_empty() {
                    blocks.push(BlockNode::BlockQuote(inlines));
                }
                i = next_i;
            }
            HtmlTok::Open { name, .. } if name == "pre" => {
                let (inner, next_i) = collect_until_close(tokens, i + 1, "pre");
                let mut code = String::new();
                for tok in inner {
                    if let HtmlTok::Text(t) = tok {
                        code.push_str(t);
                    }
                }
                blocks.push(BlockNode::CodeBlock {
                    language: None,
                    code: code.trim_matches('\n').to_owned(),
                });
                i = next_i;
            }
            HtmlTok::Open { name, .. } if name == "ul" || name == "ol" => {
                let ordered = name == "ol";
                let (inner, next_i) = collect_until_close(tokens, i + 1, name);
                let items = parse_html_list_items(inner);
                if !items.is_empty() {
                    blocks.push(BlockNode::List { ordered, items });
                }
                i = next_i;
            }
            HtmlTok::Open { name, .. } if name == "table" => {
                let (inner, next_i) = collect_until_close(tokens, i + 1, "table");
                if let Some(tbl) = parse_html_table(inner) {
                    blocks.push(tbl);
                }
                i = next_i;
            }
            HtmlTok::Open { name, .. } if name == "hr" => {
                blocks.push(BlockNode::ThematicBreak);
                i += 1;
            }
            HtmlTok::Text(t) if t.trim().is_empty() => {
                i += 1;
            }
            HtmlTok::Close(_) => {
                i += 1;
            }
            _ => {
                let start = i;
                while i < tokens.len() && !is_block_opener(&tokens[i]) {
                    i += 1;
                }
                let inlines = parse_html_inlines(&tokens[start..i]);
                if !flatten_inlines_plain(&inlines).trim().is_empty()
                    || inlines
                        .iter()
                        .any(|n| matches!(n, InlineNode::Image { .. } | InlineNode::Link { .. }))
                {
                    blocks.push(BlockNode::Paragraph(inlines));
                }
                if i == start {
                    i += 1;
                }
            }
        }
    }

    blocks
}

fn is_block_opener(tok: &HtmlTok) -> bool {
    matches!(
        tok,
        HtmlTok::Open { name, .. }
            if matches!(
                name.as_str(),
                "h1" | "h2" | "h3" | "h4" | "h5" | "h6"
                    | "p"
                    | "blockquote"
                    | "pre"
                    | "ul"
                    | "ol"
                    | "table"
                    | "hr"
            )
    )
}

fn collect_until_close<'a>(
    tokens: &'a [HtmlTok],
    start: usize,
    tag_name: &str,
) -> (&'a [HtmlTok], usize) {
    let mut depth = 1usize;
    let mut i = start;
    while i < tokens.len() {
        match &tokens[i] {
            HtmlTok::Open {
                name,
                self_closing: false,
                ..
            } if name == tag_name => {
                depth += 1;
            }
            HtmlTok::Close(name) if name == tag_name => {
                depth = depth.saturating_sub(1);
                if depth == 0 {
                    return (&tokens[start..i], i + 1);
                }
            }
            _ => {}
        }
        i += 1;
    }
    (&tokens[start..], tokens.len())
}

fn parse_html_list_items(tokens: &[HtmlTok]) -> Vec<Vec<InlineNode>> {
    let mut items = Vec::new();
    let mut i = 0usize;
    while i < tokens.len() {
        if let HtmlTok::Open { name, .. } = &tokens[i] {
            if name == "li" {
                let (inner, next_i) = collect_until_close(tokens, i + 1, "li");
                items.push(parse_html_inlines(inner));
                i = next_i;
                continue;
            }
        }
        i += 1;
    }
    items
}

fn parse_html_table(tokens: &[HtmlTok]) -> Option<BlockNode> {
    let mut headers: Vec<Vec<InlineNode>> = Vec::new();
    let mut rows: Vec<Vec<Vec<InlineNode>>> = Vec::new();
    let mut i = 0usize;

    while i < tokens.len() {
        if let HtmlTok::Open { name, .. } = &tokens[i] {
            if name == "tr" {
                let (tr_inner, next_i) = collect_until_close(tokens, i + 1, "tr");
                let mut row_cells = Vec::new();
                let mut is_header_row = false;
                let mut j = 0usize;
                while j < tr_inner.len() {
                    if let HtmlTok::Open { name: cname, .. } = &tr_inner[j] {
                        if cname == "th" || cname == "td" {
                            if cname == "th" {
                                is_header_row = true;
                            }
                            let (cell_inner, next_j) = collect_until_close(tr_inner, j + 1, cname);
                            row_cells.push(parse_html_inlines(cell_inner));
                            j = next_j;
                            continue;
                        }
                    }
                    j += 1;
                }
                if !row_cells.is_empty() {
                    if is_header_row && headers.is_empty() {
                        headers = row_cells;
                    } else {
                        rows.push(row_cells);
                    }
                }
                i = next_i;
                continue;
            }
        }
        i += 1;
    }

    if headers.is_empty() && rows.is_empty() {
        None
    } else {
        if headers.is_empty() && !rows.is_empty() {
            headers = rows.remove(0);
        }
        Some(BlockNode::Table { headers, rows })
    }
}

fn parse_html_inlines(tokens: &[HtmlTok]) -> Vec<InlineNode> {
    let mut nodes = Vec::new();
    let mut i = 0usize;

    while i < tokens.len() {
        match &tokens[i] {
            HtmlTok::Text(t) => {
                let collapsed = collapse_html_whitespace(t);
                if !collapsed.is_empty() {
                    nodes.push(InlineNode::Text(collapsed));
                }
                i += 1;
            }
            HtmlTok::Open {
                name,
                attrs,
                self_closing,
            } => match name.as_str() {
                "br" => {
                    nodes.push(InlineNode::LineBreak);
                    i += 1;
                }
                "img" => {
                    let alt = find_attr(attrs, "alt").unwrap_or_default();
                    let src = sanitize_url(&find_attr(attrs, "src").unwrap_or_default());
                    nodes.push(InlineNode::Image { alt, src });
                    i += 1;
                }
                "a" if !self_closing => {
                    let href = sanitize_url(&find_attr(attrs, "href").unwrap_or_default());
                    let (inner, next_i) = collect_until_close(tokens, i + 1, "a");
                    let text = flatten_inlines_plain(&parse_html_inlines(inner))
                        .trim()
                        .to_owned();
                    nodes.push(InlineNode::Link { text, href });
                    i = next_i;
                }
                "strong" | "b" if !self_closing => {
                    let (inner, next_i) = collect_until_close(tokens, i + 1, name);
                    let txt = flatten_inlines_plain(&parse_html_inlines(inner));
                    nodes.push(InlineNode::Strong(txt));
                    i = next_i;
                }
                "em" | "i" if !self_closing => {
                    let (inner, next_i) = collect_until_close(tokens, i + 1, name);
                    let txt = flatten_inlines_plain(&parse_html_inlines(inner));
                    nodes.push(InlineNode::Emphasis(txt));
                    i = next_i;
                }
                "code" if !self_closing => {
                    let (inner, next_i) = collect_until_close(tokens, i + 1, "code");
                    let txt = flatten_inlines_plain(&parse_html_inlines(inner));
                    nodes.push(InlineNode::Code(txt));
                    i = next_i;
                }
                _ => {
                    i += 1;
                }
            },
            HtmlTok::Close(_) => {
                i += 1;
            }
        }
    }

    nodes
}

fn collapse_html_whitespace(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut prev_ws = false;
    for ch in text.chars() {
        if ch.is_whitespace() {
            if !prev_ws {
                out.push(' ');
                prev_ws = true;
            }
        } else {
            out.push(ch);
            prev_ws = false;
        }
    }
    out
}

fn decode_html_entities(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    let mut rest = input;
    while let Some(amp) = rest.find('&') {
        out.push_str(&rest[..amp]);
        let after_amp = &rest[amp + 1..];
        if let Some(semi) = after_amp.find(';').filter(|&pos| pos <= 10) {
            let entity = &after_amp[..semi];
            if let Some(ch) = resolve_html_entity(entity) {
                out.push(ch);
                rest = &after_amp[semi + 1..];
                continue;
            }
        }
        out.push('&');
        rest = after_amp;
    }
    out.push_str(rest);
    out
}

fn resolve_html_entity(entity: &str) -> Option<char> {
    match entity {
        "amp" => Some('&'),
        "lt" => Some('<'),
        "gt" => Some('>'),
        "quot" => Some('"'),
        "apos" | "#39" => Some('\''),
        "nbsp" => Some(' '),
        _ if entity.starts_with("#x") || entity.starts_with("#X") => {
            u32::from_str_radix(&entity[2..], 16)
                .ok()
                .and_then(char::from_u32)
        }
        _ if entity.starts_with('#') => entity[1..].parse::<u32>().ok().and_then(char::from_u32),
        _ => None,
    }
}

/// Escapes reserved XML/HTML character entities (`&`, `<`, `>`, `"`, `'`).
#[must_use]
pub fn escape_html_text(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    for ch in input.chars() {
        match ch {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            other => out.push(other),
        }
    }
    out
}

fn render_markdown(doc: &MarkupDocument) -> String {
    let mut out = String::new();
    for (idx, block) in doc.blocks.iter().enumerate() {
        if idx > 0 {
            out.push_str("\n\n");
        }
        match block {
            BlockNode::Heading { level, inlines } => {
                let hashes = "#".repeat(usize::from((*level).clamp(1, 6)));
                let _ = write!(out, "{hashes} {}", render_md_inlines(inlines).trim());
            }
            BlockNode::Paragraph(inlines) => {
                out.push_str(render_md_inlines(inlines).trim());
            }
            BlockNode::BlockQuote(inlines) => {
                let _ = write!(out, "> {}", render_md_inlines(inlines).trim());
            }
            BlockNode::CodeBlock { language, code } => {
                let lang = language.as_deref().unwrap_or("");
                let _ = write!(out, "```{lang}\n{code}\n```");
            }
            BlockNode::List { ordered, items } => {
                for (row_idx, item) in items.iter().enumerate() {
                    if row_idx > 0 {
                        out.push('\n');
                    }
                    if *ordered {
                        let _ = write!(out, "{}. {}", row_idx + 1, render_md_inlines(item).trim());
                    } else {
                        let _ = write!(out, "- {}", render_md_inlines(item).trim());
                    }
                }
            }
            BlockNode::Table { headers, rows } => {
                let cols = headers.len().max(1);
                let hdr_cells: Vec<String> = headers
                    .iter()
                    .map(|c| render_md_inlines(c).trim().replace('|', "\\|"))
                    .collect();
                let _ = write!(out, "| {} |\n|", hdr_cells.join(" | "));
                for _ in 0..cols {
                    out.push_str(" --- |");
                }
                for row in rows {
                    out.push_str("\n| ");
                    let cells: Vec<String> = (0..cols)
                        .map(|ci| {
                            row.get(ci)
                                .map(|c| render_md_inlines(c).trim().replace('|', "\\|"))
                                .unwrap_or_default()
                        })
                        .collect();
                    out.push_str(&cells.join(" | "));
                    out.push_str(" |");
                }
            }
            BlockNode::ThematicBreak => {
                out.push_str("---");
            }
        }
    }
    if !out.ends_with('\n') {
        out.push('\n');
    }
    out
}

fn render_md_inlines(inlines: &[InlineNode]) -> String {
    let mut out = String::new();
    for node in inlines {
        match node {
            InlineNode::Text(t) => out.push_str(t),
            InlineNode::Strong(t) => {
                let _ = write!(out, "**{t}**");
            }
            InlineNode::Emphasis(t) => {
                let _ = write!(out, "*{t}*");
            }
            InlineNode::Code(t) => {
                let _ = write!(out, "`{t}`");
            }
            InlineNode::Link { text, href } => {
                let _ = write!(out, "[{text}]({href})");
            }
            InlineNode::Image { alt, src } => {
                let _ = write!(out, "![{alt}]({src})");
            }
            InlineNode::LineBreak => out.push('\n'),
        }
    }
    out
}

fn render_html(doc: &MarkupDocument, strip_metadata: bool) -> String {
    let mut out =
        String::from("<!DOCTYPE html>\n<html lang=\"en\">\n<head>\n<meta charset=\"utf-8\">\n");
    let page_title = doc.title.as_deref().unwrap_or("Document");
    let _ = writeln!(out, "<title>{}</title>", escape_html_text(page_title));
    if !strip_metadata {
        out.push_str("<meta name=\"generator\" content=\"FileFusion\">\n");
    }
    out.push_str("</head>\n<body>\n");

    for block in &doc.blocks {
        match block {
            BlockNode::Heading { level, inlines } => {
                let lvl = (*level).clamp(1, 6);
                let _ = writeln!(
                    out,
                    "<h{lvl}>{}</h{lvl}>",
                    render_html_inlines(inlines).trim()
                );
            }
            BlockNode::Paragraph(inlines) => {
                let _ = writeln!(out, "<p>{}</p>", render_html_inlines(inlines).trim());
            }
            BlockNode::BlockQuote(inlines) => {
                let _ = writeln!(
                    out,
                    "<blockquote>{}</blockquote>",
                    render_html_inlines(inlines).trim()
                );
            }
            BlockNode::CodeBlock { language, code } => {
                if let Some(lang) = language {
                    let _ = writeln!(
                        out,
                        "<pre><code class=\"language-{}\">{}</code></pre>",
                        escape_html_text(lang),
                        escape_html_text(code)
                    );
                } else {
                    let _ = writeln!(out, "<pre><code>{}</code></pre>", escape_html_text(code));
                }
            }
            BlockNode::List { ordered, items } => {
                let tag = if *ordered { "ol" } else { "ul" };
                let _ = writeln!(out, "<{tag}>");
                for item in items {
                    let _ = writeln!(out, "  <li>{}</li>", render_html_inlines(item).trim());
                }
                let _ = writeln!(out, "</{tag}>");
            }
            BlockNode::Table { headers, rows } => {
                out.push_str("<table>\n<thead>\n<tr>");
                for cell in headers {
                    let _ = write!(out, "<th>{}</th>", render_html_inlines(cell).trim());
                }
                out.push_str("</tr>\n</thead>\n<tbody>\n");
                for row in rows {
                    out.push_str("<tr>");
                    for ci in 0..headers.len() {
                        let cell_html = row
                            .get(ci)
                            .map(|c| render_html_inlines(c).trim().to_owned())
                            .unwrap_or_default();
                        let _ = write!(out, "<td>{cell_html}</td>");
                    }
                    out.push_str("</tr>\n");
                }
                out.push_str("</tbody>\n</table>\n");
            }
            BlockNode::ThematicBreak => {
                out.push_str("<hr>\n");
            }
        }
    }

    out.push_str("</body>\n</html>\n");
    out
}

fn render_html_inlines(inlines: &[InlineNode]) -> String {
    let mut out = String::new();
    for node in inlines {
        match node {
            InlineNode::Text(t) => out.push_str(&escape_html_text(t)),
            InlineNode::Strong(t) => {
                let _ = write!(out, "<strong>{}</strong>", escape_html_text(t));
            }
            InlineNode::Emphasis(t) => {
                let _ = write!(out, "<em>{}</em>", escape_html_text(t));
            }
            InlineNode::Code(t) => {
                let _ = write!(out, "<code>{}</code>", escape_html_text(t));
            }
            InlineNode::Link { text, href } => {
                let _ = write!(
                    out,
                    "<a href=\"{}\">{}</a>",
                    escape_html_text(&sanitize_url(href)),
                    escape_html_text(text)
                );
            }
            InlineNode::Image { alt, src } => {
                let _ = write!(
                    out,
                    "<img src=\"{}\" alt=\"{}\">",
                    escape_html_text(&sanitize_url(src)),
                    escape_html_text(alt)
                );
            }
            InlineNode::LineBreak => out.push_str("<br>"),
        }
    }
    out
}

fn render_plain_text(doc: &MarkupDocument) -> String {
    let mut out = String::new();
    for (idx, block) in doc.blocks.iter().enumerate() {
        if idx > 0 {
            out.push_str("\n\n");
        }
        match block {
            BlockNode::Heading { inlines, .. }
            | BlockNode::Paragraph(inlines)
            | BlockNode::BlockQuote(inlines) => {
                out.push_str(flatten_inlines_plain(inlines).trim());
            }
            BlockNode::CodeBlock { code, .. } => {
                out.push_str(code);
            }
            BlockNode::List { ordered, items } => {
                for (row_idx, item) in items.iter().enumerate() {
                    if row_idx > 0 {
                        out.push('\n');
                    }
                    if *ordered {
                        let _ = write!(
                            out,
                            "{}. {}",
                            row_idx + 1,
                            flatten_inlines_plain(item).trim()
                        );
                    } else {
                        let _ = write!(out, "- {}", flatten_inlines_plain(item).trim());
                    }
                }
            }
            BlockNode::Table { headers, rows } => {
                let hdr: Vec<String> = headers
                    .iter()
                    .map(|c| flatten_inlines_plain(c).trim().to_owned())
                    .collect();
                out.push_str(&hdr.join("\t"));
                for row in rows {
                    out.push('\n');
                    let cells: Vec<String> = row
                        .iter()
                        .map(|c| flatten_inlines_plain(c).trim().to_owned())
                        .collect();
                    out.push_str(&cells.join("\t"));
                }
            }
            BlockNode::ThematicBreak => {
                out.push_str("---");
            }
        }
    }
    if !out.ends_with('\n') {
        out.push('\n');
    }
    out
}

/// Flattens inline spans into plain visible text without markup punctuation.
#[must_use]
pub fn flatten_inlines_plain(inlines: &[InlineNode]) -> String {
    let mut out = String::new();
    for node in inlines {
        match node {
            InlineNode::Text(t)
            | InlineNode::Strong(t)
            | InlineNode::Emphasis(t)
            | InlineNode::Code(t) => out.push_str(t),
            InlineNode::Link { text, .. } => out.push_str(text),
            InlineNode::Image { alt, .. } => out.push_str(alt),
            InlineNode::LineBreak => out.push('\n'),
        }
    }
    out
}
