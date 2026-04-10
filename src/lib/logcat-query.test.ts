import { describe, it, expect } from "vitest";
import {
  parseQuery,
  matchesQuery,
  parseFilterGroups,
  matchesFilterGroups,
  getFrontendOnlyTokens,
  addOrGroup,
  addAndConnector,
  getGroupCount,
  getActiveGroupSegment,
  getActiveGroupOffset,
  parseAge,
  parseLogcatTimestamp,
  setAgeInQuery,
  setPackageInQuery,
  getPackageFromQuery,
  getActiveTokenContext,
  setMinePackage,
  isStackTraceLine,
  parseStackFrame,
  isProjectFrame,
} from "./logcat-query";
import type { LogcatEntry } from "@/lib/tauri-api";

// ── Test helpers ──────────────────────────────────────────────────────────────

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
  return new Date(`${year}-01-23T12:35:00.000`).getTime(); // 4 seconds after the test entry timestamp
})();

// ── parseAge ─────────────────────────────────────────────────────────────────

describe("parseAge", () => {
  it("parses seconds", () => expect(parseAge("30s")).toBe(30));
  it("parses minutes", () => expect(parseAge("5m")).toBe(300));
  it("parses hours", () => expect(parseAge("1h")).toBe(3600));
  it("parses days", () => expect(parseAge("1d")).toBe(86400));
  it("parses decimal", () => expect(parseAge("1.5h")).toBeCloseTo(5400));
  it("returns null for invalid", () => expect(parseAge("5")).toBeNull());
  it("returns null for empty", () => expect(parseAge("")).toBeNull());
  it("is case-insensitive", () => expect(parseAge("5M")).toBe(300));
});

// ── parseQuery ────────────────────────────────────────────────────────────────

describe("parseQuery — empty / blank", () => {
  it("returns empty array for empty string", () => {
    expect(parseQuery("")).toHaveLength(0);
  });
  it("returns empty array for whitespace", () => {
    expect(parseQuery("   ")).toHaveLength(0);
  });
});

describe("parseQuery — level tokens", () => {
  it("parses level:error", () => {
    const tokens = parseQuery("level:error");
    expect(tokens).toHaveLength(1);
    expect(tokens[0]).toMatchObject({ type: "level", value: "error", negate: false });
  });

  it("parses negated level:-level:warn", () => {
    const tokens = parseQuery("-level:warn");
    expect(tokens[0]).toMatchObject({ type: "level", value: "warn", negate: true });
  });

  it("parses bare level name shorthand", () => {
    const tokens = parseQuery("error");
    expect(tokens[0]).toMatchObject({ type: "level", value: "error" });
  });

  it("parses warn shorthand", () => {
    const tokens = parseQuery("warn");
    expect(tokens[0]).toMatchObject({ type: "level", value: "warn" });
  });
});

describe("parseQuery — tag tokens", () => {
  it("parses tag:MyTag", () => {
    const tokens = parseQuery("tag:MyTag");
    expect(tokens[0]).toMatchObject({ type: "tag", value: "MyTag", negate: false, regex: false });
  });

  it("parses -tag:system (negation)", () => {
    const tokens = parseQuery("-tag:system");
    expect(tokens[0]).toMatchObject({ type: "tag", value: "system", negate: true, regex: false });
  });

  it("parses tag~:My.*Tag (regex)", () => {
    const tokens = parseQuery("tag~:My.*Tag");
    expect(tokens[0]).toMatchObject({ type: "tag", value: "My.*Tag", negate: false, regex: true });
  });

  it("parses -tag~:My.*Tag (negated regex)", () => {
    const tokens = parseQuery("-tag~:My.*Tag");
    expect(tokens[0]).toMatchObject({ type: "tag", value: "My.*Tag", negate: true, regex: true });
  });
});

describe("parseQuery — message tokens", () => {
  it("parses message:crash", () => {
    const tokens = parseQuery("message:crash");
    expect(tokens[0]).toMatchObject({ type: "message", value: "crash", negate: false, regex: false });
  });

  it("parses msg: alias", () => {
    const tokens = parseQuery("msg:hello");
    expect(tokens[0]).toMatchObject({ type: "message", value: "hello" });
  });

  it("parses message~:Null.*Exception (regex)", () => {
    const tokens = parseQuery("message~:Null.*Exception");
    expect(tokens[0]).toMatchObject({ type: "message", regex: true });
  });
});

describe("parseQuery — package tokens", () => {
  it("parses package:com.example", () => {
    const tokens = parseQuery("package:com.example");
    expect(tokens[0]).toMatchObject({ type: "package", value: "com.example", negate: false });
  });

  it("parses package:mine", () => {
    const tokens = parseQuery("package:mine");
    expect(tokens[0]).toMatchObject({ type: "package", value: "mine" });
  });

  it("parses pkg: alias", () => {
    const tokens = parseQuery("pkg:com.foo");
    expect(tokens[0]).toMatchObject({ type: "package", value: "com.foo" });
  });
});

describe("parseQuery — age tokens", () => {
  it("parses age:5m", () => {
    const tokens = parseQuery("age:5m");
    expect(tokens[0]).toMatchObject({ type: "age", seconds: 300 });
  });

  it("ignores negated age (age can't be negated meaningfully)", () => {
    const tokens = parseQuery("-age:5m");
    expect(tokens.find((t) => t.type === "age")).toBeUndefined();
  });
});

describe("parseQuery — is tokens", () => {
  it("parses is:crash", () => {
    const tokens = parseQuery("is:crash");
    expect(tokens[0]).toMatchObject({ type: "is", value: "crash" });
  });

  it("parses is:stacktrace", () => {
    const tokens = parseQuery("is:stacktrace");
    expect(tokens[0]).toMatchObject({ type: "is", value: "stacktrace" });
  });
});

describe("parseQuery — freetext tokens", () => {
  it("treats unknown text as freetext", () => {
    const tokens = parseQuery("com.example");
    expect(tokens[0]).toMatchObject({ type: "freetext", value: "com.example", negate: false });
  });

  it("parses negated freetext -system", () => {
    const tokens = parseQuery("-system");
    expect(tokens[0]).toMatchObject({ type: "freetext", value: "system", negate: true });
  });
});

describe("parseQuery — multi-token queries", () => {
  it("parses multiple tokens", () => {
    const tokens = parseQuery("level:error tag:MyTag");
    expect(tokens).toHaveLength(2);
    expect(tokens[0].type).toBe("level");
    expect(tokens[1].type).toBe("tag");
  });

  it("parses complex mixed query", () => {
    const tokens = parseQuery("com.example level:error -tag:system age:5m");
    expect(tokens).toHaveLength(4);
  });
});

// ── matchesQuery ──────────────────────────────────────────────────────────────

describe("matchesQuery — no tokens", () => {
  it("matches everything with empty tokens", () => {
    expect(matchesQuery(makeEntry(), [], NOW)).toBe(true);
  });
});

