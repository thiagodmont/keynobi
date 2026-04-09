import { describe, it, expect } from "vitest";
import type { BuildRecord, BuildStatus } from "@/bindings";

// ── Helpers (duplicated from component for isolated testing) ──────────────────

function statusIcon(status: BuildStatus): string {
  if (status.state === "running") return "⟳";
  if (status.state === "success") return "✓";
  if (status.state === "failed") return "✗";
  if (status.state === "cancelled") return "◼";
  return "•";
}

function statusColor(status: BuildStatus): string {
  if (status.state === "success") return "#4ade80";
  if (status.state === "failed") return "#f87171";
  if (status.state === "cancelled") return "rgba(255,255,255,0.3)";
  return "#60a5fa";
}

function durationLabel(status: BuildStatus): string {
  if (status.state !== "success" && status.state !== "failed") return "";
  const ms = Number((status as any).durationMs ?? 0);
  if (!ms) return "";
  if (ms < 1000) return `${ms}ms`;
  if (ms < 60000) return `${(ms / 1000).toFixed(1)}s`;
  const mins = Math.floor(ms / 60000);
  const secs = Math.floor((ms % 60000) / 1000);
  return `${mins}m ${secs}s`;
}

function errorCount(record: BuildRecord): number {
  return record.errors.filter((e) => e.severity === "error").length;
}

function makeRecord(state: BuildStatus["state"], durationMs = 0, errors: any[] = []): BuildRecord {
  const status: any =
    state === "success" ? { state: "success", success: true, durationMs: BigInt(durationMs), errorCount: 0, warningCount: 0 }
    : state === "failed" ? { state: "failed", success: false, durationMs: BigInt(durationMs), errorCount: errors.length, warningCount: 0 }
    : { state };
  return { id: 1, task: "assembleDebug", status, errors, startedAt: new Date().toISOString() };
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

  it("statusColor returns green for success", () => {
    expect(statusColor(makeRecord("success").status)).toBe("#4ade80");
  });

  it("statusColor returns red for failed", () => {
    expect(statusColor(makeRecord("failed").status)).toBe("#f87171");
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
});
