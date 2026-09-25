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
  getProjectRoot,
  formatError,
  getGradleRoot,
  getApplicationId,
  listProjects,
  removeProject as removeProjectApi,
  pinProject as pinProjectApi,
  getLastActiveProject,
  updateProjectMeta,
  renameProject as renameProjectApi,
  setProjectTrust,
} from "@/lib/tauri-api";
import {
  setProject,
  setLoading,
  setApplicationId,
  beginProjectOpen,
  currentProjectGeneration,
  projectState,
} from "@/stores/project.store";
import {
  setProjects,
  setActiveProjectId,
  upsertProject,
  removeProjectFromStore,
  setPinned,
  setProjectsLoading,
  renameProjectInStore,
  updateProjectMetaInStore,
  setProjectTrustInStore,
  projectsState,
} from "@/stores/projects.store";
import { showDialog, showToast } from "@/components/ui";
import { initBuildService, cancelBuild } from "@/services/build.service";
import { resetBuildState, setBuildHistory } from "@/stores/build.store";
import { getBuildHistory } from "@/lib/tauri-api";
import { initDevices, pickDevice, onDeviceChange } from "@/stores/device.store";
import { stopLogcat } from "@/lib/tauri-api";
import { setMinePackage } from "@/lib/logcat-mine-package";
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
  saveActiveProjectMeta().catch((e) => {
    console.error(e);
    showToast(`Failed to save project state: ${formatError(e)}`, "error");
  });
});
onDeviceChange((_serial) => {
  saveActiveProjectMeta().catch((e) => {
    console.error(e);
    showToast(`Failed to save project state: ${formatError(e)}`, "error");
  });
});

export interface OpenProjectResult {
  root: string;
  projectName: string;
}

// ── Core open logic (shared by openProjectFolder and switchProject) ────────────

// Guards against interleaved project opens. Two rapid sidebar clicks each run
// several awaited IPC calls; without this the store ends up mixing one
// project's root with another's applicationId, or restoring one project's
// variant into the other. variant.store uses the same idea via isCurrentProject().
type IsCurrentOpen = () => boolean;

function isCurrentOpen(generation: number): IsCurrentOpen {
  return () => generation === currentProjectGeneration();
}

async function doOpenProject(
  path: string,
  isCurrent: IsCurrentOpen
): Promise<OpenProjectResult | null> {
  setLoading(true);
  try {
    const projectName = await openProject(path);
    if (!isCurrent()) return null;

    const canonicalRoot = (await getProjectRoot().catch(() => null)) ?? path;
    if (!isCurrent()) return null;

    const gradleRoot = await getGradleRoot().catch(() => null);
    if (!isCurrent()) return null;
    setProject(canonicalRoot, projectName, gradleRoot);

    // Resolve applicationId for `package:mine` filter.
    const appId = await getApplicationId().catch(() => null);
    if (!isCurrent()) return null;
    setApplicationId(appId);
    setMinePackage(appId);

    // Re-initialize build service and devices for the new project.
    initBuildService().catch(console.error);
    initDevices().catch(console.error);

    // Re-run health checks now that project_root and gradle_root are set in
    // FsState — the Gradle wrapper probe was false at startup because the
    // project wasn't loaded yet.
    refreshHealthChecks().catch(console.error);

    return { root: canonicalRoot, projectName };
  } catch (err) {
    if (!isCurrent()) return null;
    showToast(`Failed to open project: ${formatError(err)}`, "error");
    return null;
  } finally {
    if (isCurrent()) setLoading(false);
  }
}

// ── Project trust ─────────────────────────────────────────────────────────────

type TrustChoice = "trust" | "safe-mode" | "cancel";

const pendingTrustQuestions = new Map<string, Promise<void>>();

function askToTrust(name: string): Promise<TrustChoice> {
  return showDialog({
    title: `Trust "${name}"?`,
    message:
      "This project will run its Gradle build scripts. Trust it? In Safe Mode, build variants are read from the build files and builds are disabled until you trust the project.",
    buttons: [
      { label: "Trust", value: "trust", style: "primary" },
      { label: "Open in Safe Mode", value: "safe-mode", style: "secondary" },
    ],
  }) as Promise<TrustChoice>;
}

/** Save a trust decision; returns false (after a toast) when it was not saved. */
async function recordTrust(id: string, trusted: boolean): Promise<boolean> {
  try {
    await setProjectTrust(id, trusted);
    setProjectTrustInStore(id, trusted);
    return true;
  } catch (err) {
    showToast(`Failed to save project trust: ${formatError(err)}`, "error");
    return false;
  }
}

/**
 * Ask once, the first time a project is opened, whether it may run its Gradle
 * build scripts. Dismissing the dialog keeps Safe Mode and asks again on the
 * next open.
 */
