use crate::app::{self, AppError, AppState, MobileAttachRoute};
use serde_json::{json, Map, Value};
use sha2::{Digest, Sha256};
use std::collections::{HashMap, HashSet};
use std::io::{ErrorKind, Read, Write};
use std::net::{Shutdown, TcpListener, TcpStream};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, OnceLock, Weak};
use std::thread;
use std::time::{Duration, Instant};

const FRAME_HEADER_BYTES: usize = 4;
const MAX_FRAME_BYTES: usize = 8 * 1024 * 1024;
const MAX_ACTIVE_CONNECTIONS: usize = 10;
const MAX_IN_FLIGHT_REQUESTS_PER_CONNECTION: u64 = 64;
const MAX_ATTACH_TOKEN_BYTES: usize = 4 * 1024;
const MAX_STACK_TOKEN_BYTES: usize = 128 * 1024;
const FIRST_FRAME_TIMEOUT: Duration = Duration::from_secs(15);
const IDLE_TIMEOUT: Duration = Duration::from_secs(30);
const EVENT_POLL_INTERVAL: Duration = Duration::from_millis(100);
const AUTH_CACHE_TTL: Duration = Duration::from_secs(60);

#[derive(Clone, Debug)]
pub(crate) struct AuthContext {
    pub(crate) local_user_id: Option<String>,
    pub(crate) dev_access_token: Option<String>,
}

#[derive(Clone, Debug, Default)]
pub(crate) struct RuntimeStatus {
    pub(crate) is_running: bool,
    pub(crate) port: Option<u16>,
    pub(crate) configured_port: u16,
    pub(crate) uses_ephemeral_fallback: bool,
    pub(crate) routes: Vec<MobileAttachRoute>,
    pub(crate) active_connection_count: usize,
    pub(crate) last_error: Option<String>,
}

#[derive(Clone, Debug)]
struct TicketRecord {
    expires_at_ms: u64,
    workspace_id: String,
    terminal_id: Option<String>,
    created_workspace_ids: HashSet<String>,
    created_terminal_ids: HashSet<String>,
}

#[derive(Clone, Debug)]
struct AuthCacheEntry {
    user_id: String,
    expires_at: Instant,
}

#[derive(Default)]
struct RuntimeState {
    configured_port: u16,
    routes: Vec<MobileAttachRoute>,
    uses_ephemeral_fallback: bool,
    last_error: Option<String>,
    connections: HashMap<u64, Arc<MobileConnection>>,
    tickets: HashMap<String, TicketRecord>,
    auth_cache: HashMap<String, AuthCacheEntry>,
    workspace_signature: Option<[u8; 32]>,
    terminal_signatures: HashMap<String, [u8; 32]>,
    notification_ids: Option<HashSet<String>>,
    notification_unread_count: Option<usize>,
    chat_state_signatures: HashMap<String, [u8; 32]>,
    chat_descriptor_signatures: HashMap<String, [u8; 32]>,
    chat_history_signatures: HashMap<String, [u8; 32]>,
    chat_session_ids: Option<HashSet<String>>,
}

struct MobileHostRuntime {
    started: AtomicBool,
    next_connection_id: AtomicU64,
    state: Mutex<RuntimeState>,
}

struct MobileConnection {
    id: u64,
    writer: Mutex<TcpStream>,
    subscriptions: Mutex<HashMap<String, HashSet<String>>>,
    closed: AtomicBool,
    in_flight_requests: AtomicU64,
}

#[derive(Clone, Debug)]
struct MobileRequest {
    id: Value,
    method: String,
    params: Value,
    attach_token: Option<String>,
    stack_access_token: Option<String>,
}

fn runtime() -> &'static Arc<MobileHostRuntime> {
    static RUNTIME: OnceLock<Arc<MobileHostRuntime>> = OnceLock::new();
    RUNTIME.get_or_init(|| {
        Arc::new(MobileHostRuntime {
            started: AtomicBool::new(false),
            next_connection_id: AtomicU64::new(1),
            state: Mutex::new(RuntimeState::default()),
        })
    })
}

