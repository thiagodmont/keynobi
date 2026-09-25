//! Capture UI Automator hierarchy XML from a device via ADB and parse it.

use crate::models::ui_hierarchy::{UiHierarchySnapshot, UiLayoutContext};
use crate::services::ui_automator_lock::{self, foreign_client_busy, reports_already_registered};
use crate::services::ui_hierarchy_parse::{
    compute_screen_hash, count_interactive_nodes, parse_hierarchy_xml, ParseOutcome,
};
use crate::utils::process::{format_duration, output_with_timeout, ADB_UNRESPONSIVE_HINT};
use base64::{engine::general_purpose::STANDARD as BASE64, Engine as _};
use chrono::Utc;
use std::path::Path;
use std::process::Output;
use std::time::Duration;
use tokio::process::Command;
use tokio::time::Instant;

/// Maximum raw XML bytes read from adb (host memory bound).
pub const MAX_XML_BYTES: usize = 4 * 1024 * 1024;
/// Maximum screenshot PNG bytes accepted (8 MiB). Larger captures are silently dropped.
const MAX_SCREENSHOT_BYTES: usize = 8 * 1024 * 1024;
/// Timeout for `screencap -p` (slower than a shell probe; some emulators can be sluggish).
const SCREENSHOT_TIMEOUT: Duration = Duration::from_secs(20);
/// Wall-clock limit for a single adb dump attempt.
pub const DUMP_TIMEOUT: Duration = Duration::from_secs(25);
/// Wall-clock limit for one whole capture: waiting for the device's UI
/// Automator lock, the shell probes, every dump attempt and retry, and the
/// screenshot. Without it the fallbacks alone could run for minutes.
pub const CAPTURE_TOTAL_DEADLINE: Duration = Duration::from_secs(60);
/// Shorter limit for lightweight `dumpsys` / `wm` probes.
const LAYOUT_PROBE_TIMEOUT: Duration = Duration::from_secs(12);
/// Cap per layout-context excerpt (host memory).
const MAX_WINDOW_EXCERPT_BYTES: usize = 12 * 1024;
const MAX_DISPLAY_EXCERPT_BYTES: usize = 8 * 1024;
const MAX_WM_LINE_BYTES: usize = 512;
/// Bytes to scan from `dumpsys activity` for a resumed activity line.
const DUMPSYS_ACTIVITY_PREFIX_BYTES: usize = 512 * 1024;

/// One shell-invokable `adb` line (path + `-s` + args).
pub fn format_adb_command(adb: &Path, serial: &str, args: &[&str]) -> String {
    let mut s = adb.display().to_string();
    s.push_str(" -s ");
    s.push_str(serial);
    for a in args {
        s.push(' ');
        s.push_str(a);
    }
    s
}

fn utf8_lossy_cap(bytes: &[u8], max_bytes: usize) -> String {
    if bytes.is_empty() {
        return String::new();
    }
    let take = bytes.len().min(max_bytes);
    let mut end = take;
    while end > 0 && std::str::from_utf8(&bytes[..end]).is_err() {
        end -= 1;
    }
    String::from_utf8_lossy(&bytes[..end]).into_owned()
}

/// What is left of one capture's [`CAPTURE_TOTAL_DEADLINE`].
struct CaptureBudget {
    deadline: Instant,
    total: Duration,
}

impl CaptureBudget {
    fn new(total: Duration) -> Self {
        Self {
            deadline: Instant::now() + total,
            total,
        }
    }

    /// `limit` cut to what is left of the budget; `None` once it is spent.
    fn step(&self, limit: Duration) -> Option<Duration> {
        let left = self.deadline.saturating_duration_since(Instant::now());
        (!left.is_zero()).then(|| left.min(limit))
    }

    fn is_spent(&self) -> bool {
        self.step(Duration::MAX).is_none()
    }

    fn label(&self) -> String {
        format_duration(self.total)
    }

    fn exceeded(&self, serial: &str) -> String {
        format!(
            "UI hierarchy capture on {serial} did not finish within the {} total deadline \
             (every dump attempt and retry) — {ADB_UNRESPONSIVE_HINT}",
            self.label()
        )
    }
}

