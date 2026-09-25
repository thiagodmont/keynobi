import { test as base, type Locator, type Page } from "@playwright/test";
import { test, expect } from "../fixtures/app";

// Every step below uses the keyboard only: Tab, arrows, Enter, Escape, and shortcuts.

/** Press Tab until `target` has focus, as a keyboard user would. */
async function tabTo(page: Page, target: Locator, maxPresses = 250): Promise<void> {
  await expect(target).toBeVisible();
  for (let i = 0; i < maxPresses; i++) {
    if (await target.evaluate((el) => el === document.activeElement)) return;
    await page.keyboard.press("Tab");
  }
  throw new Error(`Tab never reached ${target}`);
}

function projectOption(page: Page): Locator {
  return page
    .getByRole("listbox", { name: "Projects" })
    .getByRole("option", { name: /MockProject/ });
}

async function openProjectWithKeyboard(page: Page): Promise<void> {
  await tabTo(page, projectOption(page));
  await page.keyboard.press("Enter");
  await expect(page.getByText("Keynobi — MockProject")).toBeVisible({ timeout: 5_000 });
  await expect(projectOption(page)).toHaveAttribute("aria-selected", "true");
}

function buildsList(page: Page): Locator {
  return page.getByRole("listbox", { name: "Builds" });
}

async function focusedRole(page: Page): Promise<string | null> {
  return page.evaluate(() => document.activeElement?.getAttribute("role") ?? null);
}

test("select a project and a device, then run and cancel a build", async ({ page }) => {
  await page.evaluate(() => window.__e2e__.setAppBuildLineDelay(5_000));

  await openProjectWithKeyboard(page);

  // The first online device is picked for you; choose the other one.
  const devices = page.getByRole("listbox", { name: "Connected devices" });
  const emulator = devices.getByRole("option", { name: /sdk_gphone64_x86_64/ });
  const phone = devices.getByRole("option", { name: /Pixel 7/ });
  await expect(emulator).toHaveAttribute("aria-selected", "true");
  await tabTo(page, emulator);
  await page.keyboard.press("ArrowDown");
  await expect(phone).toBeFocused();
  await page.keyboard.press("Enter");
  await expect(phone).toHaveAttribute("aria-selected", "true");
  await expect(emulator).toHaveAttribute("aria-selected", "false");

  await page.keyboard.press("Meta+1");
  await expect(page.getByRole("tab", { name: "Build" })).toHaveAttribute("aria-selected", "true");
  await page.keyboard.press("Meta+Shift+R");
  await expect(page.getByText("Building…", { exact: true })).toBeVisible({ timeout: 5_000 });

  await page.keyboard.press("Meta+Shift+P");
  await page.keyboard.type("Cancel Build");
  await expect(page.getByRole("option", { name: /Cancel Build/ })).toBeVisible();
  await page.keyboard.press("Enter");

  await expect(page.getByText("Build cancelled", { exact: true })).toBeVisible({ timeout: 5_000 });
});

