import type { ProjectEntry, ProjectAppInfo } from "@/bindings";

export const mockProject: ProjectEntry = {
  id: "abc123",
  path: "/mock/android-project",
  name: "MockProject",
  gradleRoot: "/mock/android-project",
  lastOpened: new Date().toISOString(),
  pinned: false,
  lastBuildVariant: "debug",
  lastDevice: null,
};

export function projectHandlers(): Record<string, (args: unknown) => unknown> {
  return {
    open_project: () => mockProject.name,
    get_project_root: () => mockProject.path,
    get_gradle_root: () => mockProject.gradleRoot,
    get_application_id: () => "com.example.mockapp",
    list_projects: () => [mockProject],
    remove_project: () => undefined,
    pin_project: () => undefined,
    get_last_active_project: () => null,
    get_project_app_info: (): ProjectAppInfo => ({
      applicationId: "com.example.mockapp",
      versionName: "1.0.0",
      versionCode: BigInt(1),
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
