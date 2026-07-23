#[path = "../src/browser_environment.rs"]
mod browser_environment;
#[allow(dead_code)]
#[path = "../src/browser_runtime.rs"]
mod browser_runtime;
#[allow(dead_code)]
#[path = "../src/gtk_webkit.rs"]
mod gtk_webkit;

use browser_environment::{BrowserEnvironmentState, BrowserGeolocationState};
use gtk4::glib;
use gtk4::prelude::*;
use std::cell::{Cell, RefCell};
use std::collections::BTreeMap;
use std::io::{ErrorKind, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::rc::Rc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

const FIRST_USER_AGENT: &str = "cmux-runtime-smoke/1.0";
const SECOND_USER_AGENT: &str = "cmux-runtime-smoke/2.0";
const REQUEST_HEADER_NAME: &str = "X-Cmux-Native";
const REQUEST_HEADER_VALUE: &str = "header-value";
const AUTHORIZATION_VALUE: &str = "Basic cnVudGltZS1hZ2VudDpydW50aW1lLXNlY3JldA==";

fn main() {
    if let Err(err) = run() {
        eprintln!("webkit runtime smoke failed: {err}");
        std::process::exit(1);
    }
    println!("webkit runtime smoke passed");
}

fn run() -> Result<(), String> {
    gtk_webkit::configure_environment();
    gtk4::init().map_err(|err| format!("initialize GTK: {err}"))?;
    let server = TestServer::start()?;
    let view = gtk_webkit::GtkWebKitView::new("webkit-runtime-smoke-offline", 0)?;
    let online_view = gtk_webkit::GtkWebKitView::new("webkit-runtime-smoke-online", 0)?;
    view.set_request_configuration(
        &[
            (
                REQUEST_HEADER_NAME.to_string(),
                REQUEST_HEADER_VALUE.to_string(),
            ),
            ("Authorization".to_string(), AUTHORIZATION_VALUE.to_string()),
        ],
        Some(("runtime-agent", "runtime-secret")),
    )?;
    view.set_offline(true)?;
    view.set_user_agent(FIRST_USER_AGENT)?;
    let first_environment = runtime_environment(true);
    let first_environment_script = first_environment.bootstrap_script()?;
    let user_init_script = "window.__cmuxRuntimeInit = 'ready';".to_string();
    view.replace_init_scripts(&[first_environment_script, user_init_script.clone()])?;
    let upload_dir = tempfile::tempdir().map_err(|err| format!("create upload fixture: {err}"))?;
    let first_upload = upload_dir.path().join("first upload.txt");
    let second_upload = upload_dir.path().join("second upload.txt");
    std::fs::write(&first_upload, "first payload")
        .map_err(|err| format!("write first upload fixture: {err}"))?;
    std::fs::write(&second_upload, "second payload")
        .map_err(|err| format!("write second upload fixture: {err}"))?;
    let upload_files = vec![
        first_upload.to_string_lossy().into_owned(),
        second_upload.to_string_lossy().into_owned(),
    ];

    let window = gtk4::Window::new();
    window.set_default_size(640, 360);
    let views = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
    views.append(view.widget());
    views.append(online_view.widget());
    window.set_child(Some(&views));
    window.present();

    let main_loop = glib::MainLoop::new(None, false);
    let outcome = Rc::new(RefCell::new(None::<Result<(), String>>));
    let stage = Rc::new(Cell::new(0_u8));
    let evaluation_started = Rc::new(Cell::new(false));
    let offline_started = Instant::now();
    let blocked_request_count = Arc::clone(&server.blocked_requests);
    let configured_document_count = Arc::clone(&server.configured_document_requests);
    let configured_subresource_count = Arc::clone(&server.configured_subresource_requests);
    let authenticated_challenge_count = Arc::clone(&server.authenticated_challenges);
    let first_url = server.url("first");
    let second_url = server.url("second");
    online_view
        .load_uri(&server.url("isolation"))
        .then_some(())
        .ok_or_else(|| "online isolation URL was rejected".to_string())?;
    view.load_uri(&server.url("blocked"))
        .then_some(())
        .ok_or_else(|| "offline document URL was rejected".to_string())?;

    let view_for_poll = view.clone();
    let loop_for_poll = main_loop.clone();
    let outcome_for_poll = Rc::clone(&outcome);
    let stage_for_poll = Rc::clone(&stage);
    let evaluation_started_for_poll = Rc::clone(&evaluation_started);
    let online_view_for_poll = online_view.clone();
    let poll_source = glib::timeout_add_local(Duration::from_millis(50), move || {
        let current_stage = stage_for_poll.get();
        if current_stage == 0 {
            if blocked_request_count.load(Ordering::Relaxed) != 0 {
                complete(
                    &outcome_for_poll,
                    &loop_for_poll,
                    Err("offline WebKit request reached the origin server".to_string()),
                );
                return glib::ControlFlow::Continue;
            }
            if offline_started.elapsed() < Duration::from_millis(500) {
                return glib::ControlFlow::Continue;
            }
            if online_view_for_poll.title().as_deref() != Some("isolation-online") {
                if offline_started.elapsed() >= Duration::from_secs(3) {
                    complete(
                        &outcome_for_poll,
                        &loop_for_poll,
                        Err(
                            "a separate online WebKit view did not load while its peer was offline"
                                .to_string(),
                        ),
                    );
                }
                return glib::ControlFlow::Continue;
            }
            if view_for_poll.is_loading() {
                if offline_started.elapsed() >= Duration::from_secs(3) {
                    complete(
                        &outcome_for_poll,
                        &loop_for_poll,
                        Err("offline WebKit request did not fail within three seconds".to_string()),
                    );
                }
                return glib::ControlFlow::Continue;
            }
            if let Err(err) = view_for_poll.set_offline(false) {
                complete(&outcome_for_poll, &loop_for_poll, Err(err));
                return glib::ControlFlow::Continue;
            }
            stage_for_poll.set(1);
            if !view_for_poll.load_uri(&first_url) {
                complete(
                    &outcome_for_poll,
                    &loop_for_poll,
                    Err("online document URL was rejected".to_string()),
                );
            }
            return glib::ControlFlow::Continue;
        }
        let expected = match current_stage {
            1 => environment_title("page", FIRST_USER_AGENT, 1, true, ""),
            2 => environment_title("updated", FIRST_USER_AGENT, 1, false, ""),
            3 => "storage|one|two".to_string(),
            4 => {
                "upload|first upload.txt:first payload|second upload.txt:second payload".to_string()
            }
            5 => environment_title("page", SECOND_USER_AGENT, 2, false, "|one|two"),
            _ => "auth-accepted".to_string(),
        };
        if view_for_poll.title().as_deref() != Some(expected.as_str()) {
            return glib::ControlFlow::Continue;
        }

        if current_stage == 1 {
            let second_environment_script = match runtime_environment(false).bootstrap_script() {
                Ok(script) => script,
                Err(err) => {
                    complete(&outcome_for_poll, &loop_for_poll, Err(err));
                    return glib::ControlFlow::Continue;
                }
            };
            if let Err(err) = view_for_poll.replace_init_scripts(&[
                second_environment_script.clone(),
                user_init_script.clone(),
            ]) {
                complete(&outcome_for_poll, &loop_for_poll, Err(err));
                return glib::ControlFlow::Continue;
            }
            view_for_poll.evaluate_javascript(&second_environment_script);
            view_for_poll.evaluate_javascript("window.__cmuxEnvironmentTitle('updated');");
            stage_for_poll.set(2);
        } else if current_stage == 2 {
            let local = BTreeMap::from([("alpha".to_string(), "one".to_string())]);
            let session = BTreeMap::from([("beta".to_string(), "two".to_string())]);
            if let Err(err) = view_for_poll.replace_storage(&local, &session) {
                complete(&outcome_for_poll, &loop_for_poll, Err(err));
                return glib::ControlFlow::Continue;
            }
            view_for_poll.evaluate_javascript(
                "document.title = 'storage|' + localStorage.getItem('alpha') + '|' + sessionStorage.getItem('beta');",
            );
            stage_for_poll.set(3);
        } else if current_stage == 3 {
            view_for_poll.evaluate_javascript(
                r#"(function() {
                    var input = document.createElement('input');
                    input.id = 'cmux-native-upload';
                    input.type = 'file';
                    input.multiple = true;
                    input.addEventListener('change', async function() {
                        var values = await Promise.all(Array.from(input.files).map(async function(file) {
                            return file.name + ':' + await file.text();
                        }));
                        document.title = 'upload|' + values.join('|');
                    });
                    document.body.appendChild(input);
                })();"#,
            );
            if let Err(err) = view_for_poll.prepare_file_selection(&upload_files) {
                complete(&outcome_for_poll, &loop_for_poll, Err(err));
                return glib::ControlFlow::Continue;
            }
            view_for_poll
                .evaluate_javascript("document.querySelector('#cmux-native-upload').click();");
            stage_for_poll.set(4);
        } else if current_stage == 4 {
            if let Err(err) = view_for_poll.set_user_agent(SECOND_USER_AGENT) {
                complete(&outcome_for_poll, &loop_for_poll, Err(err));
                return glib::ControlFlow::Continue;
            }
            stage_for_poll.set(5);
            if !view_for_poll.load_uri(&second_url) {
                complete(
                    &outcome_for_poll,
                    &loop_for_poll,
                    Err("second document URL was rejected".to_string()),
                );
            }
        } else if current_stage == 5 {
            if configured_document_count.load(Ordering::Relaxed) < 2
                || configured_subresource_count.load(Ordering::Relaxed) < 2
            {
                complete(
                    &outcome_for_poll,
                    &loop_for_poll,
                    Err(format!(
                        "native request configuration was not applied to every resource: {} documents, {} subresources",
                        configured_document_count.load(Ordering::Relaxed),
                        configured_subresource_count.load(Ordering::Relaxed)
                    )),
                );
                return glib::ControlFlow::Continue;
            }
            if let Err(err) = view_for_poll.set_request_configuration(
                &[(
                    REQUEST_HEADER_NAME.to_string(),
                    REQUEST_HEADER_VALUE.to_string(),
                )],
                Some(("runtime-agent", "runtime-secret")),
            ) {
                complete(&outcome_for_poll, &loop_for_poll, Err(err));
                return glib::ControlFlow::Continue;
            }
            stage_for_poll.set(6);
            if !view_for_poll.load_uri(&first_url.replace("/first", "/auth")) {
                complete(
                    &outcome_for_poll,
                    &loop_for_poll,
                    Err("authentication challenge URL was rejected".to_string()),
                );
            }
        } else {
            if authenticated_challenge_count.load(Ordering::Relaxed) == 0 {
                complete(
                    &outcome_for_poll,
                    &loop_for_poll,
                    Err("native HTTP authentication challenge was not satisfied".to_string()),
                );
                return glib::ControlFlow::Continue;
            }
            if evaluation_started_for_poll.replace(true) {
                return glib::ControlFlow::Continue;
            }
            let script = browser_runtime::evaluation_envelope_script(
                "window.__cmuxEvalCount = (window.__cmuxEvalCount || 0) + 1; Promise.resolve({answer: 42, count: window.__cmuxEvalCount, title: document.title})",
            );
            let outcome_for_evaluation = Rc::clone(&outcome_for_poll);
            let loop_for_evaluation = loop_for_poll.clone();
            let view_for_evaluation = view_for_poll.clone();
            let expected_title = expected.clone();
            if let Err(err) =
                view_for_poll.evaluate_javascript_with_result(&script, move |result| {
                    let result = result
                        .and_then(|result| browser_runtime::decode_evaluation_envelope(&result))
                        .and_then(|value| {
                            if value["answer"] != 42
                                || value["count"] != 1
                                || value["title"] != expected_title
                            {
                                return Err(format!("unexpected live evaluation result: {value}"));
                            }
                            Ok(())
                        });
                    if let Err(err) = result {
                        complete(&outcome_for_evaluation, &loop_for_evaluation, Err(err));
                        return;
                    }
                    let read_script = browser_runtime::live_browser_read_script(
                        "browser.get.value",
                        &serde_json::json!({}),
                        Some("#cmux-live-read"),
                    )
                    .expect("live read script");
                    let snapshot_script = browser_runtime::live_browser_read_script(
                        "browser.snapshot",
                        &serde_json::json!({}),
                        None,
                    )
                    .expect("live snapshot script");
                    let content_script = browser_runtime::live_browser_read_script(
                        "browser.content",
                        &serde_json::json!({}),
                        None,
                    )
                    .expect("live content script");
                    let inner_text_script = browser_runtime::live_browser_read_script(
                        "browser.innertext",
                        &serde_json::json!({}),
                        Some("#cmux-live-read-label"),
                    )
                    .expect("live inner text script");
                    let read_script = browser_runtime::evaluation_envelope_script(&format!(
                        "(function() {{ var input = document.createElement('input'); input.id = 'cmux-live-read'; input.value = 'native-only'; document.body.appendChild(input); var label = document.createElement('div'); label.id = 'cmux-live-read-label'; label.textContent = 'live content text'; document.body.appendChild(label); return {{ value: ({}), snapshot: ({}), content: ({}), innerText: ({}) }}; }})()",
                        read_script, snapshot_script, content_script, inner_text_script
                    ));
                    let outcome_for_read = Rc::clone(&outcome_for_evaluation);
                    let loop_for_read = loop_for_evaluation.clone();
                    let view_for_snapshot = view_for_evaluation.clone();
                    if let Err(err) = view_for_evaluation.evaluate_javascript_with_result(
                        &read_script,
                        move |result| {
                            let result = result
                                .and_then(|result| browser_runtime::decode_evaluation_envelope(&result))
                                .and_then(|value| {
                                    let snapshot = &value["snapshot"];
                                    let has_live_ref = snapshot["refs"]
                                        .as_object()
                                        .is_some_and(|refs| {
                                            refs.values().any(|selector| selector == "#cmux-live-read")
                                        });
                                    (value["value"] == "native-only"
                                        && snapshot["snapshot"]
                                            .as_str()
                                            .is_some_and(|text| text.contains("native-only"))
                                        && value["content"]["html"]
                                            .as_str()
                                            .is_some_and(|html| html.contains("cmux-live-read-label"))
                                        && value["content"]["text"]
                                            .as_str()
                                            .is_some_and(|text| text.contains("live content text"))
                                        && value["innerText"] == "live content text"
                                        && has_live_ref)
                                        .then_some(())
                                        .ok_or_else(|| format!("unexpected live read/snapshot value: {value}"))
                                });
                            if let Err(err) = result {
                                complete(&outcome_for_read, &loop_for_read, Err(err));
                                return;
                            }
                            let outcome_for_snapshot = Rc::clone(&outcome_for_read);
                            let loop_for_snapshot = loop_for_read.clone();
                            let view_for_pdf = view_for_snapshot.clone();
                            view_for_snapshot.capture_snapshot(false, move |result| {
                                let result = result.and_then(|snapshot| {
                                    (snapshot.width > 0
                                        && snapshot.height > 0
                                        && snapshot.png.starts_with(b"\x89PNG\r\n\x1a\n"))
                                    .then_some(())
                                    .ok_or_else(|| {
                                        format!(
                                            "unexpected native screenshot: {}x{}, {} bytes",
                                            snapshot.width,
                                            snapshot.height,
                                            snapshot.png.len()
                                        )
                                    })
                                });
                                if let Err(err) = result {
                                    complete(&outcome_for_snapshot, &loop_for_snapshot, Err(err));
                                    return;
                                }
                                let outcome_for_pdf = Rc::clone(&outcome_for_snapshot);
                                let loop_for_pdf = loop_for_snapshot.clone();
                                if let Err(err) = view_for_pdf.print_to_pdf(move |result| {
                                    let result = result.and_then(|pdf| {
                                        (pdf.bytes.starts_with(b"%PDF-") && pdf.bytes.len() > 500)
                                            .then_some(())
                                            .ok_or_else(|| {
                                                format!(
                                                    "unexpected native PDF: {} bytes",
                                                    pdf.bytes.len()
                                                )
                                            })
                                    });
                                    complete(&outcome_for_pdf, &loop_for_pdf, result);
                                }) {
                                    complete(&outcome_for_snapshot, &loop_for_snapshot, Err(err));
                                }
                            });
                        },
                    ) {
                        complete(&outcome_for_evaluation, &loop_for_evaluation, Err(err));
                    }
                })
            {
                complete(&outcome_for_poll, &loop_for_poll, Err(err));
            }
        }
        glib::ControlFlow::Continue
    });

    let loop_for_timeout = main_loop.clone();
    let outcome_for_timeout = Rc::clone(&outcome);
    let timeout_source = glib::timeout_add_local_once(Duration::from_secs(15), move || {
        complete(
            &outcome_for_timeout,
            &loop_for_timeout,
            Err("WebKit runtime smoke timed out".to_string()),
        );
    });
    main_loop.run();
    poll_source.remove();
    timeout_source.remove();
    let result = outcome
        .borrow_mut()
        .take()
        .ok_or_else(|| "runtime smoke produced no outcome".to_string())?;
    window.close();
    while glib::MainContext::default().pending() {
        glib::MainContext::default().iteration(false);
    }
    result
}

