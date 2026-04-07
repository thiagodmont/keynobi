import { describe, it, expect } from "vitest";
import { parseBumpType } from "./release.mjs";

describe("parseBumpType", () => {
  it("returns 'patch' when no argument is given", () => {
    expect(parseBumpType(["node", "release.mjs"])).toBe("patch");
  });

  it("accepts 'patch' explicitly", () => {
    expect(parseBumpType(["node", "release.mjs", "patch"])).toBe("patch");
  });

  it("accepts 'minor'", () => {
    expect(parseBumpType(["node", "release.mjs", "minor"])).toBe("minor");
  });

  it("accepts 'major'", () => {
    expect(parseBumpType(["node", "release.mjs", "major"])).toBe("major");
  });

  it("throws on an unknown bump type", () => {
    expect(() => parseBumpType(["node", "release.mjs", "hotfix"])).toThrow(
      'Unknown bump type "hotfix". Use: patch, minor, or major.'
    );
  });
});
