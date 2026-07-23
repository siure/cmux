use gtk::glib::{
    self,
    translate::{from_glib_full, from_glib_none},
};
use gtk::prelude::*;
use gtk4 as gtk;
use std::cell::{Cell, RefCell};
use std::collections::{BTreeMap, HashMap};
use std::ffi::{c_char, c_int, c_void, CStr, CString};
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
use std::path::{Path, PathBuf};
use std::rc::{Rc, Weak};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::OnceLock;
use std::time::Duration;

type WebViewGetType = unsafe extern "C" fn() -> usize;
type WebViewLoadUri = unsafe extern "C" fn(*mut c_void, *const c_char);
type WebViewAction = unsafe extern "C" fn(*mut c_void);
type WebViewGetString = unsafe extern "C" fn(*mut c_void) -> *const c_char;
type WebViewGetBool = unsafe extern "C" fn(*mut c_void) -> c_int;
type WebViewSetZoom = unsafe extern "C" fn(*mut c_void, f64);
type WebViewGetObject = unsafe extern "C" fn(*mut c_void) -> *mut c_void;
type FindControllerSearch = unsafe extern "C" fn(*mut c_void, *const c_char, c_int, u32);
type FindControllerAction = unsafe extern "C" fn(*mut c_void);
type SettingsSetUserAgent = unsafe extern "C" fn(*mut c_void, *const c_char);
type UserScriptNew = unsafe extern "C" fn(
    *const c_char,
    c_int,
    c_int,
    *const *const c_char,
    *const *const c_char,
) -> *mut c_void;
type UserContentManagerAddScript = unsafe extern "C" fn(*mut c_void, *mut c_void);
type UserContentManagerRemoveAllScripts = unsafe extern "C" fn(*mut c_void);
type UserScriptUnref = unsafe extern "C" fn(*mut c_void);
type AsyncReadyCallback = Option<unsafe extern "C" fn(*mut c_void, *mut c_void, *mut c_void)>;
type WebViewEvaluateJavascript = unsafe extern "C" fn(
    *mut c_void,
    *const c_char,
    isize,
    *const c_char,
    *const c_char,
    *mut c_void,
    AsyncReadyCallback,
    *mut c_void,
);
type WebViewCallAsyncJavascriptFunction = unsafe extern "C" fn(
    *mut c_void,
    *const c_char,
    isize,
    *mut c_void,
    *const c_char,
    *const c_char,
    *mut c_void,
    AsyncReadyCallback,
    *mut c_void,
);
type WebViewCallAsyncJavascriptFunctionFinish =
    unsafe extern "C" fn(*mut c_void, *mut c_void, *mut *mut glib::ffi::GError) -> *mut c_void;
type WebViewGetSnapshot =
    unsafe extern "C" fn(*mut c_void, c_int, c_int, *mut c_void, AsyncReadyCallback, *mut c_void);
type WebViewGetSnapshotFinish = unsafe extern "C" fn(
    *mut c_void,
    *mut c_void,
    *mut *mut glib::ffi::GError,
) -> *mut gtk::gdk::ffi::GdkTexture;
type TextureSaveToPngBytes =
    unsafe extern "C" fn(*mut gtk::gdk::ffi::GdkTexture) -> *mut glib::ffi::GBytes;
type PrintOperationNew = unsafe extern "C" fn(*mut c_void) -> *mut c_void;
type PrintOperationSetPrintSettings = unsafe extern "C" fn(*mut c_void, *mut c_void);
type PrintOperationPrint = unsafe extern "C" fn(*mut c_void);
type PrintSettingsNew = unsafe extern "C" fn() -> *mut c_void;
type PrintSettingsSet = unsafe extern "C" fn(*mut c_void, *const c_char, *const c_char);
type JscValueToString = unsafe extern "C" fn(*mut c_void) -> *mut c_char;
type WebViewGetInspector = unsafe extern "C" fn(*mut c_void) -> *mut c_void;
type InspectorShow = unsafe extern "C" fn(*mut c_void) -> c_int;
type WebViewGetNetworkSession = unsafe extern "C" fn(*mut c_void) -> *mut c_void;
type NetworkSessionNew = unsafe extern "C" fn(*const c_char, *const c_char) -> *mut c_void;
type NetworkSessionGetCookieManager = unsafe extern "C" fn(*mut c_void) -> *mut c_void;
type NetworkProxySettingsNew =
    unsafe extern "C" fn(*const c_char, *const *const c_char) -> *mut c_void;
type NetworkProxySettingsFree = unsafe extern "C" fn(*mut c_void);
type NetworkSessionSetProxySettings = unsafe extern "C" fn(*mut c_void, c_int, *mut c_void);
type FileChooserRequestGetSelectMultiple = unsafe extern "C" fn(*mut c_void) -> c_int;
type FileChooserRequestSelectFiles = unsafe extern "C" fn(*mut c_void, *const *const c_char);
type WebContextGetDefault = unsafe extern "C" fn() -> *mut c_void;
type WebContextSetWebProcessExtensionsDirectory = unsafe extern "C" fn(*mut c_void, *const c_char);
type WebViewGetPageId = unsafe extern "C" fn(*mut c_void) -> u64;
type CredentialNew = unsafe extern "C" fn(*const c_char, *const c_char, c_int) -> *mut c_void;
type CredentialFree = unsafe extern "C" fn(*mut c_void);
type AuthenticationRequestAuthenticate = unsafe extern "C" fn(*mut c_void, *mut c_void);
type CookieManagerMutation =
    unsafe extern "C" fn(*mut c_void, *mut c_void, *mut c_void, AsyncReadyCallback, *mut c_void);
type CookieManagerGetCookies =
    unsafe extern "C" fn(*mut c_void, *const c_char, *mut c_void, AsyncReadyCallback, *mut c_void);
type CookieManagerGetAllCookies =
    unsafe extern "C" fn(*mut c_void, *mut c_void, AsyncReadyCallback, *mut c_void);
type CookieManagerMutationFinish =
    unsafe extern "C" fn(*mut c_void, *mut c_void, *mut *mut glib::ffi::GError) -> c_int;
type CookieManagerGetCookiesFinish = unsafe extern "C" fn(
    *mut c_void,
    *mut c_void,
    *mut *mut glib::ffi::GError,
) -> *mut glib::ffi::GList;
type SoupCookieNew = unsafe extern "C" fn(
    *const c_char,
    *const c_char,
    *const c_char,
    *const c_char,
    c_int,
) -> *mut c_void;
type SoupCookieFree = unsafe extern "C" fn(*mut c_void);
type SoupCookieGetString = unsafe extern "C" fn(*mut c_void) -> *const c_char;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct WebKitCookie {
    pub name: String,
    pub value: String,
}

#[derive(Debug)]
pub(crate) struct WebKitSnapshot {
    pub(crate) png: Vec<u8>,
    pub(crate) width: u32,
    pub(crate) height: u32,
}

#[derive(Debug)]
pub(crate) struct WebKitPdf {
    pub(crate) bytes: Vec<u8>,
}

pub(crate) fn configure_environment() {
    if std::env::var_os("WAYLAND_DISPLAY").is_some()
        && std::env::var_os("WEBKIT_DISABLE_DMABUF_RENDERER").is_none()
    {
        std::env::set_var("WEBKIT_DISABLE_DMABUF_RENDERER", "1");
    }
}

fn configure_web_process_extension(api: &WebKitApi) -> Result<(), String> {
    let directory = webkit_extension_directory()?;
    let directory = CString::new(directory.to_string_lossy().as_bytes())
        .map_err(|_| "WebKit extension directory contains NUL".to_string())?;
    let context = unsafe { (api.web_context_get_default)() };
    if context.is_null() {
        return Err("webkit_web_context_get_default returned null".to_string());
    }
    unsafe { (api.web_context_set_web_process_extensions_directory)(context, directory.as_ptr()) };
    Ok(())
}

fn webkit_extension_directory() -> Result<PathBuf, String> {
    static DIRECTORY: OnceLock<Result<PathBuf, String>> = OnceLock::new();
    DIRECTORY
        .get_or_init(|| {
            let directory = private_webkit_directory("extensions")?;
            let extension = directory.join("libcmux-webkit-request-headers.so");
            let mut file = OpenOptions::new()
                .write(true)
                .create(true)
                .truncate(true)
                .mode(0o700)
                .open(&extension)
                .map_err(|err| format!("create embedded WebKit extension: {err}"))?;
            file.write_all(include_bytes!(env!("CMUX_WEBKIT_EXTENSION_PATH")))
                .map_err(|err| format!("write embedded WebKit extension: {err}"))?;
            Ok(directory)
        })
        .clone()
}

