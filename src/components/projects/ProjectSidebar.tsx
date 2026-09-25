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

import { type JSX, createEffect, createSignal, on, Show } from "solid-js";
import { projectState } from "@/stores/project.store";
import { projectsState } from "@/stores/projects.store";
import { uiState, toggleSidebar } from "@/stores/ui.store";
import { showToast } from "@/components/ui";
import { formatError } from "@/lib/tauri-api";
import {
  selectProject,
  openProjectFolder,
  removeProjectEntry,
  renameProjectEntry,
  trustProject,
  revokeProjectTrust,
} from "@/services/project.service";
import {
  Badge,
  ContextMenu,
  Icon,
  Listbox,
  MenuListItem,
  type ListboxContextMenuRequest,
} from "@/components/ui";
import type { ProjectEntry } from "@/bindings";

// ── Avatar color ──────────────────────────────────────────────────────────────

const AVATAR_COLORS = [
  "#5c7cfa",
  "#339af0",
  "#20c997",
  "#51cf66",
  "#fcc419",
  "#ff6b6b",
  "#cc5de8",
  "#f06595",
  "#74c0fc",
  "#63e6be",
];

function avatarColor(id: string): string {
  let hash = 0;
  for (let i = 0; i < id.length; i++) {
    hash = (hash * 31 + id.charCodeAt(i)) >>> 0;
  }
  return AVATAR_COLORS[hash % AVATAR_COLORS.length];
}

function initials(name: string): string {
  const words = name
    .trim()
    .split(/[\s_-]+/)
    .filter(Boolean);
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
  editing: boolean;
  onStartRename: () => void;
  onEndRename: () => void;
}

