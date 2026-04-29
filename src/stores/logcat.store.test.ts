import { describe, expect, it } from "vitest";
import type { LogcatEntry } from "@/lib/tauri-api";
import { appendEntriesIncremental, computeCrashIndices } from "./logcat.store";

function entry(id: number, isCrash = false): LogcatEntry {
  return {
    id: BigInt(id),
    timestamp: "01-01 00:00:00.000",
    level: isCrash ? "fatal" : "debug",
    tag: `Tag${id}`,
    message: `message ${id}`,
    pid: 1,
    tid: 1,
    package: null,
    isCrash,
    crashGroupId: null,
    jsonBody: null,
    flags: 0,
    kind: "normal",
    category: "general",
  };
}

describe("logcat store helpers", () => {
  it("appends without replacing arrays when no eviction is needed", () => {
    const entries = [entry(1), entry(2, true), entry(3)];
    const crashIndices = [1];
    const originalEntries = entries;
    const originalCrashIndices = crashIndices;

    const dropped = appendEntriesIncremental(entries, crashIndices, [entry(4), entry(5, true)], 10);

    expect(dropped).toBe(0);
    expect(entries).toBe(originalEntries);
    expect(crashIndices).toBe(originalCrashIndices);
    expect(entries.map((e) => e.id)).toEqual([1n, 2n, 3n, 4n, 5n]);
    expect(crashIndices).toEqual([1, 4]);
  });

  it("strictly caps entries and rebuilds crash indices only after eviction", () => {
    const entries = [entry(1), entry(2, true), entry(3)];
    const crashIndices = [1];

    const dropped = appendEntriesIncremental(
      entries,
      crashIndices,
      [entry(4), entry(5, true), entry(6)],
      4
    );

    expect(dropped).toBe(2);
    expect(entries.map((e) => e.id)).toEqual([3n, 4n, 5n, 6n]);
    expect(crashIndices).toEqual([2]);
  });

  it("handles a batch larger than the cap", () => {
    const entries = [entry(1)];
    const crashIndices: number[] = [];
    const dropped = appendEntriesIncremental(
      entries,
      crashIndices,
      [entry(2), entry(3, true), entry(4)],
      2
    );

    expect(dropped).toBe(2);
    expect(entries.map((e) => e.id)).toEqual([3n, 4n]);
    expect(crashIndices).toEqual([0]);
  });

  it("computes crash indices for an existing list", () => {
    expect(computeCrashIndices([entry(1, true), entry(2), entry(3, true)])).toEqual([0, 2]);
  });
});
