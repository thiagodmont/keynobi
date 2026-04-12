//! UI automation via UI Automator hierarchy + `adb shell input` (tap, swipe, text, keyevent).
//!
//! Used by MCP tools. Hierarchy capture reuses [`crate::services::ui_hierarchy`].

use crate::models::ui_hierarchy::{UiHierarchySnapshot, UiNode};
use crate::services::ui_hierarchy;
use crate::services::ui_hierarchy_parse::center_from_bounds;
use schemars::JsonSchema;
use serde::Deserialize;
use std::collections::HashMap;
use std::path::PathBuf;
use std::time::Duration;
use tokio::process::Command;
use tokio::time::timeout;

/// Max matches returned by [`find_ui_elements`].
pub const MAX_FIND_RESULTS: usize = 100;
/// Default cap when `max_results` omitted.
pub const DEFAULT_FIND_RESULTS: usize = 50;
/// Upper bound for coordinates (device pixels).
pub const MAX_COORD: i32 = 16_384;
/// Max UTF-8 bytes for `input text` payload (ASCII-only enforced separately).
pub const MAX_INPUT_TEXT_BYTES: usize = 1_000;
/// Truncate long strings in match output (JSON size bound).
pub const MAX_MATCH_FIELD_CHARS: usize = 512;
/// Wall-clock limit for a single `input` / `keyevent` adb call.
pub const INPUT_CMD_TIMEOUT: Duration = Duration::from_secs(30);
/// Maximum `wait_for_element` timeout the caller may request.
pub const MAX_WAIT_TIMEOUT_MS: u32 = 30_000;
/// Default `wait_for_element` timeout.
pub const DEFAULT_WAIT_TIMEOUT_MS: u32 = 15_000;
/// Default poll interval for `wait_for_element`.
pub const DEFAULT_WAIT_POLL_MS: u32 = 500;

// ── Query + match types (MCP tool params use the same shapes via `FindUiElementsParams`) ────────

#[derive(Debug, Clone, Default, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct FindUiElementsParams {
    #[schemars(description = "ADB device serial (from list_devices). Uses first online device if omitted.")]
    pub device_serial: Option<String>,
    #[schemars(description = "Substring match on node text (case-insensitive).")]
    pub text_contains: Option<String>,
    #[schemars(description = "Exact text match (case-insensitive).")]
    pub text_equals: Option<String>,
    #[schemars(description = "Substring match on content-desc (case-insensitive).")]
    pub content_desc_contains: Option<String>,
    #[schemars(description = "Exact resource-id match.")]
    pub resource_id_equals: Option<String>,
    #[schemars(description = "Substring match on resource-id (case-insensitive).")]
    pub resource_id_contains: Option<String>,
    #[schemars(description = "Substring match on full class name (case-insensitive), e.g. Button.")]
    pub class_contains: Option<String>,
    #[schemars(description = "Exact package name match.")]
    pub package_equals: Option<String>,
    #[schemars(description = "If true, only nodes with clickable=true.")]
    pub clickable_only: Option<bool>,
    #[schemars(description = "If true, only nodes with editable=true.")]
    pub editable_only: Option<bool>,
    #[schemars(description = "If true, only nodes with enabled=true.")]
    pub enabled_only: Option<bool>,
    #[schemars(description = "If true (default), skip nodes with zero-area or invalid bounds.")]
    pub require_positive_bounds: Option<bool>,
    #[schemars(description = "Max matches to return (default 50, max 100).")]
    pub max_results: Option<u32>,
}

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UiElementMatch {
    pub tree_path: String,
    pub class: String,
    pub text: String,
    pub content_desc: String,
    pub resource_id: String,
    pub package: String,
    pub bounds: String,
    pub center_x: i32,
    pub center_y: i32,
    pub clickable: bool,
    pub editable: bool,
    pub enabled: bool,
    pub scrollable: bool,
    pub focusable: bool,
    pub focused: bool,
}

#[derive(Debug, Clone, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct UiTapParams {
    #[schemars(description = "ADB device serial. Uses first online device if omitted.")]
    pub device_serial: Option<String>,
    #[schemars(description = "X in device pixels (from find_ui_elements centerX).")]
    pub x: i32,
    #[schemars(description = "Y in device pixels (from find_ui_elements centerY).")]
    pub y: i32,
    #[schemars(description = "If set, capture hierarchy first and refuse to tap unless screenHash matches.")]
    pub expect_screen_hash: Option<String>,
}

#[derive(Debug, Clone, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct UiTypeTextParams {
    #[schemars(description = "ADB device serial. Uses first online device if omitted.")]
    pub device_serial: Option<String>,
    #[schemars(
        description = "Text to send via adb shell input text. ASCII printable recommended; space as %s encoding; no emoji/unicode. Tap a field first or use tap_x/tap_y."
    )]
    pub text: String,
    #[schemars(description = "Optional tap before typing to focus an editable (device pixels).")]
    pub tap_x: Option<i32>,
    #[schemars(description = "Optional tap before typing to focus an editable (device pixels).")]
    pub tap_y: Option<i32>,
    #[schemars(description = "If set, capture hierarchy first and refuse unless screenHash matches (before tap/type).")]
    pub expect_screen_hash: Option<String>,
    #[schemars(
        description = "If true, send Ctrl+A then Delete before typing to clear existing field content (default false)."
    )]
    pub clear_before: Option<bool>,
}

