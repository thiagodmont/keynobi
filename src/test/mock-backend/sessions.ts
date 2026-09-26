import type {
  AppError,
  BuildActor,
  BuildRecord,
  DebugSession,
  DebugSessionEvent,
  DebugSessionEventData,
  DebugSessionSummary,
  InstalledBuild,
  LaunchTiming,
} from "@/bindings";

/** Most sessions kept, as `MAX_SESSIONS` in the backend. */
const MAX_MOCK_SESSIONS = 50;
/** As `MAX_KEPT_SESSIONS` in the backend. */
const MAX_MOCK_KEPT = 5;

interface MockSession {
  session: DebugSession;
  events: DebugSessionEvent[];
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
  if (event.kind === "launch") entry.session.counts.launches += 1;
  if (event.kind === "bookmark") entry.session.counts.bookmarks += 1;
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
      counts: { launches: 0, crashes: 0, anrs: 0, exits: 0, bookmarks: 0 },
      lastEventAt: now,
      eventCount: 0,
      droppedEvents: 0,
      bytes: 0,
    },
    events: [],
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

export function sessionHandlers(): Record<string, (args: unknown) => unknown> {
  return {
    list_debug_sessions: () => [...sessions].reverse().map(summary),
    get_debug_session: (args: unknown) => {
      const entry = find((args as { id: string }).id);
      return {
        session: { ...entry.session, counts: { ...entry.session.counts } },
        events: [...entry.events],
        eventsTruncated: false,
      };
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
