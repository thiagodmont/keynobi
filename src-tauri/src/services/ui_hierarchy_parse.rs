//! Parse UI Automator `uiautomator dump` XML into [`UiNode`] trees.
//!
//! Pure logic — unit-tested with fixtures (no adb).

use crate::models::ui_hierarchy::{UiInteractiveRow, UiNode};
use crate::services::ui_hierarchy_xml_sanitize::{format_xml_parse_error, preprocess_uiautomator_xml};
use roxmltree::Node;
use sha2::{Digest, Sha256};
use std::collections::BTreeSet;

/// Maximum nodes in the output tree (after parsing).
pub const MAX_NODES: usize = 8_000;
/// Maximum depth from the synthetic document root.
pub const MAX_DEPTH: usize = 64;
/// Truncate individual attribute strings to this many Unicode scalar values (characters).
pub const MAX_ATTR_LEN: usize = 2_048;

#[derive(Debug)]
pub struct ParseOutcome {
    pub root: UiNode,
    pub truncated: bool,
    pub warnings: Vec<String>,
    pub node_count: usize,
}

/// Parse hierarchy XML. On empty or invalid XML, returns a minimal placeholder root and warnings.
pub fn parse_hierarchy_xml(xml: &str) -> ParseOutcome {
    let mut warnings = Vec::new();
    let trimmed = xml.trim();
    if trimmed.is_empty() {
        warnings.push("Empty XML from device".to_string());
        return ParseOutcome {
            root: placeholder_root("Empty hierarchy"),
            truncated: false,
            warnings,
            node_count: 0,
        };
    }

    let (sanitized, sanitized_changed) = preprocess_uiautomator_xml(trimmed);
    if sanitized_changed {
        warnings.push(
            "UI hierarchy XML was sanitized (e.g. unescaped &, <, >, or HTML entities) before parsing"
                .to_string(),
        );
    }

    let doc = match roxmltree::Document::parse(&sanitized) {
        Ok(d) => d,
        Err(e) => {
            let detail = format_xml_parse_error(&sanitized, &e);
            warnings.push(format!("Invalid XML: {e}. {detail}"));
            return ParseOutcome {
                root: placeholder_root("Parse error"),
                truncated: false,
                warnings,
                node_count: 0,
            };
        }
    };

    let hierarchy = doc
        .descendants()
        .find(|n| n.is_element() && n.tag_name().name() == "hierarchy");

    let Some(hierarchy) = hierarchy else {
        warnings.push("No <hierarchy> element in dump".to_string());
        return ParseOutcome {
            root: placeholder_root("No hierarchy element"),
            truncated: false,
            warnings,
            node_count: 0,
        };
    };

    let mut counter = 0usize;
    let mut truncated = false;

    let mut children_out = Vec::new();
    for child in hierarchy.children().filter(|n| n.is_element()) {
        if child.tag_name().name() == "node" {
            if let Some(node) = parse_node_element(child, 1, &mut counter, &mut truncated, &mut warnings)
            {
                children_out.push(node);
            }
        }
    }

    if children_out.is_empty() {
        warnings.push("Hierarchy contained no <node> elements".to_string());
    }

    let root = UiNode {
        class: "android.view.KeynobiSyntheticRoot".to_string(),
        resource_id: String::new(),
        text: String::new(),
        content_desc: String::new(),
        package: String::new(),
        bounds: String::new(),
        clickable: false,
        enabled: true,
        focusable: false,
        focused: false,
        scrollable: false,
        long_clickable: false,
        password: false,
        checkable: false,
        checked: false,
        editable: false,
        selected: false,
        is_compose_heuristic: false,
        children: children_out,
    };

    ParseOutcome {
        root,
        truncated,
        warnings,
        node_count: counter,
    }
}

fn placeholder_root(message: &str) -> UiNode {
    UiNode {
        class: "android.view.KeynobiPlaceholder".to_string(),
        resource_id: String::new(),
        text: message.to_string(),
        content_desc: String::new(),
        package: String::new(),
        bounds: String::new(),
        clickable: false,
        enabled: true,
        focusable: false,
        focused: false,
        scrollable: false,
        long_clickable: false,
        password: false,
        checkable: false,
        checked: false,
        editable: false,
        selected: false,
        is_compose_heuristic: false,
        children: vec![],
    }
}

