import { describe, it, expect, beforeEach } from "vitest";
import {
  variantState,
  clearVariants,
  resetVariantState,
  createVariantCache,
} from "@/stores/variant.store";

describe("variant.store", () => {
  beforeEach(() => {
    resetVariantState();
  });

  it("starts with empty variants", () => {
    expect(variantState.variants).toHaveLength(0);
    expect(variantState.activeVariant).toBeNull();
    expect(variantState.loading).toBe(false);
  });

  it("clearVariants resets to empty", () => {
    clearVariants();
    expect(variantState.variants).toHaveLength(0);
    expect(variantState.activeVariant).toBeNull();
  });

  it("resetVariantState clears everything including error", () => {
    resetVariantState();
    expect(variantState.error).toBeNull();
    expect(variantState.loading).toBe(false);
  });
});

describe("createVariantCache", () => {
  const entry = { variants: [], defaultVariant: null };

  it("stores and retrieves entries by root", () => {
    const cache = createVariantCache({ maxEntries: 3 });
    cache.set("/p1", entry);
    expect(cache.get("/p1")).toBe(entry);
    expect(cache.size).toBe(1);
  });

  it("evicts the oldest-inserted entry past the cap", () => {
    const cache = createVariantCache({ maxEntries: 2 });
    cache.set("/p1", { ...entry, defaultVariant: "one" });
    cache.set("/p2", { ...entry, defaultVariant: "two" });
    cache.set("/p3", { ...entry, defaultVariant: "three" });

    expect(cache.size).toBe(2);
    expect(cache.get("/p1")).toBeUndefined();
    expect(cache.get("/p2")?.defaultVariant).toBe("two");
    expect(cache.get("/p3")?.defaultVariant).toBe("three");
  });

  it("refreshes recency on re-set of an existing key", () => {
    const cache = createVariantCache({ maxEntries: 2 });
    cache.set("/p1", entry);
    cache.set("/p2", entry);
    cache.set("/p1", entry);
    cache.set("/p3", entry);

    expect(cache.get("/p1")).toBeDefined();
    expect(cache.get("/p2")).toBeUndefined();
    expect(cache.get("/p3")).toBeDefined();
  });

  it("delete and clear empty the cache", () => {
    const cache = createVariantCache({ maxEntries: 3 });
    cache.set("/p1", entry);
    cache.delete("/p1");
    expect(cache.get("/p1")).toBeUndefined();

    cache.set("/p2", entry);
    cache.clear();
    expect(cache.size).toBe(0);
  });

  it("handles a zero cap by storing nothing", () => {
    const cache = createVariantCache({ maxEntries: 0 });
    cache.set("/p1", entry);
    expect(cache.get("/p1")).toBeUndefined();
    expect(cache.size).toBe(0);
  });
});
