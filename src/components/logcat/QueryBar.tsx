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

import { type JSX, createSignal, createMemo, For, Show, untrack } from "solid-js";
import {
  getQueryBarSuggestions,
  balanceMessageDraftQuotes,
  buildQueryBarPillGroups,
  buildQueryBarPillRefGroups,
  applyMessageKeySpaceAutoQuote,
  pasteIntoMessageKeyDraft,
  rebuildCommittedAfterRemovingPill,
  committedEndsWithOrSeparator,
  serializeQueryBarCommittedPart,
  insertPillAtGroupPosition,
  applyInlineEditCommit,
  commitQueryBarDraft,
  toggleQueryBarAndConnector,
  toggleQueryBarOrConnector,
} from "@/lib/logcat-query";
import {
  buildCommittedWithAndConnector,
  buildCommittedWithOrGroup,
  buildDraftAfterSuggestion,
  buildQuery,
  parseQueryState,
  removeLastCommittedPill,
} from "./querybar-query-state";
import {
  QueryBarAndBadge,
  QueryBarConnectorButton,
  QueryBarInlineEditInput,
  QueryBarOrBadge,
  QueryBarPill,
  QueryBarSuggestions,
} from "./QueryBarParts";
import { Icon } from "@/components/ui";
import {
  queryBarClearButtonStyle,
  queryBarConnectorGroupStyle,
  queryBarContainerStyle,
  queryBarDraftInputStyle,
  queryBarGroupBoxStyle,
  queryBarInputRowStyle,
  queryBarOrphanInlineEditStyle,
  queryBarRootStyle,
  searchIconStyle,
} from "./querybar-styles";

export { buildQuery, parseQueryState } from "./querybar-query-state";

// ── Props ─────────────────────────────────────────────────────────────────────

export interface QueryBarProps {
  value: string;
  onChange: (query: string) => void;
  knownTags: string[];
  knownPackages: string[];
  placeholder?: string;
  disabledPillIds?: ReadonlySet<string>;
  onTogglePillDisabled?: (id: string) => void;
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
  const pillRefGroups = createMemo(() => buildQueryBarPillRefGroups(committed()));
  const multiGroup = createMemo(() => pillGroups().filter((g) => g.length > 0).length >= 2);

