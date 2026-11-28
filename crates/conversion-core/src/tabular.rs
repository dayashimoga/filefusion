//! Pure-Rust bidirectional structured data and tabular conversion engine covering
//! `CSV`/`TSV`, `JSON`, `YAML`, and `XML` (`FR-DOC-001`, `NFR-SEC-001`).

use std::collections::BTreeSet;
use std::fmt::Write as _;

use file_core::{ErrorCode, FileFusionError, FileFusionResult, FormatId};
use serde_json::{Map, Number, Value};

use crate::markup::{MarkupDocument, document_from_table, render_markup_document};

const MAX_STRUCT_DEPTH: usize = 64;

/// Converts structured or tabular text across `Csv`, `Json`, `Yaml`, and `Xml`, or exports any
/// structured dataset cleanly into `Markdown`, `Html`, or `PlainText`.
///
/// # Errors
/// Returns [`FileFusionError`] on malformed syntax, XML `<!ENTITY>` bomb attempts, excessive
/// nesting depth (`> 64`), or unsupported source/target formats.
pub fn convert_structured_data(
    input: &str,
    source: FormatId,
    target: FormatId,
    strip_metadata: bool,
) -> FileFusionResult<String> {
    let val = parse_structured_value(input, source)?;
    match target {
        FormatId::Csv => Ok(render_csv(&val)),
        FormatId::Json => serde_json::to_string_pretty(&val)
            .map(|mut s| {
                s.push('\n');
                s
            })
            .map_err(|err| FileFusionError::new(ErrorCode::FmtCorrupt, err.to_string())),
        FormatId::Yaml => Ok(render_yaml(&val)),
        FormatId::Xml => Ok(render_xml(&val)),
        FormatId::Markdown | FormatId::Html | FormatId::PlainText => {
            let doc = value_to_markup_document(&val);
            render_markup_document(&doc, target, strip_metadata)
        }
        other => Err(FileFusionError::new(
            ErrorCode::FmtUnsupported,
            format!("structured converter does not support target format {other:?}"),
        )),
    }
}

/// Parses structured text (`Csv`, `Json`, `Yaml`, or `Xml`) into a canonical [`serde_json::Value`].
///
/// # Errors
/// Returns [`FileFusionError`] on syntax errors or security violations (`<!ENTITY>` XXE/bombs).
pub fn parse_structured_value(input: &str, format: FormatId) -> FileFusionResult<Value> {
    match format {
        FormatId::Json => serde_json::from_str::<Value>(input).map_err(|err| {
            FileFusionError::new(ErrorCode::FmtCorrupt, format!("invalid JSON: {err}"))
        }),
        FormatId::Csv => Ok(parse_csv_or_tsv(input)),
        FormatId::Yaml => parse_yaml(input),
        FormatId::Xml => parse_xml(input),
        other => Err(FileFusionError::new(
            ErrorCode::FmtUnsupported,
            format!("structured parser does not handle format {other:?}"),
        )),
    }
}

/// Returns `true` when `val` contains nested arrays or objects that require flattening into CSV.
#[must_use]
pub fn value_has_nested_hierarchy(val: &Value) -> bool {
    match val {
        Value::Array(items) => items.iter().any(|item| match item {
            Value::Object(map) => map.values().any(|v| v.is_object() || v.is_array()),
            Value::Array(_) => true,
            _ => false,
        }),
        Value::Object(map) => map.values().any(|v| v.is_object() || v.is_array()),
        _ => false,
    }
}

fn value_to_markup_document(val: &Value) -> MarkupDocument {
    let (headers, rows) = extract_table_grid(val);
    document_from_table(None, headers, rows)
}

fn detect_csv_delimiter(input: &str) -> char {
    let first_line = input.lines().next().unwrap_or("");
    let tabs = first_line.matches('\t').count();
    let commas = first_line.matches(',').count();
    if tabs > 0 && commas == 0 { '\t' } else { ',' }
}

