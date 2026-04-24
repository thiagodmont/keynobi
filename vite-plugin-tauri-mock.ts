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
        "@tauri-apps/plugin-fs": "\0tauri-mock-fs",
        "@tauri-apps/plugin-shell": "\0tauri-mock-shell",
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
export const save = () => Promise.resolve(null);
`;
      }
      if (id === "\0tauri-mock-fs") {
        return `
export const writeTextFile = () => Promise.resolve();
export const readTextFile = () => Promise.resolve("");
export const exists = () => Promise.resolve(false);
export const mkdir = () => Promise.resolve();
export const remove = () => Promise.resolve();
export default {};
`;
      }
      if (id === "\0tauri-mock-shell") return `export default {};`;
      return null;
    },
  };
}
