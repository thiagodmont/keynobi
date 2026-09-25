//! Lets `keynobi --mcp` attach to the running app instead of starting a
//! server with its own state.
//!
//! The app listens on a Unix socket in the data directory. A connecting
//! process first sends one JSON line ([`AttachRequest`]) and the app answers
//! with one JSON line ([`AttachReply`]). After an accepted reply the
//! connection carries MCP JSON-RPC, served by an [`AndroidMcpServer`] built
//! from the app's own state; `keynobi --mcp` just pipes its stdio to the socket.
use crate::services::fs_manager;
use crate::services::mcp_activity::{self, McpActivityEntry};
use crate::services::mcp_server::{AndroidMcpServer, LoggingMcpServer, ProjectSelection};
use crate::services::mcp_sessions::{McpSessionRegistry, SessionGuard};
use crate::services::settings_manager;
use crate::FsState;
use rmcp::ServiceExt;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;
use tokio::io::{AsyncBufReadExt, AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt, BufReader};
use tokio::net::unix::{OwnedReadHalf, OwnedWriteHalf};
use tokio::net::{UnixListener, UnixStream};
use tracing::{info, warn};

/// Version of the attach handshake. Bump it when the handshake changes.
pub const ATTACH_PROTOCOL: u32 = 1;
const SOCKET_FILE: &str = "mcp.sock";
/// `sun_path` holds 104 bytes on macOS, including the terminating NUL.
const MAX_SOCKET_PATH_BYTES: usize = 103;
/// Longest handshake line either side reads.
const MAX_HANDSHAKE_BYTES: u64 = 4096;
/// How long the app waits for a connecting process to send its request.
const REQUEST_TIMEOUT: Duration = Duration::from_secs(5);
/// How long `keynobi --mcp` waits to connect and get a reply before running standalone.
pub const ATTACH_TIMEOUT: Duration = Duration::from_millis(1500);
/// How long the forwarder keeps relaying replies after its client closed stdin.
const DRAIN_TIMEOUT: Duration = Duration::from_secs(5);

const APP_VERSION: &str = env!("CARGO_PKG_VERSION");

/// The socket the app serves MCP sessions on.
pub fn socket_path() -> PathBuf {
    settings_manager::data_dir().join(SOCKET_FILE)
}

// ── Handshake ─────────────────────────────────────────────────────────────────

/// First line a process sends after connecting.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AttachRequest {
    /// Handshake version ([`ATTACH_PROTOCOL`]).
    pub attach: u32,
    /// Version of the binary that is attaching.
    pub version: String,
    /// The Gradle build the client asked for, or `None` to follow the app.
    pub project: Option<PathBuf>,
    /// PID of the forwarding process.
    pub pid: Option<u32>,
    /// How the client chose `project`.
    #[serde(default)]
    pub selected_by: Option<ProjectSelection>,
}

impl AttachRequest {
    pub fn new(project: Option<PathBuf>, selected_by: Option<ProjectSelection>) -> Self {
        Self {
            attach: ATTACH_PROTOCOL,
            version: APP_VERSION.to_string(),
            project,
            pid: Some(std::process::id()),
            selected_by,
        }
    }
}

/// The app's answer to an [`AttachRequest`].
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AttachReply {
    pub accepted: bool,
    /// The app's open project, when accepted.
    #[serde(default)]
    pub project: Option<PathBuf>,
    /// Why the request was refused.
    #[serde(default)]
    pub reason: Option<String>,
    /// The app's version.
    pub version: String,
}

impl AttachReply {
    fn accept(project: Option<PathBuf>) -> Self {
        Self {
            accepted: true,
            project,
            reason: None,
            version: APP_VERSION.to_string(),
        }
    }

    fn reject(reason: impl Into<String>) -> Self {
        Self {
            accepted: false,
            project: None,
            reason: Some(reason.into()),
            version: APP_VERSION.to_string(),
        }
    }
}

/// The project open in the app, as `FsState` holds it.
#[derive(Debug, Clone, Default)]
pub struct AppProject {
    pub project_root: Option<PathBuf>,
    pub gradle_root: Option<PathBuf>,
}

