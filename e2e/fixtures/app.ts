import { test as base, expect } from "@playwright/test";
import type { MockPastBuild } from "../../src/test/mock-backend/build";

export interface E2EBridge {
  invoke: (command: string, args?: unknown) => Promise<unknown>;
  triggerEvent: (event: string, payload: unknown) => void;
  /** Starts a build as an attached agent would; returns its run ID. */
  startAgentBuild: (task: string, clientName: string | null, lineDelayMs?: number) => number;
  /** Adds a build to the history, as if recorded earlier; returns its history ID. */
  addPastBuild: (build: MockPastBuild) => number;
  /** Slows down builds the app starts (default 80 ms per output line). */
  setAppBuildLineDelay: (ms: number) => void;
  /** The mock project's shared run configuration file, as on disk; null when absent. */
  sharedRunConfigurationsFile: () => string | null;
  /** Replace the shared file, as a pulled commit would. */
  setSharedRunConfigurationsFile: (text: string | null) => void;
  /** Make the next install of a run fail with `reason`, as adb would. */
  failNextInstall: (reason: string | null) => void;
}

declare global {
  interface Window {
    __e2e__: E2EBridge;
  }
}

export const test = base.extend({
  page: async ({ page }, use) => {
    await page.goto("/");
    await page.waitForFunction(() => typeof window.__e2e__ !== "undefined", { timeout: 10_000 });
    await use(page);
  },
});

export { expect };
