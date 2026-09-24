import { describe, it, expect, beforeEach, vi, type Mock } from "vitest";
import { invoke } from "@tauri-apps/api/core";
import {
  variantState,
  clearVariants,
  resetVariantState,
  createVariantCache,
  selectVariant,
  onVariantChange,
} from "@/stores/variant.store";

const mockInvoke = vi.mocked(invoke);

function deferred<T>() {
  let resolve!: (value: T) => void;
  let reject!: (reason: unknown) => void;
  const promise = new Promise<T>((res, rej) => {
    resolve = res;
    reject = rej;
  });
  return { promise, resolve, reject };
}

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

describe("selectVariant", () => {
  let pending: Map<string, ReturnType<typeof deferred<void>>>;
  let onChange: Mock<(variant: string) => void>;

  beforeEach(async () => {
    resetVariantState();
    vi.clearAllMocks();
    pending = new Map();
    mockInvoke.mockImplementation((cmd, args) => {
      if (cmd !== "set_active_variant") return Promise.resolve(undefined);
      const d = deferred<void>();
      pending.set((args as { variant: string }).variant, d);
      return d.promise;
    });
    const initial = selectVariant("debug");
    pending.get("debug")!.resolve();
    await initial;
    onChange = vi.fn();
    onVariantChange(onChange);
  });

  it("rolls back when the backend rejects the selection", async () => {
    const pick = selectVariant("release");
    expect(variantState.activeVariant).toBe("release");

    pending.get("release")!.reject(new Error("bad variant"));
    await pick;

    expect(variantState.activeVariant).toBe("debug");
    expect(onChange).not.toHaveBeenCalled();
  });

  it("an older selection failing after a newer one succeeded keeps the newer selection", async () => {
    const pickB = selectVariant("release");
    const pickC = selectVariant("staging");

    pending.get("staging")!.resolve();
    await pickC;
    pending.get("release")!.reject(new Error("bad variant"));
    await pickB;

    expect(variantState.activeVariant).toBe("staging");
    expect(onChange).toHaveBeenCalledTimes(1);
    expect(onChange).toHaveBeenCalledWith("staging");
  });

  it("an older selection failing while a newer one is in flight keeps the newer selection", async () => {
    const pickB = selectVariant("release");
    const pickC = selectVariant("staging");

    pending.get("release")!.reject(new Error("bad variant"));
    await pickB;
    expect(variantState.activeVariant).toBe("staging");

    pending.get("staging")!.resolve();
    await pickC;
    expect(variantState.activeVariant).toBe("staging");
  });

  it("an older selection succeeding after a newer one does not report the older variant", async () => {
    const pickB = selectVariant("release");
    const pickC = selectVariant("staging");

    pending.get("staging")!.resolve();
    await pickC;
    pending.get("release")!.resolve();
    await pickB;

    expect(variantState.activeVariant).toBe("staging");
    expect(onChange).toHaveBeenCalledTimes(1);
    expect(onChange).toHaveBeenCalledWith("staging");
  });

  it("the newest selection failing still rolls back to the selection before it", async () => {
    const pickB = selectVariant("release");
    pending.get("release")!.resolve();
    await pickB;

    const pickC = selectVariant("staging");
    pending.get("staging")!.reject(new Error("bad variant"));
    await pickC;

    expect(variantState.activeVariant).toBe("release");
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
