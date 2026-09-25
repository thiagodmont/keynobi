import { describe, it, expect, beforeEach, afterEach } from "vitest";
import { mkdtempSync, mkdirSync, rmSync, utimesSync, writeFileSync } from "fs";
import { tmpdir } from "os";
import { join } from "path";
import {
  archiveName,
  collectMetrics,
  comparabilityWarnings,
  freshCriterionEstimates,
} from "./collect-metrics.mjs";
import { collectProvenance } from "./metrics-provenance.mjs";
import { parseSoakArgs, runSoak } from "./logcat-soak.mjs";

const RUN_START = Date.parse("2026-09-01T12:00:00Z");

function fakeExec({ porcelain = "" } = {}) {
  return (cmd, args) => {
    const line = [cmd, ...args].join(" ");
    if (line === "git rev-parse HEAD") return "0123456789abcdef0123456789abcdef01234567";
    if (line.startsWith("git status --porcelain")) return porcelain;
    if (line === "rustc -V") return "rustc 1.97.1 (fake)";
    if (line === "cargo -V") return "cargo 1.97.1 (fake)";
    if (line === "sysctl -n hw.model") return "FakeMac1,1";
    throw new Error(`unexpected command: ${line}`);
  };
}

function writeEstimate(dir, parts, meanNs, mtimeMs) {
  const folder = join(dir, ...parts);
  mkdirSync(folder, { recursive: true });
  const file = join(folder, "estimates.json");
  writeFileSync(
    file,
    JSON.stringify({ mean: { point_estimate: meanNs }, std_dev: { point_estimate: 1 } })
  );
  utimesSync(file, mtimeMs / 1000, mtimeMs / 1000);
}

describe("collect-metrics", () => {
  let tmp;
  let root;
  let target;

  beforeEach(() => {
    tmp = mkdtempSync(join(tmpdir(), "collect-metrics-test-"));
    root = join(tmp, "repo");
    target = join(tmp, "target");
    mkdirSync(join(root, "src-tauri"), { recursive: true });
    writeFileSync(join(root, "package.json"), JSON.stringify({ version: "9.9.9" }));
  });

  afterEach(() => {
    rmSync(tmp, { recursive: true, force: true });
  });

  /** A build runner that records commands and produces the artifacts. */
  function fakeBuilds(calls, { benchMtime = RUN_START + 5_000 } = {}) {
    return (cmd, args) => {
      calls.push([cmd, ...args].join(" "));
      if (cmd === "npm") {
        mkdirSync(join(root, "dist"), { recursive: true });
        writeFileSync(join(root, "dist", "index.js"), "x".repeat(100));
      } else if (args[0] === "build") {
        mkdirSync(join(target, "release"), { recursive: true });
        writeFileSync(join(target, "release", "keynobi"), "b".repeat(42));
      } else if (args[0] === "bench") {
        writeEstimate(join(target, "criterion"), ["find_gradle_root", "new"], 1500, benchMtime);
      }
    };
  }

  it("ignores Criterion results older than this run and base/ copies", () => {
    const dir = join(tmp, "criterion");
    writeEstimate(dir, ["fresh_bench", "new"], 1000, RUN_START + 1);
    writeEstimate(dir, ["fresh_bench", "base"], 9999, RUN_START + 1);
    writeEstimate(dir, ["group", "case_a", "new"], 2000, RUN_START + 1);
    writeEstimate(dir, ["removed_bench", "new"], 3000, RUN_START - 60_000);

    const { benchmarks, stale } = freshCriterionEstimates(dir, RUN_START);

    expect(benchmarks).toEqual({
      fresh_bench: { meanNs: 1000, stddevNs: 1 },
      group_case_a: { meanNs: 2000, stddevNs: 1 },
    });
    expect(stale).toEqual(["removed_bench"]);
  });

  it("rebuilds every artifact even when earlier builds are present", () => {
    // Leftovers from an earlier build at another commit.
    mkdirSync(join(root, "dist"), { recursive: true });
    writeFileSync(join(root, "dist", "old.js"), "old");
    mkdirSync(join(target, "release"), { recursive: true });
    writeFileSync(join(target, "release", "keynobi"), "stale");
    writeEstimate(join(target, "criterion"), ["old_bench", "new"], 7, RUN_START - 60_000);

    const calls = [];
    const entry = collectMetrics({
      root,
      options: {},
      exec: fakeExec(),
      runCommand: fakeBuilds(calls),
      targetDir: target,
      now: () => RUN_START,
      log: () => {},
    });

    expect(calls).toEqual([
      "npm run build",
      "cargo build --release --bin keynobi",
      "cargo bench --benches",
    ]);
    expect(entry.rust.binarySizeBytes).toBe(42);
    expect(entry.rust.benchmarks).toEqual({ find_gradle_root: { meanNs: 1500, stddevNs: 1 } });
    expect(entry.rust.staleBenchmarks).toEqual(["old_bench"]);
    expect(entry.provenance.artifacts.rustBinary.sha256).toMatch(/^[0-9a-f]{64}$/);
    expect(entry.provenance.artifacts.frontendDist.sha256).toMatch(/^[0-9a-f]{64}$/);
  });

  it("reports nothing for a skipped step instead of a leftover artifact", () => {
    mkdirSync(join(target, "release"), { recursive: true });
    writeFileSync(join(target, "release", "keynobi"), "stale");
    writeEstimate(join(target, "criterion"), ["old_bench", "new"], 7, RUN_START - 60_000);

    const calls = [];
    const entry = collectMetrics({
      root,
      options: { skipRust: true, skipBench: true },
      exec: fakeExec(),
      runCommand: fakeBuilds(calls),
      targetDir: target,
      now: () => RUN_START,
      log: () => {},
    });

    expect(calls).toEqual(["npm run build"]);
    expect(entry.rust.binarySizeBytes).toBeNull();
    expect(entry.rust.benchmarks).toEqual({});
    expect(entry.provenance.artifacts.rustBinary).toBeNull();
    expect(entry.skipped).toEqual(["rust-binary", "benchmarks"]);
  });

  it("refuses to report when a build does not produce its artifact", () => {
    expect(() =>
      collectMetrics({
        root,
        options: { skipFrontend: true, skipBench: true },
        exec: fakeExec(),
        runCommand: () => {},
        targetDir: target,
        now: () => RUN_START,
        log: () => {},
      })
    ).toThrow(/did not produce/);
  });

  it("records a dirty tree and archives it apart from the clean commit", () => {
    const entry = collectMetrics({
      root,
      options: { skipFrontend: true, skipRust: true, skipBench: true },
      exec: fakeExec({ porcelain: " M src-tauri/src/lib.rs\n?? scratch.txt" }),
      runCommand: () => {},
      targetDir: target,
      now: () => RUN_START,
      log: () => {},
    });

    expect(entry.dirty).toBe(true);
    expect(entry.provenance.changedFiles).toBe(2);
    expect(entry.gitCommit).toBe("0123456");
    expect(archiveName(entry)).toBe("metrics_0123456-dirty.json");
    expect(archiveName({ ...entry, dirty: false })).toBe("metrics_0123456.json");
    expect(comparabilityWarnings(entry, null)).toContain(
      "latest snapshot was taken on a dirty tree"
    );
  });

  it("records toolchain, profile, arch, and hardware", () => {
    const p = collectProvenance({ exec: fakeExec(), profile: "release" });

    expect(p.dirty).toBe(false);
    expect(p.gitCommit).toBe("0123456789abcdef0123456789abcdef01234567");
    expect(p.profile).toBe("release");
    expect(p.arch).toBe(process.arch);
    expect(p.toolchain).toEqual({
      rustc: "rustc 1.97.1 (fake)",
      cargo: "cargo 1.97.1 (fake)",
      node: process.version,
    });
    expect(p.hardware.model).toBe(
      process.platform === "darwin" ? "FakeMac1,1" : expect.any(String)
    );
  });

  it("treats an unreadable git status as dirty", () => {
    const exec = (cmd, args) => {
      if (cmd === "git" && args[0] === "status") throw new Error("not a repo");
      return fakeExec()(cmd, args);
    };

    expect(collectProvenance({ exec }).dirty).toBe(true);
  });

  it("warns when snapshots come from different machines or profiles", () => {
    const base = collectProvenance({ exec: fakeExec(), profile: "release" });
    const other = { ...base, arch: "x64", hardware: { ...base.hardware, model: "Other" } };

    const warnings = comparabilityWarnings({ provenance: other }, { provenance: base });

    expect(warnings.some((w) => w.startsWith("arch differs"))).toBe(true);
    expect(warnings.some((w) => w.startsWith("hardware differs"))).toBe(true);
  });
});

