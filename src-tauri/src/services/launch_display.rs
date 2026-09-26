//! Time to initial and full display after a launch, read from the system's
//! logcat lines: `Displayed <pkg>/<activity>: +1s234ms` when the first frame
//! was drawn, and `Fully drawn <pkg>/<activity>: +…` when the app called
//! `reportFullyDrawn`.
//!
//! A launch reads them from this process's own logcat stream, and only when
//! that stream reads the launched device. Only entries stored after the
//! launch started, for the launched package, count; anything else leaves the
//! time unknown rather than guessed.
use crate::models::build::{LaunchTiming, LaunchTimingEvent};
use crate::services::build_runner::{attach_launch_timing, BuildState};
use crate::services::logcat::{EntryDevice, LogcatState, LogcatStateInner};
use std::time::Duration;
use tauri::AppHandle;
use tokio::time::Instant;

/// How long after a launch a `Fully drawn` line still counts.
pub const FULLY_DRAWN_WINDOW: Duration = Duration::from_secs(10);
/// How long a launch waits for its `Displayed` line to reach the stream
/// before it reports its result; `am start -W` returns after the line was
/// logged, so this covers only the stream's own delay.
pub const DISPLAYED_GRACE: Duration = Duration::from_secs(1);
/// How often the stream is checked while waiting.
const POLL_INTERVAL: Duration = Duration::from_millis(100);

/// Which display line a message is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DisplayLine {
    Displayed,
    FullyDrawn,
}

/// A launch duration as Android prints it (`TimeUtils.formatDuration`), in
/// milliseconds: `+123ms`, `+1s234ms`, `+1m2s3ms`, `+2s`. Units must appear
/// largest first, each at most once.
pub fn parse_launch_duration(text: &str) -> Option<u64> {
    let mut rest = text.strip_prefix('+')?;
    const UNITS: [(&str, u64); 5] = [
        ("d", 86_400_000),
        ("h", 3_600_000),
        ("m", 60_000),
        ("s", 1_000),
        ("ms", 1),
    ];
    let mut total: u64 = 0;
    let mut next_unit = 0;
    let mut any = false;
    while !rest.is_empty() {
        let digits = rest.len() - rest.trim_start_matches(|c: char| c.is_ascii_digit()).len();
        if digits == 0 {
            return None;
        }
        let value: u64 = rest[..digits].parse().ok()?;
        rest = &rest[digits..];
        // "ms" before "m": the longest unit that matches.
        let unit = if rest.starts_with("ms") {
            4
        } else {
            UNITS.iter().position(|(u, _)| rest.starts_with(u))?
        };
        if unit < next_unit {
            return None;
        }
        total = total.checked_add(value.checked_mul(UNITS[unit].1)?)?;
        rest = &rest[UNITS[unit].0.len()..];
        next_unit = unit + 1;
        any = true;
    }
    any.then_some(total)
}

/// The first `+…` duration in `text`, ending at whitespace or `)`.
fn first_duration(text: &str) -> Option<u64> {
    let start = text.find('+')?;
    let token = text[start..]
        .split(|c: char| c.is_whitespace() || c == ')')
        .next()?;
    parse_launch_duration(token)
}

/// Parse a `Displayed` or `Fully drawn` message (the text after the tag):
/// which line it is, the component's package, and the time in milliseconds.
/// A `(total +…)` time, which counts every activity of the launch, is
/// preferred over the activity's own.
pub fn parse_display_message(message: &str) -> Option<(DisplayLine, &str, u64)> {
    let (line, rest) = match message.strip_prefix("Displayed ") {
        Some(rest) => (DisplayLine::Displayed, rest),
        None => (
            DisplayLine::FullyDrawn,
            message.strip_prefix("Fully drawn ")?,
        ),
    };
    let component = rest.split_whitespace().next()?.trim_end_matches(':');
    let (package, activity) = component.split_once('/')?;
    if package.is_empty() || activity.is_empty() {
        return None;
    }
    let after = &rest[component.len()..];
    let ms = after
        .find("(total ")
        .and_then(|i| first_duration(&after[i..]))
        .or_else(|| first_duration(after))?;
    Some((line, package, ms))
}

