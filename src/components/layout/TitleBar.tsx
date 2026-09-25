import { type JSX, Show, createSignal, onMount } from "solid-js";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { projectState } from "@/stores/project.store";
import { toggleLogMode, uiState } from "@/stores/ui.store";
import { buildState, isBuilding, isDeploying } from "@/stores/build.store";
import { runAndDeploy, cancelBuild, SAFE_MODE_BUILD_TITLE } from "@/services/build.service";
import { askToTrustActiveProject } from "@/services/project.service";
import { isActiveProjectTrusted } from "@/stores/projects.store";
import { formatError } from "@/lib/tauri-api";
import { Badge, Icon, showToast } from "@/components/ui";

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
  const [alwaysOnTop, setAlwaysOnTop] = createSignal(false);
  const [alwaysOnTopBusy, setAlwaysOnTopBusy] = createSignal(false);
  let userChangedAlwaysOnTop = false;
  const buildActive = () => uiState.activeTab === "build";
  const runInFlight = () => isBuilding() || isDeploying();
  // Only Gradle can be cancelled; install and launch run to completion.
  const deployingAfterBuild = () => isDeploying() && !isBuilding();
  const safeMode = () => !!projectState.projectRoot && !isActiveProjectTrusted();
  const runDisabled = () =>
    deployingAfterBuild() || ((!projectState.projectRoot || safeMode()) && !runInFlight());

  onMount(() => {
    getCurrentWindow()
      .isAlwaysOnTop()
      .then((current) => {
        if (!userChangedAlwaysOnTop) setAlwaysOnTop(current);
      })
      .catch(() => {
        // Web/test mode may not expose the full window surface.
      });
  });

  async function handleBuildButtonClick() {
    if (deployingAfterBuild()) return;
    if (isBuilding()) {
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
    if (isBuilding()) return "Cancel build";
    if (buildState.deployPhase === "installing") return "Installing APK…";
    if (buildState.deployPhase === "launching") return "Launching app…";
    if (deployingAfterBuild()) return "Deploying…";
    if (!projectState.projectRoot) return "Open a project to run";
    if (safeMode()) return SAFE_MODE_BUILD_TITLE;
    return "Run App — build, install & launch (Cmd+R)";
  };

  const logModeActive = () => uiState.logMode.active;

  const logModeButtonTitle = () => (logModeActive() ? "Exit Log Mode" : "Enter Log Mode");

  async function handleAlwaysOnTopToggle() {
    if (alwaysOnTopBusy()) return;
    const next = !alwaysOnTop();
    setAlwaysOnTopBusy(true);
    try {
      await getCurrentWindow().setAlwaysOnTop(next);
      userChangedAlwaysOnTop = true;
      setAlwaysOnTop(next);
    } catch (e) {
      showToast(`Failed to update window pinning: ${formatError(e)}`, "error");
    } finally {
      setAlwaysOnTopBusy(false);
    }
  }

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
          {projectState.projectName ? `Keynobi — ${projectState.projectName}` : "Keynobi"}
        </span>
      </div>
      <Show when={safeMode()}>
        <Badge
          variant="warning"
          size="xs"
          title="This project's Gradle build scripts do not run until you trust it. Click to trust it."
          onMouseDown={(e) => e.stopPropagation()}
          onClick={() => void askToTrustActiveProject().catch(console.error)}
        >
          Safe Mode
        </Badge>
      </Show>
      <button
        type="button"
        onClick={() => toggleLogMode()}
        onMouseDown={(e) => e.stopPropagation()}
        aria-pressed={logModeActive() ? "true" : undefined}
        title={logModeButtonTitle()}
        style={{
          "flex-shrink": "0",
          padding: "0 10px",
          height: "28px",
          "font-size": "12px",
          display: "flex",
          "align-items": "center",
          gap: "6px",
          color: logModeActive() ? "var(--text-primary)" : "var(--text-muted)",
          background: logModeActive() ? "var(--bg-secondary)" : "transparent",
          "border-bottom": logModeActive() ? "2px solid var(--accent)" : "2px solid transparent",
          "box-sizing": "border-box",
          cursor: "pointer",
          border: "none",
          "border-radius": "4px",
          "font-weight": logModeActive() ? "500" : "normal",
          transition: "color 0.1s, background 0.1s",
        }}
      >
        <Icon
          name="terminal"
          size={13}
          color={logModeActive() ? "var(--accent)" : "currentColor"}
        />
        Log Mode
      </button>
      <button
        type="button"
        onClick={() => void handleAlwaysOnTopToggle()}
        onMouseDown={(e) => e.stopPropagation()}
        disabled={alwaysOnTopBusy()}
        aria-pressed={alwaysOnTop() ? "true" : undefined}
        title={alwaysOnTop() ? "Stop keeping window on top" : "Keep window on top"}
        style={{
          "flex-shrink": "0",
          padding: "0 10px",
          height: "28px",
          "font-size": "12px",
          display: "flex",
          "align-items": "center",
          gap: "6px",
          color: alwaysOnTop() ? "var(--text-primary)" : "var(--text-muted)",
          background: alwaysOnTop() ? "var(--bg-secondary)" : "transparent",
          "border-bottom": alwaysOnTop() ? "2px solid var(--accent)" : "2px solid transparent",
          "box-sizing": "border-box",
          cursor: alwaysOnTopBusy() ? "wait" : "pointer",
          border: "none",
          "border-radius": "4px",
          "font-weight": alwaysOnTop() ? "500" : "normal",
          transition: "color 0.1s, background 0.1s",
          opacity: alwaysOnTopBusy() ? "0.6" : "1",
        }}
      >
        <Icon name="pin" size={13} color={alwaysOnTop() ? "var(--accent)" : "currentColor"} />
        On Top
      </button>
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
        <Show when={isBuilding()} fallback={<Icon name="play" size={13} color="var(--success)" />}>
          <Icon name="stop" size={13} color="var(--error)" />
        </Show>
        Build
      </button>
    </div>
  );
}

export default TitleBar;