fn webkit_request_configuration_directory() -> Result<PathBuf, String> {
    static DIRECTORY: OnceLock<Result<PathBuf, String>> = OnceLock::new();
    DIRECTORY
        .get_or_init(|| {
            let directory = private_webkit_directory("requests")?;
            std::env::set_var("CMUX_WEBKIT_REQUEST_CONFIG_DIR", &directory);
            Ok(directory)
        })
        .clone()
}

fn private_webkit_directory(kind: &str) -> Result<PathBuf, String> {
    let directory = std::env::temp_dir().join(format!("cmux-webkit-{kind}-{}", std::process::id()));
    if directory.exists() {
        fs::remove_dir_all(&directory)
            .map_err(|err| format!("clear private WebKit {kind} directory: {err}"))?;
    }
    fs::create_dir_all(&directory)
        .map_err(|err| format!("create private WebKit {kind} directory: {err}"))?;
    fs::set_permissions(&directory, fs::Permissions::from_mode(0o700))
        .map_err(|err| format!("secure private WebKit {kind} directory: {err}"))?;
    Ok(directory)
}

fn write_request_configuration(path: &PathBuf, payload: &[u8]) -> Result<(), String> {
    static GENERATION: AtomicU64 = AtomicU64::new(0);
    let generation = GENERATION.fetch_add(1, Ordering::Relaxed);
    let temporary = path.with_extension(format!("headers.{generation}.tmp"));
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(&temporary)
        .map_err(|err| format!("create WebKit request configuration: {err}"))?;
    if let Err(err) = file.write_all(payload) {
        let _ = fs::remove_file(&temporary);
        return Err(format!("write WebKit request configuration: {err}"));
    }
    if let Err(err) = fs::rename(&temporary, path) {
        let _ = fs::remove_file(&temporary);
        return Err(format!("replace WebKit request configuration: {err}"));
    }
    Ok(())
}

#[derive(Clone)]
pub(crate) struct GtkWebKitView {
    widget: gtk::Widget,
    api: Rc<WebKitApi>,
    _network_session: Rc<ProfileNetworkSession>,
    file_chooser: Rc<FileChooserState>,
    request_configuration: Rc<RequestConfigurationState>,
}

#[derive(Clone)]
pub(crate) struct WeakGtkWebKitView {
    widget: glib::WeakRef<gtk::Widget>,
    api: Weak<WebKitApi>,
    network_session: Weak<ProfileNetworkSession>,
    file_chooser: Weak<FileChooserState>,
    request_configuration: Weak<RequestConfigurationState>,
}

impl WeakGtkWebKitView {
    pub(crate) fn upgrade(&self) -> Option<GtkWebKitView> {
        Some(GtkWebKitView {
            widget: self.widget.upgrade()?,
            api: self.api.upgrade()?,
            _network_session: self.network_session.upgrade()?,
            file_chooser: self.file_chooser.upgrade()?,
            request_configuration: self.request_configuration.upgrade()?,
        })
    }
}

struct ProfileNetworkSession {
    raw: *mut c_void,
}

impl Drop for ProfileNetworkSession {
    fn drop(&mut self) {
        unsafe { glib::gobject_ffi::g_object_unref(self.raw.cast()) };
    }
}

thread_local! {
    static PROFILE_NETWORK_SESSIONS: RefCell<HashMap<String, Weak<ProfileNetworkSession>>> =
        RefCell::new(HashMap::new());
}

struct FileChooserState {
    api: Rc<WebKitApi>,
    generation: Cell<u64>,
    pending: RefCell<Option<PendingFileSelection>>,
}

struct PendingFileSelection {
    generation: u64,
    files: Vec<CString>,
}

struct RequestConfigurationState {
    api: Rc<WebKitApi>,
    path: PathBuf,
    credentials: RefCell<Option<(CString, CString)>>,
}

fn profile_network_session(
    api: &WebKitApi,
    profile_id: &str,
    data_generation: u64,
) -> Result<Rc<ProfileNetworkSession>, String> {
    let profile_component = browser_profile_storage_component(profile_id)?;
    let profile_key = format!("{profile_component}:{data_generation}");
    PROFILE_NETWORK_SESSIONS.with(|sessions| {
        if let Some(session) = sessions.borrow().get(&profile_key).and_then(Weak::upgrade) {
            return Ok(session);
        }
        let (data_directory, cache_directory) =
            browser_profile_storage_directories(&profile_component, data_generation)?;
        ensure_private_directory_chain(&data_directory, 4)?;
        ensure_private_directory_chain(&cache_directory, 3)?;
        let data_directory = path_cstring(&data_directory, "browser profile data directory")?;
        let cache_directory = path_cstring(&cache_directory, "browser profile cache directory")?;
        let raw =
            unsafe { (api.network_session_new)(data_directory.as_ptr(), cache_directory.as_ptr()) };
        if raw.is_null() {
            return Err("webkit_network_session_new returned null".to_string());
        }
        let session = Rc::new(ProfileNetworkSession { raw });
        let generation_prefix = format!("{profile_component}:");
        let mut sessions = sessions.borrow_mut();
        sessions.retain(|key, session| {
            !key.starts_with(&generation_prefix) && session.strong_count() > 0
        });
        sessions.insert(profile_key, Rc::downgrade(&session));
        Ok(session)
    })
}

fn browser_profile_storage_component(profile_id: &str) -> Result<String, String> {
    let normalized = profile_id.trim().to_ascii_lowercase();
    if normalized.is_empty()
        || !normalized
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
    {
        return Err("browser profile ID contains unsupported path characters".to_string());
    }
    Ok(normalized)
}

fn browser_profile_storage_directories(
    profile_key: &str,
    data_generation: u64,
) -> Result<(PathBuf, PathBuf), String> {
    let data_root = std::env::var_os("CMUX_BROWSER_WEBKIT_DATA_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| xdg_browser_root("XDG_DATA_HOME", ".local/share"));
    let cache_root = std::env::var_os("CMUX_BROWSER_WEBKIT_CACHE_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| xdg_browser_root("XDG_CACHE_HOME", ".cache"));
    Ok((
        data_root
            .join(profile_key)
            .join(format!("generation-{data_generation}"))
            .join("data"),
        cache_root
            .join(profile_key)
            .join(format!("generation-{data_generation}")),
    ))
}

fn xdg_browser_root(variable: &str, home_suffix: &str) -> PathBuf {
    if let Some(path) = std::env::var_os(variable).filter(|value| !value.is_empty()) {
        return PathBuf::from(path).join("cmux/browser-profiles");
    }
    if let Some(home) = std::env::var_os("HOME").filter(|value| !value.is_empty()) {
        return PathBuf::from(home)
            .join(home_suffix)
            .join("cmux/browser-profiles");
    }
    std::env::temp_dir()
        .join(format!("cmux-{}", std::process::id()))
        .join("browser-profiles")
}

fn ensure_private_directory(path: &Path) -> Result<(), String> {
    fs::create_dir_all(path)
        .map_err(|err| format!("create private WebKit directory {}: {err}", path.display()))?;
    fs::set_permissions(path, fs::Permissions::from_mode(0o700))
        .map_err(|err| format!("secure private WebKit directory {}: {err}", path.display()))
}

fn ensure_private_directory_chain(path: &Path, depth: usize) -> Result<(), String> {
    fs::create_dir_all(path)
        .map_err(|err| format!("create private WebKit directory {}: {err}", path.display()))?;
    let mut current = Some(path);
    for _ in 0..depth {
        let Some(directory) = current else {
            break;
        };
        ensure_private_directory(directory)?;
        current = directory.parent();
    }
    Ok(())
}

fn path_cstring(path: &Path, label: &str) -> Result<CString, String> {
    CString::new(path.to_string_lossy().as_bytes()).map_err(|_| format!("{label} contains NUL"))
}

impl Drop for RequestConfigurationState {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.path);
    }
}