pub(crate) fn start(app_state: Arc<Mutex<AppState>>) {
    let runtime = Arc::clone(runtime());
    if runtime.started.swap(true, Ordering::AcqRel) {
        return;
    }
    if env_false("CMUX_MOBILE_HOST_ENABLED") || env_false("CMUX_MOBILE_HOST_LISTEN") {
        return;
    }

    let configured_routes = app::mobile_host_routes();
    let configured_port = app::mobile_host_configured_port();
    if configured_routes.is_empty() {
        if let Ok(mut state) = runtime.state.lock() {
            state.configured_port = configured_port;
        }
        return;
    }

    let bind_host = normalized_env("CMUX_MOBILE_HOST_BIND_HOST")
        .or_else(|| normalized_env("CMUX_MOBILE_HOST_BIND"))
        .unwrap_or_else(|| {
            if configured_routes
                .iter()
                .all(|route| route.kind == "debug_loopback")
            {
                "127.0.0.1".to_string()
            } else {
                "0.0.0.0".to_string()
            }
        });

    let mut listeners = Vec::new();
    let mut bound_ports = HashMap::<u16, u16>::new();
    let mut uses_ephemeral_fallback = false;
    let mut last_error = None;
    let mut requested_ports = configured_routes
        .iter()
        .map(|route| route.port)
        .collect::<Vec<_>>();
    requested_ports.sort_unstable();
    requested_ports.dedup();

    for requested_port in requested_ports {
        match bind_mobile_listener(&bind_host, requested_port) {
            Ok((listener, actual_port, fallback_error)) => {
                if let Some(error) = fallback_error {
                    uses_ephemeral_fallback = true;
                    last_error = Some(error);
                }
                bound_ports.insert(requested_port, actual_port);
                listeners.push(listener);
            }
            Err(error) => {
                last_error = Some(error);
            }
        }
    }

    let routes = configured_routes
        .into_iter()
        .filter_map(|mut route| {
            let actual_port = bound_ports.get(&route.port).copied()?;
            route.port = actual_port;
            Some(route)
        })
        .collect::<Vec<_>>();
    if let Ok(mut state) = runtime.state.lock() {
        state.configured_port = configured_port;
        state.routes = routes;
        state.uses_ephemeral_fallback = uses_ephemeral_fallback;
        state.last_error = last_error;
    }

    let weak_app = Arc::downgrade(&app_state);
    for listener in listeners {
        let runtime = Arc::clone(&runtime);
        let weak_app = weak_app.clone();
        thread::spawn(move || accept_loop(listener, runtime, weak_app));
    }
    let runtime_for_events = Arc::clone(&runtime);
    thread::spawn(move || event_loop(runtime_for_events, weak_app));
}

fn bind_mobile_listener(
    bind_host: &str,
    requested_port: u16,
) -> Result<(TcpListener, u16, Option<String>), String> {
    match TcpListener::bind((bind_host, requested_port)) {
        Ok(listener) => {
            let port = listener
                .local_addr()
                .map_err(|err| format!("failed to read mobile listener address: {err}"))?
                .port();
            listener
                .set_nonblocking(true)
                .map_err(|err| format!("failed to configure mobile listener: {err}"))?;
            Ok((listener, port, None))
        }
        Err(preferred_error) => {
            let listener = TcpListener::bind((bind_host, 0)).map_err(|fallback_error| {
                format!(
                    "failed to bind mobile host on {bind_host}:{requested_port} ({preferred_error}); ephemeral fallback failed: {fallback_error}"
                )
            })?;
            let port = listener
                .local_addr()
                .map_err(|err| format!("failed to read mobile fallback address: {err}"))?
                .port();
            listener
                .set_nonblocking(true)
                .map_err(|err| format!("failed to configure mobile fallback listener: {err}"))?;
            Ok((
                listener,
                port,
                Some(format!(
                    "mobile host port {requested_port} was unavailable; using ephemeral port {port}: {preferred_error}"
                )),
            ))
        }
    }
}

fn accept_loop(
    listener: TcpListener,
    runtime: Arc<MobileHostRuntime>,
    app_state: Weak<Mutex<AppState>>,
) {
    loop {
        match listener.accept() {
            Ok((stream, _)) => {
                let at_capacity = runtime
                    .state
                    .lock()
                    .map(|state| state.connections.len() >= MAX_ACTIVE_CONNECTIONS)
                    .unwrap_or(true);
                if at_capacity {
                    let _ = stream.shutdown(Shutdown::Both);
                    continue;
                }
                let writer = match stream.try_clone() {
                    Ok(writer) => writer,
                    Err(_) => continue,
                };
                let id = runtime.next_connection_id.fetch_add(1, Ordering::Relaxed);
                let connection = Arc::new(MobileConnection {
                    id,
                    writer: Mutex::new(writer),
                    subscriptions: Mutex::new(HashMap::new()),
                    closed: AtomicBool::new(false),
                    in_flight_requests: AtomicU64::new(0),
                });
                if let Ok(mut state) = runtime.state.lock() {
                    if state.connections.len() >= MAX_ACTIVE_CONNECTIONS {
                        let _ = stream.shutdown(Shutdown::Both);
                        continue;
                    }
                    state.connections.insert(id, Arc::clone(&connection));
                }
                let runtime = Arc::clone(&runtime);
                let app_state = app_state.clone();
                thread::spawn(move || {
                    connection_loop(stream, Arc::clone(&connection), runtime, app_state);
                });
            }
            Err(err) if err.kind() == ErrorKind::WouldBlock => {
                thread::sleep(Duration::from_millis(20));
            }
            Err(err) => {
                if let Ok(mut state) = runtime.state.lock() {
                    state.last_error = Some(format!("mobile host accept failed: {err}"));
                }
                thread::sleep(Duration::from_millis(100));
            }
        }
    }
}

