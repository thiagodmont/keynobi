import { describe, expect, it } from "vitest";
import type { LogcatEntry } from "@/lib/tauri-api";
import { formatLogcatEntries, formatLogcatEntry } from "./logcat-entry-format";

const ENTRY = {
  id: 1n,
  timestamp: "05-10 09:30:00.000",
  pid: 123,
  tid: 456,
  level: "warn",
  tag: "MainActivity",
  message: "Started",
  package: "com.example.app",
  kind: "normal",
  isCrash: false,
  flags: 0,
  category: "general",
  crashGroupId: null,
  jsonBody: null,
} satisfies LogcatEntry;

describe("logcat entry formatting", () => {
  it("formats a single entry with package context", () => {
    expect(formatLogcatEntry(ENTRY)).toBe(
      "05-10 09:30:00.000  WARN  [com.example.app] MainActivity: Started"
    );
  });

  it("omits package context when absent", () => {
    expect(formatLogcatEntry({ ...ENTRY, package: null })).toBe(
      "05-10 09:30:00.000  WARN  MainActivity: Started"
    );
  });

  it("formats multiple entries as newline-delimited text", () => {
    expect(formatLogcatEntries([ENTRY, { ...ENTRY, id: 2n, message: "Next" }])).toBe(
      [
        "05-10 09:30:00.000  WARN  [com.example.app] MainActivity: Started",
        "05-10 09:30:00.000  WARN  [com.example.app] MainActivity: Next",
      ].join("\n")
    );
  });
});
