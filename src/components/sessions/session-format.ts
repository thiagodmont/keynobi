import type {
  AppExitReason,
  BuildActor,
  DebugSession,
  DebugSessionCrash,
  DebugSessionDevice,
  DebugSessionEvent,
  DebugSessionSummary,
} from "@/bindings";
import type { BadgeVariant } from "@/components/ui";
import { describeDisplayTimes, formatLaunchTime } from "@/lib/launch-timing";

export type SessionState = "open" | "superseded" | "ended" | "idle";

type SessionLike = Pick<DebugSessionSummary, "closedAt" | "closeReason">;

/** Open until closed; a closed session says why. */
export function sessionState(session: SessionLike): SessionState {
  if (session.closedAt === null) return "open";
  return session.closeReason ?? "ended";
}

export const STATE_LABELS: Record<SessionState, string> = {
  open: "Open",
  superseded: "Superseded",
  ended: "Ended",
  idle: "Idle",
};

export function stateVariant(state: SessionState): BadgeVariant {
  return state === "open" ? "success" : "default";
}

/** The AVD for an emulator, else the model, else the serial. */
export function sessionDeviceLabel(device: DebugSessionDevice): string {
  return device.avdName ?? device.model ?? device.serial;
}

/** "#12 · :app debug", or why no build is named. */
export function sessionBuildLabel(
  session: Pick<DebugSessionSummary, "buildId" | "module" | "variant" | "apkSha256">
): string {
  if (session.buildId === null) {
    return session.apkSha256 === null ? "No Keynobi install" : "No build record";
  }
  const where = [session.module, session.variant].filter(Boolean).join(" ");
  return where ? `#${session.buildId} · ${where}` : `#${session.buildId}`;
}

/** A session no Keynobi install opened: crashes of an app Keynobi did not install. */
export function isUnattributed(session: Pick<DebugSessionSummary, "apkSha256">): boolean {
  return session.apkSha256 === null;
}

function plural(n: number, one: string, many = `${one}s`): string {
  return `${n} ${n === 1 ? one : many}`;
}

/** "2 crashes · 1 ANR · 3 launches". */
export function sessionCountsLabel(counts: DebugSessionSummary["counts"]): string {
  return [
    plural(counts.crashes, "crash", "crashes"),
    plural(counts.anrs, "ANR"),
    plural(counts.launches, "launch", "launches"),
  ].join(" · ");
}

/** "2 crashes", "1 crash, 1 ANR", or "no crashes". */
export function crashCountLabel(counts: DebugSessionSummary["counts"]): string {
  const parts: string[] = [];
  if (counts.crashes > 0) parts.push(plural(counts.crashes, "crash", "crashes"));
  if (counts.anrs > 0) parts.push(plural(counts.anrs, "ANR"));
  return parts.length > 0 ? parts.join(", ") : "no crashes";
}

/** "10:32:05" today, else the date and time. */
export function formatSessionTime(iso: string, now: Date = new Date()): string {
  const date = new Date(iso);
  if (Number.isNaN(date.getTime())) return iso;
  if (date.toDateString() === now.toDateString()) {
    return date.toLocaleTimeString(undefined, {
      hour: "2-digit",
      minute: "2-digit",
      second: "2-digit",
    });
  }
  return date.toLocaleString(undefined, { dateStyle: "medium", timeStyle: "medium" });
}

/** "Keynobi", "an agent (Claude Code)", "Keynobi (quitting)". */
export function actorLabel(actor: BuildActor | null): string | null {
  if (!actor) return null;
  switch (actor.kind) {
    case "app":
      return "Keynobi";
    case "appQuit":
      return "Keynobi (quitting)";
    case "agent": {
      const name = actor.clientName ? `an agent (${actor.clientName})` : "an agent";
      return actor.standalone ? `${name}, standalone` : name;
    }
  }
}

export const EXIT_REASON_LABELS: Record<AppExitReason, string> = {
  crash: "Crash",
  crashNative: "Native crash",
  anr: "ANR",
  lowMemory: "Low memory",
  exitSelf: "Exited",
  signaled: "Killed by signal",
  userRequested: "Stopped by user",
  userStopped: "User stopped",
  dependencyDied: "Dependency died",
  excessiveResourceUsage: "Excessive resource use",
  initializationFailure: "Initialization failed",
  permissionChange: "Permission changed",
  freezer: "Freezer",
  other: "Killed by system",
  packageStateChange: "Package state changed",
  packageUpdated: "Package updated",
  unknown: "Unknown",
};

