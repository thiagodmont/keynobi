import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen, waitFor, within } from "@solidjs/testing-library";
import { invoke } from "@tauri-apps/api/core";
import type {
  AppError,
  DebugSession,
  DebugSessionEvent,
  DebugSessionExitRefresh,
  ProcessedEntry,
} from "@/bindings";
import { makeLogEntry } from "@/test/factories/logcat";
import { makeLaunchTiming } from "@/test/factories/build";
import {
  makeSession,
  makeSessionCrash,
  makeSessionDetail,
  makeSessionEvent,
  summaryOf,
} from "@/test/factories/sessions";
import {
  SESSIONS_POLL_MS,
  SessionsDialog,
  closeSessionsDialog,
  openSessionsDialog,
} from "./SessionsDialog";
import { CAPTURE_PAGE_LINES } from "./SessionEventDetail";

interface FakeBackend {
  /** Oldest first, as the index keeps them. */
  sessions: DebugSession[];
  events: Map<string, DebugSessionEvent[]>;
  /** Kept log lines, by `<session id>/<seq>`. */
  captures: Map<string, ProcessedEntry[]>;
  exitRefresh: DebugSessionExitRefresh;
  /** Commands that reject with this error. */
  failing: Map<string, AppError>;
}

let fake: FakeBackend;

function session(id: string): DebugSession {
  const found = fake.sessions.find((s) => s.id === id);
  if (!found) throw { kind: "notFound", message: `Debug session ${id} is no longer kept` };
  return found;
}

function append(s: DebugSession, event: DebugSessionEvent): void {
  fake.events.set(s.id, [...(fake.events.get(s.id) ?? []), event]);
  s.eventCount += 1;
  s.lastEventAt = new Date(Date.parse(s.lastEventAt) + 1000).toISOString();
}

function installFake(): void {
  vi.mocked(invoke).mockImplementation(async (command: string, args?: unknown) => {
    const failure = fake.failing.get(command);
    if (failure) throw failure;
    const a = (args ?? {}) as Record<string, unknown>;
    switch (command) {
      case "list_debug_sessions":
        return [...fake.sessions].reverse().map(summaryOf);
      case "get_debug_session": {
        const s = session(a.id as string);
        return makeSessionDetail({ ...s }, fake.events.get(s.id) ?? []);
      }
      case "set_debug_session_kept": {
        const s = session(a.id as string);
        if (a.kept && !s.kept && fake.sessions.filter((x) => x.kept).length >= 5) {
          throw {
            kind: "invalidInput",
            message: "At most 5 debug sessions can be kept. Stop keeping one first.",
          };
        }
        s.kept = a.kept as boolean;
        return null;
      }
      case "end_debug_session": {
        const s = session(a.id as string);
        s.closedAt = "2026-09-25T11:00:00.000000Z";
        s.closeReason = "ended";
        return null;
      }
      case "add_session_bookmark": {
        const s = session(a.sessionId as string);
        const event = makeSessionEvent(
          s.eventCount + 1,
          { kind: "bookmark", data: { note: a.note as string, logEntryId: null } },
          { actor: { kind: "app" } }
        );
        append(s, event);
        s.counts.bookmarks += 1;
        return event;
      }
      case "get_session_capture": {
        const lines = fake.captures.get(`${a.id}/${a.seq}`) ?? [];
        const keep = Math.min((a.limit as number | null) ?? 1000, 1000);
        return { seq: a.seq, entries: lines.slice(-keep), truncated: lines.length > keep };
      }
      case "refresh_session_exit_reasons":
        session(a.id as string);
        return fake.exitRefresh;
    }
    throw new Error(`unexpected command ${command}`);
  });
}

function calls(command: string) {
  return vi.mocked(invoke).mock.calls.filter(([c]) => c === command);
}

function lastArgs(command: string) {
  const made = calls(command);
  return made[made.length - 1]?.[1];
}

const CRASH_SEQ = 4;

