import { type JSX, For, Show, createSignal, createEffect, onCleanup } from "solid-js";
import { editorState } from "@/stores/editor.store";
import { getDocumentSymbols, type SymbolInfo } from "@/lib/tauri-api";
import Icon from "@/components/common/Icon";

export function SymbolsPanel(): JSX.Element {
  const [symbols, setSymbols] = createSignal<SymbolInfo[]>([]);
  const [loading, setLoading] = createSignal(false);
  let debounceTimer: ReturnType<typeof setTimeout> | undefined;

  createEffect(() => {
    const path = editorState.activeFilePath;
    if (debounceTimer) clearTimeout(debounceTimer);

    if (!path) {
      setSymbols([]);
      return;
    }

    debounceTimer = setTimeout(() => loadSymbols(path), 300);
  });

  onCleanup(() => {
    if (debounceTimer) clearTimeout(debounceTimer);
  });

  async function loadSymbols(path: string) {
    setLoading(true);
    try {
      const result = await getDocumentSymbols(path);
      setSymbols(result);
    } catch {
      setSymbols([]);
    } finally {
      setLoading(false);
    }
  }

  return (
    <div
      style={{
        display: "flex",
        "flex-direction": "column",
        height: "100%",
        "font-size": "var(--font-size-ui)",
      }}
    >
      <div
        style={{
          padding: "8px",
          "border-bottom": "1px solid var(--border)",
          "flex-shrink": "0",
          "font-size": "11px",
          "font-weight": "600",
          "text-transform": "uppercase",
          "letter-spacing": "0.5px",
          color: "var(--text-secondary)",
        }}
      >
        Outline
      </div>

      <div style={{ flex: "1", overflow: "auto", "min-height": "0" }}>
        <Show when={loading()}>
          <div style={{ padding: "12px", color: "var(--text-muted)", "font-size": "12px" }}>
            Loading symbols...
          </div>
        </Show>
        <Show when={!loading() && symbols().length === 0}>
          <div style={{ padding: "12px", color: "var(--text-muted)", "font-size": "12px", "text-align": "center" }}>
            <Show
              when={editorState.activeFilePath}
              fallback={<span>No file open</span>}
            >
              No symbols found
            </Show>
          </div>
        </Show>
        <Show when={!loading() && symbols().length > 0}>
          <For each={symbols()}>
            {(symbol) => <SymbolNode symbol={symbol} depth={0} />}
          </For>
        </Show>
      </div>
    </div>
  );
}

function SymbolNode(props: { symbol: SymbolInfo; depth: number }): JSX.Element {
  const [expanded, setExpanded] = createSignal(true);
  const hasChildren = () =>
    props.symbol.children !== null && props.symbol.children!.length > 0;

  async function handleClick() {
    const { openFileAtLocation } = await import("@/services/project.service");
    const path = editorState.activeFilePath;
    if (path) {
      openFileAtLocation(
        path,
        props.symbol.selectionRange.startLine + 1,
        props.symbol.selectionRange.startCol
      );
    }
  }

  return (
    <div>
      <div
        role="button"
        tabindex="0"
        onClick={handleClick}
        onKeyDown={(e) => {
          if (e.key === "Enter" || e.key === " ") {
            e.preventDefault();
            handleClick();
          }
        }}
        style={{
          display: "flex",
          "align-items": "center",
          gap: "4px",
          padding: `1px 8px 1px ${8 + props.depth * 12}px`,
          cursor: "pointer",
          "font-size": "12px",
          color: "var(--text-primary)",
          "white-space": "nowrap",
          overflow: "hidden",
        }}
        onMouseEnter={(e) => {
          e.currentTarget.style.background = "var(--bg-hover)";
        }}
        onMouseLeave={(e) => {
          e.currentTarget.style.background = "transparent";
        }}
      >
        <Show when={hasChildren()}>
          <span
            onClick={(e) => {
              e.stopPropagation();
              setExpanded((v) => !v);
            }}
            style={{ cursor: "pointer", "flex-shrink": "0" }}
          >
            <Icon
              name={expanded() ? "chevron-down" : "chevron-right"}
              size={10}
            />
          </span>
        </Show>
        <Show when={!hasChildren()}>
          <span style={{ width: "10px", "flex-shrink": "0" }} />
        </Show>
        <span
          style={{
            width: "14px",
            height: "14px",
            display: "flex",
            "align-items": "center",
            "justify-content": "center",
            "font-size": "9px",
            "font-weight": "700",
            "border-radius": "2px",
            "flex-shrink": "0",
            background: symbolKindColor(props.symbol.kind),
            color: "#fff",
          }}
        >
          {symbolKindLetter(props.symbol.kind)}
        </span>
        <span
          style={{
            overflow: "hidden",
            "text-overflow": "ellipsis",
          }}
        >
          {props.symbol.name}
        </span>
      </div>
      <Show when={hasChildren() && expanded()}>
        <For each={props.symbol.children!}>
          {(child) => <SymbolNode symbol={child} depth={props.depth + 1} />}
        </For>
      </Show>
    </div>
  );
}

function symbolKindLetter(kind: string): string {
  switch (kind) {
    case "class": return "C";
    case "interface": return "I";
    case "function": return "f";
    case "method": return "m";
    case "property": return "p";
    case "field": return "F";
    case "variable": return "v";
    case "enum": return "E";
    case "constant": return "K";
    case "constructor": return "c";
    case "struct": return "S";
    default: return "S";
  }
}

function symbolKindColor(kind: string): string {
  switch (kind) {
    case "class":
    case "struct": return "#e5a52c";
    case "interface": return "#4ec9b0";
    case "function":
    case "method":
    case "constructor": return "#b180d7";
    case "property":
    case "field":
    case "variable": return "#9cdcfe";
    case "enum": return "#e5a52c";
    case "constant": return "#569cd6";
    default: return "#6a9955";
  }
}

export default SymbolsPanel;
