import { describe, expect, it } from "vitest";
import { makeBuildRecord, makeLaunchTiming } from "@/test/factories/build";
import {
  compareLaunch,
  describeLaunchDelta,
  formatLaunchDelta,
  formatLaunchTime,
  sameLaunchDevice,
} from "@/lib/launch-timing";

const earlier = makeBuildRecord({ id: 41, launch: makeLaunchTiming({ totalMs: 758 }) });
const current = makeBuildRecord({ id: 42, launch: makeLaunchTiming({ totalMs: 812 }) });

describe("formatLaunchTime", () => {
  it("names the launch state when the device reported it", () => {
    expect(formatLaunchTime(makeLaunchTiming())).toBe("812 ms (cold)");
    expect(formatLaunchTime(makeLaunchTiming({ launchState: null }))).toBe("812 ms");
  });
});

describe("compareLaunch", () => {
  it("compares with the most recent earlier launch of the same task, state, and device", () => {
    const older = makeBuildRecord({ id: 30, launch: makeLaunchTiming({ totalMs: 900 }) });
    const later = makeBuildRecord({ id: 50, launch: makeLaunchTiming({ totalMs: 100 }) });

    expect(compareLaunch([older, earlier, current, later], current)).toEqual({
      previousId: 41,
      deltaMs: 54,
    });
  });

  it("has nothing to compare for the first launch or a build without one", () => {
    expect(compareLaunch([current], current)).toBeNull();
    const unlaunched = makeBuildRecord({ id: 43 });
    expect(compareLaunch([earlier, unlaunched], unlaunched)).toBeNull();
    const noLaunchBefore = makeBuildRecord({ id: 41 });
    expect(compareLaunch([noLaunchBefore, current], current)).toBeNull();
  });

  it("does not compare launches in different states", () => {
    const warm = makeBuildRecord({
      id: 41,
      launch: makeLaunchTiming({ totalMs: 240, launchState: "warm" }),
    });
    expect(compareLaunch([warm, current], current)).toBeNull();
    const unknown = makeBuildRecord({
      id: 41,
      launch: makeLaunchTiming({ launchState: null }),
    });
    expect(compareLaunch([unknown, current], current)).toBeNull();
  });

  it("does not compare launches on different devices", () => {
    const otherAvd = makeBuildRecord({
      id: 41,
      // Same serial, another AVD: emulator serials are reused.
      launch: makeLaunchTiming({ avdName: "Pixel_9_API_35" }),
    });
    expect(compareLaunch([otherAvd, current], current)).toBeNull();

    const phoneLaunch = (serial: string) =>
      makeLaunchTiming({ serial, avdName: null, model: "Pixel 7" });
    const phoneBefore = makeBuildRecord({ id: 41, launch: phoneLaunch("28151FDH2000Q4") });
    const otherPhone = makeBuildRecord({ id: 41, launch: phoneLaunch("0A1B2C3D") });
    const phoneNow = makeBuildRecord({ id: 42, launch: phoneLaunch("28151FDH2000Q4") });
    expect(compareLaunch([phoneBefore, phoneNow], phoneNow)?.previousId).toBe(41);
    expect(compareLaunch([otherPhone, phoneNow], phoneNow)).toBeNull();
  });

  it("does not compare different tasks or projects", () => {
    const release = makeBuildRecord({ id: 41, task: "assembleRelease", launch: earlier.launch });
    expect(compareLaunch([release, current], current)).toBeNull();
    const otherProject = makeBuildRecord({ id: 41, projectRoot: "/other", launch: earlier.launch });
    expect(compareLaunch([otherProject, current], current)).toBeNull();
  });
});

describe("sameLaunchDevice", () => {
  it("matches emulators by AVD name even across serials", () => {
    expect(
      sameLaunchDevice(makeLaunchTiming(), makeLaunchTiming({ serial: "emulator-5556" }))
    ).toBe(true);
  });
});

describe("launch delta text", () => {
  it("states the direction in text, not only color", () => {
    expect(formatLaunchDelta({ previousId: 41, deltaMs: 54 })).toBe("+54 ms vs #41");
    expect(formatLaunchDelta({ previousId: 41, deltaMs: -12 })).toBe("−12 ms vs #41");
    expect(formatLaunchDelta({ previousId: 41, deltaMs: 0 })).toBe("±0 ms vs #41");
    expect(describeLaunchDelta({ previousId: 41, deltaMs: 54 })).toBe(
      "54 ms slower than build #41"
    );
    expect(describeLaunchDelta({ previousId: 41, deltaMs: -12 })).toBe(
      "12 ms faster than build #41"
    );
  });
});
