import type {
  AppError,
  BuildActor,
  BuildRecord,
  DebugSession,
  DebugSessionCapture,
  DebugSessionEvent,
  DebugSessionEventData,
  DebugSessionExitRefresh,
  DebugSessionSummary,
  InstalledBuild,
  LaunchTiming,
  ProcessedEntry,
  RedactionRule,
  SessionExportOptions,
  SessionExportResult,
} from "@/bindings";

/** Most sessions kept, as `MAX_SESSIONS` in the backend. */
const MAX_MOCK_SESSIONS = 50;
/** As `MAX_KEPT_SESSIONS` in the backend. */
const MAX_MOCK_KEPT = 5;
/** As `MAX_CAPTURES_PER_SESSION` in the backend. */
const MAX_MOCK_CAPTURES = 10;
/** As `CAPTURE_CONTEXT_BEFORE` in the backend. */
const MOCK_CONTEXT_BEFORE = 500;
/** As `MAX_CAPTURE_ENTRIES` in the backend. */
const MAX_MOCK_CAPTURE_ENTRIES = 1000;
/** `EntryFlags.ANR`. */
const ANR_FLAG = 1 << 1;

interface MockSession {
  session: DebugSession;
  events: DebugSessionEvent[];
  /** Log lines kept with each crash event, by its `seq`. */
  captures: Map<number, ProcessedEntry[]>;
}

let sessions: MockSession[] = [];
let nextSession = 1;
/** Imported sessions, newest import first. */
let imported: MockSession[] = [];
let nextImport = 1;

/** Ids are predictable so tests can name them: the first is `mockSessionId(1)`. */
export function mockSessionId(n: number): string {
  return `s-20260925T103200Z-${n.toString(16).padStart(12, "0")}`;
}

function summary({ session }: MockSession): DebugSessionSummary {
  const apk = session.build?.apk ?? null;
  return {
    id: session.id,
    projectRoot: session.projectRoot,
    package: session.package,
    device: { ...session.device },
    buildId: session.build?.id ?? null,
    module: apk?.module ?? null,
    variant: apk?.variant ?? null,
    versionCode: apk?.versionCode ?? session.install?.versionCode ?? null,
    apkSha256: session.install?.apkSha256 ?? null,
    mappingSha256s: session.build?.mappings.map((m) => m.sha256) ?? [],
    openedAt: session.openedAt,
    closedAt: session.closedAt,
    closeReason: session.closeReason,
    recordedBy: session.recordedBy,
    kept: session.kept,
    counts: { ...session.counts },
    lastEventAt: session.lastEventAt,
    eventCount: session.eventCount,
    droppedEvents: session.droppedEvents,
    bytes: session.bytes,
  };
}

function append(entry: MockSession, actor: BuildActor | null, event: DebugSessionEventData) {
  const at = new Date().toISOString();
  const recorded = { seq: entry.events.length + 1, at, actor, ...event } as DebugSessionEvent;
  entry.events.push(recorded);
  entry.session.eventCount += 1;
  entry.session.bytes += JSON.stringify(recorded).length + 1;
  entry.session.lastEventAt = at;
  const counts = entry.session.counts;
  if (event.kind === "launch") counts.launches += 1;
  if (event.kind === "bookmark") counts.bookmarks += 1;
  if (event.kind === "crash") counts.crashes += 1;
  if (event.kind === "anr") counts.anrs += 1;
  if (event.kind === "exit") counts.exits += 1;
  if (event.kind === "agentAction") counts.agentActions += 1;
  if ((event.kind === "crash" || event.kind === "anr") && event.data.capture) counts.captures += 1;
  return recorded;
}

function sameDevice(session: DebugSession, serial: string, avdName: string | null): boolean {
  if (session.device.avdName !== null || avdName !== null) {
    return session.device.avdName === avdName;
  }
  return session.device.serial === serial;
}

function notFound(id: string): AppError {
  return { kind: "notFound", message: `Debug session ${id} is no longer kept` };
}

function find(id: string): MockSession {
  const entry = [...sessions, ...imported].find((s) => s.session.id === id);
  if (!entry) throw notFound(id);
  return entry;
}

