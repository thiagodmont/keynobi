import { For, Show, createMemo, createSignal, type JSX } from "solid-js";
import { showToast } from "@/components/ui";
import { QueryBar } from "@/components/logcat/QueryBar";
import { PackageDropdown } from "@/components/logcat/PackageDropdown";
import {
  addSavedFilter,
  deleteSavedFilter,
  loadFilterStorage,
  renameSavedFilter,
} from "@/lib/logcat-filter-storage";
import {
  buildEffectiveQueryWithDisabledPills,
  extractQueryVariables,
  isValidQueryVariableName,
  type QueryVariableValues,
} from "@/lib/logcat-query";
import { btnStyle } from "./logcat-styles";
import { SavedFilterMenu } from "./SavedFilterMenu";

export const LOGCAT_AGE_PILLS = [
  { label: "30s", value: "30s" },
  { label: "1m", value: "1m" },
  { label: "5m", value: "5m" },
  { label: "15m", value: "15m" },
  { label: "1h", value: "1h" },
  { label: "All", value: null },
] as const;

export type LogcatAgePillValue = (typeof LOGCAT_AGE_PILLS)[number]["value"];

export function LogcatFilterControls(props: {
  query: string;
  knownTags: string[];
  knownPackages: string[];
  hasAgeFilter: boolean;
  activeAge: string | null;
  activePackage: string | null;
  isFiltered: boolean;
  disabledPillIds?: ReadonlySet<string>;
  variableValues?: QueryVariableValues;
  showLifecycle: boolean;
  onQueryChange: (query: string) => void;
  onTogglePillDisabled?: (id: string) => void;
  onVariableValueChange?: (name: string, value: string) => void;
  onVariableDelete?: (name: string) => void;
  onVariableInsert?: (name: string) => void;
  onAgeSelect: (value: LogcatAgePillValue) => void;
  onPackageSelect: (pkg: string | null) => void;
  onToggleLifecycle: () => void;
  onApplySavedQuery?: (query: string) => void;
  onClear: () => void;
}): JSX.Element {
  const [savedFilters, setSavedFilters] = createSignal(loadFilterStorage().filters);

  function refreshSavedFilters(): void {
    setSavedFilters(loadFilterStorage().filters);
  }

  function saveCurrentFilter(name: string): void {
    try {
      const effectiveQuery = buildEffectiveQueryWithDisabledPills(
        props.query,
        props.disabledPillIds ?? new Set<string>()
      );
      const saved = addSavedFilter(name, effectiveQuery);
      refreshSavedFilters();
      showToast(`Saved filter "${saved.name}"`, "success");
    } catch (e) {
      showToast(String(e), "error");
    }
  }

  function handleDeleteSavedFilter(id: string): void {
    deleteSavedFilter(id);
    refreshSavedFilters();
  }

  function handleRenameSavedFilter(id: string, name: string): void {
    renameSavedFilter(id, name);
    refreshSavedFilters();
  }

  return (
    <div
      style={{
        display: "flex",
        "flex-direction": "column",
        gap: "6px",
        padding: "6px 10px",
        background: "var(--bg-secondary)",
        "border-bottom": "1px solid var(--border)",
        "flex-shrink": "0",
      }}
    >
      <div
        data-testid="logcat-filter-query-row"
        style={{
          display: "flex",
          "align-items": "flex-start",
          gap: "6px",
          width: "100%",
          "min-width": "0",
        }}
      >
        <QueryBar
          value={props.query}
          onChange={props.onQueryChange}
          knownTags={props.knownTags}
          knownPackages={props.knownPackages}
          disabledPillIds={props.disabledPillIds}
          onTogglePillDisabled={props.onTogglePillDisabled}
        />
        <DirectSaveFilterButton
          query={props.query}
          isFiltered={props.isFiltered}
          onSave={saveCurrentFilter}
        />
        <VariableManagerButton
          query={props.query}
          values={props.variableValues ?? {}}
          onChange={props.onVariableValueChange}
          onDelete={props.onVariableDelete}
          onInsert={props.onVariableInsert}
        />
      </div>

      <QueryVariableControls
        query={props.query}
        values={props.variableValues ?? {}}
        onChange={props.onVariableValueChange}
      />

      <div
        data-testid="logcat-filter-quick-row"
        style={{
          display: "flex",
          "align-items": "center",
          gap: "5px",
          "flex-wrap": "wrap",
          "min-width": "0",
        }}
      >
        <SavedFilterMenu
          savedFilters={savedFilters()}
          onApplyQuery={props.onApplySavedQuery ?? props.onQueryChange}
          onDeleteSavedFilter={handleDeleteSavedFilter}
          onRenameSavedFilter={handleRenameSavedFilter}
        />

        <div
          style={{
            width: "1px",
            height: "14px",
            background: "var(--border)",
            "flex-shrink": "0",
            "margin-right": "2px",
          }}
        />

        <span
          style={{
            "font-size": "10px",
            color: "var(--text-muted)",
            "flex-shrink": "0",
          }}
        >
          Age
        </span>
        <For each={LOGCAT_AGE_PILLS}>
          {(pill) => {
            const isActive = () =>
              pill.value === null ? !props.hasAgeFilter : props.activeAge === pill.value;
            return (
              <button
                onClick={() => props.onAgeSelect(pill.value)}
                style={{
                  padding: "1px 7px",
                  "font-size": "10px",
                  background: isActive() ? "var(--accent)" : "var(--bg-primary)",
                  color: isActive() ? "#fff" : "var(--text-muted)",
                  border: `1px solid ${isActive() ? "var(--accent)" : "var(--border)"}`,
                  "border-radius": "10px",
                  cursor: "pointer",
                  "flex-shrink": "0",
                  transition: "all 0.1s",
                }}
              >
                {pill.label}
              </button>
            );
          }}
        </For>

        <button
          onClick={() => props.onToggleLifecycle()}
          aria-pressed={props.showLifecycle}
          title={
            props.showLifecycle
              ? "Hide lifecycle and process logs"
              : "Show lifecycle and process logs"
          }
          style={{
            padding: "1px 7px",
            "font-size": "10px",
            background: props.showLifecycle ? "var(--accent)" : "var(--bg-primary)",
            color: props.showLifecycle ? "#fff" : "var(--text-muted)",
            border: `1px solid ${props.showLifecycle ? "var(--accent)" : "var(--border)"}`,
            "border-radius": "10px",
            cursor: "pointer",
            "flex-shrink": "0",
            transition: "all 0.1s",
          }}
        >
          Lifecycle
        </button>

        <div
          style={{
            width: "1px",
            height: "14px",
            background: "var(--border)",
            "flex-shrink": "0",
            "margin-left": "2px",
          }}
        />

        <PackageDropdown
          packages={props.knownPackages}
          selected={props.activePackage}
          onSelect={props.onPackageSelect}
        />

        <Show when={props.isFiltered}>
          <button
            onClick={() => props.onClear()}
            title="Clear all filters"
            style={{
              ...btnStyle("var(--text-muted)"),
              "font-size": "10px",
              padding: "1px 7px",
            }}
          >
            ✕ Clear
          </button>
        </Show>
      </div>
    </div>
  );
}

