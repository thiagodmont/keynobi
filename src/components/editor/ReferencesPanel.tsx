import { type JSX, For, Show, createSignal } from "solid-js";
import { referencesState, hideReferences } from "@/stores/references.store";
import { openFileAtLocation } from "@/services/project.service";
import Icon from "@/components/common/Icon";

export function ReferencesPanel(): JSX.Element {
  const [collapsed, setCollapsed] = createSignal<Set<string>>(new Set());

  function toggleCollapse(path: string) {
    setCollapsed((prev) => {
      const next = new Set(prev);
      if (next.has(path)) next.delete(path);
      else next.add(path);
      return next;
    });
  }

  function handleItemClick(path: string, line: number, col: number) {
    openFileAtLocation(path, line + 1, col);
  }

  return (
    <div
      style={{
        flex: "1",
        display: "flex",
        "flex-direction": "column",
        overflow: "hidden",
        "font-size": "12px",
      }}
    >
      {/* Header */}
      <div
        style={{
          display: "flex",
          "align-items": "center",
          "justify-content": "space-between",
          padding: "4px 12px",
          "border-bottom": "1px solid var(--border)",
          "flex-shrink": "0",
          color: "var(--text-secondary)",
          background: "var(--bg-secondary)",
        }}
      >
        <span>
          <Show
            when={referencesState.totalCount > 0}
            fallback={<span style={{ color: "var(--text-muted)" }}>No references found</span>}
          >
            <strong style={{ color: "var(--text-primary)" }}>{referencesState.totalCount}</strong>
            {" "}result{referencesState.totalCount !== 1 ? "s" : ""} for{" "}
            <strong style={{ color: "var(--accent)" }}>"{referencesState.query}"</strong>
            {" "}in{" "}
            <strong style={{ color: "var(--text-primary)" }}>{referencesState.groups.length}</strong>
            {" "}file{referencesState.groups.length !== 1 ? "s" : ""}
          </Show>
        </span>
        <button
          onClick={hideReferences}
          title="Close references"
          style={{
            background: "none",
            border: "none",
            color: "var(--text-muted)",
            cursor: "pointer",
            padding: "2px",
            display: "flex",
            "align-items": "center",
          }}
        >
          <Icon name="close" size={14} />
        </button>
      </div>

      {/* Results */}
      <div style={{ flex: "1", overflow: "auto" }}>
        <For each={referencesState.groups}>
          {(group) => {
            const isCollapsed = () => collapsed().has(group.path);
            return (
              <div>
                {/* File header */}
                <div
                  onClick={() => toggleCollapse(group.path)}
                  style={{
                    display: "flex",
                    "align-items": "center",
                    gap: "6px",
                    padding: "3px 8px",
                    cursor: "pointer",
                    background: "var(--bg-tertiary)",
                    "border-bottom": "1px solid var(--border)",
                    "user-select": "none",
                  }}
                >
                  <Icon name={isCollapsed() ? "chevron-right" : "chevron-down"} size={12} color="var(--text-muted)" />
                  <Icon name="file" size={14} color="var(--accent)" />
                  <span style={{ "font-weight": "600", color: "var(--text-primary)" }}>
                    {group.relativePath}
                  </span>
                  <span
                    style={{
                      "font-size": "10px",
                      color: "var(--text-muted)",
                      background: "var(--bg-quaternary)",
                      padding: "0 5px",
                      "border-radius": "8px",
                    }}
                  >
                    {group.items.length}
                  </span>
                </div>

                {/* Reference rows */}
                <Show when={!isCollapsed()}>
                  <For each={group.items}>
                    {(item) => (
                      <div
                        onClick={() => handleItemClick(item.path, item.line, item.col)}
                        role="button"
                        tabindex="0"
                        onKeyDown={(e) => {
                          if (e.key === "Enter" || e.key === " ") {
                            e.preventDefault();
                            handleItemClick(item.path, item.line, item.col);
                          }
                        }}
                        style={{
                          display: "flex",
                          "align-items": "center",
                          gap: "8px",
                          padding: "2px 8px 2px 32px",
                          cursor: "pointer",
                          "border-bottom": "1px solid var(--border)",
                          "font-family": "var(--font-mono)",
                        }}
                        onMouseEnter={(e) => {
                          (e.currentTarget as HTMLElement).style.background = "var(--bg-hover)";
                        }}
                        onMouseLeave={(e) => {
                          (e.currentTarget as HTMLElement).style.background = "transparent";
                        }}
                      >
                        <span
                          style={{
                            color: "var(--text-muted)",
                            "min-width": "32px",
                            "text-align": "right",
                            "flex-shrink": "0",
                            "font-size": "11px",
                          }}
                        >
                          {item.displayLine}
                        </span>
                        <span
                          style={{
                            color: "var(--text-secondary)",
                            overflow: "hidden",
                            "text-overflow": "ellipsis",
                            "white-space": "nowrap",
                          }}
                        >
                          {item.lineContent || `Line ${item.displayLine}, Col ${item.col}`}
                        </span>
                      </div>
                    )}
                  </For>
                </Show>
              </div>
            );
          }}
        </For>

        <Show when={referencesState.groups.length === 0 && referencesState.totalCount === 0}>
          <div
            style={{
              display: "flex",
              "align-items": "center",
              "justify-content": "center",
              height: "100%",
              color: "var(--text-muted)",
              "font-size": "12px",
            }}
          >
            No references found. Place cursor on a symbol and press Shift+F12.
          </div>
        </Show>
      </div>
    </div>
  );
}

export default ReferencesPanel;
