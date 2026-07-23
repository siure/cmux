use anyhow::{anyhow, Result};
use base64::Engine;
use serde_json::{json, Value};
use std::collections::HashMap;
use std::io::{self, BufRead, Write};
use std::net::{Shutdown, TcpStream, ToSocketAddrs};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

const REMOTE_DAEMON_VERSION: &str = concat!("cmux-linux-", env!("CARGO_PKG_VERSION"));

#[derive(Clone)]
struct StreamState {
    conn: Arc<Mutex<TcpStream>>,
    reader_started: bool,
}

#[derive(Clone)]
struct SessionAttachment {
    cols: u16,
    rows: u16,
}

#[derive(Clone)]
struct SessionState {
    session_id: String,
    command: Option<String>,
    attachments: HashMap<String, SessionAttachment>,
    cols: u16,
    rows: u16,
    next_attachment_id: u64,
}

struct RpcServer {
    next_stream_id: u64,
    next_session_id: u64,
    streams: Arc<Mutex<HashMap<String, StreamState>>>,
    sessions: HashMap<String, SessionState>,
    writer: Arc<Mutex<io::Stdout>>,
}

fn main() {
    if let Err(err) = run(std::env::args().collect()) {
        eprintln!("cmuxd-remote: {err}");
        std::process::exit(1);
    }
}

fn run(args: Vec<String>) -> Result<()> {
    match args.get(1).map(String::as_str) {
        Some("version") => {
            println!("cmuxd-remote {REMOTE_DAEMON_VERSION}");
            Ok(())
        }
        Some("serve") if args.iter().any(|arg| arg == "--stdio") => serve_stdio(),
        Some("serve") => Err(anyhow!("serve requires --stdio")),
        _ => Err(anyhow!("usage: cmuxd-remote serve --stdio")),
    }
}

fn serve_stdio() -> Result<()> {
    let stdin = io::stdin();
    let reader = io::BufReader::new(stdin.lock());
    let mut server = RpcServer {
        next_stream_id: 1,
        next_session_id: 1,
        streams: Arc::new(Mutex::new(HashMap::new())),
        sessions: HashMap::new(),
        writer: Arc::new(Mutex::new(io::stdout())),
    };

    for line in reader.lines() {
        let line = line?;
        if line.trim().is_empty() {
            continue;
        }
        let request = match serde_json::from_str::<Value>(&line) {
            Ok(request) => request,
            Err(err) => {
                let response = json!({
                    "id": Value::Null,
                    "ok": false,
                    "error": {"code": "invalid_request", "message": err.to_string()}
                });
                write_json(&server.writer, &response)?;
                continue;
            }
        };
        let response = server.handle_request(request);
        write_json(&server.writer, &response)?;
    }

    if let Ok(mut streams) = server.streams.lock() {
        for (_, state) in streams.drain() {
            if let Ok(conn) = state.conn.lock() {
                let _ = conn.shutdown(Shutdown::Both);
            }
        }
    }
    Ok(())
}

impl RpcServer {
    fn handle_request(&mut self, request: Value) -> Value {
        let id = request.get("id").cloned().unwrap_or(Value::Null);
        let method = request
            .get("method")
            .and_then(Value::as_str)
            .unwrap_or_default();
        let params = request.get("params").cloned().unwrap_or_else(|| json!({}));

        match method {
            "hello" => ok(
                id,
                json!({
                    "name": "cmuxd-remote",
                    "version": REMOTE_DAEMON_VERSION,
                    "capabilities": remote_daemon_capabilities()
                }),
            ),
            "ping" => ok(id, json!({"pong": true})),
            "proxy.open" => self.handle_proxy_open(id, &params),
            "proxy.close" => self.handle_proxy_close(id, &params),
            "proxy.write" => self.handle_proxy_write(id, &params),
            "proxy.stream.subscribe" => self.handle_proxy_stream_subscribe(id, &params),
            "session.open" => self.handle_session_open(id, &params),
            "session.attach" => self.handle_session_attach(id, &params),
            "session.resize" => self.handle_session_resize(id, &params),
            "session.detach" => self.handle_session_detach(id, &params),
            "session.status" => self.handle_session_status(id, &params),
            "session.close" => self.handle_session_close(id, &params),
            _ => err(id, "method_not_found", format!("unknown method {method:?}")),
        }
    }

