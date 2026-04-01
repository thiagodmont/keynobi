use std::path::PathBuf;

// ── Public types ──────────────────────────────────────────────────────────────

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
    pub cold_start: bool,
}

// ── Runtime state ─────────────────────────────────────────────────────────────

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

    let mut processes = Vec::new();
    for (pid, name) in &pids {
        let (threads, rss) = tokio::join!(
            get_thread_count(adb, device_serial, *pid),
            get_rss_kb(adb, device_serial, *pid)
        );
        processes.push(ProcessInfo {
            pid: *pid,
            name: name.clone(),
            thread_count: threads,
            rss_kb: rss,
        });
    }

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
pub async fn restart_app(
    adb: &PathBuf,
    device_serial: &str,
    package: &str,
    cold: bool,
) -> Result<RestartResult, String> {
    if cold {
        run_adb_shell(adb, device_serial, &["pm", "clear", package]).await?;
    } else {
        run_adb_shell(adb, device_serial, &["am", "force-stop", package]).await?;
    }

    let activity = resolve_launcher_activity(adb, device_serial, package).await?;

    let start = std::time::Instant::now();
    run_adb_shell(adb, device_serial, &["am", "start", "-n", &activity]).await?;

    let display_time_ms = wait_for_displayed(adb, device_serial, package, start).await;

    Ok(RestartResult {
        launched: true,
        activity: Some(activity),
        display_time_ms,
        cold_start: cold,
    })
}

// ── Internal helpers ──────────────────────────────────────────────────────────

/// Returns Vec of (pid, process_name) for all processes matching the package.
pub async fn find_pids_for_package(
    adb: &PathBuf,
    device_serial: Option<&str>,
    package: &str,
) -> Vec<(i32, String)> {
    let output = adb_cmd(adb, device_serial, &["shell", "ps", "-A", "-o", "PID,NAME"])
        .await
        .unwrap_or_default();
    parse_ps_for_package(&output, package)
}

pub fn parse_ps_for_package(output: &str, package: &str) -> Vec<(i32, String)> {
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
    if count > 1 { Some((count - 1) as u32) } else { None }
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

pub fn parse_vmrss(status: &str) -> Option<u64> {
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
        .ok_or_else(|| {
            format!("Could not resolve launcher activity for package '{package}'")
        })
}

async fn wait_for_displayed(
    adb: &PathBuf,
    device_serial: &str,
    package: &str,
    start: std::time::Instant,
) -> Option<u64> {
    // Anchor to entries after the launch began so stale "Displayed" lines
    // from a previous run of the same package are not matched.
    let anchor_ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0);

    let deadline = std::time::Duration::from_secs(10);
    loop {
        if start.elapsed() > deadline {
            return None;
        }
        tokio::time::sleep(std::time::Duration::from_millis(300)).await;

        let output = adb_cmd(
            adb,
            Some(device_serial),
            &[
                "logcat",
                "-d",
                "-T",
                &anchor_ms.to_string(),
                "-s",
                "ActivityManager:I",
            ],
        )
        .await
        .unwrap_or_default();

        if let Some(ms) = parse_displayed_time(&output, package) {
            return Some(ms);
        }
    }
}

pub fn parse_displayed_time(logcat_output: &str, package: &str) -> Option<u64> {
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

async fn run_adb_shell(
    adb: &PathBuf,
    device_serial: &str,
    args: &[&str],
) -> Result<String, String> {
    let output = tokio::process::Command::new(adb)
        .arg("-s")
        .arg(device_serial)
        .arg("shell")
        .args(args)
        .output()
        .await
        .map_err(|e| format!("adb shell failed: {e}"))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).to_string();
        let stdout = String::from_utf8_lossy(&output.stdout).to_string();
        let msg = if !stderr.trim().is_empty() { stderr } else { stdout };
        return Err(msg);
    }
    Ok(String::from_utf8_lossy(&output.stdout).to_string())
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
    cmd.args(args);
    let output = cmd
        .output()
        .await
        .map_err(|e| format!("adb command failed: {e}"))?;
    Ok(String::from_utf8_lossy(&output.stdout).to_string())
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

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
        let log = "I ActivityManager: Displayed com.example.app/.MainActivity: +850ms (total +1s200ms)";
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
