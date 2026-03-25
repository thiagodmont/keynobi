import { describe, it, expect, beforeEach, vi } from "vitest";
import { invoke } from "@tauri-apps/api/core";
import { open } from "@tauri-apps/plugin-dialog";
import { openProjectFolder } from "./project.service";
import { projectState, setProjectState } from "@/stores/project.store";
import type { FileNode } from "@/stores/project.store";

// ── Helpers ───────────────────────────────────────────────────────────────────

function makeTree(name = "my-app", root = "/projects/my-app"): FileNode {
  return {
    name,
    path: root,
    kind: "directory",
    children: [
      { name: "app", path: `${root}/app`, kind: "directory", children: [] },
      {
        name: "settings.gradle.kts",
        path: `${root}/settings.gradle.kts`,
        kind: "file",
        extension: "kts",
      },
    ],
  };
}

function resetProject() {
  setProjectState({ projectRoot: null, projectName: null, fileTree: null, loading: false });
}

// ── openProjectFolder ─────────────────────────────────────────────────────────

describe("openProjectFolder", () => {
  beforeEach(() => {
    resetProject();
    vi.resetAllMocks();
  });

  it("returns null when the user cancels the dialog", async () => {
    vi.mocked(open).mockResolvedValue(null);
    const result = await openProjectFolder();
    expect(result).toBeNull();
    expect(projectState.projectRoot).toBeNull();
  });

  it("calls open_project and updates the store on success", async () => {
    const root = "/projects/my-app";
    const tree = makeTree("my-app", root);
    vi.mocked(open).mockResolvedValue(root);
    vi.mocked(invoke).mockResolvedValue(tree);

    const result = await openProjectFolder();

    expect(result).not.toBeNull();
    expect(result!.root).toBe(root);
    expect(result!.tree).toBe(tree);
    expect(projectState.projectRoot).toBe(root);
    expect(projectState.projectName).toBe("my-app");
    expect(projectState.loading).toBe(false);
  });

  it("returns the top-level directory paths as rootDirs", async () => {
    const root = "/projects/my-app";
    const tree = makeTree("my-app", root);
    vi.mocked(open).mockResolvedValue(root);
    vi.mocked(invoke).mockResolvedValue(tree);

    const result = await openProjectFolder();
    expect(result!.rootDirs).toEqual([`${root}/app`]);
  });

  it("returns null and shows a toast on invoke failure", async () => {
    vi.mocked(open).mockResolvedValue("/some/path");
    vi.mocked(invoke).mockRejectedValue(new Error("Rust error"));

    const result = await openProjectFolder();
    expect(result).toBeNull();
    expect(projectState.projectRoot).toBeNull();
    // loading should be cleared even on error
    expect(projectState.loading).toBe(false);
  });

  it("sets loading=false after the operation completes", async () => {
    const root = "/projects/my-app";
    const tree = makeTree("my-app", root);
    vi.mocked(open).mockResolvedValue(root);
    vi.mocked(invoke).mockResolvedValue(tree);

    await openProjectFolder();

    expect(projectState.loading).toBe(false);
  });
});
