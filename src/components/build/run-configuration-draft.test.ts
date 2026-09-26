import { describe, it, expect } from "vitest";
import {
  configurationFrom,
  defaultTask,
  draftChanged,
  draftFrom,
  parseTarget,
  targetValue,
  validateDraft,
} from "./run-configuration-draft";
import { makeRunConfiguration } from "@/test/factories/build";
import type { TargetPreference } from "@/bindings";

describe("run configuration drafts", () => {
  it("names the module's assemble task", () => {
    expect(defaultTask(":app", "freeRelease")).toBe(":app:assembleFreeRelease");
    expect(defaultTask(":", "debug")).toBe("assembleDebug");
  });

  it("round-trips every target through a select value", () => {
    const targets: TargetPreference[] = [
      { kind: "ask" },
      { kind: "lastUsed" },
      { kind: "serial", serial: "emulator-5554" },
      { kind: "avd", name: "Pixel_7" },
    ];
    for (const target of targets) expect(parseTarget(targetValue(target))).toEqual(target);
  });

  it("shows the default task, and saves it as none", () => {
    const draft = draftFrom(makeRunConfiguration(), { kind: "lastUsed" });
    expect(draft.task).toBe(":app:assembleDebug");
    expect(configurationFrom(draft).task).toBeNull();
    expect(configurationFrom({ ...draft, task: ":app:bundleDebug" }).task).toBe(":app:bundleDebug");
  });

  it("keeps the launch it edits, and saves an empty filter as none", () => {
    const draft = draftFrom(
      makeRunConfiguration({ launch: { kind: "deepLink", uri: "myapp://home" } }),
      { kind: "ask" }
    );
    expect(draft.uri).toBe("myapp://home");
    expect(configurationFrom({ ...draft, logcatFilter: "  " })).toMatchObject({
      launch: { kind: "deepLink", uri: "myapp://home" },
      logcatFilter: null,
    });
    expect(
      configurationFrom({ ...draft, launchKind: "activity", activity: " .Settings " }).launch
    ).toEqual({ kind: "activity", name: ".Settings" });
  });

  it("finds problems before saving", () => {
    const draft = draftFrom(makeRunConfiguration(), { kind: "lastUsed" });
    expect(validateDraft(draft)).toEqual({});
    expect(validateDraft({ ...draft, name: " " }).name).toBe("Name the configuration.");
    expect(validateDraft({ ...draft, task: "" }).task).toBe("Name the task to build.");
    expect(validateDraft({ ...draft, launchKind: "activity" }).launch).toContain("activity");
    expect(validateDraft({ ...draft, launchKind: "deepLink" }).launch).toContain("deep link");
    expect(validateDraft({ ...draft, logcatFilter: "level:loud" }).logcatFilter).toBeTruthy();
    expect(validateDraft({ ...draft, logcatFilter: "" }).logcatFilter).toBeUndefined();
  });

  it("knows when a draft differs from what was saved", () => {
    const saved = draftFrom(makeRunConfiguration(), { kind: "lastUsed" });
    expect(draftChanged({ ...saved }, saved)).toBe(false);
    expect(draftChanged({ ...saved, variant: "release" }, saved)).toBe(true);
    expect(draftChanged(saved, null)).toBe(true);
  });
});
