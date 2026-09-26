// @vitest-environment node
/**
 * IPC payload contract.
 *
 * `src/test/ipc-fixtures/fixtures.ts` holds values the Rust backend actually
 * serialized (see `src-tauri/tests/ipc_fixtures.rs`). The compiler checks
 * them against the bindings (`satisfies`); these tests check the rest: the
 * fields each binding declares, event names on both sides, and that the mock
 * backend returns and emits payloads shaped like the real ones.
 */
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { existsSync, readFileSync, readdirSync, statSync } from "node:fs";
import { join } from "node:path";
import { eventFixtures, typeFixtures } from "@/test/ipc-fixtures/fixtures";
import { inferShape, shapeMismatches } from "@/test/ipc-fixtures/shape";
import { handleInvoke } from "@/test/mock-backend";
import { handleListen } from "@/test/mock-backend/events";
import { addMockPastBuild, startMockBuild } from "@/test/mock-backend/build";
import { sampleEntries } from "@/test/mock-backend/logcat";

// vitest runs with the repo root as cwd (see vite.config.ts `test.include`).
const REPO_ROOT = process.cwd();

/** Events the mock backend has no way to produce; e2e tests trigger them directly. */
const NOT_EMITTED_BY_MOCK = {
  "logcat:reconnecting": "the mock logcat stream never loses adb",
  "logcat:stopped": "the mock logcat stream never gives up",
  "monitor://stats": "the mock has no memory monitor",
  "settings:corrupted": "mock settings are never read from disk",
};

function walk(dir, match, out = []) {
  for (const name of readdirSync(dir)) {
    const full = join(dir, name);
    if (statSync(full).isDirectory()) walk(full, match, out);
    else if (match.test(name)) out.push(full);
  }
  return out;
}

function read(path) {
  return readFileSync(path, "utf8");
}

/** Frontend sources that talk to the backend: no tests, mocks, or bindings. */
function frontendSources() {
  return walk(join(REPO_ROOT, "src"), /\.tsx?$/).filter(
    (f) => !/\.test\.tsx?$/.test(f) && !f.includes("/src/test/") && !f.includes("/src/bindings/")
  );
}

