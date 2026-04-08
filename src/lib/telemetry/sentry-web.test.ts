import { describe, expect, it } from "vitest";
import type { ErrorEvent } from "@sentry/core";
import { scrubBrowserEvent, scrubWebFrameFilename } from "./sentry-web";

describe("scrubWebFrameFilename", () => {
  it("keeps bare module paths", () => {
    expect(scrubWebFrameFilename("chunk-abc.js")).toBe("chunk-abc.js");
  });

  it("reduces file URLs to basename", () => {
    expect(scrubWebFrameFilename("file:///Users/dev/proj/dist/assets/foo.js")).toBe("foo.js");
  });
});

describe("scrubBrowserEvent", () => {
  it("removes user and request and clears breadcrumbs", () => {
    const event = {
      message: "test",
      user: { id: "x" },
      request: { url: "file:///secret" } as ErrorEvent["request"],
      breadcrumbs: [{ message: "nav", level: "info" } as never],
      extra: { leak: "data" },
    } as unknown as ErrorEvent;
    const out = scrubBrowserEvent(event);
    expect(out.user).toBeUndefined();
    expect(out.request).toBeUndefined();
    expect(out.breadcrumbs).toEqual([]);
    expect(out.extra).toEqual({});
  });

  it("scrubs stack frame filenames that look like local paths", () => {
    const event = {
      message: "e",
      exception: {
        values: [
          {
            type: "Error",
            value: "fail",
            stacktrace: {
              frames: [
                { filename: "file:///Users/x/app.js", lineno: 1, colno: 1 },
              ],
            },
          },
        ],
      },
    } as unknown as ErrorEvent;
    const out = scrubBrowserEvent(event);
    const fn = out.exception?.values?.[0]?.stacktrace?.frames?.[0]?.filename;
    expect(fn).toBe("app.js");
  });
});
