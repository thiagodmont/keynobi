#!/usr/bin/env node
/**
 * mcp-smoke.mjs — check that a built `keynobi` binary serves MCP.
 *
 * Usage:
 *   node scripts/mcp-smoke.mjs <path/to/keynobi>
 *
 * Runs `keynobi --mcp --project <fake Gradle project>` with a throwaway HOME,
 * so it can neither attach to a running app nor touch the real ~/.keynobi,
 * then completes `initialize`, `tools/list`, and a `get_project_info` call
 * over stdio and checks the server runs standalone on that project. Exits
 * non-zero on any failure. The release workflow runs it on the binary inside
 * each DMG before publishing.
 */
import { spawn } from "node:child_process";
import {
  chmodSync,
  existsSync,
  mkdirSync,
  mkdtempSync,
  realpathSync,
  rmSync,
  writeFileSync,
} from "node:fs";
import { tmpdir } from "node:os";
import { basename, join, resolve } from "node:path";
import { pathToFileURL } from "node:url";

export const DEFAULT_TIMEOUT_MS = 30_000;
/** Tools a release must serve; a sample, not the full list. */
const REQUIRED_TOOLS = [
  "get_project_info",
  "run_gradle_task",
  "get_logcat_entries",
  "list_devices",
];

function writeScript(path, body) {
  writeFileSync(path, `#!/bin/sh\n${body}\n`);
  chmodSync(path, 0o755);
}

/**
 * A throwaway HOME, Android SDK, JDK, and Gradle project. The settings name
 * the fake SDK and JDK so the server does not probe this machine's.
 */
export function createSandbox() {
  // Under /tmp on macOS: the app socket lives in HOME, and socket paths are
  // limited to 103 bytes, which a long $TMPDIR can exceed.
  const base = process.platform === "darwin" && existsSync("/tmp") ? "/tmp" : tmpdir();
  const root = realpathSync(mkdtempSync(join(base, "kn-smoke-")));
  const home = join(root, "home");
  const project = join(root, "SmokeProject");
  const sdk = join(root, "sdk");
  const jdk = join(root, "jdk");

  mkdirSync(join(home, ".keynobi"), { recursive: true });
  mkdirSync(join(project, "app"), { recursive: true });
  mkdirSync(join(sdk, "platform-tools"), { recursive: true });
  mkdirSync(join(jdk, "bin"), { recursive: true });

  writeFileSync(
    join(project, "settings.gradle.kts"),
    'rootProject.name = "SmokeProject"\ninclude(":app")\n'
  );
  writeFileSync(
    join(project, "app", "build.gradle.kts"),
    'android { namespace = "com.example.smoke" }\n'
  );
  writeScript(join(project, "gradlew"), "echo 'BUILD SUCCESSFUL in 1s'");
  writeScript(join(sdk, "platform-tools", "adb"), "echo 'List of devices attached'");
  writeFileSync(join(jdk, "release"), 'JAVA_VERSION="17.0.9"\n');
  writeScript(join(jdk, "bin", "java"), "echo 'openjdk version \"17.0.9\" 2025-07-15' >&2");
  writeFileSync(
    join(home, ".keynobi", "settings.json"),
    JSON.stringify({ android: { sdkPath: sdk }, java: { home: jdk } })
  );

  return { root, home, project, cleanup: () => rmSync(root, { recursive: true, force: true }) };
}

/** A newline-delimited JSON-RPC client over a child's stdio. */
function connect(child, deadline) {
  const pending = new Map();
  let buffer = "";
  let failure = null;

  const failAll = (error) => {
    failure ??= error;
    for (const { reject } of pending.values()) reject(failure);
    pending.clear();
  };

  child.stdout.setEncoding("utf8");
  child.stdout.on("data", (chunk) => {
    buffer += chunk;
    let newline;
    while ((newline = buffer.indexOf("\n")) >= 0) {
      const line = buffer.slice(0, newline).trim();
      buffer = buffer.slice(newline + 1);
      if (!line) continue;
      let message;
      try {
        message = JSON.parse(line);
      } catch {
        failAll(new Error(`The server wrote a non-JSON line to stdout: ${line}`));
        return;
      }
      const waiter = pending.get(message.id);
      if (!waiter) continue; // A notification.
      pending.delete(message.id);
      if (message.error)
        waiter.reject(new Error(`${waiter.method} failed: ${JSON.stringify(message.error)}`));
      else waiter.resolve(message.result);
    }
  });
  child.on("error", (error) => failAll(new Error(`Could not start the server: ${error.message}`)));
  child.on("close", (code, signal) =>
    failAll(new Error(`The server exited (code ${code}, signal ${signal}) before answering`))
  );

  let nextId = 1;
  const send = (message) =>
    child.stdin.write(`${JSON.stringify({ jsonrpc: "2.0", ...message })}\n`);

  return {
    notify: (method, params) => send({ method, ...(params ? { params } : {}) }),
    request(method, params) {
      if (failure) return Promise.reject(failure);
      const id = nextId++;
      return new Promise((resolve, reject) => {
        const timer = setTimeout(
          () => reject(new Error(`No reply to ${method} before the deadline`)),
          Math.max(0, deadline - Date.now())
        );
        pending.set(id, {
          method,
          resolve: (value) => {
            clearTimeout(timer);
            resolve(value);
          },
          reject: (error) => {
            clearTimeout(timer);
            reject(error);
          },
        });
        send({ id, method, params });
      });
    },
  };
}