async function resolveProjectTrust(
  entry: ProjectEntry | null,
  isCurrent: IsCurrentOpen
): Promise<void> {
  if (!entry || entry.trusted !== null || !isCurrent()) return;
  // Reopening a project while its question is still shown waits for that answer.
  let pending = pendingTrustQuestions.get(entry.id);
  if (!pending) {
    pending = askToTrust(entry.name)
      .then(async (choice) => {
        // The answer is about this project even if another open started meanwhile.
        if (choice !== "cancel") await recordTrust(entry.id, choice === "trust");
      })
      .finally(() => pendingTrustQuestions.delete(entry.id));
    pendingTrustQuestions.set(entry.id, pending);
  }
  await pending;
}

/** Trust a project. Reloads the open project's variants so the Gradle phase runs. */
export async function trustProject(entry: ProjectEntry): Promise<void> {
  if (!(await recordTrust(entry.id, true))) return;
  if (entry.path === projectState.projectRoot) {
    await loadVariants().catch(console.error);
  }
}

/** Revoke trust: builds are disabled and the open project's running build is cancelled. */
export async function revokeProjectTrust(entry: ProjectEntry): Promise<void> {
  if (!(await recordTrust(entry.id, false))) return;
  if (entry.path === projectState.projectRoot) {
    await cancelBuild().catch(console.error);
  }
}

/** Ask the trust question again for the open project. */
export async function askToTrustActiveProject(): Promise<void> {
  const entry = projectsState.projects.find((p) => p.path === projectState.projectRoot);
  if (!entry) return;
  if ((await askToTrust(entry.name)) === "trust") await trustProject(entry);
}

/**
 * After FsState points at a project, rediscover variants and restore registry selections.
 * Call only after a successful `doOpenProject` so a failed open does not wipe variant state.
 */
async function reloadVariantsAndRestoreMeta(
  entry: ProjectEntry | null,
  isCurrent: IsCurrentOpen
): Promise<void> {
  if (!isCurrent()) return;
  await resolveProjectTrust(entry, isCurrent);
  if (!isCurrent()) return;
  resetVariantState();
  await loadVariants();
  // A newer open owns the variant state and the saved selections now.
  if (!isCurrent()) return;
  if (variantState.error) {
    showToast(`Failed to load build variants: ${variantState.error}`, "error");
  }
  const savedVariant = entry?.lastBuildVariant;
  if (savedVariant && variantState.variants.some((v) => v.name === savedVariant)) {
    await selectVariant(savedVariant).catch(console.error);
  }
  if (entry?.lastDevice && isCurrent()) {
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
  const isCurrent = isCurrentOpen(beginProjectOpen());

  // Cancel any running build from the previous project and clear its state.
  await cancelBuild().catch(() => {});
  resetBuildState();

  const result = await doOpenProject(path, isCurrent);
  if (result) {
    // Refresh the projects list so the new entry shows in the sidebar.
    await refreshProjectsList().catch(console.error);
    // Mark this project as active in the registry store.
    const projects = (await listProjects().catch(() => [])) as ProjectEntry[];
    if (!isCurrent()) return null;
    const entry = projects.find((p) => p.path === result.root);
    if (entry) {
      upsertProject(entry);
      setActiveProjectId(entry.id);
    }
    // Load build history scoped to the newly opened project.
    getBuildHistory().then(setBuildHistory).catch(console.error);

    await reloadVariantsAndRestoreMeta(entry ?? null, isCurrent);
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
  const isCurrent = isCurrentOpen(beginProjectOpen());
  // Cancel build — it targets the old project's Gradle root.
  await cancelBuild().catch(() => {
    // Ignore — no build in progress.
  });
  // Reset build state AND history so the previous project's builds don't bleed through.
  resetBuildState();

  const result = await doOpenProject(entry.path, isCurrent);
  if (result) {
    // Fetch fresh metadata for this entry (lastBuildVariant, lastDevice, etc.)
    // but do NOT replace the full list — that would re-sort by lastOpened and
    // jump the selected project to the top.
    const projects = (await listProjects().catch(() => [])) as ProjectEntry[];
    if (!isCurrent()) return;
    const fresh = projects.find((p) => p.id === entry.id) ?? entry;
    upsertProject(fresh);
    setActiveProjectId(fresh.id);

    // Load build history scoped to the newly active project.
    getBuildHistory().then(setBuildHistory).catch(console.error);

    await reloadVariantsAndRestoreMeta(fresh, isCurrent);
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

    const isCurrent = isCurrentOpen(beginProjectOpen());
    const result = await doOpenProject(lastPath, isCurrent);
    if (result) {
      const projects = (await listProjects().catch(() => [])) as ProjectEntry[];
      if (!isCurrent()) return false;
      const entry = projects.find((p) => p.path === lastPath);
      if (entry) {
        upsertProject(entry);
        setActiveProjectId(entry.id);
      }
      await reloadVariantsAndRestoreMeta(entry ?? null, isCurrent);
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
