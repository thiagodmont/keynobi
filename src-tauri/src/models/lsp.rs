use serde::{Deserialize, Serialize};
use ts_rs::TS;

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/bindings/")]
pub enum LspStatusState {
    NotInstalled,
    Downloading,
    Starting,
    Indexing,
    Ready,
    Error,
    Stopped,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/bindings/")]
pub struct LspStatus {
    pub state: LspStatusState,
    pub message: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/bindings/")]
pub struct TextRange {
    pub start_line: u32,
    pub start_col: u32,
    pub end_line: u32,
    pub end_col: u32,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/bindings/")]
pub enum DiagnosticSeverity {
    Error,
    Warning,
    Information,
    Hint,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/bindings/")]
pub struct Diagnostic {
    pub path: String,
    pub range: TextRange,
    pub severity: DiagnosticSeverity,
    pub message: String,
    pub source: Option<String>,
    pub code: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/bindings/")]
pub enum CompletionItemKind {
    Text,
    Method,
    Function,
    Constructor,
    Field,
    Variable,
    Class,
    Interface,
    Module,
    Property,
    Unit,
    Value,
    Enum,
    Keyword,
    Snippet,
    Color,
    File,
    Reference,
    Folder,
    EnumMember,
    Constant,
    Struct,
    Event,
    Operator,
    TypeParameter,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/bindings/")]
pub struct CompletionItem {
    pub label: String,
    pub kind: CompletionItemKind,
    pub detail: Option<String>,
    pub insert_text: Option<String>,
    pub sort_text: Option<String>,
    pub filter_text: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/bindings/")]
pub struct HoverResult {
    pub contents: String,
    pub range: Option<TextRange>,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/bindings/")]
pub struct Location {
    pub path: String,
    pub line: u32,
    pub col: u32,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/bindings/")]
pub enum SymbolKind {
    File,
    Module,
    Namespace,
    Package,
    Class,
    Method,
    Property,
    Field,
    Constructor,
    Enum,
    Interface,
    Function,
    Variable,
    Constant,
    String,
    Number,
    Boolean,
    Array,
    Object,
    Key,
    EnumMember,
    Struct,
    Event,
    Operator,
    TypeParameter,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/bindings/")]
pub struct SymbolInfo {
    pub name: String,
    pub kind: SymbolKind,
    pub range: TextRange,
    pub selection_range: TextRange,
    pub children: Option<Vec<SymbolInfo>>,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/bindings/")]
pub struct TextEdit {
    pub range: TextRange,
    pub new_text: String,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/bindings/")]
pub struct WorkspaceEdit {
    pub edits: Vec<FileEdit>,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/bindings/")]
pub struct FileEdit {
    pub path: String,
    pub edits: Vec<TextEdit>,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/bindings/")]
pub struct CodeAction {
    pub title: String,
    pub kind: Option<String>,
    pub is_preferred: bool,
    pub edit: Option<WorkspaceEdit>,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/bindings/")]
pub struct HighlightRange {
    pub range: TextRange,
    pub kind: HighlightKind,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/bindings/")]
pub enum HighlightKind {
    Text,
    Read,
    Write,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/bindings/")]
pub struct SignatureHelp {
    pub signatures: Vec<SignatureInfo>,
    pub active_signature: Option<u32>,
    pub active_parameter: Option<u32>,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/bindings/")]
pub struct SignatureInfo {
    pub label: String,
    pub documentation: Option<String>,
    pub parameters: Vec<ParameterInfo>,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/bindings/")]
pub struct ParameterInfo {
    pub label: String,
    pub documentation: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/bindings/")]
pub struct DownloadProgress {
    pub downloaded_bytes: u64,
    pub total_bytes: Option<u64>,
    pub percent: Option<f64>,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/bindings/")]
pub struct LspInstallation {
    pub path: String,
    pub version: String,
    pub launch_script: String,
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn lsp_status_serializes_correctly() {
        let status = LspStatus {
            state: LspStatusState::Ready,
            message: None,
        };
        let json = serde_json::to_value(&status).unwrap();
        assert_eq!(json["state"], "ready");
        assert_eq!(json["message"], serde_json::Value::Null);
    }

    #[test]
    fn diagnostic_serializes_correctly() {
        let diag = Diagnostic {
            path: "/project/Main.kt".into(),
            range: TextRange {
                start_line: 10,
                start_col: 5,
                end_line: 10,
                end_col: 15,
            },
            severity: DiagnosticSeverity::Error,
            message: "Unresolved reference".into(),
            source: Some("kotlin".into()),
            code: Some("UNRESOLVED_REFERENCE".into()),
        };
        let json = serde_json::to_value(&diag).unwrap();
        assert_eq!(json["severity"], "error");
        assert_eq!(json["range"]["startLine"], 10);
    }

