use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// A known Android project entry in the project registry.
/// Persisted inside `AppSettings.recent_projects`.
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, TS)]
#[serde(rename_all = "camelCase", default)]
#[ts(export, export_to = "../../src/bindings/")]
pub struct ProjectEntry {
    /// Deterministic hex ID derived from the canonical project path.
    pub id: String,
    /// Absolute path to the project root folder.
    pub path: String,
    /// Display name — defaults to the folder base name but can be renamed.
    pub name: String,
    /// Detected Gradle root (ancestor containing `settings.gradle(.kts)`),
    /// or `None` if not yet detected.
    pub gradle_root: Option<String>,
    /// ISO-8601 timestamp of the last time this project was opened.
    pub last_opened: String,
    /// Whether the user has pinned this project (pinned entries sort first).
    pub pinned: bool,
    /// Last-used build variant for this project (e.g. `"debug"`).
    #[serde(default)]
    pub last_build_variant: Option<String>,
    /// Last-used ADB device serial for this project.
    #[serde(default)]
    pub last_device: Option<String>,
}

impl Default for ProjectEntry {
    fn default() -> Self {
        Self {
            id: String::new(),
            path: String::new(),
            name: String::new(),
            gradle_root: None,
            last_opened: String::new(),
            pinned: false,
            last_build_variant: None,
            last_device: None,
        }
    }
}

/// All IDE settings persisted to `~/.androidide/settings.json`.
/// Every field uses `#[serde(default)]` so the file is forward-compatible —
/// adding new settings never breaks existing config files.
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, TS)]
#[serde(rename_all = "camelCase", default)]
#[ts(export, export_to = "../../src/bindings/")]
pub struct AppSettings {
    pub editor: EditorSettings,
    pub appearance: AppearanceSettings,
    pub search: SearchSettings,
    pub files: FilesSettings,
    pub android: AndroidSettings,
    pub lsp: LspSettings,
    pub java: JavaSettings,
    pub advanced: AdvancedSettings,
    pub build: BuildSettings,
    pub logcat: LogcatSettings,
    pub mcp: McpSettings,
    /// Registry of recently-opened projects.  Capped at 20 entries; oldest
    /// non-pinned entry is evicted when the cap is exceeded.
    pub recent_projects: Vec<ProjectEntry>,
    /// The path of the project that was active when the app was last closed.
    pub last_active_project: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, TS)]
#[serde(rename_all = "camelCase", default)]
#[ts(export, export_to = "../../src/bindings/")]
pub struct EditorSettings {
    pub font_family: String,
    pub font_size: u32,
    pub tab_size: u32,
    pub insert_spaces: bool,
    pub word_wrap: bool,
    pub line_numbers: bool,
    pub bracket_matching: bool,
    pub highlight_active_line: bool,
    pub auto_close_brackets: bool,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, TS)]
#[serde(rename_all = "camelCase", default)]
#[ts(export, export_to = "../../src/bindings/")]
pub struct AppearanceSettings {
    pub ui_font_size: u32,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, TS)]
#[serde(rename_all = "camelCase", default)]
#[ts(export, export_to = "../../src/bindings/")]
pub struct SearchSettings {
    pub context_lines: u32,
    pub max_results: u32,
    pub max_files: u32,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, TS)]
#[serde(rename_all = "camelCase", default)]
#[ts(export, export_to = "../../src/bindings/")]
pub struct FilesSettings {
    pub excluded_dirs: Vec<String>,
    pub excluded_extensions: Vec<String>,
    pub max_file_size_mb: u32,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, TS)]
#[serde(rename_all = "camelCase", default)]
#[ts(export, export_to = "../../src/bindings/")]
pub struct AndroidSettings {
    pub sdk_path: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, TS)]
#[serde(rename_all = "camelCase", default)]
#[ts(export, export_to = "../../src/bindings/")]
pub struct LspSettings {
    pub log_level: String,
    pub request_timeout_sec: u32,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, TS)]
#[serde(rename_all = "camelCase", default)]
#[ts(export, export_to = "../../src/bindings/")]
pub struct JavaSettings {
    pub home: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, TS)]