function getVariableNames(query: string, values: QueryVariableValues): string[] {
  const seen = new Set<string>();
  const names: string[] = [];

  for (const name of Object.keys(values)) {
    if (!seen.has(name)) {
      seen.add(name);
      names.push(name);
    }
  }

  for (const name of extractQueryVariables(query)) {
    if (!seen.has(name)) {
      seen.add(name);
      names.push(name);
    }
  }

  return names;
}

function QueryVariableControls(props: {
  query: string;
  values: QueryVariableValues;
  onChange?: (name: string, value: string) => void;
}): JSX.Element {
  const variables = createMemo(() => getVariableNames(props.query, props.values));

  return (
    <Show when={variables().length > 0}>
      <div
        data-testid="logcat-filter-variable-row"
        style={{
          display: "flex",
          "align-items": "center",
          gap: "6px",
          "flex-wrap": "wrap",
          "min-width": "0",
          padding: "1px 0",
        }}
      >
        <span
          style={{
            "font-size": "10px",
            color: "var(--text-muted)",
            "flex-shrink": "0",
          }}
        >
          Variables
        </span>
        <For each={variables()}>
          {(name) => (
            <label
              style={{
                display: "inline-flex",
                "align-items": "center",
                gap: "4px",
                "min-width": "0",
                "flex-shrink": "0",
                background: "var(--bg-primary)",
                border: "1px solid var(--border)",
                "border-radius": "4px",
                padding: "2px 5px",
              }}
            >
              <span
                style={{
                  "font-size": "10px",
                  "font-family": "var(--font-mono)",
                  color: "var(--accent)",
                  "white-space": "nowrap",
                }}
              >
                {name}
              </span>
              <input
                title={`Variable ${name}`}
                type="text"
                value={props.values[name] ?? ""}
                placeholder="value"
                spellcheck={false}
                onInput={(e) => props.onChange?.(name, e.currentTarget.value)}
                style={{
                  width: "120px",
                  "min-width": "72px",
                  background: "transparent",
                  border: "0",
                  color: "var(--text-primary)",
                  "font-size": "10px",
                  "font-family": "var(--font-mono)",
                  outline: "none",
                  padding: "0",
                }}
              />
            </label>
          )}
        </For>
      </div>
    </Show>
  );
}