impl AppProject {
    pub async fn of(fs_state: &FsState) -> Self {
        let fs = fs_state.0.lock().await;
        Self {
            project_root: fs.project_root.clone(),
            gradle_root: fs.gradle_root.clone(),
        }
    }

    /// The path to show for this project.
    pub fn display_path(&self) -> Option<&Path> {
        self.gradle_root.as_deref().or(self.project_root.as_deref())
    }

    /// Whether `requested` (a Gradle root or project folder) is this project.
    pub fn is(&self, requested: &Path) -> bool {
        let requested = canonical(requested);
        [&self.gradle_root, &self.project_root]
            .into_iter()
            .flatten()
            .any(|p| canonical(p) == requested)
    }
}

fn canonical(path: &Path) -> PathBuf {
    path.canonicalize().unwrap_or_else(|_| path.to_path_buf())
}

/// Decide an attach request against the app's open project. `Ok` carries the
/// project the session is pinned to (`None`: it follows the app).
pub fn decide_attach(request: &AttachRequest, app: &AppProject) -> Result<Option<PathBuf>, String> {
    if request.attach != ATTACH_PROTOCOL {
        return Err(format!(
            "attach protocol {} is not supported (Keynobi {APP_VERSION} speaks {ATTACH_PROTOCOL}, \
             the MCP server is version {})",
            request.attach, request.version
        ));
    }
    let Some(requested) = &request.project else {
        return Ok(None);
    };
    if app.is(requested) {
        return Ok(Some(canonical(requested)));
    }
    Err(match app.display_path() {
        Some(open) => format!(
            "Keynobi has another project open ({}); this MCP server is for {}",
            open.display(),
            requested.display()
        ),
        None => format!(
            "Keynobi has no project open; this MCP server is for {}",
            requested.display()
        ),
    })
}

/// Read one handshake line of at most [`MAX_HANDSHAKE_BYTES`].
async fn read_line<R: AsyncRead + Unpin>(reader: &mut BufReader<R>) -> std::io::Result<String> {
    let mut buf = Vec::new();
    (&mut *reader)
        .take(MAX_HANDSHAKE_BYTES)
        .read_until(b'\n', &mut buf)
        .await?;
    if buf.last() != Some(&b'\n') {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "handshake line missing or too long",
        ));
    }
    String::from_utf8(buf).map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))
}

async fn write_json_line<W: AsyncWrite + Unpin>(
    writer: &mut W,
    value: &impl Serialize,
) -> std::io::Result<()> {
    let mut line = serde_json::to_vec(value)?;
    line.push(b'\n');
    writer.write_all(&line).await?;
    writer.flush().await
}

// ── App side ──────────────────────────────────────────────────────────────────

/// Bind the app's socket at `path`.
///
/// `Ok(None)` when another app instance already answers on it. A socket file
/// nobody answers on is left over from a crash and is replaced.
pub fn bind_app_socket(path: &Path) -> Result<Option<UnixListener>, String> {
    check_socket_path(path)?;
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)
            .map_err(|e| format!("Failed to create {}: {e}", dir.display()))?;
        set_mode(dir, 0o700)?;
    }
    if let Ok(meta) = std::fs::symlink_metadata(path) {
        use std::os::unix::fs::FileTypeExt;
        if !meta.file_type().is_socket() {
            return Err(format!(
                "{} exists and is not a socket; remove it to let AI clients attach",
                path.display()
            ));
        }
        if std::os::unix::net::UnixStream::connect(path).is_ok() {
            return Ok(None);
        }
        std::fs::remove_file(path)
            .map_err(|e| format!("Failed to remove stale socket {}: {e}", path.display()))?;
    }
    let listener = UnixListener::bind(path)
        .map_err(|e| format!("Failed to listen on {}: {e}", path.display()))?;
    set_mode(path, 0o600)?;
    Ok(Some(listener))
}

fn check_socket_path(path: &Path) -> Result<(), String> {
    let len = path.as_os_str().len();
    if len > MAX_SOCKET_PATH_BYTES {
        return Err(format!(
            "Socket path is too long ({len} bytes, limit {MAX_SOCKET_PATH_BYTES}): {}",
            path.display()
        ));
    }
    Ok(())
}

