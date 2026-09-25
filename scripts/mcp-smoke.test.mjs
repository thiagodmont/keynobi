// @vitest-environment node
import { afterAll, describe, expect, it } from "vitest";
import { chmodSync, existsSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { homedir, tmpdir } from "node:os";
import { join, resolve } from "node:path";
import { runSmoke } from "./mcp-smoke.mjs";

// vitest runs with the repo root as cwd.
const REPO_ROOT = process.cwd();
const DEBUG_BINARY = join(
  resolve(REPO_ROOT, process.env.CARGO_TARGET_DIR ?? "src-tauri/target"),
  "debug",
  "keynobi"
);

const fakes = mkdtempSync(join(tmpdir(), "kn-smoke-fakes-"));
afterAll(() => rmSync(fakes, { recursive: true, force: true }));

/**
 * A stand-in `keynobi` that speaks just enough MCP. It refuses to run unless
 * invoked the way a release smoke test must: `--mcp --project <Gradle
 * project>` with a HOME that is not the real one. `behavior` breaks it in one way.
 */
function fakeServer(behavior) {
  const path = join(fakes, `keynobi-${behavior}`);
  writeFileSync(
    path,
    `#!${process.execPath}
const { existsSync } = require("node:fs");
const { basename, join } = require("node:path");
const behavior = ${JSON.stringify(behavior)};
const [flag, projectFlag, project] = process.argv.slice(2);
if (flag !== "--mcp" || projectFlag !== "--project" || !existsSync(join(project, "settings.gradle.kts"))) {
  process.stderr.write("not a Gradle project: " + process.argv.slice(2).join(" "));
  process.exit(3);
}
if (!process.env.HOME || process.env.HOME === ${JSON.stringify(homedir())}) {
  process.stderr.write("refusing to run with the real HOME");
  process.exit(4);
}
if (behavior === "crash") {
  process.stderr.write("panicked at startup");
  process.exit(101);
}
const tools = ["get_project_info", "run_gradle_task", "get_logcat_entries", "list_devices"]
  .filter((name) => behavior !== "missing-tool" || name !== "get_project_info")
  .map((name) => ({ name }));
const reply = (id, result) => process.stdout.write(JSON.stringify({ jsonrpc: "2.0", id, result }) + "\\n");
let buffer = "";
process.stdin.on("data", (chunk) => {
  buffer += chunk;
  let i;
  while ((i = buffer.indexOf("\\n")) >= 0) {
    const message = JSON.parse(buffer.slice(0, i));
    buffer = buffer.slice(i + 1);
    if (behavior === "silent" || message.id === undefined) continue;
    if (message.method === "initialize") reply(message.id, { serverInfo: { name: "keynobi", version: "0" } });
    if (message.method === "tools/list") reply(message.id, { tools });
    if (message.method === "tools/call") {
      const mode = behavior === "attached" ? "attached" : "standalone";
      reply(message.id, { structuredContent: { mode, open: true, name: basename(project) } });
    }
  }
});
process.stdin.on("end", () => process.exit(behavior === "unclean-exit" ? 1 : 0));
`
  );
  chmodSync(path, 0o755);
  return path;
}

describe("runSmoke", () => {
  it("passes a server that answers as a standalone keynobi on the fake project", async () => {
    const result = await runSmoke(fakeServer("ok"), { timeoutMs: 10_000 });
    expect(result.info.mode).toBe("standalone");
    expect(result.tools).toContain("get_project_info");
  });

  it("fails when the session attached to an app instead of running standalone", async () => {
    await expect(runSmoke(fakeServer("attached"), { timeoutMs: 10_000 })).rejects.toThrow(
      'Expected a standalone session, got mode "attached"'
    );
  });

  it("fails when a required tool is missing", async () => {
    await expect(runSmoke(fakeServer("missing-tool"), { timeoutMs: 10_000 })).rejects.toThrow(
      "tools/list is missing get_project_info"
    );
  });

  it("fails, with the server's stderr, when it exits at startup", async () => {
    await expect(runSmoke(fakeServer("crash"), { timeoutMs: 10_000 })).rejects.toThrow(
      /exited \(code 101[\s\S]*panicked at startup/
    );
  });

  it("fails when the server never answers", async () => {
    await expect(runSmoke(fakeServer("silent"), { timeoutMs: 500 })).rejects.toThrow(
      "No reply to initialize before the deadline"
    );
  });

  it("fails when the server does not exit cleanly once stdin closes", async () => {
    await expect(runSmoke(fakeServer("unclean-exit"), { timeoutMs: 10_000 })).rejects.toThrow(
      "did not exit cleanly"
    );
  });

  it("fails when the binary does not exist", async () => {
    await expect(runSmoke(join(fakes, "missing"), { timeoutMs: 10_000 })).rejects.toThrow(
      /Could not start the server|exited/
    );
  });

  // Built by `cargo build` or `cargo test --tests`; CI runs the script on it in the Rust job.
  it.skipIf(!existsSync(DEBUG_BINARY))(
    "passes the locally built debug binary",
    async () => {
      const result = await runSmoke(DEBUG_BINARY);
      expect(result.info).toMatchObject({ mode: "standalone", open: true, name: "SmokeProject" });
      expect(result.tools.length).toBeGreaterThan(20);
    },
    60_000
  );
});

/** The body of the workflow step named `name`. */
function workflowStep(workflow, name) {
  const yml = readFileSync(join(REPO_ROOT, ".github/workflows", workflow), "utf8");
  const start = yml.indexOf(`- name: ${name}`);
  expect(start, `"${name}" step not found in ${workflow}`).toBeGreaterThan(-1);
  const next = yml.indexOf("\n      - ", start + 1);
  return { yml, start, body: next === -1 ? yml.slice(start) : yml.slice(start, next) };
}

describe("workflows run the smoke test", () => {
  it("the release smoke-tests each DMG's binary after verifying it and before uploading it", () => {
    const { yml, start, body } = workflowStep("release.yml", "Smoke-test the bundled MCP server");
    expect(yml.indexOf("- name: Verify signature and notarization")).toBeLessThan(start);
    expect(yml.indexOf("- name: Upload DMG artifact")).toBeGreaterThan(start);
    expect(body).toContain("set -euo pipefail");
    expect(body).toContain('hdiutil attach "$DMG" -nobrowse -readonly');
    expect(body).toContain("hdiutil detach");
    expect(body).toMatch(/node scripts\/mcp-smoke\.mjs "\$BIN"\n/);
    expect(body).not.toContain("|| true\n          node");
    expect(body).not.toContain("continue-on-error");
  });

  it("CI smoke-tests the debug binary the Rust tests built", () => {
    const { body } = workflowStep("ci.yml", "MCP smoke test (debug binary)");
    expect(body).toContain("node scripts/mcp-smoke.mjs src-tauri/target/debug/keynobi");
    expect(body).not.toContain("|| true");
  });
});
