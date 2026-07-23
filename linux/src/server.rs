use crate::{
    app::{feed_wait_timeout, AppError, AppState},
    browser_runtime::{
        self, BrowserEvaluationAttempt, BrowserPdfAttempt, BrowserScreenshotAttempt,
    },
    mobile_host,
};
use anyhow::{Context, Result};
use base64::Engine;
use serde_json::{json, Value};
use std::fs;
use std::io::{BufRead, BufReader, Write};
use std::os::unix::fs::FileTypeExt;
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

pub fn run_server(socket_path: &str) -> Result<()> {
    let debug_log_path = debug_log_path_for_socket(socket_path);
    if let Some(parent) = debug_log_path.parent() {
        let _ = fs::create_dir_all(parent);
    }
    let _ = fs::write(&debug_log_path, "");
    let state = Arc::new(Mutex::new(AppState::with_paths(
        Some(debug_log_path),
        Some(socket_path.to_string()),
    )?));
    run_server_with_state(socket_path, state)
}

pub fn run_server_with_state(socket_path: &str, state: Arc<Mutex<AppState>>) -> Result<()> {
    mobile_host::start(Arc::clone(&state));
    let listener = bind_socket(socket_path)?;
    serve_listener(listener, state);
    Ok(())
}

pub fn spawn_server_with_state(
    socket_path: &str,
    state: Arc<Mutex<AppState>>,
) -> Result<thread::JoinHandle<()>> {
    mobile_host::start(Arc::clone(&state));
    let listener = bind_socket(socket_path)?;
    Ok(thread::spawn(move || serve_listener(listener, state)))
}

pub fn debug_log_path_for_socket(socket_path: &str) -> PathBuf {
    debug_log_path_for_socket_in_state_dir(socket_path, &state_dir())
}

fn debug_log_path_for_socket_in_state_dir(socket_path: &str, state_dir: &Path) -> PathBuf {
    let path = Path::new(socket_path);
    let file_name = path
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or_default();
    if let Some(slug) = file_name
        .strip_prefix("cmux-debug-")
        .and_then(|value| value.strip_suffix(".sock"))
    {
        return state_dir
            .join("logs")
            .join(format!("cmux-debug-{slug}.log"));
    }
    state_dir.join("logs/cmux-debug.log")
}

fn state_dir() -> PathBuf {
    let xdg_state_home = normalized_env("XDG_STATE_HOME");
    let home = normalized_env("HOME");
    state_dir_from_env(
        xdg_state_home.as_deref(),
        home.as_deref(),
        &std::env::temp_dir(),
    )
}

fn state_dir_from_env(
    xdg_state_home: Option<&str>,
    home: Option<&str>,
    temp_dir: &Path,
) -> PathBuf {
    if let Some(path) = xdg_state_home {
        return PathBuf::from(path).join("cmux");
    }
    if let Some(home) = home {
        return PathBuf::from(home).join(".local/state/cmux");
    }
    temp_dir.join("cmux")
}

fn normalized_env(key: &str) -> Option<String> {
    let value = std::env::var(key).ok()?;
    let trimmed = value.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
    }
}

fn bind_socket(socket_path: &str) -> Result<UnixListener> {
    let path = Path::new(socket_path);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).with_context(|| {
            format!(
                "failed to create socket parent directory {}",
                parent.display()
            )
        })?;
    }
    remove_confirmed_stale_socket(path)?;

    let listener = UnixListener::bind(path).with_context(|| socket_bind_error_context(path))?;
    write_socket_markers(socket_path);
    Ok(listener)
}

fn remove_confirmed_stale_socket(path: &Path) -> Result<()> {
    let metadata = match fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(err) => {
            return Err(err)
                .with_context(|| format!("failed to inspect socket path {}", path.display()));
        }
    };
    if !metadata.file_type().is_socket() {
        anyhow::bail!(
            "refusing to replace non-socket control path {}",
            path.display()
        );
    }

    match UnixStream::connect(path) {
        Ok(_) => anyhow::bail!(
            "cmux control socket is already active at {}",
            path.display()
        ),
        Err(err) if err.kind() == std::io::ErrorKind::ConnectionRefused => {
            match fs::remove_file(path) {
                Ok(()) => Ok(()),
                Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(()),
                Err(err) => Err(err)
                    .with_context(|| format!("failed to remove stale socket {}", path.display())),
            }
        }
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(err) => {
            Err(err).with_context(|| format!("failed to probe control socket {}", path.display()))
        }
    }
}

