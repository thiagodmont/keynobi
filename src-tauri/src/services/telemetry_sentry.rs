//! Opt-in crash and error reporting via Sentry (Rust side only).
//!
//! Uses the [`sentry`](https://crates.io/crates/sentry) crate **0.47** ([release notes](https://github.com/getsentry/sentry-rust/releases/tag/0.47.0)).
//! Upgrade major/minor versions together and re-run `cargo test --features telemetry`.
//!
//! **Privacy invariant:** we never intentionally send user-owned data: no project paths,
//! file contents, log lines, IPC payloads, or environment beyond coarse app/OS metadata.
//! All events pass through [`scrub_event`] before upload; breadcrumbs are dropped at capture time.
//!
//! Telemetry is gated by **both** `AppSettings.telemetry.enabled` (runtime) and compile-time
//! `SENTRY_DSN` (distribution builds). Enabling/disabling in Settings still requires an app
//! restart for the backend client to start or stop — see onboarding copy.
//!
//! ## One-off dashboard verification (local)
//!
//! After `init_if_enabled` has run successfully (telemetry on + `SENTRY_DSN` at compile time):
//! - Set `KEYNOBI_SENTRY_SMOKE=1` to send a single **info** event (scrubbed like any other).
//! - Optionally set `KEYNOBI_SENTRY_SMOKE_PANIC=1` in **debug** builds only to trigger a panic
//!   for crash ingestion testing. Unset these when done.
//!
//! Do **not** commit a DSN or enable `send_default_pii` in source; keep the DSN in env / CI secrets.

use std::borrow::Cow;
use std::sync::Arc;

use crate::models::AppSettings;
use sentry::protocol::{Context, Event, Exception, Frame, Stacktrace, TemplateInfo, Thread};
use sentry::{ClientOptions, Level};

/// Redacts known path patterns; intended for home directory and temp dirs.
pub fn scrub_string(input: &str, home: Option<&str>) -> String {
    let mut out = input.to_string();
    if let Some(h) = home {
        let h = h.trim_end_matches('/');
        if !h.is_empty() {
            out = out.replace(h, "<redacted>");
        }
    }
    // macOS per-user temp roots often appear in panic locations.
    if let Ok(re) = regex::Regex::new(r"/var/folders/[^/\s]+/[^/\s]+/T/[^\s]+") {
        out = re.replace_all(&out, "<redacted>").into_owned();
    }
    out
}

fn scrub_frame(frame: &mut Frame) {
    frame.abs_path = None;
    frame.pre_context.clear();
    frame.post_context.clear();
    frame.context_line = None;
    frame.vars.clear();
    if let Some(ref f) = frame.filename {
        frame.filename = Some(
            std::path::Path::new(f.as_str())
                .file_name()
                .and_then(|s| s.to_str())
                .map(|s| s.to_string())
                .unwrap_or_else(|| "<redacted>".into()),
        );
    }
    if let Some(ref p) = frame.package {
        if p.contains('/') || p.contains('\\') {
            frame.package = None;
        }
    }
}

fn scrub_stacktrace_opt(st: &mut Option<Stacktrace>) {
    if let Some(stack) = st {
        for frame in &mut stack.frames {
            scrub_frame(frame);
        }
        stack.registers.clear();
    }
}

fn scrub_template_opt(t: &mut Option<TemplateInfo>) {
    if let Some(template) = t {
        template.abs_path = None;
        template.pre_context.clear();
        template.post_context.clear();
        template.context_line = None;
        if let Some(ref f) = template.filename {
            template.filename = Some(
                std::path::Path::new(f.as_str())
                    .file_name()
                    .and_then(|s| s.to_str())
                    .map(|s| s.to_string())
                    .unwrap_or_else(|| "<redacted>".into()),
            );
        }
    }
}

fn scrub_exception(ex: &mut Exception, home: Option<&str>) {
    if let Some(ref mut v) = ex.value {
        *v = scrub_string(v, home);
    }
    ex.ty = scrub_string(&ex.ty, home);
    if let Some(ref mut m) = ex.module {
        *m = scrub_string(m, home);
    }
    if let Some(ref mut mech) = ex.mechanism {
        mech.data.clear();
        if let Some(ref mut d) = mech.description {
            *d = scrub_string(d, home);
        }
    }
    scrub_stacktrace_opt(&mut ex.stacktrace);
    scrub_stacktrace_opt(&mut ex.raw_stacktrace);
}

