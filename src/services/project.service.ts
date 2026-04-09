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
  updateProjectMeta,
  renameProject as renameProjectApi,
} from "@/lib/tauri-api";
import { setProject, setLoading, setApplicationId } from "@/stores/project.store";
import {
  setProjects,
  setActiveProjectId,
  upsertProject,
  removeProjectFromStore,
  setPinned,
  setProjectsLoading,
  renameProjectInStore,
  updateProjectMetaInStore,
  projectsState,
} from "@/stores/projects.store";
import { showToast } from "@/components/common/Toast";
import { initBuildService } from "@/services/build.service";
import { cancelBuild } from "@/services/build.service";
import { clearBuild } from "@/stores/build.store";
import { initDevices, pickDevice, onDeviceChange } from "@/stores/device.store";
import { stopLogcat } from "@/lib/tauri-api";
import { setMinePackage } from "@/lib/logcat-query";
import {
  loadVariants,
  onVariantChange,
  resetVariantState,
  selectVariant,
} from "@/stores/variant.store";
import { variantState } from "@/stores/variant.store";
import { deviceState } from "@/stores/device.store";
import { refreshHealthChecks } from "@/stores/health.store";
import type { ProjectEntry } from "@/bindings";

// Register callbacks so stores can notify this service without circular imports.
// This runs once when the module is first imported (hoisted function refs are safe here).
onVariantChange((_variant) => {
  saveActiveProjectMeta().catch(e => { console.error(e); showToast(`Failed to save project state: ${formatError(e)}`, "error"); });
});
onDeviceChange((_serial) => {
  saveActiveProjectMeta().catch(e => { console.error(e); showToast(`Failed to save project state: ${formatError(e)}`, "error"); });
});

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

    // Re-run health checks now that project_root and gradle_root are set in
    // FsState — the Gradle wrapper probe was false at startup because the
    // project wasn't loaded yet.
    refreshHealthChecks().catch(console.error);

    return { root: path, projectName };
  } catch (err) {
    showToast(`Failed to open project: ${formatError(err)}`, "error");
    return null;
  } finally {
    setLoading(false);
  }
}

/**
 * After FsState points at a project, rediscover variants and restore registry selections.
 * Call only after a successful `doOpenProject` so a failed open does not wipe variant state.
 */
async function reloadVariantsAndRestoreMeta(entry: ProjectEntry | null): Promise<void> {
  resetVariantState();
  await loadVariants();
  if (variantState.error) {
    showToast(`Failed to load build variants: ${variantState.error}`, "error");
  }
  const savedVariant = entry?.lastBuildVariant;
  if (
    savedVariant &&
    variantState.variants.some((v) => v.name === savedVariant)
  ) {
    await selectVariant(savedVariant).catch(console.error);
  }
  if (entry?.lastDevice) {
    await pickDevice(entry.lastDevice).catch(console.error);
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
    // Refresh the projects list so the new entry shows in the sidebar.
    await refreshProjectsList().catch(console.error);
    // Mark this project as active in the registry store.
    const projects = (await listProjects().catch(() => [])) as ProjectEntry[];
    const entry = projects.find((p) => p.path === path);
    if (entry) {
      upsertProject(entry);
      setActiveProjectId(entry.id);
    }
    await reloadVariantsAndRestoreMeta(entry ?? null);
  }
  return result;
}

/**
 * Select a project from the sidebar.
 *
 * Lighter than `switchProject` — only updates the build target.
 * Logcat and device state are intentionally NOT touched.
 *
 * 1. Cancel any in-progress build (it belongs to the previous project)
 * 2. Clear build state
 * 3. Open the project (update FsState, reload variants + appId)
 * 4. Restore per-project variant/device selections
 */
export async function selectProject(entry: ProjectEntry): Promise<void> {
  // Cancel build — it targets the old project's Gradle root.
  try {
    await cancelBuild();
  } catch {
    // Ignore — no build in progress.
  }
  clearBuild();

  const result = await doOpenProject(entry.path);
  if (result) {
    const projects = (await listProjects().catch(() => [])) as ProjectEntry[];
    const fresh = projects.find((p) => p.id === entry.id) ?? entry;
    upsertProject(fresh);
    setActiveProjectId(fresh.id);
    setProjects(projects);

    await reloadVariantsAndRestoreMeta(fresh);
  }
}

/**
 * Switch the active project with full teardown (cancel build + stop logcat).
 * Used internally for session restore. For user-initiated sidebar clicks use
 * `selectProject` instead.
 */
export async function switchProject(entry: ProjectEntry): Promise<void> {
  // Teardown current project state.
  try {
    await stopLogcat();
  } catch {
    // Ignore — logcat may not be running.
  }
  await selectProject(entry);
}

/**
 * Persist the currently active variant and device serial back to the
 * active project's registry entry.  Call after any user-initiated change.
 */
export async function saveActiveProjectMeta(): Promise<void> {
  const id = projectsState.activeProjectId;
  if (!id) return;
  const variant = variantState.activeVariant ?? null;
  const device = deviceState.selectedSerial ?? null;
  try {
    await updateProjectMeta(id, variant, device);
    updateProjectMetaInStore(id, variant, device);
  } catch (err) {
    // Non-fatal — don't surface a toast for background persistence.
    console.error("Failed to save project meta:", err);
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
      await reloadVariantsAndRestoreMeta(entry ?? null);
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

/**
 * Rename a project's display name (does not rename the folder on disk).
 */
export async function renameProjectEntry(id: string, newName: string): Promise<void> {
  try {
    await renameProjectApi(id, newName);
    renameProjectInStore(id, newName);
  } catch (err) {
    showToast(`Failed to rename project: ${formatError(err)}`, "error");
  }
}
