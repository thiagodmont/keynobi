import { type JSX, createSignal, createEffect, onCleanup, For, Show } from "solid-js";
import { editorState } from "@/stores/editor.store";
import { projectState } from "@/stores/project.store";
import { getDocumentSymbols, type SymbolInfo } from "@/lib/tauri-api";

function findEnclosingSymbol(
  symbols: SymbolInfo[],
  line: number
): SymbolInfo[] {
  for (const sym of symbols) {
    const inRange =
      line >= sym.range.startLine && line <= sym.range.endLine;
    if (inRange) {
      const children = sym.children ? findEnclosingSymbol(sym.children, line) : [];
      return [sym, ...children];
    }
  }
  return [];
}

export function Breadcrumbs(): JSX.Element {
  const [symbolChain, setSymbolChain] = createSignal<SymbolInfo[]>([]);
  let refreshTimer: ReturnType<typeof setTimeout> | undefined;

  async function refresh(path: string, line: number) {
    try {
      const symbols = await getDocumentSymbols(path);
      const chain = findEnclosingSymbol(symbols, line - 1);
      setSymbolChain(chain);
    } catch {
      setSymbolChain([]);
    }
  }

  createEffect(() => {
    const path = editorState.activeFilePath;
    const line = editorState.cursorLine ?? 1;

    if (!path) {
      setSymbolChain([]);
      return;
    }

    if (refreshTimer) clearTimeout(refreshTimer);
    refreshTimer = setTimeout(() => refresh(path, line), 300);
  });

  onCleanup(() => {
    if (refreshTimer) clearTimeout(refreshTimer);
  });

  const filePath = () => {
    const path = editorState.activeFilePath ?? "";
    const root = projectState.projectRoot ?? "";
    const rel = root && path.startsWith(root) ? path.slice(root.length + 1) : path;
    return rel.split("/");
  };

  async function jumpToSymbol(sym: SymbolInfo) {
    const path = editorState.activeFilePath;
    if (!path) return;
    const { openFileAtLocation } = await import("@/services/project.service");
    openFileAtLocation(path, sym.selectionRange.startLine + 1, sym.selectionRange.startCol);
  }

  const symbolKindIcon = (kind: string): string => {
    switch (kind.toLowerCase()) {
      case "class": return "C";
      case "interface": return "I";
      case "function":
      case "method": return "f";
      case "property":
      case "field": return "p";
      case "object": return "O";
      default: return "·";
    }
  };

  return (
    <Show when={editorState.activeFilePath}>
      <div
        style={{
          display: "flex",
          "align-items": "center",
          height: "22px",
          padding: "0 8px",
          background: "var(--bg-secondary)",
          "border-bottom": "1px solid var(--border)",
          "font-size": "11px",
          color: "var(--text-muted)",
          "overflow-x": "auto",
          "overflow-y": "hidden",
          "white-space": "nowrap",
          "scrollbar-width": "none",
          "flex-shrink": "0",
          gap: "0",
        }}
      >
        {/* File path segments */}
        <For each={filePath()}>
          {(segment, idx) => (
            <>
              <Show when={idx() > 0}>
                <span style={{ margin: "0 2px", opacity: "0.4" }}>›</span>
              </Show>
              <span
                style={{
                  color: idx() === filePath().length - 1 ? "var(--text-secondary)" : "var(--text-muted)",
                  "font-weight": idx() === filePath().length - 1 ? "500" : "400",
                }}
              >
                {segment}
              </span>
            </>
          )}
        </For>

        {/* Symbol chain */}
        <For each={symbolChain()}>
          {(sym) => (
            <>
              <span style={{ margin: "0 3px", opacity: "0.4" }}>›</span>
              <button
                onClick={() => jumpToSymbol(sym)}
                title={`Go to ${sym.name}`}
                style={{
                  background: "none",
                  border: "none",
                  color: "var(--text-secondary)",
                  cursor: "pointer",
                  padding: "0 2px",
                  "font-size": "11px",
                  display: "inline-flex",
                  "align-items": "center",
                  gap: "3px",
                }}
                onMouseEnter={(e) => { (e.currentTarget as HTMLElement).style.color = "var(--accent)"; }}
                onMouseLeave={(e) => { (e.currentTarget as HTMLElement).style.color = "var(--text-secondary)"; }}
              >
                <span
                  style={{
                    "font-size": "9px",
                    "font-weight": "700",
                    color: "var(--text-muted)",
                    "font-family": "var(--font-mono)",
                  }}
                >
                  {symbolKindIcon(sym.kind)}
                </span>
                {sym.name}
              </button>
            </>
          )}
        </For>
      </div>
    </Show>
  );
}

export default Breadcrumbs;
