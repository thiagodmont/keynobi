/**
 * project.service.ts
 *
 * Handles the "open a project folder" flow.
 */

import { openFolderDialog, openProject, formatError, getGradleRoot, getApplicationId } from "@/lib/tauri-api";
import { setProject, setLoading, setApplicationId } from "@/stores/project.store";
import { showToast } from "@/components/common/Toast";
import { initBuildService } from "@/services/build.service";
import { initDevices } from "@/stores/device.store";
import { setMinePackage } from "@/lib/logcat-query";

export interface OpenProjectResult {
  root: string;
  projectName: string;
}

/**
 * Show the native folder picker, call the Rust `open_project` command,
 * update the project store, and return metadata the caller may need.
 *
 * Returns `null` when the user cancels the dialog or an error occurs.
 */
export async function openProjectFolder(): Promise<OpenProjectResult | null> {
  const path = await openFolderDialog();
  if (!path) return null;

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
