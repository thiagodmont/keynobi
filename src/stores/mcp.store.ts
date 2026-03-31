/**
 * MCP server lifecycle state.
 *
 * Tracks whether the MCP stdio server is running, which client connected,
 * and when the connection was established. Used by StatusBar and HealthPanel.
 */
import { createStore } from "solid-js/store";
import { listen } from "@tauri-apps/api/event";

export interface McpState {
  running: boolean;
  clientName: string | null;
  connectedAt: string | null;
  transport: "stdio" | "http" | null;
}

const [mcpState, setMcpState] = createStore<McpState>({
  running: false,
  clientName: null,
  connectedAt: null,
  transport: null,
});

export { mcpState };

export function initMcpListeners() {
  listen<{ transport: string }>("mcp:started", (event) => {
    setMcpState({
      running: true,
      transport: (event.payload.transport as "stdio" | "http") ?? "stdio",
      clientName: null,
      connectedAt: null,
    });
  });

  listen<{ clientName: string; connectedAt: string }>(
    "mcp:client_connected",
    (event) => {
      setMcpState({
        clientName: event.payload.clientName,
        connectedAt: event.payload.connectedAt,
      });
    }
  );

  listen("mcp:stopped", () => {
    setMcpState({
      running: false,
      clientName: null,
      connectedAt: null,
      transport: null,
    });
  });
}
