import { describe, it, expect, beforeEach } from "vitest";
import {
  searchState,
  setSearchQuery,
  setSearching,
  setSearchOption,
  setReplaceText,
  toggleReplaceMode,
  addSearchResult,
  clearSearchResults,
  setSearchResults,
  resetSearchState,
  type SearchResult,
} from "./search.store";

function makeResult(path: string, matchCount: number): SearchResult {
  return {
    path,
    matches: Array.from({ length: matchCount }, (_, i) => ({
      line: i + 1,
      col: 0,
      endCol: 5,
      lineContent: `line ${i + 1}`,
      contextBefore: [],
      contextAfter: [],
    })),
  };
}

describe("search.store", () => {
  beforeEach(() => {
    resetSearchState();
  });

  it("starts with empty state", () => {
    expect(searchState.query).toBe("");
    expect(searchState.results).toHaveLength(0);
    expect(searchState.searching).toBe(false);
    expect(searchState.replaceMode).toBe(false);
    expect(searchState.totalMatches).toBe(0);
    expect(searchState.totalFiles).toBe(0);
  });

  it("sets query", () => {
    setSearchQuery("hello");
    expect(searchState.query).toBe("hello");
  });

  it("sets searching state", () => {
    setSearching(true);
    expect(searchState.searching).toBe(true);
  });

  it("sets search options", () => {
    setSearchOption("regex", true);
    expect(searchState.options.regex).toBe(true);

    setSearchOption("caseSensitive", true);
    expect(searchState.options.caseSensitive).toBe(true);

    setSearchOption("includePattern", "*.kt");
    expect(searchState.options.includePattern).toBe("*.kt");
  });

  it("sets replace text", () => {
    setReplaceText("world");
    expect(searchState.replaceText).toBe("world");
  });

  it("toggles replace mode", () => {
    expect(searchState.replaceMode).toBe(false);
    toggleReplaceMode();
    expect(searchState.replaceMode).toBe(true);
    toggleReplaceMode();
    expect(searchState.replaceMode).toBe(false);
  });

  it("adds streaming results and updates counts", () => {
    addSearchResult(makeResult("/project/A.kt", 3));
    expect(searchState.results).toHaveLength(1);
    expect(searchState.totalMatches).toBe(3);
    expect(searchState.totalFiles).toBe(1);

    addSearchResult(makeResult("/project/B.kt", 2));
    expect(searchState.results).toHaveLength(2);
    expect(searchState.totalMatches).toBe(5);
    expect(searchState.totalFiles).toBe(2);
  });

  it("clears search results", () => {
    addSearchResult(makeResult("/project/A.kt", 3));
    clearSearchResults();
    expect(searchState.results).toHaveLength(0);
    expect(searchState.totalMatches).toBe(0);
    expect(searchState.totalFiles).toBe(0);
    expect(searchState.searching).toBe(false);
  });

  it("sets batch results", () => {
    setSearchResults([
      makeResult("/project/A.kt", 2),
      makeResult("/project/B.kt", 5),
    ]);
    expect(searchState.results).toHaveLength(2);
    expect(searchState.totalMatches).toBe(7);
    expect(searchState.totalFiles).toBe(2);
    expect(searchState.searching).toBe(false);
  });

  it("resets everything", () => {
    setSearchQuery("test");
    setSearchOption("regex", true);
    setReplaceText("replace");
    toggleReplaceMode();
    addSearchResult(makeResult("/project/A.kt", 1));

    resetSearchState();
    expect(searchState.query).toBe("");
    expect(searchState.options.regex).toBe(false);
    expect(searchState.replaceText).toBe("");
    expect(searchState.replaceMode).toBe(false);
    expect(searchState.results).toHaveLength(0);
  });
});
