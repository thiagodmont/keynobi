import { settingsHandlers } from "./settings";
import { projectHandlers } from "./projects";
import { devicesHandlers } from "./devices";
import { addMockPastBuild, buildHandlers, setMockAppBuildLineDelay, startMockBuild } from "./build";
import { logcatHandlers } from "./logcat";
import { triggerEvent } from "./events";
export { MockChannel } from "./channel";

type Handler = (args: unknown) => unknown;

/** The app version the mock backend reports for MCP sessions. */
export const MOCK_APP_VERSION = "1.0.0-mock";

const handlers: Map<string, Handler> = new Map(
  Object.entries({
    ...settingsHandlers(),
    ...projectHandlers(),
    ...devicesHandlers(),
    ...buildHandlers(),
    ...logcatHandlers(),
    get_mcp_setup_status: () => ({
      exePath: "/mock/keynobi",
      setupCommand: "'/mock/keynobi' --mcp",
      locationProblem: null,
      claude: {
        clientFound: false,
        isConfigured: false,
        configuredCommand: null,
        configuredScope: null,
        setupCommand:
          "claude mcp add --scope user --transport stdio keynobi -- '/mock/keynobi' --mcp",
      },
      codex: {
        clientFound: false,
        isConfigured: false,
        configuredCommand: null,
        configuredScope: null,
        setupCommand: "codex mcp add keynobi -- '/mock/keynobi' --mcp",
      },
    }),
    get_mcp_activity: () => [],
    get_mcp_server_status: () => ({
      listening: true,
      appVersion: MOCK_APP_VERSION,
      attached: [],
      standalone: [],
    }),
    clear_mcp_activity: () => undefined,
  })
);

export async function handleInvoke(command: string, args: unknown = {}): Promise<unknown> {
  // The real IPC layer sends arguments as JSON; fail the same way it would.
  JSON.stringify(args);
  const handler = handlers.get(command);
  if (!handler) {
    throw new Error(`[mock-backend] unhandled command: ${command}`);
  }
  return handler(args);
}

if (import.meta.env.VITE_E2E === "true") {
  (window as typeof window & { __e2e__: unknown }).__e2e__ = {
    invoke: handleInvoke,
    triggerEvent,
    /** A build an attached agent starts, streamed to the app like any other. */
    startAgentBuild: (task: string, clientName: string | null, lineDelayMs?: number) =>
      startMockBuild(
        task,
        { kind: "agent", sessionId: 1, clientName, standalone: false },
        lineDelayMs
      ),
    /** A build already in the history; returns its history ID. */
    addPastBuild: addMockPastBuild,
    setAppBuildLineDelay: setMockAppBuildLineDelay,
  };
}
