import { afterEach, beforeEach, describe, it, expect, vi } from "vitest";
import { cleanup, render, screen, fireEvent } from "@solidjs/testing-library";
import { createSignal } from "solid-js";
import { invoke } from "@tauri-apps/api/core";
import { LogEntryDetailPanel } from "./LogEntryDetailPanel";
import type { LogcatEntry, RetraceOutcome } from "@/lib/tauri-api";

const ENTRY = {
  id: 0,
  timestamp: "2024-01-15 10:23:45.123",
  pid: 1234,
  tid: 5678,
  level: "error" as const,
  tag: "MainActivity",
  message: "NullPointerException at line 42",
  package: "com.example.app",
  kind: "normal" as const,
  isCrash: false,
  flags: 0,
  category: "general" as const,
  crashGroupId: null,
  jsonBody: null,
} satisfies LogcatEntry;

describe("LogEntryDetailPanel", () => {
  it("renders the tag", () => {
    render(() => <LogEntryDetailPanel entry={ENTRY} onClose={() => {}} />);
    expect(screen.getByText("MainActivity")).not.toBeNull();
  });

  it("renders the message", () => {
    render(() => <LogEntryDetailPanel entry={ENTRY} onClose={() => {}} />);
    expect(screen.getByText("NullPointerException at line 42")).not.toBeNull();
  });

  it("renders the package", () => {
    render(() => <LogEntryDetailPanel entry={ENTRY} onClose={() => {}} />);
    expect(screen.getByText("com.example.app")).not.toBeNull();
  });

  it("calls onClose when close button is clicked", () => {
    let closed = false;
    render(() => (
      <LogEntryDetailPanel
        entry={ENTRY}
        onClose={() => {
          closed = true;
        }}
      />
    ));
    const closeBtn = screen.getAllByRole("button").find((b) => b.getAttribute("title") === "Close");
    expect(closeBtn).not.toBeUndefined();
    closeBtn!.click();
    expect(closed).toBe(true);
  });

  it("calls onClose when Escape is pressed", () => {
    let closed = false;
    render(() => (
      <LogEntryDetailPanel
        entry={ENTRY}
        onClose={() => {
          closed = true;
        }}
      />
    ));
    fireEvent.keyDown(document, { key: "Escape" });
    expect(closed).toBe(true);
  });

  it("opens a floating filter menu when a metadata value is clicked", () => {
    render(() => <LogEntryDetailPanel entry={ENTRY} onClose={() => {}} onAddFilter={() => {}} />);

    fireEvent.click(screen.getByText("MainActivity"));

    expect(screen.getByRole("menu")).not.toBeNull();
    expect(screen.getByRole("menuitem", { name: "Add as AND" })).not.toBeNull();
    expect(screen.getByRole("menuitem", { name: "Add as OR" })).not.toBeNull();
  });

  it("emits the clicked metadata token with the selected mode", () => {
    const onAddFilter = vi.fn();
    render(() => (
      <LogEntryDetailPanel entry={ENTRY} onClose={() => {}} onAddFilter={onAddFilter} />
    ));

    fireEvent.click(screen.getByText("MainActivity"));
    fireEvent.click(screen.getByRole("menuitem", { name: "Add as AND" }));

    expect(onAddFilter).toHaveBeenCalledWith({ token: "tag:MainActivity", mode: "and" });
  });

  it("emits OR filters for package values", () => {
    const onAddFilter = vi.fn();
    render(() => (
      <LogEntryDetailPanel entry={ENTRY} onClose={() => {}} onAddFilter={onAddFilter} />
    ));

    fireEvent.click(screen.getByText("com.example.app"));
    fireEvent.click(screen.getByRole("menuitem", { name: "Add as OR" }));

    expect(onAddFilter).toHaveBeenCalledWith({ token: "package:com.example.app", mode: "or" });
  });

  it("uses selected message text instead of the full message", () => {
    const onAddFilter = vi.fn();
    render(() => (
      <LogEntryDetailPanel entry={ENTRY} onClose={() => {}} onAddFilter={onAddFilter} />
    ));
    const message = screen.getByText("NullPointerException at line 42");
    vi.spyOn(window, "getSelection").mockReturnValue({
      toString: () => "line 42",
      anchorNode: message.firstChild,
      focusNode: message.firstChild,
    } as unknown as ReturnType<typeof window.getSelection>);

    fireEvent.click(message);
    fireEvent.click(screen.getByRole("menuitem", { name: "Add as OR" }));

    expect(onAddFilter).toHaveBeenCalledWith({ token: 'message:"line 42"', mode: "or" });
  });

  it("ignores selected text outside the message field", () => {
    const onAddFilter = vi.fn();
    const outside = document.createTextNode("outside selection");
    document.body.appendChild(outside);
    render(() => (
      <LogEntryDetailPanel entry={ENTRY} onClose={() => {}} onAddFilter={onAddFilter} />
    ));
    vi.spyOn(window, "getSelection").mockReturnValue({
      toString: () => "outside selection",
      anchorNode: outside,
      focusNode: outside,
    } as unknown as ReturnType<typeof window.getSelection>);

    fireEvent.click(screen.getByText("NullPointerException at line 42"));
    fireEvent.click(screen.getByRole("menuitem", { name: "Add as OR" }));

    expect(onAddFilter).toHaveBeenCalledWith({
      token: 'message:"NullPointerException at line 42"',
      mode: "or",
    });
  });

  it("ignores selected text that crosses outside the message field", () => {
    const onAddFilter = vi.fn();
    const outside = document.createTextNode("outside selection");
    document.body.appendChild(outside);
    render(() => (
      <LogEntryDetailPanel entry={ENTRY} onClose={() => {}} onAddFilter={onAddFilter} />
    ));
    const message = screen.getByText("NullPointerException at line 42");
    vi.spyOn(window, "getSelection").mockReturnValue({
      toString: () => "line 42 plus outside selection",
      anchorNode: message.firstChild,
      focusNode: outside,
    } as unknown as ReturnType<typeof window.getSelection>);

    fireEvent.click(message);
    fireEvent.click(screen.getByRole("menuitem", { name: "Add as OR" }));

    expect(onAddFilter).toHaveBeenCalledWith({
      token: 'message:"NullPointerException at line 42"',
      mode: "or",
    });
  });
});

