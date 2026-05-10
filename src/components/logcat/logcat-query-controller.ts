import { type Accessor, createSignal, onCleanup } from "solid-js";
import { ensureQueryVariableValues, type QueryVariableValues } from "@/lib/logcat-query-variables";
import { reconcileDisabledQueryPillIds } from "@/lib/logcat-query";

interface LogcatQueryControllerOptions {
  onQueryInteractionReset: () => void;
  debounceMs?: number;
}

interface RestoreQueryOptions {
  syncDisabledPills?: boolean;
}

export interface LogcatQueryController {
  query: Accessor<string>;
  debouncedQuery: Accessor<string>;
  disabledPillIds: Accessor<Set<string>>;
  queryVariableValues: Accessor<QueryVariableValues>;
  debouncedQueryVariableValues: Accessor<QueryVariableValues>;
  updateQuery(query: string): void;
  updateQueryVariableValue(name: string, value: string): void;
  deleteQueryVariable(name: string): void;
  insertQueryVariable(name: string): void;
  applyPresetQuery(query: string): void;
  togglePillDisabled(id: string): void;
  clearQuery(): void;
  restoreQuery(query: string, options?: RestoreQueryOptions): void;
}

export function createLogcatQueryController(
  options: LogcatQueryControllerOptions
): LogcatQueryController {
  const debounceMs = options.debounceMs ?? 150;
  const [query, setQuery] = createSignal("");
  const [debouncedQuery, setDebouncedQuery] = createSignal("");
  const [disabledPillIds, setDisabledPillIds] = createSignal<Set<string>>(new Set(), {
    equals: false,
  });
  const [queryVariableValues, setQueryVariableValues] = createSignal<QueryVariableValues>({});
  const [debouncedQueryVariableValues, setDebouncedQueryVariableValues] =
    createSignal<QueryVariableValues>({});
  let queryDebounce: ReturnType<typeof setTimeout> | undefined;
  let variableDebounce: ReturnType<typeof setTimeout> | undefined;

  function resetInteraction(): void {
    options.onQueryInteractionReset();
  }

  function syncQueryVariableNames(nextQuery: string): void {
    setQueryVariableValues((values) => ensureQueryVariableValues(nextQuery, values));
    setDebouncedQueryVariableValues((values) => ensureQueryVariableValues(nextQuery, values));
  }

  function scheduleDebouncedQueryVariableValues(values: QueryVariableValues): void {
    clearTimeout(variableDebounce);
    variableDebounce = setTimeout(() => {
      setDebouncedQueryVariableValues(ensureQueryVariableValues(query(), values));
    }, debounceMs);
  }

  function updateQuery(nextQuery: string): void {
    resetInteraction();
    setQuery(nextQuery);
    syncQueryVariableNames(nextQuery);
    setDisabledPillIds((ids) => reconcileDisabledQueryPillIds(nextQuery, ids));
    clearTimeout(queryDebounce);
    queryDebounce = setTimeout(() => setDebouncedQuery(nextQuery), debounceMs);
  }

  function updateQueryVariableValue(name: string, value: string): void {
    resetInteraction();
    const nextValues = ensureQueryVariableValues(query(), {
      ...queryVariableValues(),
      [name]: value,
    });
    setQueryVariableValues(nextValues);
    scheduleDebouncedQueryVariableValues(nextValues);
  }

  function deleteQueryVariable(name: string): void {
    const next = { ...queryVariableValues() };
    delete next[name];
    const nextValues = ensureQueryVariableValues(query(), next);
    setQueryVariableValues(nextValues);
    scheduleDebouncedQueryVariableValues(nextValues);
  }

  function insertQueryVariable(name: string): void {
    updateQuery(`${query()}\${${name}}`);
  }

  function applyPresetQuery(nextQuery: string): void {
    setDisabledPillIds(new Set<string>());
    updateQuery(nextQuery);
  }

  function togglePillDisabled(id: string): void {
    resetInteraction();
    setDisabledPillIds((ids) => {
      const next = new Set(ids);
      if (next.has(id)) {
        next.delete(id);
      } else {
        next.add(id);
      }
      return reconcileDisabledQueryPillIds(query(), next);
    });
  }

  function clearQuery(): void {
    setDisabledPillIds(new Set<string>());
    updateQuery("");
  }

  function restoreQuery(nextQuery: string, options?: RestoreQueryOptions): void {
    clearTimeout(queryDebounce);
    clearTimeout(variableDebounce);
    setQuery(nextQuery);
    syncQueryVariableNames(nextQuery);
    setDebouncedQuery(nextQuery);
    if (options?.syncDisabledPills) {
      setDisabledPillIds((ids) => reconcileDisabledQueryPillIds(nextQuery, ids));
    }
  }

  onCleanup(() => {
    clearTimeout(queryDebounce);
    clearTimeout(variableDebounce);
  });

  return {
    query,
    debouncedQuery,
    disabledPillIds,
    queryVariableValues,
    debouncedQueryVariableValues,
    updateQuery,
    updateQueryVariableValue,
    deleteQueryVariable,
    insertQueryVariable,
    applyPresetQuery,
    togglePillDisabled,
    clearQuery,
    restoreQuery,
  };
}
