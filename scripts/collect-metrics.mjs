#!/usr/bin/env node
/**
 * Performance metrics collector.
 *
 * Collects frontend bundle sizes, Rust binary size, and Criterion benchmark
 * results into individual JSON files inside `perf-metrics/` for regression
 * tracking across builds.
 *
 * Every number comes from an artifact built during this run: the frontend is
 * rebuilt, the release binary is built by Cargo (a no-op when it is already
 * up to date), and only Criterion results written by this run's `cargo bench`
 * are read. A skipped step reports nothing instead of whatever an earlier
 * build left behind. Each snapshot records its provenance (commit, dirty
 * tree, profile, arch, toolchain, hardware, artifact hashes).
 *
 * File naming:
 *   perf-metrics/metrics_latest.json          — always the most recent run
 *   perf-metrics/metrics_{commit}[-dirty].json — archived runs
 *
 * Usage:
 *   node scripts/collect-metrics.mjs [--skip-frontend] [--skip-rust] [--skip-bench] [--out-dir DIR]
 *   node scripts/collect-metrics.mjs --report   # compare latest vs previous
 */

import { execFileSync } from "child_process";
import {
  existsSync,
  readFileSync,
  writeFileSync,
  renameSync,
  mkdirSync,
  statSync,
  readdirSync,
} from "fs";
import { join, relative, resolve } from "path";
import { fileURLToPath } from "url";
import { ROOT, cargoTargetDir, collectProvenance, defaultExec } from "./metrics-provenance.mjs";

const RUST_PROFILE = "release";

// ── Helpers ───────────────────────────────────────────────────────────────────

function getVersion(root) {
  try {
    const pkg = JSON.parse(readFileSync(join(root, "package.json"), "utf8"));
    return pkg.version ?? "0.0.0";
  } catch {
    return "0.0.0";
  }
}

function walkFiles(dir) {
  if (!existsSync(dir)) return [];
  return readdirSync(dir, { withFileTypes: true, recursive: true })
    .filter((entry) => entry.isFile())
    .map((entry) => join(entry.parentPath ?? entry.path, entry.name));
}

function sizeOf(path) {
  try {
    return statSync(path).size;
  } catch {
    return 0;
  }
}

function hasExt(file, extensions) {
  return !extensions || extensions.some((ext) => file.endsWith(ext));
}

function dirSize(dir, extensions) {
  return walkFiles(dir)
    .filter((f) => hasExt(f, extensions))
    .reduce((total, f) => total + sizeOf(f), 0);
}

function largestFile(dir, extensions) {
  return walkFiles(dir)
    .filter((f) => hasExt(f, extensions))
    .reduce((max, f) => Math.max(max, sizeOf(f)), 0);
}

function countFiles(dir, extensions) {
  return walkFiles(dir).filter((f) => hasExt(f, extensions)).length;
}

/** Run a build step, streaming its output. */
function defaultRunCommand(cmd, args, options = {}) {
  execFileSync(cmd, args, { cwd: ROOT, stdio: "inherit", ...options });
}

export function parseArgs(argv) {
  const args = argv.slice(2);
  const outIndex = args.indexOf("--out-dir");
  return {
    report: args.includes("--report"),
    skipFrontend: args.includes("--skip-frontend"),
    skipRust: args.includes("--skip-rust"),
    skipBench: args.includes("--skip-bench"),
    outDir:
      outIndex >= 0 && args[outIndex + 1]
        ? resolve(args[outIndex + 1])
        : join(ROOT, "perf-metrics"),
  };
}

// ── Criterion result parser ───────────────────────────────────────────────────

/**
 * Criterion results written at or after `sinceMs`. Criterion keeps every
 * result it ever produced under `target/criterion` (including benchmarks that
 * no longer exist) plus a `base/` copy of the previous run, so only the
 * `new/` estimates this run wrote are current.
 */
