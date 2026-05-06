import { test, expect } from "@playwright/test";

test("app load does not emit Solid owner warnings", async ({ page }) => {
  const solidOwnerWarnings: string[] = [];

  page.on("console", (message) => {
    if (
      message.type() === "warning" &&
      message.text().includes("computations created outside a `createRoot` or `render`")
    ) {
      solidOwnerWarnings.push(message.text());
    }
  });

  await page.goto("/");
  await page.waitForFunction(() => typeof window.__e2e__ !== "undefined", { timeout: 10_000 });

  expect(solidOwnerWarnings).toEqual([]);
});
