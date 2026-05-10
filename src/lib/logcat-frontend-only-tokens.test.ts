import { describe, expect, it } from "vitest";
import { getFrontendOnlyTokens } from "./logcat-frontend-only-tokens";
import { matchesQuery, parseQuery } from "./logcat-query";
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
    const tokens = parseQuery(
      "level:error tag:OkHttp message:socket message:IPPROTO_TCP -tag:system"
    );
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

describe("getFrontendOnlyTokens — integration with AND semantics", () => {
  it("identifies the overflow that matchesQuery enforces", () => {
    const tokens = parseQuery("message:socket message:IPPROTO_TCP");
    const fe = getFrontendOnlyTokens(tokens);
    expect(fe).toHaveLength(1);
    expect(fe[0]).toMatchObject({ type: "message", value: "IPPROTO_TCP" });

    const passEntry = makeEntry({ message: "socket IPPROTO_TCP connected" });
    const failEntry = makeEntry({ message: "socket connected" });
    expect(matchesQuery(passEntry, tokens, NOW)).toBe(true);
    expect(matchesQuery(failEntry, tokens, NOW)).toBe(false);
  });

  it("returns negated freetext as frontend-only (backend has no negation support)", () => {
    const tokens = parseQuery("-system");
    const fe = getFrontendOnlyTokens(tokens);
    expect(fe).toHaveLength(1);
    expect(fe[0]).toMatchObject({ type: "freetext", value: "system", negate: true });
  });

  it("non-negated freetext: first goes to backend, second is overflow (frontend)", () => {
    const tokens = parseQuery("login startup");
    const fe = getFrontendOnlyTokens(tokens);
    expect(fe).toHaveLength(1);
    expect(fe[0]).toMatchObject({ type: "freetext", value: "startup" });
  });
});
