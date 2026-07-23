use serde_json::Value;
use std::collections::VecDeque;
use std::sync::{mpsc, Mutex, OnceLock};
use std::thread::ThreadId;
use std::time::{Duration, Instant};

const BROWSER_EVALUATION_TIMEOUT: Duration = Duration::from_secs(5);
const BROWSER_PDF_TIMEOUT: Duration = Duration::from_secs(15);

#[derive(Debug, PartialEq)]
pub(crate) enum BrowserEvaluationAttempt {
    Unavailable,
    Completed(Result<Value, String>),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct BrowserScreenshot {
    pub(crate) png: Vec<u8>,
    pub(crate) width: u32,
    pub(crate) height: u32,
}

#[derive(Debug, PartialEq, Eq)]
pub(crate) enum BrowserScreenshotAttempt {
    Unavailable,
    Completed(Result<BrowserScreenshot, String>),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct BrowserPdf {
    pub(crate) bytes: Vec<u8>,
}

#[derive(Debug, PartialEq, Eq)]
pub(crate) enum BrowserPdfAttempt {
    Unavailable,
    Completed(Result<BrowserPdf, String>),
}

#[cfg_attr(not(feature = "gtk"), allow(dead_code))]
#[derive(Clone)]
pub(crate) struct BrowserEvaluationRequest {
    surface_id: String,
    script: String,
    deadline: Instant,
    responder: mpsc::Sender<BrowserEvaluationResponse>,
}

#[cfg_attr(not(feature = "gtk"), allow(dead_code))]
enum BrowserEvaluationResponse {
    Completed(Result<String, String>),
    Unavailable,
}

#[cfg_attr(not(feature = "gtk"), allow(dead_code))]
#[derive(Clone)]
pub(crate) struct BrowserScreenshotRequest {
    surface_id: String,
    full_document: bool,
    deadline: Instant,
    responder: mpsc::Sender<BrowserScreenshotResponse>,
}

#[cfg_attr(not(feature = "gtk"), allow(dead_code))]
enum BrowserScreenshotResponse {
    Completed(Result<BrowserScreenshot, String>),
    Unavailable,
}

#[cfg_attr(not(feature = "gtk"), allow(dead_code))]
#[derive(Clone)]
pub(crate) struct BrowserPdfRequest {
    surface_id: String,
    deadline: Instant,
    responder: mpsc::Sender<BrowserPdfResponse>,
}

#[cfg_attr(not(feature = "gtk"), allow(dead_code))]
enum BrowserPdfResponse {
    Completed(Result<BrowserPdf, String>),
    Unavailable,
}

#[cfg_attr(not(feature = "gtk"), allow(dead_code))]
impl BrowserEvaluationRequest {
    pub(crate) fn surface_id(&self) -> &str {
        &self.surface_id
    }

    pub(crate) fn script(&self) -> &str {
        &self.script
    }

    pub(crate) fn expired(&self) -> bool {
        Instant::now() >= self.deadline
    }

    pub(crate) fn complete(self, result: Result<String, String>) {
        let _ = self
            .responder
            .send(BrowserEvaluationResponse::Completed(result));
    }

    pub(crate) fn unavailable(self) {
        let _ = self.responder.send(BrowserEvaluationResponse::Unavailable);
    }
}

#[cfg_attr(not(feature = "gtk"), allow(dead_code))]
impl BrowserScreenshotRequest {
    pub(crate) fn surface_id(&self) -> &str {
        &self.surface_id
    }

    pub(crate) fn full_document(&self) -> bool {
        self.full_document
    }

    pub(crate) fn expired(&self) -> bool {
        Instant::now() >= self.deadline
    }

    pub(crate) fn complete(self, result: Result<BrowserScreenshot, String>) {
        let _ = self
            .responder
            .send(BrowserScreenshotResponse::Completed(result));
    }

    pub(crate) fn unavailable(self) {
        let _ = self.responder.send(BrowserScreenshotResponse::Unavailable);
    }
}

#[cfg_attr(not(feature = "gtk"), allow(dead_code))]
impl BrowserPdfRequest {
    pub(crate) fn surface_id(&self) -> &str {
        &self.surface_id
    }