#[derive(Debug, Clone, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ClearFocusedInputParams {
    #[schemars(description = "ADB device serial. Uses first online device if omitted.")]
    pub device_serial: Option<String>,
    #[schemars(description = "Optional tap to focus an editable before clearing (device pixels).")]
    pub tap_x: Option<i32>,
    #[schemars(description = "Optional tap to focus an editable before clearing (device pixels).")]
    pub tap_y: Option<i32>,
}

#[derive(Debug, Clone, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct UiTypeTextUnicodeParams {
    #[schemars(description = "ADB device serial. Uses first online device if omitted.")]
    pub device_serial: Option<String>,
    #[schemars(
        description = "Unicode text to type. Supports emoji and non-ASCII. Uses clipboard broadcast (requires Android 7+ / API 24). Tap the field first or use tap_x/tap_y."
    )]
    pub text: String,
    #[schemars(description = "Optional tap to focus an editable before typing (device pixels).")]
    pub tap_x: Option<i32>,
    #[schemars(description = "Optional tap to focus an editable before typing (device pixels).")]
    pub tap_y: Option<i32>,
    #[schemars(description = "If true, clear the field with Ctrl+A + Delete before typing (default false).")]
    pub clear_before: Option<bool>,
}

#[derive(Debug, Clone, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct SendUiKeyParams {
    #[schemars(description = "ADB device serial. Uses first online device if omitted.")]
    pub device_serial: Option<String>,
    #[schemars(
        description = "Key name (case-insensitive): Back, Home, Enter, Delete, Tab, Escape, Search, Menu, AppSwitch, DpadUp, DpadDown, DpadLeft, DpadRight, DpadCenter."
    )]
    pub key: String,
}

#[derive(Debug, Clone, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct FindUiParentParams {
    #[schemars(description = "ADB device serial. Uses first online device if omitted.")]
    pub device_serial: Option<String>,
    #[schemars(
        description = "Layout tree path from find_ui_elements or the Layout tab (root-relative: \"0\", \"0.1.2\"). Must be non-empty; the display root has no parent."
    )]
    pub tree_path: String,
    #[schemars(
        description = "If set, capture hierarchy first and refuse unless screenHash matches (same contract as ui_tap)."
    )]
    pub expect_screen_hash: Option<String>,
}

#[derive(Debug, Clone, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct UiSwipeParams {
    #[schemars(description = "ADB device serial. Uses first online device if omitted.")]
    pub device_serial: Option<String>,
    pub x1: i32,
    pub y1: i32,
    pub x2: i32,
    pub y2: i32,
    #[schemars(description = "Duration in ms (optional). Same start/end + duration = long-press.")]
    pub duration_ms: Option<u32>,
}

#[derive(Debug, Clone, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct GrantRuntimePermissionParams {
    #[schemars(description = "ADB device serial. Uses first online device if omitted.")]
    pub device_serial: Option<String>,
    #[schemars(description = "Application package name, e.g. com.example.app")]
    pub package: String,
    #[schemars(description = "Android permission, e.g. android.permission.CAMERA")]
    pub permission: String,
}

#[derive(Debug, Clone, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct WaitForElementParams {
    #[schemars(description = "ADB device serial. Uses first online device if omitted.")]
    pub device_serial: Option<String>,
    #[schemars(description = "Substring match on node text (case-insensitive).")]
    pub text_contains: Option<String>,
    #[schemars(description = "Exact text match (case-insensitive).")]
    pub text_equals: Option<String>,
    #[schemars(description = "Substring match on content-desc (case-insensitive).")]
    pub content_desc_contains: Option<String>,
    #[schemars(description = "Exact resource-id match.")]
    pub resource_id_equals: Option<String>,
    #[schemars(description = "Substring match on resource-id (case-insensitive).")]
    pub resource_id_contains: Option<String>,
    #[schemars(description = "Substring match on full class name (case-insensitive).")]
    pub class_contains: Option<String>,
    #[schemars(description = "Exact package name match.")]
    pub package_equals: Option<String>,
    #[schemars(description = "If true, only nodes with clickable=true.")]
    pub clickable_only: Option<bool>,
    #[schemars(description = "If true, only nodes with enabled=true.")]
    pub enabled_only: Option<bool>,
    #[schemars(
        description = "Total wait timeout in milliseconds (default 15000, max 30000)."
    )]
    pub timeout_ms: Option<u32>,
    #[schemars(description = "Poll interval in milliseconds (default 500, min 200).")]
    pub poll_interval_ms: Option<u32>,
}

