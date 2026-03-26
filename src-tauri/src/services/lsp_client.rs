use serde_json::{json, Value};
use std::collections::HashMap;
use std::path::Path;
use std::sync::atomic::{AtomicI64, Ordering};
use std::sync::Arc;
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, Command};
use tokio::sync::{mpsc, oneshot, Mutex};

#[derive(Debug, Clone, PartialEq)]
pub enum LspClientState {
    Starting,
    Running,
    ShuttingDown,
    Stopped,
    Error(String),
}

/// Environment variables forwarded to the LSP server process.
/// All fields are optional; only those present are set on the child process.
#[derive(Debug, Default)]
pub struct LspEnv {
    /// Path to the Android SDK root (`ANDROID_HOME` / `ANDROID_SDK_ROOT`).
    /// Required for Gradle to resolve Android dependencies and source roots.
    pub android_sdk_path: Option<String>,
    /// Path to the JDK home (`JAVA_HOME`).
    /// The bundled JBR handles running the LSP itself, but Gradle tasks may
    /// need a separate JDK for compilation.
    pub java_home: Option<String>,
}

pub struct LspClient {
    writer: Arc<Mutex<tokio::process::ChildStdin>>,
    pending: Arc<Mutex<HashMap<i64, oneshot::Sender<Value>>>>,
    next_id: Arc<AtomicI64>,
    child: Arc<Mutex<Option<Child>>>,
    state: Arc<Mutex<LspClientState>>,
    reader_handle: Option<tokio::task::JoinHandle<()>>,
    #[allow(dead_code)] // Kept alive to prevent the notification channel from closing
    notification_tx: mpsc::UnboundedSender<(String, Value)>,
    /// Server capabilities returned by the `initialize` response.
    /// Used by callers to check whether a given LSP method is supported before
    /// sending a request, preventing unnecessary round-trips and noisy errors.
    server_capabilities: Arc<Mutex<Value>>,
}

impl LspClient {
    pub async fn start(
        launch_script: &Path,
        workspace_root: &Path,
        system_dir: &Path,
        env: LspEnv,
        notification_tx: mpsc::UnboundedSender<(String, Value)>,
    ) -> Result<Self, String> {
        let mut cmd = Command::new(launch_script);
        cmd.arg("--stdio");
        cmd.arg(format!(
            "--system-path={}",
            system_dir.to_string_lossy()
        ));
        cmd.arg("--log-level=INFO");

        // Run from the workspace root so Gradle can discover `gradlew` and
        // all module `build.gradle` / `settings.gradle` files.
        cmd.current_dir(workspace_root);

        // On macOS the app's environment doesn't inherit shell variables like
        // ANDROID_HOME.  Pass them explicitly so Gradle can find the SDK.
        if let Some(ref sdk) = env.android_sdk_path {
            cmd.env("ANDROID_HOME", sdk);
            cmd.env("ANDROID_SDK_ROOT", sdk); // legacy alias still used by some tools
        }
        if let Some(ref java) = env.java_home {
            cmd.env("JAVA_HOME", java);
        }

        cmd.stdin(std::process::Stdio::piped());
        cmd.stdout(std::process::Stdio::piped());
        cmd.stderr(std::process::Stdio::piped());

        let mut child = cmd
            .spawn()
            .map_err(|e| format!("Failed to spawn Kotlin LSP: {e}"))?;

        let stdin = child
            .stdin
            .take()
            .ok_or("Failed to get stdin of LSP process")?;
        let stdout = child
            .stdout
            .take()
            .ok_or("Failed to get stdout of LSP process")?;
        let stderr = child.stderr.take();

        let writer = Arc::new(Mutex::new(stdin));
        let pending: Arc<Mutex<HashMap<i64, oneshot::Sender<Value>>>> =
            Arc::new(Mutex::new(HashMap::new()));

        let pending_clone = pending.clone();
        let notif_tx = notification_tx.clone();
        let writer_clone = writer.clone();

        let reader_handle = tokio::spawn(async move {
            let mut reader = BufReader::new(stdout);
            loop {
                match read_message(&mut reader).await {
                    Ok(Some(msg)) => {
                        handle_message(msg, &pending_clone, &notif_tx, &writer_clone).await;
                    }
                    Ok(None) => break,
                    Err(e) => {
                        tracing::error!("LSP read error: {}", e);
                        break;
                    }
                }
            }
        });

        if let Some(stderr) = stderr {
            let notif_tx_stderr = notification_tx.clone();
            tokio::spawn(async move {
                let mut reader = BufReader::new(stderr);
                let mut line = String::new();
                while reader.read_line(&mut line).await.unwrap_or(0) > 0 {
                    let trimmed = line.trim_end().to_owned();
                    tracing::debug!(target: "lsp_stderr", "{}", trimmed);
                    let _ = notif_tx_stderr.send((
                        "kotlin/stderr".into(),
                        serde_json::json!({ "message": trimmed }),
                    ));
                    line.clear();
                }
            });
        }

        let client = LspClient {
            writer,
            pending,
            next_id: Arc::new(AtomicI64::new(1)),
            child: Arc::new(Mutex::new(Some(child))),
            state: Arc::new(Mutex::new(LspClientState::Starting)),
            reader_handle: Some(reader_handle),
            notification_tx,
            server_capabilities: Arc::new(Mutex::new(Value::Null)),
        };

        client.initialize(workspace_root).await?;

        Ok(client)
    }

