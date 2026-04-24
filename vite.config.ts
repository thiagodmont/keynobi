import { defineConfig } from "vite";
import solid from "vite-plugin-solid";
import { sentryVitePlugin } from "@sentry/vite-plugin";
import { readFileSync } from "fs";
import { resolve } from "path";
import { fileURLToPath } from "url";
import { tauriMockPlugin } from "./vite-plugin-tauri-mock";
/// <reference types="vitest" />

const pkg = JSON.parse(
  readFileSync(fileURLToPath(new URL("./package.json", import.meta.url)), "utf-8")
) as { version: string };

// @ts-expect-error process is a nodejs global
const host = process.env.TAURI_DEV_HOST;

// @ts-expect-error process is a nodejs global
const sentryAuthToken = process.env.SENTRY_AUTH_TOKEN?.trim();
const sentryUploadEnabled = Boolean(sentryAuthToken);
// @ts-expect-error process is a nodejs global
const sentryOrg = process.env.SENTRY_ORG ?? "keynobi";
// @ts-expect-error process is a nodejs global
const sentryProject = process.env.SENTRY_PROJECT ?? "javascript-solid";

export default defineConfig(async () => ({
  plugins: [
    solid(),
    // @ts-expect-error process is a nodejs global
    ...(process.env.VITE_E2E === "true" ? [tauriMockPlugin()] : []),
    sentryVitePlugin({
      disable: !sentryUploadEnabled,
      org: sentryOrg,
      project: sentryProject,
      authToken: sentryAuthToken,
      release: { name: pkg.version },
      sourcemaps: {
        filesToDeleteAfterUpload: sentryUploadEnabled ? ["./dist/**/*.map"] : undefined,
      },
    }),
  ],

  build: {
    // Source maps are generated only when uploading to Sentry (CI release with SENTRY_AUTH_TOKEN).
    sourcemap: sentryUploadEnabled,
  },

  define: {
    "import.meta.env.VITE_APP_VERSION": JSON.stringify(pkg.version),
  },

  resolve: {
    alias: {
      "@": resolve(__dirname, "./src"),
    },
  },

  test: {
    environment: "jsdom",
    globals: true,
    setupFiles: ["./src/test/setup.ts"],
    include: ["src/**/*.test.{ts,tsx}", "scripts/**/*.test.mjs"],
    alias: {
      "@": resolve(__dirname, "./src"),
    },
  },

  clearScreen: false,
  server: {
    // @ts-expect-error process is a nodejs global
    port: process.env.VITE_E2E === "true" ? 1421 : 1420,
    strictPort: true,
    host: host || false,
    hmr: host
      ? {
          protocol: "ws",
          host,
          port: 1421,
        }
      : undefined,
    watch: {
      ignored: ["**/src-tauri/**"],
    },
  },
}));