describe("matchesQuery — level filter", () => {
  it("matches entry with exact level", () => {
    const tokens = parseQuery("level:debug");
    expect(matchesQuery(makeEntry({ level: "debug" }), tokens, NOW)).toBe(true);
  });

  it("matches entry with higher level", () => {
    const tokens = parseQuery("level:warn");
    expect(matchesQuery(makeEntry({ level: "error" }), tokens, NOW)).toBe(true);
  });

  it("excludes entry below minimum level", () => {
    const tokens = parseQuery("level:warn");
    expect(matchesQuery(makeEntry({ level: "debug" }), tokens, NOW)).toBe(false);
  });

  it("negated level: excludes matching priority", () => {
    const tokens = parseQuery("-level:error");
    expect(matchesQuery(makeEntry({ level: "fatal" }), tokens, NOW)).toBe(false);
    expect(matchesQuery(makeEntry({ level: "warn" }), tokens, NOW)).toBe(true);
  });
});

describe("matchesQuery — tag filter", () => {
  it("matches tag substring", () => {
    const tokens = parseQuery("tag:My");
    expect(matchesQuery(makeEntry({ tag: "MyTag" }), tokens, NOW)).toBe(true);
  });

  it("is case-insensitive", () => {
    const tokens = parseQuery("tag:mytag");
    expect(matchesQuery(makeEntry({ tag: "MyTag" }), tokens, NOW)).toBe(true);
  });

  it("excludes non-matching tag", () => {
    const tokens = parseQuery("tag:Other");
    expect(matchesQuery(makeEntry({ tag: "MyTag" }), tokens, NOW)).toBe(false);
  });

  it("negated tag excludes matches", () => {
    const tokens = parseQuery("-tag:system");
    expect(matchesQuery(makeEntry({ tag: "SystemUI" }), tokens, NOW)).toBe(false);
    expect(matchesQuery(makeEntry({ tag: "MyApp" }), tokens, NOW)).toBe(true);
  });

  it("regex tag matching", () => {
    const tokens = parseQuery("tag~:My.*Tag");
    expect(matchesQuery(makeEntry({ tag: "MyCustomTag" }), tokens, NOW)).toBe(true);
    expect(matchesQuery(makeEntry({ tag: "OtherTag" }), tokens, NOW)).toBe(false);
  });

  it("falls back to substring on invalid regex", () => {
    const tokens = parseQuery("tag~:[invalid");
    // Should not throw — falls back to substring
    expect(() => matchesQuery(makeEntry({ tag: "[invalid" }), tokens, NOW)).not.toThrow();
  });
});

describe("matchesQuery — message filter", () => {
  it("matches message substring", () => {
    const tokens = parseQuery("message:world");
    expect(matchesQuery(makeEntry({ message: "Hello world" }), tokens, NOW)).toBe(true);
  });

  it("regex message matching", () => {
    const tokens = parseQuery("message~:Null.*Exception");
    expect(matchesQuery(makeEntry({ message: "NullPointerException: ..." }), tokens, NOW)).toBe(true);
    expect(matchesQuery(makeEntry({ message: "Hello" }), tokens, NOW)).toBe(false);
  });
});

describe("matchesQuery — package filter", () => {
  it("matches package field", () => {
    const tokens = parseQuery("package:com.example");
    expect(matchesQuery(makeEntry({ package: "com.example.myapp" }), tokens, NOW)).toBe(true);
    expect(matchesQuery(makeEntry({ package: "com.other.app" }), tokens, NOW)).toBe(false);
  });

  it("falls back to tag when no package", () => {
    const tokens = parseQuery("package:MyTag");
    expect(matchesQuery(makeEntry({ package: null, tag: "MyTag" }), tokens, NOW)).toBe(true);
  });

  it("resolves package:mine using setMinePackage", () => {
    setMinePackage("com.myproject.app");
    const tokens = parseQuery("package:mine");
    expect(matchesQuery(makeEntry({ package: "com.myproject.app" }), tokens, NOW)).toBe(true);
    expect(matchesQuery(makeEntry({ package: "com.other.app" }), tokens, NOW)).toBe(false);
    setMinePackage(null);
  });

  it("falls back to literal 'mine' when setMinePackage is null", () => {
    setMinePackage(null);
    const tokens = parseQuery("package:mine");
    expect(matchesQuery(makeEntry({ package: "mine-corp.app" }), tokens, NOW)).toBe(true);
    expect(matchesQuery(makeEntry({ package: "com.example" }), tokens, NOW)).toBe(false);
  });

  // ── Startup race: null → resolved transition ─────────────────────────────────
  //
  // LogcatPanel.onMount may evaluate the filter before doOpenProject() finishes
  // calling setMinePackage(). The token matching must re-evaluate dynamically —
  // not capture the value at parse time — so the same token set works correctly
  // once setMinePackage is called.

  it("re-evaluates correctly when minePackage transitions from null to a resolved value", () => {
    setMinePackage(null);
    const tokens = parseQuery("package:mine");

    // At startup: _minePackage is null, so "mine" is treated as a literal string.
    // The project entry does NOT contain the word "mine" in its package name.
    expect(matchesQuery(makeEntry({ package: "com.example.app" }), tokens, NOW)).toBe(false);

    // Once doOpenProject() resolves and calls setMinePackage:
    setMinePackage("com.example.app");

    // The same parsed tokens now resolve correctly.
    expect(matchesQuery(makeEntry({ package: "com.example.app" }), tokens, NOW)).toBe(true);
    expect(matchesQuery(makeEntry({ package: "com.other.app" }), tokens, NOW)).toBe(false);

    setMinePackage(null);
  });

  it("re-evaluates correctly when minePackage changes on project switch", () => {
    setMinePackage("com.project-a.app");
    const tokens = parseQuery("package:mine");

    expect(matchesQuery(makeEntry({ package: "com.project-a.app" }), tokens, NOW)).toBe(true);
    expect(matchesQuery(makeEntry({ package: "com.project-b.app" }), tokens, NOW)).toBe(false);

    // User switches to project B — setMinePackage is called with the new id.
    setMinePackage("com.project-b.app");

    expect(matchesQuery(makeEntry({ package: "com.project-b.app" }), tokens, NOW)).toBe(true);
    expect(matchesQuery(makeEntry({ package: "com.project-a.app" }), tokens, NOW)).toBe(false);

    setMinePackage(null);
  });
});

describe("matchesQuery — age filter", () => {
  // Timestamp is "01-23 12:34:56.789", NOW is 4 seconds later.
  it("keeps entry within age window", () => {
    const tokens = parseQuery("age:5m"); // 5 minutes
    expect(matchesQuery(makeEntry(), tokens, NOW)).toBe(true);
  });

  it("excludes entry outside age window", () => {
    // Entry is 4 seconds old; age:2s should exclude it
    const tokens = parseQuery("age:2s");
    expect(matchesQuery(makeEntry(), tokens, NOW)).toBe(false);
  });

  it("keeps entry if timestamp is unparseable", () => {
    const tokens = parseQuery("age:1s");
    expect(matchesQuery(makeEntry({ timestamp: "bad-timestamp" }), tokens, NOW)).toBe(true);
  });
});

