import { type JSX } from "solid-js";
import { projectState } from "@/stores/project.store";

export function TitleBar(): JSX.Element {
  return (
    <div
      data-tauri-drag-region
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
        // CSS-level drag for WebKit — works natively even when the OS
        // title bar overlays this area in Overlay mode.
        "-webkit-app-region": "drag",
      } as JSX.CSSProperties & { "-webkit-app-region": string }}
    >
      <span
        style={{
          "font-size": "13px",
          color: "var(--text-secondary)",
          "font-weight": "400",
          // Prevent the text itself from being draggable
          "-webkit-app-region": "no-drag",
          "pointer-events": "none",
        } as JSX.CSSProperties & { "-webkit-app-region": string }}
      >
        {projectState.projectName
          ? `Android IDE — ${projectState.projectName}`
          : "Android IDE"}
      </span>
    </div>
  );
}

export default TitleBar;
