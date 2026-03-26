import { type JSX, For, Show, createSignal, createEffect } from "solid-js";
import { referencesState, hideCodeActions } from "@/stores/references.store";
import { applyWorkspaceEdit } from "@/lib/codemirror/lsp-extension";
import { getEditorView } from "@/components/editor/CodeEditor";
import { showToast } from "@/components/common/Toast";

/** Map an LSP code action kind string to a human-readable icon+label. */
function kindLabel(kind?: string): { icon: string; color: string } {
  if (!kind) return { icon: "⚡", color: "var(--accent)" };
  if (kind.startsWith("quickfix")) return { icon: "🔧", color: "#f0ad4e" };
  if (kind.startsWith("refactor")) return { icon: "♻️", color: "#5bc0de" };
  if (kind.startsWith("source.organizeImports")) return { icon: "🗂️", color: "#5cb85c" };
  if (kind.startsWith("source")) return { icon: "📁", color: "#5cb85c" };
  return { icon: "⚡", color: "var(--accent)" };
}

export function CodeActionsPopup(): JSX.Element {
  const [selectedIdx, setSelectedIdx] = createSignal(0);

  // Reset selection when the popup opens with new actions.
  createEffect(() => {
    if (referencesState.codeActionsVisible) {
      setSelectedIdx(0);
    }
  });

  async function applyAction(action: any) {
    hideCodeActions();
    try {
      if (action.edit) {
        await applyWorkspaceEdit(action.edit);
      } else if (action.command) {
        // Execute a server-side command (e.g. after applying the edit).
        const { invoke } = await import("@tauri-apps/api/core");
        await invoke("lsp_decompile", { uri: action.command.command }).catch(() => {});
      }
    } catch (err) {
      showToast(`Failed to apply action: ${err}`, "error");
    }
  }

  function onKeyDown(e: KeyboardEvent) {
    const actions = referencesState.codeActions;
    if (e.key === "ArrowDown") {
      e.preventDefault();
      setSelectedIdx((i) => Math.min(i + 1, actions.length - 1));
    } else if (e.key === "ArrowUp") {
      e.preventDefault();
      setSelectedIdx((i) => Math.max(i - 1, 0));
    } else if (e.key === "Enter") {
      e.preventDefault();
      const action = actions[selectedIdx()];
      if (action) applyAction(action);
    } else if (e.key === "Escape") {
      e.preventDefault();
      hideCodeActions();
      setTimeout(() => getEditorView()?.focus(), 0);
    }
  }

  // Compute popup position from the LSP cursor offset.
  const popupStyle = () => {
    const view = getEditorView();
    if (!view || !referencesState.codeActionsVisible) return "display:none";
    try {
      const coords = view.coordsAtPos(referencesState.codeActionsPos);
      if (!coords) return "display:none";
      return `top:${coords.bottom + 4}px;left:${coords.left}px`;
    } catch {
      return "display:none";
    }
  };

  return (
    <Show when={referencesState.codeActionsVisible && referencesState.codeActions.length > 0}>
      {/* Backdrop to close on outside click */}
      <div
        style={{
          position: "fixed",
          inset: "0",
          "z-index": "9998",
        }}
        onClick={hideCodeActions}
      />

      {/* Popup */}
      <div
        style={{
          position: "fixed",
          ...Object.fromEntries(
            popupStyle()
              .split(";")
              .filter(Boolean)
              .map((s) => s.split(":").map((v) => v.trim()) as [string, string])
          ),
          "z-index": "9999",
          background: "var(--bg-secondary)",
          border: "1px solid var(--border)",
          "border-radius": "6px",
          "box-shadow": "0 4px 16px rgba(0,0,0,0.4)",
          "min-width": "260px",
          "max-width": "420px",
          overflow: "hidden",
          outline: "none",
        }}
        tabindex="0"
        onKeyDown={onKeyDown}
        ref={(el) => {
          // Auto-focus when opened so keyboard nav works immediately.
          if (el) {
            createEffect(() => {
              if (referencesState.codeActionsVisible) {
                setTimeout(() => el.focus(), 0);
              }
            });
          }
        }}
      >
        {/* Header */}
        <div
          style={{
            padding: "6px 10px",
            "font-size": "10px",
            "font-weight": "600",
            color: "var(--text-muted)",
            "text-transform": "uppercase",
            "letter-spacing": "0.05em",
            "border-bottom": "1px solid var(--border)",
            background: "var(--bg-tertiary)",
          }}
        >
          Code Actions
        </div>

        {/* Action list */}
        <For each={referencesState.codeActions}>
          {(action, idx) => {
            const { icon, color } = kindLabel(action.kind);
            return (
              <div
                onClick={() => applyAction(action)}
                onMouseEnter={() => setSelectedIdx(idx())}
                style={{
                  display: "flex",
                  "align-items": "center",
                  gap: "8px",
                  padding: "7px 10px",
                  cursor: "pointer",
                  background: selectedIdx() === idx() ? "var(--bg-hover)" : "transparent",
                  "font-size": "12px",
                  color: "var(--text-primary)",
                  "border-bottom": "1px solid var(--border)",
                  transition: "background 0.08s",
                }}
              >
                <span style={{ color, "flex-shrink": "0", "font-size": "14px", width: "16px", "text-align": "center" }}>
                  {icon}
                </span>
                <span style={{ flex: "1", overflow: "hidden", "text-overflow": "ellipsis", "white-space": "nowrap" }}>
                  {action.title}
                </span>
                <Show when={action.kind}>
                  <span
                    style={{
                      "font-size": "9px",
                      color: "var(--text-muted)",
                      background: "var(--bg-quaternary)",
                      padding: "1px 5px",
                      "border-radius": "4px",
                      "flex-shrink": "0",
                    }}
                  >
                    {action.kind?.split(".").pop()}
                  </span>
                </Show>
              </div>
            );
          }}
        </For>
      </div>
    </Show>
  );
}

export default CodeActionsPopup;
