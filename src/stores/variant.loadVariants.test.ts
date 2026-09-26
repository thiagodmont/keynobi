import { describe, it, expect, beforeEach, afterEach, vi } from "vitest";
import type { BuildVariant, VariantList } from "@/bindings";

const mockPreview = vi.fn();
const mockGradle = vi.fn();

vi.mock("@/lib/tauri-api", () => ({
  getVariantsPreview: (...args: unknown[]) => mockPreview(...args),
  getVariantsFromGradle: (...args: unknown[]) => mockGradle(...args),
  setActiveVariant: vi.fn(),
}));

const mockTrusted = vi.fn((_root: string | null) => true);
vi.mock("@/stores/projects.store", () => ({
  isProjectTrusted: (root: string | null) => mockTrusted(root),
}));

import {
  loadVariants,
  resetVariantState,
  clearVariantCache,
  variantState,
} from "@/stores/variant.store";
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
  defaultVariant: null,
};

function variantNamed(name: string): BuildVariant {
  return {
    name,
    buildType: "debug",
    flavors: [],
    assembleTask: `assemble${name.charAt(0).toUpperCase()}${name.slice(1)}`,
    installTask: `install${name.charAt(0).toUpperCase()}${name.slice(1)}`,
  };
}

function listFor(name: string): VariantList {
  return {
    variants: [variantNamed(name)],
    active: name,
    defaultVariant: null,
  };
}

function deferred<T>() {
  let resolve!: (value: T) => void;
  const promise = new Promise<T>((r) => {
    resolve = r;
  });
  return { promise, resolve };
}

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
        })
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

  it("does not apply stale variants when project changes during an in-flight load", async () => {
    const projectA = listFor("projectADebug");
    const projectB = listFor("projectBDebug");
    const aPreview = deferred<VariantList>();
    const aGradle = deferred<VariantList>();
    const emptyList: VariantList = { variants: [], active: null, defaultVariant: null };

    mockPreview
      .mockImplementationOnce(() => aPreview.promise)
      .mockImplementationOnce(() => Promise.resolve(emptyList));
    mockGradle
      .mockImplementationOnce(() => aGradle.promise)
      .mockImplementationOnce(() => Promise.resolve(projectB));

    setProject("/projects/project-a", "project-a");
    const loadA = loadVariants();
    aPreview.resolve(emptyList);
    await new Promise((resolve) => setTimeout(resolve, 0));
    expect(mockGradle).toHaveBeenCalledTimes(1);

    setProject("/projects/project-b", "project-b");
    resetVariantState();
    const loadB = loadVariants();

    aGradle.resolve(projectA);
    await Promise.all([loadA, loadB]);

    expect(mockGradle).toHaveBeenCalledTimes(2);
    expect(variantState.activeVariant).toBe("projectBDebug");
  });
});

describe("loadVariants per application module", () => {
  beforeEach(() => {
    setProject("/projects/two-apps", "two-apps");
    resetVariantState();
    clearVariantCache();
    mockPreview.mockReset();
    mockGradle.mockReset();
    mockPreview.mockResolvedValue({ variants: [], active: null, defaultVariant: null });
    mockGradle.mockImplementation((module: string | null) =>
      Promise.resolve(listFor(module === ":wear" ? "wearDebug" : "mobileDebug"))
    );
  });

  it("asks for the named module's variants and remembers the module", async () => {
    await loadVariants({ module: ":wear" });

    expect(mockPreview).toHaveBeenCalledWith(":wear");
    expect(mockGradle).toHaveBeenCalledWith(":wear");
    expect(variantState.module).toBe(":wear");
    expect(variantState.activeVariant).toBe("wearDebug");
  });

  it("asks for the project's only module when none is named", async () => {
    await loadVariants();

    expect(mockGradle).toHaveBeenCalledWith(null);
    expect(variantState.module).toBeNull();
  });

  it("caches each module's variants separately", async () => {
    await loadVariants({ module: ":wear" });
    await loadVariants({ module: ":mobile" });
    expect(mockGradle).toHaveBeenCalledTimes(2);
    expect(variantState.activeVariant).toBe("mobileDebug");

    resetVariantState();
    await loadVariants({ module: ":wear" });
    expect(mockGradle).toHaveBeenCalledTimes(2);
    expect(variantState.variants.map((v) => v.name)).toEqual(["wearDebug"]);

    // Without a module, the last one loaded is reloaded.
    await loadVariants({ force: true });
    expect(mockGradle).toHaveBeenLastCalledWith(":wear");
  });
});

describe("loadVariants defaultVariant", () => {
  beforeEach(() => {
    setProject("/projects/flavors", "flavors");
    resetVariantState();
    clearVariantCache();
    mockPreview.mockReset();
    mockGradle.mockReset();
  });

  const paidDebug: BuildVariant = {
    name: "paidDebug",
    buildType: "debug",
    flavors: ["paid"],
    assembleTask: "assemblePaidDebug",
    installTask: "installPaidDebug",
  };
  const freeDebug: BuildVariant = {
    name: "freeDebug",
    buildType: "debug",
    flavors: ["free"],
    assembleTask: "assembleFreeDebug",
    installTask: "installFreeDebug",
  };

  const dualList: VariantList = {
    variants: [paidDebug, freeDebug],
    active: null,
    defaultVariant: "freeDebug",
  };

  it("selects defaultVariant when it is not the first entry", async () => {
    mockPreview.mockResolvedValue(dualList);
    mockGradle.mockResolvedValue(dualList);
    await loadVariants();
    expect(variantState.activeVariant).toBe("freeDebug");
  });
});

describe("loadVariants in Safe Mode", () => {
  beforeEach(() => {
    setProject("/projects/untrusted", "untrusted");
    resetVariantState();
    clearVariantCache();
    mockPreview.mockReset();
    mockGradle.mockReset();
    mockPreview.mockResolvedValue(sampleList);
    mockGradle.mockResolvedValue(listFor("fromGradle"));
    mockTrusted.mockReturnValue(false);
  });

  afterEach(() => {
    mockTrusted.mockReturnValue(true);
  });

  it("shows the static preview and never runs Gradle", async () => {
    await loadVariants();
    await loadVariants({ force: true });

    expect(mockGradle).not.toHaveBeenCalled();
    expect(variantState.variants.map((v) => v.name)).toEqual(["debug"]);
    expect(variantState.fromGradle).toBe(false);
    expect(variantState.gradleLoading).toBe(false);
    expect(variantState.gradleError).toBeNull();
    expect(variantState.error).toBeNull();
  });

  it("runs Gradle once the project is trusted, even while a Safe Mode load is pending", async () => {
    const preview = deferred<VariantList>();
    mockPreview.mockImplementationOnce(() => preview.promise);
    const safeLoad = loadVariants();

    mockTrusted.mockReturnValue(true);
    const trustedLoad = loadVariants();
    preview.resolve(sampleList);
    await Promise.all([safeLoad, trustedLoad]);

    expect(mockGradle).toHaveBeenCalledTimes(1);
    expect(variantState.fromGradle).toBe(true);
  });
});
