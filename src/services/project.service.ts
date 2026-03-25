/**
 * project.service.ts
 *
 * Centralises the "open a project folder" flow so it is not duplicated
 * between App.tsx (Cmd+O keybinding) and FileTree.tsx (sidebar button).
 */

import { openFolderDialog, openProject, formatError } from "@/lib/tauri-api";
import { setProject, setLoading } from "@/stores/project.store";
import { showToast } from "@/components/common/Toast";
import type { FileNode } from "@/stores/project.store";

export interface OpenProjectResult {
  root: string;
  tree: FileNode;
  rootDirs: string[];
}

/**
 * Show the native folder picker, call the Rust `open_project` command,
 * update the project store, and return metadata the caller may need
 * (e.g. which dirs to auto-expand).
 *
 * Returns `null` when the user cancels the dialog or an error occurs.
 */
export async function openProjectFolder(): Promise<OpenProjectResult | null> {
  const path = await openFolderDialog();
  if (!path) return null;

  setLoading(true);
  try {
    const tree = await openProject(path);
    setProject(path, tree);

    const rootDirs = (tree.children ?? [])
      .filter((c) => c.kind === "directory")
      .map((c) => c.path);

    return { root: path, tree, rootDirs };
  } catch (err) {
    showToast(`Failed to open project: ${formatError(err)}`, "error");
    return null;
  } finally {
    setLoading(false);
  }
}
