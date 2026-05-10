import type { Locator, Page } from "@playwright/test";
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

function makeScrollEntries(start: number, count: number): LogcatSeedEntry[] {
  return Array.from({ length: count }, (_, offset) => {
    const id = start + offset;
    return {
      id,
      tag: "ScrollTag",
      message: `Auto row ${String(id).padStart(3, "0")}`,
    };
  });
}

async function scrollState(scroller: Locator): Promise<{
  scrollTop: number;
  scrollHeight: number;
  clientHeight: number;
  maxScrollTop: number;
}> {
  return scroller.evaluate((element) => ({
    scrollTop: Math.round(element.scrollTop),
    scrollHeight: Math.round(element.scrollHeight),
    clientHeight: Math.round(element.clientHeight),
    maxScrollTop: Math.round(element.scrollHeight - element.clientHeight),
  }));
}

test("logcat follow-tail stays paused after manual scrolling until Jump to end", async ({ page }) => {
  await openLogcatWithEmptyEntries(page);
  await pushLogcatEntries(page, makeScrollEntries(0, 90));

  const scroller = page.getByTestId("logcat-virtual-list");
  await expect(scroller).toBeVisible({ timeout: 5_000 });
  await expect(page.getByText("Auto row 089")).toBeVisible({ timeout: 5_000 });

  const initialBottom = await scrollState(scroller);
  expect(initialBottom.scrollTop).toBeGreaterThan(0);

  await scroller.evaluate((element) => {
    element.scrollTop = 0;
    element.dispatchEvent(new Event("scroll", { bubbles: true }));
  });
  await expect(page.getByText("Auto row 000")).toBeVisible({ timeout: 5_000 });

  await pushLogcatEntries(page, makeScrollEntries(90, 1));
  await page.waitForTimeout(100);
  expect((await scrollState(scroller)).scrollTop).toBe(0);
  await expect(page.getByText("Auto row 090")).not.toBeVisible();

  const manualBottomTop = await scroller.evaluate((element) => {
    element.scrollTop = element.scrollHeight - element.clientHeight;
    element.dispatchEvent(new Event("scroll", { bubbles: true }));
    return Math.round(element.scrollTop);
  });
  await pushLogcatEntries(page, makeScrollEntries(91, 1));
  await page.waitForTimeout(100);
  const pausedAfterManualBottom = await scrollState(scroller);
  expect(pausedAfterManualBottom.scrollTop).toBe(manualBottomTop);
  // Read mode freezes the rendered list, so incoming logs do not increase scrollHeight
  // until Jump to end applies them.
  expect(pausedAfterManualBottom.maxScrollTop).toBe(manualBottomTop);
  await expect(page.getByText("2 new")).toBeVisible({ timeout: 5_000 });

  await page.getByTitle("2 new logs available - Jump to end").click();
  await page.waitForFunction(() => {
    const element = document.querySelector('[data-testid="logcat-virtual-list"]');
    if (!(element instanceof HTMLElement)) return false;
    return Math.abs(element.scrollTop - (element.scrollHeight - element.clientHeight)) <= 1;
  });
  await expect(page.getByText("Auto row 091")).toBeVisible({ timeout: 5_000 });

  const beforeFollowAppend = await scrollState(scroller);
  await pushLogcatEntries(page, makeScrollEntries(92, 1));
  await page.waitForFunction(() => {
    const element = document.querySelector('[data-testid="logcat-virtual-list"]');
    if (!(element instanceof HTMLElement)) return false;
    return Math.abs(element.scrollTop - (element.scrollHeight - element.clientHeight)) <= 1;
  });
  const afterFollowAppend = await scrollState(scroller);
  expect(afterFollowAppend.maxScrollTop).toBeGreaterThan(beforeFollowAppend.maxScrollTop);
  expect(afterFollowAppend.scrollTop).toBe(afterFollowAppend.maxScrollTop);
  await expect(page.getByText("Auto row 092")).toBeVisible({ timeout: 5_000 });
});

