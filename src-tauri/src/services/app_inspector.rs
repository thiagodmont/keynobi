use crate::services::adb_manager::am_start_failure;
use crate::utils::device_shell::quote_device_shell_arg;
use crate::utils::process::{
    describe_failure, output_with_timeout, ADB_QUERY_TIMEOUT, ADB_UNRESPONSIVE_HINT,
};
use std::path::PathBuf;

#[derive(Debug, serde::Serialize)]
pub struct ProcessInfo {
    pub pid: i32,
    pub name: String,
    pub thread_count: Option<u32>,
    pub rss_kb: Option<u64>,
}

#[derive(Debug, serde::Serialize)]
pub struct AppRuntimeState {
    pub package: String,
    pub running: bool,
    pub processes: Vec<ProcessInfo>,
    pub total_threads: u32,
    pub total_rss_kb: u64,
}

#[derive(Debug, serde::Serialize)]
pub struct RestartResult {
    pub launched: bool,
    pub activity: Option<String>,
    pub display_time_ms: Option<u64>,
    /// True when `pm clear` wiped the app's data before the relaunch.
    pub data_cleared: bool,
}

pub async fn get_runtime_state(
    adb: &PathBuf,
    device_serial: Option<&str>,
    package: &str,
) -> AppRuntimeState {
    let pids = find_pids_for_package(adb, device_serial, package).await;

    if pids.is_empty() {
        return AppRuntimeState {
            package: package.to_string(),
            running: false,
            processes: Vec::new(),
            total_threads: 0,
            total_rss_kb: 0,
        };
    }

    let processes: Vec<ProcessInfo> =
        futures_util::future::join_all(pids.iter().map(|(pid, name)| {
            let pid = *pid;
            let name = name.clone();
            async move {
                let (threads, rss) = tokio::join!(
                    get_thread_count(adb, device_serial, pid),
                    get_rss_kb(adb, device_serial, pid)
                );
                ProcessInfo {
                    pid,
                    name,
                    thread_count: threads,
                    rss_kb: rss,
                }
            }
        }))
        .await;

    let total_threads = processes.iter().filter_map(|p| p.thread_count).sum();
    let total_rss_kb = processes.iter().filter_map(|p| p.rss_kb).sum();

    AppRuntimeState {
        package: package.to_string(),
        running: true,
        processes,
        total_threads,
        total_rss_kb,
    }
}

/// Restart an app on a device. Returns launch result including display time.
///
/// The app is always force-stopped, so the relaunch is a process cold start.
/// Its data is wiped only when `clear_data` is true.
pub async fn restart_app(
    adb: &PathBuf,
    device_serial: &str,
    package: &str,
    clear_data: bool,
) -> Result<RestartResult, String> {
    if clear_data {
        adb_cmd(adb, Some(device_serial), &["shell", "pm", "clear", package]).await?;
    } else {
        adb_cmd(
            adb,
            Some(device_serial),
            &["shell", "am", "force-stop", package],
        )
        .await?;
    }

    let activity = resolve_launcher_activity(adb, device_serial, package).await?;

    let start = std::time::Instant::now();
    let output = adb_cmd(
        adb,
        Some(device_serial),
        &["shell", "am", "start", "-n", &activity],
    )
    .await?;
    if let Some(failure) = am_start_failure(&output) {
        return Err(format!("am start {activity} failed: {failure}"));
    }

    let display_time_ms = wait_for_displayed(adb, device_serial, package, start).await;

    Ok(RestartResult {
        launched: true,
        activity: Some(activity),
        display_time_ms,
        data_cleared: clear_data,
    })
}

/// Returns Vec of (pid, process_name) for all processes matching the package.
async fn find_pids_for_package(
    adb: &PathBuf,
    device_serial: Option<&str>,
    package: &str,
) -> Vec<(i32, String)> {
    let output = adb_cmd(adb, device_serial, &["shell", "ps", "-A", "-o", "PID,NAME"])
        .await
        .unwrap_or_default();
    parse_ps_for_package(&output, package)
}

fn parse_ps_for_package(output: &str, package: &str) -> Vec<(i32, String)> {
    output
        .lines()
        .skip(1) // header
        .filter_map(|line| {
            let mut parts = line.split_whitespace();
            let pid = parts.next()?.parse::<i32>().ok()?;
            let name = parts.next()?;
            if name == package || name.starts_with(&format!("{package}:")) {
                Some((pid, name.to_string()))
            } else {
                None
            }
        })
        .collect()
}