fn runtime_environment(first: bool) -> BrowserEnvironmentState {
    BrowserEnvironmentState {
        locale: if first { "nl-NL" } else { "en-GB" }.to_string(),
        timezone: if first { "Europe/Amsterdam" } else { "UTC" }.to_string(),
        media_type: if first { "print" } else { "screen" }.to_string(),
        color_scheme: if first { "dark" } else { "light" }.to_string(),
        reduced_motion: if first { "reduce" } else { "no-preference" }.to_string(),
        offline: first,
        geolocation: Some(BrowserGeolocationState {
            latitude: if first { 52.37 } else { 51.50 },
            longitude: if first { 4.90 } else { -0.12 },
            accuracy: if first { 8.0 } else { 4.0 },
        }),
        mobile: first,
        touch: first,
        device_scale_factor: if first { 2.0 } else { 1.0 },
        permissions: BTreeMap::from([(
            "geolocation".to_string(),
            if first { "granted" } else { "denied" }.to_string(),
        )]),
    }
}

fn environment_title(label: &str, user_agent: &str, page: u8, first: bool, suffix: &str) -> String {
    let values = if first {
        "nl-NL|Europe/Amsterdam|false|true|true|false|true|false|false|true|2|52.37|granted"
    } else {
        "en-GB|UTC|true|false|false|true|false|true|true|false|1|51.50|denied"
    };
    format!("{label}|{user_agent}|ready|{page}|{values}{suffix}")
}

