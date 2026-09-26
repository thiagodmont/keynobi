import type { LogcatFilterSpec, ProcessedEntry, LogStats, RetraceOutcome } from "@/bindings";
import { triggerEvent } from "./events";

let logcatRunning = false;
let streamInterval: ReturnType<typeof setInterval> | null = null;
let nextId = 4;
let activeFilter: LogcatFilterSpec = emptyFilter();

export const sampleEntries: ProcessedEntry[] = [
  {
    id: 1,
    timestamp: "2026-04-23T10:00:00.000Z",
    pid: 1234,
    tid: 1234,
    level: "info",
    tag: "MainActivity",
    message: "Activity started",
    package: "com.example.mockapp",
    kind: "normal",
    isCrash: false,
    flags: 0,
    category: "lifecycle",
    crashGroupId: null,
    jsonBody: null,
  },
  {
    id: 2,
    timestamp: "2026-04-23T10:00:01.000Z",
    pid: 1234,
    tid: 1235,
    level: "debug",
    tag: "NetworkManager",
    message: "Connection established to api.example.com",
    package: "com.example.mockapp",
    kind: "normal",
    isCrash: false,
    flags: 0,
    category: "network",
    crashGroupId: null,
    jsonBody: null,
  },
  {
    id: 3,
    timestamp: "2026-04-23T10:00:02.000Z",
    pid: 1234,
    tid: 1236,
    level: "error",
    tag: "DatabaseHelper",
    message: "Failed to open database: no such table: users",
    package: "com.example.mockapp",
    kind: "normal",
    isCrash: false,
    flags: 0,
    category: "database",
    crashGroupId: null,
    jsonBody: null,
  },
];
let storedEntries: ProcessedEntry[] = [...sampleEntries];

type LogcatEntriesArgs = {
  minLevel?: string | null;
  tag?: string | null;
  text?: string | null;
  package?: string | null;
  onlyCrashes?: boolean | null;
};

type LogcatContextEntriesArgs = {
  anchorId?: number | string | null;
  direction?: "before" | "after" | string | null;
  count?: number | null;
};

function emptyFilter(): LogcatFilterSpec {
  return { minLevel: null, tag: null, text: null, package: null, onlyCrashes: false };
}

function priority(level: string): number {
  switch (level.toLowerCase()) {
    case "verbose":
    case "v":
      return 0;
    case "debug":
    case "d":
      return 1;
    case "info":
    case "i":
      return 2;
    case "warn":
    case "warning":
    case "w":
      return 3;
    case "error":
    case "e":
      return 4;
    case "fatal":
    case "f":
      return 5;
    default:
      return 6;
  }
}

function includesCI(value: string | null, needle: string | null | undefined): boolean {
  if (!needle) return true;
  return (value ?? "").toLowerCase().includes(needle.toLowerCase());
}

function filterEntries(entries: ProcessedEntry[], spec: LogcatFilterSpec): ProcessedEntry[] {
  return entries.filter((entry) => {
    if (spec.onlyCrashes && !entry.isCrash) return false;
    if (spec.minLevel && priority(entry.level) < priority(spec.minLevel)) return false;
    if (!includesCI(entry.tag, spec.tag)) return false;
    if (spec.text && !includesCI(entry.message, spec.text) && !includesCI(entry.tag, spec.text)) {
      return false;
    }
    if (spec.package && !includesCI(entry.package ?? entry.tag, spec.package)) return false;
    return true;
  });
}

function argsToFilter(args: unknown): LogcatFilterSpec {
  const opts = (args ?? {}) as LogcatEntriesArgs;
  return {
    minLevel: opts.minLevel ?? null,
    tag: opts.tag ?? null,
    text: opts.text ?? null,
    package: opts.package ?? null,
    onlyCrashes: opts.onlyCrashes ?? false,
  };
}

function contextEntries(args: unknown): ProcessedEntry[] {
  const opts = (args ?? {}) as LogcatContextEntriesArgs;
  if (opts.anchorId === null || opts.anchorId === undefined) return [];
  const anchorId = Number(opts.anchorId);
  const anchorIndex = storedEntries.findIndex((entry) => entry.id === anchorId);
  if (anchorIndex < 0) return [];
  const count = Math.max(0, Math.floor(opts.count ?? 10));
  if (opts.direction === "before") {
    return storedEntries.slice(Math.max(0, anchorIndex - count), anchorIndex);
  }
  if (opts.direction === "after") {
    return storedEntries.slice(anchorIndex + 1, anchorIndex + 1 + count);
  }
  return [];
}

