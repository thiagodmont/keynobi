use serde::{Deserialize, Serialize};
use ts_rs::TS;

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
        }
    }
}

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
