import { For, Show, createSignal, type JSX } from "solid-js";
import { showToast } from "@/components/ui";
import {
  addSavedFilter,
  deleteSavedFilter,
  loadFilterStorage,
  MAX_SAVED_FILTERS,
  renameSavedFilter,
  type SavedFilter,
} from "@/lib/logcat-filter-storage";
import { btnStyle } from "./logcat-styles";
import {
  logcatDropdownOverlayStyle,
  logcatDropdownPanelStyle,
  logcatDropdownRootStyle,
  logcatDropdownSectionHeaderStyle,
  logcatDropdownSeparatorStyle,
} from "./logcat-dropdown-styles";

interface LogcatPreset {
  name: string;
  query: string;
  builtin?: true;
}

const BUILTIN_PRESETS: LogcatPreset[] = [
  { name: "My App", query: "package:mine", builtin: true },
  { name: "Crashes", query: "is:crash", builtin: true },
  { name: "Errors+", query: "level:error", builtin: true },
  { name: "Last 5 min", query: "age:5m", builtin: true },
  { name: "My App OR Crashes", query: "package:mine | is:crash", builtin: true },
];

export function SavedFilterMenu(props: {
  query: string;
  isFiltered: boolean;
  onApplyQuery: (query: string) => void;
}): JSX.Element {
  const [open, setOpen] = createSignal(false);
  const [savedFilters, setSavedFilters] = createSignal<SavedFilter[]>(loadFilterStorage().filters);
  const [savingPreset, setSavingPreset] = createSignal(false);
  const [presetNameDraft, setPresetNameDraft] = createSignal("");
  const [renamingId, setRenamingId] = createSignal<string | null>(null);
  const [renameDraft, setRenameDraft] = createSignal("");

  function applyPreset(q: string) {
    props.onApplyQuery(q.trimEnd() + " ");
    setOpen(false);
  }

  function saveCurrentFilter() {
    const name = presetNameDraft().trim();
    if (!name) return;
    try {
      const saved = addSavedFilter(name, props.query);
      setSavedFilters(loadFilterStorage().filters);
      setSavingPreset(false);
      setPresetNameDraft("");
      showToast(`Saved filter "${saved.name}"`, "success");
    } catch (e) {
      showToast(String(e), "error");
    }
  }

  function deleteSavedFilterItem(id: string) {
    deleteSavedFilter(id);
    setSavedFilters(loadFilterStorage().filters);
  }

  function startRename(filter: SavedFilter) {
    setRenamingId(filter.id);
    setRenameDraft(filter.name);
  }

  function commitRename() {
    const id = renamingId();
    if (id) {
      renameSavedFilter(id, renameDraft());
      setSavedFilters(loadFilterStorage().filters);
    }
    setRenamingId(null);
    setRenameDraft("");
  }

  function cancelRename() {
    setRenamingId(null);
    setRenameDraft("");
  }

  return (
    <div style={logcatDropdownRootStyle()}>
      <button
        onClick={() => {
          setOpen((v) => !v);
          setSavingPreset(false);
          setRenamingId(null);
        }}
        title="Filter presets"
        style={btnStyle("var(--text-muted)")}
      >
        ☰ Filters
      </button>
      <Show when={open()}>
        <div
          style={logcatDropdownPanelStyle({
            align: "right",
            minWidth: "260px",
            maxWidth: "340px",
          })}
        >
          <div style={{ padding: "6px 0" }}>
            <div style={logcatDropdownSectionHeaderStyle()}>Quick Filters</div>
            <For each={BUILTIN_PRESETS}>
              {(p) => (
                <div
                  onClick={() => applyPreset(p.query)}
                  style={{
                    display: "flex",
                    "align-items": "center",
                    padding: "5px 10px",
                    cursor: "pointer",
                    color: "var(--text-primary)",
                    "font-family": "var(--font-mono)",
                  }}
                  onMouseEnter={(e) => {
                    (e.currentTarget as HTMLElement).style.background = "rgba(255,255,255,0.06)";
                  }}
                  onMouseLeave={(e) => {
                    (e.currentTarget as HTMLElement).style.background = "transparent";
                  }}
                >
                  <span style={{ flex: "1" }}>{p.name}</span>
                  <span
                    style={{
                      color: "var(--text-muted)",
                      "font-size": "10px",
                      "max-width": "130px",
                      overflow: "hidden",
                      "text-overflow": "ellipsis",
                      "white-space": "nowrap",
                    }}
                  >
                    {p.query}
                  </span>
                </div>
              )}
            </For>

            <div style={logcatDropdownSeparatorStyle()} />
            <div style={{ display: "flex", "align-items": "center", padding: "2px 10px 4px" }}>
              <span
                style={{
                  "font-size": "10px",
                  color: "var(--text-muted)",
                  "text-transform": "uppercase",
                  "letter-spacing": "0.05em",
                  flex: "1",
                }}
              >
                Saved
              </span>
              <span style={{ "font-size": "10px", color: "var(--text-muted)" }}>
                {savedFilters().length} / {MAX_SAVED_FILTERS}
              </span>
            </div>

            <Show when={savedFilters().length === 0}>
              <div
                style={{
                  padding: "4px 10px 6px",
                  "font-size": "10px",
                  color: "var(--text-muted)",
                  "font-style": "italic",
                }}
              >
                No saved filters yet
              </div>
            </Show>

            <For each={savedFilters()}>
              {(f) => (
                <div
                  style={{
                    display: "flex",
                    "align-items": "center",
                    padding: "4px 10px",
                    cursor: "pointer",
                    color: "var(--text-primary)",
                    gap: "4px",
                  }}
                  onMouseEnter={(e) => {
                    (e.currentTarget as HTMLElement).style.background = "rgba(255,255,255,0.06)";
                  }}
                  onMouseLeave={(e) => {
                    (e.currentTarget as HTMLElement).style.background = "transparent";
                  }}
                >
                  <Show
                    when={renamingId() === f.id}
                    fallback={
                      <>
                        <span
                          onClick={() => applyPreset(f.query)}
                          style={{
                            flex: "1",
                            overflow: "hidden",
                            "text-overflow": "ellipsis",
                            "white-space": "nowrap",
                          }}
                          title={f.query}
                        >
                          {f.name}
                        </span>
                        <button
                          onClick={(e) => {
                            e.stopPropagation();
                            startRename(f);
                          }}
                          style={{
                            background: "none",
                            border: "none",
                            color: "var(--text-muted)",
                            cursor: "pointer",
                            padding: "0 3px",
                            "font-size": "10px",
                          }}
                          title="Rename"
                        >
                          ✎
                        </button>
                        <button
                          onClick={(e) => {
                            e.stopPropagation();
                            deleteSavedFilterItem(f.id);
                          }}
                          style={{
                            background: "none",
                            border: "none",
                            color: "var(--text-muted)",
                            cursor: "pointer",
                            padding: "0 3px",
                            "font-size": "10px",
                          }}
                          title="Delete"
                        >
                          ✕
                        </button>
                      </>
                    }
                  >
                    <input
                      type="text"
                      value={renameDraft()}
                      onInput={(e) => setRenameDraft(e.currentTarget.value)}
                      onKeyDown={(e) => {
                        if (e.key === "Enter") {
                          e.stopPropagation();
                          commitRename();
                        }
                        if (e.key === "Escape") {
                          e.stopPropagation();
                          cancelRename();
                        }
                      }}
                      autofocus
                      style={{
                        flex: "1",
                        background: "var(--bg-primary)",
                        border: "1px solid var(--accent)",
                        color: "var(--text-primary)",
                        "border-radius": "3px",
                        padding: "2px 5px",
                        "font-size": "11px",
                        outline: "none",
                      }}
                    />
                    <button
                      onClick={(e) => {
                        e.stopPropagation();
                        commitRename();
                      }}
                      style={btnStyle("var(--accent)")}
                    >
                      ✓
                    </button>
                    <button
                      onClick={(e) => {
                        e.stopPropagation();
                        cancelRename();
                      }}
                      style={btnStyle("var(--text-muted)")}
                    >
                      ✕
                    </button>
                  </Show>
                </div>
              )}
            </For>

            <div style={logcatDropdownSeparatorStyle()} />
            <Show
              when={savingPreset()}
              fallback={
                <div
                  onClick={() => {
                    setSavingPreset(true);
                    setPresetNameDraft("");
                  }}
                  style={{
                    padding: "5px 10px",
                    cursor: "pointer",
                    color: props.isFiltered ? "var(--accent)" : "var(--text-muted)",
                  }}
                  onMouseEnter={(e) => {
                    (e.currentTarget as HTMLElement).style.background = "rgba(255,255,255,0.06)";
                  }}
                  onMouseLeave={(e) => {
                    (e.currentTarget as HTMLElement).style.background = "transparent";
                  }}
                >
                  + Save current filter
                </div>
              }
            >
              <div style={{ display: "flex", gap: "4px", padding: "4px 8px" }}>
                <input
                  type="text"
                  placeholder="Filter name…"
                  value={presetNameDraft()}
                  onInput={(e) => setPresetNameDraft(e.currentTarget.value)}
                  onKeyDown={(e) => {
                    if (e.key === "Enter") saveCurrentFilter();
                    if (e.key === "Escape") setSavingPreset(false);
                  }}
                  autofocus
                  style={{
                    flex: "1",
                    background: "var(--bg-primary)",
                    border: "1px solid var(--border)",
                    color: "var(--text-primary)",
                    "border-radius": "3px",
                    padding: "3px 6px",
                    "font-size": "11px",
                    outline: "none",
                  }}
                />
                <button onClick={saveCurrentFilter} style={btnStyle("var(--accent)")}>
                  Save
                </button>
              </div>
            </Show>
          </div>
        </div>
        <div
          style={logcatDropdownOverlayStyle()}
          onClick={() => {
            setOpen(false);
            cancelRename();
          }}
        />
      </Show>
    </div>
  );
}
