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
  const entry = sessions.find((s) => s.session.id === id);
  if (!entry) throw notFound(id);
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
      counts: { launches: 0, crashes: 0, anrs: 0, exits: 0, bookmarks: 0, captures: 0 },
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
    list_debug_sessions: () => [...sessions].reverse().map(summary),
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
    refresh_session_exit_reasons: (args: unknown): DebugSessionExitRefresh => {
      find((args as { id: string }).id);
      return { added: 0, message: null };
    },
    end_debug_session: (args: unknown) => {
      const { session } = find((args as { id: string }).id);
      if (session.closedAt === null) {
        session.closedAt = new Date().toISOString();
        session.closeReason = "ended";
      }
    },
    set_debug_session_kept: (args: unknown) => {
      const { id, kept } = args as { id: string; kept: boolean };
      const { session } = find(id);
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
        ? find(sessionId)
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
