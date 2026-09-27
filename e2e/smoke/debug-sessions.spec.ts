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

  await expect(dialog.getByText("3f9c2e1 · main", { exact: true })).toBeVisible();

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

  await dialog.getByRole("button", { name: "Export…" }).click();
  const exportOptions = page.getByRole("dialog", { name: "Export Debug Session" });
  await expect(exportOptions.getByLabel("Email addresses")).toBeChecked();
  await exportOptions.getByLabel("IP addresses (not loopback or 10.0.2.2)").uncheck();
  await exportOptions.getByRole("button", { name: "Save…" }).click();
  await expect(exportOptions).toBeHidden();
  await expect(
    dialog.getByText(
      /^Saved keynobi-session-.+\.zip \(4\.0 KB\)\. Redacted 1 path\. Not redacted: IP addresses\./
    )
  ).toBeVisible();

  await page.keyboard.press("Escape");
  await expect(dialog).toBeHidden();
});

test("Import Debug Session… opens a shared bundle read-only, and it can be deleted", async ({
  page,
}) => {
  await page.keyboard.press("Meta+Shift+P");
  await page.keyboard.type("Import Debug Session");
  await expect(page.getByRole("option", { name: /Import Debug Session…/ })).toBeVisible();
  await page.keyboard.press("Enter");

  const dialog = page.getByRole("dialog", { name: "Debug Sessions" });
  await expect(dialog).toBeVisible();
  const sessions = dialog.getByRole("listbox", { name: "Debug sessions" });
  const imported = sessions.getByRole("option").filter({ hasText: "Imported" });
  await expect(imported).toHaveCount(1);
  await expect(imported).toHaveAttribute("aria-selected", "true");

  await expect(dialog.getByText("Imported session, read-only")).toBeVisible();
  await expect(dialog.getByText(/Left out: R8 mappings/)).toBeVisible();
  for (const name of ["Keep", "End session", "Refresh exit reasons", "Export…", "Add bookmark"]) {
    await expect(dialog.getByRole("button", { name, exact: true })).toHaveCount(0);
  }

  const timeline = dialog.getByRole("listbox", { name: "Timeline, oldest first" });
  await timeline.getByRole("option").filter({ hasText: "<email-1> not found" }).click();
  await dialog.getByRole("button", { name: /^Show log lines/ }).click();
  await expect(dialog.getByText("FATAL EXCEPTION: main")).toBeVisible();

  await dialog.getByRole("button", { name: "Delete", exact: true }).click();
  await dialog.getByRole("button", { name: "Delete session" }).click();
  await expect(sessions.getByRole("option").filter({ hasText: "Imported" })).toHaveCount(0);
});

test("a screenshot attached to the session shows as a thumbnail and can be left out of an export", async ({
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
  await page.keyboard.press("Enter");
  const dialog = page.getByRole("dialog", { name: "Debug Sessions" });
  await expect(dialog).toBeVisible();

  await dialog.getByRole("button", { name: "Attach screenshot" }).click();
  await expect(dialog.getByText("Screenshot attached.")).toBeVisible();
  const strip = dialog.getByRole("group", { name: "Attachments" });
  await expect(strip.getByRole("button")).toHaveCount(1);
  const timeline = dialog.getByRole("listbox", { name: "Timeline, oldest first" });
  await expect(timeline.getByRole("option").filter({ hasText: "Screenshot attached" })).toHaveCount(
    1
  );
  await expect(
    dialog.getByRole("region", { name: "Selected event" }).getByAltText(/^Screenshot from /)
  ).toBeVisible();

  await dialog.getByRole("button", { name: "Export…" }).click();
  const exportOptions = page.getByRole("dialog", { name: "Export Debug Session" });
  const screenshots = exportOptions.getByLabel(
    "Attachments: screenshots (not redacted) and UI hierarchies (redacted)"
  );
  await expect(screenshots).toBeChecked();
  await screenshots.uncheck();
  await exportOptions.getByRole("button", { name: "Save…" }).click();
  await expect(dialog.getByText(/Left out: .*attachments/)).toBeVisible();
});

test("a UI hierarchy attached to the session shows as a tree and is exported redacted", async ({
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
  await page.keyboard.press("Enter");
  const dialog = page.getByRole("dialog", { name: "Debug Sessions" });
  await expect(dialog).toBeVisible();

  await dialog.getByRole("button", { name: "Attach UI hierarchy" }).click();
  await expect(dialog.getByText("UI hierarchy attached.")).toBeVisible();
  const strip = dialog.getByRole("group", { name: "Attachments" });
  await expect(strip.getByRole("button", { name: /^UI hierarchy from / })).toHaveCount(1);
  const timeline = dialog.getByRole("listbox", { name: "Timeline, oldest first" });
  await expect(
    timeline.getByRole("option").filter({ hasText: "UI hierarchy attached · 4 nodes" })
  ).toHaveCount(1);

  const selected = dialog.getByRole("region", { name: "Selected event" });
  const tree = selected.getByRole("tree", { name: "UI hierarchy" });
  await expect(tree.getByRole("treeitem")).toHaveCount(4);
  await expect(tree.getByRole("treeitem").nth(1)).toContainText("#title");
  await expect(tree.getByRole("treeitem").nth(1)).toContainText("Hello, Keynobi");
  await tree.focus();
  await page.keyboard.press("End");
  await page.keyboard.press("ArrowLeft");
  await page.keyboard.press("ArrowLeft");
  await expect(tree.getByRole("treeitem")).toHaveCount(3);

  await dialog.getByRole("button", { name: "Export…" }).click();
  const exportOptions = page.getByRole("dialog", { name: "Export Debug Session" });
  await expect(exportOptions).toContainText("UI hierarchies are text and are redacted");
  await exportOptions.getByRole("button", { name: "Save…" }).click();
  await expect(exportOptions).toBeHidden();
  await expect(dialog.getByText(/^Saved keynobi-session-/)).toBeVisible();
});
