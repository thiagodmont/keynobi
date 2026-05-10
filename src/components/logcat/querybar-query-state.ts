import {
  balanceMessageDraftQuotes,
  getActiveTokenContext,
  parseQueryBarState,
  serializeQueryBarCommittedPart,
} from "@/lib/logcat-query";

/**
 * Split the full query into committed parts and the trailing draft.
 * Uses the same lexer as logcat query parsing.
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

export function buildDraftAfterSuggestion(draft: string, insert: string): string {
  const ctx = getActiveTokenContext(draft);
  const draftBefore = draft.slice(0, ctx.offset);

  if (ctx.key) {
    // Use the actual colon position to reconstruct the key part. Computing
    // key.length would be wrong for regex variants like `tag~:`.
    const colonPos = draft.indexOf(":", ctx.offset);
    const keyPart =
      colonPos >= 0
        ? draft.slice(ctx.offset, colonPos + 1)
        : draft.slice(ctx.offset, ctx.offset + ctx.key.length + 1);
    return `${draftBefore}${keyPart}${insert} `;
  }

  // Existing behavior preserves a leading negation prefix from the full draft.
  const negationPrefix = draft.startsWith("-") ? "-" : "";
  const nextDraft = `${draftBefore}${negationPrefix}${insert}`;
  return insert.endsWith(":") || insert.endsWith("~:") ? nextDraft : `${nextDraft} `;
}

export function removeLastCommittedPill(committed: readonly string[]): string[] {
  const parts = [...committed];
  while (endsWithConnector(parts)) parts.pop();
  if (parts.length > 0) parts.pop();
  while (endsWithConnector(parts)) parts.pop();
  return parts;
}

export function buildCommittedWithAndConnector(
  committed: readonly string[],
  draft: string
): string[] {
  const d = balanceMessageDraftQuotes(draft.trim());
  const parts = [...committed];
  if (d) parts.push(d);
  while (parts.length > 0 && parts[parts.length - 1] === "|") parts.pop();
  const last = parts[parts.length - 1];
  if (parts.length > 0 && last !== "&&" && last !== "&") parts.push("&&");
  return parts;
}

export function buildCommittedWithOrGroup(committed: readonly string[], draft: string): string[] {
  const d = balanceMessageDraftQuotes(draft.trim());
  const parts = [...committed];
  if (d) parts.push(d);
  while (
    parts.length > 0 &&
    (parts[parts.length - 1] === "&&" || parts[parts.length - 1] === "&")
  ) {
    parts.pop();
  }
  if (parts.length > 0 || d) parts.push("|");
  return parts;
}

function endsWithConnector(parts: readonly string[]): boolean {
  const last = parts[parts.length - 1];
  return last === "|" || last === "&&" || last === "&";
}
