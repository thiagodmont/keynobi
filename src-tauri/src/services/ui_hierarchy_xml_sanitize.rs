//! Repair common malformations in Android `uiautomator dump` XML.
//!
//! The platform serializer often omits escaping in attribute values (`&`, `<`, `>`)
//! and may emit HTML named entities (`&nbsp;`) that strict XML parsers reject.

use regex::Regex;
use roxmltree::{Error as XmlError, TextPos};
use std::sync::LazyLock;

static NAMED_ENTITY: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"&(?P<name>[a-zA-Z][a-zA-Z0-9]*);").expect("valid regex")
});

/// Strip / replace characters illegal in XML 1.0.
fn strip_disallowed_xml_chars(s: &str) -> String {
    s.chars()
        .map(|c| match c {
            '\t' | '\n' | '\r' => c,
            c if (' '..='\u{D7FF}').contains(&c) => c,
            c if ('\u{E000}'..='\u{FFFD}').contains(&c) => c,
            c if ('\u{10000}'..='\u{10FFFF}').contains(&c) => c,
            _ => '\u{FFFD}',
        })
        .collect()
}

/// Escape `<` and `>` inside double-quoted attribute values (`key="value"`).
/// UI Automator dumps use `"` for all attributes.
fn escape_lt_gt_in_double_quoted_values(input: &str) -> String {
    let chars: Vec<char> = input.chars().collect();
    let mut out = String::with_capacity(input.len());
    let mut i = 0;
    while i < chars.len() {
        if chars[i] == '=' && i + 1 < chars.len() && chars[i + 1] == '"' {
            out.push('=');
            out.push('"');
            i += 2;
            // Empty attribute `=""`
            if i < chars.len() && chars[i] == '"' {
                out.push('"');
                i += 1;
                continue;
            }
            while i < chars.len() {
                let c = chars[i];
                if c == '"' {
                    if is_attribute_value_closing(&chars, i) {
                        out.push('"');
                        i += 1;
                        break;
                    }
                    out.push('"');
                    i += 1;
                } else if c == '<' {
                    out.push_str("&lt;");
                    i += 1;
                } else if c == '>' {
                    out.push_str("&gt;");
                    i += 1;
                } else {
                    out.push(c);
                    i += 1;
                }
            }
        } else {
            out.push(chars[i]);
            i += 1;
        }
    }
    out
}

fn is_attribute_value_closing(chars: &[char], quote_idx: usize) -> bool {
    let mut j = quote_idx + 1;
    while j < chars.len() && matches!(chars[j], ' ' | '\t' | '\r' | '\n') {
        j += 1;
    }
    if j >= chars.len() {
        return true;
    }
    match chars[j] {
        '/' | '>' => true,
        c if c.is_ascii_alphabetic() || c == '_' => true,
        ':' => true,
        _ => false,
    }
}

/// Replace HTML-style named entities roxmltree rejects; keep XML predefined entities.
fn replace_unknown_named_entities(s: &str) -> String {
    NAMED_ENTITY
        .replace_all(s, |caps: &regex::Captures| {
            let name = caps.name("name").map(|m| m.as_str()).unwrap_or("");
            match name {
                "amp" | "lt" | "gt" | "quot" | "apos" => caps[0].to_string(),
                "nbsp" => "&#160;".to_string(),
                // Common in WebView / pasted content
                "copy" | "reg" | "trade" => " ".to_string(),
                _ => " ".to_string(),
            }
        })
        .into_owned()
}

/// Length in bytes of a well-formed entity reference starting with `&`, or None if not an entity.
fn known_entity_len(s: &str) -> Option<usize> {
    if s.is_empty() || !s.starts_with('&') {
        return None;
    }
    if s.starts_with("&amp;") {
        return Some(5);
    }
    if s.starts_with("&lt;") {
        return Some(4);
    }
    if s.starts_with("&gt;") {
        return Some(4);
    }
    if s.starts_with("&quot;") {
        return Some(6);
    }
    if s.starts_with("&apos;") {
        return Some(6);
    }
    let rest = s.strip_prefix("&#")?;
    let hexpart = rest.strip_prefix('x').or_else(|| rest.strip_prefix('X'));
    if let Some(hexpart) = hexpart {
        let mut n = 0usize;
        for b in hexpart.as_bytes() {
            if b.is_ascii_hexdigit() {
                n += 1;
            } else {
                break;
            }
        }
        if n > 0 && hexpart.as_bytes().get(n) == Some(&b';') {
            return Some(2 + 1 + n + 1); // &# + x + hex + ;
        }
        return None;
    }
    let mut n = 0usize;
    for b in rest.as_bytes() {
        if b.is_ascii_digit() {
            n += 1;
        } else {
            break;
        }
    }
    if n > 0 && rest.as_bytes().get(n) == Some(&b';') {
        return Some(2 + n + 1);
    }
    None
}

