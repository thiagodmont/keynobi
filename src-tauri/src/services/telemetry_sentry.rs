//! Opt-in crash reporting via Sentry (Rust side only).
//!
//! **Allowlist, not scrubbing.** Every outgoing event is rebuilt by [`allowlist_event`] from a
//! fixed set of fields: error type, release, environment, OS name and version, CPU architecture,
//! and app stack frames reduced to function name, app-relative file, line and column. Messages,
//! exception values, breadcrumbs, tags, extra, user, request, server name, modules and every other
//! context are never copied, so new SDK fields cannot leak by default. Panics report their type
//! and location, never their message. Only event items leave the process: the transport drops
//! everything else (for example client reports).
//!
//! **Consent.** The client starts at launch only when `settings.telemetry.enabled` is on and a
//! `SENTRY_DSN` was embedded at build time. [`set_consent`] follows every settings save;
//! `before_send` and the transport both check it, so opting out stops uploads immediately.
//! Opting back in resumes uploads if the client was started at launch; otherwise the client starts
//! on the next launch.
//!
//! Do **not** commit a DSN or enable `send_default_pii`; keep the DSN in env / CI secrets.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use crate::models::AppSettings;
use sentry::integrations::backtrace::current_stacktrace;
use sentry::integrations::contexts::ContextIntegration;
use sentry::integrations::panic::PanicIntegration;
use sentry::protocol::{
    Context, DeviceContext, Envelope, EnvelopeItem, Event, Exception, Frame, Map, Mechanism,
    OsContext, Stacktrace,
};
use sentry::{ClientOptions, Level, Transport, TransportFactory, TransportOptions};

/// Live consent flag. Nothing is sent while it is false.
static CONSENT: AtomicBool = AtomicBool::new(false);

/// Crate prefixes whose frames count as app frames.
const APP_CRATE_PREFIXES: &[&str] = &["keynobi_lib::", "keynobi::"];
/// Mechanism types the app produces; any other mechanism is dropped.
const ALLOWED_MECHANISMS: &[&str] = &["panic", "generic"];
/// Longest function name kept on a frame.
const MAX_FUNCTION_LEN: usize = 512;
/// Longest app-relative file path kept on a frame.
const MAX_PATH_LEN: usize = 256;
/// Most frames kept per exception (the newest ones, nearest the crash).
const MAX_FRAMES: usize = 100;
/// Longest OS name or version kept.
const MAX_OS_FIELD_LEN: usize = 64;

/// Serializes tests that read or write the process-wide consent flag.
#[cfg(test)]
pub(crate) static CONSENT_TEST_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

/// Applies the user's crash-reporting choice immediately.
pub fn set_consent(enabled: bool) {
    CONSENT.store(enabled, Ordering::SeqCst);
}

/// Whether crash reports may be sent right now.
pub fn consent_given() -> bool {
    CONSENT.load(Ordering::SeqCst)
}

fn safe_error_type(ty: &str) -> String {
    let mut chars = ty.chars();
    let valid = ty.len() <= 64
        && chars
            .next()
            .is_some_and(|c| c.is_ascii_alphabetic() || c == '_')
        && chars.all(|c| c.is_ascii_alphanumeric() || c == '_');
    if valid {
        ty.to_string()
    } else {
        "Error".to_string()
    }
}

fn is_app_function(function: &str) -> bool {
    let unqualified = function.trim_start_matches('<');
    function.len() <= MAX_FUNCTION_LEN
        && APP_CRATE_PREFIXES
            .iter()
            .any(|prefix| unqualified.starts_with(prefix))
        && function
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || " _:<>{}()[]&,';*#$.!=+-".contains(c))
}

fn is_path_segment(segment: &str) -> bool {
    !segment.is_empty()
        && segment
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
}

/// Reduces a source path to `src/...` (relative to its crate). Returns `None` for anything that
/// does not look like a Rust source file under a `src` directory.
fn app_relative_path(path: &str) -> Option<String> {
    let path = path.replace('\\', "/");
    let relative = if path.starts_with("src/") {
        path.as_str()
    } else {
        &path[path.rfind("/src/")? + 1..]
    };
    if relative.len() > MAX_PATH_LEN {
        return None;
    }
    let segments: Vec<&str> = relative.split('/').collect();
    let (file, dirs) = segments.split_last()?;
    let file_ok = file.strip_suffix(".rs").is_some_and(is_path_segment);
    (file_ok && dirs.iter().all(|d| is_path_segment(d))).then(|| relative.to_string())
}