describe("matchesQuery — is filter", () => {
  it("is:crash matches crash entries", () => {
    const tokens = parseQuery("is:crash");
    expect(matchesQuery(makeEntry({ isCrash: true }), tokens, NOW)).toBe(true);
    expect(matchesQuery(makeEntry({ isCrash: false }), tokens, NOW)).toBe(false);
  });

  it("is:stacktrace matches stack trace lines", () => {
    const tokens = parseQuery("is:stacktrace");
    expect(matchesQuery(makeEntry({ message: "  at com.example.Foo.bar(Foo.kt:42)" }), tokens, NOW)).toBe(true);
    expect(matchesQuery(makeEntry({ message: "Caused by: java.lang.NullPointerException" }), tokens, NOW)).toBe(true);
    expect(matchesQuery(makeEntry({ message: "Hello world" }), tokens, NOW)).toBe(false);
  });
});

describe("matchesQuery — freetext filter", () => {
  it("matches in tag", () => {
    const tokens = parseQuery("MyTag");
    expect(matchesQuery(makeEntry({ tag: "MyTag" }), tokens, NOW)).toBe(true);
  });

  it("matches in message", () => {
    const tokens = parseQuery("world");
    expect(matchesQuery(makeEntry({ message: "Hello world" }), tokens, NOW)).toBe(true);
  });

  it("matches in package", () => {
    const tokens = parseQuery("example");
    expect(matchesQuery(makeEntry({ package: "com.example.app" }), tokens, NOW)).toBe(true);
  });

  it("negated freetext excludes matching", () => {
    const tokens = parseQuery("-system");
    expect(matchesQuery(makeEntry({ tag: "SystemUI" }), tokens, NOW)).toBe(false);
    expect(matchesQuery(makeEntry({ tag: "MyApp" }), tokens, NOW)).toBe(true);
  });
});

describe("matchesQuery — separator bypass", () => {
  it("separator entries bypass all filters except age", () => {
    const tokens = parseQuery("level:error -tag:---");
    const sep = makeEntry({ kind: "processDied", level: "info", tag: "---" });
    expect(matchesQuery(sep, tokens, NOW)).toBe(true);
  });

  it("separator entries are filtered by age", () => {
    const tokens = parseQuery("age:2s"); // 2 second window, entry is 4s old
    const sep = makeEntry({ kind: "processStarted" });
    expect(matchesQuery(sep, tokens, NOW)).toBe(false);
  });
});

describe("matchesQuery — all tokens must match (AND logic)", () => {
  it("requires all tokens to match", () => {
    const tokens = parseQuery("level:error tag:MyTag");
    expect(matchesQuery(makeEntry({ level: "error", tag: "MyTag" }), tokens, NOW)).toBe(true);
    expect(matchesQuery(makeEntry({ level: "debug", tag: "MyTag" }), tokens, NOW)).toBe(false);
    expect(matchesQuery(makeEntry({ level: "error", tag: "Other" }), tokens, NOW)).toBe(false);
  });
});

// ── parseLogcatTimestamp ──────────────────────────────────────────────────────

describe("parseLogcatTimestamp", () => {
  it("parses a valid timestamp", () => {
    const t = parseLogcatTimestamp("01-23 12:34:56.789");
    expect(t).toBeGreaterThan(0);
    const d = new Date(t);
    expect(d.getMonth()).toBe(0); // January
    expect(d.getDate()).toBe(23);
    expect(d.getHours()).toBe(12);
    expect(d.getMinutes()).toBe(34);
  });

  it("returns 0 for invalid timestamp", () => {
    expect(parseLogcatTimestamp("bad")).toBe(0);
    expect(parseLogcatTimestamp("")).toBe(0);
  });
});

// ── setAgeInQuery ─────────────────────────────────────────────────────────────

describe("setAgeInQuery", () => {
  it("adds age to empty query", () => {
    expect(setAgeInQuery("", "5m")).toBe("age:5m");
  });

  it("appends age to existing query", () => {
    expect(setAgeInQuery("level:error", "5m")).toBe("level:error age:5m");
  });

  it("replaces existing age token", () => {
    expect(setAgeInQuery("level:error age:1h", "5m")).toBe("level:error age:5m");
  });

  it("removes age when null passed", () => {
    expect(setAgeInQuery("level:error age:5m", null)).toBe("level:error");
  });

  it("returns empty string when only age is removed", () => {
    expect(setAgeInQuery("age:5m", null)).toBe("");
  });
});

// ── getActiveTokenContext ─────────────────────────────────────────────────────

describe("getActiveTokenContext", () => {
  it("returns null key for empty query", () => {
    const ctx = getActiveTokenContext("");
    expect(ctx.key).toBeNull();
    expect(ctx.partial).toBe("");
  });

  it("detects key after colon", () => {
    const ctx = getActiveTokenContext("level:err");
    expect(ctx.key).toBe("level");
    expect(ctx.partial).toBe("err");
  });

  it("detects tag key", () => {
    const ctx = getActiveTokenContext("level:error tag:My");
    expect(ctx.key).toBe("tag");
    expect(ctx.partial).toBe("My");
  });

  it("returns null key for bare text", () => {
    const ctx = getActiveTokenContext("com.example");
    expect(ctx.key).toBeNull();
    expect(ctx.partial).toBe("com.example");
  });

  it("strips regex suffix from key name", () => {
    const ctx = getActiveTokenContext("tag~:My");
    expect(ctx.key).toBe("tag");
  });
});

// ── isStackTraceLine ──────────────────────────────────────────────────────────

describe("isStackTraceLine", () => {
  it("detects 'at ' prefix", () => {
    expect(isStackTraceLine("  at com.example.Foo.bar(Foo.kt:42)")).toBe(true);
  });
  it("detects 'Caused by:'", () => {
    expect(isStackTraceLine("Caused by: java.lang.NullPointerException")).toBe(true);
  });
  it("detects '... N more'", () => {
    expect(isStackTraceLine("  ... 5 more")).toBe(true);
  });
  it("does not match normal messages", () => {
    expect(isStackTraceLine("Hello world")).toBe(false);
    expect(isStackTraceLine("Error initializing app")).toBe(false);
  });
});

// ── setPackageInQuery ─────────────────────────────────────────────────────────

