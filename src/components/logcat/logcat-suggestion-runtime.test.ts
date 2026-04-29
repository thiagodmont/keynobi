import { createRoot } from "solid-js";
import { afterEach, describe, expect, it, vi } from "vitest";
import type { LogcatEntry } from "@/lib/tauri-api";
import { createLogcatSuggestionRuntime } from "./logcat-suggestion-runtime";

function entry(overrides: Partial<LogcatEntry>): LogcatEntry {
  return {
    id: BigInt(overrides.id ?? 1),
    timestamp: "01-01 00:00:00.000",
    pid: 1,
    tid: 1,
    level: "info",
    tag: "MainActivity",
    message: "message",
    package: "com.example.app",
    kind: "normal",
    isCrash: false,
    flags: 0,
    category: "general",
    crashGroupId: null,
    jsonBody: null,
    ...overrides,
  };
}

afterEach(() => {
  vi.useRealTimers();
});

describe("createLogcatSuggestionRuntime", () => {
  it("publishes ingested suggestions on immediate flush", () => {
    vi.useFakeTimers();

    createRoot((dispose) => {
      const runtime = createLogcatSuggestionRuntime();
      runtime.ingest([entry({ tag: "OkHttp", package: "com.example.network" })]);
      runtime.flush(true);

      vi.advanceTimersByTime(0);

      expect(runtime.knownTags()).toEqual(["OkHttp"]);
      expect(runtime.knownPackages()).toEqual(["com.example.network"]);
      dispose();
    });
  });

  it("cancels pending publication when cleared", () => {
    vi.useFakeTimers();

    createRoot((dispose) => {
      const runtime = createLogcatSuggestionRuntime();
      runtime.ingest([entry({ tag: "Retrofit", package: "com.example.api" })]);
      runtime.flush();
      runtime.clear();

      vi.advanceTimersByTime(3_000);

      expect(runtime.knownTags()).toEqual([]);
      expect(runtime.knownPackages()).toEqual([]);
      dispose();
    });
  });
});
