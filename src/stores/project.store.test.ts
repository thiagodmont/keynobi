import { describe, it, expect, beforeEach } from "vitest";
import {
  projectState,
  setProjectState,
  setProject,
  clearProject,
  setLoading,
  type FileNode,
} from "./project.store";

function makeTree(): FileNode {
  return {
    name: "my-app",
    path: "/projects/my-app",
    kind: "directory",
    children: [
      { name: "app", path: "/projects/my-app/app", kind: "directory", children: [] },
      {
        name: "settings.gradle.kts",
        path: "/projects/my-app/settings.gradle.kts",
        kind: "file",
        extension: "kts",
      },
    ],
  };
}

function resetState() {
  setProjectState({ projectRoot: null, projectName: null, fileTree: null, loading: false });
}

// ── setProject ────────────────────────────────────────────────────────────────

describe("setProject", () => {
  beforeEach(resetState);

  it("sets the project root, name, and tree", () => {
    const tree = makeTree();
    setProject("/projects/my-app", tree);
    expect(projectState.projectRoot).toBe("/projects/my-app");
    expect(projectState.projectName).toBe("my-app");
    expect(projectState.fileTree).toStrictEqual(tree);
    expect(projectState.loading).toBe(false);
  });

  it("derives the project name from the last path segment", () => {
    setProject("/home/user/code/cool-project", makeTree());
    expect(projectState.projectName).toBe("cool-project");
  });

  it("handles a root path gracefully", () => {
    setProject("/my-project", makeTree());
    expect(projectState.projectName).toBe("my-project");
  });
});

// ── clearProject ──────────────────────────────────────────────────────────────

describe("clearProject", () => {
  beforeEach(resetState);

  it("resets all project state to null/false", () => {
    setProject("/projects/my-app", makeTree());
    clearProject();
    expect(projectState.projectRoot).toBeNull();
    expect(projectState.projectName).toBeNull();
    expect(projectState.fileTree).toBeNull();
    expect(projectState.loading).toBe(false);
  });
});

// ── setLoading ────────────────────────────────────────────────────────────────

describe("setLoading", () => {
  beforeEach(resetState);

  it("sets loading to true", () => {
    setLoading(true);
    expect(projectState.loading).toBe(true);
  });

  it("sets loading to false", () => {
    setLoading(true);
    setLoading(false);
    expect(projectState.loading).toBe(false);
  });
});