    fn handle_proxy_open(&mut self, id: Value, params: &Value) -> Value {
        let Some(host) = string_param(params, "host").filter(|value| !value.trim().is_empty())
        else {
            return err(id, "invalid_params", "proxy.open requires host");
        };
        let Some(port) = port_param(params, "port") else {
            return err(
                id,
                "invalid_params",
                "proxy.open requires port in range 1-65535",
            );
        };
        let timeout_ms = int_param(params, "timeout_ms").unwrap_or(10_000).max(0) as u64;
        let timeout = Duration::from_millis(timeout_ms);
        let mut last_error = None;
        let addrs = match (host.as_str(), port).to_socket_addrs() {
            Ok(addrs) => addrs.collect::<Vec<_>>(),
            Err(resolve_err) => return err(id, "open_failed", resolve_err.to_string()),
        };
        for addr in addrs {
            match TcpStream::connect_timeout(&addr, timeout) {
                Ok(conn) => {
                    let _ = conn.set_nodelay(true);
                    let stream_id = format!("s-{}", self.next_stream_id);
                    self.next_stream_id += 1;
                    let state = StreamState {
                        conn: Arc::new(Mutex::new(conn)),
                        reader_started: false,
                    };
                    if let Ok(mut streams) = self.streams.lock() {
                        streams.insert(stream_id.clone(), state);
                    }
                    return ok(id, json!({"stream_id": stream_id}));
                }
                Err(err) => last_error = Some(err.to_string()),
            }
        }
        err(
            id,
            "open_failed",
            last_error.unwrap_or_else(|| "no address resolved".to_string()),
        )
    }

    fn handle_proxy_close(&mut self, id: Value, params: &Value) -> Value {
        let Some(stream_id) =
            string_param(params, "stream_id").filter(|value| !value.trim().is_empty())
        else {
            return err(id, "invalid_params", "proxy.close requires stream_id");
        };
        let state = self
            .streams
            .lock()
            .ok()
            .and_then(|mut streams| streams.remove(&stream_id));
        if let Some(state) = state {
            if let Ok(conn) = state.conn.lock() {
                let _ = conn.shutdown(Shutdown::Both);
            }
        }
        ok(id, json!({"closed": true}))
    }

    fn handle_proxy_write(&mut self, id: Value, params: &Value) -> Value {
        let Some(stream_id) =
            string_param(params, "stream_id").filter(|value| !value.trim().is_empty())
        else {
            return err(id, "invalid_params", "proxy.write requires stream_id");
        };
        let Some(data_base64) = string_param(params, "data_base64") else {
            return err(id, "invalid_params", "proxy.write requires data_base64");
        };
        let payload = match base64::engine::general_purpose::STANDARD.decode(data_base64) {
            Ok(payload) => payload,
            Err(_) => return err(id, "invalid_params", "data_base64 must be valid base64"),
        };
        let state = self
            .streams
            .lock()
            .ok()
            .and_then(|streams| streams.get(&stream_id).cloned());
        let Some(state) = state else {
            return err(id, "not_found", "stream not found");
        };
        match state
            .conn
            .lock()
            .map_err(|_| anyhow!("stream lock poisoned"))
            .and_then(|mut conn| conn.write_all(&payload).map_err(Into::into))
        {
            Ok(()) => ok(id, json!({"written": payload.len()})),
            Err(err_value) => err(id, "stream_error", err_value.to_string()),
        }
    }

    fn handle_proxy_stream_subscribe(&mut self, id: Value, params: &Value) -> Value {
        let Some(stream_id) =
            string_param(params, "stream_id").filter(|value| !value.trim().is_empty())
        else {
            return err(
                id,
                "invalid_params",
                "proxy.stream.subscribe requires stream_id",
            );
        };

        let stream = {
            let mut streams = match self.streams.lock() {
                Ok(streams) => streams,
                Err(_) => return err(id, "internal_error", "stream lock poisoned"),
            };
            let Some(state) = streams.get_mut(&stream_id) else {
                return err(id, "not_found", "stream not found");
            };
            if state.reader_started {
                None
            } else {
                state.reader_started = true;
                match state.conn.lock() {
                    Ok(conn) => match conn.try_clone() {
                        Ok(conn) => Some(conn),
                        Err(err_value) => {
                            return err(id, "stream_error", err_value.to_string());
                        }
                    },
                    Err(_) => return err(id, "internal_error", "stream lock poisoned"),
                }
            }
        };

        if let Some(stream) = stream {
            let writer = Arc::clone(&self.writer);
            let streams = Arc::clone(&self.streams);
            let stream_id_for_thread = stream_id.clone();
            thread::spawn(move || {
                stream_pump(stream_id_for_thread, stream, writer, streams);
            });
        }

        ok(id, json!({"subscribed": true}))
    }