test("logcat detail panel opens for the clicked filtered row", async ({ page }) => {
  await openLogcatWithEmptyEntries(page);
  await pushLogcatEntries(page, [
    { id: 100, tag: "AlphaTag", message: "Alpha unfiltered message" },
    { id: 101, tag: "BetaTag", message: "Beta target message" },
    { id: 102, tag: "GammaTag", message: "Gamma target message" },
  ]);

  await expect(page.getByTitle(ROW_TITLE)).toHaveCount(3, { timeout: 5_000 });

  const filterInput = page.locator('input[type="text"][placeholder*="Filter"]').first();
  await filterInput.fill("message:target");

  await expect(page.getByTitle(ROW_TITLE)).toHaveCount(2, { timeout: 5_000 });
  await page.getByText("Beta target message").click();

  await expect(page.getByTitle("Filter by message")).toHaveText("Beta target message");
});

test("logcat query connector badges toggle only the clicked condition", async ({ page }) => {
  await openLogcatWithEmptyEntries(page);
  await pushLogcatEntries(page, [
    { id: 110, tag: "AlphaOnly", message: "Alpha only row" },
    { id: 111, tag: "AlphaBeta", message: "Alpha beta row" },
    { id: 112, tag: "BetaOnly", message: "Beta only row" },
    { id: 113, tag: "GammaOnly", message: "Gamma only row" },
  ]);

  await expect(page.getByTitle(ROW_TITLE)).toHaveCount(4, { timeout: 5_000 });

  const filterInput = page.locator('input[type="text"][placeholder*="Filter"]').first();
  await filterInput.fill("tag:Alpha tag:Beta | tag:Gamma ");

  await expect(page.getByText("Alpha beta row")).toBeVisible({ timeout: 5_000 });
  await expect(page.getByText("Gamma only row")).toBeVisible();
  await expect(page.getByText("Alpha only row")).not.toBeVisible();
  await expect(page.getByText("Beta only row")).not.toBeVisible();

  await page.getByRole("button", { name: "Change AND to OR" }).click();

  await expect(page.getByText("Alpha only row")).toBeVisible({ timeout: 5_000 });
  await expect(page.getByText("Alpha beta row")).toBeVisible();
  await expect(page.getByText("Beta only row")).toBeVisible();
  await expect(page.getByText("Gamma only row")).toBeVisible();
  await expect(page.getByText(/Tab\/Enter select/)).not.toBeVisible();

  await page.getByRole("button", { name: "Change OR to AND" }).first().focus();
  await page.keyboard.press("Enter");

  await expect(page.getByText("Alpha beta row")).toBeVisible({ timeout: 5_000 });
  await expect(page.getByText("Gamma only row")).toBeVisible();
  await expect(page.getByText("Alpha only row")).not.toBeVisible();
  await expect(page.getByText("Beta only row")).not.toBeVisible();
  await expect(page.getByText(/Tab\/Enter select/)).not.toBeVisible();
});

test("logcat filter pill can be temporarily disabled and re-enabled", async ({ page }) => {
  await openLogcatWithEmptyEntries(page);
  await pushLogcatEntries(page, [
    { id: 120, tag: "AlphaOnly", message: "Alpha only disable row" },
    { id: 121, tag: "AlphaBeta", message: "Alpha beta disable row" },
    { id: 122, tag: "BetaOnly", message: "Beta only disable row" },
  ]);

  await expect(page.getByTitle(ROW_TITLE)).toHaveCount(3, { timeout: 5_000 });

  const filterInput = page.locator('input[type="text"][placeholder*="Filter"]').first();
  await filterInput.fill("tag:Alpha tag:Beta ");

  await expect(page.getByText("Alpha beta disable row")).toBeVisible({ timeout: 5_000 });
  await expect(page.getByText("Alpha only disable row")).not.toBeVisible();
  await expect(page.getByText("Beta only disable row")).not.toBeVisible();
  await expect(page.getByText("tag:Beta")).toBeVisible();

  await page.getByRole("button", { name: "Disable filter tag:Beta" }).click();

  await expect(page.getByText("Alpha only disable row")).toBeVisible({ timeout: 5_000 });
  await expect(page.getByText("Alpha beta disable row")).toBeVisible();
  await expect(page.getByText("Beta only disable row")).not.toBeVisible();
  await expect(page.getByText("tag:Beta")).toBeVisible();

  await page.getByRole("button", { name: "Re-enable filter tag:Beta" }).click();

  await expect(page.getByText("Alpha beta disable row")).toBeVisible({ timeout: 5_000 });
  await expect(page.getByText("Alpha only disable row")).not.toBeVisible();
  await expect(page.getByText("Beta only disable row")).not.toBeVisible();
});

