import type {
  AppError,
  Device,
  ProjectAppInfo,
  ProjectEntry,
  ProjectRunConfigurations,
  ResolvedRun,
  RunConfiguration,
  RunDevice,
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
  return {
    configurations: [...mockProject.runConfigurations],
    active: mockProject.activeRunConfiguration ?? null,
    local: { ...mockProject.runLocal },
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
      const { config } = args as { config: RunConfiguration };
      const configurations = mockRunConfigurations().configurations;
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
      mockProject.runLocal = {
        [config.name]: {
          target: { kind: "lastUsed" },
          lastDevice: null,
          approvedProjectFileSha256: null,
        },
        ...mockProject.runLocal,
      };
      return mockRunConfigurations();
    },
    delete_run_configuration: (args) => {
      const { name } = args as { name: string };
      requireRunConfiguration(name);
      mockProject.runConfigurations = mockProject.runConfigurations?.filter((c) => c.name !== name);
      const local = { ...mockProject.runLocal };
      delete local[name];
      mockProject.runLocal = local;
      if (mockProject.activeRunConfiguration === name) delete mockProject.activeRunConfiguration;
      return mockRunConfigurations();
    },
    resolve_run_configuration: (args) => mockResolveRun(args),
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
