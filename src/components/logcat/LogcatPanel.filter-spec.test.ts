import { describe, expect, it, beforeEach } from "vitest";
import { parseFilterGroups, parseQuery, setMinePackage } from "@/lib/logcat-query";
import { groupsToFilterSpec, hasAnyFrontendOnlyLogic, tokensToFilterSpec } from "./LogcatPanel";

describe("LogcatPanel backend filter spec conversion", () => {
  beforeEach(() => {
    setMinePackage(null);
  });

  it("keeps complex frontend-only tokens out of the backend filter", () => {
    const spec = tokensToFilterSpec(
      parseQuery("level:error tag:OkHttp tag~:Ok.*Http -message:noise age:5m is:stacktrace")
    );

    expect(spec).toEqual({
      minLevel: "error",
      tag: "OkHttp",
      text: null,
      package: null,
      onlyCrashes: false,
    });
  });

  it("resolves package:mine only after the project package is known", () => {
    expect(tokensToFilterSpec(parseQuery("package:mine")).package).toBeNull();

    setMinePackage("com.example.app");

    expect(tokensToFilterSpec(parseQuery("package:mine")).package).toBe("com.example.app");
  });

  it("uses the most permissive safe backend filter for OR groups", () => {
    const groups = parseFilterGroups(
      "level:error tag:App package:com.example | level:warn tag:System package:com.example"
    );

    expect(groupsToFilterSpec(groups)).toEqual({
      minLevel: "warn",
      tag: null,
      text: null,
      package: "com.example",
      onlyCrashes: false,
    });
  });

  it("requires frontend filtering for OR groups and overflow tokens", () => {
    expect(hasAnyFrontendOnlyLogic(parseFilterGroups("level:error | is:crash"))).toBe(true);
    expect(hasAnyFrontendOnlyLogic(parseFilterGroups("message:socket message:IPPROTO_TCP"))).toBe(
      true
    );
    expect(hasAnyFrontendOnlyLogic(parseFilterGroups("level:error tag:App"))).toBe(false);
  });
});