const EXIT_MATCH_LABELS = {
  pid: "matched by pid",
  processName: "matched by process name",
  timeWindow: "matched by time only",
} as const;

export interface EventView {
  /** Short label of the kind: "Crash", "Launch". */
  label: string;
  variant: BadgeVariant;
  /** One line describing the event. */
  text: string;
}

/** How a crash was matched to the session: "Keynobi's install · verified". */
export function attributionLabel(crash: DebugSessionCrash): string {
  const { attribution } = crash;
  if (attribution.method === "unattributed") return "Not from a Keynobi install";
  return attribution.verified
    ? "Keynobi's install · confirmed by the device"
    : "Keynobi's install · not confirmed";
}

/** The label, badge color, and one-line description of a timeline event. */
export function describeEvent(event: DebugSessionEvent): EventView {
  switch (event.kind) {
    case "build": {
      const { id, task, apk } = event.data;
      return {
        label: "Build",
        variant: "info",
        text: `Build #${id} · ${apk.module} ${apk.variant} · ${task}`,
      };
    }
    case "install": {
      const { apkSha256, versionCode } = event.data;
      const by = actorLabel(event.data.by);
      const version = versionCode !== null ? ` (version code ${versionCode})` : "";
      return {
        label: "Install",
        variant: "info",
        text: `Installed APK ${apkSha256.slice(0, 12)}…${version}${by ? ` by ${by}` : ""}`,
      };
    }
    case "launch": {
      const { timing, restart } = event.data;
      const verb = restart ? "Restarted" : "Launched";
      const time = timing
        ? [`Launch ${formatLaunchTime(timing)}`, ...describeDisplayTimes(timing)].join(" · ")
        : "no launch time reported";
      const by = actorLabel(event.actor);
      return {
        label: restart ? "Restart" : "Launch",
        variant: "accent",
        text: `${verb} · ${time}${by ? ` · by ${by}` : ""}`,
      };
    }
    case "launchTiming": {
      const times = describeDisplayTimes(event.data);
      return {
        label: "Display",
        variant: "accent",
        text: `Display times of the launch at ${formatSessionTime(event.data.measuredAt)}: ${
          times.length > 0 ? times.join(" · ") : "none"
        }`,
      };
    }
    case "logcatReconnect":
      return {
        label: "Logcat",
        variant: "warning",
        text: `Logcat reconnecting to ${event.data.serial}`,
      };
    case "logcatStopped":
      return {
        label: "Logcat",
        variant: "warning",
        text: `Logcat stopped${event.data.reason ? `: ${event.data.reason}` : ""}`,
      };
    case "logcatCleared":
      return { label: "Logcat", variant: "default", text: "Logcat cleared" };
    case "deviceOffline":
      return {
        label: "Offline",
        variant: "warning",
        text: `${event.data.serial} went offline`,
      };
    case "deviceOnline":
      return { label: "Online", variant: "success", text: `${event.data.serial} is back online` };
    case "bookmark":
      return { label: "Bookmark", variant: "accent", text: event.data.note };
    case "crash":
      return { label: "Crash", variant: "error", text: event.data.summary };
    case "anr":
      return { label: "ANR", variant: "error", text: event.data.summary };
    case "exit": {
      const { record, matchedBy } = event.data;
      const parts = [
        `Exit: ${EXIT_REASON_LABELS[record.reason]}`,
        record.processName,
        record.pid !== null ? `pid ${record.pid}` : null,
        EXIT_MATCH_LABELS[matchedBy],
      ].filter((p): p is string => Boolean(p));
      const failed = record.reason === "crash" || record.reason === "crashNative";
      return {
        label: "Exit",
        variant: failed || record.reason === "anr" ? "error" : "default",
        text: parts.join(" · "),
      };
    }
  }
}

/** A crash or ANR, with its details. */
export function crashOf(event: DebugSessionEvent): DebugSessionCrash | null {
  return event.kind === "crash" || event.kind === "anr" ? event.data : null;
}

/** Whether a session takes bookmarks and can be ended. */
export function isOpen(session: Pick<DebugSession, "closedAt">): boolean {
  return session.closedAt === null;
}
