#!/usr/bin/env node
/**
 * Logcat soak measurement: builds the `logcat_soak` example in release mode,
 * runs it (synthetic logcat through the real ingestion pipeline, no device),
 * and saves its report with build provenance to
 * `perf-metrics/soak_<timestamp>.json`.
 *
 * A baseline, not a gate: it has no pass/fail thresholds. Run it on demand
 * (before a release, or nightly), not on every change.
 *
 * Usage:
 *   node scripts/logcat-soak.mjs [--rate 1000] [--duration-secs 600]
 *       [--sample-secs 10] [--giant-line-mb 10] [--out-dir DIR]
 */

import { execFileSync } from "child_process";
import { existsSync, mkdirSync, writeFileSync } from "fs";
import { join, relative, resolve } from "path";
import { fileURLToPath } from "url";
import { ROOT, cargoTargetDir, collectProvenance, defaultExec } from "./metrics-provenance.mjs";

const PROFILE = "release";
const SOAK_FLAGS = ["--rate", "--duration-secs", "--sample-secs", "--giant-line-mb"];

export function parseSoakArgs(argv) {
  const args = argv.slice(2);
  const soakArgs = [];
  let outDir = join(ROOT, "perf-metrics");
  for (let i = 0; i < args.length; i += 2) {
    const [flag, value] = [args[i], args[i + 1]];
    if (value === undefined) throw new Error(`${flag} needs a value`);
    if (flag === "--out-dir") outDir = resolve(value);
    else if (SOAK_FLAGS.includes(flag)) soakArgs.push(flag, value);
    else throw new Error(`unknown flag ${flag}`);
  }
  return { soakArgs, outDir };
}

function defaultRunCommand(cmd, args, options = {}) {
  execFileSync(cmd, args, { cwd: ROOT, stdio: "inherit", ...options });
}

function defaultRunSoak(binary, args) {
  return execFileSync(binary, args, {
    encoding: "utf8",
    stdio: ["ignore", "pipe", "inherit"],
    maxBuffer: 64 * 1024 * 1024,
  });
}

/** Build the soak binary fresh, run it, and return the report with provenance. */
export function runSoak({
  soakArgs,
  exec = defaultExec,
  runCommand = defaultRunCommand,
  runBinary = defaultRunSoak,
  targetDir = cargoTargetDir(),
  now = () => Date.now(),
}) {
  runCommand("cargo", ["build", `--${PROFILE}`, "--example", "logcat_soak"], {
    cwd: join(ROOT, "src-tauri"),
  });
  const binary = join(targetDir, PROFILE, "examples", "logcat_soak");
  if (!existsSync(binary)) throw new Error(`cargo build did not produce ${binary}`);

  const provenance = collectProvenance({ exec, profile: PROFILE, artifacts: { soakBinary: binary } });
  const startedAt = new Date(now()).toISOString();
  const report = JSON.parse(runBinary(binary, soakArgs));
  if (report.error) throw new Error(`soak failed: ${report.error}`);
  return { kind: "logcat-soak", timestamp: startedAt, provenance, ...report };
}

function mib(bytes) {
  return `${(bytes / 1024 / 1024).toFixed(1)} MiB`;
}

function main() {
  const { soakArgs, outDir } = parseSoakArgs(process.argv);
  const result = runSoak({ soakArgs });
  mkdirSync(outDir, { recursive: true });
  const file = join(outDir, `soak_${result.timestamp.replace(/[:.]/g, "-")}.json`);
  writeFileSync(file, JSON.stringify(result, null, 2) + "\n");

  const { rss, entries, batches, trackedPids, giantLine, provenance } = result;
  console.log(`\nSoak saved to ${relative(ROOT, file)}`);
  console.log(`  Commit:      ${provenance.gitCommitShort}${provenance.dirty ? " (dirty tree)" : ""}`);
  console.log(`  RSS:         start ${mib(rss.startBytes)}, peak ${mib(rss.peakBytes)}, end ${mib(rss.endBytes)}`);
  console.log(
    `  Entries:     ${entries.ingested} ingested (${entries.ingestedPerSec.toFixed(0)}/s), ${entries.dropped} dropped`
  );
  const l = batches.latencyUs;
  console.log(
    `  Batches:     ${batches.count}, latency p50 ${l.p50} µs, p99 ${l.p99} µs, max ${l.max} µs, backlog max ${batches.backlogMax}`
  );
  console.log(`  Tracked PIDs: max ${trackedPids.max}, end ${trackedPids.end}`);
  if (giantLine) {
    console.log(
      `  Giant line:  ${mib(giantLine.sentBytes)} sent, ${giantLine.storedBytes} bytes stored, truncated visibly: ${giantLine.truncatedVisibly}`
    );
  }
}

if (process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  main();
}