/**
 * Deobfuscate a stored crash group the way the backend reports it: build #12's
 * mapping for the mock app, matched by the map id when the frames name one
 * (`r8-map-id-…`), else by the hash of the APK on the emulator.
 */
function retraceCrash(args: unknown): RetraceOutcome {
  const { crashGroupId } = (args ?? {}) as { crashGroupId?: number };
  const lines = storedEntries.filter((e) => e.crashGroupId === crashGroupId);
  if (lines.length === 0) {
    throw { kind: "NotFound", message: `Crash ${crashGroupId} is no longer in the logcat buffer.` };
  }
  const trace = lines.map((e) => `${e.message}\n`).join("");
  const byMapId = /\(r8-map-id-[^):]+/.test(trace);
  return {
    status: "retraced",
    trace: trace.replace(
      /\ba\.a\.b\((?:SourceFile|r8-map-id-[^):]+):(\d+)\)/g,
      "com.example.mockapp.MainActivity.onCreate(MainActivity.kt:$1)"
    ),
    buildId: 12,
    mapping: {
      module: ":app",
      variant: "release",
      sha256: "6b1c2f0a".repeat(8),
      bytes: 48_213_771,
      pgMapId: "6b1c2f0",
    },
    matchedBy: byMapId ? "mapId" : "deviceHash",
    device: "Pixel_7",
    package: lines.find((e) => e.package)?.package ?? null,
    reason: null,
    summary:
      "Deobfuscated with the R8 mapping of build #12 (:app release, map id 6b1c2f0), " +
      (byMapId
        ? "matched by map id."
        : "matched by the SHA-256 of the APK on Pixel_7 (4f2a9c1e7b3d…), the one Keynobi " +
          "installed at 2026-04-23T09:58:00Z."),
  };
}

export function logcatHandlers(): Record<string, (args: unknown) => unknown> {
  return {
    start_logcat: () => {
      logcatRunning = true;
      triggerEvent("logcat:entries", filterEntries(storedEntries, activeFilter));
      streamInterval = setInterval(() => {
        const entry = {
          id: nextId++,
          timestamp: new Date().toISOString(),
          pid: 1234,
          tid: 1234,
          level: "debug",
          tag: "MockTag",
          message: "Periodic log entry",
          package: "com.example.mockapp",
          kind: "normal",
          isCrash: false,
          flags: 0,
          category: "general",
          crashGroupId: null,
          jsonBody: null,
        } satisfies ProcessedEntry;
        triggerEvent("logcat:entries", filterEntries([entry], activeFilter));
      }, 2000);
    },
    stop_logcat: () => {
      logcatRunning = false;
      if (streamInterval) {
        clearInterval(streamInterval);
        streamInterval = null;
      }
    },
    export_logcat: () => null,
    clear_logcat: () => {
      storedEntries = [];
      triggerEvent("logcat:cleared", null);
    },
    get_logcat_entries: (args: unknown) => filterEntries(storedEntries, argsToFilter(args)),
    get_logcat_context_entries: (args: unknown) => contextEntries(args),
    get_logcat_status: () => logcatRunning,
    list_logcat_packages: () => ["com.example.mockapp"],
    set_logcat_filter: (args: unknown) => {
      const { filterSpec } = args as { filterSpec?: LogcatFilterSpec };
      activeFilter = filterSpec ?? emptyFilter();
    },
    __e2e_append_logcat_entries: (args: unknown) => {
      const { entries } = args as { entries?: ProcessedEntry[] };
      const nextEntries = entries ?? [];
      storedEntries = [...storedEntries, ...nextEntries];
      triggerEvent("logcat:entries", filterEntries(nextEntries, activeFilter));
    },
    retrace_crash: (args: unknown) => retraceCrash(args),
    get_logcat_stats: (): LogStats => ({
      totalIngested: storedEntries.length,
      countsByLevel: [0, 1, 1, 0, 1, 0, 0],
      crashCount: 0,
      jsonCount: 0,
      packagesSeen: 1,
      bufferUsagePct: 0.006,
      bufferEntryCount: storedEntries.length,
      droppedLines: 0,
      backlogLines: 0,
    }),
  };
}