    pub(crate) fn expired(&self) -> bool {
        Instant::now() >= self.deadline
    }

    pub(crate) fn complete(self, result: Result<BrowserPdf, String>) {
        let _ = self.responder.send(BrowserPdfResponse::Completed(result));
    }

    pub(crate) fn unavailable(self) {
        let _ = self.responder.send(BrowserPdfResponse::Unavailable);
    }
}

#[derive(Default)]
struct BrowserRuntimeBridge {
    active: bool,
    gtk_thread: Option<ThreadId>,
    pending_evaluations: VecDeque<BrowserEvaluationRequest>,
    pending_screenshots: VecDeque<BrowserScreenshotRequest>,
    pending_pdfs: VecDeque<BrowserPdfRequest>,
}

fn bridge() -> &'static Mutex<BrowserRuntimeBridge> {
    static BRIDGE: OnceLock<Mutex<BrowserRuntimeBridge>> = OnceLock::new();
    BRIDGE.get_or_init(|| Mutex::new(BrowserRuntimeBridge::default()))
}

#[cfg_attr(not(feature = "gtk"), allow(dead_code))]
pub(crate) fn activate_browser_runtime() {
    let Ok(mut bridge) = bridge().lock() else {
        return;
    };
    bridge.active = true;
    bridge.gtk_thread = Some(std::thread::current().id());
}

#[cfg_attr(not(feature = "gtk"), allow(dead_code))]
pub(crate) fn deactivate_browser_runtime() {
    let Ok(mut bridge) = bridge().lock() else {
        return;
    };
    bridge.active = false;
    bridge.gtk_thread = None;
    for request in bridge.pending_evaluations.drain(..) {
        request.unavailable();
    }
    for request in bridge.pending_screenshots.drain(..) {
        request.unavailable();
    }
    for request in bridge.pending_pdfs.drain(..) {
        request.unavailable();
    }
}

pub(crate) fn evaluate_in_live_browser(surface_id: &str, script: &str) -> BrowserEvaluationAttempt {
    let (responder, receiver) = mpsc::channel();
    {
        let Ok(mut bridge) = bridge().lock() else {
            return BrowserEvaluationAttempt::Unavailable;
        };
        if !bridge.active || bridge.gtk_thread == Some(std::thread::current().id()) {
            return BrowserEvaluationAttempt::Unavailable;
        }
        bridge
            .pending_evaluations
            .push_back(BrowserEvaluationRequest {
                surface_id: surface_id.to_string(),
                script: evaluation_envelope_script(script),
                deadline: Instant::now() + BROWSER_EVALUATION_TIMEOUT,
                responder,
            });
    }

    match receiver.recv_timeout(BROWSER_EVALUATION_TIMEOUT + Duration::from_millis(250)) {
        Ok(BrowserEvaluationResponse::Completed(Ok(result))) => {
            BrowserEvaluationAttempt::Completed(decode_evaluation_envelope(&result))
        }
        Ok(BrowserEvaluationResponse::Completed(Err(err))) => {
            BrowserEvaluationAttempt::Completed(Err(err))
        }
        Ok(BrowserEvaluationResponse::Unavailable) => BrowserEvaluationAttempt::Unavailable,
        Err(mpsc::RecvTimeoutError::Timeout) => BrowserEvaluationAttempt::Completed(Err(format!(
            "WebKitGTK evaluation timed out after {} seconds",
            BROWSER_EVALUATION_TIMEOUT.as_secs()
        ))),
        Err(mpsc::RecvTimeoutError::Disconnected) => BrowserEvaluationAttempt::Completed(Err(
            "WebKitGTK evaluation channel disconnected".to_string(),
        )),
    }
}

pub(crate) fn capture_live_browser_screenshot(
    surface_id: &str,
    full_document: bool,
) -> BrowserScreenshotAttempt {
    let (responder, receiver) = mpsc::channel();
    {
        let Ok(mut bridge) = bridge().lock() else {
            return BrowserScreenshotAttempt::Unavailable;
        };
        if !bridge.active || bridge.gtk_thread == Some(std::thread::current().id()) {
            return BrowserScreenshotAttempt::Unavailable;
        }
        bridge
            .pending_screenshots
            .push_back(BrowserScreenshotRequest {
                surface_id: surface_id.to_string(),
                full_document,
                deadline: Instant::now() + BROWSER_EVALUATION_TIMEOUT,
                responder,
            });
    }

    match receiver.recv_timeout(BROWSER_EVALUATION_TIMEOUT + Duration::from_millis(250)) {
        Ok(BrowserScreenshotResponse::Completed(result)) => {
            BrowserScreenshotAttempt::Completed(result)
        }
        Ok(BrowserScreenshotResponse::Unavailable) => BrowserScreenshotAttempt::Unavailable,
        Err(mpsc::RecvTimeoutError::Timeout) => BrowserScreenshotAttempt::Completed(Err(format!(
            "WebKitGTK screenshot timed out after {} seconds",
            BROWSER_EVALUATION_TIMEOUT.as_secs()
        ))),
        Err(mpsc::RecvTimeoutError::Disconnected) => BrowserScreenshotAttempt::Completed(Err(
            "WebKitGTK screenshot channel disconnected".to_string(),
        )),
    }
}

pub(crate) fn print_live_browser_pdf(surface_id: &str) -> BrowserPdfAttempt {
    let (responder, receiver) = mpsc::channel();
    {
        let Ok(mut bridge) = bridge().lock() else {
            return BrowserPdfAttempt::Unavailable;
        };
        if !bridge.active || bridge.gtk_thread == Some(std::thread::current().id()) {
            return BrowserPdfAttempt::Unavailable;
        }
        bridge.pending_pdfs.push_back(BrowserPdfRequest {
            surface_id: surface_id.to_string(),
            deadline: Instant::now() + BROWSER_PDF_TIMEOUT,
            responder,
        });
    }

    match receiver.recv_timeout(BROWSER_PDF_TIMEOUT + Duration::from_millis(250)) {
        Ok(BrowserPdfResponse::Completed(result)) => BrowserPdfAttempt::Completed(result),
        Ok(BrowserPdfResponse::Unavailable) => BrowserPdfAttempt::Unavailable,
        Err(mpsc::RecvTimeoutError::Timeout) => BrowserPdfAttempt::Completed(Err(format!(
            "WebKitGTK PDF generation timed out after {} seconds",
            BROWSER_PDF_TIMEOUT.as_secs()
        ))),
        Err(mpsc::RecvTimeoutError::Disconnected) => {
            BrowserPdfAttempt::Completed(Err("WebKitGTK PDF channel disconnected".to_string()))
        }
    }
}

#[cfg_attr(not(feature = "gtk"), allow(dead_code))]
pub(crate) fn take_browser_evaluation_requests() -> Vec<BrowserEvaluationRequest> {
    bridge()
        .lock()
        .map(|mut bridge| bridge.pending_evaluations.drain(..).collect())
        .unwrap_or_default()
}

#[cfg_attr(not(feature = "gtk"), allow(dead_code))]
pub(crate) fn requeue_browser_evaluation_request(request: BrowserEvaluationRequest) {
    let Ok(mut bridge) = bridge().lock() else {
        request.unavailable();
        return;
    };
    if bridge.active && !request.expired() {
        bridge.pending_evaluations.push_back(request);
    } else {
        request.unavailable();
    }
}

#[cfg_attr(not(feature = "gtk"), allow(dead_code))]
pub(crate) fn take_browser_screenshot_requests() -> Vec<BrowserScreenshotRequest> {
    bridge()
        .lock()
        .map(|mut bridge| bridge.pending_screenshots.drain(..).collect())
        .unwrap_or_default()
}

#[cfg_attr(not(feature = "gtk"), allow(dead_code))]
pub(crate) fn requeue_browser_screenshot_request(request: BrowserScreenshotRequest) {
    let Ok(mut bridge) = bridge().lock() else {
        request.unavailable();
        return;
    };
    if bridge.active && !request.expired() {
        bridge.pending_screenshots.push_back(request);
    } else {
        request.unavailable();
    }
}

#[cfg_attr(not(feature = "gtk"), allow(dead_code))]
pub(crate) fn take_browser_pdf_requests() -> Vec<BrowserPdfRequest> {
    bridge()
        .lock()
        .map(|mut bridge| bridge.pending_pdfs.drain(..).collect())
        .unwrap_or_default()
}

#[cfg_attr(not(feature = "gtk"), allow(dead_code))]
pub(crate) fn requeue_browser_pdf_request(request: BrowserPdfRequest) {
    let Ok(mut bridge) = bridge().lock() else {
        request.unavailable();
        return;
    };
    if bridge.active && !request.expired() {
        bridge.pending_pdfs.push_back(request);
    } else {
        request.unavailable();
    }
}

pub(crate) fn evaluation_envelope_script(script: &str) -> String {
    let source = serde_json::to_string(script).unwrap_or_else(|_| "\"\"".to_string());
    format!(
        r#"return (async function(source) {{
            try {{
                var seen = new WeakSet();
                var value = await (0, eval)(source);
                var encoded = JSON.stringify({{ ok: true, value: value === undefined ? null : value }}, function(_, item) {{
                    if (typeof item === 'bigint' || typeof item === 'function' || typeof item === 'symbol') return String(item);
                    if (item && typeof item === 'object') {{
                        if (seen.has(item)) return '[Circular]';
                        seen.add(item);
                    }}
                    return item;
                }});
                return encoded || JSON.stringify({{ ok: true, value: null }});
            }} catch (error) {{
                return JSON.stringify({{
                    ok: false,
                    error: String(error && (error.stack || error.message) || error)
                }});
            }}
        }})({source});"#
    )
}