fn parse_csv_or_tsv(input: &str) -> Value {
    let delim = detect_csv_delimiter(input);
    let rows = parse_rfc4180_rows(input, delim);
    if rows.is_empty() {
        return Value::Array(Vec::new());
    }

    let raw_headers = &rows[0];
    let mut used = BTreeSet::new();
    let mut headers = Vec::with_capacity(raw_headers.len());
    for (idx, raw_h) in raw_headers.iter().enumerate() {
        let base = if raw_h.trim().is_empty() {
            format!("col_{}", idx + 1)
        } else {
            raw_h.trim().to_owned()
        };
        let mut candidate = base.clone();
        let mut suffix = 2usize;
        while used.contains(&candidate) {
            candidate = format!("{base}_{suffix}");
            suffix += 1;
        }
        used.insert(candidate.clone());
        headers.push(candidate);
    }

    let mut out = Vec::with_capacity(rows.len().saturating_sub(1));
    for row in rows.iter().skip(1) {
        let mut obj = Map::new();
        for (idx, key) in headers.iter().enumerate() {
            let cell = row.get(idx).map_or("", String::as_str);
            obj.insert(key.clone(), parse_scalar(cell, false));
        }
        out.push(Value::Object(obj));
    }
    Value::Array(out)
}

fn parse_rfc4180_rows(input: &str, delim: char) -> Vec<Vec<String>> {
    let normalized = input.replace("\r\n", "\n").replace('\r', "\n");
    let chars: Vec<char> = normalized.chars().collect();
    let mut rows = Vec::new();
    let mut row = Vec::new();
    let mut field = String::new();
    let mut in_quotes = false;
    let mut i = 0usize;

    while i < chars.len() {
        let ch = chars[i];
        if in_quotes {
            if ch == '"' {
                if i + 1 < chars.len() && chars[i + 1] == '"' {
                    field.push('"');
                    i += 2;
                    continue;
                }
                in_quotes = false;
                i += 1;
                continue;
            }
            field.push(ch);
            i += 1;
            continue;
        }

        if ch == '"' && field.is_empty() {
            in_quotes = true;
            i += 1;
        } else if ch == delim {
            row.push(std::mem::take(&mut field));
            i += 1;
        } else if ch == '\n' {
            row.push(std::mem::take(&mut field));
            if row.len() == 1 && row[0].is_empty() {
                row.clear();
            } else {
                rows.push(std::mem::take(&mut row));
            }
            i += 1;
        } else {
            field.push(ch);
            i += 1;
        }
    }

    if !field.is_empty() || !row.is_empty() {
        row.push(field);
        if !(row.len() == 1 && row[0].is_empty()) {
            rows.push(row);
        }
    }

    rows
}

fn extract_table_grid(val: &Value) -> (Vec<String>, Vec<Vec<String>>) {
    match val {
        Value::Array(items) => {
            if items.is_empty() {
                return (Vec::new(), Vec::new());
            }
            if items.iter().all(Value::is_object) {
                let mut cols = BTreeSet::new();
                for item in items {
                    if let Some(map) = item.as_object() {
                        for k in map.keys() {
                            cols.insert(k.clone());
                        }
                    }
                }
                let headers: Vec<String> = cols.into_iter().collect();
                let mut rows = Vec::with_capacity(items.len());
                for item in items {
                    let row: Vec<String> = headers
                        .iter()
                        .map(|col| {
                            item.as_object()
                                .and_then(|m| m.get(col))
                                .map(cell_to_string)
                                .unwrap_or_default()
                        })
                        .collect();
                    rows.push(row);
                }
                return (headers, rows);
            }
            let rows = items.iter().map(|v| vec![cell_to_string(v)]).collect();
            (vec![String::from("value")], rows)
        }
        Value::Object(map) => {
            let rows = map
                .iter()
                .map(|(k, v)| vec![k.clone(), cell_to_string(v)])
                .collect();
            (vec![String::from("key"), String::from("value")], rows)
        }
        scalar => (
            vec![String::from("value")],
            vec![vec![cell_to_string(scalar)]],
        ),
    }
}

fn cell_to_string(val: &Value) -> String {
    match val {
        Value::Null => String::new(),
        Value::Bool(b) => b.to_string(),
        Value::Number(n) => n.to_string(),
        Value::String(s) => s.clone(),
        Value::Array(_) | Value::Object(_) => serde_json::to_string(val).unwrap_or_default(),
    }
}

