use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// One node in the Android accessibility / UI Automator XML tree.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/bindings/")]
pub struct UiNode {
    pub class: String,
    pub resource_id: String,
    pub text: String,
    pub content_desc: String,
    pub package: String,
    pub bounds: String,
    pub clickable: bool,
    pub enabled: bool,
    pub focusable: bool,
    pub focused: bool,
    pub scrollable: bool,
    pub long_clickable: bool,
    pub password: bool,
    pub checkable: bool,
    pub checked: bool,
    pub editable: bool,
    /// UI Automator `selected` (e.g. bottom navigation tab).
    pub selected: bool,
    /// True when `class` looks like a Compose container (heuristic).
    pub is_compose_heuristic: bool,
    pub children: Vec<UiNode>,
}

/// Capped excerpts from official `adb shell` probes (window / display / wm) to interpret bounds.
#[derive(Debug, Clone, Default, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/bindings/")]
pub struct UiLayoutContext {
    /// First bytes of `dumpsys window windows` (focus / window tokens).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub window_excerpt: Option<String>,
    /// First bytes of `dumpsys display`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub display_excerpt: Option<String>,
    /// Output of `wm size`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub wm_size: Option<String>,
    /// Output of `wm density`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub wm_density: Option<String>,
}

/// Result of dumping and parsing the current screen hierarchy from a device.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/bindings/")]
pub struct UiHierarchySnapshot {
    /// RFC3339 timestamp when the dump was taken (host clock).
    pub captured_at: String,
    pub truncated: bool,
    pub warnings: Vec<String>,
    pub root: UiNode,
    /// SHA-256 hex of interactive-relevant fields (for "did the screen change?").
    pub screen_hash: String,
    pub interactive_count: u32,
    /// Best-effort resumed activity line from `dumpsys activity`, if detected.
    pub foreground_activity: Option<String>,
    /// Capped `dumpsys window` / `dumpsys display` / `wm size` / `wm density` excerpts.
    pub layout_context: UiLayoutContext,
    /// Exact `adb …` lines executed during this capture (for reproducing in a terminal).
    pub command_log: Vec<String>,
    /// Base64-encoded PNG screenshot captured alongside the hierarchy dump (optional).
    /// Captured via `adb exec-out screencap -p`. Omitted when capture fails or is skipped.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub screenshot_b64: Option<String>,
}

/// Compact row for MCP when `interactive_only` is requested.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct UiInteractiveRow {
    pub class: String,
    pub resource_id: String,
    pub text: String,
    pub content_desc: String,
    pub bounds: String,
    pub center_x: i32,
    pub center_y: i32,
    pub clickable: bool,
    pub editable: bool,
    pub scrollable: bool,
    pub enabled: bool,
    pub parent_label: String,
    pub depth: u32,
}