test("logcat active filter can be saved from the query row and reapplied", async ({ page }) => {
  await openLogcatWithEmptyEntries(page);
  await pushLogcatEntries(page, [
    { id: 130, tag: "SaveAlpha", message: "Saved alpha row" },
    { id: 131, tag: "SaveBeta", message: "Saved beta row" },
  ]);

  await expect(page.getByTitle(ROW_TITLE)).toHaveCount(2, { timeout: 5_000 });

  const filterInput = page.locator('input[type="text"][placeholder*="Filter"]').first();
  await filterInput.fill("tag:SaveAlpha ");

  await expect(page.getByText("Saved alpha row")).toBeVisible({ timeout: 5_000 });
  await expect(page.getByText("Saved beta row")).not.toBeVisible();

  await page.getByTitle("Save current filter").click();
  await page.getByPlaceholder("Filter name…").fill("Saved Alpha E2E");
  await page.getByRole("button", { name: "Save" }).last().click();

  await page.getByTitle("Clear all filters").first().click();
  await expect(page.getByText("Saved alpha row")).toBeVisible({ timeout: 5_000 });
  await expect(page.getByText("Saved beta row")).toBeVisible();

  await page.getByTitle("Filter presets").click();
  await page.getByTestId("logcat-filter-quick-row").getByText("Saved Alpha E2E").click();

  await expect(page.getByText("Saved alpha row")).toBeVisible({ timeout: 5_000 });
  await expect(page.getByText("Saved beta row")).not.toBeVisible();
});

test("logcat saving with a disabled pill stores only the effective filter", async ({ page }) => {
  await openLogcatWithEmptyEntries(page);
  await pushLogcatEntries(page, [
    { id: 140, tag: "SaveAlpha", message: "Saved disabled alpha row" },
    { id: 141, tag: "SaveAlphaBeta", message: "Saved disabled alpha beta row" },
    { id: 142, tag: "SaveBeta", message: "Saved disabled beta row" },
  ]);

  await expect(page.getByTitle(ROW_TITLE)).toHaveCount(3, { timeout: 5_000 });

  const filterInput = page.locator('input[type="text"][placeholder*="Filter"]').first();
  await filterInput.fill("tag:SaveAlpha tag:Beta ");

  await expect(page.getByText("Saved disabled alpha beta row")).toBeVisible({ timeout: 5_000 });
  await expect(page.getByText("Saved disabled alpha row")).not.toBeVisible();
  await expect(page.getByText("Saved disabled beta row")).not.toBeVisible();

  await page.getByRole("button", { name: "Disable filter tag:Beta" }).click();
  await expect(page.getByText("Saved disabled alpha row")).toBeVisible({ timeout: 5_000 });

  await page.getByTitle("Save current filter").click();
  await page.getByPlaceholder("Filter name…").fill("Effective Alpha E2E");
  await page.getByRole("button", { name: "Save" }).last().click();

  await page.getByTitle("Clear all filters").first().click();
  await page.getByTitle("Filter presets").click();
  await page.getByTestId("logcat-filter-quick-row").getByText("Effective Alpha E2E").click();

  await expect(page.getByText("Saved disabled alpha row")).toBeVisible({ timeout: 5_000 });
  await expect(page.getByText("Saved disabled alpha beta row")).toBeVisible();
  await expect(page.getByText("Saved disabled beta row")).not.toBeVisible();
});

