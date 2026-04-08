/**
 * ProjectSidebar.tsx
 *
 * Persistent left sidebar showing the project registry.
 * - Expanded (220px): avatar + name + shortened path + last variant
 * - Collapsed (40px): avatar icon rail only
 *
 * Selecting a project updates the build target only.
 * Logcat and devices are not affected.
 */

import { type JSX, createSignal, Show, For } from "solid-js";
import { projectState } from "@/stores/project.store";
import { projectsState } from "@/stores/projects.store";
import { uiState, toggleSidebar } from "@/stores/ui.store";
import { showToast } from "@/components/common/Toast";
import { formatError } from "@/lib/tauri-api";
import {
  selectProject,
  openProjectFolder,
  removeProjectEntry,
  renameProjectEntry,
} from "@/services/project.service";
import { Icon } from "@/components/common/Icon";
import type { ProjectEntry } from "@/bindings";

// ── Avatar color ──────────────────────────────────────────────────────────────

const AVATAR_COLORS = [
  "#5c7cfa", "#339af0", "#20c997", "#51cf66",
  "#fcc419", "#ff6b6b", "#cc5de8", "#f06595",
  "#74c0fc", "#63e6be",
];

function avatarColor(id: string): string {
  let hash = 0;
  for (let i = 0; i < id.length; i++) {
    hash = (hash * 31 + id.charCodeAt(i)) >>> 0;
  }
  return AVATAR_COLORS[hash % AVATAR_COLORS.length];
}

function initials(name: string): string {
  const words = name.trim().split(/[\s_-]+/).filter(Boolean);
  if (words.length === 0) return "??";
  if (words.length === 1) return words[0].slice(0, 2).toUpperCase();
  return (words[0][0] + words[1][0]).toUpperCase();
}

function shortenPath(p: string): string {
  const home = p.startsWith("/Users/") || p.startsWith("/home/");
  const parts = p.split("/").filter(Boolean);
  if (parts.length <= 2) return p;
  if (home) {
    // ~/last-two/parts
    return "~/" + parts.slice(-2).join("/");
  }
  return "…/" + parts.slice(-2).join("/");
}

// ── Project row ───────────────────────────────────────────────────────────────

interface ProjectRowProps {
  entry: ProjectEntry;
  isActive: boolean;
  collapsed: boolean;
}