fn connection_loop(
    mut stream: TcpStream,
    connection: Arc<MobileConnection>,
    runtime: Arc<MobileHostRuntime>,
    app_state: Weak<Mutex<AppState>>,
) {
    let _ = stream.set_read_timeout(Some(FIRST_FRAME_TIMEOUT));
    let mut first_frame = true;
    loop {
        let payload = match read_frame(&mut stream) {
            Ok(payload) => payload,
            Err(ReadFrameError::Timeout) => {
                let subscribed = connection
                    .subscriptions
                    .lock()
                    .map(|subscriptions| !subscriptions.is_empty())
                    .unwrap_or(false);
                if !first_frame && subscribed {
                    continue;
                }
                break;
            }
            Err(ReadFrameError::Closed | ReadFrameError::Invalid | ReadFrameError::Io) => break,
        };
        if first_frame {
            first_frame = false;
            let _ = stream.set_read_timeout(Some(IDLE_TIMEOUT));
        }
        let request = match decode_request(&payload) {
            Ok(request) => request,
            Err(response) => {
                let _ = connection.send_payload(&response);
                break;
            }
        };
        let connection = Arc::clone(&connection);
        let runtime = Arc::clone(&runtime);
        let app_state = app_state.clone();
        if connection.in_flight_requests.fetch_add(1, Ordering::AcqRel)
            >= MAX_IN_FLIGHT_REQUESTS_PER_CONNECTION
        {
            connection.in_flight_requests.fetch_sub(1, Ordering::AcqRel);
            let response = error_envelope(
                request.id,
                "unavailable",
                "Too many in-flight mobile requests",
                None,
            );
            let _ = connection.send_payload(&response);
            continue;
        }
        thread::spawn(move || {
            let response = dispatch_request(&runtime, &connection, &app_state, request);
            let _ = connection.send_payload(&response);
            connection.in_flight_requests.fetch_sub(1, Ordering::AcqRel);
        });
    }
    connection.close();
    if let Ok(mut state) = runtime.state.lock() {
        state.connections.remove(&connection.id);
    }
}

enum ReadFrameError {
    Timeout,
    Closed,
    Invalid,
    Io,
}

fn read_frame(stream: &mut TcpStream) -> Result<Vec<u8>, ReadFrameError> {
    let mut header = [0_u8; FRAME_HEADER_BYTES];
    read_exact(stream, &mut header)?;
    let length = u32::from_be_bytes(header) as usize;
    if length > MAX_FRAME_BYTES {
        let response = error_envelope(Value::Null, "frame_decode_error", "Invalid frame", None);
        let _ = write_frame(stream, &response);
        return Err(ReadFrameError::Invalid);
    }
    let mut payload = vec![0_u8; length];
    read_exact(stream, &mut payload)?;
    Ok(payload)
}

fn read_exact(stream: &mut TcpStream, buffer: &mut [u8]) -> Result<(), ReadFrameError> {
    let mut offset = 0;
    while offset < buffer.len() {
        match stream.read(&mut buffer[offset..]) {
            Ok(0) => return Err(ReadFrameError::Closed),
            Ok(read) => offset += read,
            Err(err) if matches!(err.kind(), ErrorKind::WouldBlock | ErrorKind::TimedOut) => {
                return Err(ReadFrameError::Timeout);
            }
            Err(_) => return Err(ReadFrameError::Io),
        }
    }
    Ok(())
}

fn write_frame(stream: &mut TcpStream, payload: &[u8]) -> std::io::Result<()> {
    if payload.len() > MAX_FRAME_BYTES {
        return Err(std::io::Error::new(
            ErrorKind::InvalidData,
            "mobile frame exceeds maximum size",
        ));
    }
    stream.write_all(&(payload.len() as u32).to_be_bytes())?;
    stream.write_all(payload)?;
    stream.flush()
}

impl MobileConnection {
    fn send_payload(&self, payload: &[u8]) -> bool {
        if self.closed.load(Ordering::Acquire) {
            return false;
        }
        let Ok(mut writer) = self.writer.lock() else {
            return false;
        };
        if write_frame(&mut writer, payload).is_err() {
            drop(writer);
            self.close();
            return false;
        }
        true
    }

    fn send_event(&self, topic: &str, payload: &Value) -> bool {
        let subscribed = self
            .subscriptions
            .lock()
            .map(|streams| streams.values().any(|topics| topics.contains(topic)))
            .unwrap_or(false);
        if !subscribed {
            return false;
        }
        let envelope = json!({
            "kind": "event",
            "topic": topic,
            "payload": payload
        });
        serde_json::to_vec(&envelope)
            .ok()
            .is_some_and(|payload| self.send_payload(&payload))
    }

    fn close(&self) {
        if self.closed.swap(true, Ordering::AcqRel) {
            return;
        }
        if let Ok(writer) = self.writer.lock() {
            let _ = writer.shutdown(Shutdown::Both);
        }
    }
}

fn decode_request(payload: &[u8]) -> Result<MobileRequest, Vec<u8>> {
    let value = serde_json::from_slice::<Value>(payload)
        .map_err(|_| error_envelope(Value::Null, "parse_error", "Invalid JSON", None))?;
    let object = value.as_object().ok_or_else(|| {
        error_envelope(Value::Null, "invalid_request", "Expected JSON object", None)
    })?;
    let id = object.get("id").cloned().unwrap_or(Value::Null);
    let method = object
        .get("method")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|method| !method.is_empty())
        .map(ToString::to_string)
        .ok_or_else(|| error_envelope(id.clone(), "invalid_request", "Missing method", None))?;
    let params = object.get("params").cloned().unwrap_or_else(|| json!({}));
    if !params.is_object() {
        return Err(error_envelope(
            id,
            "invalid_request",
            "params must be an object",
            None,
        ));
    }
    let auth = match object.get("auth") {
        Some(Value::Object(auth)) => auth,
        Some(_) => {
            return Err(error_envelope(
                id,
                "invalid_request",
                "auth must be an object",
                None,
            ))
        }
        None => {
            static EMPTY: OnceLock<Map<String, Value>> = OnceLock::new();
            EMPTY.get_or_init(Map::new)
        }
    };
    let attach_token = auth
        .get("attach_token")
        .and_then(Value::as_str)
        .and_then(non_empty);
    if attach_token
        .as_ref()
        .is_some_and(|token| token.len() > MAX_ATTACH_TOKEN_BYTES)
    {
        return Err(error_envelope(
            id,
            "invalid_request",
            "attach_token is too large",
            None,
        ));
    }
    let stack_access_token = auth
        .get("stack_access_token")
        .and_then(Value::as_str)
        .and_then(non_empty);
    if stack_access_token
        .as_ref()
        .is_some_and(|token| token.len() > MAX_STACK_TOKEN_BYTES)
    {
        return Err(error_envelope(
            id,
            "invalid_request",
            "stack_access_token is too large",
            None,
        ));
    }
    Ok(MobileRequest {
        id,
        method,
        params,
        attach_token,
        stack_access_token,
    })
}