fn serve_listener(listener: UnixListener, state: Arc<Mutex<AppState>>) {
    for stream in listener.incoming() {
        match stream {
            Ok(stream) => {
                let state = Arc::clone(&state);
                thread::spawn(move || {
                    let _ = handle_client(stream, state);
                });
            }
            Err(err) => eprintln!("cmux-linux: socket accept failed: {err}"),
        }
    }
}

fn socket_bind_error_context(path: &Path) -> String {
    format!(
        "failed to bind socket {} (parent: {})",
        path.display(),
        path.parent()
            .map(|parent| parent.display().to_string())
            .unwrap_or_else(|| "<none>".to_string())
    )
}

fn write_socket_markers(socket_path: &str) {
    let marker = socket_marker_path_in_state_dir(&state_dir());
    if let Some(parent) = marker.parent() {
        let _ = fs::create_dir_all(parent);
    }
    let _ = fs::write(marker, format!("{socket_path}\n"));
}

fn socket_marker_path_in_state_dir(state_dir: &Path) -> PathBuf {
    state_dir.join("last-socket-path")
}

fn handle_client(stream: UnixStream, state: Arc<Mutex<AppState>>) -> Result<()> {
    let reader_stream = stream.try_clone().context("failed to clone socket")?;
    let mut reader = BufReader::new(reader_stream);
    let mut writer = stream;
    let mut line = String::new();

    loop {
        line.clear();
        let read = reader
            .read_line(&mut line)
            .context("failed to read socket line")?;
        if read == 0 {
            break;
        }

        match serde_json::from_str::<Value>(&line) {
            Ok(request) => {
                if request.get("method").and_then(Value::as_str) == Some("events.stream") {
                    handle_events_stream_request(&request, &state, &mut writer)?;
                    break;
                }
                let response = dispatch_request(&request, &state);
                serde_json::to_writer(&mut writer, &response)
                    .context("failed to write response")?;
                writer.write_all(b"\n").context("failed to write newline")?;
            }
            Err(_) => {
                let response = dispatch_legacy_request(line.trim(), &state);
                writer
                    .write_all(response.as_bytes())
                    .context("failed to write legacy response")?;
                writer.write_all(b"\n").context("failed to write newline")?;
            }
        }
        writer.flush().context("failed to flush response")?;
    }
    Ok(())
}

fn dispatch_legacy_request(command: &str, state: &Arc<Mutex<AppState>>) -> String {
    let result = match state.lock() {
        Ok(mut app) => app.handle_legacy_v1(command),
        Err(_) => Err(crate::app::AppError::internal("app state lock poisoned")),
    };
    match result {
        Ok(text) => text,
        Err(err) => format!("ERROR {}: {}", err.code, err.message),
    }
}

fn dispatch_request(request: &Value, state: &Arc<Mutex<AppState>>) -> Value {
    let id = request.get("id").cloned().unwrap_or(Value::Null);
    let method = request
        .get("method")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let params = request.get("params").cloned().unwrap_or_else(|| json!({}));

    if method == "feed.push" {
        match feed_wait_timeout(&params) {
            Ok(wait_timeout) if wait_timeout > 0.0 => {
                return dispatch_waiting_feed_push_request(id, params, state, wait_timeout);
            }
            Ok(_) => {}
            Err(err) => {
                return json!({
                    "id": id,
                    "ok": false,
                    "error": {"code": err.code, "message": err.message}
                });
            }
        }
    }
    if method == "browser.screenshot" {
        return dispatch_live_browser_screenshot_request(id, params, state);
    }
    if method == "browser.pdf" {
        return dispatch_live_browser_pdf_request(id, params, state);
    }
    if is_live_browser_read_method(method) {
        return dispatch_live_browser_read_request(id, method, params, state);
    }

    let result = match state.lock() {
        Ok(mut app) => {
            let result = app.handle(method, &params);
            if let Ok(value) = &result {
                app.record_socket_event(method, &params, value);
            }
            result
        }
        Err(_) => Err(crate::app::AppError::internal("app state lock poisoned")),
    };

    match result {
        Ok(result) => json!({"id": id, "ok": true, "result": result}),
        Err(err) => json!({
            "id": id,
            "ok": false,
            "error": {"code": err.code, "message": err.message}
        }),
    }
}

