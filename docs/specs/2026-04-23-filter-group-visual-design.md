# Filter Group Visual + Optional Paren Syntax

**Date:** 2026-04-23  
**Status:** Approved  
**Approach:** Parse-and-strip (Approach 1)

---

## Summary

Add visual grouping to the `QueryBar` pill UI so that OR-separated filter groups are clearly enclosed in a container box. Simultaneously support optional `( )` parenthesis syntax in the raw query string — parens are parsed and stripped immediately, never stored in the wire format.

---

## Goals

- Users can visually distinguish which filter conditions belong to the same AND group
- Users can optionally type `(A && B) | C` — identical semantics to `A && B | C`
- No enforcement: parens are always optional
- Zero changes to the wire format or existing stored queries

---

## Non-goals

- Nested grouping: `(A && (B | C)) && D` — not supported
- Parens as persistent syntax in the serialized query string
- Changes to `+AND` / `+OR` button behavior or keyboard shortcuts

---

## Decisions

| Decision | Choice | Rationale |
|---|---|---|
| Visual style | Option A: Enclosed container (subtle rounded box per group) | Scales cleanly to any number of groups and group sizes |
| Container visibility | Option B: Only when 2+ OR groups exist | Avoids visual noise for simple single-group queries (most common case) |
| Paren storage | Parse-and-strip — never stored in `committed[]` | Zero backward-compat risk; visual container already communicates grouping |

---

## Data Model

**No changes.** `committed: string[]` remains a flat array of token strings and structural separators (`|`, `&&`, `&`). Parens are never stored.

Wire format example — `(level:error tag:App) | message:crash` is stored and handled identically to `level:error tag:App | message:crash`:

```
committed = ["level:error", "&&", "tag:App", "|", "message:crash"]
```

---

## Architecture

```
Raw query string (may contain optional parens)
           │
           ▼
parseQueryBarState()       ← no change; parens in tokens flow through as-is
           │
           ▼
committed: string[]        ← may temporarily contain "(level:error" or "tag:App)"
           │
           ▼
buildQueryBarPillGroups()  ← NEW: normalizes group-boundary parens from tokens
           │                        skips standalone "(" and ")" tokens
           ▼
string[][]  pill groups    ← always clean (no paren chars)
           │
           ├──▶ UI render  ← wraps each group in container when 2+ groups
           │
           └──▶ parseFilterGroups()  ← NEW: stripGroupParens per OR segment
                       │
                       ▼
                FilterGroup[]  ← correct AND-group tokens, no parens
                       │
                       ▼
                matchesFilterGroups()  ← unchanged
```

---

## Changes

### `src/lib/logcat-query.ts`

#### New private `stripGroupParens(s: string): string`

Strips a matching outer `(…)` pair from a group segment:

```ts
function stripGroupParens(s: string): string {
  const t = s.trim();
  if (t.startsWith("(") && t.endsWith(")")) return t.slice(1, -1).trim();
  return t;
}
```

- `"(A && B)"` → `"A && B"`
- `"A && B"` → `"A && B"` (unchanged)
- `"(A && B"` → `"(A && B"` (unmatched, unchanged — safe fallthrough)

#### `parseFilterGroups` — update

```ts
.map((segment) => parseQuery(stripGroupParens(segment.trim())))
```

Only this one line changes. Paren-wrapped segments are stripped before tokenizing.

#### `buildQueryBarPillGroups` — update

Two changes:

1. Skip standalone `(` and `)` tokens (alongside `&&` / `&`):
   ```ts
   } else if (part !== "&&" && part !== "&" && part !== "(" && part !== ")") {
   ```

2. Apply `normalizeGroupParenTokens` to each group after building:
   ```ts
   return groups.map(normalizeGroupParenTokens);
   ```

#### New private `normalizeGroupParenTokens(group: string[]): string[]`

Strips group-boundary parens from tokens that have them attached:

