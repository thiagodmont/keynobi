import { test, expect } from "../fixtures/app";

test.describe("device IPC commands", () => {
  test("list_adb_devices returns array with required fields", async ({ page }) => {
    const devices = await page.evaluate(async () => {
      const result = await window.__e2e__.invoke("list_adb_devices");
      return result as Array<{
        serial: string;
        name: string;
        deviceKind: string;
        connectionState: string;
      }>;
    });

    expect(Array.isArray(devices)).toBe(true);
    expect(devices.length).toBeGreaterThan(0);
    expect(devices[0]).toMatchObject({
      serial: expect.any(String),
      name: expect.any(String),
      deviceKind: expect.stringMatching(/^(physical|emulator)$/),
      connectionState: expect.stringMatching(/^(online|offline|unauthorized|unknown)$/),
    });
  });

  test("select_device then get_selected_device roundtrip", async ({ page }) => {
    await page.evaluate(async () => {
      await window.__e2e__.invoke("select_device", { serial: "emulator-5554" });
    });
    const selected = await page.evaluate(async () => {
      return window.__e2e__.invoke("get_selected_device");
    });
    expect(selected).toBe("emulator-5554");
  });

  test("get_exit_reasons returns the history for the requested package", async ({ page }) => {
    const result = (await page.evaluate(async () => {
      return window.__e2e__.invoke("get_exit_reasons", {
        serial: "emulator-5554",
        package: "com.example.other",
      });
    })) as { serial: string; package: string; supported: boolean; records: unknown[] };

    expect(result).toMatchObject({
      serial: "emulator-5554",
      package: "com.example.other",
      supported: true,
    });
    expect(result.records.length).toBeGreaterThan(0);
  });

  test("list_avd_devices returns avd with required fields", async ({ page }) => {
    const avds = await page.evaluate(async () => {
      const result = await window.__e2e__.invoke("list_avd_devices");
      return result as Array<{ name: string; displayName: string }>;
    });
    expect(avds[0]).toMatchObject({
      name: expect.any(String),
      displayName: expect.any(String),
    });
  });
});
