import { type JSX, createSignal, Show, For } from "solid-js";
import { Portal } from "solid-js/web";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { projectState } from "@/stores/project.store";
import { projectsState } from "@/stores/projects.store";
import { switchProject, openProjectFolder, removeProjectEntry, togglePinProject } from "@/services/project.service";
import type { ProjectEntry } from "@/bindings";

async function startDrag(e: MouseEvent) {
  if (e.button !== 0) return;
  e.preventDefault();
  try {
    await getCurrentWindow().startDragging();
  } catch {
    // Safe to ignore — mouse button released before drag started.
  }
}

// ── Project Switcher Dropdown ─────────────────────────────────────────────────

function shortenPath(p: string): string {
  const parts = p.split("/");
  if (parts.length <= 3) return p;
  return "…/" + parts.slice(-2).join("/");
}

interface ProjectRowProps {
  entry: ProjectEntry;
  isActive: boolean;
  onSelect: () => void;
  onPin: () => void;
  onRemove: () => void;
}

function ProjectRow(props: ProjectRowProps): JSX.Element {
  const [hover, setHover] = createSignal(false);

  return (
    <div
      onMouseEnter={() => setHover(true)}
      onMouseLeave={() => setHover(false)}
      onClick={props.onSelect}
      style={{
        display: "flex",
        "align-items": "center",
        padding: "7px 10px",
        cursor: "pointer",
        background: props.isActive
          ? "var(--accent-muted, rgba(82,139,255,0.15))"
          : hover()
          ? "var(--bg-hover, rgba(255,255,255,0.05))"
          : "transparent",
        "border-radius": "4px",
        gap: "6px",
      }}
    >
      {/* Pin star */}
      <button
        onClick={(e) => { e.stopPropagation(); props.onPin(); }}
        title={props.entry.pinned ? "Unpin" : "Pin"}
        style={{
          background: "none",
          border: "none",
          padding: "0",
          cursor: "pointer",
          color: props.entry.pinned ? "var(--accent, #528bff)" : "var(--text-muted, #555)",
          "font-size": "12px",
          "flex-shrink": "0",
          opacity: props.entry.pinned || hover() ? "1" : "0",
          transition: "opacity 0.1s",
        }}
      >
        ★
      </button>

      {/* Project info */}
      <div style={{ flex: "1", "min-width": "0" }}>
        <div
          style={{
            "font-size": "12px",
            color: props.isActive ? "var(--accent, #528bff)" : "var(--text-primary)",
            "font-weight": props.isActive ? "500" : "normal",
            overflow: "hidden",
            "text-overflow": "ellipsis",
            "white-space": "nowrap",
          }}
        >
          {props.entry.name}
        </div>
        <div
          style={{
            "font-size": "10px",
            color: "var(--text-muted)",
            overflow: "hidden",
            "text-overflow": "ellipsis",
            "white-space": "nowrap",
            "margin-top": "1px",
          }}
        >
          {shortenPath(props.entry.path)}
        </div>
      </div>

      {/* Remove button */}
      <Show when={hover() && !props.isActive}>
        <button
          onClick={(e) => { e.stopPropagation(); props.onRemove(); }}
          title="Remove from list"
          style={{
            background: "none",
            border: "none",
            padding: "0 2px",
            cursor: "pointer",
            color: "var(--text-muted)",
            "font-size": "14px",
            "flex-shrink": "0",
            "line-height": "1",
          }}
        >
          ×
        </button>
      </Show>
    </div>
  );
}

// ── Main Component ─────────────────────────────────────────────────────────────

