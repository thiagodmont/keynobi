//! Drives the real `keynobi --mcp` binary over stdio, the way an MCP client
//! does, against fake `adb` and `gradlew` scripts.
//!
//! Each [`Sandbox`] gets its own `HOME`, so the child's data directory
//! (`$HOME/.keynobi`) is a temp dir and a developer's real one is never read
//! or written.

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
}

impl Sandbox {
    /// A project whose `gradlew` succeeds and an SDK whose `adb` sees no devices.
    pub fn new() -> Self {
        let dir = tempfile::tempdir().expect("create sandbox dir");
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
        std::fs::write(
            home.join(".keynobi").join("settings.json"),
            json!({ "android": { "sdkPath": sdk } }).to_string(),
        )
        .unwrap();

        let sandbox = Self {
            _dir: dir,
            home,
            project,
            sdk,
        };
        sandbox.write_adb("echo 'List of devices attached'");
        sandbox.write_gradlew("echo 'BUILD SUCCESSFUL in 1s'");
        sandbox
    }

    /// Replace the fake `adb` body. Every invocation's arguments are recorded
    /// first; see [`Sandbox::adb_calls`].
    pub fn write_adb(&self, body: &str) {
        let record = self.adb_record();
        write_script(
            &self.sdk.join("platform-tools").join("adb"),
            &format!("echo \"$*\" >> '{}'\n{body}", record.display()),
        );
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
        let mut child = Command::new(env!("CARGO_BIN_EXE_keynobi"))
            .args(["--mcp", "--project"])
            .arg(&self.project)
            .env("HOME", &self.home)
            .env_remove("RUST_LOG")
            .current_dir(&self.project)
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
        };
        client.initialize();
        client
    }
}

fn write_script(path: &Path, body: &str) {
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
        let id = self.next_id;
        self.next_id += 1;
        self.send(json!({ "jsonrpc": "2.0", "id": id, "method": method, "params": params }));
        loop {
            let line = self
                .lines
                .recv_timeout(REPLY_TIMEOUT)
                .unwrap_or_else(|_| panic!("no reply to {method} within {REPLY_TIMEOUT:?}"));
            let Ok(message) = serde_json::from_str::<Value>(&line) else {
                panic!("server wrote a non-JSON line to stdout: {line}");
            };
            if message.get("id") != Some(&json!(id)) {
                continue;
            }
            if let Some(error) = message.get("error") {
                return Err(error.clone());
            }
            return Ok(message["result"].clone());
        }
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
