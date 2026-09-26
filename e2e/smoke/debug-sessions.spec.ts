import { test as base, expect } from "@playwright/test";
import "../fixtures/app";

// Run App installs and launches only when Auto Install on Build is on.
const test = base.extend({
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

test("Run App opens a debug session that shows the install and launch, and can be kept and bookmarked", async ({
  page,
}) => {
  await page.getByText("MockProject", { exact: true }).first().click();
  await expect(page.getByText("Keynobi — MockProject")).toBeVisible({ timeout: 5_000 });
  await page.getByRole("tab", { name: "Build" }).click();
  await page
    .getByTitle(/^Run App/)
    .first()
    .click();
  await expect(page.getByText("▶ Launch time: 812 ms (cold)")).toBeVisible({ timeout: 10_000 });

  await page.keyboard.press("Meta+Shift+P");
  await page.keyboard.type("Show Debug Sessions");
  await expect(page.getByRole("option", { name: /Show Debug Sessions/ })).toBeVisible();
  await page.keyboard.press("Enter");

  const dialog = page.getByRole("dialog", { name: "Debug Sessions" });
  await expect(dialog).toBeVisible();
  const sessions = dialog.getByRole("listbox", { name: "Debug sessions" });
  await expect(sessions.getByRole("option").first()).toContainText("Open");
  await expect(sessions.getByRole("option").first()).toHaveAttribute("aria-selected", "true");
  await expect(sessions.getByRole("option").first()).toBeFocused();

  const timeline = dialog.getByRole("listbox", { name: "Timeline, oldest first" });
  await expect(timeline.getByRole("option").filter({ hasText: "Installed APK" })).toHaveCount(1);
  await expect(
    timeline.getByRole("option").filter({ hasText: "Launched · Launch 812 ms (cold)" })
  ).toHaveCount(1);

  const keep = dialog.getByRole("button", { name: "Keep" });
  await expect(keep).toHaveAttribute("aria-pressed", "false");
  await keep.click();
  await expect(keep).toHaveAttribute("aria-pressed", "true");
  await expect(sessions.getByRole("option").first()).toContainText("Kept");

  await dialog.getByLabel("Bookmark note").fill("Checkout button did nothing");
  await dialog.getByRole("button", { name: "Add bookmark" }).click();
  const bookmark = timeline.getByRole("option").filter({ hasText: "Checkout button did nothing" });
  await expect(bookmark).toHaveAttribute("aria-selected", "true");
  await expect(dialog.getByRole("region", { name: "Selected event" })).toContainText("Bookmark");

  await page.keyboard.press("Escape");
  await expect(dialog).toBeHidden();
});