describe("setPackageInQuery", () => {
  it("adds package to empty query", () => {
    expect(setPackageInQuery("", "com.example")).toBe("package:com.example");
  });

  it("appends package token to existing query", () => {
    expect(setPackageInQuery("level:error", "com.example")).toBe("level:error package:com.example");
  });

  it("replaces existing package token", () => {
    expect(setPackageInQuery("level:error package:com.old", "com.new")).toBe("level:error package:com.new");
  });

  it("removes package token when null is passed", () => {
    expect(setPackageInQuery("level:error package:com.example", null)).toBe("level:error");
  });

  it("returns empty string when only the package token is removed", () => {
    expect(setPackageInQuery("package:com.example", null)).toBe("");
  });

  it("supports the 'mine' shorthand", () => {
    expect(setPackageInQuery("", "mine")).toBe("package:mine");
  });

  it("does not disturb other tokens when replacing", () => {
    expect(setPackageInQuery("level:warn package:com.old age:5m", "com.new")).toBe(
      "level:warn age:5m package:com.new"
    );
  });

  it("composes correctly with setAgeInQuery (order-independent)", () => {
    const withPkg = setPackageInQuery("", "com.example");
    const withBoth = setAgeInQuery(withPkg, "5m");
    expect(withBoth).toContain("package:com.example");
    expect(withBoth).toContain("age:5m");
    // Round-trip: removing both leaves empty
    const cleared = setPackageInQuery(setAgeInQuery(withBoth, null), null);
    expect(cleared).toBe("");
  });
});

// ── getPackageFromQuery ───────────────────────────────────────────────────────

describe("getPackageFromQuery", () => {
  it("returns null for empty query", () => {
    expect(getPackageFromQuery("")).toBeNull();
  });

  it("returns null when no package token present", () => {
    expect(getPackageFromQuery("level:error tag:MyTag")).toBeNull();
  });

  it("extracts the package value", () => {
    expect(getPackageFromQuery("package:com.example")).toBe("com.example");
  });

  it("extracts package from a multi-token query", () => {
    expect(getPackageFromQuery("level:error package:com.example age:5m")).toBe("com.example");
  });

  it("returns 'mine' for the mine shorthand", () => {
    expect(getPackageFromQuery("package:mine")).toBe("mine");
  });

  it("is consistent with setPackageInQuery round-trip", () => {
    const q = setPackageInQuery("level:error", "com.roundtrip");
    expect(getPackageFromQuery(q)).toBe("com.roundtrip");
  });

  it("returns null after package is removed", () => {
    const q = setPackageInQuery("package:com.example", null);
    expect(getPackageFromQuery(q)).toBeNull();
  });
});

// ── parseFilterGroups ─────────────────────────────────────────────────────────

describe("parseFilterGroups", () => {
  it("returns single empty group for empty string", () => {
    const groups = parseFilterGroups("");
    expect(groups).toHaveLength(1);
    expect(groups[0]).toHaveLength(0);
  });

  it("returns single empty group for whitespace only", () => {
    const groups = parseFilterGroups("   ");
    expect(groups).toHaveLength(1);
    expect(groups[0]).toHaveLength(0);
  });

  it("returns single group when no pipe present", () => {
    const groups = parseFilterGroups("level:error tag:MyTag");
    expect(groups).toHaveLength(1);
    expect(groups[0]).toHaveLength(2);
    expect(groups[0][0].type).toBe("level");
    expect(groups[0][1].type).toBe("tag");
  });

  it("splits on pipe into two groups", () => {
    const groups = parseFilterGroups("level:error | is:crash");
    expect(groups).toHaveLength(2);
    expect(groups[0][0]).toMatchObject({ type: "level", value: "error" });
    expect(groups[1][0]).toMatchObject({ type: "is", value: "crash" });
  });

  it("splits on pipe into three groups", () => {
    const groups = parseFilterGroups("level:error | is:crash | tag:System");
    expect(groups).toHaveLength(3);
    expect(groups[2][0]).toMatchObject({ type: "tag", value: "System" });
  });

  it("handles pipe without surrounding spaces", () => {
    const groups = parseFilterGroups("level:error|is:crash");
    expect(groups).toHaveLength(2);
  });

  it("handles multiple spaces around pipe", () => {
    const groups = parseFilterGroups("level:error   |   is:crash");
    expect(groups).toHaveLength(2);
    expect(groups[0][0]).toMatchObject({ type: "level" });
    expect(groups[1][0]).toMatchObject({ type: "is" });
  });

  it("each group can have multiple tokens", () => {
    const groups = parseFilterGroups("level:error tag:App | level:warn tag:System");
    expect(groups[0]).toHaveLength(2);
    expect(groups[1]).toHaveLength(2);
  });

  it("preserves negation within groups", () => {
    const groups = parseFilterGroups("-tag:system | level:error");
    expect(groups[0][0]).toMatchObject({ type: "tag", negate: true });
  });

  it("trailing pipe produces only the non-empty group (no match-everything bug)", () => {
    // "level:error | " would previously produce [[level:error], []]
    // The empty group matched everything via matchesQuery(entry, [], now) === true
    const groups = parseFilterGroups("level:error | ");
    expect(groups).toHaveLength(1);
    expect(groups[0][0]).toMatchObject({ type: "level", value: "error" });
  });

  it("double pipe produces only the non-empty groups", () => {
    const groups = parseFilterGroups("level:error || is:crash");
    expect(groups).toHaveLength(2);
    expect(groups[0][0]).toMatchObject({ type: "level" });
    expect(groups[1][0]).toMatchObject({ type: "is" });
  });

  it("all-pipe query (no real tokens) returns single empty group", () => {
    const groups = parseFilterGroups(" | | ");
    expect(groups).toHaveLength(1);
    expect(groups[0]).toHaveLength(0);
  });
});

// ── matchesFilterGroups ───────────────────────────────────────────────────────

describe("matchesFilterGroups — empty groups", () => {
  it("returns true for empty groups array", () => {
    expect(matchesFilterGroups(makeEntry(), [], NOW)).toBe(true);
  });

  it("returns true for single empty group", () => {
    expect(matchesFilterGroups(makeEntry(), [[]], NOW)).toBe(true);
  });
});

describe("matchesFilterGroups — single group (backward compat)", () => {
  it("matches when single group matches", () => {
    const groups = parseFilterGroups("level:error");
    expect(matchesFilterGroups(makeEntry({ level: "error" }), groups, NOW)).toBe(true);
  });

  it("rejects when single group does not match", () => {
    const groups = parseFilterGroups("level:error");
    expect(matchesFilterGroups(makeEntry({ level: "debug" }), groups, NOW)).toBe(false);
  });
});

