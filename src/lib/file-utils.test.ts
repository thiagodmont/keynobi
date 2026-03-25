import { describe, it, expect } from "vitest";
import {
  detectLanguage,
  getFileTypeInfo,
  getFileTypeInfoByPath,
  basename,
  dirname,
  joinPath,
} from "./file-utils";

// ── detectLanguage ────────────────────────────────────────────────────────────

describe("detectLanguage", () => {
  it("detects .kt as kotlin", () => {
    expect(detectLanguage("/project/src/Main.kt")).toBe("kotlin");
  });

  it("detects .gradle.kts as gradle", () => {
    expect(detectLanguage("/project/app/build.gradle.kts")).toBe("gradle");
  });

  it("detects .gradle as gradle", () => {
    expect(detectLanguage("/project/build.gradle")).toBe("gradle");
  });

  it("detects .xml as xml", () => {
    expect(detectLanguage("/project/res/layout/activity_main.xml")).toBe("xml");
  });

  it("detects .json as json", () => {
    expect(detectLanguage("/project/package.json")).toBe("json");
  });

  it("returns text for unknown extensions", () => {
    expect(detectLanguage("/project/README.md")).toBe("text");
    expect(detectLanguage("/project/file.unknown")).toBe("text");
  });

  it("returns text for files without extension", () => {
    expect(detectLanguage("/project/Makefile")).toBe("text");
  });
});

// ── getFileTypeInfo ───────────────────────────────────────────────────────────

describe("getFileTypeInfo", () => {
  it("returns purple for kotlin", () => {
    const info = getFileTypeInfo("kotlin");
    expect(info.label).toBe("K");
    expect(info.color).toBe("#a97bff");
  });

  it("returns green for gradle", () => {
    const info = getFileTypeInfo("gradle");
    expect(info.label).toBe("G");
    expect(info.color).toBe("#02b10a");
  });

  it("returns muted for text", () => {
    const info = getFileTypeInfo("text");
    expect(info.label).toBe("T");
  });
});

describe("getFileTypeInfoByPath", () => {
  it("delegates to detectLanguage then getFileTypeInfo", () => {
    expect(getFileTypeInfoByPath("Foo.kt").label).toBe("K");
    expect(getFileTypeInfoByPath("build.gradle.kts").label).toBe("G");
    expect(getFileTypeInfoByPath("layout.xml").label).toBe("X");
  });
});

// ── Path helpers ──────────────────────────────────────────────────────────────

describe("basename", () => {
  it("returns the last path segment", () => {
    expect(basename("/project/src/Main.kt")).toBe("Main.kt");
  });

  it("returns the path itself when there is no slash", () => {
    expect(basename("file.kt")).toBe("file.kt");
  });

  it("handles trailing segment correctly", () => {
    expect(basename("/a/b/c")).toBe("c");
  });
});

describe("dirname", () => {
  it("returns the parent directory", () => {
    expect(dirname("/project/src/Main.kt")).toBe("/project/src");
  });

  it("returns / for a top-level path", () => {
    expect(dirname("/file.kt")).toBe("/");
  });
});

describe("joinPath", () => {
  it("joins parent and name with a slash", () => {
    expect(joinPath("/project/src", "Main.kt")).toBe("/project/src/Main.kt");
  });

  it("does not double-add a trailing slash", () => {
    expect(joinPath("/project/src/", "Main.kt")).toBe("/project/src/Main.kt");
  });
});