/** The JSON a tool returned, from its structured content or its text. */
function toolJson(result) {
  if (result?.structuredContent) return result.structuredContent;
  const text = result?.content?.find((c) => c.type === "text")?.text;
  if (!text) throw new Error(`The tool returned no content: ${JSON.stringify(result)}`);
  return JSON.parse(text);
}

function waitForExit(child, ms) {
  if (child.exitCode !== null || child.signalCode !== null) {
    return Promise.resolve({ code: child.exitCode, signal: child.signalCode });
  }
  return new Promise((resolve) => {
    const timer = setTimeout(() => {
      child.kill("SIGKILL");
      resolve({ code: null, signal: "timeout" });
    }, ms);
    child.once("exit", (code, signal) => {
      clearTimeout(timer);
      resolve({ code, signal });
    });
  });
}

/**
 * Run the smoke check against `binary`. Resolves with what the server
 * reported; rejects with the reason (and the server's stderr) on failure.
 */
export async function runSmoke(binary, { timeoutMs = DEFAULT_TIMEOUT_MS, log = () => {} } = {}) {
  const deadline = Date.now() + timeoutMs;
  const sandbox = createSandbox();
  const child = spawn(resolve(binary), ["--mcp", "--project", sandbox.project], {
    cwd: sandbox.project,
    env: { PATH: process.env.PATH ?? "/usr/bin:/bin", HOME: sandbox.home },
    stdio: ["pipe", "pipe", "pipe"],
  });
  let stderr = "";
  child.stderr.setEncoding("utf8");
  child.stderr.on("data", (chunk) => {
    stderr += chunk;
  });
  // A write after the server died must fail the check, not crash this process.
  child.stdin.on("error", () => {});

  try {
    const client = connect(child, deadline);

    const init = await client.request("initialize", {
      protocolVersion: "2025-06-18",
      capabilities: {},
      clientInfo: { name: "keynobi-smoke", version: "1" },
    });
    if (!init?.serverInfo?.name)
      throw new Error(`initialize returned no serverInfo: ${JSON.stringify(init)}`);
    client.notify("notifications/initialized");
    log(`initialize: ${init.serverInfo.name} ${init.serverInfo.version ?? ""}`.trim());

    const listed = await client.request("tools/list", {});
    const tools = (listed?.tools ?? []).map((t) => t.name);
    const missing = REQUIRED_TOOLS.filter((name) => !tools.includes(name));
    if (missing.length > 0) throw new Error(`tools/list is missing ${missing.join(", ")}`);
    log(`tools/list: ${tools.length} tools`);

    const called = await client.request("tools/call", { name: "get_project_info", arguments: {} });
    if (called?.isError) throw new Error(`get_project_info failed: ${JSON.stringify(called)}`);
    const info = toolJson(called);
    if (info.mode !== "standalone") {
      throw new Error(`Expected a standalone session, got mode ${JSON.stringify(info.mode)}`);
    }
    if (info.open !== true || info.name !== basename(sandbox.project)) {
      throw new Error(`The server did not open the --project directory: ${JSON.stringify(info)}`);
    }
    log(`get_project_info: ${info.mode} (${info.standalone_reason}), project ${info.name}`);

    child.stdin.end();
    const exit = await waitForExit(child, Math.max(1000, deadline - Date.now()));
    if (exit.code !== 0) {
      throw new Error(
        `The server did not exit cleanly after stdin closed (code ${exit.code}, signal ${exit.signal})`
      );
    }
    return { serverInfo: init.serverInfo, tools, info };
  } catch (error) {
    const detail = stderr.trim() ? `\n--- server stderr ---\n${stderr.trim()}` : "";
    throw new Error(`${error.message}${detail}`, { cause: error });
  } finally {
    if (child.exitCode === null && child.signalCode === null) child.kill("SIGKILL");
    sandbox.cleanup();
  }
}

if (process.argv[1] && import.meta.url === pathToFileURL(resolve(process.argv[1])).href) {
  const binary = process.argv[2];
  if (!binary) {
    console.error("Usage: node scripts/mcp-smoke.mjs <path/to/keynobi>");
    process.exit(2);
  }
  const started = Date.now();
  runSmoke(binary, { log: (line) => console.log(`  ${line}`) })
    .then(() => {
      console.log(
        `MCP smoke test passed in ${((Date.now() - started) / 1000).toFixed(1)}s: ${binary}`
      );
    })
    .catch((error) => {
      console.error(`MCP smoke test failed for ${binary}: ${error.message}`);
      process.exit(1);
    });
}