/// Run `adb -s <serial> <args>` for at most `limit` and what is left of the
/// budget. `Ok(None)` when the command failed to run or timed out; `Err` once
/// the budget is spent.
async fn run_step(
    adb: &Path,
    serial: &str,
    args: &[&str],
    limit: Duration,
    budget: &CaptureBudget,
) -> Result<Option<Output>, String> {
    let step = budget.step(limit).ok_or_else(|| budget.exceeded(serial))?;
    match output_with_timeout(Command::new(adb).args(["-s", serial]).args(args), step).await {
        Ok(out) => Ok(Some(out)),
        Err(_) if budget.is_spent() => Err(budget.exceeded(serial)),
        Err(_) => Ok(None),
    }
}

fn reports_foreign_client(out: &Output) -> bool {
    reports_already_registered(&out.stdout) || reports_already_registered(&out.stderr)
}

fn exec_out_dump_args(compressed: bool) -> &'static [&'static str] {
    if compressed {
        &[
            "exec-out",
            "uiautomator",
            "dump",
            "--compressed",
            "/dev/tty",
        ]
    } else {
        &["exec-out", "uiautomator", "dump", "/dev/tty"]
    }
}

async fn try_exec_out_uiautomator_dump(
    adb: &Path,
    serial: &str,
    compressed: bool,
    budget: &CaptureBudget,
) -> Result<Option<String>, String> {
    let args = exec_out_dump_args(compressed);
    let Some(out) = run_step(adb, serial, args, DUMP_TIMEOUT, budget).await? else {
        return Ok(None);
    };
    let raw = strip_ui_automator_noise(&out.stdout);
    if out.status.success() && raw.trim_start().starts_with('<') {
        return Ok(Some(raw));
    }
    // Checked only when no XML came back: app text in a dump may contain the phrase.
    if reports_foreign_client(&out) {
        return Err(foreign_client_busy(serial));
    }
    Ok(None)
}

/// Run `uiautomator dump` and return UTF-8 XML (may be truncated to [`MAX_XML_BYTES`]).
/// Tries `--compressed` first (smaller / faster on supported builds), then plain dump.
/// The third tuple element lists every adb invocation attempted (for debugging).
async fn dump_hierarchy_xml(
    adb: &Path,
    serial: &str,
    budget: &CaptureBudget,
) -> Result<(String, bool, Vec<String>), String> {
    let mut command_log = Vec::new();

    // 1) exec-out, compressed (API-dependent) then plain.
    for compressed in [true, false] {
        command_log.push(format_adb_command(
            adb,
            serial,
            exec_out_dump_args(compressed),
        ));
        if let Some(raw) = try_exec_out_uiautomator_dump(adb, serial, compressed, budget).await? {
            let (s, truncated) = truncate_utf8(raw, MAX_XML_BYTES);
            return Ok((s, truncated, command_log));
        }
    }

    // 2) Fallback: dump to default path on device, then cat to host (compressed then plain).
    for compressed in [true, false] {
        let dump_args: &[&str] = if compressed {
            &["shell", "uiautomator", "dump", "--compressed"]
        } else {
            &["shell", "uiautomator", "dump"]
        };
        command_log.push(format_adb_command(adb, serial, dump_args));
        let Some(dump) = run_step(adb, serial, dump_args, DUMP_TIMEOUT, budget).await? else {
            continue;
        };
        if reports_foreign_client(&dump) {
            return Err(foreign_client_busy(serial));
        }
        if !dump.status.success() {
            continue;
        }

        for path in [
            "/sdcard/window_dump.xml",
            "/storage/emulated/0/window_dump.xml",
        ] {
            let cat_args = ["exec-out", "cat", path];
            command_log.push(format_adb_command(adb, serial, &cat_args));
            match run_step(adb, serial, &cat_args, DUMP_TIMEOUT, budget).await? {
                Some(out) if out.status.success() && !out.stdout.is_empty() => {
                    let raw = strip_ui_automator_noise(&out.stdout);
                    if raw.trim_start().starts_with('<') {
                        let (s, truncated) = truncate_utf8(raw, MAX_XML_BYTES);
                        return Ok((s, truncated, command_log));
                    }
                }
                _ => {}
            }
        }
    }

    if budget.is_spent() {
        return Err(budget.exceeded(serial));
    }
    Err(
        "uiautomator dump failed (exec-out and shell dump, compressed and plain). Is the device online?"
            .to_string(),
    )
}