/// Poll until at least one element matching `params` appears or `timeout_ms` elapses.
/// Returns `Ok((snapshot, matches))` on success, `Err` on timeout.
pub async fn wait_for_element(
    adb: &PathBuf,
    serial: &str,
    params: &WaitForElementParams,
) -> Result<(UiHierarchySnapshot, Vec<UiElementMatch>), String> {
    let timeout_ms = params
        .timeout_ms
        .unwrap_or(DEFAULT_WAIT_TIMEOUT_MS)
        .min(MAX_WAIT_TIMEOUT_MS);
    let poll_ms = params
        .poll_interval_ms
        .unwrap_or(DEFAULT_WAIT_POLL_MS)
        .max(200);

    let q = FindUiElementsParams {
        device_serial: params.device_serial.clone(),
        text_contains: params.text_contains.clone(),
        text_equals: params.text_equals.clone(),
        content_desc_contains: params.content_desc_contains.clone(),
        resource_id_equals: params.resource_id_equals.clone(),
        resource_id_contains: params.resource_id_contains.clone(),
        class_contains: params.class_contains.clone(),
        package_equals: params.package_equals.clone(),
        clickable_only: params.clickable_only,
        editable_only: None,
        enabled_only: params.enabled_only,
        require_positive_bounds: Some(true),
        max_results: Some(DEFAULT_FIND_RESULTS as u32),
    };

    if !find_query_has_primary_filter(&q) {
        return Err(
            "wait_for_element requires at least one primary filter (textContains, textEquals, \
             contentDescContains, resourceIdEquals, resourceIdContains, classContains, or packageEquals)."
                .to_string(),
        );
    }

    let deadline = std::time::Instant::now() + Duration::from_millis(u64::from(timeout_ms));

    loop {
        let snap = capture_ui_snapshot(adb, serial).await?;
        let matches = find_ui_elements(&snap, &q, DEFAULT_FIND_RESULTS);
        if !matches.is_empty() {
            return Ok((snap, matches));
        }
        if std::time::Instant::now() >= deadline {
            return Err(format!(
                "wait_for_element timed out after {timeout_ms} ms — element not found. \
                 Last screenHash: {}",
                snap.screen_hash
            ));
        }
        tokio::time::sleep(Duration::from_millis(u64::from(poll_ms))).await;
    }
}

/// Capture hierarchy like the Layout tab / `get_ui_hierarchy`.
pub async fn capture_ui_snapshot(adb: &PathBuf, serial: &str) -> Result<UiHierarchySnapshot, String> {
    ui_hierarchy::capture_ui_hierarchy_snapshot(adb, serial).await
}

/// Returns error if `expect_screen_hash` is set and does not match current screen.
pub async fn ensure_screen_hash(
    adb: &PathBuf,
    serial: &str,
    expect_screen_hash: Option<&str>,
) -> Result<UiHierarchySnapshot, String> {
    let snap = capture_ui_snapshot(adb, serial).await?;
    if let Some(expected) = expect_screen_hash {
        if expected != snap.screen_hash {
            return Err(format!(
                "screenHash mismatch: expected {expected}, got {} — UI changed; call find_ui_elements again",
                snap.screen_hash
            ));
        }
    }
    Ok(snap)
}

fn truncate_field(s: &str) -> String {
    let t: String = s.chars().take(MAX_MATCH_FIELD_CHARS).collect();
    if t.len() < s.len() {
        format!("{t}…")
    } else {
        t
    }
}

fn positive_bounds(bounds: &str) -> bool {
    let s = bounds.replace("][", ",").replace(['[', ']'], "");
    let parts: Vec<i32> = s.split(',').filter_map(|p| p.trim().parse().ok()).collect();
    if parts.len() != 4 {
        return false;
    }
    let w = parts[2] - parts[0];
    let h = parts[3] - parts[1];
    w > 0 && h > 0
}

fn node_matches_query(node: &UiNode, q: &FindUiElementsParams, require_bounds: bool) -> bool {
    if require_bounds && !positive_bounds(&node.bounds) {
        return false;
    }
    if q.clickable_only == Some(true) && !node.clickable {
        return false;
    }
    if q.editable_only == Some(true) && !node.editable {
        return false;
    }
    if q.enabled_only == Some(true) && !node.enabled {
        return false;
    }

    if let Some(ref want) = q.text_equals {
        if want.is_empty() {
            return false;
        }
        if !node.text.eq_ignore_ascii_case(want) {
            return false;
        }
    }
    if let Some(ref sub) = q.text_contains {
        if sub.is_empty() {
            return false;
        }
        if !node.text.to_lowercase().contains(&sub.to_lowercase()) {
            return false;
        }
    }
    if let Some(ref sub) = q.content_desc_contains {
        if sub.is_empty() {
            return false;
        }
        if !node
            .content_desc
            .to_lowercase()
            .contains(&sub.to_lowercase())
        {
            return false;
        }
    }
    if let Some(ref id) = q.resource_id_equals {
        if id.is_empty() {
            return false;
        }
        if node.resource_id != *id {
            return false;
        }
    }
    if let Some(ref sub) = q.resource_id_contains {
        if sub.is_empty() {
            return false;
        }
        if !node
            .resource_id
            .to_lowercase()
            .contains(&sub.to_lowercase())
        {
            return false;
        }
    }
    if let Some(ref sub) = q.class_contains {
        if sub.is_empty() {
            return false;
        }
        if !node.class.to_lowercase().contains(&sub.to_lowercase()) {
            return false;
        }
    }
    if let Some(ref pkg) = q.package_equals {
        if pkg.is_empty() {
            return false;
        }
        if node.package != *pkg {
            return false;
        }
    }

    true
}