/// The `Displayed` time of `package` in `adb logcat` output, from its last
/// such line.
pub fn displayed_time_in_logcat(output: &str, package: &str) -> Option<u64> {
    output.lines().rev().find_map(|line| {
        let message = &line[line.find("Displayed ")?..];
        match parse_display_message(message) {
            Some((DisplayLine::Displayed, pkg, ms)) if pkg == package => Some(ms),
            _ => None,
        }
    })
}

/// What the stream showed for one launch.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct DisplayTimes {
    pub displayed_ms: Option<u32>,
    pub fully_drawn_ms: Option<u32>,
}

/// A launch's view of this process's logcat stream, from the moment it started.
#[derive(Debug, Clone)]
pub struct LaunchWatch {
    first_entry_id: u64,
    serial: String,
    package: String,
    /// The stream names no device (adb's only one), and the launched device
    /// was the only one online.
    unnamed_stream_is_device: bool,
}

impl LaunchWatch {
    /// Start watching for `package` on `serial`, before the launch. `None`
    /// when the stream is not running or reads another device.
    /// `only_online_device` says `serial` is the one device online, which is
    /// what a stream started without a serial reads.
    pub fn start(
        state: &LogcatStateInner,
        serial: &str,
        package: &str,
        only_online_device: bool,
    ) -> Option<Self> {
        if !state.streaming {
            return None;
        }
        let unnamed_stream_is_device = match state.device_serial.as_deref() {
            Some(streamed) if streamed == serial => false,
            Some(_) => return None,
            None if only_online_device => true,
            None => return None,
        };
        Some(Self {
            first_entry_id: state.ids.peek_next_entry_id(),
            serial: serial.to_string(),
            package: package.to_string(),
            unnamed_stream_is_device,
        })
    }

    /// The first display lines of the package stored since the launch.
    pub fn scan(&self, state: &LogcatStateInner) -> DisplayTimes {
        let mut times = DisplayTimes::default();
        let since: Vec<_> = state
            .store
            .iter()
            .rev()
            .take_while(|e| e.id >= self.first_entry_id)
            .collect();
        for entry in since.into_iter().rev() {
            if !matches!(
                entry.tag.as_str(),
                "ActivityTaskManager" | "ActivityManager"
            ) {
                continue;
            }
            let from_device = match state.device_of_entry(entry.id) {
                EntryDevice::Serial(serial) => serial == self.serial,
                EntryDevice::Unnamed => self.unnamed_stream_is_device,
                EntryDevice::Unknown => false,
            };
            if !from_device {
                continue;
            }
            let Some((line, package, ms)) = parse_display_message(&entry.message) else {
                continue;
            };
            if package != self.package {
                continue;
            }
            let ms = u32::try_from(ms).ok();
            match line {
                DisplayLine::Displayed => times.displayed_ms = times.displayed_ms.or(ms),
                DisplayLine::FullyDrawn => times.fully_drawn_ms = times.fully_drawn_ms.or(ms),
            }
        }
        times
    }

    /// Check the stream until what it shows is `enough` or `deadline` passes.
    pub async fn wait(
        &self,
        logcat: &LogcatState,
        deadline: Instant,
        enough: impl Fn(&DisplayTimes) -> bool,
    ) -> DisplayTimes {
        loop {
            let times = self.scan(&*logcat.lock().await);
            let now = Instant::now();
            if enough(&times) || now >= deadline {
                return times;
            }
            tokio::time::sleep(POLL_INTERVAL.min(deadline - now)).await;
        }
    }
}

/// Sent when display times arrive after `launch_app_on_device` returned.
pub const BUILD_LAUNCH_TIMING_EVENT: &str = "build:launch_timing";

/// Add to `timing` the display times the stream shows, waiting at most
/// [`DISPLAYED_GRACE`] after `am start` returned for the `Displayed` line.
pub async fn add_display_times(
    timing: &mut LaunchTiming,
    watch: &LaunchWatch,
    logcat: &LogcatState,
) {
    let times = watch
        .wait(logcat, Instant::now() + DISPLAYED_GRACE, |t| {
            t.displayed_ms.is_some()
        })
        .await;
    timing.displayed_ms = times.displayed_ms;
    timing.fully_drawn_ms = times.fully_drawn_ms;
}

