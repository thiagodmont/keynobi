import {
  type JSX,
  Show,
  For,
  createSignal,
  createEffect,
  createMemo,
} from "solid-js";
import { fuzzyFilter } from "@/lib/fuzzy-match";
import { searchActions } from "@/lib/action-registry";
import { editorState } from "@/stores/editor.store";
import { projectState } from "@/stores/project.store";
import { getDocumentSymbols, type SymbolInfo } from "@/lib/tauri-api";
import { lspState } from "@/stores/lsp.store";
import { invoke } from "@tauri-apps/api/core";
import Icon from "@/components/common/Icon";

export type PaletteMode = "files" | "commands" | "symbols" | "documentSymbols";

interface PaletteState {
  open: boolean;
  mode: PaletteMode;
}

const [paletteState, setPaletteState] = createSignal<PaletteState>({
  open: false,
  mode: "files",
});

export function openPalette(mode: PaletteMode = "files") {
  setPaletteState({ open: true, mode });
}

export function closePalette() {
  setPaletteState({ open: false, mode: "files" });
}

export function isPaletteOpen(): boolean {
  return paletteState().open;
}

export function CommandPalette(): JSX.Element {
  let inputRef!: HTMLInputElement;
  const [query, setQuery] = createSignal("");
  const [selectedIndex, setSelectedIndex] = createSignal(0);
  const [docSymbols, setDocSymbols] = createSignal<SymbolInfo[]>([]);

  const [wsSymbols, setWsSymbols] = createSignal<Array<{ name: string; kind: string; path: string; line: number }>>([]);

  createEffect(() => {
    if (paletteState().open) {
      setQuery("");
      setSelectedIndex(0);
      if (paletteState().mode === "documentSymbols") {
        loadDocumentSymbols();
      }
      if (paletteState().mode === "symbols") {
        setWsSymbols([]);
      }
      setTimeout(() => inputRef?.focus(), 10);
    }
  });

  async function loadDocumentSymbols() {
    const path = editorState.activeFilePath;
    if (!path) return;
    try {
      const symbols = await getDocumentSymbols(path);
      setDocSymbols(symbols);
    } catch {
      setDocSymbols([]);
    }
  }

  async function loadWorkspaceSymbols(q: string) {
    if (lspState.status.state !== "ready" || !q.trim()) {
      setWsSymbols([]);
      return;
    }
    try {
      const raw = await invoke<unknown[]>("lsp_workspace_symbols", { query: q });
      const items = Array.isArray(raw) ? raw : [];
      setWsSymbols(
        items.slice(0, 50).map((s: any) => ({
          name: s.name ?? "",
          kind: mapLspSymbolKind(s.kind),
          path: s.location?.uri?.replace("file://", "") ?? "",
          line: (s.location?.range?.start?.line ?? 0) + 1,
        }))
      );
    } catch {
      setWsSymbols([]);
    }
  }

  function flattenSymbols(symbols: SymbolInfo[], prefix = ""): Array<{ name: string; kind: string; line: number; fullName: string }> {
    const result: Array<{ name: string; kind: string; line: number; fullName: string }> = [];
    for (const s of symbols) {
      const fullName = prefix ? `${prefix}.${s.name}` : s.name;
      result.push({
        name: s.name,
        kind: s.kind,
        line: s.range.startLine + 1,
        fullName,
      });
      if (s.children) {
        result.push(...flattenSymbols(s.children, fullName));
      }
    }
    return result;
  }

  const allProjectFiles = createMemo(() => collectProjectFiles());

  const results = createMemo(() => {
    const q = query();
    const mode = paletteState().mode;

    if (mode === "files") {
      return getFileResults(q);
    }
    if (mode === "commands") {
      const commandQuery = q.startsWith(">") ? q.slice(1).trim() : q;
      return getCommandResults(commandQuery);
    }
    if (mode === "documentSymbols") {
      return getSymbolResults(q, docSymbols());
    }
    if (mode === "symbols") {
      return getWorkspaceSymbolResults();
    }
    return [];
  });

  function getFileResults(q: string): PaletteItem[] {
    const allFiles = allProjectFiles();
    const recent = editorState.recentFiles;

    if (!q.trim()) {
      const recentItems: PaletteItem[] = recent.slice(0, 10).map((path) => ({
        id: path,
        label: path.split("/").pop() ?? path,
        detail: relativePath(path),
        icon: "file",
        action: () => openFile(path),
      }));
      return recentItems;
    }

    const filtered = fuzzyFilter(allFiles, q, (f) => f.split("/").pop() ?? f);
    return filtered.slice(0, 50).map((r) => ({
      id: r.item,
      label: r.item.split("/").pop() ?? r.item,
      detail: relativePath(r.item),
      icon: "file",
      matchedIndices: r.matchedIndices,
      action: () => openFile(r.item),
    }));
  }

  function getCommandResults(q: string): PaletteItem[] {
    const actions = searchActions(q);
    return actions.map((a) => ({
      id: a.id,
      label: a.label,
      detail: a.shortcut ?? "",
      icon: a.icon,
      action: a.action,
    }));
  }

  function getSymbolResults(q: string, symbols: SymbolInfo[]): PaletteItem[] {
    const flat = flattenSymbols(symbols);
    if (!q.trim()) {
      return flat.map((s) => ({
        id: `${s.fullName}:${s.line}`,
        label: s.name,
        detail: `${symbolKindIcon(s.kind)} Line ${s.line}`,
        action: () => goToLine(s.line),
      }));
    }
    const filtered = fuzzyFilter(flat, q, (s) => s.name);
    return filtered.slice(0, 50).map((r) => ({
      id: `${r.item.fullName}:${r.item.line}`,
      label: r.item.name,
      detail: `${symbolKindIcon(r.item.kind)} Line ${r.item.line}`,
      matchedIndices: r.matchedIndices,
      action: () => goToLine(r.item.line),
    }));
  }

  function getWorkspaceSymbolResults(): PaletteItem[] {
    const items = wsSymbols();
    if (items.length === 0 && lspState.status.state !== "ready") {
      return [{
        id: "__lsp_not_ready",
        label: "Kotlin LSP is not ready",
        detail: lspState.status.state === "stopped" ? "Start a project first" : "Please wait...",
        action: () => {},
      }];
    }
    return items.map((s) => ({
      id: `${s.path}:${s.line}:${s.name}`,
      label: s.name,
      detail: `${symbolKindIcon(s.kind)} ${relativePath(s.path)}:${s.line}`,
      icon: "list",
      action: async () => {
        closePalette();
        const { openFileAtLocation } = await import("@/services/project.service");
        openFileAtLocation(s.path, s.line, 0);
      },
    }));
  }

  function collectProjectFiles(): string[] {
    const tree = projectState.fileTree;
    if (!tree) return [];
    const files: string[] = [];
    function walk(node: typeof tree) {
      if (!node) return;
      if (node.kind === "file") {
        files.push(node.path);
      }
      if (node.children) {
        for (const child of node.children) walk(child);
      }
    }
    walk(tree);
    return files;
  }

  function relativePath(fullPath: string): string {
    const root = projectState.projectRoot;
    if (root && fullPath.startsWith(root)) {
      return fullPath.slice(root.length + 1);
    }
    return fullPath;
  }

  async function openFile(path: string) {
    closePalette();
    const { openFileAtLocation } = await import("@/services/project.service");
    openFileAtLocation(path, 1, 0);
  }

  async function goToLine(line: number) {
    closePalette();
    const { getEditorView } = await import("@/components/editor/CodeEditor");
    const view = getEditorView();
    if (view) {
      const lineInfo = view.state.doc.line(Math.max(1, line));
      view.dispatch({
        selection: { anchor: lineInfo.from },
        scrollIntoView: true,
      });
      view.focus();
    }
  }

  function handleKeyDown(e: KeyboardEvent) {
    const items = results();
    if (e.key === "Escape") {
      e.preventDefault();
      closePalette();
      return;
    }
    if (e.key === "ArrowDown") {
      e.preventDefault();
      setSelectedIndex((i) => Math.min(i + 1, items.length - 1));
      return;
    }
    if (e.key === "ArrowUp") {
      e.preventDefault();
      setSelectedIndex((i) => Math.max(i - 1, 0));
      return;
    }
    if (e.key === "Enter") {
      e.preventDefault();
      const item = items[selectedIndex()];
      if (item) item.action();
      return;
    }
  }

  function placeholder(): string {
    const mode = paletteState().mode;
    if (mode === "files") return "Search files by name...";
    if (mode === "commands") return "Type a command...";
    if (mode === "documentSymbols") return "Go to symbol in file...";
    if (mode === "symbols") return "Search workspace symbols...";
    return "Search...";
  }

  let wsSearchTimer: ReturnType<typeof setTimeout> | undefined;

  function handleInput(value: string) {
    setQuery(value);
    setSelectedIndex(0);

    if (paletteState().mode === "files" && value.startsWith(">")) {
      setPaletteState({ open: true, mode: "commands" });
    }
    if (paletteState().mode === "commands" && !value.startsWith(">") && value === "") {
      setPaletteState({ open: true, mode: "files" });
    }
    if (paletteState().mode === "symbols") {
      if (wsSearchTimer) clearTimeout(wsSearchTimer);
      wsSearchTimer = setTimeout(() => loadWorkspaceSymbols(value), 200);
    }
  }

  return (
    <Show when={paletteState().open}>
      {/* Backdrop */}
      <div
        onClick={() => closePalette()}
        style={{
          position: "fixed",
          inset: "0",
          "z-index": "1000",
          background: "rgba(0,0,0,0.3)",
        }}
      />
      {/* Palette */}
      <div
        role="dialog"
        aria-modal="true"
        aria-label="Command Palette"
        style={{
          position: "fixed",
          top: "20%",
          left: "50%",
          transform: "translateX(-50%)",
          width: "560px",
          "max-height": "400px",
          background: "var(--bg-secondary)",
          border: "1px solid var(--border)",
          "border-radius": "8px",
          "box-shadow": "0 8px 32px rgba(0,0,0,0.5)",
          "z-index": "1001",
          display: "flex",
          "flex-direction": "column",
          overflow: "hidden",
        }}
      >
        {/* Input */}
        <div style={{ padding: "8px", "border-bottom": "1px solid var(--border)" }}>
          <input
            ref={inputRef}
            type="text"
            placeholder={placeholder()}
            aria-label={placeholder()}
            role="combobox"
            aria-expanded="true"
            aria-autocomplete="list"
            value={query()}
            onInput={(e) => handleInput(e.currentTarget.value)}
            onKeyDown={handleKeyDown}
            style={{
              width: "100%",
              background: "var(--bg-primary)",
              border: "1px solid var(--border)",
              color: "var(--text-primary)",
              padding: "6px 10px",
              "border-radius": "4px",
              outline: "none",
              "font-size": "13px",
              "font-family": "inherit",
            }}
          />
        </div>
        {/* Results */}
        <div role="listbox" style={{ flex: "1", overflow: "auto", "min-height": "0" }}>
          <For each={results()}>
            {(item, index) => (
              <div
                role="option"
                aria-selected={index() === selectedIndex()}
                onClick={() => item.action()}
                onMouseEnter={() => setSelectedIndex(index())}
                style={{
                  display: "flex",
                  "align-items": "center",
                  gap: "8px",
                  padding: "4px 12px",
                  cursor: "pointer",
                  background:
                    index() === selectedIndex()
                      ? "var(--accent)"
                      : "transparent",
                  color:
                    index() === selectedIndex()
                      ? "#fff"
                      : "var(--text-primary)",
                }}
              >
                <Show when={item.icon}>
                  <Icon name={item.icon!} size={14} />
                </Show>
                <span
                  style={{
                    flex: "1",
                    overflow: "hidden",
                    "text-overflow": "ellipsis",
                    "white-space": "nowrap",
                    "font-size": "13px",
                  }}
                >
                  {item.label}
                </span>
                <Show when={item.detail}>
                  <span
                    style={{
                      "font-size": "11px",
                      color:
                        index() === selectedIndex()
                          ? "rgba(255,255,255,0.7)"
                          : "var(--text-muted)",
                      "flex-shrink": "0",
                      "max-width": "250px",
                      overflow: "hidden",
                      "text-overflow": "ellipsis",
                      "white-space": "nowrap",
                    }}
                  >
                    {item.detail}
                  </span>
                </Show>
              </div>
            )}
          </For>
          <Show when={results().length === 0 && query().trim()}>
            <div style={{ padding: "16px", "text-align": "center", color: "var(--text-muted)", "font-size": "12px" }}>
              No matches found
            </div>
          </Show>
        </div>
      </div>
    </Show>
  );
}

interface PaletteItem {
  id: string;
  label: string;
  detail?: string;
  icon?: string;
  matchedIndices?: number[];
  action: () => void;
}

function symbolKindIcon(kind: string): string {
  switch (kind) {
    case "class":
      return "C";
    case "interface":
      return "I";
    case "function":
      return "f";
    case "method":
      return "m";
    case "property":
      return "p";
    case "field":
      return "F";
    case "variable":
      return "v";
    case "enum":
      return "E";
    case "constant":
      return "K";
    default:
      return "S";
  }
}

function mapLspSymbolKind(kind: number | undefined): string {
  const map: Record<number, string> = {
    1: "file", 2: "module", 3: "namespace", 4: "package", 5: "class",
    6: "method", 7: "property", 8: "field", 9: "constructor", 10: "enum",
    11: "interface", 12: "function", 13: "variable", 14: "constant",
  };
  return kind ? (map[kind] ?? "variable") : "variable";
}

export default CommandPalette;