fn render_csv(val: &Value) -> String {
    let (headers, rows) = extract_table_grid(val);
    if headers.is_empty() {
        return String::new();
    }
    let mut out = String::new();
    let header_line: Vec<String> = headers.iter().map(|h| escape_csv_cell(h)).collect();
    out.push_str(&header_line.join(","));
    out.push('\n');
    for row in rows {
        let cells: Vec<String> = row.iter().map(|c| escape_csv_cell(c)).collect();
        out.push_str(&cells.join(","));
        out.push('\n');
    }
    out
}

fn escape_csv_cell(raw: &str) -> String {
    if raw.contains([',', '"', '\n', '\r']) {
        let escaped = raw.replace('"', "\"\"");
        format!("\"{escaped}\"")
    } else {
        raw.to_owned()
    }
}

fn parse_scalar(raw: &str, allow_yaml_null: bool) -> Value {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return if allow_yaml_null {
            Value::Null
        } else {
            Value::String(String::new())
        };
    }
    if trimmed == "null" || (allow_yaml_null && trimmed == "~") {
        return Value::Null;
    }
    if trimmed == "true" {
        return Value::Bool(true);
    }
    if trimmed == "false" {
        return Value::Bool(false);
    }
    let preserve_string =
        (trimmed.starts_with('0') && trimmed.len() > 1 && !trimmed.starts_with("0."))
            || trimmed.starts_with('+');
    if !preserve_string {
        if let Ok(int_val) = trimmed.parse::<i64>() {
            return Value::Number(Number::from(int_val));
        }
        if let Ok(float_val) = trimmed.parse::<f64>() {
            if let Some(num) = Number::from_f64(float_val) {
                return Value::Number(num);
            }
        }
    }
    Value::String(trimmed.to_owned())
}

// =========================================================================
// Pure-Rust YAML 1.2 Subset Parser & Canonical Emitter
// =========================================================================

#[derive(Debug, Clone)]
struct YamlLine {
    indent: usize,
    text: String,
}

fn preprocess_yaml_lines(input: &str) -> Vec<YamlLine> {
    let mut out = Vec::new();
    for raw in input.replace("\r\n", "\n").lines() {
        let no_comment = strip_yaml_comment(raw);
        let trimmed = no_comment.trim();
        if trimmed.is_empty() || trimmed == "---" || trimmed == "..." {
            continue;
        }
        let indent = no_comment.bytes().take_while(|&b| b == b' ').count();
        out.push(YamlLine {
            indent,
            text: no_comment[indent..].trim_end().to_owned(),
        });
    }
    out
}

fn strip_yaml_comment(line: &str) -> &str {
    let mut in_single = false;
    let mut in_double = false;
    let mut escaped = false;
    let mut prev_space = true;

    for (idx, ch) in line.char_indices() {
        if escaped {
            escaped = false;
            prev_space = false;
            continue;
        }
        if in_double && ch == '\\' {
            escaped = true;
            prev_space = false;
            continue;
        }
        if !in_double && ch == '\'' {
            in_single = !in_single;
        } else if !in_single && ch == '"' {
            in_double = !in_double;
        } else if !in_single && !in_double && ch == '#' && prev_space {
            return &line[..idx];
        }
        prev_space = ch.is_whitespace();
    }
    line
}

fn parse_yaml(input: &str) -> FileFusionResult<Value> {
    let lines = preprocess_yaml_lines(input);
    if lines.is_empty() {
        return Ok(Value::Null);
    }
    let (val, _next) = parse_yaml_block(&lines, 0, lines[0].indent, 0)?;
    Ok(val)
}

fn parse_yaml_block(
    lines: &[YamlLine],
    start: usize,
    base_indent: usize,
    depth: usize,
) -> FileFusionResult<(Value, usize)> {
    if depth > MAX_STRUCT_DEPTH {
        return Err(FileFusionError::new(
            ErrorCode::SecBomb,
            "YAML nesting depth exceeds security ceiling",
        ));
    }
    if start >= lines.len() {
        return Ok((Value::Null, start));
    }
    if is_yaml_seq_line(&lines[start].text) {
        parse_yaml_sequence(lines, start, lines[start].indent, depth + 1)
    } else if split_yaml_mapping_kv(&lines[start].text).is_some() {
        parse_yaml_mapping(
            lines,
            start,
            base_indent.max(lines[start].indent),
            depth + 1,
        )
    } else {
        Ok((parse_yaml_value_str(&lines[start].text), start + 1))
    }
}

