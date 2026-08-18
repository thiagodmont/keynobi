/**
 * IPC contract drift guard.
 *
 * Every Tauri command has three coordinated definitions: the Rust handler
 * registered in `lib.rs`'s `generate_handler!`, the `invoke("...")` call in the
 * frontend, and a mock-backend handler so e2e/unit tests can exercise it.
 * Removing the legacy `finalize_build` command required edits in four files
 * with nothing to catch a miss — this test is that catch.
 *
 * It asserts inclusion, not equality: MCP-only commands legitimately have no
 * frontend caller, so `registered ⊆ invoked` is NOT required.
 */
import { describe, it, expect } from "vitest";
import { readFileSync, readdirSync, statSync } from "node:fs";
import { join, resolve } from "node:path";

// vitest runs with the repo root as cwd (see vite.config.ts `test.include`).
const REPO_ROOT = process.cwd();

/** Commands registered in `tauri::generate_handler![...]`. */
function registeredCommands() {
  const lib = readFileSync(join(REPO_ROOT, "src-tauri/src/lib.rs"), "utf8");
  const marker = "generate_handler![";
  const start = lib.indexOf(marker);
  expect(start, "generate_handler! not found in lib.rs").toBeGreaterThan(-1);

  // Bracket-match rather than regex — the macro body can contain nested [].
  let depth = 1;
  let i = start + marker.length;
  while (depth > 0 && i < lib.length) {
    if (lib[i] === "[") depth += 1;
    else if (lib[i] === "]") depth -= 1;
    i += 1;
  }

  const body = lib
    .slice(start + marker.length, i - 1)
    // Strip line comments BEFORE splitting: a naive split on "," swallows the
    // first entry after every comment line.
    .replace(/\/\/[^\n]*/g, "");

  return new Set(
    body
      .split(",")
      .map((s) => s.trim())
      .filter(Boolean)
  );
}

function walk(dir, out = []) {
  for (const name of readdirSync(dir)) {
    const full = join(dir, name);
    if (statSync(full).isDirectory()) walk(full, out);
    else if (/\.tsx?$/.test(name)) out.push(full);
  }
  return out;
}

const INVOKE_RE = /invoke(?:<[^>]*>)?\(\s*"([a-z0-9_]+)"/g;

/** Commands the frontend actually calls, excluding tests and the mock backend. */
function invokedCommands() {
  const found = new Map();
  for (const file of walk(join(REPO_ROOT, "src"))) {
    if (/\.test\.tsx?$/.test(file) || file.includes("mock-backend")) continue;
    const text = readFileSync(file, "utf8");
    for (const m of text.matchAll(INVOKE_RE)) {
      if (!found.has(m[1])) found.set(m[1], file.replace(`${REPO_ROOT}/`, ""));
    }
  }
  return found;
}

/** Command names handled by the mock backend. */
function mockedCommands() {
  const names = new Set();
  const dir = join(REPO_ROOT, "src/test/mock-backend");
  for (const file of walk(dir)) {
    const text = readFileSync(file, "utf8");
    for (const m of text.matchAll(/^\s{0,8}([a-z0-9_]+):\s*(?:\(|async)/gm)) {
      names.add(m[1]);
    }
  }
  return names;
}

describe("IPC contract", () => {
  it("every invoked command is registered in lib.rs", () => {
    const registered = registeredCommands();
    const invoked = invokedCommands();

    const missing = [...invoked.entries()]
      .filter(([cmd]) => !registered.has(cmd))
      .map(([cmd, file]) => `${cmd} (called from ${file})`);

    expect(
      missing,
      "These commands are invoked from the frontend but not registered in " +
        "src-tauri/src/lib.rs's generate_handler! — the call will fail at runtime."
    ).toEqual([]);
  });

  it("every invoked command has a mock-backend handler", () => {
    const invoked = invokedCommands();
    const mocked = mockedCommands();

    const missing = [...invoked.keys()].filter((cmd) => !mocked.has(cmd));

    expect(
      missing,
      "These commands have no handler in src/test/mock-backend — e2e runs will " +
        "silently receive undefined instead of a realistic response."
    ).toEqual([]);
  });

  it("parses a plausible number of commands from both sides", () => {
    // Guards against the parsers silently matching nothing and the assertions
    // above passing vacuously.
    expect(registeredCommands().size).toBeGreaterThan(50);
    expect(invokedCommands().size).toBeGreaterThan(50);
    expect(mockedCommands().size).toBeGreaterThan(30);
  });
});
