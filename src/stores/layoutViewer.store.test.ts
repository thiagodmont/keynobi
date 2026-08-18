import { beforeEach, describe, expect, it, vi } from "vitest";
import { invoke } from "@tauri-apps/api/core";
import type { Device, UiHierarchySnapshot, UiNode } from "@/bindings";
import {
  refreshLayoutHierarchy,
  layoutViewerState,
  setLayoutViewerState,
} from "./layoutViewer.store";
import { pickDevice, resetDeviceState, setDevices } from "./device.store";

const mockInvoke = vi.mocked(invoke);

function makeNode(label: string): UiNode {
  return {
    class: "android.widget.TextView",
    resourceId: "",
    text: label,
    contentDesc: "",
    package: "com.example",
    bounds: "[0,0][10,10]",
    clickable: false,
    enabled: true,
    focusable: false,
    focused: false,
    scrollable: false,
    longClickable: false,
    password: false,
    checkable: false,
    checked: false,
    editable: false,
    selected: false,
    isComposeHeuristic: false,
    children: [],
  };
}

function makeSnapshot(screenHash: string): UiHierarchySnapshot {
  return {
    capturedAt: "2026-01-01T00:00:00Z",
    truncated: false,
    warnings: [],
    root: makeNode(screenHash),
    screenHash,
    interactiveCount: 1,
    foregroundActivity: null,
    layoutContext: {
      windowExcerpt: null,
      displayExcerpt: null,
      wmSize: null,
      wmDensity: null,
    },
    commandLog: [],
    screenshotB64: null,
  };
}

function makeDevice(serial: string): Device {
  return {
    serial,
    name: serial,
    model: serial,
    deviceKind: "physical",
    connectionState: "online",
    apiLevel: 35,
    androidVersion: "15",
  };
}

function deferred<T>(): {
  promise: Promise<T>;
  resolve: (value: T) => void;
  reject: (reason?: unknown) => void;
} {
  let resolve: (value: T) => void = () => {};
  let reject: (reason?: unknown) => void = () => {};
  const promise = new Promise<T>((res, rej) => {
    resolve = res;
    reject = rej;
  });
  return { promise, resolve, reject };
}

describe("layoutViewer.store", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    resetDeviceState();
    setLayoutViewerState({
      snapshot: null,
      loading: false,
      error: null,
      interactiveOnly: false,
      hideBoilerplate: false,
      searchQuery: "",
      selectedLayoutPath: null,
      searchMatchIndex: 0,
      searchMatchPaths: [],
      autoRefreshIntervalMs: null,
    });
  });

  it("ignores a stale hierarchy response after the selected device changes", async () => {
    const first = deferred<UiHierarchySnapshot>();
    const second = deferred<UiHierarchySnapshot>();
    const responses = [first.promise, second.promise];

    mockInvoke.mockImplementation((command) => {
      if (command === "select_device") return Promise.resolve(undefined);
      if (command === "dump_ui_hierarchy") {
        const next = responses.shift();
        return next ?? Promise.reject(new Error("unexpected hierarchy request"));
      }
      return Promise.resolve(undefined);
    });

    setDevices([makeDevice("device-1"), makeDevice("device-2")]);
    const firstRefresh = refreshLayoutHierarchy();
    await pickDevice("device-2");
    const secondRefresh = refreshLayoutHierarchy();

    second.resolve(makeSnapshot("second"));
    await secondRefresh;
    expect(layoutViewerState.snapshot?.screenHash).toBe("second");

    first.resolve(makeSnapshot("first"));
    await firstRefresh;
    expect(layoutViewerState.snapshot?.screenHash).toBe("second");
  });
});
