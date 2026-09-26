import { describe, it, expect } from "vitest";
import { queryAfterLaunch, validateLogcatQuery } from "./logcat-query";

describe("validateLogcatQuery", () => {
  it("accepts queries the filter bar applies", () => {
    for (const query of [
      "",
      "package:mine level:warn",
      'tag:Net -tag:system message:"timed out"',
      "tag~:^Net.*$ age:5m is:crash pid:123",
      "error",
      "free text unknown:key",
    ]) {
      expect(validateLogcatQuery(query), query).toBeNull();
    }
  });

  it("names what is wrong", () => {
    expect(validateLogcatQuery("level:loud")).toContain("'loud' is not a log level");
    expect(validateLogcatQuery("age:soon")).toContain("'soon' is not an age");
    expect(validateLogcatQuery("is:anr")).toContain("'is:anr' is not supported");
    expect(validateLogcatQuery("pid:abc")).toBe("'pid:abc' needs a number.");
    expect(validateLogcatQuery("tag~:([")).toContain("not a valid regular expression");
    expect(validateLogcatQuery('message:"open')).toBe("A quote is not closed.");
  });
});

describe("queryAfterLaunch", () => {
  it("replaces the query with the run configuration's filter", () => {
    expect(queryAfterLaunch("tag:Old ", " package:mine level:warn ")).toBe(
      "package:mine level:warn "
    );
    expect(queryAfterLaunch("package:mine level:warn ", "package:mine level:warn")).toBeNull();
  });

  it("merges package:mine without a filter", () => {
    expect(queryAfterLaunch("level:error ", null)).toBe("level:error package:mine ");
    expect(queryAfterLaunch("pkg:mine ", null)).toBeNull();
    expect(queryAfterLaunch("", "   ")).toBe("package:mine ");
  });
});
