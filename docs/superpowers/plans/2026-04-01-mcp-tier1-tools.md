# MCP Tier 1 Tools Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add four MCP tools — `get_crash_stack_trace`, `restart_app`, `get_app_runtime_state`, and `get_build_config` — to the Android Dev Companion server.

**Architecture:** Three new service modules (`crash_inspector.rs`, `app_inspector.rs`, `build_inspector.rs`) hold all logic; `mcp_server.rs` holds thin `#[tool]` handlers that call into them. Each service is pure logic (no Tauri or rmcp imports) and ships with inline unit tests.

**Tech Stack:** Rust, Tokio async, `rmcp` for MCP tool macros, `serde_json::json!` for output, `regex` crate for Gradle parsing, inline `#[cfg(test)]` modules with `#[tokio::test]`.

---

## File Map

| Action | Path | Responsibility |
|--------|------|----------------|
| Create | `src-tauri/src/services/crash_inspector.rs` | Stack trace parsing from logcat entries |
| Create | `src-tauri/src/services/app_inspector.rs` | `ps` parsing, runtime state, restart logic |
| Create | `src-tauri/src/services/build_inspector.rs` | Gradle DSL parsing |
| Modify | `src-tauri/src/services/mod.rs` | Add 3 `pub mod` lines |
| Modify | `src-tauri/src/services/mcp_server.rs` | Param structs + 4 `#[tool]` handler methods |

---

## Task 1: `crash_inspector` — parsing logic + tests

**Files:**
- Create: `src-tauri/src/services/crash_inspector.rs`

- [ ] **Step 1.1: Write failing tests**

Add to a new file `src-tauri/src/services/crash_inspector.rs`:

```rust
use crate::models::logcat::{LogcatLevel, ProcessedEntry, LogcatKind, EntryCategory};

// ── Public types ──────────────────────────────────────────────────────────────

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

// ── Public API ────────────────────────────────────────────────────────────────

/// Find and parse a crash from a slice of logcat entries.
/// If `crash_group_id` is given, returns that group.
/// Otherwise returns the latest group for `package` (or latest overall).
pub fn find_crash(
    entries: &[ProcessedEntry],
    package: Option<&str>,
    crash_group_id: Option<u64>,
) -> Option<ParsedCrash> {
    // collect groups
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

    // pick group
    let target_gid = if let Some(gid) = crash_group_id {
        if !groups.contains_key(&gid) {
            return None;
        }
        gid
    } else {
        // latest group matching package filter
        let mut candidates: Vec<u64> = groups.keys().copied().collect();
        candidates.sort_unstable();
        if let Some(pkg) = package {
            let pkg_lower = pkg.to_lowercase();
            candidates
                .into_iter()
                .rev()
                .find(|gid| {
                    groups[gid].iter().any(|e| {
                        e.package
                            .as_deref()
                            .map(|p| p.to_lowercase().contains(&pkg_lower))
                            .unwrap_or(false)
                    })
                })?
        } else {
            *candidates.last()?
        }
    };

    let group = &groups[&target_gid];
    let package = group
        .iter()
        .find_map(|e| e.package.clone());
    let messages: Vec<&str> = group.iter().map(|e| e.message.as_str()).collect();
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

// ── Parsing helpers ────────────────────────────────────────────────────────────

/// Returns (exception_type, message, frames, caused_by)
fn parse_crash_lines(
    lines: &[&str],
) -> (Option<String>, Option<String>, Vec<StackFrame>, Vec<CausedBy>) {
    let mut exception_type: Option<String> = None;
    let mut message: Option<String> = None;
    let mut frames: Vec<StackFrame> = Vec::new();
    let mut caused_by: Vec<CausedBy> = Vec::new();

    let mut i = 0;
    // Skip until FATAL EXCEPTION or first exception line
    while i < lines.len() {
        let line = lines[i].trim();
        if line.contains("FATAL EXCEPTION") {
            i += 1;
            // Next non-empty line is "ExceptionClass: message" or just "ExceptionClass"
            while i < lines.len() && lines[i].trim().is_empty() {
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
        // No FATAL EXCEPTION header — try to treat first non-at line as exception
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

    // Collect frames and caused_by chains
    while i < lines.len() {
        let line = lines[i].trim();
        if line.starts_with("\tat ") || line.starts_with("at ") {
            let frame_str = line.trim_start_matches("at ").trim_start_matches('\t').trim_start_matches("at ");
            frames.push(parse_frame(frame_str));
        } else if line.starts_with("Caused by: ") {
            let cb_rest = &line["Caused by: ".len()..];
            let (cb_et, cb_msg) = split_exception_line(cb_rest);
            let mut cb_frames: Vec<StackFrame> = Vec::new();
            i += 1;
            while i < lines.len() {
                let inner = lines[i].trim();
                if inner.starts_with("\tat ") || inner.starts_with("at ") {
                    let frame_str = inner.trim_start_matches("at ").trim_start_matches('\t').trim_start_matches("at ");
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
    // Format: com.example.MyClass.method(File.kt:42)
    // or:     com.example.MyClass.method(Unknown Source)
    // or:     com.example.MyClass.method(Native Method)
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

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

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
```

