import { describe, expect, it } from "vitest";
import { validateProjectInfoInput } from "./ProjectInfoEditor";

describe("validateProjectInfoInput", () => {
  it("accepts valid version info and trims the version name", () => {
    const result = validateProjectInfoInput(" 1.2.3 ", "42");

    expect(result).toEqual({ ok: true, versionName: "1.2.3", versionCode: 42n });
  });

  it("rejects partially numeric version codes", () => {
    const result = validateProjectInfoInput("1.2.3", "42beta");

    expect(result).toEqual({
      ok: false,
      message: "Version code must be a non-negative integer.",
    });
  });

  it("rejects version names that would break a Gradle string literal", () => {
    const result = validateProjectInfoInput('1.2"3', "42");

    expect(result).toEqual({
      ok: false,
      message: "Version name cannot contain quotes, backslashes, '$', or line breaks.",
    });
  });

  it("rejects version names with Gradle string interpolation syntax", () => {
    const result = validateProjectInfoInput("1.$0", "42");

    expect(result).toEqual({
      ok: false,
      message: "Version name cannot contain quotes, backslashes, '$', or line breaks.",
    });
  });
});
