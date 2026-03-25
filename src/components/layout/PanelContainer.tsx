import { type JSX, For, Show } from "solid-js";
import {
  uiState,
  setActiveBottomTab,
  type BottomPanelTab,
} from "@/stores/ui.store";

const TABS: { id: BottomPanelTab; label: string }[] = [
  { id: "build", label: "Build" },
  { id: "logcat", label: "Logcat" },
  { id: "terminal", label: "Terminal" },
];

interface PanelContainerProps {
  height: number;
}

export function PanelContainer(props: PanelContainerProps): JSX.Element {
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
            return (
              <button
                onClick={() => setActiveBottomTab(tab.id)}
                style={{
                  padding: "0 16px",
                  height: "35px",
                  "font-size": "12px",
                  color: isActive()
                    ? "var(--text-primary)"
                    : "var(--text-muted)",
                  background: isActive() ? "var(--bg-secondary)" : "none",
                  "border-bottom": isActive()
                    ? "1px solid var(--accent)"
                    : "1px solid transparent",
                  cursor: "pointer",
                }}
              >
                {tab.label}
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
          "align-items": "center",
          "justify-content": "center",
          color: "var(--text-muted)",
          "font-size": "12px",
        }}
      >
        <Show
          when={uiState.activeBottomTab === "build"}
          fallback={
            <span>
              {uiState.activeBottomTab === "logcat"
                ? "Logcat — Available in Phase 4"
                : "Terminal — Available in Phase 6"}
            </span>
          }
        >
          <span>Build — Available in Phase 2</span>
        </Show>
      </div>
    </div>
  );
}

export default PanelContainer;
