/**
 * Tests for QueryBar's query-string parsing convention.
 *
 * The pill system relies on a trailing-space contract:
 *
 *   "package:mine"    → last token is the DRAFT (still in the text input)
 *   "package:mine "   → all tokens are COMMITTED (rendered as pills)
 *
 * `buildQuery` always appends a trailing space when there is no draft, so
 * values produced by user interaction are always correct.  The bug class
 * being prevented here is callers that SET the query programmatically
 * (applyPreset, handlePackageSelect, handleAgePill) without appending the
 * trailing space — causing the last token to stay as a text draft instead
 * of becoming a pill until the user presses Space.
 */

import { describe, it, expect } from "vitest";
import { parseQueryState, buildQuery } from "./QueryBar";
import { setPackageInQuery, setAgeInQuery } from "@/lib/logcat-query";

// ── parseQueryState — trailing-space convention ───────────────────────────────

describe("parseQueryState — trailing-space convention", () => {
  it("token without trailing space is a draft, not a pill", () => {
    expect(parseQueryState("package:mine")).toEqual({ committed: [], draft: "package:mine" });
  });

  it("token with trailing space is a committed pill", () => {
    expect(parseQueryState("package:mine ")).toEqual({ committed: ["package:mine"], draft: "" });
  });

  it("last token is draft when query has no trailing space", () => {
    expect(parseQueryState("level:error tag:App")).toEqual({
      committed: ["level:error"],
      draft: "tag:App",
    });
  });

  it("all tokens are committed pills when query ends with space", () => {
    expect(parseQueryState("level:error tag:App ")).toEqual({
      committed: ["level:error", "tag:App"],
      draft: "",
    });
  });

  it("returns empty for empty string", () => {
    expect(parseQueryState("")).toEqual({ committed: [], draft: "" });
  });

  it("returns empty for whitespace-only string", () => {
    expect(parseQueryState("   ")).toEqual({ committed: [], draft: "" });
  });

  it("OR separator is committed correctly", () => {
    expect(parseQueryState("level:error | is:crash ")).toEqual({
      committed: ["level:error", "|", "is:crash"],
      draft: "",
    });
  });

  it("multi-group query without trailing space has last token as draft", () => {
    expect(parseQueryState("level:error | is:crash")).toEqual({
      committed: ["level:error", "|"],
      draft: "is:crash",
    });
  });

  it("quoted string containing a space is treated as a single token", () => {
    expect(parseQueryState('tag:"hello world" ')).toEqual({
      committed: ['tag:"hello world"'],
      draft: "",
    });
  });

  it("quoted string without closing trailing space remains a draft", () => {
    expect(parseQueryState('tag:"hello world"')).toEqual({
      committed: [],
      draft: 'tag:"hello world"',
    });
  });
});

// ── buildQuery ────────────────────────────────────────────────────────────────

describe("buildQuery — trailing-space convention", () => {
  it("appends trailing space when committed and no draft (all-pills state)", () => {
    expect(buildQuery(["package:mine"], "")).toBe("package:mine ");
  });

  it("appends trailing space for multiple committed tokens with no draft", () => {
    expect(buildQuery(["level:error", "tag:App"], "")).toBe("level:error tag:App ");
  });

  it("does not add trailing space when a draft is present", () => {
    expect(buildQuery(["level:error"], "tag:App")).toBe("level:error tag:App");
  });

  it("returns empty string when nothing is committed and no draft", () => {
    expect(buildQuery([], "")).toBe("");
  });

  it("returns draft-only when nothing is committed", () => {
    expect(buildQuery([], "package:mine")).toBe("package:mine");
  });

  it("round-trips: parse fully committed → build → parse again gives same result", () => {
    const initial = parseQueryState("level:error tag:App ");
    const rebuilt = buildQuery(initial.committed, initial.draft);
    expect(parseQueryState(rebuilt)).toEqual(initial);
  });
});

// ── Programmatic query setters must commit tokens ─────────────────────────────
//
// setPackageInQuery and setAgeInQuery return trimmed strings (no trailing space).
// Callers that set the query programmatically MUST append " " so the result is
// rendered as committed pills rather than a text draft.

describe("programmatic query helpers — trailing space required for pill commit", () => {
  it("setPackageInQuery output without trailing space leaves token as draft", () => {
    const raw = setPackageInQuery("", "com.example.app");
    // Without appending " ": last token is draft, not a pill.
    expect(parseQueryState(raw).draft).toBe("package:com.example.app");
  });

  it("setPackageInQuery output WITH trailing space commits token as pill", () => {
    const raw = setPackageInQuery("", "com.example.app");
    const committed = raw.trimEnd() + " ";
    const state = parseQueryState(committed);
    expect(state.draft).toBe("");
    expect(state.committed).toContain("package:com.example.app");
  });

  it("setPackageInQuery with existing tokens — appending space commits all as pills", () => {
    const raw = setPackageInQuery("level:error ", "com.example.app");
    const committed = raw.trimEnd() + " ";
    const state = parseQueryState(committed);
    expect(state.draft).toBe("");
    expect(state.committed).toContain("level:error");
    expect(state.committed).toContain("package:com.example.app");
  });

  it("setAgeInQuery output without trailing space leaves token as draft", () => {
    const raw = setAgeInQuery("", "5m");
    expect(parseQueryState(raw).draft).toBe("age:5m");
  });

  it("setAgeInQuery output WITH trailing space commits token as pill", () => {
    const raw = setAgeInQuery("", "5m");
    const committed = raw.trimEnd() + " ";
    const state = parseQueryState(committed);
    expect(state.draft).toBe("");
    expect(state.committed).toContain("age:5m");
  });

  it("setAgeInQuery with existing tokens — appending space commits all as pills", () => {
    const raw = setAgeInQuery("package:mine ", "5m");
    const committed = raw.trimEnd() + " ";
    const state = parseQueryState(committed);
    expect(state.draft).toBe("");
    expect(state.committed).toContain("package:mine");
    expect(state.committed).toContain("age:5m");
  });

  it("preset query without trailing space leaves last token as draft", () => {
    // Simulates what applyPreset did BEFORE the fix.
    expect(parseQueryState("package:mine").draft).toBe("package:mine");
  });

  it("preset query WITH trailing space commits all tokens as pills", () => {
    // Simulates what applyPreset does AFTER the fix.
    const state = parseQueryState("package:mine ");
    expect(state.draft).toBe("");
    expect(state.committed).toContain("package:mine");
  });

  it("multi-token preset WITH trailing space commits all tokens as pills", () => {
    const state = parseQueryState("package:mine | is:crash ");
    expect(state.draft).toBe("");
    expect(state.committed).toContain("package:mine");
    expect(state.committed).toContain("is:crash");
  });

  it("trimEnd + space is safe on empty string (no phantom space token)", () => {
    const q = "";
    const normalized = q.trimEnd() ? q.trimEnd() + " " : "";
    expect(normalized).toBe("");
    expect(parseQueryState(normalized)).toEqual({ committed: [], draft: "" });
  });
});
