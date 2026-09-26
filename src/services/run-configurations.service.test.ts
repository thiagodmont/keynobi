import { describe, it, expect, beforeEach, afterEach, vi } from "vitest";
import { invoke } from "@tauri-apps/api/core";
import {
  chooseRunConfiguration,
  copyName,
  deleteRunConfiguration,
  loadRunConfigurations,
  registeredRunConfigurationActionIds,
  saveRunConfiguration,
  setRunConfigurationRunner,
} from "./run-configurations.service";
import { runConfigState, resetRunConfigurationsForTests } from "@/stores/run-configurations.store";
import { clearProject, setProject } from "@/stores/project.store";
import { executeAction, getAction } from "@/lib/action-registry";
import { makeProjectRunConfigurations, makeRunConfiguration } from "@/test/factories/build";
import type { ProjectRunConfigurations } from "@/bindings";

const variantMock = vi.hoisted(() => ({
  variantState: { module: null as string | null, activeVariant: "debug" as string | null },
  loadVariants: vi.fn(() => Promise.resolve()),
  selectVariant: vi.fn(() => Promise.resolve()),
}));

vi.mock("@/stores/variant.store", () => variantMock);

const mockInvoke = vi.mocked(invoke);

const mobile = makeRunConfiguration({ name: "Mobile", module: ":mobile" });
const wear = makeRunConfiguration({ name: "Wear", module: ":wear", variant: "release" });

function answer(responses: Record<string, (args: Record<string, unknown>) => unknown>) {
  mockInvoke.mockImplementation((cmd, args) => {
    const respond = responses[cmd];
    if (!respond) return Promise.reject(new Error(`unexpected ${cmd}`));
    return Promise.resolve(respond((args ?? {}) as Record<string, unknown>));
  });
}

function callsTo(command: string) {
  return mockInvoke.mock.calls.filter(([cmd]) => cmd === command);
}

