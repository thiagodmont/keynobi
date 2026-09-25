// @vitest-environment node
import { describe, expect, it } from "vitest";
import { inferShape, shapeMismatches } from "./shape";

const record = { id: 1, task: "assembleDebug", origin: { kind: "app" }, errors: [{ line: 3 }] };

describe("shapeMismatches", () => {
  it("accepts a value shaped like a sample", () => {
    const value = { id: 2, task: "x", origin: { kind: "agent" }, errors: [] };
    expect(shapeMismatches(value, inferShape([record]))).toEqual([]);
  });

  it("names a renamed field", () => {
    const value = { id: 2, taskName: "x", origin: { kind: "app" }, errors: [] };
    expect(shapeMismatches(value, inferShape([record]))).toEqual([
      "$: fields do not match the backend's; unexpected: taskName; missing: task",
    ]);
  });

  it("names a field of the wrong kind, however deep", () => {
    const value = { ...record, errors: [{ line: "3" }] };
    expect(shapeMismatches(value, inferShape([record]))).toEqual([
      "$.errors[0].line: is string, the backend sends number",
    ]);
  });

  it("allows null only where a sample had null", () => {
    const shape = inferShape([record, { ...record, origin: null }]);
    expect(shapeMismatches({ ...record, origin: null }, shape)).toEqual([]);
    expect(shapeMismatches({ ...record, task: null }, shape)).toEqual([
      "$.task: is null, the backend sends string",
    ]);
  });

  it("matches each variant of a tagged union by its own fields", () => {
    const shape = inferShape([{ state: "idle" }, { state: "success", durationMs: 4 }]);
    expect(shapeMismatches({ state: "cancelled" }, shape)).toEqual([]);
    expect(shapeMismatches({ state: "success", durationMs: 1 }, shape)).toEqual([]);
    expect(shapeMismatches({ state: "success", duration: 1 }, shape)).toHaveLength(1);
  });

  it("rejects a bigint, which JSON cannot carry", () => {
    expect(() => shapeMismatches({ id: BigInt(5) }, inferShape([{ id: 5 }]))).toThrow(
      "Not a JSON value"
    );
  });

  it("rejects undefined, which JSON cannot carry", () => {
    expect(shapeMismatches(undefined, inferShape([null]))).toEqual([
      "$: is undefined, the backend sends null",
    ]);
  });
});