fn dispatch_live_browser_screenshot_request(
    id: Value,
    params: Value,
    state: &Arc<Mutex<AppState>>,
) -> Value {
    let prepared = match state.lock() {
        Ok(mut app) => (|| -> Result<_, AppError> {
            let (surface_id, _) = app.browser_runtime_target("browser.screenshot", &params)?;
            let model_result = app.handle("browser.screenshot", &params)?;
            Ok((surface_id, model_result))
        })(),
        Err(_) => Err(AppError::internal("app state lock poisoned")),
    };

    let result = prepared.and_then(|(surface_id, model_result)| {
        match browser_runtime::capture_live_browser_screenshot(
            &surface_id,
            browser_screenshot_full_document(&params),
        ) {
            BrowserScreenshotAttempt::Unavailable => Ok(model_result),
            BrowserScreenshotAttempt::Completed(Ok(screenshot)) => {
                Ok(live_browser_screenshot_result(model_result, screenshot))
            }
            BrowserScreenshotAttempt::Completed(Err(err)) => Err(AppError::invalid_state(format!(
                "browser.screenshot failed in live WebKitGTK: {err}"
            ))),
        }
    });

    if let Ok(value) = &result {
        if let Ok(mut app) = state.lock() {
            app.record_socket_event("browser.screenshot", &params, value);
        }
    }
    rpc_result_response(id, result)
}

fn dispatch_live_browser_pdf_request(
    id: Value,
    params: Value,
    state: &Arc<Mutex<AppState>>,
) -> Value {
    let output_path = browser_pdf_output_path(&params);
    let prepared = match state.lock() {
        Ok(mut app) => (|| -> Result<_, AppError> {
            let (surface_id, _) = app.browser_runtime_target("browser.pdf", &params)?;
            let mut model_params = params.clone();
            if let Some(model_params) = model_params.as_object_mut() {
                model_params.remove("path");
                model_params.remove("out");
            }
            let model_result = app.handle("browser.pdf", &model_params)?;
            Ok((surface_id, model_result))
        })(),
        Err(_) => Err(AppError::internal("app state lock poisoned")),
    };

    let result = prepared.and_then(|(surface_id, model_result)| {
        let native_pdf = match browser_runtime::print_live_browser_pdf(&surface_id) {
            BrowserPdfAttempt::Unavailable => None,
            BrowserPdfAttempt::Completed(Ok(pdf)) => Some(pdf),
            BrowserPdfAttempt::Completed(Err(err)) => {
                return Err(AppError::invalid_state(format!(
                    "browser.pdf failed in live WebKitGTK: {err}"
                )));
            }
        };
        live_browser_pdf_result(model_result, native_pdf, output_path.as_deref())
    });

    if let Ok(value) = &result {
        if let Ok(mut app) = state.lock() {
            app.record_socket_event("browser.pdf", &params, value);
        }
    }
    rpc_result_response(id, result)
}

fn browser_pdf_output_path(params: &Value) -> Option<String> {
    params
        .get("path")
        .or_else(|| params.get("out"))
        .and_then(Value::as_str)
        .filter(|path| !path.is_empty())
        .map(ToString::to_string)
}

