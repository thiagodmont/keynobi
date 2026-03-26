use serde::{Deserialize, Serialize};
use ts_rs::TS;

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/bindings/")]
pub struct SearchOptions {
    pub regex: bool,
    pub case_sensitive: bool,
    pub whole_word: bool,
    pub include_pattern: Option<String>,
    pub exclude_pattern: Option<String>,
}

impl Default for SearchOptions {
    fn default() -> Self {
        Self {
            regex: false,
            case_sensitive: false,
            whole_word: false,
            include_pattern: None,
            exclude_pattern: None,
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/bindings/")]
pub struct SearchMatch {
    pub line: u32,
    pub col: u32,
    pub end_col: u32,
    pub line_content: String,
    pub context_before: Vec<String>,
    pub context_after: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/bindings/")]
pub struct SearchResult {
    pub path: String,
    pub matches: Vec<SearchMatch>,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/bindings/")]
pub struct SearchProgress {
    pub files_searched: u32,
    pub total_matches: u32,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/bindings/")]
pub struct ReplacePreview {
    pub path: String,
    pub line: u32,
    pub original: String,
    pub replaced: String,
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::Value;

    #[test]
    fn search_options_default() {
        let opts = SearchOptions::default();
        assert!(!opts.regex);
        assert!(!opts.case_sensitive);
        assert!(!opts.whole_word);
        assert!(opts.include_pattern.is_none());
    }

    #[test]
    fn search_result_serializes() {
        let result = SearchResult {
            path: "/project/Main.kt".into(),
            matches: vec![SearchMatch {
                line: 42,
                col: 10,
                end_col: 20,
                line_content: "    fun main() {".into(),
                context_before: vec!["".into()],
                context_after: vec!["        println(\"hello\")".into()],
            }],
        };
        let json: Value = serde_json::to_value(&result).unwrap();
        assert_eq!(json["path"], "/project/Main.kt");
        assert_eq!(json["matches"][0]["line"], 42);
        assert_eq!(json["matches"][0]["lineContent"], "    fun main() {");
    }

    #[test]
    fn search_options_round_trip() {
        let opts = SearchOptions {
            regex: true,
            case_sensitive: true,
            whole_word: false,
            include_pattern: Some("*.kt".into()),
            exclude_pattern: Some("*Test*".into()),
        };
        let json_str = serde_json::to_string(&opts).unwrap();
        let deserialized: SearchOptions = serde_json::from_str(&json_str).unwrap();
        assert_eq!(opts, deserialized);
    }

    #[test]
    fn replace_preview_serializes() {
        let preview = ReplacePreview {
            path: "/project/Foo.kt".into(),
            line: 5,
            original: "val x = 1".into(),
            replaced: "val y = 1".into(),
        };
        let json: Value = serde_json::to_value(&preview).unwrap();
        assert_eq!(json["original"], "val x = 1");
        assert_eq!(json["replaced"], "val y = 1");
    }
}
