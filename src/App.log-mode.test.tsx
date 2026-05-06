import { render, screen, waitFor } from "@solidjs/testing-library";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { App } from "./App";
import { clearActions, getAction } from "@/lib/action-registry";
import { resetUIStateForTests, uiState } from "@/stores/ui.store";

vi.mock("@/components/layout/TitleBar", () => ({
  default: () => <div data-testid="title-bar" />,
}));

vi.mock("@/components/layout/StatusBar", () => ({
  default: () => <div data-testid="status-bar" />,
}));

vi.mock("@/components/projects/ProjectSidebar", () => ({
  ProjectSidebar: () => <aside data-testid="project-sidebar" />,
}));

vi.mock("@/components/device/DeviceSidebar", () => ({
  DeviceSidebar: () => <aside data-testid="device-sidebar" />,
}));

vi.mock("@/components/logcat/LogcatPanel", () => ({
  LogcatPanel: () => <section data-testid="logcat-panel" />,
}));

vi.mock("@/components/build/BuildPanel", () => ({
  BuildPanel: () => <section data-testid="build-panel" />,
}));

vi.mock("@/components/ui-hierarchy/LayoutViewerPanel", () => ({
  LayoutViewerPanel: () => <section data-testid="layout-panel" />,
}));

vi.mock("@/components/common/ErrorBoundary", () => ({
  AppErrorBoundary: (props: { children: unknown }) => <>{props.children}</>,
}));

vi.mock("@/components/ui", () => ({
  ToastContainer: () => <div data-testid="toast-container" />,
  DialogHost: () => <div data-testid="dialog-host" />,
  CommandPalette: () => <div data-testid="command-palette" />,
  openPalette: vi.fn(),
  showDialog: vi.fn().mockResolvedValue("dismissed"),
  showToast: vi.fn(),
}));

vi.mock("@/components/settings/SettingsPanel", () => ({
  SettingsPanel: () => <div data-testid="settings-panel" />,
  openSettings: vi.fn(),
}));

vi.mock("@/components/health/HealthPanel", () => ({
  HealthPanel: () => <div data-testid="health-panel" />,
  openHealthPanel: vi.fn(),
}));

vi.mock("@/components/mcp/McpPanel", () => ({
  McpPanel: () => <div data-testid="mcp-panel" />,
  openMcpPanel: vi.fn(),
}));

vi.mock("@/components/projects/ProjectInfoEditor", () => ({
  ProjectInfoEditor: () => <div data-testid="project-info-editor" />,
  openProjectInfoEditor: vi.fn(),
}));

vi.mock("@/components/device/DevicePickerDialog", () => ({
  DevicePickerDialog: () => <div data-testid="device-picker-dialog" />,
}));

vi.mock("@/components/onboarding/OnboardingWizard", () => ({
  OnboardingWizard: () => <div data-testid="onboarding-wizard" />,
}));

vi.mock("@/components/build/VariantSelector", () => ({
  openVariantPicker: vi.fn(),
}));

vi.mock("@/lib/keybindings", () => ({
  initKeybindings: vi.fn(),
  registerKeybinding: vi.fn(),
}));

vi.mock("@/stores/settings.store", () => ({
  applyAppearanceSettings: vi.fn(),
  loadSettings: vi.fn().mockResolvedValue(undefined),
  settingsState: { telemetry: { enabled: false } },
}));

vi.mock("@/lib/telemetry/sentry-web", () => ({
  captureSentryException: vi.fn(),
  initSentryWeb: vi.fn(),
}));

vi.mock("@/stores/onboarding.store", () => ({
  tryOpenOnboardingAfterLoad: vi.fn(),
  openOnboardingWizard: vi.fn(),
}));

vi.mock("@/services/project.service", () => ({
  openProjectFolder: vi.fn().mockResolvedValue(undefined),
  refreshProjectsList: vi.fn().mockResolvedValue(undefined),
  restoreLastProject: vi.fn().mockResolvedValue(false),
}));

vi.mock("@/services/build.service", () => ({
  initBuildService: vi.fn().mockResolvedValue(undefined),
  runBuild: vi.fn().mockResolvedValue(undefined),
  runAndDeploy: vi.fn().mockResolvedValue(undefined),
  cancelBuild: vi.fn().mockResolvedValue(undefined),
}));

vi.mock("@/stores/device.store", () => ({
  initDevices: vi.fn().mockResolvedValue(undefined),
}));

vi.mock("@/lib/tauri-api", () => ({
  formatError: (err: unknown) => (err instanceof Error ? err.message : String(err)),
  sendNativeSentryTestEvent: vi.fn().mockResolvedValue(undefined),
}));

vi.mock("@/services/update.service", () => ({
  dismissUpdate: vi.fn(),
  openUpdateRelease: vi.fn().mockResolvedValue(undefined),
  refreshAppUpdate: vi.fn().mockResolvedValue({ available: false }),
  shouldDismissUpdatePrompt: vi.fn().mockReturnValue(false),
}));

vi.mock("@/stores/mcp.store", () => ({
  initMcpListeners: vi.fn(),
  loadMcpActivity: vi.fn().mockResolvedValue(undefined),
}));

describe("App Log Mode shell", () => {
  beforeEach(() => {
    clearActions();
    resetUIStateForTests();
  });

  it("registers a command action that toggles Log Mode", async () => {
    render(() => <App />);

    await waitFor(() => expect(getAction("view.toggleLogMode")).toBeDefined());

    getAction("view.toggleLogMode")!.action();

    expect(uiState.logMode.active).toBe(true);
  });

  it("hides navigation chrome while keeping Logcat and status visible in Log Mode", async () => {
    render(() => <App />);

    expect(screen.getByTestId("project-sidebar")).not.toBeNull();
    expect(screen.getByTestId("device-sidebar")).not.toBeNull();
    expect(screen.getByRole("tablist", { name: "Main panels" })).not.toBeNull();

    await waitFor(() => expect(getAction("view.toggleLogMode")).toBeDefined());
    getAction("view.toggleLogMode")!.action();

    await waitFor(() => {
      expect(screen.queryByTestId("project-sidebar")).toBeNull();
      expect(screen.queryByTestId("device-sidebar")).toBeNull();
      expect(screen.queryByRole("tablist", { name: "Main panels" })).toBeNull();
    });

    expect(screen.getByTestId("title-bar")).not.toBeNull();
    expect(screen.getByTestId("status-bar")).not.toBeNull();
    expect(screen.getByTestId("logcat-panel")).not.toBeNull();
  });
});