/// True if at least one "primary" filter is set (non-empty).
pub fn find_query_has_primary_filter(q: &FindUiElementsParams) -> bool {
    q.text_contains.as_ref().is_some_and(|s| !s.is_empty())
        || q.text_equals.as_ref().is_some_and(|s| !s.is_empty())
        || q.content_desc_contains
            .as_ref()
            .is_some_and(|s| !s.is_empty())
        || q.resource_id_equals.as_ref().is_some_and(|s| !s.is_empty())
        || q.resource_id_contains
            .as_ref()
            .is_some_and(|s| !s.is_empty())
        || q.class_contains.as_ref().is_some_and(|s| !s.is_empty())
        || q.package_equals.as_ref().is_some_and(|s| !s.is_empty())
}

fn push_match(node: &UiNode, tree_path: String, out: &mut Vec<UiElementMatch>) {
    let Some((cx, cy)) = center_from_bounds(&node.bounds) else {
        return;
    };
    out.push(UiElementMatch {
        tree_path,
        class: truncate_field(&node.class),
        text: truncate_field(&node.text),
        content_desc: truncate_field(&node.content_desc),
        resource_id: truncate_field(&node.resource_id),
        package: truncate_field(&node.package),
        bounds: truncate_field(&node.bounds),
        center_x: cx,
        center_y: cy,
        clickable: node.clickable,
        editable: node.editable,
        enabled: node.enabled,
        scrollable: node.scrollable,
        focusable: node.focusable,
        focused: node.focused,
    });
}

fn walk_find(
    node: &UiNode,
    path: &str,
    q: &FindUiElementsParams,
    require_bounds: bool,
    max: usize,
    out: &mut Vec<UiElementMatch>,
) {
    if out.len() >= max {
        return;
    }
    if node_matches_query(node, q, require_bounds) {
        push_match(node, path.to_string(), out);
    }
    if out.len() >= max {
        return;
    }
    for (i, c) in node.children.iter().enumerate() {
        if out.len() >= max {
            break;
        }
        let child_path = if path.is_empty() {
            i.to_string()
        } else {
            format!("{path}.{i}")
        };
        walk_find(c, &child_path, q, require_bounds, max, out);
    }
}

/// DFS from snapshot root; caps at `max_matches` (clamped to [`MAX_FIND_RESULTS`]).
/// Without a primary text/id/class/package filter, returns no rows (modifiers alone are not enough).
pub fn find_ui_elements(
    snapshot: &UiHierarchySnapshot,
    q: &FindUiElementsParams,
    max_matches: usize,
) -> Vec<UiElementMatch> {
    if !find_query_has_primary_filter(q) {
        return Vec::new();
    }
    let max = max_matches.clamp(1, MAX_FIND_RESULTS);
    let require_bounds = q.require_positive_bounds.unwrap_or(true);
    let mut out = Vec::new();
    walk_find(
        &snapshot.root,
        "",
        q,
        require_bounds,
        max,
        &mut out,
    );
    out
}

/// Normalizes a layout `tree_path`: trim, drop empty dot segments; each segment must be a non-negative integer index.
pub fn normalize_tree_path(raw: &str) -> Result<String, String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Err(
            "tree_path must be non-empty — the layout root has no parent in the tree.".to_string(),
        );
    }
    let parts: Vec<&str> = trimmed
        .split('.')
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .collect();
    if parts.is_empty() {
        return Err("tree_path must contain at least one segment.".to_string());
    }
    for p in &parts {
        if p.parse::<usize>().is_err() {
            return Err(format!(
                "invalid tree_path segment {p:?}: expected non-negative integer index"
            ));
        }
    }
    Ok(parts.join("."))
}

/// Resolve `path` from display-tree root (`""` = root; `"0.1"` = first child’s second child). Same indexing as `find_ui_elements`.
pub fn get_node_at_path<'a>(root: &'a UiNode, path: &str) -> Option<&'a UiNode> {
    if path.is_empty() {
        return Some(root);
    }
    let segments: Vec<&str> = path.split('.').filter(|s| !s.is_empty()).collect();
    let mut cur = root;
    for seg in segments {
        let idx: usize = seg.parse().ok()?;
        cur = cur.children.get(idx)?;
    }
    Some(cur)
}

/// Direct parent path: `None` only for `""` (display root). Otherwise `Some("")` means parent is the root row.
pub fn parent_layout_path(path: &str) -> Option<String> {
    if path.is_empty() {
        return None;
    }
    let parts: Vec<&str> = path.split('.').filter(|s| !s.is_empty()).collect();
    if parts.len() <= 1 {
        Some(String::new())
    } else {
        Some(parts[..parts.len() - 1].join("."))
    }
}

