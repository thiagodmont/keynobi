import { beforeEach, describe, expect, it, vi } from "vitest";
import { invoke } from "@tauri-apps/api/core";
import { open } from "@tauri-apps/plugin-dialog";
import { openProjectFolder, selectProject } from "@/services/project.service";
import { projectState, setProjectState } from "@/stores/project.store";
import { projectsState, setActiveProjectId, setProjects } from "@/stores/projects.store";
import { resetBuildState } from "@/stores/build.store";
import { resetDeviceState } from "@/stores/device.store";
import { resetVariantState } from "@/stores/variant.store";
import type { ProjectEntry } from "@/bindings";

const mockInvoke = vi.mocked(invoke);
const mockOpen = vi.mocked(open);

function resetProjectState(): void {
  setProjectState({
    projectRoot: null,
    gradleRoot: null,
    projectName: null,
    applicationId: null,
    loading: false,
  });
  setProjects([]);
  setActiveProjectId(null);
  resetBuildState();
  resetDeviceState();
  resetVariantState();
}

describe("project.service", () => {
  beforeEach(() => {
    resetProjectState();
    vi.clearAllMocks();
  });

  it("uses the canonical backend project root when matching registry entries", async () => {
    const aliasPath = "/projects/link-to-app";
    const canonicalPath = "/projects/real-app";
    const entry: ProjectEntry = {
      id: "project-1",
      path: canonicalPath,
      name: "real-app",
      gradleRoot: canonicalPath,
      lastOpened: "2026-01-01T00:00:00Z",
      pinned: false,
      lastBuildVariant: null,
      lastDevice: null,
    };

    mockOpen.mockResolvedValue(aliasPath);
    mockInvoke.mockImplementation((command) => {
      switch (command) {
        case "open_project":
          return Promise.resolve("real-app");
        case "get_project_root":
          return Promise.resolve(canonicalPath);
        case "get_gradle_root":
          return Promise.resolve(canonicalPath);
        case "get_application_id":
          return Promise.resolve(null);
        case "list_projects":
          return Promise.resolve([entry]);
        case "get_build_history":
          return Promise.resolve([]);
        case "refresh_devices":
        case "list_avd_devices":
          return Promise.resolve([]);
        case "get_variants_preview":
        case "get_variants_from_gradle":
          return Promise.resolve({ variants: [], active: null, defaultVariant: null });
        default:
          return Promise.resolve(undefined);
      }
    });

    const result = await openProjectFolder();

    expect(result?.root).toBe(canonicalPath);
    expect(projectState.projectRoot).toBe(canonicalPath);
    expect(projectsState.activeProjectId).toBe("project-1");
  });
  it("does not let a superseded project open write to the store", async () => {
    const slow = "/projects/slow";
    const fast = "/projects/fast";
    const entryFor = (path: string, id: string): ProjectEntry => ({
      id,
      path,
      name: id,
      gradleRoot: path,
      lastOpened: "2026-01-01T00:00:00Z",
      pinned: false,
      lastBuildVariant: null,
      lastDevice: null,
    });

    let releaseSlow: (v: string) => void = () => {};
    const slowOpen = new Promise<string>((r) => {
      releaseSlow = r;
    });
    let openCalls = 0;

    mockInvoke.mockImplementation((command, args) => {
      switch (command) {
        case "open_project": {
          openCalls += 1;
          // First caller (slow project) blocks; second resolves immediately.
          return openCalls === 1 ? slowOpen : Promise.resolve("fast");
        }
        case "get_project_root":
          return Promise.resolve(openCalls === 1 ? slow : fast);
        case "get_gradle_root":
          return Promise.resolve(fast);
        case "get_application_id":
          return Promise.resolve(null);
        case "list_projects":
          return Promise.resolve([entryFor(fast, "fast-id")]);
        case "get_build_history":
          return Promise.resolve([]);
        case "refresh_devices":
        case "list_avd_devices":
          return Promise.resolve([]);
        case "get_variants_preview":
        case "get_variants_from_gradle":
          return Promise.resolve({ variants: [], active: null, defaultVariant: null });
        default:
          void args;
          return Promise.resolve(undefined);
      }
    });

    mockOpen.mockResolvedValueOnce(slow);
    const firstOpen = openProjectFolder();

    mockOpen.mockResolvedValueOnce(fast);
    await openProjectFolder();

    // The slow open finishes last but must not clobber the winner.
    releaseSlow("slow");
    await firstOpen;

    expect(projectState.projectRoot).toBe(fast);
  });
  it("does not restore a superseded project's variant into the newer project", async () => {
    const entryFor = (path: string, id: string, variant: string): ProjectEntry => ({
      id,
      path,
      name: id,
      gradleRoot: path,
      lastOpened: "2026-01-01T00:00:00Z",
      pinned: false,
      lastBuildVariant: variant,
      lastDevice: null,
    });
    const first = entryFor("/projects/first", "first-id", "release");
    const second = entryFor("/projects/second", "second-id", "debug");
    const variants = {
      variants: [{ name: "debug" }, { name: "release" }],
      active: null,
      defaultVariant: null,
    };

    let backendRoot = "";
    let releaseFirstList: () => void = () => {};
    let listCalls = 0;
    mockInvoke.mockImplementation((command, args) => {
      switch (command) {
        case "open_project":
          backendRoot = (args as { path: string }).path;
          return Promise.resolve(backendRoot);
        case "get_project_root":
        case "get_gradle_root":
          return Promise.resolve(backendRoot);
        case "get_application_id":
          return Promise.resolve(null);
        case "list_projects": {
          listCalls += 1;
          // The first project's registry read is slow.
          if (listCalls === 1) {
            return new Promise((resolve) => {
              releaseFirstList = () => resolve([first, second]);
            });
          }
          return Promise.resolve([first, second]);
        }
        case "get_variants_preview":
        case "get_variants_from_gradle":
          return Promise.resolve(variants);
        case "get_build_history":
        case "refresh_devices":
        case "list_avd_devices":
          return Promise.resolve([]);
        default:
          return Promise.resolve(undefined);
      }
    });

    const firstSelect = selectProject(first);
    await vi.waitFor(() => expect(listCalls).toBe(1));
    await selectProject(second);

    releaseFirstList();
    await firstSelect;

    const variantWrites = mockInvoke.mock.calls
      .filter(([cmd]) => cmd === "set_active_variant")
      .map(([, args]) => (args as { variant: string }).variant);
    expect(variantWrites).not.toContain("release");
    expect(projectsState.activeProjectId).toBe("second-id");
    expect(projectState.projectRoot).toBe("/projects/second");
  });
});
