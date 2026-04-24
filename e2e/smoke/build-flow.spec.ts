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