    #[test]
    fn completion_item_round_trip() {
        let item = CompletionItem {
            label: "toString".into(),
            kind: CompletionItemKind::Method,
            detail: Some("fun toString(): String".into()),
            insert_text: Some("toString()".into()),
            sort_text: None,
            filter_text: None,
        };
        let json_str = serde_json::to_string(&item).unwrap();
        let deserialized: CompletionItem = serde_json::from_str(&json_str).unwrap();
        assert_eq!(item, deserialized);
    }

    #[test]
    fn symbol_info_with_children() {
        let sym = SymbolInfo {
            name: "MyClass".into(),
            kind: SymbolKind::Class,
            range: TextRange {
                start_line: 1,
                start_col: 0,
                end_line: 20,
                end_col: 1,
            },
            selection_range: TextRange {
                start_line: 1,
                start_col: 6,
                end_line: 1,
                end_col: 13,
            },
            children: Some(vec![SymbolInfo {
                name: "doStuff".into(),
                kind: SymbolKind::Method,
                range: TextRange {
                    start_line: 3,
                    start_col: 4,
                    end_line: 5,
                    end_col: 5,
                },
                selection_range: TextRange {
                    start_line: 3,
                    start_col: 8,
                    end_line: 3,
                    end_col: 15,
                },
                children: None,
            }]),
        };
        let json = serde_json::to_value(&sym).unwrap();
        assert_eq!(json["name"], "MyClass");
        assert!(json["children"].is_array());
        assert_eq!(json["children"][0]["name"], "doStuff");
    }

    #[test]
    fn download_progress_serializes() {
        let p = DownloadProgress {
            downloaded_bytes: 1024,
            total_bytes: Some(2048),
            percent: Some(50.0),
        };
        let json = serde_json::to_value(&p).unwrap();
        assert_eq!(json["downloadedBytes"], 1024);
        assert_eq!(json["percent"], 50.0);
    }

    #[test]
    fn hover_result_serializes() {
        let hover = HoverResult {
            contents: "fun greet(): String".into(),
            range: Some(TextRange {
                start_line: 5,
                start_col: 4,
                end_line: 5,
                end_col: 9,
            }),
        };
        let json = serde_json::to_value(&hover).unwrap();
        assert_eq!(json["contents"], "fun greet(): String");
        assert_eq!(json["range"]["startLine"], 5);
    }

    #[test]
    fn workspace_edit_serializes() {
        let edit = WorkspaceEdit {
            edits: vec![FileEdit {
                path: "/project/Main.kt".into(),
                edits: vec![TextEdit {
                    range: TextRange {
                        start_line: 1,
                        start_col: 0,
                        end_line: 1,
                        end_col: 5,
                    },
                    new_text: "world".into(),
                }],
            }],
        };
        let json = serde_json::to_value(&edit).unwrap();
        assert_eq!(json["edits"][0]["path"], "/project/Main.kt");
        assert_eq!(json["edits"][0]["edits"][0]["newText"], "world");
    }

    #[test]
    fn code_action_serializes() {
        let action = CodeAction {
            title: "Add import".into(),
            kind: Some("quickfix".into()),
            is_preferred: true,
            edit: None,
        };
        let json = serde_json::to_value(&action).unwrap();
        assert_eq!(json["title"], "Add import");
        assert_eq!(json["isPreferred"], true);
    }

    #[test]
    fn signature_help_serializes() {
        let sig = SignatureHelp {
            signatures: vec![SignatureInfo {
                label: "fun greet(name: String)".into(),
                documentation: Some("Greets someone".into()),
                parameters: vec![ParameterInfo {
                    label: "name".into(),
                    documentation: Some("The name".into()),
                }],
            }],
            active_signature: Some(0),
            active_parameter: Some(0),
        };
        let json = serde_json::to_value(&sig).unwrap();
        assert_eq!(json["signatures"][0]["label"], "fun greet(name: String)");
        assert_eq!(json["activeSignature"], 0);
    }

    #[test]
    fn all_status_states_serialize() {
        let variants = vec![
            (LspStatusState::NotInstalled, "notInstalled"),
            (LspStatusState::Downloading, "downloading"),
            (LspStatusState::Starting, "starting"),
            (LspStatusState::Indexing, "indexing"),
            (LspStatusState::Ready, "ready"),
            (LspStatusState::Error, "error"),
            (LspStatusState::Stopped, "stopped"),
        ];
        for (variant, expected) in variants {
            assert_eq!(
                serde_json::to_value(&variant).unwrap(),
                json!(expected),
                "Failed for {:?}",
                variant
            );
        }
    }
}