function ProjectRow(props: ProjectRowProps): JSX.Element {
  const [hover, setHover] = createSignal(false);
  const [editing, setEditing] = createSignal(false);
  // eslint-disable-next-line solid/reactivity
  const [editValue, setEditValue] = createSignal(props.entry.name);

  const color = () => avatarColor(props.entry.id);
  const letters = () => initials(props.entry.name);

  function startEdit(e: MouseEvent) {
    e.stopPropagation();
    setEditValue(props.entry.name);
    setEditing(true);
  }

  function commitEdit() {
    const trimmed = editValue().trim();
    if (trimmed && trimmed !== props.entry.name) {
      renameProjectEntry(props.entry.id, trimmed).catch(e => { console.error(e); showToast(`Failed to rename project: ${formatError(e)}`, "error"); });
    }
    setEditing(false);
  }

  function handleKeyDown(e: KeyboardEvent) {
    if (e.key === "Enter") commitEdit();
    if (e.key === "Escape") setEditing(false);
  }

  return (
    <div
      onMouseEnter={() => setHover(true)}
      onMouseLeave={() => setHover(false)}
      onClick={() => { if (!editing()) selectProject(props.entry).catch(e => { console.error(e); showToast(`Failed to open project: ${formatError(e)}`, "error"); }); }}
      title={props.collapsed ? props.entry.name : undefined}
      style={{
        display: "flex",
        "align-items": "flex-start",
        gap: props.collapsed ? "0" : "10px",
        padding: props.collapsed ? "6px 0" : "8px 10px",
        cursor: editing() ? "default" : "pointer",
        "border-radius": "6px",
        "margin-bottom": "2px",
        background: props.isActive
          ? "var(--accent-muted, rgba(92,124,250,0.18))"
          : hover()
          ? "var(--bg-hover, rgba(255,255,255,0.05))"
          : "transparent",
        transition: "background 0.1s",
        "justify-content": props.collapsed ? "center" : "flex-start",
        position: "relative",
      }}
    >
      {/* Avatar */}
      <div
        style={{
          width: "28px",
          height: "28px",
          "border-radius": "6px",
          background: color(),
          display: "flex",
          "align-items": "center",
          "justify-content": "center",
          "font-size": "11px",
          "font-weight": "700",
          color: "#fff",
          "flex-shrink": "0",
          "letter-spacing": "0.03em",
          outline: props.isActive ? `2px solid ${color()}` : "none",
          "outline-offset": "1px",
        }}
      >
        {letters()}
      </div>

      {/* Text — only when expanded */}
      <Show when={!props.collapsed}>
        <div style={{ flex: "1", "min-width": "0", overflow: "hidden" }}>
          <Show
            when={editing()}
            fallback={
              <div
                style={{
                  "font-size": "12px",
                  "font-weight": props.isActive ? "600" : "500",
                  color: props.isActive ? "var(--accent, #5c7cfa)" : "var(--text-primary)",
                  overflow: "hidden",
                  "text-overflow": "ellipsis",
                  "white-space": "nowrap",
                  "line-height": "1.3",
                }}
              >
                {props.entry.name}
              </div>
            }
          >
            <input
              type="text"
              value={editValue()}
              onInput={(e) => setEditValue(e.currentTarget.value)}
              onKeyDown={handleKeyDown}
              onBlur={commitEdit}
              onClick={(e) => e.stopPropagation()}
              ref={(el) => setTimeout(() => el?.select(), 0)}
              style={{
                width: "100%",
                background: "var(--bg-primary)",
                border: "1px solid var(--accent)",
                "border-radius": "3px",
                color: "var(--text-primary)",
                "font-size": "12px",
                padding: "1px 5px",
                outline: "none",
                "box-sizing": "border-box",
              }}
            />
          </Show>

          <div
            style={{
              display: "flex",
              "align-items": "center",
              gap: "5px",
              "margin-top": "1px",
            }}
          >
            <span
              style={{
                "font-size": "10px",
                color: "var(--text-muted)",
                overflow: "hidden",
                "text-overflow": "ellipsis",
                "white-space": "nowrap",
                "flex-shrink": "1",
                "min-width": "0",
              }}
            >
              {props.entry.lastBuildVariant
                ? `${props.entry.lastBuildVariant} · ${shortenPath(props.entry.path)}`
                : shortenPath(props.entry.path)}
            </span>
            <Show when={props.entry.gradleRoot !== null}>
              <span
                title={`gradlew found at ${props.entry.gradleRoot}`}
                style={{
                  "font-size": "9px",
                  color: "var(--success, #4ade80)",
                  background: "rgba(74,222,128,0.12)",
                  border: "1px solid rgba(74,222,128,0.25)",
                  "border-radius": "3px",
                  padding: "0 4px",
                  "line-height": "14px",
                  "flex-shrink": "0",
                  "white-space": "nowrap",
                }}
              >
                gradlew
              </span>
            </Show>
          </div>
        </div>

        {/* Hover actions */}
        <Show when={hover() && !editing()}>
          <div
            style={{
              display: "flex",
              "align-items": "center",
              gap: "2px",
              "flex-shrink": "0",
              "margin-left": "2px",
            }}
          >
            <button
              onClick={startEdit}
              title="Rename"
              style={{
                background: "none",
                border: "none",
                padding: "2px",
                cursor: "pointer",
                color: "var(--text-muted)",
                display: "flex",
                "align-items": "center",
                "border-radius": "3px",
              }}
              onMouseEnter={(e) => { (e.currentTarget as HTMLElement).style.background = "var(--bg-tertiary)"; }}
              onMouseLeave={(e) => { (e.currentTarget as HTMLElement).style.background = "none"; }}
            >
              <Icon name="pencil" size={11} />
            </button>
            <Show when={!props.isActive}>
              <button
                onClick={(e) => {
                  e.stopPropagation();
                  removeProjectEntry(props.entry.id).catch(e => { console.error(e); showToast(`Failed to remove project: ${formatError(e)}`, "error"); });
                }}
                title="Remove from list"
                style={{
                  background: "none",
                  border: "none",
                  padding: "2px",
                  cursor: "pointer",
                  color: "var(--text-muted)",
                  "font-size": "13px",
                  "line-height": "1",
                  "border-radius": "3px",
                }}
                onMouseEnter={(e) => { (e.currentTarget as HTMLElement).style.background = "var(--bg-tertiary)"; }}
                onMouseLeave={(e) => { (e.currentTarget as HTMLElement).style.background = "none"; }}
              >
                ×
              </button>
            </Show>
          </div>
        </Show>
      </Show>
    </div>
  );
}

// ── Main component ────────────────────────────────────────────────────────────