fn is_yaml_seq_line(text: &str) -> bool {
    text == "-" || text.starts_with("- ")
}

fn parse_yaml_sequence(
    lines: &[YamlLine],
    mut i: usize,
    seq_indent: usize,
    depth: usize,
) -> FileFusionResult<(Value, usize)> {
    let mut items = Vec::new();
    while i < lines.len() && lines[i].indent == seq_indent && is_yaml_seq_line(&lines[i].text) {
        let rest = lines[i].text.strip_prefix('-').unwrap_or("").trim_start();
        if rest.is_empty() {
            i += 1;
            if i < lines.len() && lines[i].indent > seq_indent {
                let (child, next_i) = parse_yaml_block(lines, i, lines[i].indent, depth + 1)?;
                items.push(child);
                i = next_i;
            } else {
                items.push(Value::Null);
            }
            continue;
        }

        if let Some((first_k, first_v)) = split_yaml_mapping_kv(rest) {
            let map_indent = seq_indent + 2;
            let mut map = Map::new();
            if first_v.is_empty() {
                i += 1;
                if i < lines.len() && lines[i].indent > map_indent {
                    let (nested, next_i) = parse_yaml_block(lines, i, lines[i].indent, depth + 1)?;
                    map.insert(first_k, nested);
                    i = next_i;
                } else {
                    map.insert(first_k, Value::Null);
                }
            } else {
                map.insert(first_k, parse_yaml_value_str(first_v));
                i += 1;
            }
            if i < lines.len() && lines[i].indent >= map_indent && !is_yaml_seq_line(&lines[i].text)
            {
                let (tail_val, next_i) = parse_yaml_mapping(lines, i, lines[i].indent, depth + 1)?;
                if let Value::Object(tail_map) = tail_val {
                    for (k, v) in tail_map {
                        map.insert(k, v);
                    }
                }
                i = next_i;
            }
            items.push(Value::Object(map));
            continue;
        }

        items.push(parse_yaml_value_str(rest));
        i += 1;
    }
    Ok((Value::Array(items), i))
}

fn parse_yaml_mapping(
    lines: &[YamlLine],
    mut i: usize,
    map_indent: usize,
    depth: usize,
) -> FileFusionResult<(Value, usize)> {
    let mut map = Map::new();
    while i < lines.len() && lines[i].indent == map_indent {
        if is_yaml_seq_line(&lines[i].text) {
            break;
        }
        let Some((key, val_part)) = split_yaml_mapping_kv(&lines[i].text) else {
            break;
        };
        if val_part == "|" || val_part == ">" {
            let folded = val_part == ">";
            let block_min_indent = map_indent + 1;
            i += 1;
            let mut parts = Vec::new();
            while i < lines.len() && lines[i].indent >= block_min_indent {
                parts.push(lines[i].text.as_str());
                i += 1;
            }
            let joined = if folded {
                parts.join(" ")
            } else {
                parts.join("\n")
            };
            map.insert(key, Value::String(joined));
            continue;
        }
        if val_part.is_empty() {
            i += 1;
            if i < lines.len()
                && (lines[i].indent > map_indent
                    || (lines[i].indent == map_indent && is_yaml_seq_line(&lines[i].text)))
            {
                let (child, next_i) = parse_yaml_block(lines, i, lines[i].indent, depth + 1)?;
                map.insert(key, child);
                i = next_i;
            } else {
                map.insert(key, Value::Null);
            }
        } else {
            map.insert(key, parse_yaml_value_str(val_part));
            i += 1;
        }
    }
    Ok((Value::Object(map), i))
}

fn split_yaml_mapping_kv(text: &str) -> Option<(String, &str)> {
    let mut in_single = false;
    let mut in_double = false;
    let chars: Vec<(usize, char)> = text.char_indices().collect();

    for idx in 0..chars.len() {
        let (byte_pos, ch) = chars[idx];
        if !in_double && ch == '\'' {
            in_single = !in_single;
        } else if !in_single && ch == '"' {
            in_double = !in_double;
        } else if !in_single && !in_double && ch == ':' {
            let next_is_boundary = idx + 1 == chars.len() || chars[idx + 1].1.is_whitespace();
            if next_is_boundary {
                let raw_key = text[..byte_pos].trim();
                let key = unquote_yaml_scalar(raw_key);
                let val_part = text[byte_pos + 1..].trim_start();
                return Some((key, val_part));
            }
        }
    }
    None
}

