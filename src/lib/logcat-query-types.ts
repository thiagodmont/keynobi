export type QueryToken =
  | { type: "level"; value: string; negate: boolean }
  | { type: "tag"; value: string; negate: boolean; regex: boolean }
  | { type: "message"; value: string; negate: boolean; regex: boolean }
  | { type: "package"; value: string; negate: boolean }
  | { type: "pid"; value: number; negate: boolean }
  | { type: "tid"; value: number; negate: boolean }
  | { type: "time"; value: string; negate: boolean }
  | { type: "age"; seconds: number }
  | { type: "is"; value: string }
  | { type: "freetext"; value: string; negate: boolean };

/**
 * A FilterGroup is a set of tokens that are AND-ed together.
 * Multiple groups joined by `|` are OR-ed together.
 */
export type FilterGroup = QueryToken[];