/** An older superseded session, and the newest: install, launch, crash. */
function seed(): { older: DebugSession; newest: DebugSession } {
  const older = makeSession({
    id: "s-20260925T090000Z-000000000001",
    build: null,
    install: null,
    recordedBy: "standalone",
    kept: true,
    closedAt: "2026-09-25T10:32:00.000000Z",
    closeReason: "superseded",
    counts: {
      launches: 0,
      crashes: 1,
      anrs: 0,
      exits: 0,
      bookmarks: 0,
      captures: 0,
      agentActions: 0,
    },
  });
  const newest = makeSession({
    id: "s-20260925T103200Z-000000000002",
    counts: {
      launches: 1,
      crashes: 1,
      anrs: 0,
      exits: 0,
      bookmarks: 0,
      captures: 1,
      agentActions: 0,
    },
    eventCount: 4,
  });
  const newestBuild = newest.build;
  const newestInstall = newest.install;
  if (!newestBuild || !newestInstall) throw new Error("seeded session has a build");
  fake.sessions = [older, newest];
  fake.events.set(newest.id, [
    makeSessionEvent(1, { kind: "build", data: newestBuild }),
    makeSessionEvent(2, { kind: "install", data: newestInstall }, { actor: { kind: "app" } }),
    makeSessionEvent(
      3,
      {
        kind: "launch",
        data: {
          serial: "emulator-5554",
          timing: makeLaunchTiming({ displayedMs: 790 }),
          restart: false,
        },
      },
      { actor: { kind: "app" } }
    ),
    makeSessionEvent(CRASH_SEQ, { kind: "crash", data: makeSessionCrash() }),
  ]);
  fake.captures.set(
    `${newest.id}/${CRASH_SEQ}`,
    Array.from({ length: 450 }, (_, i) =>
      makeLogEntry({ message: `line ${i + 1}`, isCrash: i >= 440 })
    )
  );
  return { older, newest };
}

async function openDialog(sessionId?: string) {
  render(() => <SessionsDialog />);
  openSessionsDialog(sessionId);
  const dialog = await screen.findByRole("dialog", { name: "Debug Sessions" });
  await within(dialog).findByRole("listbox", { name: "Timeline, oldest first" });
  return dialog;
}

function timelineRows() {
  return within(screen.getByRole("listbox", { name: "Timeline, oldest first" })).getAllByRole(
    "option"
  );
}

