import { describe, expect, it } from "vitest";
import { BUILTIN_LOGCAT_FILTER_PRESETS, commitSavedFilterQuery } from "./saved-filter-presets";

describe("saved filter presets", () => {
  it("keeps built-in queries available as committed filter queries", () => {
    expect(BUILTIN_LOGCAT_FILTER_PRESETS.map((preset) => preset.query)).toEqual([
      "package:mine",
      "is:crash",
      "level:error",
      "age:5m",
      "package:mine | is:crash",
    ]);
  });

  it("normalizes an applied filter query to QueryBar committed-token form", () => {
    expect(commitSavedFilterQuery("package:mine")).toBe("package:mine ");
    expect(commitSavedFilterQuery("package:mine   ")).toBe("package:mine ");
  });
});
