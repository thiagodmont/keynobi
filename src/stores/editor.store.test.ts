import { describe, it, expect, beforeEach } from "vitest";
import {
  editorState,
  addOpenFile,
  removeOpenFile,
  setActiveFile,
  markDirty,
  markClean,
  updateSavedContent,
  isFileOpen,
} from "./editor.store";
import type { OpenFile } from "./editor.store";

// Helper: build a minimal OpenFile record.
function makeFile(path: string, name = path.split("/").pop()!): OpenFile {
  return {
    path,
    name,
    savedContent: "initial content",
    dirty: false,
    editorState: null,
    language: "kotlin",
  };
}

// Reset store state between tests by removing all open files.
function clearStore() {
  [...editorState.tabOrder].forEach((p) => removeOpenFile(p));
  setActiveFile(null);
}

// ── isFileOpen ────────────────────────────────────────────────────────────────

describe("isFileOpen", () => {
  beforeEach(clearStore);

  it("returns false when file is not open", () => {
    expect(isFileOpen("/foo.kt")).toBe(false);
  });

  it("returns true after addOpenFile", () => {
    addOpenFile(makeFile("/foo.kt"));
    expect(isFileOpen("/foo.kt")).toBe(true);
  });
});

// ── addOpenFile ───────────────────────────────────────────────────────────────

describe("addOpenFile", () => {
  beforeEach(clearStore);

  it("adds file to openFiles and tabOrder", () => {
    addOpenFile(makeFile("/a.kt"));
    expect(editorState.openFiles["/a.kt"]).toBeDefined();
    expect(editorState.tabOrder).toContain("/a.kt");
  });

  it("does not add duplicates to tabOrder", () => {
    addOpenFile(makeFile("/a.kt"));
    addOpenFile(makeFile("/a.kt")); // same path again
    expect(editorState.tabOrder.filter((p) => p === "/a.kt")).toHaveLength(1);
  });

  it("tracks recent files", () => {
    addOpenFile(makeFile("/a.kt"));
    expect(editorState.recentFiles).toContain("/a.kt");
  });
});

// ── removeOpenFile ────────────────────────────────────────────────────────────

describe("removeOpenFile", () => {
  beforeEach(clearStore);

  it("removes the file from openFiles and tabOrder", () => {
    addOpenFile(makeFile("/a.kt"));
    removeOpenFile("/a.kt");
    expect(isFileOpen("/a.kt")).toBe(false);
    expect(editorState.tabOrder).not.toContain("/a.kt");
  });

  it("handles removal of a path not in tabOrder without corrupting the array", () => {
    addOpenFile(makeFile("/a.kt"));
    addOpenFile(makeFile("/b.kt"));
    // Manually corrupt: remove from tabOrder but not openFiles to simulate edge case
    removeOpenFile("/nonexistent.kt"); // should not throw or splice wrong index
    expect(editorState.tabOrder).toHaveLength(2);
  });

  it("activates the tab to the right when closing the active tab", () => {
    addOpenFile(makeFile("/a.kt"));
    addOpenFile(makeFile("/b.kt"));
    addOpenFile(makeFile("/c.kt"));
    setActiveFile("/b.kt");
    removeOpenFile("/b.kt");
    // Should activate /c.kt (next to the right)
    expect(editorState.activeFilePath).toBe("/c.kt");
  });

  it("activates the tab to the left when closing the last tab", () => {
    addOpenFile(makeFile("/a.kt"));
    addOpenFile(makeFile("/b.kt"));
    setActiveFile("/b.kt");
    removeOpenFile("/b.kt");
    expect(editorState.activeFilePath).toBe("/a.kt");
  });

  it("sets activeFilePath to null when closing the only tab", () => {
    addOpenFile(makeFile("/a.kt"));
    setActiveFile("/a.kt");
    removeOpenFile("/a.kt");
    expect(editorState.activeFilePath).toBeNull();
  });
});

// ── dirty / clean ─────────────────────────────────────────────────────────────

describe("markDirty / markClean", () => {
  beforeEach(clearStore);

  it("marks a file as dirty", () => {
    addOpenFile(makeFile("/a.kt"));
    markDirty("/a.kt");
    expect(editorState.openFiles["/a.kt"].dirty).toBe(true);
  });

  it("marks a dirty file as clean", () => {
    addOpenFile(makeFile("/a.kt"));
    markDirty("/a.kt");
    markClean("/a.kt");
    expect(editorState.openFiles["/a.kt"].dirty).toBe(false);
  });

  it("is a no-op for a file that is not open", () => {
    expect(() => markDirty("/notopen.kt")).not.toThrow();
    expect(() => markClean("/notopen.kt")).not.toThrow();
  });
});

// ── updateSavedContent ────────────────────────────────────────────────────────

describe("updateSavedContent", () => {
  beforeEach(clearStore);

  it("updates savedContent and clears dirty flag", () => {
    addOpenFile(makeFile("/a.kt"));
    markDirty("/a.kt");
    updateSavedContent("/a.kt", "new content");
    expect(editorState.openFiles["/a.kt"].savedContent).toBe("new content");
    expect(editorState.openFiles["/a.kt"].dirty).toBe(false);
  });
});

// ── setActiveFile ─────────────────────────────────────────────────────────────

describe("setActiveFile", () => {
  beforeEach(clearStore);

  it("sets the activeFilePath", () => {
    addOpenFile(makeFile("/a.kt"));
    setActiveFile("/a.kt");
    expect(editorState.activeFilePath).toBe("/a.kt");
  });

  it("updates activeLanguage from the file", () => {
    addOpenFile(makeFile("/a.kt"));
    setActiveFile("/a.kt");
    expect(editorState.activeLanguage).toBe("kotlin");
  });

  it("clears cursor and language when set to null", () => {
    addOpenFile(makeFile("/a.kt"));
    setActiveFile("/a.kt");
    setActiveFile(null);
    expect(editorState.activeFilePath).toBeNull();
    expect(editorState.activeLanguage).toBeNull();
    expect(editorState.cursorLine).toBeNull();
    expect(editorState.cursorCol).toBeNull();
  });
});
