//! Detects Google's Android CLI (`android`), the stateless command-line
//! toolchain agents use for SDK packages, emulators, screenshots, and docs.
//!
//! Keynobi does not run it for anything else. Health reports it for
//! information only: it is optional and never fails a health check.

use crate::utils::cli_lookup::CliSearch;
use crate::utils::process::{output_with_timeout, TOOL_PROBE_TIMEOUT};
use serde_json::{json, Value};
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::Duration;

/// Android CLI's documentation, linked from Health.
pub const DOCS_URL: &str = "https://developer.android.com/tools/agents/android-cli";

/// Longest version line kept from `android --version`.
const MAX_VERSION_CHARS: usize = 200;

/// Where Android CLI is installed and which version it reports.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct AndroidCli {
    /// The canonical path of the `android` binary, or `None` when it is not
    /// installed.
    pub path: Option<PathBuf>,
    /// The first line `android --version` printed, or `None` when it failed
    /// or did not answer in time.
    pub version: Option<String>,
}

/// Look for `android` on the user's `PATH` (as a login shell sees it) and in
/// the documented install locations (`~/.local/bin`, `/usr/local/bin`, and
/// Homebrew's `/opt/homebrew/bin`), then read its version.
pub async fn detect() -> AndroidCli {
    detect_with(
        &CliSearch::system("android", Vec::new()),
        TOOL_PROBE_TIMEOUT,
    )
    .await
}

/// [`detect`] with the search and the version deadline given.
pub async fn detect_with(search: &CliSearch, version_timeout: Duration) -> AndroidCli {
    let Some(found) = search.find().await else {
        return AndroidCli::default();
    };
    // A dangling symlink is not an installed tool.
    let Ok(path) = found.canonicalize() else {
        return AndroidCli::default();
    };
    let version = read_version(&path, version_timeout).await;
    AndroidCli {
        path: Some(path),
        version,
    }
}

/// `android --no-metrics --version`: the first non-empty line it prints.
async fn read_version(path: &Path, timeout: Duration) -> Option<String> {
    let out = output_with_timeout(
        tokio::process::Command::new(path)
            .args(["--no-metrics", "--version"])
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped()),
        timeout,
    )
    .await
    .ok()?;
    if !out.status.success() {
        return None;
    }
    let text = [out.stdout, out.stderr]
        .iter()
        .map(|bytes| String::from_utf8_lossy(bytes).into_owned())
        .find_map(|s| {
            s.lines()
                .map(str::trim)
                .find(|l| !l.is_empty())
                .map(str::to_string)
        })?;
    Some(text.chars().take(MAX_VERSION_CHARS).collect())
}

