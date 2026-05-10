import type { QueryToken } from "./logcat-query-types";

/**
 * Identify which tokens in a group must be evaluated on the frontend.
 *
 * The Rust backend filter spec has exactly one slot per field:
 *   minLevel, tag (single substring), text (shared by message: and freetext),
 *   package, and an onlyCrashes boolean.
 *
 * This function returns every token that the backend either cannot handle at
 * all, or cannot handle because its slot is already occupied by an earlier
 * token of the same type ("overflow").
 *
 * Tokens always handled on the frontend:
 *   • age:N          — time-based, requires live Date.now()
 *   • negated (-X)   — backend has no negation support
 *   • tag~: / msg~:  — backend does substring only, not regex
 *   • is:stacktrace  — backend does not have a stacktrace filter
 *   • pid/tid/time   — backend filter spec has no exact metadata slots
 *
 * Overflow tokens (second+ occurrence of a backend-handled type):
 *   • 2nd+ level:    — only the first minLevel is sent
 *   • 2nd+ tag:      — only the first tag substring is sent
 *   • 2nd+ message:  — only the first text slot is sent
 *   • 2nd+ freetext  — shares the same text slot as message:
 *   • 2nd+ package:  — only the first package is sent
 *
 * For example, `message:socket message:IPPROTO_TCP` — the backend pre-filters
 * by "socket"; the frontend must additionally verify "IPPROTO_TCP" for correct
 * AND semantics.
 */
export function getFrontendOnlyTokens(tokens: QueryToken[]): QueryToken[] {
  // Track which backend spec slots have been consumed.
  let levelConsumed = false;
  let tagConsumed = false;
  let textConsumed = false; // shared by message: and freetext
  let packageConsumed = false;

  return tokens.filter((token) => {
    // Always frontend-only
    if (token.type === "age") return true;
    if ("negate" in token && token.negate) return true;
    if ((token.type === "tag" || token.type === "message") && token.regex) return true;
    // is:stacktrace — backend has no handler for this
    if (token.type === "is" && token.value === "stacktrace") return true;

    // For backend-handleable tokens: first occurrence goes to backend, rest → frontend
    switch (token.type) {
      case "level":
        if (!levelConsumed) {
          levelConsumed = true;
          return false;
        }
        return true;
      case "tag":
        if (!tagConsumed) {
          tagConsumed = true;
          return false;
        }
        return true;
      case "message":
        if (!textConsumed) {
          textConsumed = true;
          return false;
        }
        return true;
      case "package":
        if (!packageConsumed) {
          packageConsumed = true;
          return false;
        }
        return true;
      case "pid":
      case "tid":
      case "time":
        return true;
      case "is":
        // is:crash → onlyCrashes flag (boolean, no overflow); handled above for stacktrace
        return false;
      case "freetext":
        if (!textConsumed) {
          textConsumed = true;
          return false;
        }
        return true;
      default:
        return false;
    }
  });
}
