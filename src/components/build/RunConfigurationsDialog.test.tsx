import { describe, it, expect, beforeEach, afterEach, vi } from "vitest";
import { render, screen, fireEvent, cleanup, within } from "@solidjs/testing-library";
import { invoke } from "@tauri-apps/api/core";
import { RunConfigurationsDialog } from "./RunConfigurationsDialog";
import { DialogHost } from "@/components/ui";
import { resetDialogHostForTests } from "@/components/ui/Dialog/Dialog.test-utils";
import {
  resetRunConfigurationsForTests,
  runConfigState,
  setRunConfigEditorOpen,
  setRunConfigurations,
} from "@/stores/run-configurations.store";
import { clearProject, setProject } from "@/stores/project.store";
import { resetDeviceState, setAvds } from "@/stores/device.store";
import {
  makeProjectRunConfigurations,
  makeResolvedRun,
  makeRunConfiguration,
} from "@/test/factories/build";
import { makeAvd } from "@/test/factories/devices";
import type { ProjectRunConfigurations, RunConfiguration, TargetPreference } from "@/bindings";

vi.mock("@/stores/variant.store", () => ({
  variantState: {
    module: null,
    activeVariant: "debug",
    variants: [{ name: "debug" }, { name: "release" }],
  },
  loadVariants: vi.fn(() => Promise.resolve()),
  selectVariant: vi.fn(() => Promise.resolve()),
}));

const mockInvoke = vi.mocked(invoke);

type Handler = (args: Record<string, unknown>) => unknown;

/** A backend holding the project's configurations; `overrides` replace commands. */
function backend(
  initial: ProjectRunConfigurations,
  overrides: Record<string, Handler> = {}
): { project: () => ProjectRunConfigurations } {
  let project = initial;
  const targets = (): Record<string, TargetPreference> =>
    Object.fromEntries(Object.entries(project.local).map(([n, l]) => [n, l.target]));
  const handlers: Record<string, Handler> = {
    list_application_modules: () => [":app"],
    resolve_run_configuration: ({ name }) =>
      makeResolvedRun({
        name: name as string,
        plan: `Run '${String(name)}': build :app:assembleDebug → install this build's APK → launch the app on Pixel_7`,
      }),
    save_run_configuration: ({ config }) => {
      const saved = config as RunConfiguration;
      const rest = project.configurations.filter((c) => c.name !== saved.name);
      project = {
        ...makeProjectRunConfigurations([...rest, saved], targets()),
        active: project.active,
      };
      return project;
    },
    set_run_configuration_target: ({ name, target }) => {
      project = {
        ...project,
        local: {
          ...project.local,
          [name as string]: {
            target: target as TargetPreference,
            lastDevice: null,
            approvedProjectFileSha256: null,
          },
        },
      };
      return project;
    },
    delete_run_configuration: ({ name }) => {
      const rest = project.configurations.filter((c) => c.name !== name);
      project = { ...makeProjectRunConfigurations(rest, targets()), active: rest[0]?.name ?? null };
      return project;
    },
    ...overrides,
  };
  mockInvoke.mockImplementation((cmd, args) => {
    const handle = handlers[cmd];
    if (!handle) return Promise.reject(new Error(`unexpected ${cmd}`));
    try {
      return Promise.resolve(handle((args ?? {}) as Record<string, unknown>));
    } catch (e) {
      return Promise.reject(e);
    }
  });
  setRunConfigurations("/projects/app", initial);
  return { project: () => project };
}

function callsTo(command: string) {
  return mockInvoke.mock.calls.filter(([cmd]) => cmd === command);
}

function open() {
  setRunConfigEditorOpen(true);
  return render(() => (
    <>
      <RunConfigurationsDialog />
      <DialogHost />
    </>
  ));
}

const dialog = () => screen.getByRole("dialog", { name: "Run Configurations" });
const field = (label: string) => within(dialog()).getByLabelText(label) as HTMLInputElement;
const button = (name: string) => within(dialog()).getByRole("button", { name });

