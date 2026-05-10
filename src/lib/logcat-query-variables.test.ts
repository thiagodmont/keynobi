import { describe, expect, it } from "vitest";
import {
  buildEffectiveQueryWithDisabledPills,
  buildQueryBarPillRefs,
  matchesFilterGroups,
  parseFilterGroups,
  parseQuery,
} from "./logcat-query";
import {
  ensureQueryVariableValues,
  extractQueryVariables,
  isValidQueryVariableName,
  reconcileQueryVariableValues,
  resolveQueryVariables,
} from "./logcat-query-variables";
import type { LogcatEntry } from "@/lib/tauri-api";

function makeEntry(overrides: Partial<LogcatEntry> = {}): LogcatEntry {
  return {
    id: 1n,
    timestamp: "01-23 12:34:56.789",
    pid: 1234,
    tid: 5678,
    level: "debug",
    tag: "MyTag",
    message: "Hello world",
    isCrash: false,
    package: null,
    kind: "normal",
    flags: 0,
    category: "general",
    crashGroupId: null,
    jsonBody: null,
    ...overrides,
  };
}

const NOW = (() => {
  const year = new Date().getFullYear();
  return new Date(`${year}-01-23T12:35:00.000`).getTime();
})();

describe("query variables", () => {
  it("extracts unique variables in first-seen order", () => {
    expect(
      extractQueryVariables("message:${action_name} tag:${screen} message:prefix_${action_name}")
    ).toEqual(["action_name", "screen"]);
  });

  it("resolves variables inside filter values", () => {
    expect(
      resolveQueryVariables("message:some_prefix_${action_name} tag:${screen}", {
        action_name: "tap",
        screen: "Home",
      })
    ).toBe("message:some_prefix_tap tag:Home");
  });

  it("resolves variables inside quoted values with spaces", () => {
    const resolved = resolveQueryVariables('message:"User ${action_name} clicked"', {
      action_name: "checkout button",
    });

    expect(resolved).toBe('message:"User checkout button clicked"');
    expect(parseQuery(resolved)[0]).toMatchObject({
      type: "message",
      value: "User checkout button clicked",
    });
  });

  it("resolves variables before OR group matching", () => {
    const groups = parseFilterGroups(
      resolveQueryVariables("message:action_${action_name}_done | tag:Fallback", {
        action_name: "checkout",
      })
    );

    expect(
      matchesFilterGroups(
        makeEntry({ tag: "VariableAction", message: "action_checkout_done" }),
        groups,
        NOW
      )
    ).toBe(true);
    expect(
      matchesFilterGroups(makeEntry({ tag: "Fallback", message: "fallback path" }), groups, NOW)
    ).toBe(true);
    expect(
      matchesFilterGroups(
        makeEntry({ tag: "VariableAction", message: "action_login_done" }),
        groups,
        NOW
      )
    ).toBe(false);
  });

  it("can disable a variable pill before resolving active variables", () => {
    const refs = buildQueryBarPillRefs(["tag:Alpha", "message:action_${action_name}_done"]);
    const disabled = new Set([
      refs.find((ref) => ref.token === "message:action_${action_name}_done")!.id,
    ]);
    const effectiveTemplate = buildEffectiveQueryWithDisabledPills(
      "tag:Alpha message:action_${action_name}_done ",
      disabled
    );

    expect(resolveQueryVariables(effectiveTemplate, { action_name: "checkout" })).toBe(
      "tag:Alpha "
    );
  });

  it("leaves empty or unset variables literal so they do not broaden filters", () => {
    expect(resolveQueryVariables("message:${action_name}", {})).toBe("message:${action_name}");
    expect(resolveQueryVariables("message:${action_name}", { action_name: "   " })).toBe(
      "message:${action_name}"
    );
  });

  it("reconciles variable values to variables still present in the query", () => {
    expect(
      reconcileQueryVariableValues("message:${next}", {
        action_name: "tap",
        next: "open",
      })
    ).toEqual({ next: "open" });
  });

  it("keeps predefined variable values while adding referenced variables", () => {
    expect(
      ensureQueryVariableValues("message:${screen}", {
        action_name: "checkout",
      })
    ).toEqual({ action_name: "checkout", screen: "" });
  });

  it("validates user-created variable names", () => {
    expect(isValidQueryVariableName("action_name")).toBe(true);
    expect(isValidQueryVariableName("_screen2")).toBe(true);
    expect(isValidQueryVariableName("2screen")).toBe(false);
    expect(isValidQueryVariableName("screen-name")).toBe(false);
  });
});
