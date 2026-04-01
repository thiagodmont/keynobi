use crate::models::logcat::{
    EntryCategory, EntryFlags, LogcatKind, LogcatLevel, ProcessedEntry,
};
use std::collections::{HashMap, HashSet};

// ── RawLogLine ────────────────────────────────────────────────────────────────

/// A parsed-but-unenriched logcat line, produced by the Ingester.
/// Internal to the pipeline — not sent over IPC.
#[derive(Debug, Clone)]
pub struct RawLogLine {
    pub timestamp: String,
    pub pid: i32,
    pub tid: i32,
    pub level: LogcatLevel,
    pub tag: String,
    pub message: String,
}

// ── PipelineContext ───────────────────────────────────────────────────────────

/// Mutable context threaded through all processors for a given session.
/// Owned by the pipeline task — no mutex required.
pub struct PipelineContext {
    next_id: u64,
    pub pid_to_package: HashMap<i32, String>,
    pub seen_packages: HashSet<String>,
    active_crash_group: Option<u64>,
    next_crash_group: u64,
    /// Packages discovered since the last sync to `LogcatStateInner.known_packages`.
    /// Drained once per 100ms tick, avoiding iteration over the full map every tick.
    pub new_packages: Vec<String>,
}

impl PipelineContext {
    pub fn new() -> Self {
        PipelineContext {
            next_id: 1,
            pid_to_package: HashMap::new(),
            seen_packages: HashSet::new(),
            active_crash_group: None,
            next_crash_group: 1,
            new_packages: Vec::new(),
        }
    }

    /// Create a context pre-seeded with a PID → package map from `adb shell ps`.
    /// This ensures that apps already running when logcat starts will have their
    /// `package` field populated from the first entry, without waiting for an
    /// ActivityManager "Start proc" event.
    pub fn with_initial_pids(pid_to_package: HashMap<i32, String>) -> Self {
        let seen_packages: HashSet<String> = pid_to_package.values().cloned().collect();
        let new_packages: Vec<String> = seen_packages.iter().cloned().collect();
        PipelineContext {
            next_id: 1,
            pid_to_package,
            seen_packages,
            active_crash_group: None,
            next_crash_group: 1,
            new_packages,
        }
    }

    pub fn next_id(&mut self) -> u64 {
        let id = self.next_id;
        self.next_id += 1;
        id
    }

    pub fn next_crash_group_id(&mut self) -> u64 {
        let id = self.next_crash_group;
        self.next_crash_group += 1;
        id
    }

    pub fn clear(&mut self) {
        self.next_id = 1;
        self.pid_to_package.clear();
        self.seen_packages.clear();
        self.active_crash_group = None;
        self.next_crash_group = 1;
        self.new_packages.clear();
    }
}

impl Default for PipelineContext {
    fn default() -> Self {
        Self::new()
    }
}

// ── LogProcessor trait ───────────────────────────────────────────────────────

pub trait LogProcessor: Send + Sync {
    fn process(&self, entry: &mut ProcessedEntry, ctx: &mut PipelineContext);
}

// ── LogPipeline ───────────────────────────────────────────────────────────────

pub struct LogPipeline {
    processors: Vec<Box<dyn LogProcessor>>,
}

impl LogPipeline {
    /// Build the default pipeline with all standard processors in order.
    /// Order matters: PackageResolver must run before CrashAnalyzer so the
    /// package field is populated when crash grouping triggers.
    pub fn default_pipeline() -> Self {
        LogPipeline {
            processors: vec![
                Box::new(PackageResolver),
                Box::new(CrashAnalyzer),
                Box::new(JsonExtractor),
                Box::new(CategoryClassifier),
            ],
        }
    }

    /// Process a raw line through all processors and return an enriched entry.
    /// This is the hot path — called once per logcat line.
    pub fn run(&self, raw: RawLogLine, ctx: &mut PipelineContext) -> ProcessedEntry {
        let id = ctx.next_id();
        let mut entry = ProcessedEntry {
            id,
            timestamp: raw.timestamp,
            pid: raw.pid,
            tid: raw.tid,
            level: raw.level,
            tag: raw.tag,
            message: raw.message,
            package: None,
            kind: LogcatKind::Normal,
            is_crash: false,
            flags: 0,
            category: EntryCategory::General,
            crash_group_id: None,
            json_body: None,
        };

        for proc in &self.processors {
            proc.process(&mut entry, ctx);
        }

        entry
    }

