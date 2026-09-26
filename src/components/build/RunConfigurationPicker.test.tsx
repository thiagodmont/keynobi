import { describe, it, expect, beforeEach, afterEach, vi } from "vitest";
import { render, screen, fireEvent, cleanup } from "@solidjs/testing-library";
import { invoke } from "@tauri-apps/api/core";
import {
  EDIT_RUN_CONFIGURATIONS,
  RunConfigurationPicker,
  focusRunConfigurationPicker,
} from "./RunConfigurationPicker";
import {
  resetRunConfigurationsForTests,
  runConfigState,
  setRunConfigurations,
} from "@/stores/run-configurations.store";
import { clearProject, setProject } from "@/stores/project.store";
import { makeProjectRunConfigurations, makeRunConfiguration } from "@/test/factories/build";

vi.mock("@/stores/variant.store", () => ({
  variantState: { module: null, activeVariant: "debug", variants: [] },
  loadVariants: vi.fn(() => Promise.resolve()),
  selectVariant: vi.fn(() => Promise.resolve()),
}));

const mockInvoke = vi.mocked(invoke);
const configurations = [
  makeRunConfiguration({ name: "Default" }),
  makeRunConfiguration({ name: "Release", variant: "release" }),
];

function picker(): HTMLSelectElement {
  return screen.getByRole("combobox", { name: "Run configuration" }) as HTMLSelectElement;
}

describe("RunConfigurationPicker", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    resetRunConfigurationsForTests();
    setProject("/projects/app", "app");
    setRunConfigurations("/projects/app", makeProjectRunConfigurations(configurations));
  });

  afterEach(() => {
    cleanup();
    clearProject();
  });

  it("lists the configurations, shows the active one, and ends with Edit Configurations…", () => {
    render(() => <RunConfigurationPicker />);

    const options = Array.from(picker().options).filter((o) => !o.disabled);
    expect(options.map((o) => o.textContent)).toEqual([
      "Default",
      "Release",
      "Edit Configurations…",
    ]);
    expect(picker().value).toBe("Default");
    expect(picker().title).toContain("Default (:app · debug)");
  });

  it("makes the chosen configuration active", async () => {
    mockInvoke.mockImplementation((cmd) =>
      cmd === "set_active_run_configuration"
        ? Promise.resolve({ ...makeProjectRunConfigurations(configurations), active: "Release" })
        : Promise.reject(new Error(`unexpected ${cmd}`))
    );
    render(() => <RunConfigurationPicker />);

    fireEvent.change(picker(), { target: { value: "Release" } });

    await vi.waitFor(() => expect(runConfigState.active).toBe("Release"));
    expect(mockInvoke).toHaveBeenCalledWith("set_active_run_configuration", { name: "Release" });
    expect(picker().value).toBe("Release");
  });

  it("opens the editor from Edit Configurations… and keeps showing the active one", () => {
    render(() => <RunConfigurationPicker />);

    fireEvent.change(picker(), { target: { value: EDIT_RUN_CONFIGURATIONS } });

    expect(runConfigState.editorOpen).toBe(true);
    expect(picker().value).toBe("Default");
    expect(mockInvoke).not.toHaveBeenCalled();
  });

  it("takes focus for Select Run Configuration…", () => {
    render(() => <RunConfigurationPicker />);

    focusRunConfigurationPicker();

    expect(document.activeElement).toBe(picker());
  });

  it("is disabled while a run is in flight", () => {
    render(() => <RunConfigurationPicker disabled />);
    expect(picker().disabled).toBe(true);
  });
});
