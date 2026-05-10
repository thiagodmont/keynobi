import { defineConfig, devices } from "@playwright/test";

// @ts-expect-error process is a nodejs global
const isCI = !!process.env.CI;

// Playwright forces colored output for its reporters. In environments that also
// set NO_COLOR, Node prints noisy FORCE_COLOR/NO_COLOR conflict warnings.
// @ts-expect-error process is a nodejs global
delete process.env.NO_COLOR;

export default defineConfig({
  testDir: "./e2e/storybook",
  fullyParallel: true,
  forbidOnly: isCI,
  retries: isCI ? 2 : 0,
  workers: isCI ? 1 : undefined,
  reporter: "html",
  outputDir: "test-results/storybook",
  use: {
    baseURL: "http://127.0.0.1:6106",
    trace: "on-first-retry",
  },
  projects: [
    {
      name: "chromium",
      use: { ...devices["Desktop Chrome"] },
    },
  ],
  webServer: {
    command: "npm run storybook:build && npm run storybook:preview",
    url: "http://127.0.0.1:6106",
    reuseExistingServer: false,
    timeout: 120_000,
  },
});
