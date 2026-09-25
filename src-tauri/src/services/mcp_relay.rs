//! Relays newline-delimited JSON-RPC between an MCP client and a server while
//! keeping track of the client's unanswered requests.
//!
//! Used on both ends of an attached session: the app pipes each socket
//! connection to its MCP server through a [`Relay`] so that, when it quits, it
//! can answer every request still in flight; `keynobi --mcp` pipes its stdio to
//! the app's socket through one so that, when the app goes away, it can answer
//! what the app did not and continue with a standalone server (replaying the
//! client's `initialize`).
use serde::Deserialize;
use serde_json::Value;
use std::collections::VecDeque;
use std::future::Future;
use tokio::io::{AsyncBufReadExt, AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt, BufReader};

/// Longest line parsed. Longer lines (a large screenshot result) are relayed
/// in pieces without being looked at.
pub const MAX_RELAY_LINE_BYTES: usize = 64 * 1024 * 1024;
/// Most unanswered client requests tracked; the oldest are forgotten past it.
pub const MAX_TRACKED_REQUESTS: usize = 256;
/// Longest `initialize` or `notifications/initialized` line kept for replay.
pub const MAX_REPLAY_LINE_BYTES: usize = 64 * 1024;
/// JSON-RPC "internal error", for requests answered by the relay.
const INTERNAL_ERROR: i64 = -32603;

/// A piece of the stream.
#[derive(Debug, PartialEq, Eq)]
pub enum Chunk {
    /// A whole line, with its newline unless the stream ended without one.
    Line(Vec<u8>),
    /// Part of a line longer than [`MAX_RELAY_LINE_BYTES`].
    Part(Vec<u8>),
    Eof,
}

/// Reads a stream line by line, within [`MAX_RELAY_LINE_BYTES`].
pub struct LineReader<R> {
    reader: BufReader<R>,
    buf: Vec<u8>,
    /// Inside a line that was too long to parse.
    overlong: bool,
    ended: bool,
}

impl<R: AsyncRead + Unpin> LineReader<R> {
    pub fn new(reader: BufReader<R>) -> Self {
        Self {
            reader,
            buf: Vec::new(),
            overlong: false,
            ended: false,
        }
    }

    /// The next chunk. Cancel safe: bytes already read stay buffered for the
    /// next call, so this can be a `select!` branch.
    pub async fn next(&mut self) -> std::io::Result<Chunk> {
        loop {
            if self.ended {
                return Ok(Chunk::Eof);
            }
            if self.buf.len() >= MAX_RELAY_LINE_BYTES {
                self.overlong = true;
                return Ok(Chunk::Part(std::mem::take(&mut self.buf)));
            }
            let limit = (MAX_RELAY_LINE_BYTES - self.buf.len()) as u64;
            let read = (&mut self.reader)
                .take(limit)
                .read_until(b'\n', &mut self.buf)
                .await?;
            let complete = self.buf.last() == Some(&b'\n');
            if read == 0 {
                self.ended = true;
                if self.buf.is_empty() {
                    return Ok(Chunk::Eof);
                }
            } else if !complete {
                continue;
            }
            let bytes = std::mem::take(&mut self.buf);
            return Ok(if std::mem::replace(&mut self.overlong, false) {
                Chunk::Part(bytes)
            } else {
                Chunk::Line(bytes)
            });
        }
    }
}

/// What a line is, as far as tracking goes.
#[derive(Debug, PartialEq)]
pub enum Message {
    Request {
        id: Value,
        method: String,
    },
    Notification {
        method: String,
        cancelled: Option<Value>,
    },
    Response {
        id: Value,
    },
    Other,
}

#[derive(Deserialize)]
struct Probe {
    #[serde(default)]
    id: Option<Value>,
    #[serde(default)]
    method: Option<String>,
    #[serde(default)]
    params: Option<ProbeParams>,
}

#[derive(Deserialize)]
struct ProbeParams {
    #[serde(default, rename = "requestId")]
    request_id: Option<Value>,
}

pub fn classify(line: &[u8]) -> Message {
    let Ok(probe) = serde_json::from_slice::<Probe>(line) else {
        return Message::Other;
    };
    match (probe.id, probe.method) {
        (Some(id), Some(method)) => Message::Request { id, method },
        (None, Some(method)) => Message::Notification {
            cancelled: (method == "notifications/cancelled")
                .then(|| probe.params.and_then(|p| p.request_id))
                .flatten(),
            method,
        },
        (Some(id), None) => Message::Response { id },
        (None, None) => Message::Other,
    }
}

/// A JSON-RPC error response to request `id`, as one line.
pub fn error_line(id: &Value, message: &str) -> Vec<u8> {
    let mut line = serde_json::json!({
        "jsonrpc": "2.0",
        "id": id,
        "error": { "code": INTERNAL_ERROR, "message": message },
    })
    .to_string()
    .into_bytes();
    line.push(b'\n');
    line
}

