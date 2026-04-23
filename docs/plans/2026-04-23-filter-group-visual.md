# Filter Group Visual + Optional Paren Syntax Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add visual enclosed-container grouping to OR filter groups in the QueryBar pill UI, and parse optional `( )` parentheses in raw query strings without storing them.

**Architecture:** Parse-and-strip — `parseFilterGroups` strips outer parens from each OR segment before tokenizing; `buildQueryBarPillGroups` normalizes group-boundary parens from tokens and skips standalone `(` `)`. The wire format (`committed: string[]`) is unchanged. The UI wraps each pill group in a subtle rounded container only when 2+ groups exist (`multiGroup` memo).

**Tech Stack:** TypeScript, SolidJS (`createMemo`, `For`, `Show`), Vitest

**Spec:** `docs/specs/2026-04-23-filter-group-visual-design.md`

---

## File Map

| File | Change |
|---|---|
| `src/lib/logcat-query.ts` | Add `stripGroupParens` (private), `normalizeGroupParenTokens` (private); update `parseFilterGroups` (1 line), `buildQueryBarPillGroups` (2 lines) |
| `src/lib/logcat-query.test.ts` | Append new describe blocks at end of file |
| `src/components/logcat/QueryBar.tsx` | Add `multiGroup` memo, `groupBoxStyle()` helper, wrap pill group render in conditional container |

---

## Task 1: `parseFilterGroups` — optional paren syntax (TDD)

**Files:**
- Modify: `src/lib/logcat-query.test.ts` (append after line 1943)
- Modify: `src/lib/logcat-query.ts:929-936` (`parseFilterGroups`)

- [ ] **Step 1.1: Write the failing tests**

Append to the end of `src/lib/logcat-query.test.ts` (after line 1943, before the final `}`):

```ts
// ── parseFilterGroups — optional paren syntax ─────────────────────────────────

describe("parseFilterGroups — optional outer parens", () => {
  it("(A && B) | C produces same groups as A && B | C", () => {
    expect(parseFilterGroups("(tag:App message:crash) | level:error")).toEqual(
      parseFilterGroups("tag:App message:crash | level:error"),
    );
  });

  it("(A) | (B) produces same groups as A | B", () => {
    expect(parseFilterGroups("(tag:App) | (tag:System)")).toEqual(
      parseFilterGroups("tag:App | tag:System"),
    );
  });

  it("parens around a quoted value with pipe inside", () => {
    expect(parseFilterGroups('(message:"hello|world") | tag:App')).toEqual(
      parseFilterGroups('message:"hello|world" | tag:App'),
    );
  });

  it("unmatched leading paren does not throw", () => {
    expect(() => parseFilterGroups("(tag:App message:crash")).not.toThrow();
  });

  it("single paren-wrapped group with no pipe", () => {
    expect(parseFilterGroups("(level:error tag:App)")).toEqual(
      parseFilterGroups("level:error tag:App"),
    );
  });
});
```

- [ ] **Step 1.2: Run and confirm RED**

```bash
npm run test -- src/lib/logcat-query.test.ts
```

Expected: the 5 new tests fail. The first failing test message will be something like:
```
AssertionError: expected [ [...], [...] ] to deeply equal [ [...], [...] ]
```
(Parens end up inside token text, making values like `"(tag:App"` instead of `"tag:App"`.)

- [ ] **Step 1.3: Add `stripGroupParens` to `src/lib/logcat-query.ts`**

Add this private function immediately before `parseFilterGroups` (before line 929):

```ts
function stripGroupParens(s: string): string {
  const t = s.trim();
  if (t.startsWith("(") && t.endsWith(")")) return t.slice(1, -1).trim();
  return t;
}
```

- [ ] **Step 1.4: Update `parseFilterGroups` to call `stripGroupParens`**

Change the `.map` line inside `parseFilterGroups` (line ~932) from:

```ts
    .map((segment) => parseQuery(segment.trim()))
```

to:

