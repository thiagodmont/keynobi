import { type JSX, Show } from "solid-js";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { projectState } from "@/stores/project.store";
import { uiState } from "@/stores/ui.store";
import { isBuilding, isDeploying } from "@/stores/build.store";
import { runAndDeploy, cancelBuild } from "@/services/build.service";
import { formatError } from "@/lib/tauri-api";
import { Icon, showToast } from "@/components/ui";

async function startDrag(e: MouseEvent) {
  if (e.button !== 0) return;
  e.preventDefault();
  try {
    await getCurrentWindow().startDragging();
  } catch {
    // Safe to ignore — mouse button released before drag started.
  }
}

export function TitleBar(): JSX.Element {
  const buildActive = () => uiState.activeTab === "build";
  const runInFlight = () => isBuilding() || isDeploying();
  const runDisabled = () => !projectState.projectRoot && !runInFlight();

  async function handleBuildButtonClick() {
    if (runInFlight()) {
      await cancelBuild().catch((err) => {
        console.error(err);
        showToast(`Failed to cancel build: ${formatError(err)}`, "error");
      });
      return;
    }
    if (!projectState.projectRoot) return;
    try {
      await runAndDeploy();
    } catch (e) {
      showToast(formatError(e) || "Run failed", "error");
    }
  }

  const buildButtonTitle = () => {
    if (runInFlight()) return "Cancel build";
    if (!projectState.projectRoot) return "Open a project to run";
    return "Run App — build, install & launch (Cmd+R)";
  };

  return (
    <div
      onMouseDown={startDrag}
      style={{
        height: "var(--titlebar-height)",
        background: "var(--bg-tertiary)",
        "border-bottom": "1px solid var(--border)",
        display: "flex",
        "align-items": "center",
        "padding-left": "80px",
        "padding-right": "16px",
        "flex-shrink": "0",
        "user-select": "none",
        cursor: "default",
        gap: "12px",
      }}
    >
      <div
        style={{
          flex: "1",
          "min-width": "0",
          overflow: "hidden",
          "text-overflow": "ellipsis",
          "white-space": "nowrap",
        }}
      >
        <span
          style={{
            "font-size": "13px",
            color: "var(--text-secondary)",
            "font-weight": "400",
            "pointer-events": "none",
          }}
        >
          {projectState.projectName
            ? `Keynobi — ${projectState.projectName}`
            : "Keynobi"}
        </span>
      </div>
      <button
        type="button"
        onClick={() => void handleBuildButtonClick()}
        onMouseDown={(e) => e.stopPropagation()}
        disabled={runDisabled()}
        title={buildButtonTitle()}
        style={{
          "flex-shrink": "0",
          padding: "0 12px",
          height: "28px",
          "font-size": "12px",
          display: "flex",
          "align-items": "center",
          gap: "6px",
          color: buildActive() ? "var(--text-primary)" : "var(--text-muted)",
          background: buildActive() ? "var(--bg-secondary)" : "transparent",
          "border-bottom": buildActive() ? "2px solid var(--accent)" : "2px solid transparent",
          "box-sizing": "border-box",
          cursor: runDisabled() ? "not-allowed" : "pointer",
          border: "none",
          "border-radius": "4px",
          "font-weight": buildActive() ? "500" : "normal",
          transition: "color 0.1s, background 0.1s",
          opacity: runDisabled() ? "0.5" : "1",
        }}
      >
        <Show
          when={runInFlight()}
          fallback={<Icon name="play" size={13} color="var(--success)" />}
        >
          <Icon name="stop" size={13} color="var(--error)" />
        </Show>
        Build
      </button>
    </div>
  );
}

export default TitleBar;