    async fn initialize(&self, workspace_root: &Path) -> Result<(), String> {
        let root_uri = format!("file://{}", workspace_root.to_string_lossy());

        let params = json!({
            "processId": std::process::id(),
            "rootUri": root_uri,
            "capabilities": {
                "textDocument": {
                    "synchronization": {
                        "didSave": true,
                        "willSave": false,
                        "willSaveWaitUntil": false
                    },
                    "completion": {
                        "completionItem": {
                            "snippetSupport": false,
                            "commitCharactersSupport": true,
                            "documentationFormat": ["markdown", "plaintext"],
                            "deprecatedSupport": true,
                            "labelDetailsSupport": true
                        },
                        "contextSupport": true
                    },
                    "hover": {
                        "contentFormat": ["markdown", "plaintext"]
                    },
                    "signatureHelp": {
                        "signatureInformation": {
                            "documentationFormat": ["markdown", "plaintext"],
                            "parameterInformation": { "labelOffsetSupport": true }
                        }
                    },
                    "definition": { "linkSupport": false },
                    "references": {},
                    "implementation": {},
                    "documentHighlight": {},
                    "documentSymbol": {
                        "hierarchicalDocumentSymbolSupport": true
                    },
                    "codeAction": {
                        "codeActionLiteralSupport": {
                            "codeActionKind": {
                                "valueSet": ["quickfix", "refactor", "source"]
                            }
                        }
                    },
                    "formatting": {},
                    "rangeFormatting": {},
                    "rename": { "prepareSupport": true },
                    "publishDiagnostics": { "relatedInformation": true },
                    "diagnostic": {}
                },
                "workspace": {
                    "workspaceFolders": true,
                    "symbol": {
                        "symbolKind": {
                            "valueSet": (1..=26).collect::<Vec<i32>>()
                        }
                    }
                }
            },
            "workspaceFolders": [{
                "uri": root_uri,
                "name": workspace_root.file_name()
                    .unwrap_or_default()
                    .to_string_lossy()
            }]
        });

        let result = self
            .request_with_timeout(
                "initialize",
                params,
                // JVM cold-start on first run can take 60-90 seconds.
                // Give it a full 3 minutes before giving up.
                std::time::Duration::from_secs(180),
            )
            .await?;

        // Store server capabilities so callers can check feature support without
        // making a round-trip on every use.
        let caps = result.get("capabilities").cloned().unwrap_or(Value::Null);
        *self.server_capabilities.lock().await = caps;

        tracing::info!("LSP initialized: {:?}", result.get("serverInfo"));

        self.notify("initialized", json!({})).await?;
        *self.state.lock().await = LspClientState::Running;

        Ok(())
    }

    /// Return a clone of the server capabilities reported in the `initialize`
    /// response.  Returns `Value::Null` before initialization completes.
    pub async fn server_capabilities(&self) -> Value {
        self.server_capabilities.lock().await.clone()
    }

