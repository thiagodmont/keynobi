import { test, expect } from "../fixtures/app";

test("Show App Exit Reasons lists the selected device's exits and closes on Escape", async ({
  page,
}) => {
  await page.keyboard.press("Meta+Shift+P");
  await page.keyboard.type("Show App Exit Reasons");
  await expect(page.getByRole("option", { name: /Show App Exit Reasons/ })).toBeVisible();
  await page.keyboard.press("Enter");

  const dialog = page.getByRole("dialog", { name: "App Exit Reasons" });
  await expect(dialog).toBeVisible();
  const exits = dialog.getByRole("list", { name: "Process exits, newest first" });
  await expect(exits.getByRole("listitem")).toHaveCount(4);
  await expect(exits.getByRole("listitem").first()).toContainText("Crash");
  await expect(dialog.getByLabel("Package")).toBeFocused();

  await page.keyboard.press("Escape");
  await expect(dialog).toBeHidden();
});
