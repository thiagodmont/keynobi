/**
 * QueryBar — pill-based chip input for logcat filtering.
 *
 * Each committed filter token is rendered as a colored chip. The draft
 * (the token currently being typed) lives in a plain text input at the end.
 * The container wraps and grows naturally — no fixed max-width.
 *
 * Token colors (theme tokens):
 *   level:* → warning   tag:* / tag~:* → info
 *   message:* / msg:* → success   package:* / pkg:* → accent
 *   age:* → accent      is:* → error
 *   freetext  → muted     negated (-*)    → same color, dimmed
 *
 * AND within a group: pills are separated by a subtle · dot.
 * OR between groups: a small accent "OR" badge.
 *
 * Keyboard:
 *   Backspace (on empty draft) — removes the last committed pill
 *   Escape (dropdown open)     — closes dropdown
 *   Escape (dropdown closed)   — clears entire query
 *   ArrowDown/Up               — navigate suggestions
 *   Tab / Enter                — apply selected suggestion
 */

import { type JSX, createSignal, createMemo, For, Show } from "solid-js";
import {
  QUERY_KEYS,
  LEVEL_NAMES,
  AGE_SUGGESTIONS,
  IS_SUGGESTIONS,
  getActiveTokenContext,
  parseQueryBarState,
  balanceMessageDraftQuotes,
  buildQueryBarPillGroups,
  applyMessageKeySpaceAutoQuote,
  pasteIntoMessageKeyDraft,
  rebuildCommittedAfterRemovingPill,
  committedEndsWithOrSeparator,
  serializeQueryBarCommittedPart,
  insertPillAtGroupPosition,
  applyInlineEditCommit,
} from "@/lib/logcat-query";

// ── Props ─────────────────────────────────────────────────────────────────────

export interface QueryBarProps {
  value: string;
  onChange: (query: string) => void;
  knownTags: string[];
  knownPackages: string[];
  placeholder?: string;
}

// ── Constants ─────────────────────────────────────────────────────────────────

const MAX_SUGGESTIONS = 10;

// ── Token styling ─────────────────────────────────────────────────────────────

interface TokenStyle {
  color: string;
  bg: string;
  border: string;
}

function getTokenStyle(tokenText: string): TokenStyle {
  const t = tokenText.startsWith("-") ? tokenText.slice(1) : tokenText;
  if (t.startsWith("level:"))
    return {
      color: "var(--warning)",
      bg: "color-mix(in srgb, var(--warning) 13%, transparent)",
      border: "color-mix(in srgb, var(--warning) 35%, transparent)",
    };
  if (t.startsWith("tag:") || t.startsWith("tag~:"))
    return {
      color: "var(--info)",
      bg: "color-mix(in srgb, var(--info) 13%, transparent)",
      border: "color-mix(in srgb, var(--info) 35%, transparent)",
    };
  if (t.startsWith("message:") || t.startsWith("message~:") || t.startsWith("msg:"))
    return {
      color: "var(--success)",
      bg: "color-mix(in srgb, var(--success) 11%, transparent)",
      border: "color-mix(in srgb, var(--success) 35%, transparent)",
    };
  if (t.startsWith("package:") || t.startsWith("pkg:"))
    return {
      color: "var(--accent)",
      bg: "color-mix(in srgb, var(--accent) 13%, transparent)",
      border: "color-mix(in srgb, var(--accent) 40%, transparent)",
    };
  if (t.startsWith("age:"))
    return {
      color: "var(--accent)",
      bg: "color-mix(in srgb, var(--accent) 13%, transparent)",
      border: "color-mix(in srgb, var(--accent) 35%, transparent)",
    };
  if (t.startsWith("is:"))
    return {
      color: "var(--error)",
      bg: "color-mix(in srgb, var(--error) 13%, transparent)",
      border: "color-mix(in srgb, var(--error) 35%, transparent)",
    };
  return { color: "var(--text-secondary)", bg: "rgba(255,255,255,0.07)", border: "var(--border)" };
}

// ── Query parsing helpers (local to this component) ───────────────────────────

/**
 * Split the full query into committed parts and the trailing draft.
 * Uses {@link parseQueryBarState} from `logcat-query` (same lexer as `parseQuery`).
 */
export function parseQueryState(value: string): { committed: string[]; draft: string } {
  return parseQueryBarState(value);
}