    pub async fn request(&self, method: &str, params: Value) -> Result<Value, String> {
        self.request_with_timeout(method, params, std::time::Duration::from_secs(30))
            .await
    }

    async fn request_with_timeout(
        &self,
        method: &str,
        params: Value,
        timeout: std::time::Duration,
    ) -> Result<Value, String> {
        let id = self.next_id.fetch_add(1, Ordering::SeqCst);

        let msg = json!({
            "jsonrpc": "2.0",
            "id": id,
            "method": method,
            "params": params
        });

        let (tx, rx) = oneshot::channel();
        self.pending.lock().await.insert(id, tx);

        self.send_raw(&msg).await?;

        tracing::debug!("LSP request [{}] {}", id, method);

        let result = tokio::time::timeout(timeout, rx)
            .await
            .map_err(|_| format!("LSP request timed out: {method}"))?
            .map_err(|_| format!("LSP response channel closed for: {method}"))?;

        if let Some(error) = result.get("error") {
            return Err(format!(
                "LSP error: {}",
                error.get("message").and_then(|m| m.as_str()).unwrap_or("unknown")
            ));
        }

        Ok(result.get("result").cloned().unwrap_or(Value::Null))
    }

    pub async fn notify(&self, method: &str, params: Value) -> Result<(), String> {
        let msg = json!({
            "jsonrpc": "2.0",
            "method": method,
            "params": params
        });
        self.send_raw(&msg).await
    }

    async fn send_raw(&self, msg: &Value) -> Result<(), String> {
        let body = serde_json::to_string(msg)
            .map_err(|e| format!("JSON serialize error: {e}"))?;
        let header = format!("Content-Length: {}\r\n\r\n", body.len());

        let mut writer = self.writer.lock().await;
        writer
            .write_all(header.as_bytes())
            .await
            .map_err(|e| format!("LSP write error: {e}"))?;
        writer
            .write_all(body.as_bytes())
            .await
            .map_err(|e| format!("LSP write error: {e}"))?;
        writer
            .flush()
            .await
            .map_err(|e| format!("LSP flush error: {e}"))?;

        Ok(())
    }

    #[allow(dead_code)] // Will be used for completion cancellation
    pub async fn cancel_request(&self, id: i64) -> Result<(), String> {
        self.notify("$/cancelRequest", json!({ "id": id })).await
    }

    // ── Document Sync ─────────────────────────────────────────────────────

    pub async fn did_open(
        &self,
        path: &Path,
        language_id: &str,
        version: i32,
        text: &str,
    ) -> Result<(), String> {
        self.notify(
            "textDocument/didOpen",
            json!({
                "textDocument": {
                    "uri": path_to_uri(path),
                    "languageId": language_id,
                    "version": version,
                    "text": text
                }
            }),
        )
        .await
    }

    pub async fn did_change(
        &self,
        path: &Path,
        version: i32,
        text: &str,
    ) -> Result<(), String> {
        self.notify(
            "textDocument/didChange",
            json!({
                "textDocument": {
                    "uri": path_to_uri(path),
                    "version": version
                },
                "contentChanges": [{ "text": text }]
            }),
        )
        .await
    }

    pub async fn did_save(&self, path: &Path, text: &str) -> Result<(), String> {
        self.notify(
            "textDocument/didSave",
            json!({
                "textDocument": { "uri": path_to_uri(path) },
                "text": text
            }),
        )
        .await
    }

    pub async fn did_close(&self, path: &Path) -> Result<(), String> {
        self.notify(
            "textDocument/didClose",
            json!({
                "textDocument": { "uri": path_to_uri(path) }
            }),
        )
        .await
    }

    // ── LSP Requests ──────────────────────────────────────────────────────

    pub async fn completion(&self, path: &Path, line: u32, col: u32) -> Result<Value, String> {
        self.request(
            "textDocument/completion",
            json!({
                "textDocument": { "uri": path_to_uri(path) },
                "position": { "line": line, "character": col }
            }),
        )
        .await
    }

    pub async fn hover(&self, path: &Path, line: u32, col: u32) -> Result<Value, String> {
        self.request(
            "textDocument/hover",
            json!({
                "textDocument": { "uri": path_to_uri(path) },
                "position": { "line": line, "character": col }
            }),
        )
        .await
    }

