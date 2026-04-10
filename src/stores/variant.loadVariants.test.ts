import { describe, it, expect, beforeEach, vi } from "vitest";
import type { BuildVariant, VariantList } from "@/bindings";

const mockPreview = vi.fn();
const mockGradle = vi.fn();

vi.mock("@/lib/tauri-api", () => ({
  getVariantsPreview: (...args: unknown[]) => mockPreview(...args),
  getVariantsFromGradle: (...args: unknown[]) => mockGradle(...args),
  setActiveVariant: vi.fn(),
}));

import { loadVariants, resetVariantState, clearVariantCache, variantState } from "@/stores/variant.store";
import { setProject, setProjectState } from "@/stores/project.store";

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

function resetProjectState() {
  setProjectState({ projectRoot: null, gradleRoot: null, projectName: null, loading: false });
}

describe("loadVariants coalescing", () => {
  beforeEach(() => {
    setProject("/projects/test-project", "test-project");
    resetVariantState();
    clearVariantCache();
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

  it("second load for same project root hits cache — Gradle called once total", async () => {
    await loadVariants();
    expect(mockGradle).toHaveBeenCalledTimes(1);

    // Second load: same project root → cache hit, no Gradle call.
    await loadVariants();
    expect(mockGradle).toHaveBeenCalledTimes(1);
    expect(variantState.fromGradle).toBe(true);
  });
});

describe("loadVariants cache", () => {
  beforeEach(() => {
    resetVariantState();
    clearVariantCache();
    resetProjectState();
    mockPreview.mockReset();
    mockGradle.mockReset();
    mockPreview.mockResolvedValue(sampleList);
    mockGradle.mockResolvedValue(sampleList);
  });

  it("does not cache when projectRoot is null", async () => {
    // projectRoot is null (no project open) — Gradle is called each time.
    await loadVariants();
    expect(mockGradle).toHaveBeenCalledTimes(1);

    await loadVariants();
    expect(mockGradle).toHaveBeenCalledTimes(2);
  });

  it("caches Gradle result per project root — different root calls Gradle again", async () => {
    setProject("/projects/project-a", "project-a");
    await loadVariants();
    expect(mockGradle).toHaveBeenCalledTimes(1);

    setProject("/projects/project-b", "project-b");
    resetVariantState();
    await loadVariants();
    expect(mockGradle).toHaveBeenCalledTimes(2);
  });

  it("switching back to a cached project root skips Gradle", async () => {
    setProject("/projects/project-a", "project-a");
    await loadVariants();
    expect(mockGradle).toHaveBeenCalledTimes(1);

    setProject("/projects/project-b", "project-b");
    resetVariantState();
    await loadVariants();
    expect(mockGradle).toHaveBeenCalledTimes(2);

    // Switch back to A — already cached.
    setProject("/projects/project-a", "project-a");
    resetVariantState();
    await loadVariants();
    expect(mockGradle).toHaveBeenCalledTimes(2);
    expect(variantState.fromGradle).toBe(true);
  });

  it("force: true bypasses cache and calls Gradle again", async () => {
    setProject("/projects/project-a", "project-a");
    await loadVariants();
    expect(mockGradle).toHaveBeenCalledTimes(1);

    // force: true should evict cache and re-run Gradle.
    await loadVariants({ force: true });
    expect(mockGradle).toHaveBeenCalledTimes(2);
  });

  it("cache hit populates variants and sets fromGradle", async () => {
    setProject("/projects/project-a", "project-a");
    await loadVariants();

    resetVariantState();
    await loadVariants(); // cache hit
    expect(variantState.variants).toHaveLength(1);
    expect(variantState.variants[0].name).toBe("debug");
    expect(variantState.fromGradle).toBe(true);
    expect(variantState.gradleLoading).toBe(false);
  });
});