fn allowlist_frame(frame: &Frame) -> Option<Frame> {
    let function = match frame.function.as_deref() {
        Some(f) if is_app_function(f) => Some(f.to_string()),
        Some(_) => return None,
        // Panic location frames carry no function name.
        None if frame.in_app == Some(true) => None,
        None => return None,
    };
    let filename = frame
        .abs_path
        .as_deref()
        .and_then(app_relative_path)
        .or_else(|| frame.filename.as_deref().and_then(app_relative_path));
    if function.is_none() && filename.is_none() {
        return None;
    }
    Some(Frame {
        function,
        filename,
        lineno: frame.lineno,
        colno: frame.colno,
        in_app: Some(true),
        ..Default::default()
    })
}

fn allowlist_exception(ex: &Exception) -> Exception {
    let mechanism = ex
        .mechanism
        .as_ref()
        .filter(|m| ALLOWED_MECHANISMS.contains(&m.ty.as_str()))
        .map(|m| Mechanism {
            ty: m.ty.clone(),
            handled: m.handled,
            ..Default::default()
        });
    let stacktrace = ex.stacktrace.as_ref().and_then(|st| {
        let mut frames: Vec<Frame> = st.frames.iter().filter_map(allowlist_frame).collect();
        if frames.len() > MAX_FRAMES {
            frames.drain(..frames.len() - MAX_FRAMES);
        }
        (!frames.is_empty()).then(|| Stacktrace {
            frames,
            ..Default::default()
        })
    });
    Exception {
        ty: safe_error_type(&ex.ty),
        mechanism,
        stacktrace,
        ..Default::default()
    }
}

fn safe_os_field(value: &str) -> Option<String> {
    (value.len() <= MAX_OS_FIELD_LEN
        && value
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || " ._-".contains(c)))
    .then(|| value.to_string())
}

