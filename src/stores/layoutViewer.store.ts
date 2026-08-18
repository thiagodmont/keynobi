import { createStore } from "solid-js/store";
import type { UiHierarchySnapshot } from "@/bindings";
import { dumpUiHierarchy, formatError } from "@/lib/tauri-api";
import { selectedDevice } from "@/stores/device.store";

export interface LayoutViewerState {
  snapshot: UiHierarchySnapshot | null;
  loading: boolean;
  error: string | null;
  interactiveOnly: boolean;
  /** Collapse same-bounds single-child boring wrapper chains. */
  hideBoilerplate: boolean;
  searchQuery: string;
  /** Path from display-tree root (`""` = root row). */
  selectedLayoutPath: string | null;
  /** Index into `searchMatchPaths` for prev/next navigation. */
  searchMatchIndex: number;
  searchMatchPaths: string[];
  /** Auto-refresh interval in ms. null = off. */
  autoRefreshIntervalMs: number | null;
}

const [layoutViewerState, setLayoutViewerState] = createStore<LayoutViewerState>({
  snapshot: null,
  loading: false,
  error: null,
  interactiveOnly: false,
  hideBoilerplate: false,
  searchQuery: "",
  selectedLayoutPath: null,
  searchMatchIndex: 0,
  searchMatchPaths: [],
  autoRefreshIntervalMs: null,
});

export { layoutViewerState, setLayoutViewerState };

export function setAutoRefreshInterval(ms: number | null): void {
  setLayoutViewerState("autoRefreshIntervalMs", ms);
}

export function setLayoutInteractiveOnly(v: boolean): void {
  setLayoutViewerState("interactiveOnly", v);
}

export function setLayoutHideBoilerplate(v: boolean): void {
  setLayoutViewerState("hideBoilerplate", v);
}

export function setLayoutSearchQuery(q: string): void {
  setLayoutViewerState("searchQuery", q);
  setLayoutViewerState("searchMatchIndex", 0);
}

export function setLayoutSelectedPath(path: string | null): void {
  setLayoutViewerState("selectedLayoutPath", path);
}

export function setSearchMatchPaths(paths: string[]): void {
  setLayoutViewerState("searchMatchPaths", paths);
}

export function setSearchMatchIndex(i: number): void {
  setLayoutViewerState("searchMatchIndex", i);
}

let refreshRequestId = 0;

/** Refresh hierarchy from the selected device (or pass a specific serial). */
export async function refreshLayoutHierarchy(deviceSerial?: string | null): Promise<void> {
  const requestId = ++refreshRequestId;
  const explicitSerial = deviceSerial !== undefined && deviceSerial !== null;
  const selectedSerialAtStart = selectedDevice()?.serial ?? null;
  const serial = explicitSerial ? deviceSerial : selectedSerialAtStart;

  setLayoutViewerState("loading", true);
  setLayoutViewerState("error", null);
  try {
    const snap = await dumpUiHierarchy(serial ?? null);
    if (requestId !== refreshRequestId) return;
    if (!explicitSerial && selectedDevice()?.serial !== selectedSerialAtStart) return;
    setLayoutViewerState("snapshot", snap);
    setLayoutViewerState("selectedLayoutPath", null);
    setLayoutViewerState("searchMatchPaths", []);
    setLayoutViewerState("searchMatchIndex", 0);
  } catch (e) {
    if (requestId !== refreshRequestId) return;
    if (!explicitSerial && selectedDevice()?.serial !== selectedSerialAtStart) return;
    setLayoutViewerState("error", formatError(e));
    // Keep previous snapshot visible — user can still inspect the last capture
    setLayoutViewerState("selectedLayoutPath", null);
    setLayoutViewerState("searchMatchPaths", []);
  } finally {
    if (requestId === refreshRequestId) {
      setLayoutViewerState("loading", false);
    }
  }
}