/// Parameters for `compare_ui_state` MCP tool.
#[derive(Debug, Clone, Default, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct CompareUiStateParams {
    #[schemars(description = "ADB device serial (from list_devices). Uses first online device if omitted.")]
    pub device_serial: Option<String>,
    #[schemars(description = "screenHash from a previous find_ui_elements, dump_ui_hierarchy, or compare_ui_state response.")]
    pub baseline_screen_hash: String,
    #[schemars(description = "Max interactive nodes to return when state changed (default 30, max 100).")]
    pub max_results: Option<u32>,
}

/// Collect all interactive (clickable or editable) nodes up to `max` from the snapshot root.
pub fn collect_interactive_nodes(
    snapshot: &UiHierarchySnapshot,
    max: usize,
) -> Vec<UiElementMatch> {
    let max = max.clamp(1, MAX_FIND_RESULTS);
    let mut out = Vec::new();
    walk_interactive(&snapshot.root, "", max, &mut out);
    out
}

fn walk_interactive(node: &UiNode, path: &str, max: usize, out: &mut Vec<UiElementMatch>) {
    if out.len() >= max {
        return;
    }
    if (node.clickable || node.editable) && center_from_bounds(&node.bounds).is_some() {
        push_match(node, path.to_string(), out);
    }
    for (i, c) in node.children.iter().enumerate() {
        if out.len() >= max {
            break;
        }
        let child_path = if path.is_empty() {
            i.to_string()
        } else {
            format!("{path}.{i}")
        };
        walk_interactive(c, &child_path, max, out);
    }
}

fn ui_element_match_from_node(node: &UiNode, tree_path: String) -> UiElementMatch {
    let (center_x, center_y) = center_from_bounds(&node.bounds).unwrap_or((0, 0));
    UiElementMatch {
        tree_path,
        class: truncate_field(&node.class),
        text: truncate_field(&node.text),
        content_desc: truncate_field(&node.content_desc),
        resource_id: truncate_field(&node.resource_id),
        package: truncate_field(&node.package),
        bounds: truncate_field(&node.bounds),
        center_x,
        center_y,
        clickable: node.clickable,
        editable: node.editable,
        enabled: node.enabled,
        scrollable: node.scrollable,
        focusable: node.focusable,
        focused: node.focused,
    }
}

/// Resolves the direct parent of the node at `tree_path` in `snapshot.root` (same tree as `find_ui_elements` / Layout viewer).
pub fn find_ui_parent_from_snapshot(
    snapshot: &UiHierarchySnapshot,
    tree_path_raw: &str,
) -> Result<(String, UiElementMatch), String> {
    let normalized = normalize_tree_path(tree_path_raw)?;
    get_node_at_path(&snapshot.root, &normalized).ok_or_else(|| {
        format!(
            "no node at tree_path {normalized:?} in current hierarchy; call find_ui_elements or get_ui_hierarchy again"
        )
    })?;
    let parent_path = parent_layout_path(&normalized)
        .expect("non-empty normalized path always has a defined parent path");
    let parent_node = get_node_at_path(&snapshot.root, &parent_path).ok_or_else(|| {
        format!("internal: missing parent node at tree_path {parent_path:?}")
    })?;
    Ok((
        normalized,
        ui_element_match_from_node(parent_node, parent_path),
    ))
}

pub fn validate_coordinates(x: i32, y: i32) -> Result<(), String> {
    if x < 0 || y < 0 || x > MAX_COORD || y > MAX_COORD {
        return Err(format!(
            "coordinates out of range: ({x},{y}); expected 0..={MAX_COORD}"
        ));
    }
    Ok(())
}

/// Reject a lone `tap_x` or `tap_y` so callers do not skip the tap and act on the wrong focus target.
pub fn validate_tap_coordinate_pair(tap_x: Option<i32>, tap_y: Option<i32>) -> Result<(), String> {
    match (tap_x, tap_y) {
        (Some(_), None) | (None, Some(_)) => Err(
            "tap_x and tap_y must both be set or both omitted".to_string(),
        ),
        _ => Ok(()),
    }
}

/// Android `adb shell input text` encoding: `%` → `%%`, space → `%s`. ASCII printable only.
pub fn encode_adb_input_text(text: &str) -> Result<String, String> {
    if text.len() > MAX_INPUT_TEXT_BYTES {
        return Err(format!(
            "text too long (max {MAX_INPUT_TEXT_BYTES} bytes)"
        ));
    }
    let mut out = String::with_capacity(text.len() * 2);
    for ch in text.chars() {
        if ch > '\x7f' {
            return Err(
                "non-ASCII characters are not supported by adb input text; use ASCII or paste another way"
                    .to_string(),
            );
        }
        match ch {
            '%' => out.push_str("%%"),
            ' ' => out.push_str("%s"),
            _ if ch.is_ascii_control() => {
                return Err(format!(
                    "control character U+{:04X} not allowed in input text",
                    ch as u32
                ));
            }
            _ => out.push(ch),
        }
    }
    Ok(out)
}

