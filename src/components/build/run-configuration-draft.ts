import type { RunConfiguration, RunLaunch, TargetPreference } from "@/bindings";
import { validateLogcatQuery } from "@/lib/logcat-query";

/** A run configuration as the editor holds it while it is being edited. */
export interface RunConfigurationDraft {
  /** Its saved name; null for one not saved yet. */
  savedName: string | null;
  name: string;
  module: string;
  variant: string;
  /** The task to build; the module's assemble task unless changed. */
  task: string;
  launchKind: RunLaunch["kind"];
  activity: string;
  uri: string;
  logcatFilter: string;
  /** The target as a select value (see `targetValue`). */
  target: string;
}

/** The task Run builds when a configuration names none: `:app:assembleDebug`. */
export function defaultTask(module: string, variant: string): string {
  const capitalized = variant.charAt(0).toUpperCase() + variant.slice(1);
  return module === ":" ? `assemble${capitalized}` : `${module}:assemble${capitalized}`;
}

export function targetValue(target: TargetPreference): string {
  switch (target.kind) {
    case "serial":
      return `serial:${target.serial}`;
    case "avd":
      return `avd:${target.name}`;
    default:
      return target.kind;
  }
}

export function parseTarget(value: string): TargetPreference {
  if (value.startsWith("serial:")) return { kind: "serial", serial: value.slice(7) };
  if (value.startsWith("avd:")) return { kind: "avd", name: value.slice(4) };
  return value === "ask" ? { kind: "ask" } : { kind: "lastUsed" };
}

export function draftFrom(
  config: RunConfiguration,
  target: TargetPreference,
  savedName: string | null = config.name
): RunConfigurationDraft {
  return {
    savedName,
    name: config.name,
    module: config.module,
    variant: config.variant,
    task: config.task ?? defaultTask(config.module, config.variant),
    launchKind: config.launch.kind,
    activity: config.launch.kind === "activity" ? config.launch.name : "",
    uri: config.launch.kind === "deepLink" ? config.launch.uri : "",
    logcatFilter: config.logcatFilter ?? "",
    target: targetValue(target),
  };
}

/** Problems the editor shows next to their fields before saving. */
export interface DraftErrors {
  name?: string;
  task?: string;
  launch?: string;
  logcatFilter?: string;
}

export function validateDraft(draft: RunConfigurationDraft): DraftErrors {
  const errors: DraftErrors = {};
  if (!draft.name.trim()) errors.name = "Name the configuration.";
  if (!draft.task.trim()) errors.task = "Name the task to build.";
  if (draft.launchKind === "activity" && !draft.activity.trim()) {
    errors.launch = "Name the activity to launch, for example .MainActivity.";
  }
  if (draft.launchKind === "deepLink" && !draft.uri.trim()) {
    errors.launch = "Enter the deep link to open, for example myapp://home.";
  }
  const filterError = draft.logcatFilter.trim() ? validateLogcatQuery(draft.logcatFilter) : null;
  if (filterError) errors.logcatFilter = filterError;
  return errors;
}

/** The configuration a valid draft saves. The default task and an empty filter save as null. */
export function configurationFrom(draft: RunConfigurationDraft): RunConfiguration {
  const task = draft.task.trim();
  const filter = draft.logcatFilter.trim();
  let launch: RunLaunch;
  switch (draft.launchKind) {
    case "activity":
      launch = { kind: "activity", name: draft.activity.trim() };
      break;
    case "deepLink":
      launch = { kind: "deepLink", uri: draft.uri.trim() };
      break;
    default:
      launch = { kind: draft.launchKind };
  }
  return {
    name: draft.name.trim(),
    module: draft.module,
    variant: draft.variant,
    task: task === defaultTask(draft.module, draft.variant) ? null : task,
    launch,
    logcatFilter: filter || null,
  };
}

/** True when the draft differs from what was saved. */
export function draftChanged(
  draft: RunConfigurationDraft,
  saved: RunConfigurationDraft | null
): boolean {
  if (!saved) return true;
  return (Object.keys(draft) as (keyof RunConfigurationDraft)[]).some(
    (key) => draft[key] !== saved[key]
  );
}