fn set_mode(path: &Path, mode: u32) -> Result<(), String> {
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(mode))
        .map_err(|e| format!("Failed to set permissions on {}: {e}", path.display()))
}

/// Serve every connection on `listener` as its own MCP session over the app's
/// state. `make_server` builds a server over that state for each session.
/// Runs until the listener fails permanently.
pub async fn serve_mcp_socket<F>(
    listener: UnixListener,
    fs_state: FsState,
    registry: McpSessionRegistry,
    make_server: F,
) where
    F: Fn() -> AndroidMcpServer + Send + Sync + 'static,
{
    let make_server = Arc::new(make_server);
    loop {
        let stream = match listener.accept().await {
            Ok((stream, _)) => stream,
            Err(e) => {
                warn!("MCP socket accept failed: {e}");
                tokio::time::sleep(Duration::from_millis(100)).await;
                continue;
            }
        };
        let fs_state = fs_state.clone();
        let registry = registry.clone();
        let make_server = make_server.clone();
        tokio::spawn(async move {
            serve_connection(stream, fs_state, registry, make_server.as_ref()).await;
        });
    }
}

async fn serve_connection(
    stream: UnixStream,
    fs_state: FsState,
    registry: McpSessionRegistry,
    make_server: &(dyn Fn() -> AndroidMcpServer + Send + Sync),
) {
    // The socket is 0600 in a 0700 directory; this is defence in depth.
    // SAFETY: getuid has no preconditions.
    let my_uid = unsafe { libc::getuid() };
    match stream.peer_cred() {
        Ok(cred) if cred.uid() == my_uid => {}
        _ => {
            warn!("Refused an MCP socket connection from another user");
            return;
        }
    }

    let (read_half, mut write_half) = stream.into_split();
    let mut reader = BufReader::new(read_half);
    let request = match tokio::time::timeout(REQUEST_TIMEOUT, read_line(&mut reader)).await {
        Ok(Ok(line)) => serde_json::from_str::<AttachRequest>(&line),
        _ => return,
    };
    let request = match request {
        Ok(request) => request,
        Err(e) => {
            let _ = write_json_line(
                &mut write_half,
                &AttachReply::reject(format!("unreadable attach request: {e}")),
            )
            .await;
            return;
        }
    };

    let app = AppProject::of(&fs_state).await;
    let pinned = match decide_attach(&request, &app) {
        Ok(pinned) => pinned,
        Err(reason) => {
            info!("MCP attach refused: {reason}");
            let _ = write_json_line(&mut write_half, &AttachReply::reject(reason)).await;
            return;
        }
    };
    let Some(id) = registry.add(request.pid, pinned.as_deref()) else {
        let _ = write_json_line(
            &mut write_half,
            &AttachReply::reject(format!(
                "Keynobi already serves {} MCP sessions",
                crate::services::mcp_sessions::MAX_ATTACHED_SESSIONS
            )),
        )
        .await;
        return;
    };
    let _guard = SessionGuard::new(registry.clone(), id);
    let reply = AttachReply::accept(app.display_path().map(Path::to_path_buf));
    if write_json_line(&mut write_half, &reply).await.is_err() {
        return;
    }

    let project_label = pinned
        .as_ref()
        .map(|p| p.to_string_lossy().into_owned())
        .unwrap_or_else(|| "follows the app".into());
    mcp_activity::log_activity(&McpActivityEntry::lifecycle(format!(
        "Client attached (pid {}) — project: {project_label}",
        request
            .pid
            .map(|p| p.to_string())
            .unwrap_or_else(|| "?".into())
    )));

    let selection = match (&pinned, request.selected_by) {
        (None, _) => ProjectSelection::App,
        (Some(_), Some(how)) => how,
        (Some(_), None) => ProjectSelection::Argument,
    };
    let server = make_server().attached(pinned, selection);
    let logging = LoggingMcpServer::new(server).with_session(registry.clone(), id);
    match logging.serve((reader, write_half)).await {
        Ok(running) => {
            if let Err(e) = running.waiting().await {
                warn!("MCP session {id} ended with an error: {e}");
            }
        }
        Err(e) => warn!("MCP session {id} failed to initialize: {e}"),
    }
    mcp_activity::log_activity(&McpActivityEntry::lifecycle(format!(
        "Client detached (pid {})",
        request
            .pid
            .map(|p| p.to_string())
            .unwrap_or_else(|| "?".into())
    )));
}