export function TitleBar(): JSX.Element {
  const [dropdownOpen, setDropdownOpen] = createSignal(false);

  function closeDropdown() {
    setDropdownOpen(false);
  }

  // Close on outside click.
  function handleBackdropClick() {
    closeDropdown();
  }

  async function handleSelectProject(entry: ProjectEntry) {
    closeDropdown();
    await switchProject(entry);
  }

  async function handleOpenFolder() {
    closeDropdown();
    await openProjectFolder();
  }

  async function handlePin(entry: ProjectEntry) {
    await togglePinProject(entry.id, !entry.pinned);
  }

  async function handleRemove(entry: ProjectEntry) {
    await removeProjectEntry(entry.id);
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
        position: "relative",
      }}
    >
      {/* Project switcher button */}
      <button
        onMouseDown={(e) => e.stopPropagation()}
        onClick={(e) => {
          e.stopPropagation();
          setDropdownOpen((v) => !v);
        }}
        title="Switch project"
        style={{
          background: "none",
          border: "none",
          padding: "2px 6px",
          cursor: "pointer",
          display: "flex",
          "align-items": "center",
          gap: "4px",
          "border-radius": "4px",
          transition: "background 0.1s",
          color: "var(--text-secondary)",
        }}
        onMouseEnter={(e) => {
          (e.currentTarget as HTMLElement).style.background = "var(--bg-hover, rgba(255,255,255,0.06))";
        }}
        onMouseLeave={(e) => {
          (e.currentTarget as HTMLElement).style.background = "none";
        }}
      >
        <span style={{ "font-size": "13px", "font-weight": "400" }}>
          {projectState.projectName
            ? `Android IDE — ${projectState.projectName}`
            : "Android IDE"}
        </span>
        <span style={{ "font-size": "9px", opacity: "0.6", "margin-top": "1px" }}>▾</span>
      </button>

      {/* Dropdown portal */}
      <Show when={dropdownOpen()}>
        <Portal>
          {/* Invisible backdrop to close on outside click */}
          <div
            style={{
              position: "fixed",
              inset: "0",
              "z-index": "9998",
            }}
            onClick={handleBackdropClick}
          />

          {/* Dropdown panel */}
          <div
            onMouseDown={(e) => e.stopPropagation()}
            style={{
              position: "fixed",
              top: "var(--titlebar-height, 38px)",
              left: "80px",
              "z-index": "9999",
              background: "var(--bg-secondary)",
              border: "1px solid var(--border)",
              "border-radius": "6px",
              "box-shadow": "0 8px 24px rgba(0,0,0,0.5)",
              width: "300px",
              padding: "6px",
              "max-height": "380px",
              "overflow-y": "auto",
            }}
          >
            <Show
              when={projectsState.projects.length > 0}
              fallback={
                <div
                  style={{
                    padding: "12px 10px",
                    "font-size": "12px",
                    color: "var(--text-muted)",
                    "text-align": "center",
                  }}
                >
                  No projects yet. Open a folder to get started.
                </div>
              }
            >
              <For each={projectsState.projects}>
                {(entry) => (
                  <ProjectRow
                    entry={entry}
                    isActive={entry.path === projectState.projectRoot}
                    onSelect={() => handleSelectProject(entry)}
                    onPin={() => handlePin(entry)}
                    onRemove={() => handleRemove(entry)}
                  />
                )}
              </For>
            </Show>

            {/* Divider + Open Folder */}
            <div style={{ height: "1px", background: "var(--border)", margin: "6px 0" }} />
            <button
              onClick={handleOpenFolder}
              style={{
                width: "100%",
                background: "none",
                border: "none",
                "text-align": "left",
                padding: "7px 10px",
                "font-size": "12px",
                color: "var(--text-secondary)",
                cursor: "pointer",
                "border-radius": "4px",
                display: "flex",
                "align-items": "center",
                gap: "8px",
              }}
              onMouseEnter={(e) => {
                (e.currentTarget as HTMLElement).style.background = "var(--bg-hover, rgba(255,255,255,0.05))";
              }}
              onMouseLeave={(e) => {
                (e.currentTarget as HTMLElement).style.background = "none";
              }}
            >
              <span>Open Folder…</span>
              <span style={{ "margin-left": "auto", color: "var(--text-muted)", "font-size": "11px" }}>
                Cmd+O
              </span>
            </button>
          </div>
        </Portal>
      </Show>
    </div>
  );
}

export default TitleBar;
