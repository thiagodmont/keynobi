use crate::models::logcat::ProcessedEntry;

#[derive(Debug, serde::Serialize)]
pub struct StackFrame {
    pub class: String,
    pub method: String,
    pub file: Option<String>,
    pub line: Option<u32>,
}

#[derive(Debug, serde::Serialize)]
pub struct ParsedCrash {
    pub crash_group_id: u64,
    pub package: Option<String>,
    pub exception_type: Option<String>,
    pub message: Option<String>,
    pub frames: Vec<StackFrame>,
    pub caused_by: Vec<CausedBy>,
    pub raw_line_count: usize,
}

#[derive(Debug, serde::Serialize)]
pub struct CausedBy {
    pub exception_type: String,
    pub message: Option<String>,
    pub frames: Vec<StackFrame>,
}

/// Find and parse a crash from a slice of logcat entries.
/// If `crash_group_id` is given, returns that group.
/// Otherwise returns the latest group for `package` (or latest overall).
pub fn find_crash(
    entries: &[ProcessedEntry],
    package: Option<&str>,
    crash_group_id: Option<u64>,
) -> Option<ParsedCrash> {
    let mut groups: std::collections::HashMap<u64, Vec<&ProcessedEntry>> =
        std::collections::HashMap::new();
    for e in entries {
        if let Some(gid) = e.crash_group_id {
            groups.entry(gid).or_default().push(e);
        }
    }
    if groups.is_empty() {
        return None;
    }

    let target_gid = if let Some(gid) = crash_group_id {
        if !groups.contains_key(&gid) {
            return None;
        }
        gid
    } else if let Some(pkg) = package {
        let pkg_lower = pkg.to_lowercase();
        *groups
            .keys()
            .filter(|gid| {
                groups[*gid].iter().any(|e| {
                    e.package
                        .as_deref()
                        .map(|p| p.to_lowercase().contains(&pkg_lower))
                        .unwrap_or(false)
                })
            })
            .max()?
    } else {
        *groups.keys().max()?
    };

    let group = &groups[&target_gid];
    let package = group.iter().find_map(|e| e.package.clone());
    let mut sorted_group: Vec<&&ProcessedEntry> = group.iter().collect();
    sorted_group.sort_by_key(|e| e.id);
    let messages: Vec<&str> = sorted_group.iter().map(|e| e.message.as_str()).collect();
    let parsed = parse_crash_lines(&messages);

    Some(ParsedCrash {
        crash_group_id: target_gid,
        package,
        exception_type: parsed.0,
        message: parsed.1,
        frames: parsed.2,
        caused_by: parsed.3,
        raw_line_count: group.len(),
    })
}

/// Returns (exception_type, message, frames, caused_by)
fn parse_crash_lines(
    lines: &[&str],
) -> (Option<String>, Option<String>, Vec<StackFrame>, Vec<CausedBy>) {
    let mut exception_type: Option<String> = None;
    let mut message: Option<String> = None;
    let mut frames: Vec<StackFrame> = Vec::new();
    let mut caused_by: Vec<CausedBy> = Vec::new();

    let mut i = 0;
    while i < lines.len() {
        let line = lines[i].trim();
        if line.contains("FATAL EXCEPTION") {
            i += 1;
            while i < lines.len() && lines[i].trim().is_empty() {
                i += 1;
            }
            // skip "Process: ..., PID: ..." line if present
            if i < lines.len() && lines[i].trim().starts_with("Process:") {
                i += 1;
            }
            if i < lines.len() {
                let (et, msg) = split_exception_line(lines[i].trim());
                exception_type = Some(et);
                message = msg;
                i += 1;
            }
            break;
        }
        if !line.starts_with("\tat ") && !line.starts_with("at ") && !line.is_empty()
            && !line.starts_with("Process:") && !line.starts_with("PID:")
        {
            let (et, msg) = split_exception_line(line);
            if et.contains('.') {
                exception_type = Some(et);
                message = msg;
                i += 1;
                break;
            }
        }
        i += 1;
    }

    while i < lines.len() {
        let line = lines[i].trim();
        if line.starts_with("at ") {
            let frame_str = line.trim_start_matches("at ").trim();
            frames.push(parse_frame(frame_str));
        } else if line.starts_with("Caused by: ") {
            let cb_rest = &line["Caused by: ".len()..];
            let (cb_et, cb_msg) = split_exception_line(cb_rest);
            let mut cb_frames: Vec<StackFrame> = Vec::new();
            i += 1;
            while i < lines.len() {
                let inner = lines[i].trim();
                if inner.starts_with("at ") {
                    let frame_str = inner.trim_start_matches("at ").trim();
                    cb_frames.push(parse_frame(frame_str));
                } else {
                    break;
                }
                i += 1;
            }
            caused_by.push(CausedBy {
                exception_type: cb_et,
                message: cb_msg,
                frames: cb_frames,
            });
            continue;
        }
        i += 1;
    }

    (exception_type, message, frames, caused_by)
}

fn split_exception_line(line: &str) -> (String, Option<String>) {
    if let Some(pos) = line.find(": ") {
        let et = line[..pos].to_string();
        let msg = line[pos + 2..].to_string();
        (et, if msg.is_empty() { None } else { Some(msg) })
    } else {
        (line.to_string(), None)
    }
}

fn parse_frame(s: &str) -> StackFrame {
    if let Some(paren) = s.find('(') {
        let method_part = &s[..paren];
        let loc_part = s[paren + 1..].trim_end_matches(')');

        let (class, method) = if let Some(dot) = method_part.rfind('.') {
            (method_part[..dot].to_string(), method_part[dot + 1..].to_string())
        } else {
            (method_part.to_string(), String::new())
        };

        let (file, line) = parse_location(loc_part);
        StackFrame { class, method, file, line }
    } else {
        StackFrame { class: s.to_string(), method: String::new(), file: None, line: None }
    }
}