pub(crate) fn live_browser_read_script(
    method: &str,
    params: &Value,
    selector: Option<&str>,
) -> Option<String> {
    let selector = serde_json::to_string(selector.unwrap_or("body")).ok()?;
    let script = match method {
        "browser.eval" | "browser.evalhandle" => params
            .get("script")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string(),
        "browser.snapshot" => live_browser_snapshot_script(),
        "browser.content" => r#"(function() {
            var html = document.documentElement ? document.documentElement.outerHTML : '';
            var text = document.body ? String(document.body.innerText == null ? (document.body.textContent || '') : document.body.innerText) : '';
            return { html: html, content: html, text: text, inner_text: text, title: document.title || '', url: location.href };
        })()"#
            .to_string(),
        "browser.innertext" => format!(
            "(function() {{ var el = document.querySelector({selector}); var text = el ? String(el.innerText == null ? (el.textContent || '') : el.innerText) : ''; return text; }})()"
        ),
        "browser.get.text" => format!(
            "(function() {{ var el = document.querySelector({selector}); return el ? String(el.innerText == null ? (el.textContent || '') : el.innerText) : ''; }})()"
        ),
        "browser.get.html" => {
            "document.documentElement ? document.documentElement.outerHTML : ''".to_string()
        }
        "browser.get.value" => format!(
            "(function() {{ var el = document.querySelector({selector}); return el && 'value' in el ? String(el.value == null ? '' : el.value) : ''; }})()"
        ),
        "browser.get.attr" => {
            let attr = serde_json::to_string(params.get("attr")?.as_str()?).ok()?;
            format!(
                "(function() {{ var el = document.querySelector({selector}); return el ? el.getAttribute({attr}) : null; }})()"
            )
        }
        "browser.get.title" => "document.title || ''".to_string(),
        "browser.get.count" => format!(
            "(function() {{ try {{ return {selector} ? document.querySelectorAll({selector}).length : 0; }} catch (_) {{ return 0; }} }})()"
        ),
        "browser.get.box" => format!(
            "(function() {{ var el = document.querySelector({selector}); if (!el) return null; var r = el.getBoundingClientRect(); return {{ x: r.x, y: r.y, width: r.width, height: r.height, top: r.top, right: r.right, bottom: r.bottom, left: r.left }}; }})()"
        ),
        "browser.get.styles" => {
            let property = params.get("property").and_then(Value::as_str);
            if let Some(property) = property {
                let property = serde_json::to_string(property).ok()?;
                format!(
                    "(function() {{ var el = document.querySelector({selector}); return el ? getComputedStyle(el).getPropertyValue({property}) : ''; }})()"
                )
            } else {
                format!(
                    "(function() {{ var el = document.querySelector({selector}); if (!el) return {{}}; var style = getComputedStyle(el); return Array.from(style).reduce(function(result, name) {{ result[name] = style.getPropertyValue(name); return result; }}, {{ display: style.display, color: style.color }}); }})()"
                )
            }
        }
        "browser.is.visible" => format!(
            "(function() {{ var el = document.querySelector({selector}); if (!el) return false; var style = getComputedStyle(el); return style.display !== 'none' && style.visibility !== 'hidden' && style.opacity !== '0' && !!(el.getClientRects().length || el.offsetWidth || el.offsetHeight); }})()"
        ),
        "browser.is.enabled" => format!(
            "(function() {{ var el = document.querySelector({selector}); return !!el && !el.disabled; }})()"
        ),
        "browser.is.checked" => format!(
            "(function() {{ var el = document.querySelector({selector}); return !!(el && el.checked); }})()"
        ),
        _ => return None,
    };
    Some(script)
}

