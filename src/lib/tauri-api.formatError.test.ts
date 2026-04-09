import { describe, it, expect } from "vitest";
import { formatError } from "@/lib/tauri-api";

describe("formatError", () => {
  it("returns strings as-is", () => {
    expect(formatError("oops")).toBe("oops");
  });

  it("uses Error.message", () => {
    expect(formatError(new Error("failed"))).toBe("failed");
  });

  it("formats AppError-shaped IPC objects with kind and message", () => {
    expect(
      formatError({ kind: "io", message: "gradlew not found" }),
    ).toBe("io: gradlew not found");
  });

  it("uses string error property when present", () => {
    expect(formatError({ error: "shell failed" })).toBe("shell failed");
  });

  it("JSON-stringifies plain objects without message", () => {
    expect(formatError({})).toBe("{}");
  });

  it("does not produce [object Object] for a typical Tauri AppError payload", () => {
    const s = formatError({
      kind: "notFound",
      message: "gradlew not found at project root",
    });
    expect(s).toContain("notFound");
    expect(s).toContain("gradlew");
    expect(s).not.toContain("[object Object]");
  });
});
