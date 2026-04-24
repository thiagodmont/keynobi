import type { LogcatFilterSpec, ProcessedEntry, LogStats } from "@/bindings";
import { triggerEvent } from "./events";

let logcatRunning = false;
let streamInterval: ReturnType<typeof setInterval> | null = null;
let nextId = BigInt(4);
let activeFilter: LogcatFilterSpec = emptyFilter();

export const sampleEntries: ProcessedEntry[] = [
  {
    id: BigInt(1),
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
    id: BigInt(2),
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
    id: BigInt(3),
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

type LogcatEntriesArgs = {
  minLevel?: string | null;
  tag?: string | null;
  text?: string | null;
  package?: string | null;
  onlyCrashes?: boolean | null;
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

export function logcatHandlers(): Record<string, (args: unknown) => unknown> {
  return {
    start_logcat: () => {
      logcatRunning = true;
      triggerEvent("logcat:entries", filterEntries(sampleEntries, activeFilter));
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
    clear_logcat: () => {
      triggerEvent("logcat:cleared", undefined);
    },
    get_logcat_entries: (args: unknown) => filterEntries(sampleEntries, argsToFilter(args)),
    get_logcat_status: () => logcatRunning,
    list_logcat_packages: () => ["com.example.mockapp"],
    set_logcat_filter: (args: unknown) => {
      const { filterSpec } = args as { filterSpec?: LogcatFilterSpec };
      activeFilter = filterSpec ?? emptyFilter();
    },
    get_logcat_stats: (): LogStats => ({
      totalIngested: BigInt(3),
      countsByLevel: [BigInt(0), BigInt(1), BigInt(1), BigInt(0), BigInt(1), BigInt(0), BigInt(0)],
      crashCount: BigInt(0),
      jsonCount: BigInt(0),
      packagesSeen: 1,
      bufferUsagePct: 0.006,
      bufferEntryCount: BigInt(3),
    }),
  };
}
