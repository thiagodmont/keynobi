import { createStore, produce } from "solid-js/store";

export interface SearchOptions {
  regex: boolean;
  caseSensitive: boolean;
  wholeWord: boolean;
  includePattern: string | null;
  excludePattern: string | null;
}

export interface SearchMatch {
  line: number;
  col: number;
  endCol: number;
  lineContent: string;
  contextBefore: string[];
  contextAfter: string[];
}

export interface SearchResult {
  path: string;
  matches: SearchMatch[];
}

interface SearchStoreState {
  query: string;
  results: SearchResult[];
  searching: boolean;
  options: SearchOptions;
  replaceText: string;
  replaceMode: boolean;
  totalMatches: number;
  totalFiles: number;
}

const [searchState, setSearchState] = createStore<SearchStoreState>({
  query: "",
  results: [],
  searching: false,
  options: {
    regex: false,
    caseSensitive: false,
    wholeWord: false,
    includePattern: null,
    excludePattern: null,
  },
  replaceText: "",
  replaceMode: false,
  totalMatches: 0,
  totalFiles: 0,
});

export { searchState, setSearchState };

export function setSearchQuery(query: string) {
  setSearchState("query", query);
}

export function setSearching(searching: boolean) {
  setSearchState("searching", searching);
}

export function setSearchOption<K extends keyof SearchOptions>(
  key: K,
  value: SearchOptions[K]
) {
  setSearchState("options", key, value);
}

export function setReplaceText(text: string) {
  setSearchState("replaceText", text);
}

export function toggleReplaceMode() {
  setSearchState("replaceMode", (v) => !v);
}

const MAX_SEARCH_FILES = 500;
const MAX_SEARCH_MATCHES = 10_000;

export function addSearchResult(result: SearchResult) {
  setSearchState(
    produce((s) => {
      if (s.totalFiles >= MAX_SEARCH_FILES || s.totalMatches >= MAX_SEARCH_MATCHES) {
        return;
      }
      s.results.push(result);
      s.totalFiles = s.results.length;
      s.totalMatches += result.matches.length;
    })
  );
}

export function isSearchCapped(): boolean {
  return (
    searchState.totalFiles >= MAX_SEARCH_FILES ||
    searchState.totalMatches >= MAX_SEARCH_MATCHES
  );
}

export function clearSearchResults() {
  setSearchState({
    results: [],
    searching: false,
    totalMatches: 0,
    totalFiles: 0,
  });
}

export function setSearchResults(results: SearchResult[]) {
  let totalMatches = 0;
  for (const r of results) totalMatches += r.matches.length;
  setSearchState({
    results,
    searching: false,
    totalMatches,
    totalFiles: results.length,
  });
}

export function resetSearchState() {
  setSearchState({
    query: "",
    results: [],
    searching: false,
    options: {
      regex: false,
      caseSensitive: false,
      wholeWord: false,
      includePattern: null,
      excludePattern: null,
    },
    replaceText: "",
    replaceMode: false,
    totalMatches: 0,
    totalFiles: 0,
  });
}
