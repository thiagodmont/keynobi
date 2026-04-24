import { test as base, expect } from "@playwright/test";

const test = base.extend({
  page: async ({ page }, use) => {
    await page.addInitScript(() => {
      const windowWithOverrides = window as Window & {
        __keynobi_e2e_settings_overrides?: Record<string, unknown>;
      };
      windowWithOverrides.__keynobi_e2e_settings_overrides = {
        onboardingCompleted: false,
      };
    });

    await page.goto("/");
    await page.waitForFunction(
      () => typeof (window as Window & { __e2e__?: unknown }).__e2e__ !== "undefined",
      { timeout: 10_000 }
    );
    await use(page);
  },
});

test("onboarding wizard appears when onboardingCompleted is false", async ({ page }) => {
  await expect(page.getByRole("dialog", { name: "Setup wizard" })).toBeVisible({ timeout: 5_000 });
});