/**
 * Build the canonical query string from committed parts + current draft.
 * When there is no draft the committed section always gets a trailing space
 * so on the next render all parts are recognised as committed (not draft).
 */
export function buildQuery(committed: string[], draft: string): string {
  const base = committed.map(serializeQueryBarCommittedPart).join(" ");
  if (!draft) return base ? `${base} ` : "";
  return base ? `${base} ${draft}` : draft;
}

// ── Component ─────────────────────────────────────────────────────────────────

interface InlineEditState {
  /**
   * Pill coordinates **before** the pill was removed for editing — same indices
   * used to splice the token back into the post-removal `buildQueryBarPillGroups(committed())`.
   */
  groupIdx: number;
  tokenIdx: number;
  text: string;
  originalToken: string;
}

export function QueryBar(props: QueryBarProps): JSX.Element {
  let draftRef!: HTMLInputElement;
  let inlineEditRef!: HTMLInputElement;
  let containerRef!: HTMLDivElement;

  const [open, setOpen] = createSignal(false);
  const [selectedIdx, setSelectedIdx] = createSignal(0);
  const [inlineEdit, setInlineEdit] = createSignal<InlineEditState | null>(null);

  // ── Parsed state ──────────────────────────────────────────────────────────

  const queryState = createMemo(() => parseQueryState(props.value));
  const committed = createMemo(() => queryState().committed);
  const draft = createMemo(() => queryState().draft);
  const pillGroups = createMemo(() => buildQueryBarPillGroups(committed()));

  function totalPillCount(): number {
    return pillGroups().reduce((n, gr) => n + gr.length, 0);
  }

  function inlineSlotStyle(): Record<string, string> {
    return {
      flex: "1",
      "min-width": "120px",
      "max-width": "420px",
      background: "var(--bg-primary)",
      border: "1px solid var(--accent)",
      color: "var(--text-primary)",
      "font-size": "11px",
      "font-family": "var(--font-mono)",
      "border-radius": "10px",
      padding: "1px 6px",
      outline: "none",
    };
  }

  /** True when the draft lives in a new OR group (last committed part is `|`). */
  const draftInNewGroup = createMemo(() => committedEndsWithOrSeparator(committed()));

  const hasAnyPills = createMemo(() => pillGroups().some((g) => g.length > 0));
  const isActive = createMemo(() => props.value.trim() !== "");

  /**
   * Editing the sole pill of a trailing OR branch drops that empty group from
   * `pillGroups()`, so `inlineEdit.groupIdx` is no longer a valid index — render
   * the inline field after the surviving groups (with an OR badge).
   */
  const inlineEditOrphanAfterOrBranch = createMemo(() => {
    const st = inlineEdit();
    if (!st) return false;
    return st.groupIdx >= pillGroups().length;
  });

  // ── Suggestions (based on the draft being typed) ──────────────────────────

  const suggestions = createMemo(() => {
    const ctx = getActiveTokenContext(draft());

    if (ctx.key) {
      const partial = ctx.partial.toLowerCase();
      switch (ctx.key.toLowerCase()) {
        case "level":
          return LEVEL_NAMES.filter((l) => l.startsWith(partial)).map((l) => ({
            display: l,
            insert: l,
          }));
        case "tag":
          return props.knownTags
            .filter((t) => t.toLowerCase().includes(partial))
            .slice(0, MAX_SUGGESTIONS)
            .map((t) => ({ display: t, insert: t }));
        case "package":
          return ["mine", ...props.knownPackages]
            .filter((p) => p.toLowerCase().includes(partial))
            .slice(0, MAX_SUGGESTIONS)
            .map((p) => ({ display: p, insert: p }));
        case "is":
          return IS_SUGGESTIONS.filter((s) => s.startsWith(partial)).map((s) => ({
            display: s,
            insert: s,
          }));
        case "age":
          return AGE_SUGGESTIONS.filter((a) => a.startsWith(partial)).map((a) => ({
            display: a,
            insert: a,
          }));
        default:
          return [];
      }
    }

    const partial = ctx.partial.toLowerCase();
    if (!partial) return QUERY_KEYS.map((k) => ({ display: k, insert: k }));

    const keySuggestions = QUERY_KEYS.filter((k) => k.toLowerCase().startsWith(partial)).map(
      (k) => ({ display: k, insert: k })
    );
    const levelSuggestions = LEVEL_NAMES.filter((l) => l.startsWith(partial)).map((l) => ({
      display: `${l} (level shorthand)`,
      insert: l,
    }));
    return [...keySuggestions, ...levelSuggestions].slice(0, MAX_SUGGESTIONS);
  });

  const hasSuggestions = () => open() && suggestions().length > 0;

  // ── Autocomplete selection ────────────────────────────────────────────────

  function applySelection(insert: string) {
    const ctx = getActiveTokenContext(draft());
    const draftBefore = draft().slice(0, ctx.offset);

    let newDraft: string;
    if (ctx.key) {
      // Use the actual colon position to reconstruct the key part.
      // Computing `key.length + 1` would be wrong for regex variants like
      // `tag~:` where the key is reported as "tag" (length 3) but the prefix
      // in the draft string is "tag~:" (5 chars including ~ and :).
      const colonPos = draft().indexOf(":", ctx.offset);
      const keyPart =
        colonPos >= 0
          ? draft().slice(ctx.offset, colonPos + 1)
          : draft().slice(ctx.offset, ctx.offset + ctx.key.length + 1); // fallback
      newDraft = `${draftBefore}${keyPart}${insert} `;
    } else {
      // When there is no key context the entire draft token is being replaced.
      // If the draft starts with `-` (negation), preserve that prefix so that
      // selecting e.g. "tag:" after typing "-ta" produces "-tag:" not "tag:".
      const negationPrefix = draft().startsWith("-") ? "-" : "";
      newDraft = `${draftBefore}${negationPrefix}${insert}`;
      if (!insert.endsWith(":") && !insert.endsWith("~:")) newDraft += " ";
    }

    props.onChange(buildQuery(committed(), newDraft));
    setOpen(false);
    setSelectedIdx(0);
    queueMicrotask(() => draftRef?.focus());
  }

  // ── Draft input handlers ──────────────────────────────────────────────────

  function handleDraftInput(e: InputEvent) {
    const newDraft = (e.currentTarget as HTMLInputElement).value;
    props.onChange(buildQuery(committed(), newDraft));
    setSelectedIdx(0);
    setOpen(true);
  }

  function handleDraftPaste(e: ClipboardEvent) {
    const clip = e.clipboardData?.getData("text/plain") ?? "";
    const input = e.currentTarget as HTMLInputElement;
    const d = draft();
    const selStart = input.selectionStart ?? d.length;
    const selEnd = input.selectionEnd ?? d.length;
    const merged = pasteIntoMessageKeyDraft(d, selStart, selEnd, clip);
    if (!merged) return;
    e.preventDefault();
    props.onChange(buildQuery(committed(), merged.newDraft));
    setSelectedIdx(0);
    setOpen(true);
    queueMicrotask(() => {
      input.setSelectionRange(merged.cursor, merged.cursor);
    });
  }

  function handleKeyDown(e: KeyboardEvent) {
    const input = e.currentTarget as HTMLInputElement;

    if (e.key === " " || e.code === "Space") {
      const d = draft();
      const cursor = input.selectionStart ?? d.length;
      const applied = applyMessageKeySpaceAutoQuote(d, cursor);
      if (applied) {
        e.preventDefault();
        props.onChange(buildQuery(committed(), applied.draft));
        setSelectedIdx(0);
        setOpen(true);
        queueMicrotask(() => input.setSelectionRange(applied.cursor, applied.cursor));
        return;
      }
    }

    // Backspace on empty draft → remove the last committed pill
    if (e.key === "Backspace" && draft() === "") {
      e.preventDefault();
      if (inlineEdit()) commitInlineEdit();
      const parts = [...committed()];
      // Strip trailing separators
      while (
        parts.length > 0 &&
        (parts[parts.length - 1] === "|" ||
          parts[parts.length - 1] === "&&" ||
          parts[parts.length - 1] === "&")
      ) {
        parts.pop();
      }
      if (parts.length > 0) parts.pop();
      // Strip newly-trailing separators
      while (
        parts.length > 0 &&
        (parts[parts.length - 1] === "|" ||
          parts[parts.length - 1] === "&&" ||
          parts[parts.length - 1] === "&")
      ) {
        parts.pop();
      }
      props.onChange(buildQuery(parts, ""));
      return;
    }

    const suggs = suggestions();

    if (e.key === "Escape") {
      if (inlineEdit()) {
        e.preventDefault();
        cancelInlineEdit();
        return;
      }
      if (open()) {
        e.preventDefault();
        setOpen(false);
      } else if (props.value) {
        e.preventDefault();
        props.onChange("");
      }
      return;
    }

    if (!open() || suggs.length === 0) {
      if (e.key === "ArrowDown" && suggs.length > 0) {
        e.preventDefault();
        setOpen(true);
        setSelectedIdx(0);
      }
      return;
    }

    if (e.key === "ArrowDown") {
      e.preventDefault();
      setSelectedIdx((i) => Math.min(i + 1, suggs.length - 1));
    } else if (e.key === "ArrowUp") {
      e.preventDefault();
      setSelectedIdx((i) => Math.max(i - 1, 0));
    } else if (e.key === "Tab" || e.key === "Enter") {
      if (suggs[selectedIdx()]) {
        e.preventDefault();
        applySelection(suggs[selectedIdx()].insert);
      }
    }
  }

  function handleBlur() {
    const balanced = balanceMessageDraftQuotes(draft());
    if (balanced !== draft()) props.onChange(buildQuery(committed(), balanced));
    setTimeout(() => {
      if (!containerRef?.contains(document.activeElement)) setOpen(false);
    }, 150);
  }

  function commitInlineEdit() {
    const st = inlineEdit();
    if (!st) return;
    const curCommitted = committed();
    const dig = draftInNewGroup();
    setInlineEdit(null);
    const next = applyInlineEditCommit(curCommitted, st.groupIdx, st.tokenIdx, st.text, dig);
    props.onChange(buildQuery(next, ""));
  }

  function cancelInlineEdit() {
    const st = inlineEdit();
    if (!st) return;
    const curCommitted = committed();
    const dig = draftInNewGroup();
    setInlineEdit(null);
    const next = insertPillAtGroupPosition(
      curCommitted,
      st.groupIdx,
      st.tokenIdx,
      st.originalToken,
      dig
    );
    props.onChange(buildQuery(next, ""));
  }

  function handleInlineInput(e: InputEvent) {
    setInlineEdit((s) => (s ? { ...s, text: (e.currentTarget as HTMLInputElement).value } : null));
  }

  function handleInlineKeyDown(e: KeyboardEvent) {
    if (e.key === "Enter") {
      e.preventDefault();
      commitInlineEdit();
    } else if (e.key === "Escape") {
      e.preventDefault();
      cancelInlineEdit();
    }
  }

  function handleInlineBlur() {
    queueMicrotask(() => {
      if (inlineEdit()) commitInlineEdit();
    });
  }

  // ── Pill removal ──────────────────────────────────────────────────────────

  function removeToken(groupIdx: number, tokenIdx: number) {
    if (inlineEdit()) commitInlineEdit();
    const newParts = rebuildCommittedAfterRemovingPill(
      committed(),
      groupIdx,
      tokenIdx,
      draftInNewGroup()
    );
    props.onChange(buildQuery(newParts, draft()));
  }

  function editToken(groupIdx: number, tokenIdx: number, tokenText: string) {
    if (inlineEdit()) commitInlineEdit();
    const newParts = rebuildCommittedAfterRemovingPill(
      committed(),
      groupIdx,
      tokenIdx,
      draftInNewGroup()
    );
    props.onChange(buildQuery(newParts, ""));
    setInlineEdit({
      groupIdx,
      tokenIdx,
      text: serializeQueryBarCommittedPart(tokenText),
      originalToken: tokenText,
    });
    // After the same pointer gesture, the container's onClick focuses the draft;
    // defer so the inline field wins focus after that click phase completes.
    setTimeout(() => inlineEditRef?.focus(), 0);
  }

  // ── AND / OR connector buttons ────────────────────────────────────────────

  function handleAddAndConnector() {
    if (inlineEdit()) commitInlineEdit();
    const d = balanceMessageDraftQuotes(draft().trim());
    const parts = [...committed()];
    if (d) parts.push(d);
    while (parts.length > 0 && parts[parts.length - 1] === "|") parts.pop();
    const last = parts[parts.length - 1];
    if (parts.length > 0 && last !== "&&" && last !== "&") parts.push("&&");
    props.onChange(buildQuery(parts, ""));
    queueMicrotask(() => draftRef?.focus());
  }

  function handleAddOrGroup() {
    if (inlineEdit()) commitInlineEdit();
    const d = balanceMessageDraftQuotes(draft().trim());
    const parts = [...committed()];
    if (d) parts.push(d);
    while (
      parts.length > 0 &&
      (parts[parts.length - 1] === "&&" || parts[parts.length - 1] === "&")
    )
      parts.pop();
    if (parts.length > 0 || d) parts.push("|");
    props.onChange(buildQuery(parts, ""));
    queueMicrotask(() => draftRef?.focus());
  }

  // ── Render ────────────────────────────────────────────────────────────────

  return (
    <div ref={containerRef} style={{ position: "relative", flex: "1", "min-width": "280px" }}>
      {/* ── Pill + draft input container ─────────────────────────────────── */}
      {/*                                                                      */}
      {/* Single flex-wrap container. Everything is an inline flex item:       */}
      {/*   ⌕ icon | pills | [AND] | [OR] | draft + +AND + +OR + ✕           */}
      {/*                                                                      */}
      {/* The last group (input + buttons) has min-width so it wraps as a     */}
      {/* unit — preventing the buttons from separating from the input.        */}
      {/* ⌕ and ✕ are inline (no absolute positioning) so they follow         */}
      {/* the natural flex height as pills wrap to multiple rows.              */}
      <div
        onClick={() => draftRef?.focus()}
        style={{
          display: "flex",
          "flex-wrap": "wrap",
          "align-items": "center",
          gap: "3px",
          "min-height": "26px",
          padding: "3px 6px",
          background: "var(--bg-primary)",
          border: `1px solid ${isActive() ? "var(--accent)" : "var(--border)"}`,
          "border-radius": "4px",
          cursor: "text",
          transition: "border-color 0.1s",
        }}
      >
        {/* Search icon — inline flex item, always at start of first row */}
        <span
          style={{
            "flex-shrink": "0",
            color: "var(--text-muted)",
            "font-size": "11px",
            "pointer-events": "none",
            opacity: "0.6",
            "line-height": "1",
          }}
        >
          ⌕
        </span>

        {/* ── Pill groups ─────────────────────────────────────────────── */}
        <For each={pillGroups()}>
          {(group, gi) => (
            <Show when={group.length > 0}>
              <>
                {/* OR badge between groups */}
                <Show when={gi() > 0}>
                  <span
                    style={{
                      "font-size": "9px",
                      "font-weight": "700",
                      "letter-spacing": "0.05em",
                      color: "var(--accent)",
                      background: "rgba(var(--accent-rgb,59,130,246),0.13)",
                      border: "1px solid rgba(var(--accent-rgb,59,130,246),0.35)",
                      "border-radius": "10px",
                      padding: "1px 5px",
                      "flex-shrink": "0",
                      "user-select": "none",
                    }}
                  >
                    OR
                  </span>
                </Show>

                {/* Tokens in this group */}
                <For each={group}>
                  {(token, ti) => {
                    const s = getTokenStyle(token);
                    const negated = token.startsWith("-");
                    return (
                      <>
                        <Show
                          when={inlineEdit()?.groupIdx === gi() && inlineEdit()?.tokenIdx === ti()}
                        >
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
                        {/* AND badge between pills in the same group */}
                        <Show when={ti() > 0}>
                          <span
                            style={{
                              "font-size": "9px",
                              "font-weight": "600",
                              "letter-spacing": "0.04em",
                              color: "var(--text-muted)",
                              border: "1px dashed var(--border)",
                              "border-radius": "10px",
                              padding: "1px 5px",
                              "flex-shrink": "0",
                              "user-select": "none",
                            }}
                          >
                            AND
                          </span>
                        </Show>

                        {/* Token pill — stop click bubbling so the container does not
                            focus the draft on the same gesture (would skip focusing
                            the inline editor and leave the pill removed). */}
                        <span
                          onClick={(e) => e.stopPropagation()}
                          style={{
                            display: "inline-flex",
                            "align-items": "center",
                            gap: "2px",
                            "font-size": "10px",
                            "font-family": "var(--font-mono)",
                            color: s.color,
                            background: s.bg,
                            border: `1px solid ${s.border}`,
                            "border-radius": "10px",
                            padding: "1px 4px 1px 6px",
                            "flex-shrink": "0",
                            "white-space": "nowrap",
                            opacity: negated ? "0.65" : "1",
                          }}
                        >
                          <span
                            onMouseDown={(e) => {
                              e.preventDefault();
                              e.stopPropagation();
                              editToken(gi(), ti(), token);
                            }}
                            title="Edit filter"
                            style={{ cursor: "text", "user-select": "none" }}
                          >
                            {token}
                          </span>
                          <button
                            onMouseDown={(e) => {
                              e.preventDefault();
                              e.stopPropagation();
                              removeToken(gi(), ti());
                            }}
                            title="Remove filter"
                            style={{
                              background: "none",
                              border: "none",
                              color: s.color,
                              cursor: "pointer",
                              padding: "0 1px",
                              "font-size": "9px",
                              "line-height": "1",
                              opacity: "0.55",
                              display: "flex",
                              "align-items": "center",
                            }}
                            onMouseEnter={(e) => {
                              (e.currentTarget as HTMLElement).style.opacity = "1";
                            }}
                            onMouseLeave={(e) => {
                              (e.currentTarget as HTMLElement).style.opacity = "0.55";
                            }}
                          >
                            ✕
                          </button>
                        </span>
                      </>
                    );
                  }}
                </For>
                <Show
                  when={inlineEdit()?.groupIdx === gi() && inlineEdit()?.tokenIdx === group.length}
                >
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
              </>
            </Show>
          )}
        </For>

        <Show when={inlineEditOrphanAfterOrBranch()}>
          <span
            onClick={(e) => e.stopPropagation()}
            style={{ display: "inline-flex", "align-items": "center", gap: "3px" }}
          >
            <span
              style={{
                "font-size": "9px",
                "font-weight": "700",
                "letter-spacing": "0.05em",
                color: "var(--accent)",
                background: "rgba(var(--accent-rgb,59,130,246),0.13)",
                border: "1px solid rgba(var(--accent-rgb,59,130,246),0.35)",
                "border-radius": "10px",
                padding: "1px 5px",
                "flex-shrink": "0",
                "user-select": "none",
              }}
            >
              OR
            </span>
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
          </span>
        </Show>

        <Show when={!!inlineEdit() && totalPillCount() === 0}>
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

        {/* OR badge before the draft when it starts a new group */}
        <Show when={draftInNewGroup() && hasAnyPills()}>
          <span
            style={{
              "font-size": "9px",
              "font-weight": "700",
              "letter-spacing": "0.05em",
              color: "var(--accent)",
              background: "rgba(var(--accent-rgb,59,130,246),0.13)",
              border: "1px solid rgba(var(--accent-rgb,59,130,246),0.35)",
              "border-radius": "10px",
              padding: "1px 5px",
              "flex-shrink": "0",
              "user-select": "none",
            }}
          >
            OR
          </span>
        </Show>

        {/* ── Input row — draft + connector buttons + clear ──────────────── */}
        {/*                                                                    */}
        {/* Wrapped in a non-wrapping sub-row (flex, no wrap) so that the      */}
        {/* input and buttons always stay together on the same line.           */}
        {/* min-width forces this group to wrap as a unit when pills fill      */}
        {/* the available horizontal space.                                    */}
        <div
          style={{
            flex: "1",
            display: "flex",
            "align-items": "center",
            gap: "3px",
            "min-width": "180px",
          }}
        >
          {/* Draft input */}
          <input
            ref={draftRef}
            type="text"
            spellcheck={false}
            value={draft()}
            onInput={handleDraftInput}
            onPaste={handleDraftPaste}
            onKeyDown={handleKeyDown}
            onFocus={() => setOpen(true)}
            onClick={() => setOpen(true)}
            onBlur={handleBlur}
            placeholder={
              hasAnyPills()
                ? "Add condition…"
                : (props.placeholder ?? "Filter… (level:error, tag:App, is:crash…)")
            }
            style={{
              flex: "1",
              "min-width": "0", // allow shrinking below content size in flex
              background: "transparent",
              border: "none",
              color: "var(--text-primary)",
              "font-size": "11px",
              "font-family": "var(--font-mono)",
              outline: "none",
              padding: "1px 0",
            }}
          />

          {/* + AND / + OR connector buttons */}
          <Show when={isActive()}>
            <div style={{ display: "flex", gap: "3px", "flex-shrink": "0" }}>
              <button
                onMouseDown={(e) => {
                  e.preventDefault();
                  handleAddAndConnector();
                }}
                title="Add AND condition — both conditions must match"
                style={connectorBtnStyle()}
                onMouseEnter={(e) => {
                  (e.currentTarget as HTMLElement).style.color = "var(--success)";
                  (e.currentTarget as HTMLElement).style.borderColor = "var(--success)";
                }}
                onMouseLeave={(e) => {
                  (e.currentTarget as HTMLElement).style.color = "var(--text-muted)";
                  (e.currentTarget as HTMLElement).style.borderColor = "var(--border)";
                }}
              >
                + AND
              </button>
              <button
                onMouseDown={(e) => {
                  e.preventDefault();
                  handleAddOrGroup();
                }}
                title="Add OR group — entries matching either group will be shown"
                style={connectorBtnStyle()}
                onMouseEnter={(e) => {
                  (e.currentTarget as HTMLElement).style.color = "var(--accent)";
                  (e.currentTarget as HTMLElement).style.borderColor = "var(--accent)";
                }}
                onMouseLeave={(e) => {
                  (e.currentTarget as HTMLElement).style.color = "var(--text-muted)";
                  (e.currentTarget as HTMLElement).style.borderColor = "var(--border)";
                }}
              >
                + OR
              </button>
            </div>
          </Show>

          {/* Clear all — inline at the end of the input row */}
          <Show when={isActive()}>
            <button
              onMouseDown={(e) => {
                e.preventDefault();
                setInlineEdit(null);
                props.onChange("");
                draftRef?.focus();
              }}
              title="Clear all filters (Esc)"
              style={{
                "flex-shrink": "0",
                background: "none",
                border: "none",
                color: "var(--text-muted)",
                cursor: "pointer",
                "font-size": "11px",
                padding: "0 2px",
                "line-height": "1",
                display: "flex",
                "align-items": "center",
                opacity: "0.6",
              }}
              onMouseEnter={(e) => {
                (e.currentTarget as HTMLElement).style.opacity = "1";
              }}
              onMouseLeave={(e) => {
                (e.currentTarget as HTMLElement).style.opacity = "0.6";
              }}
            >
              ✕
            </button>
          </Show>
        </div>
      </div>

      {/* ── Autocomplete dropdown ───────────────────────────────────────── */}
      <Show when={hasSuggestions()}>
        <div
          style={{
            position: "absolute",
            top: "calc(100% + 3px)",
            left: "0",
            "min-width": "220px",
            "max-width": "360px",
            "max-height": "260px",
            "overflow-y": "auto",
            background: "var(--bg-secondary)",
            border: "1px solid var(--border)",
            "border-radius": "4px",
            "box-shadow": "0 6px 20px rgba(0,0,0,0.45)",
            "z-index": "600",
          }}
        >
          <For each={suggestions()}>
            {(s, i) => {
              const isSelected = () => i() === selectedIdx();
              return (
                <div
                  onMouseDown={(e) => {
                    e.preventDefault();
                    applySelection(s.insert);
                  }}
                  onMouseEnter={() => setSelectedIdx(i())}
                  style={{
                    padding: "5px 10px",
                    "font-size": "11px",
                    "font-family": "var(--font-mono)",
                    color: isSelected() ? "#fff" : "var(--text-primary)",
                    background: isSelected() ? "var(--accent)" : "transparent",
                    cursor: "pointer",
                    "white-space": "nowrap",
                    overflow: "hidden",
                    "text-overflow": "ellipsis",
                  }}
                >
                  {s.display}
                </div>
              );
            }}
          </For>
          <div
            style={{
              padding: "4px 10px",
              "font-size": "10px",
              color: "var(--text-disabled, #4b5563)",
              "border-top": "1px solid var(--border)",
              "user-select": "none",
            }}
          >
            ↑↓ navigate · Tab/Enter select · Esc close · && = AND · | = OR group
          </div>
        </div>
      </Show>
    </div>
  );
}

// ── Style helper ──────────────────────────────────────────────────────────────

function connectorBtnStyle(): Record<string, string> {
  return {
    background: "none",
    border: "1px solid var(--border)",
    color: "var(--text-muted)",
    "border-radius": "4px",
    cursor: "pointer",
    "font-size": "10px",
    "font-family": "var(--font-mono)",
    padding: "2px 6px",
    "white-space": "nowrap",
    transition: "color 0.1s, border-color 0.1s",
  };
}

export default QueryBar;
