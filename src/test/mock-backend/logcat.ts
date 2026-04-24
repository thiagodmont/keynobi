import type { ProcessedEntry, LogStats } from "@/bindings";
import { triggerEvent } from "./events";

let logcatRunning = false;
let streamInterval: ReturnType<typeof setInterval> | null = null;
let nextId = BigInt(4);

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

export function logcatHandlers(): Record<string, (args: unknown) => unknown> {
  return {
    start_logcat: () => {
      logcatRunning = true;
      triggerEvent("logcat:entries", sampleEntries);
      streamInterval = setInterval(() => {
        triggerEvent("logcat:entries", [
          {
            id: nextId++,
            timestamp: new Date().toISOString(),
            pid: 1234,
            tid: 1234,
            level: "debug",
            tag: "MockTag",
            message: `Periodic log entry`,
            package: "com.example.mockapp",
            kind: "normal",
            isCrash: false,
            flags: 0,
            category: "general",
            crashGroupId: null,
            jsonBody: null,
          } satisfies ProcessedEntry,
        ]);
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
    get_logcat_entries: () => [...sampleEntries],
    get_logcat_status: () => logcatRunning,
    list_logcat_packages: () => ["com.example.mockapp"],
    set_logcat_filter: () => undefined,
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
