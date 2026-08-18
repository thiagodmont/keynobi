import { describe, it, expect, beforeEach } from "vitest";
import {
  logcatState,
  setLogcatDroppedLines,
  setLogcatRingBufferTotal,
  setLogcatStreaming,
  clearLogcatEntries,
} from "@/stores/logcat.store";
import { formatLogcatToolbarCount } from "@/components/logcat/logcat-toolbar-count";

describe("dropped-line reporting", () => {
  beforeEach(() => {
    clearLogcatEntries();
    setLogcatDroppedLines(0);
    setLogcatRingBufferTotal(null);
    setLogcatStreaming(false);
  });

  it("defaults to zero drops", () => {
    expect(logcatState.droppedLines).toBe(0);
  });

  it("stores the backend drop count", () => {
    setLogcatDroppedLines(12);
    expect(logcatState.droppedLines).toBe(12);
  });

  // The backend resets its shared counter on clear; the store must be able to
  // follow it back down, otherwise the warning sticks forever in the UI.
  it("can return to zero after a clear", () => {
    setLogcatDroppedLines(12);
    setLogcatDroppedLines(0);
    expect(logcatState.droppedLines).toBe(0);

    const { text } = formatLogcatToolbarCount({
      queryActive: false,
      visible: 5,
      ringTotal: 5,
      droppedLines: logcatState.droppedLines,
    });
    expect(text).not.toContain("dropped");
  });

  it("treats an omitted droppedLines as no drops", () => {
    const { text, title } = formatLogcatToolbarCount({
      queryActive: true,
      visible: 1,
      ringTotal: 10,
    });
    expect(text).not.toContain("dropped");
    expect(title).not.toContain("incomplete");
  });
});