fn live_browser_pdf_result(
    mut model_result: Value,
    native_pdf: Option<browser_runtime::BrowserPdf>,
    output_path: Option<&str>,
) -> Result<Value, AppError> {
    let native = native_pdf.is_some();
    let bytes = match native_pdf {
        Some(pdf) => pdf.bytes,
        None => {
            let encoded = model_result
                .get("pdf_base64")
                .and_then(Value::as_str)
                .ok_or_else(|| AppError::internal("browser.pdf returned no PDF data"))?;
            base64::engine::general_purpose::STANDARD
                .decode(encoded)
                .map_err(|err| AppError::internal(format!("invalid browser PDF data: {err}")))?
        }
    };
    if !bytes.starts_with(b"%PDF-") {
        return Err(AppError::internal("browser.pdf result is not a PDF"));
    }
    if let Some(path) = output_path {
        let path = Path::new(path);
        if let Some(parent) = path
            .parent()
            .filter(|parent| !parent.as_os_str().is_empty())
        {
            fs::create_dir_all(parent).map_err(|err| AppError::internal(err.to_string()))?;
        }
        fs::write(path, &bytes).map_err(|err| AppError::internal(err.to_string()))?;
    }

    let fallback_page_count = model_result
        .get("page_count")
        .and_then(Value::as_u64)
        .unwrap_or(1);
    let page_count = if native {
        pdf_page_count(&bytes).unwrap_or(fallback_page_count)
    } else {
        fallback_page_count
    };
    let encoded = base64::engine::general_purpose::STANDARD.encode(&bytes);
    if let Some(result) = model_result.as_object_mut() {
        result.insert("pdf_base64".to_string(), json!(encoded));
        result.insert("mime_type".to_string(), json!("application/pdf"));
        result.insert("bytes".to_string(), json!(bytes.len()));
        result.insert("path".to_string(), json!(output_path));
        result.insert("page_count".to_string(), json!(page_count));
        Ok(model_result)
    } else {
        Ok(json!({
            "pdf_base64": encoded,
            "mime_type": "application/pdf",
            "bytes": bytes.len(),
            "path": output_path,
            "page_count": page_count
        }))
    }
}

fn pdf_page_count(bytes: &[u8]) -> Option<u64> {
    let spaced = bytes
        .windows(b"/Type /Page".len())
        .enumerate()
        .filter(|(index, value)| {
            *value == b"/Type /Page" && bytes.get(index + value.len()) != Some(&b's')
        })
        .count();
    let compact = bytes
        .windows(b"/Type/Page".len())
        .enumerate()
        .filter(|(index, value)| {
            *value == b"/Type/Page" && bytes.get(index + value.len()) != Some(&b's')
        })
        .count();
    u64::try_from(spaced + compact)
        .ok()
        .filter(|count| *count > 0)
}

fn browser_screenshot_full_document(params: &Value) -> bool {
    ["full_page", "fullPage", "full_document", "full"]
        .into_iter()
        .find_map(|key| params.get(key).and_then(Value::as_bool))
        .unwrap_or(false)
}

fn live_browser_screenshot_result(
    mut model_result: Value,
    screenshot: browser_runtime::BrowserScreenshot,
) -> Value {
    let png_base64 = base64::engine::general_purpose::STANDARD.encode(screenshot.png);
    if let Some(result) = model_result.as_object_mut() {
        result.insert("png_base64".to_string(), json!(png_base64));
        result.insert("width".to_string(), json!(screenshot.width));
        result.insert("height".to_string(), json!(screenshot.height));
        model_result
    } else {
        json!({
            "png_base64": png_base64,
            "width": screenshot.width,
            "height": screenshot.height
        })
    }
}

fn dispatch_live_browser_read_request(
    id: Value,
    method: &str,
    params: Value,
    state: &Arc<Mutex<AppState>>,
) -> Value {
    let prepared = match state.lock() {
        Ok(mut app) => (|| -> Result<_, AppError> {
            let (surface_id, selector) = app.browser_runtime_target(method, &params)?;
            let model_result = app.handle(method, &params)?;
            let script =
                browser_runtime::live_browser_read_script(method, &params, selector.as_deref())
                    .ok_or_else(|| {
                        AppError::internal(format!("no live browser script for {method}"))
                    })?;
            Ok((surface_id, model_result, script))
        })(),
        Err(_) => Err(AppError::internal("app state lock poisoned")),
    };

    let result = prepared.and_then(|(surface_id, model_result, script)| {
        match browser_runtime::evaluate_in_live_browser(&surface_id, &script) {
            BrowserEvaluationAttempt::Unavailable => Ok(model_result),
            BrowserEvaluationAttempt::Completed(Ok(value)) => {
                if method == "browser.evalhandle" {
                    let mut app = state
                        .lock()
                        .map_err(|_| AppError::internal("app state lock poisoned"))?;
                    app.browser_runtime_replace_eval_handle(&surface_id, &model_result, value)
                } else if method == "browser.snapshot" {
                    let mut app = state
                        .lock()
                        .map_err(|_| AppError::internal("app state lock poisoned"))?;
                    app.browser_runtime_replace_snapshot(&surface_id, &model_result, &value)
                } else {
                    Ok(live_browser_read_result(method, &model_result, value))
                }
            }
            BrowserEvaluationAttempt::Completed(Err(err)) => Err(AppError::invalid_state(format!(
                "{method} failed in live WebKitGTK: {err}"
            ))),
        }
    });

    if let Ok(value) = &result {
        if let Ok(mut app) = state.lock() {
            app.record_socket_event(method, &params, value);
        }
    }
    rpc_result_response(id, result)
}

