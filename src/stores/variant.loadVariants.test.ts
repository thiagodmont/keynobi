import { describe, it, expect, beforeEach, vi } from "vitest";
import type { BuildVariant, VariantList } from "@/bindings";

const mockPreview = vi.fn();
const mockGradle = vi.fn();

vi.mock("@/lib/tauri-api", () => ({
  getVariantsPreview: (...args: unknown[]) => mockPreview(...args),
  getVariantsFromGradle: (...args: unknown[]) => mockGradle(...args),
  setActiveVariant: vi.fn(),
}));

import { loadVariants, resetVariantState, variantState } from "@/stores/variant.store";

const sampleVariant: BuildVariant = {
  name: "debug",
  buildType: "debug",
  flavors: [],
  assembleTask: "assembleDebug",
  installTask: "installDebug",
};

const sampleList: VariantList = {
  variants: [sampleVariant],
  active: "debug",
};

describe("loadVariants coalescing", () => {
  beforeEach(() => {
    resetVariantState();
    mockPreview.mockReset();
    mockGradle.mockReset();
    mockPreview.mockResolvedValue(sampleList);
    mockGradle.mockImplementation(
      () =>
        new Promise<VariantList>((resolve) => {
          setTimeout(() => resolve(sampleList), 15);
        }),
    );
  });

  it("merges concurrent callers into one Gradle invocation", async () => {
    const a = loadVariants();
    const b = loadVariants();
    await Promise.all([a, b]);
    expect(mockGradle).toHaveBeenCalledTimes(1);
    expect(variantState.fromGradle).toBe(true);
  });

  it("allows a second load after the first completes", async () => {
    await loadVariants();
    expect(mockGradle).toHaveBeenCalledTimes(1);
    await loadVariants();
    expect(mockGradle).toHaveBeenCalledTimes(2);
  });
});
