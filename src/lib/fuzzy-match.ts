export interface FuzzyResult<T> {
  item: T;
  score: number;
  /** Indices into the text that matched the query */
  matchedIndices: number[];
}

/**
 * Fuzzy match a query against a text string.
 * Returns a score (higher = better match) and the indices of matched characters.
 * Returns null if no match.
 */
export function fuzzyMatch(
  query: string,
  text: string
): { score: number; matchedIndices: number[] } | null {
  const queryLower = query.toLowerCase();
  const textLower = text.toLowerCase();

  if (queryLower.length === 0) return { score: 0, matchedIndices: [] };
  if (queryLower.length > textLower.length) return null;

  const matchedIndices: number[] = [];
  let score = 0;
  let qi = 0;
  let prevMatchIdx = -2;

  for (let ti = 0; ti < textLower.length && qi < queryLower.length; ti++) {
    if (textLower[ti] === queryLower[qi]) {
      matchedIndices.push(ti);

      // Consecutive match bonus
      if (ti === prevMatchIdx + 1) {
        score += 5;
      }

      // Word boundary bonus (after /, ., -, _, space, or uppercase in camelCase)
      if (ti === 0) {
        score += 10;
      } else {
        const prev = text[ti - 1];
        const curr = text[ti];
        if (
          prev === "/" ||
          prev === "." ||
          prev === "-" ||
          prev === "_" ||
          prev === " " ||
          (prev === prev.toLowerCase() && curr === curr.toUpperCase())
        ) {
          score += 8;
        }
      }

      // Exact case match bonus
      if (text[ti] === query[qi]) {
        score += 1;
      }

      prevMatchIdx = ti;
      qi++;
    }
  }

  if (qi < queryLower.length) return null;

  // Penalty for longer strings (prefer shorter, more specific matches)
  score -= Math.floor(text.length / 10);

  // Bonus for matching near the start
  if (matchedIndices.length > 0) {
    score -= Math.floor(matchedIndices[0] / 5);
  }

  return { score, matchedIndices };
}

/**
 * Rank a list of items by fuzzy matching against a query.
 * `getText` extracts the searchable text from each item.
 * Returns items sorted by score (best first), filtered to only matches.
 */
export function fuzzyFilter<T>(
  items: T[],
  query: string,
  getText: (item: T) => string
): FuzzyResult<T>[] {
  if (!query.trim()) {
    return items.map((item) => ({ item, score: 0, matchedIndices: [] }));
  }

  const results: FuzzyResult<T>[] = [];

  for (const item of items) {
    const text = getText(item);
    const match = fuzzyMatch(query, text);
    if (match) {
      results.push({
        item,
        score: match.score,
        matchedIndices: match.matchedIndices,
      });
    }
  }

  results.sort((a, b) => b.score - a.score);
  return results;
}
