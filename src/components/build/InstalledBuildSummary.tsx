import { type JSX, For, createResource } from "solid-js";
import type { BuildRecord, InstalledBuild } from "@/bindings";
import { Badge } from "@/components/ui";
import { listInstalledBuilds } from "@/lib/tauri-api";
import { buildState } from "@/stores/build.store";

/** The AVD for an emulator, else the model, else the serial. */
export function installedDeviceLabel(install: InstalledBuild): string {
  return install.avdName ?? install.model ?? install.serial;
}

/** "10:32" today, else the date and time. */
export function formatInstalledAt(iso: string, now: Date = new Date()): string {
  const date = new Date(iso);
  if (Number.isNaN(date.getTime())) return iso;
  if (date.toDateString() === now.toDateString()) {
    return date.toLocaleTimeString(undefined, { hour: "2-digit", minute: "2-digit" });
  }
  return date.toLocaleString(undefined, { dateStyle: "medium", timeStyle: "short" });
}

/** "Installed on Pixel_7 · 10:32". */
export function installLabel(install: InstalledBuild, now?: Date): string {
  return `Installed on ${installedDeviceLabel(install)} · ${formatInstalledAt(install.installedAt, now)}`;
}

function installDetails(install: InstalledBuild): string {
  return [
    install.package,
    install.avdName ? `${install.avdName} (${install.serial})` : install.serial,
    install.versionCode !== null ? `version code ${install.versionCode}` : null,
    `SHA-256 ${install.apkSha256.slice(0, 12)}…`,
  ]
    .filter((part) => part !== null)
    .join(" · ");
}

/**
 * The devices where `record`'s APK is still the last one Keynobi installed of
 * its package. The hash is compared too, so a record ID reused after the
 * history was cleared is not mistaken for the installed build.
 */
export function installsOf(
  record: BuildRecord,
  installs: readonly InstalledBuild[]
): InstalledBuild[] {
  return installs.filter(
    (i) => i.buildId === record.id && record.apks.some((a) => a.sha256 === i.apkSha256)
  );
}

/** One badge per device the past build is installed on. */
export function InstalledBuildSummary(props: { record: BuildRecord }): JSX.Element {
  // Read again when another build is viewed and as a deploy moves through its phases.
  const [installs] = createResource(
    () => ({ id: props.record.id, deploy: buildState.deployPhase }),
    () =>
      listInstalledBuilds().catch((e: unknown) => {
        console.warn("[build] Failed to read installed builds:", e);
        return [] as InstalledBuild[];
      })
  );
  return (
    <For each={installsOf(props.record, installs() ?? [])}>
      {(install) => (
        <Badge variant="info" size="xs" subtle title={installDetails(install)}>
          {installLabel(install)}
        </Badge>
      )}
    </For>
  );
}
