import type {
  AppError,
  Device,
  LocalRunState,
  ProjectAppInfo,
  ProjectEntry,
  ProjectRunConfigurations,
  ResolvedRun,
  RunConfiguration,
  RunDevice,
  SharedRunConfigurationsFile,
  TargetPreference,
} from "@/bindings";
import { mockDeviceSelection } from "./devices";

export const mockProject: ProjectEntry = {
  id: "abc123",
  path: "/mock/android-project",
  name: "MockProject",
  gradleRoot: "/mock/android-project",
  lastOpened: new Date().toISOString(),
  pinned: false,
  lastBuildVariant: "debug",
  lastDevice: null,
  trusted: true,
};

/** Most run configurations per project, as `MAX_RUN_CONFIGURATIONS` in the backend. */
const MAX_MOCK_RUN_CONFIGURATIONS = 32;

function appError(kind: AppError["kind"], message: string): AppError {
  return { kind, message } as AppError;
}

/** The mock project's shared file, as the backend's `.keynobi/run-configurations.json`. */
const SHARED_FILE = ".keynobi/run-configurations.json";

/** The shared file's text; null when the mock project has none. */
let mockSharedFile: string | null = null;

/** The text of the mock project's shared file, as a test would read it on disk. */
export function mockSharedRunConfigurationsFile(): string | null {
  return mockSharedFile;
}

/** Replace the shared file, as a pulled commit would. */
export function setMockSharedRunConfigurationsFile(text: string | null): void {
  mockSharedFile = text;
}

/** A stand-in for the file's SHA-256: 64 hex characters that change with the text. */
function mockSha256(text: string): string {
  let out = "";
  for (let seed = 0; out.length < 64; seed++) {
    let hash = 0x811c9dc5 ^ seed;
    for (let i = 0; i < text.length; i++) {
      hash = Math.imul(hash ^ text.charCodeAt(i), 0x01000193) >>> 0;
    }
    out += hash.toString(16).padStart(8, "0");
  }
  return out.slice(0, 64);
}

/** The shared file's configurations, or the reason it cannot be used. */
function readMockSharedFile(): { configurations: RunConfiguration[]; error: string | null } {
  if (mockSharedFile === null) return { configurations: [], error: null };
  try {
    const parsed = JSON.parse(mockSharedFile) as {
      schemaVersion?: number;
      configurations?: Partial<RunConfiguration>[];
    };
    if (parsed.schemaVersion !== 1) {
      return { configurations: [], error: "It has no schemaVersion (expected 1)." };
    }
    return {
      configurations: (parsed.configurations ?? []).map((c) => ({
        name: c.name ?? "",
        module: c.module ?? ":app",
        variant: c.variant ?? "debug",
        task: c.task ?? null,
        launch: c.launch ?? { kind: "default" },
        logcatFilter: c.logcatFilter ?? null,
      })),
      error: null,
    };
  } catch (e) {
    return { configurations: [], error: `It is not valid JSON: ${String(e)}.` };
  }
}

/** Write the shared file with only the portable fields, or remove it when empty. */
function writeMockSharedFile(configurations: RunConfiguration[]): void {
  if (readMockSharedFile().error) {
    throw appError(
      "invalidInput",
      `The project's shared run configurations (${SHARED_FILE}) cannot be changed because the file cannot be used. Fix or remove the file first.`
    );
  }
  if (configurations.length === 0) {
    mockSharedFile = null;
    return;
  }
  const portable = configurations.map((c) => ({
    name: c.name,
    module: c.module,
    variant: c.variant,
    ...(c.task ? { task: c.task } : {}),
    ...(c.launch.kind !== "default" ? { launch: c.launch } : {}),
    ...(c.logcatFilter ? { logcatFilter: c.logcatFilter } : {}),
  }));
  mockSharedFile = `${JSON.stringify({ schemaVersion: 1, configurations: portable }, null, 2)}\n`;
}

function sameName(a: string, b: string): boolean {
  return a.toLowerCase() === b.toLowerCase();
}

/** Shared configurations no local one hides, as the backend merges them. */
function visibleShared(): RunConfiguration[] {
  const locals = mockProject.runConfigurations ?? [];
  return readMockSharedFile().configurations.filter(
    (shared) => !locals.some((local) => sameName(local.name, shared.name))
  );
}