fn scrub_thread(th: &mut Thread, home: Option<&str>) {
    th.name = th.name.take().map(|n| scrub_string(&n, home));
    scrub_stacktrace_opt(&mut th.stacktrace);
    scrub_stacktrace_opt(&mut th.raw_stacktrace);
}

fn scrub_contexts(contexts: &mut sentry::protocol::Map<String, Context>) {
    for (_, ctx) in contexts.iter_mut() {
        match ctx {
            Context::Device(d) => {
                d.name = None;
            }
            Context::Other(map) => {
                map.clear();
            }
            _ => {}
        }
    }
}

/// Removes PII and path-like data from a Sentry event before upload.
pub fn scrub_event(mut event: Event<'static>, home: Option<&str>) -> Event<'static> {
    event.user = None;
    event.request = None;
    event.server_name = None;
    event.breadcrumbs = sentry::protocol::Values::new();
    event.modules.clear();
    event.extra.clear();
    event.culprit = None;
    event.transaction = None;

    event.debug_meta = Cow::Owned(sentry::protocol::DebugMeta {
        sdk_info: None,
        images: Vec::new(),
    });

    if let Some(ref mut m) = event.message {
        *m = scrub_string(m, home);
    }
    if let Some(ref mut entry) = event.logentry {
        entry.message = scrub_string(&entry.message, home);
    }

    for ex in &mut event.exception.values {
        scrub_exception(ex, home);
    }
    scrub_stacktrace_opt(&mut event.stacktrace);
    scrub_template_opt(&mut event.template);

    for th in &mut event.threads.values {
        scrub_thread(th, home);
    }

    scrub_contexts(&mut event.contexts);

    event.tags.insert(
        "build.profile".into(),
        if cfg!(debug_assertions) {
            "debug"
        } else {
            "release"
        }
        .into(),
    );
    event.tags.insert(
        "app.arch".into(),
        std::env::consts::ARCH.to_string(),
    );
    event.tags.insert("app.version".into(), env!("CARGO_PKG_VERSION").to_string());

    event
}

/// Initialize Sentry when telemetry is enabled and a DSN was embedded at build time.
pub fn init_if_enabled(settings: &AppSettings) -> Option<sentry::ClientInitGuard> {
    if !settings.telemetry.enabled {
        return None;
    }
    let dsn = option_env!("SENTRY_DSN")?;
    let home = dirs::home_dir().map(|p| p.to_string_lossy().into_owned());
    let before_send = Arc::new(move |event: Event<'static>| {
        Some(scrub_event(event, home.as_deref()))
    });
    let before_breadcrumb = Arc::new(|_breadcrumb: sentry::protocol::Breadcrumb| {
        Option::<sentry::protocol::Breadcrumb>::None
    });

    Some(sentry::init((
        dsn,
        ClientOptions {
            release: sentry::release_name!(),
            environment: Some(Cow::Borrowed(if cfg!(debug_assertions) {
                "development"
            } else {
                "production"
            })),
            send_default_pii: false,
            sample_rate: 1.0,
            traces_sample_rate: 0.0,
            max_breadcrumbs: 0,
            attach_stacktrace: false,
            before_send: Some(before_send),
            before_breadcrumb: Some(before_breadcrumb),
            ..Default::default()
        },
    )))
}

/// Send a test message (and optionally panic in debug) when env vars request it.
/// Call only after [`init_if_enabled`] returned `Some` in the same process.
pub fn run_optional_smoke_test() {
    if std::env::var("KEYNOBI_SENTRY_SMOKE").as_deref() != Ok("1") {
        return;
    }
    sentry::capture_message(
        "Keynobi Sentry smoke test — unset KEYNOBI_SENTRY_SMOKE after verifying the dashboard.",
        Level::Info,
    );
    #[cfg(debug_assertions)]
    if std::env::var("KEYNOBI_SENTRY_SMOKE_PANIC").as_deref() == Ok("1") {
        panic!(
            "Keynobi Sentry panic smoke test (debug only; unset KEYNOBI_SENTRY_SMOKE_PANIC)"
        );
    }
}

