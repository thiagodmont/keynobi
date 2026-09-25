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