```ts
function normalizeGroupParenTokens(group: string[]): string[] {
  if (group.length === 0) return group;
  const result = [...group];
  if (result[0].startsWith("(")) result[0] = result[0].slice(1);
  const last = result.length - 1;
  // Don't strip trailing ) when it's part of a value: message:(pattern)
  if (result[last].endsWith(")") && !result[last].includes(":(")) {
    result[last] = result[last].slice(0, -1);
  }
  return result.filter((t) => t.length > 0);
}
```

Edge cases:
- `["(level:error", "tag:App)"]` → `["level:error", "tag:App"]`
- `["message:(pattern)"]` → `["message:(pattern)"]` (heuristic: contains `:(`  — not stripped)
- `["(level:error"]` (single token) → `["level:error"]` (strip leading only)

### `src/components/logcat/QueryBar.tsx`

#### New `multiGroup` memo

```ts
const multiGroup = createMemo(() =>
  pillGroups().filter((g) => g.length > 0).length >= 2
);
```

#### New `groupBoxStyle()` helper

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

#### Pill group render — wrap with conditional container

Wrap each group's token list in a `div`. When `multiGroup()` is true, apply `groupBoxStyle()`. When false, apply `display: contents` so pills are direct flex children of the outer row (no layout change for single-group queries):

```tsx
<div style={multiGroup() ? groupBoxStyle() : { display: "contents" }}>
  {/* AND badges + pills + end-of-group inline edit */}
</div>
```

No changes to: OR badge, draft input row, `inlineEditOrphanAfterOrBranch`, `+AND`/`+OR` buttons, clear button, autocomplete dropdown.

---

## Testing Plan (TDD — Red first)

### `parseFilterGroups` — paren syntax tests

| Input | Expected behaviour |
|---|---|
| `(A && B) \| C` | Same result as `A && B \| C` |
| `(A) \| (B)` | Same result as `A \| B` |
| `(message:"hello world") \| tag:App` | Parens + quoted value handled correctly |
| `(A && B` | Unmatched paren — falls through, parsed as-is without crash |

### `buildQueryBarPillGroups` — paren normalization tests

| Input (`committed`) | Expected groups |
|---|---|
| `["(level:error", "tag:App)", "\|", "message:crash"]` | `[["level:error","tag:App"],["message:crash"]]` |
| `["(", "level:error", ")", "\|", "crash"]` | `[["level:error"],["crash"]]` |
| `["message:(pattern)"]` | `[["message:(pattern)"]]` — value paren NOT stripped |
| `["(level:error"]` | `[["level:error"]]` — leading paren only |

### End-to-end filter correctness

Entry with `level=error, tag=App` matches both:
- `(level:error tag:App) | message:crash`
- `level:error tag:App | message:crash`

(Identical `FilterGroup[]` output from `parseFilterGroups`.)

---

## Known Limitations

**Value-paren heuristic** — `normalizeGroupParenTokens` skips trailing `)` stripping when the token contains `:(`. This correctly handles `message:(pattern)` but would incorrectly strip `)` from a token like `message:hello)` (where `)` is a literal part of the filter value). This edge case requires the user to have manually typed `message:hello)` without a matching leading `(` — rare in practice, acceptable for v1.

**Container flicker during inline edit** — when editing the sole pill of a trailing OR group, that group temporarily disappears from `pillGroups()`, dropping `multiGroup()` to false and removing the container from the remaining group. The container reappears when the edit is committed or cancelled. Minor transient state, not worth the added complexity to fix.

---

## Files Changed

| File | Change type |
|---|---|
| `src/lib/logcat-query.ts` | Add `stripGroupParens`, `normalizeGroupParenTokens`; update `parseFilterGroups`, `buildQueryBarPillGroups` |
| `src/components/logcat/QueryBar.tsx` | Add `multiGroup` memo, `groupBoxStyle()`, conditional wrapper div |
| `src/lib/logcat-query.test.ts` | New tests for paren parsing, normalization, end-to-end filter |