    /// Run the pipeline and also return any synthetic separator entries
    /// produced for process lifecycle events (ProcessDied / ProcessStarted).
    /// The separators are generated before enrichment of the main entry.
    pub fn run_with_separators(
        &self,
        raw: RawLogLine,
        ctx: &mut PipelineContext,
    ) -> Vec<ProcessedEntry> {
        // Generate lifecycle separators before the main entry is processed.
        let separators = generate_lifecycle_separators(&raw, ctx);
        let entry = self.run(raw, ctx);

        let mut result = separators;
        result.push(entry);
        result
    }

    /// Drain `rx`, run each raw line through the full pipeline, and push all
    /// output entries (separators + processed) directly into `out`.
    ///
    /// This is the preferred hot path: it avoids allocating a temporary
    /// `Vec<ProcessedEntry>` per line (which `run_with_separators` does).
    /// Callers provide the target buffer so it can be pre-allocated or reused.
    pub fn run_batch_into(
        &self,
        rx: &mut tokio::sync::mpsc::UnboundedReceiver<RawLogLine>,
        ctx: &mut PipelineContext,
        out: &mut Vec<ProcessedEntry>,
    ) {
        while let Ok(raw) = rx.try_recv() {
            // Only ActivityManager lines can produce separator entries.
            // For all others we skip the separator check entirely.
            if raw.tag == "ActivityManager" {
                generate_lifecycle_separators_into(&raw, ctx, out);
            }
            out.push(self.run(raw, ctx));
        }
    }
}

// ── Separator generation (process lifecycle) ──────────────────────────────────

fn generate_lifecycle_separators(
    raw: &RawLogLine,
    ctx: &mut PipelineContext,
) -> Vec<ProcessedEntry> {
    let mut separators = Vec::new();
    generate_lifecycle_separators_into(raw, ctx, &mut separators);
    separators
}

/// Push lifecycle separator entries directly into `out`, avoiding an
/// intermediate Vec allocation.  Only call this for ActivityManager lines.
fn generate_lifecycle_separators_into(
    raw: &RawLogLine,
    ctx: &mut PipelineContext,
    out: &mut Vec<ProcessedEntry>,
) {
    // Check for process death
    if let Some(dead_pkg) = extract_process_death(&raw.tag, &raw.message) {
        let sep_id = ctx.next_id();
        out.push(make_separator(
            &raw.timestamp,
            raw.pid,
            &dead_pkg,
            LogcatKind::ProcessDied,
            sep_id,
        ));
    }

    // Check for process start / restart
    if let Some((new_pid, pkg)) = extract_pid_package(&raw.tag, &raw.message) {
        let is_restart = ctx.seen_packages.contains(&pkg);
        if !ctx.seen_packages.contains(&pkg) {
            ctx.seen_packages.insert(pkg.clone());
            ctx.new_packages.push(pkg.clone()); // track for dirty sync
        }
        ctx.pid_to_package.insert(new_pid, pkg.clone());

        if is_restart {
            let sep_id = ctx.next_id();
            out.push(make_separator(
                &raw.timestamp,
                new_pid,
                &pkg,
                LogcatKind::ProcessStarted,
                sep_id,
            ));
        }
    }
}

fn make_separator(
    timestamp: &str,
    pid: i32,
    package: &str,
    kind: LogcatKind,
    id: u64,
) -> ProcessedEntry {
    let message = match kind {
        LogcatKind::ProcessDied => format!("{} process died", package),
        LogcatKind::ProcessStarted => format!("{} process (re)started", package),
        LogcatKind::Normal => String::new(),
    };
    ProcessedEntry {
        id,
        timestamp: timestamp.to_owned(),
        pid,
        tid: 0,
        level: match kind {
            LogcatKind::ProcessDied => LogcatLevel::Error,
            _ => LogcatLevel::Info,
        },
        tag: "---".to_owned(),
        message,
        package: Some(package.to_owned()),
        kind,
        is_crash: false,
        flags: 0,
        category: EntryCategory::Lifecycle,
        crash_group_id: None,
        json_body: None,
    }
}

// ── Processor 1: PackageResolver ──────────────────────────────────────────────

