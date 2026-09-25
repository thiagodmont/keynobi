import { describe, expect, it } from "vitest";
import { makeLogEntry } from "@/test/factories/logcat";
import {
  isExpandedContextRow,
  mergeExpandedContextEntries,
  mergeLogcatEntriesChronologically,
} from "./logcat-context-expansion";

describe("logcat context expansion", () => {
  it("merges filtered and expanded entries in chronological id order", () => {
    const filtered = [
      makeLogEntry({ id: 10, message: "filtered 10" }),
      makeLogEntry({ id: 20, message: "filtered 20" }),
    ];
    const expanded = [
      makeLogEntry({ id: 19, message: "expanded 19" }),
      makeLogEntry({ id: 9, message: "expanded 9" }),
    ];

    const merged = mergeLogcatEntriesChronologically(filtered, expanded);

    expect(merged.map((entry) => entry.id)).toEqual([9, 10, 19, 20]);
  });

  it("deduplicates expanded context rows and keeps them bounded", () => {
    const current = [
      makeLogEntry({ id: 2, message: "current 2" }),
      makeLogEntry({ id: 4, message: "current 4" }),
    ];
    const incoming = [
      makeLogEntry({ id: 1, message: "incoming 1" }),
      makeLogEntry({ id: 2, message: "duplicate 2" }),
      makeLogEntry({ id: 3, message: "incoming 3" }),
    ];

    const merged = mergeExpandedContextEntries(current, incoming, 3);

    expect(merged.map((entry) => entry.id)).toEqual([1, 2, 4]);
    expect(merged.find((entry) => entry.id === 2)?.message).toBe("current 2");
  });

  it("marks only rows brought in as expanded context", () => {
    const expandedIds = new Set([1, 2]);
    const filteredIds = new Set([2, 3]);

    expect(isExpandedContextRow(1, expandedIds, filteredIds)).toBe(true);
    expect(isExpandedContextRow(2, expandedIds, filteredIds)).toBe(false);
    expect(isExpandedContextRow(3, expandedIds, filteredIds)).toBe(false);
  });
});
