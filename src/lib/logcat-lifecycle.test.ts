import { describe, expect, it } from "vitest";
import type { LogcatEntry } from "@/lib/tauri-api";
import { isLifecycleLogcatEntry } from "./logcat-lifecycle";

function entry(overrides: Partial<LogcatEntry>): LogcatEntry {
  return {
    id: 1n,
    timestamp: "05-08 12:00:00.000",
    pid: 100,
    tid: 100,
    level: "info",
    tag: "App",
    message: "hello",
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

describe("isLifecycleLogcatEntry", () => {
  it("matches normal entries classified as lifecycle", () => {
    expect(isLifecycleLogcatEntry(entry({ category: "lifecycle", kind: "normal" }))).toBe(true);
  });

  it("matches special process separator kinds", () => {
    expect(isLifecycleLogcatEntry(entry({ category: "general", kind: "processDied" }))).toBe(true);
    expect(isLifecycleLogcatEntry(entry({ category: "general", kind: "processStarted" }))).toBe(
      true
    );
  });

  it("does not match normal non-lifecycle entries", () => {
    expect(isLifecycleLogcatEntry(entry({ category: "general", kind: "normal" }))).toBe(false);
    expect(isLifecycleLogcatEntry(entry({ category: "network", kind: "normal" }))).toBe(false);
  });
});
