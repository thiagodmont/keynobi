import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { invoke } from "@tauri-apps/api/core";
import { runAndDeploy, runBuild, resetBuildServiceForTests } from "@/services/build.service";
import { buildState, resetBuildState } from "@/stores/build.store";
import { setProject, setProjectState } from "@/stores/project.store";
import { setProjects } from "@/stores/projects.store";
import type { ProjectEntry } from "@/bindings";

const mockInvoke = vi.mocked(invoke);

function openProject(trusted: boolean | null): void {
  const entry: ProjectEntry = {
    id: "p1",
    path: "/projects/app",
    name: "app",
    gradleRoot: "/projects/app",
    lastOpened: "2026-01-01T00:00:00Z",
    pinned: false,
    lastBuildVariant: "debug",
    lastDevice: null,
    trusted,
  };
  setProject(entry.path, entry.name);
  setProjects([entry]);
}

describe("builds in Safe Mode", () => {
  beforeEach(() => {
    resetBuildState();
    resetBuildServiceForTests();
    vi.clearAllMocks();
  });

  afterEach(() => {
    setProjects([]);
    setProjectState({ projectRoot: null, gradleRoot: null, projectName: null, loading: false });
  });

  for (const trusted of [false, null]) {
    it(`refuses to build or run an untrusted project (trusted: ${trusted})`, async () => {
      openProject(trusted);

      await expect(runBuild()).rejects.toThrow("Safe Mode — trust this project to build");
      await expect(runBuild("clean")).rejects.toThrow("Trust Project");
      await expect(runAndDeploy()).rejects.toThrow("Safe Mode");

      expect(mockInvoke.mock.calls.map(([cmd]) => cmd)).not.toContain("run_gradle_task");
      expect(buildState.phase).toBe("idle");
    });
  }

  it("starts a build for a trusted project", async () => {
    openProject(true);
    mockInvoke.mockRejectedValueOnce("stop here");

    await expect(runBuild()).rejects.toBe("stop here");

    expect(mockInvoke.mock.calls.map(([cmd]) => cmd)).toContain("run_gradle_task");
  });
});