test("open a past build from history and read its log and problems", async ({ page }) => {
  await page.evaluate(() => {
    window.__e2e__.addPastBuild({
      task: "assembleRelease",
      state: "failed",
      minutesAgo: 30,
      errors: [
        {
          message: "Unresolved reference: Foo",
          file: "app/src/main/java/com/example/Foo.kt",
          line: 12,
          col: 5,
          severity: "error",
        },
      ],
      lines: [
        {
          kind: "taskStart",
          content: "> Task :app:compileReleaseKotlin",
          file: null,
          line: null,
          col: null,
        },
        { kind: "summary", content: "BUILD FAILED in 9s", file: null, line: null, col: null },
      ],
    });
    // Its log was removed by retention.
    window.__e2e__.addPastBuild({
      task: "assembleDebug",
      state: "success",
      minutesAgo: 10,
      lines: null,
    });
  });
  await openProjectWithKeyboard(page);
  await page.keyboard.press("Meta+1");

  // One tab stop for the whole list; the newest build comes first.
  const newest = buildsList(page).getByRole("option").first();
  await tabTo(page, newest);
  await page.keyboard.press("ArrowDown");
  const release = buildsList(page).getByRole("option", { name: /assembleRelease/ });
  await expect(release).toBeFocused();
  await page.keyboard.press("Enter");

  await expect(page.getByText(/^Viewing build #1 from /)).toBeVisible();
  await expect(page.getByText("BUILD FAILED in 9s")).toBeVisible();
  await expect(page.getByText("Build failed in 4.0s — 1 error")).toBeVisible();

  const problems = page.getByRole("button", { name: "Problems (1)" });
  await tabTo(page, problems);
  await page.keyboard.press("Enter");
  await expect(page.getByText("Unresolved reference: Foo")).toBeVisible();

  // Back in the list (the viewed build is its tab stop), move to the build whose log expired.
  await tabTo(page, release);
  await page.keyboard.press("ArrowUp");
  await page.keyboard.press("Enter");
  await expect(page.getByText(/^Viewing build #2 from /)).toBeVisible();
  await tabTo(page, page.getByRole("button", { name: "Log", exact: true }));
  await page.keyboard.press("Enter");
  await expect(page.getByText("This build's log was removed")).toBeVisible();

  await tabTo(page, page.getByRole("button", { name: "Back to current build" }));
  await page.keyboard.press("Enter");
  await expect(page.getByTestId("build-history-banner")).toBeHidden();
  expect(await focusedRole(page)).toBe("option");
});

test("an agent's build does not replace the past build being read", async ({ page }) => {
  await page.evaluate(() => {
    window.__e2e__.addPastBuild({ task: "assembleRelease", state: "success", minutesAgo: 30 });
  });
  await openProjectWithKeyboard(page);
  await page.keyboard.press("Meta+1");
  await tabTo(page, buildsList(page).getByRole("option").first());
  await page.keyboard.press("Enter");
  await expect(page.getByText(/^Viewing build #1 from /)).toBeVisible();

  await page.evaluate(() => window.__e2e__.startAgentBuild("assembleDebug", "Claude Code", 5_000));

  await expect(
    page.getByText("A build started by an agent (Claude Code) is running")
  ).toBeVisible();
  await expect(page.getByText(/^Viewing build #1 from /)).toBeVisible();

  await tabTo(page, page.getByRole("button", { name: "Show running build" }));
  await page.keyboard.press("Enter");
  await expect(page.getByTestId("build-history-banner")).toBeHidden();
  await expect(page.getByText("Building… · Started by an agent (Claude Code)")).toBeVisible();
});

test("dialogs close on Escape, return focus, and hold back shortcuts while open", async ({
  page,
}) => {
  await openProjectWithKeyboard(page);
  const project = projectOption(page);
  await expect(project).toBeFocused();

  // Settings: focus moves in, a background shortcut does nothing, Escape closes.
  await page.keyboard.press("Meta+,");
  const settings = page.getByRole("dialog", { name: "Settings" });
  await expect(settings).toBeVisible();
  expect(await settings.evaluate((el) => el.contains(document.activeElement))).toBe(true);
  await page.keyboard.press("Meta+Shift+R");
  await page.keyboard.press("Meta+1");
  await page.keyboard.press("Escape");
  await expect(settings).toBeHidden();
  await expect(project).toBeFocused();
  await expect(page.getByRole("tab", { name: "Logcat" })).toHaveAttribute("aria-selected", "true");
  const status = await page.evaluate(() => window.__e2e__.invoke("get_build_status"));
  expect((status as { state: string }).state).toBe("idle");

  // Command palette.
  await page.keyboard.press("Meta+Shift+P");
  await expect(page.getByRole("dialog", { name: "Command Palette" })).toBeVisible();
  await page.keyboard.press("Escape");
  await expect(page.getByRole("dialog", { name: "Command Palette" })).toBeHidden();
  await expect(project).toBeFocused();

  // Health Center.
  await page.keyboard.press("Meta+Shift+H");
  await expect(page.getByRole("dialog", { name: "Health Center" })).toBeVisible();
  await page.keyboard.press("Escape");
  await expect(page.getByRole("dialog", { name: "Health Center" })).toBeHidden();
  await expect(project).toBeFocused();

  // A row's actions menu, then a confirmation dialog opened from another menu.
  await page.keyboard.press("Shift+F10");
  await expect(page.getByRole("menu", { name: "Actions for MockProject" })).toBeVisible();
  await page.keyboard.press("Escape");
  await expect(page.getByRole("menu")).toBeHidden();
  await expect(project).toBeFocused();

  const more = page.getByRole("button", { name: "More options for Pixel 6 API 34" });
  await tabTo(page, more);
  await page.keyboard.press("Enter");
  await expect(page.getByRole("menuitem", { name: "Wipe Data…" })).toBeVisible();
  await page.keyboard.press("ArrowDown");
  await page.keyboard.press("Enter");
  const confirm = page.getByRole("dialog", { name: "Wipe Device Data" });
  await expect(confirm).toBeVisible();
  await expect(confirm.getByRole("button", { name: "Cancel" })).toBeFocused();
  await page.keyboard.press("Escape");
  await expect(confirm).toBeHidden();
  await expect(more).toBeFocused();
});

// Run App needs a device when Auto Install on Build is on.
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

deployTest("with no device online, start an emulator from the Run App dialog", async ({ page }) => {
  await openProjectWithKeyboard(page);
  const project = projectOption(page);
  await expect(page.getByTitle(/^Active build variant: debug/)).toBeVisible();
  await page.evaluate(() =>
    window.__e2e__.triggerEvent("device:list_changed", {
      devices: [
        {
          serial: "emulator-5554",
          name: "Pixel 6 API 34",
          model: "sdk_gphone64_x86_64",
          deviceKind: "emulator",
          connectionState: "offline",
          apiLevel: 34,
          androidVersion: "14",
        },
      ],
    })
  );

  await page.keyboard.press("Meta+R");
  const picker = page.getByRole("dialog", { name: "Select a Device" });
  await expect(picker).toBeVisible();
  await page.keyboard.press("Escape");
  await expect(picker).toBeHidden();
  await expect(project).toBeFocused();

  await page.keyboard.press("Meta+R");
  await expect(picker).toBeVisible();
  await expect(picker.getByRole("button", { name: /Pixel 6 API 34/ })).toBeFocused();
  await page.keyboard.press("Enter");
  await expect(picker).toBeHidden();

  await expect(page.getByText("BUILD SUCCESSFUL in 4s")).toBeVisible({ timeout: 10_000 });
  await expect(page.getByText(/Launch: Started/)).toBeVisible();
});
