import { render, screen } from "@solidjs/testing-library";
import { describe, expect, it } from "vitest";
import type { LogcatEntry } from "@/lib/tauri-api";
import { LogcatVirtualRow } from "./LogcatRows";

const ENTRY: LogcatEntry = {
  id: 1n,
  timestamp: "2026-05-06T12:00:00.000Z",
  pid: 123,
  tid: 456,
  level: "info",
  tag: "MainActivity",
  message: "Activity started",
  package: "com.example.app",
  kind: "normal",
  isCrash: false,
  flags: 0,
  category: "general",
  crashGroupId: null,
  jsonBody: null,
};

describe("LogcatRows", () => {
  it("sizes log output rows from the Logcat font CSS variables", () => {
    render(() => (
      <LogcatVirtualRow
        entry={ENTRY}
        getIndex={() => 0}
        getSelectionRange={() => null}
        getAnchor={() => null}
        getEnd={() => null}
        getDetailEntry={() => null}
        getJsonEntry={() => null}
        onRowClick={() => {}}
        onJsonClick={() => {}}
      />
    ));

    const row = screen.getByTitle("Click to copy · Shift+click to select range");

    expect(row.style.fontSize).toBe("var(--font-size-logcat-output)");
    expect(row.style.height).toBe("var(--logcat-row-height)");
    expect(row.style.minHeight).toBe("var(--logcat-row-height)");
  });
});
