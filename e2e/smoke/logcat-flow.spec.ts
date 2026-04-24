import { test, expect } from "../fixtures/app";

test("logcat tab shows sample entries after start_logcat", async ({ page }) => {
  await page.getByRole("tab", { name: "Logcat" }).click();

  await page.evaluate(async () => {
    await window.__e2e__.invoke("start_logcat", {});
  });

  await expect(page.getByText("Activity started").first()).toBeVisible({ timeout: 5_000 });
});

test("typing a filter tag hides non-matching entries", async ({ page }) => {
  await page.getByRole("tab", { name: "Logcat" }).click();

  await page.evaluate(async () => {
    await window.__e2e__.invoke("start_logcat", {});
  });

  const activityStarted = page.getByText("Activity started").first();
  await expect(activityStarted).toBeVisible({ timeout: 5_000 });

  const filterInput = page.locator('input[type="text"][placeholder*="Filter"]').first();
  await filterInput.fill("tag:DatabaseHelper ");

  await expect(page.getByText(/Failed to open database/i).first()).toBeVisible({ timeout: 5_000 });
  await expect(activityStarted).not.toBeVisible();
});
