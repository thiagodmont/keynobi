import { cleanup, render, screen } from "@solidjs/testing-library";
import { afterEach, beforeEach, describe, expect, it } from "vitest";
import { mappingLabel } from "./MappingSnapshotSummary";
import { HistoryViewBanner } from "./BuildHistoryView";
import {
  resetBuildState,
  setBuildHistory,
  viewHistoryBuild,
  viewedBuild,
} from "@/stores/build.store";
import { makeBuildRecord, makeMappingSnapshot } from "@/test/factories/build";

describe("mappingLabel", () => {
  it("names the variant and the map id", () => {
    expect(mappingLabel(makeMappingSnapshot(), false)).toBe(
      "R8 mapping saved: release (map id 6b1c2f0)"
    );
  });

  it("shortens a long map id", () => {
    const mapping = makeMappingSnapshot({ pgMapId: "6b1c2f0aa9e1" });
    expect(mappingLabel(mapping, false)).toBe("R8 mapping saved: release (map id 6b1c2f0…)");
  });

  it("leaves out a missing map id and names the module when asked", () => {
    const mapping = makeMappingSnapshot({ module: ":wear", variant: "paidRelease", pgMapId: null });
    expect(mappingLabel(mapping, true)).toBe("R8 mapping saved: :wear paidRelease");
  });
});

describe("saved mappings in the past-build banner", () => {
  beforeEach(() => {
    resetBuildState();
    setBuildHistory([
      makeBuildRecord({ id: 1, task: "assembleDebug" }),
      makeBuildRecord({ id: 2, task: "assembleRelease", mappings: [makeMappingSnapshot()] }),
      makeBuildRecord({
        id: 3,
        task: "assembleRelease",
        mappings: [
          makeMappingSnapshot(),
          makeMappingSnapshot({ module: ":wear", pgMapId: null, sha256: "0f".repeat(32) }),
        ],
      }),
    ]);
  });

  afterEach(() => {
    cleanup();
    resetBuildState();
  });

  it("says a mapping was saved, with its details in the tooltip", () => {
    viewHistoryBuild(2);
    render(() => <HistoryViewBanner view={viewedBuild()} onBack={() => {}} />);

    const badge = screen.getByText("R8 mapping saved: release (map id 6b1c2f0)");
    expect(badge.getAttribute("title")).toBe(
      ":app release · map id 6b1c2f0 · SHA-256 6b1c2f0a6b1c… · 46.0 MB"
    );
  });

  it("names the module when the build saved mappings of several", () => {
    viewHistoryBuild(3);
    render(() => <HistoryViewBanner view={viewedBuild()} onBack={() => {}} />);

    expect(screen.getByText("R8 mapping saved: :app release (map id 6b1c2f0)")).not.toBeNull();
    expect(screen.getByText("R8 mapping saved: :wear release")).not.toBeNull();
  });

  it("says nothing about mappings for a build that saved none", () => {
    viewHistoryBuild(1);
    render(() => <HistoryViewBanner view={viewedBuild()} onBack={() => {}} />);

    expect(screen.queryByText(/R8 mapping saved/)).toBeNull();
  });
});