fn live_browser_snapshot_script() -> String {
    r#"(function() {
        function cssEscape(value) {
            if (window.CSS && typeof window.CSS.escape === 'function') return window.CSS.escape(value);
            return String(value).replace(/[^a-zA-Z0-9_-]/g, function(ch) { return '\\' + ch; });
        }
        function uniqueIdSelector(el) {
            if (!el.id) return null;
            var selector = '#' + cssEscape(el.id);
            try { return document.querySelectorAll(selector).length === 1 ? selector : null; }
            catch (_) { return null; }
        }
        function selectorFor(el) {
            if (el === document.body) return 'body';
            var parts = [];
            while (el && el.nodeType === 1 && el !== document.documentElement) {
                var byId = uniqueIdSelector(el);
                if (byId) { parts.unshift(byId); break; }
                var tag = String(el.localName || el.tagName || '*').toLowerCase();
                var parent = el.parentElement;
                if (parent) {
                    var siblings = Array.from(parent.children).filter(function(item) {
                        return String(item.localName || item.tagName).toLowerCase() === tag;
                    });
                    if (siblings.length > 1) tag += ':nth-of-type(' + (siblings.indexOf(el) + 1) + ')';
                }
                parts.unshift(tag);
                el = parent;
            }
            return parts.join(' > ');
        }
        function roleFor(el) {
            var explicit = el.getAttribute && el.getAttribute('role');
            if (explicit) return explicit;
            var tag = String(el.localName || '').toLowerCase();
            if (/^h[1-6]$/.test(tag)) return 'heading';
            if (tag === 'a' && el.hasAttribute('href')) return 'link';
            if (tag === 'button' || tag === 'summary') return 'button';
            if (tag === 'textarea') return 'textbox';
            if (tag === 'select') return el.multiple ? 'listbox' : 'combobox';
            if (tag === 'option') return 'option';
            if (tag === 'img') return 'img';
            if (tag === 'label') return 'label';
            if (tag === 'li') return 'listitem';
            if (tag === 'input') {
                var type = String(el.type || 'text').toLowerCase();
                if (type === 'checkbox') return 'checkbox';
                if (type === 'radio') return 'radio';
                if (['button', 'submit', 'reset', 'image'].includes(type)) return 'button';
                if (type === 'range') return 'slider';
                return 'textbox';
            }
            if (el.isContentEditable) return 'textbox';
            return tag || 'element';
        }
        function accessibleName(el) {
            var labelledBy = el.getAttribute && el.getAttribute('aria-labelledby');
            if (labelledBy) {
                var labelled = labelledBy.split(/\s+/).map(function(id) {
                    var node = document.getElementById(id);
                    return node ? (node.innerText || node.textContent || '') : '';
                }).join(' ').trim();
                if (labelled) return labelled;
            }
            var aria = el.getAttribute && el.getAttribute('aria-label');
            if (aria) return aria.trim();
            if (el.labels && el.labels.length) {
                var labels = Array.from(el.labels).map(function(label) {
                    return label.innerText || label.textContent || '';
                }).join(' ').trim();
                if (labels) return labels;
            }
            for (var key of ['alt', 'placeholder', 'title']) {
                var attr = el.getAttribute && el.getAttribute(key);
                if (attr) return attr.trim();
            }
            var text = String(el.innerText || el.textContent || '').replace(/\s+/g, ' ').trim();
            if (text) return text;
            if ('value' in el && el.value != null) return String(el.value).trim();
            return '';
        }
        function isCandidate(el) {
            if (el === document.body || el.id) return true;
            var tag = String(el.localName || '').toLowerCase();
            if (/^h[1-6]$/.test(tag)) return true;
            if (['a', 'button', 'input', 'textarea', 'select', 'option', 'summary', 'label', 'li', 'img'].includes(tag)) return true;
            return !!(el.getAttribute && el.getAttribute('role')) || !!el.isContentEditable;
        }
        function isHidden(el) {
            if (el === document.body) return false;
            if (el.hidden || (el.getAttribute && el.getAttribute('aria-hidden') === 'true')) return true;
            var style = getComputedStyle(el);
            return style.display === 'none' || style.visibility === 'hidden';
        }
        function quoted(value) {
            value = String(value || '').replace(/\s+/g, ' ').trim();
            if (value.length > 160) value = value.slice(0, 157) + '...';
            return value ? ' "' + value.replace(/\\/g, '\\\\').replace(/"/g, '\\"') + '"' : '';
        }

        var refs = {};
        var lines = ['document' + quoted(document.title || location.href)];
        var nodes = document.body ? [document.body].concat(Array.from(document.body.querySelectorAll('*'))) : [];
        var next = 1;
        for (var el of nodes) {
            if (next > 500 || !isCandidate(el) || isHidden(el)) continue;
            var selector = selectorFor(el);
            if (!selector) continue;
            var ref = 'e' + next++;
            refs[ref] = selector;
            var depth = 0;
            for (var parent = el.parentElement; parent && parent !== document.body; parent = parent.parentElement) depth++;
            lines.push('  '.repeat(Math.min(depth, 12)) + '- ' + roleFor(el) + quoted(accessibleName(el)) + ' [ref=' + ref + ']');
        }
        if (!refs.e1 && document.body) refs.e1 = 'body';
        return { snapshot: lines.join('\n'), refs: refs };
    })()"#
        .to_string()
}

