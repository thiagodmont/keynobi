import { describe, expect, it } from "vitest";
import type { LogcatEntry } from "@/lib/tauri-api";
import { createLogcatSuggestionIndex } from "./logcat-suggestions";

function entry(id: number, tag: string, pkg?: string): LogcatEntry {
  return {
    id: BigInt(id),
    timestamp: "01-01 00:00:00.000",
    level: "debug",
    tag,
    message: "message",
    pid: 1,
    tid: 1,
    package: pkg ?? null,
    isCrash: false,
    crashGroupId: null,
    jsonBody: null,
    flags: 0,
    kind: "normal",
    category: "general",
  };
}

describe("createLogcatSuggestionIndex", () => {
  it("bounds remembered packages and tags while preserving ranked tag suggestions", () => {
    const index = createLogcatSuggestionIndex({ maxPackages: 3, maxTags: 4, maxVisibleTags: 2 });

    index.ingest([
      entry(1, "Noisy", "com.example.one"),
      entry(2, "Noisy", "com.example.two"),
      entry(3, "Rare", "com.example.three"),
      entry(4, "Noisy", "com.example.four"),
      entry(5, "Other", "com.example.five"),
      entry(6, "Other"),
      entry(7, "IgnoredSeparator", "com.example.six"),
      { ...entry(8, "ProcessDied", "com.example.seven"), kind: "processDied" },
    ]);

    expect(index.packages()).toHaveLength(3);
    expect(index.packages()).toEqual(["com.example.five", "com.example.seven", "com.example.six"]);
    expect(index.tags()).toEqual(["Noisy", "Other"]);
  });

  it("clears all accumulated suggestion data", () => {
    const index = createLogcatSuggestionIndex({ maxPackages: 3, maxTags: 3, maxVisibleTags: 3 });

    index.ingest([entry(1, "App", "com.example")]);
    index.clear();

    expect(index.packages()).toEqual([]);
    expect(index.tags()).toEqual([]);
  });
});
