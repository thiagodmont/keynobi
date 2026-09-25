import { test, expect } from "../fixtures/app";

/** The app version the mock backend reports. */
const APP_VERSION = "1.0.0-mock";

const session = (id: number, version = APP_VERSION) => ({
  id,
  pid: 1000 + id,
  project: null,
  connectedAt: "2026-01-01T00:00:00Z",
  clientName: "claude-code",
  version,
});

test("the status bar counts AI clients attached to the app", async ({ page }) => {
  const indicator = page.getByRole("button", { name: /^MCP/ });
  await expect(indicator).toHaveText("MCP", { timeout: 5_000 });

  // The listener registers asynchronously after the app mounts; retry the event.
  await expect(async () => {
    await page.evaluate(
      (sessions) => window.__e2e__.triggerEvent("mcp:sessions_changed", sessions),
      [session(1), session(2)]
    );
    await expect(indicator).toHaveText("MCP: 2 agents", { timeout: 500 });
  }).toPass({ timeout: 5_000 });
  await expect(indicator).not.toHaveAttribute("title", /different Keynobi version/);

  await page.evaluate(() => window.__e2e__.triggerEvent("mcp:sessions_changed", []));
  await expect(indicator).toHaveText("MCP");
});

test("the status bar warns when an agent runs another Keynobi version", async ({ page }) => {
  const indicator = page.getByRole("button", { name: /^MCP/ });
  await expect(indicator).toHaveText("MCP", { timeout: 5_000 });

  await expect(async () => {
    await page.evaluate(
      (sessions) => window.__e2e__.triggerEvent("mcp:sessions_changed", sessions),
      [session(1, "0.9.0")]
    );
    await expect(indicator).toHaveText("MCP: 1 agent", { timeout: 500 });
  }).toPass({ timeout: 5_000 });
  await expect(indicator).toHaveAttribute(
    "title",
    /different Keynobi version \(0\.9\.0\) than the app \(1\.0\.0-mock\)\. Restart your AI client/
  );
});
