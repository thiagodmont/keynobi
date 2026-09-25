//! One UI Automator client per device at a time.
//!
//! A device accepts a single UiAutomation registration. A second
//! `uiautomator dump`, or a dump during an instrumentation test run, fails on
//! the device with "UiAutomationService ... already registered". Every UI
//! Automator call therefore takes the device's lock here first, and fails fast
//! while an instrumentation run that Keynobi started is using the device.
//!
//! Both registries are process-wide, so GUI commands and MCP tools running in
//! the same process share them. A separate `keynobi --mcp` process has its own.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, LazyLock, Mutex as StdMutex, MutexGuard, Weak};
use tokio::sync::{Mutex as AsyncMutex, OwnedMutexGuard};
use tokio::time::Instant;

/// Most devices that can have a UI Automator call running or waiting at once.
/// Entries exist only while a call holds or waits for a device's lock.
pub const MAX_LOCKED_SERIALS: usize = 64;

/// The device output that means another UiAutomation client is registered.
const ALREADY_REGISTERED: &str = "already registered";

static DEVICE_LOCKS: LazyLock<StdMutex<HashMap<String, Weak<AsyncMutex<()>>>>> =
    LazyLock::new(|| StdMutex::new(HashMap::new()));

static INSTRUMENTATION: LazyLock<InstrumentationRegistry> =
    LazyLock::new(InstrumentationRegistry::default);