/// Turn bare `&` (not starting a known entity) into `&amp;`.
fn fix_bare_ampersands(input: &str) -> String {
    let mut out = String::with_capacity(input.len() + 16);
    let mut i = 0;
    while i < input.len() {
        if input.as_bytes().get(i) == Some(&b'&') {
            if let Some(len) = known_entity_len(&input[i..]) {
                out.push_str(&input[i..i + len]);
                i += len;
            } else {
                out.push_str("&amp;");
                i += 1;
            }
        } else {
            let rest = &input[i..];
            let Some(ch) = rest.chars().next() else {
                break;
            };
            out.push(ch);
            i += ch.len_utf8();
        }
    }
    out
}

/// Apply all fixes. Returns `(sanitized, applied_any_change)`.
pub fn preprocess_uiautomator_xml(xml: &str) -> (String, bool) {
    let s0 = strip_disallowed_xml_chars(xml);
    let s1 = escape_lt_gt_in_double_quoted_values(&s0);
    let s2 = replace_unknown_named_entities(&s1);
    let s3 = fix_bare_ampersands(&s2);
    let changed = s3 != xml;
    (s3, changed)
}

/// Map roxmltree 1-based row/col (character columns) to a UTF-8 byte offset.
pub fn text_pos_to_byte_offset(s: &str, pos: TextPos) -> Option<usize> {
    let mut row = 1u32;
    let mut col = 1u32;
    for (byte_idx, ch) in s.char_indices() {
        if row == pos.row && col == pos.col {
            return Some(byte_idx);
        }
        if ch == '\n' {
            row += 1;
            col = 1;
        } else {
            col += 1;
        }
    }
    if row == pos.row && col == pos.col {
        Some(s.len())
    } else {
        None
    }
}

fn snippet_around_byte(s: &str, byte_off: usize, radius: usize) -> String {
    let len = s.len();
    let start = byte_off.saturating_sub(radius);
    let end = (byte_off + radius).min(len);
    let start = s.floor_char_boundary(start);
    let end = s.ceil_char_boundary(end);
    let slice = &s[start..end];
    slice
        .chars()
        .map(|c| {
            if c.is_control() && c != '\n' {
                '\u{FFFD}'
            } else {
                c
            }
        })
        .collect()
}

/// Human-readable context for MCP / UI warnings when parsing still fails.
pub fn format_xml_parse_error(xml: &str, err: &XmlError) -> String {
    let pos = err.pos();
    let hint = match err {
        XmlError::UnknownEntityReference(name, _) => {
            format!(" (unknown entity &{name}; — try updating app text or sanitization)")
        }
        XmlError::MalformedEntityReference(_) => " (malformed &…; reference)".to_string(),
        XmlError::InvalidAttributeValue(_) => " (unescaped '<' in an attribute value?)".to_string(),
        XmlError::NonXmlChar(c, _) => format!(" (invalid XML character U+{:04X})", *c as u32),
        _ => String::new(),
    };
    if let Some(byte) = text_pos_to_byte_offset(xml, pos) {
        let snip = snippet_around_byte(xml, byte, 100);
        format!(
            "Parse error at line {} column {} (byte offset ~{}).{hint} Context: …{}…",
            pos.row, pos.col, byte, snip
        )
    } else {
        format!(
            "Parse error at line {} column {}.{hint} Could not map to byte offset (document may use unusual newlines).",
            pos.row, pos.col
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bare_ampersand_fixed() {
        let raw = r#"<hierarchy><node text="A & B" bounds="[0,0][1,1]"/></hierarchy>"#;
        let (s, _) = preprocess_uiautomator_xml(raw);
        assert!(roxmltree::Document::parse(&s).is_ok());
    }

    #[test]
    fn lt_in_attribute_fixed() {
        let raw = r#"<hierarchy><node text="1 < 2" bounds="[0,0][1,1]"/></hierarchy>"#;
        let (s, _) = preprocess_uiautomator_xml(raw);
        assert!(roxmltree::Document::parse(&s).is_ok());
    }

    #[test]
    fn nbsp_replaced() {
        let raw = r#"<hierarchy><node text="Hi&nbsp;there" bounds="[0,0][1,1]"/></hierarchy>"#;
        let (s, _) = preprocess_uiautomator_xml(raw);
        assert!(roxmltree::Document::parse(&s).is_ok());
    }

    #[test]
    fn preserves_valid_entities() {
        let raw = r#"<hierarchy><node text="a&amp;b&lt;c" bounds="[0,0][1,1]"/></hierarchy>"#;
        let (s, _) = preprocess_uiautomator_xml(raw);
        let doc = roxmltree::Document::parse(&s).expect("parse");
        let n = doc
            .descendants()
            .find(|n| n.has_tag_name("node"))
            .expect("node");
        assert_eq!(n.attribute("text"), Some("a&b<c"));
    }
}
