import { render, screen } from "@solidjs/testing-library";
import { invoke } from "@tauri-apps/api/core";
import { beforeEach, describe, expect, it, vi } from "vitest";
import type { McpServerStatus } from "@/bindings";
import { loadMcpActivity, resetMcpStateForTests } from "@/stores/mcp.store";
import { mcpSessionLines } from "@/components/mcp/McpPanel";
import { McpStatusIndicator } from "./StatusBar";

async function withStatus(status: McpServerStatus): Promise<void> {
  vi.mocked(invoke).mockImplementation(async (cmd: string) => {
    if (cmd === "get_mcp_activity") return [];
    if (cmd === "get_mcp_server_status") return status;
    return undefined;
  });
  await loadMcpActivity();
}

const session = {
  id: 1,
  pid: 1234,
  project: null,
  connectedAt: "2026-01-01T00:00:00Z",
  clientName: "claude-code",
  version: "1.0.0",
};

const standaloneServer = {
  pid: 99,
  startedAt: "2026-01-01T00:00:00Z",
  project: "/p/app",
  reason: "Keynobi has another project open (/p/other); this MCP server is for /p/app",
  version: "1.0.0",
};

describe("McpStatusIndicator", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    resetMcpStateForTests();
  });

  it("shows plain MCP when no AI client is connected", () => {
    render(() => <McpStatusIndicator />);
    const button = screen.getByRole("button", { name: /MCP — click to set up/ });
    expect(button.textContent).toBe("MCP");
  });

  it("counts agents attached to the app", async () => {
    await withStatus({
      listening: true,
      appVersion: "1.0.0",
      attached: [session, { ...session, id: 2, pid: 5678 }],
      standalone: [],
    });
    render(() => <McpStatusIndicator />);

    const button = screen.getByRole("button", { name: /2 agents connected/ });
    expect(button.textContent).toBe("MCP: 2 agents");
    expect(screen.getByRole("img", { name: "ok" })).toBeTruthy();
  });

  it("warns about standalone servers that the app does not see", async () => {
    await withStatus({
      listening: true,
      appVersion: "1.0.0",
      attached: [],
      standalone: [standaloneServer],
    });
    render(() => <McpStatusIndicator />);

    const button = screen.getByRole("button", {
      name: /1 standalone server \(not shared with the app\)/,
    });
    expect(button.textContent).toBe("MCP: 1 standalone");
    expect(screen.getByRole("img", { name: "warning" })).toBeTruthy();
  });

  it("warns when an attached agent runs another Keynobi version", async () => {
    await withStatus({
      listening: true,
      appVersion: "1.1.0",
      attached: [session],
      standalone: [],
    });
    render(() => <McpStatusIndicator />);

    const button = screen.getByRole("button", {
      name: /1 MCP server runs a different Keynobi version \(1\.0\.0\) than the app \(1\.1\.0\)\. Restart your AI client/,
    });
    expect(button.textContent).toBe("MCP: 1 agent");
    expect(screen.getByRole("img", { name: "warning" })).toBeTruthy();
  });

  it("does not warn when every agent runs the app's version", async () => {
    await withStatus({ listening: true, appVersion: "1.0.0", attached: [session], standalone: [] });
    render(() => <McpStatusIndicator />);

    const button = screen.getByRole("button", { name: /1 agent connected/ });
    expect(button.getAttribute("title")).not.toMatch(/different Keynobi version/);
    expect(screen.getByRole("img", { name: "ok" })).toBeTruthy();
  });
});

describe("mcpSessionLines", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    resetMcpStateForTests();
  });

  it("describes each attached client and standalone server", async () => {
    await withStatus({
      listening: true,
      appVersion: "1.0.0",
      attached: [session, { ...session, id: 2, clientName: null, project: "/p/app" }],
      standalone: [standaloneServer],
    });

    expect(mcpSessionLines()).toEqual([
      "claude-code (Keynobi 1.0.0) — follows the app",
      "AI client (Keynobi 1.0.0) — /p/app",
      "Standalone server (PID 99, Keynobi 1.0.0) — /p/app: Keynobi has another project open (/p/other); this MCP server is for /p/app. Its builds and logcat are not shown here.",
    ]);
  });
});
