import { cleanup, render, screen } from "@solidjs/testing-library";
import { afterEach, beforeEach, describe, expect, it } from "vitest";
import { LaunchTimingSummary } from "./LaunchTimingSummary";
import { BuildHistoryPanel } from "./BuildHistoryPanel";
import { HistoryViewBanner } from "./BuildHistoryView";
import {
  resetBuildState,
  setBuildHistory,
  viewHistoryBuild,
  viewedBuild,
} from "@/stores/build.store";
import { makeBuildRecord, makeLaunchTiming } from "@/test/factories/build";

const earlier = makeBuildRecord({
  id: 41,
  task: "assembleDebug",
  launch: makeLaunchTiming({ totalMs: 758 }),
});
const current = makeBuildRecord({
  id: 42,
  task: "assembleDebug",
  launch: makeLaunchTiming({ totalMs: 812 }),
});

describe("LaunchTimingSummary", () => {
  afterEach(cleanup);

  it("shows the launch time, its state, and the change from the comparable build", () => {
    render(() => <LaunchTimingSummary record={current} history={[earlier, current]} />);

    const summary = screen.getByTestId("launch-timing");
    expect(summary.textContent).toBe("Launch 812 ms (cold) · +54 ms vs #41");
    expect(screen.getByTitle("54 ms slower than build #41").textContent).toBe("+54 ms vs #41");
  });

  it("shows no comparison when no earlier build qualifies", () => {
    render(() => <LaunchTimingSummary record={current} history={[current]} />);

    expect(screen.getByTestId("launch-timing").textContent).toBe("Launch 812 ms (cold)");
  });

  it("renders nothing for a build without a launch time", () => {
    render(() => <LaunchTimingSummary record={makeBuildRecord()} history={[]} />);

    expect(screen.queryByTestId("launch-timing")).toBeNull();
  });
});

describe("launch time in the build history", () => {
  beforeEach(() => {
    resetBuildState();
    setBuildHistory([earlier, current, makeBuildRecord({ id: 43, task: "assembleRelease" })]);
  });

  afterEach(() => {
    cleanup();
    resetBuildState();
  });

  it("lists the launch time on the rows of builds that launched", () => {
    render(() => <BuildHistoryPanel selectedId={null} onSelect={() => {}} />);

    const rows = screen.getAllByTestId("launch-timing").map((el) => el.textContent);
    expect(rows).toEqual(["Launch 812 ms (cold) · +54 ms vs #41", "Launch 758 ms (cold)"]);
  });

  it("shows the launch time and comparison of the build being viewed", () => {
    viewHistoryBuild(42);
    render(() => <HistoryViewBanner view={viewedBuild()} onBack={() => {}} />);

    expect(screen.getByTestId("launch-timing").textContent).toBe(
      "Launch 812 ms (cold) · +54 ms vs #41"
    );
  });

  it("shows no launch time in the banner of a build that did not launch", () => {
    viewHistoryBuild(43);
    render(() => <HistoryViewBanner view={viewedBuild()} onBack={() => {}} />);

    expect(screen.queryByTestId("launch-timing")).toBeNull();
  });
});
