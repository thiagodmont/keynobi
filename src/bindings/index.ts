/**
 * Auto-generated TypeScript bindings from Rust model types.
 *
 * These files are produced by `ts-rs` (https://github.com/Aleph-Alpha/ts-rs)
 * when running `cargo test` in the `src-tauri` directory. Do NOT edit the
 * individual `.ts` files manually — they will be overwritten.
 *
 * To regenerate after changing a Rust model:
 *   cd src-tauri && cargo test
 *
 * All frontend code should import IPC types from this module:
 *   import type { FileNode, FileEvent } from "@/bindings";
 */

// File system
export type { FileNode } from "./FileNode";
export type { FileKind } from "./FileKind";
export type { FileEvent } from "./FileEvent";
export type { FileEventKind } from "./FileEventKind";

// LSP
export type { LspStatus } from "./LspStatus";
export type { LspStatusState } from "./LspStatusState";
export type { Diagnostic } from "./Diagnostic";
export type { DiagnosticSeverity } from "./DiagnosticSeverity";
export type { TextRange } from "./TextRange";
export type { CompletionItem } from "./CompletionItem";
export type { CompletionItemKind } from "./CompletionItemKind";
export type { HoverResult } from "./HoverResult";
export type { Location } from "./Location";
export type { SymbolInfo } from "./SymbolInfo";
export type { SymbolKind } from "./SymbolKind";
export type { TextEdit } from "./TextEdit";
export type { WorkspaceEdit } from "./WorkspaceEdit";
export type { FileEdit } from "./FileEdit";
export type { CodeAction } from "./CodeAction";
export type { HighlightRange } from "./HighlightRange";
export type { HighlightKind } from "./HighlightKind";
export type { SignatureHelp } from "./SignatureHelp";
export type { SignatureInfo } from "./SignatureInfo";
export type { ParameterInfo } from "./ParameterInfo";
export type { DownloadProgress } from "./DownloadProgress";
export type { LspInstallation } from "./LspInstallation";

// Search
export type { SearchOptions } from "./SearchOptions";
export type { SearchMatch } from "./SearchMatch";
export type { SearchResult } from "./SearchResult";
export type { SearchProgress } from "./SearchProgress";
export type { ReplacePreview } from "./ReplacePreview";

// Logs
export type { LogEntry } from "./LogEntry";
export type { LogLevel } from "./LogLevel";

// Health
export type { SystemHealthReport } from "./SystemHealthReport";

// Build system
export type { BuildLine } from "./BuildLine";
export type { BuildLineKind } from "./BuildLineKind";
export type { BuildError } from "./BuildError";
export type { BuildErrorSeverity } from "./BuildErrorSeverity";
export type { BuildResult } from "./BuildResult";
export type { BuildStatus } from "./BuildStatus";
export type { BuildRecord } from "./BuildRecord";
export type { BuildSettings } from "./BuildSettings";

// Devices & variants
export type { Device } from "./Device";
export type { DeviceKind } from "./DeviceKind";
export type { DeviceConnectionState } from "./DeviceConnectionState";
export type { AvdInfo } from "./AvdInfo";
export type { BuildVariant } from "./BuildVariant";
export type { VariantList } from "./VariantList";