/** `invoke<T>("command")` → T, for every command the frontend calls. */
function invokedTypes() {
  const found = new Map();
  for (const file of frontendSources()) {
    for (const m of read(file).matchAll(/\binvoke<([^(]+?)>\(\s*"([a-z0-9_]+)"/g)) {
      found.set(m[2], m[1].trim());
    }
  }
  return found;
}

/** `listen<T>("event")` → T, or null without a type argument. */
function listenedTypes() {
  const found = new Map();
  for (const file of frontendSources()) {
    for (const m of read(file).matchAll(/\blisten(?:<([^(]+?)>)?\(\s*"([^"]+)"/g)) {
      found.set(m[2], m[1]?.trim() ?? null);
    }
  }
  return found;
}

/** Events the Rust backend emits, by literal name or through a `&str` constant. */
function rustEmittedEvents() {
  const files = walk(join(REPO_ROOT, "src-tauri/src"), /\.rs$/).map(read);
  const constants = new Map();
  for (const text of files) {
    for (const m of text.matchAll(/const ([A-Z0-9_]+): &str = "([^"]+)"/g)) {
      constants.set(m[1], m[2]);
    }
  }
  const emitted = new Set();
  for (const text of files) {
    for (const m of text.matchAll(/\.emit\(\s*(?:"([^"]+)"|([A-Za-z_][\w:]*))/g)) {
      const name = m[1] ?? constants.get(m[2].split("::").pop());
      if (name) emitted.add(name);
    }
  }
  return emitted;
}

/** Event names passed to `triggerEvent(...)` under `dirs`. */
function triggeredEvents(dirs) {
  const found = new Set();
  for (const dir of dirs) {
    for (const file of walk(join(REPO_ROOT, dir), /\.tsx?$/)) {
      for (const m of read(file).matchAll(/\btriggerEvent\(\s*"([^"]+)"/g)) found.add(m[1]);
    }
  }
  return found;
}

/** `Foo` for `Foo[]` or `Foo | null`. */
function baseType(type) {
  return type.split("|")[0].trim().replace(/\[\]$/, "");
}

function isNamedType(type) {
  return /^[A-Z]/.test(baseType(type));
}

/** Split `text` on `separator` outside brackets. */
function splitTopLevel(text, separator) {
  const parts = [];
  let depth = 0;
  let start = 0;
  for (let i = 0; i < text.length; i++) {
    const c = text[i];
    if ("{[(<".includes(c)) depth++;
    else if ("}])>".includes(c)) depth--;
    else if (c === separator && depth === 0) {
      parts.push(text.slice(start, i));
      start = i + 1;
    }
  }
  parts.push(text.slice(start));
  return parts.map((p) => p.trim()).filter(Boolean);
}

/**
 * Fields of a binding that is a plain object type, as name → { type, optional },
 * or null for unions and aliases.
 */
function bindingFields(type) {
  const source = read(join(REPO_ROOT, "src/bindings", `${type}.ts`)).replace(
    /\/\*\*[\s\S]*?\*\//g,
    ""
  );
  const marker = `export type ${type} = `;
  const start = source.indexOf(marker);
  if (start < 0) throw new Error(`src/bindings/${type}.ts does not declare ${type}`);
  const body = source
    .slice(start + marker.length)
    .trim()
    .replace(/;$/, "");
  if (!body.startsWith("{")) return null;
  if (splitTopLevel(body, "|").length > 1 || splitTopLevel(body, "&").length > 1) return null;
  const fields = new Map();
  for (const field of splitTopLevel(body.slice(1, -1), ",")) {
    const m = /^"?([A-Za-z0-9_]+)"?(\?)?\s*:\s*([\s\S]+)$/.exec(field);
    if (!m) throw new Error(`Cannot parse field "${field}" of ${type}`);
    fields.set(m[1], { type: m[3].trim(), optional: m[2] === "?" });
  }
  return fields;
}

function samplesOf(type) {
  const samples = typeFixtures[type];
  if (!samples) throw new Error(`No fixture for ${type}`);
  return samples;
}

/** How `value` departs from what the backend sends as `type` (`Foo`, `Foo[]`, `Foo | null`). */
function mismatchesAgainst(value, type) {
  if (value === null && /\|\s*null$/.test(type)) return [];
  const base = baseType(type);
  if (type.split("|")[0].trim().endsWith("[]")) {
    if (!Array.isArray(value)) return [`$: is not an array of ${base}`];
    return shapeMismatches(value, inferShape([samplesOf(base)]));
  }
  return shapeMismatches(value, inferShape(samplesOf(base)));
}

/** How `payload` departs from what the backend emits as `event`. */
function eventMismatches(event, payload) {
  const { type, samples } = eventFixtures[event];
  return isNamedType(type)
    ? mismatchesAgainst(payload, type)
    : shapeMismatches(payload, inferShape(samples));
}

describe("IPC fixtures match the bindings", () => {
  it("every fixture names a generated binding", () => {
    const missing = Object.keys(typeFixtures).filter(
      (type) => !existsSync(join(REPO_ROOT, "src/bindings", `${type}.ts`))
    );
    expect(missing, "Fixture types without a src/bindings file").toEqual([]);
  });

  it("each fixture has exactly the fields its binding declares", () => {
    const problems = [];
    for (const [type, samples] of Object.entries(typeFixtures)) {
      const fields = bindingFields(type);
      if (!fields) continue;
      for (const sample of samples) {
        const keys = Object.keys(sample);
        for (const key of keys) {
          if (!fields.has(key))
            problems.push(`${type}: sends "${key}", the binding has no such field`);
        }
        for (const [name, field] of fields) {
          if (!field.optional && !keys.includes(name)) {
            problems.push(`${type}: the binding declares "${name}", the backend does not send it`);
          }
        }
      }
    }
    expect(
      problems,
      "The serialized form and the TypeScript binding disagree. Regenerate both " +
        "(npm run generate:bindings, npm run generate:ipc-fixtures) and check the serde/ts attributes."
    ).toEqual([]);
  });

  it("no new 64-bit field is declared bigint", () => {
    const declared = [];
    for (const file of readdirSync(join(REPO_ROOT, "src/bindings"))) {
      const type = file.replace(/\.ts$/, "");
      if (type === "index") continue;
      for (const [name, field] of bindingFields(type) ?? []) {
        if (/\bbigint\b/.test(field.type)) declared.push(`${type}.${name}`);
      }
    }
    expect(
      declared.sort(),
      "These fields are typed bigint but arrive as JSON numbers, and a bigint " +
        'argument cannot be sent at all. Mark new ones #[ts(type = "number")].'
    ).toEqual([]);
  });

  it("every type the frontend invokes or listens for has a fixture", () => {
    const used = [...invokedTypes().values(), ...listenedTypes().values()]
      .filter((type) => type !== null && isNamedType(type))
      .map(baseType);
    const missing = [...new Set(used)].filter((type) => !(type in typeFixtures));
    expect(missing, "Add samples for these types in src-tauri/tests/ipc_fixtures.rs").toEqual([]);
  });

  it("parses a plausible number of commands and events", () => {
    // Guards against the parsers silently matching nothing.
    expect(invokedTypes().size).toBeGreaterThan(50);
    expect(listenedTypes().size).toBeGreaterThan(8);
    expect(rustEmittedEvents().size).toBeGreaterThan(8);
    expect(bindingFields("BuildRecord")?.size).toBeGreaterThan(5);
  });
});

describe("IPC event names", () => {
  it("every event the frontend listens for is emitted by the backend", () => {
    const emitted = rustEmittedEvents();
    const missing = [...listenedTypes().keys()].filter((event) => !emitted.has(event));
    expect(missing, "Listened for in src/ but never emitted in src-tauri/src").toEqual([]);
  });

  it("every event the backend emits has a payload fixture, and no fixture is stale", () => {
    expect(
      [...rustEmittedEvents()].sort(),
      "Keep the events in src-tauri/tests/ipc_fixtures.rs in step with the emit sites"
    ).toEqual(Object.keys(eventFixtures).sort());
  });

  it("the frontend listens with the payload type the backend sends", () => {
    const wrong = [...listenedTypes()]
      .filter(([event, type]) => type !== null && event in eventFixtures)
      .filter(([event, type]) => eventFixtures[event].type !== type)
      .map(([event, type]) => `${event}: listens as ${type}, sent as ${eventFixtures[event].type}`);
    expect(wrong).toEqual([]);
  });

  it("every event the frontend listens for is produced by the mock backend", () => {
    const triggered = triggeredEvents(["src/test/mock-backend", "e2e"]);
    const listened = [...listenedTypes().keys()];
    const missing = listened.filter((e) => !triggered.has(e) && !(e in NOT_EMITTED_BY_MOCK));
    const stale = Object.keys(NOT_EMITTED_BY_MOCK).filter(
      (e) => triggered.has(e) || !listened.includes(e)
    );
    expect(missing, "The mock backend never emits these; web-mode e2e cannot reach them").toEqual(
      []
    );
    expect(stale, "Remove these from NOT_EMITTED_BY_MOCK").toEqual([]);
  });
});

describe("the mock backend matches the real payloads", () => {
  const captured = new Map();
  const unlisten = [];

  beforeEach(async () => {
    vi.useFakeTimers();
    captured.clear();
    for (const event of Object.keys(eventFixtures)) {
      unlisten.push(
        await handleListen(event, ({ payload }) => {
          captured.set(event, [...(captured.get(event) ?? []), payload]);
        })
      );
    }
  });

  afterEach(() => {
    unlisten.splice(0).forEach((fn) => fn());
    vi.useRealTimers();
  });

  it("command responses", async () => {
    const problems = [];
    let checked = 0;
    // Commands that look something up need something to find. A release
    // build's record carries a saved mapping, so its shape is compared too.
    const releaseBuild = addMockPastBuild({ task: "assembleRelease", state: "success" });
    // One install matched to that build, one of an APK no build wrote.
    await handleInvoke("install_apk_on_device", {
      serial: "emulator-5554",
      apkPath: "/mock/app-release.apk",
    });
    await handleInvoke("install_apk_on_device", {
      serial: "28151FDH2000Q4",
      apkPath: "/mock/other.apk",
    });
    // A crash in the logcat buffer to deobfuscate.
    const crash = { ...sampleEntries[2], id: 90, isCrash: true, crashGroupId: 90 };
    await handleInvoke("__e2e_append_logcat_entries", {
      entries: [crash, { ...crash, id: 91, message: "\tat a.a.b(SourceFile:12)" }],
    });
    const args = {
      get_build_log_entries: { id: addMockPastBuild({ task: "assembleDebug", state: "success" }) },
      launch_app_on_device: { serial: "emulator-5554", package: "com.example.mockapp" },
      retrace_crash: { crashGroupId: 90 },
      find_apk_path: { variant: "release", module: ":app", buildId: releaseBuild },
    };
    for (const [command, type] of invokedTypes()) {
      if (!isNamedType(type)) continue;
      const response = await handleInvoke(command, args[command] ?? {});
      checked++;
      for (const problem of mismatchesAgainst(response, type)) {
        problems.push(`${command} (${type}) ${problem}`);
      }
    }
    expect(checked).toBeGreaterThan(20);
    expect(problems, "Fix the mock in src/test/mock-backend to match the backend").toEqual([]);
  });

  it("event payloads", async () => {
    await handleInvoke("refresh_devices");
    await handleInvoke("start_logcat");
    await vi.advanceTimersByTimeAsync(2000);
    await handleInvoke("stop_logcat");
    await handleInvoke("__e2e_append_logcat_entries", { entries: sampleEntries });
    await handleInvoke("clear_logcat");
    await handleInvoke("run_gradle_task", { task: "assembleDebug" });
    // A launch recorded on a build: its fully drawn time arrives later.
    await handleInvoke("launch_app_on_device", {
      serial: "emulator-5554",
      package: "com.example.mockapp",
      buildId: addMockPastBuild({ task: "assembleDebug", state: "success" }),
    });
    await vi.runAllTimersAsync();
    startMockBuild("assembleRelease", {
      kind: "agent",
      sessionId: 1,
      clientName: null,
      standalone: false,
    });
    await handleInvoke("cancel_build");

    const problems = [];
    for (const [event, payloads] of captured) {
      for (const payload of payloads) {
        problems.push(...eventMismatches(event, payload).map((m) => `${event} ${m}`));
      }
    }
    expect(problems, "Fix the mock in src/test/mock-backend to match the backend").toEqual([]);

    const unexercised = [...triggeredEvents(["src/test/mock-backend"])].filter(
      (event) => !captured.has(event)
    );
    expect(unexercised, "Drive these mock events above so their payloads are checked").toEqual([]);
  });
});