    fn handle_session_open(&mut self, id: Value, params: &Value) -> Value {
        let session_id = string_param(params, "session_id")
            .filter(|value| !value.trim().is_empty())
            .unwrap_or_else(|| {
                let value = format!("pty-{}", self.next_session_id);
                self.next_session_id += 1;
                value
            });
        if self.sessions.contains_key(&session_id) {
            return err(id, "already_exists", "session already exists");
        }
        let cols = positive_dimension(params, "cols").unwrap_or(80);
        let rows = positive_dimension(params, "rows").unwrap_or(24);
        let mut session = SessionState {
            session_id: session_id.clone(),
            command: string_param(params, "command").filter(|value| !value.trim().is_empty()),
            attachments: HashMap::new(),
            cols,
            rows,
            next_attachment_id: 1,
        };
        if let Some(attachment_id) =
            string_param(params, "attachment_id").filter(|value| !value.trim().is_empty())
        {
            session
                .attachments
                .insert(attachment_id, SessionAttachment { cols, rows });
        }
        let result = session_value(&session);
        self.sessions.insert(session_id, session);
        ok(id, result)
    }

    fn handle_session_attach(&mut self, id: Value, params: &Value) -> Value {
        let Some(session_id) =
            string_param(params, "session_id").filter(|value| !value.trim().is_empty())
        else {
            return err(id, "invalid_params", "session.attach requires session_id");
        };
        let Some(session) = self.sessions.get_mut(&session_id) else {
            return err(id, "not_found", "session not found");
        };
        let attachment_id = string_param(params, "attachment_id")
            .filter(|value| !value.trim().is_empty())
            .unwrap_or_else(|| {
                let value = format!("a-{}", session.next_attachment_id);
                session.next_attachment_id += 1;
                value
            });
        let cols = positive_dimension(params, "cols").unwrap_or(session.cols);
        let rows = positive_dimension(params, "rows").unwrap_or(session.rows);
        session
            .attachments
            .insert(attachment_id.clone(), SessionAttachment { cols, rows });
        recompute_session_size(session);
        let mut result = session_value(session);
        result["attachment_id"] = json!(attachment_id);
        ok(id, result)
    }

    fn handle_session_resize(&mut self, id: Value, params: &Value) -> Value {
        let Some(session_id) =
            string_param(params, "session_id").filter(|value| !value.trim().is_empty())
        else {
            return err(id, "invalid_params", "session.resize requires session_id");
        };
        let Some(attachment_id) =
            string_param(params, "attachment_id").filter(|value| !value.trim().is_empty())
        else {
            return err(
                id,
                "invalid_params",
                "session.resize requires attachment_id",
            );
        };
        let Some(cols) = positive_dimension(params, "cols") else {
            return err(id, "invalid_params", "session.resize requires cols");
        };
        let Some(rows) = positive_dimension(params, "rows") else {
            return err(id, "invalid_params", "session.resize requires rows");
        };
        let Some(session) = self.sessions.get_mut(&session_id) else {
            return err(id, "not_found", "session not found");
        };
        if !session.attachments.contains_key(&attachment_id) {
            return err(id, "not_found", "attachment not found");
        }
        session
            .attachments
            .insert(attachment_id.clone(), SessionAttachment { cols, rows });
        recompute_session_size(session);
        let mut result = session_value(session);
        result["attachment_id"] = json!(attachment_id);
        ok(id, result)
    }

    fn handle_session_detach(&mut self, id: Value, params: &Value) -> Value {
        let Some(session_id) =
            string_param(params, "session_id").filter(|value| !value.trim().is_empty())
        else {
            return err(id, "invalid_params", "session.detach requires session_id");
        };
        let Some(attachment_id) =
            string_param(params, "attachment_id").filter(|value| !value.trim().is_empty())
        else {
            return err(
                id,
                "invalid_params",
                "session.detach requires attachment_id",
            );
        };
        let Some(session) = self.sessions.get_mut(&session_id) else {
            return err(id, "not_found", "session not found");
        };
        let detached = session.attachments.remove(&attachment_id).is_some();
        recompute_session_size(session);
        let mut result = session_value(session);
        result["attachment_id"] = json!(attachment_id);
        result["detached"] = json!(detached);
        ok(id, result)
    }

    fn handle_session_status(&mut self, id: Value, params: &Value) -> Value {
        if let Some(session_id) =
            string_param(params, "session_id").filter(|value| !value.trim().is_empty())
        {
            let Some(session) = self.sessions.get(&session_id) else {
                return err(id, "not_found", "session not found");
            };
            return ok(id, session_value(session));
        }
        let mut sessions = self
            .sessions
            .values()
            .map(session_value)
            .collect::<Vec<_>>();
        sessions.sort_by(|left, right| {
            left["session_id"]
                .as_str()
                .unwrap_or_default()
                .cmp(right["session_id"].as_str().unwrap_or_default())
        });
        ok(
            id,
            json!({
                "session_count": sessions.len(),
                "sessions": sessions
            }),
        )
    }