fn keyevent_map() -> HashMap<&'static str, i32> {
    [
        ("back", 4),
        ("home", 3),
        ("enter", 66),
        ("delete", 67),
        ("tab", 61),
        ("escape", 111),
        ("search", 84),
        ("menu", 82),
        ("appswitch", 187),
        ("dpadup", 19),
        ("dpaddown", 20),
        ("dpadleft", 21),
        ("dpadright", 22),
        ("dpadcenter", 23),
    ]
    .into_iter()
    .collect()
}

/// Resolve allowlisted key name to Android keyevent code.
pub fn resolve_ui_key_code(key: &str) -> Result<i32, String> {
    let k = key.trim().to_lowercase().replace('_', "");
    let map = keyevent_map();
    map.get(k.as_str())
        .copied()
        .ok_or_else(|| {
            format!(
                "unknown key '{key}'; use Back, Home, Enter, Delete, Tab, Escape, Search, Menu, AppSwitch, DpadUp, DpadDown, DpadLeft, DpadRight, DpadCenter"
            )
        })
}

pub fn validate_runtime_permission(permission: &str) -> Result<(), String> {
    if !permission.starts_with("android.permission.") {
        return Err("permission must start with android.permission.".to_string());
    }
    let rest = &permission["android.permission.".len()..];
    if rest.is_empty() {
        return Err("permission suffix empty".to_string());
    }
    let ok = rest
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '_');
    if !ok {
        return Err("permission contains invalid characters".to_string());
    }
    Ok(())
}

async fn run_adb_shell(
    adb: &PathBuf,
    serial: &str,
    args: &[&str],
) -> Result<String, String> {
    let out = timeout(
        INPUT_CMD_TIMEOUT,
        Command::new(adb)
            .args(["-s", serial, "shell"])
            .args(args)
            .output(),
    )
    .await
    .map_err(|_| "adb shell timed out".to_string())?
    .map_err(|e| format!("adb failed: {e}"))?;

    let stdout = String::from_utf8_lossy(&out.stdout).trim().to_string();
    let stderr = String::from_utf8_lossy(&out.stderr).trim().to_string();
    let combined = if stderr.is_empty() {
        stdout
    } else if stdout.is_empty() {
        stderr
    } else {
        format!("{stdout}\n{stderr}")
    };

    if !out.status.success() {
        return Err(if combined.is_empty() {
            format!("adb shell exited with status {:?}", out.status.code())
        } else {
            combined
        });
    }
    Ok(combined)
}

pub async fn adb_input_tap(adb: &PathBuf, serial: &str, x: i32, y: i32) -> Result<String, String> {
    validate_coordinates(x, y)?;
    run_adb_shell(
        adb,
        serial,
        &[
            "input",
            "tap",
            &x.to_string(),
            &y.to_string(),
        ],
    )
    .await
}

pub async fn adb_input_swipe(
    adb: &PathBuf,
    serial: &str,
    x1: i32,
    y1: i32,
    x2: i32,
    y2: i32,
    duration_ms: Option<u32>,
) -> Result<String, String> {
    validate_coordinates(x1, y1)?;
    validate_coordinates(x2, y2)?;
    let mut args: Vec<String> = vec![
        "input".to_string(),
        "swipe".to_string(),
        x1.to_string(),
        y1.to_string(),
        x2.to_string(),
        y2.to_string(),
    ];
    if let Some(d) = duration_ms {
        if d > 30_000 {
            return Err("duration_ms must be at most 30000".to_string());
        }
        args.push(d.to_string());
    }
    let args_ref: Vec<&str> = args.iter().map(|s| s.as_str()).collect();
    run_adb_shell(adb, serial, &args_ref).await
}

pub async fn adb_input_text(adb: &PathBuf, serial: &str, text: &str) -> Result<String, String> {
    let encoded = encode_adb_input_text(text)?;
    run_adb_shell(adb, serial, &["input", "text", &encoded]).await
}

pub async fn adb_keyevent(adb: &PathBuf, serial: &str, keycode: i32) -> Result<String, String> {
    if !(0..=300).contains(&keycode) {
        return Err("keycode out of range".to_string());
    }
    run_adb_shell(adb, serial, &["input", "keyevent", &keycode.to_string()]).await
}

/// Shell pipeline to set the device clipboard: try Clipper broadcast, then `content insert`.
/// Must not end with a forced success (`|| true`); otherwise paste can succeed while the
/// clipboard still holds stale text.
fn unicode_clipboard_set_shell(escaped_text: &str) -> String {
    format!(
        "am broadcast -a clipper.set -e text '{escaped_text}' 2>/dev/null || \
         content insert --uri content://com.android.providers.clipboard/primary \
           --bind text:s:'{escaped_text}' 2>/dev/null"
    )
}

