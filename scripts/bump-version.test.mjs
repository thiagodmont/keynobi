import { describe, it, expect } from "vitest";
import { bumpVersion } from "./bump-version.mjs";

describe("bumpVersion", () => {
  it("increments patch by 1", () => {
    expect(bumpVersion("0.1.0", "patch")).toBe("0.1.1");
  });

  it("patch is the default when type is omitted", () => {
    expect(bumpVersion("0.1.0")).toBe("0.1.1");
  });

  it("patch increment resets nothing", () => {
    expect(bumpVersion("1.2.3", "patch")).toBe("1.2.4");
  });

  it("increments minor by 1 and resets patch to 0", () => {
    expect(bumpVersion("0.1.3", "minor")).toBe("0.2.0");
  });

  it("minor increment preserves major", () => {
    expect(bumpVersion("2.4.7", "minor")).toBe("2.5.0");
  });

  it("increments major by 1 and resets minor and patch to 0", () => {
    expect(bumpVersion("0.1.3", "major")).toBe("1.0.0");
  });

  it("major increment from non-zero minor and patch", () => {
    expect(bumpVersion("3.7.9", "major")).toBe("4.0.0");
  });

  it("throws on unknown bump type", () => {
    expect(() => bumpVersion("0.1.0", "hotfix")).toThrow(
      'unknown bump type "hotfix". Use: patch, minor, or major.'
    );
  });

  it("throws when version is not valid semver", () => {
    expect(() => bumpVersion("1.2", "patch")).toThrow(
      'could not parse version "1.2" from package.json. Expected semver (e.g. 1.2.3).'
    );
  });

  it("throws when version contains non-numeric parts", () => {
    expect(() => bumpVersion("1.x.0", "patch")).toThrow(
      'could not parse version "1.x.0" from package.json. Expected semver (e.g. 1.2.3).'
    );
  });
});