/// Resolves the `package` field from the PID→package map built by
/// ActivityManager "Start proc" lines processed in the same session.
pub struct PackageResolver;

impl LogProcessor for PackageResolver {
    fn process(&self, entry: &mut ProcessedEntry, ctx: &mut PipelineContext) {
        entry.package = ctx.pid_to_package.get(&entry.pid).cloned();
    }
}

// ── Processor 2: CrashAnalyzer ────────────────────────────────────────────────

/// Detects crash/ANR signals and groups consecutive stack-trace lines under
/// a shared `crash_group_id`.
pub struct CrashAnalyzer;

impl LogProcessor for CrashAnalyzer {
    fn process(&self, entry: &mut ProcessedEntry, ctx: &mut PipelineContext) {
        let msg = &entry.message;
        let tag = &entry.tag;

        // A real crash from AndroidRuntime always contains "FATAL EXCEPTION".
        // Using `tag == "AndroidRuntime"` alone is too broad — it also matches
        // normal VM lifecycle messages from tools like `monkey` (used by launch_app),
        // which share the tag but are not crashes.
        let is_fatal = msg.contains("FATAL EXCEPTION")
            || msg.contains("Uncaught handler: thread");
        // ANR: tag check short-circuits the message scan for non-AM lines.
        let is_anr = tag == "ActivityManager" && msg.contains("ANR in");
        let is_native = msg.contains("signal 11 (SIGSEGV)")
            || msg.contains("signal 6 (SIGABRT)")
            || msg.contains("Fatal signal");

        // Stack trace continuation lines: start with \tat or \t (tab) or "  at "
        let is_stack_line = msg.starts_with('\t') || msg.starts_with("  at ");

        if is_fatal || is_native {
            entry.flags |= EntryFlags::CRASH;
            entry.is_crash = true;

            if ctx.active_crash_group.is_none() {
                // Start a new crash group
                let gid = ctx.next_crash_group_id();
                ctx.active_crash_group = Some(gid);
            }
        } else if is_anr {
            entry.flags |= EntryFlags::ANR;
            if ctx.active_crash_group.is_none() {
                let gid = ctx.next_crash_group_id();
                ctx.active_crash_group = Some(gid);
            }
        }

        if let Some(gid) = ctx.active_crash_group {
            entry.crash_group_id = Some(gid);
            entry.flags |= EntryFlags::CRASH;
            entry.is_crash = true;

            // End the group when we see a non-crash, non-stack-trace entry
            // at error or below level — heuristic: crash groups end when the
            // next non-related error appears.
            if !is_fatal && !is_anr && !is_native && !is_stack_line {
                // Keep grouping only while we're still seeing exception output.
                // If message doesn't look like exception content, close the group.
                let looks_like_trace = msg.contains("Exception")
                    || msg.contains("Error")
                    || msg.contains("Caused by")
                    || msg.contains("...");
                if !looks_like_trace {
                    ctx.active_crash_group = None;
                }
            }
        }
    }
}

// ── Processor 3: JsonExtractor ────────────────────────────────────────────────

/// Detects valid JSON payloads in the message field.
///
/// Uses a cheap heuristic before attempting a full parse to avoid calling
/// `serde_json::from_str` on the ~95% of lines that contain no JSON.
pub struct JsonExtractor;

impl LogProcessor for JsonExtractor {
    fn process(&self, entry: &mut ProcessedEntry, _ctx: &mut PipelineContext) {
        let msg = &entry.message;

        // Fast pre-check: must contain `{` and `"` to be worth trying.
        if !msg.contains('{') || !msg.contains('"') {
            return;
        }

        // Find the first `{` and attempt to parse from there.
        if let Some(start) = msg.find('{') {
            let candidate = &msg[start..];
            if serde_json::from_str::<serde_json::Value>(candidate).is_ok() {
                entry.flags |= EntryFlags::JSON_BODY;
                entry.json_body = Some(candidate.to_owned());
            }
        }
    }
}

// ── Processor 4: CategoryClassifier ──────────────────────────────────────────

/// Classifies entries into high-level categories based on their tag.
///
/// Uses a static lookup table for O(1) categorisation per entry.
pub struct CategoryClassifier;

impl LogProcessor for CategoryClassifier {
    fn process(&self, entry: &mut ProcessedEntry, _ctx: &mut PipelineContext) {
        entry.category = classify_tag(&entry.tag);
    }
}