describe("LogEntryDetailPanel deobfuscation", () => {
  const CRASH = {
    ...ENTRY,
    id: 7,
    tag: "AndroidRuntime",
    message: "java.lang.RuntimeException: boom",
    isCrash: true,
    crashGroupId: 7,
  } satisfies LogcatEntry;

  const OBFUSCATED = "java.lang.RuntimeException: boom\n\tat a.a.onCreate(SourceFile:1)\n";

  function outcome(overrides: Partial<RetraceOutcome>): RetraceOutcome {
    return {
      status: "refused",
      trace: OBFUSCATED,
      buildId: null,
      mapping: null,
      matchedBy: null,
      device: null,
      package: "com.example.app",
      reason: null,
      summary: "",
      ...overrides,
    };
  }

  const RETRACED = outcome({
    status: "retraced",
    trace:
      "java.lang.RuntimeException: boom\n\tat com.example.app.MainActivity.onCreate(MainActivity.kt:24)\n",
    buildId: 12,
    mapping: {
      module: ":app",
      variant: "release",
      sha256: "ab".repeat(32),
      bytes: 100,
      pgMapId: "6b1c2f0",
    },
    matchedBy: "installRecord",
    device: "Pixel_7",
    summary:
      "Deobfuscated with the R8 mapping of build #12 (:app release, map id 6b1c2f0), matched by Keynobi's install on Pixel_7.",
  });

  function stubRetrace(respond: () => Promise<RetraceOutcome>) {
    vi.mocked(invoke).mockImplementation((command: string) => {
      if (command === "retrace_crash") return respond();
      return Promise.reject(new Error(`unexpected command ${command}`));
    });
  }

  const deobfuscateButton = () => screen.queryByRole("button", { name: /Deobfuscat/ });

  beforeEach(() => {
    vi.mocked(invoke).mockReset();
  });

  afterEach(() => {
    cleanup();
  });

  it("is offered only for crash entries", () => {
    render(() => <LogEntryDetailPanel entry={ENTRY} onClose={() => {}} />);
    expect(deobfuscateButton()).toBeNull();
    cleanup();
    render(() => <LogEntryDetailPanel entry={CRASH} onClose={() => {}} />);
    expect(deobfuscateButton()).not.toBeNull();
  });

  it("shows the deobfuscated stack and the mapping it used", async () => {
    stubRetrace(() => Promise.resolve(RETRACED));
    render(() => <LogEntryDetailPanel entry={CRASH} onClose={() => {}} />);

    fireEvent.click(deobfuscateButton()!);

    expect(await screen.findByText("Deobfuscated")).not.toBeNull();
    expect(vi.mocked(invoke)).toHaveBeenCalledWith("retrace_crash", { crashGroupId: 7 });
    expect(
      screen.getByText(/mapping of build #12 \(:app release, map id 6b1c2f0\)/)
    ).not.toBeNull();
    expect(screen.getByLabelText("Deobfuscated stack").textContent).toBe(RETRACED.trace);
    expect(screen.getByRole("button", { name: /Copy stack/ })).not.toBeNull();
  });

  it("says why nothing was deobfuscated and shows no stack", async () => {
    stubRetrace(() =>
      Promise.resolve(
        outcome({
          status: "refused",
          buildId: 12,
          reason: "the app was reinstalled outside Keynobi after build #12",
          summary: "Not deobfuscated: the app was reinstalled outside Keynobi after build #12.",
        })
      )
    );
    render(() => <LogEntryDetailPanel entry={CRASH} onClose={() => {}} />);

    fireEvent.click(deobfuscateButton()!);

    expect(await screen.findByText("Not deobfuscated")).not.toBeNull();
    expect(
      screen.getByText("the app was reinstalled outside Keynobi after build #12")
    ).not.toBeNull();
    expect(screen.queryByLabelText("Deobfuscated stack")).toBeNull();
    expect(screen.queryByRole("button", { name: /Copy stack/ })).toBeNull();
  });

  it("says what to install when retrace is not available", async () => {
    stubRetrace(() =>
      Promise.resolve(
        outcome({
          status: "unavailable",
          reason:
            'retrace was not found in the Android SDK. Install "Android SDK Command-line Tools".',
        })
      )
    );
    render(() => <LogEntryDetailPanel entry={CRASH} onClose={() => {}} />);

    fireEvent.click(deobfuscateButton()!);

    expect(await screen.findByText("Retrace not available")).not.toBeNull();
    expect(screen.getByText(/Install "Android SDK Command-line Tools"/)).not.toBeNull();
    expect(screen.queryByLabelText("Deobfuscated stack")).toBeNull();
  });

  it("reports a failed request", async () => {
    stubRetrace(() =>
      Promise.reject({ kind: "NotFound", message: "Crash 7 is no longer in the logcat buffer." })
    );
    render(() => <LogEntryDetailPanel entry={CRASH} onClose={() => {}} />);

    fireEvent.click(deobfuscateButton()!);

    expect(await screen.findByText("Retrace failed")).not.toBeNull();
    expect(screen.getByText(/no longer in the logcat buffer/)).not.toBeNull();
  });

  it("is busy while the request runs", async () => {
    let answer!: (value: RetraceOutcome) => void;
    stubRetrace(() => new Promise((resolve) => (answer = resolve)));
    render(() => <LogEntryDetailPanel entry={CRASH} onClose={() => {}} />);

    fireEvent.click(deobfuscateButton()!);

    const busy = await screen.findByRole("button", { name: /Deobfuscating…/ });
    expect((busy as HTMLButtonElement).disabled).toBe(true);
    answer(RETRACED);
    expect(await screen.findByText("Deobfuscated")).not.toBeNull();
    expect((deobfuscateButton() as HTMLButtonElement).disabled).toBe(false);
  });

  it("drops an answer for a crash that is no longer shown", async () => {
    let answer!: (value: RetraceOutcome) => void;
    stubRetrace(() => new Promise((resolve) => (answer = resolve)));
    const [entry, setEntry] = createSignal<LogcatEntry>(CRASH);
    render(() => <LogEntryDetailPanel entry={entry()} onClose={() => {}} />);

    fireEvent.click(deobfuscateButton()!);
    setEntry({ ...CRASH, id: 8, crashGroupId: 8, message: "java.lang.IllegalStateException" });
    answer(RETRACED);
    await new Promise((resolve) => setTimeout(resolve, 0));

    expect(screen.queryByText("Deobfuscated")).toBeNull();
    expect(deobfuscateButton()!.textContent).toContain("Deobfuscate");
    expect((deobfuscateButton() as HTMLButtonElement).disabled).toBe(false);
  });
});