fn unquote_yaml_scalar(raw: &str) -> String {
    if let Some(inner) = raw.strip_prefix('"').and_then(|s| s.strip_suffix('"')) {
        return inner
            .replace("\\n", "\n")
            .replace("\\t", "\t")
            .replace("\\\"", "\"")
            .replace("\\\\", "\\");
    }
    if let Some(inner) = raw.strip_prefix('\'').and_then(|s| s.strip_suffix('\'')) {
        return inner.replace("''", "'");
    }
    raw.to_owned()
}

fn parse_yaml_value_str(raw: &str) -> Value {
    let trimmed = raw.trim();
    if (trimmed.starts_with('{') && trimmed.ends_with('}'))
        || (trimmed.starts_with('[') && trimmed.ends_with(']'))
    {
        if let Ok(v) = serde_json::from_str::<Value>(trimmed) {
            return v;
        }
        if let Some(inner) = trimmed.strip_prefix('[').and_then(|s| s.strip_suffix(']')) {
            let items = split_flow_items(inner)
                .into_iter()
                .map(|part| parse_yaml_value_str(&part))
                .collect();
            return Value::Array(items);
        }
        if let Some(inner) = trimmed.strip_prefix('{').and_then(|s| s.strip_suffix('}')) {
            let mut map = Map::new();
            for pair in split_flow_items(inner) {
                if let Some((k, v)) = split_yaml_mapping_kv(&pair) {
                    map.insert(k, parse_yaml_value_str(v));
                }
            }
            return Value::Object(map);
        }
    }

    if (trimmed.starts_with('"') && trimmed.ends_with('"'))
        || (trimmed.starts_with('\'') && trimmed.ends_with('\''))
    {
        return Value::String(unquote_yaml_scalar(trimmed));
    }

    parse_scalar(trimmed, true)
}

fn split_flow_items(inner: &str) -> Vec<String> {
    let mut items = Vec::new();
    let mut cur = String::new();
    let mut in_single = false;
    let mut in_double = false;
    let mut depth = 0usize;

    for ch in inner.chars() {
        if !in_double && ch == '\'' {
            in_single = !in_single;
        } else if !in_single && ch == '"' {
            in_double = !in_double;
        } else if !in_single && !in_double {
            match ch {
                '[' | '{' => depth += 1,
                ']' | '}' => depth = depth.saturating_sub(1),
                ',' if depth == 0 => {
                    let s = std::mem::take(&mut cur);
                    if !s.trim().is_empty() {
                        items.push(s.trim().to_owned());
                    }
                    continue;
                }
                _ => {}
            }
        }
        cur.push(ch);
    }
    if !cur.trim().is_empty() {
        items.push(cur.trim().to_owned());
    }
    items
}

fn render_yaml(val: &Value) -> String {
    let mut out = String::new();
    write_yaml_node(&mut out, val, 0);
    if !out.ends_with('\n') {
        out.push('\n');
    }
    out
}

fn write_yaml_node(out: &mut String, val: &Value, indent: usize) {
    let pad = " ".repeat(indent);
    match val {
        Value::Null => out.push_str("null\n"),
        Value::Bool(b) => {
            let _ = writeln!(out, "{b}");
        }
        Value::Number(n) => {
            let _ = writeln!(out, "{n}");
        }
        Value::String(s) => {
            let _ = writeln!(out, "{}", format_yaml_string(s));
        }
        Value::Array(items) => {
            if items.is_empty() {
                out.push_str("[]\n");
                return;
            }
            for item in items {
                let _ = write!(out, "{pad}- ");
                match item {
                    Value::Object(map) if !map.is_empty() => {
                        let mut first = true;
                        for (k, v) in map {
                            if !first {
                                out.push_str(&pad);
                                out.push_str("  ");
                            }
                            first = false;
                            write_yaml_kv(out, k, v, indent + 2);
                        }
                    }
                    Value::Array(sub) if !sub.is_empty() => {
                        out.push('\n');
                        write_yaml_node(out, item, indent + 2);
                    }
                    other => write_yaml_node(out, other, indent + 2),
                }
            }
        }
        Value::Object(map) => {
            if map.is_empty() {
                out.push_str("{}\n");
                return;
            }
            for (k, v) in map {
                out.push_str(&pad);
                write_yaml_kv(out, k, v, indent);
            }
        }
    }
}