```ts
    .map((segment) => parseQuery(stripGroupParens(segment.trim())))
```

Full function after the change:

```ts
export function parseFilterGroups(raw: string): FilterGroup[] {
  if (!raw.trim()) return [[]];
  const groups = splitOnOrSeparator(raw)
    .map((segment) => parseQuery(stripGroupParens(segment.trim())))
    .filter((g) => g.length > 0);
  return groups.length > 0 ? groups : [[]];
}
```

- [ ] **Step 1.5: Run and confirm GREEN**

```bash
npm run test -- src/lib/logcat-query.test.ts
```

Expected: all tests pass, including the 5 new ones. Total count increases by 5.

- [ ] **Step 1.6: Commit**

```bash
git add src/lib/logcat-query.ts src/lib/logcat-query.test.ts
git commit -m "feat(filter): support optional outer parens in parseFilterGroups"
```

---

## Task 2: `buildQueryBarPillGroups` — paren normalization (TDD)

**Files:**
- Modify: `src/lib/logcat-query.test.ts` (append after Task 1 tests)
- Modify: `src/lib/logcat-query.ts:697-707` (`buildQueryBarPillGroups`)

- [ ] **Step 2.1: Write the failing tests**

Append to `src/lib/logcat-query.test.ts` (after the Task 1 block):

```ts
// ── buildQueryBarPillGroups — paren normalization ─────────────────────────────

describe("buildQueryBarPillGroups — paren normalization", () => {
  it("strips group-boundary parens attached to tokens", () => {
    expect(
      buildQueryBarPillGroups(["(level:error", "tag:App)", "|", "message:crash"]),
    ).toEqual([["level:error", "tag:App"], ["message:crash"]]);
  });

  it("skips standalone ( and ) tokens", () => {
    expect(
      buildQueryBarPillGroups(["(", "level:error", ")", "|", "crash"]),
    ).toEqual([["level:error"], ["crash"]]);
  });

  it("does not strip trailing ) that is part of a value expression (contains :()", () => {
    expect(buildQueryBarPillGroups(["message:(pattern)"])).toEqual([
      ["message:(pattern)"],
    ]);
  });

  it("strips leading ( only — when token is the only one in its group", () => {
    expect(buildQueryBarPillGroups(["(level:error"])).toEqual([["level:error"]]);
  });

  it("empty tokens produced by stripping are removed", () => {
    // "(" alone would become "" after stripping — should be filtered out
    expect(buildQueryBarPillGroups(["("])).toEqual([[]]);
  });
});
```

- [ ] **Step 2.2: Run and confirm RED**

```bash
npm run test -- src/lib/logcat-query.test.ts
```

Expected: the 5 new tests fail. Example failure:
```
AssertionError: expected [ [ '(level:error', 'tag:App)' ], [ 'message:crash' ] ]
to deeply equal [ [ 'level:error', 'tag:App' ], [ 'message:crash' ] ]
```

- [ ] **Step 2.3: Add `normalizeGroupParenTokens` to `src/lib/logcat-query.ts`**

Add this private function immediately before `buildQueryBarPillGroups` (before line 697):

```ts
function normalizeGroupParenTokens(group: string[]): string[] {
  if (group.length === 0) return group;
  const result = [...group];
  if (result[0].startsWith("(")) result[0] = result[0].slice(1);
  const last = result.length - 1;
  // Don't strip trailing ) when it's part of a value like message:(pattern)
  if (result[last].endsWith(")") && !result[last].includes(":(")) {
    result[last] = result[last].slice(0, -1);
  }
  return result.filter((t) => t.length > 0);
}
```

- [ ] **Step 2.4: Update `buildQueryBarPillGroups`**

Replace the current function body (lines 697-707):

```ts
export function buildQueryBarPillGroups(committed: string[]): string[][] {
  const groups: string[][] = [[]];
  for (const part of committed) {
    if (part === "|") {
      groups.push([]);
    } else if (part !== "&&" && part !== "&") {
      groups[groups.length - 1].push(part);
    }
  }
  return groups;
}
```

