/**
 * QueryBar — smart single-input query field for logcat filtering.
 *
 * Replaces the four separate filter controls (level dropdown, tag input,
 * text input, package autocomplete) with a single unified query bar that
 * supports the full logcat query language from logcat-query.ts.
 *
 * Autocomplete is context-aware:
 *   - After "level:"   → level name suggestions
 *   - After "tag:"     → frequent tags from current entries
 *   - After "package:" → known packages + "mine"
 *   - After "is:"      → crash, stacktrace
 *   - After "age:"     → 30s, 1m, 5m, 15m, 1h, 6h, 1d
 *   - Empty / bare     → key name suggestions + quick shortcuts
 */

import {
  type JSX,
  createSignal,
  createMemo,
  For,
  Show,
} from "solid-js";
import {
  QUERY_KEYS,
  LEVEL_NAMES,
  AGE_SUGGESTIONS,
  IS_SUGGESTIONS,
  getActiveTokenContext,
} from "@/lib/logcat-query";

// ── Props ─────────────────────────────────────────────────────────────────────

export interface QueryBarProps {
  value: string;
  onChange: (query: string) => void;
  /** Frequent tags from the current buffer (for autocomplete). */
  knownTags: string[];
  /** Known package names (for autocomplete). */
  knownPackages: string[];
  placeholder?: string;
}

// ── Component ─────────────────────────────────────────────────────────────────

const MAX_SUGGESTIONS = 10;

export function QueryBar(props: QueryBarProps): JSX.Element {
  let inputRef!: HTMLInputElement;
  let containerRef!: HTMLDivElement;

  const [selectedIdx, setSelectedIdx] = createSignal(0);
  const [open, setOpen] = createSignal(false);

  // ── Suggestion computation ────────────────────────────────────────────────

  const suggestions = createMemo(() => {
    const q = props.value;
    const ctx = getActiveTokenContext(q);

    // After a specific key:
    if (ctx.key) {
      const partial = ctx.partial.toLowerCase();
      switch (ctx.key.toLowerCase()) {
        case "level":
          return LEVEL_NAMES
            .filter((l) => l.startsWith(partial))
            .map((l) => ({ display: l, insert: l }));

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
          return IS_SUGGESTIONS
            .filter((s) => s.startsWith(partial))
            .map((s) => ({ display: s, insert: s }));

        case "age":
          return AGE_SUGGESTIONS
            .filter((a) => a.startsWith(partial))
            .map((a) => ({ display: a, insert: a }));

        default:
          return [];
      }
    }

    // No key — suggest key names or quick bare-level shortcuts
    const partial = ctx.partial.toLowerCase();
    if (!partial) {
      return QUERY_KEYS.map((k) => ({ display: k, insert: k }));
    }

    const keySuggestions = QUERY_KEYS
      .filter((k) => k.toLowerCase().startsWith(partial))
      .map((k) => ({ display: k, insert: k }));

    const levelSuggestions = LEVEL_NAMES
      .filter((l) => l.startsWith(partial))
      .map((l) => ({ display: `${l} (level shorthand)`, insert: l }));

    return [...keySuggestions, ...levelSuggestions].slice(0, MAX_SUGGESTIONS);
  });

  // ── Keyboard handling ─────────────────────────────────────────────────────

  function handleKeyDown(e: KeyboardEvent) {
    const suggs = suggestions();

    if (e.key === "Escape") {
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

  function applySelection(insert: string) {
    const ctx = getActiveTokenContext(props.value);
    const before = props.value.slice(0, ctx.offset);

    let replacement: string;
    if (ctx.key) {
      // Replace only the value part after the colon
      const keyPart = props.value.slice(ctx.offset, ctx.offset + ctx.key.length + 1);
      replacement = `${before}${keyPart}${insert} `;
    } else {
      // Replace the whole last partial token
      replacement = `${before}${insert}`;
      // If it ends with ":" we don't add a space yet — user will type value next
      if (!insert.endsWith(":") && !insert.endsWith("~:")) {
        replacement += " ";
      }
    }

    props.onChange(replacement);
    setOpen(false);
    setSelectedIdx(0);
    queueMicrotask(() => inputRef?.focus());
  }

  function handleInput(e: InputEvent) {
    props.onChange((e.currentTarget as HTMLInputElement).value);
    setSelectedIdx(0);
    setOpen(true);
  }

  function handleFocus() {
    setOpen(true);
  }

  function handleBlur() {
    setTimeout(() => {
      if (!containerRef?.contains(document.activeElement)) {
        setOpen(false);
      }
    }, 150);
  }

  const hasSuggestions = () => open() && suggestions().length > 0;
  const isActive = () => props.value.trim() !== "";

  return (
    <div
      ref={containerRef}
      style={{ position: "relative", flex: "1", "min-width": "180px", "max-width": "420px" }}
    >
      <div style={{ position: "relative", display: "flex", "align-items": "center" }}>
        {/* Search icon */}
        <span
          style={{
            position: "absolute",
            left: "7px",
            color: "var(--text-muted)",
            "font-size": "11px",
            "pointer-events": "none",
            opacity: "0.6",
          }}
        >
          ⌕
        </span>

        <input
          ref={inputRef}
          type="text"
          spellcheck={false}
          placeholder={props.placeholder ?? "Filter logs… (level:error -tag:system age:5m)"}
          value={props.value}
          onInput={handleInput}
          onKeyDown={handleKeyDown}
          onFocus={handleFocus}
          onBlur={handleBlur}
          style={{
            width: "100%",
            background: "var(--bg-primary)",
            border: `1px solid ${isActive() ? "var(--accent)" : "var(--border)"}`,
            color: "var(--text-primary)",
            "border-radius": "4px",
            padding: "3px 24px 3px 22px",
            "font-size": "11px",
            "font-family": "var(--font-mono)",
            outline: "none",
            transition: "border-color 0.1s",
          }}
        />

        {/* Clear button */}
        <Show when={isActive()}>
          <button
            onMouseDown={(e) => { e.preventDefault(); props.onChange(""); inputRef?.focus(); }}
            title="Clear query (Esc)"
            style={{
              position: "absolute",
              right: "5px",
              background: "none",
              border: "none",
              color: "var(--text-muted)",
              cursor: "pointer",
              "font-size": "11px",
              padding: "0",
              "line-height": "1",
              display: "flex",
              "align-items": "center",
            }}
          >
            ✕
          </button>
        </Show>
      </div>

      {/* Autocomplete dropdown */}
      <Show when={hasSuggestions()}>
        <div
          style={{
            position: "absolute",
            top: "calc(100% + 3px)",
            left: "0",
            "min-width": "100%",
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
                  onMouseDown={(e) => { e.preventDefault(); applySelection(s.insert); }}
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

          {/* Footer hint */}
          <div
            style={{
              padding: "4px 10px",
              "font-size": "10px",
              color: "var(--text-disabled, #4b5563)",
              "border-top": "1px solid var(--border)",
              "user-select": "none",
            }}
          >
            ↑↓ navigate · Tab/Enter select · Esc close
          </div>
        </div>
      </Show>
    </div>
  );
}

export default QueryBar;