function localStateOf(name: string): LocalRunState {
  return (
    mockProject.runLocal?.[name] ?? {
      target: { kind: "lastUsed" },
      lastDevice: null,
      approvedProjectFileSha256: null,
    }
  );
}

/**
 * Like the backend: the first read creates a Default configuration for the
 * mock project's only application module, from its last variant and device.
 */
function mockRunConfigurations(): ProjectRunConfigurations {
  if (mockProject.runConfigurations === undefined) {
    mockProject.runConfigurations = [
      {
        name: "Default",
        module: ":app",
        variant: mockProject.lastBuildVariant ?? "debug",
        task: null,
        launch: { kind: "default" },
        logcatFilter: null,
      },
    ];
    mockProject.runLocal = {
      Default: {
        target: { kind: "lastUsed" },
        lastDevice: mockProject.lastDevice,
        approvedProjectFileSha256: null,
      },
    };
    mockProject.activeRunConfiguration = "Default";
  }
  const shared = visibleShared();
  const { configurations: inFile, error } = readMockSharedFile();
  const sharedFile: SharedRunConfigurationsFile | null =
    mockSharedFile === null
      ? null
      : {
          path: SHARED_FILE,
          sha256: mockSha256(mockSharedFile),
          error,
          problems: inFile
            .filter((c) => !shared.includes(c))
            .map((c) => ({
              name: c.name,
              message: "Your local configuration has the same name and is used instead.",
            })),
        };
  return {
    configurations: [...mockProject.runConfigurations, ...shared],
    active: mockProject.activeRunConfiguration ?? null,
    local: { ...mockProject.runLocal },
    shared: shared.map((c) => c.name),
    sharedFile,
  };
}

function requireRunConfiguration(name: string): void {
  if (!mockRunConfigurations().configurations.some((c) => c.name === name)) {
    throw appError("notFound", `There is no run configuration named '${name}'.`);
  }
}

function mockRunDevice(device: Device): RunDevice {
  return { serial: device.serial, label: device.avdName ?? device.model ?? device.serial };
}

/** Like the backend: the one online device the configuration's target names. */
function mockRunTarget(config: RunConfiguration, selectedSerial: string | null): RunDevice {
  const selection = mockDeviceSelection();
  const online = (serial: string | null | undefined) =>
    selection.devices.find((d) => d.serial === serial && d.connectionState === "online");
  const target = mockProject.runLocal?.[config.name]?.target ?? { kind: "lastUsed" };
  const selected = online(selectedSerial ?? selection.selected);
  let device: Device | undefined;
  switch (target.kind) {
    case "serial":
      device = online(target.serial);
      if (!device) {
        throw appError(
          "notFound",
          `Run configuration '${config.name}' runs on device ${target.serial}, which is not online. Connect it, or change the configuration's target.`
        );
      }
      return mockRunDevice(device);
    case "avd":
      device = selection.devices.find(
        (d) => d.avdName === target.name && d.connectionState === "online"
      );
      if (!device) {
        throw appError(
          "notFound",
          `Run configuration '${config.name}' runs on the AVD ${target.name}, which is not running. Launch it, then run again.`
        );
      }
      return mockRunDevice(device);
    case "lastUsed":
      device = online(mockProject.runLocal?.[config.name]?.lastDevice) ?? selected;
      break;
    case "ask":
      device = selected;
      break;
  }
  if (!device) {
    throw appError(
      "notFound",
      `Run configuration '${config.name}' runs on the selected device, and no device is selected. Pick a device in the Devices sidebar, or launch an AVD, then run again.`
    );
  }
  return mockRunDevice(device);
}

/** Like the backend's plan line. */
function mockRunPlan(config: RunConfiguration, task: string, device: RunDevice | null): string {
  const steps = [`build ${task}`];
  if (device) {
    const on = device.label;
    const launch = config.launch;
    if (launch.kind === "none") {
      steps.push(`install this build's APK on ${on} (no launch)`);
    } else {
      steps.push("install this build's APK");
      if (launch.kind === "default") steps.push(`launch the app on ${on}`);
      if (launch.kind === "activity") steps.push(`launch ${launch.name} on ${on}`);
      if (launch.kind === "deepLink") steps.push(`open ${launch.uri} on ${on}`);
      steps.push(`filter ${config.logcatFilter ?? "package:mine"}`);
    }
  }
  return `${device ? "Run" : "Build"} '${config.name}': ${steps.join(" → ")}`;
}

