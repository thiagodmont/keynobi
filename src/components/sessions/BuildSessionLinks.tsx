import { type JSX, For, createResource } from "solid-js";
import type { BuildRecord, DebugSessionSummary } from "@/bindings";
import { Button } from "@/components/ui";
import { listDebugSessions } from "@/lib/tauri-api";
import { buildState } from "@/stores/build.store";
import { crashCountLabel, sessionDeviceLabel } from "./session-format";
import { openSessionsDialog } from "./SessionsDialog";

/**
 * The newest debug session of `record`'s APK on each device, newest first.
 * The hash is compared too, so a build ID reused after the history was
 * cleared is not mistaken for the record.
 */
export function sessionsOfBuild(
  record: BuildRecord,
  sessions: readonly DebugSessionSummary[]
): DebugSessionSummary[] {
  const devices = new Set<string>();
  return sessions.filter((s) => {
    if (s.buildId !== record.id || !record.apks.some((a) => a.sha256 === s.apkSha256)) {
      return false;
    }
    const device = s.device.avdName ?? s.device.serial;
    if (devices.has(device)) return false;
    devices.add(device);
    return true;
  });
}

/** "Session: 2 crashes on Pixel_7", one link per device, opening the session. */
export function BuildSessionLinks(props: { record: BuildRecord }): JSX.Element {
  // Read again when another build is viewed and as a deploy moves through its phases.
  const [sessions] = createResource(
    () => ({ id: props.record.id, deploy: buildState.deployPhase }),
    () =>
      listDebugSessions().catch((e: unknown) => {
        console.warn("[sessions] Failed to read debug sessions:", e);
        return [] as DebugSessionSummary[];
      })
  );
  return (
    <For each={sessionsOfBuild(props.record, sessions() ?? [])}>
      {(session) => (
        <Button
          variant="ghost"
          size="xs"
          title="Open this debug session"
          onClick={() => openSessionsDialog(session.id)}
        >
          Session: {crashCountLabel(session.counts)} on {sessionDeviceLabel(session.device)}
        </Button>
      )}
    </For>
  );
}