impl GtkWebKitView {
    pub(crate) fn new(profile_id: &str, profile_data_generation: u64) -> Result<Self, String> {
        let api = Rc::new(WebKitApi::load()?);
        configure_web_process_extension(&api)?;
        let request_configuration_directory = webkit_request_configuration_directory()?;
        let session = profile_network_session(&api, profile_id, profile_data_generation)?;
        let raw = unsafe {
            let raw = glib::gobject_ffi::g_object_new(
                (api.web_view_get_type)(),
                c"network-session".as_ptr(),
                session.raw,
                std::ptr::null::<c_char>(),
            )
            .cast::<gtk::ffi::GtkWidget>();
            raw
        };
        if raw.is_null() {
            return Err(
                "constructing WebKitWebView with an isolated network session failed".to_string(),
            );
        }
        let widget: gtk::Widget = unsafe { from_glib_none(raw) };
        let file_chooser = Rc::new(FileChooserState {
            api: Rc::clone(&api),
            generation: Cell::new(0),
            pending: RefCell::new(None),
        });
        let file_chooser_for_signal = Rc::clone(&file_chooser);
        widget.connect_local("run-file-chooser", false, move |values| {
            let handled = values
                .get(1)
                .and_then(|value| value.get::<glib::Object>().ok())
                .is_some_and(|request| {
                    handle_file_chooser(&file_chooser_for_signal, request.as_ptr().cast())
                });
            Some(handled.to_value())
        });
        let page_id = unsafe { (api.web_view_get_page_id)(raw.cast()) };
        let request_configuration = Rc::new(RequestConfigurationState {
            api: Rc::clone(&api),
            path: request_configuration_directory.join(format!("{page_id}.headers")),
            credentials: RefCell::new(None),
        });
        let request_configuration_for_auth = Rc::clone(&request_configuration);
        widget.connect_local("authenticate", false, move |values| {
            let handled = values
                .get(1)
                .and_then(|value| value.get::<glib::Object>().ok())
                .is_some_and(|request| {
                    authenticate_request(&request_configuration_for_auth, request.as_ptr().cast())
                });
            Some(handled.to_value())
        });
        widget.set_hexpand(true);
        widget.set_vexpand(true);
        widget.add_css_class("cmux-browser-view");
        Ok(Self {
            widget,
            api,
            _network_session: session,
            file_chooser,
            request_configuration,
        })
    }

    pub(crate) fn widget(&self) -> &gtk::Widget {
        &self.widget
    }

    pub(crate) fn downgrade(&self) -> WeakGtkWebKitView {
        WeakGtkWebKitView {
            widget: self.widget.downgrade(),
            api: Rc::downgrade(&self.api),
            network_session: Rc::downgrade(&self._network_session),
            file_chooser: Rc::downgrade(&self.file_chooser),
            request_configuration: Rc::downgrade(&self.request_configuration),
        }
    }

    pub(crate) fn load_uri(&self, uri: &str) -> bool {
        let Ok(uri) = CString::new(uri) else {
            return false;
        };
        unsafe { (self.api.load_uri)(self.raw(), uri.as_ptr()) };
        true
    }

    pub(crate) fn uri(&self) -> Option<String> {
        self.get_string(self.api.get_uri)
    }

    pub(crate) fn title(&self) -> Option<String> {
        self.get_string(self.api.get_title)
    }

    pub(crate) fn go_back(&self) {
        unsafe { (self.api.go_back)(self.raw()) };
    }

    pub(crate) fn go_forward(&self) {
        unsafe { (self.api.go_forward)(self.raw()) };
    }

    pub(crate) fn reload(&self) {
        unsafe { (self.api.reload)(self.raw()) };
    }

    pub(crate) fn reload_bypass_cache(&self) {
        unsafe { (self.api.reload_bypass_cache)(self.raw()) };
    }

    pub(crate) fn set_offline(&self, offline: bool) -> Result<(), String> {
        let session = unsafe { (self.api.web_view_get_network_session)(self.raw()) };
        if session.is_null() {
            return Err("webkit_web_view_get_network_session returned null".to_string());
        }
        if !offline {
            unsafe {
                (self.api.network_session_set_proxy_settings)(
                    session,
                    WEBKIT_NETWORK_PROXY_MODE_DEFAULT,
                    std::ptr::null_mut(),
                )
            };
            return Ok(());
        }

        let proxy = unsafe {
            (self.api.network_proxy_settings_new)(c"http://127.0.0.1:0".as_ptr(), std::ptr::null())
        };
        if proxy.is_null() {
            return Err("webkit_network_proxy_settings_new returned null".to_string());
        }
        unsafe {
            (self.api.stop_loading)(self.raw());
            (self.api.network_session_set_proxy_settings)(
                session,
                WEBKIT_NETWORK_PROXY_MODE_CUSTOM,
                proxy,
            );
            (self.api.network_proxy_settings_free)(proxy);
        }
        Ok(())
    }

    pub(crate) fn set_zoom_level(&self, level: f64) {
        unsafe { (self.api.set_zoom_level)(self.raw(), level.clamp(0.25, 5.0)) };
    }

    pub(crate) fn find_text(&self, text: &str) -> bool {
        let Some(controller) = self.find_controller() else {
            return false;
        };
        if text.is_empty() {
            unsafe { (self.api.find_controller_search_finish)(controller) };
            return true;
        }
        let Ok(text) = CString::new(text) else {
            return false;
        };
        const CASE_INSENSITIVE: c_int = 1 << 0;
        const WRAP_AROUND: c_int = 1 << 4;
        unsafe {
            (self.api.find_controller_search)(
                controller,
                text.as_ptr(),
                CASE_INSENSITIVE | WRAP_AROUND,
                0,
            )
        };
        true
    }

    pub(crate) fn find_next(&self) {
        if let Some(controller) = self.find_controller() {
            unsafe { (self.api.find_controller_search_next)(controller) };
        }
    }

    pub(crate) fn find_previous(&self) {
        if let Some(controller) = self.find_controller() {
            unsafe { (self.api.find_controller_search_previous)(controller) };
        }
    }

    pub(crate) fn finish_find(&self) {
        if let Some(controller) = self.find_controller() {
            unsafe { (self.api.find_controller_search_finish)(controller) };
        }
    }

    pub(crate) fn set_user_agent(&self, user_agent: &str) -> Result<(), String> {
        let user_agent =
            CString::new(user_agent).map_err(|_| "user agent contains NUL".to_string())?;
        let settings = unsafe { (self.api.get_settings)(self.raw()) };
        if settings.is_null() {
            return Err("webkit_web_view_get_settings returned null".to_string());
        }
        unsafe { (self.api.settings_set_user_agent)(settings, user_agent.as_ptr()) };
        Ok(())
    }

    pub(crate) fn set_request_configuration(
        &self,
        headers: &[(String, String)],
        credentials: Option<(&str, &str)>,
    ) -> Result<(), String> {
        let mut payload = Vec::new();
        for (name, value) in headers {
            let name = CString::new(name.as_str())
                .map_err(|_| format!("request header name contains NUL: {name:?}"))?;
            let value = CString::new(value.as_str())
                .map_err(|_| "request header value contains NUL".to_string())?;
            payload.extend_from_slice(name.as_bytes_with_nul());
            payload.extend_from_slice(value.as_bytes_with_nul());
        }
        let credentials = credentials
            .map(
                |(username, password)| -> Result<(CString, CString), String> {
                    Ok((
                        CString::new(username)
                            .map_err(|_| "credential username contains NUL".to_string())?,
                        CString::new(password)
                            .map_err(|_| "credential password contains NUL".to_string())?,
                    ))
                },
            )
            .transpose()?;
        write_request_configuration(&self.request_configuration.path, &payload)?;
        self.request_configuration.credentials.replace(credentials);
        Ok(())
    }

    pub(crate) fn prepare_file_selection(&self, files: &[String]) -> Result<(), String> {
        if files.is_empty() {
            return Err("file selection requires at least one path".to_string());
        }
        let files = files
            .iter()
            .map(|path| {
                CString::new(path.as_str())
                    .map_err(|_| format!("file selection path contains NUL: {path:?}"))
            })
            .collect::<Result<Vec<_>, _>>()?;
        let generation = self.file_chooser.generation.get().wrapping_add(1).max(1);
        self.file_chooser.generation.set(generation);
        self.file_chooser
            .pending
            .replace(Some(PendingFileSelection { generation, files }));

        let weak_state = Rc::downgrade(&self.file_chooser);
        glib::timeout_add_local_once(Duration::from_secs(5), move || {
            let Some(state) = weak_state.upgrade() else {
                return;
            };
            let mut pending = state.pending.borrow_mut();
            if pending
                .as_ref()
                .is_some_and(|selection| selection.generation == generation)
            {
                pending.take();
            }
        });
        Ok(())
    }

