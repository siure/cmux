use serde_json::{json, Value};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum GlobalShortcutAction {
    GlobalSearch,
    ShowHideAllWindows,
}

impl GlobalShortcutAction {
    pub(crate) fn id(self) -> &'static str {
        match self {
            Self::GlobalSearch => "globalSearch",
            Self::ShowHideAllWindows => "showHideAllWindows",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct GlobalShortcutSpec {
    pub(crate) action: GlobalShortcutAction,
    pub(crate) combo: String,
}

impl GlobalShortcutSpec {
    #[cfg_attr(not(feature = "gtk"), allow(dead_code))]
    fn description(&self) -> &'static str {
        match self.action {
            GlobalShortcutAction::GlobalSearch => "Search all open cmux panels",
            GlobalShortcutAction::ShowHideAllWindows => "Show or hide all cmux windows",
        }
    }

    pub(crate) fn value(&self) -> Value {
        json!({
            "id": self.action.id(),
            "combo": self.combo,
            "portal_trigger": portal_trigger(&self.combo)
        })
    }
}

pub(crate) fn portal_trigger(combo: &str) -> Option<String> {
    let mut modifiers = Vec::<String>::new();
    let mut key = None;
    for part in combo
        .split('+')
        .map(str::trim)
        .filter(|part| !part.is_empty())
    {
        let normalized = part.to_ascii_lowercase();
        match normalized.as_str() {
            "cmd" | "super" | "logo" => modifiers.push("LOGO".to_string()),
            "ctrl" | "control" => modifiers.push("CTRL".to_string()),
            "opt" | "alt" | "option" => modifiers.push("ALT".to_string()),
            "shift" => modifiers.push("SHIFT".to_string()),
            raw if key.is_none() => key = portal_key_name(raw),
            _ => return None,
        }
    }
    let key = key?;
    modifiers.push(key);
    Some(modifiers.join("+"))
}

fn portal_key_name(key: &str) -> Option<String> {
    let name = match key {
        "." => Some("period"),
        "," => Some("comma"),
        "[" => Some("bracketleft"),
        "]" => Some("bracketright"),
        "=" => Some("equal"),
        "-" => Some("minus"),
        "enter" | "return" => Some("Return"),
        "escape" | "esc" => Some("Escape"),
        "space" => Some("space"),
        "left" => Some("Left"),
        "right" => Some("Right"),
        "up" => Some("Up"),
        "down" => Some("Down"),
        raw if raw.len() == 1 && raw.as_bytes()[0].is_ascii_alphanumeric() => Some(raw),
        raw if raw.starts_with('f')
            && raw[1..]
                .parse::<u8>()
                .is_ok_and(|number| (1..=35).contains(&number)) =>
        {
            Some(raw)
        }
        _ => None,
    }?;
    Some(name.to_string())
}

#[cfg(feature = "gtk")]
mod backend {
    use super::*;
    use crate::app::AppState;
    use ashpd::desktop::global_shortcuts::{BindShortcutsOptions, GlobalShortcuts, NewShortcut};
    use ashpd::desktop::CreateSessionOptions;
    use futures_util::StreamExt;
    use std::ffi::{c_char, c_int, c_long, c_uint, c_ulong, c_void, CString};
    use std::mem;
    use std::ptr;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::{Arc, Mutex};
    use std::thread;
    use std::time::Duration;

    const POLL_INTERVAL: Duration = Duration::from_millis(100);

    pub(crate) struct GlobalShortcutManager {
        stop: Arc<AtomicBool>,
    }

    impl GlobalShortcutManager {
        pub(crate) fn start(app_state: Arc<Mutex<AppState>>) -> Self {
            let stop = Arc::new(AtomicBool::new(false));
            let worker_stop = Arc::clone(&stop);
            thread::Builder::new()
                .name("cmux-global-shortcuts".to_string())
                .spawn(move || registration_loop(app_state, worker_stop))
                .expect("global shortcut worker thread");
            Self { stop }
        }
    }