fn is_live_browser_read_method(method: &str) -> bool {
    matches!(
        method,
        "browser.eval"
            | "browser.evalhandle"
            | "browser.snapshot"
            | "browser.content"
            | "browser.innertext"
            | "browser.get.text"
            | "browser.get.html"
            | "browser.get.value"
            | "browser.get.attr"
            | "browser.get.title"
            | "browser.get.count"
            | "browser.get.box"
            | "browser.get.styles"
            | "browser.is.visible"
            | "browser.is.enabled"
            | "browser.is.checked"
    )
}

fn live_browser_read_result(method: &str, model_result: &Value, value: Value) -> Value {
    match method {
        "browser.content" => {
            let mut result = model_result.clone();
            if let (Some(result), Some(live)) = (result.as_object_mut(), value.as_object()) {
                for key in ["html", "content", "text", "inner_text", "title", "url"] {
                    if let Some(value) = live.get(key) {
                        result.insert(key.to_string(), value.clone());
                    }
                }
            }
            result
        }
        "browser.innertext" => {
            json!({"value": value.clone(), "text": value.clone(), "inner_text": value})
        }
        "browser.get.title" => json!({"title": value.clone(), "value": value}),
        "browser.get.count" => json!({"count": value.clone(), "value": value}),
        "browser.get.styles" if model_result.get("styles").is_some() => {
            json!({"styles": value.clone(), "value": value})
        }
        _ => json!({"value": value}),
    }
}

fn rpc_result_response(id: Value, result: Result<Value, AppError>) -> Value {
    match result {
        Ok(result) => json!({"id": id, "ok": true, "result": result}),
        Err(err) => json!({
            "id": id,
            "ok": false,
            "error": {"code": err.code, "message": err.message}
        }),
    }
}

fn dispatch_waiting_feed_push_request(
    id: Value,
    params: Value,
    state: &Arc<Mutex<AppState>>,
    wait_timeout: f64,
) -> Value {
    let item_id = match state.lock() {
        Ok(mut app) => match app.handle("feed.push", &params) {
            Ok(result) => {
                app.record_feed_push_received_events(&params);
                match result.get("item_id").and_then(Value::as_str) {
                    Some(item_id) if !item_id.is_empty() => item_id.to_string(),
                    _ => {
                        return json!({
                            "id": id,
                            "ok": false,
                            "error": {
                                "code": "internal_error",
                                "message": "feed.push did not return item_id"
                            }
                        });
                    }
                }
            }
            Err(err) => {
                return json!({
                    "id": id,
                    "ok": false,
                    "error": {"code": err.code, "message": err.message}
                });
            }
        },
        Err(_) => {
            return json!({
                "id": id,
                "ok": false,
                "error": {"code": "internal_error", "message": "app state lock poisoned"}
            });
        }
    };

    let deadline = Instant::now() + Duration::from_secs_f64(wait_timeout);
    loop {
        match state.lock() {
            Ok(mut app) => {
                if let Some(result) = app.feed_completed_wait_result(&item_id) {
                    app.record_feed_push_completed_event(&params, &result);
                    return json!({"id": id, "ok": true, "result": result});
                }
            }
            Err(_) => {
                return json!({
                    "id": id,
                    "ok": false,
                    "error": {"code": "internal_error", "message": "app state lock poisoned"}
                });
            }
        }

        let now = Instant::now();
        if now >= deadline {
            break;
        }
        let remaining = deadline.saturating_duration_since(now);
        thread::sleep(remaining.min(Duration::from_millis(25)));
    }

    match state.lock() {
        Ok(mut app) => {
            let result = app.feed_timeout_wait_result(&item_id);
            app.record_feed_push_completed_event(&params, &result);
            json!({"id": id, "ok": true, "result": result})
        }
        Err(_) => json!({
            "id": id,
            "ok": false,
            "error": {"code": "internal_error", "message": "app state lock poisoned"}
        }),
    }
}

