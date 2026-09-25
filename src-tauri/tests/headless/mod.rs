//! Drives the real `keynobi --mcp` binary over stdio, the way an MCP client
//! does, against fake `adb` and `gradlew` scripts.
//!
//! Each [`Sandbox`] gets its own `HOME`, so the child's data directory
//! (`$HOME/.keynobi`) is a temp dir and a developer's real one is never read
//! or written. [`TestApp`] plays the running app: it serves attach requests
//! on a sandbox's socket from this test process.

use keynobi_lib::services::adb_manager::DeviceState;
use keynobi_lib::services::build_runner::{BuildState, BuildStateInner};
use keynobi_lib::services::mcp_attach;
use keynobi_lib::services::mcp_server::AndroidMcpServer;
use keynobi_lib::services::mcp_sessions::McpSessionRegistry;
use keynobi_lib::services::process_manager::ProcessManager;
use keynobi_lib::FsState;
use serde_json::{json, Value};
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::mpsc::{self, Receiver};
use std::time::Duration;

const REPLY_TIMEOUT: Duration = Duration::from_secs(30);

/// A throwaway home, Android SDK, and Gradle project for one server process.
pub struct Sandbox {
    _dir: tempfile::TempDir,
    pub home: PathBuf,
    pub project: PathBuf,
    sdk: PathBuf,
    jdk: PathBuf,
}

impl Sandbox {
    /// A project whose `gradlew` succeeds and an SDK whose `adb` sees no devices.
    pub fn new() -> Self {
        // Under /tmp: the app socket lives in the data dir, and macOS limits
        // socket paths to 103 bytes.
        let dir = tempfile::Builder::new()
            .prefix("kn")
            .tempdir_in("/tmp")
            .expect("create sandbox dir");
        let root = dir.path().canonicalize().expect("canonicalize sandbox dir");
        let home = root.join("home");
        let project = root.join("project");
        let sdk = root.join("sdk");
        std::fs::create_dir_all(home.join(".keynobi")).unwrap();
        std::fs::create_dir_all(sdk.join("platform-tools")).unwrap();
        std::fs::create_dir_all(&project).unwrap();
        std::fs::write(
            project.join("settings.gradle.kts"),
            "rootProject.name = \"sandbox\"\n",
        )
        .unwrap();
        // A configured JDK stops the server from searching this machine's
        // Android Studio and installed JDKs.
        let jdk = root.join("jdk");
        write_fake_jdk(&jdk, "17.0.9");

        let sandbox = Self {
            _dir: dir,
            home,
            project,
            sdk,
            jdk,
        };
        // The user trusted the project in the app.
        sandbox.write_projects(json!([project_entry(&sandbox.project, json!(true))]), None);
        sandbox.write_adb("echo 'List of devices attached'");
        sandbox.write_gradlew("echo 'BUILD SUCCESSFUL in 1s'");
        sandbox
    }

    /// Rewrite the settings with this project registry and last active project.
    pub fn write_projects(&self, recent_projects: Value, last_active_project: Option<&Path>) {
        std::fs::write(
            self.home.join(".keynobi").join("settings.json"),
            json!({
                "android": { "sdkPath": self.sdk },
                "java": { "home": self.jdk },
                "recentProjects": recent_projects,
                "lastActiveProject": last_active_project,
            })
            .to_string(),
        )
        .unwrap();
    }

    /// Replace the fake `adb` body. Every invocation's arguments are recorded
    /// first; see [`Sandbox::adb_calls`].
    pub fn write_adb(&self, body: &str) {
        let record = self.adb_record();
        let adb = self.sdk.join("platform-tools").join("adb");
        write_script(
            &adb,
            &format!("echo \"$*\" >> '{}'\n{body}", record.display()),
        );
        run_once(&adb);
        let _ = std::fs::remove_file(&record);
    }

    /// The fake Android SDK the settings name.
    pub fn sdk(&self) -> &Path {
        &self.sdk
    }

    /// Replace the project's fake `gradlew` body.
    pub fn write_gradlew(&self, body: &str) {
        write_script(&self.project.join("gradlew"), body);
    }

    /// The argument lists `adb` was invoked with, one string per call.
    pub fn adb_calls(&self) -> Vec<String> {
        std::fs::read_to_string(self.adb_record())
            .unwrap_or_default()
            .lines()
            .map(str::to_string)
            .collect()
    }

