//! Which MCP sessions are alive, for the status bar and the MCP panel.
//!
//! - **Attached** sessions are served by the app itself over its socket. The
//!   app keeps them in an in-memory [`McpSessionRegistry`] and emits
//!   [`SESSIONS_CHANGED_EVENT`] when the list changes.
//! - **Standalone** servers are separate `keynobi --mcp` processes that could
//!   not attach. Each writes `<data dir>/mcp-sessions/<pid>.json` at start and
//!   removes it on exit; readers drop records whose process is gone.
use crate::services::settings_manager;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use ts_rs::TS;

/// Most sessions the app serves at once; further attach requests are refused
/// and those clients run standalone.
pub const MAX_ATTACHED_SESSIONS: usize = 16;
/// Most standalone records read from the sessions directory.
pub const MAX_STANDALONE_RECORDS: usize = 64;
/// Emitted to the frontend with the new `Vec<McpAttachedSession>`.
pub const SESSIONS_CHANGED_EVENT: &str = "mcp:sessions_changed";

const SESSIONS_DIR: &str = "mcp-sessions";
/// Version of this binary, recorded for every session so the app can tell
/// which MCP servers still run an older release.
pub const APP_VERSION: &str = env!("CARGO_PKG_VERSION");
/// Written by releases before session records existed.
const LEGACY_PID_FILE: &str = "mcp-server.pid";

// ── Models ────────────────────────────────────────────────────────────────────

/// An MCP client attached to the running app.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/bindings/")]
pub struct McpAttachedSession {
    /// Unique for the life of the app process.
    pub id: u32,
    /// PID of the `keynobi --mcp` process forwarding the client's stdio.
    pub pid: Option<u32>,
    /// The project the session is pinned to, or `None` when it follows the app.
    pub project: Option<String>,
    /// ISO 8601 UTC time the session attached.
    pub connected_at: String,
    /// The MCP client's name, once it has initialized.
    pub client_name: Option<String>,
    /// Version of the `keynobi --mcp` binary, from its attach request.
    pub version: String,
}

/// A `keynobi --mcp` process running with its own state, not shared with the app.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/bindings/")]
pub struct McpStandaloneServer {
    pub pid: u32,
    /// ISO 8601 UTC start time.
    pub started_at: String,
    /// The server's project, if it found one.
    pub project: Option<String>,
    /// Why it could not attach to the app.
    pub reason: String,
    /// Version of the binary, or `None` for records written by releases
    /// that did not record it (older than the app reading them).
    #[serde(default)]
    pub version: Option<String>,
    /// The binary that wrote the record; used to tell a reused PID apart.
    /// Kept out of IPC responses.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(skip)]
    pub exe: Option<String>,
}

/// Live MCP sessions: attached to the app, and standalone servers.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/bindings/")]
pub struct McpServerStatus {
    /// Whether the app is accepting MCP sessions on its socket.
    pub listening: bool,
    /// Version of the app; a session whose version differs runs another
    /// release and needs its AI client restarted.
    pub app_version: String,
    pub attached: Vec<McpAttachedSession>,
    pub standalone: Vec<McpStandaloneServer>,
}

// ── Attached sessions ─────────────────────────────────────────────────────────

type ChangeListener = Arc<dyn Fn(&[McpAttachedSession]) + Send + Sync>;

#[derive(Default)]
struct RegistryInner {
    next_id: u32,
    sessions: Vec<McpAttachedSession>,
    listening: bool,
    socket_path: Option<PathBuf>,
}

/// Sessions attached to this app process. Cheap to clone; clones share state.
#[derive(Clone)]
pub struct McpSessionRegistry {
    inner: Arc<Mutex<RegistryInner>>,
    on_change: Option<ChangeListener>,
    /// Set once, with the message for requests still in flight, when every
    /// session must close (the app is quitting).
    closing: Arc<tokio::sync::watch::Sender<Option<String>>>,
}

impl Default for McpSessionRegistry {
    fn default() -> Self {
        Self {
            inner: Arc::default(),
            on_change: None,
            closing: Arc::new(tokio::sync::watch::channel(None).0),
        }
    }
}