fn handle_events_stream_request(
    request: &Value,
    state: &Arc<Mutex<AppState>>,
    writer: &mut UnixStream,
) -> Result<()> {
    let params = request.get("params").unwrap_or(&Value::Null);
    let after_sequence = stream_i64_param(params.get("after_seq").or_else(|| params.get("after")));
    let names = stream_string_list(params.get("names").or_else(|| params.get("name")));
    let categories =
        stream_string_list(params.get("categories").or_else(|| params.get("category")));
    let include_heartbeats = stream_bool_param(
        params
            .get("include_heartbeats")
            .or_else(|| params.get("include_heartbeat")),
    )
    .unwrap_or(true);

    let (ack, replay) = match state.lock() {
        Ok(app) => app.event_stream_snapshot(after_sequence, &names, &categories),
        Err(_) => {
            let error = json!({
                "type": "error",
                "ok": false,
                "error": {"code": "internal_error", "message": "app state lock poisoned"}
            });
            write_stream_frame(writer, &error)?;
            return Ok(());
        }
    };
    write_stream_frame(writer, &ack)?;
    for event in &replay {
        write_stream_frame(writer, event)?;
    }
    writer.flush().context("failed to flush event stream")?;

    let mut last_sequence = replay
        .iter()
        .filter_map(|event| event.get("seq").and_then(Value::as_i64))
        .max()
        .or_else(|| {
            ack.get("resume")
                .and_then(|resume| resume.get("latest_seq"))
                .and_then(Value::as_i64)
        })
        .unwrap_or(0);
    let subscription_id = ack
        .get("subscription_id")
        .and_then(Value::as_str)
        .unwrap_or("events")
        .to_string();
    let heartbeat_interval = Duration::from_secs(1);
    let mut next_heartbeat = Instant::now() + heartbeat_interval;

    loop {
        thread::sleep(Duration::from_millis(50));
        let events = match state.lock() {
            Ok(app) => app.event_stream_events_after(last_sequence, &names, &categories),
            Err(_) => Vec::new(),
        };
        for event in events {
            if let Some(seq) = event.get("seq").and_then(Value::as_i64) {
                last_sequence = last_sequence.max(seq);
            }
            if write_stream_frame(writer, &event).is_err() {
                return Ok(());
            }
        }
        if include_heartbeats && Instant::now() >= next_heartbeat {
            let heartbeat = match state.lock() {
                Ok(app) => app.event_heartbeat(&subscription_id),
                Err(_) => json!({
                    "type": "error",
                    "ok": false,
                    "error": {"code": "internal_error", "message": "app state lock poisoned"}
                }),
            };
            if write_stream_frame(writer, &heartbeat).is_err() {
                return Ok(());
            }
            next_heartbeat = Instant::now() + heartbeat_interval;
        }
        writer.flush().context("failed to flush event stream")?;
    }
}

fn write_stream_frame(writer: &mut UnixStream, frame: &Value) -> Result<()> {
    serde_json::to_writer(&mut *writer, frame).context("failed to write event stream frame")?;
    writer
        .write_all(b"\n")
        .context("failed to write event stream newline")?;
    Ok(())
}

fn stream_i64_param(value: Option<&Value>) -> Option<i64> {
    match value? {
        Value::Number(number) => number.as_i64(),
        Value::String(text) => text.trim().parse::<i64>().ok(),
        _ => None,
    }
}

fn stream_bool_param(value: Option<&Value>) -> Option<bool> {
    match value? {
        Value::Bool(value) => Some(*value),
        Value::Number(number) => number.as_i64().and_then(|value| match value {
            0 => Some(false),
            1 => Some(true),
            _ => None,
        }),
        Value::String(text) => match text.trim().to_ascii_lowercase().as_str() {
            "1" | "true" | "yes" | "on" => Some(true),
            "0" | "false" | "no" | "off" => Some(false),
            _ => None,
        },
        _ => None,
    }
}