/// Builds the event that is actually sent, copying only allowlisted fields. Events without an
/// exception (plain messages) are dropped.
pub fn allowlist_event(event: &Event<'static>) -> Option<Event<'static>> {
    let exception: Vec<Exception> = event
        .exception
        .values
        .iter()
        .map(allowlist_exception)
        .collect();
    if exception.is_empty() {
        return None;
    }

    let mut contexts = Map::new();
    if let Some(Context::Os(os)) = event.contexts.get("os") {
        contexts.insert(
            "os".to_string(),
            Context::Os(Box::new(OsContext {
                name: os.name.as_deref().and_then(safe_os_field),
                version: os.version.as_deref().and_then(safe_os_field),
                ..Default::default()
            })),
        );
    }
    contexts.insert(
        "device".to_string(),
        Context::Device(Box::new(DeviceContext {
            arch: Some(std::env::consts::ARCH.to_string()),
            ..Default::default()
        })),
    );

    Some(Event {
        event_id: event.event_id,
        level: event.level,
        timestamp: event.timestamp,
        platform: event.platform.clone(),
        release: event.release.clone(),
        environment: event.environment.clone(),
        exception: exception.into(),
        contexts,
        ..Default::default()
    })
}

fn before_send(consent: &AtomicBool, event: Event<'static>) -> Option<Event<'static>> {
    if !consent.load(Ordering::SeqCst) {
        return None;
    }
    allowlist_event(&event)
}

/// Keeps only allowlisted event items; everything else in the envelope is dropped.
fn allowlist_envelope(envelope: &Envelope) -> Option<Envelope> {
    let mut out = Envelope::new();
    for item in envelope.items() {
        if let EnvelopeItem::Event(event) = item {
            if let Some(event) = allowlist_event(event) {
                out.add_item(event);
            }
        }
    }
    out.items().next().is_some().then_some(out)
}

struct ConsentGatedTransport {
    consent: &'static AtomicBool,
    inner: Arc<dyn Transport>,
}

impl Transport for ConsentGatedTransport {
    fn send_envelope(&self, envelope: Envelope) {
        if !self.consent.load(Ordering::SeqCst) {
            return;
        }
        if let Some(envelope) = allowlist_envelope(&envelope) {
            self.inner.send_envelope(envelope);
        }
    }

    fn flush(&self, timeout: Duration) -> bool {
        self.inner.flush(timeout)
    }

    fn shutdown(&self, timeout: Duration) -> bool {
        self.inner.shutdown(timeout)
    }
}

struct ConsentGatedTransportFactory {
    consent: &'static AtomicBool,
    inner: Arc<dyn TransportFactory>,
}

impl TransportFactory for ConsentGatedTransportFactory {
    fn create_transport_with_options(&self, options: TransportOptions) -> Arc<dyn Transport> {
        Arc::new(ConsentGatedTransport {
            consent: self.consent,
            inner: self.inner.create_transport_with_options(options),
        })
    }
}

/// Event for a panic: its type and location only. The panic message is never read.
fn panic_event(location: Option<&std::panic::Location<'_>>) -> Event<'static> {
    let mut stacktrace = current_stacktrace().unwrap_or_default();
    if let Some(location) = location {
        stacktrace.frames.push(Frame {
            filename: Some(location.file().to_string()),
            lineno: Some(location.line().into()),
            colno: Some(location.column().into()),
            in_app: Some(true),
            ..Default::default()
        });
    }
    Event {
        exception: vec![Exception {
            ty: "panic".into(),
            mechanism: Some(Mechanism {
                ty: "panic".into(),
                handled: Some(false),
                ..Default::default()
            }),
            stacktrace: (!stacktrace.frames.is_empty()).then_some(stacktrace),
            ..Default::default()
        }]
        .into(),
        level: Level::Fatal,
        ..Default::default()
    }
}

fn client_options(
    dsn: &str,
    consent: &'static AtomicBool,
    transport: Arc<dyn TransportFactory>,
) -> ClientOptions {
    // Integrations are listed explicitly so nothing else attaches data to events.
    ClientOptions::new()
        .dsn(dsn)
        .maybe_release(sentry::release_name!())
        .environment(if cfg!(debug_assertions) {
            "development"
        } else {
            "production"
        })
        .send_default_pii(false)
        .sample_rate(1.0)
        .traces_sample_rate(0.0)
        .max_breadcrumbs(0)
        .attach_stacktrace(false)
        .default_integrations(false)
        .add_integration(ContextIntegration::new())
        .add_integration(
            PanicIntegration::new().add_extractor(|info| Some(panic_event(info.location()))),
        )
        .transport(ConsentGatedTransportFactory {
            consent,
            inner: transport,
        })
        .before_send(move |event: Event<'static>| before_send(consent, event))
        .before_breadcrumb(|_breadcrumb: sentry::protocol::Breadcrumb| {
            Option::<sentry::protocol::Breadcrumb>::None
        })
}

/// Initialize Sentry when telemetry is enabled and a DSN was embedded at build time.
pub fn init_if_enabled(settings: &AppSettings) -> Option<sentry::ClientInitGuard> {
    set_consent(settings.telemetry.enabled);
    if !settings.telemetry.enabled {
        return None;
    }
    let dsn = option_env!("SENTRY_DSN")?;
    Some(sentry::init(client_options(
        dsn,
        &CONSENT,
        Arc::new(sentry::transports::DefaultTransportFactory),
    )))
}

/// Event used by the developer "send test event" command. Carries a type only.
pub fn test_event() -> Event<'static> {
    Event {
        exception: vec![Exception {
            ty: "NativeTelemetryTest".into(),
            ..Default::default()
        }]
        .into(),
        level: Level::Info,
        ..Default::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::AppSettings;
    use sentry::protocol::{Breadcrumb, Request, Stacktrace, User};
    use sentry::{Client, Hub, Scope};
    use serde_json::json;
    use std::sync::Mutex;

    /// Synthetic secrets that must never appear in anything sent.
    const SECRETS: &[&str] = &[
        "alice",
        "/Users/",
        "/opt/secret-project",
        "com.secret.app",
        "R58M123ABC",
        "emulator-5554",
        "abc123SECRET",
        "api.example.com",
        "alice@example.com",
        "AndroidRuntime",
        "FATAL EXCEPTION",
        "My-MacBook",
        "MacBookPro18",
        "let token",
    ];

    const HOME_PATH: &str = "/Users/alice/Projects/com.secret.app/app/src/main/Main.kt";
    const OTHER_PATH: &str = "/opt/secret-project/build/output.apk";
    const URL: &str = "https://api.example.com/v1/devices?token=abc123SECRET";
    const LOGCAT: &str =
        "09-25 12:00:00.000  1234  1234 E AndroidRuntime: FATAL EXCEPTION: main com.secret.app";
    const EMAIL: &str = "alice@example.com";

    fn secret_blob() -> String {
        format!("{HOME_PATH} {OTHER_PATH} {URL} {LOGCAT} {EMAIL} R58M123ABC emulator-5554")
    }

    fn assert_no_secrets(serialized: &str) {
        for secret in SECRETS {
            assert!(
                !serialized.contains(secret),
                "`{secret}` leaked into: {serialized}"
            );
        }
    }

    fn secret_frame(function: &str) -> Frame {
        Frame {
            function: Some(function.into()),
            symbol: Some(format!("_ZN{}", secret_blob())),
            module: Some("com.secret.app".into()),
            package: Some(OTHER_PATH.into()),
            abs_path: Some("/Users/alice/keynobi/src-tauri/src/services/build_runner.rs".into()),
            filename: Some("build_runner.rs".into()),
            lineno: Some(42),
            colno: Some(7),
            pre_context: vec!["let token = \"abc123SECRET\";".into()],
            context_line: Some(LOGCAT.into()),
            post_context: vec![EMAIL.into()],
            vars: {
                let mut vars = Map::new();
                vars.insert("serial".into(), json!("R58M123ABC"));
                vars
            },
            ..Default::default()
        }
    }

    /// An event carrying secrets in every field the SDK or app could populate.
    fn event_with_secrets() -> Event<'static> {
        let mut event = Event {
            message: Some(secret_blob()),
            logentry: Some(sentry::protocol::LogEntry {
                message: secret_blob(),
                params: vec![json!(EMAIL)],
            }),
            user: Some(User {
                email: Some(EMAIL.into()),
                username: Some("alice".into()),
                ..Default::default()
            }),
            request: Some(Request {
                url: URL.parse().ok(),
                ..Default::default()
            }),
            server_name: Some("My-MacBook.local".into()),
            culprit: Some(HOME_PATH.into()),
            transaction: Some(URL.into()),
            release: Some("keynobi@1.0.0".into()),
            environment: Some("production".into()),
            exception: vec![Exception {
                ty: "panic".into(),
                value: Some(secret_blob()),
                module: Some("com.secret.app".into()),
                mechanism: Some(Mechanism {
                    ty: "panic".into(),
                    description: Some(LOGCAT.into()),
                    handled: Some(false),
                    data: {
                        let mut data = Map::new();
                        data.insert("path".into(), json!(OTHER_PATH));
                        data
                    },
                    ..Default::default()
                }),
                stacktrace: Some(Stacktrace {
                    frames: vec![
                        secret_frame("std::panicking::begin_panic"),
                        secret_frame("com.secret.app::Main::onCreate"),
                        secret_frame("keynobi_lib::services::build_runner::run_build"),
                    ],
                    ..Default::default()
                }),
                ..Default::default()
            }]
            .into(),
            ..Default::default()
        };
        event.tags.insert("device".into(), "R58M123ABC".into());
        event.tags.insert("package".into(), "com.secret.app".into());
        event.extra.insert("logcat".into(), json!(LOGCAT));
        event.modules.insert("com.secret.app".into(), "1.0".into());
        event.breadcrumbs = vec![Breadcrumb {
            message: Some(LOGCAT.into()),
            ..Default::default()
        }]
        .into();
        event.contexts.insert(
            "device".into(),
            Context::Device(Box::new(DeviceContext {
                name: Some("My-MacBook.local".into()),
                model: Some("MacBookPro18,3".into()),
                ..Default::default()
            })),
        );
        event.contexts.insert(
            "os".into(),
            Context::Os(Box::new(OsContext {
                name: Some("macOS".into()),
                version: Some("15.1".into()),
                build: Some("R58M123ABC".into()),
                kernel_version: Some(secret_blob()),
                ..Default::default()
            })),
        );
        event.contexts.insert("project".into(), {
            let mut other = Map::new();
            other.insert("root".into(), json!(HOME_PATH));
            Context::Other(other)
        });
        event
    }

    #[derive(Default)]
    struct CapturingTransport(Mutex<Vec<Envelope>>);

    impl Transport for CapturingTransport {
        fn send_envelope(&self, envelope: Envelope) {
            if let Ok(mut sent) = self.0.lock() {
                sent.push(envelope);
            }
        }
    }

    impl CapturingTransport {
        fn serialized(&self) -> Vec<String> {
            let sent = self.0.lock().expect("capture lock");
            sent.iter()
                .map(|envelope| {
                    let mut bytes = Vec::new();
                    envelope.to_writer(&mut bytes).expect("serialize envelope");
                    String::from_utf8(bytes).expect("utf8 envelope")
                })
                .collect()
        }
    }

    /// A client wired exactly like production, but sending to an in-memory transport with a
    /// placeholder DSN, and with its own consent flag.
    fn test_client(consent: &'static AtomicBool) -> (Arc<Hub>, Arc<CapturingTransport>) {
        let transport = Arc::new(CapturingTransport::default());
        let factory: Arc<dyn TransportFactory> = Arc::new(transport.clone());
        let options = client_options("https://public@sentry.invalid/1", consent, factory);
        let client = Arc::new(Client::from(options));
        let hub = Arc::new(Hub::new(Some(client), Arc::new(Scope::default())));
        (hub, transport)
    }

    fn leaked_flag(value: bool) -> &'static AtomicBool {
        Box::leak(Box::new(AtomicBool::new(value)))
    }

    fn capture_secret_event(hub: &Arc<Hub>) {
        Hub::run(hub.clone(), || {
            sentry::configure_scope(|scope| {
                scope.set_tag("serial", "R58M123ABC");
                scope.set_extra("url", json!(URL));
                scope.set_user(Some(User {
                    email: Some(EMAIL.into()),
                    ..Default::default()
                }));
            });
            sentry::add_breadcrumb(Breadcrumb {
                message: Some(LOGCAT.into()),
                ..Default::default()
            });
            sentry::capture_event(event_with_secrets());
        });
    }

    #[test]
    fn allowlisted_event_contains_no_secrets() {
        let out = allowlist_event(&event_with_secrets()).expect("event with exception is kept");
        let serialized = serde_json::to_string(&out).expect("serialize");
        assert_no_secrets(&serialized);
    }

    #[test]
    fn allowlisted_event_keeps_type_release_os_arch_and_app_frames() {
        let out = allowlist_event(&event_with_secrets()).expect("kept");
        let ex = &out.exception.values[0];
        assert_eq!(ex.ty, "panic");
        assert!(ex.value.is_none());
        assert_eq!(ex.mechanism.as_ref().map(|m| m.handled), Some(Some(false)));
        let frames = &ex.stacktrace.as_ref().expect("app frame kept").frames;
        assert_eq!(frames.len(), 1, "only the app frame survives");
        assert_eq!(
            frames[0].function.as_deref(),
            Some("keynobi_lib::services::build_runner::run_build")
        );
        assert_eq!(
            frames[0].filename.as_deref(),
            Some("src/services/build_runner.rs")
        );
        assert_eq!(frames[0].lineno, Some(42));
        assert!(frames[0].abs_path.is_none() && frames[0].pre_context.is_empty());
        assert_eq!(out.release.as_deref(), Some("keynobi@1.0.0"));
        match out.contexts.get("os") {
            Some(Context::Os(os)) => {
                assert_eq!(os.name.as_deref(), Some("macOS"));
                assert_eq!(os.version.as_deref(), Some("15.1"));
            }
            other => panic!("expected os context, got {other:?}"),
        }
        match out.contexts.get("device") {
            Some(Context::Device(d)) => {
                assert_eq!(d.arch.as_deref(), Some(std::env::consts::ARCH));
                assert!(d.name.is_none() && d.model.is_none());
            }
            other => panic!("expected device context, got {other:?}"),
        }
        assert_eq!(out.contexts.len(), 2);
        assert!(out.tags.is_empty() && out.extra.is_empty() && out.breadcrumbs.is_empty());
    }

    #[test]
    fn events_without_exception_are_dropped() {
        let event = Event {
            message: Some(secret_blob()),
            ..Default::default()
        };
        assert!(allowlist_event(&event).is_none());
    }

    #[test]
    fn unsafe_error_types_are_replaced() {
        assert_eq!(safe_error_type("panic"), "panic");
        assert_eq!(safe_error_type("com.secret.app"), "Error");
        assert_eq!(safe_error_type("/Users/alice/x"), "Error");
    }

    #[test]
    fn app_relative_path_accepts_only_rust_sources_under_src() {
        assert_eq!(
            app_relative_path("/Users/alice/keynobi/src-tauri/src/lib.rs").as_deref(),
            Some("src/lib.rs")
        );
        assert_eq!(
            app_relative_path("src/services/logcat.rs").as_deref(),
            Some("src/services/logcat.rs")
        );
        assert_eq!(app_relative_path(HOME_PATH), None);
        assert_eq!(app_relative_path(OTHER_PATH), None);
        assert_eq!(
            app_relative_path("/Users/alice/src/com.secret.app/x.rs"),
            None
        );
        assert_eq!(app_relative_path("src/../../etc/passwd.rs"), None);
    }

    #[test]
    fn panic_event_reports_location_not_message() {
        // The extractor receives only the location; build one from a real call site.
        let location = std::panic::Location::caller();
        let out = allowlist_event(&panic_event(Some(location))).expect("kept");
        let ex = &out.exception.values[0];
        assert_eq!(ex.ty, "panic");
        assert!(ex.value.is_none());
        let last = ex
            .stacktrace
            .as_ref()
            .and_then(|st| st.frames.last())
            .expect("location frame");
        assert_eq!(
            last.filename.as_deref(),
            Some("src/services/telemetry_sentry.rs")
        );
        assert_eq!(last.lineno, Some(u64::from(location.line())));
    }

    #[test]
    fn sdk_panic_event_message_is_dropped() {
        // Shape produced by the SDK's default panic handler, which embeds the panic message.
        let event = Event {
            exception: vec![Exception {
                ty: "panic".into(),
                value: Some(format!("called `Result::unwrap()` on {}", secret_blob())),
                ..Default::default()
            }]
            .into(),
            ..Default::default()
        };
        let out = allowlist_event(&event).expect("kept");
        assert_no_secrets(&serde_json::to_string(&out).expect("serialize"));
    }

    #[test]
    fn sent_envelopes_contain_no_secrets() {
        let (hub, transport) = test_client(leaked_flag(true));
        capture_secret_event(&hub);
        let sent = transport.serialized();
        assert_eq!(sent.len(), 1, "one envelope sent");
        assert!(sent[0].contains("\"type\":\"panic\""));
        assert!(!sent[0].contains("server_name"));
        assert!(!sent[0].contains("client_report"));
        assert_no_secrets(&sent[0]);
    }

    #[test]
    fn opt_out_stops_sending_immediately() {
        let consent = leaked_flag(true);
        let (hub, transport) = test_client(consent);
        consent.store(false, Ordering::SeqCst);
        capture_secret_event(&hub);
        assert!(transport.serialized().is_empty());
        assert!(before_send(consent, event_with_secrets()).is_none());
    }

    #[test]
    fn transport_drops_envelopes_after_opt_out() {
        // Covers envelopes that were built before the user opted out.
        let consent = leaked_flag(true);
        let capture = Arc::new(CapturingTransport::default());
        let gated = ConsentGatedTransport {
            consent,
            inner: capture.clone(),
        };
        consent.store(false, Ordering::SeqCst);
        gated.send_envelope(Envelope::from(event_with_secrets()));
        assert!(capture.serialized().is_empty());
    }

    #[test]
    fn transport_keeps_only_allowlisted_event_items() {
        let capture = Arc::new(CapturingTransport::default());
        let gated = ConsentGatedTransport {
            consent: leaked_flag(true),
            inner: capture.clone(),
        };
        // An envelope that skipped before_send still leaves allowlisted.
        gated.send_envelope(Envelope::from(event_with_secrets()));
        let sent = capture.serialized();
        assert_eq!(sent.len(), 1);
        assert_no_secrets(&sent[0]);
    }

    #[test]
    fn toggling_consent_off_on_off_on_resumes_sending() {
        let consent = leaked_flag(true);
        let (hub, transport) = test_client(consent);
        for enabled in [false, true, false] {
            consent.store(enabled, Ordering::SeqCst);
        }
        capture_secret_event(&hub);
        assert!(transport.serialized().is_empty(), "off drops the event");
        consent.store(true, Ordering::SeqCst);
        capture_secret_event(&hub);
        let sent = transport.serialized();
        assert_eq!(sent.len(), 1, "on sends again");
        // The dropped event's loss report is not forwarded either.
        assert!(!sent[0].contains("client_report"));
        assert_no_secrets(&sent[0]);
    }

    #[test]
    fn init_if_enabled_returns_none_and_revokes_consent_when_disabled() {
        let _guard = CONSENT_TEST_LOCK
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        set_consent(true);
        let mut settings = AppSettings::default();
        settings.telemetry.enabled = false;
        assert!(init_if_enabled(&settings).is_none());
        assert!(!consent_given());
    }
}