describe("matchesFilterGroups — OR semantics", () => {
  it("returns true when only the first group matches", () => {
    const groups = parseFilterGroups("tag:MyTag | tag:Other");
    const entry = makeEntry({ tag: "MyTag", level: "debug" });
    expect(matchesFilterGroups(entry, groups, NOW)).toBe(true);
  });

  it("returns true when only the second group matches", () => {
    const groups = parseFilterGroups("tag:NoMatch | tag:MyTag");
    const entry = makeEntry({ tag: "MyTag" });
    expect(matchesFilterGroups(entry, groups, NOW)).toBe(true);
  });

  it("returns true when both groups match", () => {
    const groups = parseFilterGroups("level:debug | tag:MyTag");
    const entry = makeEntry({ level: "debug", tag: "MyTag" });
    expect(matchesFilterGroups(entry, groups, NOW)).toBe(true);
  });

  it("returns false when no group matches", () => {
    const groups = parseFilterGroups("tag:NoMatch | level:fatal");
    const entry = makeEntry({ tag: "MyTag", level: "debug" });
    expect(matchesFilterGroups(entry, groups, NOW)).toBe(false);
  });

  it("AND logic within each group still applies", () => {
    // Group 1: level:error AND tag:MyTag — entry has error but wrong tag
    // Group 2: level:fatal — entry is not fatal
    const groups = parseFilterGroups("level:error tag:WrongTag | level:fatal");
    const entry = makeEntry({ level: "error", tag: "MyTag" });
    expect(matchesFilterGroups(entry, groups, NOW)).toBe(false);
  });

  it("three groups — matches last group only", () => {
    const groups = parseFilterGroups("tag:A | tag:B | tag:MyTag");
    const entry = makeEntry({ tag: "MyTag" });
    expect(matchesFilterGroups(entry, groups, NOW)).toBe(true);
  });
});

describe("matchesFilterGroups — separator bypass with OR groups", () => {
  it("separator entries bypass all filters in each group", () => {
    const groups = parseFilterGroups("level:error tag:App | tag:System");
    const sep = makeEntry({ kind: "processDied", level: "info", tag: "---" });
    expect(matchesFilterGroups(sep, groups, NOW)).toBe(true);
  });

  it("separator entries respect age even with OR groups", () => {
    // Entry is 4 seconds old; age:2s should exclude it
    const groups = parseFilterGroups("age:2s | level:error");
    const sep = makeEntry({ kind: "processStarted" });
    // age:2s rejects (entry too old), level:error also rejected for separator
    // but separator bypass means group2 returns true (no age token)
    expect(matchesFilterGroups(sep, groups, NOW)).toBe(true);
  });

  it("separator excluded only when ALL groups have age that excludes it", () => {
    const groups = parseFilterGroups("age:2s | age:2s");
    const sep = makeEntry({ kind: "processDied" });
    expect(matchesFilterGroups(sep, groups, NOW)).toBe(false);
  });
});

describe("matchesFilterGroups — age token in one group only", () => {
  it("entry inside age window passes the group with age", () => {
    // Entry is 4 seconds old; age:5m keeps it; second group requires fatal
    const groups = parseFilterGroups("age:5m | level:fatal");
    const entry = makeEntry({ level: "debug" }); // within 5m, but not fatal
    // group1 (age:5m) → matches; group2 (level:fatal) → does not
    expect(matchesFilterGroups(entry, groups, NOW)).toBe(true);
  });

  it("entry outside age window still matches via other group", () => {
    // Entry is 4 seconds old; age:2s excludes it in group1, but group2 has no restriction
    const groups = parseFilterGroups("age:2s tag:Unreachable | tag:MyTag");
    const entry = makeEntry({ tag: "MyTag" });
    expect(matchesFilterGroups(entry, groups, NOW)).toBe(true);
  });
});

// ── addOrGroup ────────────────────────────────────────────────────────────────

describe("addOrGroup", () => {
  it("appends ' | ' to a non-empty query", () => {
    expect(addOrGroup("level:error")).toBe("level:error | ");
  });

  it("does nothing to an empty string", () => {
    expect(addOrGroup("")).toBe("");
  });

  it("does nothing to whitespace-only string", () => {
    const result = addOrGroup("   ");
    expect(result.trim()).toBe("");
  });

  it("does not double-append if query already ends with |", () => {
    expect(addOrGroup("level:error | ")).toBe("level:error | ");
  });

  it("trims trailing spaces before appending", () => {
    expect(addOrGroup("level:error   ")).toBe("level:error | ");
  });
});

// ── getGroupCount ─────────────────────────────────────────────────────────────

describe("getGroupCount", () => {
  it("returns 0 for empty string", () => {
    expect(getGroupCount("")).toBe(0);
  });

  it("returns 1 for a query with no pipe", () => {
    expect(getGroupCount("level:error tag:App")).toBe(1);
  });

  it("returns 2 for a query with one pipe", () => {
    expect(getGroupCount("level:error | is:crash")).toBe(2);
  });

  it("returns 3 for two pipes", () => {
    expect(getGroupCount("a | b | c")).toBe(3);
  });
});

// ── getActiveGroupSegment ─────────────────────────────────────────────────────

describe("getActiveGroupSegment", () => {
  it("returns full query when no pipe", () => {
    expect(getActiveGroupSegment("level:error")).toBe("level:error");
  });

  it("returns text after last pipe", () => {
    expect(getActiveGroupSegment("level:error | tag:App")).toBe("tag:App");
  });

  it("returns empty string when query ends with pipe", () => {
    const seg = getActiveGroupSegment("level:error | ");
    expect(seg.trim()).toBe("");
  });

  it("respects cursor position — returns segment before cursor pipe boundary", () => {
    const query = "level:error | tag:App";
    // cursor at position 12 (within first group)
    expect(getActiveGroupSegment(query, 12)).toBe("level:error");
  });

  it("strips leading whitespace from the returned segment", () => {
    expect(getActiveGroupSegment("level:error |   tag:App")).toBe("tag:App");
  });
});

// ── getActiveGroupOffset ──────────────────────────────────────────────────────

describe("getActiveGroupOffset", () => {
  it("returns 0 when no pipe in query", () => {
    expect(getActiveGroupOffset("level:error")).toBe(0);
  });

  it("returns offset after pipe and whitespace", () => {
    // "level:error | tag:App"
    //  0123456789012345678901
    //              ^13 = after '| '
    const query = "level:error | tag:App";
    expect(getActiveGroupOffset(query)).toBe(14);
  });

  it("returns offset correctly with no space after pipe", () => {
    const query = "level:error|tag:App";
    expect(getActiveGroupOffset(query)).toBe(12);
  });
});

// ── AND connector (&&) ────────────────────────────────────────────────────────