/// Official shell excerpts: window focus, display, logical size / density.
async fn probe_layout_context(
    adb: &Path,
    serial: &str,
    budget: &CaptureBudget,
) -> (UiLayoutContext, Vec<String>) {
    let mut command_log = Vec::new();

    let w_args = ["-s", serial, "shell", "dumpsys", "window", "windows"];
    command_log.push(format_adb_command(adb, serial, &w_args[2..]));
    let d_args = ["-s", serial, "shell", "dumpsys", "display"];
    command_log.push(format_adb_command(adb, serial, &d_args[2..]));
    let sz_args = ["-s", serial, "shell", "wm", "size"];
    command_log.push(format_adb_command(adb, serial, &sz_args[2..]));
    let den_args = ["-s", serial, "shell", "wm", "density"];
    command_log.push(format_adb_command(adb, serial, &den_args[2..]));

    // Best-effort context: skipped once the capture's budget is spent.
    let Some(limit) = budget.step(LAYOUT_PROBE_TIMEOUT) else {
        return (UiLayoutContext::default(), command_log);
    };
    let mut win_cmd = Command::new(adb);
    win_cmd.args(w_args);
    let mut disp_cmd = Command::new(adb);
    disp_cmd.args(d_args);
    let mut sz_cmd = Command::new(adb);
    sz_cmd.args(sz_args);
    let mut den_cmd = Command::new(adb);
    den_cmd.args(den_args);

    let (win_o, disp_o, sz_o, den_o) = tokio::join!(
        output_with_timeout(&mut win_cmd, limit),
        output_with_timeout(&mut disp_cmd, limit),
        output_with_timeout(&mut sz_cmd, limit),
        output_with_timeout(&mut den_cmd, limit),
    );

    let window_excerpt = match win_o {
        Ok(o) if o.status.success() && !o.stdout.is_empty() => {
            Some(utf8_lossy_cap(&o.stdout, MAX_WINDOW_EXCERPT_BYTES))
        }
        _ => None,
    };
    let display_excerpt = match disp_o {
        Ok(o) if o.status.success() && !o.stdout.is_empty() => {
            Some(utf8_lossy_cap(&o.stdout, MAX_DISPLAY_EXCERPT_BYTES))
        }
        _ => None,
    };
    let wm_size = match sz_o {
        Ok(o) if o.status.success() && !o.stdout.is_empty() => {
            Some(utf8_lossy_cap(&o.stdout, MAX_WM_LINE_BYTES))
        }
        _ => None,
    };
    let wm_density = match den_o {
        Ok(o) if o.status.success() && !o.stdout.is_empty() => {
            Some(utf8_lossy_cap(&o.stdout, MAX_WM_LINE_BYTES))
        }
        _ => None,
    };

    (
        UiLayoutContext {
            window_excerpt,
            display_excerpt,
            wm_size,
            wm_density,
        },
        command_log,
    )
}

/// Capture a PNG screenshot via `adb exec-out screencap -p`.
/// Returns base64-encoded PNG on success, `None` if the command fails or the output
/// is too large / clearly not a PNG.
async fn capture_screenshot_b64(
    adb: &Path,
    serial: &str,
    budget: &CaptureBudget,
) -> Option<String> {
    let limit = budget.step(SCREENSHOT_TIMEOUT)?;
    let out = output_with_timeout(
        Command::new(adb).args(["-s", serial, "exec-out", "screencap", "-p"]),
        limit,
    )
    .await
    .ok()?;
    if !out.status.success() || out.stdout.is_empty() {
        return None;
    }
    // Validate PNG magic bytes (first 8 bytes: \x89PNG\r\n\x1a\n).
    let magic: &[u8] = &[0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a];
    if !out.stdout.starts_with(magic) {
        return None;
    }
    if out.stdout.len() > MAX_SCREENSHOT_BYTES {
        return None;
    }
    Some(BASE64.encode(&out.stdout))
}