    pub async fn definition(&self, path: &Path, line: u32, col: u32) -> Result<Value, String> {
        self.request(
            "textDocument/definition",
            json!({
                "textDocument": { "uri": path_to_uri(path) },
                "position": { "line": line, "character": col }
            }),
        )
        .await
    }

    pub async fn references(
        &self,
        path: &Path,
        line: u32,
        col: u32,
        include_declaration: bool,
    ) -> Result<Value, String> {
        self.request(
            "textDocument/references",
            json!({
                "textDocument": { "uri": path_to_uri(path) },
                "position": { "line": line, "character": col },
                "context": { "includeDeclaration": include_declaration }
            }),
        )
        .await
    }

    pub async fn implementation(&self, path: &Path, line: u32, col: u32) -> Result<Value, String> {
        self.request(
            "textDocument/implementation",
            json!({
                "textDocument": { "uri": path_to_uri(path) },
                "position": { "line": line, "character": col }
            }),
        )
        .await
    }

    pub async fn document_symbol(&self, path: &Path) -> Result<Value, String> {
        self.request(
            "textDocument/documentSymbol",
            json!({
                "textDocument": { "uri": path_to_uri(path) }
            }),
        )
        .await
    }

    pub async fn workspace_symbol(&self, query: &str) -> Result<Value, String> {
        self.request(
            "workspace/symbol",
            json!({ "query": query }),
        )
        .await
    }

    pub async fn code_action(
        &self,
        path: &Path,
        start_line: u32,
        start_col: u32,
        end_line: u32,
        end_col: u32,
    ) -> Result<Value, String> {
        self.request(
            "textDocument/codeAction",
            json!({
                "textDocument": { "uri": path_to_uri(path) },
                "range": {
                    "start": { "line": start_line, "character": start_col },
                    "end": { "line": end_line, "character": end_col }
                },
                "context": { "diagnostics": [] }
            }),
        )
        .await
    }

    pub async fn rename(
        &self,
        path: &Path,
        line: u32,
        col: u32,
        new_name: &str,
    ) -> Result<Value, String> {
        self.request(
            "textDocument/rename",
            json!({
                "textDocument": { "uri": path_to_uri(path) },
                "position": { "line": line, "character": col },
                "newName": new_name
            }),
        )
        .await
    }

    pub async fn formatting(&self, path: &Path) -> Result<Value, String> {
        self.request(
            "textDocument/formatting",
            json!({
                "textDocument": { "uri": path_to_uri(path) },
                "options": {
                    "tabSize": 4,
                    "insertSpaces": true
                }
            }),
        )
        .await
    }

    pub async fn pull_diagnostics(&self, path: &Path) -> Result<Value, String> {
        self.request(
            "textDocument/diagnostic",
            json!({
                "textDocument": { "uri": path_to_uri(path) }
            }),
        )
        .await
    }

    pub async fn document_highlight(
        &self,
        path: &Path,
        line: u32,
        col: u32,
    ) -> Result<Value, String> {
        self.request(
            "textDocument/documentHighlight",
            json!({
                "textDocument": { "uri": path_to_uri(path) },
                "position": { "line": line, "character": col }
            }),
        )
        .await
    }

    pub async fn signature_help(
        &self,
        path: &Path,
        line: u32,
        col: u32,
    ) -> Result<Value, String> {
        self.request(
            "textDocument/signatureHelp",
            json!({
                "textDocument": { "uri": path_to_uri(path) },
                "position": { "line": line, "character": col }
            }),
        )
        .await
    }

    // ── Lifecycle ─────────────────────────────────────────────────────────

    pub async fn shutdown(&self) -> Result<(), String> {
        *self.state.lock().await = LspClientState::ShuttingDown;
        self.request("shutdown", Value::Null).await?;
        self.notify("exit", Value::Null).await?;
        *self.state.lock().await = LspClientState::Stopped;
        Ok(())
    }

    pub async fn kill(&self) {
        let mut child = self.child.lock().await;
        if let Some(ref mut c) = *child {
            c.kill().await.ok();
        }
        *self.state.lock().await = LspClientState::Stopped;
    }

