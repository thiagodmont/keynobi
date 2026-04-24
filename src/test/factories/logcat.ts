import type { ProcessedEntry } from "@/bindings";

let _id = BigInt(1);

export function makeLogEntry(overrides: Partial<ProcessedEntry> = {}): ProcessedEntry {
  return {
    id: _id++,
    timestamp: new Date().toISOString(),
    pid: 1234,
    tid: 1234,
    level: "info",
    tag: "MainActivity",
    message: "Activity started",
    package: "com.example.app",
    kind: "normal",
    isCrash: false,
    flags: 0,
    category: "lifecycle",
    crashGroupId: null,
    jsonBody: null,
    ...overrides,
  };
}

export function resetLogEntryId(): void {
  _id = BigInt(1);
}