#[serde(rename_all = "camelCase", default)]
#[ts(export, export_to = "../../src/bindings/")]
pub struct AdvancedSettings {
    pub tree_sitter_cache_size: u32,
    pub lsp_max_message_size_mb: u32,
    pub watcher_debounce_ms: u32,
    pub lsp_did_change_debounce_ms: u32,
    pub diagnostics_pull_delay_ms: u32,
    pub hover_delay_ms: u32,
    pub navigation_history_depth: u32,
    pub recent_files_limit: u32,
}

/// Build system settings: Gradle flags, auto-deploy behaviour, last-used selections.
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, TS)]
#[serde(rename_all = "camelCase", default)]
#[ts(export, export_to = "../../src/bindings/")]
pub struct BuildSettings {
    /// Extra JVM arguments passed to the Gradle daemon (e.g. `"-Xmx4g"`).
    pub gradle_jvm_args: Option<String>,
    /// Pass `--parallel` to Gradle for faster multi-module builds.
    pub gradle_parallel: bool,
    /// Pass `--offline` to Gradle to skip dependency downloads.
    pub gradle_offline: bool,
    /// Automatically install and launch the app after a successful build.
    pub auto_install_on_build: bool,
    /// Last-used build variant name, persisted across sessions.
    pub build_variant: Option<String>,
    /// Last-used ADB device serial, persisted across sessions.
    pub selected_device: Option<String>,
}

/// Logcat settings.
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, TS)]
#[serde(rename_all = "camelCase", default)]
#[ts(export, export_to = "../../src/bindings/")]
pub struct LogcatSettings {
    /// Automatically start logcat streaming when a device connects.
    pub auto_start: bool,
}

/// Settings for the MCP (Model Context Protocol) server integration.
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, TS)]
#[serde(rename_all = "camelCase", default)]
#[ts(export, export_to = "../../src/bindings/")]
pub struct McpSettings {
    /// Automatically start the MCP stdio server when the app launches.
    ///
    /// When enabled, Claude Code can connect immediately after the app opens
    /// without needing to trigger "Start MCP Server" from the command palette.
    pub auto_start: bool,
    /// Maximum seconds to wait for a Gradle build via the `run_gradle_task`
    /// MCP tool before cancelling. Increase for very large projects.
    pub build_timeout_sec: u32,
    /// Default number of logcat entries returned by `get_logcat_entries`
    /// when the caller does not specify a `count` argument.
    pub logcat_default_count: u32,
    /// Default number of raw build log lines returned by `get_build_log`
    /// when the caller does not specify a `lines` argument.
    pub build_log_default_lines: u32,
}

// ── Defaults ──────────────────────────────────────────────────────────────────

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            editor: EditorSettings::default(),
            appearance: AppearanceSettings::default(),
            search: SearchSettings::default(),
            files: FilesSettings::default(),
            android: AndroidSettings::default(),
            lsp: LspSettings::default(),
            java: JavaSettings::default(),
            advanced: AdvancedSettings::default(),
            build: BuildSettings::default(),
            logcat: LogcatSettings::default(),
            mcp: McpSettings::default(),
            recent_projects: Vec::new(),
            last_active_project: None,
        }
    }
}

/// Maximum number of entries kept in `AppSettings.recent_projects`.
/// The oldest non-pinned entry is evicted when this limit is exceeded.
pub const MAX_RECENT_PROJECTS: usize = 20;

impl Default for EditorSettings {
    fn default() -> Self {
        Self {
            font_family: r#""SF Mono", "Fira Code", "JetBrains Mono", "Menlo", monospace"#.into(),
            font_size: 13,
            tab_size: 4,
            insert_spaces: true,
            word_wrap: false,
            line_numbers: true,
            bracket_matching: true,
            highlight_active_line: true,
            auto_close_brackets: true,
        }
    }
}

impl Default for AppearanceSettings {
    fn default() -> Self {
        Self { ui_font_size: 12 }
    }
}

impl Default for SearchSettings {
    fn default() -> Self {
        Self {
            context_lines: 2,
            max_results: 10_000,
            max_files: 500,
        }
    }
}

