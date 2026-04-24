import { settingsHandlers } from "./settings";
import { projectHandlers } from "./projects";
import { devicesHandlers } from "./devices";
import { buildHandlers } from "./build";
import { logcatHandlers } from "./logcat";
import { triggerEvent } from "./events";
export { MockChannel } from "./channel";

type Handler = (args: unknown) => unknown;

const handlers: Map<string, Handler> = new Map(
  Object.entries({
    ...settingsHandlers(),
    ...projectHandlers(),
    ...devicesHandlers(),
    ...buildHandlers(),
    ...logcatHandlers(),
    start_mcp_server: () => undefined,
    get_mcp_setup_status: () => ({
      exePath: "/mock/keynobi",
      setupCommand: "/mock/keynobi --mcp",
      claudeFound: false,
      isConfigured: false,
      configuredCommand: null,
    }),
    configure_mcp_in_claude: () => "Configured",
    get_mcp_activity: () => [],
    get_mcp_server_status: () => ({ alive: false, pid: null }),
    clear_mcp_activity: () => undefined,
  })
);

export async function handleInvoke(command: string, args: unknown = {}): Promise<unknown> {
  const handler = handlers.get(command);
  if (!handler) {
    console.warn(`[mock-backend] unhandled command: ${command}`);
    return undefined;
  }
  return handler(args);
}

if (import.meta.env.VITE_E2E === "true") {
  (window as typeof window & { __e2e__: unknown }).__e2e__ = {
    invoke: handleInvoke,
    triggerEvent,
  };
}
