import {
  deleteRunConfiguration as deleteRunConfigurationApi,
  formatError,
  launchAvd,
  listRunConfigurations,
  refreshDevices,
  resolveRunConfiguration,
  saveRunConfiguration as saveRunConfigurationApi,
  setActiveRunConfiguration,
  setRunConfigurationTarget,
} from "@/lib/tauri-api";
import type {
  ProjectRunConfigurations,
  ResolvedRun,
  RunConfiguration,
  TargetPreference,
} from "@/bindings";
import { registerAction, unregisterAction, type ActionCategory } from "@/lib/action-registry";
import { projectState } from "@/stores/project.store";
import {
  activeRunConfiguration,
  resetRunConfigurations,
  runConfigState,
  setRunConfigEditorOpen,
  setRunConfigurations,
} from "@/stores/run-configurations.store";
import { deviceState, setDevices, setLaunchingAvd } from "@/stores/device.store";
import { loadVariants, selectVariant, variantState } from "@/stores/variant.store";
import { showToast } from "@/components/ui";

/** Palette action IDs registered for the open project's configurations. */
let registeredActionIds: string[] = [];

/** Runs and builds a named configuration; set by the build service to avoid an import cycle. */
export interface RunConfigurationRunner {
  run: (name: string) => Promise<void>;
  build: (name: string) => Promise<void>;
}
let runner: RunConfigurationRunner | null = null;

export function setRunConfigurationRunner(next: RunConfigurationRunner): void {
  runner = next;
}

function show(root: string, project: ProjectRunConfigurations): void {
  setRunConfigurations(root, project);
  syncRunConfigurationActions();
}

/** The open project's root, or an error when none is open. */
function openRoot(): string {
  const root = projectState.projectRoot;
  if (!root) throw new Error("Open a project first.");
  return root;
}

/**
 * Load the open project's run configurations (the first read creates them),
 * and follow the active one's module and variant. Clears them when no project
 * is open.
 */
export async function loadRunConfigurations(): Promise<void> {
  const root = projectState.projectRoot;
  if (runConfigState.projectRoot !== root) {
    // Never offer another project's configurations, even when this load fails.
    resetRunConfigurations();
    syncRunConfigurationActions();
  }
  if (!root) return;
  const project = await listRunConfigurations();
  // A newer open owns the store now.
  if (projectState.projectRoot !== root) return;
  show(root, project);
  await followActiveConfiguration();
}

/**
 * The variant picker lists the active configuration's module, and shows its
 * variant. With one application module the module stays implicit.
 */
async function followActiveConfiguration(): Promise<void> {
  const config = activeRunConfiguration();
  if (!config) return;
  const modules = new Set(runConfigState.configurations.map((c) => c.module));
  const module = modules.size > 1 ? config.module : null;
  if (variantState.module !== module) await loadVariants({ module }).catch(console.error);
  if (variantState.activeVariant !== config.variant) {
    await selectVariant(config.variant);
  }
}

/** Make the configuration named `name` the active one. */
export async function chooseRunConfiguration(name: string): Promise<void> {
  const root = openRoot();
  const project = await setActiveRunConfiguration(name);
  if (projectState.projectRoot !== root) return;
  show(root, project);
  await followActiveConfiguration();
}

/** Reload after the variant picker changed the active configuration's variant. */
export async function refreshRunConfigurations(): Promise<void> {
  const root = projectState.projectRoot;
  if (!root || runConfigState.projectRoot !== root) return;
  const project = await listRunConfigurations();
  if (projectState.projectRoot === root) show(root, project);
}

/**
 * Save `config` with its `target`. `previousName` is the name it had when
 * editing started (null for a new one): a renamed configuration replaces it,
 * and stays active when it was. Rejects with the backend's reason, nothing
 * saved.
 */
export async function saveRunConfiguration(
  config: RunConfiguration,
  target: TargetPreference,
  previousName: string | null
): Promise<void> {
  const root = openRoot();
  const renamed = previousName !== null && previousName !== config.name;
  if (renamed && runConfigState.configurations.some((c) => c.name === config.name)) {
    throw new Error(`A run configuration named '${config.name}' already exists.`);
  }
  await saveRunConfigurationApi(config);
  let project = await setRunConfigurationTarget(config.name, target);
  if (renamed) {
    const wasActive = project.active === previousName;
    project = await deleteRunConfigurationApi(previousName);
    if (wasActive) project = await setActiveRunConfiguration(config.name);
  }
  if (projectState.projectRoot !== root) return;
  show(root, project);
  if (project.active === config.name) await followActiveConfiguration();
}

/** Delete the configuration named `name`. */
export async function deleteRunConfiguration(name: string): Promise<void> {
  const root = openRoot();
  const project = await deleteRunConfigurationApi(name);
  if (projectState.projectRoot === root) show(root, project);
}

/** A name for a copy of `name` that no configuration has: `Default copy`, `Default copy 2`. */
export function copyName(name: string, existing: readonly string[]): string {
  const taken = new Set(existing.map((n) => n.toLowerCase()));
  const base = `${name} copy`;
  if (!taken.has(base.toLowerCase())) return base;
  for (let i = 2; ; i++) {
    const next = `${base} ${i}`;
    if (!taken.has(next.toLowerCase())) return next;
  }
}

/** What Run App would do with the configuration named `name` now. */
export function previewRunPlan(name: string): Promise<ResolvedRun> {
  return resolveRunConfiguration({ name, selectedSerial: deviceState.selectedSerial });
}

/**
 * Start the AVD a configuration runs on, at the user's request. Keynobi never
 * starts an emulator on its own.
 */
export async function launchRunAvd(avdName: string): Promise<void> {
  setLaunchingAvd(avdName);
  showToast(`Starting ${avdName}… Run again once it is online.`, "info");
  try {
    await launchAvd(avdName);
    setDevices(await refreshDevices());
  } catch (e) {
    showToast(`Failed to launch ${avdName}: ${formatError(e)}`, "error");
  } finally {
    setLaunchingAvd(null);
  }
}

export function openRunConfigurationsEditor(): void {
  setRunConfigEditorOpen(true);
}

export function closeRunConfigurationsEditor(): void {
  setRunConfigEditorOpen(false);
}

/**
 * Register "Run: <name>" and "Build: <name>" palette actions for the loaded
 * configurations, replacing the previous project's.
 */
export function syncRunConfigurationActions(): void {
  registeredActionIds.forEach(unregisterAction);
  registeredActionIds = [];
  const category: ActionCategory = "Build";
  for (const config of runConfigState.configurations) {
    const name = config.name;
    const runId = `runConfiguration.run:${name}`;
    const buildId = `runConfiguration.build:${name}`;
    registerAction({
      id: runId,
      label: `Run: ${name}`,
      category,
      action: () => void runNamed("run", name),
    });
    registerAction({
      id: buildId,
      label: `Build: ${name}`,
      category,
      action: () => void runNamed("build", name),
    });
    registeredActionIds.push(runId, buildId);
  }
}

async function runNamed(kind: "run" | "build", name: string): Promise<void> {
  if (!runner) return;
  try {
    await (kind === "run" ? runner.run(name) : runner.build(name));
  } catch (e) {
    showToast(formatError(e) || `${kind === "run" ? "Run" : "Build"} failed`, "error");
  }
}

/** Test helper: the palette action IDs registered now. */
export function registeredRunConfigurationActionIds(): readonly string[] {
  return registeredActionIds;
}
