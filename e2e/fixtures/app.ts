import { test as base, expect } from "@playwright/test";

export interface E2EBridge {
  invoke: (command: string, args?: unknown) => Promise<unknown>;
  triggerEvent: (event: string, payload: unknown) => void;
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