function mockResolveRun(args: unknown): ResolvedRun {
  const { name, selectedSerial, buildOnly } = (args ?? {}) as {
    name?: string | null;
    selectedSerial?: string | null;
    buildOnly?: boolean;
  };
  const project = mockRunConfigurations();
  const chosen = name ?? project.active;
  if (!chosen) {
    const listed = project.configurations
      .map((c) => `${c.name} (${c.module} ${c.variant})`)
      .join(", ");
    throw appError(
      "invalidInput",
      `No run configuration is active. Choose the one to run: ${listed}.`
    );
  }
  const config = project.configurations.find((c) => c.name === chosen);
  if (!config) throw appError("notFound", `There is no run configuration named '${chosen}'.`);
  const variant = config.variant.charAt(0).toUpperCase() + config.variant.slice(1);
  const task = config.task ?? `${config.module}:assemble${variant}`;
  if (mockProject.trusted !== true) {
    throw appError(
      "permissionDenied",
      "This project is not trusted, so Keynobi will not run its Gradle build scripts."
    );
  }
  if (project.shared.includes(config.name)) {
    const reasons: string[] = [];
    const taskName = task.split(":").pop() ?? task;
    if (!taskName.startsWith("assemble")) {
      reasons.push(`builds ${task}, which is not an assemble task`);
    }
    if (!buildOnly && config.launch.kind === "deepLink") {
      reasons.push(`opens the deep link ${config.launch.uri}`);
    }
    const approved = localStateOf(config.name).approvedProjectFileSha256;
    if (reasons.length && approved !== project.sharedFile?.sha256) {
      throw appError(
        "approvalRequired",
        `Run configuration '${config.name}' is shared with the project (${SHARED_FILE}) and ${reasons.join(" and ")}. ${approved ? "The file changed since you approved it." : "You have not approved it yet."} Review it, then approve it to run it.`
      );
    }
  }
  const device = buildOnly ? null : mockRunTarget(config, selectedSerial ?? null);
  return {
    name: config.name,
    module: config.module,
    variant: config.variant,
    task,
    launch: config.launch,
    logcatFilter: config.logcatFilter,
    device,
    plan: mockRunPlan(config, task, device),
  };
}

