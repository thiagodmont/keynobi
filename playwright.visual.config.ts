import { defineConfig, devices } from "@playwright/test";

// @ts-expect-error process is a nodejs global
const isCI = !!process.env.CI;

// Playwright forces colored output for its reporters. In environments that also
// set NO_COLOR, Node prints noisy FORCE_COLOR/NO_COLOR conflict warnings.
// @ts-expect-error process is a nodejs global
delete process.env.NO_COLOR;

export default defineConfig({
  testDir: "./e2e/visual",
  fullyParallel: false,
  forbidOnly: isCI,
  retries: isCI ? 2 : 0,
  workers: 1,
  reporter: isCI ? [["github"], ["list"], ["html", { open: "never" }]] : "list",
  expect: {
    toHaveScreenshot: {
      maxDiffPixels: 80,
      threshold: 0.2,
    },
  },
  use: {
    baseURL: "http://localhost:1421",
    trace: "on-first-retry",
    viewport: { width: 1280, height: 720 },
    deviceScaleFactor: 1,
    colorScheme: "dark",
  },
  projects: [
    {
      name: "chromium",
      use: { ...devices["Desktop Chrome"] },
    },
  ],
  webServer: {
    command: "npm run dev:web",
    url: "http://localhost:1421",
    reuseExistingServer: !isCI,
    timeout: 120_000,
  },
});