/// Single pipeline: resumed activity, layout shell context, hierarchy XML, screenshot, snapshot.
///
/// Holds the device's UI Automator lock for the whole capture, so GUI and MCP
/// calls on one device run one at a time, and gives up after
/// [`CAPTURE_TOTAL_DEADLINE`], counting the wait for the lock.
pub async fn capture_ui_hierarchy_snapshot(
    adb: &Path,
    serial: &str,
) -> Result<UiHierarchySnapshot, String> {
    capture_within(adb, serial, CAPTURE_TOTAL_DEADLINE).await
}

async fn capture_within(
    adb: &Path,
    serial: &str,
    total: Duration,
) -> Result<UiHierarchySnapshot, String> {
    let budget = CaptureBudget::new(total);
    let _device = ui_automator_lock::acquire(serial, budget.deadline, &budget.label()).await?;

    let mut command_log = vec![format_adb_command(
        adb,
        serial,
        &["shell", "dumpsys", "activity", "activities"],
    )];
    let fg = probe_foreground_activity(adb, serial, &budget).await;

    let (layout_ctx, mut layout_cmds) = probe_layout_context(adb, serial, &budget).await;
    command_log.append(&mut layout_cmds);

    let (xml, xml_truncated, mut dump_cmds) = dump_hierarchy_xml(adb, serial, &budget).await?;
    command_log.append(&mut dump_cmds);

    command_log.push(format_adb_command(
        adb,
        serial,
        &["exec-out", "screencap", "-p"],
    ));
    let screenshot_b64 = capture_screenshot_b64(adb, serial, &budget).await;

    Ok(build_snapshot(
        &xml,
        xml_truncated,
        fg,
        layout_ctx,
        command_log,
        screenshot_b64,
    ))
}

/// Best-effort foreground activity / resumed component line.
async fn probe_foreground_activity(
    adb: &Path,
    serial: &str,
    budget: &CaptureBudget,
) -> Option<String> {
    let limit = budget.step(DUMP_TIMEOUT)?;
    let output = output_with_timeout(
        Command::new(adb).args(["-s", serial, "shell", "dumpsys", "activity", "activities"]),
        limit,
    )
    .await
    .ok()?;
    if !output.status.success() {
        return None;
    }

    let take = output.stdout.len().min(DUMPSYS_ACTIVITY_PREFIX_BYTES);
    let text = String::from_utf8_lossy(&output.stdout[..take]);
    text.lines()
        .find(|l| {
            l.contains("mResumedActivity")
                || l.contains("topResumedActivity")
                || l.contains("ResumedActivity")
        })
        .map(|l| l.trim().to_string())
}

/// Byte index just after `/>` when `<hierarchy` opens a self-closing root element.
/// Returns `None` if the tag is not self-closing (content starts with `>`), in which case
/// callers should rely on `</hierarchy>`.
fn find_self_closing_hierarchy_end(s: &str, tag_open_lt: usize) -> Option<usize> {
    const TAG: &[u8] = b"<hierarchy";
    let b = s.as_bytes();
    if tag_open_lt + TAG.len() > b.len() {
        return None;
    }
    if &b[tag_open_lt..tag_open_lt + TAG.len()] != TAG {
        return None;
    }
    let mut i = tag_open_lt + TAG.len();
    let mut in_dquote = false;
    let mut in_squote = false;
    while i < b.len() {
        let ch = b[i];
        if in_dquote {
            if ch == b'"' {
                in_dquote = false;
            }
            i += 1;
            continue;
        }
        if in_squote {
            if ch == b'\'' {
                in_squote = false;
            }
            i += 1;
            continue;
        }
        match ch {
            b'"' => {
                in_dquote = true;
                i += 1;
            }
            b'\'' => {
                in_squote = true;
                i += 1;
            }
            b'/' if i + 1 < b.len() && b[i + 1] == b'>' => return Some(i + 2),
            b'>' => return None,
            _ => i += 1,
        }
    }
    None
}

