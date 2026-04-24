import { test, expect } from "../fixtures/app";

test.describe("settings IPC commands", () => {
  test("get_settings returns AppSettings shape", async ({ page }) => {
    const settings = await page.evaluate(async () => {
      return window.__e2e__.invoke("get_settings") as Promise<{
        onboardingCompleted: boolean;
        appearance: { uiFontSize: number };
        build: { autoInstallOnBuild: boolean };
        logcat: { autoStart: boolean };
      }>;
    });
    expect(settings).toMatchObject({
      onboardingCompleted: expect.any(Boolean),
      appearance: { uiFontSize: expect.any(Number) },
      build: { autoInstallOnBuild: expect.any(Boolean) },
      logcat: { autoStart: expect.any(Boolean) },
    });
  });

  test("save_settings then get_settings reflects the saved value", async ({ page }) => {
    await page.evaluate(async () => {
      const settings = (await window.__e2e__.invoke("get_settings")) as Record<string, unknown>;
      await window.__e2e__.invoke("save_settings", {
        settings: { ...settings, appearance: { uiFontSize: 20 } },
      });
    });
    const after = await page.evaluate(async () => {
      const settings = (await window.__e2e__.invoke("get_settings")) as {
        appearance: { uiFontSize: number };
      };
      return settings.appearance.uiFontSize;
    });
    expect(after).toBe(20);
  });

  test("reset_settings restores defaults", async ({ page }) => {
    await page.evaluate(async () => {
      const settings = (await window.__e2e__.invoke("get_settings")) as Record<string, unknown>;
      await window.__e2e__.invoke("save_settings", {
        settings: { ...settings, appearance: { uiFontSize: 99 } },
      });
    });

    const restored = await page.evaluate(async () => {
      const defaults = (await window.__e2e__.invoke("reset_settings")) as {
        appearance: { uiFontSize: number };
      };
      return defaults.appearance.uiFontSize;
    });
    expect(restored).toBe(14);
  });
});