    pub(crate) fn replace_init_scripts(&self, scripts: &[String]) -> Result<(), String> {
        let scripts = scripts
            .iter()
            .map(|script| {
                CString::new(script.as_str()).map_err(|_| "init script contains NUL".to_string())
            })
            .collect::<Result<Vec<_>, _>>()?;
        let manager = unsafe { (self.api.get_user_content_manager)(self.raw()) };
        if manager.is_null() {
            return Err("webkit_web_view_get_user_content_manager returned null".to_string());
        }

        let mut user_scripts = Vec::with_capacity(scripts.len());
        for script in &scripts {
            let user_script = unsafe {
                (self.api.user_script_new)(
                    script.as_ptr(),
                    WEBKIT_USER_CONTENT_INJECT_ALL_FRAMES,
                    WEBKIT_USER_SCRIPT_INJECT_AT_DOCUMENT_START,
                    std::ptr::null(),
                    std::ptr::null(),
                )
            };
            if user_script.is_null() {
                for user_script in user_scripts {
                    unsafe { (self.api.user_script_unref)(user_script) };
                }
                return Err("webkit_user_script_new returned null".to_string());
            }
            user_scripts.push(user_script);
        }

        unsafe { (self.api.user_content_manager_remove_all_scripts)(manager) };
        for user_script in user_scripts {
            unsafe {
                (self.api.user_content_manager_add_script)(manager, user_script);
                (self.api.user_script_unref)(user_script);
            }
        }
        Ok(())
    }

    pub(crate) fn replace_storage(
        &self,
        local: &BTreeMap<String, String>,
        session: &BTreeMap<String, String>,
    ) -> Result<(), String> {
        let payload = serde_json::to_string(&serde_json::json!({
            "local": local,
            "session": session
        }))
        .map_err(|err| format!("serialize browser storage: {err}"))?;
        let script = format!(
            r#"(function(payload) {{
                var apply = function(storage, entries) {{
                    if (!storage) return;
                    storage.clear();
                    Object.entries(entries || {{}}).forEach(function(entry) {{
                        storage.setItem(String(entry[0]), entry[1] == null ? '' : String(entry[1]));
                    }});
                }};
                try {{ apply(window.localStorage, payload.local); }} catch (_) {{}}
                try {{ apply(window.sessionStorage, payload.session); }} catch (_) {{}}
                return true;
            }})({payload})"#
        );
        self.evaluate_javascript(&script)
            .then_some(())
            .ok_or_else(|| "browser storage script contains NUL".to_string())
    }

    pub(crate) fn evaluate_javascript(&self, script: &str) -> bool {
        let Ok(script) = CString::new(script) else {
            return false;
        };
        unsafe {
            (self.api.evaluate_javascript)(
                self.raw(),
                script.as_ptr(),
                -1,
                std::ptr::null(),
                std::ptr::null(),
                std::ptr::null_mut(),
                None,
                std::ptr::null_mut(),
            )
        };
        true
    }

    pub(crate) fn evaluate_javascript_with_result(
        &self,
        function_body: &str,
        callback: impl FnOnce(Result<String, String>) + 'static,
    ) -> Result<(), String> {
        let function_body =
            CString::new(function_body).map_err(|_| "JavaScript contains NUL".to_string())?;
        let context = Box::new(JavascriptEvaluationContext {
            api: Rc::clone(&self.api),
            callback: Some(Box::new(callback)),
        });
        unsafe {
            (self.api.call_async_javascript_function)(
                self.raw(),
                function_body.as_ptr(),
                -1,
                std::ptr::null_mut(),
                std::ptr::null(),
                std::ptr::null(),
                std::ptr::null_mut(),
                Some(javascript_evaluation_ready),
                Box::into_raw(context).cast(),
            )
        };
        Ok(())
    }

    pub(crate) fn capture_snapshot(
        &self,
        full_document: bool,
        callback: impl FnOnce(Result<WebKitSnapshot, String>) + 'static,
    ) {
        let context = Box::new(SnapshotContext {
            api: Rc::clone(&self.api),
            callback: Some(Box::new(callback)),
        });
        let region = if full_document {
            WEBKIT_SNAPSHOT_REGION_FULL_DOCUMENT
        } else {
            WEBKIT_SNAPSHOT_REGION_VISIBLE
        };
        unsafe {
            (self.api.get_snapshot)(
                self.raw(),
                region,
                WEBKIT_SNAPSHOT_OPTIONS_NONE,
                std::ptr::null_mut(),
                Some(snapshot_ready),
                Box::into_raw(context).cast(),
            )
        };
    }

    pub(crate) fn print_to_pdf(
        &self,
        callback: impl FnOnce(Result<WebKitPdf, String>) + 'static,
    ) -> Result<(), String> {
        static NEXT_PDF_ID: AtomicU64 = AtomicU64::new(1);

        let path = std::env::temp_dir().join(format!(
            "cmux-webkit-{}-{}.pdf",
            std::process::id(),
            NEXT_PDF_ID.fetch_add(1, Ordering::Relaxed)
        ));
        let _ = fs::remove_file(&path);
        let uri = glib::filename_to_uri(&path, None)
            .map_err(|err| format!("could not create PDF output URI: {err}"))?;
        let uri =
            CString::new(uri.as_str()).map_err(|_| "PDF output URI contains NUL".to_string())?;

        unsafe {
            let operation = (self.api.print_operation_new)(self.raw());
            if operation.is_null() {
                return Err("webkit_print_operation_new returned null".to_string());
            }
            let settings = (self.api.print_settings_new)();
            if settings.is_null() {
                glib::gobject_ffi::g_object_unref(operation.cast());
                return Err("gtk_print_settings_new returned null".to_string());
            }
            (self.api.print_settings_set)(
                settings,
                b"printer\0".as_ptr().cast(),
                b"Print to File\0".as_ptr().cast(),
            );
            (self.api.print_settings_set)(
                settings,
                b"output-file-format\0".as_ptr().cast(),
                b"pdf\0".as_ptr().cast(),
            );
            (self.api.print_settings_set)(settings, b"output-uri\0".as_ptr().cast(), uri.as_ptr());
            (self.api.print_operation_set_print_settings)(operation, settings);
            glib::gobject_ffi::g_object_unref(settings.cast());

            let context = Box::new(PrintContext {
                _api: Rc::clone(&self.api),
                path,
                callback: Some(Box::new(callback)),
                error: None,
            });
            let context = Box::into_raw(context);
            let failed_handler = connect_signal(
                operation,
                b"failed\0",
                print_failed
                    as unsafe extern "C" fn(*mut c_void, *mut glib::ffi::GError, *mut c_void),
                context.cast(),
            );
            let finished_handler = connect_signal(
                operation,
                b"finished\0",
                print_finished as unsafe extern "C" fn(*mut c_void, *mut c_void),
                context.cast(),
            );
            if failed_handler == 0 || finished_handler == 0 {
                if failed_handler != 0 {
                    glib::gobject_ffi::g_signal_handler_disconnect(
                        operation.cast(),
                        failed_handler,
                    );
                }
                if finished_handler != 0 {
                    glib::gobject_ffi::g_signal_handler_disconnect(
                        operation.cast(),
                        finished_handler,
                    );
                }
                let context = Box::from_raw(context);
                let _ = fs::remove_file(&context.path);
                glib::gobject_ffi::g_object_unref(operation.cast());
                return Err("could not connect WebKitGTK print operation signals".to_string());
            }
            (self.api.print_operation_print)(operation);
        }
        Ok(())
    }

    pub(crate) fn set_inspector_visible(&self, visible: bool) -> bool {
        let inspector = unsafe { (self.api.get_inspector)(self.raw()) };
        if inspector.is_null() {
            return false;
        }
        if visible {
            unsafe { (self.api.inspector_show)(inspector) != 0 }
        } else {
            unsafe { (self.api.inspector_close)(inspector) };
            true
        }
    }

    pub(crate) fn can_go_back(&self) -> bool {
        unsafe { (self.api.can_go_back)(self.raw()) != 0 }
    }

    pub(crate) fn can_go_forward(&self) -> bool {
        unsafe { (self.api.can_go_forward)(self.raw()) != 0 }
    }

    pub(crate) fn is_loading(&self) -> bool {
        unsafe { (self.api.is_loading)(self.raw()) != 0 }
    }

    pub(crate) fn set_cookie(
        &self,
        url: &str,
        name: &str,
        value: &str,
        domain: Option<&str>,
        path: Option<&str>,
        max_age: Option<i32>,
    ) -> Result<(), String> {
        let manager = self.cookie_manager()?;
        let domain = domain
            .filter(|domain| !domain.trim().is_empty())
            .map(ToString::to_string)
            .or_else(|| cookie_domain_for_uri(url))
            .ok_or_else(|| format!("cookie URL has no host: {url}"))?;
        let path = path.filter(|path| !path.is_empty()).unwrap_or("/");
        let name = CString::new(name).map_err(|_| "cookie name contains NUL".to_string())?;
        let value = CString::new(value).map_err(|_| "cookie value contains NUL".to_string())?;
        let domain = CString::new(domain).map_err(|_| "cookie domain contains NUL".to_string())?;
        let path = CString::new(path).map_err(|_| "cookie path contains NUL".to_string())?;
        let cookie = unsafe {
            (self.api.soup_cookie_new)(
                name.as_ptr(),
                value.as_ptr(),
                domain.as_ptr(),
                path.as_ptr(),
                max_age.unwrap_or(-1),
            )
        };
        if cookie.is_null() {
            return Err("soup_cookie_new returned null".to_string());
        }
        begin_cookie_mutation(
            Rc::clone(&self.api),
            manager,
            cookie,
            self.api.cookie_manager_add_cookie,
            self.api.cookie_manager_add_cookie_finish,
            "add",
        );
        Ok(())
    }

    pub(crate) fn get_cookies<F>(&self, url: &str, callback: F) -> Result<(), String>
    where
        F: FnOnce(Result<Vec<WebKitCookie>, String>) + 'static,
    {
        let manager = self.cookie_manager()?;
        let url = CString::new(url).map_err(|_| "cookie URL contains NUL".to_string())?;
        let context = Box::new(CookieReadContext {
            api: Rc::clone(&self.api),
            finish: self.api.cookie_manager_get_cookies_finish,
            callback: Some(Box::new(callback)),
        });
        unsafe {
            (self.api.cookie_manager_get_cookies)(
                manager,
                url.as_ptr(),
                std::ptr::null_mut(),
                Some(cookie_read_ready),
                Box::into_raw(context).cast(),
            )
        };
        Ok(())
    }

    pub(crate) fn clear_cookies(&self, url: &str, name: Option<&str>) -> Result<(), String> {
        let manager = self.cookie_manager()?;
        let url = name
            .map(|_| CString::new(url).map_err(|_| "cookie URL contains NUL".to_string()))
            .transpose()?;
        let context = Box::new(CookieDeleteContext {
            api: Rc::clone(&self.api),
            finish: if name.is_some() {
                self.api.cookie_manager_get_cookies_finish
            } else {
                self.api.cookie_manager_get_all_cookies_finish
            },
            name: name.map(ToString::to_string),
        });
        let context: *mut c_void = Box::into_raw(context).cast();
        unsafe {
            if let Some(url) = url {
                (self.api.cookie_manager_get_cookies)(
                    manager,
                    url.as_ptr(),
                    std::ptr::null_mut(),
                    Some(cookie_delete_list_ready),
                    context,
                );
            } else {
                (self.api.cookie_manager_get_all_cookies)(
                    manager,
                    std::ptr::null_mut(),
                    Some(cookie_delete_list_ready),
                    context,
                );
            }
        }
        Ok(())
    }

    fn raw(&self) -> *mut c_void {
        self.widget.as_ptr().cast()
    }

    fn find_controller(&self) -> Option<*mut c_void> {
        let controller = unsafe { (self.api.web_view_get_find_controller)(self.raw()) };
        (!controller.is_null()).then_some(controller)
    }

    fn cookie_manager(&self) -> Result<*mut c_void, String> {
        let session = unsafe { (self.api.web_view_get_network_session)(self.raw()) };
        if session.is_null() {
            return Err("webkit_web_view_get_network_session returned null".to_string());
        }
        let manager = unsafe { (self.api.network_session_get_cookie_manager)(session) };
        if manager.is_null() {
            return Err("webkit_network_session_get_cookie_manager returned null".to_string());
        }
        Ok(manager)
    }

    fn get_string(&self, getter: WebViewGetString) -> Option<String> {
        let value = unsafe { getter(self.raw()) };
        if value.is_null() {
            return None;
        }
        Some(
            unsafe { CStr::from_ptr(value) }
                .to_string_lossy()
                .into_owned(),
        )
    }
}