- [ ] **Step 1.2: Run tests to verify they fail**

```bash
cd /Users/thiagodmont/Documents/projects/android-ide/src-tauri
cargo test crash_inspector 2>&1 | head -30
```

Expected: compile error — module not registered yet (or file not in module tree).

- [ ] **Step 1.3: Register module in `mod.rs`**

Edit `src-tauri/src/services/mod.rs` — add after the last `pub mod` line:

```rust
pub mod crash_inspector;
```

- [ ] **Step 1.4: Run tests again — should compile and pass**

```bash
cd /Users/thiagodmont/Documents/projects/android-ide/src-tauri
cargo test crash_inspector 2>&1
```

Expected: all 7 tests pass, no warnings.

- [ ] **Step 1.5: Commit**

```bash
cd /Users/thiagodmont/Documents/projects/android-ide
git add src-tauri/src/services/crash_inspector.rs src-tauri/src/services/mod.rs
git commit -m "feat(mcp): add crash_inspector service with stack trace parsing"
```

---

## Task 2: `build_inspector` — Gradle config parsing + tests

**Files:**
- Create: `src-tauri/src/services/build_inspector.rs`

- [ ] **Step 2.1: Write failing tests**

Create `src-tauri/src/services/build_inspector.rs`:

```rust
use std::path::{Path, PathBuf};

// ── Public types ──────────────────────────────────────────────────────────────

#[derive(Debug, serde::Serialize)]
pub struct BuildConfig {
    pub module: String,
    pub file: String,
    pub compile_sdk: Option<i64>,
    pub min_sdk: Option<i64>,
    pub target_sdk: Option<i64>,
    pub application_id: Option<String>,
    pub namespace: Option<String>,
    pub build_types: Vec<BuildType>,
    pub product_flavors: Vec<ProductFlavor>,
}

#[derive(Debug, serde::Serialize)]
pub struct BuildType {
    pub name: String,
    pub minify_enabled: Option<bool>,
    pub debuggable: Option<bool>,
}

#[derive(Debug, serde::Serialize)]
pub struct ProductFlavor {
    pub name: String,
    pub dimension: Option<String>,
}

// ── Public API ────────────────────────────────────────────────────────────────

pub fn parse_build_config(
    gradle_root: &Path,
    module: &str,
) -> Result<BuildConfig, String> {
    let module_dir = gradle_root.join(module);
    if !module_dir.is_dir() {
        return Err(format!(
            "Module '{}' not found under {}",
            module,
            gradle_root.display()
        ));
    }

    let (file_path, content) = read_build_gradle(&module_dir)?;
    let relative = file_path
        .strip_prefix(gradle_root)
        .unwrap_or(&file_path)
        .to_string_lossy()
        .to_string();

    Ok(BuildConfig {
        module: module.to_string(),
        file: relative,
        compile_sdk: extract_int_value(&content, &["compileSdk", "compileSdkVersion"]),
        min_sdk: extract_int_value(&content, &["minSdk", "minSdkVersion"]),
        target_sdk: extract_int_value(&content, &["targetSdk", "targetSdkVersion"]),
        application_id: extract_string_value(&content, "applicationId"),
        namespace: extract_string_value(&content, "namespace"),
        build_types: parse_build_types(&content),
        product_flavors: parse_product_flavors(&content),
    })
}

// ── Internal ──────────────────────────────────────────────────────────────────

fn read_build_gradle(module_dir: &Path) -> Result<(PathBuf, String), String> {
    for name in &["build.gradle.kts", "build.gradle"] {
        let p = module_dir.join(name);
        if p.is_file() {
            let content = std::fs::read_to_string(&p)
                .map_err(|e| format!("Failed to read {}: {e}", p.display()))?;
            return Ok((p, content));
        }
    }
    Err(format!(
        "No build.gradle(.kts) found in {}",
        module_dir.display()
    ))
}

/// Extract an integer value for any of the given key names.
/// Matches: `compileSdk = 35`, `compileSdkVersion(35)`, `compileSdkVersion = 35`
fn extract_int_value(content: &str, keys: &[&str]) -> Option<i64> {
    for key in keys {
        for line in content.lines() {
            let trimmed = line.trim();
            // KTS style: key = 35  or  key(35)
            if trimmed.starts_with(key) {
                let rest = trimmed[key.len()..].trim_start();
                let digits: String = rest
                    .trim_start_matches(|c| c == '=' || c == '(' || c == ' ')
                    .chars()
                    .take_while(|c| c.is_ascii_digit())
                    .collect();
                if let Ok(n) = digits.parse::<i64>() {
                    return Some(n);
                }
            }
        }
    }
    None
}

/// Extract a quoted string value: `applicationId = "com.example"` or `applicationId "com.example"`
fn extract_string_value(content: &str, key: &str) -> Option<String> {
    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with(key) {
            let rest = trimmed[key.len()..].trim_start();
            let rest = rest.trim_start_matches(|c| c == '=' || c == ' ');
            // find quoted string
            if let Some(start) = rest.find('"') {
                if let Some(end) = rest[start + 1..].find('"') {
                    return Some(rest[start + 1..start + 1 + end].to_string());
                }
            }
        }
    }
    None
}

/// Extract the content inside a top-level named block: `buildTypes { ... }`.
/// Returns the inner content (everything between the first `{` and matching `}`).
fn extract_block<'a>(content: &'a str, block_name: &str) -> Option<&'a str> {
    let marker = format!("{} {{", block_name);
    let alt_marker = format!("{}{{", block_name);
    let start_pos = content
        .find(&marker)
        .or_else(|| content.find(&alt_marker))?;
    let brace_start = content[start_pos..].find('{')? + start_pos;
    let inner_start = brace_start + 1;

    let mut depth = 1usize;
    let mut pos = inner_start;
    for c in content[inner_start..].chars() {
        match c {
            '{' => depth += 1,
            '}' => {
                depth -= 1;
                if depth == 0 {
                    return Some(&content[inner_start..pos]);
                }
            }
            _ => {}
        }
        pos += c.len_utf8();
    }
    None
}

fn parse_build_types(content: &str) -> Vec<BuildType> {
    let block = match extract_block(content, "buildTypes") {
        Some(b) => b,
        None => return Vec::new(),
    };

    // Find named sub-blocks: `debug { ... }` or `create("release") { ... }`
    parse_named_blocks(block)
        .into_iter()
        .map(|(name, inner)| BuildType {
            name,
            minify_enabled: extract_bool_value(&inner, &["isMinifyEnabled", "minifyEnabled"]),
            debuggable: extract_bool_value(&inner, &["isDebuggable", "debuggable"]),
        })
        .collect()
}

fn parse_product_flavors(content: &str) -> Vec<ProductFlavor> {
    let block = match extract_block(content, "productFlavors") {
        Some(b) => b,
        None => return Vec::new(),
    };

    parse_named_blocks(block)
        .into_iter()
        .map(|(name, inner)| ProductFlavor {
            name,
            dimension: extract_string_value(&inner, "dimension"),
        })
        .collect()
}

/// Parse immediate child blocks from a block body.
/// Handles: `debug { ... }` and `create("release") { ... }`
fn parse_named_blocks(block: &str) -> Vec<(String, String)> {
    let mut result = Vec::new();
    let mut pos = 0;
    let bytes = block.as_bytes();

    while pos < bytes.len() {
        // Skip whitespace and newlines
        while pos < bytes.len() && bytes[pos].is_ascii_whitespace() {
            pos += 1;
        }
        // Find the next identifier or create(...) call
        let word_start = pos;
        while pos < bytes.len()
            && (bytes[pos].is_ascii_alphanumeric() || bytes[pos] == b'_')
        {
            pos += 1;
        }
        if pos == word_start {
            // Not an identifier — skip to next line
            while pos < bytes.len() && bytes[pos] != b'\n' {
                pos += 1;
            }
            continue;
        }
        let word = &block[word_start..pos];

        // Skip whitespace
        while pos < bytes.len() && bytes[pos] == b' ' {
            pos += 1;
        }

        // Check for create("name") pattern (KTS)
        let name = if pos < bytes.len() && bytes[pos] == b'(' {
            // find quoted name inside parens
            if let Some(q_start) = block[pos..].find('"') {
                let abs_start = pos + q_start + 1;
                if let Some(q_end) = block[abs_start..].find('"') {
                    let n = block[abs_start..abs_start + q_end].to_string();
                    // advance pos past closing paren
                    pos = abs_start + q_end + 1;
                    while pos < bytes.len() && bytes[pos] != b'{' {
                        pos += 1;
                    }
                    n
                } else {
                    word.to_string()
                }
            } else {
                word.to_string()
            }
        } else {
            // skip to opening brace
            while pos < bytes.len() && bytes[pos] != b'{' && bytes[pos] != b'\n' {
                pos += 1;
            }
            if pos >= bytes.len() || bytes[pos] == b'\n' {
                continue;
            }
            word.to_string()
        };

        if pos >= bytes.len() || bytes[pos] != b'{' {
            continue;
        }

        // Extract inner block
        let inner_start = pos + 1;
        let mut depth = 1usize;
        pos = inner_start;
        for c in block[inner_start..].chars() {
            match c {
                '{' => depth += 1,
                '}' => {
                    depth -= 1;
                    if depth == 0 {
                        break;
                    }
                }
                _ => {}
            }
            pos += c.len_utf8();
        }
        let inner = &block[inner_start..pos];
        pos += 1; // skip closing `}`

        // filter out Gradle DSL keywords that aren't type/flavor names
        if !matches!(name.as_str(), "getByName" | "maybeCreate" | "all" | "configureEach") {
            result.push((name, inner.to_string()));
        }
    }
    result
}

fn extract_bool_value(content: &str, keys: &[&str]) -> Option<bool> {
    for key in keys {
        for line in content.lines() {
            let trimmed = line.trim();
            if trimmed.starts_with(key) {
                let rest = trimmed[key.len()..].trim();
                let rest = rest.trim_start_matches(|c| c == '=' || c == ' ');
                if rest.starts_with("true") {
                    return Some(true);
                }
                if rest.starts_with("false") {
                    return Some(false);
                }
            }
        }
    }
    None
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    fn make_project(module: &str, content: &str) -> TempDir {
        let dir = tempfile::tempdir().unwrap();
        let mod_dir = dir.path().join(module);
        fs::create_dir_all(&mod_dir).unwrap();
        fs::write(mod_dir.join("build.gradle.kts"), content).unwrap();
        dir
    }

    #[test]
    fn extract_sdk_levels_kts() {
        let dir = make_project(
            "app",
            r#"
android {
    compileSdk = 35
    defaultConfig {
        applicationId = "com.example.app"
        minSdk = 24
        targetSdk = 35
    }
}
"#,
        );
        let cfg = parse_build_config(dir.path(), "app").unwrap();
        assert_eq!(cfg.compile_sdk, Some(35));
        assert_eq!(cfg.min_sdk, Some(24));
        assert_eq!(cfg.target_sdk, Some(35));
        assert_eq!(cfg.application_id.as_deref(), Some("com.example.app"));
    }

    #[test]
    fn extract_sdk_levels_groovy() {
        let dir = tempfile::tempdir().unwrap();
        let mod_dir = dir.path().join("app");
        fs::create_dir_all(&mod_dir).unwrap();
        fs::write(
            mod_dir.join("build.gradle"),
            r#"
android {
    compileSdkVersion 33
    defaultConfig {
        applicationId "com.example.groovy"
        minSdkVersion 21
        targetSdkVersion 33
    }
}
"#,
        )
        .unwrap();
        let cfg = parse_build_config(dir.path(), "app").unwrap();
        assert_eq!(cfg.compile_sdk, Some(33));
        assert_eq!(cfg.min_sdk, Some(21));
        assert_eq!(cfg.application_id.as_deref(), Some("com.example.groovy"));
    }

    #[test]
    fn parse_build_types_groovy_style() {
        let dir = make_project(
            "app",
            r#"
android {
    buildTypes {
        release {
            minifyEnabled true
            debuggable false
        }
        debug {
            minifyEnabled false
            debuggable true
        }
    }
}
"#,
        );
        let cfg = parse_build_config(dir.path(), "app").unwrap();
        let release = cfg.build_types.iter().find(|b| b.name == "release").unwrap();
        assert_eq!(release.minify_enabled, Some(true));
        assert_eq!(release.debuggable, Some(false));
        let debug = cfg.build_types.iter().find(|b| b.name == "debug").unwrap();
        assert_eq!(debug.minify_enabled, Some(false));
        assert_eq!(debug.debuggable, Some(true));
    }

    #[test]
    fn parse_build_types_kts_style() {
        let dir = make_project(
            "app",
            r#"
android {
    buildTypes {
        release {
            isMinifyEnabled = true
            isDebuggable = false
        }
        debug {
            isMinifyEnabled = false
            isDebuggable = true
        }
    }
}
"#,
        );
        let cfg = parse_build_config(dir.path(), "app").unwrap();
        let release = cfg.build_types.iter().find(|b| b.name == "release").unwrap();
        assert_eq!(release.minify_enabled, Some(true));
    }

    #[test]
    fn parse_product_flavors() {
        let dir = make_project(
            "app",
            r#"
android {
    flavorDimensions "tier"
    productFlavors {
        free {
            dimension "tier"
        }
        paid {
            dimension "tier"
        }
    }
}
"#,
        );
        let cfg = parse_build_config(dir.path(), "app").unwrap();
        assert_eq!(cfg.product_flavors.len(), 2);
        let free = cfg.product_flavors.iter().find(|f| f.name == "free").unwrap();
        assert_eq!(free.dimension.as_deref(), Some("tier"));
    }

    #[test]
    fn missing_module_returns_error() {
        let dir = tempfile::tempdir().unwrap();
        let result = parse_build_config(dir.path(), "nonexistent");
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("not found"));
    }

    #[test]
    fn no_gradle_file_returns_error() {
        let dir = tempfile::tempdir().unwrap();
        fs::create_dir_all(dir.path().join("app")).unwrap();
        let result = parse_build_config(dir.path(), "app");
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("No build.gradle"));
    }

    #[test]
    fn missing_fields_return_null() {
        let dir = make_project("app", "android { }");
        let cfg = parse_build_config(dir.path(), "app").unwrap();
        assert!(cfg.compile_sdk.is_none());
        assert!(cfg.min_sdk.is_none());
        assert!(cfg.application_id.is_none());
        assert!(cfg.build_types.is_empty());
        assert!(cfg.product_flavors.is_empty());
    }
}
```

