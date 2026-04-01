import { describe, it, expect, beforeEach } from "vitest";
import {
  loadFilterStorage,
  saveFilterStorage,
  addSavedFilter,
  deleteSavedFilter,
  renameSavedFilter,
  getLastActiveQuery,
  setLastActiveQuery,
  MAX_SAVED_FILTERS,
  type SavedFilter,
} from "./logcat-filter-storage";

// ── localStorage stub ─────────────────────────────────────────────────────────

const store: Record<string, string> = {};

beforeEach(() => {
  // Clear the in-memory store before every test
  for (const key of Object.keys(store)) delete store[key];

  // Patch globalThis.localStorage for the test environment (jsdom may not
  // have it, or tests may run in a non-browser worker)
  Object.defineProperty(globalThis, "localStorage", {
    value: {
      getItem: (k: string) => store[k] ?? null,
      setItem: (k: string, v: string) => { store[k] = v; },
      removeItem: (k: string) => { delete store[k]; },
      clear: () => { for (const key of Object.keys(store)) delete store[key]; },
    },
    writable: true,
    configurable: true,
  });
});

// ── loadFilterStorage / saveFilterStorage ─────────────────────────────────────

describe("loadFilterStorage", () => {
  it("returns empty storage when localStorage is empty", () => {
    const s = loadFilterStorage();
    expect(s.filters).toHaveLength(0);
    expect(s.lastActiveQuery).toBe("");
  });

  it("round-trips filters through save/load", () => {
    const filter: SavedFilter = {
      id: "abc",
      name: "My filter",
      query: "level:error",
      createdAt: 1000,
    };
    saveFilterStorage({ filters: [filter], lastActiveQuery: "level:error" });
    const loaded = loadFilterStorage();
    expect(loaded.filters).toHaveLength(1);
    expect(loaded.filters[0]).toEqual(filter);
    expect(loaded.lastActiveQuery).toBe("level:error");
  });

  it("returns valid empty storage when localStorage contains invalid JSON", () => {
    store["logcat_saved_filters_v1"] = "not-json";
    const s = loadFilterStorage();
    expect(s.filters).toHaveLength(0);
    expect(s.lastActiveQuery).toBe("");
  });

  it("handles missing filters array gracefully", () => {
    store["logcat_saved_filters_v1"] = JSON.stringify({ lastActiveQuery: "test" });
    const s = loadFilterStorage();
    expect(s.filters).toHaveLength(0);
    expect(s.lastActiveQuery).toBe("test");
  });
});

describe("saveFilterStorage", () => {
  it("caps filters at MAX_SAVED_FILTERS", () => {
    const filters: SavedFilter[] = Array.from({ length: MAX_SAVED_FILTERS + 5 }, (_, i) => ({
      id: `id-${i}`,
      name: `Filter ${i}`,
      query: `tag:Tag${i}`,
      createdAt: i,
    }));
    saveFilterStorage({ filters, lastActiveQuery: "" });
    const loaded = loadFilterStorage();
    expect(loaded.filters).toHaveLength(MAX_SAVED_FILTERS);
  });
});

// ── Legacy migration ──────────────────────────────────────────────────────────