/** Like the backend: an imported session is read-only. */
function findRecorded(id: string): MockSession {
  const entry = find(id);
  if (entry.session.recordedBy === "imported") {
    const error: AppError = {
      kind: "invalidInput",
      message: `Debug session ${id} was imported and is read-only`,
    };
    throw error;
  }
  return entry;
}

/**
 * Like the backend after the user picked a bundle in its open dialog: a
 * redacted session with a crash and its kept lines, read-only.
 */
function importMockSession(): MockSession {
  const at = "2026-09-25T10:32:00.000000Z";
  const n = nextImport++;
  const install = {
    apkSha256: "c3".repeat(32),
    versionCode: 7,
    installedAt: at,
    by: { kind: "app" } as BuildActor,
  };
  const entry: MockSession = {
    session: {
      schemaVersion: 1,
      id: `i-20260926T081500Z-${n.toString(16).padStart(12, "0")}`,
      projectRoot: "<project>",
      package: "com.example.mockapp",
      device: { serial: "<device-1>", avdName: null, model: "Pixel 8" },
      build: null,
      install,
      openedAt: at,
      closedAt: "2026-09-25T11:00:00.000000Z",
      closeReason: "ended",
      recordedBy: "imported",
      kept: false,
      counts: {
        launches: 0,
        crashes: 0,
        anrs: 0,
        exits: 0,
        bookmarks: 0,
        captures: 0,
        agentActions: 0,
      },
      lastEventAt: at,
      eventCount: 0,
      droppedEvents: 0,
      bytes: 0,
      imported: {
        fileName: "keynobi-session-com.example.mockapp-20260925.zip",
        exportedAt: "2026-09-25T12:00:00.000000Z",
        importedAt: new Date().toISOString(),
        originalId: "s-20260925T103200Z-4f2a9c00b1de",
        keynobiVersion: "0.9.0",
        omitted: [
          {
            item: "R8 mappings",
            reason: "never exported; the session names each by SHA-256 and map id",
          },
        ],
        redactions: [
          { rule: "emails", enabled: true, count: 1 },
          { rule: "secrets", enabled: true, count: 0 },
          { rule: "ipAddresses", enabled: true, count: 0 },
          { rule: "paths", enabled: true, count: 2 },
          { rule: "deviceSerials", enabled: true, count: 1 },
        ],
      },
    },
    events: [],
    captures: new Map(),
  };
  append(entry, { kind: "app" }, { kind: "install", data: install });
  const crash = append(entry, null, {
    kind: "crash",
    data: {
      serial: "<device-1>",
      pid: 4242,
      summary: "java.lang.IllegalStateException: <email-1> not found",
      signature: "00000000deadbeef",
      receivedAt: at,
      deviceTime: "09-25 10:32:05.123",
      attribution: { method: "installRecord", verified: true, reason: null },
      capture: { entries: 2, bytes: 512, truncated: false },
      droppedLines: 0,
    },
  });
  const line = (id: number, message: string): ProcessedEntry => ({
    id,
    timestamp: "09-25 10:32:05.123",
    pid: 4242,
    tid: 4242,
    level: "error",
    tag: "AndroidRuntime",
    message,
    package: null,
    kind: "normal",
    isCrash: false,
    flags: 0,
    category: "general",
    crashGroupId: null,
    jsonBody: null,
  });
  entry.captures.set(crash.seq, [
    line(1, "FATAL EXCEPTION: main"),
    line(2, "java.lang.IllegalStateException: <email-1> not found"),
  ]);
  return entry;
}