fn classify_tag(tag: &str) -> EntryCategory {
    match tag {
        // Network
        "OkHttp" | "Retrofit" | "Volley" | "HttpClient" | "HttpURLConnection"
        | "NetworkSecurityConfig" | "ConnectivityManager" | "WifiManager" => EntryCategory::Network,

        // App lifecycle
        "ActivityManager" | "ActivityThread" | "Fragment" | "Application"
        | "LifecycleObserver" | "ComponentCallbacks" | "WindowManager" => EntryCategory::Lifecycle,

        // Performance / rendering
        "Choreographer" | "SurfaceFlinger" | "GLES" | "OpenGLRenderer"
        | "Skia" | "RenderThread" | "FrameEvents" | "VSYNC" => EntryCategory::Performance,

        // GC / memory
        "art" | "dalvikvm" | "GC" | "HeapTaskDaemon" | "zygote" => EntryCategory::Gc,

        // Database
        "SQLiteDatabase" | "SQLiteException" | "SQLiteCursor" | "Room" => EntryCategory::Database,

        // Security
        "Keystore" | "KeyGuard" | "CertBlacklist"
        | "BiometricManager" => EntryCategory::Security,

        _ => EntryCategory::General,
    }
}

// ── ADB log line parsing ──────────────────────────────────────────────────────

/// Parse a single logcat line in `threadtime` format:
/// `MM-DD HH:MM:SS.mmm  PID  TID LEVEL TAG: message`
///
/// Returns `None` for lines that do not match (e.g. logcat startup banner).
pub fn parse_logcat_line(line: &str) -> Option<RawLogLine> {
    let mut parts = line.splitn(2, ' ');
    let date = parts.next()?.to_owned();
    let rest = parts.next()?.trim_start();

    let mut rest_parts = rest.splitn(2, ' ');
    let time = rest_parts.next()?.to_owned();
    let rest = rest_parts.next()?.trim_start();

    let timestamp = format!("{} {}", date, time);

    // PID
    let mut rest_parts = rest.splitn(2, |c: char| c == ' ');
    let pid: i32 = rest_parts.next()?.trim().parse().ok()?;
    let rest = rest_parts.next()?.trim_start();

    // TID
    let mut rest_parts = rest.splitn(2, |c: char| c == ' ');
    let tid: i32 = rest_parts.next()?.trim().parse().ok()?;
    let rest = rest_parts.next()?.trim_start();

    // Level (single char)
    let mut chars = rest.chars();
    let level_char = chars.next()?;
    let level = LogcatLevel::from_char(level_char);
    let rest = chars.as_str().trim_start();

    // "TAG: message" or "TAG:message"
    let (tag, message) = if let Some(idx) = rest.find(": ") {
        (&rest[..idx], rest[idx + 2..].to_owned())
    } else if let Some(idx) = rest.find(':') {
        (&rest[..idx], rest[idx + 1..].to_owned())
    } else {
        ("", rest.to_owned())
    };

    Some(RawLogLine {
        timestamp,
        pid,
        tid,
        level,
        tag: tag.trim().to_owned(),
        message: message.trim_end_matches('\n').to_owned(),
    })
}

// ── ActivityManager extraction helpers ───────────────────────────────────────

/// Extract a (pid, package_name) pair from ActivityManager "Start proc" lines.
pub fn extract_pid_package(tag: &str, message: &str) -> Option<(i32, String)> {
    if tag != "ActivityManager" {
        return None;
    }
    if let Some(rest) = message.strip_prefix("Start proc ") {
        if let Some(colon_pos) = rest.find(':') {
            let pid_str = &rest[..colon_pos];
            if let Ok(pid) = pid_str.trim().parse::<i32>() {
                let after_pid = &rest[colon_pos + 1..];
                let end = after_pid
                    .find(|c| c == '/' || c == ' ')
                    .unwrap_or(after_pid.len());
                let raw_pkg = &after_pid[..end];
                let pkg = strip_process_suffix(raw_pkg);
                if looks_like_package(pkg) {
                    return Some((pid, pkg.to_owned()));
                }
            }
        }
        let end = rest.find(" for ").or_else(|| rest.find(' ')).unwrap_or(rest.len());
        let pkg = strip_process_suffix(&rest[..end]);
        if looks_like_package(pkg) {
            return None; // legacy format — no pid available
        }
    }
    None
}

