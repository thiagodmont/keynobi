import type { Page } from "@playwright/test";
import { test, expect } from "../fixtures/app";

const ROW_TITLE = "Click to copy · Shift+click to select range";

type LogcatSeedEntry = {
  id: number;
  tag: string;
  message: string;
  level?: string;
};

async function openLogcatWithEmptyEntries(page: Page): Promise<void> {
  await page.getByRole("tab", { name: "Logcat" }).click();
  await page.evaluate(async () => {
    await window.__e2e__.invoke("clear_logcat", {});
  });
}

async function pushLogcatEntries(page: Page, entries: LogcatSeedEntry[]): Promise<void> {
  await page.evaluate(async (seedEntries) => {
    const payload = seedEntries.map((entry) => ({
      id: BigInt(entry.id),
      timestamp: "2026-05-06T12:00:00.000Z",
      pid: 1234,
      tid: 5678,
      level: entry.level ?? "info",
      tag: entry.tag,
      message: entry.message,
      package: "com.example.mockapp",
      kind: "normal",
      isCrash: false,
      flags: 0,
      category: "general",
      crashGroupId: null,
      jsonBody: null,
    }));
    await window.__e2e__.invoke("__e2e_append_logcat_entries", { entries: payload });
  }, entries);
}

async function seedContextRows(page: Page): Promise<void> {
  await openLogcatWithEmptyEntries(page);
  await pushLogcatEntries(page, [
    { id: 200, tag: "VisualBefore", message: "Visual context before one" },
    { id: 201, tag: "VisualBefore", message: "Visual context before two" },
    { id: 202, tag: "VisualTarget", message: "Visual focused context match" },
    { id: 203, tag: "VisualAfter", message: "Visual context after one" },
    { id: 204, tag: "VisualAfter", message: "Visual context after two" },
  ]);

  await expect(page.getByTitle(ROW_TITLE)).toHaveCount(5, { timeout: 5_000 });
  await page.locator('input[type="text"][placeholder*="Filter"]').first().fill("message:focused ");
  await expect(page.getByTitle(ROW_TITLE)).toHaveCount(1, { timeout: 5_000 });
}

test("logcat filtered row context menu surface", async ({ page }) => {
  await seedContextRows(page);

  await page.getByText("Visual focused context match").click({ button: "right" });
  const menu = page.locator(".logcat-context-menu");
  await expect(menu).toBeVisible();

  await expect(menu).toHaveScreenshot("logcat-filtered-context-menu.png");
});

test("logcat expanded context rows surface", async ({ page }) => {
  await seedContextRows(page);

  await page.getByText("Visual focused context match").click({ button: "right" });
  await page.getByRole("menuitem", { name: "Expand 10 up" }).click();
  await expect(page.getByText("Visual context before one")).toBeVisible({ timeout: 5_000 });

  await expect(page.getByTestId("logcat-virtual-list")).toHaveScreenshot(
    "logcat-expanded-context-rows.png"
  );
});

test("logcat entry detail filter menu surface", async ({ page }) => {
  await openLogcatWithEmptyEntries(page);
  await pushLogcatEntries(page, [
    { id: 205, tag: "VisualDetailTag", message: "Visual detail menu row" },
  ]);

  await expect(page.getByText("Visual detail menu row")).toBeVisible({ timeout: 5_000 });
  await page.getByText("Visual detail menu row").click();
  await page.getByTitle("Filter by Tag").click();
  const menu = page.getByRole("menu");
  await expect(menu).toBeVisible();

  await expect(menu).toHaveScreenshot("logcat-entry-detail-filter-menu.png");
});

test("logcat query suggestion menu surface", async ({ page }) => {
  await openLogcatWithEmptyEntries(page);

  await page.locator('input[type="text"][placeholder*="Filter"]').first().fill("l");
  const menu = page.locator(".logcat-query-suggestions");
  await expect(menu).toBeVisible();

  await expect(menu).toHaveScreenshot("logcat-query-suggestions.png");
});