impl McpSessionRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    /// Call `listener` with the new list after every change.
    pub fn with_listener(listener: impl Fn(&[McpAttachedSession]) + Send + Sync + 'static) -> Self {
        Self {
            on_change: Some(Arc::new(listener)),
            ..Self::default()
        }
    }

    /// Ask every session to close, answering its requests in flight with
    /// `message`. New sessions are refused from then on.
    pub fn close_all(&self, message: impl Into<String>) {
        self.closing.send_replace(Some(message.into()));
    }

    pub fn is_closing(&self) -> bool {
        self.closing.borrow().is_some()
    }

    /// Resolves with the message once [`McpSessionRegistry::close_all`] is called.
    pub async fn closed(&self) -> String {
        let mut closing = self.closing.subscribe();
        let message = match closing.wait_for(Option::is_some).await {
            Ok(message) => message.clone(),
            Err(_) => None,
        };
        match message {
            Some(message) => message,
            None => std::future::pending().await,
        }
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, RegistryInner> {
        self.inner.lock().unwrap_or_else(|p| p.into_inner())
    }

    fn changed(&self, sessions: Vec<McpAttachedSession>) {
        if let Some(listener) = &self.on_change {
            listener(&sessions);
        }
    }

    /// Register a session of a `keynobi --mcp` binary at `version`, or `None`
    /// when [`MAX_ATTACHED_SESSIONS`] are attached.
    pub fn add(&self, pid: Option<u32>, project: Option<&Path>, version: &str) -> Option<u32> {
        let (id, snapshot) = {
            let mut inner = self.lock();
            if inner.sessions.len() >= MAX_ATTACHED_SESSIONS {
                return None;
            }
            inner.next_id = inner.next_id.wrapping_add(1);
            let id = inner.next_id;
            inner.sessions.push(McpAttachedSession {
                id,
                pid,
                project: project.map(|p| p.to_string_lossy().into_owned()),
                connected_at: chrono::Utc::now().to_rfc3339(),
                client_name: None,
                version: version.to_string(),
            });
            (id, inner.sessions.clone())
        };
        self.changed(snapshot);
        Some(id)
    }

    pub fn set_client_name(&self, id: u32, name: &str) {
        let snapshot = {
            let mut inner = self.lock();
            let Some(session) = inner.sessions.iter_mut().find(|s| s.id == id) else {
                return;
            };
            session.client_name = Some(name.to_string());
            inner.sessions.clone()
        };
        self.changed(snapshot);
    }

    pub fn remove(&self, id: u32) {
        let snapshot = {
            let mut inner = self.lock();
            let before = inner.sessions.len();
            inner.sessions.retain(|s| s.id != id);
            if inner.sessions.len() == before {
                return;
            }
            inner.sessions.clone()
        };
        self.changed(snapshot);
    }

    pub fn sessions(&self) -> Vec<McpAttachedSession> {
        self.lock().sessions.clone()
    }

    /// Record the socket the app is serving on (or `None` when it is not).
    pub fn set_listening(&self, socket_path: Option<PathBuf>) {
        let mut inner = self.lock();
        inner.listening = socket_path.is_some();
        inner.socket_path = socket_path;
    }

    pub fn is_listening(&self) -> bool {
        self.lock().listening
    }

    /// The socket this process bound, so it can be removed on exit.
    pub fn take_socket_path(&self) -> Option<PathBuf> {
        let mut inner = self.lock();
        inner.listening = false;
        inner.socket_path.take()
    }
}

/// Removes an attached session from the registry when the connection ends,
/// however the serving task finishes.
pub struct SessionGuard {
    registry: McpSessionRegistry,
    pub id: u32,
}

impl SessionGuard {
    pub fn new(registry: McpSessionRegistry, id: u32) -> Self {
        Self { registry, id }
    }
}

impl Drop for SessionGuard {
    fn drop(&mut self) {
        self.registry.remove(self.id);
    }
}

// ── Standalone records ────────────────────────────────────────────────────────

fn sessions_dir_in(data_dir: &Path) -> PathBuf {
    data_dir.join(SESSIONS_DIR)
}

fn record_path_in(data_dir: &Path, pid: u32) -> PathBuf {
    sessions_dir_in(data_dir).join(format!("{pid}.json"))
}

