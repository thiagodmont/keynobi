import { type JSX, For, Show, Switch, Match } from "solid-js";
import {
  uiState,
  setActiveBottomTab,
  type BottomPanelTab,
} from "@/stores/ui.store";
import { getDiagnosticCounts } from "@/stores/lsp.store";
import { referencesState } from "@/stores/references.store";
import { ProblemsPanel } from "@/components/layout/ProblemsPanel";
import { ReferencesPanel } from "@/components/editor/ReferencesPanel";

const TABS: { id: BottomPanelTab; label: string }[] = [
  { id: "problems", label: "Problems" },
  { id: "references", label: "References" },
  { id: "build", label: "Build" },
  { id: "logcat", label: "Logcat" },
  { id: "terminal", label: "Terminal" },
];

interface PanelContainerProps {
  height: number;
}

export function PanelContainer(props: PanelContainerProps): JSX.Element {
  const counts = () => getDiagnosticCounts();

  return (
    <div
      style={{
        height: `${props.height}px`,
        "min-height": "100px",
        background: "var(--bg-secondary)",
        "border-top": "1px solid var(--border)",
        display: "flex",
        "flex-direction": "column",
        overflow: "hidden",
        "flex-shrink": "0",
      }}
    >
      {/* Tab bar */}
      <div
        style={{
          display: "flex",
          "align-items": "center",
          height: "35px",
          background: "var(--bg-tertiary)",
          "border-bottom": "1px solid var(--border)",
          "padding-left": "8px",
          "flex-shrink": "0",
        }}
      >
        <For each={TABS}>
          {(tab) => {
            const isActive = () => uiState.activeBottomTab === tab.id;
            const badge = () => {
              if (tab.id === "problems") {
                const c = counts();
                const total = c.errors + c.warnings;
                return total > 0 ? total : null;
              }
              if (tab.id === "references") {
                return referencesState.totalCount > 0 ? referencesState.totalCount : null;
              }
              return null;
            };

            return (
              <button
                onClick={() => setActiveBottomTab(tab.id)}
                style={{
                  padding: "0 16px",
                  height: "35px",
                  "font-size": "12px",
                  display: "flex",
                  "align-items": "center",
                  gap: "4px",
                  color: isActive()
                    ? "var(--text-primary)"
                    : "var(--text-muted)",
                  background: isActive() ? "var(--bg-secondary)" : "none",
                  "border-bottom": isActive()
                    ? "1px solid var(--accent)"
                    : "1px solid transparent",
                  cursor: "pointer",
                  border: "none",
                  "border-top": "none",
                  "border-left": "none",
                  "border-right": "none",
                }}
              >
                {tab.label}
                <Show when={badge()}>
                  <span
                    style={{
                      "font-size": "10px",
                      "min-width": "16px",
                      height: "16px",
                      display: "flex",
                      "align-items": "center",
                      "justify-content": "center",
                      "border-radius": "8px",
                      background: "var(--accent)",
                      color: "#fff",
                      padding: "0 4px",
                    }}
                  >
                    {badge()}
                  </span>
                </Show>
              </button>
            );
          }}
        </For>
      </div>

      {/* Content */}
      <div
        style={{
          flex: "1",
          overflow: "hidden",
          display: "flex",
          "flex-direction": "column",
        }}
      >
        <Switch>
          <Match when={uiState.activeBottomTab === "problems"}>
            <ProblemsPanel />
          </Match>
          <Match when={uiState.activeBottomTab === "references"}>
            <ReferencesPanel />
          </Match>
          <Match when={uiState.activeBottomTab === "build"}>
            <PanelPlaceholder text="Build — Available in Phase 3" />
          </Match>
          <Match when={uiState.activeBottomTab === "logcat"}>
            <PanelPlaceholder text="Logcat — Available in Phase 4" />
          </Match>
          <Match when={uiState.activeBottomTab === "terminal"}>
            <PanelPlaceholder text="Terminal — Available in Phase 6" />
          </Match>
        </Switch>
      </div>
    </div>
  );
}

function PanelPlaceholder(props: { text: string }): JSX.Element {
  return (
    <div
      style={{
        flex: "1",
        display: "flex",
        "align-items": "center",
        "justify-content": "center",
        color: "var(--text-muted)",
        "font-size": "12px",
      }}
    >
      {props.text}
    </div>
  );
}

export default PanelContainer;
