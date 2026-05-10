import { describe, expect, it } from "vitest";
import {
  buildCommittedWithAndConnector,
  buildCommittedWithOrGroup,
  buildDraftAfterSuggestion,
  buildQuery,
  parseQueryState,
  removeLastCommittedPill,
} from "./querybar-query-state";
import {
  quoteMessageTokenForEditDraft,
  rebuildCommittedAfterRemovingPill,
  setAgeInQuery,
  setPackageInQuery,
} from "@/lib/logcat-query";

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
      committed: ["tag:hello world"],
      draft: "",
    });
  });

  it("quoted string without closing trailing space remains a draft", () => {
    expect(parseQueryState('tag:"hello world"')).toEqual({
      committed: [],
      draft: 'tag:"hello world"',
    });
  });

  it("message with quoted spaces is one committed pill when trailing space", () => {
    expect(parseQueryState('message:"hello world" ')).toEqual({
      committed: ["message:hello world"],
      draft: "",
    });
  });

  it("message:socket login splits (AND) without quotes", () => {
    expect(parseQueryState("message:socket login")).toEqual({
      committed: ["message:socket"],
      draft: "login",
    });
  });

  it("edit-pill flow: rebuild committed + buildQuery restores parse state", () => {
    const full = "level:error tag:App ";
    const { committed } = parseQueryState(full);
    const next = rebuildCommittedAfterRemovingPill(committed, 0, 1, false);
    const q = buildQuery(next, "tag:App");
    expect(parseQueryState(q)).toEqual({ committed: ["level:error"], draft: "tag:App" });
  });

  it("edit-pill for message with spaces uses quoted draft so it is not split", () => {
    const full = 'level:error message:"hello world" ';
    const { committed } = parseQueryState(full);
    const token = committed[1]!;
    expect(token).toBe("message:hello world");
    const next = rebuildCommittedAfterRemovingPill(committed, 0, 1, false);
    const draft = quoteMessageTokenForEditDraft(token);
    expect(draft).toBe('message:"hello world"');
    const q = buildQuery(next, draft);
    expect(parseQueryState(q)).toEqual({
      committed: ["level:error"],
      draft: 'message:"hello world"',
    });
  });
});

describe("buildQuery — trailing-space convention", () => {
  it("serializes multi-word message so parse round-trips with another pill", () => {
    const q = buildQuery(["level:error", "message:hello world"], "");
    expect(q).toBe('level:error message:"hello world" ');
    expect(parseQueryState(q)).toEqual({
      committed: ["level:error", "message:hello world"],
      draft: "",
    });
  });

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

describe("programmatic query helpers — trailing space required for pill commit", () => {
  it("setPackageInQuery output without trailing space leaves token as draft", () => {
    const raw = setPackageInQuery("", "com.example.app");
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
    expect(parseQueryState("package:mine").draft).toBe("package:mine");
  });

  it("preset query WITH trailing space commits all tokens as pills", () => {
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

describe("querybar pure editing commands", () => {
  it("builds draft after key-context suggestion insertion", () => {
    expect(buildDraftAfterSuggestion("tag:Ok", "OkHttp")).toBe("tag:OkHttp ");
  });

  it("preserves regex key text while inserting suggestion", () => {
    expect(buildDraftAfterSuggestion("tag~:Ok", "Ok.*")).toBe("tag~:Ok.* ");
  });

  it("preserves negation prefix during bare suggestion insertion", () => {
    expect(buildDraftAfterSuggestion("-ta", "tag:")).toBe("-tag:");
  });

  it("removes the last committed pill and trailing connectors", () => {
    expect(removeLastCommittedPill(["level:error", "&&", "tag:App"])).toEqual(["level:error"]);
    expect(removeLastCommittedPill(["level:error", "|"])).toEqual([]);
  });

  it("builds committed parts for +AND using current draft", () => {
    expect(buildCommittedWithAndConnector(["level:error"], "tag:App")).toEqual([
      "level:error",
      "tag:App",
      "&&",
    ]);
  });

  it("preserves current +AND idempotency behavior", () => {
    expect(buildCommittedWithAndConnector(["level:error", "&&"], "")).toEqual([
      "level:error",
      "&&",
    ]);
  });

  it("builds committed parts for +OR using current draft", () => {
    expect(buildCommittedWithOrGroup(["level:error"], "tag:App")).toEqual([
      "level:error",
      "tag:App",
      "|",
    ]);
  });

  it("drops trailing AND connector before adding +OR", () => {
    expect(buildCommittedWithOrGroup(["level:error", "&&"], "")).toEqual(["level:error", "|"]);
  });
});