/** Like the backend: a recorded install opens a session and supersedes the open one. */
export function openMockSession(entry: InstalledBuild, record: BuildRecord | undefined) {
  const now = new Date().toISOString();
  for (const { session } of sessions) {
    if (
      session.closedAt === null &&
      session.package === entry.package &&
      sameDevice(session, entry.serial, entry.avdName)
    ) {
      session.closedAt = now;
      session.closeReason = "superseded";
    }
  }
  const apk = record?.apks.find((a) => a.sha256 === entry.apkSha256);
  const build =
    record && apk
      ? {
          id: record.id,
          task: record.task,
          startedAt: record.startedAt,
          origin: record.origin,
          apk: {
            module: apk.module,
            variant: apk.variant,
            sha256: apk.sha256,
            versionCode: apk.versionCode,
          },
          mappings: entry.mappings.map((m) => ({ sha256: m.sha256, pgMapId: m.pgMapId })),
          ...(record.provenance ? { provenance: record.provenance } : {}),
        }
      : null;
  const install = {
    apkSha256: entry.apkSha256,
    versionCode: entry.versionCode,
    installedAt: entry.installedAt,
    by: { kind: "app" } as BuildActor,
  };
  const created: MockSession = {
    session: {
      schemaVersion: 1,
      id: mockSessionId(nextSession++),
      projectRoot: record?.projectRoot ?? null,
      package: entry.package,
      device: { serial: entry.serial, avdName: entry.avdName, model: entry.model },
      build,
      install,
      openedAt: now,
      closedAt: null,
      closeReason: null,
      recordedBy: "app",
      kept: false,
      counts: {
        launches: 0,
        crashes: 0,
        anrs: 0,
        exits: 0,
        bookmarks: 0,
        captures: 0,
        agentActions: 0,
      },
      lastEventAt: now,
      eventCount: 0,
      droppedEvents: 0,
      bytes: 0,
    },
    events: [],
    captures: new Map(),
  };
  if (build) append(created, record?.origin ?? null, { kind: "build", data: build });
  append(created, { kind: "app" }, { kind: "install", data: install });
  sessions = [...sessions, created].slice(-MAX_MOCK_SESSIONS);
}

/**
 * Like the backend: a launch, or display times that arrived after it, are
 * added to the open session of its device and package.
 */
export function recordMockLaunch(pkg: string, timing: LaunchTiming, late = false) {
  const event: DebugSessionEventData = late
    ? { kind: "launchTiming", data: timing }
    : { kind: "launch", data: { serial: timing.serial, timing, restart: false } };
  for (const entry of sessions) {
    const { session } = entry;
    if (session.closedAt === null && session.package === pkg) {
      if (!sameDevice(session, timing.serial, timing.avdName)) continue;
      append(entry, { kind: "app" }, event);
    }
  }
}

let lastCrashGroup = 0;

/**
 * Like the backend: the first entry of each new crash group adds a crash to
 * the newest open session of its package, keeping the lines before it and
 * the group's lines while the session has captures left.
 */
export function recordMockCrashes(added: ProcessedEntry[], buffer: ProcessedEntry[]) {
  for (const first of added) {
    const gid = first.crashGroupId;
    if (gid === null || gid <= lastCrashGroup) continue;
    lastCrashGroup = gid;
    const entry = [...sessions]
      .reverse()
      .find((s) => s.session.closedAt === null && s.session.package === first.package);
    if (!entry) continue;
    const at = buffer.indexOf(first);
    const lines = [
      ...buffer.slice(Math.max(0, at - MOCK_CONTEXT_BEFORE), at),
      ...buffer.slice(at).filter((e) => e.crashGroupId === gid),
    ].slice(-MAX_MOCK_CAPTURE_ENTRIES);
    const captured = entry.session.counts.captures < MAX_MOCK_CAPTURES;
    const recorded = append(entry, null, {
      kind: (first.flags & ANR_FLAG) !== 0 ? "anr" : "crash",
      data: {
        serial: entry.session.device.serial,
        pid: first.pid,
        summary: first.message,
        signature: gid.toString(16).padStart(16, "0"),
        receivedAt: new Date().toISOString(),
        deviceTime: first.timestamp,
        attribution: {
          method: entry.session.install ? "installRecord" : "unattributed",
          verified: entry.session.install !== null,
          reason: null,
        },
        capture: captured
          ? {
              entries: lines.length,
              bytes: lines.reduce((n, e) => n + JSON.stringify(e).length + 1, 0),
              truncated: false,
            }
          : null,
        droppedLines: 0,
      },
    });
    if (captured) entry.captures.set(recorded.seq, lines);
  }
}