fn write_yaml_kv(out: &mut String, key: &str, val: &Value, indent: usize) {
    let fmt_key = format_yaml_key(key);
    match val {
        Value::Object(m) if !m.is_empty() => {
            let _ = writeln!(out, "{fmt_key}:");
            write_yaml_node(out, val, indent + 2);
        }
        Value::Array(a) if !a.is_empty() => {
            let _ = writeln!(out, "{fmt_key}:");
            write_yaml_node(out, val, indent + 2);
        }
        other => {
            let _ = write!(out, "{fmt_key}: ");
            write_yaml_node(out, other, indent + 2);
        }
    }
}

fn format_yaml_key(raw: &str) -> String {
    if !raw.is_empty()
        && raw
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | '.'))
    {
        raw.to_owned()
    } else {
        format_yaml_string(raw)
    }
}

fn format_yaml_string(s: &str) -> String {
    let looks_ambiguous = s.is_empty()
        || matches!(s, "true" | "false" | "null" | "~")
        || s.parse::<f64>().is_ok()
        || s.contains('\n')
        || s.contains(": ")
        || s.starts_with([
            '-', '?', ':', '[', ']', '{', '}', '#', '&', '*', '!', '|', '>', '\'', '"', ' ', '@',
            '`',
        ]);
    if looks_ambiguous {
        serde_json::to_string(s).unwrap_or_else(|_| format!("\"{s}\""))
    } else {
        s.to_owned()
    }
}

// =========================================================================
// Security-Hardened XML Parser & Emitter (`NFR-SEC-001` XXE/Entity Guard)
// =========================================================================

fn parse_xml(input: &str) -> FileFusionResult<Value> {
    let lower = input.to_ascii_lowercase();
    if lower.contains("<!entity") || lower.contains("<!doctype") {
        return Err(FileFusionError::new(
            ErrorCode::SecBomb,
            "XML DTD / ENTITY declarations are blocked by security policy (NFR-SEC-001)",
        ));
    }

    let trimmed = input.trim();
    if trimmed.is_empty() {
        return Err(FileFusionError::new(
            ErrorCode::FmtCorrupt,
            "XML input is empty",
        ));
    }

    let mut parser = XmlCursor {
        src: trimmed,
        pos: 0,
    };
    parser.skip_prolog_and_ws();
    let (root_name, mut root_val) = parser.parse_element(0)?;

    if root_name == "root" {
        return Ok(root_val);
    }
    if root_name == "records" {
        if let Value::Object(ref mut map) = root_val {
            if let Some(record_val) = map.remove("record") {
                return Ok(match record_val {
                    Value::Array(arr) => Value::Array(arr),
                    single => Value::Array(vec![single]),
                });
            }
        }
    }
    let mut wrapper = Map::new();
    wrapper.insert(root_name, root_val);
    Ok(Value::Object(wrapper))
}

struct XmlCursor<'a> {
    src: &'a str,
    pos: usize,
}