describe("RunConfigurationsDialog", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    resetRunConfigurationsForTests();
    resetDeviceState();
    setProject("/projects/app", "app");
  });

  afterEach(() => {
    cleanup();
    resetDialogHostForTests();
    clearProject();
  });

  it("is a modal dialog that traps focus and closes on Escape", async () => {
    backend(makeProjectRunConfigurations());
    open();

    expect(dialog().getAttribute("aria-modal")).toBe("true");
    expect(dialog().contains(document.activeElement)).toBe(true);

    const save = button("Save");
    const close = button("Close");
    // Save is disabled until something changes, so Close is the last focusable control.
    expect((save as HTMLButtonElement).disabled).toBe(true);
    close.focus();
    fireEvent.keyDown(close, { key: "Tab" });
    expect(dialog().contains(document.activeElement)).toBe(true);
    expect(document.activeElement).not.toBe(close);

    fireEvent.keyDown(dialog(), { key: "Escape" });
    await vi.waitFor(() => expect(runConfigState.editorOpen).toBe(false));
  });

  it("lists the configurations, marks the active one, and edits the selected one", async () => {
    backend(
      makeProjectRunConfigurations([
        makeRunConfiguration(),
        makeRunConfiguration({ name: "Settings", launch: { kind: "activity", name: ".Settings" } }),
      ])
    );
    open();

    const list = within(dialog()).getByRole("listbox", { name: "Run configurations" });
    const options = within(list).getAllByRole("option");
    expect(options.map((o) => o.textContent)).toEqual(["DefaultActive", "Settings"]);
    expect(options[0].getAttribute("aria-selected")).toBe("true");
    expect(field("Name").value).toBe("Default");
    expect(field("Gradle task").value).toBe(":app:assembleDebug");

    fireEvent.click(options[1]);

    await vi.waitFor(() => expect(field("Name").value).toBe("Settings"));
    expect(field("Activity").value).toBe(".Settings");
  });

  it("shows the resolved plan of the saved configuration", async () => {
    backend(makeProjectRunConfigurations());
    open();

    const plan = await within(dialog()).findByTestId("run-config-plan");
    expect(plan.textContent).toContain("Run 'Default': build :app:assembleDebug");
    expect(callsTo("resolve_run_configuration")[0][1]).toMatchObject({ name: "Default" });
  });

  it("offers to launch the configuration's AVD when it is not running", async () => {
    setAvds([makeAvd({ name: "Pixel_7", displayName: "Pixel 7" })]);
    backend(
      makeProjectRunConfigurations([makeRunConfiguration()], {
        Default: { kind: "avd", name: "Pixel_7" },
      }),
      {
        resolve_run_configuration: () => {
          throw {
            kind: "notFound",
            message:
              "Run configuration 'Default' runs on the AVD Pixel_7, which is not running. Launch it, then run again.",
          };
        },
        launch_avd: () => "emulator-5554",
        refresh_devices: () => [],
      }
    );
    open();

    const alert = await within(dialog()).findByText(
      /runs on the AVD Pixel_7, which is not running/
    );
    expect(alert.textContent).not.toContain("notFound");
    expect(field("Target device").value).toBe("avd:Pixel_7");

    fireEvent.click(button("Launch AVD"));

    await vi.waitFor(() => expect(callsTo("launch_avd")).toHaveLength(1));
    expect(callsTo("launch_avd")[0][1]).toEqual({ avdName: "Pixel_7" });
  });

  it("does not offer to launch an AVD for other plan problems", async () => {
    backend(makeProjectRunConfigurations(), {
      resolve_run_configuration: () => {
        throw { kind: "permissionDenied", message: "Trust the project to run it." };
      },
    });
    open();

    await within(dialog()).findByText("Trust the project to run it.");
    expect(within(dialog()).queryByRole("button", { name: "Launch AVD" })).toBeNull();
  });

  it("validates fields before saving, and shows a bad logcat filter as it is typed", async () => {
    backend(makeProjectRunConfigurations());
    open();

    fireEvent.input(field("Logcat filter"), { target: { value: 'message:"unclosed' } });
    expect(
      within(dialog())
        .getAllByRole("alert")
        .map((a) => a.textContent)
    ).toEqual([expect.stringMatching(/quote/i)]);

    fireEvent.input(field("Name"), { target: { value: " " } });
    fireEvent.click(button("Save"));

    expect(await within(dialog()).findByText("Name the configuration.")).toBeTruthy();
    expect(field("Name").getAttribute("aria-invalid")).toBe("true");
    expect(callsTo("save_run_configuration")).toHaveLength(0);
  });

  it("shows why the backend refused a save, next to the form", async () => {
    backend(makeProjectRunConfigurations(), {
      save_run_configuration: () => {
        throw {
          kind: "invalidInput",
          message: "The task ':wear:assembleDebug' is not a task of :app.",
        };
      },
    });
    open();

    fireEvent.input(field("Gradle task"), { target: { value: ":wear:assembleDebug" } });
    fireEvent.click(button("Save"));

    expect(await within(dialog()).findByText(/is not a task of :app/)).toBeTruthy();
    expect(runConfigState.editorOpen).toBe(true);
  });

  it("adds a configuration with its target, and selects it once saved", async () => {
    const server = backend(makeProjectRunConfigurations());
    open();

    fireEvent.click(button("Add"));
    await vi.waitFor(() => expect(field("Name").value).toBe("Configuration"));
    fireEvent.input(field("Name"), { target: { value: "Release" } });
    fireEvent.change(field("Variant"), { target: { value: "release" } });
    fireEvent.change(field("Target device"), { target: { value: "ask" } });
    expect(field("Gradle task").value).toBe(":app:assembleRelease");
    fireEvent.click(button("Save"));

    await vi.waitFor(() => expect(callsTo("set_run_configuration_target")).toHaveLength(1));
    expect(callsTo("save_run_configuration")[0][1]).toEqual({
      config: {
        name: "Release",
        module: ":app",
        variant: "release",
        task: null,
        launch: { kind: "default" },
        logcatFilter: "package:mine",
      },
    });
    expect(callsTo("set_run_configuration_target")[0][1]).toEqual({
      name: "Release",
      target: { kind: "ask" },
    });
    expect(server.project().configurations.map((c) => c.name)).toEqual(["Default", "Release"]);
    await vi.waitFor(() =>
      expect(
        within(dialog()).getByRole("option", { name: "Release" }).getAttribute("aria-selected")
      ).toBe("true")
    );
  });

  it("duplicates the selected configuration under a new name", async () => {
    backend(
      makeProjectRunConfigurations([makeRunConfiguration({ variant: "release" })], {
        Default: { kind: "serial", serial: "R58M" },
      })
    );
    open();

    fireEvent.click(button("Duplicate"));
    await vi.waitFor(() => expect(field("Name").value).toBe("Default copy"));
    fireEvent.click(button("Save"));

    await vi.waitFor(() => expect(callsTo("set_run_configuration_target")).toHaveLength(1));
    expect(callsTo("save_run_configuration")[0][1]).toMatchObject({
      config: { name: "Default copy", variant: "release" },
    });
    expect(callsTo("set_run_configuration_target")[0][1]).toEqual({
      name: "Default copy",
      target: { kind: "serial", serial: "R58M" },
    });
  });

  it("deletes a configuration after confirming", async () => {
    const server = backend(
      makeProjectRunConfigurations([makeRunConfiguration(), makeRunConfiguration({ name: "Wear" })])
    );
    open();

    fireEvent.click(button("Delete"));
    const confirm = await screen.findByRole("dialog", { name: "Delete run configuration?" });
    fireEvent.click(within(confirm).getByRole("button", { name: "Delete" }));

    await vi.waitFor(() => expect(callsTo("delete_run_configuration")).toHaveLength(1));
    expect(callsTo("delete_run_configuration")[0][1]).toEqual({ name: "Default" });
    expect(server.project().configurations.map((c) => c.name)).toEqual(["Wear"]);
    await vi.waitFor(() => expect(field("Name").value).toBe("Wear"));
  });

  it("keeps a configuration when deleting is cancelled", async () => {
    backend(makeProjectRunConfigurations());
    open();

    fireEvent.click(button("Delete"));
    const confirm = await screen.findByRole("dialog", { name: "Delete run configuration?" });
    fireEvent.click(within(confirm).getByRole("button", { name: "Cancel" }));

    await vi.waitFor(() =>
      expect(screen.queryByRole("dialog", { name: "Delete run configuration?" })).toBeNull()
    );
    expect(callsTo("delete_run_configuration")).toHaveLength(0);
  });

  it("asks before discarding unsaved changes", async () => {
    backend(makeProjectRunConfigurations());
    open();

    fireEvent.input(field("Name"), { target: { value: "Renamed" } });
    fireEvent.click(button("Close"));
    const confirm = await screen.findByRole("dialog", { name: "Discard changes?" });
    fireEvent.click(within(confirm).getByRole("button", { name: "Keep Editing" }));

    await vi.waitFor(() =>
      expect(screen.queryByRole("dialog", { name: "Discard changes?" })).toBeNull()
    );
    expect(runConfigState.editorOpen).toBe(true);
    expect(field("Name").value).toBe("Renamed");
  });
});
