import { describe, it, expect, beforeEach } from "vitest";
import {
  projectState,
  setProjectState,
  setProject,
  clearProject,
  setLoading,
} from "./project.store";

function resetState() {
  setProjectState({ projectRoot: null, gradleRoot: null, projectName: null, loading: false });
}

// ── setProject ────────────────────────────────────────────────────────────────

describe("setProject", () => {
  beforeEach(resetState);

  it("sets the project root and name", () => {
    setProject("/projects/my-app", "my-app");
    expect(projectState.projectRoot).toBe("/projects/my-app");
    expect(projectState.projectName).toBe("my-app");
    expect(projectState.loading).toBe(false);
  });

  it("derives the project name from the last path segment when given an object", () => {
    setProject("/home/user/code/cool-project", {});
    expect(projectState.projectName).toBe("cool-project");
  });

  it("stores gradleRoot when provided", () => {
    setProject("/projects/the-crazy-project/the-crazy-app", "the-crazy-app", "/projects/the-crazy-project");
    expect(projectState.projectRoot).toBe("/projects/the-crazy-project/the-crazy-app");
    expect(projectState.gradleRoot).toBe("/projects/the-crazy-project");
  });

  it("sets gradleRoot to null when not provided", () => {
    setProject("/projects/my-app", "my-app");
    expect(projectState.gradleRoot).toBeNull();
  });
});

// ── clearProject ──────────────────────────────────────────────────────────────

describe("clearProject", () => {
  beforeEach(resetState);

  it("resets all project state to null/false", () => {
    setProject("/projects/my-app", "my-app", "/projects");
    clearProject();
    expect(projectState.projectRoot).toBeNull();
    expect(projectState.gradleRoot).toBeNull();
    expect(projectState.projectName).toBeNull();
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