function VariableManagerButton(props: {
  query: string;
  values: QueryVariableValues;
  onChange?: (name: string, value: string) => void;
  onDelete?: (name: string) => void;
  onInsert?: (name: string) => void;
}): JSX.Element {
  const [open, setOpen] = createSignal(false);
  const [draftName, setDraftName] = createSignal("");
  const [draftValue, setDraftValue] = createSignal("");
  const variables = createMemo(() => getVariableNames(props.query, props.values));
  const queryVariables = createMemo(() => new Set(extractQueryVariables(props.query)));
  const normalizedName = createMemo(() => draftName().trim());
  const canAdd = createMemo(() => isValidQueryVariableName(normalizedName()));

  function close(): void {
    setOpen(false);
    setDraftName("");
    setDraftValue("");
  }

  function addVariable(): void {
    const name = normalizedName();
    if (!isValidQueryVariableName(name)) return;
    props.onChange?.(name, draftValue());
    setDraftName("");
    setDraftValue("");
  }

  return (
    <div style={{ position: "relative", "flex-shrink": "0" }}>
      <button
        type="button"
        title="Manage filter variables"
        onClick={() => setOpen((value) => !value)}
        style={{
          ...btnStyle("var(--text-muted)"),
          height: "28px",
          padding: "0 10px",
          "font-size": "11px",
          "font-weight": "600",
        }}
      >
        Variables
      </button>

      <Show when={open()}>
        <>
          <div
            data-testid="logcat-variable-manager"
            style={{
              position: "absolute",
              top: "32px",
              right: "0",
              "z-index": "1000",
              display: "flex",
              "flex-direction": "column",
              gap: "8px",
              padding: "8px",
              background: "var(--bg-secondary)",
              border: "1px solid var(--border)",
              "border-radius": "6px",
              "box-shadow": "0 8px 24px rgba(0,0,0,0.35)",
              width: "360px",
            }}
          >
            <div
              style={{
                display: "grid",
                "grid-template-columns": "minmax(0, 1fr) minmax(0, 1fr) auto",
                gap: "6px",
                "align-items": "center",
              }}
            >
              <input
                type="text"
                placeholder="variable_name"
                value={draftName()}
                onInput={(e) => setDraftName(e.currentTarget.value)}
                onKeyDown={(e) => {
                  if (e.key === "Enter") addVariable();
                  if (e.key === "Escape") close();
                }}
                style={variableManagerInputStyle()}
              />
              <input
                type="text"
                placeholder="Initial value"
                value={draftValue()}
                onInput={(e) => setDraftValue(e.currentTarget.value)}
                onKeyDown={(e) => {
                  if (e.key === "Enter") addVariable();
                  if (e.key === "Escape") close();
                }}
                style={variableManagerInputStyle()}
              />
              <button
                type="button"
                disabled={!canAdd()}
                onClick={addVariable}
                style={{
                  ...btnStyle(canAdd() ? "var(--accent)" : "var(--text-muted)"),
                  opacity: canAdd() ? "1" : "0.45",
                  cursor: canAdd() ? "pointer" : "not-allowed",
                }}
              >
                Add variable
              </button>
            </div>

            <Show
              when={variables().length > 0}
              fallback={
                <div
                  style={{
                    color: "var(--text-muted)",
                    "font-size": "11px",
                    padding: "4px 2px",
                  }}
                >
                  No variables
                </div>
              }
            >
              <div
                style={{
                  display: "flex",
                  "flex-direction": "column",
                  gap: "5px",
                  "max-height": "220px",
                  overflow: "auto",
                }}
              >
                <For each={variables()}>
                  {(name) => (
                    <div
                      style={{
                        display: "grid",
                        "grid-template-columns": "minmax(80px, 0.8fr) minmax(0, 1fr) auto auto",
                        gap: "6px",
                        "align-items": "center",
                      }}
                    >
                      <div
                        style={{
                          display: "flex",
                          "align-items": "center",
                          gap: "4px",
                          "min-width": "0",
                        }}
                      >
                        <span
                          style={{
                            "font-size": "11px",
                            "font-family": "var(--font-mono)",
                            color: "var(--accent)",
                            overflow: "hidden",
                            "text-overflow": "ellipsis",
                            "white-space": "nowrap",
                          }}
                          title={`\${${name}}`}
                        >
                          {name}
                        </span>
                        <Show when={queryVariables().has(name)}>
                          <span
                            style={{
                              "font-size": "9px",
                              color: "var(--text-muted)",
                              border: "1px solid var(--border)",
                              "border-radius": "8px",
                              padding: "0 4px",
                              "flex-shrink": "0",
                            }}
                          >
                            used
                          </span>
                        </Show>
                      </div>
                      <input
                        type="text"
                        title={`Variable ${name}`}
                        value={props.values[name] ?? ""}
                        placeholder="value"
                        onInput={(e) => props.onChange?.(name, e.currentTarget.value)}
                        style={variableManagerInputStyle()}
                      />
                      <button
                        type="button"
                        title={`Insert variable ${name}`}
                        onClick={() => props.onInsert?.(name)}
                        style={btnStyle("var(--accent)")}
                      >
                        Insert
                      </button>
                      <button
                        type="button"
                        title={`Delete variable ${name}`}
                        onClick={() => props.onDelete?.(name)}
                        style={btnStyle("var(--text-muted)")}
                      >
                        Delete
                      </button>
                    </div>
                  )}
                </For>
              </div>
            </Show>
          </div>
          <div
            onClick={close}
            style={{
              position: "fixed",
              inset: "0",
              "z-index": "999",
            }}
          />
        </>
      </Show>
    </div>
  );
}