fn strip_ui_automator_noise(bytes: &[u8]) -> String {
    let s = String::from_utf8_lossy(bytes).into_owned();
    // Some builds prefix lines like "UI hierchary dumped to: ..."; exec-out /dev/tty may also
    // append the same message after the document (after `</hierarchy>` or `<hierarchy/>`),
    // which breaks strict XML parsers.
    let mut s = if let Some(idx) = s.find("<?xml") {
        s[idx..].to_string()
    } else if let Some(idx) = s.find("<hierarchy") {
        s[idx..].to_string()
    } else {
        s
    };
    const END: &str = "</hierarchy>";
    if let Some(pos) = s.rfind(END) {
        let end = pos + END.len();
        s.truncate(end);
    } else if let Some(start) = s.find("<hierarchy") {
        if let Some(end) = find_self_closing_hierarchy_end(&s, start) {
            s.truncate(end);
        }
    }
    s
}

fn truncate_utf8(s: String, max_bytes: usize) -> (String, bool) {
    if s.len() <= max_bytes {
        return (s, false);
    }
    let mut end = max_bytes;
    while end > 0 && !s.is_char_boundary(end) {
        end -= 1;
    }
    (s[..end].to_string(), true)
}

/// Full pipeline: dump XML, parse, build snapshot.
pub fn build_snapshot(
    xml: &str,
    xml_truncated: bool,
    foreground_activity: Option<String>,
    layout_context: UiLayoutContext,
    command_log: Vec<String>,
    screenshot_b64: Option<String>,
) -> UiHierarchySnapshot {
    let ParseOutcome {
        root,
        truncated: parse_truncated,
        warnings: mut parse_warnings,
        node_count: _,
    } = parse_hierarchy_xml(xml);

    if xml_truncated {
        parse_warnings.push(format!(
            "Raw XML exceeded {} MiB and was truncated before parse",
            MAX_XML_BYTES / (1024 * 1024)
        ));
    }

    let truncated = xml_truncated || parse_truncated;
    let screen_hash = compute_screen_hash(&root);
    let interactive_count = count_interactive_nodes(&root);

    UiHierarchySnapshot {
        captured_at: Utc::now().to_rfc3339(),
        truncated,
        warnings: parse_warnings,
        root,
        screen_hash,
        interactive_count,
        foreground_activity,
        layout_context,
        command_log,
        screenshot_b64,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strip_noise_before_xml() {
        let raw = "UI hierchary dumped to: /dev/tty\n<?xml version='1.0'?><hierarchy/>";
        let s = strip_ui_automator_noise(raw.as_bytes());
        assert!(s.starts_with("<?xml"));
    }

    #[test]
    fn strip_trailing_message_after_hierarchy() {
        let raw = "<?xml version='1.0' encoding='UTF-8' standalone='yes'?><hierarchy></hierarchy>UI hierchary dumped to: /dev/tty\n";
        let s = strip_ui_automator_noise(raw.as_bytes());
        assert_eq!(
            s,
            "<?xml version='1.0' encoding='UTF-8' standalone='yes'?><hierarchy></hierarchy>"
        );
    }

    #[test]
    fn strip_trailing_message_after_self_closing_hierarchy() {
        let raw = "<?xml version='1.0'?><hierarchy/>UI hierchary dumped to: /dev/tty\n";
        let s = strip_ui_automator_noise(raw.as_bytes());
        assert_eq!(s, "<?xml version='1.0'?><hierarchy/>");
    }

    #[test]
    fn strip_trailing_noise_after_hierarchy_with_attrs_self_closed() {
        let raw = "<?xml version='1.0'?><hierarchy rotation=\"0\"/>junk";
        let s = strip_ui_automator_noise(raw.as_bytes());
        assert_eq!(s, "<?xml version='1.0'?><hierarchy rotation=\"0\"/>");
    }

    #[test]
    fn self_closing_scanner_ignores_gt_inside_quoted_attrs() {
        let s = r#"<hierarchy bounds="[0,0][1>2]"/>"#;
        let end = find_self_closing_hierarchy_end(s, 0).expect("closed");
        assert_eq!(&s[..end], s);
    }

    // ── Device lock, deadlines, and busy devices ──────────────────────────────
    //
    // Each test uses its own serials: the lock and instrumentation registries
    // are process-wide and tests run in parallel.

    use crate::services::ui_automator_lock::test_support::begin_instrumentation_on;
    use std::path::PathBuf;
    use std::time::Instant as StdInstant;

    const SAMPLE_XML: &str = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/src/services/fixtures/ui_hierarchy_sample.xml"
    );
    const ALREADY_REGISTERED_LINE: &str = "java.lang.IllegalStateException: UiAutomationService \
         android.accessibilityservice.IAccessibilityServiceClient@1 already registered!";

    /// An `adb` that records `$*` per call in `calls`, answers shell probes
    /// with nothing, and runs `uiautomator_arm` for any `uiautomator` call.
    /// In the arm, `$D` is the test directory and `$2` the serial.
    fn scripted_adb(dir: &Path, uiautomator_arm: &str) -> PathBuf {
        use std::os::unix::fs::PermissionsExt;
        let adb = dir.join("adb");
        std::fs::write(
            &adb,
            format!(
                "#!/bin/sh\nD='{}'\necho \"$*\" >> \"$D/calls\"\ncase \"$*\" in\n\
                 *uiautomator*)\n{uiautomator_arm}\n;;\n*) exit 0 ;;\nesac\n",
                dir.display()
            ),
        )
        .unwrap();
        std::fs::set_permissions(&adb, std::fs::Permissions::from_mode(0o755)).unwrap();
        adb
    }

    fn lines(path: &Path) -> Vec<String> {
        std::fs::read_to_string(path)
            .unwrap_or_default()
            .lines()
            .map(str::to_string)
            .collect()
    }

    fn uiautomator_calls(dir: &Path) -> usize {
        lines(&dir.join("calls"))
            .iter()
            .filter(|c| c.contains("uiautomator"))
            .count()
    }

    fn is_alive(pid: &str) -> bool {
        std::process::Command::new("kill")
            .args(["-0", pid])
            .stderr(std::process::Stdio::null())
            .status()
            .map(|s| s.success())
            .unwrap_or(false)
    }

    #[tokio::test]
    async fn a_hung_dump_is_killed_when_the_total_deadline_passes() {
        let dir = tempfile::tempdir().unwrap();
        // `exec` keeps the recorded pid as the child we spawn.
        let adb = scripted_adb(dir.path(), "echo $$ >> \"$D/pids\"; exec sleep 30");

        let start = StdInstant::now();
        let err = capture_within(&adb, "hier-hung", Duration::from_secs(2))
            .await
            .unwrap_err();
        assert!(err.contains("2 s total deadline"), "{err}");
        assert!(start.elapsed() < Duration::from_secs(8));

        let pids = lines(&dir.path().join("pids"));
        assert!(!pids.is_empty(), "the dump never ran");
        let deadline = StdInstant::now() + Duration::from_secs(5);
        while pids.iter().any(|p| is_alive(p)) && StdInstant::now() < deadline {
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
        for pid in &pids {
            assert!(
                !is_alive(pid),
                "uiautomator dump {pid} outlived the capture"
            );
        }
    }

    #[tokio::test]
    async fn retries_stop_at_the_total_deadline() {
        let dir = tempfile::tempdir().unwrap();
        // Four failing attempts would take 2.4 s without a total deadline.
        let adb = scripted_adb(dir.path(), "sleep 0.6; exit 1");

        let start = StdInstant::now();
        let err = capture_within(&adb, "hier-retries", Duration::from_secs(1))
            .await
            .unwrap_err();
        let elapsed = start.elapsed();

        assert!(err.contains("1 s total deadline"), "{err}");
        assert!(elapsed < Duration::from_millis(2000), "took {elapsed:?}");
        assert!(uiautomator_calls(dir.path()) < 4);
    }

    #[tokio::test]
    async fn concurrent_captures_on_one_device_take_turns() {
        let dir = tempfile::tempdir().unwrap();
        // Like a device: a second UiAutomation client is refused while one is registered.
        let arm = format!(
            "if ! mkdir \"$D/registered-$2\" 2>/dev/null; then echo '{ALREADY_REGISTERED_LINE}' >&2; exit 1; fi\n\
             sleep 0.3; cat '{SAMPLE_XML}'; rmdir \"$D/registered-$2\""
        );
        let adb = scripted_adb(dir.path(), &arm);

        let (a, b) = tokio::join!(
            capture_within(&adb, "hier-same", Duration::from_secs(10)),
            capture_within(&adb, "hier-same", Duration::from_secs(10)),
        );
        assert!(a.is_ok(), "{:?}", a.err());
        assert!(b.is_ok(), "{:?}", b.err());
        assert_eq!(uiautomator_calls(dir.path()), 2);
    }

    #[tokio::test]
    async fn captures_on_different_devices_do_not_wait_for_each_other() {
        let dir = tempfile::tempdir().unwrap();
        // Each dump waits up to 3 s to see the other device's dump running.
        let arm = format!(
            "touch \"$D/inside-$2\"; i=0\n\
             while [ $i -lt 30 ]; do\n\
               if [ \"$(ls \"$D\" | grep -c '^inside-')\" -ge 2 ]; then echo \"$2\" >> \"$D/overlaps\"; break; fi\n\
               sleep 0.1; i=$((i+1))\n\
             done\n\
             cat '{SAMPLE_XML}'"
        );
        let adb = scripted_adb(dir.path(), &arm);

        let (a, b) = tokio::join!(
            capture_within(&adb, "hier-par-a", Duration::from_secs(10)),
            capture_within(&adb, "hier-par-b", Duration::from_secs(10)),
        );
        assert!(a.is_ok() && b.is_ok());
        assert_eq!(
            lines(&dir.path().join("overlaps")).len(),
            2,
            "each dump should have run while the other was running"
        );
    }

    #[tokio::test]
    async fn an_instrumentation_run_makes_the_device_busy_without_touching_it() {
        let dir = tempfile::tempdir().unwrap();
        let adb = scripted_adb(dir.path(), &format!("cat '{SAMPLE_XML}'"));

        let run = begin_instrumentation_on("hier-instr");
        let err = capture_within(&adb, "hier-instr", Duration::from_secs(10))
            .await
            .unwrap_err();
        assert!(err.contains("busy: instrumentation running"), "{err}");
        assert!(
            lines(&dir.path().join("calls")).is_empty(),
            "a busy device must not be sent any command"
        );

        capture_within(&adb, "hier-instr-other", Duration::from_secs(10))
            .await
            .expect("other devices stay usable");

        drop(run);
        capture_within(&adb, "hier-instr", Duration::from_secs(10))
            .await
            .expect("the device is free once the run ends");
    }

    #[tokio::test]
    async fn another_ui_automation_client_fails_fast_as_busy() {
        let dir = tempfile::tempdir().unwrap();
        let adb = scripted_adb(
            dir.path(),
            &format!("echo '{ALREADY_REGISTERED_LINE}'; exit 1"),
        );

        let err = capture_within(&adb, "hier-foreign", Duration::from_secs(10))
            .await
            .unwrap_err();
        assert!(err.contains("another UI Automator client"), "{err}");
        assert_eq!(uiautomator_calls(dir.path()), 1, "no retries");
    }

    #[tokio::test]
    async fn app_text_mentioning_already_registered_is_not_a_busy_device() {
        let dir = tempfile::tempdir().unwrap();
        let adb = scripted_adb(
            dir.path(),
            &format!("sed 's/text=\"Hello\"/text=\"Email already registered\"/' '{SAMPLE_XML}'"),
        );

        let snap = capture_within(&adb, "hier-app-text", Duration::from_secs(10))
            .await
            .expect("a dump is a dump");
        let tree = serde_json::to_string(&snap.root).unwrap();
        assert!(tree.contains("Email already registered"), "{tree}");
    }
}