describe("parseQuery — && AND connector", () => {
  it("treats standalone && as whitespace (skipped)", () => {
    const tokens = parseQuery("level:error && tag:App");
    expect(tokens).toHaveLength(2);
    expect(tokens[0]).toMatchObject({ type: "level", value: "error" });
    expect(tokens[1]).toMatchObject({ type: "tag", value: "App" });
  });

  it("treats standalone & as whitespace (skipped)", () => {
    const tokens = parseQuery("level:error & tag:App");
    expect(tokens).toHaveLength(2);
    expect(tokens[0]).toMatchObject({ type: "level" });
    expect(tokens[1]).toMatchObject({ type: "tag" });
  });

  it("produces identical tokens to space-separated query", () => {
    const withAnd = parseQuery("level:error && tag:App && age:5m");
    const withSpaces = parseQuery("level:error tag:App age:5m");
    expect(withAnd).toEqual(withSpaces);
  });

  it("preserves && embedded inside a token value", () => {
    // "tag:a&&b" is a single token — && is not standalone
    const tokens = parseQuery("tag:a&&b");
    expect(tokens).toHaveLength(1);
    expect(tokens[0]).toMatchObject({ type: "tag", value: "a&&b" });
  });

  it("preserves freetext containing &&", () => {
    const tokens = parseQuery("a&&b");
    expect(tokens).toHaveLength(1);
    expect(tokens[0]).toMatchObject({ type: "freetext", value: "a&&b" });
  });

  it("handles leading/trailing && gracefully", () => {
    const tokens = parseQuery("&& level:error &&");
    expect(tokens).toHaveLength(1);
    expect(tokens[0]).toMatchObject({ type: "level", value: "error" });
  });

  it("handles multiple consecutive && connectors", () => {
    const tokens = parseQuery("level:error && && tag:App");
    expect(tokens).toHaveLength(2);
  });

  it("works with negation: -tag:system && level:warn", () => {
    const tokens = parseQuery("-tag:system && level:warn");
    expect(tokens).toHaveLength(2);
    expect(tokens[0]).toMatchObject({ type: "tag", negate: true });
    expect(tokens[1]).toMatchObject({ type: "level", value: "warn" });
  });

  it("works inside OR groups: level:error && tag:App | is:crash", () => {
    const groups = parseFilterGroups("level:error && tag:App | is:crash");
    expect(groups).toHaveLength(2);
    expect(groups[0]).toHaveLength(2);
    expect(groups[0][0]).toMatchObject({ type: "level" });
    expect(groups[0][1]).toMatchObject({ type: "tag" });
    expect(groups[1]).toHaveLength(1);
    expect(groups[1][0]).toMatchObject({ type: "is" });
  });
});

describe("matchesQuery — && AND connector (same as space)", () => {
  it("matches when both AND conditions are satisfied", () => {
    const tokens = parseQuery("level:error && tag:MyTag");
    expect(matchesQuery(makeEntry({ level: "error", tag: "MyTag" }), tokens, NOW)).toBe(true);
  });

  it("rejects when one AND condition is not satisfied", () => {
    const tokens = parseQuery("level:error && tag:MyTag");
    expect(matchesQuery(makeEntry({ level: "debug", tag: "MyTag" }), tokens, NOW)).toBe(false);
  });
});

// ── addAndConnector ───────────────────────────────────────────────────────────

describe("addAndConnector", () => {
  it("appends ' && ' to a non-empty query", () => {
    expect(addAndConnector("level:error")).toBe("level:error && ");
  });

  it("does nothing to an empty string", () => {
    expect(addAndConnector("")).toBe("");
  });

  it("does nothing if query already ends with &&", () => {
    expect(addAndConnector("level:error && ")).toBe("level:error && ");
  });

  it("does nothing if query ends with |", () => {
    expect(addAndConnector("level:error | ")).toBe("level:error | ");
  });

  it("trims trailing spaces before appending", () => {
    expect(addAndConnector("level:error   ")).toBe("level:error && ");
  });

  it("does not append if query ends with bare &", () => {
    expect(addAndConnector("level:error &")).toBe("level:error &");
  });
});

// ── getActiveTokenContext — AND connector edge cases ──────────────────────────

describe("getActiveTokenContext — AND connector handling", () => {
  it("returns empty partial when query ends with &&", () => {
    const ctx = getActiveTokenContext("level:error &&");
    expect(ctx.key).toBeNull();
    expect(ctx.partial).toBe("");
  });

  it("returns empty partial when query ends with && and space", () => {
    const ctx = getActiveTokenContext("level:error && ");
    expect(ctx.key).toBeNull();
    expect(ctx.partial).toBe("");
  });

  it("returns empty partial when query ends with bare &", () => {
    const ctx = getActiveTokenContext("level:error &");
    expect(ctx.key).toBeNull();
    expect(ctx.partial).toBe("");
  });

  it("continues detecting token after && with typed partial", () => {
    // After typing "level:error && tag:" the cursor is completing a value
    const ctx = getActiveTokenContext("level:error && tag:App");
    expect(ctx.key).toBe("tag");
    expect(ctx.partial).toBe("App");
  });

  it("detects key suggestion after && space", () => {
    const ctx = getActiveTokenContext("level:error && lev");
    expect(ctx.key).toBeNull();
    expect(ctx.partial).toBe("lev");
  });
});

// ── getFrontendOnlyTokens ─────────────────────────────────────────────────────

describe("getFrontendOnlyTokens — always-frontend types", () => {
  it("returns age tokens", () => {
    const tokens = parseQuery("age:5m");
    expect(getFrontendOnlyTokens(tokens)).toHaveLength(1);
    expect(getFrontendOnlyTokens(tokens)[0].type).toBe("age");
  });

  it("returns negated tokens", () => {
    const tokens = parseQuery("-tag:system");
    expect(getFrontendOnlyTokens(tokens)).toHaveLength(1);
  });

  it("returns regex tag tokens", () => {
    const tokens = parseQuery("tag~:My.*Tag");
    expect(getFrontendOnlyTokens(tokens)).toHaveLength(1);
  });

  it("returns regex message tokens", () => {
    const tokens = parseQuery("message~:Null.*Ex");
    expect(getFrontendOnlyTokens(tokens)).toHaveLength(1);
  });

  it("returns is:stacktrace (backend has no stacktrace filter)", () => {
    const tokens = parseQuery("is:stacktrace");
    expect(getFrontendOnlyTokens(tokens)).toHaveLength(1);
    expect(getFrontendOnlyTokens(tokens)[0]).toMatchObject({ type: "is", value: "stacktrace" });
  });
});

describe("getFrontendOnlyTokens — single backend-handled tokens (no overflow)", () => {
  it("first level: goes to backend → not returned", () => {
    const tokens = parseQuery("level:error");
    expect(getFrontendOnlyTokens(tokens)).toHaveLength(0);
  });

  it("first tag: goes to backend → not returned", () => {
    const tokens = parseQuery("tag:OkHttp");
    expect(getFrontendOnlyTokens(tokens)).toHaveLength(0);
  });

  it("first message: goes to backend → not returned", () => {
    const tokens = parseQuery("message:socket");
    expect(getFrontendOnlyTokens(tokens)).toHaveLength(0);
  });

  it("first package: goes to backend → not returned", () => {
    const tokens = parseQuery("package:com.example");
    expect(getFrontendOnlyTokens(tokens)).toHaveLength(0);
  });

  it("is:crash goes to backend onlyCrashes flag → not returned", () => {
    const tokens = parseQuery("is:crash");
    expect(getFrontendOnlyTokens(tokens)).toHaveLength(0);
  });

  it("first freetext goes to backend text slot → not returned", () => {
    const tokens = parseQuery("login");
    expect(getFrontendOnlyTokens(tokens)).toHaveLength(0);
  });
});