- [ ] **Step 2.2: Run tests to verify they fail**

```bash
cd /Users/thiagodmont/Documents/projects/android-ide/src-tauri
cargo test build_inspector 2>&1 | head -20
```

Expected: compile error — module not registered.

- [ ] **Step 2.3: Register module in `mod.rs`**

Edit `src-tauri/src/services/mod.rs` — add:

```rust
pub mod build_inspector;
```

- [ ] **Step 2.4: Add `tempfile` to dev-dependencies if not present**

Check `src-tauri/Cargo.toml`:

```bash
grep -n "tempfile" /Users/thiagodmont/Documents/projects/android-ide/src-tauri/Cargo.toml
```

If not found, add to `[dev-dependencies]`:

```toml
tempfile = "3"
```

- [ ] **Step 2.5: Run tests — should pass**

```bash
cd /Users/thiagodmont/Documents/projects/android-ide/src-tauri
cargo test build_inspector 2>&1
```

Expected: all 8 tests pass.

- [ ] **Step 2.6: Commit**

```bash
cd /Users/thiagodmont/Documents/projects/android-ide
git add src-tauri/src/services/build_inspector.rs src-tauri/src/services/mod.rs src-tauri/Cargo.toml
git commit -m "feat(mcp): add build_inspector service with Gradle DSL parsing"
```