    fn handle_session_close(&mut self, id: Value, params: &Value) -> Value {
        let Some(session_id) =
            string_param(params, "session_id").filter(|value| !value.trim().is_empty())
        else {
            return err(id, "invalid_params", "session.close requires session_id");
        };
        let closed = self.sessions.remove(&session_id).is_some();
        ok(
            id,
            json!({
                "session_id": session_id,
                "closed": closed
            }),
        )
    }
}

fn recompute_session_size(session: &mut SessionState) {
    if session.attachments.is_empty() {
        return;
    }
    session.cols = session
        .attachments
        .values()
        .map(|attachment| attachment.cols)
        .min()
        .unwrap_or(session.cols);
    session.rows = session
        .attachments
        .values()
        .map(|attachment| attachment.rows)
        .min()
        .unwrap_or(session.rows);
}

fn session_value(session: &SessionState) -> Value {
    let mut attachments = session
        .attachments
        .iter()
        .map(|(attachment_id, attachment)| {
            json!({
                "attachment_id": attachment_id,
                "cols": attachment.cols,
                "rows": attachment.rows
            })
        })
        .collect::<Vec<_>>();
    attachments.sort_by(|left, right| {
        left["attachment_id"]
            .as_str()
            .unwrap_or_default()
            .cmp(right["attachment_id"].as_str().unwrap_or_default())
    });
    json!({
        "session_id": session.session_id,
        "command": session.command,
        "cols": session.cols,
        "rows": session.rows,
        "attachment_count": attachments.len(),
        "attachments": attachments
    })
}

fn stream_pump(
    stream_id: String,
    mut conn: TcpStream,
    writer: Arc<Mutex<io::Stdout>>,
    streams: Arc<Mutex<HashMap<String, StreamState>>>,
) {
    let mut buffer = [0_u8; 16 * 1024];
    loop {
        match std::io::Read::read(&mut conn, &mut buffer) {
            Ok(0) => {
                let _ = write_json(
                    &writer,
                    &json!({"event": "proxy.stream.eof", "stream_id": stream_id}),
                );
                break;
            }
            Ok(count) => {
                let payload = base64::engine::general_purpose::STANDARD.encode(&buffer[..count]);
                let _ = write_json(
                    &writer,
                    &json!({
                        "event": "proxy.stream.data",
                        "stream_id": stream_id,
                        "data_base64": payload
                    }),
                );
            }
            Err(err_value) => {
                let _ = write_json(
                    &writer,
                    &json!({
                        "event": "proxy.stream.error",
                        "stream_id": stream_id,
                        "error": err_value.to_string()
                    }),
                );
                break;
            }
        }
    }
    if let Ok(mut streams) = streams.lock() {
        streams.remove(&stream_id);
    }
}

fn write_json(writer: &Arc<Mutex<io::Stdout>>, value: &Value) -> Result<()> {
    let mut writer = writer
        .lock()
        .map_err(|_| anyhow!("stdout writer lock poisoned"))?;
    serde_json::to_writer(&mut *writer, value)?;
    writer.write_all(b"\n")?;
    writer.flush()?;
    Ok(())
}

fn ok(id: Value, result: Value) -> Value {
    json!({"id": id, "ok": true, "result": result})
}

fn err(id: Value, code: &str, message: impl ToString) -> Value {
    json!({
        "id": id,
        "ok": false,
        "error": {"code": code, "message": message.to_string()}
    })
}

fn string_param(value: &Value, key: &str) -> Option<String> {
    value
        .get(key)
        .and_then(Value::as_str)
        .map(ToString::to_string)
}

fn int_param(value: &Value, key: &str) -> Option<i64> {
    match value.get(key)? {
        Value::Number(number) => number.as_i64(),
        _ => None,
    }
}

fn port_param(value: &Value, key: &str) -> Option<u16> {
    let port = int_param(value, key)?;
    (1..=u16::MAX as i64).contains(&port).then_some(port as u16)
}

fn positive_dimension(value: &Value, key: &str) -> Option<u16> {
    let dimension = int_param(value, key)?;
    (1..=u16::MAX as i64)
        .contains(&dimension)
        .then_some(dimension as u16)
}

fn remote_daemon_capabilities() -> Vec<&'static str> {
    vec![
        "session.basic",
        "session.lifecycle",
        "session.attach",
        "session.detach",
        "session.status",
        "session.close",
        "session.resize.min",
        "pty.session.persistent_daemon",
        "proxy.http_connect",
        "proxy.socks5",
        "proxy.stream",
        "proxy.stream.push",
    ]
}
