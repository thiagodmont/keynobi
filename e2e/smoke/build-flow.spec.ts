import { test as base } from "@playwright/test";
import { test, expect } from "../fixtures/app";

async function selectMockProject(page: import("@playwright/test").Page): Promise<void> {
  await page.getByText("MockProject", { exact: true }).first().click();
  await expect(page.getByText("Keynobi — MockProject")).toBeVisible({ timeout: 5_000 });
}

test("build tab is reachable and visible", async ({ page }) => {
  const buildTab = page.getByRole("tab", { name: "Build" });
  await expect(buildTab).toBeVisible({ timeout: 5_000 });
  await buildTab.click();
  await expect(buildTab).toHaveAttribute("aria-selected", "true");
});

test("running a build shows build lines then success indicator", async ({ page }) => {
  await selectMockProject(page);

  const buildTab = page.getByRole("tab", { name: "Build" });
  await buildTab.click();

  const buildOnlyButton = page.getByTitle(/Build only/i);
  await expect(buildOnlyButton).toBeVisible({ timeout: 5_000 });
  await buildOnlyButton.click();

  await expect(page.getByText(/BUILD SUCCESSFUL in 4s/i)).toBeVisible({ timeout: 10_000 });
});

// Run App installs and launches only when Auto Install on Build is on.
const deployTest = base.extend({
  page: async ({ page }, use) => {
    await page.addInitScript(() => {
      (
        window as Window & { __keynobi_e2e_settings_overrides?: Record<string, unknown> }
      ).__keynobi_e2e_settings_overrides = {
        build: {
          autoInstallOnBuild: true,
          autoScrollBuildLog: true,
          buildLogRetentionDays: 7,
          buildLogMaxFolderMb: 100,
        },
      };
    });
    await page.goto("/");
    await page.waitForFunction(() => typeof window.__e2e__ !== "undefined", { timeout: 10_000 });
    await use(page);
  },
});

deployTest("Run App records the launch time on the build it installed", async ({ page }) => {
  await selectMockProject(page);
  await page.getByRole("tab", { name: "Build" }).click();

  // The mock project's emulator is online and selected, so no device prompt.
  await page
    .getByTitle(/^Run App/)
    .first()
    .click();

  await expect(page.getByText("▶ Launch time: 812 ms (cold) · displayed 790 ms")).toBeVisible({
    timeout: 10_000,
  });
  // The fully drawn time arrives after the launch returned (build:launch_timing).
  await expect(page.getByTestId("launch-timing").first()).toHaveText(
    "Launch 812 ms (cold) · displayed 790 ms · fully drawn 1.4 s",
    { timeout: 10_000 }
  );
});

deployTest("a past build says which device Run App installed it on", async ({ page }) => {
  await selectMockProject(page);
  await page.getByRole("tab", { name: "Build" }).click();
  await page
    .getByTitle(/^Run App/)
    .first()
    .click();
  await expect(page.getByText("▶ Launch time: 812 ms (cold)")).toBeVisible({ timeout: 10_000 });

  await page.getByRole("listbox", { name: "Builds" }).getByRole("option").first().click();

  await expect(page.getByText(/^Viewing build #\d+ from /)).toBeVisible();
  await expect(page.getByText(/^Installed on Pixel_6_API_34 · /)).toBeVisible();
});

test("an agent's build shows who started it and can be cancelled from the app", async ({
  page,
}) => {
  await selectMockProject(page);
  await page.getByRole("tab", { name: "Build" }).click();

  // Slow enough to act on while it runs.
  await page.evaluate(() => window.__e2e__.startAgentBuild("assembleDebug", "Claude Code", 5_000));

  await expect(page.getByText(/Started by an agent \(Claude Code\)/).first()).toBeVisible({
    timeout: 5_000,
  });
  await expect(
    page.getByTitle("A build started by an agent (Claude Code) is running")
  ).toBeDisabled();

  await page.getByTitle("Cancel the build started by an agent (Claude Code)").first().click();

  await expect(
    page.getByText("Build cancelled · Started by an agent (Claude Code) · Cancelled in Keynobi")
  ).toBeVisible({ timeout: 5_000 });
});