---

## Task 3: `app_inspector` — runtime state + restart logic + tests

**Files:**
- Create: `src-tauri/src/services/app_inspector.rs`

- [ ] **Step 3.1: Write the service**

Create `src-tauri/src/services/app_inspector.rs`:

```rust
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
    // Step 1: find PIDs for package
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

    // Step 2: get thread count and RSS for each PID concurrently
    let mut processes = Vec::new();
    for (pid, name) in &pids {
        let (threads, rss) =
            tokio::join!(get_thread_count(adb, device_serial, *pid), get_rss_kb(adb, device_serial, *pid));
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
    // Step 1: stop
    if cold {
        run_adb_shell(adb, device_serial, &["pm", "clear", package]).await?;
    } else {
        run_adb_shell(adb, device_serial, &["am", "force-stop", package]).await?;
    }

    // Step 2: resolve launcher activity
    let activity = resolve_launcher_activity(adb, device_serial, package).await?;

    // Step 3: launch
    let start = std::time::Instant::now();
    run_adb_shell(adb, device_serial, &["am", "start", "-n", &activity]).await?;

    // Step 4: poll logcat for "Displayed" line (5s timeout via adb logcat -T 1)
    let display_time_ms =
        wait_for_displayed(adb, device_serial, package, start).await;

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
            // match exact package or sub-process like com.example.app:push
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
    // count lines minus header
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
    let component = output
        .lines()
        .find(|l| l.contains('/'))
        .map(|l| l.trim().to_string())
        .ok_or_else(|| {
            format!("Could not resolve launcher activity for package '{package}'")
        })?;

    Ok(component)
}

async fn wait_for_displayed(
    adb: &PathBuf,
    device_serial: &str,
    package: &str,
    start: std::time::Instant,
) -> Option<u64> {
    // Use `adb logcat -d ActivityManager:I *:S` to get recent ActivityManager lines
    // and look for "Displayed <package>" — poll up to 10s.
    let deadline = std::time::Duration::from_secs(10);
    loop {
        if start.elapsed() > deadline {
            return None;
        }
        tokio::time::sleep(std::time::Duration::from_millis(300)).await;

        let output = adb_cmd(
            adb,
            Some(device_serial),
            &["logcat", "-d", "-s", "ActivityManager:I"],
        )
        .await
        .unwrap_or_default();

        if let Some(ms) = parse_displayed_time(&output, package) {
            return Some(ms);
        }
    }
}

pub fn parse_displayed_time(logcat_output: &str, package: &str) -> Option<u64> {
    // Look for lines like: "I ActivityManager: Displayed com.example.app/.MainActivity: +850ms"
    for line in logcat_output.lines().rev() {
        if line.contains("Displayed") && line.contains(package) {
            // parse "+850ms" or "+1s234ms"
            if let Some(ms) = extract_display_ms(line) {
                return Some(ms);
            }
        }
    }
    None
}

fn extract_display_ms(line: &str) -> Option<u64> {
    // Find "+NNNms" pattern
    let plus_pos = line.rfind('+')?;
    let rest = &line[plus_pos + 1..];
    // Handle "1s234ms" or "850ms"
    if let Some(s_pos) = rest.find('s') {
        let secs: u64 = rest[..s_pos].parse().ok()?;
        let ms_str = rest[s_pos + 1..].trim_end_matches("ms").trim_end_matches(' ');
        let ms: u64 = ms_str.parse().unwrap_or(0);
        Some(secs * 1000 + ms)
    } else if let Some(ms_str) = rest.strip_suffix("ms") {
        ms_str.parse().ok()
    } else {
        None
    }
}

async fn run_adb_shell(
    adb: &PathBuf,
    device_serial: &str,
    args: &[&str],
) -> Result<String, String> {
    let mut full_args = vec!["-s", device_serial, "shell"];
    full_args.extend_from_slice(args);
    adb_cmd(adb, Some(device_serial), {
        let mut a = vec!["shell"];
        a.extend_from_slice(args);
        // rebuild properly
        &[]
    })
    .await
    .map_err(|e| e)?;

    // Inline the call directly:
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
        if !stderr.is_empty() {
            return Err(stderr);
        }
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
```