describe("logcat-soak", () => {
  it("builds the soak binary before running it and attaches provenance", () => {
    const tmp = mkdtempSync(join(tmpdir(), "logcat-soak-test-"));
    try {
      const calls = [];
      const result = runSoak({
        soakArgs: ["--duration-secs", "5"],
        exec: fakeExec(),
        targetDir: tmp,
        now: () => RUN_START,
        runCommand: (cmd, args) => {
          calls.push([cmd, ...args].join(" "));
          mkdirSync(join(tmp, "release", "examples"), { recursive: true });
          writeFileSync(join(tmp, "release", "examples", "logcat_soak"), "bin");
        },
        runBinary: (binary, args) => {
          calls.push(`run ${args.join(" ")}`);
          return JSON.stringify({ entries: { ingested: 5 } });
        },
      });

      expect(calls).toEqual([
        "cargo build --release --example logcat_soak",
        "run --duration-secs 5",
      ]);
      expect(result.kind).toBe("logcat-soak");
      expect(result.entries.ingested).toBe(5);
      expect(result.provenance.profile).toBe("release");
      expect(result.provenance.artifacts.soakBinary.sha256).toMatch(/^[0-9a-f]{64}$/);
    } finally {
      rmSync(tmp, { recursive: true, force: true });
    }
  });

  it("passes soak flags through and rejects unknown ones", () => {
    expect(parseSoakArgs(["node", "s", "--rate", "5000", "--out-dir", "/tmp/x"])).toEqual({
      soakArgs: ["--rate", "5000"],
      outDir: "/tmp/x",
    });
    expect(() => parseSoakArgs(["node", "s", "--bogus", "1"])).toThrow(/unknown flag/);
  });
});
