import { beforeEach, describe, expect, it, vi } from "vitest";
import { invoke } from "@tauri-apps/api/core";
import { open } from "@tauri-apps/plugin-dialog";
import { openProjectFolder } from "@/services/project.service";
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
});