- [ ] **Step 3.2: Register module in `mod.rs`**

Edit `src-tauri/src/services/mod.rs` — add:

```rust
pub mod app_inspector;
```

- [ ] **Step 3.3: Run tests**

```bash
cd /Users/thiagodmont/Documents/projects/android-ide/src-tauri
cargo test app_inspector 2>&1
```

Expected: all 7 tests pass with no warnings.

- [ ] **Step 3.4: Commit**

```bash
cd /Users/thiagodmont/Documents/projects/android-ide
git add src-tauri/src/services/app_inspector.rs src-tauri/src/services/mod.rs
git commit -m "feat(mcp): add app_inspector service with runtime state and restart logic"
```

---

## Task 4: MCP tool handlers in `mcp_server.rs`

**Files:**
- Modify: `src-tauri/src/services/mcp_server.rs`

- [ ] **Step 4.1: Add imports at the top of `mcp_server.rs`**

After the existing `use crate::services::variant_manager;` line, add:

```rust
use crate::services::crash_inspector;
use crate::services::app_inspector;
use crate::services::build_inspector;
```

- [ ] **Step 4.2: Add param structs**

After the existing `pub struct RunTestsParams { ... }` block (around line 210), add:

```rust
#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct GetCrashStackTraceParams {
    #[schemars(description = "Filter to a specific package name, e.g. com.example.app")]
    pub package: Option<String>,
    #[schemars(description = "Return a specific crash group by ID (from get_crash_logs crash_group_id field)")]
    pub crash_group_id: Option<u64>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct RestartAppParams {
    #[schemars(description = "Android package name, e.g. com.example.app")]
    pub package: String,
    #[schemars(description = "ADB device serial (from list_devices). Uses first connected device if omitted.")]
    pub device_serial: Option<String>,
    #[schemars(description = "Cold start: clears app data with pm clear before launching (default true). Set false for warm restart.")]
    pub cold: Option<bool>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct GetAppRuntimeStateParams {
    #[schemars(description = "Android package name, e.g. com.example.app")]
    pub package: String,
    #[schemars(description = "ADB device serial (from list_devices). Uses first connected device if omitted.")]
    pub device_serial: Option<String>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct GetBuildConfigParams {
    #[schemars(description = "Gradle module name (subdirectory), e.g. app (default) or feature-login")]
    pub module: Option<String>,
}
```