async fn get_thread_count(adb: &PathBuf, device_serial: Option<&str>, pid: i32) -> Option<u32> {
    let output = adb_cmd(
        adb,
        device_serial,
        &["shell", "ps", "-T", "-p", &pid.to_string()],
    )
    .await
    .ok()?;
    let count = output.lines().filter(|l| !l.trim().is_empty()).count();
    if count > 1 {
        Some((count - 1) as u32)
    } else {
        None
    }
}

async fn get_rss_kb(adb: &PathBuf, device_serial: Option<&str>, pid: i32) -> Option<u64> {
    let output = adb_cmd(
        adb,
        device_serial,
        &["shell", "cat", &format!("/proc/{pid}/status")],
    )
    .await
    .ok()?;
    parse_vmrss(&output)
}

fn parse_vmrss(status: &str) -> Option<u64> {
    status
        .lines()
        .find(|l| l.starts_with("VmRSS:"))
        .and_then(|l| l.split_whitespace().nth(1))
        .and_then(|v| v.parse::<u64>().ok())
}

async fn resolve_launcher_activity(
    adb: &PathBuf,
    device_serial: &str,
    package: &str,
) -> Result<String, String> {
    let output = adb_cmd(
        adb,
        Some(device_serial),
        &[
            "shell",
            "cmd",
            "package",
            "resolve-activity",
            "--brief",
            "-c",
            "android.intent.category.LAUNCHER",
            package,
        ],
    )
    .await
    .map_err(|e| format!("resolve-activity failed: {e}"))?;

    // Output is typically two lines: priority then component
    // e.g.: "0\ncom.example.app/.MainActivity"
    output
        .lines()
        .find(|l| l.contains('/'))
        .map(|l| l.trim().to_string())
        .ok_or_else(|| format!("Could not resolve launcher activity for package '{package}'"))
}

async fn wait_for_displayed(
    adb: &PathBuf,
    device_serial: &str,
    package: &str,
    start: std::time::Instant,
) -> Option<u64> {
    // Format the anchor timestamp as "MM-DD HH:MM:SS.mmm" — the format
    // Android logcat's -T flag expects for a timestamp anchor.
    // A bare integer would be interpreted as a line count, not a timestamp.
    let anchor = {
        let now = chrono::Local::now();
        now.format("%m-%d %H:%M:%S%.3f").to_string()
    };

    let deadline = std::time::Duration::from_secs(10);
    loop {
        if start.elapsed() > deadline {
            return None;
        }
        tokio::time::sleep(std::time::Duration::from_millis(300)).await;

        let output = adb_cmd(
            adb,
            Some(device_serial),
            &["logcat", "-d", "-T", &anchor, "-s", "ActivityManager:I"],
        )
        .await
        .unwrap_or_default();

        if let Some(ms) = parse_displayed_time(&output, package) {
            return Some(ms);
        }
    }
}

fn parse_displayed_time(logcat_output: &str, package: &str) -> Option<u64> {
    for line in logcat_output.lines().rev() {
        if line.contains("Displayed") && line.contains(package) {
            if let Some(ms) = extract_display_ms(line) {
                return Some(ms);
            }
        }
    }
    None
}

fn extract_display_ms(line: &str) -> Option<u64> {
    let plus_pos = line.rfind('+')?;
    let rest = &line[plus_pos + 1..];
    // Trim any trailing punctuation/whitespace (e.g. closing paren) but keep alphanumeric
    let rest = rest.trim_end_matches(|c: char| !c.is_alphanumeric());

    // Try "Xs" or "XsYms" (seconds with optional milliseconds)
    // Look for a bare 's' that is preceded only by digits (not part of "ms")
    if let Some(s_pos) = rest.find("s") {
        let before_s = &rest[..s_pos];
        if before_s.chars().all(|c| c.is_ascii_digit()) && !before_s.is_empty() {
            let secs: u64 = before_s.parse().ok()?;
            let after_s = &rest[s_pos + 1..];
            // after_s may be empty or "YYYms"
            let ms: u64 = if after_s.is_empty() {
                0
            } else {
                after_s.trim_end_matches("ms").parse().unwrap_or(0)
            };
            return Some(secs * 1000 + ms);
        }
    }

    // Pure milliseconds: "YYYms"
    if let Some(ms_str) = rest.strip_suffix("ms") {
        return ms_str.parse().ok();
    }

    None
}

