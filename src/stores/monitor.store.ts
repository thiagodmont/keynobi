/**
 * monitor.store.ts
 *
 * Reactive bridge for the Rust background monitor task.
 * Listens to the "monitor://stats" Tauri event (emitted every 5s) and
 * exposes three signals consumed by StatusBar's MemoryIndicator and
 * LogSizeIndicator components.
 */

import { createSignal } from "solid-js";
import { listen } from "@tauri-apps/api/event";

export interface MonitorStats {
  appMemoryBytes: number;
  logFolderBytes: number;
  rotationTriggered: boolean;
}

const [appMemoryBytes, setAppMemoryBytes] = createSignal(0);
const [logFolderBytes, setLogFolderBytes] = createSignal(0);
const [rotationTriggered, setRotationTriggered] = createSignal(false);

export { appMemoryBytes, logFolderBytes, rotationTriggered };

if (typeof window !== "undefined") {
  listen<MonitorStats>("monitor://stats", (event) => {
    setAppMemoryBytes(event.payload.appMemoryBytes);
    setLogFolderBytes(event.payload.logFolderBytes);
    setRotationTriggered(event.payload.rotationTriggered);
  }).catch(() => {}); // fire-and-forget; listener failure is non-fatal
}
