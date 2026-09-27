import type {
  AppError,
  BuildActor,
  DeployPhase,
  DeployPhaseEvent,
  DeployResult,
  ResolvedRun,
  RunApk,
  RunDevice,
} from "@/bindings";
import {
  mockRunApk,
  recordMockInstall,
  startMockAppBuild,
  startMockBuild,
  type MockBuildOutcome,
} from "./build";
import { mockDevice, mockLaunchApp } from "./devices";
import { mockResolveRun, recordMockRunDevice } from "./projects";
import { triggerEvent } from "./events";

/** The package the mock reads from every APK, as aapt2 would. */
const MOCK_APK_PACKAGE = "com.example.mockapp.debug";

/** Why the next install fails; null when installs succeed. */
let installFailure: string | null = null;

/** Make the next install of a run fail with `reason`, as adb would. */
export function failNextMockInstall(reason: string | null): void {
  installFailure = reason;
}

function describeApk(apk: RunApk): string {
  if (apk.fromThisBuild) return `APK (build #${apk.buildId}): ${apk.path}`;
  if (apk.buildId !== null) return `APK unchanged since build #${apk.buildId}: ${apk.path}`;
  return `APK unchanged by this build, and no build in the history wrote it: ${apk.path}`;
}

function deviceLabel(serial: string): string {
  const device = mockDevice(serial);
  if (!device) return serial;
  const api = device.apiLevel !== null ? ` (API ${device.apiLevel})` : "";
  return `${device.model ?? device.name}${api} [${serial}]`;
}

/**
 * Like the backend's `run_run_configuration`: resolve, build, install the
 * APK that build wrote, record the device, and launch, sending `deploy:phase`
 * with each phase's steps.
 */
function mockRunConfiguration(args: unknown): Promise<DeployResult> {
  return mockRun(args, { kind: "app" }, 0);
}

/**
 * An attached agent's `run_run_configuration`: its build streams to the app
 * as the agent's (`lineDelayMs` per output line), and its phases are sent to
 * the app with the agent as their origin.
 */
export function startMockAgentRun(
  name: string,
  serial: string | null,
  clientName: string | null,
  lineDelayMs = 80
): Promise<DeployResult> {
  const agent: BuildActor = { kind: "agent", sessionId: 1, clientName, standalone: false };
  return mockRun({ name, selectedSerial: serial }, agent, lineDelayMs);
}

async function mockRun(
  args: unknown,
  origin: BuildActor,
  lineDelayMs: number
): Promise<DeployResult> {
  const { name, selectedSerial } = (args ?? {}) as {
    name?: string | null;
    selectedSerial?: string | null;
  };
  const run: ResolvedRun = mockResolveRun({ name, selectedSerial, buildOnly: false });
  const device = run.device as RunDevice;
  let steps: string[] = [];
  let buildId: number | null = null;
  const phase = (to: DeployPhase, error: string | null = null) => {
    const event: DeployPhaseEvent = {
      phase: to,
      origin,
      name: run.name,
      plan: run.plan,
      device,
      buildId,
      steps,
      error,
    };
    steps = [];
    triggerEvent("deploy:phase", event);
  };
  const result: DeployResult = {
    run,
    outcome: "done",
    buildId: null,
    device,
    apk: null,
    apkSha256: null,
    package: null,
    launch: null,
    logcatFilter: run.logcatFilter,
  };

  phase("building");
  const built = await new Promise<MockBuildOutcome>((resolve) => {
    if (origin.kind === "app") startMockAppBuild(run.task, resolve);
    else startMockBuild(run.task, origin, lineDelayMs, resolve);
  });
  buildId = built.recordId;
  result.buildId = built.recordId;
  if (built.cancelled || !built.success) {
    result.outcome = built.cancelled ? "cancelled" : "buildFailed";
    phase(
      built.cancelled ? "cancelled" : "failed",
      built.cancelled ? null : "The build failed; nothing was installed."
    );
    return result;
  }

  const apk = mockRunApk(run.variant, built.recordId);
  steps.push(describeApk(apk), `Installing on: ${deviceLabel(device.serial)}`);
  steps.push(`adb install ${apk.path}`);
  result.apk = apk;
  phase("installing");
  if (installFailure) {
    const error: AppError = { kind: "processFailed", message: installFailure };
    installFailure = null;
    phase("failed", error.message);
    throw error;
  }
  recordMockInstall(device.serial, mockDevice(device.serial), apk.path);
  result.apkSha256 = "0".repeat(64);
  steps.push("Install: Success (0ms)");
  recordMockRunDevice(run.name, device.serial);

  const launch = run.launch;
  if (launch.kind === "none") {
    steps.push(`Run configuration '${run.name}' installs only — not launching.`);
    phase("done");
    return result;
  }
  const pkg = MOCK_APK_PACKAGE;
  result.package = pkg;
  steps.push(`Package (from APK): ${pkg}`);
  if (launch.kind === "deepLink") {
    steps.push(`adb shell am start -a android.intent.action.VIEW -d ${launch.uri} -p ${pkg}`);
    phase("launching");
    result.launch = {
      output: `Starting: Intent { act=android.intent.action.VIEW dat=${launch.uri} pkg=${pkg} }`,
      timing: null,
    };
  } else {
    const activity = launch.kind === "activity" ? launch.name : null;
    steps.push(`adb shell am start -W (${activity ? `${pkg}/${activity}` : `package: ${pkg}`})`);
    phase("launching");
    result.launch = mockLaunchApp(device.serial, pkg, built.recordId);
  }
  phase("done");
  return result;
}

export function deployHandlers(): Record<string, (args: unknown) => unknown> {
  return {
    run_run_configuration: (args: unknown) => mockRunConfiguration(args),
  };
}
