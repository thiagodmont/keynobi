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
    list_run_configurations: () => project,
    save_run_configuration: ({ config, shared }) => {
      const saved = config as RunConfiguration;
      const rest = project.configurations.filter((c) => c.name !== saved.name);
      const others = project.shared.filter((n) => n !== saved.name);
      const toShared = (shared as boolean | null) ?? project.shared.includes(saved.name);
      project = {
        ...makeProjectRunConfigurations(
          [...rest, saved],
          targets(),
          toShared ? [...others, saved.name] : others
        ),
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
      shared: false,
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

  it("shares a configuration with the project", async () => {
    const { project } = backend(makeProjectRunConfigurations());
    open();

    const share = within(dialog()).getByRole("checkbox", { name: "Share with project" });
    expect((share as HTMLInputElement).checked).toBe(false);
    fireEvent.click(share);
    fireEvent.click(button("Save"));

    await vi.waitFor(() => expect(callsTo("save_run_configuration")).toHaveLength(1));
    expect(callsTo("save_run_configuration")[0][1]).toMatchObject({
      config: { name: "Default" },
      shared: true,
    });
    expect(project().shared).toEqual(["Default"]);
    const list = within(dialog()).getByRole("listbox", { name: "Run configurations" });
    await vi.waitFor(() =>
      expect(within(list).getByRole("option").textContent).toContain("Shared")
    );
  });

  it("marks a shared configuration and moves it back to this Mac when unshared", async () => {
    const { project } = backend(
      makeProjectRunConfigurations(
        [makeRunConfiguration(), makeRunConfiguration({ name: "Team" })],
        {},
        ["Team"]
      )
    );
    open();

    const list = within(dialog()).getByRole("listbox", { name: "Run configurations" });
    const options = within(list).getAllByRole("option");
    expect(options[0].textContent).not.toContain("Shared");
    expect(options[1].textContent).toContain("Shared");
    fireEvent.click(options[1]);
    await vi.waitFor(() => expect(field("Name").value).toBe("Team"));
    const share = within(dialog()).getByRole("checkbox", {
      name: "Share with project",
    }) as HTMLInputElement;
    expect(share.checked).toBe(true);

    fireEvent.click(share);
    fireEvent.click(button("Save"));

    await vi.waitFor(() => expect(callsTo("save_run_configuration")).toHaveLength(1));
    expect(callsTo("save_run_configuration")[0][1]).toMatchObject({
      config: { name: "Team" },
      shared: false,
    });
    expect(project().shared).toEqual([]);
  });

  it("shows why the project's shared file cannot be used", () => {
    backend({
      ...makeProjectRunConfigurations(),
      sharedFile: {
        path: ".keynobi/run-configurations.json",
        sha256: null,
        error: "It is not valid JSON: expected value at line 1 column 1.",
        problems: [],
      },
    });
    open();

    const alert = within(dialog()).getByTestId("shared-run-config-problems");
    expect(alert.textContent).toContain("It is not valid JSON");
    expect(dialog().textContent).toContain(
      "The project's shared run configurations (.keynobi/run-configurations.json) cannot be used"
    );
  });

  it("names the shared configurations it does not offer, and why", () => {
    backend({
      ...makeProjectRunConfigurations([makeRunConfiguration()], {}, []),
      sharedFile: {
        path: ".keynobi/run-configurations.json",
        sha256: "b".repeat(64),
        error: null,
        problems: [
          { name: "Wear", message: "The module :wear is not a module of this project." },
          { name: null, message: "The file has more than 50 configurations." },
        ],
      },
    });
    open();

    const items = within(within(dialog()).getByTestId("shared-run-config-problems")).getAllByRole(
      "listitem"
    );
    expect(items.map((i) => i.textContent)).toEqual([
      "'Wear': The module :wear is not a module of this project.",
      "A configuration: The file has more than 50 configurations.",
    ]);
  });

  it("approves a shared configuration from its plan, for the file it was shown", async () => {
    let approved: string | null = null;
    const initial = makeProjectRunConfigurations(
      [makeRunConfiguration({ name: "Bundle", task: ":app:bundleDebug" })],
      {},
      ["Bundle"]
    );
    backend(initial, {
      resolve_run_configuration: ({ name }) => {
        if (approved === null) {
          throw {
            kind: "approvalRequired",
            message:
              "Run configuration 'Bundle' is shared with the project (.keynobi/run-configurations.json) and builds :app:bundleDebug, which is not an assemble task. You have not approved it yet. Review it, then approve it to run it.",
          };
        }
        return makeResolvedRun({
          name: name as string,
          plan: "Run 'Bundle': build :app:bundleDebug",
        });
      },
      approve_shared_run_configuration: ({ sha256 }) => {
        approved = sha256 as string;
        return initial;
      },
    });
    open();

    await within(dialog()).findByText(/You have not approved it yet/);
    fireEvent.click(button("Approve…"));
    const confirm = await screen.findByRole("dialog", {
      name: "Approve shared run configuration?",
    });
    expect(confirm.textContent).toContain("builds :app:bundleDebug");
    fireEvent.click(within(confirm).getByRole("button", { name: "Approve" }));

    await vi.waitFor(() => expect(callsTo("approve_shared_run_configuration")).toHaveLength(1));
    expect(callsTo("approve_shared_run_configuration")[0][1]).toEqual({
      name: "Bundle",
      sha256: "a".repeat(64),
    });
    const plan = await within(dialog()).findByTestId("run-config-plan");
    expect(plan.textContent).toContain("Run 'Bundle': build :app:bundleDebug");
  });

  it("does not approve when the confirmation is cancelled", async () => {
    backend(
      makeProjectRunConfigurations([makeRunConfiguration({ name: "Bundle" })], {}, ["Bundle"]),
      {
        resolve_run_configuration: () => {
          throw { kind: "approvalRequired", message: "Approve it to run it." };
        },
      }
    );
    open();

    await within(dialog()).findByText("Approve it to run it.");
    fireEvent.click(button("Approve…"));
    const confirm = await screen.findByRole("dialog", {
      name: "Approve shared run configuration?",
    });
    fireEvent.click(within(confirm).getByRole("button", { name: "Cancel" }));

    await vi.waitFor(() =>
      expect(screen.queryByRole("dialog", { name: "Approve shared run configuration?" })).toBeNull()
    );
    expect(callsTo("approve_shared_run_configuration")).toHaveLength(0);
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