fn handle_file_chooser(state: &FileChooserState, request: *mut c_void) -> bool {
    if request.is_null() {
        return false;
    }
    let Some(mut selection) = state.pending.borrow_mut().take() else {
        return false;
    };
    if unsafe { (state.api.file_chooser_request_get_select_multiple)(request) } == 0 {
        selection.files.truncate(1);
    }
    let mut files = selection
        .files
        .iter()
        .map(|path| path.as_ptr())
        .collect::<Vec<_>>();
    files.push(std::ptr::null());
    unsafe { (state.api.file_chooser_request_select_files)(request, files.as_ptr()) };
    true
}

fn authenticate_request(state: &RequestConfigurationState, request: *mut c_void) -> bool {
    if request.is_null() {
        return false;
    }
    let credentials = state.credentials.borrow();
    let Some((username, password)) = credentials.as_ref() else {
        return false;
    };
    let credential = unsafe {
        (state.api.credential_new)(
            username.as_ptr(),
            password.as_ptr(),
            WEBKIT_CREDENTIAL_PERSISTENCE_NONE,
        )
    };
    if credential.is_null() {
        return false;
    }
    unsafe {
        (state.api.authentication_request_authenticate)(request, credential);
        (state.api.credential_free)(credential);
    }
    true
}

struct WebKitApi {
    web_view_get_type: WebViewGetType,
    load_uri: WebViewLoadUri,
    go_back: WebViewAction,
    go_forward: WebViewAction,
    reload: WebViewAction,
    reload_bypass_cache: WebViewAction,
    stop_loading: WebViewAction,
    set_zoom_level: WebViewSetZoom,
    web_view_get_find_controller: WebViewGetObject,
    find_controller_search: FindControllerSearch,
    find_controller_search_next: FindControllerAction,
    find_controller_search_previous: FindControllerAction,
    find_controller_search_finish: FindControllerAction,
    get_settings: WebViewGetObject,
    settings_set_user_agent: SettingsSetUserAgent,
    get_user_content_manager: WebViewGetObject,
    user_script_new: UserScriptNew,
    user_content_manager_add_script: UserContentManagerAddScript,
    user_content_manager_remove_all_scripts: UserContentManagerRemoveAllScripts,
    user_script_unref: UserScriptUnref,
    evaluate_javascript: WebViewEvaluateJavascript,
    call_async_javascript_function: WebViewCallAsyncJavascriptFunction,
    call_async_javascript_function_finish: WebViewCallAsyncJavascriptFunctionFinish,
    get_snapshot: WebViewGetSnapshot,
    get_snapshot_finish: WebViewGetSnapshotFinish,
    texture_save_to_png_bytes: TextureSaveToPngBytes,
    print_operation_new: PrintOperationNew,
    print_operation_set_print_settings: PrintOperationSetPrintSettings,
    print_operation_print: PrintOperationPrint,
    print_settings_new: PrintSettingsNew,
    print_settings_set: PrintSettingsSet,
    jsc_value_to_string: JscValueToString,
    get_inspector: WebViewGetInspector,
    inspector_show: InspectorShow,
    inspector_close: WebViewAction,
    get_uri: WebViewGetString,
    get_title: WebViewGetString,
    can_go_back: WebViewGetBool,
    can_go_forward: WebViewGetBool,
    is_loading: WebViewGetBool,
    web_view_get_network_session: WebViewGetNetworkSession,
    network_session_new: NetworkSessionNew,
    network_session_get_cookie_manager: NetworkSessionGetCookieManager,
    network_proxy_settings_new: NetworkProxySettingsNew,
    network_proxy_settings_free: NetworkProxySettingsFree,
    network_session_set_proxy_settings: NetworkSessionSetProxySettings,
    file_chooser_request_get_select_multiple: FileChooserRequestGetSelectMultiple,
    file_chooser_request_select_files: FileChooserRequestSelectFiles,
    web_context_get_default: WebContextGetDefault,
    web_context_set_web_process_extensions_directory: WebContextSetWebProcessExtensionsDirectory,
    web_view_get_page_id: WebViewGetPageId,
    credential_new: CredentialNew,
    credential_free: CredentialFree,
    authentication_request_authenticate: AuthenticationRequestAuthenticate,
    cookie_manager_add_cookie: CookieManagerMutation,
    cookie_manager_add_cookie_finish: CookieManagerMutationFinish,
    cookie_manager_delete_cookie: CookieManagerMutation,
    cookie_manager_delete_cookie_finish: CookieManagerMutationFinish,
    cookie_manager_get_cookies: CookieManagerGetCookies,
    cookie_manager_get_cookies_finish: CookieManagerGetCookiesFinish,
    cookie_manager_get_all_cookies: CookieManagerGetAllCookies,
    cookie_manager_get_all_cookies_finish: CookieManagerGetCookiesFinish,
    soup_cookie_new: SoupCookieNew,
    soup_cookie_free: SoupCookieFree,
    soup_cookie_get_name: SoupCookieGetString,
    soup_cookie_get_value: SoupCookieGetString,
}

