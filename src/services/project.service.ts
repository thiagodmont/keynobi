/**
 * project.service.ts
 *
 * Handles opening, switching, pinning, and removing projects.
 * All project-switching logic lives here so callers don't need to know
 * about build/logcat teardown ordering.
 */

import {
  openFolderDialog,
  openProject,
  formatError,
  getGradleRoot,
  getApplicationId,
  listProjects,
  removeProject as removeProjectApi,
  pinProject as pinProjectApi,
  getLastActiveProject,
} from "@/lib/tauri-api";
import { setProject, setLoading, setApplicationId } from "@/stores/project.store";
import {
  setProjects,
  setActiveProjectId,
  upsertProject,
  removeProjectFromStore,
  setPinned,
  setProjectsLoading,
} from "@/stores/projects.store";
import { showToast } from "@/components/common/Toast";
import { initBuildService } from "@/services/build.service";
import { cancelBuild } from "@/services/build.service";
import { clearBuild } from "@/stores/build.store";
import { initDevices } from "@/stores/device.store";
import { stopLogcat } from "@/lib/tauri-api";
import { setMinePackage } from "@/lib/logcat-query";
import type { ProjectEntry } from "@/bindings";

export interface OpenProjectResult {
  root: string;
  projectName: string;
}

// ── Core open logic (shared by openProjectFolder and switchProject) ────────────

async function doOpenProject(path: string): Promise<OpenProjectResult | null> {
  setLoading(true);
  try {
    const projectName = await openProject(path);
    const gradleRoot = await getGradleRoot().catch(() => null);
    setProject(path, projectName, gradleRoot);

    // Resolve applicationId for `package:mine` filter.
    const appId = await getApplicationId().catch(() => null);
    setApplicationId(appId);
    setMinePackage(appId);

    // Re-initialize build service and devices for the new project.
    initBuildService().catch(console.error);
    initDevices().catch(console.error);

    return { root: path, projectName };
  } catch (err) {
    showToast(`Failed to open project: ${formatError(err)}`, "error");
    return null;
  } finally {
    setLoading(false);
  }
}

// ── Public API ────────────────────────────────────────────────────────────────

/**
 * Show the native folder picker, call the Rust `open_project` command,
 * update the project store, and return metadata the caller may need.
 *
 * Returns `null` when the user cancels the dialog or an error occurs.
 */
export async function openProjectFolder(): Promise<OpenProjectResult | null> {
  const path = await openFolderDialog();
  if (!path) return null;

  const result = await doOpenProject(path);
  if (result) {
    // Refresh the projects list so the new entry shows in the switcher.
    await refreshProjectsList().catch(console.error);
    // Mark this project as active in the registry store.
    const projects = (await listProjects().catch(() => [])) as ProjectEntry[];
    const entry = projects.find((p) => p.path === path);
    if (entry) {
      upsertProject(entry);
      setActiveProjectId(entry.id);
    }
  }
  return result;
}

/**
 * Switch the active project to the given registry entry.
 * Gracefully cancels any running build, stops logcat, clears state,
 * then opens the new project.
 */
export async function switchProject(entry: ProjectEntry): Promise<void> {
  // Teardown current project state.
  try {
    await cancelBuild();
  } catch {
    // Ignore — no build in progress.
  }
  try {
    await stopLogcat();
  } catch {
    // Ignore — logcat may not be running.
  }
  clearBuild();

  const result = await doOpenProject(entry.path);
  if (result) {
    // Re-read the entry from backend so lastOpened is fresh.
    const projects = (await listProjects().catch(() => [])) as ProjectEntry[];
    const fresh = projects.find((p) => p.id === entry.id);
    if (fresh) {
      upsertProject(fresh);
    }
    setActiveProjectId(entry.id);
    setProjects(projects);
  }
}

/**
 * Load the project registry into the store.
 * Call once on app startup.
 */
export async function refreshProjectsList(): Promise<void> {
  setProjectsLoading(true);
  try {
    const projects = await listProjects();
    setProjects(projects);
  } catch (err) {
    console.error("Failed to load projects list:", err);
  } finally {
    setProjectsLoading(false);
  }
}

/**
 * Restore the last-active project on startup.
 * Returns true if a project was restored.
 */
export async function restoreLastProject(): Promise<boolean> {
  try {
    const lastPath = await getLastActiveProject();
    if (!lastPath) return false;

    const result = await doOpenProject(lastPath);
    if (result) {
      const projects = (await listProjects().catch(() => [])) as ProjectEntry[];
      const entry = projects.find((p) => p.path === lastPath);
      if (entry) {
        setActiveProjectId(entry.id);
      }
      setProjects(projects);
      return true;
    }
  } catch (err) {
    console.error("Failed to restore last project:", err);
  }
  return false;
}

/**
 * Remove a project from the registry (does not delete from disk).
 */
export async function removeProjectEntry(id: string): Promise<void> {
  try {
    await removeProjectApi(id);
    removeProjectFromStore(id);
  } catch (err) {
    showToast(`Failed to remove project: ${formatError(err)}`, "error");
  }
}

/**
 * Toggle the pinned flag for a project.
 */
export async function togglePinProject(id: string, pinned: boolean): Promise<void> {
  try {
    await pinProjectApi(id, pinned);
    setPinned(id, pinned);
  } catch (err) {
    showToast(`Failed to pin project: ${formatError(err)}`, "error");
  }
}