    fn adb_record(&self) -> PathBuf {
        self.sdk.join("adb-calls.txt")
    }

    /// Launch `keynobi --mcp --project <project>` and complete the MCP handshake.
    pub fn start(&self) -> McpClient {
        self.start_in(&self.project, Some(&self.project))
    }

    /// The socket the app would serve on for this sandbox's data dir.
    pub fn socket_path(&self) -> PathBuf {
        self.home.join(".keynobi").join("mcp.sock")
    }

    /// The activity log in this sandbox's data dir.
    pub fn activity_log(&self) -> String {
        std::fs::read_to_string(self.home.join(".keynobi").join("mcp-activity.jsonl"))
            .unwrap_or_default()
    }

    /// `keynobi --mcp [--project <project>]` in `working_dir`, not yet spawned.
    pub fn command(&self, working_dir: &Path, project: Option<&Path>) -> Command {
        let mut command = Command::new(env!("CARGO_BIN_EXE_keynobi"));
        command.arg("--mcp");
        if let Some(project) = project {
            command.arg("--project").arg(project);
        }
        command
            .env("HOME", &self.home)
            .env_remove("GRADLE_USER_HOME")
            .env_remove("RUST_LOG")
            .current_dir(working_dir);
        command
    }

    /// Launch `keynobi --mcp [--project <project>]` in `working_dir`.
    pub fn start_in(&self, working_dir: &Path, project: Option<&Path>) -> McpClient {
        let mut child = self
            .command(working_dir, project)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .expect("spawn keynobi --mcp");

        let stdin = child.stdin.take().unwrap();
        let stdout = child.stdout.take().unwrap();
        let (tx, lines) = mpsc::channel();
        std::thread::spawn(move || {
            for line in BufReader::new(stdout).lines() {
                let Ok(line) = line else { break };
                if tx.send(line).is_err() {
                    break;
                }
            }
        });

        let mut client = McpClient {
            child,
            stdin,
            lines,
            next_id: 1,
            init: Value::Null,
        };
        client.initialize();
        client
    }
}

/// A project registry entry as the app writes it; `trusted` is `true`,
/// `false`, or `null` (never asked).
pub fn project_entry(path: &Path, trusted: Value) -> Value {
    json!({
        "id": path.to_string_lossy(),
        "path": path,
        "name": path.file_name().map(|n| n.to_string_lossy().into_owned()),
        "gradleRoot": path,
        "lastOpened": "2026-01-01T00:00:00Z",
        "pinned": false,
        "trusted": trusted,
    })
}

/// A JDK home whose `java -version` reports `version`.
pub fn write_fake_jdk(home: &Path, version: &str) {
    std::fs::create_dir_all(home.join("bin")).unwrap();
    std::fs::write(
        home.join("release"),
        format!("JAVA_VERSION=\"{version}\"\n"),
    )
    .unwrap();
    let java = home.join("bin").join("java");
    write_script(
        &java,
        &format!("echo 'openjdk version \"{version}\" 2025-07-15' >&2"),
    );
    run_once(&java);
}

/// Run a freshly written fake executable once, with no arguments, and wait
/// for it.
///
/// macOS checks a new executable on its first run, which takes seconds on a
/// busy machine; later runs start at once. The server gives `adb` and
/// `java -version` only a few seconds, so a fake it runs against a deadline
/// is run once here first. The fake must exit promptly when run this way.
pub fn run_once(path: &Path) {
    Command::new(path)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .unwrap_or_else(|e| panic!("could not run {}: {e}", path.display()));
}

pub fn write_script(path: &Path, body: &str) {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).unwrap();
    }
    std::fs::write(path, format!("#!/bin/sh\n{body}\n")).unwrap();
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o755)).unwrap();
}

/// A connected MCP session with the child server. The process is killed on drop.
pub struct McpClient {
    child: Child,
    stdin: ChildStdin,
    lines: Receiver<String>,
    next_id: u64,
    /// The `initialize` result.
    pub init: Value,
}

/// The outcome of a `tools/call`.
#[derive(Debug)]
pub struct ToolOutput {
    pub text: String,
    pub is_error: bool,
}

impl McpClient {
    fn initialize(&mut self) {
        let result = self.request(
            "initialize",
            json!({
                "protocolVersion": "2025-06-18",
                "capabilities": {},
                "clientInfo": { "name": "keynobi-headless-test", "version": "0" },
            }),
        );
        assert!(
            result.get("serverInfo").is_some(),
            "initialize returned no serverInfo: {result}"
        );
        self.init = result;
        self.send(json!({ "jsonrpc": "2.0", "method": "notifications/initialized" }));
    }

