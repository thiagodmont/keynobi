import { describe, expect, it } from "vitest";
import type { AppExitRecord, DebugSessionEvent } from "@/bindings";
import { makeLaunchTiming } from "@/test/factories/build";
import {
  makeSession,
  makeSessionCrash,
  makeSessionDetail,
  makeSessionEvent,
  summaryOf,
} from "@/test/factories/sessions";
import {
  actorLabel,
  attributionLabel,
  crashCountLabel,
  describeEvent,
  isUnattributed,
  sessionBuildLabel,
  sessionDeviceLabel,
  sessionState,
} from "./session-format";
import { timelineEvents } from "./SessionDetail";

const exitRecord: AppExitRecord = {
  timestamp: "2026-09-25 10:15:03.482",
  timestampLocal: "2026-09-25T10:15:03.482",
  pid: 4242,
  processName: "com.example.app",
  reason: "crash",
  reasonCode: 4,
  reasonLabel: "APP CRASH(EXCEPTION)",
  subReasonCode: 0,
  subReason: null,
  status: 0,
  importance: 100,
  importanceName: "foreground",
  pssKb: null,
  rssKb: null,
  description: "crash",
};

describe("session labels", () => {
  it("names the state, closed sessions by why they closed", () => {
    expect(sessionState({ closedAt: null, closeReason: null })).toBe("open");
    expect(sessionState({ closedAt: "2026-09-25T11:00:00Z", closeReason: "superseded" })).toBe(
      "superseded"
    );
    expect(sessionState({ closedAt: "2026-09-25T11:00:00Z", closeReason: "idle" })).toBe("idle");
  });

  it("names the build, or why there is none", () => {
    const withBuild = summaryOf(makeSession());
    expect(sessionBuildLabel(withBuild)).toBe("#12 · :app debug");
    expect(sessionBuildLabel(summaryOf(makeSession({ build: null })))).toBe("No build record");
    const unattributed = summaryOf(makeSession({ build: null, install: null }));
    expect(sessionBuildLabel(unattributed)).toBe("No Keynobi install");
    expect(isUnattributed(unattributed)).toBe(true);
    expect(isUnattributed(withBuild)).toBe(false);
  });

  it("names an emulator by its AVD, else the model, else the serial", () => {
    expect(sessionDeviceLabel({ serial: "emulator-5554", avdName: "Pixel_7", model: "x" })).toBe(
      "Pixel_7"
    );
    expect(sessionDeviceLabel({ serial: "R58M", avdName: null, model: "SM-G991B" })).toBe(
      "SM-G991B"
    );
    expect(sessionDeviceLabel({ serial: "R58M", avdName: null, model: null })).toBe("R58M");
  });

  it("counts crashes and ANRs in words", () => {
    const counts = makeSession().counts;
    expect(crashCountLabel(counts)).toBe("no crashes");
    expect(crashCountLabel({ ...counts, crashes: 2 })).toBe("2 crashes");
    expect(crashCountLabel({ ...counts, crashes: 1, anrs: 1 })).toBe("1 crash, 1 ANR");
  });

  it("names who acted, and says when an agent ran standalone", () => {
    expect(actorLabel(null)).toBeNull();
    expect(actorLabel({ kind: "app" })).toBe("Keynobi");
    expect(
      actorLabel({ kind: "agent", sessionId: 3, clientName: "Claude Code", standalone: false })
    ).toBe("an agent (Claude Code)");
    expect(actorLabel({ kind: "agent", sessionId: null, clientName: null, standalone: true })).toBe(
      "an agent, standalone"
    );
  });

  it("says how a crash was attributed", () => {
    expect(attributionLabel(makeSessionCrash())).toBe(
      "Keynobi's install · confirmed by the device"
    );
    expect(
      attributionLabel(
        makeSessionCrash({
          attribution: { method: "installRecord", verified: false, reason: "offline" },
        })
      )
    ).toBe("Keynobi's install · not confirmed");
    expect(
      attributionLabel(
        makeSessionCrash({
          attribution: { method: "unattributed", verified: false, reason: null },
        })
      )
    ).toBe("Not from a Keynobi install");
  });
});

