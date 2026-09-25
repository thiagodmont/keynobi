import { cleanup, render, screen } from "@solidjs/testing-library";
import { invoke } from "@tauri-apps/api/core";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { McpServerStatus, McpSetupStatus } from "@/bindings";
import { resetMcpStateForTests } from "@/stores/mcp.store";
import { McpPanel, closeMcpPanel, openMcpPanel } from "./McpPanel";

const CLAUDE_COMMAND =
  "claude mcp add --scope user --transport stdio keynobi -- '/Applications/Keynobi.app/Contents/MacOS/keynobi' --mcp";
const CODEX_COMMAND =
  "codex mcp add keynobi -- '/Applications/Keynobi.app/Contents/MacOS/keynobi' --mcp";

function setupStatus(over: Partial<McpSetupStatus> = {}): McpSetupStatus {
  const client = {
    clientFound: true,
    isConfigured: false,
    configuredCommand: null,
    configuredScope: null,
  };
  return {
    exePath: "/Applications/Keynobi.app/Contents/MacOS/keynobi",
    setupCommand: "'/Applications/Keynobi.app/Contents/MacOS/keynobi' --mcp",
    locationProblem: null,
    claude: { ...client, setupCommand: CLAUDE_COMMAND },
    codex: { ...client, setupCommand: CODEX_COMMAND },
    ...over,
  };
}

const idleServer: McpServerStatus = {
  listening: true,
  appVersion: "1.1.0",
  attached: [],
  standalone: [],
};

function withBackend(setup: McpSetupStatus, server: McpServerStatus = idleServer): void {
  vi.mocked(invoke).mockImplementation(async (cmd: string) => {
    if (cmd === "get_mcp_activity") return [];
    if (cmd === "get_mcp_server_status") return server;
    if (cmd === "get_mcp_setup_status") return setup;
    return undefined;
  });
}

describe("McpPanel", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    resetMcpStateForTests();
  });

  afterEach(() => {
    closeMcpPanel();
    cleanup();
  });

  it("offers user-scope setup commands for an installed app", async () => {
    withBackend(setupStatus());
    render(() => <McpPanel />);
    openMcpPanel();

    expect(
      await screen.findByText(new RegExp(`Claude Code: ${escape(CLAUDE_COMMAND)}`))
    ).toBeTruthy();
    expect(screen.getByRole("button", { name: "Copy" })).toBeTruthy();
    expect(screen.queryByText(/Move Keynobi to Applications first/)).toBeNull();
  });

  it("refuses to offer commands when the app runs from a temporary location", async () => {
    const problem =
      "Keynobi is running from a disk image or removable volume (/Volumes/Keynobi), whose path stops working once it is ejected.";
    withBackend(
      setupStatus({
        locationProblem: problem,
        setupCommand: null,
        claude: { ...setupStatus().claude, setupCommand: null },
        codex: { ...setupStatus().codex, setupCommand: null },
      })
    );
    render(() => <McpPanel />);
    openMcpPanel();

    expect(await screen.findByText("Move Keynobi to Applications first")).toBeTruthy();
    expect(screen.getByText(problem)).toBeTruthy();
    expect(screen.queryByRole("button", { name: "Copy" })).toBeNull();
    expect(screen.queryByText(/claude mcp add/)).toBeNull();
  });

  it("shows each server's version and warns when one differs from the app", async () => {
    withBackend(setupStatus(), {
      ...idleServer,
      attached: [
        {
          id: 1,
          pid: 10,
          project: null,
          connectedAt: "2026-01-01T00:00:00Z",
          clientName: "claude-code",
          version: "1.0.0",
        },
      ],
      standalone: [
        {
          pid: 20,
          startedAt: "2026-01-01T00:00:00Z",
          project: null,
          reason: "the Keynobi app is not running",
          version: "1.1.0",
        },
      ],
    });
    render(() => <McpPanel />);
    openMcpPanel();

    expect(await screen.findByText("MCP server version differs from the app")).toBeTruthy();
    expect(
      screen.getByText(
        "1 MCP server runs a different Keynobi version (1.0.0) than the app (1.1.0). Restart your AI client to load the app's version."
      )
    ).toBeTruthy();
    const sessions = screen.getByRole("list", { name: "MCP sessions" });
    expect(sessions.textContent).toContain("claude-code (Keynobi 1.0.0) — follows the app");
    expect(sessions.textContent).toContain("Standalone server (PID 20, Keynobi 1.1.0)");
  });
});

function escape(text: string): string {
  return text.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
}