/// Start serving MCP sessions for the running app. Logs and returns when the
/// socket cannot be bound or another instance already serves it.
pub async fn start_app_listener(app: tauri::AppHandle, registry: McpSessionRegistry) {
    use tauri::Manager;
    let path = socket_path();
    let listener = match bind_app_socket(&path) {
        Ok(Some(listener)) => listener,
        Ok(None) => {
            warn!(
                "Another Keynobi instance serves MCP on {}; AI clients attach to it",
                path.display()
            );
            return;
        }
        Err(e) => {
            warn!("MCP attach is unavailable: {e}");
            return;
        }
    };
    info!("Serving MCP sessions on {}", path.display());
    registry.set_listening(Some(path));
    let fs_state = app.state::<FsState>().inner().clone();
    let handle = app.clone();
    serve_mcp_socket(listener, fs_state, registry.clone(), move || {
        AndroidMcpServer::from_app_handle(&handle)
    })
    .await;
    // Only reached if the accept loop ever ends.
    remove_app_socket(&registry);
}

/// Remove the socket this process bound (on app exit).
pub fn remove_app_socket(registry: &McpSessionRegistry) {
    if let Some(path) = registry.take_socket_path() {
        let _ = std::fs::remove_file(path);
    }
}

// ── Client side (`keynobi --mcp`) ─────────────────────────────────────────────

/// A connection the app accepted.
pub struct Attached {
    pub reader: BufReader<OwnedReadHalf>,
    pub writer: OwnedWriteHalf,
    pub reply: AttachReply,
}

/// Connect to the app at `path` and ask to attach. `Err` carries why the
/// server has to run standalone, phrased for the user.
pub async fn try_attach(
    path: &Path,
    request: &AttachRequest,
    timeout: Duration,
) -> Result<Attached, String> {
    if check_socket_path(path).is_err() {
        return Err(format!(
            "the Keynobi socket path is too long to connect to ({})",
            path.display()
        ));
    }
    match tokio::time::timeout(timeout, handshake(path, request)).await {
        Ok(result) => result,
        Err(_) => Err(format!(
            "the Keynobi app did not answer within {} ms",
            timeout.as_millis()
        )),
    }
}

async fn handshake(path: &Path, request: &AttachRequest) -> Result<Attached, String> {
    let stream = UnixStream::connect(path)
        .await
        .map_err(|e| match e.kind() {
            std::io::ErrorKind::NotFound | std::io::ErrorKind::ConnectionRefused => {
                "the Keynobi app is not running".to_string()
            }
            _ => format!("could not reach the Keynobi app: {e}"),
        })?;
    let (read_half, mut writer) = stream.into_split();
    write_json_line(&mut writer, request)
        .await
        .map_err(|e| format!("could not reach the Keynobi app: {e}"))?;
    let mut reader = BufReader::new(read_half);
    let line = read_line(&mut reader)
        .await
        .map_err(|e| format!("the Keynobi app closed the connection: {e}"))?;
    let reply: AttachReply = serde_json::from_str(&line)
        .map_err(|e| format!("the Keynobi app sent an unreadable reply: {e}"))?;
    if !reply.accepted {
        let mut reason = reply
            .reason
            .clone()
            .unwrap_or_else(|| "the Keynobi app refused the session".into());
        if reply.version != APP_VERSION {
            reason.push_str(&format!(
                " (app version {}, MCP server version {APP_VERSION})",
                reply.version
            ));
        }
        return Err(reason);
    }
    Ok(Attached {
        reader,
        writer,
        reply,
    })
}

/// How an attached forwarding session ended.
#[derive(Debug, PartialEq, Eq)]
pub enum ForwardEnd {
    /// The MCP client closed stdin.
    ClientClosed,
    /// The app closed the socket (for example, it quit).
    AppClosed,
}

