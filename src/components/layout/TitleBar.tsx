import { type JSX } from "solid-js";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { projectState } from "@/stores/project.store";

async function startDrag(e: MouseEvent) {
  // Only respond to left-button press; ignore right-click / middle-click
  if (e.button !== 0) return;

  // Prevent text selection while dragging
  e.preventDefault();

  try {
    await getCurrentWindow().startDragging();
  } catch {
    // startDragging() can throw if the mouse button was already released —
    // safe to ignore.
  }
}

export function TitleBar(): JSX.Element {
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
          ? `Android IDE — ${projectState.projectName}`
          : "Android IDE"}
      </span>
    </div>
  );
}

export default TitleBar;
