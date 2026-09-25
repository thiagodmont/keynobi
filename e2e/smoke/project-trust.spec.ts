import { test, expect } from "../fixtures/app";

test("revoking trust disables builds until the project is trusted again", async ({ page }) => {
  const row = page.getByText("MockProject", { exact: true }).first();
  await row.click();
  await expect(page.getByText("Keynobi — MockProject")).toBeVisible({ timeout: 5_000 });
  await page.getByRole("tab", { name: "Build" }).click();
  await expect(page.getByTitle(/Build only/i)).toBeEnabled({ timeout: 5_000 });

  await row.click({ button: "right" });
  await page.getByRole("menuitem", { name: "Revoke Trust" }).click();

  const safeModeTitle = "Safe Mode — trust this project to build";
  await expect(page.getByTitle(safeModeTitle).first()).toBeDisabled();
  await expect(page.getByRole("button", { name: "Safe Mode", exact: true })).toBeVisible();

  await row.click({ button: "right" });
  await page.getByRole("menuitem", { name: "Trust Project" }).click();

  await expect(page.getByTitle(/Build only/i)).toBeEnabled();
  await expect(page.getByRole("button", { name: "Safe Mode", exact: true })).toHaveCount(0);
});