    impl Drop for GlobalShortcutManager {
        fn drop(&mut self) {
            self.stop.store(true, Ordering::Release);
        }
    }

    fn registration_loop(app_state: Arc<Mutex<AppState>>, stop: Arc<AtomicBool>) {
        while !stop.load(Ordering::Acquire) {
            let Some((generation, specs)) = registration_snapshot(&app_state) else {
                return;
            };
            if specs.is_empty() {
                update_status(&app_state, "inactive", "unbound", None, &specs);
                while !stop.load(Ordering::Acquire)
                    && !registration_changed(&app_state, generation, &specs)
                {
                    thread::sleep(POLL_INTERVAL);
                }
                continue;
            }
            update_status(&app_state, "portal", "registering", None, &specs);
            let portal_result = run_portal(&app_state, &stop, generation, &specs);
            if portal_result.is_ok() || stop.load(Ordering::Acquire) {
                continue;
            }

            let portal_error = portal_result.unwrap_err();
            update_status(
                &app_state,
                "x11",
                "registering",
                Some(format!("portal unavailable: {portal_error}")),
                &specs,
            );
            match run_x11(&app_state, &stop, generation, &specs) {
                Ok(()) => {}
                Err(x11_error) => {
                    update_status(
                        &app_state,
                        "unavailable",
                        "unavailable",
                        Some(format!(
                            "portal unavailable: {portal_error}; X11 unavailable: {x11_error}"
                        )),
                        &specs,
                    );
                    for _ in 0..20 {
                        if stop.load(Ordering::Acquire)
                            || registration_changed(&app_state, generation, &specs)
                        {
                            break;
                        }
                        thread::sleep(POLL_INTERVAL);
                    }
                }
            }
        }
    }

    fn registration_snapshot(
        app_state: &Arc<Mutex<AppState>>,
    ) -> Option<(u64, Vec<GlobalShortcutSpec>)> {
        let app = app_state.lock().ok()?;
        Some((app.config_reload_generation(), app.global_shortcut_specs()))
    }

    fn registration_changed(
        app_state: &Arc<Mutex<AppState>>,
        generation: u64,
        specs: &[GlobalShortcutSpec],
    ) -> bool {
        registration_snapshot(app_state).is_none_or(|(current_generation, current_specs)| {
            current_generation != generation || current_specs != specs
        })
    }

    fn update_status(
        app_state: &Arc<Mutex<AppState>>,
        backend: &str,
        state: &str,
        detail: Option<String>,
        specs: &[GlobalShortcutSpec],
    ) {
        if let Ok(mut app) = app_state.lock() {
            app.set_global_shortcut_status(backend, state, detail, specs);
        }
    }

    fn activate(app_state: &Arc<Mutex<AppState>>, action: GlobalShortcutAction) {
        if let Ok(mut app) = app_state.lock() {
            app.activate_global_shortcut(action);
        }
    }

    fn run_portal(
        app_state: &Arc<Mutex<AppState>>,
        stop: &AtomicBool,
        generation: u64,
        specs: &[GlobalShortcutSpec],
    ) -> Result<(), String> {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .map_err(|error| error.to_string())?;
        runtime.block_on(run_portal_async(app_state, stop, generation, specs))
    }

