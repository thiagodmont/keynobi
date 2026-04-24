import type { BuildError, BuildLine, BuildRecord, BuildStatus } from "@/bindings";

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
      durationMs: BigInt(4000),
      errorCount: 0,
      warningCount: 0,
    } as BuildStatus,
    errors: [],
    startedAt: new Date().toISOString(),
    projectRoot: "/mock/android-project",
    ...overrides,
  };
}
