import { test, expect } from "../fixtures/app";

test.describe("build IPC commands", () => {
  test("get_build_status returns idle on fresh load", async ({ page }) => {
    const status = await page.evaluate(async () => {
      return window.__e2e__.invoke("get_build_status") as Promise<{ state: string }>;
    });
    expect((status as { state: string }).state).toBe("idle");
  });

  test("get_build_errors returns empty array", async ({ page }) => {
    const errors = await page.evaluate(async () => window.__e2e__.invoke("get_build_errors"));
    expect(Array.isArray(errors)).toBe(true);
  });

  test("get_variants_preview returns variant list shape", async ({ page }) => {
    const variants = await page.evaluate(async () => {
      const result = await window.__e2e__.invoke("get_variants_preview");
      return result as {
        variants: Array<{ name: string; assembleTask: string }>;
        active: string | null;
      };
    });
    expect(Array.isArray(variants.variants)).toBe(true);
    expect(variants.variants[0]).toMatchObject({
      name: expect.any(String),
      assembleTask: expect.any(String),
    });
  });

  test("cancel_build transitions status to cancelled", async ({ page }) => {
    await page.evaluate(async () => window.__e2e__.invoke("cancel_build"));
    const status = await page.evaluate(async () => {
      return window.__e2e__.invoke("get_build_status") as Promise<{ state: string }>;
    });
    expect((status as { state: string }).state).toBe("cancelled");
  });
});