async fn adb_cmd(
    adb: &PathBuf,
    device_serial: Option<&str>,
    args: &[&str],
) -> Result<String, String> {
    let mut cmd = tokio::process::Command::new(adb);
    if let Some(serial) = device_serial {
        cmd.arg("-s").arg(serial);
    }
    match args.split_first() {
        // The device shell re-parses everything after `shell`.
        Some((&"shell", rest)) => {
            cmd.arg("shell")
                .args(rest.iter().map(|a| quote_device_shell_arg(a)));
        }
        _ => {
            cmd.args(args);
        }
    }
    let output = output_with_timeout(&mut cmd, ADB_QUERY_TIMEOUT)
        .await
        .map_err(|e| describe_failure("adb command", &e, ADB_UNRESPONSIVE_HINT))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).to_string();
        let stdout = String::from_utf8_lossy(&output.stdout).to_string();
        let msg = if !stderr.trim().is_empty() {
            stderr
        } else {
            stdout
        };
        return Err(msg);
    }
    Ok(String::from_utf8_lossy(&output.stdout).to_string())
}

#[cfg(test)]
mod tests {
    use crate::utils::device_shell::test_support::{fake_adb, recorded_calls};

    #[tokio::test]
    async fn shell_arguments_are_quoted_for_the_device() {
        let dir = tempfile::tempdir().unwrap();
        let (adb, record) = fake_adb(dir.path());

        // No real device answers, so launcher resolution fails after the stop;
        // only the force-stop invocation matters here.
        let _ = restart_app(&adb, "emulator-5554", "com.x;exit 3", false).await;

        assert_eq!(
            recorded_calls(&record).first(),
            Some(&vec![
                "am".to_string(),
                "force-stop".to_string(),
                "com.x;exit 3".to_string()
            ])
        );
    }

    use super::*;

    #[tokio::test]
    async fn restart_reports_an_activity_that_did_not_start() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().unwrap();
        let adb = dir.path().join("adb");
        std::fs::write(
            &adb,
            "#!/bin/sh\ncase \"$*\" in\n\
             *resolve-activity*) echo 0; echo com.example.app/.Main ;;\n\
             *'am start'*) echo 'Error: Activity class {com.example.app/.Main} does not exist.' ;;\n\
             esac\n",
        )
        .unwrap();
        std::fs::set_permissions(&adb, std::fs::Permissions::from_mode(0o755)).unwrap();
        crate::utils::process::test_support::run_once(&adb);

        let err = restart_app(&adb, "emulator-5554", "com.example.app", false)
            .await
            .unwrap_err();

        assert!(err.contains("does not exist"), "{err}");
    }

    #[test]
    fn parse_ps_finds_main_process() {
        let ps = "PID  NAME\n12345 com.example.app\n12389 com.example.app:push\n99999 com.other\n";
        let pids = parse_ps_for_package(ps, "com.example.app");
        assert_eq!(pids.len(), 2);
        assert!(pids.iter().any(|(pid, _)| *pid == 12345));
        assert!(pids.iter().any(|(pid, _)| *pid == 12389));
        assert!(!pids.iter().any(|(_, name)| name == "com.other"));
    }

    #[test]
    fn parse_ps_returns_empty_when_not_running() {
        let ps = "PID  NAME\n99999 com.other.app\n";
        let pids = parse_ps_for_package(ps, "com.example.app");
        assert!(pids.is_empty());
    }

    #[test]
    fn parse_vmrss_extracts_value() {
        let status = "Name:\tcom.example.app\nVmRSS:\t128456 kB\nVmPeak:\t200000 kB\n";
        assert_eq!(parse_vmrss(status), Some(128456));
    }

    #[test]
    fn parse_vmrss_returns_none_when_missing() {
        let status = "Name:\tcom.example.app\n";
        assert!(parse_vmrss(status).is_none());
    }

    #[test]
    fn parse_displayed_time_ms_format() {
        let log =
            "I ActivityManager: Displayed com.example.app/.MainActivity: +850ms (total +1s200ms)";
        assert_eq!(parse_displayed_time(log, "com.example.app"), Some(1200));
    }

    #[test]
    fn parse_displayed_time_simple_ms() {
        let log = "01-01 00:00:00 I ActivityManager: Displayed com.example.app/.Main: +450ms";
        assert_eq!(parse_displayed_time(log, "com.example.app"), Some(450));
    }

    #[test]
    fn parse_displayed_time_returns_none_when_absent() {
        let log = "01-01 00:00:00 I ActivityManager: Starting com.example.app";
        assert!(parse_displayed_time(log, "com.example.app").is_none());
    }
}