    pub async fn get_state(&self) -> LspClientState {
        self.state.lock().await.clone()
    }

    pub async fn is_running(&self) -> bool {
        matches!(*self.state.lock().await, LspClientState::Running)
    }
}

impl Drop for LspClient {
    fn drop(&mut self) {
        if let Some(handle) = self.reader_handle.take() {
            handle.abort();
        }
        // Best-effort kill to prevent orphaned LSP processes
        let child = self.child.clone();
        tokio::spawn(async move {
            let mut guard = child.lock().await;
            if let Some(ref mut c) = *guard {
                c.kill().await.ok();
            }
        });
    }
}

// ── Protocol helpers ──────────────────────────────────────────────────────────

const MAX_MESSAGE_SIZE: usize = 64 * 1024 * 1024; // 64 MB

async fn read_message<R: tokio::io::AsyncRead + Unpin>(
    reader: &mut BufReader<R>,
) -> Result<Option<Value>, String> {
    let mut content_length: Option<usize> = None;
    let mut header_line = String::new();

    loop {
        header_line.clear();
        let bytes_read = reader
            .read_line(&mut header_line)
            .await
            .map_err(|e| format!("Header read error: {e}"))?;

        if bytes_read == 0 {
            return Ok(None);
        }

        let trimmed = header_line.trim();
        if trimmed.is_empty() {
            break;
        }

        if let Some(len_str) = trimmed.strip_prefix("Content-Length: ") {
            content_length = Some(
                len_str
                    .parse()
                    .map_err(|e| format!("Invalid Content-Length: {e}"))?,
            );
        }
    }

    let length = content_length.ok_or("Missing Content-Length header")?;

    if length > MAX_MESSAGE_SIZE {
        return Err(format!(
            "LSP message too large: {} bytes (max {})",
            length, MAX_MESSAGE_SIZE
        ));
    }

    let mut body = vec![0u8; length];
    reader
        .read_exact(&mut body)
        .await
        .map_err(|e| format!("Body read error: {e}"))?;

    let msg: Value = serde_json::from_slice(&body)
        .map_err(|e| format!("JSON parse error: {e}"))?;

    Ok(Some(msg))
}

async fn handle_message(
    msg: Value,
    pending: &Arc<Mutex<HashMap<i64, oneshot::Sender<Value>>>>,
    notif_tx: &mpsc::UnboundedSender<(String, Value)>,
    writer: &Arc<Mutex<tokio::process::ChildStdin>>,
) {
    if let Some(id) = msg.get("id").and_then(|v| v.as_i64()) {
        if let Some(method) = msg.get("method").and_then(|v| v.as_str()) {
            // Server-initiated request — must reply or server may hang
            tracing::debug!("LSP server request [{}]: {}", id, method);
            let response = match method {
                "client/registerCapability" | "client/unregisterCapability" => {
                    json!({ "jsonrpc": "2.0", "id": id, "result": null })
                }
                "window/showMessageRequest" => {
                    json!({ "jsonrpc": "2.0", "id": id, "result": null })
                }
                "workspace/configuration" => {
                    // The LSP spec requires one result element per requested item.
                    // Returning [] when N items are requested is malformed and
                    // causes the server to disable features like Gradle support.
                    let items = msg
                        .get("params")
                        .and_then(|p| p.get("items"))
                        .and_then(|i| i.as_array());

                    // Forward what the server requested to the Output panel so
                    // the developer can see if configuration is needed.
                    if let Some(items) = items {
                        let sections: Vec<&str> = items
                            .iter()
                            .filter_map(|i| i.get("section").and_then(|s| s.as_str()))
                            .collect();
                        if !sections.is_empty() {
                            let msg_text = format!(
                                "workspace/configuration requested: [{}]",
                                sections.join(", ")
                            );
                            let _ = notif_tx.send(("kotlin/client-debug".into(), json!({ "message": msg_text })));
                        }
                        // Return null for each requested item — correct per LSP spec.
                        let nulls: Vec<Value> = (0..items.len()).map(|_| Value::Null).collect();
                        json!({ "jsonrpc": "2.0", "id": id, "result": nulls })
                    } else {
                        json!({ "jsonrpc": "2.0", "id": id, "result": [] })
                    }
                }
                _ => {
                    json!({
                        "jsonrpc": "2.0",
                        "id": id,
                        "error": { "code": -32601, "message": "Method not supported" }
                    })
                }
            };
            if let Ok(body) = serde_json::to_string(&response) {
                let header = format!("Content-Length: {}\r\n\r\n", body.len());
                let mut w = writer.lock().await;
                let _ = w.write_all(header.as_bytes()).await;
                let _ = w.write_all(body.as_bytes()).await;
                let _ = w.flush().await;
            }
            return;
        }
        // Response to our request
        let mut map = pending.lock().await;
        if let Some(sender) = map.remove(&id) {
            sender.send(msg).ok();
        }
    } else if let Some(method) = msg.get("method").and_then(|v| v.as_str()) {
        let params = msg.get("params").cloned().unwrap_or(Value::Null);
        tracing::debug!("LSP notification: {}", method);
        notif_tx.send((method.to_string(), params)).ok();
    }
}