    /// Send a request and wait for the response with the same id, skipping
    /// notifications. Panics on a JSON-RPC error or timeout.
    pub fn request(&mut self, method: &str, params: Value) -> Value {
        self.request_result(method, params)
            .unwrap_or_else(|error| panic!("{method} failed: {error}"))
    }

    /// Like [`McpClient::request`], but returns a JSON-RPC error instead of panicking.
    pub fn request_result(&mut self, method: &str, params: Value) -> Result<Value, Value> {
        let id = self.send_request(method, params);
        self.wait_response(id)
    }

    /// Send a request without waiting for its response; returns its id.
    pub fn send_request(&mut self, method: &str, params: Value) -> u64 {
        let id = self.next_id;
        self.next_id += 1;
        self.send(json!({ "jsonrpc": "2.0", "id": id, "method": method, "params": params }));
        id
    }

    /// Send a notification.
    pub fn notify(&mut self, method: &str, params: Value) {
        self.send(json!({ "jsonrpc": "2.0", "method": method, "params": params }));
    }

    /// Wait for the response to request `id`, skipping other messages.
    pub fn wait_response(&mut self, id: u64) -> Result<Value, Value> {
        self.wait_response_noting(id, |_| {})
    }

    /// Like [`McpClient::wait_response`], passing every other message to `other`.
    pub fn wait_response_noting(
        &mut self,
        id: u64,
        mut other: impl FnMut(&Value),
    ) -> Result<Value, Value> {
        loop {
            let line = self
                .lines
                .recv_timeout(REPLY_TIMEOUT)
                .unwrap_or_else(|_| panic!("no reply to request {id} within {REPLY_TIMEOUT:?}"));
            let Ok(message) = serde_json::from_str::<Value>(&line) else {
                panic!("server wrote a non-JSON line to stdout: {line}");
            };
            if message.get("id") != Some(&json!(id)) {
                other(&message);
                continue;
            }
            if let Some(error) = message.get("error") {
                return Err(error.clone());
            }
            return Ok(message["result"].clone());
        }
    }

    /// PID of the `keynobi --mcp` process.
    pub fn pid(&self) -> u32 {
        self.child.id()
    }

    /// Names of the tools the server advertises.
    pub fn tool_names(&mut self) -> Vec<String> {
        self.request("tools/list", json!({}))["tools"]
            .as_array()
            .expect("tools/list returned no tools array")
            .iter()
            .filter_map(|t| t["name"].as_str().map(str::to_string))
            .collect()
    }

    /// The `instructions` the server gave at initialize.
    pub fn instructions(&self) -> &str {
        self.init["instructions"].as_str().unwrap_or_default()
    }

    /// Call a tool that returns JSON and parse it.
    pub fn call_tool_json(&mut self, name: &str, arguments: Value) -> Value {
        let out = self.call_tool(name, arguments);
        assert!(!out.is_error, "{name}: {}", out.text);
        serde_json::from_str(&out.text).unwrap_or_else(|_| panic!("{name}: {}", out.text))
    }

    pub fn call_tool(&mut self, name: &str, arguments: Value) -> ToolOutput {
        let result = self.request(
            "tools/call",
            json!({ "name": name, "arguments": arguments }),
        );
        let text = result["content"]
            .as_array()
            .map(|blocks| {
                blocks
                    .iter()
                    .filter_map(|b| b["text"].as_str())
                    .collect::<Vec<_>>()
                    .join("\n")
            })
            .unwrap_or_default();
        ToolOutput {
            text,
            is_error: result["isError"].as_bool().unwrap_or(false),
        }
    }

    /// Call a tool that must be rejected with a JSON-RPC error (invalid
    /// arguments); returns the error message.
    pub fn call_tool_rejected(&mut self, name: &str, arguments: Value) -> String {
        match self.request_result(
            "tools/call",
            json!({ "name": name, "arguments": arguments }),
        ) {
            Ok(result) => panic!("{name} was not rejected: {result}"),
            Err(error) => error["message"].as_str().unwrap_or_default().to_string(),
        }
    }

    fn send(&mut self, message: Value) {
        writeln!(self.stdin, "{message}").expect("write to keynobi --mcp");
        self.stdin.flush().unwrap();
    }
}