function ProjectRow(props: ProjectRowProps): JSX.Element {
  const [hover, setHover] = createSignal(false);
  // eslint-disable-next-line solid/reactivity
  const [editValue, setEditValue] = createSignal(props.entry.name);
  const editing = () => props.editing;

  createEffect(
    on(editing, (isEditing) => {
      if (isEditing) setEditValue(props.entry.name);
    })
  );

  const color = () => avatarColor(props.entry.id);
  const letters = () => initials(props.entry.name);

  function startEdit(e: MouseEvent) {
    e.stopPropagation();
    props.onStartRename();
  }

  function commitEdit() {
    if (!editing()) return;
    const trimmed = editValue().trim();
    if (trimmed && trimmed !== props.entry.name) {
      renameProjectEntry(props.entry.id, trimmed).catch((e) => {
        console.error(e);
        showToast(`Failed to rename project: ${formatError(e)}`, "error");
      });
    }
    props.onEndRename();
  }

  function handleKeyDown(e: KeyboardEvent) {
    if (e.key === "Enter") commitEdit();
    if (e.key === "Escape") {
      // Escape ends the rename only; it must not also close anything behind it.
      e.stopPropagation();
      props.onEndRename();
    }
  }

  return (
    <div
      onMouseEnter={() => setHover(true)}
      onMouseLeave={() => setHover(false)}
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
            <Show when={props.entry.trusted !== true}>
              <Badge
                variant="warning"
                size="xs"
                title="Safe Mode: this project's Gradle build scripts do not run until you trust it (right-click → Trust Project)"
              >
                Safe Mode
              </Badge>
            </Show>
          </div>
        </div>

        {/* Pointer shortcuts; from the keyboard the same actions are in the row's menu. */}
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
              tabIndex={-1}
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
              onMouseEnter={(e) => {
                (e.currentTarget as HTMLElement).style.background = "var(--bg-tertiary)";
              }}
              onMouseLeave={(e) => {
                (e.currentTarget as HTMLElement).style.background = "none";
              }}
            >
              <Icon name="pencil" size={11} />
            </button>
            <Show when={!props.isActive}>
              <button
                onClick={(e) => {
                  e.stopPropagation();
                  removeProjectEntry(props.entry.id).catch((e) => {
                    console.error(e);
                    showToast(`Failed to remove project: ${formatError(e)}`, "error");
                  });
                }}
                tabIndex={-1}
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
                onMouseEnter={(e) => {
                  (e.currentTarget as HTMLElement).style.background = "var(--bg-tertiary)";
                }}
                onMouseLeave={(e) => {
                  (e.currentTarget as HTMLElement).style.background = "none";
                }}
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

const MENU_WIDTH = 176;

export function ProjectSidebar(): JSX.Element {
  const collapsed = () => uiState.sidebarCollapsed;
  const [menu, setMenu] = createSignal<ListboxContextMenuRequest<ProjectEntry> | null>(null);
  const [renamingId, setRenamingId] = createSignal<string | null>(null);
  let listRef!: HTMLDivElement;

  function runFromMenu(action: (entry: ProjectEntry) => Promise<void>): void {
    const current = menu();
    setMenu(null);
    if (current) action(current.item).catch(console.error);
  }

  function startRename(entry: ProjectEntry): void {
    setMenu(null);
    setRenamingId(entry.id);
  }

  function endRename(entry: ProjectEntry): void {
    setRenamingId(null);
    // The rename field held focus; give it back to the project's row.
    const option = Array.from(listRef.querySelectorAll<HTMLElement>('[role="option"]')).find(
      (el) => el.dataset.key === entry.id
    );
    option?.focus();
  }

  function open(entry: ProjectEntry): void {
    if (renamingId() === entry.id) return;
    selectProject(entry).catch((e) => {
      console.error(e);
      showToast(`Failed to open project: ${formatError(e)}`, "error");
    });
  }

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
          onMouseEnter={(e) => {
            (e.currentTarget as HTMLElement).style.color = "var(--text-primary)";
          }}
          onMouseLeave={(e) => {
            (e.currentTarget as HTMLElement).style.color = "var(--text-muted)";
          }}
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
        ref={listRef}
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
          <Listbox
            label="Projects"
            items={projectsState.projects}
            getKey={(entry) => entry.id}
            isSelected={(entry) => entry.path === projectState.projectRoot}
            getOptionLabel={(entry) => (collapsed() ? entry.name : undefined)}
            getOptionTitle={(entry) => (collapsed() ? entry.name : undefined)}
            onSelect={open}
            onContextMenu={setMenu}
          >
            {(entry) => (
              <ProjectRow
                entry={entry()}
                isActive={entry().path === projectState.projectRoot}
                collapsed={collapsed()}
                editing={renamingId() === entry().id}
                onStartRename={() => startRename(entry())}
                onEndRename={() => endRename(entry())}
              />
            )}
          </Listbox>
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
          onClick={() =>
            openProjectFolder().catch((e) => {
              console.error(e);
              showToast(`Failed to open folder: ${formatError(e)}`, "error");
            })
          }
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
            (e.currentTarget as HTMLElement).style.background =
              "var(--bg-hover, rgba(255,255,255,0.06))";
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

      <Show when={menu()}>
        {(open) => (
          <ContextMenu
            x={open().x}
            y={open().y}
            width={MENU_WIDTH}
            label={`Actions for ${open().item.name}`}
            onClose={() => setMenu(null)}
          >
            <MenuListItem role="menuitem" onClick={() => startRename(open().item)}>
              Rename…
            </MenuListItem>
            <Show
              when={open().item.trusted === true}
              fallback={
                <MenuListItem role="menuitem" onClick={() => runFromMenu(trustProject)}>
                  Trust Project
                </MenuListItem>
              }
            >
              <MenuListItem role="menuitem" onClick={() => runFromMenu(revokeProjectTrust)}>
                Revoke Trust
              </MenuListItem>
            </Show>
            <Show when={open().item.path !== projectState.projectRoot}>
              <MenuListItem
                role="menuitem"
                destructive
                onClick={() => runFromMenu((entry) => removeProjectEntry(entry.id))}
              >
                Remove from List
              </MenuListItem>
            </Show>
          </ContextMenu>
        )}
      </Show>
    </div>
  );
}

export default ProjectSidebar;