describe("SessionsDialog", () => {
  beforeEach(() => {
    if (!window.ResizeObserver) {
      class MockResizeObserver {
        observe = vi.fn();
        unobserve = vi.fn();
        disconnect = vi.fn();
      }
      window.ResizeObserver = MockResizeObserver as unknown as typeof ResizeObserver;
    }
    vi.mocked(invoke).mockReset();
    fake = {
      sessions: [],
      events: new Map(),
      captures: new Map(),
      exitRefresh: { added: 0, message: null },
      failing: new Map(),
    };
    installFake();
  });

  afterEach(() => {
    closeSessionsDialog();
    cleanup();
    vi.useRealTimers();
  });

  it("lists sessions newest first with their build, device, state, counts, and badges", async () => {
    seed();
    const dialog = await openDialog();

    const list = within(dialog).getByRole("listbox", { name: "Debug sessions" });
    const options = within(list).getAllByRole("option");
    expect(options).toHaveLength(2);
    expect(options[0].textContent).toContain("#12 · :app debug");
    expect(options[0].textContent).toContain("Open");
    expect(options[0].textContent).toContain("Pixel_7 · com.example.app");
    expect(options[0].textContent).toContain("1 crash · 0 ANRs · 1 launch");
    expect(options[0].getAttribute("aria-selected")).toBe("true");
    expect(options[1].textContent).toContain("No Keynobi install");
    expect(options[1].textContent).toContain("Superseded");
    for (const badge of ["Kept", "Standalone", "Unattributed"]) {
      expect(within(options[1]).getByText(badge)).toBeTruthy();
      expect(within(options[0]).queryByText(badge)).toBeNull();
    }
  });

  it("shows the selected session's timeline: build, install, launch with its time, crash", async () => {
    seed();
    await openDialog();

    const rows = timelineRows();
    expect(rows.map((r) => r.textContent)).toEqual([
      expect.stringContaining("Build #12 · :app debug · :app:assembleDebug"),
      expect.stringContaining("Installed APK a1a1a1a1a1a1… (version code 42) by Keynobi"),
      expect.stringContaining("Launched · Launch 812 ms (cold) · displayed 790 ms · by Keynobi"),
      expect.stringContaining("java.lang.IllegalStateException: boom"),
    ]);
    expect(screen.getByText("1 crash · 0 ANRs · 1 launch · 0 exits · 0 bookmarks")).toBeTruthy();
  });

  it("shows what an agent did on the device in the timeline", async () => {
    const { newest } = seed();
    append(
      newest,
      makeSessionEvent(
        5,
        {
          kind: "agentAction",
          data: {
            tool: "ui_tap",
            kind: "write",
            ok: false,
            durationMs: 412,
            serial: "emulator-5554",
          },
        },
        {
          actor: { kind: "agent", sessionId: null, clientName: "Claude Code", standalone: false },
        }
      )
    );
    await openDialog();

    const row = timelineRows()[4];
    expect(within(row).getByText("Agent")).toBeTruthy();
    expect(row.textContent).toContain("ui_tap by an agent (Claude Code) · 412 ms · failed");
  });

  it("opens on the session it was asked for", async () => {
    const { older } = seed();
    fake.events.set(older.id, [
      makeSessionEvent(1, {
        kind: "crash",
        data: makeSessionCrash({
          summary: "java.lang.NullPointerException",
          attribution: { method: "unattributed", verified: false, reason: "reinstalled" },
          capture: null,
        }),
      }),
    ]);
    const dialog = await openDialog(older.id);

    const selected = within(dialog)
      .getAllByRole("option", { selected: true })
      .find((o) => o.textContent?.includes("No Keynobi install"));
    expect(selected).toBeTruthy();
    await waitFor(() => expect(timelineRows()).toHaveLength(1));
    expect(timelineRows()[0].textContent).toContain("java.lang.NullPointerException");
  });

  it("keeps a session, and says inline why a sixth cannot be kept", async () => {
    seed();
    await openDialog();

    const keep = screen.getByRole("button", { name: "Keep" });
    expect(keep.getAttribute("aria-pressed")).toBe("false");
    fireEvent.click(keep);
    await waitFor(() => expect(keep.getAttribute("aria-pressed")).toBe("true"));
    expect(lastArgs("set_debug_session_kept")).toEqual({
      id: "s-20260925T103200Z-000000000002",
      kept: true,
    });

    fireEvent.click(keep);
    await waitFor(() => expect(keep.getAttribute("aria-pressed")).toBe("false"));

    for (let i = 0; i < 4; i++) {
      fake.sessions.unshift(makeSession({ id: `s-20260925T080000Z-00000000001${i}`, kept: true }));
    }
    fireEvent.click(keep);
    const alert = await screen.findByRole("alert");
    expect(alert.textContent).toContain("At most 5 debug sessions can be kept");
    expect(keep.getAttribute("aria-pressed")).toBe("false");
  });

  it("ends an open session and then refuses bookmarks", async () => {
    seed();
    await openDialog();

    const end = screen.getByRole("button", { name: "End session" });
    fireEvent.click(end);
    await waitFor(() => expect(calls("end_debug_session")).toHaveLength(1));
    const list = screen.getByRole("listbox", { name: "Debug sessions" });
    await waitFor(() =>
      expect(within(list).getAllByRole("option")[0].textContent).toContain("Ended")
    );
    expect((end as HTMLButtonElement).disabled).toBe(true);
    expect((screen.getByLabelText("Bookmark note") as HTMLInputElement).disabled).toBe(true);
  });

  it("adds a bookmark to the session and selects it in the timeline", async () => {
    seed();
    await openDialog();

    const input = screen.getByLabelText("Bookmark note");
    fireEvent.input(input, { target: { value: "  tapped Checkout twice  " } });
    fireEvent.keyDown(input, { key: "Enter" });

    await waitFor(() => expect(calls("add_session_bookmark")).toHaveLength(1));
    expect(calls("add_session_bookmark")[0][1]).toEqual({
      sessionId: "s-20260925T103200Z-000000000002",
      note: "tapped Checkout twice",
      logEntryId: null,
    });
    await waitFor(() => expect(timelineRows()).toHaveLength(5));
    const bookmark = timelineRows()[4];
    expect(bookmark.textContent).toContain("tapped Checkout twice");
    expect(bookmark.getAttribute("aria-selected")).toBe("true");
    expect((input as HTMLInputElement).value).toBe("");
  });

  it("refuses a bookmark note over 500 characters before calling the backend", async () => {
    seed();
    await openDialog();

    const input = screen.getByLabelText("Bookmark note");
    fireEvent.input(input, { target: { value: "x".repeat(501) } });
    expect(screen.getByRole("alert").textContent).toContain("at most 500 characters");
    const add = screen.getByRole("button", { name: "Add bookmark" }) as HTMLButtonElement;
    expect(add.disabled).toBe(true);
    fireEvent.keyDown(input, { key: "Enter" });

    fireEvent.input(input, { target: { value: "x".repeat(500) } });
    expect(screen.queryByRole("alert")).toBeNull();
    expect(add.disabled).toBe(false);
    expect(calls("add_session_bookmark")).toHaveLength(0);
  });

  it("reports what refreshing the exit reasons found", async () => {
    seed();
    await openDialog();
    const refresh = screen.getByRole("button", {
      name: "Refresh exit reasons",
    }) as HTMLButtonElement;
    const idle = () => waitFor(() => expect(refresh.disabled).toBe(false));

    fake.exitRefresh = { added: 2, message: null };
    fireEvent.click(refresh);
    expect(await screen.findByText("Added 2 process exits.")).toBeTruthy();
    await idle();

    fake.exitRefresh = {
      added: 0,
      message: "Process exit reasons need Android 11 (API 30) or later.",
    };
    fireEvent.click(refresh);
    expect(await screen.findByText(/need Android 11/)).toBeTruthy();
    await idle();

    fake.failing.set("refresh_session_exit_reasons", {
      kind: "processFailed",
      message: "emulator-5554 is not connected",
    });
    fireEvent.click(refresh);
    expect((await screen.findByRole("alert")).textContent).toContain("is not connected");
  });

  it("opens a crash's kept log lines and pages back through older ones", async () => {
    seed();
    await openDialog();

    fireEvent.click(timelineRows()[3]);
    expect(screen.getByText("Keynobi's install · confirmed by the device")).toBeTruthy();
    fireEvent.click(screen.getByRole("button", { name: "Show log lines (450)" }));

    const status = await screen.findByText(/Showing the last 200 of 450 lines/);
    expect(lastArgs("get_session_capture")).toEqual({
      id: "s-20260925T103200Z-000000000002",
      seq: CRASH_SEQ,
      limit: CAPTURE_PAGE_LINES,
    });
    // The newest 200 lines, the crash's last.
    const lines = screen.getByTestId("session-capture-lines");
    expect(lines.textContent).toContain("line 251");
    expect(lines.textContent).not.toContain("line 250I");
    const older = screen.getByRole("button", { name: "Load older lines" }) as HTMLButtonElement;

    fireEvent.click(older);
    await waitFor(() => expect(status.textContent).toContain("Showing the last 400 of 450"));
    expect(lastArgs("get_session_capture")).toMatchObject({ limit: 400 });

    fireEvent.click(older);
    await waitFor(() => expect(status.textContent).toContain("Showing the last 450 of 450"));
    expect(lastArgs("get_session_capture")).toMatchObject({ limit: 450 });
    expect(older.disabled).toBe(true);
  });

  it("says when a crash kept no log lines", async () => {
    const { newest } = seed();
    fake.events.set(newest.id, [
      makeSessionEvent(1, { kind: "anr", data: makeSessionCrash({ capture: null }) }),
    ]);
    await openDialog();

    fireEvent.click(timelineRows()[0]);
    expect(screen.getByText(/No log lines were kept with this crash/)).toBeTruthy();
    expect(screen.queryByRole("button", { name: /Show log lines/ })).toBeNull();
  });

  it("moves through the timeline with the keyboard and opens a crash with Enter", async () => {
    seed();
    await openDialog();
    const timeline = screen.getByRole("listbox", { name: "Timeline, oldest first" });
    expect(timeline.tabIndex).toBe(0);

    fireEvent.keyDown(timeline, { key: "ArrowDown" });
    expect(timeline.getAttribute("aria-activedescendant")).toBe(`session-event-${CRASH_SEQ}`);
    fireEvent.keyDown(timeline, { key: "Home" });
    expect(timeline.getAttribute("aria-activedescendant")).toBe("session-event-1");
    expect(timelineRows()[0].getAttribute("aria-selected")).toBe("true");
    fireEvent.keyDown(timeline, { key: "End" });
    fireEvent.keyDown(timeline, { key: "Enter" });

    expect(await screen.findByText(/Showing the last 200 of 450 lines/)).toBeTruthy();
  });

  it("shows why the list could not be read, and retries", async () => {
    fake.failing.set("list_debug_sessions", { kind: "io", message: "index unreadable" });
    render(() => <SessionsDialog />);
    openSessionsDialog();

    const alert = await screen.findByRole("alert");
    expect(alert.textContent).toContain("Could not read debug sessions");
    expect(alert.textContent).toContain("index unreadable");

    fake.failing.delete("list_debug_sessions");
    seed();
    fireEvent.click(within(alert).getByRole("button", { name: "Retry" }));
    expect(await screen.findByRole("listbox", { name: "Debug sessions" })).toBeTruthy();
  });

  it("says when a session can no longer be read", async () => {
    seed();
    fake.failing.set("get_debug_session", {
      kind: "notFound",
      message: "Debug session s-20260925T103200Z-000000000002 is no longer kept",
    });
    render(() => <SessionsDialog />);
    openSessionsDialog();

    const alert = await screen.findByRole("alert");
    expect(alert.textContent).toContain("Could not read this session");
    expect(alert.textContent).toContain("is no longer kept");
  });

  it("says when there are no sessions yet", async () => {
    render(() => <SessionsDialog />);
    openSessionsDialog();
    expect(await screen.findByText("No debug sessions yet")).toBeTruthy();
  });

  it("reads the list again while open and shows new events of the selected session", async () => {
    vi.useFakeTimers({ shouldAdvanceTime: true });
    const { newest } = seed();
    await openDialog();
    expect(timelineRows()).toHaveLength(4);

    append(
      newest,
      makeSessionEvent(5, {
        kind: "deviceOffline",
        data: { serial: "emulator-5554" },
      })
    );
    await vi.advanceTimersByTimeAsync(SESSIONS_POLL_MS);

    await waitFor(() => expect(timelineRows()).toHaveLength(5));
    expect(timelineRows()[4].textContent).toContain("emulator-5554 went offline");
    expect(calls("list_debug_sessions").length).toBeGreaterThanOrEqual(2);

    closeSessionsDialog();
    const listed = calls("list_debug_sessions").length;
    await vi.advanceTimersByTimeAsync(SESSIONS_POLL_MS * 2);
    expect(calls("list_debug_sessions")).toHaveLength(listed);
  });

  it("is a labelled modal dialog that closes on Escape and moves focus to the list", async () => {
    seed();
    const opener = document.createElement("button");
    document.body.appendChild(opener);
    opener.focus();
    const dialog = await openDialog();

    expect(dialog.getAttribute("aria-modal")).toBe("true");
    await waitFor(() => expect(document.activeElement?.getAttribute("role")).toBe("option"));
    for (const name of ["Keep", "End session", "Refresh exit reasons", "Add bookmark", "Close"]) {
      expect(within(dialog).getByRole("button", { name })).toBeTruthy();
    }

    fireEvent.keyDown(dialog, { key: "Escape" });
    await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
    expect(document.activeElement).toBe(opener);
    opener.remove();
  });
});