- [ ] **Step 4.3: Add the four `#[tool]` handler methods**

Inside the `#[tool_router] impl AndroidMcpServer { ... }` block, add these four methods after the existing `run_tests` handler:

```rust
/// Get a parsed crash stack trace from the in-memory logcat buffer.
/// Requires logcat to be running (call start_logcat first).
/// Returns exception type, message, stack frames, and caused-by chains.
#[tool(description = "Get a parsed crash stack trace from logcat. Returns exception type, message, stack frames, and caused-by chain. Requires start_logcat to be running.")]
async fn get_crash_stack_trace(
    &self,
    Parameters(p): Parameters<GetCrashStackTraceParams>,
) -> Result<CallToolResult, McpError> {
    if let Some(ref pkg) = p.package {
        validate_package_name(pkg)?;
    }

    let logcat = self.logcat_state.lock().await;

    if !logcat.streaming && logcat.store.iter().count() == 0 {
        return Ok(CallToolResult::error(vec![Content::text(
            "Logcat not running — call start_logcat first.",
        )]));
    }

    let entries: Vec<_> = logcat.store.iter().cloned().collect();
    drop(logcat);

    match crash_inspector::find_crash(
        &entries,
        p.package.as_deref(),
        p.crash_group_id,
    ) {
        None => {
            let msg = if let Some(pkg) = &p.package {
                format!("No crashes found for package '{pkg}'.")
            } else {
                "No crashes found in the logcat buffer.".to_string()
            };
            Ok(CallToolResult::structured(json!({ "found": false, "message": msg })))
        }
        Some(crash) => Ok(CallToolResult::structured(json!(crash))),
    }
}

/// Restart an Android app: stop it (optionally clearing data), then relaunch and wait for display.
#[tool(description = "Restart an Android app: force-stop or pm clear, then relaunch and wait for the activity to display. Returns launch time.")]
async fn restart_app(
    &self,
    Parameters(p): Parameters<RestartAppParams>,
) -> Result<CallToolResult, McpError> {
    validate_package_name(&p.package)?;

    let settings = settings_manager::load_settings();
    let adb = adb_manager::get_adb_path(&settings);

    // Resolve device serial
    let serial = match resolve_device_serial(&adb, p.device_serial.as_deref()).await {
        Some(s) => s,
        None => return Ok(CallToolResult::error(vec![Content::text(
            "No device connected. Connect a device or launch an emulator first.",
        )])),
    };

    let cold = p.cold.unwrap_or(true);

    match app_inspector::restart_app(&adb, &serial, &p.package, cold).await {
        Ok(result) => Ok(CallToolResult::structured(json!(result))),
        Err(e) => Ok(CallToolResult::error(vec![Content::text(e)])),
    }
}

/// Get process list, thread counts, and RSS memory for all processes of an app.
#[tool(description = "Get runtime state for an Android app: running processes, thread counts per process, and RSS memory. Lightweight — no SIGQUIT.")]
async fn get_app_runtime_state(
    &self,
    Parameters(p): Parameters<GetAppRuntimeStateParams>,
) -> Result<CallToolResult, McpError> {
    validate_package_name(&p.package)?;

    let settings = settings_manager::load_settings();
    let adb = adb_manager::get_adb_path(&settings);

    let serial = if let Some(s) = p.device_serial.as_deref() {
        Some(s.to_string())
    } else {
        // Use device serial from active logcat session if available
        let logcat = self.logcat_state.lock().await;
        logcat.device_serial.clone()
    };

    let state = app_inspector::get_runtime_state(
        &adb,
        serial.as_deref(),
        &p.package,
    )
    .await;

    Ok(CallToolResult::structured(json!(state)))
}

/// Parse the module's build.gradle(.kts) for SDK levels, build types, and product flavors.
#[tool(description = "Parse build.gradle(.kts) for SDK levels, applicationId, buildTypes, and productFlavors. No Gradle execution needed.")]
async fn get_build_config(
    &self,
    Parameters(p): Parameters<GetBuildConfigParams>,
) -> Result<CallToolResult, McpError> {
    let gradle_root = self.get_gradle_root().await
        .ok_or_else(|| McpError::invalid_params(
            "No project open. Open an Android project first.", None,
        ))?;

    let module = p.module.as_deref().unwrap_or("app");

    match build_inspector::parse_build_config(&gradle_root, module) {
        Ok(config) => Ok(CallToolResult::structured(json!(config))),
        Err(e) => Ok(CallToolResult::error(vec![Content::text(e)])),
    }
}
```

