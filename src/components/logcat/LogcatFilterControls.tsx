import { For, Show, createMemo, createSignal, type JSX } from "solid-js";
import {
  Badge,
  Button,
  ControlStrip,
  FilterChip,
  Input,
  Popover,
  Separator,
  showToast,
} from "@/components/ui";
import { QueryBar } from "@/components/logcat/QueryBar";
import { PackageDropdown } from "@/components/logcat/PackageDropdown";
import {
  addSavedFilter,
  deleteSavedFilter,
  loadFilterStorage,
  renameSavedFilter,
} from "@/lib/logcat-filter-storage";
import { buildEffectiveQueryWithDisabledPills } from "@/lib/logcat-query";
import {
  extractQueryVariables,
  isValidQueryVariableName,
  type QueryVariableValues,
} from "@/lib/logcat-query-variables";
import { SavedFilterMenu } from "./SavedFilterMenu";
import styles from "./LogcatFilterControls.module.css";

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
    <ControlStrip direction="column" class={styles.root}>
      <div data-testid="logcat-filter-query-row" class={styles.queryRow}>
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

      <div data-testid="logcat-filter-quick-row" class={styles.quickRow}>
        <SavedFilterMenu
          savedFilters={savedFilters()}
          onApplyQuery={props.onApplySavedQuery ?? props.onQueryChange}
          onDeleteSavedFilter={handleDeleteSavedFilter}
          onRenameSavedFilter={handleRenameSavedFilter}
        />

        <Separator orientation="vertical" class={styles.ageSeparator} />

        <span class={styles.rowLabel}>Age</span>
        <For each={LOGCAT_AGE_PILLS}>
          {(pill) => {
            const isActive = () =>
              pill.value === null ? !props.hasAgeFilter : props.activeAge === pill.value;
            return (
              <FilterChip active={isActive()} onClick={() => props.onAgeSelect(pill.value)}>
                {pill.label}
              </FilterChip>
            );
          }}
        </For>

        <FilterChip
          active={props.showLifecycle}
          onClick={() => props.onToggleLifecycle()}
          ariaPressed={props.showLifecycle}
          title={
            props.showLifecycle
              ? "Hide lifecycle and process logs"
              : "Show lifecycle and process logs"
          }
        >
          Lifecycle
        </FilterChip>

        <Separator orientation="vertical" class={styles.packageSeparator} />

        <PackageDropdown
          packages={props.knownPackages}
          selected={props.activePackage}
          onSelect={props.onPackageSelect}
        />

        <Show when={props.isFiltered}>
          <Button
            variant="outline"
            size="xs"
            tone="muted"
            onClick={() => props.onClear()}
            title="Clear all filters"
          >
            ✕ Clear
          </Button>
        </Show>
      </div>
    </ControlStrip>
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
      <div data-testid="logcat-filter-variable-row" class={styles.variableRow}>
        <span class={styles.rowLabel}>Variables</span>
        <For each={variables()}>
          {(name) => (
            <label class={styles.variableToken}>
              <span class={styles.variableName}>{name}</span>
              <Input
                title={`Variable ${name}`}
                type="text"
                value={props.values[name] ?? ""}
                placeholder="value"
                size="xs"
                mono
                spellcheck={false}
                onInput={(value) => props.onChange?.(name, value)}
                class={styles.variableValueInput}
                inputClass={styles.variableValueInputControl}
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
    <Popover
      open={open()}
      onOpenChange={(nextOpen) => {
        if (nextOpen) setOpen(true);
        else close();
      }}
      align="right"
      panelClass={styles.variableManagerPanel}
      trigger={() => (
        <Button
          variant="outline"
          size="xs"
          tone="muted"
          title="Manage filter variables"
          onClick={() => {
            if (open()) close();
            else setOpen(true);
          }}
          class={styles.strongButton}
        >
          Variables
        </Button>
      )}
    >
      <div data-testid="logcat-variable-manager" class={styles.variableManager}>
        <div class={styles.addVariableGrid}>
          <Input
            type="text"
            placeholder="variable_name"
            value={draftName()}
            size="sm"
            mono
            onInput={setDraftName}
            onKeyDown={(e) => {
              if (e.key === "Enter") addVariable();
              if (e.key === "Escape") close();
            }}
          />
          <Input
            type="text"
            placeholder="Initial value"
            value={draftValue()}
            size="sm"
            mono
            onInput={setDraftValue}
            onKeyDown={(e) => {
              if (e.key === "Enter") addVariable();
              if (e.key === "Escape") close();
            }}
          />
          <Button
            variant="outline"
            size="xs"
            tone={canAdd() ? "accent" : "muted"}
            disabled={!canAdd()}
            onClick={addVariable}
            class={styles.addVariableButton}
          >
            Add variable
          </Button>
        </div>

        <Show
          when={variables().length > 0}
          fallback={<div class={styles.emptyVariables}>No variables</div>}
        >
          <div class={styles.variableList}>
            <For each={variables()}>
              {(name) => (
                <div class={styles.variableListItem}>
                  <div class={styles.variableListName}>
                    <span class={styles.variableListNameText} title={`\${${name}}`}>
                      {name}
                    </span>
                    <Show when={queryVariables().has(name)}>
                      <Badge size="xs" variant="default">
                        used
                      </Badge>
                    </Show>
                  </div>
                  <Input
                    type="text"
                    title={`Variable ${name}`}
                    value={props.values[name] ?? ""}
                    placeholder="value"
                    size="sm"
                    mono
                    onInput={(value) => props.onChange?.(name, value)}
                  />
                  <Button
                    variant="outline"
                    size="xs"
                    tone="accent"
                    title={`Insert variable ${name}`}
                    onClick={() => props.onInsert?.(name)}
                  >
                    Insert
                  </Button>
                  <Button
                    variant="outline"
                    size="xs"
                    tone="muted"
                    title={`Delete variable ${name}`}
                    onClick={() => props.onDelete?.(name)}
                  >
                    Delete
                  </Button>
                </div>
              )}
            </For>
          </div>
        </Show>
      </div>
    </Popover>
  );
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
      <Popover
        open={open()}
        onOpenChange={(nextOpen) => {
          if (nextOpen) setOpen(true);
          else close();
        }}
        align="right"
        minWidth="260px"
        panelClass={styles.savePanel}
        trigger={() => (
          <Button
            variant="outline"
            size="xs"
            tone="accent"
            title="Save current filter"
            onClick={() => {
              setDraft("");
              if (open()) close();
              else setOpen(true);
            }}
            class={styles.strongButton}
          >
            Save
          </Button>
        )}
      >
        <Input
          type="text"
          placeholder="Filter name…"
          value={draft()}
          size="sm"
          onInput={setDraft}
          onKeyDown={(e) => {
            if (e.key === "Enter") save();
            if (e.key === "Escape") close();
          }}
          autofocus
          class={styles.saveInput}
        />
        <Button variant="outline" size="xs" tone="accent" onClick={save}>
          Save
        </Button>
        <Button variant="outline" size="xs" tone="muted" onClick={close}>
          Cancel
        </Button>
      </Popover>
    </Show>
  );
}
