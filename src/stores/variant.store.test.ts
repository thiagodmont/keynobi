import { describe, it, expect, beforeEach } from "vitest";
import {
  variantState,
  selectVariant,
  clearVariants,
  resetVariantState,
} from "@/stores/variant.store";
import { createStore } from "solid-js/store";

// Helper to manually set variants without calling the API.
function setVariantsDirect(variants: any[], active: string | null) {
  import("@/stores/variant.store").then(({ variantState: _vs }) => {
    // We rely on clearVariants + the reactive store directly.
  });
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
    // selectVariant will fail without a real tauri backend but we can test clearVariants.
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