export function sessionHandlers(): Record<string, (args: unknown) => unknown> {
  return {
    list_debug_sessions: () => [...[...sessions].reverse(), ...imported].map(summary),
    import_debug_session: (): DebugSessionSummary => {
      const entry = importMockSession();
      imported = [entry, ...imported];
      return summary(entry);
    },
    delete_imported_debug_session: (args: unknown) => {
      const { id } = args as { id: string };
      const entry = find(id);
      if (entry.session.recordedBy !== "imported") {
        const error: AppError = {
          kind: "invalidInput",
          message: "Only an imported debug session can be deleted",
        };
        throw error;
      }
      imported = imported.filter((s) => s !== entry);
    },
    get_debug_session: (args: unknown) => {
      const entry = find((args as { id: string }).id);
      return {
        session: { ...entry.session, counts: { ...entry.session.counts } },
        events: [...entry.events],
        eventsTruncated: false,
        crashes: entry.events.filter((e) => e.kind === "crash" || e.kind === "anr"),
      };
    },
    get_session_capture: (args: unknown): DebugSessionCapture => {
      const { id, seq, limit } = args as { id: string; seq: number; limit?: number | null };
      const lines = find(id).captures.get(seq);
      if (!lines) {
        const error: AppError = {
          kind: "notFound",
          message: `Debug session ${id} kept no log lines for event ${seq}`,
        };
        throw error;
      }
      const keep = Math.min(
        Math.max(limit ?? MAX_MOCK_CAPTURE_ENTRIES, 1),
        MAX_MOCK_CAPTURE_ENTRIES
      );
      return { seq, entries: lines.slice(-keep), truncated: lines.length > keep };
    },
    // Like the backend after the user picked a file in its save dialog.
    export_debug_session: (args: unknown): SessionExportResult => {
      const { id, options } = args as { id: string; options: SessionExportOptions };
      const entry = findRecorded(id);
      const logs = options.includeCrashLogs
        ? [...entry.captures.keys()].map((seq) => `logs/crash-${seq}.log`)
        : [];
      const omitted = [
        {
          item: "R8 mappings",
          reason: "never exported; the session names each by SHA-256 and map id",
        },
      ];
      if (!options.includeCrashLogs && entry.captures.size > 0) {
        omitted.push({ item: "crash log lines", reason: "not selected" });
      }
      const rules = options.redaction;
      const enabled: Record<RedactionRule, boolean> = {
        emails: rules.emails,
        secrets: rules.secrets,
        ipAddresses: rules.ipAddresses,
        paths: rules.paths,
        deviceSerials: rules.deviceSerials,
      };
      return {
        path: `/mock/Desktop/keynobi-session-${entry.session.package}.zip`,
        bytes: 4096,
        entries: ["manifest.json", "session.json", "timeline.jsonl", ...logs, "redaction.json"],
        redactions: (Object.keys(enabled) as RedactionRule[]).map((rule) => ({
          rule,
          enabled: enabled[rule],
          count: enabled[rule] && rule === "paths" ? 1 : 0,
        })),
        omitted,
      };
    },
    refresh_session_exit_reasons: (args: unknown): DebugSessionExitRefresh => {
      findRecorded((args as { id: string }).id);
      return { added: 0, message: null };
    },
    end_debug_session: (args: unknown) => {
      const { session } = findRecorded((args as { id: string }).id);
      if (session.closedAt === null) {
        session.closedAt = new Date().toISOString();
        session.closeReason = "ended";
      }
    },
    set_debug_session_kept: (args: unknown) => {
      const { id, kept } = args as { id: string; kept: boolean };
      const { session } = findRecorded(id);
      if (kept && !session.kept && sessions.filter((s) => s.session.kept).length >= MAX_MOCK_KEPT) {
        const error: AppError = {
          kind: "invalidInput",
          message: `At most ${MAX_MOCK_KEPT} debug sessions can be kept. Stop keeping one first.`,
        };
        throw error;
      }
      session.kept = kept;
    },
    add_session_bookmark: (args: unknown) => {
      const { sessionId, note, logEntryId } = (args ?? {}) as {
        sessionId?: string | null;
        note?: string;
        logEntryId?: number | null;
      };
      const entry = sessionId
        ? findRecorded(sessionId)
        : [...sessions].reverse().find((s) => s.session.closedAt === null);
      if (!entry) {
        const error: AppError = { kind: "notFound", message: "No debug session is open" };
        throw error;
      }
      return append(
        entry,
        { kind: "app" },
        {
          kind: "bookmark",
          data: { note: note ?? "Bookmark", logEntryId: logEntryId ?? null },
        }
      );
    },
  };
}