- [ ] **Step 4.4: Add `resolve_device_serial` free function**

Add this module-level function (outside `impl AndroidMcpServer`, e.g. near the other helper functions after line ~1450):

```rust
/// Resolve a device serial: use the provided one, or fall back to the first
/// online device reported by `adb devices`.
async fn resolve_device_serial(adb: &std::path::PathBuf, requested: Option<&str>) -> Option<String> {
    if let Some(s) = requested {
        return Some(s.to_string());
    }
    let output = tokio::process::Command::new(adb)
        .arg("devices")
        .output()
        .await
        .ok()?;
    let text = String::from_utf8_lossy(&output.stdout);
    text.lines()
        .skip(1)
        .filter(|l| l.contains("\tdevice"))
        .next()
        .map(|l| l.split_whitespace().next().unwrap_or("").to_string())
        .filter(|s| !s.is_empty())
}
```

- [ ] **Step 4.5: Build to check compilation**

```bash
cd /Users/thiagodmont/Documents/projects/android-ide/src-tauri
cargo build 2>&1 | grep -E "^error"
```

Expected: no errors. Fix any type mismatches or missing imports before continuing.

- [ ] **Step 4.6: Run all tests to check nothing regressed**

```bash
cd /Users/thiagodmont/Documents/projects/android-ide/src-tauri
cargo test 2>&1 | tail -20
```