export function projectHandlers(): Record<string, (args: unknown) => unknown> {
  return {
    list_run_configurations: () => mockRunConfigurations(),
    save_run_configuration: (args) => {
      const { config, shared } = args as { config: RunConfiguration; shared?: boolean | null };
      const isShared = mockRunConfigurations().shared.includes(config.name);
      const toShared = shared ?? isShared;
      if (config.module !== ":app") {
        throw appError(
          "invalidInput",
          `'${config.module}' is not an application module of this project. Application modules: :app.`
        );
      }
      if (!config.name.trim()) {
        throw appError("invalidInput", "A run configuration needs a name.");
      }
      if (config.task && !config.task.startsWith(`${config.module}:`)) {
        throw appError(
          "invalidInput",
          `The task '${config.task}' is not a task of ${config.module}: name it in the module (for example ${config.module}:assembleDebug).`
        );
      }
      if (toShared) {
        const inFile = readMockSharedFile().configurations;
        const index = inFile.findIndex((c) => sameName(c.name, config.name));
        if (index < 0) inFile.push(config);
        else inFile[index] = config;
        writeMockSharedFile(inFile);
        mockProject.runConfigurations = mockProject.runConfigurations?.filter(
          (c) => c.name !== config.name
        );
        mockProject.runLocal = {
          ...mockProject.runLocal,
          [config.name]: localStateOf(config.name),
        };
        return mockRunConfigurations();
      }
      if (isShared) {
        writeMockSharedFile(
          readMockSharedFile().configurations.filter((c) => c.name !== config.name)
        );
      }
      const configurations = [...(mockProject.runConfigurations ?? [])];
      const index = configurations.findIndex((c) => c.name === config.name);
      if (index < 0 && configurations.length >= MAX_MOCK_RUN_CONFIGURATIONS) {
        throw appError(
          "invalidInput",
          `A project can have at most ${MAX_MOCK_RUN_CONFIGURATIONS} run configurations. Delete one first.`
        );
      }
      if (index < 0) configurations.push(config);
      else configurations[index] = config;
      mockProject.runConfigurations = configurations;
      mockProject.runLocal = { ...mockProject.runLocal, [config.name]: localStateOf(config.name) };
      return mockRunConfigurations();
    },
    delete_run_configuration: (args) => {
      const { name } = args as { name: string };
      requireRunConfiguration(name);
      if (mockRunConfigurations().shared.includes(name)) {
        writeMockSharedFile(readMockSharedFile().configurations.filter((c) => c.name !== name));
      }
      mockProject.runConfigurations = mockProject.runConfigurations?.filter((c) => c.name !== name);
      const local = { ...mockProject.runLocal };
      delete local[name];
      mockProject.runLocal = local;
      if (mockProject.activeRunConfiguration === name) delete mockProject.activeRunConfiguration;
      return mockRunConfigurations();
    },
    resolve_run_configuration: (args) => mockResolveRun(args),
    approve_shared_run_configuration: (args) => {
      const { name, sha256 } = args as { name: string; sha256: string };
      const project = mockRunConfigurations();
      if (!project.shared.includes(name)) {
        throw appError("notFound", `There is no shared run configuration named '${name}'.`);
      }
      if (project.sharedFile?.sha256 !== sha256) {
        throw appError(
          "invalidInput",
          `The project's shared run configurations (${SHARED_FILE}) changed since you reviewed them. Review them again.`
        );
      }
      mockProject.runLocal = {
        ...mockProject.runLocal,
        [name]: { ...localStateOf(name), approvedProjectFileSha256: sha256 },
      };
      return mockRunConfigurations();
    },
    list_application_modules: () => [":app"],
    set_run_configuration_target: (args) => {
      const { name, target } = args as { name: string; target: TargetPreference };
      requireRunConfiguration(name);
      const local = mockProject.runLocal?.[name] ?? {
        target: { kind: "lastUsed" },
        lastDevice: null,
        approvedProjectFileSha256: null,
      };
      mockProject.runLocal = { ...mockProject.runLocal, [name]: { ...local, target } };
      return mockRunConfigurations();
    },
    record_run_device: (args) => {
      const { name, serial } = args as { name: string; serial: string };
      requireRunConfiguration(name);
      const local = mockProject.runLocal?.[name] ?? {
        target: { kind: "lastUsed" },
        lastDevice: null,
        approvedProjectFileSha256: null,
      };
      mockProject.runLocal = { ...mockProject.runLocal, [name]: { ...local, lastDevice: serial } };
      return mockRunConfigurations();
    },
    set_active_run_configuration: (args) => {
      const { name } = args as { name: string };
      requireRunConfiguration(name);
      mockProject.activeRunConfiguration = name;
      return mockRunConfigurations();
    },
    open_project: () => mockProject.name,
    get_project_root: () => mockProject.path,
    get_gradle_root: () => mockProject.gradleRoot,
    get_application_id: () => "com.example.mockapp",
    list_projects: () => [{ ...mockProject }],
    remove_project: () => undefined,
    pin_project: () => undefined,
    set_project_trust: (args) => {
      mockProject.trusted = (args as { trusted: boolean }).trusted;
    },
    get_last_active_project: () => null,
    get_project_app_info: (): ProjectAppInfo => ({
      applicationId: "com.example.mockapp",
      versionName: "1.0.0",
      versionCode: 1,
      versionNameUnavailable: null,
      versionCodeUnavailable: null,
    }),
    save_project_app_info: () => undefined,
    update_project_meta: () => undefined,
    rename_project: () => undefined,
    get_variants_preview: () => ({
      variants: [
        {
          name: "debug",
          buildType: "debug",
          flavors: [],
          assembleTask: "assembleDebug",
          installTask: "installDebug",
        },
        {
          name: "release",
          buildType: "release",
          flavors: [],
          assembleTask: "assembleRelease",
          installTask: "installRelease",
        },
      ],
      active: "debug",
      defaultVariant: "debug",
    }),
    get_variants_from_gradle: () => ({
      variants: [
        {
          name: "debug",
          buildType: "debug",
          flavors: [],
          assembleTask: "assembleDebug",
          installTask: "installDebug",
        },
        {
          name: "release",
          buildType: "release",
          flavors: [],
          assembleTask: "assembleRelease",
          installTask: "installRelease",
        },
      ],
      active: "debug",
      defaultVariant: "debug",
    }),
    set_active_variant: () => undefined,
    open_in_studio: () => "/mock/file.kt",
  };
}