fn parse_node_element(
    el: Node<'_, '_>,
    depth: usize,
    counter: &mut usize,
    truncated: &mut bool,
    warnings: &mut Vec<String>,
) -> Option<UiNode> {
    if depth > MAX_DEPTH {
        if !*truncated {
            warnings.push(format!("Tree depth exceeded {MAX_DEPTH}; subtree skipped"));
            *truncated = true;
        }
        return None;
    }

    if *counter >= MAX_NODES {
        if !*truncated {
            warnings.push(format!("Node budget {MAX_NODES} reached; subtree skipped"));
            *truncated = true;
        }
        return None;
    }

    *counter += 1;

    let class = attr_str(el, "class");
    let resource_id = attr_str(el, "resource-id");
    let text = attr_str(el, "text");
    let content_desc = attr_str(el, "content-desc");
    let package = attr_str(el, "package");
    let bounds = attr_str(el, "bounds");

    let clickable = attr_bool(el, "clickable", false);
    let enabled = attr_bool(el, "enabled", true);
    let focusable = attr_bool(el, "focusable", false);
    let focused = attr_bool(el, "focused", false);
    let scrollable = attr_bool(el, "scrollable", false);
    let long_clickable = attr_bool(el, "long-clickable", false);
    let password = attr_bool(el, "password", false);
    let checkable = attr_bool(el, "checkable", false);
    let checked = attr_bool(el, "checked", false);
    let selected = attr_bool(el, "selected", false);
    let editable_attr = attr_bool(el, "editable", false);
    let editable = editable_attr
        || class.contains("EditText")
        || class.contains("AutoCompleteTextView");

    let is_compose_heuristic =
        class.contains("androidx.compose") || class.contains("ComposeView");

    let mut children = Vec::new();
    for child in el.children().filter(|n| n.is_element()) {
        if child.tag_name().name() != "node" {
            continue;
        }
        if *counter >= MAX_NODES {
            if !*truncated {
                warnings.push(format!("Node budget {MAX_NODES} reached; subtree skipped"));
                *truncated = true;
            }
            break;
        }
        if let Some(ch) = parse_node_element(child, depth + 1, counter, truncated, warnings) {
            children.push(ch);
        }
    }

    Some(UiNode {
        class,
        resource_id,
        text,
        content_desc,
        package,
        bounds,
        clickable,
        enabled,
        focusable,
        focused,
        scrollable,
        long_clickable,
        password,
        checkable,
        checked,
        editable,
        selected,
        is_compose_heuristic,
        children,
    })
}

fn attr_str(node: Node<'_, '_>, name: &str) -> String {
    let v = node.attribute(name).unwrap_or("").to_string();
    truncate_chars(&v, MAX_ATTR_LEN)
}

fn truncate_chars(s: &str, max_chars: usize) -> String {
    if s.chars().count() <= max_chars {
        return s.to_string();
    }
    s.chars().take(max_chars).collect::<String>()
}

fn attr_bool(node: Node<'_, '_>, name: &str, default: bool) -> bool {
    match node.attribute(name) {
        None => default,
        Some("true") => true,
        Some("false") => false,
        Some(_) => default,
    }
}

/// Count nodes that are “interactive” in the Droidclaw sense (tap/type/scroll targets or labeled).
pub fn count_interactive_nodes(root: &UiNode) -> u32 {
    let mut n = 0u32;
    walk_interactive(root, &mut |_| n += 1);
    n
}

fn walk_interactive(root: &UiNode, f: &mut impl FnMut(&UiNode)) {
    if is_interactive_row(root) {
        f(root);
    }
    for c in &root.children {
        walk_interactive(c, f);
    }
}

fn is_interactive_row(node: &UiNode) -> bool {
    let has_content = !node.text.is_empty() || !node.content_desc.is_empty();
    let interactive = node.clickable
        || node.long_clickable
        || node.scrollable
        || node.editable;
    if !interactive && !has_content {
        return false;
    }
    parse_bounds(node.bounds.as_str()).is_some_and(|(w, h)| w > 0 && h > 0)
}

/// Stable hash over interactive rows (sorted) — Droidclaw-style screen fingerprint.
pub fn compute_screen_hash(root: &UiNode) -> String {
    let mut parts: BTreeSet<String> = BTreeSet::new();
    walk_interactive(root, &mut |node| {
        let key = format!(
            "{}|{}|{}|{}|{}|{}",
            node.resource_id,
            node.text,
            node.content_desc,
            node.bounds,
            node.enabled,
            node.checked
        );
        parts.insert(key);
    });
    let joined = parts.into_iter().collect::<Vec<_>>().join(";");
    let digest = Sha256::digest(joined.as_bytes());
    format!("{digest:x}")
}

/// Extract flat interactive rows (depth-first), capped for MCP payloads.
pub fn extract_interactive_rows(root: &UiNode, limit: usize) -> Vec<UiInteractiveRow> {
    let mut out = Vec::new();
    walk_rows_limited(root, "root", 0, limit, &mut out);
    out
}

fn walk_rows_limited(
    node: &UiNode,
    parent_label: &str,
    depth: u32,
    limit: usize,
    out: &mut Vec<UiInteractiveRow>,
) {
    if out.len() >= limit {
        return;
    }

    let type_name = node
        .class
        .rsplit('.')
        .next()
        .unwrap_or(node.class.as_str())
        .to_string();
    let node_label = if !node.text.is_empty() {
        truncate_chars(&node.text, 80)
    } else if !node.content_desc.is_empty() {
        truncate_chars(&node.content_desc, 80)
    } else if !node.resource_id.is_empty() {
        node.resource_id.rsplit('/').next().unwrap_or("").to_string()
    } else {
        type_name.clone()
    };

    if is_interactive_row(node) {
        if let Some((cx, cy)) = center_from_bounds(&node.bounds) {
            out.push(UiInteractiveRow {
                class: node.class.clone(),
                resource_id: node.resource_id.clone(),
                text: node.text.clone(),
                content_desc: node.content_desc.clone(),
                bounds: node.bounds.clone(),
                center_x: cx,
                center_y: cy,
                clickable: node.clickable,
                editable: node.editable,
                scrollable: node.scrollable,
                enabled: node.enabled,
                parent_label: parent_label.to_string(),
                depth,
            });
        }
    }

    for c in &node.children {
        if out.len() >= limit {
            break;
        }
        walk_rows_limited(c, &node_label, depth + 1, limit, out);
    }
}

