import { type JSX } from "solid-js";
import { editorState } from "@/stores/editor.store";
import { projectState } from "@/stores/project.store";

export function StatusBar(): JSX.Element {
  return (
    <div
      style={{
        height: "var(--statusbar-height)",
        background: "var(--accent)",
        display: "flex",
        "align-items": "center",
        "justify-content": "space-between",
        "padding": "0 8px",
        "flex-shrink": "0",
        "font-size": "11px",
        color: "#ffffff",
        "user-select": "none",
      }}
    >
      {/* Left side */}
      <div style={{ display: "flex", gap: "12px", "align-items": "center" }}>
        <span>
          {projectState.projectName
            ? `📁 ${projectState.projectName}`
            : "Ready"}
        </span>
      </div>

      {/* Right side */}
      <div style={{ display: "flex", gap: "12px", "align-items": "center" }}>
        <span>
          {editorState.cursorLine !== null && editorState.cursorCol !== null
            ? `Ln ${editorState.cursorLine}, Col ${editorState.cursorCol}`
            : ""}
        </span>
        <span>UTF-8</span>
        <span>
          {editorState.activeLanguage
            ? editorState.activeLanguage.charAt(0).toUpperCase() +
              editorState.activeLanguage.slice(1)
            : ""}
        </span>
      </div>
    </div>
  );
}

export default StatusBar;