    async fn run_portal_async(
        app_state: &Arc<Mutex<AppState>>,
        stop: &AtomicBool,
        generation: u64,
        specs: &[GlobalShortcutSpec],
    ) -> Result<(), String> {
        let proxy = tokio::time::timeout(Duration::from_secs(2), GlobalShortcuts::new())
            .await
            .map_err(|_| "portal discovery timed out".to_string())?
            .map_err(|error| error.to_string())?;
        let session = tokio::time::timeout(
            Duration::from_secs(2),
            proxy.create_session(CreateSessionOptions::default()),
        )
        .await
        .map_err(|_| "portal session creation timed out".to_string())?
        .map_err(|error| error.to_string())?;
        let shortcuts = specs
            .iter()
            .map(|spec| {
                let trigger = portal_trigger(&spec.combo);
                NewShortcut::new(spec.action.id(), spec.description())
                    .preferred_trigger(trigger.as_deref())
            })
            .collect::<Vec<_>>();
        let request = proxy
            .bind_shortcuts(&session, &shortcuts, None, BindShortcutsOptions::default())
            .await
            .map_err(|error| error.to_string())?;
        request.response().map_err(|error| error.to_string())?;
        let session_path = serde_json::to_value(&session)
            .ok()
            .and_then(|value| value.as_str().map(str::to_string));
        let mut activated = proxy
            .receive_activated()
            .await
            .map_err(|error| error.to_string())?;
        update_status(app_state, "portal", "registered", None, specs);

        while !stop.load(Ordering::Acquire) && !registration_changed(app_state, generation, specs) {
            if let Ok(Some(event)) =
                tokio::time::timeout(Duration::from_millis(250), activated.next()).await
            {
                if session_path.as_deref() == Some(event.session_handle().as_str()) {
                    if let Some(spec) = specs
                        .iter()
                        .find(|spec| spec.action.id() == event.shortcut_id())
                    {
                        activate(app_state, spec.action);
                    }
                }
            }
        }
        let _ = session.close().await;
        Ok(())
    }

    #[derive(Clone, Copy)]
    struct X11Binding {
        action: GlobalShortcutAction,
        keycode: c_uint,
        modifiers: c_uint,
    }

    fn run_x11(
        app_state: &Arc<Mutex<AppState>>,
        stop: &AtomicBool,
        generation: u64,
        specs: &[GlobalShortcutSpec],
    ) -> Result<(), String> {
        if std::env::var_os("DISPLAY").is_none() {
            return Err("DISPLAY is not set".to_string());
        }
        let x11 = X11::load()?;
        let display = unsafe { (x11.open_display)(ptr::null()) };
        if display.is_null() {
            return Err("XOpenDisplay failed".to_string());
        }
        let root = unsafe { (x11.default_root_window)(display) };
        let bindings_result = specs
            .iter()
            .map(|spec| x11_binding(&x11, display, spec))
            .collect::<Result<Vec<_>, _>>();
        let bindings = match bindings_result {
            Ok(bindings) => bindings,
            Err(error) => {
                unsafe { (x11.close_display)(display) };
                return Err(error);
            }
        };

        X11_GRAB_ERROR.store(false, Ordering::Release);
        let previous_handler = unsafe { (x11.set_error_handler)(Some(x11_error_handler)) };
        for binding in &bindings {
            for lock_mask in [0, LOCK_MASK, MOD2_MASK, LOCK_MASK | MOD2_MASK] {
                unsafe {
                    (x11.grab_key)(
                        display,
                        binding.keycode as c_int,
                        binding.modifiers | lock_mask,
                        root,
                        0,
                        GRAB_MODE_ASYNC,
                        GRAB_MODE_ASYNC,
                    );
                }
            }
        }
        unsafe { (x11.sync)(display, 0) };
        unsafe { (x11.set_error_handler)(previous_handler) };
        if X11_GRAB_ERROR.load(Ordering::Acquire) {
            unsafe {
                (x11.ungrab_key)(display, ANY_KEY, ANY_MODIFIER, root);
                (x11.close_display)(display);
            }
            return Err("one or more shortcuts are already registered".to_string());
        }
        update_status(app_state, "x11", "registered", None, specs);

        while !stop.load(Ordering::Acquire) && !registration_changed(app_state, generation, specs) {
            while unsafe { (x11.pending)(display) } > 0 {
                let mut event = XEvent { pad: [0; 24] };
                unsafe { (x11.next_event)(display, &mut event) };
                let event_type = unsafe { event.type_ };
                if event_type != KEY_PRESS {
                    continue;
                }
                let key = unsafe { event.key };
                let modifiers = key.state & !(LOCK_MASK | MOD2_MASK);
                if let Some(binding) = bindings.iter().find(|binding| {
                    binding.keycode == key.keycode && binding.modifiers == modifiers
                }) {
                    activate(app_state, binding.action);
                }
            }
            thread::sleep(Duration::from_millis(20));
        }
        unsafe {
            (x11.ungrab_key)(display, ANY_KEY, ANY_MODIFIER, root);
            (x11.sync)(display, 0);
            (x11.close_display)(display);
        }
        Ok(())
    }

