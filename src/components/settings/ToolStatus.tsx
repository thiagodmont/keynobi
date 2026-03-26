import { type JSX, Show, createSignal } from "solid-js";
import { lspState } from "@/stores/lsp.store";
import { settingsState } from "@/stores/settings.store";
import { detectSdkPath, detectJavaPath, formatError } from "@/lib/tauri-api";
import { invoke } from "@tauri-apps/api/core";
import Icon from "@/components/common/Icon";
import { showToast } from "@/components/common/Toast";

function statusBadge(
  status: "found" | "not-found" | "checking" | "downloading",
  label: string
): JSX.Element {
  const colors = {
    found: { bg: "rgba(74,222,128,0.15)", text: "#4ade80" },
    "not-found": { bg: "rgba(248,113,113,0.15)", text: "#f87171" },
    checking: { bg: "rgba(251,191,36,0.15)", text: "#fbbf24" },
    downloading: { bg: "rgba(96,165,250,0.15)", text: "#60a5fa" },
  };
  const c = colors[status];
  return (
    <span
      style={{
        display: "inline-flex",
        "align-items": "center",
        gap: "4px",
        padding: "2px 8px",
        "border-radius": "10px",
        background: c.bg,
        color: c.text,
        "font-size": "11px",
        "font-weight": "500",
      }}
    >
      <span
        style={{
          width: "6px",
          height: "6px",
          "border-radius": "50%",
          background: c.text,
        }}
      />
      {label}
    </span>
  );
}

export function AndroidSdkStatus(): JSX.Element {
  const [detected, setDetected] = createSignal<string | null | undefined>(undefined);

  async function detect() {
    setDetected(undefined);
    try {
      const path = await detectSdkPath();
      setDetected(path);
    } catch {
      setDetected(null);
    }
  }

  if (detected() === undefined) detect();

  const sdkPath = () => settingsState.android.sdkPath ?? detected() ?? null;

  return (
    <div style={{ display: "flex", "flex-direction": "column", gap: "6px" }}>
      <div style={{ display: "flex", "align-items": "center", gap: "8px" }}>
        {sdkPath()
          ? statusBadge("found", "Found")
          : statusBadge("not-found", "Not Found")}
        <button
          onClick={detect}
          style={{
            background: "none",
            border: "1px solid var(--border)",
            color: "var(--text-secondary)",
            padding: "2px 8px",
            "border-radius": "4px",
            cursor: "pointer",
            "font-size": "11px",
          }}
        >
          Detect
        </button>
      </div>
      <Show when={sdkPath()}>
        <span style={{ "font-size": "11px", color: "var(--text-muted)", "word-break": "break-all" }}>
          {sdkPath()}
        </span>
      </Show>
    </div>
  );
}

export function KotlinLspStatus(): JSX.Element {
  const [downloading, setDownloading] = createSignal(false);

  const status = () => {
    if (downloading()) return "downloading";
    switch (lspState.status.state) {
      case "ready": return "found";
      case "notInstalled":
      case "stopped": return "not-found";
      default: return "checking";
    }
  };

  const label = () => {
    if (downloading()) return "Downloading...";
    switch (lspState.status.state) {
      case "ready": return "Ready";
      case "starting": return "Starting...";
      case "indexing": return "Indexing...";
      case "downloading": return "Downloading...";
      case "error": return "Error";
      case "notInstalled": return "Not Installed";
      default: return "Stopped";
    }
  };

  async function downloadLsp() {
    setDownloading(true);
    try {
      await invoke("lsp_download");
      showToast("Kotlin LSP downloaded successfully", "success");
    } catch (err) {
      showToast(`Download failed: ${formatError(err)}`, "error");
    } finally {
      setDownloading(false);
    }
  }

  async function restartLsp() {
    try {
      await invoke("lsp_stop");
      const { getProjectRoot } = await import("@/lib/tauri-api");
      const root = await getProjectRoot();
      if (root) {
        await invoke("lsp_start", { projectRoot: root });
        showToast("Kotlin LSP restarted", "success");
      }
    } catch (err) {
      showToast(`Restart failed: ${formatError(err)}`, "error");
    }
  }

  return (
    <div style={{ display: "flex", "flex-direction": "column", gap: "6px" }}>
      <div style={{ display: "flex", "align-items": "center", gap: "8px", "flex-wrap": "wrap" }}>
        {statusBadge(status() as "found" | "not-found" | "checking" | "downloading", label())}
        <Show when={lspState.status.state === "notInstalled" || lspState.status.state === "stopped"}>
          <button
            onClick={downloadLsp}
            disabled={downloading()}
            style={{
              background: "var(--accent)",
              border: "none",
              color: "#fff",
              padding: "3px 10px",
              "border-radius": "4px",
              cursor: "pointer",
              "font-size": "11px",
            }}
          >
            <Icon name="download" size={12} /> Download
          </button>
        </Show>
        <Show when={lspState.status.state === "ready" || lspState.status.state === "error"}>
          <button
            onClick={restartLsp}
            style={{
              background: "none",
              border: "1px solid var(--border)",
              color: "var(--text-secondary)",
              padding: "2px 8px",
              "border-radius": "4px",
              cursor: "pointer",
              "font-size": "11px",
            }}
          >
            Restart LSP
          </button>
        </Show>
      </div>
      <span style={{ "font-size": "11px", color: "var(--text-muted)" }}>
        Version: 262.2310.0 | Path: ~/.androidide/kotlin-lsp/
      </span>
    </div>
  );
}

export function JavaStatus(): JSX.Element {
  const [detected, setDetected] = createSignal<string | null | undefined>(undefined);

  async function detect() {
    setDetected(undefined);
    try {
      const path = await detectJavaPath();
      setDetected(path);
    } catch {
      setDetected(null);
    }
  }

  if (detected() === undefined) detect();

  const javaPath = () => settingsState.java.home ?? detected() ?? null;

  return (
    <div style={{ display: "flex", "flex-direction": "column", gap: "6px" }}>
      <div style={{ display: "flex", "align-items": "center", gap: "8px" }}>
        {javaPath()
          ? statusBadge("found", "Found")
          : statusBadge("not-found", "Not Found")}
        <button
          onClick={detect}
          style={{
            background: "none",
            border: "1px solid var(--border)",
            color: "var(--text-secondary)",
            padding: "2px 8px",
            "border-radius": "4px",
            cursor: "pointer",
            "font-size": "11px",
          }}
        >
          Detect
        </button>
      </div>
      <Show when={javaPath()}>
        <span style={{ "font-size": "11px", color: "var(--text-muted)", "word-break": "break-all" }}>
          {javaPath()}
        </span>
      </Show>
    </div>
  );
}