export function freshCriterionEstimates(criterionDir, sinceMs) {
  const benchmarks = {};
  const stale = [];
  for (const file of walkFiles(criterionDir)) {
    const rel = relative(criterionDir, file).split(/[\\/]/);
    if (rel.length < 3 || rel.at(-1) !== "estimates.json" || rel.at(-2) !== "new") continue;
    const key = rel.slice(0, -2).join("_");
    if (statSync(file).mtimeMs < sinceMs) {
      stale.push(key);
      continue;
    }
    try {
      const data = JSON.parse(readFileSync(file, "utf8"));
      if (data.mean) {
        benchmarks[key] = {
          meanNs: Math.round(data.mean.point_estimate),
          stddevNs: Math.round(data.std_dev?.point_estimate ?? 0),
        };
      }
    } catch {
      // skip unparseable files
    }
  }
  return { benchmarks, stale: stale.sort() };
}

// ── Collection ────────────────────────────────────────────────────────────────

export function archiveName(entry) {
  const commit = entry.gitCommit ?? "unknown";
  return `metrics_${commit}${entry.dirty ? "-dirty" : ""}.json`;
}

/**
 * Build fresh artifacts, measure them, and return the snapshot. Throws when a
 * build fails or does not produce its artifact.
 */
export function collectMetrics({
  root = ROOT,
  options,
  exec = defaultExec,
  runCommand = defaultRunCommand,
  targetDir = cargoTargetDir(),
  now = () => Date.now(),
  log = console.log,
}) {
  const distDir = join(root, "dist");
  const tauriDir = join(root, "src-tauri");
  const binaryPath = join(targetDir, RUST_PROFILE, "keynobi");
  const skipped = [];

  let frontend = null;
  if (options.skipFrontend) {
    skipped.push("frontend");
  } else {
    log("  [1/3] Frontend bundle (npm run build)...");
    runCommand("npm", ["run", "build"], { cwd: root });
    if (!existsSync(distDir)) throw new Error("npm run build did not produce dist/");
    frontend = {
      bundleSizeBytes: dirSize(distDir, [".js", ".mjs"]),
      cssSizeBytes: dirSize(distDir, [".css"]),
      chunkCount: countFiles(distDir, [".js", ".mjs"]),
      largestChunkBytes: largestFile(distDir, [".js", ".mjs"]),
      totalDistBytes: dirSize(distDir),
    };
  }

  let binarySizeBytes = null;
  if (options.skipRust) {
    skipped.push("rust-binary");
  } else {
    log(`  [2/3] Rust binary (cargo build --${RUST_PROFILE})...`);
    runCommand("cargo", ["build", `--${RUST_PROFILE}`, "--bin", "keynobi"], { cwd: tauriDir });
    if (!existsSync(binaryPath)) throw new Error(`cargo build did not produce ${binaryPath}`);
    binarySizeBytes = statSync(binaryPath).size;
  }

  let benchmarks = {};
  let staleBenchmarks = [];
  if (options.skipBench) {
    skipped.push("benchmarks");
  } else {
    log("  [3/3] Criterion benchmarks (cargo bench)...");
    const benchStarted = now();
    runCommand("cargo", ["bench", "--benches"], { cwd: tauriDir });
    ({ benchmarks, stale: staleBenchmarks } = freshCriterionEstimates(
      join(targetDir, "criterion"),
      benchStarted
    ));
  }

  const provenance = collectProvenance({
    exec,
    profile: options.skipRust && options.skipBench ? null : RUST_PROFILE,
    artifacts: {
      frontendDist: frontend ? distDir : null,
      rustBinary: binarySizeBytes == null ? null : binaryPath,
    },
  });

  return {
    timestamp: new Date(now()).toISOString(),
    version: getVersion(root),
    gitCommit: provenance.gitCommitShort,
    dirty: provenance.dirty,
    provenance,
    skipped,
    frontend,
    rust: {
      binarySizeBytes,
      benchmarks,
      // Results left in target/criterion by earlier runs; not reported.
      staleBenchmarks,
    },
  };
}

// ── Archive management ────────────────────────────────────────────────────────

/**
 * If metrics_latest.json exists, rename it to its archive name so it is
 * preserved as a historical record.
 */