impl WebKitApi {
    fn load() -> Result<Self, String> {
        let handle = open_webkit_library()?;
        let soup_handle = open_soup_library()?;
        let jsc_handle = open_javascriptcore_library()?;
        let gtk_handle = open_gtk_library()?;
        unsafe {
            Ok(Self {
                web_view_get_type: load_symbol(handle, b"webkit_web_view_get_type\0")?,
                load_uri: load_symbol(handle, b"webkit_web_view_load_uri\0")?,
                go_back: load_symbol(handle, b"webkit_web_view_go_back\0")?,
                go_forward: load_symbol(handle, b"webkit_web_view_go_forward\0")?,
                reload: load_symbol(handle, b"webkit_web_view_reload\0")?,
                reload_bypass_cache: load_symbol(handle, b"webkit_web_view_reload_bypass_cache\0")?,
                stop_loading: load_symbol(handle, b"webkit_web_view_stop_loading\0")?,
                set_zoom_level: load_symbol(handle, b"webkit_web_view_set_zoom_level\0")?,
                web_view_get_find_controller: load_symbol(
                    handle,
                    b"webkit_web_view_get_find_controller\0",
                )?,
                find_controller_search: load_symbol(handle, b"webkit_find_controller_search\0")?,
                find_controller_search_next: load_symbol(
                    handle,
                    b"webkit_find_controller_search_next\0",
                )?,
                find_controller_search_previous: load_symbol(
                    handle,
                    b"webkit_find_controller_search_previous\0",
                )?,
                find_controller_search_finish: load_symbol(
                    handle,
                    b"webkit_find_controller_search_finish\0",
                )?,
                get_settings: load_symbol(handle, b"webkit_web_view_get_settings\0")?,
                settings_set_user_agent: load_symbol(handle, b"webkit_settings_set_user_agent\0")?,
                get_user_content_manager: load_symbol(
                    handle,
                    b"webkit_web_view_get_user_content_manager\0",
                )?,
                user_script_new: load_symbol(handle, b"webkit_user_script_new\0")?,
                user_content_manager_add_script: load_symbol(
                    handle,
                    b"webkit_user_content_manager_add_script\0",
                )?,
                user_content_manager_remove_all_scripts: load_symbol(
                    handle,
                    b"webkit_user_content_manager_remove_all_scripts\0",
                )?,
                user_script_unref: load_symbol(handle, b"webkit_user_script_unref\0")?,
                evaluate_javascript: load_symbol(handle, b"webkit_web_view_evaluate_javascript\0")?,
                call_async_javascript_function: load_symbol(
                    handle,
                    b"webkit_web_view_call_async_javascript_function\0",
                )?,
                call_async_javascript_function_finish: load_symbol(
                    handle,
                    b"webkit_web_view_call_async_javascript_function_finish\0",
                )?,
                get_snapshot: load_symbol(handle, b"webkit_web_view_get_snapshot\0")?,
                get_snapshot_finish: load_symbol(handle, b"webkit_web_view_get_snapshot_finish\0")?,
                texture_save_to_png_bytes: load_symbol(
                    gtk_handle,
                    b"gdk_texture_save_to_png_bytes\0",
                )?,
                print_operation_new: load_symbol(handle, b"webkit_print_operation_new\0")?,
                print_operation_set_print_settings: load_symbol(
                    handle,
                    b"webkit_print_operation_set_print_settings\0",
                )?,
                print_operation_print: load_symbol(handle, b"webkit_print_operation_print\0")?,
                print_settings_new: load_symbol(gtk_handle, b"gtk_print_settings_new\0")?,
                print_settings_set: load_symbol(gtk_handle, b"gtk_print_settings_set\0")?,
                jsc_value_to_string: load_symbol(jsc_handle, b"jsc_value_to_string\0")?,
                get_inspector: load_symbol(handle, b"webkit_web_view_get_inspector\0")?,
                inspector_show: load_symbol(handle, b"webkit_web_inspector_show\0")?,
                inspector_close: load_symbol(handle, b"webkit_web_inspector_close\0")?,
                get_uri: load_symbol(handle, b"webkit_web_view_get_uri\0")?,
                get_title: load_symbol(handle, b"webkit_web_view_get_title\0")?,
                can_go_back: load_symbol(handle, b"webkit_web_view_can_go_back\0")?,
                can_go_forward: load_symbol(handle, b"webkit_web_view_can_go_forward\0")?,
                is_loading: load_symbol(handle, b"webkit_web_view_is_loading\0")?,
                web_view_get_network_session: load_symbol(
                    handle,
                    b"webkit_web_view_get_network_session\0",
                )?,
                network_session_new: load_symbol(handle, b"webkit_network_session_new\0")?,
                network_session_get_cookie_manager: load_symbol(
                    handle,
                    b"webkit_network_session_get_cookie_manager\0",
                )?,
                network_proxy_settings_new: load_symbol(
                    handle,
                    b"webkit_network_proxy_settings_new\0",
                )?,
                network_proxy_settings_free: load_symbol(
                    handle,
                    b"webkit_network_proxy_settings_free\0",
                )?,
                network_session_set_proxy_settings: load_symbol(
                    handle,
                    b"webkit_network_session_set_proxy_settings\0",
                )?,
                file_chooser_request_get_select_multiple: load_symbol(
                    handle,
                    b"webkit_file_chooser_request_get_select_multiple\0",
                )?,
                file_chooser_request_select_files: load_symbol(
                    handle,
                    b"webkit_file_chooser_request_select_files\0",
                )?,
                web_context_get_default: load_symbol(handle, b"webkit_web_context_get_default\0")?,
                web_context_set_web_process_extensions_directory: load_symbol(
                    handle,
                    b"webkit_web_context_set_web_process_extensions_directory\0",
                )?,
                web_view_get_page_id: load_symbol(handle, b"webkit_web_view_get_page_id\0")?,
                credential_new: load_symbol(handle, b"webkit_credential_new\0")?,
                credential_free: load_symbol(handle, b"webkit_credential_free\0")?,
                authentication_request_authenticate: load_symbol(
                    handle,
                    b"webkit_authentication_request_authenticate\0",
                )?,
                cookie_manager_add_cookie: load_symbol(
                    handle,
                    b"webkit_cookie_manager_add_cookie\0",
                )?,
                cookie_manager_add_cookie_finish: load_symbol(
                    handle,
                    b"webkit_cookie_manager_add_cookie_finish\0",
                )?,
                cookie_manager_delete_cookie: load_symbol(
                    handle,
                    b"webkit_cookie_manager_delete_cookie\0",
                )?,
                cookie_manager_delete_cookie_finish: load_symbol(
                    handle,
                    b"webkit_cookie_manager_delete_cookie_finish\0",
                )?,
                cookie_manager_get_cookies: load_symbol(
                    handle,
                    b"webkit_cookie_manager_get_cookies\0",
                )?,
                cookie_manager_get_cookies_finish: load_symbol(
                    handle,
                    b"webkit_cookie_manager_get_cookies_finish\0",
                )?,
                cookie_manager_get_all_cookies: load_symbol(
                    handle,
                    b"webkit_cookie_manager_get_all_cookies\0",
                )?,
                cookie_manager_get_all_cookies_finish: load_symbol(
                    handle,
                    b"webkit_cookie_manager_get_all_cookies_finish\0",
                )?,
                soup_cookie_new: load_symbol(soup_handle, b"soup_cookie_new\0")?,
                soup_cookie_free: load_symbol(soup_handle, b"soup_cookie_free\0")?,
                soup_cookie_get_name: load_symbol(soup_handle, b"soup_cookie_get_name\0")?,
                soup_cookie_get_value: load_symbol(soup_handle, b"soup_cookie_get_value\0")?,
            })
        }
    }
}

