/**
 * WelcomePanel.tsx
 *
 * Shown in the main content area when no project is open.
 * Provides a clear "Open Project Folder" entry point and a recent-projects list.
 */

import { type JSX, Show, For } from "solid-js";
import { projectsState } from "@/stores/projects.store";
import { openProjectFolder, switchProject } from "@/services/project.service";
import { Icon } from "@/components/common/Icon";
import type { ProjectEntry } from "@/bindings";

function shortenPath(p: string): string {
  const parts = p.split("/");
  if (parts.length <= 4) return p;
  return "~/" + parts.slice(-3).join("/");
}

function RecentProjectRow(props: { entry: ProjectEntry }): JSX.Element {
  return (
    <button
      onClick={() => switchProject(props.entry)}
      style={{
        display: "flex",
        "align-items": "center",
        gap: "12px",
        width: "100%",
        background: "none",
        border: "none",
        "border-radius": "6px",
        padding: "10px 14px",
        cursor: "pointer",
        "text-align": "left",
        transition: "background 0.1s",
      }}
      onMouseEnter={(e) => {
        (e.currentTarget as HTMLElement).style.background = "var(--bg-hover, rgba(255,255,255,0.06))";
      }}
      onMouseLeave={(e) => {
        (e.currentTarget as HTMLElement).style.background = "none";
      }}
    >
      <Icon name="folder" size={18} color="var(--text-muted)" />
      <div style={{ "min-width": "0", flex: "1" }}>
        <div
          style={{
            "font-size": "13px",
            "font-weight": "500",
            color: "var(--text-primary)",
            overflow: "hidden",
            "text-overflow": "ellipsis",
            "white-space": "nowrap",
          }}
        >
          {props.entry.name}
        </div>
        <div
          style={{
            "font-size": "11px",
            color: "var(--text-muted)",
            "margin-top": "2px",
            overflow: "hidden",
            "text-overflow": "ellipsis",
            "white-space": "nowrap",
          }}
        >
          {shortenPath(props.entry.path)}
        </div>
      </div>
      <Icon name="chevron-right" size={14} color="var(--text-muted)" />
    </button>
  );
}

export function WelcomePanel(): JSX.Element {
  return (
    <div
      style={{
        flex: "1",
        display: "flex",
        "align-items": "center",
        "justify-content": "center",
        background: "var(--bg-primary)",
        overflow: "auto",
      }}
    >
      <div
        style={{
          width: "100%",
          "max-width": "480px",
          padding: "48px 24px",
          display: "flex",
          "flex-direction": "column",
          "align-items": "center",
          gap: "0",
        }}
      >
        {/* App icon / heading */}
        <div
          style={{
            width: "56px",
            height: "56px",
            background: "var(--accent)",
            "border-radius": "14px",
            display: "flex",
            "align-items": "center",
            "justify-content": "center",
            "margin-bottom": "20px",
          }}
        >
          <Icon name="terminal" size={28} color="#fff" />
        </div>

        <h1
          style={{
            "font-size": "20px",
            "font-weight": "600",
            color: "var(--text-primary)",
            margin: "0 0 6px 0",
          }}
        >
          Android Dev Companion
        </h1>
        <p
          style={{
            "font-size": "13px",
            color: "var(--text-muted)",
            margin: "0 0 32px 0",
            "text-align": "center",
            "line-height": "1.5",
          }}
        >
          Build logs, logcat, and device management for your Android projects.
        </p>

        {/* Primary action */}
        <button
          onClick={() => openProjectFolder()}
          style={{
            display: "flex",
            "align-items": "center",
            gap: "8px",
            padding: "10px 24px",
            background: "var(--accent)",
            color: "#fff",
            border: "none",
            "border-radius": "6px",
            "font-size": "14px",
            "font-weight": "500",
            cursor: "pointer",
            transition: "opacity 0.1s",
            "margin-bottom": "40px",
          }}
          onMouseEnter={(e) => { (e.currentTarget as HTMLElement).style.opacity = "0.85"; }}
          onMouseLeave={(e) => { (e.currentTarget as HTMLElement).style.opacity = "1"; }}
        >
          <Icon name="folder-open" size={16} color="#fff" />
          Open Project Folder…
        </button>

        {/* Recent projects */}
        <Show when={projectsState.projects.length > 0}>
          <div style={{ width: "100%" }}>
            <div
              style={{
                "font-size": "11px",
                "font-weight": "500",
                color: "var(--text-muted)",
                "text-transform": "uppercase",
                "letter-spacing": "0.06em",
                "margin-bottom": "6px",
                "padding-left": "14px",
              }}
            >
              Recent
            </div>
            <div
              style={{
                background: "var(--bg-secondary)",
                border: "1px solid var(--border)",
                "border-radius": "8px",
                overflow: "hidden",
                padding: "4px",
              }}
            >
              <For each={projectsState.projects.slice(0, 8)}>
                {(entry) => <RecentProjectRow entry={entry} />}
              </For>
            </div>
          </div>
        </Show>

        {/* Keyboard hint */}
        <p
          style={{
            "font-size": "11px",
            color: "var(--text-muted)",
            "margin-top": "24px",
            opacity: "0.6",
          }}
        >
          Tip: press <kbd style={{ "font-family": "monospace", "font-size": "10px", padding: "1px 4px", background: "var(--bg-tertiary)", "border-radius": "3px", border: "1px solid var(--border)" }}>Cmd+O</kbd> to open a project folder
        </p>
      </div>
    </div>
  );
}

export default WelcomePanel;