impl Drop for McpClient {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

/// The running app, as far as attaching is concerned: this test process
/// serves attach requests on a sandbox's socket over state it owns, the way
/// the app serves them over its managed state.
pub struct TestApp {
    rt: Option<tokio::runtime::Runtime>,
    pub fs_state: FsState,
    pub build_state: BuildState,
    pub process_manager: ProcessManager,
    pub registry: McpSessionRegistry,
}

impl TestApp {
    /// Listen on `sandbox`'s socket with `project` open.
    pub fn listen(sandbox: &Sandbox, project: Option<&Path>) -> Self {
        let rt = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(2)
            .enable_all()
            .build()
            .expect("build test app runtime");
        let app = Self {
            rt: None,
            fs_state: FsState::new(),
            build_state: crate::common::isolated_build_state(),
            process_manager: ProcessManager::new(),
            registry: McpSessionRegistry::new(),
        };
        app.open(project);
        app.serve(rt, sandbox)
    }

    fn serve(mut self, rt: tokio::runtime::Runtime, sandbox: &Sandbox) -> Self {
        let listener = rt
            .block_on(async { mcp_attach::bind_app_socket(&sandbox.socket_path()) })
            .expect("bind the app socket")
            .expect("no other app is listening");
        let (fs_state, build_state) = (self.fs_state.clone(), self.build_state.clone());
        let device_state = DeviceState::new();
        let logcat_state = keynobi_lib::commands::logcat::new_logcat_state();
        let process_manager = self.process_manager.clone();
        rt.spawn(mcp_attach::serve_mcp_socket(
            listener,
            self.fs_state.clone(),
            self.registry.clone(),
            move || {
                AndroidMcpServer::new_headless(
                    build_state.clone(),
                    device_state.clone(),
                    logcat_state.clone(),
                    fs_state.clone(),
                    process_manager.clone(),
                    None,
                )
            },
        ));
        self.rt = Some(rt);
        self
    }

    /// Quit the way the app does: cancel the build, answer and close every
    /// attached session.
    pub fn quit(&self) {
        let rt = self.rt.as_ref().expect("the app is listening");
        rt.block_on(mcp_attach::quit_sessions(
            &self.build_state,
            &self.process_manager,
            &self.registry,
        ));
    }

    /// Run `future` on the app's runtime.
    pub fn block_on<F: std::future::Future>(&self, future: F) -> F::Output {
        self.rt
            .as_ref()
            .expect("the app is listening")
            .block_on(future)
    }

    /// Switch the app to `project` (or close it).
    pub fn open(&self, project: Option<&Path>) {
        let mut fs = self.fs_state.0.blocking_lock();
        fs.project_root = project.map(Path::to_path_buf);
        fs.gradle_root = project.map(Path::to_path_buf);
    }

    /// Record in this process's settings that the user trusted `project`.
    pub fn trust(&self, project: &Path) {
        keynobi_lib::services::settings_manager::mutate_settings(|settings| {
            settings
                .recent_projects
                .push(keynobi_lib::models::settings::ProjectEntry {
                    id: project.to_string_lossy().into_owned(),
                    path: project.to_string_lossy().into_owned(),
                    name: "sandbox".into(),
                    gradle_root: Some(project.to_string_lossy().into_owned()),
                    trusted: Some(true),
                    ..Default::default()
                });
        })
        .expect("trust the project");
    }

    /// Wait until `done` holds for this app's build state.
    pub fn wait_for_build(&self, what: &str, done: impl Fn(&BuildStateInner) -> bool) {
        let deadline = std::time::Instant::now() + REPLY_TIMEOUT;
        loop {
            if done(&self.build_state.inner.blocking_lock()) {
                return;
            }
            assert!(std::time::Instant::now() < deadline, "timed out: {what}");
            std::thread::sleep(Duration::from_millis(20));
        }
    }

    /// Wait until `count` sessions are attached.
    pub fn wait_for_sessions(&self, count: usize) {
        let deadline = std::time::Instant::now() + REPLY_TIMEOUT;
        while self.registry.sessions().len() != count {
            assert!(
                std::time::Instant::now() < deadline,
                "expected {count} attached sessions, have {:?}",
                self.registry.sessions()
            );
            std::thread::sleep(Duration::from_millis(20));
        }
    }
}

impl Drop for TestApp {
    fn drop(&mut self) {
        if let Some(rt) = self.rt.take() {
            rt.shutdown_background();
        }
    }
}