describe("legacy migration from logcat_presets_v1", () => {
  it("migrates user presets to new schema", () => {
    store["logcat_presets_v1"] = JSON.stringify([
      { name: "Old preset", query: "level:warn" },
      { name: "Another", query: "is:crash" },
    ]);
    const s = loadFilterStorage();
    expect(s.filters).toHaveLength(2);
    expect(s.filters[0].name).toBe("Old preset");
    expect(s.filters[0].query).toBe("level:warn");
    expect(typeof s.filters[0].id).toBe("string");
    expect(s.filters[0].createdAt).toBeGreaterThan(0);
  });

  it("removes legacy key after migration", () => {
    store["logcat_presets_v1"] = JSON.stringify([{ name: "P", query: "q" }]);
    loadFilterStorage();
    expect(store["logcat_presets_v1"]).toBeUndefined();
  });

  it("persists migrated filters to new key so a second load returns them (data loss regression)", () => {
    store["logcat_presets_v1"] = JSON.stringify([
      { name: "Migrated", query: "level:warn" },
    ]);
    // First load triggers migration
    loadFilterStorage();
    // Legacy key is gone now — second load must still return the migrated filter
    const s2 = loadFilterStorage();
    expect(s2.filters).toHaveLength(1);
    expect(s2.filters[0].name).toBe("Migrated");
  });

  it("setLastActiveQuery after migration does not erase migrated filters", () => {
    store["logcat_presets_v1"] = JSON.stringify([{ name: "Keep", query: "is:crash" }]);
    loadFilterStorage(); // migrate
    setLastActiveQuery("level:error");
    const s = loadFilterStorage();
    expect(s.filters).toHaveLength(1);
    expect(s.filters[0].name).toBe("Keep");
    expect(s.lastActiveQuery).toBe("level:error");
  });

  it("loadFilterStorage called twice after migration returns filters both times", () => {
    store["logcat_presets_v1"] = JSON.stringify([{ name: "A", query: "tag:App" }]);
    const s1 = loadFilterStorage();
    const s2 = loadFilterStorage();
    expect(s1.filters).toHaveLength(1);
    expect(s2.filters).toHaveLength(1);
    expect(s2.filters[0].name).toBe("A");
  });

  it("does not migrate builtin presets (builtin: true)", () => {
    store["logcat_presets_v1"] = JSON.stringify([
      { name: "My App", query: "package:mine", builtin: true },
      { name: "User", query: "level:error" },
    ]);
    const s = loadFilterStorage();
    expect(s.filters).toHaveLength(1);
    expect(s.filters[0].name).toBe("User");
  });

  it("deduplicates by name when merging with existing filters", () => {
    // Pre-populate new storage with a filter named "Old preset"
    saveFilterStorage({ filters: [{ id: "x", name: "Old preset", query: "age:5m", createdAt: 0 }], lastActiveQuery: "" });
    // Also set legacy key with the same name
    store["logcat_presets_v1"] = JSON.stringify([{ name: "Old preset", query: "level:warn" }]);
    const s = loadFilterStorage();
    // Should not duplicate — existing filter takes priority
    expect(s.filters.filter((f) => f.name === "Old preset")).toHaveLength(1);
    expect(s.filters.find((f) => f.name === "Old preset")?.query).toBe("age:5m");
  });

  it("handles malformed legacy JSON gracefully", () => {
    store["logcat_presets_v1"] = "not-valid-json";
    const s = loadFilterStorage();
    expect(s.filters).toHaveLength(0);
    // Legacy key not removed on parse failure (nothing to clean up)
  });

  it("handles legacy key containing non-array JSON", () => {
    store["logcat_presets_v1"] = JSON.stringify({ key: "value" });
    const s = loadFilterStorage();
    expect(s.filters).toHaveLength(0);
  });
});

// ── addSavedFilter ────────────────────────────────────────────────────────────

describe("addSavedFilter", () => {
  it("adds a new filter and returns it", () => {
    const f = addSavedFilter("My Filter", "level:error");
    expect(f.name).toBe("My Filter");
    expect(f.query).toBe("level:error");
    expect(typeof f.id).toBe("string");
    expect(f.createdAt).toBeGreaterThan(0);
  });

  it("persists the filter to localStorage", () => {
    addSavedFilter("Persistent", "tag:App");
    const s = loadFilterStorage();
    expect(s.filters).toHaveLength(1);
    expect(s.filters[0].name).toBe("Persistent");
  });

  it("trims whitespace from the name", () => {
    const f = addSavedFilter("  Spaces  ", "level:info");
    expect(f.name).toBe("Spaces");
  });

  it("throws for an empty name", () => {
    expect(() => addSavedFilter("", "level:error")).toThrow();
    expect(() => addSavedFilter("   ", "level:error")).toThrow();
  });

  it("overwrites an existing filter with the same name (no duplicates)", () => {
    addSavedFilter("Same Name", "level:error");
    const updated = addSavedFilter("Same Name", "level:warn");
    const s = loadFilterStorage();
    expect(s.filters.filter((f) => f.name === "Same Name")).toHaveLength(1);
    expect(s.filters[0].query).toBe("level:warn");
    expect(updated.query).toBe("level:warn");
  });

  it("throws when cap is reached and name is new", () => {
    const filters: SavedFilter[] = Array.from({ length: MAX_SAVED_FILTERS }, (_, i) => ({
      id: `id-${i}`,
      name: `Filter ${i}`,
      query: `tag:Tag${i}`,
      createdAt: i,
    }));
    saveFilterStorage({ filters, lastActiveQuery: "" });
    expect(() => addSavedFilter("New One", "level:error")).toThrow();
  });

  it("allows overwrite when cap is reached (same name)", () => {
    const filters: SavedFilter[] = Array.from({ length: MAX_SAVED_FILTERS }, (_, i) => ({
      id: `id-${i}`,
      name: `Filter ${i}`,
      query: `tag:Tag${i}`,
      createdAt: i,
    }));
    saveFilterStorage({ filters, lastActiveQuery: "" });
    // "Filter 0" already exists — overwrite is allowed even at cap
    expect(() => addSavedFilter("Filter 0", "new:query")).not.toThrow();
  });

  it("stores OR-group queries correctly", () => {
    const f = addSavedFilter("OR filter", "level:error | is:crash");
    expect(f.query).toBe("level:error | is:crash");
  });
});