fn dispatch_request(
    runtime: &Arc<MobileHostRuntime>,
    connection: &Arc<MobileConnection>,
    app_state: &Weak<Mutex<AppState>>,
    request: MobileRequest,
) -> Vec<u8> {
    let Some(app_state) = app_state.upgrade() else {
        return error_envelope(
            request.id,
            "unavailable",
            "Mobile host is shutting down",
            None,
        );
    };

    if request.method == "mobile.host.status" {
        let include_identity =
            authorize_stack(runtime, &app_state, request.stack_access_token.as_deref()).is_ok();
        let result = network_status(include_identity);
        return ok_envelope(request.id, result);
    }

    if let Err(error) = authorize_stack(runtime, &app_state, request.stack_access_token.as_deref())
    {
        return app_error_envelope(request.id, error);
    }
    if let Some(token) = request.attach_token.as_deref() {
        if let Err(error) = authorize_ticket(runtime, token, &request.method, &request.params) {
            return app_error_envelope(request.id, error);
        }
    }

    if request.method == "mobile.events.subscribe" {
        return subscribe(connection, request);
    }
    if request.method == "mobile.events.unsubscribe" {
        return unsubscribe(connection, request);
    }
    if !is_mobile_method(&request.method) {
        return error_envelope(
            request.id,
            "method_not_found",
            "Unknown mobile method",
            Some(json!({"method": request.method})),
        );
    }

    let method = canonical_mobile_method(&request.method);
    let mut routed_params = request.params.clone();
    if method == "workspace.create" {
        routed_params["focus"] = json!(false);
    }
    let result = match app_state.lock() {
        Ok(mut app) => {
            let result = app.handle(method, &routed_params).and_then(|mut result| {
                if method == "workspace.create" {
                    let workspace_id = result
                        .get("workspace_id")
                        .and_then(Value::as_str)
                        .ok_or_else(|| {
                            AppError::internal(
                                "workspace.create did not return a workspace identifier",
                            )
                        })?
                        .to_string();
                    let mut mobile_result = app.handle(
                        "mobile.workspace.list",
                        &json!({"workspace_id": workspace_id}),
                    )?;
                    mobile_result["created_workspace_id"] = json!(workspace_id);
                    result = mobile_result;
                }
                Ok(result)
            });
            if let Ok(value) = &result {
                app.record_socket_event(method, &routed_params, value);
            }
            result
        }
        Err(_) => Err(AppError::internal("app state lock poisoned")),
    };
    match result {
        Ok(result) => {
            if let Some(token) = request.attach_token.as_deref() {
                record_created_resources(runtime, token, method, &result);
            }
            ok_envelope(request.id, result)
        }
        Err(error) => app_error_envelope(request.id, error),
    }
}

fn subscribe(connection: &Arc<MobileConnection>, request: MobileRequest) -> Vec<u8> {
    let Some(topics) = request.params.get("topics").and_then(Value::as_array) else {
        return error_envelope(request.id, "invalid_params", "topics is required", None);
    };
    let topics = topics
        .iter()
        .filter_map(Value::as_str)
        .filter_map(non_empty)
        .collect::<HashSet<_>>();
    if topics.is_empty() {
        return error_envelope(request.id, "invalid_params", "topics is required", None);
    }
    let stream_id = request
        .params
        .get("stream_id")
        .and_then(Value::as_str)
        .and_then(non_empty)
        .unwrap_or_else(|| format!("linux-mobile-{}", connection.id));
    let already_subscribed = connection
        .subscriptions
        .lock()
        .map(|mut subscriptions| {
            subscriptions
                .insert(stream_id.clone(), topics.clone())
                .is_some()
        })
        .unwrap_or(false);
    let mut topics = topics.into_iter().collect::<Vec<_>>();
    topics.sort();
    ok_envelope(
        request.id,
        json!({
            "stream_id": stream_id,
            "topics": topics,
            "already_subscribed": already_subscribed
        }),
    )
}

fn unsubscribe(connection: &Arc<MobileConnection>, request: MobileRequest) -> Vec<u8> {
    let stream_id = request
        .params
        .get("stream_id")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string();
    let removed = connection
        .subscriptions
        .lock()
        .map(|mut subscriptions| subscriptions.remove(&stream_id).is_some())
        .unwrap_or(false);
    ok_envelope(
        request.id,
        json!({"stream_id": stream_id, "removed": removed}),
    )
}