impl Default for FilesSettings {
    fn default() -> Self {
        Self {
            excluded_dirs: vec![
                "build".into(),
                ".gradle".into(),
                ".idea".into(),
                ".git".into(),
                "node_modules".into(),
            ],
            excluded_extensions: vec![
                "class".into(),
                "dex".into(),
                "apk".into(),
                "aar".into(),
            ],
            max_file_size_mb: 10,
        }
    }
}

impl Default for AndroidSettings {
    fn default() -> Self {
        Self { sdk_path: None }
    }
}

impl Default for LspSettings {
    fn default() -> Self {
        Self {
            log_level: "INFO".into(),
            request_timeout_sec: 30,
        }
    }
}

impl Default for JavaSettings {
    fn default() -> Self {
        Self { home: None }
    }
}

impl Default for AdvancedSettings {
    fn default() -> Self {
        Self {
            tree_sitter_cache_size: 50,
            lsp_max_message_size_mb: 64,
            watcher_debounce_ms: 200,
            lsp_did_change_debounce_ms: 300,
            diagnostics_pull_delay_ms: 1000,
            hover_delay_ms: 500,
            navigation_history_depth: 50,
            recent_files_limit: 20,
        }
    }
}

impl Default for BuildSettings {
    fn default() -> Self {
        Self {
            gradle_jvm_args: None,
            gradle_parallel: true,
            gradle_offline: false,
            auto_install_on_build: true,
            build_variant: None,
            selected_device: None,
        }
    }
}

impl Default for LogcatSettings {
    fn default() -> Self {
        Self { auto_start: true }
    }
}

impl Default for McpSettings {
    fn default() -> Self {
        Self {
            auto_start: false,
            build_timeout_sec: 600,
            logcat_default_count: 200,
            build_log_default_lines: 200,
        }
    }
}

/// App version information read from (and written back to) `app/build.gradle(.kts)`.
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/bindings/")]
pub struct ProjectAppInfo {
    /// The `applicationId` declared in the app module (read-only — editing it
    /// would require a full Gradle sync and is out of scope).
    pub application_id: Option<String>,
    /// Human-readable version string (e.g. `"1.2.3"`).
    pub version_name: Option<String>,
    /// Integer version code used by the Play Store.
    pub version_code: Option<i64>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_settings_serialize_and_deserialize() {
        let settings = AppSettings::default();
        let json = serde_json::to_string_pretty(&settings).unwrap();
        let parsed: AppSettings = serde_json::from_str(&json).unwrap();
        assert_eq!(settings, parsed);
    }

    #[test]
    fn empty_json_produces_defaults() {
        let parsed: AppSettings = serde_json::from_str("{}").unwrap();
        assert_eq!(parsed, AppSettings::default());
    }

    #[test]
    fn partial_json_merges_with_defaults() {
        let json = r#"{"editor": {"fontSize": 16}}"#;
        let parsed: AppSettings = serde_json::from_str(json).unwrap();
        assert_eq!(parsed.editor.font_size, 16);
        assert_eq!(parsed.editor.tab_size, 4); // default preserved
        assert_eq!(parsed.search.context_lines, 2); // other section defaults
    }

    #[test]
    fn editor_defaults_are_correct() {
        let d = EditorSettings::default();
        assert_eq!(d.font_size, 13);
        assert_eq!(d.tab_size, 4);
        assert!(d.insert_spaces);
        assert!(!d.word_wrap);
        assert!(d.line_numbers);
        assert!(d.bracket_matching);
        assert!(d.auto_close_brackets);
    }

    #[test]
    fn files_excludes_defaults() {
        let d = FilesSettings::default();
        assert!(d.excluded_dirs.contains(&"build".to_string()));
        assert!(d.excluded_dirs.contains(&".git".to_string()));
        assert!(d.excluded_extensions.contains(&"class".to_string()));
        assert_eq!(d.max_file_size_mb, 10);
    }

    #[test]
    fn advanced_defaults() {
        let d = AdvancedSettings::default();
        assert_eq!(d.tree_sitter_cache_size, 50);
        assert_eq!(d.lsp_max_message_size_mb, 64);
        assert_eq!(d.hover_delay_ms, 500);
        assert_eq!(d.recent_files_limit, 20);
    }
}
