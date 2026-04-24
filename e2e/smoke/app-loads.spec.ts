import { test, expect } from "../fixtures/app";

test("app loads and mock backend is available", async ({ page }) => {
  const hasBridge = await page.evaluate(() => typeof window.__e2e__ !== "undefined");
  expect(hasBridge).toBe(true);
});
