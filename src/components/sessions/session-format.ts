import type {
  AppExitReason,
  BuildActor,
  BuildProvenance,
  DebugSession,
  DebugSessionCrash,
  DebugSessionDevice,
  DebugSessionEvent,
  DebugSessionImport,
  DebugSessionSummary,
  RedactionRule,
  SessionExportResult,
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

/**
 * The source the build came from: "3f9c2e1 · main · 2 uncommitted changes",
 * or why it has no commit.
 */
export function sessionSourceLabel(provenance: BuildProvenance | null | undefined): string {
  if (!provenance) return "Not recorded";
  if (provenance.commit === null) {
    return provenance.gitUnavailable ? `No commit (${provenance.gitUnavailable})` : "No commit";
  }
  const parts = [provenance.commit.slice(0, 7)];
  if (provenance.branch) parts.push(provenance.branch);
  if (provenance.dirty) {
    parts.push(
      provenance.changedFiles !== null
        ? plural(provenance.changedFiles, "uncommitted change")
        : "uncommitted changes"
    );
  }
  return parts.join(" · ");
}

/** A session no Keynobi install opened: crashes of an app Keynobi did not install. */
export function isUnattributed(session: Pick<DebugSessionSummary, "apkSha256">): boolean {
  return session.apkSha256 === null;
}

/** A session imported from a bundle: read-only, recorded elsewhere. */
export function isImported(session: Pick<DebugSessionSummary, "recordedBy">): boolean {
  return session.recordedBy === "imported";
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
    case "agentAction": {
      const { tool, ok, durationMs } = event.data;
      const by = actorLabel(event.actor);
      const parts = [
        `${tool}${by ? ` by ${by}` : ""}`,
        `${durationMs} ms`,
        ok ? null : "failed",
      ].filter((p): p is string => Boolean(p));
      return {
        label: "Agent",
        variant: ok ? "default" : "warning",
        text: parts.join(" · "),
      };
    }
    case "attachment": {
      const { width, height, nodeCount, bytes } = event.data;
      const by = actorLabel(event.actor);
      const tail = `${formatBytes(bytes)}${by ? ` · by ${by}` : ""}`;
      if (event.data.kind === "hierarchy") {
        return {
          label: "Hierarchy",
          variant: "info",
          text: `UI hierarchy attached · ${plural(nodeCount ?? 0, "node")} · ${tail}`,
        };
      }
      return {
        label: "Screenshot",
        variant: "info",
        text: `Screenshot attached · ${width ?? 0}×${height ?? 0} · ${tail}`,
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

/** What each redaction rule replaces, for the export options. */
export const REDACTION_RULE_LABELS: Record<RedactionRule, string> = {
  emails: "Email addresses",
  secrets: "Secrets: tokens, keys, passwords, credentials",
  ipAddresses: "IP addresses (not loopback or 10.0.2.2)",
  paths: "Home and project folders",
  deviceSerials: "Physical device serials (emulators stay)",
};

const REDACTION_NOUNS: Record<RedactionRule, [string, string]> = {
  emails: ["email", "emails"],
  secrets: ["secret", "secrets"],
  ipAddresses: ["IP address", "IP addresses"],
  paths: ["path", "paths"],
  deviceSerials: ["device serial", "device serials"],
};

/** Bytes for people: "812 B", "4.0 KB", "1.2 MB". */
function formatBytes(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
  return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
}

/** "Saved x.zip (4.0 KB). Redacted 2 emails, 1 path. Left out: R8 mappings." */
export function exportResultLabel(result: SessionExportResult): string {
  const name = result.path.split("/").pop() ?? result.path;
  const redacted = result.redactions
    .filter((r) => r.count > 0)
    .map((r) => plural(r.count, ...REDACTION_NOUNS[r.rule]));
  const off = result.redactions.filter((r) => !r.enabled).map((r) => REDACTION_NOUNS[r.rule][1]);
  return [
    `Saved ${name} (${formatBytes(result.bytes)}).`,
    redacted.length > 0 ? `Redacted ${redacted.join(", ")}.` : "Nothing needed redacting.",
    off.length > 0 ? `Not redacted: ${off.join(", ")}.` : null,
    result.omitted.length > 0 ? `Left out: ${result.omitted.map((o) => o.item).join(", ")}.` : null,
  ]
    .filter(Boolean)
    .join(" ");
}

/**
 * What an imported bundle says about itself: "Exported … by Keynobi 0.9.0 as
 * x.zip. Redacted 2 emails. Left out: R8 mappings (never exported)."
 */
export function importSummaryLabel(imported: DebugSessionImport): string {
  const redacted = imported.redactions
    .filter((r) => r.count > 0)
    .map((r) => plural(r.count, ...REDACTION_NOUNS[r.rule]));
  const off = imported.redactions.filter((r) => !r.enabled).map((r) => REDACTION_NOUNS[r.rule][1]);
  return [
    `Exported ${formatSessionTime(imported.exportedAt)} by Keynobi ${imported.keynobiVersion} as ${imported.fileName}.`,
    redacted.length > 0 ? `Redacted ${redacted.join(", ")}.` : "Nothing was redacted.",
    off.length > 0 ? `Not redacted: ${off.join(", ")}.` : null,
    imported.omitted.length > 0
      ? `Left out: ${imported.omitted.map((o) => `${o.item} (${o.reason})`).join("; ")}.`
      : null,
  ]
    .filter(Boolean)
    .join(" ");
}