with:

```ts
export function buildQueryBarPillGroups(committed: string[]): string[][] {
  const groups: string[][] = [[]];
  for (const part of committed) {
    if (part === "|") {
      groups.push([]);
    } else if (part !== "&&" && part !== "&" && part !== "(" && part !== ")") {
      groups[groups.length - 1].push(part);
    }
  }
  return groups.map(normalizeGroupParenTokens);
}
```

- [ ] **Step 2.5: Run and confirm GREEN**

```bash
npm run test -- src/lib/logcat-query.test.ts
```

Expected: all tests pass. Total count increases by 5 more.

- [ ] **Step 2.6: Commit**

```bash
git add src/lib/logcat-query.ts src/lib/logcat-query.test.ts
git commit -m "feat(filter): normalize group-boundary parens in buildQueryBarPillGroups"
```

---

## Task 3: End-to-end filter correctness test

**Files:**
- Modify: `src/lib/logcat-query.test.ts` (append after Task 2 tests)

- [ ] **Step 3.1: Write the test**

Append to `src/lib/logcat-query.test.ts`:

```ts
// ── parseFilterGroups / matchesFilterGroups — paren round-trip ────────────────

describe("matchesFilterGroups — paren query produces identical results", () => {
  it("(level:error tag:App) | message:crash matches same entries as without parens", () => {
    const now = Date.now();

    const withParens = parseFilterGroups("(level:error tag:App) | message:crash");
    const withoutParens = parseFilterGroups("level:error tag:App | message:crash");

    const entries = [
      makeEntry({ level: "error", tag: "App", message: "something" }),  // matches group 1
      makeEntry({ level: "debug", tag: "Other", message: "crash" }),     // matches group 2
      makeEntry({ level: "debug", tag: "Other", message: "nothing" }),   // matches neither
    ];

    for (const entry of entries) {
      expect(matchesFilterGroups(entry, withParens, now)).toBe(
        matchesFilterGroups(entry, withoutParens, now),
      );
    }
  });

  it("(A) | (B) | (C) matches same entries as A | B | C", () => {
    const now = Date.now();
    const withParens = parseFilterGroups("(tag:Alpha) | (tag:Beta) | (tag:Gamma)");
    const withoutParens = parseFilterGroups("tag:Alpha | tag:Beta | tag:Gamma");

    const alphaEntry = makeEntry({ tag: "Alpha" });
    const betaEntry = makeEntry({ tag: "Beta" });
    const otherEntry = makeEntry({ tag: "Other" });

    expect(matchesFilterGroups(alphaEntry, withParens, now)).toBe(
      matchesFilterGroups(alphaEntry, withoutParens, now),
    );
    expect(matchesFilterGroups(betaEntry, withParens, now)).toBe(
      matchesFilterGroups(betaEntry, withoutParens, now),
    );
    expect(matchesFilterGroups(otherEntry, withParens, now)).toBe(
      matchesFilterGroups(otherEntry, withoutParens, now),
    );
  });
});
```

- [ ] **Step 3.2: Run and confirm GREEN** (these should pass immediately since Tasks 1 & 2 are done)

```bash
npm run test -- src/lib/logcat-query.test.ts
```

Expected: all tests pass including the 2 new end-to-end tests.

- [ ] **Step 3.3: Commit**

```bash
git add src/lib/logcat-query.test.ts
git commit -m "test(filter): end-to-end paren round-trip correctness tests"
```

---

## Task 4: QueryBar — visual group container

**Files:**
- Modify: `src/components/logcat/QueryBar.tsx`

No automated tests for this task — visual change verified manually via dev server.

- [ ] **Step 4.1: Add `multiGroup` memo**

In `QueryBar.tsx`, locate the block of existing memos (around line 155–180). After the line:

```ts
const pillGroups = createMemo(() => buildQueryBarPillGroups(committed()));
```

Add:

