import type {
  AppError,
  ProjectAppInfo,
  ProjectEntry,
  ProjectRunConfigurations,
  RunConfiguration,
} from "@/bindings";

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
