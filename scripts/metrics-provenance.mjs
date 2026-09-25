/**
 * Provenance for performance measurements: what was measured, built how, on
 * what machine. Shared by `collect-metrics.mjs` and `logcat-soak.mjs` so
 * every recorded number can be traced to a commit, a clean or dirty tree, a
 * build profile, a toolchain, and the exact artifact.
 */

import { execFileSync } from "child_process";
import { createHash } from "crypto";
import { existsSync, readFileSync, readdirSync, statSync } from "fs";
import os from "os";
import { join, relative, resolve } from "path";

export const ROOT = resolve(import.meta.dirname, "..");

/** Run a command without a shell and return its trimmed stdout. */
export function defaultExec(cmd, args, options = {}) {
  return execFileSync(cmd, args, {
    cwd: ROOT,
    encoding: "utf8",
    stdio: ["ignore", "pipe", "pipe"],
    ...options,
  })
    .toString()
    .trim();
}

function tryExec(exec, cmd, args, options) {
  try {
    return exec(cmd, args, options);
  } catch {
    return null;
  }
}

/** Cargo's target directory, honouring `CARGO_TARGET_DIR`. */
export function cargoTargetDir(env = process.env) {
  return env.CARGO_TARGET_DIR ? resolve(env.CARGO_TARGET_DIR) : join(ROOT, "src-tauri", "target");
}

/** SHA-256 of a file, or of every file under a directory (path + content). */
export function hashArtifact(path) {
  const hash = createHash("sha256");
  const stat = statSync(path);
  if (!stat.isDirectory()) {
    hash.update(readFileSync(path));
    return hash.digest("hex");
  }
  const files = readdirSync(path, { withFileTypes: true, recursive: true })
    .filter((entry) => entry.isFile())
    .map((entry) => join(entry.parentPath ?? entry.path, entry.name))
    .sort();
  for (const file of files) {
    hash.update(relative(path, file));
    hash.update("\0");
    hash.update(readFileSync(file));
    hash.update("\0");
  }
  return hash.digest("hex");
}

function hardwareModel(exec) {
  if (process.platform === "darwin") {
    return tryExec(exec, "sysctl", ["-n", "hw.model"]) ?? "unknown";
  }
  const dmi = "/sys/devices/virtual/dmi/id/product_name";
  if (existsSync(dmi)) {
    try {
      return readFileSync(dmi, "utf8").trim();
    } catch {
      // fall through
    }
  }
  return "unknown";
}

/**
 * Describe the tree, toolchain, and machine.
 *
 * `profile` is the Cargo profile the Rust artifacts were built with.
 * `artifacts` maps a name to a path; each is hashed so two runs can be
 * compared only when they measured the same bytes.
 */
export function collectProvenance({ exec = defaultExec, profile, artifacts = {} } = {}) {
  const commit = tryExec(exec, "git", ["rev-parse", "HEAD"]) ?? "unknown";
  const status = tryExec(exec, "git", ["status", "--porcelain", "--untracked-files=normal"]);
  const changed = status == null ? null : status.split("\n").filter((line) => line.trim() !== "");
  const cpus = os.cpus();
  const hashed = {};
  for (const [name, path] of Object.entries(artifacts)) {
    hashed[name] =
      path && existsSync(path) ? { path: relative(ROOT, path), sha256: hashArtifact(path) } : null;
  }
  return {
    gitCommit: commit,
    gitCommitShort: commit === "unknown" ? commit : commit.slice(0, 7),
    // Unknown counts as dirty: a number that cannot be tied to a commit is not clean.
    dirty: changed == null ? true : changed.length > 0,
    changedFiles: changed?.length ?? null,
    profile: profile ?? null,
    platform: process.platform,
    arch: process.arch,
    osRelease: os.release(),
    toolchain: {
      rustc: tryExec(exec, "rustc", ["-V"], { cwd: join(ROOT, "src-tauri") }),
      cargo: tryExec(exec, "cargo", ["-V"], { cwd: join(ROOT, "src-tauri") }),
      node: process.version,
    },
    hardware: {
      model: hardwareModel(exec),
      cpu: cpus[0]?.model ?? "unknown",
      cpuCount: cpus.length,
      memoryBytes: os.totalmem(),
    },
    artifacts: hashed,
  };
}
