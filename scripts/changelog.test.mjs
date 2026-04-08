import { describe, it, expect, beforeEach, afterEach } from "vitest";
import { writeFileSync, readFileSync, unlinkSync } from "fs";
import { resolve } from "path";
import {
  parseCommits,
  filterUserFacing,
  groupBySection,
  formatEntry,
  prependToChangelog,
} from "./changelog.mjs";

// ── parseCommits ──────────────────────────────────────────────────────────────

describe("parseCommits", () => {
  it("parses a standard conventional commit", () => {
    const result = parseCommits(["abc1234 feat(ui): add toast system"]);
    expect(result).toEqual([
      { hash: "abc1234", type: "feat", scope: "ui", breaking: false, description: "add toast system" },
    ]);
  });

  it("parses a commit without scope", () => {
    const result = parseCommits(["abc1234 fix: correct typo"]);
    expect(result).toEqual([
      { hash: "abc1234", type: "fix", scope: null, breaking: false, description: "correct typo" },
    ]);
  });

  it("parses a breaking commit (! suffix on type)", () => {
    const result = parseCommits(["abc1234 feat!: redesign API"]);
    expect(result).toEqual([
      { hash: "abc1234", type: "feat", scope: null, breaking: true, description: "redesign API" },
    ]);
  });

  it("parses a breaking commit with scope", () => {
    const result = parseCommits(["abc1234 fix!(auth): remove legacy endpoint"]);
    expect(result).toEqual([
      { hash: "abc1234", type: "fix", scope: "auth", breaking: true, description: "remove legacy endpoint" },
    ]);
  });

  it("parses non-conforming lines as unknown type", () => {
    const result = parseCommits(["abc1234 Merge branch main into feature"]);
    expect(result).toEqual([
      { hash: "abc1234", type: "unknown", scope: null, breaking: false, description: "Merge branch main into feature" },
    ]);
  });

  it("handles multiple lines", () => {
    const result = parseCommits([
      "aaa0001 feat: first",
      "bbb0002 fix: second",
    ]);
    expect(result).toHaveLength(2);
    expect(result[0].type).toBe("feat");
    expect(result[1].type).toBe("fix");
  });

  it("returns empty array for empty input", () => {
    expect(parseCommits([])).toEqual([]);
  });
});

// ── filterUserFacing ──────────────────────────────────────────────────────────

describe("filterUserFacing", () => {
  function commit(type, description, breaking = false) {
    return { hash: "abc", type, scope: null, breaking, description };
  }

  it("keeps feat commits", () => {
    expect(filterUserFacing([commit("feat", "add thing")])).toHaveLength(1);
  });

  it("keeps fix commits", () => {
    expect(filterUserFacing([commit("fix", "fix bug")])).toHaveLength(1);
  });

  it("keeps perf commits", () => {
    expect(filterUserFacing([commit("perf", "faster")])).toHaveLength(1);
  });

  it("keeps refactor commits", () => {
    expect(filterUserFacing([commit("refactor", "clean up")])).toHaveLength(1);
  });

  it("excludes docs commits", () => {
    expect(filterUserFacing([commit("docs", "update readme")])).toHaveLength(0);
  });

  it("excludes chore commits", () => {
    expect(filterUserFacing([commit("chore", "update deps")])).toHaveLength(0);
  });

  it("excludes test commits", () => {
    expect(filterUserFacing([commit("test", "add unit test")])).toHaveLength(0);
  });

  it("excludes ci commits", () => {
    expect(filterUserFacing([commit("ci", "fix pipeline")])).toHaveLength(0);
  });

  it("excludes style commits", () => {
    expect(filterUserFacing([commit("style", "format code")])).toHaveLength(0);
  });

  it("excludes unknown type commits", () => {
    expect(filterUserFacing([commit("unknown", "Merge branch x")])).toHaveLength(0);
  });

  it("excludes release commits (chore: release v*)", () => {
    expect(filterUserFacing([commit("chore", "release v0.1.4")])).toHaveLength(0);
  });

  it("excludes [skip ci] commits", () => {
    expect(filterUserFacing([commit("feat", "add thing [skip ci]")])).toHaveLength(0);
  });

  it("keeps breaking feat commits", () => {
    expect(filterUserFacing([commit("feat", "new API", true)])).toHaveLength(1);
  });
});

// ── groupBySection ────────────────────────────────────────────────────────────