fn parse_bounds(bounds: &str) -> Option<(i32, i32)> {
    // "[0,0][1080,2400]" -> width, height
    let s: String = bounds.replace("][", ",").chars().filter(|&c| c != '[' && c != ']').collect();
    let parts: Vec<i32> = s.split(',').filter_map(|p| p.trim().parse().ok()).collect();
    if parts.len() != 4 {
        return None;
    }
    let w = parts[2] - parts[0];
    let h = parts[3] - parts[1];
    Some((w, h))
}

pub(crate) fn center_from_bounds(bounds: &str) -> Option<(i32, i32)> {
    let s: String = bounds.replace("][", ",").chars().filter(|&c| c != '[' && c != ']').collect();
    let parts: Vec<i32> = s.split(',').filter_map(|p| p.trim().parse().ok()).collect();
    if parts.len() != 4 {
        return None;
    }
    let x1 = parts[0];
    let y1 = parts[1];
    let x2 = parts[2];
    let y2 = parts[3];
    Some(((x1 + x2) / 2, (y1 + y2) / 2))
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE_VIEW: &str = include_str!("fixtures/ui_hierarchy_sample.xml");
    const SAMPLE_COMPOSE: &str = include_str!("fixtures/ui_hierarchy_compose.xml");

    #[test]
    fn parses_sample_view_tree() {
        let out = parse_hierarchy_xml(SAMPLE_VIEW);
        assert!(out.warnings.is_empty() || !out.warnings[0].contains("Invalid"));
        assert!(!out.root.children.is_empty());
        let first = &out.root.children[0];
        assert!(first.class.contains("FrameLayout"));
        assert!(!first.bounds.is_empty());
    }

    /// `uiautomator dump --compressed` emits minimal whitespace; parsing must still succeed.
    #[test]
    fn parses_minified_single_line_xml() {
        let xml = "<?xml version='1.0'?><hierarchy rotation=\"0\"><node index=\"0\" text=\"OK\" resource-id=\"\" class=\"android.widget.Button\" package=\"p\" content-desc=\"\" checkable=\"false\" checked=\"false\" clickable=\"true\" enabled=\"true\" focusable=\"true\" focused=\"false\" scrollable=\"false\" long-clickable=\"false\" password=\"false\" selected=\"false\" bounds=\"[0,0][100,100]\"/></hierarchy>";
        let out = parse_hierarchy_xml(xml);
        assert!(
            !out.root.children.is_empty(),
            "expected hierarchy child; warnings={:?}",
            out.warnings
        );
        let btn = &out.root.children[0];
        assert_eq!(btn.text, "OK");
        assert!(btn.clickable);
    }

    #[test]
    fn compose_fixture_marks_heuristic() {
        let out = parse_hierarchy_xml(SAMPLE_COMPOSE);
        let compose_node = find_first_compose_heuristic(&out.root);
        assert!(
            compose_node.is_some(),
            "expected androidx.compose or ComposeView in fixture"
        );
    }

    fn find_first_compose_heuristic(n: &UiNode) -> Option<&UiNode> {
        if n.is_compose_heuristic {
            return Some(n);
        }
        for c in &n.children {
            if let Some(x) = find_first_compose_heuristic(c) {
                return Some(x);
            }
        }
        None
    }

    #[test]
    fn screen_hash_stable() {
        let out = parse_hierarchy_xml(SAMPLE_VIEW);
        let a = compute_screen_hash(&out.root);
        let b = compute_screen_hash(&out.root);
        assert_eq!(a, b);
        assert_eq!(a.len(), 64);
    }

    #[test]
    fn interactive_rows_non_empty_for_button() {
        let out = parse_hierarchy_xml(SAMPLE_VIEW);
        let rows = extract_interactive_rows(&out.root, 100);
        assert!(!rows.is_empty());
    }

    #[test]
    fn parses_selected_true() {
        let xml = r#"<?xml version='1.0'?><hierarchy><node index="0" text="" resource-id="" class="android.view.View" package="p" content-desc="" checkable="false" checked="false" clickable="false" enabled="true" focusable="false" focused="false" scrollable="false" long-clickable="false" password="false" selected="true" bounds="[0,0][10,10]"/></hierarchy>"#;
        let out = parse_hierarchy_xml(xml);
        assert_eq!(out.root.children.len(), 1);
        assert!(out.root.children[0].selected);
    }
}
