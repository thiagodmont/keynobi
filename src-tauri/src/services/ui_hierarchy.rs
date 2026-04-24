//! Capture UI Automator hierarchy XML from a device via ADB and parse it.

use crate::models::ui_hierarchy::{UiHierarchySnapshot, UiLayoutContext};
use crate::services::ui_hierarchy_parse::{
    compute_screen_hash, count_interactive_nodes, parse_hierarchy_xml, ParseOutcome,
};
use base64::{engine::general_purpose::STANDARD as BASE64, Engine as _};
use chrono::Utc;
use std::path::{Path, PathBuf};
use std::time::Duration;
use tokio::process::Command;
use tokio::time::timeout;

/// Maximum raw XML bytes read from adb (host memory bound).
pub const MAX_XML_BYTES: usize = 4 * 1024 * 1024;
/// Maximum screenshot PNG bytes accepted (8 MiB). Larger captures are silently dropped.
const MAX_SCREENSHOT_BYTES: usize = 8 * 1024 * 1024;
/// Timeout for `screencap -p` (slower than a shell probe; some emulators can be sluggish).
const SCREENSHOT_TIMEOUT: Duration = Duration::from_secs(20);
/// Wall-clock limit for a single adb dump attempt.
pub const DUMP_TIMEOUT: Duration = Duration::from_secs(25);
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

async fn try_exec_out_uiautomator_dump(
    adb: &PathBuf,
    serial: &str,
    compressed: bool,
) -> Option<String> {
    let args: &[&str] = if compressed {
        &[
            "-s",
            serial,
            "exec-out",
            "uiautomator",
            "dump",
            "--compressed",
            "/dev/tty",
        ]
    } else {
        &["-s", serial, "exec-out", "uiautomator", "dump", "/dev/tty"]
    };
    let fut = Command::new(adb).args(args).output();
    let Ok(Ok(out)) = timeout(DUMP_TIMEOUT, fut).await else {
        return None;
    };
    if !out.status.success() {
        return None;
    }
    let raw = strip_ui_automator_noise(&out.stdout);
    if raw.trim().is_empty() || !raw.trim_start().starts_with('<') {
        return None;
    }
    Some(raw)
}