type CookieReadCallback = Box<dyn FnOnce(Result<Vec<WebKitCookie>, String>)>;
type JavascriptEvaluationCallback = Box<dyn FnOnce(Result<String, String>)>;
type SnapshotCallback = Box<dyn FnOnce(Result<WebKitSnapshot, String>)>;
type PrintCallback = Box<dyn FnOnce(Result<WebKitPdf, String>)>;

struct JavascriptEvaluationContext {
    api: Rc<WebKitApi>,
    callback: Option<JavascriptEvaluationCallback>,
}

struct SnapshotContext {
    api: Rc<WebKitApi>,
    callback: Option<SnapshotCallback>,
}

struct PrintContext {
    _api: Rc<WebKitApi>,
    path: PathBuf,
    callback: Option<PrintCallback>,
    error: Option<String>,
}

struct CookieMutationContext {
    api: Rc<WebKitApi>,
    cookie: *mut c_void,
    finish: CookieManagerMutationFinish,
    operation: &'static str,
}

struct CookieReadContext {
    api: Rc<WebKitApi>,
    finish: CookieManagerGetCookiesFinish,
    callback: Option<CookieReadCallback>,
}

struct CookieDeleteContext {
    api: Rc<WebKitApi>,
    finish: CookieManagerGetCookiesFinish,
    name: Option<String>,
}

unsafe extern "C" fn javascript_evaluation_ready(
    web_view: *mut c_void,
    result: *mut c_void,
    user_data: *mut c_void,
) {
    let mut context = Box::from_raw(user_data.cast::<JavascriptEvaluationContext>());
    let mut error = std::ptr::null_mut();
    let value = (context.api.call_async_javascript_function_finish)(web_view, result, &mut error);
    let evaluation = if !error.is_null() {
        Err(take_glib_error(error).unwrap_or_else(|| "unknown WebKitGTK error".to_string()))
    } else if value.is_null() {
        Err("WebKitGTK JavaScript evaluation returned no value".to_string())
    } else {
        let string = (context.api.jsc_value_to_string)(value);
        let output = if string.is_null() {
            Err("JavaScriptCore could not convert the evaluation result".to_string())
        } else {
            let output = CStr::from_ptr(string).to_string_lossy().into_owned();
            glib::ffi::g_free(string.cast());
            Ok(output)
        };
        glib::gobject_ffi::g_object_unref(value.cast());
        output
    };
    if let Some(callback) = context.callback.take() {
        let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| callback(evaluation)));
    }
}

unsafe extern "C" fn snapshot_ready(
    web_view: *mut c_void,
    result: *mut c_void,
    user_data: *mut c_void,
) {
    let mut context = Box::from_raw(user_data.cast::<SnapshotContext>());
    let mut error = std::ptr::null_mut();
    let texture = (context.api.get_snapshot_finish)(web_view, result, &mut error);
    let snapshot = if !error.is_null() {
        Err(take_glib_error(error).unwrap_or_else(|| "unknown WebKitGTK error".to_string()))
    } else if texture.is_null() {
        Err("WebKitGTK screenshot returned no texture".to_string())
    } else {
        let texture: gtk::gdk::Texture = from_glib_full(texture);
        let width = texture.width();
        let height = texture.height();
        if width <= 0 || height <= 0 {
            Err(format!(
                "WebKitGTK screenshot returned invalid dimensions {width}x{height}"
            ))
        } else {
            let bytes = (context.api.texture_save_to_png_bytes)(texture.as_ptr());
            if bytes.is_null() {
                Err("GTK could not encode the WebKit screenshot as PNG".to_string())
            } else {
                let bytes: glib::Bytes = from_glib_full(bytes);
                Ok(WebKitSnapshot {
                    png: bytes.as_ref().to_vec(),
                    width: width as u32,
                    height: height as u32,
                })
            }
        }
    };
    if let Some(callback) = context.callback.take() {
        let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| callback(snapshot)));
    }
}

unsafe fn connect_signal<T: Copy>(
    instance: *mut c_void,
    signal: &'static [u8],
    callback: T,
    user_data: *mut c_void,
) -> u64 {
    let callback = std::mem::transmute_copy::<T, unsafe extern "C" fn()>(&callback);
    glib::gobject_ffi::g_signal_connect_data(
        instance.cast(),
        signal.as_ptr().cast(),
        Some(callback),
        user_data,
        None,
        glib::gobject_ffi::G_CONNECT_DEFAULT,
    )
}

unsafe extern "C" fn print_failed(
    _operation: *mut c_void,
    error: *mut glib::ffi::GError,
    user_data: *mut c_void,
) {
    let context = &mut *user_data.cast::<PrintContext>();
    context.error = Some(if error.is_null() || (*error).message.is_null() {
        "unknown WebKitGTK print error".to_string()
    } else {
        CStr::from_ptr((*error).message)
            .to_string_lossy()
            .into_owned()
    });
}

unsafe extern "C" fn print_finished(operation: *mut c_void, user_data: *mut c_void) {
    let mut context = Box::from_raw(user_data.cast::<PrintContext>());
    let result = if let Some(err) = context.error.take() {
        Err(err)
    } else {
        fs::read(&context.path)
            .map_err(|err| format!("could not read WebKitGTK PDF: {err}"))
            .and_then(|bytes| {
                bytes
                    .starts_with(b"%PDF-")
                    .then_some(WebKitPdf { bytes })
                    .ok_or_else(|| "WebKitGTK print output is not a PDF".to_string())
            })
    };
    let _ = fs::remove_file(&context.path);
    if let Some(callback) = context.callback.take() {
        let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| callback(result)));
    }
    glib::gobject_ffi::g_object_unref(operation.cast());
}

fn begin_cookie_mutation(
    api: Rc<WebKitApi>,
    manager: *mut c_void,
    cookie: *mut c_void,
    mutation: CookieManagerMutation,
    finish: CookieManagerMutationFinish,
    operation: &'static str,
) {
    let context = Box::new(CookieMutationContext {
        api,
        cookie,
        finish,
        operation,
    });
    unsafe {
        mutation(
            manager,
            cookie,
            std::ptr::null_mut(),
            Some(cookie_mutation_ready),
            Box::into_raw(context).cast(),
        )
    };
}

unsafe extern "C" fn cookie_mutation_ready(
    manager: *mut c_void,
    result: *mut c_void,
    user_data: *mut c_void,
) {
    let context = Box::from_raw(user_data.cast::<CookieMutationContext>());
    let mut error = std::ptr::null_mut();
    let succeeded = (context.finish)(manager, result, &mut error) != 0;
    if !succeeded {
        eprintln!(
            "cmux: WebKit cookie {} failed: {}",
            context.operation,
            take_glib_error(error).unwrap_or_else(|| "unknown error".to_string())
        );
    } else if !error.is_null() {
        glib::ffi::g_error_free(error);
    }
    (context.api.soup_cookie_free)(context.cookie);
}

unsafe extern "C" fn cookie_read_ready(
    manager: *mut c_void,
    result: *mut c_void,
    user_data: *mut c_void,
) {
    let mut context = Box::from_raw(user_data.cast::<CookieReadContext>());
    let cookies = finish_cookie_list(&context.api, context.finish, manager, result);
    if let Some(callback) = context.callback.take() {
        let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| callback(cookies)));
    }
}