```ts
const multiGroup = createMemo(() => pillGroups().filter((g) => g.length > 0).length >= 2);
```

- [ ] **Step 4.2: Add `groupBoxStyle()` helper**

At the bottom of the file, alongside `connectorBtnStyle()` (around line 966), add:

```ts
function groupBoxStyle(): Record<string, string> {
  return {
    display: "inline-flex",
    "align-items": "center",
    gap: "3px",
    padding: "3px 6px",
    border: "1px solid rgba(255,255,255,0.10)",
    "border-radius": "8px",
    background: "rgba(255,255,255,0.04)",
    "flex-shrink": "0",
  };
}
```

- [ ] **Step 4.3: Wrap the pill group content in a conditional container**

Inside the `<For each={pillGroups()}>` render (starting around line 552), find the section after the OR badge `</Show>` and before the `{/* Tokens in this group */}` comment. The current structure is:

```tsx
<Show when={gi() > 0}>
  <span style={{ /* OR badge styles */ }}>OR</span>
</Show>

{/* Tokens in this group */}
<For each={group}>
  ...
</For>
<Show when={inlineEdit()?.groupIdx === gi() && inlineEdit()?.tokenIdx === group.length}>
  <input ref={inlineEditRef} ... />
</Show>
```

Wrap the `<For>` and the trailing `<Show>` in a container `div`:

```tsx
<Show when={gi() > 0}>
  <span style={{ /* OR badge styles — unchanged */ }}>OR</span>
</Show>

{/* Group container: visible box only when 2+ groups exist */}
<div style={multiGroup() ? groupBoxStyle() : { display: "contents" }}>
  {/* Tokens in this group */}
  <For each={group}>
    {(token, ti) => {
      // ... unchanged token + AND badge + pill render
    }}
  </For>
  <Show when={inlineEdit()?.groupIdx === gi() && inlineEdit()?.tokenIdx === group.length}>
    <input
      ref={inlineEditRef}
      type="text"
      spellcheck={false}
      value={inlineEdit()?.text ?? ""}
      onInput={handleInlineInput}
      onKeyDown={handleInlineKeyDown}
      onBlur={handleInlineBlur}
      onClick={(e) => e.stopPropagation()}
      onMouseDown={(e) => e.stopPropagation()}
      placeholder="Edit filter…"
      style={{ ...inlineSlotStyle(), "flex-shrink": "1" }}
    />
  </Show>
</div>
```

`display: contents` makes the wrapper invisible to the layout engine when single-group mode is active — pills are direct flex children of the outer row, identical to the current behaviour.

- [ ] **Step 4.4: Start the dev server and verify visually**

```bash
npm run dev
```

Open the app. In the logcat filter bar:

1. **Single group** — type `level:error` and commit (press Space). Then type `tag:App` and commit. Verify: **no container box** appears, pills look identical to before.

2. **Two groups** — click `+ OR`, then type `message:crash` and commit. Verify: **a subtle rounded box** appears around each group, OR badge sits between the two boxes.

3. **Three groups** — click `+ OR` again, type `tag:System` and commit. Verify: three boxes, two OR badges.

4. **Remove a group** — remove all pills from one group until only one group remains. Verify: **boxes disappear**, back to single-group appearance.

5. **Paren syntax** — clear the filter, paste `(level:error tag:App) | message:crash` into the draft and press Space/Enter to commit tokens. Verify: pills display as `level:error`, `tag:App`, `message:crash` (no `(` or `)` visible in pill text), two groups with boxes.

- [ ] **Step 4.5: Commit**

```bash
git add src/components/logcat/QueryBar.tsx
git commit -m "feat(ui): show enclosed group container in QueryBar when 2+ OR groups exist"
```

---

## Task 5: Full test suite sanity check

- [ ] **Step 5.1: Run all tests**

```bash
npm run test
```

Expected: all tests pass (previous count + 12 new tests). Zero regressions.

- [ ] **Step 5.2: Run lint**

```bash
npm run lint
```

Expected: no new lint errors.
