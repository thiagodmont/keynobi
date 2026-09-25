/**
 * A binding type as it arrives over IPC. Payloads are JSON, so a Rust integer
 * the bindings declare as `bigint` arrives as a `number`.
 */
export type Wire<T> = T extends bigint
  ? number
  : T extends object
    ? { [K in keyof T]: Wire<T[K]> }
    : T;