/// Type Unicode text into the focused field via clipboard paste.
///
/// Strategy: write text to the device clipboard via `content insert`, then paste with Ctrl+V.
/// This works on API 24+ without requiring a custom broadcast receiver.
/// The text is shell-escaped to prevent injection.
pub async fn adb_type_text_unicode(
    adb: &PathBuf,
    serial: &str,
    text: &str,
) -> Result<String, String> {
    if text.is_empty() {
        return Err("text must not be empty".to_string());
    }
    if text.len() > MAX_INPUT_TEXT_BYTES * 4 {
        return Err(format!(
            "text too long (max {} bytes)",
            MAX_INPUT_TEXT_BYTES * 4
        ));
    }
    // Shell-escape the text to prevent injection: replace ' with '"'"'
    let escaped = text.replace('\'', "'\"'\"'");

    let clip_cmd = unicode_clipboard_set_shell(&escaped);
    run_adb_shell(adb, serial, &["sh", "-c", &clip_cmd]).await?;

    // Paste via Ctrl+V (META_CTRL_ON=4096, KEYCODE_V=50).
    run_adb_shell(
        adb,
        serial,
        &["input", "keyevent", "--metastate", "4096", "50"],
    )
    .await
}

pub async fn adb_pm_grant(
    adb: &PathBuf,
    serial: &str,
    package: &str,
    permission: &str,
) -> Result<String, String> {
    validate_runtime_permission(permission)?;
    run_adb_shell(adb, serial, &["pm", "grant", package, permission]).await
}

