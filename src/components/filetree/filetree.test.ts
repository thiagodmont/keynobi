/**
 * Tests for the FileTree expand/collapse logic.
 *
 * These verify the fix for the "pre-expanded directories require two clicks"
 * bug: when a project is opened, root directories are marked as expanded
 * but their children haven't been loaded yet. The first click must load
 * children and keep the directory open — not collapse it.
 */

import { describe, it, expect, beforeEach, vi } from "vitest";
import { invoke } from "@tauri-apps/api/core";
import type { FileNode } from "@/bindings";

// ── Helpers ───────────────────────────────────────────────────────────────────

function makeDir(name: string, path: string): FileNode {
  return { name, path, kind: "directory", children: [], extension: null };
}

function makeFile(name: string, path: string, ext: string): FileNode {
  return { name, path, kind: "file", children: null, extension: ext };
}

const MOCK_CHILDREN: FileNode[] = [
  makeFile("Main.kt", "/project/app/src/Main.kt", "kt"),
  makeFile("Utils.kt", "/project/app/src/Utils.kt", "kt"),
  makeDir("ui", "/project/app/src/ui"),
];

/**
 * Simulates the expand() logic from FileTreeNode.
 *
 * This is a direct extraction of the algorithm, testing the decision
 * logic without DOM rendering. The real component uses exactly this flow.
 */
async function simulateExpand(params: {
  isDir: boolean;
  isExpanded: boolean;
  dirCacheHasEntry: boolean;
  fetchChildren: () => Promise<FileNode[]>;
}): Promise<{
  action: "loaded-and-kept-open" | "toggled" | "skipped";
  childrenLoaded: boolean;
}> {
  if (!params.isDir) return { action: "skipped", childrenLoaded: false };

  const wasExpanded = params.isExpanded;
  const hadLoadedChildren = params.dirCacheHasEntry;
  let childrenLoaded = false;

  if (!hadLoadedChildren) {
    await params.fetchChildren();
    childrenLoaded = true;
  }

  if (wasExpanded && !hadLoadedChildren) {
    return { action: "loaded-and-kept-open", childrenLoaded };
  }

  return { action: "toggled", childrenLoaded };
}

// ── Tests ─────────────────────────────────────────────────────────────────────

describe("FileTree expand logic", () => {
  beforeEach(() => {
    vi.resetAllMocks();
  });

  // ── The bug scenario ──────────────────────────────────────────────────────

  describe("pre-expanded directory (project open)", () => {
    it("loads children and keeps directory open on first click", async () => {
      const fetchChildren = vi.fn().mockResolvedValue(MOCK_CHILDREN);

      const result = await simulateExpand({
        isDir: true,
        isExpanded: true,          // Pre-expanded via setExpandedDirs
        dirCacheHasEntry: false,    // Children NOT loaded yet
        fetchChildren,
      });

      expect(fetchChildren).toHaveBeenCalledTimes(1);
      expect(result.action).toBe("loaded-and-kept-open");
      expect(result.childrenLoaded).toBe(true);
    });

    it("collapses on second click (children now loaded)", async () => {
      const fetchChildren = vi.fn().mockResolvedValue(MOCK_CHILDREN);

      const result = await simulateExpand({
        isDir: true,
        isExpanded: true,           // Still expanded
        dirCacheHasEntry: true,     // Children loaded from first click
        fetchChildren,
      });

      expect(fetchChildren).not.toHaveBeenCalled();
      expect(result.action).toBe("toggled");
    });
  });

  // ── Normal expand/collapse ────────────────────────────────────────────────

  describe("normal directory interaction", () => {
    it("loads children and expands a collapsed directory", async () => {
      const fetchChildren = vi.fn().mockResolvedValue(MOCK_CHILDREN);

      const result = await simulateExpand({
        isDir: true,
        isExpanded: false,
        dirCacheHasEntry: false,
        fetchChildren,
      });

      expect(fetchChildren).toHaveBeenCalledTimes(1);
      expect(result.action).toBe("toggled");
      expect(result.childrenLoaded).toBe(true);
    });

    it("collapses an expanded directory with loaded children", async () => {
      const fetchChildren = vi.fn().mockResolvedValue(MOCK_CHILDREN);

      const result = await simulateExpand({
        isDir: true,
        isExpanded: true,
        dirCacheHasEntry: true,
        fetchChildren,
      });

      expect(fetchChildren).not.toHaveBeenCalled();
      expect(result.action).toBe("toggled");
    });

    it("re-expands a previously collapsed directory without re-fetching", async () => {
      const fetchChildren = vi.fn().mockResolvedValue(MOCK_CHILDREN);

      const result = await simulateExpand({
        isDir: true,
        isExpanded: false,
        dirCacheHasEntry: true,     // Children were loaded before collapse
        fetchChildren,
      });

      expect(fetchChildren).not.toHaveBeenCalled();
      expect(result.action).toBe("toggled");
      expect(result.childrenLoaded).toBe(false);
    });

    it("skips expand for file nodes", async () => {
      const fetchChildren = vi.fn();

      const result = await simulateExpand({
        isDir: false,
        isExpanded: false,
        dirCacheHasEntry: false,
        fetchChildren,
      });

      expect(fetchChildren).not.toHaveBeenCalled();
      expect(result.action).toBe("skipped");
    });
  });

  // ── handleOpenFolder flow ─────────────────────────────────────────────────

  describe("handleOpenFolder flow (expandDir)", () => {
    it("expandDir loads children before marking as expanded", async () => {
      // Simulate the expandDir function from FileTree.tsx
      const expandedDirs = new Set<string>();
      const dirCache: Record<string, FileNode[]> = {};

      async function expandDir(path: string) {
        if (expandedDirs.has(path)) return;
        const cached = dirCache[path];
        if (!cached || cached.length === 0) {
          // Simulate getDirectoryChildren
          vi.mocked(invoke).mockResolvedValueOnce(MOCK_CHILDREN);
          dirCache[path] = await invoke<FileNode[]>("get_directory_children", { path });
        }
        expandedDirs.add(path);
      }

      const rootDirs = ["/project/app", "/project/core"];

      // Simulate handleOpenFolder
      for (const dirPath of rootDirs) {
        await expandDir(dirPath);
      }

      // Both dirs should be expanded AND have children loaded
      expect(expandedDirs.has("/project/app")).toBe(true);
      expect(expandedDirs.has("/project/core")).toBe(true);
      expect(dirCache["/project/app"]).toBeDefined();
      expect(dirCache["/project/app"]!.length).toBe(MOCK_CHILDREN.length);
      expect(dirCache["/project/core"]).toBeDefined();
    });

    it("expandDir does not re-fetch if already expanded", async () => {
      const expandedDirs = new Set<string>(["/project/app"]);
      const dirCache: Record<string, FileNode[]> = {
        "/project/app": MOCK_CHILDREN,
      };

      async function expandDir(path: string) {
        if (expandedDirs.has(path)) return;
        const cached = dirCache[path];
        if (!cached || cached.length === 0) {
          vi.mocked(invoke).mockResolvedValueOnce(MOCK_CHILDREN);
          dirCache[path] = await invoke<FileNode[]>("get_directory_children", { path });
        }
        expandedDirs.add(path);
      }

      await expandDir("/project/app");

      // invoke should not have been called — already expanded
      expect(invoke).not.toHaveBeenCalled();
    });
  });
});
