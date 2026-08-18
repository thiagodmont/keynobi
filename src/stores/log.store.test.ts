import { describe, it, expect } from "vitest";
import { createLogStore } from "@/stores/log.store";
import type { LogEntry } from "@/bindings";

function entry(id: number): LogEntry {
  return {
    id,
    timestamp: "2026-01-01T00:00:00Z",
    level: "info",
    source: "gradle",
    message: `line ${id}`,
  };
}

describe("createLogStore", () => {
  it("appends entries in order", () => {
    const store = createLogStore({ maxEntries: 10 });
    store.pushEntry(entry(1));
    store.pushEntry(entry(2));

    expect(store.entries.map((e) => e.id)).toEqual([1, 2]);
  });

  it("evicts the oldest entries past the cap", () => {
    const store = createLogStore({ maxEntries: 3 });
    for (let i = 1; i <= 5; i += 1) store.pushEntry(entry(i));

    expect(store.entries.map((e) => e.id)).toEqual([3, 4, 5]);
  });

  it("applies the cap to a batch push", () => {
    const store = createLogStore({ maxEntries: 3 });
    store.pushEntries([entry(1), entry(2), entry(3), entry(4), entry(5)]);

    expect(store.entries.map((e) => e.id)).toEqual([3, 4, 5]);
  });

  it("ignores an empty batch", () => {
    const store = createLogStore({ maxEntries: 3 });
    store.pushEntry(entry(1));
    store.pushEntries([]);

    expect(store.entries).toHaveLength(1);
  });

  it("clears all entries", () => {
    const store = createLogStore({ maxEntries: 10 });
    store.pushEntries([entry(1), entry(2)]);
    store.clearEntries();

    expect(store.entries).toHaveLength(0);
  });

  it("keeps stores independent of one another", () => {
    const a = createLogStore({ maxEntries: 10 });
    const b = createLogStore({ maxEntries: 10 });

    a.pushEntry(entry(1));

    expect(a.entries).toHaveLength(1);
    expect(b.entries).toHaveLength(0);
  });

  it("defaults to a 2000-entry cap", () => {
    const store = createLogStore();
    store.pushEntries(Array.from({ length: 2100 }, (_, i) => entry(i)));

    expect(store.entries).toHaveLength(2000);
  });
});