/// Extract a package name from ActivityManager process-death messages.
pub fn extract_process_death(tag: &str, message: &str) -> Option<String> {
    if tag != "ActivityManager" {
        return None;
    }
    if let Some(rest) = message.strip_prefix("Process ") {
        if let Some(space) = rest.find(' ') {
            let pkg = strip_process_suffix(&rest[..space]);
            if looks_like_package(pkg) && rest.contains("has died") {
                return Some(pkg.to_owned());
            }
        }
    }
    if let Some(rest) = message.strip_prefix("Killing ") {
        if let Some(colon) = rest.find(':') {
            let after = &rest[colon + 1..];
            let end = after.find(|c| c == '/' || c == ' ' || c == ':').unwrap_or(after.len());
            let pkg = strip_process_suffix(&after[..end]);
            if looks_like_package(pkg) {
                return Some(pkg.to_owned());
            }
        }
    }
    if let Some(rest) = message.strip_prefix("Force finishing activity ") {
        let end = rest.find('/').or_else(|| rest.find(' ')).unwrap_or(rest.len());
        let pkg = strip_process_suffix(&rest[..end]);
        if looks_like_package(pkg) {
            return Some(pkg.to_owned());
        }
    }
    None
}

fn strip_process_suffix(name: &str) -> &str {
    name.find(':').map_or(name, |i| &name[..i])
}

