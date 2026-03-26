import { describe, it, expect } from "vitest";
import { fuzzyMatch, fuzzyFilter } from "./fuzzy-match";

describe("fuzzyMatch", () => {
  it("matches exact substring", () => {
    const result = fuzzyMatch("main", "MainActivity.kt");
    expect(result).not.toBeNull();
    expect(result!.matchedIndices).toHaveLength(4);
  });

  it("matches scattered characters", () => {
    const result = fuzzyMatch("mak", "MainActivity.kt");
    expect(result).not.toBeNull();
  });

  it("returns null for no match", () => {
    expect(fuzzyMatch("xyz", "MainActivity.kt")).toBeNull();
  });

  it("returns null when query is longer than text", () => {
    expect(fuzzyMatch("abcdef", "abc")).toBeNull();
  });

  it("returns match for empty query", () => {
    const result = fuzzyMatch("", "anything");
    expect(result).not.toBeNull();
    expect(result!.score).toBe(0);
  });

  it("scores start-of-string matches higher", () => {
    const startMatch = fuzzyMatch("ma", "MainActivity.kt");
    const midMatch = fuzzyMatch("ma", "getMain");
    expect(startMatch).not.toBeNull();
    expect(midMatch).not.toBeNull();
    expect(startMatch!.score).toBeGreaterThan(midMatch!.score);
  });

  it("scores consecutive matches higher", () => {
    const consecutive = fuzzyMatch("act", "Activity.kt");
    const scattered = fuzzyMatch("act", "ApplicationController.kt");
    expect(consecutive).not.toBeNull();
    expect(scattered).not.toBeNull();
    expect(consecutive!.score).toBeGreaterThan(scattered!.score);
  });

  it("scores word boundary matches higher", () => {
    const boundary = fuzzyMatch("fb", "FooBar.kt");
    const nonBoundary = fuzzyMatch("fb", "afob.kt");
    expect(boundary).not.toBeNull();
    expect(nonBoundary).not.toBeNull();
    expect(boundary!.score).toBeGreaterThan(nonBoundary!.score);
  });
});

describe("fuzzyFilter", () => {
  const files = [
    "src/main/MainActivity.kt",
    "src/main/MainViewModel.kt",
    "src/test/MainTest.kt",
    "build.gradle.kts",
    "settings.gradle.kts",
    "README.md",
  ];

  it("returns all items for empty query", () => {
    const results = fuzzyFilter(files, "", (f) => f);
    expect(results).toHaveLength(files.length);
  });

  it("filters to matching items", () => {
    const results = fuzzyFilter(files, "main", (f) => f);
    expect(results.length).toBeGreaterThanOrEqual(3);
    expect(results.every((r) => r.item.toLowerCase().includes("main"))).toBe(true);
  });

  it("ranks better matches first", () => {
    const results = fuzzyFilter(files, "gradle", (f) => f);
    expect(results.length).toBe(2);
    expect(results[0].item).toContain("gradle");
  });

  it("works with custom getText", () => {
    const items = [
      { name: "Alpha", value: 1 },
      { name: "Beta", value: 2 },
      { name: "Gamma", value: 3 },
    ];
    const results = fuzzyFilter(items, "alp", (i) => i.name);
    expect(results).toHaveLength(1);
    expect(results[0].item.name).toBe("Alpha");
  });

  it("handles empty items array", () => {
    const results = fuzzyFilter([], "test", (x: string) => x);
    expect(results).toHaveLength(0);
  });

  it("handles single-character query", () => {
    const results = fuzzyFilter(files, "R", (f) => f);
    expect(results.length).toBeGreaterThanOrEqual(1);
  });

  it("handles unicode characters", () => {
    const items = ["Ñoño.kt", "Über.kt", "日本語.kt"];
    const results = fuzzyFilter(items, "Ñ", (x) => x);
    expect(results.length).toBeGreaterThanOrEqual(1);
  });

  it("handles whitespace-only query", () => {
    const results = fuzzyFilter(files, "   ", (f) => f);
    expect(results).toHaveLength(files.length);
  });
});
