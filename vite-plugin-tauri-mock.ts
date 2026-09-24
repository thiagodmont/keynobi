import type { Plugin } from "vite";
// @ts-expect-error process is a nodejs global
import { resolve } from "path";

// @ts-expect-error process is a nodejs global
const mockDir = resolve(process.cwd(), "src/test/mock-backend");

function ref(file: string, namedExports: string): string {
  const abs = resolve(mockDir, file);
  return `export { ${namedExports} } from ${JSON.stringify(abs)};`;
}

export function tauriMockPlugin(): Plugin {
  return {
    name: "vite-plugin-tauri-mock",
    enforce: "pre",
    resolveId(id) {
      const map: Record<string, string> = {
        "@tauri-apps/api/core": "\0tauri-mock-core",
        "@tauri-apps/api/event": "\0tauri-mock-event",
        "@tauri-apps/api/window": "\0tauri-mock-window",
        "@tauri-apps/api/app": "\0tauri-mock-app",
        "@tauri-apps/plugin-dialog": "\0tauri-mock-dialog",
      };
      return map[id] ?? null;
    },
    load(id) {
      if (id === "\0tauri-mock-core") {
        return [
          ref("index", "handleInvoke as invoke"),
          ref("channel", "MockChannel as Channel"),
        ].join("\n");
      }
      if (id === "\0tauri-mock-event") {
        return ref("events", "handleListen as listen, handleEmit as emit");
      }
      if (id === "\0tauri-mock-window") {
        return `
export const getCurrentWindow = () => ({
  startDragging: () => Promise.resolve(),
  setTitle: () => Promise.resolve(),
  isAlwaysOnTop: () => Promise.resolve(false),
  setAlwaysOnTop: () => Promise.resolve(),
  isMaximized: () => Promise.resolve(false),
  maximize: () => Promise.resolve(),
  unmaximize: () => Promise.resolve(),
  minimize: () => Promise.resolve(),
  close: () => Promise.resolve(),
});
`;
      }
      if (id === "\0tauri-mock-app") {
        return `export const getVersion = () => Promise.resolve("0.0.0-e2e");`;
      }
      if (id === "\0tauri-mock-dialog") {
        return `
export const open = () => Promise.resolve(null);
`;
      }
      return null;
    },
  };
}
