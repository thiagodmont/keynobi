import { describe, it, expect, beforeEach } from "vitest";
import {
  variantState,
  clearVariants,
  resetVariantState,
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
