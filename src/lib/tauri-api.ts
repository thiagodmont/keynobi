import { invoke } from "@tauri-apps/api/core";
import { open } from "@tauri-apps/plugin-dialog";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import type { FileNode } from "@/stores/project.store";

// ── File System ──────────────────────────────────────────────────────────────

export async function openFolderDialog(): Promise<string | null> {
  const result = await open({
    directory: true,
    multiple: false,
    title: "Open Android Project",
  });
  if (Array.isArray(result)) return result[0] ?? null;
  return result as string | null;
}

export async function openProject(path: string): Promise<FileNode> {
  return invoke<FileNode>("open_project", { path });
}

export async function getFileTree(): Promise<FileNode> {
  return invoke<FileNode>("get_file_tree");
}

export async function getDirectoryChildren(path: string): Promise<FileNode[]> {
  return invoke<FileNode[]>("get_directory_children", { path });
}

export async function readFile(path: string): Promise<string> {
  return invoke<string>("read_file", { path });
}

export async function writeFile(path: string, content: string): Promise<void> {
  return invoke<void>("write_file", { path, content });
}

export async function createFile(path: string): Promise<void> {
  return invoke<void>("create_file", { path });
}

export async function createDirectory(path: string): Promise<void> {
  return invoke<void>("create_directory", { path });
}

export async function deletePath(path: string): Promise<void> {
  return invoke<void>("delete_path", { path });
}

export async function renamePath(
  oldPath: string,
  newPath: string
): Promise<void> {
  return invoke<void>("rename_path", { oldPath, newPath });
}

export async function getProjectRoot(): Promise<string | null> {
  return invoke<string | null>("get_project_root");
}

// ── File Events ───────────────────────────────────────────────────────────────

export interface FileEvent {
  kind: "created" | "modified" | "deleted" | "renamed";
  path: string;
  newPath?: string;
}

export function onFileChanged(
  callback: (event: FileEvent) => void
): Promise<UnlistenFn> {
  return listen<FileEvent>("file:changed", (e) => callback(e.payload));
}

// ── Error helpers ─────────────────────────────────────────────────────────────

export function formatError(err: unknown): string {
  if (typeof err === "string") return err;
  if (err instanceof Error) return err.message;
  return String(err);
}