struct TestServer {
    address: String,
    blocked_requests: Arc<AtomicU64>,
    configured_document_requests: Arc<AtomicU64>,
    configured_subresource_requests: Arc<AtomicU64>,
    authenticated_challenges: Arc<AtomicU64>,
    stop: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
}

impl TestServer {
    fn start() -> Result<Self, String> {
        let listener =
            TcpListener::bind("127.0.0.1:0").map_err(|err| format!("bind test server: {err}"))?;
        listener
            .set_nonblocking(true)
            .map_err(|err| format!("configure test server: {err}"))?;
        let address = listener
            .local_addr()
            .map_err(|err| format!("read test server address: {err}"))?
            .to_string();
        let stop = Arc::new(AtomicBool::new(false));
        let blocked_requests = Arc::new(AtomicU64::new(0));
        let configured_document_requests = Arc::new(AtomicU64::new(0));
        let configured_subresource_requests = Arc::new(AtomicU64::new(0));
        let authenticated_challenges = Arc::new(AtomicU64::new(0));
        let stop_for_thread = Arc::clone(&stop);
        let blocked_requests_for_thread = Arc::clone(&blocked_requests);
        let configured_document_requests_for_thread = Arc::clone(&configured_document_requests);
        let configured_subresource_requests_for_thread =
            Arc::clone(&configured_subresource_requests);
        let authenticated_challenges_for_thread = Arc::clone(&authenticated_challenges);
        let thread = thread::spawn(move || {
            while !stop_for_thread.load(Ordering::Relaxed) {
                match listener.accept() {
                    Ok((mut stream, _)) => {
                        serve_document(
                            &mut stream,
                            &blocked_requests_for_thread,
                            &configured_document_requests_for_thread,
                            &configured_subresource_requests_for_thread,
                            &authenticated_challenges_for_thread,
                        );
                    }
                    Err(err) if err.kind() == ErrorKind::WouldBlock => {
                        thread::sleep(Duration::from_millis(10));
                    }
                    Err(_) => break,
                }
            }
        });
        Ok(Self {
            address,
            blocked_requests,
            configured_document_requests,
            configured_subresource_requests,
            authenticated_challenges,
            stop,
            thread: Some(thread),
        })
    }