describe("groupBySection", () => {
  function commit(type, description, breaking = false) {
    return { hash: "abc", type, scope: null, breaking, description };
  }

  it("groups feat into Added", () => {
    const sections = groupBySection([commit("feat", "new feature")]);
    expect(sections.get("Added")).toEqual(["new feature"]);
  });

  it("groups fix into Fixed", () => {
    const sections = groupBySection([commit("fix", "fix bug")]);
    expect(sections.get("Fixed")).toEqual(["fix bug"]);
  });

  it("groups perf into Changed", () => {
    const sections = groupBySection([commit("perf", "faster query")]);
    expect(sections.get("Changed")).toEqual(["faster query"]);
  });

  it("groups refactor into Changed", () => {
    const sections = groupBySection([commit("refactor", "clean up auth")]);
    expect(sections.get("Changed")).toEqual(["clean up auth"]);
  });

  it("groups breaking commits into Breaking Changes and not their normal section", () => {
    const sections = groupBySection([commit("feat", "new API", true)]);
    expect(sections.get("Breaking Changes")).toEqual(["new API"]);
    expect(sections.has("Added")).toBe(false);
  });

  it("omits empty sections", () => {
    const sections = groupBySection([commit("fix", "a fix")]);
    expect(sections.has("Added")).toBe(false);
    expect(sections.has("Changed")).toBe(false);
    expect(sections.has("Breaking Changes")).toBe(false);
  });

  it("preserves section order: Breaking > Added > Fixed > Changed", () => {
    const sections = groupBySection([
      commit("refactor", "clean"),
      commit("feat", "add"),
      commit("fix", "fix"),
      commit("feat", "break", true),
    ]);
    expect([...sections.keys()]).toEqual(["Breaking Changes", "Added", "Fixed", "Changed"]);
  });

  it("returns empty Map for empty input", () => {
    expect(groupBySection([])).toEqual(new Map());
  });
});

// ── formatEntry ───────────────────────────────────────────────────────────────

describe("formatEntry", () => {
  it("renders a complete entry with multiple sections", () => {
    const sections = new Map([
      ["Added", ["add toast system"]],
      ["Fixed", ["fix typo", "fix crash"]],
    ]);
    const result = formatEntry("0.1.5", "2026-04-09", sections);
    expect(result).toBe(
      "## [0.1.5] — 2026-04-09\n\n" +
      "### Added\n- add toast system\n\n" +
      "### Fixed\n- fix typo\n- fix crash\n"
    );
  });

  it("omits sections with no entries", () => {
    const sections = new Map([["Fixed", ["fix bug"]]]);
    const result = formatEntry("0.1.5", "2026-04-09", sections);
    expect(result).not.toContain("### Added");
    expect(result).toContain("### Fixed");
  });

  it("renders a minimal entry with no sections", () => {
    const result = formatEntry("0.1.5", "2026-04-09", new Map());
    expect(result).toBe("## [0.1.5] — 2026-04-09\n");
  });

  it("renders Breaking Changes section before Added", () => {
    const sections = new Map([
      ["Breaking Changes", ["remove old API"]],
      ["Added", ["new API"]],
    ]);
    const result = formatEntry("1.0.0", "2026-04-09", sections);
    expect(result.indexOf("### Breaking Changes")).toBeLessThan(result.indexOf("### Added"));
  });
});

// ── prependToChangelog ────────────────────────────────────────────────────────

describe("prependToChangelog", () => {
  const TMP = resolve(import.meta.dirname, "__test_changelog.md");

  const INITIAL = `# Changelog

All notable changes.

---

## [0.1.4] — 2026-04-08

### Added
- Previous entry

---

## [0.1.3] and earlier

See GitHub.
`;

  beforeEach(() => {
    writeFileSync(TMP, INITIAL, "utf-8");
  });

  afterEach(() => {
    try { unlinkSync(TMP); } catch { /* ignore */ }
  });

  it("inserts the new entry before existing entries", () => {
    const entry = "## [0.1.5] — 2026-04-09\n\n### Added\n- new thing\n";
    prependToChangelog(TMP, entry);
    const content = readFileSync(TMP, "utf-8");
    const newIdx = content.indexOf("## [0.1.5]");
    const oldIdx = content.indexOf("## [0.1.4]");
    expect(newIdx).toBeGreaterThan(-1);
    expect(newIdx).toBeLessThan(oldIdx);
  });

  it("preserves all existing content", () => {
    const entry = "## [0.1.5] — 2026-04-09\n";
    prependToChangelog(TMP, entry);
    const content = readFileSync(TMP, "utf-8");
    expect(content).toContain("## [0.1.4]");
    expect(content).toContain("Previous entry");
    expect(content).toContain("## [0.1.3] and earlier");
  });

  it("places a --- separator between the new entry and old content", () => {
    const entry = "## [0.1.5] — 2026-04-09\n";
    prependToChangelog(TMP, entry);
    const content = readFileSync(TMP, "utf-8");
    const entryEnd = content.indexOf("## [0.1.5]") + entry.length;
    const afterEntry = content.slice(entryEnd);
    expect(afterEntry.trimStart()).toMatch(/^---/);
  });

  it("places a second entry before the first on repeated calls", () => {
    prependToChangelog(TMP, "## [0.1.5] — 2026-04-09\n");
    prependToChangelog(TMP, "## [0.1.6] — 2026-04-10\n");
    const content = readFileSync(TMP, "utf-8");
    const idx6 = content.indexOf("## [0.1.6]");
    const idx5 = content.indexOf("## [0.1.5]");
    const idx4 = content.indexOf("## [0.1.4]");
    expect(idx6).toBeLessThan(idx5);
    expect(idx5).toBeLessThan(idx4);
  });
});