fn looks_like_package(s: &str) -> bool {
    s.contains('.') && s.chars().all(|c| c.is_alphanumeric() || c == '_' || c == '.')
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    // ── parse_logcat_line ─────────────────────────────────────────────────────

    #[test]
    fn parse_standard_threadtime_line() {
        let line = "01-23 12:34:56.789  1234  5678 D MyTag: Hello world";
        let raw = parse_logcat_line(line).expect("should parse");
        assert_eq!(raw.level, LogcatLevel::Debug);
        assert_eq!(raw.tag, "MyTag");
        assert_eq!(raw.message, "Hello world");
        assert_eq!(raw.pid, 1234);
        assert_eq!(raw.tid, 5678);
    }

    #[test]
    fn parse_returns_none_for_garbage() {
        assert!(parse_logcat_line("not a logcat line").is_none());
        assert!(parse_logcat_line("").is_none());
    }

    // ── PackageResolver ───────────────────────────────────────────────────────

    #[test]
    fn package_resolver_attaches_package_for_known_pid() {
        let pipeline = LogPipeline::default_pipeline();
        let mut ctx = PipelineContext::new();
        ctx.pid_to_package.insert(1234, "com.example.app".into());

        let raw = RawLogLine {
            timestamp: "01-01 00:00:00.000".into(),
            pid: 1234,
            tid: 1234,
            level: LogcatLevel::Debug,
            tag: "MyTag".into(),
            message: "hello".into(),
        };
        let entry = pipeline.run(raw, &mut ctx);
        assert_eq!(entry.package, Some("com.example.app".into()));
    }

    // ── CrashAnalyzer ─────────────────────────────────────────────────────────

    #[test]
    fn crash_analyzer_does_not_flag_monkey_tool_startup_as_crash() {
        // Regression: launch_app uses `adb shell monkey` which emits AndroidRuntime
        // tag entries that are NOT crashes. The analyzer must not flag these.
        let pipeline = LogPipeline::default_pipeline();
        let mut ctx = PipelineContext::new();

        let monkey_messages = [
            ">>>>>> START com.android.internal.os.RuntimeInit uid 2000 <<<<<<",
            "Using default boot image",
            "Calling main entry com.android.commands.monkey.Monkey",
            "VM exiting with result code 0.",
        ];

        for msg in monkey_messages {
            let raw = RawLogLine {
                timestamp: "01-01 00:00:00.000".into(),
                pid: 9000,
                tid: 9000,
                level: LogcatLevel::Info,
                tag: "AndroidRuntime".into(),
                message: msg.into(),
            };
            let entry = pipeline.run(raw, &mut ctx);
            assert!(
                !entry.is_crash,
                "monkey message must not be flagged as crash: {msg}"
            );
        }
    }

    #[test]
    fn crash_analyzer_flags_fatal_exception() {
        let pipeline = LogPipeline::default_pipeline();
        let mut ctx = PipelineContext::new();

        let raw = RawLogLine {
            timestamp: "01-01 00:00:00.000".into(),
            pid: 1,
            tid: 1,
            level: LogcatLevel::Error,
            tag: "AndroidRuntime".into(),
            message: "FATAL EXCEPTION: main".into(),
        };
        let entry = pipeline.run(raw, &mut ctx);
        assert!(entry.is_crash);
        assert_ne!(entry.flags & EntryFlags::CRASH, 0);
        assert!(entry.crash_group_id.is_some());
    }

    #[test]
    fn crash_analyzer_groups_stack_trace_lines() {
        let pipeline = LogPipeline::default_pipeline();
        let mut ctx = PipelineContext::new();

        let fatal = RawLogLine {
            timestamp: "01-01 00:00:00.000".into(),
            pid: 1,
            tid: 1,
            level: LogcatLevel::Error,
            tag: "AndroidRuntime".into(),
            message: "FATAL EXCEPTION: main".into(),
        };
        let stack = RawLogLine {
            timestamp: "01-01 00:00:00.001".into(),
            pid: 1,
            tid: 1,
            level: LogcatLevel::Error,
            tag: "AndroidRuntime".into(),
            message: "\tat com.example.app.MainActivity.onCreate(MainActivity.kt:42)".into(),
        };

        let e1 = pipeline.run(fatal, &mut ctx);
        let e2 = pipeline.run(stack, &mut ctx);

        assert!(e1.crash_group_id.is_some());
        assert_eq!(e1.crash_group_id, e2.crash_group_id);
    }

    // ── JsonExtractor ─────────────────────────────────────────────────────────

    #[test]
    fn json_extractor_detects_valid_json() {
        let pipeline = LogPipeline::default_pipeline();
        let mut ctx = PipelineContext::new();

        let raw = RawLogLine {
            timestamp: "01-01 00:00:00.000".into(),
            pid: 1,
            tid: 1,
            level: LogcatLevel::Debug,
            tag: "API".into(),
            message: r#"{"status":"ok","code":200}"#.into(),
        };
        let entry = pipeline.run(raw, &mut ctx);
        assert_ne!(entry.flags & EntryFlags::JSON_BODY, 0);
        assert!(entry.json_body.is_some());
    }

    #[test]
    fn json_extractor_ignores_non_json() {
        let pipeline = LogPipeline::default_pipeline();
        let mut ctx = PipelineContext::new();

        let raw = RawLogLine {
            timestamp: "01-01 00:00:00.000".into(),
            pid: 1,
            tid: 1,
            level: LogcatLevel::Debug,
            tag: "MyTag".into(),
            message: "just a normal log message".into(),
        };
        let entry = pipeline.run(raw, &mut ctx);
        assert_eq!(entry.flags & EntryFlags::JSON_BODY, 0);
        assert!(entry.json_body.is_none());
    }

    #[test]
    fn json_extractor_extracts_from_prefix() {
        let pipeline = LogPipeline::default_pipeline();
        let mut ctx = PipelineContext::new();

        let raw = RawLogLine {
            timestamp: "01-01 00:00:00.000".into(),
            pid: 1,
            tid: 1,
            level: LogcatLevel::Debug,
            tag: "HTTP".into(),
            message: r#"Response body: {"data":{"id":1}}"#.into(),
        };
        let entry = pipeline.run(raw, &mut ctx);
        assert_ne!(entry.flags & EntryFlags::JSON_BODY, 0);
        assert!(entry.json_body.is_some());
    }

    // ── CategoryClassifier ────────────────────────────────────────────────────

    #[test]
    fn classifier_categorises_okhttp_as_network() {
        assert_eq!(classify_tag("OkHttp"), EntryCategory::Network);
    }

    #[test]
    fn classifier_categorises_activity_manager_as_lifecycle() {
        assert_eq!(classify_tag("ActivityManager"), EntryCategory::Lifecycle);
    }

    #[test]
    fn classifier_defaults_unknown_tag_to_general() {
        assert_eq!(classify_tag("SomeRandomTag"), EntryCategory::General);
    }

    // ── extract_pid_package ───────────────────────────────────────────────────

    #[test]
    fn extract_modern_start_proc_format() {
        let result = extract_pid_package(
            "ActivityManager",
            "Start proc 12345:com.example.myapp/u0a123 for activity ...",
        );
        assert_eq!(result, Some((12345, "com.example.myapp".to_string())));
    }

    #[test]
    fn extract_strips_process_role_suffix() {
        let result = extract_pid_package(
            "ActivityManager",
            "Start proc 9876:com.example.myapp:pushservice/u0a123",
        );
        assert_eq!(result, Some((9876, "com.example.myapp".to_string())));
    }

    #[test]
    fn extract_returns_none_for_non_activity_manager() {
        assert!(extract_pid_package("SomeTag", "Start proc 123:com.foo/u0 for ...").is_none());
    }

    // ── lifecycle separators ──────────────────────────────────────────────────

    #[test]
    fn run_with_separators_produces_separator_for_process_death() {
        let pipeline = LogPipeline::default_pipeline();
        let mut ctx = PipelineContext::new();

        let raw = RawLogLine {
            timestamp: "01-01 00:00:00.000".into(),
            pid: 1000,
            tid: 1000,
            level: LogcatLevel::Info,
            tag: "ActivityManager".into(),
            message: "Process com.example.app (pid 1000) has died".into(),
        };

        let entries = pipeline.run_with_separators(raw, &mut ctx);
        assert!(entries.len() >= 2, "expected at least separator + main entry");
        let sep = &entries[0];
        assert_eq!(sep.kind, LogcatKind::ProcessDied);
    }

    // ── PipelineContext::with_initial_pids ────────────────────────────────────

    #[test]
    fn with_initial_pids_populates_pid_map() {
        let mut map = HashMap::new();
        map.insert(1234, "com.example.app".into());
        map.insert(5678, "com.google.android.gms".into());

        let ctx = PipelineContext::with_initial_pids(map);

        assert_eq!(ctx.pid_to_package.get(&1234).map(String::as_str), Some("com.example.app"));
        assert_eq!(ctx.pid_to_package.get(&5678).map(String::as_str), Some("com.google.android.gms"));
    }

    #[test]
    fn with_initial_pids_populates_seen_packages() {
        let mut map = HashMap::new();
        map.insert(1234, "com.example.app".into());
        map.insert(5678, "com.example.app".into()); // duplicate package, different PID

        let ctx = PipelineContext::with_initial_pids(map);

        assert!(ctx.seen_packages.contains("com.example.app"));
        assert_eq!(ctx.seen_packages.len(), 1, "duplicate package names must be deduplicated");
    }

    #[test]
    fn with_initial_pids_populates_new_packages_for_first_sync() {
        let mut map = HashMap::new();
        map.insert(42, "com.example.first".into());

        let ctx = PipelineContext::with_initial_pids(map);

        assert!(ctx.new_packages.contains(&"com.example.first".to_string()),
            "initial packages must appear in new_packages so the first sync pushes them to state");
    }

    #[test]
    fn package_resolver_uses_pre_seeded_pid_map() {
        // Verify that a context built with with_initial_pids resolves packages
        // for entries whose PIDs were known BEFORE logcat started — this is the
        // core bug that with_initial_pids was added to fix.
        let mut map = HashMap::new();
        map.insert(9999, "com.already.running".into());

        let pipeline = LogPipeline::default_pipeline();
        let mut ctx = PipelineContext::with_initial_pids(map);

        let raw = RawLogLine {
            timestamp: "01-01 00:00:00.000".into(),
            pid: 9999,
            tid: 9999,
            level: LogcatLevel::Info,
            tag: "SomeTag".into(),
            message: "log from pre-existing process".into(),
        };

        let entry = pipeline.run(raw, &mut ctx);
        assert_eq!(
            entry.package.as_deref(),
            Some("com.already.running"),
            "package must be resolved for a PID that was running before logcat started"
        );
    }

    #[test]
    fn with_initial_pids_empty_map_behaves_like_new() {
        let ctx_seeded = PipelineContext::with_initial_pids(HashMap::new());
        let ctx_plain = PipelineContext::new();

        assert_eq!(ctx_seeded.pid_to_package.len(), ctx_plain.pid_to_package.len());
        assert_eq!(ctx_seeded.seen_packages.len(), ctx_plain.seen_packages.len());
        assert_eq!(ctx_seeded.new_packages.len(), ctx_plain.new_packages.len());
    }
}
