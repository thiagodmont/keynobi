import { type JSX, For, Show, createSignal, onCleanup } from "solid-js";
import {
  searchState,
  setSearchQuery,
  setSearchOption,
  setSearchResults,
  setSearching,
  clearSearchResults,
  setReplaceText,
} from "@/stores/search.store";
import { searchProject, formatError } from "@/lib/tauri-api";
import { projectState } from "@/stores/project.store";
import { showToast } from "@/components/common/Toast";
import { SearchResultItem } from "@/components/search/SearchResult";

export function SearchPanel(): JSX.Element {
  let inputRef!: HTMLInputElement;
  let debounceTimer: ReturnType<typeof setTimeout> | undefined;

  const [collapsedFiles, setCollapsedFiles] = createSignal<Set<string>>(
    new Set()
  );

  let searchSeq = 0;

  onCleanup(() => {
    if (debounceTimer) clearTimeout(debounceTimer);
  });

  function handleInput(value: string) {
    setSearchQuery(value);
    if (debounceTimer) clearTimeout(debounceTimer);
    debounceTimer = setTimeout(() => executeSearch(value), 300);
  }

  async function executeSearch(query?: string) {
    const q = query ?? searchState.query;
    if (!q.trim() || !projectState.projectRoot) {
      clearSearchResults();
      return;
    }

    const thisSeq = ++searchSeq;
    setSearching(true);
    try {
      const results = await searchProject(q, {
        regex: searchState.options.regex,
        caseSensitive: searchState.options.caseSensitive,
        wholeWord: searchState.options.wholeWord,
        includePattern: searchState.options.includePattern,
        excludePattern: searchState.options.excludePattern,
      });
      if (thisSeq !== searchSeq) return;
      setSearchResults(results);
    } catch (err) {
      if (thisSeq !== searchSeq) return;
      showToast(`Search failed: ${formatError(err)}`, "error");
      setSearching(false);
    }
  }

  function toggleCollapsed(path: string) {
    setCollapsedFiles((prev) => {
      const next = new Set(prev);
      if (next.has(path)) next.delete(path);
      else next.add(path);
      return next;
    });
  }

  function toggleOption(opt: "regex" | "caseSensitive" | "wholeWord") {
    setSearchOption(opt, !searchState.options[opt]);
    if (searchState.query.trim()) executeSearch();
  }

  function relativePath(fullPath: string): string {
    const root = projectState.projectRoot;
    if (root && fullPath.startsWith(root)) {
      return fullPath.slice(root.length + 1);
    }
    return fullPath;
  }

  return (
    <div
      style={{
        display: "flex",
        "flex-direction": "column",
        height: "100%",
        "font-size": "var(--font-size-ui)",
      }}
    >
      {/* Header */}
      <div
        style={{
          padding: "8px 8px 4px",
          "border-bottom": "1px solid var(--border)",
          "flex-shrink": "0",
        }}
      >
        <div style={{ "font-size": "11px", "font-weight": "600", "text-transform": "uppercase", "letter-spacing": "0.5px", color: "var(--text-secondary)", "margin-bottom": "6px" }}>
          Search
        </div>

        {/* Search input row */}
        <div style={{ display: "flex", gap: "2px", "margin-bottom": "4px" }}>
          <input
            ref={inputRef}
            type="text"
            placeholder="Search"
            value={searchState.query}
            onInput={(e) => handleInput(e.currentTarget.value)}
            onKeyDown={(e) => {
              if (e.key === "Enter") executeSearch();
              if (e.key === "Escape") {
                clearSearchResults();
                setSearchQuery("");
              }
            }}
            style={{
              flex: "1",
              background: "var(--bg-primary)",
              border: "1px solid var(--border)",
              color: "var(--text-primary)",
              padding: "3px 6px",
              "border-radius": "3px",
              outline: "none",
              "font-size": "12px",
              "font-family": "inherit",
            }}
          />
          <ToggleButton
            active={searchState.options.regex}
            onClick={() => toggleOption("regex")}
            title="Use Regular Expression"
            label=".*"
          />
          <ToggleButton
            active={searchState.options.caseSensitive}
            onClick={() => toggleOption("caseSensitive")}
            title="Match Case"
            label="Aa"
          />
          <ToggleButton
            active={searchState.options.wholeWord}
            onClick={() => toggleOption("wholeWord")}
            title="Match Whole Word"
            label="ab"
          />
        </div>

        {/* Replace row */}
        <Show when={searchState.replaceMode}>
          <div style={{ display: "flex", gap: "2px", "margin-bottom": "4px" }}>
            <input
              type="text"
              placeholder="Replace"
              value={searchState.replaceText}
              onInput={(e) => setReplaceText(e.currentTarget.value)}
              style={{
                flex: "1",
                background: "var(--bg-primary)",
                border: "1px solid var(--border)",
                color: "var(--text-primary)",
                padding: "3px 6px",
                "border-radius": "3px",
                outline: "none",
                "font-size": "12px",
                "font-family": "inherit",
              }}
            />
          </div>
        </Show>

        {/* Result summary */}
        <Show when={searchState.query.trim() && !searchState.searching}>
          <div style={{ "font-size": "11px", color: "var(--text-muted)", padding: "2px 0" }}>
            {searchState.totalMatches} result{searchState.totalMatches !== 1 ? "s" : ""} in {searchState.totalFiles} file{searchState.totalFiles !== 1 ? "s" : ""}
          </div>
        </Show>
        <Show when={searchState.searching}>
          <div style={{ "font-size": "11px", color: "var(--text-muted)", padding: "2px 0" }}>
            Searching...
          </div>
        </Show>
      </div>

      {/* Results */}
      <div style={{ flex: "1", overflow: "auto", "min-height": "0" }}>
        <Show
          when={searchState.results.length > 0}
          fallback={
            <Show when={searchState.query.trim() && !searchState.searching}>
              <div
                style={{
                  padding: "16px",
                  color: "var(--text-muted)",
                  "text-align": "center",
                  "font-size": "12px",
                }}
              >
                No results found
              </div>
            </Show>
          }
        >
          <For each={searchState.results}>
            {(result) => (
              <SearchResultItem
                result={result}
                collapsed={collapsedFiles().has(result.path)}
                onToggle={() => toggleCollapsed(result.path)}
                relativePath={relativePath(result.path)}
              />
            )}
          </For>
        </Show>
      </div>
    </div>
  );
}

function ToggleButton(props: {
  active: boolean;
  onClick: () => void;
  title: string;
  label: string;
}): JSX.Element {
  return (
    <button
      title={props.title}
      onClick={props.onClick}
      style={{
        width: "24px",
        height: "24px",
        display: "flex",
        "align-items": "center",
        "justify-content": "center",
        background: props.active ? "var(--accent)" : "transparent",
        color: props.active ? "#fff" : "var(--text-muted)",
        border: `1px solid ${props.active ? "var(--accent)" : "var(--border)"}`,
        "border-radius": "3px",
        cursor: "pointer",
        "font-size": "10px",
        "font-weight": "600",
        "font-family": "var(--font-mono)",
        "flex-shrink": "0",
      }}
    >
      {props.label}
    </button>
  );
}

export default SearchPanel;