function archiveLatest(metricsDir) {
  const latestFile = join(metricsDir, "metrics_latest.json");
  if (!existsSync(latestFile)) return;

  try {
    const data = JSON.parse(readFileSync(latestFile, "utf8"));
    const name = archiveName(data);
    const archivePath = join(metricsDir, name);

    // If an archive with the same name already exists, skip the rename
    // (same commit was collected twice — keep the first one).
    if (!existsSync(archivePath)) {
      renameSync(latestFile, archivePath);
      console.log(`  Archived previous metrics as ${name}`);
    }
  } catch {
    // If the latest file is malformed, just overwrite it.
  }
}

/**
 * Load the most recent archived metrics for comparison in --report mode.
 * Returns the data object or null if no archive exists.
 */
function loadPreviousMetrics(metricsDir) {
  if (!existsSync(metricsDir)) return null;

  const files = readdirSync(metricsDir).filter(
    (f) => f.startsWith("metrics_") && f !== "metrics_latest.json" && f.endsWith(".json")
  );

  let mostRecent = null;
  let mostRecentTime = "";

  for (const file of files) {
    try {
      const data = JSON.parse(readFileSync(join(metricsDir, file), "utf8"));
      if (data.timestamp && data.timestamp > mostRecentTime) {
        mostRecentTime = data.timestamp;
        mostRecent = data;
      }
    } catch {
      // skip malformed files
    }
  }

  return mostRecent;
}

/** Count the total number of metrics files in the folder. */
function countMetricsFiles(metricsDir) {
  if (!existsSync(metricsDir)) return 0;
  return readdirSync(metricsDir).filter((f) => f.endsWith(".json")).length;
}

// ── Report mode ───────────────────────────────────────────────────────────────

/** Repo-relative path when inside the repo, absolute otherwise. */
function displayPath(path) {
  const rel = relative(ROOT, path);
  return rel.startsWith("..") ? path : rel || ".";
}

function describe(entry) {
  const p = entry.provenance;
  if (!p) return `${entry.timestamp} (${entry.gitCommit}, no provenance recorded)`;
  const dirty = p.dirty ? ", DIRTY tree" : "";
  return `${entry.timestamp} (${p.gitCommitShort}${dirty}, ${p.profile ?? "?"}, ${p.arch}, ${p.hardware?.model})`;
}

/** Reasons two snapshots are not directly comparable. */
export function comparabilityWarnings(latest, previous) {
  const warnings = [];
  for (const [label, entry] of [
    ["latest", latest],
    ["previous", previous],
  ]) {
    if (!entry) continue;
    if (!entry.provenance) warnings.push(`${label} snapshot has no provenance`);
    else if (entry.provenance.dirty) warnings.push(`${label} snapshot was taken on a dirty tree`);
  }
  const a = latest?.provenance;
  const b = previous?.provenance;
  if (a && b) {
    if (a.arch !== b.arch) warnings.push(`arch differs (${b.arch} → ${a.arch})`);
    if (a.profile !== b.profile) warnings.push(`profile differs (${b.profile} → ${a.profile})`);
    if (a.hardware?.model !== b.hardware?.model) {
      warnings.push(`hardware differs (${b.hardware?.model} → ${a.hardware?.model})`);
    }
    if (a.toolchain?.rustc !== b.toolchain?.rustc) warnings.push("rustc version differs");
  }
  return warnings;
}

