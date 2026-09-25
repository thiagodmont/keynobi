import { beforeEach, describe, expect, it, vi } from "vitest";
import { invoke } from "@tauri-apps/api/core";
import { open } from "@tauri-apps/plugin-dialog";
import type { ProjectEntry, VariantList } from "@/bindings";

const mockDialog = vi.fn();
vi.mock("@/components/ui", async (importOriginal) => ({
  ...(await importOriginal<Record<string, unknown>>()),
  showDialog: (...args: unknown[]) => mockDialog(...args),
}));

import {
  openProjectFolder,
  revokeProjectTrust,
  selectProject,
  trustProject,
} from "@/services/project.service";
import { setProjectState } from "@/stores/project.store";
import {
  isActiveProjectTrusted,
  projectsState,
  setActiveProjectId,
  setProjects,
} from "@/stores/projects.store";
import { resetBuildState } from "@/stores/build.store";
import { resetDeviceState } from "@/stores/device.store";
import { clearVariantCache, resetVariantState } from "@/stores/variant.store";

const mockInvoke = vi.mocked(invoke);
const mockOpen = vi.mocked(open);

const variants: VariantList = {
  variants: [
    {
      name: "debug",
      buildType: "debug",
      flavors: [],
      assembleTask: "assembleDebug",
      installTask: "installDebug",
    },
  ],
  active: null,
  defaultVariant: null,
};

function entryFor(path: string, trusted: boolean | null): ProjectEntry {
  return {
    id: `${path}-id`,
    path,
    name: path.split("/").pop() ?? path,
    gradleRoot: path,
    lastOpened: "2026-01-01T00:00:00Z",
    pinned: false,
    lastBuildVariant: null,
    lastDevice: null,
    trusted,
  };
}

/** A backend whose registry holds `entries`; `open_project` switches its root. */
function mockBackend(entries: ProjectEntry[]): void {
  let root = "";
  mockInvoke.mockImplementation((command, args) => {
    switch (command) {
      case "open_project":
        root = (args as { path: string }).path;
        return Promise.resolve(root);
      case "get_project_root":
      case "get_gradle_root":
        return Promise.resolve(root);
      case "list_projects":
        return Promise.resolve(entries.map((e) => ({ ...e })));
      case "get_variants_preview":
      case "get_variants_from_gradle":
        return Promise.resolve(variants);
      case "get_build_history":
      case "refresh_devices":
      case "list_avd_devices":
        return Promise.resolve([]);
      default:
        return Promise.resolve(null);
    }
  });
}

function calls(command: string): unknown[] {
  return mockInvoke.mock.calls.filter(([cmd]) => cmd === command).map(([, args]) => args);
}

describe("project trust", () => {
  beforeEach(() => {
    setProjectState({
      projectRoot: null,
      gradleRoot: null,
      projectName: null,
      applicationId: null,
      loading: false,
    });
    setProjects([]);
    setActiveProjectId(null);
    resetBuildState();
    resetDeviceState();
    resetVariantState();
    clearVariantCache();
    vi.clearAllMocks();
  });

  it("asks on first open and persists Trust before running Gradle", async () => {
    const entry = entryFor("/projects/fresh", null);
    mockBackend([entry]);
    mockOpen.mockResolvedValue(entry.path);
    mockDialog.mockResolvedValue("trust");

    await openProjectFolder();

    expect(mockDialog).toHaveBeenCalledTimes(1);
    expect(mockDialog.mock.calls[0][0].message).toContain(
      "This project will run its Gradle build scripts. Trust it?"
    );
    expect(calls("set_project_trust")).toEqual([{ id: entry.id, trusted: true }]);
    expect(isActiveProjectTrusted()).toBe(true);
    expect(calls("get_variants_from_gradle")).toHaveLength(1);
  });

  it("lists the safe choice last so it has focus", async () => {
    const entry = entryFor("/projects/fresh", null);
    mockBackend([entry]);
    mockOpen.mockResolvedValue(entry.path);
    mockDialog.mockResolvedValue("safe-mode");

    await openProjectFolder();

    const buttons = mockDialog.mock.calls[0][0].buttons as { label: string }[];
    expect(buttons.map((b) => b.label)).toEqual(["Trust", "Open in Safe Mode"]);
  });

  it("opens in Safe Mode without running Gradle", async () => {
    const entry = entryFor("/projects/fresh", null);
    mockBackend([entry]);
    mockOpen.mockResolvedValue(entry.path);
    mockDialog.mockResolvedValue("safe-mode");

    await openProjectFolder();

    expect(calls("set_project_trust")).toEqual([{ id: entry.id, trusted: false }]);
    expect(isActiveProjectTrusted()).toBe(false);
    expect(calls("get_variants_preview")).toHaveLength(1);
    expect(calls("get_variants_from_gradle")).toHaveLength(0);
  });

  it("keeps Safe Mode without saving when the question is dismissed", async () => {
    const entry = entryFor("/projects/fresh", null);
    mockBackend([entry]);
    mockOpen.mockResolvedValue(entry.path);
    mockDialog.mockResolvedValue("cancel");

    await openProjectFolder();

    expect(calls("set_project_trust")).toHaveLength(0);
    expect(calls("get_variants_from_gradle")).toHaveLength(0);
  });

  it("does not ask about a trusted or grandfathered project", async () => {
    const entry = entryFor("/projects/known", true);
    mockBackend([entry]);

    await selectProject(entry);

    expect(mockDialog).not.toHaveBeenCalled();
    expect(calls("get_variants_from_gradle")).toHaveLength(1);
  });

  it("does not ask again about a project already in Safe Mode", async () => {
    const entry = entryFor("/projects/declined", false);
    mockBackend([entry]);

    await selectProject(entry);

    expect(mockDialog).not.toHaveBeenCalled();
    expect(calls("get_variants_from_gradle")).toHaveLength(0);
  });

  it("does not ask about a project whose open was superseded", async () => {
    const stale = entryFor("/projects/stale", null);
    const current = entryFor("/projects/current", true);
    mockBackend([stale, current]);
    let releaseStale: () => void = () => {};
    const inner = mockInvoke.getMockImplementation();
    let firstList = true;
    mockInvoke.mockImplementation((command, args) => {
      if (command === "list_projects" && firstList) {
        firstList = false;
        return new Promise((resolve) => {
          releaseStale = () => resolve([{ ...stale }, { ...current }]);
        });
      }
      return inner!(command, args);
    });

    const staleSelect = selectProject(stale);
    await vi.waitFor(() => expect(firstList).toBe(false));
    await selectProject(current);
    releaseStale();
    await staleSelect;

    expect(mockDialog).not.toHaveBeenCalled();
    expect(projectsState.activeProjectId).toBe(current.id);
  });

  it("reloads variants with Gradle after trusting the open project", async () => {
    const entry = entryFor("/projects/declined", false);
    mockBackend([entry]);
    await selectProject(entry);
    expect(calls("get_variants_from_gradle")).toHaveLength(0);

    await trustProject(projectsState.projects[0]);

    expect(calls("set_project_trust")).toEqual([{ id: entry.id, trusted: true }]);
    expect(calls("get_variants_from_gradle")).toHaveLength(1);
  });

  it("records a revoke and disables builds for the open project", async () => {
    const entry = entryFor("/projects/known", true);
    mockBackend([entry]);
    await selectProject(entry);
    expect(isActiveProjectTrusted()).toBe(true);

    await revokeProjectTrust(projectsState.projects[0]);

    expect(calls("set_project_trust")).toEqual([{ id: entry.id, trusted: false }]);
    expect(isActiveProjectTrusted()).toBe(false);
  });
});