// ── deleteSavedFilter ─────────────────────────────────────────────────────────

describe("deleteSavedFilter", () => {
  it("removes the filter with the matching id", () => {
    const f = addSavedFilter("To Delete", "level:error");
    deleteSavedFilter(f.id);
    const s = loadFilterStorage();
    expect(s.filters).toHaveLength(0);
  });

  it("does nothing for an unknown id", () => {
    addSavedFilter("Keep", "level:warn");
    deleteSavedFilter("nonexistent-id");
    const s = loadFilterStorage();
    expect(s.filters).toHaveLength(1);
  });

  it("only removes the specified filter, leaving others intact", () => {
    const f1 = addSavedFilter("One", "level:error");
    addSavedFilter("Two", "level:warn");
    deleteSavedFilter(f1.id);
    const s = loadFilterStorage();
    expect(s.filters).toHaveLength(1);
    expect(s.filters[0].name).toBe("Two");
  });
});

// ── renameSavedFilter ─────────────────────────────────────────────────────────

describe("renameSavedFilter", () => {
  it("renames the filter", () => {
    const f = addSavedFilter("Old Name", "level:error");
    renameSavedFilter(f.id, "New Name");
    const s = loadFilterStorage();
    expect(s.filters[0].name).toBe("New Name");
  });

  it("does nothing for unknown id", () => {
    addSavedFilter("Existing", "level:error");
    renameSavedFilter("no-such-id", "Different");
    const s = loadFilterStorage();
    expect(s.filters[0].name).toBe("Existing");
  });

  it("ignores rename to empty string", () => {
    const f = addSavedFilter("Keep Name", "level:error");
    renameSavedFilter(f.id, "");
    const s = loadFilterStorage();
    expect(s.filters[0].name).toBe("Keep Name");
  });

  it("ignores rename to whitespace-only string", () => {
    const f = addSavedFilter("Keep Name", "level:error");
    renameSavedFilter(f.id, "   ");
    const s = loadFilterStorage();
    expect(s.filters[0].name).toBe("Keep Name");
  });

  it("ignores rename that would collide with another filter name", () => {
    addSavedFilter("Filter A", "level:error");
    const b = addSavedFilter("Filter B", "level:warn");
    renameSavedFilter(b.id, "Filter A"); // collision
    const s = loadFilterStorage();
    expect(s.filters.find((f) => f.id === b.id)?.name).toBe("Filter B");
  });
});

// ── getLastActiveQuery / setLastActiveQuery ───────────────────────────────────

describe("getLastActiveQuery / setLastActiveQuery", () => {
  it("returns empty string when nothing saved", () => {
    expect(getLastActiveQuery()).toBe("");
  });

  it("round-trips a query string", () => {
    setLastActiveQuery("level:error tag:App");
    expect(getLastActiveQuery()).toBe("level:error tag:App");
  });

  it("persists OR-group queries", () => {
    setLastActiveQuery("level:error | is:crash");
    expect(getLastActiveQuery()).toBe("level:error | is:crash");
  });

  it("overwrites previous value", () => {
    setLastActiveQuery("first");
    setLastActiveQuery("second");
    expect(getLastActiveQuery()).toBe("second");
  });

  it("does not overwrite saved filters when updating last active query", () => {
    addSavedFilter("Keep Me", "level:warn");
    setLastActiveQuery("new:query");
    const s = loadFilterStorage();
    expect(s.filters).toHaveLength(1);
    expect(s.filters[0].name).toBe("Keep Me");
    expect(s.lastActiveQuery).toBe("new:query");
  });
});