    fn url(&self, page: &str) -> String {
        format!("http://{}/{page}", self.address)
    }
}

impl Drop for TestServer {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

fn serve_document(
    stream: &mut TcpStream,
    blocked_requests: &AtomicU64,
    configured_document_requests: &AtomicU64,
    configured_subresource_requests: &AtomicU64,
    authenticated_challenges: &AtomicU64,
) {
    let _ = stream.set_read_timeout(Some(Duration::from_secs(1)));
    let mut request = [0_u8; 4096];
    let bytes = stream.read(&mut request).unwrap_or(0);
    let request = String::from_utf8_lossy(&request[..bytes]);
    if request.starts_with("GET /blocked ") {
        blocked_requests.fetch_add(1, Ordering::Relaxed);
    }
    if request.starts_with("GET /isolation ") {
        let body = "<!doctype html><meta charset='utf-8'><title>isolation-online</title>";
        let response = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
            body.len(),
            body
        );
        let _ = stream.write_all(response.as_bytes());
        return;
    }
    let configured = request_header(&request, REQUEST_HEADER_NAME) == Some(REQUEST_HEADER_VALUE)
        && request_header(&request, "Authorization") == Some(AUTHORIZATION_VALUE);
    if request.starts_with("GET /header-subresource?") {
        if configured {
            configured_subresource_requests.fetch_add(1, Ordering::Relaxed);
        }
        let response =
            "HTTP/1.1 204 No Content\r\nCache-Control: no-store\r\nConnection: close\r\n\r\n";
        let _ = stream.write_all(response.as_bytes());
        return;
    }
    if request.starts_with("GET /auth ") {
        if request_header(&request, REQUEST_HEADER_NAME) == Some(REQUEST_HEADER_VALUE)
            && request_header(&request, "Authorization") == Some(AUTHORIZATION_VALUE)
        {
            authenticated_challenges.fetch_add(1, Ordering::Relaxed);
            let body = "<!doctype html><meta charset='utf-8'><title>auth-accepted</title>";
            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                body.len(), body
            );
            let _ = stream.write_all(response.as_bytes());
        } else {
            let response = "HTTP/1.1 401 Unauthorized\r\nWWW-Authenticate: Basic realm=\"cmux-runtime\"\r\nContent-Length: 0\r\nConnection: close\r\n\r\n";
            let _ = stream.write_all(response.as_bytes());
        }
        return;
    }
    let second = request.starts_with("GET /second ");
    if (second || request.starts_with("GET /first ")) && configured {
        configured_document_requests.fetch_add(1, Ordering::Relaxed);
    }
    let storage_suffix = if second {
        " + '|' + localStorage.getItem('alpha') + '|' + sessionStorage.getItem('beta')"
    } else {
        ""
    };
    let stage = if second { 2 } else { 1 };
    let body = format!(
        r#"<!doctype html><meta charset='utf-8'><title>pending</title><script>
        window.__cmuxEnvironmentTitle = async function(label) {{
            var permission = await navigator.permissions.query({{name: 'geolocation'}});
            var position = await new Promise(function(resolve, reject) {{
                navigator.geolocation.getCurrentPosition(resolve, reject);
            }});
            document.title = [
                label,
                navigator.userAgent,
                String(window.__cmuxRuntimeInit),
                '{stage}',
                navigator.language,
                Intl.DateTimeFormat().resolvedOptions().timeZone,
                String(matchMedia('screen').matches),
                String(matchMedia('print').matches),
                String(matchMedia('(prefers-color-scheme: dark)').matches),
                String(matchMedia('(prefers-color-scheme: light)').matches),
                String(matchMedia('(prefers-reduced-motion: reduce)').matches),
                String(matchMedia('(prefers-reduced-motion: no-preference)').matches),
                String(navigator.onLine),
                String(navigator.maxTouchPoints > 0),
                String(window.devicePixelRatio),
                Number(position.coords.latitude).toFixed(2),
                permission.state
            ].join('|'){storage_suffix};
        }};
        window.__cmuxEnvironmentTitle('page');
        </script><body>cmux runtime smoke<img src='/header-subresource?stage={stage}'></body>"#
    );
    let response = format!(
        "HTTP/1.1 200 OK\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
        body.len(),
        body
    );
    let _ = stream.write_all(response.as_bytes());
}

fn request_header<'a>(request: &'a str, name: &str) -> Option<&'a str> {
    request.lines().skip(1).find_map(|line| {
        let (candidate, value) = line.split_once(':')?;
        candidate.eq_ignore_ascii_case(name).then(|| value.trim())
    })
}

fn complete(
    outcome: &Rc<RefCell<Option<Result<(), String>>>>,
    main_loop: &glib::MainLoop,
    result: Result<(), String>,
) {
    if outcome.borrow().is_none() {
        *outcome.borrow_mut() = Some(result);
        main_loop.quit();
    }
}