  function totalPillCount(): number {
    return pillGroups().reduce((n, gr) => n + gr.length, 0);
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

  const suggestions = createMemo(() =>
    getQueryBarSuggestions(draft(), props.knownTags, props.knownPackages)
  );

  const hasSuggestions = () => open() && suggestions().length > 0;

  // ── Autocomplete selection ────────────────────────────────────────────────

  function applySelection(insert: string) {
    const newDraft = buildDraftAfterSuggestion(draft(), insert);
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
      props.onChange(buildQuery(removeLastCommittedPill(committed()), ""));
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

    if (e.key === "Enter" && draft().trim() !== "") {
      const selectedSuggestion = open() ? suggs[selectedIdx()] : undefined;
      if (!selectedSuggestion) {
        e.preventDefault();
        const next = commitQueryBarDraft(committed(), draft());
        props.onChange(buildQuery(next, ""));
        setOpen(false);
        setSelectedIdx(0);
        return;
      }
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

  function handleInlineInput(value: string) {
    setInlineEdit((s) => (s ? { ...s, text: value } : null));
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
      untrack(() => {
        if (inlineEdit()) commitInlineEdit();
      });
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
    props.onChange(buildQuery(buildCommittedWithAndConnector(committed(), draft()), ""));
    queueMicrotask(() => draftRef?.focus());
  }

  function handleAddOrGroup() {
    if (inlineEdit()) commitInlineEdit();
    props.onChange(buildQuery(buildCommittedWithOrGroup(committed(), draft()), ""));
    queueMicrotask(() => draftRef?.focus());
  }

  function toggleAndConnector(groupIdx: number, tokenIdxAfterConnector: number) {
    if (inlineEdit()) commitInlineEdit();
    setOpen(false);
    setSelectedIdx(0);
    const next = toggleQueryBarAndConnector(
      committed(),
      groupIdx,
      tokenIdxAfterConnector,
      draftInNewGroup()
    );
    props.onChange(buildQuery(next, draft()));
  }

  function toggleOrConnector(groupIdxAfterConnector: number) {
    if (inlineEdit()) commitInlineEdit();
    setOpen(false);
    setSelectedIdx(0);
    const next = toggleQueryBarOrConnector(committed(), groupIdxAfterConnector, draftInNewGroup());
    props.onChange(buildQuery(next, draft()));
  }

  // ── Render ────────────────────────────────────────────────────────────────

  return (
    <div ref={containerRef} style={queryBarRootStyle()}>
      {/* ── Pill + draft input container ─────────────────────────────────── */}
      {/*                                                                      */}
      {/* Single flex-wrap container. Everything is an inline flex item:       */}
      {/*   ⌕ icon | pills | [AND] | [OR] | draft + +AND + +OR + ✕           */}
      {/*                                                                      */}
      {/* The last group (input + buttons) has min-width so it wraps as a     */}
      {/* unit — preventing the buttons from separating from the input.        */}
      {/* ⌕ and ✕ are inline (no absolute positioning) so they follow         */}
      {/* the natural flex height as pills wrap to multiple rows.              */}
      <div onClick={() => draftRef?.focus()} style={queryBarContainerStyle(isActive())}>
        {/* Search icon — inline flex item, always at start of first row */}
        <span style={searchIconStyle()}>
          <Icon name="search" size={12} />
        </span>

        {/* ── Pill groups ─────────────────────────────────────────────── */}
        <For each={pillRefGroups()}>
          {(group, gi) => {
            const groupIdx = () => group[0]?.groupIdx ?? gi();
            return (
              <Show when={group.length > 0}>
                <>
                  {/* OR badge between groups */}
                  <Show when={gi() > 0}>
                    <QueryBarOrBadge onToggle={() => toggleOrConnector(groupIdx())} />
                  </Show>

                  {/* Group container: visible box only when 2+ groups exist */}
                  <div style={multiGroup() ? queryBarGroupBoxStyle() : { display: "contents" }}>
                    {/* Tokens in this group */}
                    <For each={group}>
                      {(tokenRef, ti) => (
                        <>
                          <Show
                            when={
                              inlineEdit()?.groupIdx === tokenRef.groupIdx &&
                              inlineEdit()?.tokenIdx === tokenRef.tokenIdx
                            }
                          >
                            <QueryBarInlineEditInput
                              inputRef={(el) => {
                                inlineEditRef = el;
                              }}
                              value={inlineEdit()?.text ?? ""}
                              onInput={handleInlineInput}
                              onKeyDown={handleInlineKeyDown}
                              onBlur={handleInlineBlur}
                              style={{ flex: "0 1 auto" }}
                            />
                          </Show>
                          {/* AND badge between pills in the same group */}
                          <Show when={ti() > 0}>
                            <QueryBarAndBadge
                              onToggle={() =>
                                toggleAndConnector(tokenRef.groupIdx, tokenRef.tokenIdx)
                              }
                            />
                          </Show>

                          <QueryBarPill
                            token={tokenRef.token}
                            disabled={props.disabledPillIds?.has(tokenRef.id) ?? false}
                            onToggleDisabled={
                              props.onTogglePillDisabled
                                ? () => props.onTogglePillDisabled?.(tokenRef.id)
                                : undefined
                            }
                            onEdit={() =>
                              editToken(tokenRef.groupIdx, tokenRef.tokenIdx, tokenRef.token)
                            }
                            onRemove={() => removeToken(tokenRef.groupIdx, tokenRef.tokenIdx)}
                          />
                        </>
                      )}
                    </For>
                    <Show
                      when={
                        inlineEdit()?.groupIdx === groupIdx() &&
                        inlineEdit()?.tokenIdx === group.length
                      }
                    >
                      <QueryBarInlineEditInput
                        inputRef={(el) => {
                          inlineEditRef = el;
                        }}
                        value={inlineEdit()?.text ?? ""}
                        onInput={handleInlineInput}
                        onKeyDown={handleInlineKeyDown}
                        onBlur={handleInlineBlur}
                        style={{ flex: "0 1 auto" }}
                      />
                    </Show>
                  </div>
                </>
              </Show>
            );
          }}
        </For>

        <Show when={inlineEditOrphanAfterOrBranch()}>
          <span onClick={(e) => e.stopPropagation()} style={queryBarOrphanInlineEditStyle()}>
            <QueryBarOrBadge />
            <QueryBarInlineEditInput
              inputRef={(el) => {
                inlineEditRef = el;
              }}
              value={inlineEdit()?.text ?? ""}
              onInput={handleInlineInput}
              onKeyDown={handleInlineKeyDown}
              onBlur={handleInlineBlur}
              style={{ "flex-shrink": "1" }}
            />
          </span>
        </Show>

        <Show when={!!inlineEdit() && totalPillCount() === 0}>
          <QueryBarInlineEditInput
            inputRef={(el) => {
              inlineEditRef = el;
            }}
            value={inlineEdit()?.text ?? ""}
            onInput={handleInlineInput}
            onKeyDown={handleInlineKeyDown}
            onBlur={handleInlineBlur}
            style={{ "flex-shrink": "1" }}
          />
        </Show>

        {/* OR badge before the draft when it starts a new group */}
        <Show when={draftInNewGroup() && hasAnyPills()}>
          <QueryBarOrBadge />
        </Show>

        {/* ── Input row — draft + connector buttons + clear ──────────────── */}
        {/*                                                                    */}
        {/* Wrapped in a non-wrapping sub-row (flex, no wrap) so that the      */}
        {/* input and buttons always stay together on the same line.           */}
        {/* min-width forces this group to wrap as a unit when pills fill      */}
        {/* the available horizontal space.                                    */}
        <div style={queryBarInputRowStyle()}>
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
            style={queryBarDraftInputStyle()}
          />

          {/* + AND / + OR connector buttons */}
          <Show when={isActive()}>
            <div style={queryBarConnectorGroupStyle()}>
              <QueryBarConnectorButton
                title="Add AND condition — both conditions must match"
                hoverColor="var(--success)"
                onMouseDown={handleAddAndConnector}
              >
                + AND
              </QueryBarConnectorButton>
              <QueryBarConnectorButton
                title="Add OR group — entries matching either group will be shown"
                hoverColor="var(--accent)"
                onMouseDown={handleAddOrGroup}
              >
                + OR
              </QueryBarConnectorButton>
            </div>
          </Show>

          {/* Clear all — inline at the end of the input row */}
          <Show when={isActive()}>
            <button
              onMouseDown={(e) => {
                e.preventDefault();
                setInlineEdit(null);
                setOpen(false);
                setSelectedIdx(0);
                props.onChange("");
              }}
              title="Clear all filters (Esc)"
              style={queryBarClearButtonStyle()}
              onMouseEnter={(e) => {
                (e.currentTarget as HTMLElement).style.opacity = "1";
              }}
              onMouseLeave={(e) => {
                (e.currentTarget as HTMLElement).style.opacity = "0.6";
              }}
            >
              <Icon name="close" size={11} />
            </button>
          </Show>
        </div>
      </div>

      {/* ── Autocomplete dropdown ───────────────────────────────────────── */}
      <Show when={hasSuggestions()}>
        <QueryBarSuggestions
          suggestions={suggestions()}
          selectedIdx={selectedIdx()}
          onSelect={applySelection}
          onHover={setSelectedIdx}
        />
      </Show>
    </div>
  );
}

export default QueryBar;