/// Run `uiautomator dump` and return UTF-8 XML (may be truncated to [`MAX_XML_BYTES`]).
/// Tries `--compressed` first (smaller / faster on supported builds), then plain dump.
/// The third tuple element lists every adb invocation attempted (for debugging).
pub async fn dump_hierarchy_xml(
    adb: &PathBuf,
    serial: &str,
) -> Result<(String, bool, Vec<String>), String> {
    let mut command_log = Vec::new();

    // 1a) exec-out compressed (API-dependent; falls through if unsupported or empty).
    command_log.push(format_adb_command(
        adb,
        serial,
        &[
            "exec-out",
            "uiautomator",
            "dump",
            "--compressed",
            "/dev/tty",
        ],
    ));
    if let Some(raw) = try_exec_out_uiautomator_dump(adb, serial, true).await {
        let (s, truncated) = truncate_utf8(raw, MAX_XML_BYTES);
        return Ok((s, truncated, command_log));
    }

    // 1b) exec-out without --compressed
    command_log.push(format_adb_command(
        adb,
        serial,
        &["exec-out", "uiautomator", "dump", "/dev/tty"],
    ));
    if let Some(raw) = try_exec_out_uiautomator_dump(adb, serial, false).await {
        let (s, truncated) = truncate_utf8(raw, MAX_XML_BYTES);
        return Ok((s, truncated, command_log));
    }

    // 2) Fallback: dump to default path on device, then cat to host (compressed then plain).
    for compressed in [true, false] {
        let shell_args: Vec<String> = if compressed {
            vec![
                "-s".into(),
                serial.into(),
                "shell".into(),
                "uiautomator".into(),
                "dump".into(),
                "--compressed".into(),
            ]
        } else {
            vec![
                "-s".into(),
                serial.into(),
                "shell".into(),
                "uiautomator".into(),
                "dump".into(),
            ]
        };
        let cmd_line: Vec<&str> = shell_args.iter().map(|s| s.as_str()).collect();
        command_log.push(format_adb_command(adb, serial, &cmd_line[2..]));

        let dump_default = Command::new(adb).args(&shell_args).output();
        let dump_ok = match timeout(DUMP_TIMEOUT, dump_default).await {
            Ok(Ok(o)) => o.status.success(),
            _ => false,
        };
        if !dump_ok {
            continue;
        }

        for path in [
            "/sdcard/window_dump.xml",
            "/storage/emulated/0/window_dump.xml",
        ] {
            command_log.push(format_adb_command(adb, serial, &["exec-out", "cat", path]));
            let cat = Command::new(adb)
                .args(["-s", serial, "exec-out", "cat", path])
                .output();

            match timeout(DUMP_TIMEOUT, cat).await {
                Ok(Ok(out)) if out.status.success() && !out.stdout.is_empty() => {
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

    Err(
        "uiautomator dump failed (exec-out and shell dump, compressed and plain). Is the device online?"
            .to_string(),
    )
}

/// Official shell excerpts: window focus, display, logical size / density.
pub async fn probe_layout_context(adb: &PathBuf, serial: &str) -> (UiLayoutContext, Vec<String>) {
    let mut command_log = Vec::new();

    let w_args = ["-s", serial, "shell", "dumpsys", "window", "windows"];
    command_log.push(format_adb_command(adb, serial, &w_args[2..]));
    let d_args = ["-s", serial, "shell", "dumpsys", "display"];
    command_log.push(format_adb_command(adb, serial, &d_args[2..]));
    let sz_args = ["-s", serial, "shell", "wm", "size"];
    command_log.push(format_adb_command(adb, serial, &sz_args[2..]));
    let den_args = ["-s", serial, "shell", "wm", "density"];
    command_log.push(format_adb_command(adb, serial, &den_args[2..]));

    let win_fut = Command::new(adb).args(w_args).output();
    let disp_fut = Command::new(adb).args(d_args).output();
    let sz_fut = Command::new(adb).args(sz_args).output();
    let den_fut = Command::new(adb).args(den_args).output();

    let probe = async { tokio::join!(win_fut, disp_fut, sz_fut, den_fut) };
    let (win_o, disp_o, sz_o, den_o) = match timeout(LAYOUT_PROBE_TIMEOUT, probe).await {
        Ok(quads) => quads,
        Err(_) => {
            let err = || {
                Err(std::io::Error::new(
                    std::io::ErrorKind::TimedOut,
                    "layout probe timeout",
                ))
            };
            (err(), err(), err(), err())
        }
    };

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
pub async fn capture_screenshot_b64(adb: &PathBuf, serial: &str) -> Option<String> {
    let cmd_fut = Command::new(adb)
        .args(["-s", serial, "exec-out", "screencap", "-p"])
        .output();
    let out = match timeout(SCREENSHOT_TIMEOUT, cmd_fut).await {
        Ok(Ok(o)) => o,
        _ => return None,
    };
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
pub async fn capture_ui_hierarchy_snapshot(
    adb: &PathBuf,
    serial: &str,
) -> Result<UiHierarchySnapshot, String> {
    let mut command_log = vec![format_adb_command(
        adb,
        serial,
        &["shell", "dumpsys", "activity", "activities"],
    )];
    let fg = probe_foreground_activity(adb, serial).await;

    let (layout_ctx, mut layout_cmds) = probe_layout_context(adb, serial).await;
    command_log.append(&mut layout_cmds);

    let (xml, xml_truncated, mut dump_cmds) = dump_hierarchy_xml(adb, serial).await?;
    command_log.append(&mut dump_cmds);

    // Screenshot is best-effort and runs concurrently with nothing (already sequential here).
    command_log.push(format_adb_command(
        adb,
        serial,
        &["exec-out", "screencap", "-p"],
    ));
    let screenshot_b64 = capture_screenshot_b64(adb, serial).await;

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
pub async fn probe_foreground_activity(adb: &PathBuf, serial: &str) -> Option<String> {
    let out = Command::new(adb)
        .args(["-s", serial, "shell", "dumpsys", "activity", "activities"])
        .output();

    let Ok(Ok(output)) = timeout(DUMP_TIMEOUT, out).await else {
        return None;
    };
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
}