/// Record this standalone server. Removed by [`remove_standalone_record`].
pub fn write_standalone_record(project: Option<&Path>, reason: &str) {
    let record = McpStandaloneServer {
        pid: std::process::id(),
        started_at: chrono::Utc::now().to_rfc3339(),
        project: project.map(|p| p.to_string_lossy().into_owned()),
        reason: reason.to_string(),
        version: Some(APP_VERSION.to_string()),
        exe: std::env::current_exe()
            .ok()
            .map(|p| p.to_string_lossy().into_owned()),
    };
    if let Err(e) = write_record_in(&settings_manager::data_dir(), &record) {
        tracing::warn!("Failed to write the MCP session record: {e}");
    }
}

fn write_record_in(data_dir: &Path, record: &McpStandaloneServer) -> std::io::Result<()> {
    let dir = sessions_dir_in(data_dir);
    std::fs::create_dir_all(&dir)?;
    let path = record_path_in(data_dir, record.pid);
    let tmp = settings_manager::unique_tmp_path(&path);
    std::fs::write(&tmp, serde_json::to_vec(record)?)?;
    std::fs::rename(&tmp, &path).inspect_err(|_| {
        let _ = std::fs::remove_file(&tmp);
    })
}

pub fn remove_standalone_record() {
    let _ = std::fs::remove_file(record_path_in(
        &settings_manager::data_dir(),
        std::process::id(),
    ));
}

/// Standalone servers that are still running. Records of exited processes
/// (or of a PID now used by another program) and unreadable records are deleted.
pub fn list_standalone_servers() -> Vec<McpStandaloneServer> {
    list_standalone_in(&settings_manager::data_dir(), process_is_record_owner)
        .into_iter()
        .map(|server| McpStandaloneServer {
            exe: None,
            ..server
        })
        .collect()
}

fn list_standalone_in(
    data_dir: &Path,
    is_alive: impl Fn(&McpStandaloneServer) -> bool,
) -> Vec<McpStandaloneServer> {
    let Ok(entries) = std::fs::read_dir(sessions_dir_in(data_dir)) else {
        return Vec::new();
    };
    let mut servers = Vec::new();
    for entry in entries.flatten().take(MAX_STANDALONE_RECORDS) {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("json") {
            continue;
        }
        let record = std::fs::read(&path)
            .ok()
            .and_then(|bytes| serde_json::from_slice::<McpStandaloneServer>(&bytes).ok());
        match record {
            Some(record) if is_alive(&record) => servers.push(record),
            _ => {
                let _ = std::fs::remove_file(&path);
            }
        }
    }
    servers.sort_by(|a, b| a.started_at.cmp(&b.started_at));
    servers
}

/// Whether `record.pid` is alive and is still the process that wrote it.
fn process_is_record_owner(record: &McpStandaloneServer) -> bool {
    if !pid_is_alive(record.pid) {
        return false;
    }
    match (&record.exe, process_exe(record.pid)) {
        (Some(expected), Some(actual)) => Path::new(expected) == actual,
        // Cannot tell: trust the liveness check.
        _ => true,
    }
}

fn pid_is_alive(pid: u32) -> bool {
    let Ok(pid) = libc::pid_t::try_from(pid) else {
        return false;
    };
    if pid <= 0 {
        return false;
    }
    // SAFETY: kill(pid, 0) sends no signal; it only checks the process exists.
    let ret = unsafe { libc::kill(pid, 0) };
    ret == 0 || std::io::Error::last_os_error().raw_os_error() == Some(libc::EPERM)
}

#[cfg(target_os = "macos")]
fn process_exe(pid: u32) -> Option<PathBuf> {
    let pid = libc::c_int::try_from(pid).ok()?;
    let mut buf = vec![0u8; libc::PROC_PIDPATHINFO_MAXSIZE as usize];
    // SAFETY: the buffer is valid for `buf.len()` bytes and proc_pidpath
    // writes at most that many.
    let len = unsafe { libc::proc_pidpath(pid, buf.as_mut_ptr().cast(), buf.len() as u32) };
    if len <= 0 {
        return None;
    }
    buf.truncate(len as usize);
    String::from_utf8(buf).ok().map(PathBuf::from)
}

#[cfg(target_os = "linux")]
fn process_exe(pid: u32) -> Option<PathBuf> {
    std::fs::read_link(format!("/proc/{pid}/exe")).ok()
}

#[cfg(not(any(target_os = "macos", target_os = "linux")))]
fn process_exe(_pid: u32) -> Option<PathBuf> {
    None
}

/// Delete the single-slot PID file older releases wrote.
pub fn remove_legacy_pid_file() {
    let _ = std::fs::remove_file(settings_manager::data_dir().join(LEGACY_PID_FILE));
}