fn lock_ignoring_poison<T>(m: &StdMutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// Held for the whole UI Automator call; releases the device when dropped.
pub struct DeviceUiLease {
    _guard: OwnedMutexGuard<()>,
}

fn device_lock(serial: &str) -> Result<Arc<AsyncMutex<()>>, String> {
    let mut locks = lock_ignoring_poison(&DEVICE_LOCKS);
    if let Some(lock) = locks.get(serial).and_then(Weak::upgrade) {
        return Ok(lock);
    }
    locks.retain(|_, lock| lock.strong_count() > 0);
    if locks.len() >= MAX_LOCKED_SERIALS {
        return Err(format!(
            "UI Automator is busy on {MAX_LOCKED_SERIALS} devices; try again when a call finishes"
        ));
    }
    let lock = Arc::new(AsyncMutex::new(()));
    locks.insert(serial.to_owned(), Arc::downgrade(&lock));
    Ok(lock)
}

/// Wait for `serial`'s UI Automator lock until `deadline`.
///
/// Fails at once while a Keynobi instrumentation run is using the device, and
/// checks again after waiting in case a run started meanwhile. The returned
/// lease is a tokio mutex guard, deliberately held across the awaits of the
/// whole call: it is the async lock that serializes the device.
pub async fn acquire(
    serial: &str,
    deadline: Instant,
    deadline_label: &str,
) -> Result<DeviceUiLease, String> {
    INSTRUMENTATION.ensure_idle(serial)?;
    let lock = device_lock(serial)?;
    let guard = tokio::time::timeout_at(deadline, lock.lock_owned())
        .await
        .map_err(|_| {
            format!(
                "UI Automator on {serial} is busy with another request; gave up after the \
                 {deadline_label} total deadline"
            )
        })?;
    INSTRUMENTATION.ensure_idle(serial)?;
    Ok(DeviceUiLease { _guard: guard })
}

/// True when device output reports another UiAutomation client.
pub fn reports_already_registered(output: &[u8]) -> bool {
    String::from_utf8_lossy(output).contains(ALREADY_REGISTERED)
}

/// The error for a device whose UiAutomation is held by a client outside Keynobi.
pub fn foreign_client_busy(serial: &str) -> String {
    format!(
        "Device {serial} is busy: another UI Automator client is registered (for example a test \
         run from Android Studio or another tool). Try again when it finishes."
    )
}

fn instrumentation_busy(serial: &str) -> String {
    format!(
        "Device {serial} is busy: instrumentation running. A connected test run started by \
         Keynobi is using UI Automator; try again when the tests finish."
    )
}

/// Whether a Gradle task runs instrumentation tests on connected devices
/// (`connectedAndroidTest`, `:app:connectedDebugAndroidTest`, `connectedCheck`).
pub fn is_instrumentation_task(task: &str) -> bool {
    task.rsplit(':')
        .next()
        .is_some_and(|name| name.starts_with("connected"))
}

/// Registers an instrumentation run for as long as the value lives.
pub struct InstrumentationRun {
    id: u64,
    registry: &'static InstrumentationRegistry,
}

impl Drop for InstrumentationRun {
    fn drop(&mut self) {
        lock_ignoring_poison(&self.registry.runs).remove(&self.id);
    }
}

/// Mark the devices `task` tests on as busy until the returned value drops.
///
/// Connected tests run on every connected device unless `ANDROID_SERIAL`
/// (from `env`, else the inherited environment) names one. Returns `None` for
/// tasks that are not instrumentation runs.
pub fn begin_instrumentation_for_task(
    task: &str,
    env: &[(String, String)],
) -> Option<InstrumentationRun> {
    if !is_instrumentation_task(task) {
        return None;
    }
    let serial = env
        .iter()
        .find(|(k, _)| k == "ANDROID_SERIAL")
        .map(|(_, v)| v.clone())
        .or_else(|| std::env::var("ANDROID_SERIAL").ok())
        .filter(|s| !s.trim().is_empty());
    Some(INSTRUMENTATION.begin(serial))
}

/// Running instrumentation, by run id: `Some(serial)` for one device, `None`
/// for all. Builds are one at a time per process, so this holds at most one
/// entry per build that has not finished yet.
#[derive(Default)]
pub struct InstrumentationRegistry {
    runs: StdMutex<HashMap<u64, Option<String>>>,
    next_id: AtomicU64,
}

impl InstrumentationRegistry {
    fn begin(&'static self, serial: Option<String>) -> InstrumentationRun {
        let id = self.next_id.fetch_add(1, Ordering::Relaxed);
        lock_ignoring_poison(&self.runs).insert(id, serial);
        InstrumentationRun { id, registry: self }
    }

    fn ensure_idle(&self, serial: &str) -> Result<(), String> {
        let busy = lock_ignoring_poison(&self.runs)
            .values()
            .any(|scope| scope.as_deref().is_none_or(|s| s == serial));
        if busy {
            Err(instrumentation_busy(serial))
        } else {
            Ok(())
        }
    }
}

#[cfg(test)]
pub(crate) mod test_support {
    use super::*;

    /// Mark one device as running instrumentation (tests share the process,
    /// so they never mark every device on the global registry).
    pub fn begin_instrumentation_on(serial: &str) -> InstrumentationRun {
        INSTRUMENTATION.begin(Some(serial.to_owned()))
    }

    pub fn instrumentation_active_on(serial: &str) -> bool {
        INSTRUMENTATION.ensure_idle(serial).is_err()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    fn leaked_registry() -> &'static InstrumentationRegistry {
        Box::leak(Box::default())
    }

    #[test]
    fn connected_test_tasks_are_instrumentation() {
        for task in [
            "connectedAndroidTest",
            "connectedDebugAndroidTest",
            ":app:connectedDebugAndroidTest",
            "connectedCheck",
        ] {
            assert!(is_instrumentation_task(task), "{task}");
        }
        for task in ["testDebug", "assembleDebug", ":app:lint", "app:deviceCheck"] {
            assert!(!is_instrumentation_task(task), "{task}");
        }
    }

    #[test]
    fn a_run_without_a_serial_makes_every_device_busy_until_it_ends() {
        let registry = leaked_registry();
        let run = registry.begin(None);
        let err = registry.ensure_idle("emulator-5554").unwrap_err();
        assert!(err.contains("busy: instrumentation running"), "{err}");
        assert!(registry.ensure_idle("R58M123ABC").is_err());
        drop(run);
        assert!(registry.ensure_idle("emulator-5554").is_ok());
    }

    #[test]
    fn a_run_on_one_serial_leaves_other_devices_free() {
        let registry = leaked_registry();
        let _run = registry.begin(Some("emulator-5554".into()));
        assert!(registry.ensure_idle("emulator-5554").is_err());
        assert!(registry.ensure_idle("emulator-5556").is_ok());
    }

    #[test]
    fn android_serial_in_the_gradle_env_scopes_the_run() {
        let serial = "lock-test-android-serial";
        let env = vec![("ANDROID_SERIAL".to_string(), serial.to_string())];
        assert!(begin_instrumentation_for_task("assembleDebug", &env).is_none());
        let run = begin_instrumentation_for_task(":app:connectedDebugAndroidTest", &env)
            .expect("connected tests register a run");
        assert!(test_support::instrumentation_active_on(serial));
        assert!(!test_support::instrumentation_active_on(
            "lock-test-other-device"
        ));
        drop(run);
        assert!(!test_support::instrumentation_active_on(serial));
    }

    #[test]
    fn already_registered_output_is_recognised() {
        assert!(reports_already_registered(
            b"java.lang.IllegalStateException: UiAutomationService \
              android.accessibilityservice.IAccessibilityServiceClient$Stub$Proxy@1 already registered!"
        ));
        assert!(!reports_already_registered(b"ERROR: null root node"));
    }

    #[tokio::test]
    async fn released_serials_are_forgotten() {
        let serial = "lock-test-forgotten";
        let lease = acquire(serial, Instant::now() + Duration::from_secs(1), "1 s")
            .await
            .unwrap();
        assert!(lock_ignoring_poison(&DEVICE_LOCKS)
            .get(serial)
            .is_some_and(|w| w.strong_count() > 0));
        drop(lease);
        // The next insert prunes it; the entry no longer keeps a lock alive.
        assert!(lock_ignoring_poison(&DEVICE_LOCKS)
            .get(serial)
            .is_none_or(|w| w.strong_count() == 0));
    }

    #[tokio::test]
    async fn waiting_for_a_held_device_gives_up_at_the_deadline() {
        let serial = "lock-test-deadline";
        let _held = acquire(serial, Instant::now() + Duration::from_secs(1), "1 s")
            .await
            .unwrap();
        let start = std::time::Instant::now();
        let err = acquire(
            serial,
            Instant::now() + Duration::from_millis(200),
            "200 ms",
        )
        .await
        .err()
        .expect("the device is held");
        assert!(
            err.contains("gave up after the 200 ms total deadline"),
            "{err}"
        );
        assert!(start.elapsed() < Duration::from_secs(2));
    }
}