describe("getFrontendOnlyTokens — overflow (same type, second+ occurrence)", () => {
  it("second message: is returned (overflow — text slot already taken)", () => {
    const tokens = parseQuery("message:socket message:IPPROTO_TCP");
    const fe = getFrontendOnlyTokens(tokens);
    expect(fe).toHaveLength(1);
    expect(fe[0]).toMatchObject({ type: "message", value: "IPPROTO_TCP" });
  });

  it("third message: is returned too", () => {
    const tokens = parseQuery("message:A message:B message:C");
    const fe = getFrontendOnlyTokens(tokens);
    expect(fe).toHaveLength(2);
    expect(fe[0]).toMatchObject({ type: "message", value: "B" });
    expect(fe[1]).toMatchObject({ type: "message", value: "C" });
  });

  it("second tag: is returned (overflow)", () => {
    const tokens = parseQuery("tag:OkHttp tag:Retrofit");
    const fe = getFrontendOnlyTokens(tokens);
    expect(fe).toHaveLength(1);
    expect(fe[0]).toMatchObject({ type: "tag", value: "Retrofit" });
  });

  it("second level: is returned (overflow)", () => {
    const tokens = parseQuery("level:warn level:error");
    const fe = getFrontendOnlyTokens(tokens);
    expect(fe).toHaveLength(1);
    expect(fe[0]).toMatchObject({ type: "level", value: "error" });
  });

  it("second package: is returned (overflow)", () => {
    const tokens = parseQuery("package:com.a package:com.b");
    const fe = getFrontendOnlyTokens(tokens);
    expect(fe).toHaveLength(1);
    expect(fe[0]).toMatchObject({ type: "package", value: "com.b" });
  });

  it("second freetext is returned (overflow — shares text slot with message:)", () => {
    const tokens = parseQuery("hello world");
    const fe = getFrontendOnlyTokens(tokens);
    expect(fe).toHaveLength(1);
    expect(fe[0]).toMatchObject({ type: "freetext", value: "world" });
  });

  it("freetext after message: is returned (overflow — same text slot)", () => {
    const tokens = parseQuery("message:socket login");
    const fe = getFrontendOnlyTokens(tokens);
    expect(fe).toHaveLength(1);
    expect(fe[0]).toMatchObject({ type: "freetext", value: "login" });
  });

  it("message: after freetext is returned (overflow)", () => {
    const tokens = parseQuery("login message:socket");
    const fe = getFrontendOnlyTokens(tokens);
    expect(fe).toHaveLength(1);
    expect(fe[0]).toMatchObject({ type: "message", value: "socket" });
  });
});

describe("getFrontendOnlyTokens — mixed queries", () => {
  it("complex query: only overflow and special tokens are returned", () => {
    // level:error → backend (no overflow)
    // tag:OkHttp  → backend (no overflow)
    // message:socket → backend (no overflow)
    // message:IPPROTO_TCP → frontend (overflow)
    // -tag:system → frontend (negated)
    const tokens = parseQuery("level:error tag:OkHttp message:socket message:IPPROTO_TCP -tag:system");
    const fe = getFrontendOnlyTokens(tokens);
    expect(fe).toHaveLength(2);
    expect(fe[0]).toMatchObject({ type: "message", value: "IPPROTO_TCP" });
    expect(fe[1]).toMatchObject({ type: "tag", value: "system", negate: true });
  });

  it("age + overflow: both returned", () => {
    const tokens = parseQuery("age:5m message:socket message:IPPROTO_TCP");
    const fe = getFrontendOnlyTokens(tokens);
    expect(fe).toHaveLength(2);
    expect(fe.find((t) => t.type === "age")).toBeDefined();
    expect(fe.find((t) => t.type === "message")).toBeDefined();
  });

  it("no frontend-only tokens for a simple single-condition query", () => {
    expect(getFrontendOnlyTokens(parseQuery("level:error"))).toHaveLength(0);
    expect(getFrontendOnlyTokens(parseQuery("tag:App"))).toHaveLength(0);
    expect(getFrontendOnlyTokens(parseQuery("is:crash"))).toHaveLength(0);
    expect(getFrontendOnlyTokens(parseQuery("package:mine"))).toHaveLength(0);
  });
});

describe("matchesQuery — AND semantics for same-type tokens (the key correctness property)", () => {
  it("message:socket AND message:IPPROTO_TCP requires both in message", () => {
    const tokens = parseQuery("message:socket message:IPPROTO_TCP");
    const both = makeEntry({ message: "connect socket IPPROTO_TCP ok" });
    const onlyFirst = makeEntry({ message: "socket established" });
    const onlySecond = makeEntry({ message: "IPPROTO_TCP header" });
    const neither = makeEntry({ message: "unrelated log" });
    expect(matchesQuery(both, tokens, NOW)).toBe(true);
    expect(matchesQuery(onlyFirst, tokens, NOW)).toBe(false);
    expect(matchesQuery(onlySecond, tokens, NOW)).toBe(false);
    expect(matchesQuery(neither, tokens, NOW)).toBe(false);
  });

  it("tag:A AND tag:B requires entry tag to contain both substrings", () => {
    const tokens = parseQuery("tag:Network tag:Client");
    expect(matchesQuery(makeEntry({ tag: "NetworkClient" }), tokens, NOW)).toBe(true);
    expect(matchesQuery(makeEntry({ tag: "NetworkLayer" }), tokens, NOW)).toBe(false);
    expect(matchesQuery(makeEntry({ tag: "HttpClient" }), tokens, NOW)).toBe(false);
  });

  it("three message: tokens all must be present", () => {
    const tokens = parseQuery("message:A message:B message:C");
    expect(matchesQuery(makeEntry({ message: "A B C here" }), tokens, NOW)).toBe(true);
    expect(matchesQuery(makeEntry({ message: "A B only" }), tokens, NOW)).toBe(false);
  });

  it("getFrontendOnlyTokens identifies the overflow that matchesQuery enforces", () => {
    const tokens = parseQuery("message:socket message:IPPROTO_TCP");
    const fe = getFrontendOnlyTokens(tokens);
    // The overflow token is the second message:
    expect(fe).toHaveLength(1);
    expect(fe[0]).toMatchObject({ type: "message", value: "IPPROTO_TCP" });
    // matchesQuery (used in filteredEntries) does verify both
    const passEntry = makeEntry({ message: "socket IPPROTO_TCP connected" });
    const failEntry = makeEntry({ message: "socket connected" });
    expect(matchesQuery(passEntry, tokens, NOW)).toBe(true);
    expect(matchesQuery(failEntry, tokens, NOW)).toBe(false);
  });
});

