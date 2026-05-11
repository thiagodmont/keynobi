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
      makeLogEntry({ id: 10n, message: "filtered 10" }),
      makeLogEntry({ id: 20n, message: "filtered 20" }),
    ];
    const expanded = [
      makeLogEntry({ id: 19n, message: "expanded 19" }),
      makeLogEntry({ id: 9n, message: "expanded 9" }),
    ];

    const merged = mergeLogcatEntriesChronologically(filtered, expanded);

    expect(merged.map((entry) => entry.id)).toEqual([9n, 10n, 19n, 20n]);
  });

  it("deduplicates expanded context rows and keeps them bounded", () => {
    const current = [
      makeLogEntry({ id: 2n, message: "current 2" }),
      makeLogEntry({ id: 4n, message: "current 4" }),
    ];
    const incoming = [
      makeLogEntry({ id: 1n, message: "incoming 1" }),
      makeLogEntry({ id: 2n, message: "duplicate 2" }),
      makeLogEntry({ id: 3n, message: "incoming 3" }),
    ];

    const merged = mergeExpandedContextEntries(current, incoming, 3);

    expect(merged.map((entry) => entry.id)).toEqual([1n, 2n, 4n]);
    expect(merged.find((entry) => entry.id === 2n)?.message).toBe("current 2");
  });

  it("marks only rows brought in as expanded context", () => {
    const expandedIds = new Set([1n, 2n]);
    const filteredIds = new Set([2n, 3n]);

    expect(isExpandedContextRow(1n, expandedIds, filteredIds)).toBe(true);
    expect(isExpandedContextRow(2n, expandedIds, filteredIds)).toBe(false);
    expect(isExpandedContextRow(3n, expandedIds, filteredIds)).toBe(false);
  });
});