fn path_to_uri(path: &Path) -> String {
    format!("file://{}", path.to_string_lossy())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn uri_to_path(uri: &str) -> Option<PathBuf> {
        uri.strip_prefix("file://").map(PathBuf::from)
    }

    #[test]
    fn path_to_uri_converts_correctly() {
        let path = Path::new("/Users/test/project/Main.kt");
        assert_eq!(
            path_to_uri(path),
            "file:///Users/test/project/Main.kt"
        );
    }

    #[test]
    fn uri_to_path_converts_correctly() {
        let result = uri_to_path("file:///Users/test/project/Main.kt");
        assert_eq!(
            result,
            Some(PathBuf::from("/Users/test/project/Main.kt"))
        );
    }

    #[test]
    fn uri_to_path_returns_none_for_non_file_uri() {
        assert_eq!(uri_to_path("https://example.com"), None);
    }

    #[tokio::test]
    async fn read_message_parses_valid_frame() {
        let body = r#"{"jsonrpc":"2.0","id":1,"result":null}"#;
        let frame = format!("Content-Length: {}\r\n\r\n{}", body.len(), body);
        let mut reader = BufReader::new(frame.as_bytes());
        let msg = read_message(&mut reader).await.unwrap().unwrap();
        assert_eq!(msg.get("id").unwrap().as_i64(), Some(1));
    }

    #[tokio::test]
    async fn read_message_rejects_oversized() {
        let fake_header = format!("Content-Length: {}\r\n\r\n", MAX_MESSAGE_SIZE + 1);
        let mut reader = BufReader::new(fake_header.as_bytes());
        let result = read_message(&mut reader).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("too large"));
    }

    #[tokio::test]
    async fn read_message_returns_none_on_eof() {
        let mut reader = BufReader::new(&b""[..]);
        let result = read_message(&mut reader).await.unwrap();
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn read_message_rejects_missing_content_length() {
        let frame = "X-Custom: value\r\n\r\n{}";
        let mut reader = BufReader::new(frame.as_bytes());
        let result = read_message(&mut reader).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Missing Content-Length"));
    }

    #[tokio::test]
    async fn read_message_rejects_malformed_json() {
        let body = "not valid json!!!";
        let frame = format!("Content-Length: {}\r\n\r\n{}", body.len(), body);
        let mut reader = BufReader::new(frame.as_bytes());
        let result = read_message(&mut reader).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("JSON parse error"));
    }

    // ── LspEnv ───────────────────────────────────────────────────────────────

    #[test]
    fn lsp_env_default_has_no_fields_set() {
        let env = LspEnv::default();
        assert!(env.android_sdk_path.is_none());
        assert!(env.java_home.is_none());
    }

    #[test]
    fn lsp_env_stores_sdk_and_java_home() {
        let env = LspEnv {
            android_sdk_path: Some("/Library/Android/sdk".into()),
            java_home: Some("/opt/homebrew/opt/openjdk@17".into()),
        };
        assert_eq!(env.android_sdk_path.as_deref(), Some("/Library/Android/sdk"));
        assert_eq!(env.java_home.as_deref(), Some("/opt/homebrew/opt/openjdk@17"));
    }
}