    fn x11_binding(
        x11: &X11,
        display: *mut c_void,
        spec: &GlobalShortcutSpec,
    ) -> Result<X11Binding, String> {
        let mut modifiers = 0;
        let mut key_name = None;
        for part in spec
            .combo
            .split('+')
            .map(str::trim)
            .filter(|part| !part.is_empty())
        {
            let normalized = part.to_ascii_lowercase();
            match normalized.as_str() {
                "cmd" | "super" | "logo" => modifiers |= MOD4_MASK,
                "ctrl" | "control" => modifiers |= CONTROL_MASK,
                "opt" | "alt" | "option" => modifiers |= MOD1_MASK,
                "shift" => modifiers |= SHIFT_MASK,
                raw if key_name.is_none() => key_name = Some(x11_key_name(raw).to_string()),
                _ => return Err(format!("invalid global shortcut {}", spec.combo)),
            }
        }
        let key_name = CString::new(
            key_name.ok_or_else(|| format!("global shortcut {} has no key", spec.combo))?,
        )
        .map_err(|error| error.to_string())?;
        let keysym = unsafe { (x11.string_to_keysym)(key_name.as_ptr()) };
        if keysym == 0 {
            return Err(format!("unknown X11 key in {}", spec.combo));
        }
        let keycode = unsafe { (x11.keysym_to_keycode)(display, keysym) } as c_uint;
        if keycode == 0 {
            return Err(format!("unmapped X11 key in {}", spec.combo));
        }
        Ok(X11Binding {
            action: spec.action,
            keycode,
            modifiers,
        })
    }

    fn x11_key_name(key: &str) -> &str {
        match key {
            "." => "period",
            "," => "comma",
            "[" => "bracketleft",
            "]" => "bracketright",
            "=" => "equal",
            "-" => "minus",
            "enter" | "return" => "Return",
            "escape" | "esc" => "Escape",
            "space" => "space",
            "left" => "Left",
            "right" => "Right",
            "up" => "Up",
            "down" => "Down",
            raw => raw,
        }
    }

    type Display = c_void;
    type Window = c_ulong;
    type KeySym = c_ulong;
    type XErrorHandler = Option<unsafe extern "C" fn(*mut Display, *mut XErrorEvent) -> c_int>;

    #[repr(C)]
    #[derive(Clone, Copy)]
    struct XKeyEvent {
        type_: c_int,
        serial: c_ulong,
        send_event: c_int,
        display: *mut Display,
        window: Window,
        root: Window,
        subwindow: Window,
        time: c_ulong,
        x: c_int,
        y: c_int,
        x_root: c_int,
        y_root: c_int,
        state: c_uint,
        keycode: c_uint,
        same_screen: c_int,
    }

    #[repr(C)]
    union XEvent {
        type_: c_int,
        key: XKeyEvent,
        pad: [c_long; 24],
    }

    #[repr(C)]
    struct XErrorEvent {
        type_: c_int,
        display: *mut Display,
        resourceid: c_ulong,
        serial: c_ulong,
        error_code: u8,
        request_code: u8,
        minor_code: u8,
    }

    static X11_GRAB_ERROR: AtomicBool = AtomicBool::new(false);

    unsafe extern "C" fn x11_error_handler(
        _display: *mut Display,
        event: *mut XErrorEvent,
    ) -> c_int {
        if !event.is_null() && unsafe { (*event).error_code } == BAD_ACCESS {
            X11_GRAB_ERROR.store(true, Ordering::Release);
        }
        0
    }

