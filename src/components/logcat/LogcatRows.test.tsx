import { render, screen } from "@solidjs/testing-library";
import { describe, expect, it } from "vitest";
import type { LogcatEntry } from "@/lib/tauri-api";
import { LogcatVirtualRow } from "./LogcatRows";
import styles from "./LogcatRows.module.css";

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
  it("uses the Logcat row chrome class and keeps dynamic row styles inline", () => {
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

    expect(row.classList.contains(styles.row)).toBe(true);
    expect(row.style.background).not.toBe("");
    expect(row.style.borderLeft).toBe("2px solid transparent");
  });
});