fn authorize_stack(
    runtime: &Arc<MobileHostRuntime>,
    app_state: &Arc<Mutex<AppState>>,
    access_token: Option<&str>,
) -> Result<(), AppError> {
    let context = app_state
        .lock()
        .map_err(|_| AppError::internal("app state lock poisoned"))?
        .mobile_network_auth_context();
    let access_token = access_token
        .and_then(non_empty)
        .ok_or_else(|| AppError::unauthorized("Mobile sync authorization failed."))?;
    if context.dev_access_token.as_deref() == Some(access_token.as_str()) {
        return Ok(());
    }
    let local_user_id = context
        .local_user_id
        .and_then(|value| non_empty(&value))
        .ok_or_else(|| AppError::unauthorized("Mobile sync authorization failed."))?;
    let key = format!("{:x}", Sha256::digest(access_token.as_bytes()));
    let now = Instant::now();
    let cached_user = runtime.state.lock().ok().and_then(|mut state| {
        state.auth_cache.retain(|_, entry| entry.expires_at > now);
        state
            .auth_cache
            .get(&key)
            .map(|entry| entry.user_id.clone())
    });
    let remote_user_id = match cached_user {
        Some(user_id) => user_id,
        None => {
            let user_id = app::verify_mobile_stack_access_token_user(&access_token)?;
            if let Ok(mut state) = runtime.state.lock() {
                state.auth_cache.insert(
                    key,
                    AuthCacheEntry {
                        user_id: user_id.clone(),
                        expires_at: now + AUTH_CACHE_TTL,
                    },
                );
            }
            user_id
        }
    };
    if remote_user_id != local_user_id {
        return Err(AppError::new(
            "account_mismatch",
            "Sign in with the account that owns this Linux host to continue.",
        ));
    }
    Ok(())
}

fn authorize_ticket(
    runtime: &Arc<MobileHostRuntime>,
    token: &str,
    method: &str,
    params: &Value,
) -> Result<(), AppError> {
    let now = app::current_unix_millis();
    let record = runtime
        .state
        .lock()
        .map_err(|_| AppError::internal("mobile host state lock poisoned"))?
        .tickets
        .get(token)
        .cloned()
        .ok_or_else(|| AppError::unauthorized("Attach ticket is invalid or expired."))?;
    if record.expires_at_ms <= now {
        return Err(AppError::unauthorized(
            "Attach ticket is invalid or expired.",
        ));
    }
    if record.workspace_id.is_empty() {
        return Ok(());
    }
    if matches!(
        method,
        "mobile.workspace.list"
            | "workspace.list"
            | "workspace.create"
            | "mobile.terminal.create"
            | "terminal.create"
            | "mobile.events.subscribe"
            | "mobile.events.unsubscribe"
    ) {
        return Ok(());
    }
    let workspace = string_selection(params, &["workspace_id"]);
    let terminal = string_selection(params, &["surface_id", "terminal_id", "tab_id"]);
    if workspace
        .as_deref()
        .is_some_and(|workspace| record.created_workspace_ids.contains(workspace))
        || terminal
            .as_deref()
            .is_some_and(|terminal| record.created_terminal_ids.contains(terminal))
    {
        return Ok(());
    }
    if workspace.as_deref() != Some(record.workspace_id.as_str()) {
        return Err(AppError::forbidden(
            "Attach ticket is not valid for this workspace or terminal.",
        ));
    }
    if let Some(ticket_terminal) = record.terminal_id.as_deref() {
        if terminal.as_deref() != Some(ticket_terminal) {
            return Err(AppError::forbidden(
                "Attach ticket is not valid for this workspace or terminal.",
            ));
        }
    }
    Ok(())
}

fn record_created_resources(
    runtime: &Arc<MobileHostRuntime>,
    token: &str,
    method: &str,
    result: &Value,
) {
    let Ok(mut state) = runtime.state.lock() else {
        return;
    };
    let Some(record) = state.tickets.get_mut(token) else {
        return;
    };
    match method {
        "workspace.create" => {
            if let Some(id) = result.get("created_workspace_id").and_then(Value::as_str) {
                record.created_workspace_ids.insert(id.to_string());
            }
        }
        "mobile.terminal.create" | "terminal.create" => {
            if let Some(id) = result.get("created_terminal_id").and_then(Value::as_str) {
                record.created_terminal_ids.insert(id.to_string());
            }
        }
        _ => {}
    }
}

pub(crate) fn register_ticket(
    token: String,
    expires_at_ms: u64,
    workspace_id: String,
    terminal_id: Option<String>,
) {
    let Ok(mut state) = runtime().state.lock() else {
        return;
    };
    let now = app::current_unix_millis();
    state.tickets.retain(|_, ticket| ticket.expires_at_ms > now);
    state.tickets.insert(
        token,
        TicketRecord {
            expires_at_ms,
            workspace_id,
            terminal_id,
            created_workspace_ids: HashSet::new(),
            created_terminal_ids: HashSet::new(),
        },
    );
}