Expected: all existing tests pass plus the new crash_inspector, build_inspector, and app_inspector tests.

- [ ] **Step 4.7: Commit**

```bash
cd /Users/thiagodmont/Documents/projects/android-ide
git add src-tauri/src/services/mcp_server.rs
git commit -m "feat(mcp): add get_crash_stack_trace, restart_app, get_app_runtime_state, get_build_config tools"
```

---

## Task 5: Build release binary and smoke-test via MCP

- [ ] **Step 5.1: Build the release binary**

```bash
cd /Users/thiagodmont/Documents/projects/android-ide
cargo tauri build 2>&1 | tail -5
```

Or for a faster dev build:

```bash
cd /Users/thiagodmont/Documents/projects/android-ide/src-tauri
cargo build 2>&1 | tail -5
```

- [ ] **Step 5.2: Kill any running MCP server process**

```bash
pkill -f "android-dev-companion --mcp" 2>/dev/null; sleep 1
```

Then reconnect via `/mcp` in Claude Code.

- [ ] **Step 5.3: Smoke test `get_build_config`**

Call `get_build_config` with no arguments. Verify it returns `compile_sdk`, `min_sdk`, `build_types`.

- [ ] **Step 5.4: Smoke test `get_app_runtime_state`**

Call `get_app_runtime_state` with `package: "com.example.yourapp"` (or whatever package is installed). If app is running, verify processes and thread counts appear.

- [ ] **Step 5.5: Smoke test `get_crash_stack_trace`**

Start logcat, trigger a crash in the test app, then call `get_crash_stack_trace`. Verify frames and exception_type are populated.

- [ ] **Step 5.6: Smoke test `restart_app`**

Call `restart_app` with the test package. Verify `launched: true` and `display_time_ms` is populated (or null if no "Displayed" line appeared).

- [ ] **Step 5.7: Final commit (if any fixes needed)**

```bash
cd /Users/thiagodmont/Documents/projects/android-ide
git add -p
git commit -m "fix(mcp): post-smoke-test fixes for tier1 tools"
```