#[cfg(test)]
mod tests {
    use super::*;

    fn record(pid: u32, exe: Option<&str>) -> McpStandaloneServer {
        McpStandaloneServer {
            pid,
            started_at: format!("2026-01-01T00:00:{:02}Z", pid % 60),
            project: Some("/p".into()),
            reason: "the Keynobi app is not running".into(),
            version: Some("0.1.0".into()),
            exe: exe.map(str::to_string),
        }
    }

    #[test]
    fn records_of_live_servers_are_listed_and_dead_ones_removed() {
        let dir = tempfile::tempdir().unwrap();
        write_record_in(dir.path(), &record(100, None)).unwrap();
        write_record_in(dir.path(), &record(200, None)).unwrap();
        std::fs::write(sessions_dir_in(dir.path()).join("300.json"), "not json").unwrap();

        let listed = list_standalone_in(dir.path(), |r| r.pid == 100);

        assert_eq!(listed, vec![record(100, None)]);
        assert!(record_path_in(dir.path(), 100).exists());
        assert!(
            !record_path_in(dir.path(), 200).exists(),
            "dead record kept"
        );
        assert!(!sessions_dir_in(dir.path()).join("300.json").exists());
    }

    #[test]
    fn a_record_without_a_version_reads_as_an_older_release() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(sessions_dir_in(dir.path())).unwrap();
        std::fs::write(
            record_path_in(dir.path(), 100),
            r#"{"pid":100,"startedAt":"2026-01-01T00:00:00Z","project":null,"reason":"r"}"#,
        )
        .unwrap();

        let listed = list_standalone_in(dir.path(), |_| true);

        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].version, None);
    }

    #[test]
    fn a_standalone_record_carries_the_binary_version() {
        write_standalone_record(None, "the Keynobi app is not running");
        let path = record_path_in(&settings_manager::data_dir(), std::process::id());
        let written = std::fs::read(&path).unwrap();
        remove_standalone_record();

        let server: McpStandaloneServer = serde_json::from_slice(&written).unwrap();
        assert_eq!(server.version.as_deref(), Some(APP_VERSION));
    }

    #[test]
    fn this_process_owns_its_record() {
        let exe = std::env::current_exe().unwrap();
        let mine = record(std::process::id(), Some(&exe.to_string_lossy()));
        assert!(process_is_record_owner(&mine));
    }

    #[test]
    fn a_reused_pid_running_another_program_is_not_the_owner() {
        // This process is alive, but it is not the binary the record names.
        let other = record(std::process::id(), Some("/Applications/Other.app/other"));
        assert!(!process_is_record_owner(&other));
    }

    #[test]
    fn an_exited_process_is_not_alive() {
        let mut child = std::process::Command::new("/usr/bin/true").spawn().unwrap();
        let pid = child.id();
        child.wait().unwrap();
        assert!(!pid_is_alive(pid));
        assert!(!process_is_record_owner(&record(pid, None)));
    }

    #[test]
    fn registry_tracks_sessions_and_notifies_on_change() {
        let seen = Arc::new(Mutex::new(Vec::<usize>::new()));
        let registry = McpSessionRegistry::with_listener({
            let seen = seen.clone();
            move |sessions| seen.lock().unwrap().push(sessions.len())
        });

        let a = registry.add(Some(1), None, "0.1.0").unwrap();
        let b = registry
            .add(Some(2), Some(Path::new("/p")), "0.2.0")
            .unwrap();
        registry.set_client_name(b, "client");
        drop(SessionGuard::new(registry.clone(), a));

        let sessions = registry.sessions();
        assert_eq!(sessions.len(), 1);
        assert_eq!(sessions[0].id, b);
        assert_eq!(sessions[0].project.as_deref(), Some("/p"));
        assert_eq!(sessions[0].client_name.as_deref(), Some("client"));
        assert_eq!(sessions[0].version, "0.2.0");
        assert_eq!(*seen.lock().unwrap(), vec![1, 2, 2, 1]);
    }

    #[test]
    fn registry_refuses_sessions_past_the_cap() {
        let registry = McpSessionRegistry::new();
        for _ in 0..MAX_ATTACHED_SESSIONS {
            assert!(registry.add(None, None, APP_VERSION).is_some());
        }
        assert_eq!(registry.add(None, None, APP_VERSION), None);
    }
}