pub(crate) fn status(configured_routes: &[MobileAttachRoute]) -> RuntimeStatus {
    let runtime = runtime();
    let Ok(state) = runtime.state.lock() else {
        return RuntimeStatus::default();
    };
    let routes = if state.routes.is_empty() {
        if runtime.started.load(Ordering::Acquire) {
            Vec::new()
        } else {
            configured_routes.to_vec()
        }
    } else {
        state.routes.clone()
    };
    RuntimeStatus {
        is_running: !state.routes.is_empty(),
        port: routes.first().map(|route| route.port),
        configured_port: if state.configured_port == 0 {
            app::mobile_host_configured_port()
        } else {
            state.configured_port
        },
        uses_ephemeral_fallback: state.uses_ephemeral_fallback,
        routes,
        active_connection_count: state.connections.len(),
        last_error: state.last_error.clone(),
    }
}

fn network_status(include_identity: bool) -> Value {
    let configured = app::mobile_host_routes();
    let status = status(&configured);
    let mut payload = json!({
        "routes": app::mobile_attach_route_values(&status.routes),
        "terminal_fidelity": "render_grid",
        "capabilities": app::mobile_host_capabilities()
    });
    if include_identity {
        payload["host_platform"] = json!("linux");
        payload["mac_device_id"] = json!(app::mobile_host_device_id());
        payload["mac_display_name"] = json!(app::mobile_host_display_name());
        payload["mac_app_version"] = json!(env!("CARGO_PKG_VERSION"));
    }
    payload
}

fn event_loop(runtime: Arc<MobileHostRuntime>, app_state: Weak<Mutex<AppState>>) {
    loop {
        thread::sleep(EVENT_POLL_INTERVAL);
        let connections = runtime
            .state
            .lock()
            .map(|state| state.connections.values().cloned().collect::<Vec<_>>())
            .unwrap_or_default();
        if connections.is_empty() {
            continue;
        }
        let topics = connections
            .iter()
            .flat_map(|connection| {
                connection
                    .subscriptions
                    .lock()
                    .map(|streams| {
                        streams
                            .values()
                            .flat_map(|topics| topics.iter().cloned())
                            .collect::<Vec<_>>()
                    })
                    .unwrap_or_default()
            })
            .collect::<HashSet<_>>();
        if topics.is_empty() {
            continue;
        }
        let Some(app_state) = app_state.upgrade() else {
            for connection in connections {
                connection.close();
            }
            return;
        };

        let (workspace_payload, terminal_frames, notifications, chat_sessions) =
            match app_state.lock() {
                Ok(mut app) => {
                    let workspace_payload = topics
                        .contains("workspace.updated")
                        .then(|| app.handle("mobile.workspace.list", &json!({})).ok())
                        .flatten();
                    let terminal_frames = if topics.contains("terminal.render_grid") {
                        mobile_terminal_frames(&mut app)
                    } else {
                        Vec::new()
                    };
                    let notifications = if topics.contains("notification.dismissed")
                        || topics.contains("notification.badge")
                    {
                        mobile_notification_snapshot(&mut app)
                    } else {
                        None
                    };
                    let chat_sessions = if topics.contains("chat.message") {
                        mobile_chat_snapshots(&mut app)
                    } else {
                        Vec::new()
                    };
                    (
                        workspace_payload,
                        terminal_frames,
                        notifications,
                        chat_sessions,
                    )
                }
                Err(_) => continue,
            };

        if let Some(workspaces) = workspace_payload {
            let signature = value_signature(&workspaces);
            let changed = runtime
                .state
                .lock()
                .map(|mut state| {
                    if state.workspace_signature == Some(signature) {
                        false
                    } else {
                        state.workspace_signature = Some(signature);
                        true
                    }
                })
                .unwrap_or(false);
            if changed {
                for connection in &connections {
                    connection.send_event("workspace.updated", &json!({}));
                }
            }
        }

        for (surface_id, frame) in terminal_frames {
            let signature = value_signature(&frame);
            let changed = runtime
                .state
                .lock()
                .map(|mut state| {
                    if state.terminal_signatures.get(&surface_id) == Some(&signature) {
                        false
                    } else {
                        state.terminal_signatures.insert(surface_id, signature);
                        true
                    }
                })
                .unwrap_or(false);
            if changed {
                for connection in &connections {
                    connection.send_event("terminal.render_grid", &frame);
                }
            }
        }

        if let Some((notification_ids, unread_count)) = notifications {
            let (dismissed_ids, badge_changed) = runtime
                .state
                .lock()
                .map(|mut state| {
                    let dismissed = state
                        .notification_ids
                        .as_ref()
                        .map(|previous| {
                            previous
                                .difference(&notification_ids)
                                .cloned()
                                .collect::<Vec<_>>()
                        })
                        .unwrap_or_default();
                    let badge_changed = state.notification_unread_count != Some(unread_count);
                    state.notification_ids = Some(notification_ids);
                    state.notification_unread_count = Some(unread_count);
                    (dismissed, badge_changed)
                })
                .unwrap_or_default();
            if !dismissed_ids.is_empty() {
                let payload = json!({
                    "ids": dismissed_ids,
                    "unread_count": unread_count
                });
                for connection in &connections {
                    connection.send_event("notification.dismissed", &payload);
                }
            }
            if badge_changed {
                let payload = json!({"unread_count": unread_count});
                for connection in &connections {
                    connection.send_event("notification.badge", &payload);
                }
            }
        }

        if topics.contains("chat.message") {
            let current_chat_session_ids = chat_sessions
                .iter()
                .map(|(session_id, _, _)| session_id.clone())
                .collect::<HashSet<_>>();
            let removed_chat_session_ids = runtime
                .state
                .lock()
                .map(|mut state| {
                    let removed = state
                        .chat_session_ids
                        .as_ref()
                        .map(|previous| {
                            previous
                                .difference(&current_chat_session_ids)
                                .cloned()
                                .collect::<Vec<_>>()
                        })
                        .unwrap_or_default();
                    state.chat_session_ids = Some(current_chat_session_ids);
                    for session_id in &removed {
                        state.chat_state_signatures.remove(session_id);
                        state.chat_descriptor_signatures.remove(session_id);
                        state.chat_history_signatures.remove(session_id);
                    }
                    removed
                })
                .unwrap_or_default();
            for session_id in removed_chat_session_ids {
                let payload = json!({
                    "session_id": session_id,
                    "event": {
                        "event": "state_changed",
                        "state": {"state": "ended"}
                    }
                });
                for connection in &connections {
                    connection.send_event("chat.message", &payload);
                }
            }

            for (session_id, descriptor, history) in chat_sessions {
                let state_value = descriptor
                    .get("state")
                    .cloned()
                    .unwrap_or_else(|| json!({"state": "idle"}));
                let state_signature =
                    value_signature(&chat_state_for_signature(state_value.clone()));
                let descriptor_for_signature = chat_descriptor_for_signature(descriptor.clone());
                let descriptor_signature = value_signature(&descriptor_for_signature);
                let history_signature = value_signature(&history);
                let (state_changed, descriptor_changed, history_changed) = runtime
                    .state
                    .lock()
                    .map(|mut state| {
                        let state_changed = state
                            .chat_state_signatures
                            .insert(session_id.clone(), state_signature)
                            != Some(state_signature);
                        let descriptor_changed = state
                            .chat_descriptor_signatures
                            .insert(session_id.clone(), descriptor_signature)
                            != Some(descriptor_signature);
                        let history_changed = state
                            .chat_history_signatures
                            .insert(session_id.clone(), history_signature)
                            != Some(history_signature);
                        (state_changed, descriptor_changed, history_changed)
                    })
                    .unwrap_or((false, false, false));
                if state_changed {
                    let payload = json!({
                        "session_id": session_id.clone(),
                        "event": {
                            "event": "state_changed",
                            "state": state_value
                        }
                    });
                    for connection in &connections {
                        connection.send_event("chat.message", &payload);
                    }
                }
                if descriptor_changed {
                    let payload = json!({
                        "session_id": session_id.clone(),
                        "event": {
                            "event": "descriptor_changed",
                            "descriptor": descriptor
                        }
                    });
                    for connection in &connections {
                        connection.send_event("chat.message", &payload);
                    }
                }
                if history_changed {
                    let blocks = history
                        .get("terminal_blocks")
                        .cloned()
                        .unwrap_or_else(|| json!([]));
                    let payload = json!({
                        "session_id": session_id.clone(),
                        "event": {
                            "event": "terminal_blocks",
                            "blocks": blocks
                        }
                    });
                    for connection in &connections {
                        connection.send_event("chat.message", &payload);
                    }
                }
            }
        }
    }
}

