//! One-shot external commands with a deadline.
//!
//! A wedged adb server, an unresponsive device, or an SDK tool stuck on a
//! prompt can keep a child alive forever, and a bare `output().await` waits
//! with it. Every one-shot command goes through [`output_with_timeout`] with
//! one of the deadlines below. Long-lived processes (logcat, Gradle, the
//! emulator, `sdkmanager` downloads) stream their output and are not one-shot.

use std::io;
use std::process::Output;
use std::time::Duration;
use tokio::process::Command;

/// Quick adb queries: `devices`, `getprop`, `pm`, `dumpsys`, `wm size`,
/// `cmd package resolve-activity`, `am force-stop`.
pub const ADB_QUERY_TIMEOUT: Duration = Duration::from_secs(10);
/// App launches (`am start`, `monkey`), which wait for the activity to start.
pub const ADB_LAUNCH_TIMEOUT: Duration = Duration::from_secs(30);
/// `adb install`: large APKs, slow USB links, and on-device verification.
pub const ADB_INSTALL_TIMEOUT: Duration = Duration::from_secs(5 * 60);
/// `adb exec-out screencap -p`: transfers a full-screen PNG.
pub const ADB_SCREENSHOT_TIMEOUT: Duration = Duration::from_secs(30);
/// `adb emu kill`.
pub const EMULATOR_STOP_TIMEOUT: Duration = Duration::from_secs(15);
/// `aapt2 dump packagename`.
pub const AAPT2_TIMEOUT: Duration = Duration::from_secs(30);
/// `avdmanager` list/create/delete (JVM start-up plus disk work).
pub const AVDMANAGER_TIMEOUT: Duration = Duration::from_secs(60);
/// `sdkmanager --list`, which downloads the remote repository index.
pub const SDKMANAGER_LIST_TIMEOUT: Duration = Duration::from_secs(120);
/// Health-check probes: `java -version`, `adb version`, `which studio`.
pub const TOOL_PROBE_TIMEOUT: Duration = Duration::from_secs(10);

/// What to try when an Android SDK command-line tool stops answering.
pub const SDK_TOOL_HINT: &str = "check the Android SDK and Java paths in Settings and try again";
/// What to try when adb stops answering.
pub const ADB_UNRESPONSIVE_HINT: &str = "the device or the adb server is not responding; \
     reconnect the device or run `adb kill-server` and try again";

/// Run `cmd` to completion and collect its output, like `Command::output`,
/// but give up after `timeout`.
///
/// On timeout the child is killed (`kill_on_drop`) and reaped by the tokio
/// runtime, and the error has kind [`io::ErrorKind::TimedOut`] with a message
/// such as `timed out after 10 s`.
pub async fn output_with_timeout(cmd: &mut Command, timeout: Duration) -> io::Result<Output> {
    cmd.kill_on_drop(true);
    match tokio::time::timeout(timeout, cmd.output()).await {
        Ok(result) => result,
        Err(_) => Err(timed_out(timeout)),
    }
}

/// The error [`output_with_timeout`] returns when `timeout` elapses, for call
/// sites that must drive the child themselves (for example to write stdin).
pub fn timed_out(timeout: Duration) -> io::Error {
    io::Error::new(
        io::ErrorKind::TimedOut,
        format!("timed out after {}", format_duration(timeout)),
    )
}

/// Describe a failed command for the user: `"{what} failed: {err}"`, or on
/// timeout `"{what} timed out after 10 s — {hint}"`.
pub fn describe_failure(what: &str, err: &io::Error, hint: &str) -> String {
    if err.kind() == io::ErrorKind::TimedOut {
        format!("{what} {err} — {hint}")
    } else {
        format!("{what} failed: {err}")
    }
}

fn format_duration(d: Duration) -> String {
    let secs = d.as_secs();
    if secs >= 60 && secs.is_multiple_of(60) {
        format!("{} min", secs / 60)
    } else if secs >= 1 && d.subsec_millis() == 0 {
        format!("{secs} s")
    } else {
        format!("{} ms", d.as_millis())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Instant;

    fn is_alive(pid: &str) -> bool {
        std::process::Command::new("kill")
            .args(["-0", pid])
            .status()
            .map(|s| s.success())
            .unwrap_or(false)
    }

    #[tokio::test]
    async fn fast_command_returns_its_output() {
        let out = output_with_timeout(
            Command::new("sh").args(["-c", "echo hello; echo oops >&2; exit 3"]),
            Duration::from_secs(10),
        )
        .await
        .unwrap();
        assert_eq!(String::from_utf8_lossy(&out.stdout), "hello\n");
        assert_eq!(String::from_utf8_lossy(&out.stderr), "oops\n");
        assert_eq!(out.status.code(), Some(3));
    }

    #[tokio::test]
    async fn spawn_failure_is_reported_as_is() {
        let err = output_with_timeout(
            &mut Command::new("/nonexistent/keynobi-test-tool"),
            Duration::from_secs(10),
        )
        .await
        .unwrap_err();
        assert_eq!(err.kind(), io::ErrorKind::NotFound);
    }

    #[tokio::test]
    async fn hung_command_times_out_and_is_killed() {
        let dir = tempfile::tempdir().unwrap();
        let pid_file = dir.path().join("pid");
        // `exec` keeps the pid we record as the direct child we spawn.
        let script = format!("echo $$ > '{}'; exec sleep 30", pid_file.display());

        let start = Instant::now();
        let err = output_with_timeout(
            Command::new("sh").args(["-c", &script]),
            Duration::from_millis(200),
        )
        .await
        .unwrap_err();
        assert_eq!(err.kind(), io::ErrorKind::TimedOut);
        assert_eq!(err.to_string(), "timed out after 200 ms");
        assert!(start.elapsed() < Duration::from_secs(5));

        let pid = std::fs::read_to_string(&pid_file).unwrap();
        let pid = pid.trim();
        assert!(!pid.is_empty());
        // `kill -0` also succeeds for an unreaped zombie, so this checks that
        // the child was both killed and reaped.
        let deadline = Instant::now() + Duration::from_secs(5);
        while is_alive(pid) && Instant::now() < deadline {
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
        assert!(!is_alive(pid), "child {pid} survived the timeout");
    }

    #[test]
    fn timeout_failures_carry_the_hint() {
        let timed_out = io::Error::new(io::ErrorKind::TimedOut, "timed out after 10 s");
        assert_eq!(
            describe_failure("adb devices", &timed_out, "try again"),
            "adb devices timed out after 10 s — try again"
        );
        let missing = io::Error::new(io::ErrorKind::NotFound, "no such file");
        assert_eq!(
            describe_failure("adb devices", &missing, "try again"),
            "adb devices failed: no such file"
        );
    }

    #[test]
    fn durations_are_formatted_for_people() {
        assert_eq!(format_duration(Duration::from_secs(300)), "5 min");
        assert_eq!(format_duration(Duration::from_secs(90)), "90 s");
        assert_eq!(format_duration(Duration::from_secs(10)), "10 s");
        assert_eq!(format_duration(Duration::from_millis(200)), "200 ms");
    }
}
