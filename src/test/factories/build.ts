import type {
  BuildError,
  BuildLine,
  BuildRecord,
  BuildStatus,
  LaunchTiming,
  MappingSnapshot,
} from "@/bindings";

export function makeBuildLine(overrides: Partial<BuildLine> = {}): BuildLine {
  return {
    kind: "output",
    content: "> Task :app:assembleDebug",
    file: null,
    line: null,
    col: null,
    ...overrides,
  };
}

export function makeBuildError(overrides: Partial<BuildError> = {}): BuildError {
  return {
    message: "error: unresolved reference: Foo",
    file: "app/src/main/java/com/example/MainActivity.kt",
    line: 42,
    col: 8,
    severity: "error",
    ...overrides,
  };
}

export function makeBuildRecord(overrides: Partial<BuildRecord> = {}): BuildRecord {
  return {
    id: 1,
    task: "assembleDebug",
    status: {
      state: "success",
      success: true,
      durationMs: 4000,
      errorCount: 0,
      warningCount: 0,
    } as BuildStatus,
    errors: [],
    startedAt: new Date().toISOString(),
    projectRoot: "/mock/android-project",
    origin: null,
    cancelledBy: null,
    launch: null,
    mappings: [],
    ...overrides,
  };
}

export function makeMappingSnapshot(overrides: Partial<MappingSnapshot> = {}): MappingSnapshot {
  return {
    module: ":app",
    variant: "release",
    sha256: "6b1c2f0a".repeat(8),
    bytes: 48_213_771,
    pgMapId: "6b1c2f0",
    ...overrides,
  };
}

export function makeLaunchTiming(overrides: Partial<LaunchTiming> = {}): LaunchTiming {
  return {
    totalMs: 812,
    waitMs: 815,
    launchState: "cold",
    measuredAt: new Date().toISOString(),
    serial: "emulator-5554",
    avdName: "Pixel_7_API_34",
    model: "sdk_gphone64_arm64",
    ...overrides,
  };
}