fn mobile_terminal_frames(app: &mut AppState) -> Vec<(String, Value)> {
    let Ok(workspace_list) = app.handle("mobile.workspace.list", &json!({})) else {
        return Vec::new();
    };
    let mut frames = Vec::new();
    for workspace in workspace_list
        .get("workspaces")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        let Some(workspace_id) = workspace
            .get("workspace_id")
            .or_else(|| workspace.get("id"))
            .and_then(Value::as_str)
        else {
            continue;
        };
        for terminal in workspace
            .get("terminals")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
        {
            let Some(surface_id) = terminal
                .get("surface_id")
                .or_else(|| terminal.get("id"))
                .and_then(Value::as_str)
            else {
                continue;
            };
            let Ok(replay) = app.handle(
                "mobile.terminal.replay",
                &json!({
                    "workspace_id": workspace_id,
                    "terminal_id": surface_id
                }),
            ) else {
                continue;
            };
            if let Some(frame) = replay.get("render_grid").cloned() {
                frames.push((surface_id.to_string(), frame));
            }
        }
    }
    frames
}

fn mobile_notification_snapshot(app: &mut AppState) -> Option<(HashSet<String>, usize)> {
    let list = app.handle("notification.list", &json!({})).ok()?;
    let rows = list.get("notifications")?.as_array()?;
    let ids = rows
        .iter()
        .filter_map(|row| row.get("id").and_then(Value::as_str))
        .filter_map(non_empty)
        .collect::<HashSet<_>>();
    let unread_count = rows
        .iter()
        .filter(|row| !row.get("read").and_then(Value::as_bool).unwrap_or(false))
        .count();
    Some((ids, unread_count))
}

fn mobile_chat_snapshots(app: &mut AppState) -> Vec<(String, Value, Value)> {
    let Ok(sessions) = app.handle("mobile.chat.sessions", &json!({})) else {
        return Vec::new();
    };
    sessions
        .get("sessions")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|descriptor| {
            let session_id = descriptor.get("session_id")?.as_str()?.to_string();
            let history = app
                .handle(
                    "mobile.chat.history",
                    &json!({"session_id": session_id.clone(), "limit": 200}),
                )
                .ok()?;
            Some((session_id, descriptor.clone(), history))
        })
        .collect()
}