/// The `checks.android_cli` object of MCP `run_health_check`.
pub fn health_json(cli: &AndroidCli) -> Value {
    let installed = cli.path.is_some();
    json!({
        "installed": installed,
        "path": cli.path.as_ref().map(|p| p.to_string_lossy().into_owned()),
        "version": cli.version,
        "docs": DOCS_URL,
        "hint": if installed {
            Value::Null
        } else {
            json!(format!(
                "Android CLI (`android`) is not installed. It is optional; Keynobi works without it. \
                 It covers stateless tasks such as SDK packages, creating and starting emulators, \
                 and docs. See {DOCS_URL}"
            ))
        },
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::utils::process::test_support::run_once;
    use std::os::unix::fs::PermissionsExt;

    /// A fake `android` that answers `--version` with `body` and exits at once
    /// when run with no arguments.
    fn fake_android(path: &Path, body: &str) {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, format!("#!/bin/sh\n[ $# -eq 0 ] && exit 0\n{body}\n")).unwrap();
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o755)).unwrap();
        run_once(path);
    }

    fn search_path(dirs: &[&Path]) -> CliSearch {
        CliSearch {
            name: "android".into(),
            path_var: dirs
                .iter()
                .map(|d| d.display().to_string())
                .collect::<Vec<_>>()
                .join(":"),
            candidates: Vec::new(),
            login_shell: None,
            shell_timeout: Duration::from_secs(30),
        }
    }

    #[tokio::test]
    async fn finds_android_on_path_and_reads_its_version() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().canonicalize().unwrap();
        let bin = root.join("bin");
        let args = root.join("args.txt");
        fake_android(
            &bin.join("android"),
            &format!(
                "printf '%s\\n' \"$@\" > '{}'\necho\necho 'Android CLI 1.0.16406183'",
                args.display()
            ),
        );

        let cli = detect_with(&search_path(&[&bin]), Duration::from_secs(30)).await;

        assert_eq!(cli.path, Some(bin.join("android")));
        assert_eq!(cli.version.as_deref(), Some("Android CLI 1.0.16406183"));
        assert_eq!(
            std::fs::read_to_string(args).unwrap(),
            "--no-metrics\n--version\n"
        );
    }

    #[tokio::test]
    async fn missing_android_is_not_installed() {
        let dir = tempfile::tempdir().unwrap();
        let empty = dir.path().join("empty");
        std::fs::create_dir_all(&empty).unwrap();

        let cli = detect_with(&search_path(&[&empty]), Duration::from_secs(30)).await;

        assert_eq!(cli, AndroidCli::default());
        let json = health_json(&cli);
        assert_eq!(json["installed"], false);
        assert_eq!(json["path"], Value::Null);
        assert!(json["hint"].as_str().unwrap().contains(DOCS_URL));
    }

    #[tokio::test]
    async fn a_version_command_that_hangs_is_cut_off_at_the_deadline() {
        let dir = tempfile::tempdir().unwrap();
        let bin = dir.path().canonicalize().unwrap().join("bin");
        fake_android(&bin.join("android"), "exec sleep 30");

        let start = std::time::Instant::now();
        let cli = detect_with(&search_path(&[&bin]), Duration::from_millis(300)).await;

        assert!(
            start.elapsed() < Duration::from_secs(10),
            "{:?}",
            start.elapsed()
        );
        assert_eq!(cli.path, Some(bin.join("android")));
        assert_eq!(cli.version, None);
    }

    #[tokio::test]
    async fn a_failing_version_command_reports_no_version() {
        let dir = tempfile::tempdir().unwrap();
        let bin = dir.path().canonicalize().unwrap().join("bin");
        fake_android(&bin.join("android"), "echo 'unknown option' >&2\nexit 2");

        let cli = detect_with(&search_path(&[&bin]), Duration::from_secs(30)).await;

        assert!(cli.path.is_some());
        assert_eq!(cli.version, None);
    }

    #[tokio::test]
    async fn a_symlinked_android_is_reported_at_its_real_path() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().canonicalize().unwrap();
        let real = root
            .join("Cellar")
            .join("android-cli")
            .join("bin")
            .join("android");
        fake_android(&real, "echo 1.0");
        let links = root.join("links");
        std::fs::create_dir_all(&links).unwrap();
        std::os::unix::fs::symlink(&real, links.join("android")).unwrap();

        let cli = detect_with(&search_path(&[&links]), Duration::from_secs(30)).await;

        assert_eq!(cli.path, Some(real));
        assert_eq!(cli.version.as_deref(), Some("1.0"));
    }

    #[tokio::test]
    async fn a_dangling_symlink_is_not_installed() {
        let dir = tempfile::tempdir().unwrap();
        let links = dir.path().join("links");
        std::fs::create_dir_all(&links).unwrap();
        std::os::unix::fs::symlink(dir.path().join("gone"), links.join("android")).unwrap();

        let cli = detect_with(&search_path(&[&links]), Duration::from_secs(30)).await;

        assert_eq!(cli, AndroidCli::default());
    }

    #[test]
    fn health_json_reports_an_installed_cli() {
        let json = health_json(&AndroidCli {
            path: Some("/opt/homebrew/Cellar/android-cli/1.0/bin/android".into()),
            version: Some("1.0.16406183".into()),
        });
        assert_eq!(json["installed"], true);
        assert_eq!(json["version"], "1.0.16406183");
        assert_eq!(json["hint"], Value::Null);
        assert_eq!(json["docs"], DOCS_URL);
    }
}