export function ProjectSidebar(): JSX.Element {
  const collapsed = () => uiState.sidebarCollapsed;

  return (
    <div
      style={{
        width: collapsed() ? "48px" : "220px",
        "min-width": collapsed() ? "48px" : "220px",
        "max-width": collapsed() ? "48px" : "220px",
        transition: "width 0.18s ease, min-width 0.18s ease, max-width 0.18s ease",
        height: "100%",
        display: "flex",
        "flex-direction": "column",
        background: "var(--bg-secondary)",
        "border-right": "1px solid var(--border)",
        overflow: "hidden",
        "flex-shrink": "0",
      }}
    >
      {/* ── Header ── */}
      <div
        style={{
          display: "flex",
          "align-items": "center",
          "justify-content": collapsed() ? "center" : "space-between",
          padding: collapsed() ? "10px 0" : "10px 10px 8px 12px",
          "flex-shrink": "0",
        }}
      >
        <Show when={!collapsed()}>
          <span
            style={{
              "font-size": "10px",
              "font-weight": "600",
              color: "var(--text-muted)",
              "text-transform": "uppercase",
              "letter-spacing": "0.07em",
            }}
          >
            Projects
          </span>
        </Show>

        <button
          onClick={() => toggleSidebar()}
          title={collapsed() ? "Expand sidebar" : "Collapse sidebar"}
          style={{
            background: "none",
            border: "none",
            padding: "3px",
            cursor: "pointer",
            color: "var(--text-muted)",
            display: "flex",
            "align-items": "center",
            "border-radius": "4px",
          }}
          onMouseEnter={(e) => { (e.currentTarget as HTMLElement).style.color = "var(--text-primary)"; }}
          onMouseLeave={(e) => { (e.currentTarget as HTMLElement).style.color = "var(--text-muted)"; }}
        >
          <Icon name={collapsed() ? "chevron-right" : "chevron-right"} size={14} />
          <span
            style={{
              "font-size": "12px",
              "font-weight": "400",
              transform: collapsed() ? "none" : "rotate(180deg)",
              display: "inline-block",
              transition: "transform 0.18s ease",
              "line-height": "1",
            }}
          >
            ›
          </span>
        </button>
      </div>

      {/* ── Project list ── */}
      <div
        style={{
          flex: "1",
          "overflow-y": "auto",
          "overflow-x": "hidden",
          padding: collapsed() ? "0 8px" : "0 8px",
        }}
      >
        <Show
          when={projectsState.projects.length > 0}
          fallback={
            <Show when={!collapsed()}>
              <div
                style={{
                  "font-size": "11px",
                  color: "var(--text-muted)",
                  "text-align": "center",
                  padding: "20px 8px",
                  "line-height": "1.5",
                }}
              >
                No projects yet.
                <br />
                Add one below.
              </div>
            </Show>
          }
        >
          <For each={projectsState.projects}>
            {(entry) => (
              <ProjectRow
                entry={entry}
                isActive={entry.path === projectState.projectRoot}
                collapsed={collapsed()}
              />
            )}
          </For>
        </Show>
      </div>

      {/* ── Bottom divider + Add Project ── */}
      <div
        style={{
          "border-top": "1px solid var(--border)",
          "flex-shrink": "0",
          padding: collapsed() ? "8px" : "8px",
        }}
      >
        <button
          onClick={() => openProjectFolder().catch(e => { console.error(e); showToast(`Failed to open folder: ${formatError(e)}`, "error"); })}
          title={collapsed() ? "Add Project" : undefined}
          style={{
            width: "100%",
            display: "flex",
            "align-items": "center",
            gap: collapsed() ? "0" : "6px",
            "justify-content": "center",
            background: "none",
            border: "none",
            "border-radius": "6px",
            padding: "7px 6px",
            cursor: "pointer",
            color: "var(--text-secondary)",
            "font-size": "12px",
            transition: "background 0.1s",
          }}
          onMouseEnter={(e) => {
            (e.currentTarget as HTMLElement).style.background = "var(--bg-hover, rgba(255,255,255,0.06))";
            (e.currentTarget as HTMLElement).style.color = "var(--text-primary)";
          }}
          onMouseLeave={(e) => {
            (e.currentTarget as HTMLElement).style.background = "none";
            (e.currentTarget as HTMLElement).style.color = "var(--text-secondary)";
          }}
        >
          <span style={{ "font-size": "16px", "font-weight": "300", "line-height": "1" }}>⊕</span>
          <Show when={!collapsed()}>
            <span>Add Project…</span>
          </Show>
        </button>
      </div>
    </div>
  );
}

export default ProjectSidebar;