impl<'a> XmlCursor<'a> {
    fn rest(&self) -> &'a str {
        &self.src[self.pos..]
    }

    fn skip_prolog_and_ws(&mut self) {
        loop {
            let trimmed = self.rest().trim_start();
            self.pos = self.src.len() - trimmed.len();
            if self.rest().starts_with("<?") {
                if let Some(end) = self.rest().find("?>") {
                    self.pos += end + 2;
                    continue;
                }
            }
            if self.rest().starts_with("<!--") {
                if let Some(end) = self.rest().find("-->") {
                    self.pos += end + 3;
                    continue;
                }
            }
            break;
        }
    }

    fn parse_element(&mut self, depth: usize) -> FileFusionResult<(String, Value)> {
        if depth > MAX_STRUCT_DEPTH {
            return Err(FileFusionError::new(
                ErrorCode::SecBomb,
                "XML nesting depth exceeds security ceiling",
            ));
        }
        let (tag_name, self_closing, mut map) = self.parse_opening_tag()?;
        if self_closing {
            return Ok((
                tag_name,
                if map.is_empty() {
                    Value::Null
                } else {
                    Value::Object(map)
                },
            ));
        }

        let (text_buf, has_children) = self.parse_element_body(&tag_name, depth, &mut map)?;
        let trimmed_text = text_buf.trim();
        if map.is_empty() && !has_children {
            return Ok((tag_name, parse_scalar(trimmed_text, true)));
        }
        if !trimmed_text.is_empty() {
            map.insert(
                String::from("#text"),
                Value::String(trimmed_text.to_owned()),
            );
        }
        Ok((tag_name, Value::Object(map)))
    }

    fn parse_opening_tag(&mut self) -> FileFusionResult<(String, bool, Map<String, Value>)> {
        self.skip_prolog_and_ws();
        if !self.rest().starts_with('<') || self.rest().starts_with("</") {
            return Err(FileFusionError::new(
                ErrorCode::FmtCorrupt,
                "expected XML opening element",
            ));
        }
        self.pos += 1;
        let Some(gt_rel) = self.rest().find('>') else {
            return Err(FileFusionError::new(
                ErrorCode::FmtCorrupt,
                "unterminated XML opening tag",
            ));
        };
        let raw_tag = &self.rest()[..gt_rel];
        self.pos += gt_rel + 1;

        let self_closing = raw_tag.trim_end().ends_with('/');
        let inner_tag = raw_tag.trim().trim_end_matches('/').trim();
        let (tag_name, attr_raw) = split_xml_tag(inner_tag)?;
        let attrs = parse_xml_attrs(attr_raw);

        let mut map = Map::new();
        for (ak, av) in attrs {
            map.insert(format!("@{ak}"), parse_scalar(&av, false));
        }
        Ok((tag_name, self_closing, map))
    }

    fn parse_element_body(
        &mut self,
        tag_name: &str,
        depth: usize,
        map: &mut Map<String, Value>,
    ) -> FileFusionResult<(String, bool)> {
        let mut text_buf = String::new();
        let mut has_children = false;
        loop {
            self.skip_comments_only();
            if self.rest().is_empty() {
                return Err(FileFusionError::new(
                    ErrorCode::FmtCorrupt,
                    format!("unclosed XML tag <{tag_name}>"),
                ));
            }
            if let Some(after_close) = self.rest().strip_prefix("</") {
                let Some(close_gt) = after_close.find('>') else {
                    return Err(FileFusionError::new(
                        ErrorCode::FmtCorrupt,
                        "unterminated XML closing tag",
                    ));
                };
                let close_name = after_close[..close_gt].trim();
                if close_name != tag_name {
                    return Err(FileFusionError::new(
                        ErrorCode::FmtCorrupt,
                        format!("mismatched XML tag </{close_name}> for <{tag_name}>"),
                    ));
                }
                self.pos += 2 + close_gt + 1;
                return Ok((text_buf, has_children));
            }
            if let Some(cdata_rest) = self.rest().strip_prefix("<![CDATA[") {
                let Some(end) = cdata_rest.find("]]>") else {
                    return Err(FileFusionError::new(
                        ErrorCode::FmtCorrupt,
                        "unterminated XML CDATA section",
                    ));
                };
                text_buf.push_str(&cdata_rest[..end]);
                self.pos += 9 + end + 3;
                continue;
            }
            if self.rest().starts_with('<') {
                let (child_name, child_val) = self.parse_element(depth + 1)?;
                has_children = true;
                match map.get_mut(&child_name) {
                    None => {
                        map.insert(child_name, child_val);
                    }
                    Some(Value::Array(arr)) => arr.push(child_val),
                    Some(existing) => {
                        let first = existing.take();
                        *existing = Value::Array(vec![first, child_val]);
                    }
                }
            } else if let Some(next_lt) = self.rest().find('<') {
                text_buf.push_str(&decode_xml_entities(&self.rest()[..next_lt]));
                self.pos += next_lt;
            } else {
                return Err(FileFusionError::new(
                    ErrorCode::FmtCorrupt,
                    format!("unexpected EOF inside <{tag_name}>"),
                ));
            }
        }
    }

    fn skip_comments_only(&mut self) {
        while self.rest().trim_start().starts_with("<!--") {
            let trimmed = self.rest().trim_start();
            self.pos = self.src.len() - trimmed.len();
            if let Some(end) = self.rest().find("-->") {
                self.pos += end + 3;
            } else {
                break;
            }
        }
    }
}