function printReport(metricsDir) {
  const latestFile = join(metricsDir, "metrics_latest.json");
  if (!existsSync(latestFile)) {
    console.log("No metrics_latest.json found. Run `npm run perf:collect` first.");
    process.exit(1);
  }

  const latest = JSON.parse(readFileSync(latestFile, "utf8"));
  const previous = loadPreviousMetrics(metricsDir);

  console.log("\n=== Performance Metrics Report ===\n");
  console.log(`Latest:   ${describe(latest)}`);
  if (previous) {
    console.log(`Previous: ${describe(previous)}`);
  }
  console.log(`Metrics:  ${displayPath(metricsDir)}/ (${countMetricsFiles(metricsDir)} files)`);
  for (const warning of comparabilityWarnings(latest, previous)) {
    console.log(`\x1b[33mWarning:  ${warning}\x1b[0m`);
  }
  console.log("");

  // Frontend
  console.log("── Frontend Bundle ──");
  printMetric(
    "JS bundle",
    latest.frontend?.bundleSizeBytes,
    previous?.frontend?.bundleSizeBytes,
    "bytes"
  );
  printMetric("CSS", latest.frontend?.cssSizeBytes, previous?.frontend?.cssSizeBytes, "bytes");
  printMetric("Chunks", latest.frontend?.chunkCount, previous?.frontend?.chunkCount, "");
  printMetric(
    "Largest chunk",
    latest.frontend?.largestChunkBytes,
    previous?.frontend?.largestChunkBytes,
    "bytes"
  );
  console.log("");

  // Rust
  console.log("── Rust Binary ──");
  printMetric(
    "Binary size",
    latest.rust?.binarySizeBytes,
    previous?.rust?.binarySizeBytes,
    "bytes"
  );
  console.log("");

  // Benchmarks
  const benchKeys = new Set([
    ...Object.keys(latest.rust?.benchmarks ?? {}),
    ...Object.keys(previous?.rust?.benchmarks ?? {}),
  ]);

  if (benchKeys.size > 0) {
    console.log("── Criterion Benchmarks ──");
    for (const key of [...benchKeys].sort()) {
      const curr = latest.rust?.benchmarks?.[key]?.meanNs;
      const prev = previous?.rust?.benchmarks?.[key]?.meanNs;
      printMetric(key, curr, prev, "ns");
    }
  }

  console.log("");
}

function printMetric(label, current, previous, unit) {
  const fmt = (v) => {
    if (v == null) return "N/A";
    if (unit === "bytes") return `${(v / 1024).toFixed(1)} KB`;
    if (unit === "ns") return `${(v / 1000).toFixed(1)} µs`;
    return String(v);
  };

  let delta = "";
  if (current != null && previous != null && previous !== 0) {
    const pct = ((current - previous) / previous) * 100;
    const sign = pct > 0 ? "+" : "";
    const color = pct > 5 ? "\x1b[31m" : pct < -5 ? "\x1b[32m" : "\x1b[90m";
    delta = `  ${color}${sign}${pct.toFixed(1)}%\x1b[0m`;
  }

  console.log(`  ${label.padEnd(28)} ${fmt(current).padStart(12)}${delta}`);
}

// ── Main ──────────────────────────────────────────────────────────────────────

function main() {
  const options = parseArgs(process.argv);
  if (options.report) {
    printReport(options.outDir);
    return;
  }

  console.log("Collecting performance metrics...\n");
  const entry = collectMetrics({ options });

  mkdirSync(options.outDir, { recursive: true });
  archiveLatest(options.outDir);
  writeFileSync(join(options.outDir, "metrics_latest.json"), JSON.stringify(entry, null, 2) + "\n");

  const where = displayPath(options.outDir);
  console.log(
    `\nMetrics saved to ${where}/metrics_latest.json (${countMetricsFiles(options.outDir)} total snapshots)`
  );
  console.log(
    `  Commit:     ${entry.provenance.gitCommitShort}${entry.dirty ? " (dirty tree)" : ""}`
  );
  if (entry.frontend) {
    console.log(`  JS bundle:  ${(entry.frontend.bundleSizeBytes / 1024).toFixed(1)} KB`);
    console.log(`  CSS:        ${(entry.frontend.cssSizeBytes / 1024).toFixed(1)} KB`);
    console.log(`  Chunks:     ${entry.frontend.chunkCount}`);
  }
  if (entry.rust.binarySizeBytes) {
    console.log(`  Rust bin:   ${(entry.rust.binarySizeBytes / 1024 / 1024).toFixed(1)} MB`);
  }
  console.log(`  Benchmarks: ${Object.keys(entry.rust.benchmarks).length} results captured`);
  if (entry.rust.staleBenchmarks.length > 0) {
    console.log(`  Ignored stale Criterion results: ${entry.rust.staleBenchmarks.join(", ")}`);
  }
  if (entry.skipped.length > 0) console.log(`  Skipped:    ${entry.skipped.join(", ")}`);
  console.log("");
}

if (process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  main();
}
