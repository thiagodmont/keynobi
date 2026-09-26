import type {
  DebugSession,
  DebugSessionCrash,
  DebugSessionDetail,
  DebugSessionEvent,
  DebugSessionEventData,
  DebugSessionSummary,
} from "@/bindings";

const AT = "2026-09-25T10:32:00.000000Z";

export function makeSession(overrides: Partial<DebugSession> = {}): DebugSession {
  return {
    schemaVersion: 1,
    id: "s-20260925T103200Z-00000000000a",
    projectRoot: "/mock/android-project",
    package: "com.example.app",
    device: { serial: "emulator-5554", avdName: "Pixel_7", model: "sdk_gphone64_arm64" },
    build: {
      id: 12,
      task: ":app:assembleDebug",
      startedAt: AT,
      origin: { kind: "app" },
      apk: { module: ":app", variant: "debug", sha256: "a1".repeat(32), versionCode: 42 },
      mappings: [],
    },
    install: {
      apkSha256: "a1".repeat(32),
      versionCode: 42,
      installedAt: AT,
      by: { kind: "app" },
    },
    openedAt: AT,
    closedAt: null,
    closeReason: null,
    recordedBy: "app",
    kept: false,
    counts: { launches: 0, crashes: 0, anrs: 0, exits: 0, bookmarks: 0, captures: 0 },
    lastEventAt: AT,
    eventCount: 2,
    droppedEvents: 0,
    bytes: 512,
    ...overrides,
  };
}

/** The list row of `session`, as the backend derives it. */
export function summaryOf(session: DebugSession): DebugSessionSummary {
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

export function makeSessionEvent(
  seq: number,
  data: DebugSessionEventData,
  overrides: Partial<Omit<DebugSessionEvent, "kind" | "data">> = {}
): DebugSessionEvent {
  return { seq, at: AT, actor: null, ...overrides, ...data } as DebugSessionEvent;
}

export function makeSessionCrash(overrides: Partial<DebugSessionCrash> = {}): DebugSessionCrash {
  return {
    serial: "emulator-5554",
    pid: 4242,
    summary: "java.lang.IllegalStateException: boom",
    signature: "00000000deadbeef",
    receivedAt: AT,
    deviceTime: "09-25 10:32:05.123",
    attribution: { method: "installRecord", verified: true, reason: null },
    capture: { entries: 450, bytes: 90_000, truncated: false },
    droppedLines: 0,
    ...overrides,
  };
}

export function makeSessionDetail(
  session: DebugSession,
  events: DebugSessionEvent[]
): DebugSessionDetail {
  return {
    session,
    events,
    eventsTruncated: false,
    crashes: events.filter((e) => e.kind === "crash" || e.kind === "anr"),
  };
}