describe("run configurations service", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    resetRunConfigurationsForTests();
    variantMock.variantState.module = null;
    variantMock.variantState.activeVariant = "debug";
    setProject("/projects/app", "app");
  });

  afterEach(async () => {
    clearProject();
    await loadRunConfigurations();
  });

  it("loads the open project's configurations and follows the active one's module and variant", async () => {
    answer({ list_run_configurations: () => makeProjectRunConfigurations([wear, mobile]) });

    await loadRunConfigurations();

    expect(runConfigState.projectRoot).toBe("/projects/app");
    expect(runConfigState.configurations.map((c) => c.name)).toEqual(["Wear", "Mobile"]);
    expect(runConfigState.active).toBe("Wear");
    expect(variantMock.loadVariants).toHaveBeenCalledWith({ module: ":wear" });
    expect(variantMock.selectVariant).toHaveBeenCalledWith("release");
  });

  it("keeps the module implicit when the project has one application module", async () => {
    answer({ list_run_configurations: () => makeProjectRunConfigurations([mobile]) });

    await loadRunConfigurations();

    expect(variantMock.loadVariants).not.toHaveBeenCalled();
    expect(variantMock.selectVariant).not.toHaveBeenCalled();
  });

  it("ignores a load a newer project open replaced", async () => {
    let finish!: (value: ProjectRunConfigurations) => void;
    mockInvoke.mockImplementation(() => new Promise((resolve) => (finish = resolve)));

    const load = loadRunConfigurations();
    setProject("/projects/other", "other");
    finish(makeProjectRunConfigurations([mobile]));
    await load;

    expect(runConfigState.configurations).toEqual([]);
  });

  it("chooses the active configuration", async () => {
    answer({
      list_run_configurations: () => makeProjectRunConfigurations([mobile, wear]),
      set_active_run_configuration: () => ({
        ...makeProjectRunConfigurations([mobile, wear]),
        active: "Wear",
      }),
    });
    await loadRunConfigurations();

    await chooseRunConfiguration("Wear");

    expect(callsTo("set_active_run_configuration")[0][1]).toEqual({ name: "Wear" });
    expect(runConfigState.active).toBe("Wear");
    expect(variantMock.selectVariant).toHaveBeenLastCalledWith("release");
  });

  it("saves a configuration with its target", async () => {
    const added = makeRunConfiguration({ name: "Tablet", variant: "release" });
    answer({
      save_run_configuration: () => makeProjectRunConfigurations([mobile, added]),
      set_run_configuration_target: () =>
        makeProjectRunConfigurations([mobile, added], { Tablet: { kind: "ask" } }),
    });

    await saveRunConfiguration(added, { kind: "ask" }, null);

    expect(callsTo("save_run_configuration")[0][1]).toEqual({ config: added });
    expect(callsTo("set_run_configuration_target")[0][1]).toEqual({
      name: "Tablet",
      target: { kind: "ask" },
    });
    expect(callsTo("delete_run_configuration")).toHaveLength(0);
    expect(runConfigState.local.Tablet.target).toEqual({ kind: "ask" });
  });

  it("renames by saving the new name, deleting the old one, and keeping it active", async () => {
    const renamed = makeRunConfiguration({ name: "Phone", module: ":mobile" });
    const afterSave = makeProjectRunConfigurations([mobile, renamed]);
    answer({
      list_run_configurations: () => makeProjectRunConfigurations([mobile]),
      save_run_configuration: () => afterSave,
      set_run_configuration_target: () => afterSave,
      delete_run_configuration: () => ({
        ...makeProjectRunConfigurations([renamed]),
        active: null,
      }),
      set_active_run_configuration: () => makeProjectRunConfigurations([renamed]),
    });
    await loadRunConfigurations();

    await saveRunConfiguration(renamed, { kind: "lastUsed" }, "Mobile");

    expect(callsTo("delete_run_configuration")[0][1]).toEqual({ name: "Mobile" });
    expect(callsTo("set_active_run_configuration")[0][1]).toEqual({ name: "Phone" });
    expect(runConfigState.configurations.map((c) => c.name)).toEqual(["Phone"]);
    expect(runConfigState.active).toBe("Phone");
  });

  it("refuses a rename onto another configuration's name, saving nothing", async () => {
    answer({ list_run_configurations: () => makeProjectRunConfigurations([mobile, wear]) });
    await loadRunConfigurations();

    await expect(
      saveRunConfiguration({ ...mobile, name: "Wear" }, { kind: "lastUsed" }, "Mobile")
    ).rejects.toThrow("A run configuration named 'Wear' already exists.");
    expect(callsTo("save_run_configuration")).toHaveLength(0);
  });

  it("deletes a configuration", async () => {
    answer({
      list_run_configurations: () => makeProjectRunConfigurations([mobile, wear]),
      delete_run_configuration: () => makeProjectRunConfigurations([mobile]),
    });
    await loadRunConfigurations();

    await deleteRunConfiguration("Wear");

    expect(callsTo("delete_run_configuration")[0][1]).toEqual({ name: "Wear" });
    expect(runConfigState.configurations.map((c) => c.name)).toEqual(["Mobile"]);
  });

  it("names a copy after the original, skipping taken names", () => {
    expect(copyName("Default", ["Default"])).toBe("Default copy");
    expect(copyName("Default", ["Default", "default COPY", "Default copy 2"])).toBe(
      "Default copy 3"
    );
  });

  describe("palette actions", () => {
    it("registers Run and Build actions per configuration, replacing them on a project switch", async () => {
      answer({ list_run_configurations: () => makeProjectRunConfigurations([mobile, wear]) });
      await loadRunConfigurations();

      expect(getAction("runConfiguration.run:Mobile")?.label).toBe("Run: Mobile");
      expect(getAction("runConfiguration.build:Wear")?.label).toBe("Build: Wear");
      expect(registeredRunConfigurationActionIds()).toHaveLength(4);

      setProject("/projects/other", "other");
      const other = makeRunConfiguration({ name: "Other" });
      answer({ list_run_configurations: () => makeProjectRunConfigurations([other]) });
      await loadRunConfigurations();

      expect(getAction("runConfiguration.run:Mobile")).toBeUndefined();
      expect(getAction("runConfiguration.build:Wear")).toBeUndefined();
      expect(getAction("runConfiguration.run:Other")?.label).toBe("Run: Other");
      expect(registeredRunConfigurationActionIds()).toEqual([
        "runConfiguration.run:Other",
        "runConfiguration.build:Other",
      ]);

      clearProject();
      await loadRunConfigurations();
      expect(getAction("runConfiguration.run:Other")).toBeUndefined();
      expect(registeredRunConfigurationActionIds()).toEqual([]);
    });

    it("drops the previous project's actions even when the new project's load fails", async () => {
      answer({ list_run_configurations: () => makeProjectRunConfigurations([mobile]) });
      await loadRunConfigurations();

      setProject("/projects/other", "other");
      mockInvoke.mockImplementation(() => Promise.reject({ kind: "notFound", message: "gone" }));
      await expect(loadRunConfigurations()).rejects.toMatchObject({ kind: "notFound" });

      expect(getAction("runConfiguration.run:Mobile")).toBeUndefined();
      expect(runConfigState.configurations).toEqual([]);
    });

    it("runs and builds the configuration it names", async () => {
      const run = vi.fn(() => Promise.resolve());
      const build = vi.fn(() => Promise.resolve());
      setRunConfigurationRunner({ run, build });
      answer({ list_run_configurations: () => makeProjectRunConfigurations([mobile, wear]) });
      await loadRunConfigurations();

      executeAction("runConfiguration.run:Wear");
      executeAction("runConfiguration.build:Mobile");

      await vi.waitFor(() => expect(run).toHaveBeenCalledWith("Wear"));
      expect(build).toHaveBeenCalledWith("Mobile");
    });
  });
});
