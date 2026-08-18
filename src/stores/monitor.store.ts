/**
 * monitor.store.ts
 *
 * Reactive bridge for the Rust background monitor task.
 * Listens to the "monitor://stats" Tauri event (emitted every 5s) and
 * exposes three signals consumed by StatusBar's MemoryIndicator and
 * LogSizeIndicator components.
 */

import { createSignal } from "solid-js";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";

export interface MonitorStats {
  appMemoryBytes: number;
  logFolderBytes: number;
  rotationTriggered: boolean;
}

const [appMemoryBytes, setAppMemoryBytes] = createSignal(0);
const [logFolderBytes, setLogFolderBytes] = createSignal(0);
const [rotationTriggered, setRotationTriggered] = createSignal(false);

export { appMemoryBytes, logFolderBytes, rotationTriggered };

let monitorUnlisten: UnlistenFn | null = null;
let monitorInit: Promise<void> | null = null;

/**
 * Register the monitor listener. Idempotent and safe to call from onMount.
 *
 * This used to run as a module side effect at import time, which meant merely
 * importing the store started a listener that was never disposed — and made the
 * module untestable.
 */
export function initMonitorListeners(): Promise<void> {
  if (monitorInit) return monitorInit;

  monitorInit = listen<MonitorStats>("monitor://stats", (event) => {
    setAppMemoryBytes(event.payload.appMemoryBytes);
    setLogFolderBytes(event.payload.logFolderBytes);
    setRotationTriggered(event.payload.rotationTriggered);
  })
    .then((unlisten) => {
      if (monitorUnlisten) {
        unlisten();
        return;
      }
      monitorUnlisten = unlisten;
    })
    .catch((err) => {
      // Non-fatal: the status-bar indicators simply stay at their defaults.
      monitorInit = null;
      console.error("[monitor] Failed to register stats listener:", err);
    });

  return monitorInit;
}

/** Test-only teardown. */
export function resetMonitorListenersForTests(): void {
  monitorUnlisten?.();
  monitorUnlisten = null;
  monitorInit = null;
  setAppMemoryBytes(0);
  setLogFolderBytes(0);
  setRotationTriggered(false);
}