fn parse_location(loc: &str) -> (Option<String>, Option<u32>) {
    if loc == "Unknown Source" || loc == "Native Method" || loc.is_empty() {
        return (None, None);
    }
    if let Some(colon) = loc.rfind(':') {
        let file = loc[..colon].to_string();
        let line = loc[colon + 1..].parse::<u32>().ok();
        (Some(file), line)
    } else {
        (Some(loc.to_string()), None)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::logcat::{LogcatLevel, LogcatKind, EntryCategory};

    fn make_entry(id: u64, msg: &str, gid: Option<u64>, pkg: Option<&str>) -> ProcessedEntry {
        ProcessedEntry {
            id,
            timestamp: "2026-01-01 00:00:00.000".into(),
            pid: 1234,
            tid: 1234,
            level: LogcatLevel::Error,
            tag: "AndroidRuntime".into(),
            message: msg.to_string(),
            package: pkg.map(|s| s.to_string()),
            kind: LogcatKind::Normal,
            is_crash: true,
            flags: 1,
            category: EntryCategory::General,
            crash_group_id: gid,
            json_body: None,
        }
    }

    #[test]
    fn parse_frame_with_file_and_line() {
        let f = parse_frame("com.example.MyActivity.onCreate(MyActivity.kt:42)");
        assert_eq!(f.class, "com.example.MyActivity");
        assert_eq!(f.method, "onCreate");
        assert_eq!(f.file.as_deref(), Some("MyActivity.kt"));
        assert_eq!(f.line, Some(42));
    }

    #[test]
    fn parse_frame_native_method() {
        let f = parse_frame("com.example.Foo.bar(Native Method)");
        assert_eq!(f.class, "com.example.Foo");
        assert_eq!(f.method, "bar");
        assert!(f.file.is_none());
        assert!(f.line.is_none());
    }

    #[test]
    fn parse_crash_lines_extracts_exception_and_frames() {
        let lines = vec![
            "FATAL EXCEPTION: main",
            "Process: com.example.app, PID: 12345",
            "java.lang.NullPointerException: Attempt to invoke virtual method",
            "\tat com.example.MyActivity.onCreate(MyActivity.kt:42)",
            "\tat android.app.Activity.performCreate(Activity.java:8051)",
        ];
        let (et, msg, frames, caused_by) = parse_crash_lines(&lines);
        assert_eq!(et.as_deref(), Some("java.lang.NullPointerException"));
        assert!(msg.as_deref().unwrap().contains("Attempt"));
        assert_eq!(frames.len(), 2);
        assert_eq!(frames[0].class, "com.example.MyActivity");
        assert!(caused_by.is_empty());
    }

    #[test]
    fn parse_crash_lines_extracts_caused_by_chain() {
        let lines = vec![
            "FATAL EXCEPTION: main",
            "java.lang.RuntimeException: Wrapper",
            "\tat com.example.Foo.bar(Foo.kt:10)",
            "Caused by: java.io.IOException: File not found",
            "\tat com.example.Io.read(Io.kt:5)",
        ];
        let (et, _, frames, caused_by) = parse_crash_lines(&lines);
        assert_eq!(et.as_deref(), Some("java.lang.RuntimeException"));
        assert_eq!(frames.len(), 1);
        assert_eq!(caused_by.len(), 1);
        assert_eq!(caused_by[0].exception_type, "java.io.IOException");
        assert_eq!(caused_by[0].frames.len(), 1);
    }

    #[test]
    fn find_crash_returns_none_for_empty_buffer() {
        let entries: Vec<ProcessedEntry> = Vec::new();
        let result = find_crash(&entries, None, None);
        assert!(result.is_none());
    }

    #[test]
    fn find_crash_returns_latest_group() {
        let entries = vec![
            make_entry(1, "FATAL EXCEPTION: main", Some(1), Some("com.example.app")),
            make_entry(2, "java.lang.NPE: old", Some(1), Some("com.example.app")),
            make_entry(3, "FATAL EXCEPTION: main", Some(2), Some("com.example.app")),
            make_entry(4, "java.lang.IllegalState: newer", Some(2), Some("com.example.app")),
        ];
        let result = find_crash(&entries, None, None).unwrap();
        assert_eq!(result.crash_group_id, 2);
    }

    #[test]
    fn find_crash_by_specific_group_id() {
        let entries = vec![
            make_entry(1, "FATAL EXCEPTION: main", Some(1), Some("com.example.app")),
            make_entry(2, "java.lang.NPE: old", Some(1), Some("com.example.app")),
            make_entry(3, "FATAL EXCEPTION: main", Some(2), Some("com.example.app")),
        ];
        let result = find_crash(&entries, None, Some(1)).unwrap();
        assert_eq!(result.crash_group_id, 1);
    }

    #[test]
    fn find_crash_filters_by_package() {
        let entries = vec![
            make_entry(1, "FATAL EXCEPTION: main", Some(1), Some("com.other.app")),
            make_entry(2, "java.lang.NPE", Some(1), Some("com.other.app")),
            make_entry(3, "FATAL EXCEPTION: main", Some(2), Some("com.example.app")),
            make_entry(4, "java.lang.ISE", Some(2), Some("com.example.app")),
        ];
        let result = find_crash(&entries, Some("com.example.app"), None).unwrap();
        assert_eq!(result.crash_group_id, 2);
        assert_eq!(result.package.as_deref(), Some("com.example.app"));
    }
}