unsafe extern "C" fn cookie_delete_list_ready(
    manager: *mut c_void,
    result: *mut c_void,
    user_data: *mut c_void,
) {
    let context = Box::from_raw(user_data.cast::<CookieDeleteContext>());
    let mut error = std::ptr::null_mut();
    let list = (context.finish)(manager, result, &mut error);
    if !error.is_null() {
        eprintln!(
            "cmux: WebKit cookie lookup before clear failed: {}",
            take_glib_error(error).unwrap_or_else(|| "unknown error".to_string())
        );
        free_cookie_list(&context.api, list);
        return;
    }

    let mut node = list;
    while !node.is_null() {
        let cookie = (*node).data;
        let next = (*node).next;
        let matches = context.name.as_deref().is_none_or(|wanted| {
            cookie_string(cookie, context.api.soup_cookie_get_name).as_deref() == Some(wanted)
        });
        if matches {
            begin_cookie_mutation(
                Rc::clone(&context.api),
                manager,
                cookie,
                context.api.cookie_manager_delete_cookie,
                context.api.cookie_manager_delete_cookie_finish,
                "delete",
            );
        } else {
            (context.api.soup_cookie_free)(cookie);
        }
        node = next;
    }
    glib::ffi::g_list_free(list);
}

unsafe fn free_cookie_list(api: &WebKitApi, list: *mut glib::ffi::GList) {
    let mut node = list;
    while !node.is_null() {
        (api.soup_cookie_free)((*node).data);
        node = (*node).next;
    }
    glib::ffi::g_list_free(list);
}

unsafe fn finish_cookie_list(
    api: &WebKitApi,
    finish: CookieManagerGetCookiesFinish,
    manager: *mut c_void,
    result: *mut c_void,
) -> Result<Vec<WebKitCookie>, String> {
    let mut error = std::ptr::null_mut();
    let list = finish(manager, result, &mut error);
    if !error.is_null() {
        return Err(take_glib_error(error).unwrap_or_else(|| "unknown error".to_string()));
    }

    let mut cookies = Vec::new();
    let mut node = list;
    while !node.is_null() {
        let cookie = (*node).data;
        if let (Some(name), Some(value)) = (
            cookie_string(cookie, api.soup_cookie_get_name),
            cookie_string(cookie, api.soup_cookie_get_value),
        ) {
            cookies.push(WebKitCookie { name, value });
        }
        (api.soup_cookie_free)(cookie);
        node = (*node).next;
    }
    glib::ffi::g_list_free(list);
    Ok(cookies)
}

unsafe fn cookie_string(cookie: *mut c_void, getter: SoupCookieGetString) -> Option<String> {
    let value = getter(cookie);
    if value.is_null() {
        None
    } else {
        Some(CStr::from_ptr(value).to_string_lossy().into_owned())
    }
}

unsafe fn take_glib_error(error: *mut glib::ffi::GError) -> Option<String> {
    if error.is_null() {
        return None;
    }
    let message = if (*error).message.is_null() {
        None
    } else {
        Some(
            CStr::from_ptr((*error).message)
                .to_string_lossy()
                .into_owned(),
        )
    };
    glib::ffi::g_error_free(error);
    message
}

fn cookie_domain_for_uri(uri: &str) -> Option<String> {
    glib::Uri::parse(uri, glib::UriFlags::NONE)
        .ok()?
        .host()
        .filter(|host| !host.is_empty())
        .map(|host| host.to_string())
}

fn open_webkit_library() -> Result<*mut c_void, String> {
    open_shared_library(
        &["libwebkitgtk-6.0.so.4", "libwebkitgtk-6.0.so"],
        "WebKitGTK 6.0 runtime",
    )
}

fn open_soup_library() -> Result<*mut c_void, String> {
    open_shared_library(
        &["libsoup-3.0.so.0", "libsoup-3.0.so"],
        "libsoup 3.0 runtime",
    )
}

fn open_javascriptcore_library() -> Result<*mut c_void, String> {
    open_shared_library(
        &[
            "libjavascriptcoregtk-6.0.so.1",
            "libjavascriptcoregtk-6.0.so",
        ],
        "JavaScriptCoreGTK 6.0 runtime",
    )
}

fn open_gtk_library() -> Result<*mut c_void, String> {
    open_shared_library(&["libgtk-4.so.1", "libgtk-4.so"], "GTK 4 runtime")
}

fn open_shared_library(names: &[&str], label: &str) -> Result<*mut c_void, String> {
    for name in names {
        let name = CString::new(*name).expect("library name has no NUL");
        let handle = unsafe { dlopen(name.as_ptr(), RTLD_LAZY | RTLD_LOCAL) };
        if !handle.is_null() {
            // WebKitGTK types and instances can outlive the Rust cache while GTK
            // tears down the widget tree, so retain the module for process life.
            return Ok(handle);
        }
    }
    Err(last_dl_error().unwrap_or_else(|| format!("{label} not found")))
}

unsafe fn load_symbol<T: Copy>(handle: *mut c_void, name: &'static [u8]) -> Result<T, String> {
    let symbol = dlsym(handle, name.as_ptr().cast());
    if symbol.is_null() {
        return Err(format!(
            "missing WebKitGTK symbol {}: {}",
            String::from_utf8_lossy(&name[..name.len().saturating_sub(1)]),
            last_dl_error().unwrap_or_else(|| "unknown dynamic loader error".to_string())
        ));
    }
    Ok(std::mem::transmute_copy(&symbol))
}

fn last_dl_error() -> Option<String> {
    let error = unsafe { dlerror() };
    if error.is_null() {
        None
    } else {
        Some(
            unsafe { CStr::from_ptr(error) }
                .to_string_lossy()
                .into_owned(),
        )
    }
}

const RTLD_LAZY: c_int = 1;
const RTLD_LOCAL: c_int = 0;
const WEBKIT_USER_CONTENT_INJECT_ALL_FRAMES: c_int = 0;
const WEBKIT_USER_SCRIPT_INJECT_AT_DOCUMENT_START: c_int = 0;
const WEBKIT_SNAPSHOT_REGION_VISIBLE: c_int = 0;
const WEBKIT_SNAPSHOT_REGION_FULL_DOCUMENT: c_int = 1;
const WEBKIT_SNAPSHOT_OPTIONS_NONE: c_int = 0;
const WEBKIT_NETWORK_PROXY_MODE_DEFAULT: c_int = 0;
const WEBKIT_NETWORK_PROXY_MODE_CUSTOM: c_int = 2;
const WEBKIT_CREDENTIAL_PERSISTENCE_NONE: c_int = 0;

unsafe extern "C" {
    fn dlopen(filename: *const c_char, flags: c_int) -> *mut c_void;
    fn dlsym(handle: *mut c_void, symbol: *const c_char) -> *mut c_void;
    fn dlerror() -> *const c_char;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn webkit_runtime_resolves_required_browser_symbols_when_installed() {
        if open_webkit_library().is_err() {
            return;
        }
        WebKitApi::load().expect("installed WebKitGTK runtime should expose browser symbols");
    }

    #[test]
    fn cookie_domain_uses_uri_host_and_rejects_hostless_urls() {
        assert_eq!(
            cookie_domain_for_uri("https://example.test:8443/account"),
            Some("example.test".to_string())
        );
        assert_eq!(cookie_domain_for_uri("about:blank"), None);
    }

    #[test]
    fn browser_profile_storage_components_are_stable_and_path_safe() {
        assert_eq!(
            browser_profile_storage_component(" 52B43C05-4A1D-45D3-8FD5-9EF94952E445 ")
                .expect("profile component"),
            "52b43c05-4a1d-45d3-8fd5-9ef94952e445"
        );
        assert!(browser_profile_storage_component("../default").is_err());
        assert!(browser_profile_storage_component("profile/name").is_err());
        assert!(browser_profile_storage_component("").is_err());
    }

    #[test]
    fn browser_profile_storage_generation_selects_distinct_directories() {
        let first = browser_profile_storage_directories("profile-a", 1).expect("first generation");
        let second =
            browser_profile_storage_directories("profile-a", 2).expect("second generation");
        assert_ne!(first, second);
        assert!(first.0.ends_with("profile-a/generation-1/data"));
        assert!(first.1.ends_with("profile-a/generation-1"));
        assert!(second.0.ends_with("profile-a/generation-2/data"));
        assert!(second.1.ends_with("profile-a/generation-2"));
    }

    #[test]
    fn browser_profile_storage_chain_is_private() {
        let tmp = tempfile::tempdir().expect("profile storage root");
        let root = tmp.path().join("browser-profiles");
        let profile = root.join("profile-a");
        let generation = profile.join("generation-3");
        let data = generation.join("data");
        ensure_private_directory_chain(&data, 4).expect("private storage chain");
        for directory in [&root, &profile, &generation, &data] {
            assert_eq!(
                fs::metadata(directory)
                    .expect("directory metadata")
                    .permissions()
                    .mode()
                    & 0o777,
                0o700
            );
        }
    }
}