/// Relay the MCP client (`input`/`output`, normally stdio) to the attached
/// app until either side closes.
pub async fn forward<I, O>(attached: Attached, mut input: I, mut output: O) -> ForwardEnd
where
    I: AsyncRead + Unpin,
    O: AsyncWrite + Unpin,
{
    let Attached {
        mut reader,
        mut writer,
        ..
    } = attached;
    let down = async {
        let _ = tokio::io::copy_buf(&mut reader, &mut output).await;
        let _ = output.flush().await;
    };
    let up = async {
        let _ = tokio::io::copy(&mut input, &mut writer).await;
        let _ = writer.shutdown().await;
    };
    tokio::pin!(down);
    tokio::select! {
        _ = &mut down => ForwardEnd::AppClosed,
        _ = up => {
            // Let the app answer requests already sent, then stop.
            let _ = tokio::time::timeout(DRAIN_TIMEOUT, &mut down).await;
            ForwardEnd::ClientClosed
        }
    }
}

/// The project `keynobi --mcp` asks to attach to: the Gradle build containing
/// the selected folder, canonicalized.
pub fn attach_project_key(selected: &Path) -> PathBuf {
    let root = fs_manager::find_gradle_root(selected).unwrap_or_else(|| selected.to_path_buf());
    canonical(&root)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn gradle_build(dir: &Path) -> PathBuf {
        std::fs::create_dir_all(dir.join("app")).unwrap();
        std::fs::write(dir.join("settings.gradle.kts"), "").unwrap();
        dir.canonicalize().unwrap()
    }

    fn app_with(project: Option<&Path>) -> AppProject {
        AppProject {
            project_root: project.map(Path::to_path_buf),
            gradle_root: project.map(Path::to_path_buf),
        }
    }

    #[test]
    fn a_request_without_a_project_follows_the_app() {
        let tmp = tempfile::tempdir().unwrap();
        let open = gradle_build(&tmp.path().join("open"));
        let request = AttachRequest::new(None, None);
        assert_eq!(decide_attach(&request, &app_with(Some(&open))), Ok(None));
        assert_eq!(decide_attach(&request, &app_with(None)), Ok(None));
    }

    #[test]
    fn a_request_for_the_open_project_is_pinned_to_it() {
        let tmp = tempfile::tempdir().unwrap();
        let open = gradle_build(&tmp.path().join("open"));
        // Same folder through a non-canonical path.
        let request = AttachRequest::new(Some(open.join("app").join("..")), None);
        assert_eq!(
            decide_attach(&request, &app_with(Some(&open))),
            Ok(Some(open))
        );
    }

    #[test]
    fn a_request_for_another_project_is_refused_naming_the_open_one() {
        let tmp = tempfile::tempdir().unwrap();
        let open = gradle_build(&tmp.path().join("open"));
        let other = gradle_build(&tmp.path().join("other"));
        let request = AttachRequest::new(Some(other.clone()), None);

        let reason = decide_attach(&request, &app_with(Some(&open))).unwrap_err();
        assert!(reason.contains(&open.display().to_string()), "{reason}");
        assert!(reason.contains(&other.display().to_string()), "{reason}");

        let reason = decide_attach(&request, &app_with(None)).unwrap_err();
        assert!(reason.contains("no project open"), "{reason}");
    }

    #[test]
    fn an_unknown_protocol_version_is_refused_with_both_versions() {
        let request = AttachRequest {
            attach: ATTACH_PROTOCOL + 1,
            version: "9.9.9".into(),
            project: None,
            pid: None,
            selected_by: None,
        };
        let reason = decide_attach(&request, &app_with(None)).unwrap_err();
        assert!(reason.contains("9.9.9"), "{reason}");
        assert!(reason.contains(APP_VERSION), "{reason}");
    }

    #[test]
    fn attach_project_key_is_the_gradle_root_of_a_module_folder() {
        let tmp = tempfile::tempdir().unwrap();
        let root = gradle_build(&tmp.path().join("build"));
        assert_eq!(attach_project_key(&root.join("app")), root);
    }

    /// Socket paths are limited to 103 bytes on macOS; test dirs stay short.
    fn short_dir() -> tempfile::TempDir {
        tempfile::Builder::new()
            .prefix("kn")
            .tempdir_in("/tmp")
            .unwrap()
    }

    #[tokio::test]
    async fn a_too_long_socket_path_is_an_error_not_a_panic() {
        let long = PathBuf::from("/tmp")
            .join("x".repeat(120))
            .join(SOCKET_FILE);
        let err = bind_app_socket(&long).unwrap_err();
        assert!(err.contains("too long"), "{err}");
        let err = try_attach(&long, &AttachRequest::new(None, None), ATTACH_TIMEOUT)
            .await
            .err()
            .unwrap();
        assert!(err.contains("too long"), "{err}");
    }

    #[tokio::test]
    async fn a_stale_socket_file_is_replaced_and_a_live_one_is_left_alone() {
        let dir = short_dir();
        let path = dir.path().join(SOCKET_FILE);

        // A socket file whose listener is gone, as after a crash.
        drop(std::os::unix::net::UnixListener::bind(&path).unwrap());
        assert!(path.exists());
        let listener = bind_app_socket(&path)
            .unwrap()
            .expect("stale socket replaced");

        // A second instance finds the first one answering and does not unlink it.
        assert!(bind_app_socket(&path).unwrap().is_none());
        assert!(path.exists());
        drop(listener);

        use std::os::unix::fs::PermissionsExt;
        let listener = bind_app_socket(&path).unwrap().unwrap();
        let mode = std::fs::metadata(&path).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o600);
        let dir_mode = std::fs::metadata(dir.path()).unwrap().permissions().mode() & 0o777;
        assert_eq!(dir_mode, 0o700);
        drop(listener);
    }

    #[tokio::test]
    async fn a_regular_file_in_the_way_is_not_deleted() {
        let dir = short_dir();
        let path = dir.path().join(SOCKET_FILE);
        std::fs::write(&path, "user data").unwrap();
        assert!(bind_app_socket(&path).is_err());
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "user data");
    }

    #[tokio::test]
    async fn nothing_listening_means_the_app_is_not_running() {
        let dir = short_dir();
        let err = try_attach(
            &dir.path().join(SOCKET_FILE),
            &AttachRequest::new(None, None),
            ATTACH_TIMEOUT,
        )
        .await
        .err()
        .unwrap();
        assert_eq!(err, "the Keynobi app is not running");
    }

    #[tokio::test]
    async fn an_app_that_never_answers_times_out() {
        let dir = short_dir();
        let path = dir.path().join(SOCKET_FILE);
        let _listener = UnixListener::bind(&path).unwrap();
        let err = try_attach(
            &path,
            &AttachRequest::new(None, None),
            Duration::from_millis(200),
        )
        .await
        .err()
        .unwrap();
        assert!(err.contains("did not answer"), "{err}");
    }

    #[tokio::test]
    async fn the_forwarder_stops_when_the_app_closes_the_socket() {
        let (app_side, client_side) = UnixStream::pair().unwrap();
        let (read_half, writer) = client_side.into_split();
        let attached = Attached {
            reader: BufReader::new(read_half),
            writer,
            reply: AttachReply::accept(None),
        };
        // stdin that never ends, like an idle MCP client.
        let (_stdin_keepalive, stdin) = tokio::io::duplex(64);
        let (stdout, mut stdout_reader) = tokio::io::duplex(1024);

        let mut app_side = app_side;
        app_side
            .write_all(b"{\"jsonrpc\":\"2.0\"}\n")
            .await
            .unwrap();
        drop(app_side);

        let end = tokio::time::timeout(Duration::from_secs(5), forward(attached, stdin, stdout))
            .await
            .expect("forwarder must exit when the app goes away");
        assert_eq!(end, ForwardEnd::AppClosed);
        let mut out = String::new();
        stdout_reader.read_to_string(&mut out).await.unwrap();
        assert_eq!(out, "{\"jsonrpc\":\"2.0\"}\n");
    }
}
