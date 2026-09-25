/**
 * Structural shapes inferred from serialized samples, used to check that the
 * mock backend sends what the real backend sends.
 *
 * A shape records, per path, which JSON kinds the samples held there, and for
 * objects the exact key sets seen. A value matches when every key set and
 * kind it has was seen in some sample.
 */

type Kind = "string" | "number" | "boolean" | "null" | "array" | "object" | "undefined";

export interface Shape {
  kinds: Set<Kind>;
  /** Shape of every array element seen here. */
  element?: Shape;
  /** Field shapes, per sorted key list. Objects with different keys are different variants. */
  variants?: Map<string, Map<string, Shape>>;
}

function kindOf(value: unknown): Kind {
  if (value === null) return "null";
  if (Array.isArray(value)) return "array";
  const kind = typeof value;
  if (kind === "string" || kind === "number" || kind === "boolean" || kind === "undefined") {
    return kind;
  }
  if (kind === "object") return "object";
  throw new Error(`Not a JSON value: ${String(value)}`);
}

function emptyShape(): Shape {
  return { kinds: new Set() };
}

function variantKey(value: object): string {
  return Object.keys(value).sort().join(",");
}

function add(shape: Shape, value: unknown): void {
  const kind = kindOf(value);
  shape.kinds.add(kind);
  if (kind === "array") {
    shape.element ??= emptyShape();
    for (const item of value as unknown[]) add(shape.element, item);
  } else if (kind === "object") {
    shape.variants ??= new Map();
    const key = variantKey(value as object);
    const fields = shape.variants.get(key) ?? new Map<string, Shape>();
    shape.variants.set(key, fields);
    for (const [name, field] of Object.entries(value as Record<string, unknown>)) {
      const fieldShape = fields.get(name) ?? emptyShape();
      fields.set(name, fieldShape);
      add(fieldShape, field);
    }
  }
}

/** The shape of all `samples` together. */
export function inferShape(samples: readonly unknown[]): Shape {
  const shape = emptyShape();
  for (const sample of samples) add(shape, sample);
  return shape;
}

function closestKeys(keys: string[], variants: Iterable<string>): string[] {
  let best: string[] = [];
  let bestScore = -1;
  for (const variant of variants) {
    const candidate = variant === "" ? [] : variant.split(",");
    const score = candidate.filter((k) => keys.includes(k)).length;
    if (score > bestScore) {
      best = candidate;
      bestScore = score;
    }
  }
  return best;
}

/** Where and how `value` departs from `shape`; empty when it matches. */
export function shapeMismatches(value: unknown, shape: Shape, path = "$"): string[] {
  const kind = kindOf(value);
  if (!shape.kinds.has(kind)) {
    return [`${path}: is ${kind}, the backend sends ${[...shape.kinds].join(" | ")}`];
  }
  if (kind === "array") {
    const element = shape.element;
    // Only empty arrays were sampled here: nothing to compare against.
    if (!element || element.kinds.size === 0) return [];
    return (value as unknown[]).flatMap((item, i) =>
      shapeMismatches(item, element, `${path}[${i}]`)
    );
  }
  if (kind === "object") {
    const variants = shape.variants ?? new Map<string, Map<string, Shape>>();
    const fields = variants.get(variantKey(value as object));
    if (!fields) {
      const keys = Object.keys(value as object);
      const expected = closestKeys(keys, variants.keys());
      const extra = keys.filter((k) => !expected.includes(k));
      const missing = expected.filter((k) => !keys.includes(k));
      return [
        `${path}: fields do not match the backend's` +
          (extra.length ? `; unexpected: ${extra.join(", ")}` : "") +
          (missing.length ? `; missing: ${missing.join(", ")}` : ""),
      ];
    }
    return Object.entries(value as Record<string, unknown>).flatMap(([name, field]) => {
      const fieldShape = fields.get(name);
      return fieldShape ? shapeMismatches(field, fieldShape, `${path}.${name}`) : [];
    });
  }
  return [];
}