/// Capture a handled internal error (sanitized). Call only for invariant / unexpected failures
/// where diagnostics help the team and messages are known not to embed user paths.
#[allow(dead_code)]
pub fn capture_internal_error(domain: &'static str, err: &dyn std::error::Error) {
    let home = dirs::home_dir().map(|p| p.to_string_lossy().into_owned());
    let chain = format!("{domain}: {err}");
    let scrubbed = scrub_string(&chain, home.as_deref());
    sentry::configure_scope(|scope| {
        scope.set_tag("internal.domain", domain);
    });
    // `capture_message` runs the event through `before_send` (scrub_event).
    sentry::capture_message(&scrubbed, Level::Error);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::AppSettings;
    use sentry::protocol::{DeviceContext, Mechanism};
    use serde_json::json;

    #[test]
    fn scrub_string_strips_home() {
        let home = "/Users/testuser";
        let s = format!("panic at {home}/myproject/src/main.rs:12");
        let out = scrub_string(&s, Some(home));
        assert!(!out.contains("testuser"));
        assert!(out.contains("<redacted>"));
    }

    #[test]
    fn scrub_string_redacts_macos_var_folders_temp() {
        let s = "note at /var/folders/ab/cdefg12/T/rustctest123/foo.txt";
        let out = scrub_string(&s, None);
        assert!(!out.contains("/var/folders/"));
        assert!(out.contains("<redacted>"));
    }

    #[test]
    fn scrub_event_removes_user_and_request() {
        let mut event = Event {
            user: Some(sentry::User {
                username: Some("leak".into()),
                ..Default::default()
            }),
            request: Some(Default::default()),
            ..Default::default()
        };
        event = scrub_event(event, None);
        assert!(event.user.is_none());
        assert!(event.request.is_none());
    }

    #[test]
    fn scrub_event_clears_extra_and_modules_and_sets_tags() {
        let mut event = Event::default();
        event
            .extra
            .insert("k".into(), json!("/Users/secret/path"));
        event.modules.insert("m".into(), "/Users/x/lib".into());
        event = scrub_event(event, None);
        assert!(event.extra.is_empty());
        assert!(event.modules.is_empty());
        assert_eq!(
            event.tags.get("app.version").map(String::as_str),
            Some(env!("CARGO_PKG_VERSION"))
        );
        assert_eq!(
            event.tags.get("build.profile").map(String::as_str),
            Some(if cfg!(debug_assertions) {
                "debug"
            } else {
                "release"
            })
        );
        assert_eq!(
            event.tags.get("app.arch").map(String::as_str),
            Some(std::env::consts::ARCH)
        );
    }

    #[test]
    fn scrub_event_strips_device_hostname() {
        let mut event = Event::default();
        event.contexts.insert(
            "device".into(),
            Context::Device(Box::new(DeviceContext {
                name: Some("My-MacBook.local".into()),
                ..Default::default()
            })),
        );
        event = scrub_event(event, None);
        let ctx = event.contexts.get("device").expect("device context");
        match ctx {
            Context::Device(d) => assert!(d.name.is_none()),
            _ => panic!("expected Context::Device"),
        }
    }

    #[test]
    fn scrub_exception_clears_mechanism_data() {
        let mut ex = Exception {
            ty: "panic".into(),
            value: Some("msg".into()),
            mechanism: Some(Mechanism {
                ty: "panic".into(),
                data: {
                    let mut m = sentry::protocol::Map::new();
                    m.insert("path".into(), json!("/home/user/secret"));
                    m
                },
                ..Default::default()
            }),
            ..Default::default()
        };
        scrub_exception(&mut ex, None);
        assert!(ex.mechanism.as_ref().unwrap().data.is_empty());
    }

    #[test]
    fn init_if_enabled_returns_none_when_telemetry_disabled() {
        let mut settings = AppSettings::default();
        settings.telemetry.enabled = false;
        assert!(init_if_enabled(&settings).is_none());
    }

    #[test]
    fn scrub_frame_drops_abs_path() {
        let mut frame = Frame {
            abs_path: Some("/Users/x/foo.rs".into()),
            filename: Some("/Users/x/foo.rs".into()),
            pre_context: vec!["secret".into()],
            ..Default::default()
        };
        scrub_frame(&mut frame);
        assert!(frame.abs_path.is_none());
        assert_eq!(frame.filename.as_deref(), Some("foo.rs"));
        assert!(frame.pre_context.is_empty());
    }
}