pub(crate) fn decode_evaluation_envelope(result: &str) -> Result<Value, String> {
    let envelope: Value = serde_json::from_str(result)
        .map_err(|err| format!("WebKitGTK returned invalid evaluation JSON: {err}"))?;
    if envelope.get("ok").and_then(Value::as_bool) == Some(true) {
        return Ok(envelope.get("value").cloned().unwrap_or(Value::Null));
    }
    Err(envelope
        .get("error")
        .and_then(Value::as_str)
        .unwrap_or("WebKitGTK JavaScript evaluation failed")
        .to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn evaluation_envelope_script_quotes_source_and_decodes_values() {
        let script = evaluation_envelope_script("window.value = `quoted`; window.value");
        assert!(script.contains(r#""window.value = `quoted`; window.value""#));
        assert_eq!(
            decode_evaluation_envelope(r#"{"ok":true,"value":{"ready":7}}"#),
            Ok(json!({"ready": 7}))
        );
        assert_eq!(
            decode_evaluation_envelope(r#"{"ok":false,"error":"ReferenceError: missing"}"#),
            Err("ReferenceError: missing".to_string())
        );
    }

    #[test]
    fn live_read_scripts_escape_selectors_and_preserve_response_inputs() {
        let selector = r#"input[name='quoted\"value']"#;
        let value_script =
            live_browser_read_script("browser.get.value", &json!({}), Some(selector))
                .expect("value script");
        assert!(value_script.contains(&serde_json::to_string(selector).unwrap()));

        let attr_script = live_browser_read_script(
            "browser.get.attr",
            &json!({"attr": "data-value"}),
            Some("#target"),
        )
        .expect("attribute script");
        assert!(attr_script.contains("getAttribute(\"data-value\")"));

        let content_script =
            live_browser_read_script("browser.content", &json!({}), None).expect("content script");
        assert!(content_script.contains("document.documentElement.outerHTML"));
        assert!(content_script.contains("location.href"));

        let inner_text_script =
            live_browser_read_script("browser.innertext", &json!({}), Some("#output"))
                .expect("inner text script");
        assert!(inner_text_script.contains("document.querySelector(\"#output\")"));

        assert_eq!(
            live_browser_read_script("browser.unknown", &json!({}), None),
            None
        );
    }

    #[test]
    fn live_snapshot_script_builds_accessible_refs_without_mutating_dom() {
        let script = live_browser_read_script("browser.snapshot", &json!({}), None)
            .expect("snapshot script");

        assert!(script.contains("return { snapshot: lines.join('\\n'), refs: refs }"));
        assert!(script.contains("document.body.querySelectorAll('*')"));
        assert!(script.contains("next > 500"));
        assert!(script.contains("[ref="));
        assert!(!script.contains("setAttribute("));
    }

    #[test]
    fn evaluation_bridge_round_trips_off_the_gtk_thread() {
        activate_browser_runtime();
        assert_eq!(
            evaluate_in_live_browser("surface-main", "1 + 1"),
            BrowserEvaluationAttempt::Unavailable
        );

        let worker = std::thread::spawn(|| evaluate_in_live_browser("surface-live", "6 * 7"));
        let deadline = Instant::now() + Duration::from_secs(1);
        let request = loop {
            if let Some(request) = take_browser_evaluation_requests().into_iter().next() {
                break request;
            }
            assert!(
                Instant::now() < deadline,
                "evaluation request was not queued"
            );
            std::thread::sleep(Duration::from_millis(5));
        };
        assert_eq!(request.surface_id(), "surface-live");
        assert!(request.script().contains("6 * 7"));
        request.complete(Ok(r#"{"ok":true,"value":42}"#.to_string()));
        assert_eq!(
            worker.join().expect("worker"),
            BrowserEvaluationAttempt::Completed(Ok(json!(42)))
        );

        let screenshot_worker =
            std::thread::spawn(|| capture_live_browser_screenshot("surface-live", true));
        let deadline = Instant::now() + Duration::from_secs(1);
        let request = loop {
            if let Some(request) = take_browser_screenshot_requests().into_iter().next() {
                break request;
            }
            assert!(
                Instant::now() < deadline,
                "screenshot request was not queued"
            );
            std::thread::sleep(Duration::from_millis(5));
        };
        assert_eq!(request.surface_id(), "surface-live");
        assert!(request.full_document());
        request.complete(Ok(BrowserScreenshot {
            png: b"native-png".to_vec(),
            width: 800,
            height: 1200,
        }));
        assert_eq!(
            screenshot_worker.join().expect("screenshot worker"),
            BrowserScreenshotAttempt::Completed(Ok(BrowserScreenshot {
                png: b"native-png".to_vec(),
                width: 800,
                height: 1200,
            }))
        );

        let pdf_worker = std::thread::spawn(|| print_live_browser_pdf("surface-live"));
        let deadline = Instant::now() + Duration::from_secs(1);
        let request = loop {
            if let Some(request) = take_browser_pdf_requests().into_iter().next() {
                break request;
            }
            assert!(Instant::now() < deadline, "PDF request was not queued");
            std::thread::sleep(Duration::from_millis(5));
        };
        assert_eq!(request.surface_id(), "surface-live");
        request.complete(Ok(BrowserPdf {
            bytes: b"%PDF-native".to_vec(),
        }));
        assert_eq!(
            pdf_worker.join().expect("PDF worker"),
            BrowserPdfAttempt::Completed(Ok(BrowserPdf {
                bytes: b"%PDF-native".to_vec(),
            }))
        );
        deactivate_browser_runtime();
    }
}
