import { describe, it, expect, beforeEach } from "vitest";
import {
  projectsState,
  setProjects,
  setActiveProjectId,
  setProjectsLoading,
  upsertProject,
  removeProjectFromStore,
  setPinned,
  renameProjectInStore,
  updateProjectMetaInStore,
} from "@/stores/projects.store";
import type { ProjectEntry } from "@/bindings";

function entry(over: Partial<ProjectEntry> & { id: string }): ProjectEntry {
  return {
    id: over.id,
    path: over.path ?? `/projects/${over.id}`,
    name: over.name ?? over.id,
    gradleRoot: over.gradleRoot ?? null,
    lastOpened: over.lastOpened ?? "2026-01-01T00:00:00Z",
    pinned: over.pinned ?? false,
    lastBuildVariant: over.lastBuildVariant ?? null,
    lastDevice: over.lastDevice ?? null,
  };
}

describe("projects.store", () => {
  beforeEach(() => {
    setProjects([]);
    setActiveProjectId(null);
    setProjectsLoading(false);
  });

  it("keeps pinned projects above unpinned ones after an upsert", () => {
    setProjects([entry({ id: "pinned", pinned: true, lastOpened: "2026-01-01T00:00:00Z" })]);

    // A freshly opened unpinned project must not jump above a pinned one.
    upsertProject(entry({ id: "fresh", lastOpened: "2026-06-01T00:00:00Z" }));

    expect(projectsState.projects.map((p) => p.id)).toEqual(["pinned", "fresh"]);
  });

  it("orders unpinned projects by most recently opened", () => {
    upsertProject(entry({ id: "old", lastOpened: "2026-01-01T00:00:00Z" }));
    upsertProject(entry({ id: "new", lastOpened: "2026-06-01T00:00:00Z" }));

    expect(projectsState.projects.map((p) => p.id)).toEqual(["new", "old"]);
  });

  it("replaces an existing entry rather than duplicating it", () => {
    upsertProject(entry({ id: "a", name: "first" }));
    upsertProject(entry({ id: "a", name: "second" }));

    expect(projectsState.projects).toHaveLength(1);
    expect(projectsState.projects[0].name).toBe("second");
  });

  it("re-sorts when a project is pinned", () => {
    setProjects([
      entry({ id: "a", lastOpened: "2026-06-01T00:00:00Z" }),
      entry({ id: "b", lastOpened: "2026-01-01T00:00:00Z" }),
    ]);

    setPinned("b", true);

    expect(projectsState.projects.map((p) => p.id)).toEqual(["b", "a"]);
  });

  it("removes a project by id", () => {
    setProjects([entry({ id: "a" }), entry({ id: "b" })]);
    removeProjectFromStore("a");
    expect(projectsState.projects.map((p) => p.id)).toEqual(["b"]);
  });

  it("renames a project in place", () => {
    setProjects([entry({ id: "a", name: "old" })]);
    renameProjectInStore("a", "new");
    expect(projectsState.projects[0].name).toBe("new");
  });

  it("updates per-project meta", () => {
    setProjects([entry({ id: "a" })]);
    updateProjectMetaInStore("a", "debug", "emulator-5554");
    expect(projectsState.projects[0].lastBuildVariant).toBe("debug");
    expect(projectsState.projects[0].lastDevice).toBe("emulator-5554");
  });

  it("ignores meta updates for unknown ids", () => {
    setProjects([entry({ id: "a" })]);
    updateProjectMetaInStore("missing", "debug", null);
    expect(projectsState.projects[0].lastBuildVariant).toBeNull();
  });
});