test("logcat filter variables resolve from the variable row", async ({ page }) => {
  await openLogcatWithEmptyEntries(page);
  await pushLogcatEntries(page, [
    { id: 150, tag: "VariableAction", message: "action_checkout_done" },
    { id: 151, tag: "VariableAction", message: "action_login_done" },
  ]);

  await expect(page.getByTitle(ROW_TITLE)).toHaveCount(2, { timeout: 5_000 });

  const filterInput = page.locator('input[type="text"][placeholder*="Filter"]').first();
  await filterInput.fill("message:action_${action_name}_done ");

  await expect(page.getByTestId("logcat-filter-variable-row")).toBeVisible({ timeout: 5_000 });
  await expect(page.getByText("action_checkout_done")).not.toBeVisible();
  await expect(page.getByText("action_login_done")).not.toBeVisible();

  await page.getByTitle("Variable action_name").fill("checkout");

  await expect(page.getByText("action_checkout_done")).toBeVisible({ timeout: 5_000 });
  await expect(page.getByText("action_login_done")).not.toBeVisible();

  await page.getByTitle("Save current filter").click();
  await page.getByPlaceholder("Filter name…").fill("Variable Action E2E");
  await page.getByRole("button", { name: "Save" }).last().click();

  await page.getByTitle("Clear all filters").first().click();
  await expect(page.getByText("action_checkout_done")).toBeVisible({ timeout: 5_000 });
  await expect(page.getByText("action_login_done")).toBeVisible();

  await page.getByTitle("Filter presets").click();
  await page.getByTestId("logcat-filter-quick-row").getByText("Variable Action E2E").click();

  await expect(page.getByTestId("logcat-filter-variable-row")).toBeVisible({ timeout: 5_000 });
  await expect(page.getByText("action_checkout_done")).toBeVisible({ timeout: 5_000 });
  await expect(page.getByText("action_login_done")).not.toBeVisible();

  await page.getByTitle("Variable action_name").fill("login");

  await expect(page.getByText("action_login_done")).toBeVisible({ timeout: 5_000 });
  await expect(page.getByText("action_checkout_done")).not.toBeVisible();
});

test("logcat variables can be created before they are used in a filter", async ({ page }) => {
  await openLogcatWithEmptyEntries(page);
  await pushLogcatEntries(page, [
    { id: 155, tag: "PrecreatedVariable", message: "action_checkout" },
    { id: 156, tag: "PrecreatedVariable", message: "action_login" },
  ]);

  await expect(page.getByTitle(ROW_TITLE)).toHaveCount(2, { timeout: 5_000 });

  await page.getByTitle("Manage filter variables").click();
  await page.getByPlaceholder("variable_name").fill("action_name");
  await page.getByPlaceholder("Initial value").fill("checkout");
  await page.getByRole("button", { name: "Add variable" }).click();
  await expect(
    page.getByTestId("logcat-filter-variable-row").getByTitle("Variable action_name")
  ).toHaveValue("checkout");

  await page.mouse.click(5, 5);
  await expect(page.getByTestId("logcat-variable-manager")).not.toBeVisible();
  await page.locator('input[type="text"][placeholder*="Filter"]').first().fill("message:action_");
  await page.getByTitle("Manage filter variables").click();
  await page.getByTitle("Insert variable action_name").click();

  await expect(page.getByText("action_checkout")).toBeVisible({ timeout: 5_000 });
  await expect(page.getByText("action_login")).not.toBeVisible();
});

test("logcat filter variables work inside quoted OR group filters", async ({ page }) => {
  await openLogcatWithEmptyEntries(page);
  await pushLogcatEntries(page, [
    { id: 160, tag: "VariableAction", message: "User checkout button clicked" },
    { id: 161, tag: "VariableAction", message: "User login button clicked" },
    { id: 162, tag: "VariableFallback", message: "fallback variable row" },
  ]);

  await expect(page.getByTitle(ROW_TITLE)).toHaveCount(3, { timeout: 5_000 });

  const filterInput = page.locator('input[type="text"][placeholder*="Filter"]').first();
  await filterInput.fill('message:"User ${action_name} clicked" | tag:VariableFallback ');

  await expect(page.getByTestId("logcat-filter-variable-row")).toBeVisible({ timeout: 5_000 });
  await expect(page.getByText("fallback variable row")).toBeVisible();
  await expect(page.getByText("User checkout button clicked")).not.toBeVisible();
  await expect(page.getByText("User login button clicked")).not.toBeVisible();

  await page.getByTitle("Variable action_name").fill("checkout button");

  await expect(page.getByText("User checkout button clicked")).toBeVisible({ timeout: 5_000 });
  await expect(page.getByText("fallback variable row")).toBeVisible();
  await expect(page.getByText("User login button clicked")).not.toBeVisible();
});