/// Select all text in the focused field (Ctrl+A) then delete it.
/// Uses `--metastate 0x1000` (META_CTRL_ON) with KEYCODE_A (29) for a true Ctrl+A chord.
/// Best-effort: errors from select-all are swallowed since the field may be empty.
pub async fn adb_clear_field(adb: &PathBuf, serial: &str) -> Result<(), String> {
    // Ctrl+A: META_CTRL_ON (0x1000 = 4096) + KEYCODE_A (29) — selects all text in a focused field.
    // Supported on API 11+ / all modern emulators.
    let _ = run_adb_shell(
        adb,
        serial,
        &["input", "keyevent", "--metastate", "4096", "29"],
    )
    .await;
    // KEYCODE_DEL (67) deletes the selection (or does nothing if selection is empty).
    run_adb_shell(adb, serial, &["input", "keyevent", "67"]).await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::ui_hierarchy::UiLayoutContext;
    use crate::services::ui_hierarchy_parse::parse_hierarchy_xml;

    const SAMPLE_VIEW: &str = include_str!("fixtures/ui_hierarchy_sample.xml");

    #[test]
    fn encode_space_and_percent() {
        assert_eq!(encode_adb_input_text("a b").unwrap(), "a%sb");
        assert_eq!(encode_adb_input_text("100%").unwrap(), "100%%");
    }

    #[test]
    fn encode_rejects_unicode() {
        assert!(encode_adb_input_text("café").is_err());
    }

    #[test]
    fn validate_tap_coordinate_pair_accepts_both_or_neither() {
        assert!(validate_tap_coordinate_pair(None, None).is_ok());
        assert!(validate_tap_coordinate_pair(Some(0), Some(0)).is_ok());
    }

    #[test]
    fn validate_tap_coordinate_pair_rejects_lone_axis() {
        assert!(validate_tap_coordinate_pair(Some(10), None).is_err());
        assert!(validate_tap_coordinate_pair(None, Some(20)).is_err());
    }

    #[test]
    fn unicode_clipboard_shell_tries_broadcast_then_content_insert_without_forcing_success() {
        let cmd = unicode_clipboard_set_shell("hello");
        assert!(
            !cmd.contains("|| true"),
            "clipboard pipeline must not mask failures or paste can insert stale clipboard text"
        );
        assert!(cmd.contains("clipper.set"));
        assert!(cmd.contains("content://com.android.providers.clipboard/primary"));
        assert!(cmd.contains("'hello'"));
    }

    #[test]
    fn unicode_clipboard_shell_escapes_apostrophe_for_single_quoted_segments() {
        let escaped = "it's".replace('\'', "'\"'\"'");
        let cmd = unicode_clipboard_set_shell(&escaped);
        assert!(cmd.contains(&format!("'{escaped}'")));
    }

    #[test]
    fn find_by_text_ok_button() {
        let out = parse_hierarchy_xml(SAMPLE_VIEW);
        let snap = UiHierarchySnapshot {
            captured_at: "t".to_string(),
            truncated: false,
            warnings: vec![],
            root: out.root,
            screen_hash: "h".to_string(),
            interactive_count: 0,
            foreground_activity: None,
            layout_context: UiLayoutContext::default(),
            command_log: vec![],
            screenshot_b64: None,
        };
        let q = FindUiElementsParams {
            text_equals: Some("OK".to_string()),
            ..Default::default()
        };
        assert!(find_query_has_primary_filter(&q));
        let m = find_ui_elements(&snap, &q, 10);
        assert_eq!(m.len(), 1);
        assert_eq!(m[0].center_x, (800 + 1032) / 2);
        assert_eq!(m[0].clickable, true);
        assert!(m[0].tree_path.len() > 0);
    }

    #[test]
    fn find_requires_primary_filter_contract() {
        let out = parse_hierarchy_xml(SAMPLE_VIEW);
        let snap = UiHierarchySnapshot {
            captured_at: "t".to_string(),
            truncated: false,
            warnings: vec![],
            root: out.root,
            screen_hash: "h".to_string(),
            interactive_count: 0,
            foreground_activity: None,
            layout_context: UiLayoutContext::default(),
            command_log: vec![],
            screenshot_b64: None,
        };
        let q = FindUiElementsParams {
            clickable_only: Some(true),
            ..Default::default()
        };
        assert!(!find_query_has_primary_filter(&q));
        let m = find_ui_elements(&snap, &q, 10);
        assert!(m.is_empty());
    }

    #[test]
    fn resolve_back() {
        assert_eq!(resolve_ui_key_code("Back").unwrap(), 4);
        assert_eq!(resolve_ui_key_code("DPAD_UP").unwrap(), 19);
    }

    #[test]
    fn find_by_content_desc() {
        let out = parse_hierarchy_xml(SAMPLE_VIEW);
        let snap = UiHierarchySnapshot {
            captured_at: "t".to_string(),
            truncated: false,
            warnings: vec![],
            root: out.root,
            screen_hash: "h".to_string(),
            interactive_count: 0,
            foreground_activity: None,
            layout_context: UiLayoutContext::default(),
            command_log: vec![],
            screenshot_b64: None,
        };
        let q = FindUiElementsParams {
            content_desc_contains: Some("Confirm".to_string()),
            ..Default::default()
        };
        let m = find_ui_elements(&snap, &q, 10);
        assert_eq!(m.len(), 1);
        assert_eq!(m[0].text, "OK");
    }

    #[test]
    fn validate_permission_accepts_camera() {
        assert!(validate_runtime_permission("android.permission.CAMERA").is_ok());
        assert!(validate_runtime_permission("android.permission.POST_NOTIFICATIONS").is_ok());
        assert!(validate_runtime_permission("com.app.CUSTOM").is_err());
    }

    fn sample_snapshot() -> UiHierarchySnapshot {
        let out = parse_hierarchy_xml(SAMPLE_VIEW);
        UiHierarchySnapshot {
            captured_at: "t".to_string(),
            truncated: false,
            warnings: vec![],
            root: out.root,
            screen_hash: "h".to_string(),
            interactive_count: 0,
            foreground_activity: None,
            layout_context: UiLayoutContext::default(),
            command_log: vec![],
            screenshot_b64: None,
        }
    }

    #[test]
    fn normalize_tree_path_trims_and_drops_empty_segments() {
        assert_eq!(normalize_tree_path(" 0.1 ").unwrap(), "0.1");
        assert_eq!(normalize_tree_path(".0..1.").unwrap(), "0.1");
        assert!(normalize_tree_path("").is_err());
        assert!(normalize_tree_path("   ").is_err());
        assert!(normalize_tree_path("0.x").is_err());
    }

    #[test]
    fn parent_layout_path_matches_layout_viewer() {
        assert_eq!(parent_layout_path(""), None);
        assert_eq!(parent_layout_path("0").as_deref(), Some(""));
        assert_eq!(parent_layout_path("0.1.2").as_deref(), Some("0.1"));
    }

    #[test]
    fn get_node_at_path_walks_children() {
        let snap = sample_snapshot();
        assert!(get_node_at_path(&snap.root, "").is_some());
        let leaf_path = {
            let q = FindUiElementsParams {
                text_equals: Some("OK".to_string()),
                ..Default::default()
            };
            find_ui_elements(&snap, &q, 1)
                .into_iter()
                .next()
                .map(|m| m.tree_path)
                .expect("OK node")
        };
        let n = get_node_at_path(&snap.root, &leaf_path).expect("leaf");
        assert_eq!(n.text, "OK");
        let parent_path = parent_layout_path(&leaf_path).expect("parent path");
        let p = get_node_at_path(&snap.root, &parent_path).expect("parent node");
        assert!(p.children.iter().any(|c| c.text == "OK"));
    }

    #[test]
    fn find_ui_parent_from_snapshot_returns_parent_match() {
        let snap = sample_snapshot();
        let q = FindUiElementsParams {
            text_equals: Some("OK".to_string()),
            ..Default::default()
        };
        let path = find_ui_elements(&snap, &q, 1)
            .into_iter()
            .next()
            .map(|m| m.tree_path)
            .expect("OK path");
        let (norm, parent) = find_ui_parent_from_snapshot(&snap, &path).unwrap();
        assert_eq!(norm, path);
        assert_eq!(parent.tree_path, parent_layout_path(&path).expect("pp"));
        assert!(!parent.class.is_empty());
    }

    #[test]
    fn find_ui_parent_errors_on_missing_path() {
        let snap = sample_snapshot();
        assert!(find_ui_parent_from_snapshot(&snap, "99999").is_err());
    }
}