/// Wait until [`FULLY_DRAWN_WINDOW`] after `started` for the display lines
/// `timing` lacks, then record what arrived on build `record_id` (through
/// the same locked history update as the launch time) and tell the app.
/// Records nothing when nothing new arrived.
pub async fn record_late_display_times(
    build_state: BuildState,
    logcat: LogcatState,
    app: Option<AppHandle>,
    record_id: u32,
    mut timing: LaunchTiming,
    watch: LaunchWatch,
    started: Instant,
) -> Option<LaunchTiming> {
    let times = watch
        .wait(&logcat, started + FULLY_DRAWN_WINDOW, |t| {
            (timing.displayed_ms.is_some() || t.displayed_ms.is_some())
                && t.fully_drawn_ms.is_some()
        })
        .await;
    let displayed_ms = timing.displayed_ms.or(times.displayed_ms);
    let fully_drawn_ms = timing.fully_drawn_ms.or(times.fully_drawn_ms);
    if (displayed_ms, fully_drawn_ms) == (timing.displayed_ms, timing.fully_drawn_ms) {
        return None;
    }
    timing.displayed_ms = displayed_ms;
    timing.fully_drawn_ms = fully_drawn_ms;
    if let Err(e) = attach_launch_timing(&build_state, record_id, timing.clone()).await {
        tracing::warn!("Display times not recorded on build #{record_id}: {e}");
        return None;
    }
    if let Some(app) = app {
        use tauri::Emitter;
        let event = LaunchTimingEvent {
            record_id,
            launch: timing.clone(),
        };
        let _ = app.emit(BUILD_LAUNCH_TIMING_EVENT, event);
    }
    Some(timing)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::logcat::{EntryCategory, LogcatKind, LogcatLevel, ProcessedEntry};

    #[test]
    fn launch_durations_parse_in_every_unit_android_prints() {
        for (text, ms) in [
            ("+123ms", 123),
            ("+0ms", 0),
            ("+1s234ms", 1_234),
            ("+2s", 2_000),
            ("+1m2s3ms", 62_003),
            ("+1m", 60_000),
            ("+1h0m1s5ms", 3_601_005),
            ("+1d2h", 93_600_000),
        ] {
            assert_eq!(parse_launch_duration(text), Some(ms), "{text}");
        }
        for text in [
            "",
            "+",
            "123ms",
            "+ms",
            "+12",
            "+1ms2s",
            "+1s1s",
            "+1x",
            "+1.5s",
            "+-1ms",
            "+99999999999999999999ms",
        ] {
            assert_eq!(parse_launch_duration(text), None, "{text}");
        }
    }

    #[test]
    fn display_messages_give_the_component_package_and_time() {
        assert_eq!(
            parse_display_message("Displayed com.example.app/.MainActivity: +1s234ms"),
            Some((DisplayLine::Displayed, "com.example.app", 1_234))
        );
        assert_eq!(
            parse_display_message(
                "Displayed com.example.app.debug/com.example.app.Main: +850ms (total +1s200ms)"
            ),
            Some((DisplayLine::Displayed, "com.example.app.debug", 1_200))
        );
        assert_eq!(
            parse_display_message("Fully drawn com.example.app/.MainActivity: +1m2s3ms"),
            Some((DisplayLine::FullyDrawn, "com.example.app", 62_003))
        );
        for message in [
            "Displayed com.example.app: +10ms",
            "Displayed com.example.app/.Main: soon",
            "Start proc 123:com.example.app/u0a1 for activity",
            "Fully drawn",
        ] {
            assert_eq!(parse_display_message(message), None, "{message}");
        }
    }

    #[test]
    fn adb_logcat_output_gives_the_packages_last_displayed_time() {
        let log = "01-01 00:00:00.000  1  2 I ActivityTaskManager: Displayed com.example.apple/.Main: +9ms\n\
                   01-01 00:00:01.000  1  2 I ActivityTaskManager: Displayed com.example.app/.Main: +450ms\n";
        assert_eq!(displayed_time_in_logcat(log, "com.example.app"), Some(450));
        assert_eq!(displayed_time_in_logcat(log, "com.example"), None);
    }

    fn entry(id: u64, tag: &str, message: &str) -> ProcessedEntry {
        ProcessedEntry {
            id,
            timestamp: "01-01 00:00:00.000".into(),
            pid: 1,
            tid: 1,
            level: LogcatLevel::Info,
            tag: tag.into(),
            message: message.into(),
            package: None,
            kind: LogcatKind::Normal,
            is_crash: false,
            flags: 0,
            category: EntryCategory::General,
            crash_group_id: None,
            json_body: None,
        }
    }

    /// A stream of `serial` (or of adb's only device) with `before` stored
    /// before the watch starts and `after` stored after.
    fn stream(serial: Option<&str>, before: &[(&str, &str)]) -> LogcatStateInner {
        let mut state = LogcatStateInner::new();
        state.streaming = true;
        state.device_serial = serial.map(str::to_string);
        state.record_stream_start(serial.map(str::to_string));
        for (tag, message) in before {
            let id = state.ids.next_entry_id();
            state.store.push(entry(id, tag, message));
        }
        state
    }

    fn store(state: &mut LogcatStateInner, lines: &[(&str, &str)]) {
        for (tag, message) in lines {
            let id = state.ids.next_entry_id();
            state.store.push(entry(id, tag, message));
        }
    }

    #[test]
    fn only_lines_after_the_launch_for_its_package_count() {
        let mut state = stream(
            Some("emulator-5554"),
            &[(
                "ActivityTaskManager",
                "Displayed com.example.app/.Main: +5s",
            )],
        );
        let watch = LaunchWatch::start(&state, "emulator-5554", "com.example.app", false)
            .expect("the stream reads the device");
        assert_eq!(watch.scan(&state), DisplayTimes::default());

        store(
            &mut state,
            &[
                (
                    "ActivityTaskManager",
                    "Displayed com.example.apple/.Main: +9ms",
                ),
                ("MyApp", "Displayed com.example.app/.Main: +1ms"),
                (
                    "ActivityTaskManager",
                    "Displayed com.example.app/.Main: +790ms",
                ),
                (
                    "ActivityTaskManager",
                    "Displayed com.example.app/.Second: +2s",
                ),
            ],
        );
        assert_eq!(
            watch.scan(&state),
            DisplayTimes {
                displayed_ms: Some(790),
                fully_drawn_ms: None
            }
        );
        store(
            &mut state,
            &[(
                "ActivityManager",
                "Fully drawn com.example.app/.Main: +1s400ms",
            )],
        );
        assert_eq!(watch.scan(&state).fully_drawn_ms, Some(1_400));
    }

    #[test]
    fn a_stream_of_another_device_or_none_is_not_used() {
        let mut state = stream(Some("emulator-5556"), &[]);
        assert!(LaunchWatch::start(&state, "emulator-5554", "com.example.app", true).is_none());
        state.streaming = false;
        state.device_serial = Some("emulator-5554".into());
        assert!(LaunchWatch::start(&state, "emulator-5554", "com.example.app", true).is_none());

        // A stream without a serial reads adb's only device.
        let state = stream(None, &[]);
        assert!(LaunchWatch::start(&state, "emulator-5554", "com.example.app", false).is_none());
        assert!(LaunchWatch::start(&state, "emulator-5554", "com.example.app", true).is_some());
    }

    #[test]
    fn lines_from_a_stream_restarted_on_another_device_do_not_count() {
        let mut state = stream(Some("emulator-5554"), &[]);
        let watch = LaunchWatch::start(&state, "emulator-5554", "com.example.app", false).unwrap();
        state.record_stream_start(Some("emulator-5556".into()));
        store(
            &mut state,
            &[(
                "ActivityTaskManager",
                "Displayed com.example.app/.Main: +790ms",
            )],
        );
        assert_eq!(watch.scan(&state), DisplayTimes::default());
    }

    #[tokio::test]
    async fn waiting_returns_what_arrived_by_the_deadline() {
        let state = stream(Some("emulator-5554"), &[]);
        let watch = LaunchWatch::start(&state, "emulator-5554", "com.example.app", false).unwrap();
        let logcat: LogcatState = std::sync::Arc::new(tokio::sync::Mutex::new(state));

        let start = std::time::Instant::now();
        let none = watch
            .wait(&logcat, Instant::now() + Duration::from_millis(200), |t| {
                t.fully_drawn_ms.is_some()
            })
            .await;
        assert_eq!(none, DisplayTimes::default());
        assert!(start.elapsed() >= Duration::from_millis(200));

        let writer = logcat.clone();
        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(50)).await;
            store(
                &mut *writer.lock().await,
                &[(
                    "ActivityTaskManager",
                    "Displayed com.example.app/.Main: +790ms",
                )],
            );
        });
        let start = std::time::Instant::now();
        let displayed = watch
            .wait(&logcat, Instant::now() + Duration::from_secs(30), |t| {
                t.displayed_ms.is_some()
            })
            .await;
        assert_eq!(displayed.displayed_ms, Some(790));
        assert!(start.elapsed() < Duration::from_secs(5));
    }

    fn launch() -> LaunchTiming {
        LaunchTiming {
            total_ms: 812,
            wait_ms: Some(815),
            launch_state: None,
            measured_at: "2026-01-01T00:01:00Z".into(),
            serial: "emulator-5554".into(),
            avd_name: None,
            model: None,
            displayed_ms: Some(790),
            fully_drawn_ms: None,
        }
    }

    /// A build this process holds only in memory, so the test does not
    /// write the shared history file.
    async fn succeeded_build(id: u32) -> BuildState {
        let state = BuildState::new();
        let record = serde_json::from_value(serde_json::json!({
            "id": id, "task": "assembleDebug", "errors": [],
            "status": { "state": "success", "success": true, "durationMs": 1000,
                        "errorCount": 0, "warningCount": 0 },
            "startedAt": "2026-01-01T00:00:00Z", "projectRoot": "/p",
        }))
        .unwrap();
        state.inner.lock().await.history.push_back(record);
        state
    }

    #[tokio::test]
    async fn a_fully_drawn_line_that_arrives_later_is_recorded_on_the_build() {
        // Far above any ID the shared test data directory holds.
        let id = u32::MAX - 11;
        let build_state = succeeded_build(id).await;
        let state = stream(Some("emulator-5554"), &[]);
        let watch = LaunchWatch::start(&state, "emulator-5554", "com.example.app", false).unwrap();
        let logcat: LogcatState = std::sync::Arc::new(tokio::sync::Mutex::new(state));

        let recording = tokio::spawn(record_late_display_times(
            build_state.clone(),
            logcat.clone(),
            None,
            id,
            launch(),
            watch,
            Instant::now(),
        ));
        tokio::time::sleep(Duration::from_millis(50)).await;
        store(
            &mut *logcat.lock().await,
            &[(
                "ActivityTaskManager",
                "Fully drawn com.example.app/.Main: +1s400ms",
            )],
        );
        let recorded = recording.await.unwrap().expect("recorded");
        assert_eq!(recorded.fully_drawn_ms, Some(1_400));
        assert_eq!(recorded.displayed_ms, Some(790));
        let bs = build_state.inner.lock().await;
        assert_eq!(bs.history.back().unwrap().launch, Some(recorded));
    }

    #[tokio::test]
    async fn nothing_is_recorded_when_no_line_arrives_in_the_window() {
        let id = u32::MAX - 12;
        let build_state = succeeded_build(id).await;
        let mut state = stream(Some("emulator-5554"), &[]);
        let watch = LaunchWatch::start(&state, "emulator-5554", "com.example.app", false).unwrap();
        // Another app's line does not count.
        store(
            &mut state,
            &[(
                "ActivityTaskManager",
                "Fully drawn com.other.app/.Main: +1s",
            )],
        );
        let logcat: LogcatState = std::sync::Arc::new(tokio::sync::Mutex::new(state));

        // The window ends 200 ms from now.
        let started = Instant::now() + Duration::from_millis(200) - FULLY_DRAWN_WINDOW;
        let recorded = record_late_display_times(
            build_state.clone(),
            logcat,
            None,
            id,
            launch(),
            watch,
            started,
        )
        .await;
        assert_eq!(recorded, None);
        assert_eq!(
            build_state
                .inner
                .lock()
                .await
                .history
                .back()
                .unwrap()
                .launch,
            None
        );
    }
}