function variableManagerInputStyle(): JSX.CSSProperties {
  return {
    background: "var(--bg-primary)",
    border: "1px solid var(--border)",
    color: "var(--text-primary)",
    "border-radius": "4px",
    padding: "4px 7px",
    "font-size": "11px",
    "font-family": "var(--font-mono)",
    outline: "none",
    "min-width": "0",
  };
}

function DirectSaveFilterButton(props: {
  query: string;
  isFiltered: boolean;
  onSave: (name: string) => void;
}): JSX.Element {
  const [open, setOpen] = createSignal(false);
  const [draft, setDraft] = createSignal("");

  function close(): void {
    setOpen(false);
    setDraft("");
  }

  function save(): void {
    const name = draft().trim();
    if (!name) return;
    props.onSave(name);
    close();
  }

  return (
    <Show when={props.isFiltered}>
      <div style={{ position: "relative", "flex-shrink": "0" }}>
        <button
          type="button"
          title="Save current filter"
          onClick={() => {
            setOpen((v) => !v);
            setDraft("");
          }}
          style={{
            ...btnStyle("var(--accent)"),
            height: "28px",
            padding: "0 10px",
            "font-size": "11px",
            "font-weight": "600",
          }}
        >
          Save
        </button>

        <Show when={open()}>
          <>
            <div
              style={{
                position: "absolute",
                top: "32px",
                right: "0",
                "z-index": "1000",
                display: "flex",
                gap: "6px",
                padding: "8px",
                background: "var(--bg-secondary)",
                border: "1px solid var(--border)",
                "border-radius": "6px",
                "box-shadow": "0 8px 24px rgba(0,0,0,0.35)",
                "min-width": "260px",
              }}
            >
              <input
                type="text"
                placeholder="Filter name…"
                value={draft()}
                onInput={(e) => setDraft(e.currentTarget.value)}
                onKeyDown={(e) => {
                  if (e.key === "Enter") save();
                  if (e.key === "Escape") close();
                }}
                autofocus
                style={{
                  flex: "1",
                  background: "var(--bg-primary)",
                  border: "1px solid var(--border)",
                  color: "var(--text-primary)",
                  "border-radius": "4px",
                  padding: "4px 7px",
                  "font-size": "11px",
                  outline: "none",
                }}
              />
              <button type="button" onClick={save} style={btnStyle("var(--accent)")}>
                Save
              </button>
              <button type="button" onClick={close} style={btnStyle("var(--text-muted)")}>
                Cancel
              </button>
            </div>
            <div
              onClick={close}
              style={{
                position: "fixed",
                inset: "0",
                "z-index": "999",
              }}
            />
          </>
        </Show>
      </div>
    </Show>
  );
}
