#!/usr/bin/env node
/**
 * Performance metrics collector.
 *
 * Collects frontend bundle sizes, Rust binary size, and Criterion benchmark
 * results into individual JSON files inside `perf-metrics/` for regression
 * tracking across builds.
 *
 * File naming:
 *   perf-metrics/metrics_latest.json    — always the most recent run
 *   perf-metrics/metrics_{commit}.json  — archived runs keyed by git commit
 *
 * On each collection:
 *   1. If metrics_latest.json exists, rename it to metrics_{its gitCommit}.json
 *   2. Write the new metrics as metrics_latest.json
 *
 * Usage:
 *   node scripts/collect-metrics.mjs          # collect metrics
 *   node scripts/collect-metrics.mjs --report  # compare latest vs previous
 */

import { execSync } from "child_process";
import {
  existsSync,
  readFileSync,
  writeFileSync,
  renameSync,
  mkdirSync,
  statSync,
  readdirSync,
} from "fs";
import { join, resolve } from "path";

const ROOT = resolve(import.meta.dirname, "..");
const METRICS_DIR = join(ROOT, "perf-metrics");
const LATEST_FILE = join(METRICS_DIR, "metrics_latest.json");
const DIST_DIR = join(ROOT, "dist");
const RUST_TARGET = join(ROOT, "src-tauri", "target");
const CRITERION_DIR = join(RUST_TARGET, "criterion");

// ── Helpers ───────────────────────────────────────────────────────────────────

function getGitCommit() {
  try {
    return execSync("git rev-parse --short HEAD", { cwd: ROOT, encoding: "utf8" }).trim();
  } catch {
    return "unknown";
  }
}

function getVersion() {
  try {
    const pkg = JSON.parse(readFileSync(join(ROOT, "package.json"), "utf8"));
    return pkg.version ?? "0.0.0";
  } catch {
    return "0.0.0";
  }
}

function dirSize(dir, extensions) {
  if (!existsSync(dir)) return 0;
  let total = 0;
  for (const entry of readdirSync(dir, { withFileTypes: true, recursive: true })) {
    if (entry.isFile()) {
      const fullPath = join(entry.parentPath ?? entry.path, entry.name);
      if (extensions && !extensions.some((ext) => entry.name.endsWith(ext))) continue;
      try {
        total += statSync(fullPath).size;
      } catch {
        // skip inaccessible files
      }
    }
  }
  return total;
}

function findFiles(dir, pattern) {
  if (!existsSync(dir)) return [];
  const results = [];
  for (const entry of readdirSync(dir, { withFileTypes: true, recursive: true })) {
    if (entry.isFile() && entry.name.match(pattern)) {
      results.push(join(entry.parentPath ?? entry.path, entry.name));
    }
  }
  return results;
}

function largestFile(dir, extensions) {
  if (!existsSync(dir)) return 0;
  let max = 0;
  for (const entry of readdirSync(dir, { withFileTypes: true, recursive: true })) {
    if (entry.isFile() && extensions.some((ext) => entry.name.endsWith(ext))) {
      try {
        const size = statSync(join(entry.parentPath ?? entry.path, entry.name)).size;
        if (size > max) max = size;
      } catch {
        // skip
      }
    }
  }
  return max;
}

function countFiles(dir, extensions) {
  if (!existsSync(dir)) return 0;
  let count = 0;
  for (const entry of readdirSync(dir, { withFileTypes: true, recursive: true })) {
    if (entry.isFile() && extensions.some((ext) => entry.name.endsWith(ext))) {
      count++;
    }
  }
  return count;
}

// ── Criterion result parser ───────────────────────────────────────────────────