test("logcat disabled variable pills stay editable and stop affecting filtering", async ({ page }) => {
  await openLogcatWithEmptyEntries(page);
  await pushLogcatEntries(page, [
    { id: 170, tag: "VariableAlpha", message: "alpha_checkout_done" },
    { id: 171, tag: "VariableAlpha", message: "alpha_login_done" },
    { id: 172, tag: "VariableBeta", message: "beta_checkout_done" },
  ]);

  await expect(page.getByTitle(ROW_TITLE)).toHaveCount(3, { timeout: 5_000 });

  const filterInput = page.locator('input[type="text"][placeholder*="Filter"]').first();
  await filterInput.fill("tag:VariableAlpha message:${action_name}_checkout_done ");
  await page.getByTitle("Variable action_name").fill("alpha");

  await expect(page.getByText("alpha_checkout_done")).toBeVisible({ timeout: 5_000 });
  await expect(page.getByText("alpha_login_done")).not.toBeVisible();
  await expect(page.getByText("beta_checkout_done")).not.toBeVisible();

  await page
    .getByRole("button", { name: "Disable filter message:${action_name}_checkout_done" })
    .click();

  await expect(page.getByTestId("logcat-filter-variable-row")).toBeVisible();
  await expect(page.getByText("message:${action_name}_checkout_done")).toBeVisible();
  await expect(page.getByText("alpha_checkout_done")).toBeVisible({ timeout: 5_000 });
  await expect(page.getByText("alpha_login_done")).toBeVisible();
  await expect(page.getByText("beta_checkout_done")).not.toBeVisible();

  await page.getByTitle("Variable action_name").fill("beta");

  await expect(page.getByText("alpha_checkout_done")).toBeVisible({ timeout: 5_000 });
  await expect(page.getByText("alpha_login_done")).toBeVisible();
  await expect(page.getByText("beta_checkout_done")).not.toBeVisible();
});

test("logcat selected row stays frozen while the live buffer overflows", async ({ page }) => {
  await page.evaluate(() => {
    window.__keynobi_e2e_settings_overrides = {
      logcat: {
        autoStart: false,
        autoScrollToEnd: true,
        outputFontSize: 11,
        maxUiLines: 5000,
        ringMaxEntries: 50000,
      },
    };
  });
  await page.reload();
  await page.waitForFunction(() => typeof window.__e2e__ !== "undefined", { timeout: 10_000 });
  await openLogcatWithEmptyEntries(page);
  await pushLogcatEntries(page, makeScrollEntries(0, 5000));

  const scroller = page.getByTestId("logcat-virtual-list");
  await expect(scroller).toBeVisible({ timeout: 5_000 });
  await scroller.evaluate((element) => {
    element.scrollTop = 0;
    element.dispatchEvent(new Event("scroll", { bubbles: true }));
  });

  await expect(page.getByText("Auto row 000")).toBeVisible({ timeout: 5_000 });
  await page.getByText("Auto row 000").click();

  await pushLogcatEntries(page, makeScrollEntries(5000, 120));

  await expect(scroller.getByText("Auto row 000")).toBeVisible({ timeout: 5_000 });
  await expect(scroller.getByText("Auto row 5000")).not.toBeVisible();
  await expect(page.getByText("120 new")).toBeVisible({ timeout: 5_000 });

  await page.getByTitle("120 new logs available - Jump to end").click();
  await expect(scroller.getByText("Auto row 5119")).toBeVisible({ timeout: 5_000 });
  await expect(scroller.getByText("Auto row 000")).not.toBeVisible();

  await scroller.evaluate((element) => {
    element.scrollTop = 0;
    element.dispatchEvent(new Event("scroll", { bubbles: true }));
  });
  await expect(scroller.getByText("Auto row 120")).toBeVisible({ timeout: 5_000 });
  await expect(scroller.getByText("Auto row 119")).not.toBeVisible();
});

test("logcat package filter dropdown closes when the window loses focus", async ({ page }) => {
  await page.getByRole("tab", { name: "Logcat" }).click();
  await page.evaluate(async () => {
    await window.__e2e__.invoke("start_logcat", {});
  });
  await expect(page.getByText("Activity started").first()).toBeVisible({ timeout: 5_000 });

  await page.getByTitle("Filter by package").click();
  await expect(page.getByPlaceholder("Search packages…")).toBeVisible();

  await page.evaluate(() => window.dispatchEvent(new Event("blur")));

  await expect(page.getByPlaceholder("Search packages…")).not.toBeVisible();
});