// ── Additional edge-case tests ─────────────────────────────────────────────────

describe("parseQuery — quoted multi-word token values", () => {
  it("produces a single token for a quoted tag value", () => {
    const tokens = parseQuery('tag:"hello world"');
    expect(tokens).toHaveLength(1);
    expect(tokens[0]).toMatchObject({ type: "tag", value: "hello world" });
  });

  it("produces a single token for a quoted message value with spaces", () => {
    const tokens = parseQuery('message:"null pointer exception"');
    expect(tokens).toHaveLength(1);
    expect(tokens[0]).toMatchObject({ type: "message", value: "null pointer exception" });
  });

  it("handles multiple quoted tokens correctly", () => {
    const tokens = parseQuery('tag:"Foo Bar" level:error');
    expect(tokens).toHaveLength(2);
    expect(tokens[0]).toMatchObject({ type: "tag", value: "Foo Bar" });
    expect(tokens[1]).toMatchObject({ type: "level", value: "error" });
  });
});

describe("parseQuery — double negation edge case", () => {
  it("double dash (--tag:foo) becomes freetext because -tag:foo has no key", () => {
    // `-` is stripped to get `p = "-tag:foo"`, which has a colonIdx at 4.
    // key = "-tag", which is not a recognised key → falls to freetext.
    const tokens = parseQuery("--tag:foo");
    // The outer - is negation; after stripping it we have "-tag:foo".
    // colonIdx finds ':' at index 4, key = "-tag" → unknown → freetext.
    expect(tokens).toHaveLength(1);
    expect(tokens[0]).toMatchObject({ type: "freetext", negate: true });
  });
});

describe("getFrontendOnlyTokens — negated freetext", () => {
  it("returns negated freetext as frontend-only (backend has no negation support)", () => {
    const tokens = parseQuery("-system");
    const fe = getFrontendOnlyTokens(tokens);
    expect(fe).toHaveLength(1);
    expect(fe[0]).toMatchObject({ type: "freetext", value: "system", negate: true });
  });

  it("non-negated freetext: first goes to backend, second is overflow (frontend)", () => {
    const tokens = parseQuery("login startup");
    // "login" → first freetext → backend text slot consumed
    // "startup" → second freetext → frontend overflow
    const fe = getFrontendOnlyTokens(tokens);
    expect(fe).toHaveLength(1);
    expect(fe[0]).toMatchObject({ type: "freetext", value: "startup" });
  });
});

describe("matchesFilterGroups — age is scoped per group", () => {
  // Entry timestamp is 4 seconds before NOW.
  // Group 1 has age:2s (too short — entry is excluded from group 1).
  // Group 2 has no age constraint (entry passes group 2).
  it("entry passes when age filter in one group excludes it but other group has no age", () => {
    const groups = parseFilterGroups("age:2s tag:Unreachable | tag:MyTag");
    // Group 1: age:2s (entry is 4s old → excluded) AND tag:Unreachable → fails group 1
    // Group 2: tag:MyTag → passes group 2
    const entry = makeEntry({ tag: "MyTag" });
    expect(matchesFilterGroups(entry, groups, NOW)).toBe(true);
  });

  it("entry excluded when age filter in every group excludes it", () => {
    const groups = parseFilterGroups("age:2s | age:2s");
    const entry = makeEntry();
    // Both groups have age:2s; entry is 4s old → excluded by both
    expect(matchesFilterGroups(entry, groups, NOW)).toBe(false);
  });

  it("trailing pipe after fix does not bypass age filter", () => {
    // After the trailing-pipe fix, "age:2s | " produces only [[age:2s]].
    // Entry is 4s old and should be excluded.
    const groups = parseFilterGroups("age:2s | ");
    expect(groups).toHaveLength(1);
    const entry = makeEntry();
    expect(matchesFilterGroups(entry, groups, NOW)).toBe(false);
  });
});

// ── parseStackFrame ────────────────────────────────────────────────────────────

describe("parseStackFrame", () => {
  it("parses a Kotlin frame with tab indent", () => {
    const f = parseStackFrame("\tat com.example.app.MainActivity.onCreate(MainActivity.kt:42)");
    expect(f).not.toBeNull();
    expect(f!.filename).toBe("MainActivity.kt");
    expect(f!.line).toBe(42);
    expect(f!.packagePath).toBe("com.example.app");
    expect(f!.classPath).toBe("com.example.app.MainActivity");
  });

  it("parses a Java frame with spaces indent", () => {
    const f = parseStackFrame("  at android.app.Activity.performCreate(Activity.java:8290)");
    expect(f).not.toBeNull();
    expect(f!.filename).toBe("Activity.java");
    expect(f!.line).toBe(8290);
    expect(f!.packagePath).toBe("android.app");
  });

  it("returns null for Caused by lines", () => {
    const f = parseStackFrame("Caused by: java.lang.NullPointerException");
    expect(f).toBeNull();
  });

  it("returns null for ... N more lines", () => {
    const f = parseStackFrame("\t... 5 more");
    expect(f).toBeNull();
  });

  it("returns null for non-frame lines", () => {
    const f = parseStackFrame("E/AndroidRuntime: FATAL EXCEPTION: main");
    expect(f).toBeNull();
  });
});

// ── isProjectFrame ────────────────────────────────────────────────────────────

describe("isProjectFrame", () => {
  it("returns true for a user package", () => {
    expect(isProjectFrame("com.example.app.MainActivity")).toBe(true);
    expect(isProjectFrame("io.mycompany.feature.SomeClass")).toBe(true);
  });

  it("returns false for android. prefix", () => {
    expect(isProjectFrame("android.app.Activity")).toBe(false);
    expect(isProjectFrame("android.view.View")).toBe(false);
  });

  it("returns false for androidx. prefix", () => {
    expect(isProjectFrame("androidx.lifecycle.ViewModel")).toBe(false);
  });

  it("returns false for com.android. prefix", () => {
    expect(isProjectFrame("com.android.internal.os.ZygoteInit")).toBe(false);
  });

  it("returns false for kotlin. and kotlinx. prefixes", () => {
    expect(isProjectFrame("kotlin.jvm.internal.Intrinsics")).toBe(false);
    expect(isProjectFrame("kotlinx.coroutines.CoroutineScope")).toBe(false);
  });

  it("returns false for java. and javax. prefixes", () => {
    expect(isProjectFrame("java.lang.Thread")).toBe(false);
    expect(isProjectFrame("javax.inject.Inject")).toBe(false);
  });

  it("returns false for com.google.android. prefix", () => {
    expect(isProjectFrame("com.google.android.gms.common.GoogleApiAvailability")).toBe(false);
  });
});