fn split_xml_tag(raw: &str) -> FileFusionResult<(String, &str)> {
    if raw.is_empty() {
        return Err(FileFusionError::new(ErrorCode::FmtCorrupt, "empty XML tag"));
    }
    for (idx, ch) in raw.char_indices() {
        if ch.is_whitespace() {
            return Ok((raw[..idx].to_owned(), &raw[idx..]));
        }
    }
    Ok((raw.to_owned(), ""))
}

fn parse_xml_attrs(raw: &str) -> Vec<(String, String)> {
    let chars: Vec<char> = raw.chars().collect();
    let mut attrs = Vec::new();
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
        while i < chars.len() && (chars[i].is_whitespace() || chars[i] == '=') {
            i += 1;
        }
        if i < chars.len() && (chars[i] == '"' || chars[i] == '\'') {
            let q = chars[i];
            i += 1;
            let v_start = i;
            while i < chars.len() && chars[i] != q {
                i += 1;
            }
            let val: String = chars[v_start..i].iter().collect();
            if i < chars.len() {
                i += 1;
            }
            if !key.is_empty() {
                attrs.push((key, decode_xml_entities(&val)));
            }
        }
    }
    attrs
}

fn decode_xml_entities(input: &str) -> String {
    input
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&apos;", "'")
        .replace("&amp;", "&")
}

fn escape_xml(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    for ch in input.chars() {
        match ch {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&apos;"),
            other => out.push(other),
        }
    }
    out
}

fn sanitize_xml_name(raw: &str) -> String {
    let mut out: String = raw
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | '.') {
                c
            } else {
                '_'
            }
        })
        .collect();
    if out.is_empty() || out.as_bytes()[0].is_ascii_digit() {
        out.insert(0, '_');
    }
    out
}

fn render_xml(val: &Value) -> String {
    let mut out = String::from("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n");
    match val {
        Value::Array(items) => {
            out.push_str("<records>\n");
            for item in items {
                write_xml_element(&mut out, "record", item, 2);
            }
            out.push_str("</records>\n");
        }
        Value::Object(map) if map.len() == 1 => {
            if let Some((k, v)) = map.iter().next() {
                write_xml_element(&mut out, k, v, 0);
            }
        }
        other => write_xml_element(&mut out, "root", other, 0),
    }
    out
}

fn write_xml_element(out: &mut String, name: &str, val: &Value, indent: usize) {
    let tag = sanitize_xml_name(name);
    let pad = " ".repeat(indent);

    match val {
        Value::Null => {
            let _ = writeln!(out, "{pad}<{tag}/>");
        }
        Value::Bool(b) => {
            let _ = writeln!(out, "{pad}<{tag}>{b}</{tag}>");
        }
        Value::Number(n) => {
            let _ = writeln!(out, "{pad}<{tag}>{n}</{tag}>");
        }
        Value::String(s) => {
            let _ = writeln!(out, "{pad}<{tag}>{}</{tag}>", escape_xml(s));
        }
        Value::Array(items) => {
            for item in items {
                write_xml_element(out, &tag, item, indent);
            }
        }
        Value::Object(map) => {
            let mut attrs = String::new();
            let mut text_node = None;
            let mut children = Vec::new();
            for (k, v) in map {
                if let Some(attr_name) = k.strip_prefix('@') {
                    let _ = write!(
                        attrs,
                        " {}=\"{}\"",
                        sanitize_xml_name(attr_name),
                        escape_xml(&cell_to_string(v))
                    );
                } else if k == "#text" {
                    text_node = Some(cell_to_string(v));
                } else {
                    children.push((k, v));
                }
            }
            if children.is_empty() && text_node.is_none() {
                let _ = writeln!(out, "{pad}<{tag}{attrs}/>");
                return;
            }
            if children.is_empty() {
                let _ = writeln!(
                    out,
                    "{pad}<{tag}{attrs}>{}</{tag}>",
                    escape_xml(&text_node.unwrap_or_default())
                );
                return;
            }
            let _ = writeln!(out, "{pad}<{tag}{attrs}>");
            for (ck, cv) in children {
                write_xml_element(out, ck, cv, indent + 2);
            }
            let _ = writeln!(out, "{pad}</{tag}>");
        }
    }
}