/// The client's side of a session: its unanswered requests and handshake.
#[derive(Debug, Default)]
pub struct SessionTracker {
    in_flight: VecDeque<Value>,
    /// The client's `initialize` request (its ID and line).
    pub initialize: Option<(Value, Vec<u8>)>,
    /// The client's `notifications/initialized` line.
    pub initialized: Option<Vec<u8>>,
}

impl SessionTracker {
    /// Note a line the client sent.
    pub fn from_client(&mut self, line: &[u8]) {
        match classify(line) {
            Message::Request { id, method } => {
                if method == "initialize" && line.len() <= MAX_REPLAY_LINE_BYTES {
                    self.initialize = Some((id.clone(), line.to_vec()));
                }
                if self.in_flight.len() >= MAX_TRACKED_REQUESTS {
                    self.in_flight.pop_front();
                }
                self.in_flight.push_back(id);
            }
            Message::Notification { method, cancelled } => {
                if method == "notifications/initialized" && line.len() <= MAX_REPLAY_LINE_BYTES {
                    self.initialized = Some(line.to_vec());
                }
                // A cancelled request gets no answer.
                if let Some(id) = cancelled {
                    self.answered(&id);
                }
            }
            Message::Response { .. } | Message::Other => {}
        }
    }

    /// Note a line the server sent.
    pub fn from_server(&mut self, line: &[u8]) {
        if let Message::Response { id } = classify(line) {
            self.answered(&id);
        }
    }

    fn answered(&mut self, id: &Value) {
        self.in_flight.retain(|pending| pending != id);
    }

    /// The client's requests still waiting for an answer, oldest first.
    pub fn take_unanswered(&mut self) -> Vec<Value> {
        self.in_flight.drain(..).collect()
    }
}

/// How [`Relay::run`] ended.
#[derive(Debug, PartialEq, Eq)]
pub enum RelayEnd {
    /// The client closed its side (or cannot be written to).
    ClientClosed,
    /// The server closed its side (or cannot be written to).
    ServerClosed,
    /// The stop signal fired, with its message.
    Stopped(String),
}

/// A client connection piped to a server.
pub struct Relay<CR, CW, SR, SW> {
    pub client: LineReader<CR>,
    pub client_out: CW,
    pub server: LineReader<SR>,
    pub server_out: SW,
    pub tracker: SessionTracker,
}

impl<CR, CW, SR, SW> Relay<CR, CW, SR, SW>
where
    CR: AsyncRead + Unpin,
    CW: AsyncWrite + Unpin,
    SR: AsyncRead + Unpin,
    SW: AsyncWrite + Unpin,
{
    /// Pipe both ways until either side closes or `stop` resolves.
    pub async fn run(&mut self, stop: impl Future<Output = String>) -> RelayEnd {
        tokio::pin!(stop);
        loop {
            tokio::select! {
                biased;
                message = &mut stop => return RelayEnd::Stopped(message),
                chunk = self.server.next() => {
                    let bytes = match chunk {
                        Ok(Chunk::Line(line)) => {
                            self.tracker.from_server(&line);
                            line
                        }
                        Ok(Chunk::Part(part)) => part,
                        Ok(Chunk::Eof) | Err(_) => return RelayEnd::ServerClosed,
                    };
                    if write_all_flush(&mut self.client_out, &bytes).await.is_err() {
                        return RelayEnd::ClientClosed;
                    }
                }
                chunk = self.client.next() => {
                    let bytes = match chunk {
                        Ok(Chunk::Line(line)) => {
                            self.tracker.from_client(&line);
                            line
                        }
                        Ok(Chunk::Part(part)) => part,
                        Ok(Chunk::Eof) | Err(_) => return RelayEnd::ClientClosed,
                    };
                    if write_all_flush(&mut self.server_out, &bytes).await.is_err() {
                        return RelayEnd::ServerClosed;
                    }
                }
            }
        }
    }

    /// Answer every unanswered client request with `message`.
    pub async fn answer_unanswered(&mut self, message: &str) {
        for id in self.tracker.take_unanswered() {
            if write_all_flush(&mut self.client_out, &error_line(&id, message))
                .await
                .is_err()
            {
                return;
            }
        }
    }

    /// After the client closed: end the server's input, then keep relaying
    /// its answers to requests already sent, for at most `timeout`.
    pub async fn drain_server(&mut self, timeout: std::time::Duration) {
        let _ = self.server_out.shutdown().await;
        let _ = tokio::time::timeout(timeout, async {
            loop {
                match self.server.next().await {
                    Ok(Chunk::Line(bytes) | Chunk::Part(bytes)) => {
                        if write_all_flush(&mut self.client_out, &bytes).await.is_err() {
                            return;
                        }
                    }
                    Ok(Chunk::Eof) | Err(_) => return,
                }
            }
        })
        .await;
    }
}