fn stream_string_list(value: Option<&Value>) -> Vec<String> {
    match value {
        Some(Value::String(text)) => vec![text.trim().to_string()],
        Some(Value::Array(items)) => items
            .iter()
            .filter_map(Value::as_str)
            .map(str::trim)
            .filter(|text| !text.is_empty())
            .map(ToString::to_string)
            .collect(),
        _ => Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn debug_log_path_uses_cmux_state_logs_directory() {
        let state = Path::new("/run/user/1000/cmux");

        assert_eq!(
            debug_log_path_for_socket_in_state_dir("/tmp/cmux-debug-abcd.sock", state),
            state.join("logs/cmux-debug-abcd.log")
        );
        assert_eq!(
            debug_log_path_for_socket_in_state_dir("/run/user/1000/cmux/cmux.sock", state),
            state.join("logs/cmux-debug.log")
        );
    }

    #[test]
    fn state_dir_prefers_xdg_then_home_then_temp() {
        assert_eq!(
            state_dir_from_env(
                Some("/run/user/1000/state"),
                Some("/home/me"),
                Path::new("/tmp")
            ),
            PathBuf::from("/run/user/1000/state/cmux")
        );
        assert_eq!(
            state_dir_from_env(None, Some("/home/me"), Path::new("/tmp")),
            PathBuf::from("/home/me/.local/state/cmux")
        );
        assert_eq!(
            state_dir_from_env(None, None, Path::new("/tmp/codex")),
            PathBuf::from("/tmp/codex/cmux")
        );
    }

    #[test]
    fn socket_marker_lives_in_cmux_state_directory() {
        assert_eq!(
            socket_marker_path_in_state_dir(Path::new("/run/user/1000/state/cmux")),
            PathBuf::from("/run/user/1000/state/cmux/last-socket-path")
        );
    }

    #[test]
    fn socket_bind_error_context_includes_parent_directory() {
        assert_eq!(
            socket_bind_error_context(Path::new("/run/user/1000/state/cmux/cmux.sock")),
            "failed to bind socket /run/user/1000/state/cmux/cmux.sock (parent: /run/user/1000/state/cmux)"
        );
    }

    #[test]
    fn bind_socket_preserves_a_live_listener() {
        let temp = tempfile::tempdir().expect("socket tempdir");
        let path = temp.path().join("cmux.sock");
        let original = UnixListener::bind(&path).expect("original listener");

        let error = bind_socket(path.to_str().unwrap()).expect_err("live socket rejected");

        assert!(error.to_string().contains("already active"));
        assert!(path.exists());
        let _client = UnixStream::connect(&path).expect("original listener remains reachable");
        let _ = original.accept().expect("accept bind probe");
        let _ = original.accept().expect("accept reachability probe");
    }

    #[test]
    fn bind_socket_replaces_a_confirmed_stale_socket() {
        let temp = tempfile::tempdir().expect("socket tempdir");
        let path = temp.path().join("cmux.sock");
        drop(UnixListener::bind(&path).expect("stale listener"));

        let replacement = bind_socket(path.to_str().unwrap()).expect("replacement listener");

        let _client = UnixStream::connect(&path).expect("replacement listener reachable");
        let _ = replacement.accept().expect("accept replacement connection");
    }

    #[test]
    fn bind_socket_does_not_delete_a_non_socket_path() {
        let temp = tempfile::tempdir().expect("socket tempdir");
        let path = temp.path().join("cmux.sock");
        fs::write(&path, "keep me").expect("control-path fixture");

        let error = bind_socket(path.to_str().unwrap()).expect_err("regular file rejected");

        assert!(error.to_string().contains("non-socket control path"));
        assert_eq!(fs::read_to_string(path).unwrap(), "keep me");
    }

    #[test]
    fn live_browser_reads_preserve_socket_result_shapes() {
        for method in [
            "browser.eval",
            "browser.evalhandle",
            "browser.snapshot",
            "browser.content",
            "browser.innertext",
            "browser.get.text",
            "browser.get.html",
            "browser.get.value",
            "browser.get.attr",
            "browser.get.title",
            "browser.get.count",
            "browser.get.box",
            "browser.get.styles",
            "browser.is.visible",
            "browser.is.enabled",
            "browser.is.checked",
        ] {
            assert!(is_live_browser_read_method(method), "method was {method}");
        }
        assert!(!is_live_browser_read_method("browser.click"));

        assert_eq!(
            live_browser_read_result(
                "browser.content",
                &json!({"html": "model", "content": "model", "text": "model", "inner_text": "model", "title": "Model", "url": "about:blank"}),
                json!({"html": "<html>live</html>", "content": "<html>live</html>", "text": "live", "inner_text": "live", "title": "Live", "url": "https://example.test/"})
            ),
            json!({"html": "<html>live</html>", "content": "<html>live</html>", "text": "live", "inner_text": "live", "title": "Live", "url": "https://example.test/"})
        );
        assert_eq!(
            live_browser_read_result("browser.innertext", &json!({}), json!("Live text")),
            json!({"value": "Live text", "text": "Live text", "inner_text": "Live text"})
        );
        assert_eq!(
            live_browser_read_result("browser.get.title", &json!({}), json!("Live title")),
            json!({"title": "Live title", "value": "Live title"})
        );
        assert_eq!(
            live_browser_read_result("browser.get.count", &json!({}), json!(3)),
            json!({"count": 3, "value": 3})
        );
        assert_eq!(
            live_browser_read_result(
                "browser.get.styles",
                &json!({"styles": {"display": "block"}}),
                json!({"display": "grid"})
            ),
            json!({"styles": {"display": "grid"}, "value": {"display": "grid"}})
        );
        assert_eq!(
            live_browser_read_result(
                "browser.get.styles",
                &json!({"value": "block"}),
                json!("grid")
            ),
            json!({"value": "grid"})
        );
        assert_eq!(
            live_browser_read_result("browser.is.visible", &json!({}), json!(true)),
            json!({"value": true})
        );
    }

    #[test]
    fn live_browser_screenshot_preserves_metadata_and_uses_native_png() {
        let result = live_browser_screenshot_result(
            json!({"device_scale_factor": 1.5}),
            browser_runtime::BrowserScreenshot {
                png: b"native-png".to_vec(),
                width: 640,
                height: 960,
            },
        );

        assert_eq!(result["width"], 640);
        assert_eq!(result["height"], 960);
        assert_eq!(result["device_scale_factor"], 1.5);
        assert_eq!(
            result["png_base64"],
            base64::engine::general_purpose::STANDARD.encode(b"native-png")
        );
        assert!(!browser_screenshot_full_document(&json!({})));
        assert!(browser_screenshot_full_document(
            &json!({"full_page": true})
        ));
        assert!(browser_screenshot_full_document(&json!({"fullPage": true})));
    }

    #[test]
    fn live_browser_pdf_preserves_metadata_and_uses_native_bytes() {
        let native = b"%PDF-1.7\n1 0 obj << /Type /Pages >>\n2 0 obj << /Type /Page >>\n3 0 obj << /Type/Page >>\n";
        let temp = tempfile::tempdir().expect("PDF tempdir");
        let output_path = temp.path().join("native/browser.pdf");
        let output_path = output_path.to_string_lossy().to_string();
        let result = live_browser_pdf_result(
            json!({
                "title": "Native PDF",
                "pdf_base64": base64::engine::general_purpose::STANDARD.encode(b"%PDF-model"),
                "page_count": 1
            }),
            Some(browser_runtime::BrowserPdf {
                bytes: native.to_vec(),
            }),
            Some(&output_path),
        )
        .expect("native PDF result");

        assert_eq!(result["title"], "Native PDF");
        assert_eq!(result["bytes"], native.len());
        assert_eq!(result["page_count"], 2);
        assert_eq!(result["path"], output_path);
        assert_eq!(fs::read(&output_path).unwrap(), native);
        assert_eq!(
            base64::engine::general_purpose::STANDARD
                .decode(result["pdf_base64"].as_str().unwrap())
                .unwrap(),
            native
        );
    }
}