    struct X11 {
        _library: *mut c_void,
        open_display: unsafe extern "C" fn(*const c_char) -> *mut Display,
        close_display: unsafe extern "C" fn(*mut Display) -> c_int,
        default_root_window: unsafe extern "C" fn(*mut Display) -> Window,
        string_to_keysym: unsafe extern "C" fn(*const c_char) -> KeySym,
        keysym_to_keycode: unsafe extern "C" fn(*mut Display, KeySym) -> u8,
        grab_key: unsafe extern "C" fn(*mut Display, c_int, c_uint, Window, c_int, c_int, c_int),
        ungrab_key: unsafe extern "C" fn(*mut Display, c_int, c_uint, Window),
        pending: unsafe extern "C" fn(*mut Display) -> c_int,
        next_event: unsafe extern "C" fn(*mut Display, *mut XEvent) -> c_int,
        sync: unsafe extern "C" fn(*mut Display, c_int) -> c_int,
        set_error_handler: unsafe extern "C" fn(XErrorHandler) -> XErrorHandler,
    }

    impl X11 {
        fn load() -> Result<Self, String> {
            let library = ["libX11.so.6", "libX11.so"]
                .into_iter()
                .find_map(|name| {
                    let name = CString::new(name).ok()?;
                    let handle = unsafe { dlopen(name.as_ptr(), RTLD_NOW | RTLD_LOCAL) };
                    (!handle.is_null()).then_some(handle)
                })
                .ok_or_else(|| "libX11 could not be loaded".to_string())?;
            unsafe {
                Ok(Self {
                    _library: library,
                    open_display: symbol(library, "XOpenDisplay")?,
                    close_display: symbol(library, "XCloseDisplay")?,
                    default_root_window: symbol(library, "XDefaultRootWindow")?,
                    string_to_keysym: symbol(library, "XStringToKeysym")?,
                    keysym_to_keycode: symbol(library, "XKeysymToKeycode")?,
                    grab_key: symbol(library, "XGrabKey")?,
                    ungrab_key: symbol(library, "XUngrabKey")?,
                    pending: symbol(library, "XPending")?,
                    next_event: symbol(library, "XNextEvent")?,
                    sync: symbol(library, "XSync")?,
                    set_error_handler: symbol(library, "XSetErrorHandler")?,
                })
            }
        }
    }

    unsafe fn symbol<T: Copy>(library: *mut c_void, name: &str) -> Result<T, String> {
        let raw_name = CString::new(name).map_err(|error| error.to_string())?;
        let pointer = unsafe { dlsym(library, raw_name.as_ptr()) };
        if pointer.is_null() {
            Err(format!("libX11 is missing {name}"))
        } else {
            Ok(unsafe { mem::transmute_copy(&pointer) })
        }
    }

    const RTLD_NOW: c_int = 2;
    const RTLD_LOCAL: c_int = 0;
    const KEY_PRESS: c_int = 2;
    const BAD_ACCESS: u8 = 10;
    const GRAB_MODE_ASYNC: c_int = 1;
    const ANY_KEY: c_int = 0;
    const ANY_MODIFIER: c_uint = 1 << 15;
    const SHIFT_MASK: c_uint = 1 << 0;
    const LOCK_MASK: c_uint = 1 << 1;
    const CONTROL_MASK: c_uint = 1 << 2;
    const MOD1_MASK: c_uint = 1 << 3;
    const MOD2_MASK: c_uint = 1 << 4;
    const MOD4_MASK: c_uint = 1 << 6;

    unsafe extern "C" {
        fn dlopen(filename: *const c_char, flags: c_int) -> *mut c_void;
        fn dlsym(handle: *mut c_void, symbol: *const c_char) -> *mut c_void;
    }
}

#[cfg(feature = "gtk")]
pub(crate) use backend::GlobalShortcutManager;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn portal_triggers_use_xdg_modifier_and_key_names() {
        assert_eq!(portal_trigger("cmd+opt+f").as_deref(), Some("LOGO+ALT+f"));
        assert_eq!(
            portal_trigger("cmd+ctrl+opt+.").as_deref(),
            Some("LOGO+CTRL+ALT+period")
        );
        assert_eq!(
            portal_trigger("ctrl+shift+left").as_deref(),
            Some("CTRL+SHIFT+Left")
        );
        assert_eq!(portal_trigger("cmd+ctrl"), None);
    }
}