pub async fn write_all_flush<W: AsyncWrite + Unpin>(
    writer: &mut W,
    bytes: &[u8],
) -> std::io::Result<()> {
    writer.write_all(bytes).await?;
    writer.flush().await
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn line(value: Value) -> Vec<u8> {
        let mut line = value.to_string().into_bytes();
        line.push(b'\n');
        line
    }

    #[test]
    fn messages_are_classified_by_id_and_method() {
        assert_eq!(
            classify(&line(json!({"jsonrpc":"2.0","id":4,"method":"tools/call"}))),
            Message::Request {
                id: json!(4),
                method: "tools/call".into()
            }
        );
        assert_eq!(
            classify(&line(json!({"jsonrpc":"2.0","id":"a","result":{}}))),
            Message::Response { id: json!("a") }
        );
        assert_eq!(
            classify(&line(
                json!({"jsonrpc":"2.0","method":"notifications/cancelled",
                "params":{"requestId":4,"reason":"user"}})
            )),
            Message::Notification {
                method: "notifications/cancelled".into(),
                cancelled: Some(json!(4))
            }
        );
        assert_eq!(classify(b"not json\n"), Message::Other);
    }

    #[test]
    fn the_tracker_keeps_unanswered_requests_and_the_handshake() {
        let mut tracker = SessionTracker::default();
        let init = line(json!({"jsonrpc":"2.0","id":0,"method":"initialize","params":{}}));
        tracker.from_client(&init);
        tracker.from_server(&line(json!({"jsonrpc":"2.0","id":0,"result":{}})));
        let initialized = line(json!({"jsonrpc":"2.0","method":"notifications/initialized"}));
        tracker.from_client(&initialized);
        for id in 1..=3 {
            tracker.from_client(&line(
                json!({"jsonrpc":"2.0","id":id,"method":"tools/call","params":{}}),
            ));
        }
        tracker.from_server(&line(json!({"jsonrpc":"2.0","id":2,"result":{}})));
        tracker.from_client(&line(
            json!({"jsonrpc":"2.0","method":"notifications/cancelled",
            "params":{"requestId":3}}),
        ));

        assert_eq!(tracker.take_unanswered(), vec![json!(1)]);
        assert_eq!(tracker.initialize, Some((json!(0), init)));
        assert_eq!(tracker.initialized, Some(initialized));
    }

    #[test]
    fn tracked_requests_are_capped() {
        let mut tracker = SessionTracker::default();
        for id in 0..MAX_TRACKED_REQUESTS + 10 {
            tracker.from_client(&line(json!({"jsonrpc":"2.0","id":id,"method":"ping"})));
        }
        let pending = tracker.take_unanswered();
        assert_eq!(pending.len(), MAX_TRACKED_REQUESTS);
        assert_eq!(pending[0], json!(10));
    }

    #[tokio::test]
    async fn lines_are_read_whole_and_a_final_unterminated_line_is_kept() {
        let data: &[u8] = b"{\"a\":1}\n{\"b\":2}";
        let mut reader = LineReader::new(BufReader::new(data));
        assert_eq!(
            reader.next().await.unwrap(),
            Chunk::Line(b"{\"a\":1}\n".to_vec())
        );
        assert_eq!(
            reader.next().await.unwrap(),
            Chunk::Line(b"{\"b\":2}".to_vec())
        );
        assert_eq!(reader.next().await.unwrap(), Chunk::Eof);
    }

    #[tokio::test]
    async fn a_stopped_relay_answers_every_request_still_in_flight() {
        let (client, mut client_peer) = tokio::io::duplex(4096);
        let (server, mut server_peer) = tokio::io::duplex(4096);
        let (client_r, client_w) = tokio::io::split(client);
        let (server_r, server_w) = tokio::io::split(server);
        let mut relay = Relay {
            client: LineReader::new(BufReader::new(client_r)),
            client_out: client_w,
            server: LineReader::new(BufReader::new(server_r)),
            server_out: server_w,
            tracker: SessionTracker::default(),
        };
        client_peer
            .write_all(&line(json!({"jsonrpc":"2.0","id":7,"method":"tools/call"})))
            .await
            .unwrap();

        let (stop_tx, stop_rx) = tokio::sync::oneshot::channel::<String>();
        let run = async {
            let end = relay.run(async { stop_rx.await.unwrap_or_default() }).await;
            relay.answer_unanswered("Keynobi is quitting.").await;
            end
        };
        let stop = async {
            // The server received the request and never answers.
            let mut got = vec![0u8; 64];
            let n = server_peer.read(&mut got).await.unwrap();
            assert!(String::from_utf8_lossy(&got[..n]).contains("\"id\":7"));
            stop_tx.send("Keynobi is quitting.".into()).unwrap();
        };
        let (end, ()) = tokio::join!(run, stop);
        assert_eq!(end, RelayEnd::Stopped("Keynobi is quitting.".into()));
        drop(relay);

        let mut answer = String::new();
        client_peer.read_to_string(&mut answer).await.unwrap();
        let answer: Value = serde_json::from_str(answer.trim()).unwrap();
        assert_eq!(answer["id"], 7);
        assert_eq!(answer["error"]["message"], "Keynobi is quitting.");
    }
}
