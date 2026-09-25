import { test, expect } from "../fixtures/app";

test("the status bar counts AI clients attached to the app", async ({ page }) => {
  const indicator = page.getByRole("button", { name: /^MCP/ });
  await expect(indicator).toHaveText("MCP", { timeout: 5_000 });

  const session = (id: number) => ({
    id,
    pid: 1000 + id,
    project: null,
    connectedAt: "2026-01-01T00:00:00Z",
    clientName: "claude-code",
  });
  // The listener registers asynchronously after the app mounts; retry the event.
  await expect(async () => {
    await page.evaluate(
      (sessions) => window.__e2e__.triggerEvent("mcp:sessions_changed", sessions),
      [session(1), session(2)]
    );
    await expect(indicator).toHaveText("MCP: 2 agents", { timeout: 500 });
  }).toPass({ timeout: 5_000 });

  await page.evaluate(() => window.__e2e__.triggerEvent("mcp:sessions_changed", []));
  await expect(indicator).toHaveText("MCP");
});