function collectCriterionBenchmarks() {
  const benchmarks = {};
  if (!existsSync(CRITERION_DIR)) return benchmarks;

  const estimateFiles = findFiles(CRITERION_DIR, /estimates\.json$/);
  for (const file of estimateFiles) {
    try {
      const data = JSON.parse(readFileSync(file, "utf8"));
      const parts = file.replace(CRITERION_DIR + "/", "").split("/");
      const group = parts[0];
      const id = parts[1]?.replace(/\//g, "_") ?? "default";
      const key = `${group}_${id}`;

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
  return benchmarks;
}

// ── Rust binary size ──────────────────────────────────────────────────────────

function getRustBinarySize() {
  const candidates = [
    join(RUST_TARGET, "release", "android-ide"),
    join(RUST_TARGET, "debug", "android-ide"),
    join(RUST_TARGET, "release", "bundle", "macos", "Android IDE.app"),
  ];
  for (const path of candidates) {
    if (existsSync(path)) {
      const stat = statSync(path);
      return stat.isDirectory() ? dirSize(path) : stat.size;
    }
  }
  return null;
}

// ── Frontend bundle analysis ──────────────────────────────────────────────────

function collectFrontendMetrics() {
  if (!existsSync(DIST_DIR)) {
    console.log("  Building frontend (npm run build)...");
    execSync("npm run build", { cwd: ROOT, stdio: "inherit" });
  }

  return {
    bundleSizeBytes: dirSize(DIST_DIR, [".js", ".mjs"]),
    cssSizeBytes: dirSize(DIST_DIR, [".css"]),
    chunkCount: countFiles(DIST_DIR, [".js", ".mjs"]),
    largestChunkBytes: largestFile(DIST_DIR, [".js", ".mjs"]),
    totalDistBytes: dirSize(DIST_DIR),
  };
}

// ── Archive management ────────────────────────────────────────────────────────

/**
 * If metrics_latest.json exists, rename it to metrics_{gitCommit}.json
 * so it is preserved as a historical record.
 */
function archiveLatest() {
  if (!existsSync(LATEST_FILE)) return;

  try {
    const data = JSON.parse(readFileSync(LATEST_FILE, "utf8"));
    const commit = data.gitCommit ?? "unknown";
    const archiveName = `metrics_${commit}.json`;
    const archivePath = join(METRICS_DIR, archiveName);

    // If an archive with the same commit already exists, skip the rename
    // (same commit was collected twice — keep the first one).
    if (!existsSync(archivePath)) {
      renameSync(LATEST_FILE, archivePath);
      console.log(`  Archived previous metrics as ${archiveName}`);
    }
  } catch {
    // If the latest file is malformed, just overwrite it.
  }
}

/**
 * Load the most recent archived metrics for comparison in --report mode.
 * Returns the data object or null if no archive exists.
 */
function loadPreviousMetrics() {
  if (!existsSync(METRICS_DIR)) return null;

  const files = readdirSync(METRICS_DIR)
    .filter((f) => f.startsWith("metrics_") && f !== "metrics_latest.json" && f.endsWith(".json"))
    .sort();

  if (files.length === 0) return null;

  // Most recent archived file (sorted alphabetically by commit hash).
  // For a more reliable ordering, compare timestamps inside the files.
  let mostRecent = null;
  let mostRecentTime = "";

  for (const file of files) {
    try {
      const data = JSON.parse(readFileSync(join(METRICS_DIR, file), "utf8"));
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
function countMetricsFiles() {
  if (!existsSync(METRICS_DIR)) return 0;
  return readdirSync(METRICS_DIR).filter((f) => f.endsWith(".json")).length;
}

// ── Report mode ───────────────────────────────────────────────────────────────

function printReport() {
  if (!existsSync(LATEST_FILE)) {
    console.log("No metrics_latest.json found. Run `npm run perf:collect` first.");
    process.exit(1);
  }

  const latest = JSON.parse(readFileSync(LATEST_FILE, "utf8"));
  const previous = loadPreviousMetrics();

  console.log("\n=== Performance Metrics Report ===\n");
  console.log(`Latest:   ${latest.timestamp} (${latest.gitCommit})`);
  if (previous) {
    console.log(`Previous: ${previous.timestamp} (${previous.gitCommit})`);
  }
  console.log(`Metrics:  perf-metrics/ (${countMetricsFiles()} files)`);
  console.log("");

  // Frontend
  console.log("── Frontend Bundle ──");
  printMetric("JS bundle", latest.frontend?.bundleSizeBytes, previous?.frontend?.bundleSizeBytes, "bytes");
  printMetric("CSS", latest.frontend?.cssSizeBytes, previous?.frontend?.cssSizeBytes, "bytes");
  printMetric("Chunks", latest.frontend?.chunkCount, previous?.frontend?.chunkCount, "");
  printMetric("Largest chunk", latest.frontend?.largestChunkBytes, previous?.frontend?.largestChunkBytes, "bytes");
  console.log("");

  // Rust
  console.log("── Rust Binary ──");
  printMetric("Binary size", latest.rust?.binarySizeBytes, previous?.rust?.binarySizeBytes, "bytes");
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

if (process.argv.includes("--report")) {
  printReport();
  process.exit(0);
}

console.log("Collecting performance metrics...\n");

// Ensure the metrics directory exists
mkdirSync(METRICS_DIR, { recursive: true });

// 1. Frontend
console.log("  [1/3] Frontend bundle...");
const frontend = collectFrontendMetrics();

// 2. Rust binary
console.log("  [2/3] Rust binary...");
const binarySizeBytes = getRustBinarySize();

// 3. Criterion benchmarks (if available)
console.log("  [3/3] Criterion benchmarks...");
const benchmarks = collectCriterionBenchmarks();

const entry = {
  timestamp: new Date().toISOString(),
  version: getVersion(),
  gitCommit: getGitCommit(),
  frontend,
  rust: {
    binarySizeBytes,
    benchmarks,
  },
};

// Archive the current latest (if any) before writing the new one.
archiveLatest();

// Write the new metrics as _latest
writeFileSync(LATEST_FILE, JSON.stringify(entry, null, 2) + "\n");

const totalFiles = countMetricsFiles();
console.log(`\nMetrics saved to perf-metrics/metrics_latest.json (${totalFiles} total snapshots)`);
console.log(`  JS bundle:  ${(frontend.bundleSizeBytes / 1024).toFixed(1)} KB`);
console.log(`  CSS:        ${(frontend.cssSizeBytes / 1024).toFixed(1)} KB`);
console.log(`  Chunks:     ${frontend.chunkCount}`);
if (binarySizeBytes) {
  console.log(`  Rust bin:   ${(binarySizeBytes / 1024 / 1024).toFixed(1)} MB`);
}
const benchCount = Object.keys(benchmarks).length;
if (benchCount > 0) {
  console.log(`  Benchmarks: ${benchCount} results captured`);
} else {
  console.log('  Benchmarks: none (run "cd src-tauri && cargo bench" first)');
}
console.log("");