describe("describeEvent", () => {
  const cases: [DebugSessionEvent, string, string][] = [
    [
      makeSessionEvent(1, {
        kind: "launch",
        data: { serial: "emulator-5554", timing: null, restart: true },
      }),
      "Restart",
      "Restarted · no launch time reported",
    ],
    [
      makeSessionEvent(
        1,
        {
          kind: "launch",
          data: { serial: "emulator-5554", timing: makeLaunchTiming(), restart: false },
        },
        {
          actor: { kind: "agent", sessionId: null, clientName: "Codex", standalone: true },
        }
      ),
      "Launch",
      "Launched · Launch 812 ms (cold) · by an agent (Codex), standalone",
    ],
    [
      makeSessionEvent(1, { kind: "logcatStopped", data: { serial: "e", reason: "adb died" } }),
      "Logcat",
      "Logcat stopped: adb died",
    ],
    [
      makeSessionEvent(1, { kind: "logcatReconnect", data: { serial: "e", reason: null } }),
      "Logcat",
      "Logcat reconnecting to e",
    ],
    [
      makeSessionEvent(1, { kind: "logcatCleared", data: { serial: "e", reason: null } }),
      "Logcat",
      "Logcat cleared",
    ],
    [
      makeSessionEvent(1, { kind: "deviceOnline", data: { serial: "emulator-5554" } }),
      "Online",
      "emulator-5554 is back online",
    ],
    [
      makeSessionEvent(1, { kind: "bookmark", data: { note: "tapped Pay", logEntryId: 7 } }),
      "Bookmark",
      "tapped Pay",
    ],
    [
      makeSessionEvent(1, { kind: "anr", data: makeSessionCrash({ summary: "ANR in x" }) }),
      "ANR",
      "ANR in x",
    ],
    [
      makeSessionEvent(1, {
        kind: "exit",
        data: {
          serial: "emulator-5554",
          exitedAt: "2026-09-25T10:15:03Z",
          matchedBy: "pid",
          record: exitRecord,
        },
      }),
      "Exit",
      "Exit: Crash · com.example.app · pid 4242 · matched by pid",
    ],
    [
      makeSessionEvent(
        1,
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
      ),
      "Agent",
      "ui_tap by an agent (Claude Code) · 412 ms · failed",
    ],
    [
      makeSessionEvent(1, {
        kind: "agentAction",
        data: { tool: "launch_app", kind: "write", ok: true, durationMs: 90, serial: "e" },
      }),
      "Agent",
      "launch_app · 90 ms",
    ],
  ];

  it.each(cases)("describes %#", (event, label, text) => {
    const view = describeEvent(event);
    expect(view.label).toBe(label);
    expect(view.text).toBe(text);
  });

  it("describes display times that arrived later", () => {
    const view = describeEvent(
      makeSessionEvent(1, {
        kind: "launchTiming",
        data: makeLaunchTiming({ displayedMs: 790, fullyDrawnMs: 1400 }),
      })
    );
    expect(view.text).toMatch(/displayed 790 ms · fully drawn 1.4 s$/);
  });
});

describe("timelineEvents", () => {
  it("adds the crashes older than the events returned, in order", () => {
    const crash = makeSessionEvent(2, { kind: "crash", data: makeSessionCrash() });
    const recent = [
      makeSessionEvent(700, { kind: "deviceOnline", data: { serial: "e" } }),
      makeSessionEvent(701, { kind: "deviceOffline", data: { serial: "e" } }),
    ];
    const detail = { ...makeSessionDetail(makeSession(), recent), crashes: [crash] };
    expect(timelineEvents(detail).map((e) => e.seq)).toEqual([2, 700, 701]);
    const plain = makeSessionDetail(makeSession(), recent);
    expect(timelineEvents(plain)).toBe(plain.events);
  });
});
