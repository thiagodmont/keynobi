import { describe, it, expect } from "vitest";
import {
  parseQuery,
  matchesQuery,
  parseAge,
  parseLogcatTimestamp,
  setAgeInQuery,
  setPackageInQuery,
  getPackageFromQuery,
  getActiveTokenContext,
  setMinePackage,
  isStackTraceLine,
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
