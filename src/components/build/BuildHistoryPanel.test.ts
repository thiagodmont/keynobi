import { describe, it, expect } from "vitest";
import type { BuildRecord, BuildStatus } from "@/bindings";
import { statusIcon, statusColor, durationLabel, errorCount } from "@/components/build/BuildHistoryPanel";
import type { BuildHistoryPanelProps } from "@/components/build/BuildHistoryPanel";

function makeRecord(state: BuildStatus["state"], durationMs = 0, errors: any[] = []): BuildRecord {
  const status: any =
    state === "success" ? { state: "success", success: true, durationMs: BigInt(durationMs), errorCount: 0, warningCount: 0 }
    : state === "failed" ? { state: "failed", success: false, durationMs: BigInt(durationMs), errorCount: errors.length, warningCount: 0 }
    : { state };
  return { id: 1, task: "assembleDebug", status, errors, startedAt: new Date().toISOString(), projectRoot: null };
}

// ── Tests ─────────────────────────────────────────────────────────────────────

describe("BuildHistoryPanel helpers", () => {
  it("statusIcon returns correct icon for each state", () => {
    expect(statusIcon({ state: "running", task: "t", started_at: "" })).toBe("⟳");
    expect(statusIcon(makeRecord("success").status)).toBe("✓");
    expect(statusIcon(makeRecord("failed").status)).toBe("✗");
    expect(statusIcon({ state: "cancelled" })).toBe("◼");
    expect(statusIcon({ state: "idle" })).toBe("•");
  });

  it("statusColor returns success token for success", () => {
    expect(statusColor(makeRecord("success").status)).toBe("var(--success)");
  });

  it("statusColor returns error token for failed", () => {
    expect(statusColor(makeRecord("failed").status)).toBe("var(--error)");
  });

  it("durationLabel returns empty for running/idle/cancelled", () => {
    expect(durationLabel({ state: "running", task: "t", started_at: "" })).toBe("");
    expect(durationLabel({ state: "idle" })).toBe("");
    expect(durationLabel({ state: "cancelled" })).toBe("");
  });

  it("durationLabel formats milliseconds", () => {
    expect(durationLabel(makeRecord("success", 500).status)).toBe("500ms");
  });

  it("durationLabel formats seconds", () => {
    expect(durationLabel(makeRecord("success", 42000).status)).toBe("42.0s");
  });

  it("durationLabel formats minutes and seconds", () => {
    expect(durationLabel(makeRecord("success", 78000).status)).toBe("1m 18s");
  });

  it("errorCount counts errors only (not warnings)", () => {
    const record = makeRecord("failed", 0, [
      { severity: "error", message: "e1", file: null, line: null, col: null },
      { severity: "warning", message: "w1", file: null, line: null, col: null },
      { severity: "error", message: "e2", file: null, line: null, col: null },
    ]);
    expect(errorCount(record)).toBe(2);
  });

  it("BuildHistoryPanelProps accepts optional onClear", () => {
    // Type-level test: if this compiles, onClear is optional
    const props: BuildHistoryPanelProps = {
      selectedId: null,
      onSelect: () => {},
      // onClear omitted — must be optional
    };
    expect(props.selectedId).toBeNull();
  });
});