fn is_mobile_method(method: &str) -> bool {
    matches!(
        method,
        "mobile.attach_ticket.create"
            | "mobile.workspace.list"
            | "workspace.list"
            | "workspace.create"
            | "mobile.terminal.create"
            | "terminal.create"
            | "mobile.terminal.input"
            | "terminal.input"
            | "mobile.terminal.paste"
            | "terminal.paste"
            | "mobile.terminal.paste_image"
            | "terminal.paste_image"
            | "mobile.terminal.replay"
            | "terminal.replay"
            | "mobile.terminal.viewport"
            | "terminal.viewport"
            | "mobile.terminal.scroll"
            | "terminal.scroll"
            | "mobile.terminal.mouse"
            | "terminal.mouse"
            | "workspace.action"
            | "workspace.close"
            | "workspace.group.collapse"
            | "workspace.group.expand"
            | "notification.dismiss"
            | "notification.reconcile"
            | "dogfood.feedback.submit"
            | "mobile.chat.sessions"
            | "mobile.chat.history"
            | "mobile.chat.send"
            | "mobile.chat.interrupt"
            | "mobile.chat.answer"
    )
}

fn canonical_mobile_method(method: &str) -> &str {
    match method {
        "workspace.list" => "mobile.workspace.list",
        "workspace.action" => "mobile.workspace.action",
        "workspace.close" => "mobile.workspace.close",
        "workspace.group.collapse" => "mobile.workspace.group.collapse",
        "workspace.group.expand" => "mobile.workspace.group.expand",
        other => other,
    }
}

fn string_selection(params: &Value, keys: &[&str]) -> Option<String> {
    keys.iter()
        .filter_map(|key| params.get(*key).and_then(Value::as_str))
        .find_map(non_empty)
}

fn value_signature(value: &Value) -> [u8; 32] {
    let bytes = serde_json::to_vec(value).unwrap_or_default();
    Sha256::digest(bytes).into()
}

fn chat_state_for_signature(mut state: Value) -> Value {
    if let Some(object) = state.as_object_mut() {
        object.remove("since");
    }
    state
}

fn chat_descriptor_for_signature(mut descriptor: Value) -> Value {
    if let Some(object) = descriptor.as_object_mut() {
        object.remove("last_activity_at");
        if let Some(state) = object.get_mut("state") {
            *state = chat_state_for_signature(state.clone());
        }
    }
    descriptor
}

fn ok_envelope(id: Value, result: Value) -> Vec<u8> {
    serde_json::to_vec(&json!({"id": id, "ok": true, "result": result}))
        .unwrap_or_else(|_| encode_fallback_error())
}

fn app_error_envelope(id: Value, error: AppError) -> Vec<u8> {
    error_envelope(id, error.code, &error.message, None)
}

fn error_envelope(id: Value, code: &str, message: &str, data: Option<Value>) -> Vec<u8> {
    let mut error = json!({"code": code, "message": message});
    if let Some(data) = data {
        error["data"] = data;
    }
    serde_json::to_vec(&json!({"id": id, "ok": false, "error": error}))
        .unwrap_or_else(|_| encode_fallback_error())
}

fn encode_fallback_error() -> Vec<u8> {
    br#"{"id":null,"ok":false,"error":{"code":"encode_error","message":"Failed to encode JSON"}}"#
        .to_vec()
}

fn non_empty(value: &str) -> Option<String> {
    let value = value.trim();
    (!value.is_empty()).then(|| value.to_string())
}

fn normalized_env(key: &str) -> Option<String> {
    std::env::var(key).ok().and_then(|value| non_empty(&value))
}

fn env_false(key: &str) -> bool {
    normalized_env(key).is_some_and(|value| {
        matches!(
            value.to_ascii_lowercase().as_str(),
            "0" | "false" | "no" | "off"
        )
    })
}

#[cfg(test)]
mod tests {
    use super::{chat_descriptor_for_signature, chat_state_for_signature, value_signature};
    use serde_json::json;

    #[test]
    fn chat_signatures_ignore_activity_timestamps_but_track_state() {
        let first_state = json!({"state": "working", "since": "2026-07-16T12:00:00Z"});
        let later_state = json!({"state": "working", "since": "2026-07-16T12:00:10Z"});
        assert_eq!(
            value_signature(&chat_state_for_signature(first_state)),
            value_signature(&chat_state_for_signature(later_state))
        );

        let first = json!({
            "session_id": "session",
            "title": "Codex",
            "state": {"state": "working", "since": "2026-07-16T12:00:00Z"},
            "last_activity_at": "2026-07-16T12:00:00Z"
        });
        let later = json!({
            "session_id": "session",
            "title": "Codex",
            "state": {"state": "working", "since": "2026-07-16T12:00:10Z"},
            "last_activity_at": "2026-07-16T12:00:10Z"
        });
        assert_eq!(
            value_signature(&chat_descriptor_for_signature(first)),
            value_signature(&chat_descriptor_for_signature(later))
        );

        let ended = json!({
            "session_id": "session",
            "title": "Codex",
            "state": {"state": "ended"},
            "last_activity_at": "2026-07-16T12:00:10Z"
        });
        assert_ne!(
            value_signature(&chat_descriptor_for_signature(json!({
                "session_id": "session",
                "title": "Codex",
                "state": {"state": "working"},
            }))),
            value_signature(&chat_descriptor_for_signature(ended))
        );
    }
}
