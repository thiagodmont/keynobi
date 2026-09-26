import { test, expect } from "../fixtures/app";

test("Deobfuscate in Entry Detail shows the crash's stack with the mapping it used", async ({
  page,
}) => {
  await page.getByRole("tab", { name: "Logcat" }).click();
  await page.evaluate(async () => {
    await window.__e2e__.invoke("clear_logcat", {});
    const line = (id: number, message: string) => ({
      id,
      timestamp: "2026-05-06T12:00:00.000Z",
      pid: 1234,
      tid: 1234,
      level: "error",
      tag: "AndroidRuntime",
      message,
      package: "com.example.mockapp",
      kind: "normal",
      isCrash: true,
      flags: 1,
      category: "general",
      crashGroupId: 500,
      jsonBody: null,
    });
    await window.__e2e__.invoke("__e2e_append_logcat_entries", {
      entries: [
        line(500, "FATAL EXCEPTION: main"),
        line(501, "java.lang.RuntimeException: boom"),
        line(502, "\tat a.a.b(r8-map-id-6b1c2f0:24)"),
      ],
    });
  });

  await page.getByText("java.lang.RuntimeException: boom").click();
  await page.getByRole("button", { name: "Deobfuscate" }).click();

  await expect(page.getByText("Deobfuscated", { exact: true })).toBeVisible();
  await expect(
    page.getByText(/R8 mapping of build #12 \(:app release, map id 6b1c2f0\), matched by map id/)
  ).toBeVisible();
  await expect(page.getByLabel("Deobfuscated stack")).toContainText(
    "at com.example.mockapp.MainActivity.onCreate(MainActivity.kt:24)"
  );
});
