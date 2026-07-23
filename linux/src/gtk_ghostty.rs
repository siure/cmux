use crate::app::{
    AppState, EmbeddedTerminalInheritedOptions, EmbeddedTerminalInput, EmbeddedTerminalSize,
};
use crate::config;
use crate::ghostty_embed::{
    ghostty_physical_keycode, ghostty_string_to_string, load_and_finalize_default_config,
    validate_surface_env_var_count, GhosttyAction, GhosttyActionKeyTable, GhosttyApp,
    GhosttyAppGuard, GhosttyAppTick, GhosttyAppUpdateConfig, GhosttyClipboardContent,
    GhosttyConfig, GhosttyConfigDiagnosticsCount, GhosttyConfigFinalize, GhosttyConfigFree,
    GhosttyConfigGetDiagnostic, GhosttyConfigGuard, GhosttyConfigLoadDefaultFiles,
    GhosttyConfigLoadRecursiveFiles, GhosttyConfigNew, GhosttyConfigOpenPath, GhosttyEnvVar,
    GhosttyImePoint, GhosttyInputKey, GhosttyInputTrigger, GhosttyInspectorGuard, GhosttyIoWriteCb,
    GhosttyLibrary, GhosttyLinuxGetProcAddress, GhosttyLinuxMakeCurrent, GhosttyPlatformLinux,
    GhosttyRuntimeCallbacks, GhosttyRuntimeConfig, GhosttyStringFree, GhosttySurface,
    GhosttySurfaceCompleteClipboardRequest, GhosttySurfaceGuard, GhosttySurfaceInheritedConfig,
    GhosttySurfaceInheritedConfigFree, GhosttySurfaceNeedsConfirmQuit, GhosttySurfaceUpdateConfig,
    GhosttyTarget, GHOSTTY_ACTION_CELL_SIZE, GHOSTTY_ACTION_CHECK_FOR_UPDATES,
    GHOSTTY_ACTION_CLOSE_ALL_WINDOWS, GHOSTTY_ACTION_CLOSE_TAB, GHOSTTY_ACTION_CLOSE_WINDOW,
    GHOSTTY_ACTION_COLOR_CHANGE, GHOSTTY_ACTION_COMMAND_FINISHED, GHOSTTY_ACTION_CONFIG_CHANGE,
    GHOSTTY_ACTION_COPY_TITLE_TO_CLIPBOARD, GHOSTTY_ACTION_DESKTOP_NOTIFICATION,
    GHOSTTY_ACTION_END_SEARCH, GHOSTTY_ACTION_EQUALIZE_SPLITS, GHOSTTY_ACTION_FLOAT_WINDOW,
    GHOSTTY_ACTION_GOTO_SPLIT, GHOSTTY_ACTION_GOTO_TAB, GHOSTTY_ACTION_GOTO_WINDOW,
    GHOSTTY_ACTION_INITIAL_SIZE, GHOSTTY_ACTION_INSPECTOR, GHOSTTY_ACTION_KEY_SEQUENCE,
    GHOSTTY_ACTION_KEY_TABLE, GHOSTTY_ACTION_MOUSE_OVER_LINK, GHOSTTY_ACTION_MOUSE_SHAPE,
    GHOSTTY_ACTION_MOUSE_VISIBILITY, GHOSTTY_ACTION_MOVE_TAB, GHOSTTY_ACTION_NEW_SPLIT,
    GHOSTTY_ACTION_NEW_TAB, GHOSTTY_ACTION_NEW_WINDOW, GHOSTTY_ACTION_OPEN_CONFIG,
    GHOSTTY_ACTION_OPEN_URL, GHOSTTY_ACTION_PRESENT_TERMINAL, GHOSTTY_ACTION_PRESS,
    GHOSTTY_ACTION_PROGRESS_REPORT, GHOSTTY_ACTION_PROMPT_TITLE, GHOSTTY_ACTION_PWD,
    GHOSTTY_ACTION_QUIT, GHOSTTY_ACTION_QUIT_TIMER, GHOSTTY_ACTION_READONLY, GHOSTTY_ACTION_REDO,
    GHOSTTY_ACTION_RELEASE, GHOSTTY_ACTION_RELOAD_CONFIG, GHOSTTY_ACTION_RENDER,
    GHOSTTY_ACTION_RENDERER_HEALTH, GHOSTTY_ACTION_RENDER_INSPECTOR,
    GHOSTTY_ACTION_RESET_WINDOW_SIZE, GHOSTTY_ACTION_RESIZE_SPLIT, GHOSTTY_ACTION_RING_BELL,
    GHOSTTY_ACTION_SCROLLBAR, GHOSTTY_ACTION_SEARCH_SELECTED, GHOSTTY_ACTION_SEARCH_TOTAL,
    GHOSTTY_ACTION_SECURE_INPUT, GHOSTTY_ACTION_SELECTION_CHANGED, GHOSTTY_ACTION_SET_TAB_TITLE,
    GHOSTTY_ACTION_SET_TITLE, GHOSTTY_ACTION_SHOW_CHILD_EXITED, GHOSTTY_ACTION_SHOW_GTK_INSPECTOR,
    GHOSTTY_ACTION_SHOW_ON_SCREEN_KEYBOARD, GHOSTTY_ACTION_SIZE_LIMIT, GHOSTTY_ACTION_START_SEARCH,
    GHOSTTY_ACTION_TOGGLE_BACKGROUND_OPACITY, GHOSTTY_ACTION_TOGGLE_COMMAND_PALETTE,
    GHOSTTY_ACTION_TOGGLE_FULLSCREEN, GHOSTTY_ACTION_TOGGLE_MAXIMIZE,
    GHOSTTY_ACTION_TOGGLE_QUICK_TERMINAL, GHOSTTY_ACTION_TOGGLE_SPLIT_ZOOM,
    GHOSTTY_ACTION_TOGGLE_TAB_OVERVIEW, GHOSTTY_ACTION_TOGGLE_VISIBILITY,
    GHOSTTY_ACTION_TOGGLE_WINDOW_DECORATIONS, GHOSTTY_ACTION_UNDO, GHOSTTY_BINDING_FLAGS_ALL,
    GHOSTTY_BINDING_FLAGS_CONSUMED, GHOSTTY_BINDING_FLAGS_GLOBAL, GHOSTTY_CLIPBOARD_PRIMARY,
    GHOSTTY_CLIPBOARD_REQUEST_OSC_52_READ, GHOSTTY_CLIPBOARD_REQUEST_OSC_52_WRITE,
    GHOSTTY_CLIPBOARD_REQUEST_PASTE, GHOSTTY_CLIPBOARD_SELECTION, GHOSTTY_CLIPBOARD_STANDARD,
    GHOSTTY_CLOSE_TAB_MODE_OTHER, GHOSTTY_CLOSE_TAB_MODE_RIGHT, GHOSTTY_CLOSE_TAB_MODE_THIS,
    GHOSTTY_COLOR_KIND_BACKGROUND, GHOSTTY_COLOR_KIND_CURSOR, GHOSTTY_COLOR_KIND_FOREGROUND,
    GHOSTTY_COLOR_SCHEME_DARK, GHOSTTY_COLOR_SCHEME_LIGHT, GHOSTTY_FLOAT_WINDOW_OFF,
    GHOSTTY_FLOAT_WINDOW_ON, GHOSTTY_FLOAT_WINDOW_TOGGLE, GHOSTTY_FULLSCREEN_MACOS_NON_NATIVE,
    GHOSTTY_FULLSCREEN_MACOS_NON_NATIVE_PADDED_NOTCH,
    GHOSTTY_FULLSCREEN_MACOS_NON_NATIVE_VISIBLE_MENU, GHOSTTY_FULLSCREEN_NATIVE,
    GHOSTTY_GOTO_SPLIT_DOWN, GHOSTTY_GOTO_SPLIT_LEFT, GHOSTTY_GOTO_SPLIT_NEXT,
    GHOSTTY_GOTO_SPLIT_PREVIOUS, GHOSTTY_GOTO_SPLIT_RIGHT, GHOSTTY_GOTO_SPLIT_UP,
    GHOSTTY_GOTO_WINDOW_NEXT, GHOSTTY_GOTO_WINDOW_PREVIOUS, GHOSTTY_INSPECTOR_HIDE,
    GHOSTTY_INSPECTOR_SHOW, GHOSTTY_INSPECTOR_TOGGLE, GHOSTTY_KEY_ARROW_DOWN,
    GHOSTTY_KEY_ARROW_LEFT, GHOSTTY_KEY_ARROW_RIGHT, GHOSTTY_KEY_ARROW_UP, GHOSTTY_KEY_BACKSPACE,
    GHOSTTY_KEY_DELETE, GHOSTTY_KEY_END, GHOSTTY_KEY_ENTER, GHOSTTY_KEY_ESCAPE, GHOSTTY_KEY_F1,
    GHOSTTY_KEY_F10, GHOSTTY_KEY_F11, GHOSTTY_KEY_F12, GHOSTTY_KEY_F13, GHOSTTY_KEY_F14,
    GHOSTTY_KEY_F15, GHOSTTY_KEY_F16, GHOSTTY_KEY_F17, GHOSTTY_KEY_F18, GHOSTTY_KEY_F19,
    GHOSTTY_KEY_F2, GHOSTTY_KEY_F20, GHOSTTY_KEY_F21, GHOSTTY_KEY_F22, GHOSTTY_KEY_F23,
    GHOSTTY_KEY_F24, GHOSTTY_KEY_F25, GHOSTTY_KEY_F3, GHOSTTY_KEY_F4, GHOSTTY_KEY_F5,
    GHOSTTY_KEY_F6, GHOSTTY_KEY_F7, GHOSTTY_KEY_F8, GHOSTTY_KEY_F9, GHOSTTY_KEY_HOME,
    GHOSTTY_KEY_INSERT, GHOSTTY_KEY_PAGE_DOWN, GHOSTTY_KEY_PAGE_UP, GHOSTTY_KEY_PAUSE,
    GHOSTTY_KEY_PRINT_SCREEN, GHOSTTY_KEY_SCROLL_LOCK, GHOSTTY_KEY_SPACE, GHOSTTY_KEY_TAB,
    GHOSTTY_KEY_TABLE_ACTIVATE, GHOSTTY_KEY_TABLE_DEACTIVATE, GHOSTTY_KEY_TABLE_DEACTIVATE_ALL,
    GHOSTTY_MODS_ALT, GHOSTTY_MODS_ALT_RIGHT, GHOSTTY_MODS_CAPS, GHOSTTY_MODS_CTRL,
    GHOSTTY_MODS_CTRL_RIGHT, GHOSTTY_MODS_NUM, GHOSTTY_MODS_SHIFT, GHOSTTY_MODS_SHIFT_RIGHT,
    GHOSTTY_MODS_SUPER, GHOSTTY_MODS_SUPER_RIGHT, GHOSTTY_MOUSE_BUTTON_EIGHT,
    GHOSTTY_MOUSE_BUTTON_ELEVEN, GHOSTTY_MOUSE_BUTTON_FIVE, GHOSTTY_MOUSE_BUTTON_FOUR,
    GHOSTTY_MOUSE_BUTTON_LEFT, GHOSTTY_MOUSE_BUTTON_MIDDLE, GHOSTTY_MOUSE_BUTTON_NINE,
    GHOSTTY_MOUSE_BUTTON_RIGHT, GHOSTTY_MOUSE_BUTTON_SEVEN, GHOSTTY_MOUSE_BUTTON_SIX,
    GHOSTTY_MOUSE_BUTTON_TEN, GHOSTTY_MOUSE_BUTTON_UNKNOWN, GHOSTTY_MOUSE_HIDDEN,
    GHOSTTY_MOUSE_PRESS, GHOSTTY_MOUSE_PRESSURE_DEEP, GHOSTTY_MOUSE_PRESSURE_NONE,
    GHOSTTY_MOUSE_PRESSURE_NORMAL, GHOSTTY_MOUSE_RELEASE, GHOSTTY_MOUSE_SHAPE_ALIAS,
    GHOSTTY_MOUSE_SHAPE_ALL_SCROLL, GHOSTTY_MOUSE_SHAPE_CELL, GHOSTTY_MOUSE_SHAPE_COL_RESIZE,
    GHOSTTY_MOUSE_SHAPE_CONTEXT_MENU, GHOSTTY_MOUSE_SHAPE_COPY, GHOSTTY_MOUSE_SHAPE_CROSSHAIR,
    GHOSTTY_MOUSE_SHAPE_DEFAULT, GHOSTTY_MOUSE_SHAPE_EW_RESIZE, GHOSTTY_MOUSE_SHAPE_E_RESIZE,
    GHOSTTY_MOUSE_SHAPE_GRAB, GHOSTTY_MOUSE_SHAPE_GRABBING, GHOSTTY_MOUSE_SHAPE_HELP,
    GHOSTTY_MOUSE_SHAPE_MOVE, GHOSTTY_MOUSE_SHAPE_NESW_RESIZE, GHOSTTY_MOUSE_SHAPE_NE_RESIZE,
    GHOSTTY_MOUSE_SHAPE_NOT_ALLOWED, GHOSTTY_MOUSE_SHAPE_NO_DROP, GHOSTTY_MOUSE_SHAPE_NS_RESIZE,
    GHOSTTY_MOUSE_SHAPE_NWSE_RESIZE, GHOSTTY_MOUSE_SHAPE_NW_RESIZE, GHOSTTY_MOUSE_SHAPE_N_RESIZE,
    GHOSTTY_MOUSE_SHAPE_POINTER, GHOSTTY_MOUSE_SHAPE_PROGRESS, GHOSTTY_MOUSE_SHAPE_ROW_RESIZE,
    GHOSTTY_MOUSE_SHAPE_SE_RESIZE, GHOSTTY_MOUSE_SHAPE_SW_RESIZE, GHOSTTY_MOUSE_SHAPE_S_RESIZE,
    GHOSTTY_MOUSE_SHAPE_TEXT, GHOSTTY_MOUSE_SHAPE_VERTICAL_TEXT, GHOSTTY_MOUSE_SHAPE_WAIT,
    GHOSTTY_MOUSE_SHAPE_W_RESIZE, GHOSTTY_MOUSE_SHAPE_ZOOM_IN, GHOSTTY_MOUSE_SHAPE_ZOOM_OUT,
    GHOSTTY_MOUSE_VISIBLE, GHOSTTY_PROGRESS_STATE_ERROR, GHOSTTY_PROGRESS_STATE_INDETERMINATE,
    GHOSTTY_PROGRESS_STATE_PAUSE, GHOSTTY_PROGRESS_STATE_REMOVE, GHOSTTY_PROGRESS_STATE_SET,
    GHOSTTY_PROMPT_TITLE_SURFACE, GHOSTTY_PROMPT_TITLE_TAB, GHOSTTY_QUIT_TIMER_START,
    GHOSTTY_QUIT_TIMER_STOP, GHOSTTY_READONLY_OFF, GHOSTTY_READONLY_ON,
    GHOSTTY_RENDERER_HEALTH_HEALTHY, GHOSTTY_RENDERER_HEALTH_UNHEALTHY, GHOSTTY_RESIZE_SPLIT_DOWN,
    GHOSTTY_RESIZE_SPLIT_LEFT, GHOSTTY_RESIZE_SPLIT_RIGHT, GHOSTTY_RESIZE_SPLIT_UP,
    GHOSTTY_SECURE_INPUT_OFF, GHOSTTY_SECURE_INPUT_ON, GHOSTTY_SECURE_INPUT_TOGGLE,
    GHOSTTY_SPLIT_DIRECTION_DOWN, GHOSTTY_SPLIT_DIRECTION_LEFT, GHOSTTY_SPLIT_DIRECTION_RIGHT,
    GHOSTTY_SPLIT_DIRECTION_UP, GHOSTTY_SURFACE_CONTEXT_SPLIT, GHOSTTY_SURFACE_CONTEXT_TAB,
    GHOSTTY_SURFACE_CONTEXT_WINDOW, GHOSTTY_TARGET_APP, GHOSTTY_TRIGGER_CATCH_ALL,
    GHOSTTY_TRIGGER_PHYSICAL, GHOSTTY_TRIGGER_UNICODE,
};
use crate::linux_update;
use crate::terminal::terminal_key_bytes;
use crate::terminal_copy_mode::{
    CopyModeAction, CopyModeCursor, CopyModeInputState, CopyModeKey, CopyModeModifiers,
    CopyModeMove, CopyModeResolution,
};
use anyhow::{anyhow, Error, Result};
use gtk::gdk;
use gtk::gio;
use gtk::glib;
use gtk::glib::translate::{from_glib, FromGlibPtrContainer, ToGlibPtr};
use gtk::prelude::*;
use gtk4 as gtk;
use serde_json::{json, Value};
use std::cell::{Cell, RefCell};
use std::collections::{HashMap, HashSet};
use std::ffi::{c_char, c_int, c_void, CStr, CString};
use std::path::{Path, PathBuf};
use std::ptr;
use std::rc::{Rc, Weak};
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::sync::OnceLock;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

const RTLD_NOW: i32 = 2;
const GHOSTTY_TEXT_SYNC_INTERVAL: Duration = Duration::from_millis(250);
const GHOSTTY_SCROLLBACK_SYNC_INTERVAL: Duration = Duration::from_secs(5);
const GHOSTTY_SCROLLBACK_SYNC_MAX_BYTES: usize = 262_144;
const GHOSTTY_RESIZE_SETTLE_INTERVAL: Duration = Duration::from_millis(50);
const GHOSTTY_FOCUS_RETRY_ATTEMPTS: u8 = 8;
const GHOSTTY_RENDERER_ACTIVE_STATUS: &str = "Ghostty renderer active";
const GHOSTTY_SCROLL_MOD_PRECISION: c_int = 1;
const GHOSTTY_STYLUS_NORMAL_PRESSURE: f64 = 0.5;
const GHOSTTY_STYLUS_DEEP_PRESSURE_THRESHOLD: f64 = 0.75;
static NEXT_GHOSTTY_CALLBACK_TOKEN: AtomicU64 = AtomicU64::new(1);

thread_local! {
    static SHARED_GHOSTTY_APP: RefCell<Weak<GtkGhosttyApp>> = RefCell::new(Weak::new());
}

#[link(name = "dl")]
extern "C" {
    fn dlopen(filename: *const c_char, flags: i32) -> *mut c_void;
    fn dlsym(handle: *mut c_void, symbol: *const c_char) -> *mut c_void;
}

type GtkGlGetProcAddress = unsafe extern "C" fn(*const c_char) -> *mut c_void;
type GdkFileListGetType = unsafe extern "C" fn() -> glib::ffi::GType;
type GdkFileListGetFiles = unsafe extern "C" fn(*mut c_void) -> *mut glib::ffi::GSList;

#[derive(Clone)]
pub struct GhosttySurfaceOptions {
    pub working_directory: Option<String>,
    pub command: Option<String>,
    pub initial_input: Option<String>,
    pub initial_output: Option<String>,
    pub font_size: Option<f32>,
    pub wait_after_command: bool,
    pub env: Vec<(String, String)>,
    pub manual_io: bool,
    pub focused: bool,
    pub occluded: bool,
    pub copy_mode_active: bool,
    pub show_scroll_bar: bool,
    pub scrollbar: Option<GhosttyScrollbarState>,
    pub config_reload_generation: u64,
    pub close_surface_id: Option<String>,
    pub app_state: Option<Arc<Mutex<AppState>>>,
}

impl Default for GhosttySurfaceOptions {
    fn default() -> Self {
        Self {
            working_directory: None,
            command: None,
            initial_input: None,
            initial_output: None,
            font_size: None,
            wait_after_command: false,
            env: Vec::new(),
            manual_io: false,
            focused: false,
            occluded: false,
            copy_mode_active: false,
            show_scroll_bar: true,
            scrollbar: None,
            config_reload_generation: 0,
            close_surface_id: None,
            app_state: None,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct GhosttyScrollbarState {
    pub total: u64,
    pub offset: u64,
    pub len: u64,
}

#[derive(Clone)]
pub struct GhosttySurfaceWidget {
    root: gtk::Box,
    area: gtk::GLArea,
    scrollbar: gtk::Scrollbar,
    scrollbar_adjustment: gtk::Adjustment,
    scrollbar_syncing: Rc<Cell<bool>>,
    model_focused: Rc<Cell<bool>>,
    focus_retry_active: Rc<Cell<bool>>,
    host: Option<Rc<RefCell<GtkGhosttyHost>>>,
}

impl GhosttySurfaceWidget {
    pub fn root(&self) -> &gtk::Box {
        &self.root
    }

    pub fn update_options(&self, options: GhosttySurfaceOptions) {
        if let Some(host) = self.host.as_ref() {
            host.borrow_mut().update_options(options.clone());
            self.sync_scrollbar(
                options.show_scroll_bar && host.borrow().app_context.scrollbar_allowed(),
                options.scrollbar,
            );
        }
    }

    fn sync_scrollbar(&self, allowed: bool, state: Option<GhosttyScrollbarState>) {
        self.scrollbar
            .set_visible(ghostty_scrollbar_visible(allowed, state));
        let Some(state) = state else {
            return;
        };
        let total = state.total.max(state.len).max(1) as f64;
        let page_size = state.len.max(1).min(state.total.max(1)) as f64;
        let offset = state.offset.min(state.total.saturating_sub(state.len)) as f64;
        self.scrollbar_syncing.set(true);
        self.scrollbar_adjustment
            .configure(offset, 0.0, total, 1.0, page_size, page_size);
        self.scrollbar_syncing.set(false);
    }

    pub fn update_presentation(&self, focused: bool, occluded: bool) {
        self.model_focused.set(focused);
        if let Some(host) = self.host.as_ref() {
            host.borrow_mut().update_presentation(focused, occluded);
        }
        if focused && !self.area.has_focus() {
            request_ghostty_area_focus(&self.area, &self.model_focused, &self.focus_retry_active);
        }
    }

    pub fn perform_binding_action(&self, action: &str) -> bool {
        self.host
            .as_ref()
            .and_then(|host| {
                host.borrow()
                    .surface
                    .as_ref()
                    .and_then(|surface| surface.binding_action(action).ok())
            })
            .unwrap_or(false)
    }

    pub fn grab_focus(&self) {
        self.area.grab_focus();
    }

    pub fn input_ready(&self) -> bool {
        self.host
            .as_ref()
            .is_some_and(|host| host.borrow().surface.is_some())
    }

    pub fn sync_scrollback_snapshot(&self) -> bool {
        self.host
            .as_ref()
            .is_some_and(|host| host.borrow_mut().sync_scrollback_snapshot())
    }

    pub fn shutdown(&self) {
        if let Some(host) = self.host.as_ref() {
            host.borrow_mut().release_surface();
        }
    }

    pub fn copy_mode_active(&self) -> bool {
        self.host
            .as_ref()
            .is_some_and(|host| host.borrow().copy_mode.active)
    }

    pub fn set_keyboard_copy_mode_active(&self, active: bool) -> bool {
        self.host.as_ref().is_some_and(|host| {
            host.borrow_mut()
                .set_keyboard_copy_mode_active(active, false)
        })
    }

    pub fn handle_keyboard_copy_mode_key(
        &self,
        keyval: gdk::Key,
        keycode: u32,
        modifiers: gdk::ModifierType,
    ) -> bool {
        self.host.as_ref().is_some_and(|host| {
            host.borrow_mut()
                .handle_keyboard_copy_mode_key(keyval, keycode, modifiers)
        })
    }

    pub fn contains_widget(&self, widget: &gtk::Widget) -> bool {
        let root = self.root.clone().upcast::<gtk::Widget>();
        let mut current = Some(widget.clone());
        while let Some(widget) = current {
            if widget == root {
                return true;
            }
            current = widget.parent();
        }
        false
    }
}

fn request_ghostty_area_focus(
    area: &gtk::GLArea,
    model_focused: &Rc<Cell<bool>>,
    retry_active: &Rc<Cell<bool>>,
) {
    if !model_focused.get() {
        return;
    }
    area.grab_focus();
    if area.has_focus() {
        retry_active.set(false);
        return;
    }
    if retry_active.replace(true) {
        return;
    }

    let idle_area = area.clone();
    let idle_focused = Rc::clone(model_focused);
    glib::idle_add_local_once(move || {
        if idle_focused.get() && !idle_area.has_focus() {
            idle_area.grab_focus();
        }
    });

    let area = area.clone();
    let still_focused = Rc::clone(model_focused);
    let retry_active = Rc::clone(retry_active);
    let attempts = Rc::new(Cell::new(0_u8));
    glib::timeout_add_local(Duration::from_millis(16), move || {
        if !still_focused.get() || area.has_focus() {
            retry_active.set(false);
            return glib::ControlFlow::Break;
        }
        let attempt = attempts.get().saturating_add(1);
        attempts.set(attempt);
        area.grab_focus();
        if attempt >= GHOSTTY_FOCUS_RETRY_ATTEMPTS {
            retry_active.set(false);
            glib::ControlFlow::Break
        } else {
            glib::ControlFlow::Continue
        }
    });
}

fn ghostty_scrollbar_visible(allowed: bool, state: Option<GhosttyScrollbarState>) -> bool {
    allowed && state.is_none_or(|state| state.total > state.len)
}

fn ghostty_status_visible(status: &str) -> bool {
    status != GHOSTTY_RENDERER_ACTIVE_STATUS
}

pub fn ghostty_surface_widget(options: GhosttySurfaceOptions) -> GhosttySurfaceWidget {
    let root = gtk::Box::new(gtk::Orientation::Vertical, 6);
    root.set_hexpand(true);
    root.set_vexpand(true);

    let status = gtk::Label::new(Some("Initializing Ghostty renderer"));
    status.set_xalign(0.0);
    status.add_css_class("cmux-muted");
    status.connect_label_notify(|status| {
        status.set_visible(ghostty_status_visible(&status.label()));
    });
    root.append(&status);

    let area = gtk::GLArea::builder()
        .hexpand(true)
        .vexpand(true)
        .auto_render(false)
        .use_es(false)
        .build();
    area.set_required_version(4, 3);
    area.set_has_depth_buffer(false);
    area.set_focusable(true);
    area.set_cursor_from_name(Some(GhosttyCursorState::default().cursor_name()));
    let overlay = gtk::Overlay::new();
    overlay.set_hexpand(true);
    overlay.set_vexpand(true);
    overlay.set_child(Some(&area));

    let scrollbar_adjustment = gtk::Adjustment::new(0.0, 0.0, 1.0, 1.0, 1.0, 1.0);
    let scrollbar = gtk::Scrollbar::new(gtk::Orientation::Vertical, Some(&scrollbar_adjustment));
    scrollbar.add_css_class("cmux-terminal-scrollbar");
    scrollbar.set_halign(gtk::Align::End);
    scrollbar.set_valign(gtk::Align::Fill);
    scrollbar.set_visible(false);
    overlay.add_overlay(&scrollbar);
    let scrollbar_syncing = Rc::new(Cell::new(false));
    let model_focused = Rc::new(Cell::new(false));
    let focus_retry_active = Rc::new(Cell::new(false));
    let map_focused = Rc::clone(&model_focused);
    let map_retry_active = Rc::clone(&focus_retry_active);
    area.connect_map(move |area| {
        if map_focused.get() {
            request_ghostty_area_focus(area, &map_focused, &map_retry_active);
        }
    });

    let copy_cursor = gtk::Frame::new(None);
    copy_cursor.add_css_class("cmux-terminal-copy-cursor");
    copy_cursor.set_halign(gtk::Align::Start);
    copy_cursor.set_valign(gtk::Align::Start);
    copy_cursor.set_can_target(false);
    copy_cursor.set_visible(false);
    overlay.add_overlay(&copy_cursor);

    let copy_badge = gtk::Label::new(Some("vim"));
    copy_badge.add_css_class("cmux-terminal-copy-badge");
    copy_badge.set_halign(gtk::Align::End);
    copy_badge.set_valign(gtk::Align::Start);
    copy_badge.set_can_target(false);
    copy_badge.set_visible(false);
    overlay.add_overlay(&copy_badge);
    root.append(&overlay);

    let host = match GtkGhosttyHost::new(options.clone(), copy_cursor, copy_badge) {
        Ok(host) => Some(Rc::new(RefCell::new(host))),
        Err(err) => {
            eprintln!("cmux: Ghostty app initialization failed: {err:#}");
            status.set_text(&format!("Ghostty renderer unavailable: {err}"));
            None
        }
    };

    if let Some(host) = host.as_ref() {
        connect_ghostty_area(&area, &status, Rc::clone(host));
        let scroll_host = Rc::clone(host);
        let syncing = Rc::clone(&scrollbar_syncing);
        scrollbar_adjustment.connect_value_changed(move |adjustment| {
            if syncing.get() {
                return;
            }
            let row = adjustment.value().round().max(0.0) as u64;
            let _ =
                scroll_host.borrow().surface.as_ref().and_then(|surface| {
                    surface.binding_action(&format!("scroll_to_row:{row}")).ok()
                });
        });
    }

    let widget = GhosttySurfaceWidget {
        root,
        area,
        scrollbar,
        scrollbar_adjustment,
        scrollbar_syncing,
        model_focused,
        focus_retry_active,
        host,
    };
    widget.update_options(options);
    widget
}

fn focus_embedded_terminal_surface(app_state: &Arc<Mutex<AppState>>, surface_id: &str) -> bool {
    let Ok(mut app) = app_state.lock() else {
        return false;
    };
    if app
        .handle("surface.focus", &json!({"surface_id": surface_id}))
        .is_err()
    {
        return false;
    }
    let _ = app.set_embedded_terminal_widget_focused(surface_id, true);
    let _ = app.handle(
        "terminal.textbox.set_focus",
        &json!({"surface_id": surface_id, "focus": "terminal"}),
    );
    true
}

fn connect_ghostty_area(
    area: &gtk::GLArea,
    status: &gtk::Label,
    host: Rc<RefCell<GtkGhosttyHost>>,
) {
    let realize_host = Rc::clone(&host);
    let realize_status = status.clone();
    area.connect_realize(move |area| {
        if gtk_ghostty_allocated_surface_size(area).is_none() {
            return;
        }
        realize_ghostty_area(area, &realize_status, &realize_host);
    });

    let resize_host = Rc::clone(&host);
    let resize_status = status.clone();
    let resize_generation = Rc::new(Cell::new(0_u64));
    area.connect_resize(move |area, _, _| {
        let generation = resize_generation.get().wrapping_add(1);
        resize_generation.set(generation);

        let pending_generation = Rc::clone(&resize_generation);
        let resize_host = Rc::clone(&resize_host);
        let resize_status = resize_status.clone();
        let area = area.downgrade();
        glib::timeout_add_local_once(GHOSTTY_RESIZE_SETTLE_INTERVAL, move || {
            if pending_generation.get() != generation {
                return;
            }
            let Some(area) = area.upgrade() else {
                return;
            };
            let width = area.allocated_width();
            let height = area.allocated_height();
            if !area.is_mapped() || width <= 1 || height <= 1 {
                return;
            }
            if resize_host.borrow().callbacks.area.is_none() {
                realize_ghostty_area(&area, &resize_status, &resize_host);
            } else {
                resize_host
                    .borrow_mut()
                    .resize(&area, width, height, &resize_status);
            }
        });
    });
    let scale_host = Rc::clone(&host);
    let scale_status = status.clone();
    area.connect_scale_factor_notify(move |area| {
        if !scale_host.borrow().update_content_scale(area) {
            scale_status.set_text("Ghostty renderer scale update failed");
        }
    });

    let root_host = Rc::clone(&host);
    let root_status = status.clone();
    area.connect_root_notify(move |area| {
        if !root_host
            .borrow_mut()
            .connect_keyboard_changed(area, Some(&root_status))
        {
            root_status.set_text("Ghostty keyboard map reload failed");
        }
    });

    let render_host = Rc::clone(&host);
    let render_status = status.clone();
    area.connect_render(move |area, _| {
        render_host.borrow_mut().render(area, &render_status);
        glib::Propagation::Stop
    });

    connect_ghostty_im_context(&host);

    let key_host = Rc::clone(&host);
    let key_status = status.clone();
    let key_controller = gtk::EventControllerKey::new();
    key_controller.connect_key_pressed(move |controller, keyval, keycode, modifiers| {
        if key_host.borrow_mut().key_event(
            GHOSTTY_ACTION_PRESS,
            controller,
            keyval,
            keycode,
            modifiers,
            &key_status,
        ) {
            glib::Propagation::Stop
        } else {
            glib::Propagation::Proceed
        }
    });
    let key_release_host = Rc::clone(&host);
    let key_release_status = status.clone();
    key_controller.connect_key_released(move |controller, keyval, keycode, modifiers| {
        key_release_host.borrow_mut().key_event(
            GHOSTTY_ACTION_RELEASE,
            controller,
            keyval,
            keycode,
            modifiers,
            &key_release_status,
        );
    });
    area.add_controller(key_controller);

    let focus_host = Rc::clone(&host);
    let focus_status = status.clone();
    let focus_controller = gtk::EventControllerFocus::new();
    focus_controller.connect_enter(move |_| {
        let (focused, app_state, surface_id) = {
            let host = focus_host.borrow();
            (
                host.set_focus(true),
                host.options.app_state.clone(),
                host.options.close_surface_id.clone(),
            )
        };
        if !focused {
            focus_status.set_text("Ghostty renderer focus update failed");
        }
        if let (Some(app_state), Some(surface_id)) = (app_state, surface_id) {
            focus_embedded_terminal_surface(&app_state, &surface_id);
        }
    });
    let blur_host = Rc::clone(&host);
    let blur_status = status.clone();
    focus_controller.connect_leave(move |_| {
        let (focused, app_state, surface_id) = {
            let host = blur_host.borrow();
            (
                host.set_focus(false),
                host.options.app_state.clone(),
                host.options.close_surface_id.clone(),
            )
        };
        if !focused {
            blur_status.set_text("Ghostty renderer focus update failed");
        }
        if let (Some(app_state), Some(surface_id)) = (app_state, surface_id) {
            if let Ok(mut app) = app_state.lock() {
                let _ = app.set_embedded_terminal_widget_focused(&surface_id, false);
            }
        }
    });
    area.add_controller(focus_controller);

    let motion_host = Rc::clone(&host);
    let motion_status = status.clone();
    let motion_controller = gtk::EventControllerMotion::new();
    motion_controller.connect_motion(move |controller, x, y| {
        let content_scale = gtk_ghostty_controller_scale(controller);
        if !motion_host
            .borrow()
            .mouse_pos(x, y, controller.current_event_state(), content_scale)
        {
            motion_status.set_text("Ghostty pointer input failed");
        }
    });
    let leave_host = Rc::clone(&host);
    let leave_status = status.clone();
    motion_controller.connect_leave(move |controller| {
        if !leave_host
            .borrow()
            .mouse_pos(-1.0, -1.0, controller.current_event_state(), 1.0)
        {
            leave_status.set_text("Ghostty pointer input failed");
        }
    });
    area.add_controller(motion_controller);

    let click_host = Rc::clone(&host);
    let click = gtk::GestureClick::new();
    click.set_button(0);
    click.connect_pressed(move |gesture, _, x, y| {
        if let Some(widget) = gesture.widget() {
            widget.grab_focus();
        }
        let (app_state, surface_id) = {
            let host = click_host.borrow();
            (
                host.options.app_state.clone(),
                host.options.close_surface_id.clone(),
            )
        };
        if let (Some(app_state), Some(surface_id)) = (app_state, surface_id) {
            focus_embedded_terminal_surface(&app_state, &surface_id);
        }
        let content_scale = gtk_ghostty_controller_scale(gesture);
        if click_host.borrow().mouse_button(
            GHOSTTY_MOUSE_PRESS,
            gesture.current_button(),
            x,
            y,
            gesture.current_event_state(),
            content_scale,
        ) {
            gesture.set_state(gtk::EventSequenceState::Claimed);
        }
    });
    let release_host = Rc::clone(&host);
    click.connect_released(move |gesture, _, x, y| {
        let content_scale = gtk_ghostty_controller_scale(gesture);
        if release_host.borrow().mouse_button(
            GHOSTTY_MOUSE_RELEASE,
            gesture.current_button(),
            x,
            y,
            gesture.current_event_state(),
            content_scale,
        ) {
            gesture.set_state(gtk::EventSequenceState::Claimed);
        }
    });
    area.add_controller(click);

    let stylus = gtk::GestureStylus::new();
    let stylus_down_host = Rc::clone(&host);
    let stylus_down_status = status.clone();
    stylus.connect_down(move |stylus, x, y| {
        if let Some(widget) = stylus.widget() {
            widget.grab_focus();
        }
        let (app_state, surface_id) = {
            let host = stylus_down_host.borrow();
            (
                host.options.app_state.clone(),
                host.options.close_surface_id.clone(),
            )
        };
        if let (Some(app_state), Some(surface_id)) = (app_state, surface_id) {
            focus_embedded_terminal_surface(&app_state, &surface_id);
        }
        let content_scale = gtk_ghostty_controller_scale(stylus);
        if !stylus_down_host.borrow().stylus_pressure(
            x,
            y,
            ghostty_stylus_pressure(stylus, GHOSTTY_STYLUS_NORMAL_PRESSURE),
            stylus.current_event_state(),
            content_scale,
        ) {
            stylus_down_status.set_text("Ghostty stylus pressure input failed");
        }
    });
    let stylus_motion_host = Rc::clone(&host);
    let stylus_motion_status = status.clone();
    stylus.connect_motion(move |stylus, x, y| {
        let content_scale = gtk_ghostty_controller_scale(stylus);
        if !stylus_motion_host.borrow().stylus_pressure(
            x,
            y,
            ghostty_stylus_pressure(stylus, GHOSTTY_STYLUS_NORMAL_PRESSURE),
            stylus.current_event_state(),
            content_scale,
        ) {
            stylus_motion_status.set_text("Ghostty stylus pressure input failed");
        }
    });
    let stylus_up_host = Rc::clone(&host);
    let stylus_up_status = status.clone();
    stylus.connect_up(move |stylus, x, y| {
        let content_scale = gtk_ghostty_controller_scale(stylus);
        if !stylus_up_host.borrow().stylus_pressure(
            x,
            y,
            0.0,
            stylus.current_event_state(),
            content_scale,
        ) {
            stylus_up_status.set_text("Ghostty stylus pressure input failed");
        }
    });
    area.add_controller(stylus);

    let scroll_host = Rc::clone(&host);
    let scroll_status = status.clone();
    let scroll_controller =
        gtk::EventControllerScroll::new(gtk::EventControllerScrollFlags::BOTH_AXES);
    let precision_scroll = Rc::new(Cell::new(false));
    let precision_begin = Rc::clone(&precision_scroll);
    scroll_controller.connect_scroll_begin(move |_| precision_begin.set(true));
    let precision_end = Rc::clone(&precision_scroll);
    scroll_controller.connect_scroll_end(move |_| precision_end.set(false));
    scroll_controller.connect_scroll(move |controller, dx, dy| {
        let content_scale = gtk_ghostty_controller_scale(controller);
        if !scroll_host
            .borrow()
            .mouse_scroll(dx, dy, precision_scroll.get(), content_scale)
        {
            scroll_status.set_text("Ghostty pointer input failed");
        }
        glib::Propagation::Stop
    });
    area.add_controller(scroll_controller);

    let drop_host = Rc::clone(&host);
    let drop_status = status.clone();
    let drop_target = gtk::DropTarget::new(String::static_type(), gdk::DragAction::COPY);
    let mut drop_types = Vec::new();
    if let Some(file_list_type) = gdk_file_list_type() {
        drop_types.push(file_list_type);
    }
    drop_types.extend([gio::File::static_type(), String::static_type()]);
    drop_target.set_types(&drop_types);
    drop_target.connect_drop(move |_, value, _, _| {
        let dropped = drop_host.borrow().drop_value(value);
        if !dropped {
            drop_status.set_text("Ghostty text input failed");
        }
        dropped
    });
    area.add_controller(drop_target);

    let unrealize_host = Rc::clone(&host);
    area.connect_unrealize(move |area| {
        unrealize_host.borrow_mut().unrealize(area);
    });

    let tick_host = Rc::clone(&host);
    let tick_status = status.clone();
    let weak_area = area.downgrade();
    glib::timeout_add_local(Duration::from_millis(16), move || {
        if weak_area.upgrade().is_none() {
            return glib::ControlFlow::Break;
        }
        // Ghostty's redraw callback queues GLArea paints on this thread. This
        // timer only services the app loop; painting here leaks idle buffers.
        tick_host.borrow_mut().tick(&tick_status);
        glib::ControlFlow::Continue
    });
}

fn realize_ghostty_area(
    area: &gtk::GLArea,
    status: &gtk::Label,
    host: &Rc<RefCell<GtkGhosttyHost>>,
) {
    match host.borrow_mut().realize(area) {
        Ok(()) => status.set_text(GHOSTTY_RENDERER_ACTIVE_STATUS),
        Err(err) => {
            eprintln!("cmux: Ghostty surface realization failed: {err:#}");
            status.set_text(&format!("Ghostty renderer failed: {err}"));
        }
    }
}

fn connect_ghostty_im_context(host: &Rc<RefCell<GtkGhosttyHost>>) {
    let (im_context, im_state) = {
        let host = host.borrow();
        (host.im_context.clone(), Rc::clone(&host.im_state))
    };
    im_context.connect_preedit_start(move |_| {
        im_state.borrow_mut().preedit_start();
    });

    let (im_context, im_state) = {
        let host = host.borrow();
        (host.im_context.clone(), Rc::clone(&host.im_state))
    };
    im_context.connect_preedit_changed(move |context| {
        let (preedit, _, _) = context.preedit_string();
        im_state.borrow_mut().preedit_changed(preedit.to_string());
    });

    let (im_context, im_state) = {
        let host = host.borrow();
        (host.im_context.clone(), Rc::clone(&host.im_state))
    };
    im_context.connect_preedit_end(move |_| {
        im_state.borrow_mut().preedit_end();
    });

    let (im_context, im_state) = {
        let host = host.borrow();
        (host.im_context.clone(), Rc::clone(&host.im_state))
    };
    im_context.connect_commit(move |_, text| {
        im_state.borrow_mut().commit(text.to_string());
    });
}

struct GtkGhosttyApp {
    // Rust drops fields in declaration order. Keep app callback userdata and
    // loaded code alive while app/config destructors call into libghostty.
    app: GhosttyAppGuard,
    _config: GhosttyConfigGuard,
    callbacks: Box<GtkGhosttyAppCallbacks>,
    library: GhosttyLibrary,
    color_scheme: Cell<c_int>,
    last_tick: Cell<Option<Instant>>,
    config_reload_generation: Cell<u64>,
    scrollbar_allowed: Cell<bool>,
}

impl GtkGhosttyApp {
    fn shared() -> Result<Rc<Self>> {
        SHARED_GHOSTTY_APP.with(|shared| {
            if let Some(app) = shared.borrow().upgrade() {
                return Ok(app);
            }
            let app = Rc::new(Self::new()?);
            *shared.borrow_mut() = Rc::downgrade(&app);
            Ok(app)
        })
    }

    fn new() -> Result<Self> {
        let library = GhosttyLibrary::open_discovered()?;
        library.initialize()?;
        let config = load_cmux_managed_ghostty_config(&library)?;
        let scrollbar_allowed =
            library.config_string(&config, "scrollbar").as_deref() != Some("never");
        let mut callbacks = Box::new(GtkGhosttyAppCallbacks {
            token: next_ghostty_callback_token(),
            app: None,
            app_tick: None,
            surfaces: Mutex::new(HashMap::new()),
            focused_surface: AtomicUsize::new(0),
        });
        register_ghostty_app_userdata(callbacks.as_ref());
        let userdata = callbacks.as_mut() as *mut GtkGhosttyAppCallbacks as *mut c_void;
        let app = library.create_app_with_runtime(
            &config,
            GhosttyRuntimeConfig::with_callbacks(GhosttyRuntimeCallbacks {
                userdata,
                wakeup: gtk_ghostty_wakeup,
                action: gtk_ghostty_action,
                read_clipboard: gtk_ghostty_read_clipboard,
                confirm_read_clipboard: gtk_ghostty_confirm_read_clipboard,
                write_clipboard: gtk_ghostty_write_clipboard,
                close_surface: Some(gtk_ghostty_close_surface),
                redraw_surface: gtk_ghostty_redraw_surface,
                supports_selection_clipboard: true,
            }),
        )?;
        callbacks.app = Some(app.raw());
        callbacks.app_tick = Some(app.tick_fn());
        register_ghostty_app(callbacks.as_ref());
        Ok(Self {
            app,
            _config: config,
            callbacks,
            library,
            color_scheme: Cell::new(-1),
            last_tick: Cell::new(None),
            config_reload_generation: Cell::new(0),
            scrollbar_allowed: Cell::new(scrollbar_allowed),
        })
    }

    fn register_surface(&self, surface: GhosttySurface, callbacks: &GtkGhosttyCallbacks) {
        if let Ok(mut surfaces) = self.callbacks.surfaces.lock() {
            surfaces.insert(
                surface as usize,
                (ghostty_callback_ptr(callbacks), callbacks.token),
            );
        }
    }

    fn unregister_surface(&self, surface: GhosttySurface, callbacks: &GtkGhosttyCallbacks) {
        let surface = surface as usize;
        if let Ok(mut surfaces) = self.callbacks.surfaces.lock() {
            if surfaces.get(&surface).copied()
                == Some((ghostty_callback_ptr(callbacks), callbacks.token))
            {
                surfaces.remove(&surface);
            }
        }
        let _ = self.callbacks.focused_surface.compare_exchange(
            surface,
            0,
            Ordering::AcqRel,
            Ordering::Acquire,
        );
    }

    fn set_surface_focus(&self, surface: GhosttySurface, focused: bool) -> bool {
        let surface = surface as usize;
        if focused {
            self.callbacks
                .focused_surface
                .store(surface, Ordering::Release);
        } else {
            let _ = self.callbacks.focused_surface.compare_exchange(
                surface,
                0,
                Ordering::AcqRel,
                Ordering::Acquire,
            );
        }
        self.app
            .set_focus(self.callbacks.focused_surface.load(Ordering::Acquire) != 0)
    }

    fn set_color_scheme(&self, color_scheme: c_int) -> bool {
        if self.color_scheme.get() == color_scheme {
            return true;
        }
        if !self.app.set_color_scheme(color_scheme) {
            return false;
        }
        self.color_scheme.set(color_scheme);
        true
    }

    fn tick(&self) -> bool {
        let now = Instant::now();
        if self
            .last_tick
            .get()
            .is_some_and(|last| now.duration_since(last) < Duration::from_millis(1))
        {
            return true;
        }
        self.last_tick.set(Some(now));
        self.app.tick()
    }

    fn reload_config(&self, generation: u64) -> bool {
        if self.config_reload_generation.get() == generation {
            return true;
        }
        let Ok(config) = load_cmux_managed_ghostty_config(&self.library) else {
            return false;
        };
        if !self.app.update_config(&config) {
            return false;
        }
        self.scrollbar_allowed
            .set(self.library.config_string(&config, "scrollbar").as_deref() != Some("never"));
        self.config_reload_generation.set(generation);
        true
    }

    fn scrollbar_allowed(&self) -> bool {
        self.scrollbar_allowed.get()
    }
}

fn load_cmux_managed_ghostty_config(library: &GhosttyLibrary) -> Result<GhosttyConfigGuard> {
    library.load_default_config_with_string(&config::terminal_managed_ghostty_config())
}

struct GtkGhosttyAppCallbacks {
    token: u64,
    app: Option<GhosttyApp>,
    app_tick: Option<GhosttyAppTick>,
    surfaces: Mutex<HashMap<usize, (usize, u64)>>,
    focused_surface: AtomicUsize,
}

impl Drop for GtkGhosttyAppCallbacks {
    fn drop(&mut self) {
        unregister_ghostty_app(self);
    }
}

struct GtkGhosttyHost {
    surface: Option<GhosttySurfaceGuard>,
    inspector: Option<GhosttyInspectorGuard>,
    last_surface_size: Option<(u32, u32)>,
    options: GhosttySurfaceOptions,
    im_context: gtk::IMMulticontext,
    im_state: Rc<RefCell<GhosttyImState>>,
    app_context: Rc<GtkGhosttyApp>,
    callbacks: Box<GtkGhosttyCallbacks>,
    color_scheme: c_int,
    last_text_sync: Option<Instant>,
    last_scrollback_sync: Option<Instant>,
    selection_sync_requested: Arc<AtomicBool>,
    inspector_visible: Arc<AtomicBool>,
    keyboard_changed_handler: Option<(gtk::Window, glib::SignalHandlerId)>,
    super_surface_binding_keys: RefCell<HashSet<GhosttySurfaceBindingKey>>,
    copy_mode: GtkTerminalCopyMode,
    copy_mode_cursor: gtk::Frame,
    copy_mode_badge: gtk::Label,
    copy_mode_release_keys: RefCell<HashSet<u32>>,
}

#[derive(Default)]
struct GtkTerminalCopyMode {
    active: bool,
    visual: bool,
    input: CopyModeInputState,
    cursor: Option<CopyModeCursor>,
}

#[derive(Clone, Copy)]
struct GtkTerminalCopyModeGrid {
    rows: i32,
    columns: i32,
    cell_width: f64,
    cell_height: f64,
    x_inset: f64,
    y_inset: f64,
    width: f64,
    height: f64,
}

impl GtkGhosttyHost {
    fn new(
        options: GhosttySurfaceOptions,
        copy_mode_cursor: gtk::Frame,
        copy_mode_badge: gtk::Label,
    ) -> Result<Self> {
        let app_context = GtkGhosttyApp::shared()?;
        let selection_sync_requested = Arc::new(AtomicBool::new(false));
        let inspector_visible = Arc::new(AtomicBool::new(false));
        let mut callbacks = Box::new(GtkGhosttyCallbacks {
            token: next_ghostty_callback_token(),
            app: None,
            app_tick: None,
            area: None,
            surface: None,
            config_new: Some(app_context.library.config_new_fn()),
            config_free: Some(app_context.library.config_free_fn()),
            config_load_default_files: Some(app_context.library.config_load_default_files_fn()),
            config_load_recursive_files: Some(app_context.library.config_load_recursive_files_fn()),
            config_finalize: Some(app_context.library.config_finalize_fn()),
            config_diagnostics_count: Some(app_context.library.config_diagnostics_count_fn()),
            config_get_diagnostic: Some(app_context.library.config_get_diagnostic_fn()),
            config_open_path: Some(app_context.library.config_open_path_fn()),
            string_free: Some(app_context.library.string_free_fn()),
            app_update_config: Some(app_context.library.app_update_config_fn()),
            surface_inherited_config: Some(app_context.library.surface_inherited_config_fn()),
            surface_inherited_config_free: Some(
                app_context.library.surface_inherited_config_free_fn(),
            ),
            surface_update_config: Some(app_context.library.surface_update_config_fn()),
            complete_clipboard_request: Some(app_context.library.complete_clipboard_request_fn()),
            surface_needs_confirm_quit: Some(app_context.library.surface_needs_confirm_quit_fn()),
            close_surface_id: options.close_surface_id.clone(),
            app_state: options.app_state.clone(),
            cursor_state: Arc::new(Mutex::new(GhosttyCursorState::default())),
            selection_sync_requested: Arc::clone(&selection_sync_requested),
            inspector_visible: Arc::clone(&inspector_visible),
            rendering: AtomicBool::new(false),
        });
        register_ghostty_callbacks(callbacks.as_ref());
        callbacks.app = Some(app_context.app.raw());
        callbacks.app_tick = Some(app_context.app.tick_fn());

        let color_scheme = gtk_ghostty_color_scheme();
        if !app_context.set_color_scheme(color_scheme) {
            unregister_ghostty_callbacks(callbacks.as_ref());
            return Err(anyhow!("ghostty_app_set_color_scheme returned false"));
        }
        let im_context = gtk::IMMulticontext::new();

        let host = Self {
            surface: None,
            inspector: None,
            last_surface_size: None,
            callbacks,
            options,
            im_context,
            im_state: Rc::new(RefCell::new(GhosttyImState::default())),
            app_context,
            color_scheme,
            last_text_sync: None,
            last_scrollback_sync: None,
            selection_sync_requested,
            inspector_visible,
            keyboard_changed_handler: None,
            super_surface_binding_keys: RefCell::new(HashSet::new()),
            copy_mode: GtkTerminalCopyMode::default(),
            copy_mode_cursor,
            copy_mode_badge,
            copy_mode_release_keys: RefCell::new(HashSet::new()),
        };
        Ok(host)
    }

    fn realize(&mut self, area: &gtk::GLArea) -> Result<()> {
        area.make_current();
        if let Some(err) = area.error() {
            return Err(anyhow!("GTK GLArea context error: {err}"));
        }

        self.callbacks.area = Some(area.clone());
        if self.surface.is_some() {
            if !self.connect_keyboard_changed(area, None) {
                return Err(anyhow!("ghostty_app_keyboard_changed returned false"));
            }
            let scale = gtk_ghostty_scale_factor(area);
            let surface_size = gtk_ghostty_allocated_surface_size(area);
            let surface = self.surface.as_mut().expect("checked above");
            if !surface.display_realized()
                || !surface.set_content_scale(scale, scale)
                || surface_size.is_some_and(|size| !surface.set_size(size.0, size.1))
                || !surface.set_focus(self.options.focused)
                || !surface.set_visible(!self.options.occluded)
            {
                self.release_surface();
                return Err(anyhow!("Ghostty surface display re-realization failed"));
            }
            self.last_surface_size = surface_size;
            self.im_context.set_client_widget(Some(area));
            if self.options.focused {
                self.im_context.focus_in();
            }
            if self.options.copy_mode_active && !self.copy_mode.active {
                let _ = self.set_keyboard_copy_mode_active(true, false);
            } else {
                self.sync_copy_mode_overlay();
            }
            self.mark_runtime_ready(true);
            area.queue_render();
            return Ok(());
        }

        let userdata = self.callbacks.as_mut() as *mut GtkGhosttyCallbacks as *mut c_void;
        let platform = GhosttyPlatformLinux::new(
            userdata,
            gtk_ghostty_make_current as GhosttyLinuxMakeCurrent,
            gtk_ghostty_get_proc_address as GhosttyLinuxGetProcAddress,
            None,
        );
        let mut config = self.app_context.library.linux_surface_config(platform);
        config.set_userdata(userdata);
        config.set_context(GHOSTTY_SURFACE_CONTEXT_WINDOW);
        if self.options.manual_io {
            config.set_manual_io(gtk_ghostty_manual_io_write as GhosttyIoWriteCb, userdata);
        }
        let scale = gtk_ghostty_scale_factor(area);
        config.set_scale_factor(scale);
        let surface_size = gtk_ghostty_allocated_surface_size(area);
        if let Some(size) = surface_size {
            config.set_initial_size(size.0, size.1);
        }
        if let Some(font_size) = self.options.font_size {
            config.set_font_size(font_size);
        }
        let working_directory = self
            .options
            .working_directory
            .as_deref()
            .filter(|value| !value.trim().is_empty())
            .and_then(|value| CString::new(value).ok());
        if let Some(working_directory) = working_directory.as_ref() {
            config.set_working_directory(working_directory.as_ptr());
        }
        let command = self
            .options
            .command
            .as_deref()
            .filter(|value| !value.trim().is_empty())
            .and_then(|value| CString::new(value).ok());
        if let Some(command) = command.as_ref() {
            config.set_command(command.as_ptr());
        }
        let initial_input = self
            .options
            .initial_input
            .as_deref()
            .filter(|value| !value.is_empty())
            .and_then(|value| CString::new(value).ok());
        if let Some(initial_input) = initial_input.as_ref() {
            config.set_initial_input(initial_input.as_ptr());
        }
        if let Some(initial_output) = self
            .options
            .initial_output
            .as_deref()
            .filter(|value| !value.is_empty())
        {
            config.set_initial_output(initial_output.as_bytes());
        }
        if self.options.wait_after_command {
            config.set_wait_after_command(true);
        }
        let env_storage = self
            .options
            .env
            .iter()
            .filter_map(|(key, value)| {
                if key.is_empty() || key.contains('\0') || key.contains('=') || value.contains('\0')
                {
                    return None;
                }
                Some((
                    CString::new(key.as_str()).ok()?,
                    CString::new(value.as_str()).ok()?,
                ))
            })
            .collect::<Vec<_>>();
        validate_surface_env_var_count(env_storage.len())?;
        let env_vars = env_storage
            .iter()
            .map(|(key, value)| GhosttyEnvVar {
                key: key.as_ptr(),
                value: value.as_ptr(),
            })
            .collect::<Vec<_>>();
        if !env_vars.is_empty() {
            config.set_env_vars(env_vars.as_ptr(), env_vars.len());
        }

        if !self.connect_keyboard_changed(area, None) {
            return self.fail_realize(anyhow!("ghostty_app_keyboard_changed returned false"));
        }
        let mut surface = match self
            .app_context
            .library
            .create_surface(&self.app_context.app, &config)
        {
            Ok(surface) => surface,
            Err(err) => return self.fail_realize(err),
        };
        self.callbacks.surface = Some(surface.raw());
        self.app_context
            .register_surface(surface.raw(), self.callbacks.as_ref());
        if !surface.set_content_scale(scale, scale) {
            return self.fail_realize_with_surface(
                surface,
                anyhow!("ghostty_surface_set_content_scale returned false"),
            );
        }
        if !surface.set_color_scheme(self.color_scheme) {
            return self.fail_realize_with_surface(
                surface,
                anyhow!("ghostty_surface_set_color_scheme returned false"),
            );
        }
        if let Some(size) = surface_size {
            if !surface.set_size(size.0, size.1) {
                return self.fail_realize_with_surface(
                    surface,
                    anyhow!("ghostty_surface_set_size returned false"),
                );
            }
        }
        if !self
            .app_context
            .set_surface_focus(surface.raw(), self.options.focused)
            || !surface.set_focus(self.options.focused)
        {
            return self.fail_realize_with_surface(
                surface,
                anyhow!("ghostty_surface_set_focus returned false"),
            );
        }
        if !surface.set_renderer_realized(!self.options.occluded) {
            return self.fail_realize_with_surface(
                surface,
                anyhow!("ghostty_surface_set_renderer_realized returned false"),
            );
        }
        if !surface.set_visible(!self.options.occluded) {
            return self.fail_realize_with_surface(
                surface,
                anyhow!("ghostty_surface_set_visible returned false"),
            );
        }
        self.im_context.set_client_widget(Some(area));
        if self.options.focused {
            self.im_context.focus_in();
        }
        self.surface = Some(surface);
        self.last_surface_size = surface_size;
        if self.options.copy_mode_active {
            let _ = self.set_keyboard_copy_mode_active(true, false);
        }
        self.mark_runtime_ready(true);
        area.queue_render();
        Ok(())
    }

    fn fail_realize<T>(&mut self, err: Error) -> Result<T> {
        self.cleanup_failed_realize();
        Err(err)
    }

    fn fail_realize_with_surface<T>(
        &mut self,
        surface: GhosttySurfaceGuard,
        err: Error,
    ) -> Result<T> {
        self.drop_surface_after_unrealize(surface);
        self.cleanup_failed_realize();
        Err(err)
    }

    fn cleanup_failed_realize(&mut self) {
        self.callbacks.surface = None;
        self.disconnect_keyboard_changed();
        self.release_inspector();
        self.im_context.reset();
        self.im_context.set_client_widget(None::<&gtk::Widget>);
        self.im_state.borrow_mut().cancel_composition();
        self.reset_runtime_sync_state();
        self.callbacks.area = None;
        self.clear_runtime_state();
        rotate_ghostty_callback_registration(self.callbacks.as_mut());
    }

    fn set_focus(&self, focused: bool) -> bool {
        if focused {
            self.im_context.focus_in();
        } else {
            self.im_context.focus_out();
            self.im_context.reset();
            self.im_state.borrow_mut().cancel_composition();
        }
        if let Some(inspector) = self.inspector.as_ref() {
            if !inspector.set_focus(focused) {
                return false;
            }
        }
        if let Some(surface) = self.surface.as_ref() {
            if !self.app_context.set_surface_focus(surface.raw(), focused) {
                return false;
            }
            if !focused {
                if !surface.preedit(None) {
                    return false;
                }
            }
            return surface.set_focus(focused);
        }
        true
    }

    fn set_renderer_realized(&mut self, realized: bool) -> bool {
        if let Some(surface) = self.surface.as_mut() {
            return surface.set_renderer_realized(realized);
        }
        true
    }

    fn set_occluded(&mut self, occluded: bool) -> bool {
        if occluded {
            let visible = self
                .surface
                .as_ref()
                .map_or(true, |surface| surface.set_occlusion(false));
            let realized = self.set_renderer_realized(false);
            visible && realized
        } else if self.set_renderer_realized(true) {
            self.surface
                .as_ref()
                .map_or(true, |surface| surface.set_occlusion(true))
        } else {
            false
        }
    }

    fn connect_keyboard_changed(
        &mut self,
        area: &gtk::GLArea,
        status: Option<&gtk::Label>,
    ) -> bool {
        let Some(window) = gtk_ghostty_root_window(area) else {
            self.disconnect_keyboard_changed();
            return true;
        };
        if self
            .keyboard_changed_handler
            .as_ref()
            .is_some_and(|(connected, _)| connected == &window)
        {
            return true;
        }
        self.disconnect_keyboard_changed();
        let app = self.app_context.app.raw();
        let keyboard_changed = self.app_context.app.keyboard_changed_fn();
        let status = status.cloned();
        let handler = window.connect_keys_changed(move |_| unsafe {
            if !keyboard_changed(app) {
                if let Some(status) = status.as_ref() {
                    status.set_text("Ghostty keyboard map reload failed");
                }
            }
        });
        self.keyboard_changed_handler = Some((window, handler));
        self.app_context.app.keyboard_changed()
    }

    fn disconnect_keyboard_changed(&mut self) {
        if let Some((window, handler)) = self.keyboard_changed_handler.take() {
            window.disconnect(handler);
        }
    }

    fn update_options(&mut self, options: GhosttySurfaceOptions) {
        let focus_changed = self.options.focused != options.focused;
        let occlusion_changed = self.options.occluded != options.occluded;
        let copy_mode_changed = self.options.copy_mode_active != options.copy_mode_active;
        let previous_config_reload_generation = self.options.config_reload_generation;
        let config_reload_changed =
            previous_config_reload_generation != options.config_reload_generation;

        self.callbacks.close_surface_id = options.close_surface_id.clone();
        self.callbacks.app_state = options.app_state.clone();

        if focus_changed {
            let _ = self.set_focus(options.focused);
        }
        if occlusion_changed {
            let _ = self.set_occluded(options.occluded);
        }
        if copy_mode_changed {
            let _ = self.set_keyboard_copy_mode_active(options.copy_mode_active, false);
        }
        let config_reloaded = if config_reload_changed && self.surface.is_some() {
            self.reload_config_from_snapshot(options.config_reload_generation)
        } else {
            true
        };

        let mut next_options = options;
        if config_reload_changed && !config_reloaded {
            next_options.config_reload_generation = previous_config_reload_generation;
        }

        self.options = next_options;
    }

    fn set_keyboard_copy_mode_active(&mut self, active: bool, sync_model: bool) -> bool {
        if active && self.surface.is_none() {
            return false;
        }
        if self.copy_mode.active == active {
            self.sync_copy_mode_overlay();
            return true;
        }

        self.copy_mode.input.reset();
        self.copy_mode.visual = false;
        self.copy_mode.active = active;
        self.copy_mode.cursor = if active {
            if let Some(surface) = self.surface.as_ref() {
                let _ = surface.clear_selection();
            }
            self.initial_copy_mode_cursor()
        } else {
            if let Some(surface) = self.surface.as_ref() {
                let _ = surface.clear_selection();
            }
            None
        };
        self.options.copy_mode_active = active;
        self.sync_copy_mode_overlay();
        if sync_model {
            self.sync_copy_mode_to_app(active);
        }
        true
    }

    fn sync_copy_mode_to_app(&self, active: bool) {
        let (Some(app_state), Some(surface_id)) = (
            self.callbacks.app_state.as_ref(),
            self.callbacks.close_surface_id.as_deref(),
        ) else {
            return;
        };
        let Ok(mut app) = app_state.lock() else {
            return;
        };
        let _ = app.update_embedded_terminal_copy_mode(surface_id, active);
    }

    fn copy_mode_grid(&self) -> Option<GtkTerminalCopyModeGrid> {
        let surface = self.surface.as_ref()?;
        let area = self.callbacks.area.as_ref()?;
        let size = surface.size();
        let scale = gtk_ghostty_scale_factor(area);
        let width = f64::from(area.allocated_width().max(1));
        let height = f64::from(area.allocated_height().max(1));
        let cell_width = if size.cell_width_px > 0 {
            f64::from(size.cell_width_px) / scale
        } else {
            width / f64::from(size.columns.max(1))
        };
        let cell_height = if size.cell_height_px > 0 {
            f64::from(size.cell_height_px) / scale
        } else {
            height / f64::from(size.rows.max(1))
        };
        if !cell_width.is_finite()
            || !cell_height.is_finite()
            || cell_width <= 0.0
            || cell_height <= 0.0
        {
            return None;
        }
        let columns = i32::from(size.columns.max(1));
        let fitted_rows = (height / cell_height).floor().max(1.0) as i32;
        let rows = i32::from(size.rows.max(1)).min(fitted_rows);
        let terminal_width = cell_width * f64::from(columns);
        let terminal_height = cell_height * f64::from(rows);
        Some(GtkTerminalCopyModeGrid {
            rows,
            columns,
            cell_width,
            cell_height,
            x_inset: ((width - terminal_width) / 2.0).max(0.0),
            y_inset: ((height - terminal_height) / 2.0).max(0.0),
            width,
            height,
        })
    }

    fn initial_copy_mode_cursor(&self) -> Option<CopyModeCursor> {
        let surface = self.surface.as_ref()?;
        let grid = self.copy_mode_grid()?;
        let point = surface.ime_point();
        let mut cursor = if let Some(point) = point {
            CopyModeCursor {
                row: (((point.y - grid.y_inset) / grid.cell_height) - 1.0).floor() as i32,
                column: ((point.x - grid.x_inset) / grid.cell_width).floor() as i32,
            }
        } else {
            CopyModeCursor {
                row: grid.rows - 1,
                column: 0,
            }
        };
        cursor.clamp(grid.rows, grid.columns);
        Some(cursor)
    }

    fn sync_copy_mode_overlay(&mut self) {
        self.copy_mode_badge.set_visible(self.copy_mode.active);
        let Some(grid) = self.copy_mode_grid() else {
            self.copy_mode_cursor.set_visible(false);
            return;
        };
        let Some(mut cursor) = self.copy_mode.cursor else {
            self.copy_mode_cursor.set_visible(false);
            return;
        };
        cursor.clamp(grid.rows, grid.columns);
        self.copy_mode.cursor = Some(cursor);
        self.copy_mode_cursor.set_size_request(
            grid.cell_width.ceil() as i32,
            grid.cell_height.ceil() as i32,
        );
        self.copy_mode_cursor.set_margin_start(
            (grid.x_inset + f64::from(cursor.column) * grid.cell_width).round() as i32,
        );
        self.copy_mode_cursor.set_margin_top(
            (grid.y_inset + f64::from(cursor.row) * grid.cell_height).round() as i32,
        );
        self.copy_mode_cursor
            .set_visible(self.copy_mode.active && !self.copy_mode.visual);
    }

    fn update_presentation(&mut self, focused: bool, occluded: bool) {
        let mut options = self.options.clone();
        options.focused = focused;
        options.occluded = occluded;
        self.update_options(options);
    }

    fn reload_config_from_snapshot(&mut self, generation: u64) -> bool {
        if !self.app_context.reload_config(generation) {
            return false;
        }
        if let Some(surface) = self.surface.as_ref() {
            if !surface.refresh() {
                return false;
            }
        }
        self.record_snapshot_config_reload(false);
        true
    }

    fn record_snapshot_config_reload(&self, soft: bool) {
        let (Some(app_state), Some(surface_id)) = (
            self.callbacks.app_state.as_ref(),
            self.callbacks.close_surface_id.as_deref(),
        ) else {
            return;
        };
        let Ok(mut app) = app_state.lock() else {
            return;
        };
        let _ = app.reload_embedded_terminal_config(surface_id, soft);
    }

    fn resize(&mut self, area: &gtk::GLArea, width: i32, height: i32, status: &gtk::Label) {
        if !self.update_content_scale(area) {
            status.set_text("Ghostty renderer scale update failed");
        }
        let size = (positive_u32(width), positive_u32(height));
        if self.last_surface_size == Some(size) {
            self.sync_copy_mode_overlay();
            return;
        }
        if let Some(inspector) = self.inspector.as_ref() {
            if !inspector.set_size(size.0, size.1) {
                status.set_text("Ghostty inspector resize failed");
            }
        }
        if let Some(surface) = self.surface.as_ref() {
            if !surface.set_size(size.0, size.1) {
                status.set_text("Ghostty renderer resize failed");
                return;
            }
            self.last_surface_size = Some(size);
            if !surface.refresh() {
                status.set_text("Ghostty renderer refresh failed");
            }
        }
        self.sync_copy_mode_overlay();
    }

    fn update_content_scale(&self, area: &gtk::GLArea) -> bool {
        let scale = gtk_ghostty_scale_factor(area);
        if let Some(inspector) = self.inspector.as_ref() {
            if !inspector.set_content_scale(scale, scale) {
                return false;
            }
        }
        if let Some(surface) = self.surface.as_ref() {
            return surface.set_content_scale(scale, scale);
        }
        true
    }

    fn sync_color_scheme(&mut self) -> bool {
        let color_scheme = gtk_ghostty_color_scheme();
        if color_scheme == self.color_scheme {
            return true;
        }
        self.color_scheme = color_scheme;
        if !self.app_context.set_color_scheme(color_scheme) {
            return false;
        }
        if let Some(surface) = self.surface.as_ref() {
            if !surface.set_color_scheme(color_scheme) {
                return false;
            }
            return surface.refresh();
        }
        true
    }

    fn render(&mut self, area: &gtk::GLArea, status: &gtk::Label) {
        area.make_current();
        if area.error().is_some() {
            return;
        }
        let _render_guard = GhosttyRenderGuard::enter(&self.callbacks.rendering);
        if let Some(surface) = self.surface.as_ref() {
            if !surface.draw() {
                status.set_text("Ghostty renderer draw failed");
            }
        }
        self.render_inspector(area, status);
    }

    fn unrealize(&mut self, _area: &gtk::GLArea) {
        self.disconnect_keyboard_changed();
        self.im_context.reset();
        self.im_context.set_client_widget(None::<&gtk::Widget>);
        self.im_state.borrow_mut().cancel_composition();
        if let Some(surface) = self.surface.as_mut() {
            let _ = surface.preedit(None);
            let _ = surface.display_unrealized();
        }
        self.release_inspector();
        self.last_surface_size = None;
        self.callbacks.area = None;
    }

    fn release_surface(&mut self) {
        self.last_surface_size = None;
        self.copy_mode = GtkTerminalCopyMode::default();
        self.copy_mode_cursor.set_visible(false);
        self.copy_mode_badge.set_visible(false);
        self.copy_mode_release_keys.borrow_mut().clear();
        self.disconnect_keyboard_changed();
        self.im_context.reset();
        self.im_context.set_client_widget(None::<&gtk::Widget>);
        self.im_state.borrow_mut().cancel_composition();
        if let Some(mut surface) = self.surface.take() {
            let raw_surface = surface.raw();
            let _ = surface.preedit(None);
            let _ = surface.display_unrealized();
            self.release_inspector();
            drop(surface);
            self.app_context
                .unregister_surface(raw_surface, self.callbacks.as_ref());
            self.callbacks.surface = None;
        } else {
            self.release_inspector();
            self.callbacks.surface = None;
        }
        let _ = self.app_context.app.set_focus(
            self.app_context
                .callbacks
                .focused_surface
                .load(Ordering::Acquire)
                != 0,
        );
        self.reset_runtime_sync_state();
        self.clear_runtime_state();
        self.callbacks.area = None;
        rotate_ghostty_callback_registration(self.callbacks.as_mut());
    }

    fn drop_surface_after_unrealize(&mut self, mut surface: GhosttySurfaceGuard) {
        self.last_surface_size = None;
        let raw_surface = surface.raw();
        let _ = surface.display_unrealized();
        drop(surface);
        self.app_context
            .unregister_surface(raw_surface, self.callbacks.as_ref());
        self.callbacks.surface = None;
    }

    fn render_inspector(&mut self, area: &gtk::GLArea, status: &gtk::Label) {
        if !self.inspector_visible.load(Ordering::Acquire) {
            self.release_inspector();
            return;
        }
        if self.inspector.is_none() && !self.create_inspector(area) {
            self.inspector_visible.store(false, Ordering::Release);
            return;
        }
        let Some(inspector) = self.inspector.as_ref() else {
            return;
        };
        if !inspector.render() {
            status.set_text("Ghostty inspector render failed");
        }
    }

    fn create_inspector(&mut self, area: &gtk::GLArea) -> bool {
        let Some(surface) = self.surface.as_ref() else {
            return false;
        };
        let Ok(inspector) = surface.create_inspector() else {
            return false;
        };
        if !inspector.init_opengl() {
            return false;
        }
        let scale = gtk_ghostty_scale_factor(area);
        if !inspector.set_content_scale(scale, scale) {
            return false;
        }
        if !inspector.set_size(
            positive_u32(area.allocated_width()),
            positive_u32(area.allocated_height()),
        ) {
            return false;
        }
        if !inspector.set_focus(self.options.focused) {
            return false;
        }
        self.inspector = Some(inspector);
        true
    }

    fn release_inspector(&mut self) {
        let mut inspector = self.inspector.take();
        let context_current = if inspector.is_some() {
            self.make_context_current_for_cleanup()
        } else {
            false
        };
        if let Some(inspector) = inspector.as_mut() {
            let _ = inspector.shutdown_opengl();
        }
        drop(inspector);
        if context_current && !self.callbacks.rendering.load(Ordering::SeqCst) {
            gdk::GLContext::clear_current();
        }
    }

    fn make_context_current_for_cleanup(&self) -> bool {
        let Some(area) = self.callbacks.area.as_ref() else {
            return false;
        };
        area.make_current();
        area.error().is_none()
    }

    fn mark_runtime_ready(&self, ready: bool) {
        let (Some(app_state), Some(surface_id)) = (
            self.callbacks.app_state.as_ref(),
            self.callbacks.close_surface_id.as_deref(),
        ) else {
            return;
        };
        let Ok(mut app) = app_state.lock() else {
            return;
        };
        let _ = app.set_embedded_terminal_runtime_ready(surface_id, ready);
    }

    fn clear_runtime_state(&self) {
        let (Some(app_state), Some(surface_id)) = (
            self.callbacks.app_state.as_ref(),
            self.callbacks.close_surface_id.as_deref(),
        ) else {
            return;
        };
        let Ok(mut app) = app_state.lock() else {
            return;
        };
        let _ = app.clear_embedded_terminal_runtime_state(surface_id);
    }

    fn reset_runtime_sync_state(&mut self) {
        self.last_text_sync = None;
        self.last_scrollback_sync = None;
        self.selection_sync_requested
            .store(false, Ordering::Release);
        self.super_surface_binding_keys.borrow_mut().clear();
        self.reset_cursor_state();
    }

    fn reset_cursor_state(&self) {
        let mut cursor_name = GhosttyCursorState::default().cursor_name();
        if let Ok(mut state) = self.callbacks.cursor_state.lock() {
            state.reset();
            cursor_name = state.cursor_name();
        }
        if let Some(area) = self.callbacks.area.as_ref() {
            area.set_cursor_from_name(Some(cursor_name));
        }
    }

    fn tick(&mut self, status: &gtk::Label) {
        if !self.sync_color_scheme() {
            status.set_text("Ghostty renderer refresh failed");
        }
        if !self.app_context.tick() {
            status.set_text("Ghostty app tick failed");
            return;
        }
        if !self.drain_app_input() {
            status.set_text("Ghostty text input failed");
            return;
        }
        let force_sync = self.selection_sync_requested.swap(false, Ordering::AcqRel);
        self.sync_app_surface_snapshot(force_sync);
    }

    fn drain_app_input(&self) -> bool {
        let Some(surface) = self.surface.as_ref() else {
            return true;
        };
        let (Some(app_state), Some(surface_id)) = (
            self.callbacks.app_state.as_ref(),
            self.callbacks.close_surface_id.as_deref(),
        ) else {
            return true;
        };
        let Ok(mut app) = app_state.lock() else {
            return true;
        };
        let input = app
            .drain_embedded_terminal_input(surface_id)
            .unwrap_or_default();
        drop(app);
        for input in input {
            match input {
                EmbeddedTerminalInput::Text(text) => {
                    if !surface.text(&text) {
                        return false;
                    }
                }
                EmbeddedTerminalInput::Key(key) => {
                    if let Some((press, _press_text)) =
                        ghostty_queued_key_event(GHOSTTY_ACTION_PRESS, &key)
                    {
                        surface.key(press);
                        let Some((release, _release_text)) =
                            ghostty_queued_key_event(GHOSTTY_ACTION_RELEASE, &key)
                        else {
                            return false;
                        };
                        surface.key(release);
                    } else {
                        let Ok(bytes) = terminal_key_bytes(&key) else {
                            return false;
                        };
                        let Ok(text) = String::from_utf8(bytes) else {
                            return false;
                        };
                        if !surface.text(&text) {
                            return false;
                        }
                    }
                }
                EmbeddedTerminalInput::BindingAction(action) => {
                    if !surface.binding_action(&action).unwrap_or(false) {
                        return false;
                    }
                }
                EmbeddedTerminalInput::ProcessOutput(bytes) => {
                    if !surface.process_output(&bytes) {
                        return false;
                    }
                }
            }
        }
        true
    }

    fn sync_app_surface_snapshot(&mut self, force: bool) {
        let now = Instant::now();
        if !force
            && self
                .last_text_sync
                .is_some_and(|last| now.duration_since(last) < GHOSTTY_TEXT_SYNC_INTERVAL)
        {
            return;
        }

        let Some(surface) = self.surface.as_ref() else {
            return;
        };
        let (Some(app_state), Some(surface_id)) = (
            self.callbacks.app_state.as_ref(),
            self.callbacks.close_surface_id.as_deref(),
        ) else {
            return;
        };
        let tty = surface.tty_name();
        let title = surface.title();
        let pwd = surface.pwd();
        let foreground_pid = surface.foreground_pid();
        let process_exited = surface.process_exited();
        let size = surface.size();
        let mouse_captured = surface.mouse_captured();
        let needs_confirm_quit = surface.needs_confirm_quit();
        let has_selection = surface.has_selection();
        let selection_text = if has_selection {
            surface.read_selection_text()
        } else {
            None
        };
        let text = surface.read_viewport_text();
        let scrollback_due = self
            .last_scrollback_sync
            .is_none_or(|last| now.duration_since(last) >= GHOSTTY_SCROLLBACK_SYNC_INTERVAL);
        let scrollback = scrollback_due
            .then(|| surface.read_scrollback_text(GHOSTTY_SCROLLBACK_SYNC_MAX_BYTES))
            .flatten();
        let Ok(mut app) = app_state.lock() else {
            return;
        };
        self.last_text_sync = Some(now);
        if scrollback_due {
            self.last_scrollback_sync = Some(now);
        }
        let _ = app.update_embedded_terminal_runtime_metadata(
            surface_id,
            tty.as_deref(),
            foreground_pid,
            process_exited,
            Some(EmbeddedTerminalSize {
                columns: size.columns,
                rows: size.rows,
                width_px: size.width_px,
                height_px: size.height_px,
                cell_width_px: size.cell_width_px,
                cell_height_px: size.cell_height_px,
            }),
        );
        if let Some(title) = title.as_deref() {
            let _ = app.apply_embedded_terminal_title(surface_id, title);
        }
        if let Some(pwd) = pwd.as_deref() {
            let _ = app.apply_embedded_terminal_pwd(surface_id, pwd);
        }
        let _ = app.update_embedded_terminal_mouse_captured(surface_id, mouse_captured);
        let _ = app.update_embedded_terminal_close_confirmation(surface_id, needs_confirm_quit);
        let _ = app.update_embedded_terminal_selection(
            surface_id,
            has_selection,
            selection_text.as_deref(),
        );
        if let Some(text) = text {
            let _ = app.update_embedded_terminal_text_snapshot(surface_id, &text);
        }
        if let Some(scrollback) = scrollback {
            let _ = app.update_embedded_terminal_scrollback_snapshot(surface_id, &scrollback);
        }
    }

    fn sync_scrollback_snapshot(&mut self) -> bool {
        let Some(surface) = self.surface.as_ref() else {
            return false;
        };
        let (Some(app_state), Some(surface_id)) = (
            self.callbacks.app_state.as_ref(),
            self.callbacks.close_surface_id.as_deref(),
        ) else {
            return false;
        };
        let Some(scrollback) = surface.read_scrollback_text(GHOSTTY_SCROLLBACK_SYNC_MAX_BYTES)
        else {
            return false;
        };
        let Ok(mut app) = app_state.lock() else {
            return false;
        };
        self.last_scrollback_sync = Some(Instant::now());
        app.update_embedded_terminal_scrollback_snapshot(surface_id, &scrollback)
            .is_ok()
    }

    fn handle_keyboard_copy_mode_key(
        &mut self,
        keyval: gdk::Key,
        keycode: u32,
        modifiers: gdk::ModifierType,
    ) -> bool {
        if !self.copy_mode.active {
            return false;
        }
        let modifiers = gtk_copy_mode_modifiers(modifiers);
        if modifiers.bypasses_copy_mode() {
            self.copy_mode.input.reset();
            return false;
        }
        let resolution = self.copy_mode.input.resolve(
            gtk_copy_mode_key(keyval),
            modifiers,
            self.copy_mode.visual,
        );
        self.copy_mode_release_keys.borrow_mut().insert(keycode);
        if let CopyModeResolution::Perform(action, count) = resolution {
            self.perform_copy_mode_action(action, count);
        }
        true
    }

    fn perform_copy_mode_action(&mut self, action: CopyModeAction, count: usize) {
        let Some(surface) = self.surface.as_ref() else {
            let _ = self.set_keyboard_copy_mode_active(false, true);
            return;
        };
        match action {
            CopyModeAction::Exit => {
                let _ = self.set_keyboard_copy_mode_active(false, true);
            }
            CopyModeAction::StartSelection => {
                if self.select_copy_mode_cursor_cell() {
                    self.copy_mode.visual = true;
                    self.sync_copy_mode_overlay();
                }
            }
            CopyModeAction::ClearSelection => {
                self.copy_mode.visual = false;
                let _ = surface.clear_selection();
                self.sync_copy_mode_overlay();
            }
            CopyModeAction::CopyAndExit => {
                let _ = surface.binding_action("copy_to_clipboard");
                let _ = self.set_keyboard_copy_mode_active(false, true);
            }
            CopyModeAction::CopyLineAndExit => {
                let _ = self.copy_current_viewport_lines(count);
                let _ = self.set_keyboard_copy_mode_active(false, true);
            }
            CopyModeAction::ScrollLines(delta) => {
                let delta = delta.saturating_mul(count as i32);
                let _ = surface.binding_action(&format!("scroll_page_lines:{delta}"));
                if let (Some(grid), Some(cursor)) =
                    (self.copy_mode_grid(), self.copy_mode.cursor.as_mut())
                {
                    cursor.shift_for_scroll(delta, grid.rows, grid.columns);
                }
                self.sync_copy_mode_overlay();
            }
            CopyModeAction::ScrollPage(delta) => {
                let binding = if delta > 0 {
                    "scroll_page_down"
                } else {
                    "scroll_page_up"
                };
                for _ in 0..count {
                    let _ = surface.binding_action(binding);
                }
                if let (Some(grid), Some(cursor)) =
                    (self.copy_mode_grid(), self.copy_mode.cursor.as_mut())
                {
                    cursor.shift_for_scroll(
                        delta.saturating_mul(grid.rows).saturating_mul(count as i32),
                        grid.rows,
                        grid.columns,
                    );
                }
                self.sync_copy_mode_overlay();
            }
            CopyModeAction::ScrollHalfPage(delta) => {
                let binding = if delta > 0 {
                    "scroll_page_fractional:0.5"
                } else {
                    "scroll_page_fractional:-0.5"
                };
                for _ in 0..count {
                    let _ = surface.binding_action(binding);
                }
                if let (Some(grid), Some(cursor)) =
                    (self.copy_mode_grid(), self.copy_mode.cursor.as_mut())
                {
                    cursor.shift_for_scroll(
                        delta
                            .saturating_mul(grid.rows / 2)
                            .saturating_mul(count as i32),
                        grid.rows,
                        grid.columns,
                    );
                }
                self.sync_copy_mode_overlay();
            }
            CopyModeAction::ScrollToTop => {
                if let (Some(grid), Some(cursor)) =
                    (self.copy_mode_grid(), self.copy_mode.cursor.as_mut())
                {
                    let _ = cursor.move_cursor(CopyModeMove::Home, 1, grid.rows, grid.columns);
                }
                let _ = surface.binding_action("scroll_to_top");
                self.sync_copy_mode_overlay();
            }
            CopyModeAction::ScrollToBottom => {
                if let (Some(grid), Some(cursor)) =
                    (self.copy_mode_grid(), self.copy_mode.cursor.as_mut())
                {
                    let _ = cursor.move_cursor(CopyModeMove::End, 1, grid.rows, grid.columns);
                }
                let _ = surface.binding_action("scroll_to_bottom");
                self.sync_copy_mode_overlay();
            }
            CopyModeAction::JumpToPrompt(delta) => {
                let delta = delta.saturating_mul(count as i32);
                let _ = surface.binding_action(&format!("jump_to_prompt:{delta}"));
                self.sync_copy_mode_overlay();
            }
            CopyModeAction::StartSearch => {
                let _ = surface.binding_action("start_search");
            }
            CopyModeAction::SearchNext | CopyModeAction::SearchPrevious => {
                let binding = if action == CopyModeAction::SearchNext {
                    "navigate_search:next"
                } else {
                    "navigate_search:previous"
                };
                for _ in 0..count {
                    let _ = surface.binding_action(binding);
                }
                self.sync_copy_mode_overlay();
            }
            CopyModeAction::AdjustSelection(movement) => {
                let Some(grid) = self.copy_mode_grid() else {
                    return;
                };
                if self.copy_mode.visual {
                    for _ in 0..count {
                        let _ = surface.binding_action(&format!(
                            "adjust_selection:{}",
                            movement.binding_name()
                        ));
                    }
                    if let Some(cursor) = self.copy_mode.cursor.as_mut() {
                        let _ = cursor.move_cursor(movement, count, grid.rows, grid.columns);
                    }
                } else if let Some(cursor) = self.copy_mode.cursor.as_mut() {
                    let scroll_delta = cursor.move_cursor(movement, count, grid.rows, grid.columns);
                    if scroll_delta != 0 {
                        let _ =
                            surface.binding_action(&format!("scroll_page_lines:{scroll_delta}"));
                    }
                }
                self.sync_copy_mode_overlay();
            }
        }
    }

    fn select_copy_mode_cursor_cell(&mut self) -> bool {
        let (Some(surface), Some(grid), Some(mut cursor)) = (
            self.surface.as_ref(),
            self.copy_mode_grid(),
            self.copy_mode.cursor,
        ) else {
            return false;
        };
        cursor.clamp(grid.rows, grid.columns);
        self.copy_mode.cursor = Some(cursor);
        let cell_left = grid.x_inset + f64::from(cursor.column) * grid.cell_width;
        let start_x = (cell_left + 0.5).clamp(0.0, (grid.width - 1.0).max(0.0));
        let end_x = (cell_left + grid.cell_width - 0.5).clamp(0.0, (grid.width - 1.0).max(0.0));
        if end_x <= start_x {
            return false;
        }
        let y = (grid.y_inset + (f64::from(cursor.row) + 0.5) * grid.cell_height)
            .clamp(0.0, (grid.height - 1.0).max(0.0));
        let _ = surface.clear_selection();
        if !surface.mouse_pos(start_x, y, 0)
            || !surface.mouse_button(GHOSTTY_MOUSE_PRESS, GHOSTTY_MOUSE_BUTTON_LEFT, 0)
            || !surface.mouse_pos(end_x, y, 0)
        {
            let _ = surface.clear_selection();
            return false;
        }
        let _ = surface.mouse_button(GHOSTTY_MOUSE_RELEASE, GHOSTTY_MOUSE_BUTTON_LEFT, 0);
        let selected = surface.has_selection();
        if !selected {
            let _ = surface.clear_selection();
        }
        selected
    }

    fn copy_current_viewport_lines(&mut self, count: usize) -> bool {
        let (Some(surface), Some(grid), Some(mut cursor)) = (
            self.surface.as_ref(),
            self.copy_mode_grid(),
            self.copy_mode.cursor,
        ) else {
            return false;
        };
        cursor.clamp(grid.rows, grid.columns);
        self.copy_mode.cursor = Some(cursor);
        let end_row = (cursor.row + count.saturating_sub(1) as i32).min(grid.rows - 1);
        if !surface.select_viewport_rows(cursor.row as u32, end_row as u32) {
            return false;
        }
        surface.binding_action("copy_to_clipboard").unwrap_or(false)
    }

    fn key_event(
        &mut self,
        action: c_int,
        controller: &gtk::EventControllerKey,
        keyval: gdk::Key,
        keycode: u32,
        modifiers: gdk::ModifierType,
        status: &gtk::Label,
    ) -> bool {
        if action == GHOSTTY_ACTION_RELEASE
            && self.copy_mode_release_keys.borrow_mut().remove(&keycode)
        {
            return true;
        }
        if modifiers.intersects(gdk::ModifierType::SUPER_MASK | gdk::ModifierType::META_MASK) {
            return self.super_meta_key_event(action, controller, keyval, keycode, modifiers);
        }

        if self.copy_mode.active {
            if action == GHOSTTY_ACTION_RELEASE {
                return true;
            }
            return self.handle_keyboard_copy_mode_key(keyval, keycode, modifiers);
        }

        if let Some(inspector) = self.inspector.as_ref() {
            let mods = ghostty_mods(modifiers);
            if let Some(key) = ghostty_inspector_key(keyval) {
                if !inspector.key(action, key, mods) {
                    status.set_text("Ghostty inspector key input failed");
                    return true;
                }
            }
            if action == GHOSTTY_ACTION_PRESS {
                if let Some(text) = ghostty_key_text(keyval) {
                    if !inspector.text(&text) {
                        status.set_text("Ghostty inspector text input failed");
                    }
                }
            }
            return true;
        }

        let Some(surface) = self.surface.as_ref() else {
            return false;
        };
        let (im_key_event, im_handled) = if let Some(event) = controller.current_event() {
            if !self.update_im_cursor_location(surface) {
                status.set_text("Ghostty IME cursor update failed");
                return true;
            }
            let im_key_event = self.im_state.borrow_mut().begin_key_event();
            let im_handled = self.im_context.filter_keypress(&event);
            if !self.apply_im_effects(surface) {
                status.set_text("Ghostty text input failed");
                return true;
            }
            (im_key_event, im_handled)
        } else {
            (GhosttyImKeyEvent::False, false)
        };
        let (im_composing, im_text, stop_for_im) = {
            let mut im_state = self.im_state.borrow_mut();
            let stop_for_im = im_state.should_stop_key_event(im_key_event, im_handled);
            let im_text = if action == GHOSTTY_ACTION_PRESS && !stop_for_im {
                im_state.take_key_commit()
            } else {
                None
            };
            let im_composing = im_state.composing;
            im_state.end_key_event();
            (im_composing, im_text, stop_for_im)
        };
        if stop_for_im {
            return true;
        }

        let text = if action == GHOSTTY_ACTION_PRESS {
            im_text.or_else(|| ghostty_key_text(keyval))
        } else {
            None
        };
        let history_input = (action == GHOSTTY_ACTION_PRESS)
            .then(|| ghostty_input_history_bytes(keyval, modifiers, text.as_deref()))
            .flatten();
        let (event, _text_cstring) =
            ghostty_input_key_event(action, controller, keycode, modifiers, text, im_composing);

        if action == GHOSTTY_ACTION_PRESS {
            self.record_terminal_input(history_input.as_deref());
        }
        surface.key(event)
    }

    fn super_meta_key_event(
        &self,
        action: c_int,
        controller: &gtk::EventControllerKey,
        keyval: gdk::Key,
        keycode: u32,
        modifiers: gdk::ModifierType,
    ) -> bool {
        let text = if action == GHOSTTY_ACTION_PRESS {
            ghostty_key_text(keyval)
        } else {
            None
        };
        let (event, _text_cstring) =
            ghostty_input_key_event(action, controller, keycode, modifiers, text, false);
        if self.app_context.app.key(event) {
            return true;
        }

        let Some(surface) = self.surface.as_ref() else {
            return false;
        };
        let key = GhosttySurfaceBindingKey::from_input(event);
        if action == GHOSTTY_ACTION_RELEASE {
            if self.super_surface_binding_keys.borrow_mut().remove(&key) {
                return surface.key(event);
            }
            return false;
        }
        if let Some(flags) = surface.key_binding_flags(event) {
            let consumed = surface.key(event);
            if ghostty_should_track_super_surface_binding(flags, consumed) {
                self.super_surface_binding_keys.borrow_mut().insert(key);
            }
            return consumed;
        }

        false
    }

    fn update_im_cursor_location(&self, surface: &GhosttySurfaceGuard) -> bool {
        let Some(point) = surface.ime_point() else {
            return false;
        };
        self.im_context
            .set_cursor_location(&gtk_ghostty_im_rectangle(point));
        true
    }

    fn apply_im_effects(&self, surface: &GhosttySurfaceGuard) -> bool {
        let (preedit, commits) = self.im_state.borrow_mut().drain_effects();
        if let Some(preedit) = preedit {
            if !surface.preedit(preedit.as_deref()) {
                return false;
            }
        }
        for commit in commits {
            self.record_terminal_input(Some(commit.as_bytes()));
            if !surface.text(&commit) {
                return false;
            }
        }
        true
    }

    fn mouse_pos(&self, x: f64, y: f64, modifiers: gdk::ModifierType, content_scale: f64) -> bool {
        let (x, y) = ghostty_pointer_input(x, y, content_scale);
        if let Some(inspector) = self.inspector.as_ref() {
            return inspector.mouse_pos(x, y);
        }
        if let Some(surface) = self.surface.as_ref() {
            return surface.mouse_pos(x, y, ghostty_mods(modifiers));
        }
        true
    }

    fn mouse_button(
        &self,
        action: c_int,
        button: u32,
        x: f64,
        y: f64,
        modifiers: gdk::ModifierType,
        content_scale: f64,
    ) -> bool {
        let (x, y) = ghostty_pointer_input(x, y, content_scale);
        if let Some(inspector) = self.inspector.as_ref() {
            let mods = ghostty_mods(modifiers);
            return inspector.mouse_pos(x, y)
                && inspector.mouse_button(action, ghostty_mouse_button(button), mods);
        }
        let Some(surface) = self.surface.as_ref() else {
            return false;
        };
        let mods = ghostty_mods(modifiers);
        if !surface.mouse_pos(x, y, mods) {
            return false;
        }
        surface.mouse_button(action, ghostty_mouse_button(button), mods)
    }

    fn mouse_scroll(&self, dx: f64, dy: f64, precision: bool, content_scale: f64) -> bool {
        if let Some(inspector) = self.inspector.as_ref() {
            let (x, y, scroll_mods) = ghostty_inspector_scroll_input(dx, dy, precision);
            return inspector.mouse_scroll(x, y, scroll_mods);
        }
        if let Some(surface) = self.surface.as_ref() {
            let (x, y, scroll_mods) =
                ghostty_surface_scroll_input(dx, dy, precision, content_scale);
            return surface.mouse_scroll(x, y, scroll_mods);
        }
        true
    }

    fn stylus_pressure(
        &self,
        x: f64,
        y: f64,
        pressure: f64,
        modifiers: gdk::ModifierType,
        content_scale: f64,
    ) -> bool {
        let pressure = ghostty_pressure_value(pressure);
        let (x, y) = ghostty_pointer_input(x, y, content_scale);
        if let Some(inspector) = self.inspector.as_ref() {
            return inspector.mouse_pos(x, y);
        }
        let Some(surface) = self.surface.as_ref() else {
            return false;
        };
        if !surface.mouse_pos(x, y, ghostty_mods(modifiers)) {
            return false;
        }
        surface.mouse_pressure(ghostty_pressure_stage(pressure), pressure)
    }

    fn drop_value(&self, value: &glib::Value) -> bool {
        let Some(text) = ghostty_drop_text(value) else {
            return false;
        };
        let Some(surface) = self.surface.as_ref() else {
            return false;
        };
        self.record_terminal_input(Some(text.as_bytes()));
        surface.text(&text)
    }

    fn record_terminal_input(&self, input: Option<&[u8]>) {
        let (Some(app_state), Some(surface_id)) = (
            self.callbacks.app_state.as_ref(),
            self.callbacks.close_surface_id.as_deref(),
        ) else {
            return;
        };
        if let Ok(mut app) = app_state.lock() {
            app.record_embedded_terminal_input(surface_id, input);
        }
    }
}

impl Drop for GtkGhosttyHost {
    fn drop(&mut self) {
        self.release_surface();
        unregister_ghostty_callbacks(self.callbacks.as_ref());
    }
}

struct GtkGhosttyCallbacks {
    token: u64,
    app: Option<GhosttyApp>,
    app_tick: Option<GhosttyAppTick>,
    area: Option<gtk::GLArea>,
    surface: Option<GhosttySurface>,
    config_new: Option<GhosttyConfigNew>,
    config_free: Option<GhosttyConfigFree>,
    config_load_default_files: Option<GhosttyConfigLoadDefaultFiles>,
    config_load_recursive_files: Option<GhosttyConfigLoadRecursiveFiles>,
    config_finalize: Option<GhosttyConfigFinalize>,
    config_diagnostics_count: Option<GhosttyConfigDiagnosticsCount>,
    config_get_diagnostic: Option<GhosttyConfigGetDiagnostic>,
    config_open_path: Option<GhosttyConfigOpenPath>,
    string_free: Option<GhosttyStringFree>,
    app_update_config: Option<GhosttyAppUpdateConfig>,
    surface_inherited_config: Option<GhosttySurfaceInheritedConfig>,
    surface_inherited_config_free: Option<GhosttySurfaceInheritedConfigFree>,
    surface_update_config: Option<GhosttySurfaceUpdateConfig>,
    complete_clipboard_request: Option<GhosttySurfaceCompleteClipboardRequest>,
    surface_needs_confirm_quit: Option<GhosttySurfaceNeedsConfirmQuit>,
    close_surface_id: Option<String>,
    app_state: Option<Arc<Mutex<AppState>>>,
    cursor_state: Arc<Mutex<GhosttyCursorState>>,
    selection_sync_requested: Arc<AtomicBool>,
    inspector_visible: Arc<AtomicBool>,
    rendering: AtomicBool,
}

impl GtkGhosttyCallbacks {
    fn has_live_surface(&self) -> bool {
        self.surface.is_some()
    }

    fn redraw_area(&self) -> Option<gtk::GLArea> {
        if !self.has_live_surface() {
            return None;
        }
        self.area.clone()
    }
}

struct GhosttyRenderGuard {
    rendering: *const AtomicBool,
}

impl GhosttyRenderGuard {
    fn enter(rendering: &AtomicBool) -> Self {
        rendering.store(true, Ordering::SeqCst);
        Self {
            rendering: rendering as *const AtomicBool,
        }
    }
}

impl Drop for GhosttyRenderGuard {
    fn drop(&mut self) {
        if !self.rendering.is_null() {
            unsafe {
                (*self.rendering).store(false, Ordering::SeqCst);
            }
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct GhosttyCursorState {
    shape: c_int,
    link_hover: bool,
    visible: bool,
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
struct GhosttySurfaceBindingKey {
    keycode: u32,
    mods: c_int,
    unshifted_codepoint: u32,
}

impl GhosttySurfaceBindingKey {
    fn from_input(event: GhosttyInputKey) -> Self {
        Self {
            keycode: event.keycode,
            mods: event.mods,
            unshifted_codepoint: event.unshifted_codepoint,
        }
    }
}

impl Default for GhosttyCursorState {
    fn default() -> Self {
        Self {
            shape: GHOSTTY_MOUSE_SHAPE_TEXT,
            link_hover: false,
            visible: true,
        }
    }
}

impl GhosttyCursorState {
    fn cursor_name(&self) -> &'static str {
        if !self.visible {
            return "none";
        }
        if self.link_hover {
            return ghostty_cursor_name(GHOSTTY_MOUSE_SHAPE_POINTER);
        }
        ghostty_cursor_name(self.shape)
    }

    fn reset(&mut self) {
        *self = Self::default();
    }
}

fn ghostty_cursor_name(shape: c_int) -> &'static str {
    match shape {
        GHOSTTY_MOUSE_SHAPE_DEFAULT => "default",
        GHOSTTY_MOUSE_SHAPE_HELP => "help",
        GHOSTTY_MOUSE_SHAPE_POINTER => "pointer",
        GHOSTTY_MOUSE_SHAPE_CONTEXT_MENU => "context-menu",
        GHOSTTY_MOUSE_SHAPE_PROGRESS => "progress",
        GHOSTTY_MOUSE_SHAPE_WAIT => "wait",
        GHOSTTY_MOUSE_SHAPE_CELL => "cell",
        GHOSTTY_MOUSE_SHAPE_CROSSHAIR => "crosshair",
        GHOSTTY_MOUSE_SHAPE_TEXT => "text",
        GHOSTTY_MOUSE_SHAPE_VERTICAL_TEXT => "vertical-text",
        GHOSTTY_MOUSE_SHAPE_ALIAS => "alias",
        GHOSTTY_MOUSE_SHAPE_COPY => "copy",
        GHOSTTY_MOUSE_SHAPE_NO_DROP => "no-drop",
        GHOSTTY_MOUSE_SHAPE_MOVE => "move",
        GHOSTTY_MOUSE_SHAPE_NOT_ALLOWED => "not-allowed",
        GHOSTTY_MOUSE_SHAPE_GRAB => "grab",
        GHOSTTY_MOUSE_SHAPE_GRABBING => "grabbing",
        GHOSTTY_MOUSE_SHAPE_ALL_SCROLL => "all-scroll",
        GHOSTTY_MOUSE_SHAPE_COL_RESIZE => "col-resize",
        GHOSTTY_MOUSE_SHAPE_ROW_RESIZE => "row-resize",
        GHOSTTY_MOUSE_SHAPE_N_RESIZE => "n-resize",
        GHOSTTY_MOUSE_SHAPE_E_RESIZE => "e-resize",
        GHOSTTY_MOUSE_SHAPE_S_RESIZE => "s-resize",
        GHOSTTY_MOUSE_SHAPE_W_RESIZE => "w-resize",
        GHOSTTY_MOUSE_SHAPE_NE_RESIZE => "ne-resize",
        GHOSTTY_MOUSE_SHAPE_NW_RESIZE => "nw-resize",
        GHOSTTY_MOUSE_SHAPE_SE_RESIZE => "se-resize",
        GHOSTTY_MOUSE_SHAPE_SW_RESIZE => "sw-resize",
        GHOSTTY_MOUSE_SHAPE_EW_RESIZE => "ew-resize",
        GHOSTTY_MOUSE_SHAPE_NS_RESIZE => "ns-resize",
        GHOSTTY_MOUSE_SHAPE_NESW_RESIZE => "nesw-resize",
        GHOSTTY_MOUSE_SHAPE_NWSE_RESIZE => "nwse-resize",
        GHOSTTY_MOUSE_SHAPE_ZOOM_IN => "zoom-in",
        GHOSTTY_MOUSE_SHAPE_ZOOM_OUT => "zoom-out",
        _ => "default",
    }
}

fn ghostty_mouse_visibility(value: c_int) -> Option<bool> {
    match value {
        GHOSTTY_MOUSE_VISIBLE => Some(true),
        GHOSTTY_MOUSE_HIDDEN => Some(false),
        _ => None,
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum GhosttyImKeyEvent {
    False,
    Composing,
    NotComposing,
}

impl Default for GhosttyImKeyEvent {
    fn default() -> Self {
        Self::False
    }
}

#[derive(Default)]
struct GhosttyImState {
    composing: bool,
    in_key_event: GhosttyImKeyEvent,
    key_commit: Option<String>,
    pending_preedit: Option<Option<String>>,
    direct_commits: Vec<String>,
}

impl GhosttyImState {
    fn begin_key_event(&mut self) -> GhosttyImKeyEvent {
        self.key_commit = None;
        self.in_key_event = if self.composing {
            GhosttyImKeyEvent::Composing
        } else {
            GhosttyImKeyEvent::NotComposing
        };
        self.in_key_event
    }

    fn end_key_event(&mut self) {
        self.in_key_event = GhosttyImKeyEvent::False;
        self.key_commit = None;
    }

    fn preedit_start(&mut self) {
        self.composing = true;
        self.key_commit = None;
    }

    fn preedit_changed(&mut self, preedit: String) {
        self.composing = true;
        self.pending_preedit = Some(Some(preedit));
    }

    fn preedit_end(&mut self) {
        self.composing = false;
        self.pending_preedit = Some(None);
    }

    fn commit(&mut self, text: String) {
        if text.is_empty() {
            return;
        }
        match self.in_key_event {
            GhosttyImKeyEvent::NotComposing => {
                self.key_commit = Some(text);
            }
            GhosttyImKeyEvent::False | GhosttyImKeyEvent::Composing => {
                self.composing = false;
                self.pending_preedit = Some(None);
                self.direct_commits.push(text);
            }
        }
    }

    fn should_stop_key_event(&self, prior: GhosttyImKeyEvent, im_handled: bool) -> bool {
        im_handled
            && (self.composing
                || prior == GhosttyImKeyEvent::Composing
                || self.key_commit.is_none())
    }

    fn take_key_commit(&mut self) -> Option<String> {
        self.key_commit.take()
    }

    fn drain_effects(&mut self) -> (Option<Option<String>>, Vec<String>) {
        (
            self.pending_preedit.take(),
            std::mem::take(&mut self.direct_commits),
        )
    }

    fn cancel_composition(&mut self) {
        self.composing = false;
        self.in_key_event = GhosttyImKeyEvent::False;
        self.key_commit = None;
        self.pending_preedit = Some(None);
        self.direct_commits.clear();
    }
}

fn next_ghostty_callback_token() -> u64 {
    NEXT_GHOSTTY_CALLBACK_TOKEN.fetch_add(1, Ordering::Relaxed)
}

fn ghostty_callback_registry() -> &'static Mutex<HashMap<usize, u64>> {
    static REGISTRY: OnceLock<Mutex<HashMap<usize, u64>>> = OnceLock::new();
    REGISTRY.get_or_init(|| Mutex::new(HashMap::new()))
}

fn ghostty_app_callback_registry() -> &'static Mutex<HashMap<usize, (usize, u64)>> {
    static REGISTRY: OnceLock<Mutex<HashMap<usize, (usize, u64)>>> = OnceLock::new();
    REGISTRY.get_or_init(|| Mutex::new(HashMap::new()))
}

fn ghostty_app_userdata_registry() -> &'static Mutex<HashMap<usize, u64>> {
    static REGISTRY: OnceLock<Mutex<HashMap<usize, u64>>> = OnceLock::new();
    REGISTRY.get_or_init(|| Mutex::new(HashMap::new()))
}

fn ghostty_callback_ptr(callbacks: &GtkGhosttyCallbacks) -> usize {
    callbacks as *const GtkGhosttyCallbacks as usize
}

fn register_ghostty_callbacks(callbacks: &GtkGhosttyCallbacks) {
    if let Ok(mut registry) = ghostty_callback_registry().lock() {
        registry.insert(ghostty_callback_ptr(callbacks), callbacks.token);
    }
}

fn unregister_ghostty_callbacks(callbacks: &GtkGhosttyCallbacks) {
    let ptr = ghostty_callback_ptr(callbacks);
    if let Ok(mut registry) = ghostty_callback_registry().lock() {
        if registry.get(&ptr).copied() == Some(callbacks.token) {
            registry.remove(&ptr);
        }
    }
}

fn ghostty_app_callback_ptr(callbacks: &GtkGhosttyAppCallbacks) -> usize {
    callbacks as *const GtkGhosttyAppCallbacks as usize
}

fn register_ghostty_app_userdata(callbacks: &GtkGhosttyAppCallbacks) {
    if let Ok(mut registry) = ghostty_app_userdata_registry().lock() {
        registry.insert(ghostty_app_callback_ptr(callbacks), callbacks.token);
    }
}

fn register_ghostty_app(callbacks: &GtkGhosttyAppCallbacks) {
    register_ghostty_app_userdata(callbacks);
    if let Some(app) = callbacks.app {
        if let Ok(mut registry) = ghostty_app_callback_registry().lock() {
            registry.insert(
                app as usize,
                (ghostty_app_callback_ptr(callbacks), callbacks.token),
            );
        }
    }
}

fn unregister_ghostty_app(callbacks: &GtkGhosttyAppCallbacks) {
    let ptr = ghostty_app_callback_ptr(callbacks);
    if let Ok(mut registry) = ghostty_app_userdata_registry().lock() {
        if registry.get(&ptr).copied() == Some(callbacks.token) {
            registry.remove(&ptr);
        }
    }
    if let Some(app) = callbacks.app {
        if let Ok(mut registry) = ghostty_app_callback_registry().lock() {
            if registry.get(&(app as usize)).copied() == Some((ptr, callbacks.token)) {
                registry.remove(&(app as usize));
            }
        }
    }
}

fn rotate_ghostty_callback_registration(callbacks: &mut GtkGhosttyCallbacks) {
    unregister_ghostty_callbacks(callbacks);
    callbacks.token = next_ghostty_callback_token();
    register_ghostty_callbacks(callbacks);
}

fn ghostty_callback_token(callbacks: usize) -> Option<u64> {
    ghostty_callback_registry()
        .lock()
        .ok()
        .and_then(|registry| registry.get(&callbacks).copied())
}

fn ghostty_callback_ref(userdata: *mut c_void) -> Option<(usize, u64)> {
    if userdata.is_null() {
        return None;
    }
    let callbacks = userdata as usize;
    let token = ghostty_callback_token(callbacks)?;
    Some((callbacks, token))
}

fn ghostty_app_callback_ref(app: GhosttyApp) -> Option<(usize, u64)> {
    if app.is_null() {
        return None;
    }
    ghostty_app_callback_registry()
        .lock()
        .ok()
        .and_then(|registry| registry.get(&(app as usize)).copied())
        .filter(|(callbacks, token)| {
            ghostty_app_userdata_registry()
                .lock()
                .ok()
                .and_then(|registry| registry.get(callbacks).copied())
                == Some(*token)
        })
}

fn ghostty_app_callback_ref_from_userdata(userdata: *mut c_void) -> Option<(usize, u64)> {
    if userdata.is_null() {
        return None;
    }
    let callbacks = userdata as usize;
    let token = ghostty_app_userdata_registry()
        .lock()
        .ok()
        .and_then(|registry| registry.get(&callbacks).copied())?;
    Some((callbacks, token))
}

fn with_ghostty_callbacks<R>(
    callbacks: usize,
    token: u64,
    f: impl FnOnce(&GtkGhosttyCallbacks) -> R,
) -> Option<R> {
    if ghostty_callback_token(callbacks) != Some(token) {
        return None;
    }
    Some(f(unsafe { &*(callbacks as *const GtkGhosttyCallbacks) }))
}

fn with_ghostty_app_callbacks<R>(
    callbacks: usize,
    token: u64,
    f: impl FnOnce(&GtkGhosttyAppCallbacks) -> R,
) -> Option<R> {
    if ghostty_app_userdata_registry()
        .lock()
        .ok()
        .and_then(|registry| registry.get(&callbacks).copied())
        != Some(token)
    {
        return None;
    }
    Some(f(unsafe { &*(callbacks as *const GtkGhosttyAppCallbacks) }))
}

fn ghostty_callback_ref_for_action(
    app: GhosttyApp,
    target: GhosttyTarget,
) -> Option<(usize, u64, usize)> {
    let (app_callbacks, app_token) = ghostty_app_callback_ref(app)?;
    with_ghostty_app_callbacks(app_callbacks, app_token, |callbacks| {
        let requested = target.surface().map(|surface| surface as usize);
        let focused = callbacks.focused_surface.load(Ordering::Acquire);
        let surfaces = callbacks.surfaces.lock().ok()?;
        requested
            .and_then(|surface| {
                surfaces
                    .get(&surface)
                    .copied()
                    .map(|(callbacks, token)| (callbacks, token, surface))
            })
            .or_else(|| {
                (focused != 0)
                    .then(|| {
                        surfaces
                            .get(&focused)
                            .copied()
                            .map(|(callbacks, token)| (callbacks, token, focused))
                    })
                    .flatten()
            })
            .or_else(|| {
                surfaces
                    .iter()
                    .next()
                    .map(|(surface, (callbacks, token))| (*callbacks, *token, *surface))
            })
    })
    .flatten()
    .filter(|(callbacks, token, _)| ghostty_callback_token(*callbacks) == Some(*token))
}

unsafe extern "C" fn gtk_ghostty_make_current(userdata: *mut c_void) -> bool {
    if !glib::MainContext::default().is_owner() {
        return false;
    }
    let Some((callbacks, token)) = ghostty_callback_ref(userdata) else {
        return false;
    };
    with_ghostty_callbacks(callbacks, token, |callbacks| {
        let Some(area) = callbacks.area.as_ref() else {
            return false;
        };
        area.make_current();
        area.error().is_none()
    })
    .unwrap_or(false)
}

unsafe extern "C" fn gtk_ghostty_get_proc_address(
    _userdata: *mut c_void,
    name: *const c_char,
) -> *mut c_void {
    if name.is_null() {
        return ptr::null_mut();
    }
    gtk_gl_proc_resolver().resolve(name)
}

unsafe extern "C" fn gtk_ghostty_wakeup(userdata: *mut c_void) {
    let Some((callbacks, token)) = ghostty_app_callback_ref_from_userdata(userdata) else {
        return;
    };
    if glib::MainContext::default().is_owner() {
        gtk_ghostty_wakeup_on_main(callbacks, token);
    } else {
        glib::idle_add_once(move || {
            gtk_ghostty_wakeup_on_main(callbacks, token);
        });
    }
}

fn gtk_ghostty_wakeup_on_main(callbacks: usize, token: u64) {
    let _ = with_ghostty_app_callbacks(callbacks, token, |callbacks| {
        let (Some(app), Some(tick)) = (callbacks.app, callbacks.app_tick) else {
            return;
        };
        unsafe {
            let _ = tick(app);
        }
    });
}

unsafe extern "C" fn gtk_ghostty_manual_io_write(
    userdata: *mut c_void,
    bytes: *const c_char,
    len: usize,
) {
    if bytes.is_null() || len == 0 {
        return;
    }
    let Some((callbacks, token)) = ghostty_callback_ref(userdata) else {
        return;
    };
    let Some((app_state, surface_id)) = with_ghostty_callbacks(callbacks, token, |callbacks| {
        Some((
            Arc::clone(callbacks.app_state.as_ref()?),
            callbacks.close_surface_id.clone()?,
        ))
    })
    .flatten() else {
        return;
    };
    let bytes = std::slice::from_raw_parts(bytes.cast::<u8>(), len);
    if let Ok(mut app) = app_state.lock() {
        let _ = app.remote_tmux_surface_input(&surface_id, bytes);
    };
}

unsafe extern "C" fn gtk_ghostty_redraw_surface(userdata: *mut c_void) {
    let Some((callbacks, token)) = ghostty_callback_ref(userdata) else {
        return;
    };
    if glib::MainContext::default().is_owner() {
        gtk_ghostty_redraw_surface_on_main(callbacks, token);
    } else {
        glib::idle_add_once(move || {
            gtk_ghostty_redraw_surface_on_main(callbacks, token);
        });
    }
}

fn gtk_ghostty_redraw_surface_on_main(callbacks: usize, token: u64) {
    let _ = with_ghostty_callbacks(callbacks, token, |callbacks| {
        if let Some(area) = callbacks.redraw_area() {
            area.queue_render();
        }
    });
}

unsafe extern "C" fn gtk_ghostty_close_surface(userdata: *mut c_void, process_alive: bool) {
    let Some((callbacks, token)) = ghostty_callback_ref(userdata) else {
        return;
    };
    if glib::MainContext::default().is_owner() {
        gtk_ghostty_close_surface_on_main(callbacks, token, process_alive);
    } else {
        glib::idle_add_once(move || {
            gtk_ghostty_close_surface_on_main(callbacks, token, process_alive);
        });
    }
}

fn gtk_ghostty_close_surface_on_main(callbacks: usize, token: u64, process_alive: bool) {
    let Some((app_state, surface_id, surface, needs_confirm_quit)) =
        with_ghostty_callbacks(callbacks, token, gtk_ghostty_close_surface_context).flatten()
    else {
        return;
    };
    let needs_confirm = match needs_confirm_quit {
        Some(needs_confirm_quit) => unsafe { needs_confirm_quit(surface) },
        None => false,
    };
    let Ok(mut app) = app_state.lock() else {
        return;
    };
    if let Some(value) = gtk_ghostty_close_request_value(process_alive, needs_confirm) {
        let _ = app.record_embedded_terminal_ui_action(
            &surface_id,
            "close_requested",
            Some(value),
            None,
        );
        return;
    }
    let _ = app.handle(
        "surface.close",
        &json!({
            "surface_id": surface_id,
            "process_alive": process_alive,
            "source": if process_alive { "ghostty_runtime" } else { "child_exited" }
        }),
    );
}

fn gtk_ghostty_close_surface_context(
    callbacks: &GtkGhosttyCallbacks,
) -> Option<(
    Arc<Mutex<AppState>>,
    String,
    GhosttySurface,
    Option<GhosttySurfaceNeedsConfirmQuit>,
)> {
    let surface = callbacks.surface?;
    let app_state = callbacks.app_state.as_ref()?;
    let surface_id = callbacks.close_surface_id.as_deref()?;
    Some((
        Arc::clone(app_state),
        surface_id.to_string(),
        surface,
        callbacks.surface_needs_confirm_quit,
    ))
}

fn gtk_ghostty_close_request_value(
    process_alive: bool,
    needs_confirm: bool,
) -> Option<&'static str> {
    if !needs_confirm {
        None
    } else if process_alive {
        Some("needs_confirm:process_alive")
    } else {
        Some("needs_confirm")
    }
}

fn gtk_ghostty_target_needs_confirm_quit(target: &GtkGhosttyActionTarget) -> bool {
    let (Some(surface), Some(needs_confirm_quit)) =
        (target.surface, target.surface_needs_confirm_quit)
    else {
        return false;
    };
    unsafe { needs_confirm_quit(surface) }
}

fn gtk_ghostty_app_close_request_action(action: &str, needs_confirm: bool) -> Option<&'static str> {
    if !needs_confirm {
        return None;
    }
    match action {
        "quit" => Some("quit_requested"),
        "close_all_windows" => Some("close_all_windows_requested"),
        _ => Some("close_requested"),
    }
}

fn gtk_ghostty_window_close_request_action(
    action: &str,
    needs_confirm: bool,
) -> Option<&'static str> {
    if needs_confirm && action == "close_window" {
        Some("close_window_requested")
    } else {
        None
    }
}

fn gtk_ghostty_current_tab_close_request_action(
    mode: &str,
    needs_confirm: bool,
) -> Option<&'static str> {
    if needs_confirm && mode == "this" {
        Some("close_tab_requested")
    } else {
        None
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
enum GtkGhosttyActionScope {
    App,
    Surface,
}

#[derive(Clone, Debug, Eq, PartialEq)]
enum GtkGhosttyActionEvent {
    SetTitle {
        surface: usize,
        title: String,
    },
    Pwd {
        surface: usize,
        pwd: String,
    },
    DesktopNotification {
        surface: usize,
        title: String,
        body: String,
    },
    ToggleCommandPalette {
        surface: usize,
    },
    OpenConfig {
        surface: usize,
    },
    ReloadConfig {
        surface: usize,
        scope: GtkGhosttyActionScope,
        soft: bool,
    },
    Readonly {
        surface: usize,
        readonly: bool,
    },
    CopyTitleToClipboard {
        surface: usize,
    },
    SelectionChanged {
        surface: usize,
    },
    PresentTerminal {
        surface: usize,
    },
    SizeLimit {
        surface: usize,
        min_width: u32,
        min_height: u32,
        max_width: u32,
        max_height: u32,
    },
    InitialSize {
        surface: usize,
        width: u32,
        height: u32,
    },
    CellSize {
        surface: usize,
        width: u32,
        height: u32,
    },
    RendererHealth {
        surface: usize,
        status: &'static str,
    },
    PromptTitle {
        surface: usize,
        target: &'static str,
    },
    QuitTimer {
        surface: usize,
        mode: &'static str,
    },
    FloatWindow {
        surface: usize,
        mode: &'static str,
    },
    SecureInput {
        surface: usize,
        mode: &'static str,
    },
    ColorChange {
        surface: usize,
        kind: &'static str,
        palette_index: Option<u32>,
        r: u8,
        g: u8,
        b: u8,
    },
    ConfigChange {
        surface: usize,
    },
    WindowAction {
        surface: usize,
        action: &'static str,
        value: Option<&'static str>,
    },
    TabAction {
        surface: usize,
        action: &'static str,
        amount: Option<i64>,
    },
    Render {
        surface: usize,
    },
    Quit {
        surface: usize,
    },
    CloseAllWindows {
        surface: usize,
    },
    UiAction {
        surface: usize,
        action: &'static str,
        value: Option<&'static str>,
        amount: Option<i64>,
    },
    KeySequence {
        surface: usize,
        active: bool,
        trigger: String,
    },
    KeyTable {
        surface: usize,
        mode: &'static str,
        name: Option<String>,
    },
    ShowOnScreenKeyboard {
        surface: usize,
    },
    NewTab {
        surface: usize,
    },
    NewSplit {
        surface: usize,
        direction: &'static str,
    },
    CloseTab {
        surface: usize,
        mode: &'static str,
    },
    GotoSplit {
        surface: usize,
        direction: &'static str,
    },
    ResizeSplit {
        surface: usize,
        direction: &'static str,
        amount: u16,
    },
    EqualizeSplits {
        surface: usize,
    },
    ToggleSplitZoom {
        surface: usize,
    },
    OpenUrl {
        surface: usize,
        kind: c_int,
        url: String,
    },
    ProgressReport {
        surface: usize,
        state: &'static str,
        progress: Option<i8>,
    },
    CommandFinished {
        surface: usize,
        exit_code: Option<i16>,
        duration_ns: u64,
    },
    StartSearch {
        surface: usize,
        needle: String,
    },
    EndSearch {
        surface: usize,
    },
    SearchTotal {
        surface: usize,
        total: Option<u64>,
    },
    SearchSelected {
        surface: usize,
        selected: Option<u64>,
    },
    Scrollbar {
        surface: usize,
        total: u64,
        offset: u64,
        len: u64,
    },
    ShowChildExited {
        surface: usize,
        exit_code: u32,
        runtime_ms: u64,
    },
    MouseOverLink {
        surface: usize,
        url: Option<String>,
    },
    MouseShape {
        surface: usize,
        shape: c_int,
    },
    MouseVisibility {
        surface: usize,
        visible: bool,
    },
    RingBell {
        surface: usize,
    },
}

impl GtkGhosttyActionEvent {
    fn surface(&self) -> usize {
        match self {
            Self::SetTitle { surface, .. }
            | Self::Pwd { surface, .. }
            | Self::DesktopNotification { surface, .. }
            | Self::ToggleCommandPalette { surface }
            | Self::OpenConfig { surface }
            | Self::ReloadConfig { surface, .. }
            | Self::Readonly { surface, .. }
            | Self::CopyTitleToClipboard { surface }
            | Self::SelectionChanged { surface }
            | Self::PresentTerminal { surface }
            | Self::SizeLimit { surface, .. }
            | Self::InitialSize { surface, .. }
            | Self::CellSize { surface, .. }
            | Self::RendererHealth { surface, .. }
            | Self::PromptTitle { surface, .. }
            | Self::QuitTimer { surface, .. }
            | Self::FloatWindow { surface, .. }
            | Self::SecureInput { surface, .. }
            | Self::ColorChange { surface, .. }
            | Self::ConfigChange { surface }
            | Self::WindowAction { surface, .. }
            | Self::TabAction { surface, .. }
            | Self::Render { surface }
            | Self::Quit { surface }
            | Self::CloseAllWindows { surface }
            | Self::UiAction { surface, .. }
            | Self::KeySequence { surface, .. }
            | Self::KeyTable { surface, .. }
            | Self::ShowOnScreenKeyboard { surface }
            | Self::NewTab { surface }
            | Self::NewSplit { surface, .. }
            | Self::CloseTab { surface, .. }
            | Self::GotoSplit { surface, .. }
            | Self::ResizeSplit { surface, .. }
            | Self::EqualizeSplits { surface }
            | Self::ToggleSplitZoom { surface }
            | Self::OpenUrl { surface, .. }
            | Self::ProgressReport { surface, .. }
            | Self::CommandFinished { surface, .. }
            | Self::StartSearch { surface, .. }
            | Self::EndSearch { surface }
            | Self::SearchTotal { surface, .. }
            | Self::SearchSelected { surface, .. }
            | Self::Scrollbar { surface, .. }
            | Self::ShowChildExited { surface, .. }
            | Self::MouseOverLink { surface, .. }
            | Self::MouseShape { surface, .. }
            | Self::MouseVisibility { surface, .. }
            | Self::RingBell { surface } => *surface,
        }
    }
}

struct GtkGhosttyActionTarget {
    area: Option<gtk::GLArea>,
    app: Option<GhosttyApp>,
    surface: Option<GhosttySurface>,
    config_new: Option<GhosttyConfigNew>,
    config_free: Option<GhosttyConfigFree>,
    config_load_default_files: Option<GhosttyConfigLoadDefaultFiles>,
    config_load_recursive_files: Option<GhosttyConfigLoadRecursiveFiles>,
    config_finalize: Option<GhosttyConfigFinalize>,
    config_diagnostics_count: Option<GhosttyConfigDiagnosticsCount>,
    config_get_diagnostic: Option<GhosttyConfigGetDiagnostic>,
    config_open_path: Option<GhosttyConfigOpenPath>,
    string_free: Option<GhosttyStringFree>,
    app_update_config: Option<GhosttyAppUpdateConfig>,
    surface_inherited_config: Option<GhosttySurfaceInheritedConfig>,
    surface_inherited_config_free: Option<GhosttySurfaceInheritedConfigFree>,
    surface_update_config: Option<GhosttySurfaceUpdateConfig>,
    surface_needs_confirm_quit: Option<GhosttySurfaceNeedsConfirmQuit>,
    app_state: Option<Arc<Mutex<AppState>>>,
    surface_id: Option<String>,
    cursor_state: Arc<Mutex<GhosttyCursorState>>,
    selection_sync_requested: Arc<AtomicBool>,
    inspector_visible: Arc<AtomicBool>,
}

fn gtk_ghostty_inherited_options(
    target: &GtkGhosttyActionTarget,
    context: c_int,
) -> Option<EmbeddedTerminalInheritedOptions> {
    let (Some(surface), Some(inherited_config), Some(inherited_config_free)) = (
        target.surface,
        target.surface_inherited_config,
        target.surface_inherited_config_free,
    ) else {
        return None;
    };
    let mut config = unsafe { inherited_config(surface, context) };
    let options = EmbeddedTerminalInheritedOptions {
        working_directory: config.working_directory(),
        font_size: config.font_size(),
    };
    unsafe {
        inherited_config_free(surface, &mut config);
    }
    (options.working_directory.is_some() || options.font_size.is_some()).then_some(options)
}

unsafe extern "C" fn gtk_ghostty_action(
    app: GhosttyApp,
    target: GhosttyTarget,
    action: GhosttyAction,
) -> bool {
    let Some((callbacks, token, fallback_surface)) = ghostty_callback_ref_for_action(app, target)
    else {
        return false;
    };
    let Some(event) = ghostty_action_event_with_fallback(target, action, Some(fallback_surface))
    else {
        return false;
    };
    if glib::MainContext::default().is_owner() {
        gtk_ghostty_action_on_main(callbacks, token, event);
    } else {
        glib::idle_add_once(move || {
            gtk_ghostty_action_on_main(callbacks, token, event);
        });
    }
    true
}

fn gtk_ghostty_action_on_main(callbacks: usize, token: u64, event: GtkGhosttyActionEvent) {
    let Some(target) = with_ghostty_callbacks(callbacks, token, |callbacks| {
        if callbacks.surface.map(|surface| surface as usize) != Some(event.surface()) {
            return None;
        }
        Some(GtkGhosttyActionTarget {
            area: callbacks.area.clone(),
            app: callbacks.app,
            surface: callbacks.surface,
            config_new: callbacks.config_new,
            config_free: callbacks.config_free,
            config_load_default_files: callbacks.config_load_default_files,
            config_load_recursive_files: callbacks.config_load_recursive_files,
            config_finalize: callbacks.config_finalize,
            config_diagnostics_count: callbacks.config_diagnostics_count,
            config_get_diagnostic: callbacks.config_get_diagnostic,
            config_open_path: callbacks.config_open_path,
            string_free: callbacks.string_free,
            app_update_config: callbacks.app_update_config,
            surface_inherited_config: callbacks.surface_inherited_config,
            surface_inherited_config_free: callbacks.surface_inherited_config_free,
            surface_update_config: callbacks.surface_update_config,
            surface_needs_confirm_quit: callbacks.surface_needs_confirm_quit,
            app_state: callbacks.app_state.as_ref().map(Arc::clone),
            surface_id: callbacks.close_surface_id.clone(),
            cursor_state: Arc::clone(&callbacks.cursor_state),
            selection_sync_requested: Arc::clone(&callbacks.selection_sync_requested),
            inspector_visible: Arc::clone(&callbacks.inspector_visible),
        })
    })
    .flatten() else {
        return;
    };

    match event {
        GtkGhosttyActionEvent::Render { .. } => {
            if let Some(area) = target.area.as_ref() {
                area.queue_render();
            }
            return;
        }
        GtkGhosttyActionEvent::Quit { .. } | GtkGhosttyActionEvent::CloseAllWindows { .. } => {
            let action = match event {
                GtkGhosttyActionEvent::Quit { .. } => "quit",
                GtkGhosttyActionEvent::CloseAllWindows { .. } => "close_all_windows",
                _ => unreachable!("matched quit or close-all-windows"),
            };
            let needs_confirm = gtk_ghostty_target_needs_confirm_quit(&target);
            if let (Some(app_state), Some(surface_id)) =
                (target.app_state.as_ref(), target.surface_id.as_ref())
            {
                if let Ok(mut app) = app_state.lock() {
                    let _ =
                        app.update_embedded_terminal_close_confirmation(surface_id, needs_confirm);
                    let _ = app.record_embedded_terminal_app_action(surface_id, action);
                    let result = app
                        .handle(
                            "app.quit.request",
                            &json!({"source": "ghostty", "surface_id": surface_id}),
                        )
                        .ok();
                    if result
                        .as_ref()
                        .is_some_and(|value| value["blocked"] == true)
                    {
                        let request_action = gtk_ghostty_app_close_request_action(action, true)
                            .unwrap_or("quit_requested");
                        let _ = app.record_embedded_terminal_ui_action(
                            surface_id,
                            request_action,
                            Some("needs_confirm"),
                            None,
                        );
                    }
                }
            }
            return;
        }
        GtkGhosttyActionEvent::MouseShape { shape, .. } => {
            gtk_ghostty_record_cursor_shape(&target, shape);
            if let Some(area) = target.area.as_ref() {
                gtk_ghostty_apply_cursor_shape(area, &target.cursor_state, shape);
            }
            return;
        }
        GtkGhosttyActionEvent::MouseVisibility { visible, .. } => {
            gtk_ghostty_record_cursor_visibility(&target, visible);
            if let Some(area) = target.area.as_ref() {
                gtk_ghostty_apply_cursor_visibility(area, &target.cursor_state, visible);
            }
            return;
        }
        GtkGhosttyActionEvent::MouseOverLink { ref url, .. } => {
            gtk_ghostty_record_link_hover(&target, url.as_deref());
            if let Some(area) = target.area.as_ref() {
                gtk_ghostty_apply_link_hover(area, &target.cursor_state, url.is_some());
            }
            return;
        }
        _ => {}
    }

    if let GtkGhosttyActionEvent::ReloadConfig { ref scope, .. } = event {
        gtk_ghostty_reload_config(&target, scope);
    }

    if matches!(event, GtkGhosttyActionEvent::CopyTitleToClipboard { .. }) {
        let (Some(app_state), Some(surface_id), Some(area)) = (
            target.app_state.as_ref(),
            target.surface_id.as_ref(),
            target.area.as_ref(),
        ) else {
            return;
        };
        let Ok(app) = app_state.lock() else {
            return;
        };
        if let Ok(Some(title)) = app.embedded_terminal_clipboard_title(surface_id) {
            area.clipboard().set_text(&title);
        }
        return;
    }

    if matches!(event, GtkGhosttyActionEvent::SelectionChanged { .. }) {
        target
            .selection_sync_requested
            .store(true, Ordering::Release);
        if let Some(area) = target.area.as_ref() {
            area.queue_render();
        }
        return;
    }

    if matches!(event, GtkGhosttyActionEvent::ShowOnScreenKeyboard { .. }) {
        if let Some(area) = target.area.as_ref() {
            area.grab_focus();
            area.queue_render();
        }
    }

    if let GtkGhosttyActionEvent::UiAction { action, value, .. } = &event {
        if *action == "inspector" {
            if let Some(next) = gtk_ghostty_next_inspector_visible(
                target.inspector_visible.load(Ordering::Acquire),
                *value,
            ) {
                target.inspector_visible.store(next, Ordering::Release);
                if let Some(area) = target.area.as_ref() {
                    area.queue_render();
                }
            }
        } else if *action == "render_inspector" {
            if let Some(area) = target.area.as_ref() {
                area.queue_render();
            }
        } else if let Some(area) = target.area.as_ref() {
            gtk_ghostty_handle_window_ui_action(area, action, *value);
        }
    }

    let (Some(app_state), Some(surface_id)) = (
        target.app_state.as_ref().map(Arc::clone),
        target.surface_id.clone(),
    ) else {
        return;
    };
    let Ok(mut app) = app_state.lock() else {
        return;
    };
    match event {
        GtkGhosttyActionEvent::SetTitle { title, .. } => {
            let _ = app.apply_embedded_terminal_title(&surface_id, &title);
        }
        GtkGhosttyActionEvent::Pwd { pwd, .. } => {
            let _ = app.apply_embedded_terminal_pwd(&surface_id, &pwd);
        }
        GtkGhosttyActionEvent::DesktopNotification { title, body, .. } => {
            let _ = app.create_embedded_terminal_notification(&surface_id, &title, &body);
        }
        GtkGhosttyActionEvent::ToggleCommandPalette { .. } => {
            let _ = app.toggle_embedded_terminal_command_palette(&surface_id);
        }
        GtkGhosttyActionEvent::OpenConfig { .. } => {
            let config_path = gtk_ghostty_config_open_path(&target);
            let _ = if let Some(config_path) = config_path.as_deref() {
                app.open_embedded_terminal_config_with_path(&surface_id, Some(config_path))
            } else {
                app.open_embedded_terminal_config(&surface_id)
            };
        }
        GtkGhosttyActionEvent::ReloadConfig { soft, .. } => {
            let _ = app.reload_embedded_terminal_config(&surface_id, soft);
        }
        GtkGhosttyActionEvent::Readonly { readonly, .. } => {
            let _ = app.set_embedded_terminal_readonly(&surface_id, readonly);
        }
        GtkGhosttyActionEvent::CopyTitleToClipboard { .. } => {}
        GtkGhosttyActionEvent::SelectionChanged { .. } => {}
        GtkGhosttyActionEvent::PresentTerminal { .. } => {
            let _ = app.perform_embedded_terminal_layout_action(
                &surface_id,
                "present_terminal",
                None,
                None,
            );
        }
        GtkGhosttyActionEvent::SizeLimit {
            min_width,
            min_height,
            max_width,
            max_height,
            ..
        } => {
            let _ = app.update_embedded_terminal_size_limit(
                &surface_id,
                min_width,
                min_height,
                max_width,
                max_height,
            );
        }
        GtkGhosttyActionEvent::InitialSize { width, height, .. } => {
            let _ = app.update_embedded_terminal_initial_size(&surface_id, width, height);
        }
        GtkGhosttyActionEvent::CellSize { width, height, .. } => {
            let _ = app.update_embedded_terminal_cell_size(&surface_id, width, height);
        }
        GtkGhosttyActionEvent::RendererHealth { status, .. } => {
            let _ = app.update_embedded_terminal_renderer_health(&surface_id, status);
        }
        GtkGhosttyActionEvent::PromptTitle { target, .. } => {
            let _ = app.record_embedded_terminal_prompt_title(&surface_id, target);
        }
        GtkGhosttyActionEvent::QuitTimer { mode, .. } => {
            let _ = app.update_embedded_terminal_quit_timer(&surface_id, mode);
        }
        GtkGhosttyActionEvent::FloatWindow { mode, .. } => {
            let _ = app.update_embedded_terminal_float_window(&surface_id, mode);
        }
        GtkGhosttyActionEvent::SecureInput { mode, .. } => {
            let _ = app.update_embedded_terminal_secure_input(&surface_id, mode);
        }
        GtkGhosttyActionEvent::ColorChange {
            kind,
            palette_index,
            r,
            g,
            b,
            ..
        } => {
            let _ = app.record_embedded_terminal_color_change(
                &surface_id,
                kind,
                palette_index,
                r,
                g,
                b,
            );
        }
        GtkGhosttyActionEvent::ConfigChange { .. } => {
            let _ = app.record_embedded_terminal_config_change(&surface_id);
        }
        GtkGhosttyActionEvent::WindowAction { action, value, .. } => {
            let needs_confirm = gtk_ghostty_target_needs_confirm_quit(&target);
            if let Some(request_action) =
                gtk_ghostty_window_close_request_action(action, needs_confirm)
            {
                let _ = app.record_embedded_terminal_window_action(&surface_id, action, value);
                let _ = app.record_embedded_terminal_ui_action(
                    &surface_id,
                    request_action,
                    Some("needs_confirm"),
                    None,
                );
            } else {
                let _ = app.perform_embedded_terminal_window_action(&surface_id, action, value);
            }
        }
        GtkGhosttyActionEvent::TabAction { action, amount, .. } => {
            let _ = app.perform_embedded_terminal_tab_action(&surface_id, action, amount);
        }
        GtkGhosttyActionEvent::Render { .. }
        | GtkGhosttyActionEvent::Quit { .. }
        | GtkGhosttyActionEvent::CloseAllWindows { .. } => {}
        GtkGhosttyActionEvent::UiAction {
            action,
            value,
            amount,
            ..
        } => {
            if action == "toggle_tab_overview" {
                let _ = app.toggle_embedded_terminal_tab_overview(&surface_id);
            }
            let _ = app.record_embedded_terminal_ui_action(&surface_id, action, value, amount);
        }
        GtkGhosttyActionEvent::KeySequence {
            active, trigger, ..
        } => {
            let _ = app.update_embedded_terminal_key_sequence(&surface_id, active, &trigger);
        }
        GtkGhosttyActionEvent::KeyTable { mode, name, .. } => {
            let _ = app.update_embedded_terminal_key_table(&surface_id, mode, name.as_deref());
        }
        GtkGhosttyActionEvent::ShowOnScreenKeyboard { .. } => {
            let _ = app.request_embedded_terminal_on_screen_keyboard(&surface_id);
        }
        GtkGhosttyActionEvent::NewTab { .. } => {
            let inherited_options =
                gtk_ghostty_inherited_options(&target, GHOSTTY_SURFACE_CONTEXT_TAB);
            let _ = app.perform_embedded_terminal_layout_action_with_inherited_options(
                &surface_id,
                "new_tab",
                None,
                None,
                inherited_options,
            );
        }
        GtkGhosttyActionEvent::NewSplit { direction, .. } => {
            let inherited_options =
                gtk_ghostty_inherited_options(&target, GHOSTTY_SURFACE_CONTEXT_SPLIT);
            let _ = app.perform_embedded_terminal_layout_action_with_inherited_options(
                &surface_id,
                "new_split",
                Some(direction),
                None,
                inherited_options,
            );
        }
        GtkGhosttyActionEvent::CloseTab { mode, .. } => {
            let needs_confirm = gtk_ghostty_target_needs_confirm_quit(&target);
            if let Some(request_action) =
                gtk_ghostty_current_tab_close_request_action(mode, needs_confirm)
            {
                let _ = app.record_embedded_terminal_layout_action(
                    &surface_id,
                    "close_tab",
                    Some(mode),
                    None,
                );
                let _ = app.record_embedded_terminal_ui_action(
                    &surface_id,
                    request_action,
                    Some("needs_confirm"),
                    None,
                );
            } else {
                let _ = app.perform_embedded_terminal_layout_action(
                    &surface_id,
                    "close_tab",
                    Some(mode),
                    None,
                );
            }
        }
        GtkGhosttyActionEvent::GotoSplit { direction, .. } => {
            let _ = app.perform_embedded_terminal_layout_action(
                &surface_id,
                "goto_split",
                Some(direction),
                None,
            );
        }
        GtkGhosttyActionEvent::ResizeSplit {
            direction, amount, ..
        } => {
            let _ = app.perform_embedded_terminal_layout_action(
                &surface_id,
                "resize_split",
                Some(direction),
                Some(amount),
            );
        }
        GtkGhosttyActionEvent::EqualizeSplits { .. } => {
            let _ = app.perform_embedded_terminal_layout_action(
                &surface_id,
                "equalize_splits",
                None,
                None,
            );
        }
        GtkGhosttyActionEvent::ToggleSplitZoom { .. } => {
            let _ = app.perform_embedded_terminal_layout_action(
                &surface_id,
                "toggle_split_zoom",
                None,
                None,
            );
        }
        GtkGhosttyActionEvent::OpenUrl { url, .. } => {
            let _ = app.open_embedded_terminal_url(&surface_id, &url);
        }
        GtkGhosttyActionEvent::ProgressReport {
            state, progress, ..
        } => {
            let _ = app.update_embedded_terminal_progress_report(&surface_id, state, progress);
        }
        GtkGhosttyActionEvent::CommandFinished {
            exit_code,
            duration_ns,
            ..
        } => {
            let _ =
                app.record_embedded_terminal_command_finished(&surface_id, exit_code, duration_ns);
        }
        GtkGhosttyActionEvent::StartSearch { needle, .. } => {
            let _ = app.start_embedded_terminal_search(&surface_id, &needle);
        }
        GtkGhosttyActionEvent::EndSearch { .. } => {
            let _ = app.end_embedded_terminal_search(&surface_id);
        }
        GtkGhosttyActionEvent::SearchTotal { total, .. } => {
            let _ = app.update_embedded_terminal_search_total(&surface_id, total);
        }
        GtkGhosttyActionEvent::SearchSelected { selected, .. } => {
            let _ = app.update_embedded_terminal_search_selected(&surface_id, selected);
        }
        GtkGhosttyActionEvent::Scrollbar {
            total, offset, len, ..
        } => {
            let _ = app.update_embedded_terminal_scrollbar(&surface_id, total, offset, len);
        }
        GtkGhosttyActionEvent::ShowChildExited {
            exit_code,
            runtime_ms,
            ..
        } => {
            let _ = app.record_embedded_terminal_child_exited(&surface_id, exit_code, runtime_ms);
        }
        GtkGhosttyActionEvent::RingBell { .. } => {
            let _ = app.ring_embedded_terminal_bell(&surface_id);
        }
        GtkGhosttyActionEvent::MouseShape { .. }
        | GtkGhosttyActionEvent::MouseOverLink { .. }
        | GtkGhosttyActionEvent::MouseVisibility { .. } => {}
    }
}

fn gtk_ghostty_record_cursor_shape(target: &GtkGhosttyActionTarget, shape: c_int) {
    let (Some(app_state), Some(surface_id)) =
        (target.app_state.as_ref(), target.surface_id.as_ref())
    else {
        return;
    };
    let Ok(mut app) = app_state.lock() else {
        return;
    };
    let _ = app.update_embedded_terminal_cursor_shape(surface_id, ghostty_cursor_name(shape));
}

fn gtk_ghostty_record_cursor_visibility(target: &GtkGhosttyActionTarget, visible: bool) {
    let (Some(app_state), Some(surface_id)) =
        (target.app_state.as_ref(), target.surface_id.as_ref())
    else {
        return;
    };
    let Ok(mut app) = app_state.lock() else {
        return;
    };
    let _ = app.update_embedded_terminal_cursor_visibility(surface_id, visible);
}

fn gtk_ghostty_record_link_hover(target: &GtkGhosttyActionTarget, url: Option<&str>) {
    let (Some(app_state), Some(surface_id)) =
        (target.app_state.as_ref(), target.surface_id.as_ref())
    else {
        return;
    };
    let Ok(mut app) = app_state.lock() else {
        return;
    };
    let _ = app.update_embedded_terminal_link_hover(surface_id, url);
}

fn gtk_ghostty_apply_cursor_shape(
    area: &gtk::GLArea,
    cursor_state: &Arc<Mutex<GhosttyCursorState>>,
    shape: c_int,
) {
    let Ok(mut state) = cursor_state.lock() else {
        return;
    };
    state.shape = shape;
    area.set_cursor_from_name(Some(state.cursor_name()));
}

fn gtk_ghostty_apply_link_hover(
    area: &gtk::GLArea,
    cursor_state: &Arc<Mutex<GhosttyCursorState>>,
    link_hover: bool,
) {
    let Ok(mut state) = cursor_state.lock() else {
        return;
    };
    state.link_hover = link_hover;
    area.set_cursor_from_name(Some(state.cursor_name()));
}

fn gtk_ghostty_apply_cursor_visibility(
    area: &gtk::GLArea,
    cursor_state: &Arc<Mutex<GhosttyCursorState>>,
    visible: bool,
) {
    let Ok(mut state) = cursor_state.lock() else {
        return;
    };
    state.visible = visible;
    area.set_cursor_from_name(Some(state.cursor_name()));
}

struct GtkGhosttyReloadConfigGuard {
    config: GhosttyConfig,
    free: GhosttyConfigFree,
}

impl Drop for GtkGhosttyReloadConfigGuard {
    fn drop(&mut self) {
        if !self.config.is_null() {
            unsafe {
                (self.free)(self.config);
            }
        }
    }
}

fn gtk_ghostty_config_open_path(target: &GtkGhosttyActionTarget) -> Option<String> {
    let open_path = target.config_open_path?;
    let string_free = target.string_free?;
    let path = unsafe { open_path() };
    ghostty_string_to_string(path, string_free)
}

fn gtk_ghostty_reload_config(
    target: &GtkGhosttyActionTarget,
    scope: &GtkGhosttyActionScope,
) -> bool {
    let (
        Some(config_new),
        Some(config_free),
        Some(config_load_default_files),
        Some(config_load_recursive_files),
        Some(config_finalize),
        Some(config_diagnostics_count),
        Some(config_get_diagnostic),
    ) = (
        target.config_new,
        target.config_free,
        target.config_load_default_files,
        target.config_load_recursive_files,
        target.config_finalize,
        target.config_diagnostics_count,
        target.config_get_diagnostic,
    )
    else {
        return false;
    };

    let config = unsafe { config_new() };
    if config.is_null() {
        return false;
    }
    let config = GtkGhosttyReloadConfigGuard {
        config,
        free: config_free,
    };
    if load_and_finalize_default_config(
        config.config,
        config_load_default_files,
        config_load_recursive_files,
        config_finalize,
        config_diagnostics_count,
        config_get_diagnostic,
    )
    .is_err()
    {
        return false;
    }
    unsafe {
        match scope {
            GtkGhosttyActionScope::App => {
                let (Some(app), Some(update_config)) = (target.app, target.app_update_config)
                else {
                    return false;
                };
                update_config(app, config.config)
            }
            GtkGhosttyActionScope::Surface => {
                let (Some(surface), Some(update_config)) =
                    (target.surface, target.surface_update_config)
                else {
                    return false;
                };
                update_config(surface, config.config)
            }
        }
    }
}

#[cfg(test)]
fn ghostty_action_event(
    target: GhosttyTarget,
    action: GhosttyAction,
) -> Option<GtkGhosttyActionEvent> {
    ghostty_action_event_with_fallback(target, action, None)
}

fn ghostty_action_event_with_fallback(
    target: GhosttyTarget,
    action: GhosttyAction,
    fallback_surface: Option<usize>,
) -> Option<GtkGhosttyActionEvent> {
    let surface = target
        .surface()
        .map(|surface| surface as usize)
        .or(fallback_surface)?;
    match action.tag {
        GHOSTTY_ACTION_SET_TITLE | GHOSTTY_ACTION_SET_TAB_TITLE => {
            let payload = unsafe { action.action.set_title };
            Some(GtkGhosttyActionEvent::SetTitle {
                surface,
                title: ghostty_c_string(payload.title)?,
            })
        }
        GHOSTTY_ACTION_PWD => {
            let payload = unsafe { action.action.pwd };
            Some(GtkGhosttyActionEvent::Pwd {
                surface,
                pwd: ghostty_c_string(payload.pwd)?,
            })
        }
        GHOSTTY_ACTION_DESKTOP_NOTIFICATION => {
            let payload = unsafe { action.action.desktop_notification };
            Some(GtkGhosttyActionEvent::DesktopNotification {
                surface,
                title: ghostty_c_string(payload.title).unwrap_or_default(),
                body: ghostty_c_string(payload.body).unwrap_or_default(),
            })
        }
        GHOSTTY_ACTION_TOGGLE_COMMAND_PALETTE => {
            Some(GtkGhosttyActionEvent::ToggleCommandPalette { surface })
        }
        GHOSTTY_ACTION_OPEN_CONFIG => Some(GtkGhosttyActionEvent::OpenConfig { surface }),
        GHOSTTY_ACTION_RELOAD_CONFIG => {
            let payload = unsafe { action.action.reload_config };
            Some(GtkGhosttyActionEvent::ReloadConfig {
                surface,
                scope: if target.tag == GHOSTTY_TARGET_APP {
                    GtkGhosttyActionScope::App
                } else {
                    GtkGhosttyActionScope::Surface
                },
                soft: payload.soft,
            })
        }
        GHOSTTY_ACTION_READONLY => {
            let readonly = ghostty_readonly(unsafe { action.action.readonly })?;
            Some(GtkGhosttyActionEvent::Readonly { surface, readonly })
        }
        GHOSTTY_ACTION_COPY_TITLE_TO_CLIPBOARD => {
            Some(GtkGhosttyActionEvent::CopyTitleToClipboard { surface })
        }
        GHOSTTY_ACTION_SELECTION_CHANGED => {
            Some(GtkGhosttyActionEvent::SelectionChanged { surface })
        }
        GHOSTTY_ACTION_PRESENT_TERMINAL => Some(GtkGhosttyActionEvent::PresentTerminal { surface }),
        GHOSTTY_ACTION_SIZE_LIMIT => {
            let payload = unsafe { action.action.size_limit };
            Some(GtkGhosttyActionEvent::SizeLimit {
                surface,
                min_width: payload.min_width,
                min_height: payload.min_height,
                max_width: payload.max_width,
                max_height: payload.max_height,
            })
        }
        GHOSTTY_ACTION_INITIAL_SIZE => {
            let payload = unsafe { action.action.initial_size };
            Some(GtkGhosttyActionEvent::InitialSize {
                surface,
                width: payload.width,
                height: payload.height,
            })
        }
        GHOSTTY_ACTION_CELL_SIZE => {
            let payload = unsafe { action.action.cell_size };
            Some(GtkGhosttyActionEvent::CellSize {
                surface,
                width: payload.width,
                height: payload.height,
            })
        }
        GHOSTTY_ACTION_RENDERER_HEALTH => {
            let health = unsafe { action.action.renderer_health };
            Some(GtkGhosttyActionEvent::RendererHealth {
                surface,
                status: ghostty_renderer_health(health)?,
            })
        }
        GHOSTTY_ACTION_PROMPT_TITLE => {
            let target = unsafe { action.action.prompt_title };
            Some(GtkGhosttyActionEvent::PromptTitle {
                surface,
                target: ghostty_prompt_title(target)?,
            })
        }
        GHOSTTY_ACTION_QUIT_TIMER => {
            let mode = unsafe { action.action.quit_timer };
            Some(GtkGhosttyActionEvent::QuitTimer {
                surface,
                mode: ghostty_quit_timer(mode)?,
            })
        }
        GHOSTTY_ACTION_FLOAT_WINDOW => {
            let mode = unsafe { action.action.float_window };
            Some(GtkGhosttyActionEvent::FloatWindow {
                surface,
                mode: ghostty_on_off_toggle(
                    mode,
                    GHOSTTY_FLOAT_WINDOW_ON,
                    GHOSTTY_FLOAT_WINDOW_OFF,
                    GHOSTTY_FLOAT_WINDOW_TOGGLE,
                )?,
            })
        }
        GHOSTTY_ACTION_SECURE_INPUT => {
            let mode = unsafe { action.action.secure_input };
            Some(GtkGhosttyActionEvent::SecureInput {
                surface,
                mode: ghostty_on_off_toggle(
                    mode,
                    GHOSTTY_SECURE_INPUT_ON,
                    GHOSTTY_SECURE_INPUT_OFF,
                    GHOSTTY_SECURE_INPUT_TOGGLE,
                )?,
            })
        }
        GHOSTTY_ACTION_KEY_SEQUENCE => {
            let payload = unsafe { action.action.key_sequence };
            Some(GtkGhosttyActionEvent::KeySequence {
                surface,
                active: payload.active,
                trigger: ghostty_input_trigger(payload.trigger)?,
            })
        }
        GHOSTTY_ACTION_KEY_TABLE => {
            let payload = unsafe { action.action.key_table };
            let (mode, name) = ghostty_key_table(payload)?;
            Some(GtkGhosttyActionEvent::KeyTable {
                surface,
                mode,
                name,
            })
        }
        GHOSTTY_ACTION_COLOR_CHANGE => {
            let payload = unsafe { action.action.color_change };
            let (kind, palette_index) = ghostty_color_kind(payload.kind)?;
            Some(GtkGhosttyActionEvent::ColorChange {
                surface,
                kind,
                palette_index,
                r: payload.r,
                g: payload.g,
                b: payload.b,
            })
        }
        GHOSTTY_ACTION_CONFIG_CHANGE => Some(GtkGhosttyActionEvent::ConfigChange { surface }),
        GHOSTTY_ACTION_NEW_WINDOW => Some(GtkGhosttyActionEvent::WindowAction {
            surface,
            action: "new_window",
            value: None,
        }),
        GHOSTTY_ACTION_CLOSE_WINDOW => Some(GtkGhosttyActionEvent::WindowAction {
            surface,
            action: "close_window",
            value: None,
        }),
        GHOSTTY_ACTION_GOTO_WINDOW => {
            let direction = unsafe { action.action.goto_window };
            Some(GtkGhosttyActionEvent::WindowAction {
                surface,
                action: "goto_window",
                value: Some(ghostty_goto_window(direction)?),
            })
        }
        GHOSTTY_ACTION_MOVE_TAB => {
            let payload = unsafe { action.action.move_tab };
            Some(GtkGhosttyActionEvent::TabAction {
                surface,
                action: "move_tab",
                amount: i64::try_from(payload.amount).ok(),
            })
        }
        GHOSTTY_ACTION_GOTO_TAB => Some(GtkGhosttyActionEvent::TabAction {
            surface,
            action: "goto_tab",
            amount: Some(i64::from(unsafe { action.action.goto_tab })),
        }),
        GHOSTTY_ACTION_QUIT => Some(GtkGhosttyActionEvent::Quit { surface }),
        GHOSTTY_ACTION_CLOSE_ALL_WINDOWS => {
            Some(GtkGhosttyActionEvent::CloseAllWindows { surface })
        }
        GHOSTTY_ACTION_TOGGLE_MAXIMIZE => Some(GtkGhosttyActionEvent::UiAction {
            surface,
            action: "toggle_maximize",
            value: None,
            amount: None,
        }),
        GHOSTTY_ACTION_TOGGLE_FULLSCREEN => {
            let mode = unsafe { action.action.toggle_fullscreen };
            Some(GtkGhosttyActionEvent::UiAction {
                surface,
                action: "toggle_fullscreen",
                value: Some(ghostty_fullscreen(mode)?),
                amount: None,
            })
        }
        GHOSTTY_ACTION_TOGGLE_TAB_OVERVIEW => Some(GtkGhosttyActionEvent::UiAction {
            surface,
            action: "toggle_tab_overview",
            value: None,
            amount: None,
        }),
        GHOSTTY_ACTION_TOGGLE_WINDOW_DECORATIONS => Some(GtkGhosttyActionEvent::UiAction {
            surface,
            action: "toggle_window_decorations",
            value: None,
            amount: None,
        }),
        GHOSTTY_ACTION_TOGGLE_QUICK_TERMINAL => Some(GtkGhosttyActionEvent::UiAction {
            surface,
            action: "toggle_quick_terminal",
            value: None,
            amount: None,
        }),
        GHOSTTY_ACTION_TOGGLE_VISIBILITY => Some(GtkGhosttyActionEvent::UiAction {
            surface,
            action: "toggle_visibility",
            value: None,
            amount: None,
        }),
        GHOSTTY_ACTION_TOGGLE_BACKGROUND_OPACITY => Some(GtkGhosttyActionEvent::UiAction {
            surface,
            action: "toggle_background_opacity",
            value: None,
            amount: None,
        }),
        GHOSTTY_ACTION_TOGGLE_SPLIT_ZOOM => {
            Some(GtkGhosttyActionEvent::ToggleSplitZoom { surface })
        }
        GHOSTTY_ACTION_RESET_WINDOW_SIZE => Some(GtkGhosttyActionEvent::UiAction {
            surface,
            action: "reset_window_size",
            value: None,
            amount: None,
        }),
        GHOSTTY_ACTION_RENDER => Some(GtkGhosttyActionEvent::Render { surface }),
        GHOSTTY_ACTION_INSPECTOR => {
            let mode = unsafe { action.action.inspector };
            Some(GtkGhosttyActionEvent::UiAction {
                surface,
                action: "inspector",
                value: Some(ghostty_inspector(mode)?),
                amount: None,
            })
        }
        GHOSTTY_ACTION_SHOW_GTK_INSPECTOR => Some(GtkGhosttyActionEvent::UiAction {
            surface,
            action: "show_gtk_inspector",
            value: None,
            amount: None,
        }),
        GHOSTTY_ACTION_RENDER_INSPECTOR => Some(GtkGhosttyActionEvent::UiAction {
            surface,
            action: "render_inspector",
            value: None,
            amount: None,
        }),
        GHOSTTY_ACTION_UNDO => Some(GtkGhosttyActionEvent::UiAction {
            surface,
            action: "undo",
            value: None,
            amount: None,
        }),
        GHOSTTY_ACTION_REDO => Some(GtkGhosttyActionEvent::UiAction {
            surface,
            action: "redo",
            value: None,
            amount: None,
        }),
        GHOSTTY_ACTION_CHECK_FOR_UPDATES => Some(GtkGhosttyActionEvent::UiAction {
            surface,
            action: "check_for_updates",
            value: None,
            amount: None,
        }),
        GHOSTTY_ACTION_SHOW_ON_SCREEN_KEYBOARD => {
            Some(GtkGhosttyActionEvent::ShowOnScreenKeyboard { surface })
        }
        GHOSTTY_ACTION_NEW_TAB => Some(GtkGhosttyActionEvent::NewTab { surface }),
        GHOSTTY_ACTION_NEW_SPLIT => {
            let direction = unsafe { action.action.new_split };
            Some(GtkGhosttyActionEvent::NewSplit {
                surface,
                direction: ghostty_split_direction(direction)?,
            })
        }
        GHOSTTY_ACTION_CLOSE_TAB => {
            let mode = unsafe { action.action.close_tab_mode };
            Some(GtkGhosttyActionEvent::CloseTab {
                surface,
                mode: ghostty_close_tab_mode(mode)?,
            })
        }
        GHOSTTY_ACTION_GOTO_SPLIT => {
            let direction = unsafe { action.action.goto_split };
            Some(GtkGhosttyActionEvent::GotoSplit {
                surface,
                direction: ghostty_goto_split_direction(direction)?,
            })
        }
        GHOSTTY_ACTION_RESIZE_SPLIT => {
            let payload = unsafe { action.action.resize_split };
            Some(GtkGhosttyActionEvent::ResizeSplit {
                surface,
                direction: ghostty_resize_split_direction(payload.direction)?,
                amount: payload.amount,
            })
        }
        GHOSTTY_ACTION_EQUALIZE_SPLITS => Some(GtkGhosttyActionEvent::EqualizeSplits { surface }),
        GHOSTTY_ACTION_OPEN_URL => {
            let payload = unsafe { action.action.open_url };
            Some(GtkGhosttyActionEvent::OpenUrl {
                surface,
                kind: payload.kind,
                url: ghostty_sized_string(payload.url, payload.len)?,
            })
        }
        GHOSTTY_ACTION_MOUSE_OVER_LINK => {
            let payload = unsafe { action.action.mouse_over_link };
            let url = ghostty_sized_string(payload.url, payload.len)
                .and_then(|url| (!url.trim().is_empty()).then_some(url));
            Some(GtkGhosttyActionEvent::MouseOverLink { surface, url })
        }
        GHOSTTY_ACTION_PROGRESS_REPORT => {
            let payload = unsafe { action.action.progress_report };
            Some(GtkGhosttyActionEvent::ProgressReport {
                surface,
                state: ghostty_progress_state(payload.state)?,
                progress: (0..=100)
                    .contains(&payload.progress)
                    .then_some(payload.progress),
            })
        }
        GHOSTTY_ACTION_COMMAND_FINISHED => {
            let payload = unsafe { action.action.command_finished };
            Some(GtkGhosttyActionEvent::CommandFinished {
                surface,
                exit_code: (0..=255)
                    .contains(&payload.exit_code)
                    .then_some(payload.exit_code),
                duration_ns: payload.duration,
            })
        }
        GHOSTTY_ACTION_START_SEARCH => {
            let payload = unsafe { action.action.start_search };
            Some(GtkGhosttyActionEvent::StartSearch {
                surface,
                needle: ghostty_c_string(payload.needle).unwrap_or_default(),
            })
        }
        GHOSTTY_ACTION_END_SEARCH => Some(GtkGhosttyActionEvent::EndSearch { surface }),
        GHOSTTY_ACTION_SEARCH_TOTAL => {
            let payload = unsafe { action.action.search_total };
            Some(GtkGhosttyActionEvent::SearchTotal {
                surface,
                total: ghostty_search_count(payload.total),
            })
        }
        GHOSTTY_ACTION_SEARCH_SELECTED => {
            let payload = unsafe { action.action.search_selected };
            Some(GtkGhosttyActionEvent::SearchSelected {
                surface,
                selected: ghostty_search_count(payload.selected),
            })
        }
        GHOSTTY_ACTION_SCROLLBAR => {
            let payload = unsafe { action.action.scrollbar };
            Some(GtkGhosttyActionEvent::Scrollbar {
                surface,
                total: payload.total,
                offset: payload.offset,
                len: payload.len,
            })
        }
        GHOSTTY_ACTION_SHOW_CHILD_EXITED => {
            let payload = unsafe { action.action.child_exited };
            Some(GtkGhosttyActionEvent::ShowChildExited {
                surface,
                exit_code: payload.exit_code,
                runtime_ms: payload.runtime_ms,
            })
        }
        GHOSTTY_ACTION_MOUSE_SHAPE => Some(GtkGhosttyActionEvent::MouseShape {
            surface,
            shape: unsafe { action.action.mouse_shape },
        }),
        GHOSTTY_ACTION_MOUSE_VISIBILITY => {
            let visible = ghostty_mouse_visibility(unsafe { action.action.mouse_visibility })?;
            Some(GtkGhosttyActionEvent::MouseVisibility { surface, visible })
        }
        GHOSTTY_ACTION_RING_BELL => Some(GtkGhosttyActionEvent::RingBell { surface }),
        _ => None,
    }
}

fn ghostty_split_direction(value: c_int) -> Option<&'static str> {
    match value {
        GHOSTTY_SPLIT_DIRECTION_RIGHT => Some("right"),
        GHOSTTY_SPLIT_DIRECTION_DOWN => Some("down"),
        GHOSTTY_SPLIT_DIRECTION_LEFT => Some("left"),
        GHOSTTY_SPLIT_DIRECTION_UP => Some("up"),
        _ => None,
    }
}

fn ghostty_close_tab_mode(value: c_int) -> Option<&'static str> {
    match value {
        GHOSTTY_CLOSE_TAB_MODE_THIS => Some("this"),
        GHOSTTY_CLOSE_TAB_MODE_OTHER => Some("other"),
        GHOSTTY_CLOSE_TAB_MODE_RIGHT => Some("right"),
        _ => None,
    }
}

fn ghostty_goto_split_direction(value: c_int) -> Option<&'static str> {
    match value {
        GHOSTTY_GOTO_SPLIT_PREVIOUS => Some("previous"),
        GHOSTTY_GOTO_SPLIT_NEXT => Some("next"),
        GHOSTTY_GOTO_SPLIT_UP => Some("up"),
        GHOSTTY_GOTO_SPLIT_LEFT => Some("left"),
        GHOSTTY_GOTO_SPLIT_DOWN => Some("down"),
        GHOSTTY_GOTO_SPLIT_RIGHT => Some("right"),
        _ => None,
    }
}

fn ghostty_goto_window(value: c_int) -> Option<&'static str> {
    match value {
        GHOSTTY_GOTO_WINDOW_PREVIOUS => Some("previous"),
        GHOSTTY_GOTO_WINDOW_NEXT => Some("next"),
        _ => None,
    }
}

fn ghostty_resize_split_direction(value: c_int) -> Option<&'static str> {
    match value {
        GHOSTTY_RESIZE_SPLIT_UP => Some("up"),
        GHOSTTY_RESIZE_SPLIT_DOWN => Some("down"),
        GHOSTTY_RESIZE_SPLIT_LEFT => Some("left"),
        GHOSTTY_RESIZE_SPLIT_RIGHT => Some("right"),
        _ => None,
    }
}

fn ghostty_fullscreen(value: c_int) -> Option<&'static str> {
    match value {
        GHOSTTY_FULLSCREEN_NATIVE
        | GHOSTTY_FULLSCREEN_MACOS_NON_NATIVE
        | GHOSTTY_FULLSCREEN_MACOS_NON_NATIVE_VISIBLE_MENU
        | GHOSTTY_FULLSCREEN_MACOS_NON_NATIVE_PADDED_NOTCH => Some("native"),
        _ => None,
    }
}

fn ghostty_inspector(value: c_int) -> Option<&'static str> {
    match value {
        GHOSTTY_INSPECTOR_TOGGLE => Some("toggle"),
        GHOSTTY_INSPECTOR_SHOW => Some("show"),
        GHOSTTY_INSPECTOR_HIDE => Some("hide"),
        _ => None,
    }
}

fn gtk_ghostty_next_inspector_visible(current: bool, action: Option<&str>) -> Option<bool> {
    match action {
        Some("toggle") => Some(!current),
        Some("show") => Some(true),
        Some("hide") => Some(false),
        _ => None,
    }
}

fn gtk_ghostty_handle_window_ui_action(area: &gtk::GLArea, action: &str, value: Option<&str>) {
    match action {
        "show_gtk_inspector" => gtk::Window::set_interactive_debugging(true),
        "toggle_quick_terminal" | "toggle_visibility" => {
            if let Some(window) = gtk_ghostty_root_window(area) {
                gtk_ghostty_toggle_window_visibility(&window);
            }
        }
        "toggle_maximize" => {
            if let Some(window) = gtk_ghostty_root_window(area) {
                if window.is_maximized() {
                    window.unmaximize();
                } else {
                    window.maximize();
                }
            }
        }
        "toggle_fullscreen" => {
            if gtk_ghostty_fullscreen_mode_is_supported(value) {
                if let Some(window) = gtk_ghostty_root_window(area) {
                    if window.is_fullscreen() {
                        window.unfullscreen();
                    } else {
                        window.fullscreen();
                    }
                }
            }
        }
        "toggle_window_decorations" => {
            if let Some(window) = gtk_ghostty_root_window(area) {
                window.set_decorated(!window.is_decorated());
            }
        }
        "reset_window_size" => {
            if let Some(window) = gtk_ghostty_root_window(area) {
                gtk_ghostty_reset_window_size(&window);
            }
        }
        "check_for_updates" => gtk_ghostty_show_update_dialog(area),
        _ => {}
    }
}

fn gtk_ghostty_show_update_dialog(area: &gtk::GLArea) {
    let Some(window) = gtk_ghostty_root_window(area) else {
        return;
    };
    let result = linux_update::check_for_updates();
    let (title, body, release_url) = match result {
        Ok(status) => {
            let title = if status.get("update_available").and_then(Value::as_bool) == Some(true) {
                "cmux Update Available"
            } else {
                "cmux Linux Update"
            };
            (
                title.to_string(),
                linux_update::update_status_text(&status),
                status
                    .get("release_url")
                    .and_then(Value::as_str)
                    .map(ToString::to_string),
            )
        }
        Err(err) => (
            "cmux Update Check Failed".to_string(),
            format!("The latest Linux release could not be checked.\n\n{err}"),
            None,
        ),
    };
    let dialog = gtk::Dialog::builder()
        .title(&title)
        .transient_for(&window)
        .modal(true)
        .destroy_with_parent(true)
        .default_width(520)
        .build();
    dialog.add_button("Close", gtk::ResponseType::Close);
    if release_url.is_some() {
        let open = dialog.add_button("Open Release", gtk::ResponseType::Accept);
        open.add_css_class("suggested-action");
        dialog.set_default_response(gtk::ResponseType::Accept);
    }
    let label = gtk::Label::builder()
        .label(&body)
        .wrap(true)
        .selectable(true)
        .xalign(0.0)
        .build();
    label.set_margin_top(16);
    label.set_margin_bottom(16);
    label.set_margin_start(18);
    label.set_margin_end(18);
    dialog.content_area().append(&label);
    dialog.connect_response(move |dialog, response| {
        if response == gtk::ResponseType::Accept {
            if let Some(url) = release_url.as_deref() {
                let _ = gio::AppInfo::launch_default_for_uri(url, None::<&gio::AppLaunchContext>);
            }
        }
        dialog.close();
    });
    dialog.present();
}

fn gtk_ghostty_toggle_window_visibility(window: &gtk::Window) {
    if window.is_visible() {
        window.hide();
    } else {
        window.present();
    }
}

fn gtk_ghostty_close_all_windows(area: &gtk::GLArea) {
    let Some(window) = gtk_ghostty_root_window(area) else {
        return;
    };
    let application = window.application();

    // Ghostty actions can arrive while GtkGhosttyHost::tick holds the host's
    // RefCell mutably. Closing a window synchronously emits GLArea::unrealize,
    // whose callback needs the same mutable borrow. Defer destruction until
    // the current Ghostty callback has returned to the GTK main loop.
    glib::idle_add_local_once(move || {
        let Some(application) = application else {
            window.close();
            return;
        };
        for window in application.windows() {
            window.close();
        }
    });
}

fn gtk_ghostty_reset_window_size(window: &gtk::Window) {
    if window.is_fullscreen() {
        window.unfullscreen();
    }
    if window.is_maximized() {
        window.unmaximize();
    }
    window.set_default_size(
        crate::gtk_ui::GTK_APP_DEFAULT_WIDTH,
        crate::gtk_ui::GTK_APP_DEFAULT_HEIGHT,
    );
    window.queue_resize();
    window.present();
}

fn gtk_ghostty_fullscreen_mode_is_supported(value: Option<&str>) -> bool {
    matches!(
        value,
        None | Some("native")
            | Some("macos_non_native")
            | Some("macos_non_native_visible_menu")
            | Some("macos_non_native_padded_notch")
    )
}

fn gtk_ghostty_root_window(area: &gtk::GLArea) -> Option<gtk::Window> {
    area.root()?.downcast::<gtk::Window>().ok()
}

fn ghostty_readonly(value: c_int) -> Option<bool> {
    match value {
        GHOSTTY_READONLY_OFF => Some(false),
        GHOSTTY_READONLY_ON => Some(true),
        _ => None,
    }
}

fn ghostty_progress_state(value: c_int) -> Option<&'static str> {
    match value {
        GHOSTTY_PROGRESS_STATE_REMOVE => Some("remove"),
        GHOSTTY_PROGRESS_STATE_SET => Some("set"),
        GHOSTTY_PROGRESS_STATE_ERROR => Some("error"),
        GHOSTTY_PROGRESS_STATE_INDETERMINATE => Some("indeterminate"),
        GHOSTTY_PROGRESS_STATE_PAUSE => Some("pause"),
        _ => None,
    }
}

fn ghostty_renderer_health(value: c_int) -> Option<&'static str> {
    match value {
        GHOSTTY_RENDERER_HEALTH_HEALTHY => Some("healthy"),
        GHOSTTY_RENDERER_HEALTH_UNHEALTHY => Some("unhealthy"),
        _ => None,
    }
}

fn ghostty_prompt_title(value: c_int) -> Option<&'static str> {
    match value {
        GHOSTTY_PROMPT_TITLE_SURFACE => Some("surface"),
        GHOSTTY_PROMPT_TITLE_TAB => Some("tab"),
        _ => None,
    }
}

fn ghostty_quit_timer(value: c_int) -> Option<&'static str> {
    match value {
        GHOSTTY_QUIT_TIMER_START => Some("start"),
        GHOSTTY_QUIT_TIMER_STOP => Some("stop"),
        _ => None,
    }
}

fn ghostty_on_off_toggle(
    value: c_int,
    on: c_int,
    off: c_int,
    toggle: c_int,
) -> Option<&'static str> {
    if value == on {
        Some("on")
    } else if value == off {
        Some("off")
    } else if value == toggle {
        Some("toggle")
    } else {
        None
    }
}

fn ghostty_input_trigger(trigger: GhosttyInputTrigger) -> Option<String> {
    let mut value = match trigger.tag {
        GHOSTTY_TRIGGER_PHYSICAL => format!("physical:{}", unsafe { trigger.key.physical }),
        GHOSTTY_TRIGGER_UNICODE => format!("unicode:U+{:04X}", unsafe { trigger.key.unicode }),
        GHOSTTY_TRIGGER_CATCH_ALL => "catch_all".to_string(),
        _ => return None,
    };
    if trigger.mods != 0 {
        value.push_str(" mods=");
        value.push_str(&ghostty_trigger_mod_names(trigger.mods));
    }
    Some(value)
}

fn ghostty_trigger_mod_names(mods: c_int) -> String {
    let mut names = Vec::new();
    let mut rest = mods;
    for (bit, name) in [
        (GHOSTTY_MODS_SHIFT, "shift"),
        (GHOSTTY_MODS_CTRL, "ctrl"),
        (GHOSTTY_MODS_ALT, "alt"),
        (GHOSTTY_MODS_SUPER, "super"),
        (GHOSTTY_MODS_CAPS, "caps"),
        (GHOSTTY_MODS_NUM, "num"),
        (GHOSTTY_MODS_SHIFT_RIGHT, "shift-right"),
        (GHOSTTY_MODS_CTRL_RIGHT, "ctrl-right"),
        (GHOSTTY_MODS_ALT_RIGHT, "alt-right"),
        (GHOSTTY_MODS_SUPER_RIGHT, "super-right"),
    ] {
        if mods & bit != 0 {
            names.push(name.to_string());
            rest &= !bit;
        }
    }
    if rest != 0 {
        names.push(format!("0x{rest:x}"));
    }
    names.join("+")
}

fn ghostty_key_table(payload: GhosttyActionKeyTable) -> Option<(&'static str, Option<String>)> {
    match payload.tag {
        GHOSTTY_KEY_TABLE_ACTIVATE => {
            let activate = unsafe { payload.value.activate };
            Some((
                "activate",
                Some(ghostty_sized_string(activate.name, activate.len)?),
            ))
        }
        GHOSTTY_KEY_TABLE_DEACTIVATE => Some(("deactivate", None)),
        GHOSTTY_KEY_TABLE_DEACTIVATE_ALL => Some(("deactivate_all", None)),
        _ => None,
    }
}

fn ghostty_color_kind(value: c_int) -> Option<(&'static str, Option<u32>)> {
    match value {
        GHOSTTY_COLOR_KIND_FOREGROUND => Some(("foreground", None)),
        GHOSTTY_COLOR_KIND_BACKGROUND => Some(("background", None)),
        GHOSTTY_COLOR_KIND_CURSOR => Some(("cursor", None)),
        value if value >= 0 => u32::try_from(value)
            .ok()
            .map(|index| ("palette", Some(index))),
        _ => None,
    }
}

fn ghostty_search_count(value: isize) -> Option<u64> {
    u64::try_from(value).ok()
}

fn ghostty_sized_string(value: *const c_char, len: usize) -> Option<String> {
    if len == 0 {
        return Some(String::new());
    }
    if value.is_null() {
        return None;
    }
    let bytes = unsafe { std::slice::from_raw_parts(value as *const u8, len) };
    Some(String::from_utf8_lossy(bytes).into_owned())
}

fn ghostty_c_string(value: *const c_char) -> Option<String> {
    if value.is_null() {
        return None;
    }
    Some(
        unsafe { CStr::from_ptr(value) }
            .to_string_lossy()
            .into_owned(),
    )
}

unsafe extern "C" fn gtk_ghostty_read_clipboard(
    userdata: *mut c_void,
    clipboard: c_int,
    request: *mut c_void,
) -> bool {
    if request.is_null() {
        return false;
    }

    let Some((callbacks, token)) = ghostty_callback_ref(userdata) else {
        return false;
    };
    let request = request as usize;
    if glib::MainContext::default().is_owner() {
        return gtk_ghostty_read_clipboard_on_main(callbacks, token, clipboard, request);
    }

    glib::idle_add_once(move || {
        gtk_ghostty_read_clipboard_on_main(callbacks, token, clipboard, request);
    });
    true
}

fn gtk_ghostty_read_clipboard_on_main(
    callbacks: usize,
    token: u64,
    clipboard: c_int,
    request: usize,
) -> bool {
    with_ghostty_callbacks(callbacks, token, |callback_state| {
        let Some(surface) = callback_state.surface else {
            return false;
        };
        let Some(complete) = callback_state.complete_clipboard_request else {
            return false;
        };
        let Some(area) = callback_state.area.as_ref() else {
            let empty = c"";
            return unsafe { complete(surface, empty.as_ptr(), request as *mut c_void, true) };
        };
        let Some(clipboard) = gtk_ghostty_clipboard(area, clipboard) else {
            let empty = c"";
            return unsafe { complete(surface, empty.as_ptr(), request as *mut c_void, true) };
        };

        clipboard.read_text_async(None::<&gio::Cancellable>, move |result| {
            let text = result
                .ok()
                .flatten()
                .map(|text| text.to_string())
                .unwrap_or_default();
            let text = clipboard_cstring(&text);
            let _ = with_ghostty_callbacks(callbacks, token, |callback_state| {
                if callback_state.surface != Some(surface) {
                    return;
                }
                let _ =
                    gtk_ghostty_complete_initial_clipboard_read(complete, surface, request, &text);
            });
        });
        true
    })
    .unwrap_or(false)
}

fn gtk_ghostty_complete_initial_clipboard_read(
    complete: GhosttySurfaceCompleteClipboardRequest,
    surface: GhosttySurface,
    request: usize,
    text: &CStr,
) -> bool {
    unsafe { complete(surface, text.as_ptr(), request as *mut c_void, false) }
}

unsafe extern "C" fn gtk_ghostty_confirm_read_clipboard(
    userdata: *mut c_void,
    text: *const c_char,
    request: *mut c_void,
    request_type: c_int,
) {
    if request.is_null() {
        return;
    }

    let Some((callbacks, token)) = ghostty_callback_ref(userdata) else {
        return;
    };
    let Some(surface) = with_ghostty_callbacks(callbacks, token, |callback_state| {
        callback_state.surface.map(|surface| surface as usize)
    })
    .flatten() else {
        return;
    };
    let text = ghostty_c_string(text).unwrap_or_default();
    let request = request as usize;
    if glib::MainContext::default().is_owner() {
        gtk_ghostty_confirm_read_clipboard_on_main(
            callbacks,
            token,
            surface,
            text,
            request,
            request_type,
        );
    } else {
        glib::idle_add_once(move || {
            gtk_ghostty_confirm_read_clipboard_on_main(
                callbacks,
                token,
                surface,
                text,
                request,
                request_type,
            );
        });
    }
}

fn gtk_ghostty_confirm_read_clipboard_on_main(
    callbacks: usize,
    token: u64,
    surface: usize,
    text: String,
    request: usize,
    request_type: c_int,
) {
    let area = with_ghostty_callbacks(callbacks, token, |callback_state| {
        (callback_state.surface.map(|surface| surface as usize) == Some(surface))
            .then(|| callback_state.area.clone())
            .flatten()
    })
    .flatten();
    let spec = gtk_ghostty_clipboard_confirmation_spec(request_type);
    let shown = area.zip(spec).is_some_and(|(area, spec)| {
        let decision_text = text.clone();
        gtk_ghostty_show_clipboard_confirmation(&area, spec, &text, move |confirmed| {
            let _ = gtk_ghostty_complete_clipboard_confirmation(
                callbacks,
                token,
                surface,
                request,
                &decision_text,
                confirmed,
            );
        })
    });
    if !shown {
        let _ = gtk_ghostty_complete_clipboard_confirmation(
            callbacks, token, surface, request, "", false,
        );
    }
}

fn gtk_ghostty_complete_clipboard_confirmation(
    callbacks: usize,
    token: u64,
    surface: usize,
    request: usize,
    text: &str,
    confirmed: bool,
) -> bool {
    with_ghostty_callbacks(callbacks, token, |callback_state| {
        if callback_state.surface.map(|surface| surface as usize) != Some(surface) {
            return false;
        }
        let Some(complete) = callback_state.complete_clipboard_request else {
            return false;
        };
        let text = clipboard_cstring(if confirmed { text } else { "" });
        unsafe {
            complete(
                surface as GhosttySurface,
                text.as_ptr(),
                request as *mut c_void,
                true,
            )
        }
    })
    .unwrap_or(false)
}

unsafe extern "C" fn gtk_ghostty_write_clipboard(
    userdata: *mut c_void,
    clipboard: c_int,
    contents: *const GhosttyClipboardContent,
    len: usize,
    confirm: bool,
) {
    let Some(text) = ghostty_clipboard_text(contents, len) else {
        return;
    };

    let Some((callbacks, token)) = ghostty_callback_ref(userdata) else {
        return;
    };
    if glib::MainContext::default().is_owner() {
        gtk_ghostty_write_clipboard_on_main(callbacks, token, clipboard, text, confirm);
    } else {
        glib::idle_add_once(move || {
            gtk_ghostty_write_clipboard_on_main(callbacks, token, clipboard, text, confirm);
        });
    }
}

fn gtk_ghostty_write_clipboard_on_main(
    callbacks: usize,
    token: u64,
    clipboard: c_int,
    text: String,
    confirm: bool,
) {
    let Some((surface, area)) = with_ghostty_callbacks(callbacks, token, |callback_state| {
        Some((
            callback_state.surface? as usize,
            callback_state.area.clone()?,
        ))
    })
    .flatten() else {
        return;
    };
    if !confirm {
        let _ = gtk_ghostty_write_clipboard_text(callbacks, token, surface, clipboard, &text);
        return;
    }
    let _ = gtk_ghostty_show_clipboard_confirmation(
        &area,
        gtk_ghostty_clipboard_confirmation_spec(GHOSTTY_CLIPBOARD_REQUEST_OSC_52_WRITE)
            .expect("OSC 52 write confirmation spec"),
        &text,
        {
            let decision_text = text.clone();
            move |confirmed| {
                if confirmed {
                    let _ = gtk_ghostty_write_clipboard_text(
                        callbacks,
                        token,
                        surface,
                        clipboard,
                        &decision_text,
                    );
                }
            }
        },
    );
}

fn gtk_ghostty_write_clipboard_text(
    callbacks: usize,
    token: u64,
    surface: usize,
    clipboard: c_int,
    text: &str,
) -> bool {
    with_ghostty_callbacks(callbacks, token, |callback_state| {
        if callback_state.surface.map(|surface| surface as usize) != Some(surface) {
            return false;
        }
        let Some(area) = callback_state.area.as_ref() else {
            return false;
        };
        let Some(clipboard) = gtk_ghostty_clipboard(area, clipboard) else {
            return false;
        };
        clipboard.set_text(text);
        true
    })
    .unwrap_or(false)
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct GtkGhosttyClipboardConfirmationSpec {
    title: &'static str,
    body: &'static str,
    deny_label: &'static str,
    allow_label: &'static str,
}

fn gtk_ghostty_clipboard_confirmation_spec(
    request_type: c_int,
) -> Option<GtkGhosttyClipboardConfirmationSpec> {
    match request_type {
        GHOSTTY_CLIPBOARD_REQUEST_PASTE => Some(GtkGhosttyClipboardConfirmationSpec {
            title: "Warning: Potentially Unsafe Paste",
            body: "Pasting this text into the terminal may execute one or more commands.",
            deny_label: "Cancel",
            allow_label: "Paste",
        }),
        GHOSTTY_CLIPBOARD_REQUEST_OSC_52_READ => Some(GtkGhosttyClipboardConfirmationSpec {
            title: "Authorize Clipboard Access",
            body: "An application in the terminal is attempting to read the clipboard.",
            deny_label: "Deny",
            allow_label: "Allow",
        }),
        GHOSTTY_CLIPBOARD_REQUEST_OSC_52_WRITE => Some(GtkGhosttyClipboardConfirmationSpec {
            title: "Authorize Clipboard Access",
            body: "An application in the terminal is attempting to write the following text to the clipboard.",
            deny_label: "Deny",
            allow_label: "Allow",
        }),
        _ => None,
    }
}

fn gtk_ghostty_show_clipboard_confirmation(
    area: &gtk::GLArea,
    spec: GtkGhosttyClipboardConfirmationSpec,
    text: &str,
    on_decision: impl FnOnce(bool) + 'static,
) -> bool {
    let Some(window) = gtk_ghostty_root_window(area) else {
        return false;
    };
    let dialog = gtk::Dialog::builder()
        .title(spec.title)
        .transient_for(&window)
        .modal(true)
        .default_width(560)
        .default_height(360)
        .build();
    dialog.add_button(spec.deny_label, gtk::ResponseType::Cancel);
    let allow = dialog.add_button(spec.allow_label, gtk::ResponseType::Accept);
    allow.add_css_class("suggested-action");
    dialog.set_default_response(gtk::ResponseType::Accept);

    let content = dialog.content_area();
    content.set_spacing(12);
    content.set_margin_top(16);
    content.set_margin_bottom(16);
    content.set_margin_start(16);
    content.set_margin_end(16);
    let body = gtk::Label::builder()
        .label(spec.body)
        .wrap(true)
        .xalign(0.0)
        .build();
    content.append(&body);
    let buffer = gtk::TextBuffer::new(None);
    buffer.set_text(text);
    let text_view = gtk::TextView::builder()
        .buffer(&buffer)
        .editable(false)
        .cursor_visible(false)
        .monospace(true)
        .wrap_mode(gtk::WrapMode::WordChar)
        .build();
    let scroll = gtk::ScrolledWindow::builder()
        .hexpand(true)
        .vexpand(true)
        .min_content_height(180)
        .child(&text_view)
        .build();
    content.append(&scroll);

    let on_decision = Rc::new(RefCell::new(Some(on_decision)));
    dialog.connect_response(move |dialog, response| {
        if let Some(on_decision) = on_decision.borrow_mut().take() {
            on_decision(response == gtk::ResponseType::Accept);
        }
        dialog.close();
    });
    dialog.present();
    true
}

fn gtk_ghostty_clipboard(area: &gtk::GLArea, clipboard: c_int) -> Option<gdk::Clipboard> {
    match clipboard {
        GHOSTTY_CLIPBOARD_STANDARD => Some(area.clipboard()),
        GHOSTTY_CLIPBOARD_SELECTION | GHOSTTY_CLIPBOARD_PRIMARY => Some(area.primary_clipboard()),
        _ => None,
    }
}

fn ghostty_clipboard_text(contents: *const GhosttyClipboardContent, len: usize) -> Option<String> {
    if contents.is_null() || len == 0 {
        return None;
    }
    let contents = unsafe { std::slice::from_raw_parts(contents, len) };
    contents
        .iter()
        .filter_map(ghostty_clipboard_content_text)
        .max_by_key(|(score, _)| *score)
        .map(|(_, text)| text)
}

fn ghostty_clipboard_content_text(content: &GhosttyClipboardContent) -> Option<(u8, String)> {
    if content.data.is_null() {
        return None;
    }
    let mime = if content.mime.is_null() {
        ""
    } else {
        unsafe { CStr::from_ptr(content.mime) }
            .to_str()
            .unwrap_or_default()
    };
    let score = match mime {
        "text/plain;charset=utf-8" | "text/plain;charset=UTF-8" => 3,
        "text/plain" => 2,
        mime if mime.starts_with("text/") => 1,
        "" => 0,
        _ => return None,
    };
    let text = unsafe { CStr::from_ptr(content.data) }
        .to_string_lossy()
        .into_owned();
    Some((score, text))
}

fn clipboard_cstring(text: &str) -> CString {
    CString::new(text).unwrap_or_else(|_| {
        CString::new(text.replace('\0', "")).expect("NUL bytes were stripped from clipboard text")
    })
}

fn ghostty_drop_text(value: &glib::Value) -> Option<String> {
    if let Some(text) = ghostty_drop_file_list_text(value) {
        return Some(text);
    }
    if let Ok(file) = value.get::<gio::File>() {
        return ghostty_drop_files_text([file]);
    }
    value
        .get::<String>()
        .ok()
        .and_then(|text| ghostty_drop_string_text(&text))
}

fn ghostty_drop_file_list_text(value: &glib::Value) -> Option<String> {
    let file_list_type = gdk_file_list_type()?;
    if !value.type_().is_a(file_list_type) {
        return None;
    }
    let get_files = gdk_file_list_get_files()?;
    let file_list = unsafe { glib::gobject_ffi::g_value_get_boxed(value.to_glib_none().0) };
    if file_list.is_null() {
        return None;
    }
    let files = unsafe { get_files(file_list) };
    let files: Vec<gio::File> = unsafe { FromGlibPtrContainer::from_glib_container(files) };
    ghostty_drop_files_text(files)
}

fn ghostty_drop_string_text(text: &str) -> Option<String> {
    if let Some(paths) = ghostty_drop_uri_list_text(text) {
        return Some(paths);
    }
    if text.is_empty() {
        None
    } else {
        Some(text.to_string())
    }
}

fn ghostty_drop_uri_list_text(text: &str) -> Option<String> {
    let mut paths = Vec::new();
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if !line.starts_with("file://") {
            return None;
        }
        paths.push(PathBuf::from(normalize_drop_file_path(line)));
    }
    ghostty_drop_path_text(paths)
}

fn ghostty_drop_files_text(files: impl IntoIterator<Item = gio::File>) -> Option<String> {
    ghostty_drop_path_text(files.into_iter().filter_map(|file| file.path()))
}

fn ghostty_drop_path_text(paths: impl IntoIterator<Item = PathBuf>) -> Option<String> {
    let mut text = String::new();
    for path in paths {
        let path = path_to_terminal_string(&path);
        text.push_str(&shell_escape_drop_path(&path));
        text.push('\n');
    }
    if text.is_empty() {
        None
    } else {
        Some(text)
    }
}

fn path_to_terminal_string(path: &Path) -> String {
    path.to_string_lossy().into_owned()
}

fn shell_escape_drop_path(path: &str) -> String {
    if path.contains('\n') || path.contains('\r') {
        return format!("'{}'", path.replace('\'', "'\\''"));
    }
    let mut out = String::new();
    for ch in path.chars() {
        if "\\ ()[]{}<>\"'`!#$&;|*?\t".contains(ch) {
            out.push('\\');
        }
        out.push(ch);
    }
    out
}

fn normalize_drop_file_path(value: &str) -> String {
    let trimmed = value.trim();
    if let Some(path) = trimmed.strip_prefix("file://localhost/") {
        return format!("/{}", percent_decode_drop_value(path));
    }
    if let Some(path) = trimmed.strip_prefix("file://") {
        return percent_decode_drop_value(path);
    }
    trimmed.to_string()
}

fn percent_decode_drop_value(value: &str) -> String {
    let bytes = value.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'%' && index + 2 < bytes.len() {
            if let Ok(hex) = std::str::from_utf8(&bytes[index + 1..index + 3]) {
                if let Ok(byte) = u8::from_str_radix(hex, 16) {
                    out.push(byte);
                    index += 3;
                    continue;
                }
            }
        }
        out.push(if bytes[index] == b'+' {
            b' '
        } else {
            bytes[index]
        });
        index += 1;
    }
    String::from_utf8_lossy(&out).to_string()
}

fn positive_u32(value: i32) -> u32 {
    value.max(1) as u32
}

fn gtk_ghostty_allocated_surface_size(area: &gtk::GLArea) -> Option<(u32, u32)> {
    let width = area.allocated_width();
    let height = area.allocated_height();
    (width > 1 && height > 1).then(|| (width as u32, height as u32))
}

fn gtk_copy_mode_modifiers(modifiers: gdk::ModifierType) -> CopyModeModifiers {
    CopyModeModifiers {
        super_key: modifiers
            .intersects(gdk::ModifierType::SUPER_MASK | gdk::ModifierType::META_MASK),
        shift: modifiers.contains(gdk::ModifierType::SHIFT_MASK),
        control: modifiers.contains(gdk::ModifierType::CONTROL_MASK),
        alt: modifiers.contains(gdk::ModifierType::ALT_MASK),
    }
}

fn gtk_copy_mode_key(keyval: gdk::Key) -> CopyModeKey {
    match keyval {
        gdk::Key::Escape => CopyModeKey::Escape,
        gdk::Key::Up | gdk::Key::KP_Up => CopyModeKey::ArrowUp,
        gdk::Key::Down | gdk::Key::KP_Down => CopyModeKey::ArrowDown,
        gdk::Key::Left | gdk::Key::KP_Left => CopyModeKey::ArrowLeft,
        gdk::Key::Right | gdk::Key::KP_Right => CopyModeKey::ArrowRight,
        gdk::Key::Page_Up | gdk::Key::KP_Page_Up => CopyModeKey::PageUp,
        gdk::Key::Page_Down | gdk::Key::KP_Page_Down => CopyModeKey::PageDown,
        gdk::Key::Home | gdk::Key::KP_Home => CopyModeKey::Home,
        gdk::Key::End | gdk::Key::KP_End => CopyModeKey::End,
        _ => keyval
            .to_unicode()
            .map(CopyModeKey::Character)
            .unwrap_or(CopyModeKey::Other),
    }
}

fn gtk_ghostty_scale_factor(area: &gtk::GLArea) -> f64 {
    f64::from(area.scale_factor().max(1))
}

fn gtk_ghostty_controller_scale(controller: &impl IsA<gtk::EventController>) -> f64 {
    controller
        .upcast_ref::<gtk::EventController>()
        .widget()
        .map(|widget| f64::from(widget.scale_factor().max(1)))
        .unwrap_or(1.0)
}

fn gtk_ghostty_color_scheme() -> c_int {
    ghostty_color_scheme_for_dark_preference(
        gtk::Settings::default()
            .is_some_and(|settings| settings.is_gtk_application_prefer_dark_theme()),
    )
}

fn ghostty_color_scheme_for_dark_preference(prefer_dark: bool) -> c_int {
    if prefer_dark {
        GHOSTTY_COLOR_SCHEME_DARK
    } else {
        GHOSTTY_COLOR_SCHEME_LIGHT
    }
}

fn gtk_ghostty_im_rectangle(point: GhosttyImePoint) -> gdk::Rectangle {
    gdk::Rectangle::new(
        finite_i32(point.x, 0),
        finite_i32(point.y, 0),
        positive_finite_i32(point.width),
        positive_finite_i32(point.height),
    )
}

fn finite_i32(value: f64, fallback: i32) -> i32 {
    if !value.is_finite() {
        return fallback;
    }
    if value <= f64::from(i32::MIN) {
        i32::MIN
    } else if value >= f64::from(i32::MAX) {
        i32::MAX
    } else {
        value.round() as i32
    }
}

fn positive_finite_i32(value: f64) -> i32 {
    finite_i32(value, 1).max(1)
}

fn ghostty_mods(modifiers: gdk::ModifierType) -> c_int {
    let mut mods = 0;
    if modifiers.contains(gdk::ModifierType::SHIFT_MASK) {
        mods |= GHOSTTY_MODS_SHIFT;
    }
    if modifiers.contains(gdk::ModifierType::CONTROL_MASK) {
        mods |= GHOSTTY_MODS_CTRL;
    }
    if modifiers.contains(gdk::ModifierType::ALT_MASK) {
        mods |= GHOSTTY_MODS_ALT;
    }
    if modifiers.intersects(gdk::ModifierType::SUPER_MASK | gdk::ModifierType::META_MASK) {
        mods |= GHOSTTY_MODS_SUPER;
    }
    if modifiers.contains(gdk::ModifierType::LOCK_MASK) {
        mods |= GHOSTTY_MODS_CAPS;
    }
    mods
}

fn ghostty_scroll_mods(precision: bool) -> c_int {
    if precision {
        GHOSTTY_SCROLL_MOD_PRECISION
    } else {
        0
    }
}

fn ghostty_pointer_input(x: f64, y: f64, content_scale: f64) -> (f64, f64) {
    // GTK events are logical coordinates; Ghostty hit-tests device pixels.
    let content_scale = if content_scale.is_finite() {
        content_scale.max(1.0)
    } else {
        1.0
    };
    (x * content_scale, y * content_scale)
}

fn ghostty_surface_scroll_input(
    dx: f64,
    dy: f64,
    precision: bool,
    content_scale: f64,
) -> (f64, f64, c_int) {
    let content_scale = if content_scale.is_finite() {
        content_scale.max(1.0)
    } else {
        1.0
    };
    let multiplier = if precision { 10.0 } else { 1.0 };
    (
        -dx * content_scale * multiplier,
        -dy * content_scale * multiplier,
        ghostty_scroll_mods(precision),
    )
}

fn ghostty_inspector_scroll_input(dx: f64, dy: f64, precision: bool) -> (f64, f64, c_int) {
    (dx, -dy, ghostty_scroll_mods(precision))
}

fn ghostty_consumed_mods(controller: &gtk::EventControllerKey) -> c_int {
    controller
        .current_event()
        .as_ref()
        .and_then(|event| event.downcast_ref::<gdk::KeyEvent>())
        .map(|event| ghostty_mods(event.consumed_modifiers()))
        .unwrap_or(0)
}

fn ghostty_unshifted_codepoint(controller: &gtk::EventControllerKey, keycode: u32) -> u32 {
    let Some(event) = controller.current_event() else {
        return 0;
    };
    let Some(key_event) = event.downcast_ref::<gdk::KeyEvent>() else {
        return 0;
    };
    let Some(display) = event.display() else {
        return 0;
    };
    let Some(mappings) = display.map_keycode(keycode) else {
        return 0;
    };
    let layout = key_event.layout() as i32;
    mappings
        .into_iter()
        .find(|(key, _)| key.group() == layout && key.level() == 0)
        .and_then(|(_, keyval)| keyval.to_unicode())
        .map(|ch| ch as u32)
        .unwrap_or(0)
}

fn ghostty_binding_flags_consume_input(flags: c_int) -> bool {
    flags
        & (GHOSTTY_BINDING_FLAGS_CONSUMED
            | GHOSTTY_BINDING_FLAGS_ALL
            | GHOSTTY_BINDING_FLAGS_GLOBAL)
        != 0
}

fn ghostty_should_track_super_surface_binding(flags: c_int, key_consumed: bool) -> bool {
    key_consumed || !ghostty_binding_flags_consume_input(flags)
}

fn ghostty_input_key_event(
    action: c_int,
    controller: &gtk::EventControllerKey,
    keycode: u32,
    modifiers: gdk::ModifierType,
    text: Option<String>,
    composing: bool,
) -> (GhosttyInputKey, Option<CString>) {
    let text_cstring = text.and_then(|text| CString::new(text).ok());
    let event = GhosttyInputKey {
        action,
        mods: ghostty_mods(modifiers),
        consumed_mods: ghostty_consumed_mods(controller),
        keycode,
        text: text_cstring
            .as_ref()
            .map_or(ptr::null(), |text| text.as_ptr()),
        unshifted_codepoint: ghostty_unshifted_codepoint(controller, keycode),
        composing,
    };
    (event, text_cstring)
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct QueuedGhosttyKey {
    key: String,
    ctrl: bool,
    alt: bool,
    shift: bool,
    super_mod: bool,
}

impl QueuedGhosttyKey {
    fn parse(key: &str) -> Option<Self> {
        let normalized = key
            .trim()
            .replace('+', "-")
            .split_whitespace()
            .collect::<Vec<_>>()
            .join("-");
        if normalized.is_empty() {
            return None;
        }
        let parts = normalized
            .split('-')
            .filter(|part| !part.is_empty())
            .collect::<Vec<_>>();
        let mut ctrl = false;
        let mut alt = false;
        let mut shift = false;
        let mut super_mod = false;
        let mut index = 0;
        while index < parts.len() {
            match parts[index].to_ascii_lowercase().as_str() {
                "ctrl" | "control" => ctrl = true,
                "alt" | "option" | "meta" => alt = true,
                "cmd" | "command" | "super" => super_mod = true,
                "shift" => shift = true,
                "c" if !ctrl && index + 1 < parts.len() => ctrl = true,
                "m" if !alt && index + 1 < parts.len() => alt = true,
                "s" if !shift && index + 1 < parts.len() => shift = true,
                _ => break,
            }
            index += 1;
        }
        if index >= parts.len() {
            return None;
        }
        let raw_key = parts[index..].join("-").to_ascii_lowercase();
        Some(Self {
            key: ghostty_queued_canonical_key_name(&raw_key),
            ctrl,
            alt,
            shift,
            super_mod,
        })
    }

    fn mods(&self) -> c_int {
        let mut mods = 0;
        if self.shift {
            mods |= GHOSTTY_MODS_SHIFT;
        }
        if self.ctrl {
            mods |= GHOSTTY_MODS_CTRL;
        }
        if self.alt {
            mods |= GHOSTTY_MODS_ALT;
        }
        if self.super_mod {
            mods |= GHOSTTY_MODS_SUPER;
        }
        mods
    }

    fn text(&self, action: c_int) -> Option<String> {
        if action != GHOSTTY_ACTION_PRESS || self.ctrl {
            return None;
        }
        match self.key.as_str() {
            "space" => Some(" ".to_string()),
            key if key.chars().count() == 1 => {
                let mut ch = key.chars().next()?;
                if self.shift && ch.is_ascii_lowercase() {
                    ch = ch.to_ascii_uppercase();
                }
                Some(ch.to_string())
            }
            _ => None,
        }
    }

    fn unshifted_codepoint(&self) -> u32 {
        match self.key.as_str() {
            "enter" => b'\r' as u32,
            "tab" => b'\t' as u32,
            "backspace" => 0x08,
            "escape" => 0x1b,
            "space" => b' ' as u32,
            key if key.chars().count() == 1 => key.chars().next().unwrap() as u32,
            _ => 0,
        }
    }
}

fn ghostty_queued_canonical_key_name(key: &str) -> String {
    let normalized = if key == "_" {
        "_".to_string()
    } else {
        key.replace('_', "-")
    };
    match normalized.as_str() {
        "return" => "enter",
        "esc" => "escape",
        "del" => "delete",
        "ins" => "insert",
        "bs" | "bspace" => "backspace",
        "pgup" | "pageup" => "page-up",
        "pgdn" | "pagedown" => "page-down",
        "arrowleft" | "arrow-left" => "left",
        "arrowright" | "arrow-right" => "right",
        "arrowup" | "arrow-up" => "up",
        "arrowdown" | "arrow-down" => "down",
        other => other,
    }
    .to_string()
}

fn ghostty_queued_key_event(
    action: c_int,
    key: &str,
) -> Option<(GhosttyInputKey, Option<CString>)> {
    let key = QueuedGhosttyKey::parse(key)?;
    let (keycode, base_mods) = ghostty_queued_xkb_keycode(&key.key)?;
    let text_cstring = key.text(action).and_then(|text| CString::new(text).ok());
    let event = GhosttyInputKey {
        action,
        mods: key.mods() | base_mods,
        consumed_mods: 0,
        keycode,
        text: text_cstring
            .as_ref()
            .map_or(ptr::null(), |text| text.as_ptr()),
        unshifted_codepoint: key.unshifted_codepoint(),
        composing: false,
    };
    Some((event, text_cstring))
}

fn ghostty_queued_xkb_keycode(key: &str) -> Option<(u32, c_int)> {
    Some(match key {
        "enter" => (0x24, 0),
        "escape" => (0x09, 0),
        "backspace" => (0x16, 0),
        "tab" => (0x17, 0),
        "space" => (0x41, 0),
        "insert" => (0x76, 0),
        "home" => (0x6e, 0),
        "page-up" => (0x70, 0),
        "delete" => (0x77, 0),
        "end" => (0x73, 0),
        "page-down" => (0x75, 0),
        "right" => (0x72, 0),
        "left" => (0x71, 0),
        "down" => (0x74, 0),
        "up" => (0x6f, 0),
        key if key.chars().count() == 1 => (
            ghostty_queued_char_xkb_keycode(key.chars().next().unwrap())?,
            0,
        ),
        key => ghostty_queued_function_xkb_keycode(key)?,
    })
}

fn ghostty_queued_char_xkb_keycode(ch: char) -> Option<u32> {
    Some(match ch {
        'a' => 0x26,
        'b' => 0x38,
        'c' => 0x36,
        'd' => 0x28,
        'e' => 0x1a,
        'f' => 0x29,
        'g' => 0x2a,
        'h' => 0x2b,
        'i' => 0x1f,
        'j' => 0x2c,
        'k' => 0x2d,
        'l' => 0x2e,
        'm' => 0x3a,
        'n' => 0x39,
        'o' => 0x20,
        'p' => 0x21,
        'q' => 0x18,
        'r' => 0x1b,
        's' => 0x27,
        't' => 0x1c,
        'u' => 0x1e,
        'v' => 0x37,
        'w' => 0x19,
        'x' => 0x35,
        'y' => 0x1d,
        'z' => 0x34,
        '1' => 0x0a,
        '2' => 0x0b,
        '3' => 0x0c,
        '4' => 0x0d,
        '5' => 0x0e,
        '6' => 0x0f,
        '7' => 0x10,
        '8' => 0x11,
        '9' => 0x12,
        '0' => 0x13,
        '-' => 0x14,
        '=' => 0x15,
        '[' => 0x22,
        ']' => 0x23,
        '\\' => 0x33,
        ';' => 0x2f,
        '\'' => 0x30,
        '`' => 0x31,
        ',' => 0x3b,
        '.' => 0x3c,
        '/' => 0x3d,
        _ => return None,
    })
}

fn ghostty_queued_function_xkb_keycode(key: &str) -> Option<(u32, c_int)> {
    let number = key.strip_prefix('f')?.parse::<u32>().ok()?;
    Some(match number {
        1 => (0x43, 0),
        2 => (0x44, 0),
        3 => (0x45, 0),
        4 => (0x46, 0),
        5 => (0x47, 0),
        6 => (0x48, 0),
        7 => (0x49, 0),
        8 => (0x4a, 0),
        9 => (0x4b, 0),
        10 => (0x4c, 0),
        11 => (0x5f, 0),
        12 => (0x60, 0),
        13..=24 => (0xbf + (number - 13), 0),
        25 => (ghostty_physical_keycode(GHOSTTY_KEY_F25), 0),
        _ => return None,
    })
}

fn ghostty_key_text(keyval: gdk::Key) -> Option<String> {
    let ch = keyval.to_unicode()?;
    if ch.is_control() {
        return None;
    }
    Some(ch.to_string())
}

fn ghostty_input_history_bytes(
    keyval: gdk::Key,
    modifiers: gdk::ModifierType,
    text: Option<&str>,
) -> Option<Vec<u8>> {
    if modifiers.contains(gdk::ModifierType::CONTROL_MASK) {
        if let Some(ch) = keyval.to_unicode().filter(|ch| ch.is_ascii_alphabetic()) {
            return Some(vec![ch.to_ascii_lowercase() as u8 - b'a' + 1]);
        }
    }
    let control = match keyval {
        gdk::Key::Return | gdk::Key::KP_Enter => Some(b'\r'),
        gdk::Key::BackSpace => Some(0x7f),
        gdk::Key::Tab | gdk::Key::KP_Tab | gdk::Key::ISO_Left_Tab => Some(b'\t'),
        gdk::Key::Escape => Some(0x1b),
        _ => None,
    };
    control.map(|byte| vec![byte]).or_else(|| {
        text.filter(|text| !text.is_empty())
            .map(|text| text.as_bytes().to_vec())
    })
}

fn ghostty_inspector_key(keyval: gdk::Key) -> Option<c_int> {
    Some(match keyval {
        gdk::Key::BackSpace => GHOSTTY_KEY_BACKSPACE,
        gdk::Key::Return | gdk::Key::KP_Enter => GHOSTTY_KEY_ENTER,
        gdk::Key::space => GHOSTTY_KEY_SPACE,
        gdk::Key::Tab | gdk::Key::ISO_Left_Tab => GHOSTTY_KEY_TAB,
        gdk::Key::Delete | gdk::Key::KP_Delete => GHOSTTY_KEY_DELETE,
        gdk::Key::End | gdk::Key::KP_End => GHOSTTY_KEY_END,
        gdk::Key::Home | gdk::Key::KP_Home => GHOSTTY_KEY_HOME,
        gdk::Key::Insert | gdk::Key::KP_Insert => GHOSTTY_KEY_INSERT,
        gdk::Key::Page_Down | gdk::Key::KP_Page_Down => GHOSTTY_KEY_PAGE_DOWN,
        gdk::Key::Page_Up | gdk::Key::KP_Page_Up => GHOSTTY_KEY_PAGE_UP,
        gdk::Key::Down | gdk::Key::KP_Down => GHOSTTY_KEY_ARROW_DOWN,
        gdk::Key::Left | gdk::Key::KP_Left => GHOSTTY_KEY_ARROW_LEFT,
        gdk::Key::Right | gdk::Key::KP_Right => GHOSTTY_KEY_ARROW_RIGHT,
        gdk::Key::Up | gdk::Key::KP_Up => GHOSTTY_KEY_ARROW_UP,
        gdk::Key::Escape => GHOSTTY_KEY_ESCAPE,
        gdk::Key::F1 => GHOSTTY_KEY_F1,
        gdk::Key::F2 => GHOSTTY_KEY_F2,
        gdk::Key::F3 => GHOSTTY_KEY_F3,
        gdk::Key::F4 => GHOSTTY_KEY_F4,
        gdk::Key::F5 => GHOSTTY_KEY_F5,
        gdk::Key::F6 => GHOSTTY_KEY_F6,
        gdk::Key::F7 => GHOSTTY_KEY_F7,
        gdk::Key::F8 => GHOSTTY_KEY_F8,
        gdk::Key::F9 => GHOSTTY_KEY_F9,
        gdk::Key::F10 => GHOSTTY_KEY_F10,
        gdk::Key::F11 => GHOSTTY_KEY_F11,
        gdk::Key::F12 => GHOSTTY_KEY_F12,
        gdk::Key::F13 => GHOSTTY_KEY_F13,
        gdk::Key::F14 => GHOSTTY_KEY_F14,
        gdk::Key::F15 => GHOSTTY_KEY_F15,
        gdk::Key::F16 => GHOSTTY_KEY_F16,
        gdk::Key::F17 => GHOSTTY_KEY_F17,
        gdk::Key::F18 => GHOSTTY_KEY_F18,
        gdk::Key::F19 => GHOSTTY_KEY_F19,
        gdk::Key::F20 => GHOSTTY_KEY_F20,
        gdk::Key::F21 => GHOSTTY_KEY_F21,
        gdk::Key::F22 => GHOSTTY_KEY_F22,
        gdk::Key::F23 => GHOSTTY_KEY_F23,
        gdk::Key::F24 => GHOSTTY_KEY_F24,
        gdk::Key::F25 => GHOSTTY_KEY_F25,
        gdk::Key::Print => GHOSTTY_KEY_PRINT_SCREEN,
        gdk::Key::Scroll_Lock => GHOSTTY_KEY_SCROLL_LOCK,
        gdk::Key::Pause => GHOSTTY_KEY_PAUSE,
        _ => return None,
    })
}

fn ghostty_mouse_button(button: u32) -> c_int {
    match button {
        1 => GHOSTTY_MOUSE_BUTTON_LEFT,
        2 => GHOSTTY_MOUSE_BUTTON_MIDDLE,
        3 => GHOSTTY_MOUSE_BUTTON_RIGHT,
        4 => GHOSTTY_MOUSE_BUTTON_FOUR,
        5 => GHOSTTY_MOUSE_BUTTON_FIVE,
        6 => GHOSTTY_MOUSE_BUTTON_SIX,
        7 => GHOSTTY_MOUSE_BUTTON_SEVEN,
        8 => GHOSTTY_MOUSE_BUTTON_EIGHT,
        9 => GHOSTTY_MOUSE_BUTTON_NINE,
        10 => GHOSTTY_MOUSE_BUTTON_TEN,
        11 => GHOSTTY_MOUSE_BUTTON_ELEVEN,
        _ => GHOSTTY_MOUSE_BUTTON_UNKNOWN,
    }
}

fn ghostty_stylus_pressure(stylus: &gtk::GestureStylus, fallback: f64) -> f64 {
    stylus
        .axis(gdk::AxisUse::Pressure)
        .map(ghostty_pressure_value)
        .unwrap_or_else(|| ghostty_pressure_value(fallback))
}

fn ghostty_pressure_value(pressure: f64) -> f64 {
    if !pressure.is_finite() || pressure <= 0.0 {
        0.0
    } else if pressure >= 1.0 {
        1.0
    } else {
        pressure
    }
}

fn ghostty_pressure_stage(pressure: f64) -> c_int {
    let pressure = ghostty_pressure_value(pressure);
    if pressure <= 0.0 {
        GHOSTTY_MOUSE_PRESSURE_NONE
    } else if pressure >= GHOSTTY_STYLUS_DEEP_PRESSURE_THRESHOLD {
        GHOSTTY_MOUSE_PRESSURE_DEEP
    } else {
        GHOSTTY_MOUSE_PRESSURE_NORMAL
    }
}

struct GtkGlProcResolver {
    handles: Vec<usize>,
    egl_get_proc_address: Option<GtkGlGetProcAddress>,
    glx_get_proc_address: Option<GtkGlGetProcAddress>,
}

impl GtkGlProcResolver {
    fn load() -> Self {
        let mut handles = Vec::new();
        for library in [
            c"libOpenGL.so.0",
            c"libOpenGL.so",
            c"libGL.so.1",
            c"libGL.so",
            c"libEGL.so.1",
            c"libEGL.so",
        ] {
            let handle = unsafe { dlopen(library.as_ptr(), RTLD_NOW) };
            if !handle.is_null() && !handles.contains(&(handle as usize)) {
                handles.push(handle as usize);
            }
        }
        let egl_get_proc_address = load_gl_proc_resolver(&handles, c"eglGetProcAddress");
        let glx_get_proc_address = load_gl_proc_resolver(&handles, c"glXGetProcAddressARB")
            .or_else(|| load_gl_proc_resolver(&handles, c"glXGetProcAddress"));
        Self {
            handles,
            egl_get_proc_address,
            glx_get_proc_address,
        }
    }

    fn resolve(&self, name: *const c_char) -> *mut c_void {
        for handle in &self.handles {
            let symbol = unsafe { dlsym(*handle as *mut c_void, name) };
            if !symbol.is_null() {
                return symbol;
            }
        }
        for resolver in [self.egl_get_proc_address, self.glx_get_proc_address]
            .into_iter()
            .flatten()
        {
            let symbol = unsafe { resolver(name) };
            if !symbol.is_null() {
                return symbol;
            }
        }
        ptr::null_mut()
    }
}

fn load_gl_proc_resolver(handles: &[usize], name: &CStr) -> Option<GtkGlGetProcAddress> {
    handles.iter().find_map(|handle| {
        let symbol = unsafe { dlsym(*handle as *mut c_void, name.as_ptr()) };
        (!symbol.is_null()).then(|| unsafe { std::mem::transmute_copy(&symbol) })
    })
}

fn gtk_gl_proc_resolver() -> &'static GtkGlProcResolver {
    static RESOLVER: OnceLock<GtkGlProcResolver> = OnceLock::new();
    RESOLVER.get_or_init(GtkGlProcResolver::load)
}

fn gdk_file_list_type() -> Option<glib::Type> {
    static GDK_FILE_LIST_TYPE: OnceLock<Option<glib::Type>> = OnceLock::new();
    *GDK_FILE_LIST_TYPE.get_or_init(load_gdk_file_list_type)
}

fn load_gdk_file_list_type() -> Option<glib::Type> {
    let symbol = load_gtk_symbol(c"gdk_file_list_get_type")?;
    let get_type: GdkFileListGetType = unsafe { std::mem::transmute_copy(&symbol) };
    let type_ = unsafe { get_type() };
    if type_ == 0 {
        None
    } else {
        Some(unsafe { from_glib(type_) })
    }
}

fn gdk_file_list_get_files() -> Option<GdkFileListGetFiles> {
    static GDK_FILE_LIST_GET_FILES: OnceLock<Option<GdkFileListGetFiles>> = OnceLock::new();
    *GDK_FILE_LIST_GET_FILES.get_or_init(load_gdk_file_list_get_files)
}

fn load_gdk_file_list_get_files() -> Option<GdkFileListGetFiles> {
    let symbol = load_gtk_symbol(c"gdk_file_list_get_files")?;
    Some(unsafe { std::mem::transmute_copy(&symbol) })
}

fn load_gtk_symbol(symbol: &CStr) -> Option<*mut c_void> {
    for library in [c"libgtk-4.so.1", c"libgtk-4.so"] {
        let handle = unsafe { dlopen(library.as_ptr(), RTLD_NOW) };
        if handle.is_null() {
            continue;
        }
        let symbol = unsafe { dlsym(handle, symbol.as_ptr()) };
        if !symbol.is_null() {
            return Some(symbol);
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ghostty_embed::{
        GhosttyActionCellSize, GhosttyActionChildExited, GhosttyActionColorChange,
        GhosttyActionCommandFinished, GhosttyActionDesktopNotification, GhosttyActionInitialSize,
        GhosttyActionKeySequence, GhosttyActionKeyTable, GhosttyActionKeyTableActivate,
        GhosttyActionKeyTableValue, GhosttyActionMouseOverLink, GhosttyActionMoveTab,
        GhosttyActionOpenUrl, GhosttyActionPayload, GhosttyActionProgressReport, GhosttyActionPwd,
        GhosttyActionReloadConfig, GhosttyActionResizeSplit, GhosttyActionScrollbar,
        GhosttyActionSearchSelected, GhosttyActionSearchTotal, GhosttyActionSetTitle,
        GhosttyActionSizeLimit, GhosttyActionStartSearch, GhosttyDiagnostic, GhosttyInputTrigger,
        GhosttyInputTriggerKey, GhosttyTargetValue, GHOSTTY_ACTION_OPEN_URL_KIND_HTML,
        GHOSTTY_BINDING_FLAGS_PERFORMABLE, GHOSTTY_TARGET_APP, GHOSTTY_TARGET_SURFACE,
    };
    use std::sync::atomic::AtomicUsize;

    static TEST_RELOAD_CONFIG_NEW: AtomicUsize = AtomicUsize::new(0);
    static TEST_RELOAD_CONFIG_LOAD_DEFAULT: AtomicUsize = AtomicUsize::new(0);
    static TEST_RELOAD_CONFIG_LOAD_RECURSIVE: AtomicUsize = AtomicUsize::new(0);
    static TEST_RELOAD_CONFIG_FINALIZE: AtomicUsize = AtomicUsize::new(0);
    static TEST_RELOAD_CONFIG_DIAGNOSTICS: AtomicUsize = AtomicUsize::new(0);
    static TEST_RELOAD_CONFIG_FREE: AtomicUsize = AtomicUsize::new(0);
    static TEST_RELOAD_LOCK: Mutex<()> = Mutex::new(());
    static TEST_CLIPBOARD_LOCK: Mutex<()> = Mutex::new(());
    static TEST_CLIPBOARD_COMPLETE_CALLS: AtomicUsize = AtomicUsize::new(0);
    static TEST_CLIPBOARD_COMPLETE_SURFACE: AtomicUsize = AtomicUsize::new(0);
    static TEST_CLIPBOARD_COMPLETE_REQUEST: AtomicUsize = AtomicUsize::new(0);
    static TEST_CLIPBOARD_COMPLETE_TEXT_LEN: AtomicUsize = AtomicUsize::new(0);
    static TEST_CLIPBOARD_COMPLETE_CONFIRMED: AtomicBool = AtomicBool::new(false);

    unsafe extern "C" fn test_complete_clipboard_request(
        surface: GhosttySurface,
        text: *const c_char,
        request: *mut c_void,
        confirmed: bool,
    ) -> bool {
        TEST_CLIPBOARD_COMPLETE_CALLS.fetch_add(1, Ordering::SeqCst);
        TEST_CLIPBOARD_COMPLETE_SURFACE.store(surface as usize, Ordering::SeqCst);
        TEST_CLIPBOARD_COMPLETE_REQUEST.store(request as usize, Ordering::SeqCst);
        TEST_CLIPBOARD_COMPLETE_TEXT_LEN.store(
            (!text.is_null())
                .then(|| CStr::from_ptr(text).to_bytes().len())
                .unwrap_or(0),
            Ordering::SeqCst,
        );
        TEST_CLIPBOARD_COMPLETE_CONFIRMED.store(confirmed, Ordering::SeqCst);
        true
    }

    fn test_ghostty_callbacks(surface: Option<GhosttySurface>) -> GtkGhosttyCallbacks {
        GtkGhosttyCallbacks {
            token: next_ghostty_callback_token(),
            app: None,
            app_tick: None,
            area: None,
            surface,
            config_new: None,
            config_free: None,
            config_load_default_files: None,
            config_load_recursive_files: None,
            config_finalize: None,
            config_diagnostics_count: None,
            config_get_diagnostic: None,
            config_open_path: None,
            string_free: None,
            app_update_config: None,
            surface_inherited_config: None,
            surface_inherited_config_free: None,
            surface_update_config: None,
            complete_clipboard_request: None,
            surface_needs_confirm_quit: None,
            close_surface_id: None,
            app_state: None,
            cursor_state: Arc::new(Mutex::new(GhosttyCursorState::default())),
            selection_sync_requested: Arc::new(AtomicBool::new(false)),
            inspector_visible: Arc::new(AtomicBool::new(false)),
            rendering: AtomicBool::new(false),
        }
    }

    fn test_ghostty_app_callbacks(app: Option<GhosttyApp>) -> GtkGhosttyAppCallbacks {
        GtkGhosttyAppCallbacks {
            token: next_ghostty_callback_token(),
            app,
            app_tick: None,
            surfaces: Mutex::new(HashMap::new()),
            focused_surface: AtomicUsize::new(0),
        }
    }

    #[derive(Default)]
    struct ReloadUpdateCounters {
        app_updates: usize,
        surface_updates: usize,
        last_config: GhosttyConfig,
    }

    fn test_reload_config_ptr() -> GhosttyConfig {
        0x51c0ffeeusize as GhosttyConfig
    }

    fn reset_test_reload_config_counters() {
        TEST_RELOAD_CONFIG_NEW.store(0, Ordering::SeqCst);
        TEST_RELOAD_CONFIG_LOAD_DEFAULT.store(0, Ordering::SeqCst);
        TEST_RELOAD_CONFIG_LOAD_RECURSIVE.store(0, Ordering::SeqCst);
        TEST_RELOAD_CONFIG_FINALIZE.store(0, Ordering::SeqCst);
        TEST_RELOAD_CONFIG_DIAGNOSTICS.store(0, Ordering::SeqCst);
        TEST_RELOAD_CONFIG_FREE.store(0, Ordering::SeqCst);
    }

    #[test]
    fn pointer_focus_selects_the_embedded_terminal_surface() {
        let mut app = AppState::with_paths(None, None).expect("app state");
        let first = app
            .handle("surface.current", &json!({}))
            .expect("current surface")["surface_id"]
            .as_str()
            .expect("surface id")
            .to_string();
        let second = app
            .handle(
                "surface.split",
                &json!({
                    "surface_id": first,
                    "direction": "right",
                    "type": "terminal",
                    "focus": true
                }),
            )
            .expect("second terminal")["surface_id"]
            .as_str()
            .expect("second surface id")
            .to_string();
        assert_ne!(first, second);

        let app_state = Arc::new(Mutex::new(app));
        assert!(focus_embedded_terminal_surface(&app_state, &first));

        let mut app = app_state.lock().expect("app lock");
        let current = app
            .handle("surface.current", &json!({}))
            .expect("focused surface");
        assert_eq!(current["surface_id"], first);
        let terminals = app
            .handle("debug.terminals", &json!({}))
            .expect("terminal list");
        let first_row = terminals["terminals"]
            .as_array()
            .expect("terminals")
            .iter()
            .find(|row| row["surface_id"] == first)
            .expect("first terminal");
        assert_eq!(first_row["focused"], true);
        assert_eq!(first_row["widget_focused"], true);
    }

    unsafe extern "C" fn test_reload_config_new() -> GhosttyConfig {
        TEST_RELOAD_CONFIG_NEW.fetch_add(1, Ordering::SeqCst);
        test_reload_config_ptr()
    }

    unsafe extern "C" fn test_reload_config_load_default_files(_config: GhosttyConfig) -> bool {
        TEST_RELOAD_CONFIG_LOAD_DEFAULT.fetch_add(1, Ordering::SeqCst);
        true
    }

    unsafe extern "C" fn test_reload_config_load_default_files_false(
        _config: GhosttyConfig,
    ) -> bool {
        TEST_RELOAD_CONFIG_LOAD_DEFAULT.fetch_add(1, Ordering::SeqCst);
        false
    }

    unsafe extern "C" fn test_reload_config_load_recursive_files(_config: GhosttyConfig) -> bool {
        TEST_RELOAD_CONFIG_LOAD_RECURSIVE.fetch_add(1, Ordering::SeqCst);
        true
    }

    unsafe extern "C" fn test_reload_config_load_recursive_files_false(
        _config: GhosttyConfig,
    ) -> bool {
        TEST_RELOAD_CONFIG_LOAD_RECURSIVE.fetch_add(1, Ordering::SeqCst);
        false
    }

    unsafe extern "C" fn test_reload_config_finalize(_config: GhosttyConfig) -> bool {
        TEST_RELOAD_CONFIG_FINALIZE.fetch_add(1, Ordering::SeqCst);
        true
    }

    unsafe extern "C" fn test_reload_config_finalize_false(_config: GhosttyConfig) -> bool {
        TEST_RELOAD_CONFIG_FINALIZE.fetch_add(1, Ordering::SeqCst);
        false
    }

    unsafe extern "C" fn test_reload_config_free(_config: GhosttyConfig) {
        TEST_RELOAD_CONFIG_FREE.fetch_add(1, Ordering::SeqCst);
    }

    unsafe extern "C" fn test_reload_config_diagnostics_count_empty(_config: GhosttyConfig) -> u32 {
        TEST_RELOAD_CONFIG_DIAGNOSTICS.fetch_add(1, Ordering::SeqCst);
        0
    }

    unsafe extern "C" fn test_reload_config_get_diagnostic_empty(
        _config: GhosttyConfig,
        _idx: u32,
    ) -> GhosttyDiagnostic {
        GhosttyDiagnostic {
            message: ptr::null(),
        }
    }

    unsafe extern "C" fn test_reload_app_update_config(
        app: GhosttyApp,
        config: GhosttyConfig,
    ) -> bool {
        unsafe {
            let counters = &mut *(app as *mut ReloadUpdateCounters);
            counters.app_updates += 1;
            counters.last_config = config;
        }
        true
    }

    unsafe extern "C" fn test_reload_app_update_config_false(
        app: GhosttyApp,
        config: GhosttyConfig,
    ) -> bool {
        unsafe {
            let counters = &mut *(app as *mut ReloadUpdateCounters);
            counters.app_updates += 1;
            counters.last_config = config;
        }
        false
    }

    unsafe extern "C" fn test_reload_surface_update_config(
        surface: GhosttySurface,
        config: GhosttyConfig,
    ) -> bool {
        unsafe {
            let counters = &mut *(surface as *mut ReloadUpdateCounters);
            counters.surface_updates += 1;
            counters.last_config = config;
        }
        true
    }

    unsafe extern "C" fn test_reload_surface_update_config_false(
        surface: GhosttySurface,
        config: GhosttyConfig,
    ) -> bool {
        unsafe {
            let counters = &mut *(surface as *mut ReloadUpdateCounters);
            counters.surface_updates += 1;
            counters.last_config = config;
        }
        false
    }

    fn test_reload_target(
        app: Option<GhosttyApp>,
        surface: Option<GhosttySurface>,
    ) -> GtkGhosttyActionTarget {
        GtkGhosttyActionTarget {
            area: None,
            app,
            surface,
            config_new: Some(test_reload_config_new),
            config_free: Some(test_reload_config_free),
            config_load_default_files: Some(test_reload_config_load_default_files),
            config_load_recursive_files: Some(test_reload_config_load_recursive_files),
            config_finalize: Some(test_reload_config_finalize),
            config_diagnostics_count: Some(test_reload_config_diagnostics_count_empty),
            config_get_diagnostic: Some(test_reload_config_get_diagnostic_empty),
            config_open_path: None,
            string_free: None,
            app_update_config: Some(test_reload_app_update_config),
            surface_inherited_config: None,
            surface_inherited_config_free: None,
            surface_update_config: Some(test_reload_surface_update_config),
            surface_needs_confirm_quit: None,
            app_state: None,
            surface_id: None,
            cursor_state: Arc::new(Mutex::new(GhosttyCursorState::default())),
            selection_sync_requested: Arc::new(AtomicBool::new(false)),
            inspector_visible: Arc::new(AtomicBool::new(false)),
        }
    }

    #[test]
    fn gtk_ghostty_positive_u32_clamps_non_positive_sizes() {
        assert_eq!(positive_u32(-10), 1);
        assert_eq!(positive_u32(0), 1);
        assert_eq!(positive_u32(42), 42);
    }

    #[test]
    fn gtk_ghostty_color_scheme_maps_gtk_dark_preference() {
        assert_eq!(
            ghostty_color_scheme_for_dark_preference(false),
            GHOSTTY_COLOR_SCHEME_LIGHT
        );
        assert_eq!(
            ghostty_color_scheme_for_dark_preference(true),
            GHOSTTY_COLOR_SCHEME_DARK
        );
    }

    #[test]
    fn gtk_ghostty_reload_config_updates_app_config() {
        let _lock = TEST_RELOAD_LOCK.lock().expect("reload test lock");
        reset_test_reload_config_counters();
        let mut counters = ReloadUpdateCounters::default();
        let target = test_reload_target(Some(&mut counters as *mut _ as GhosttyApp), None);

        assert!(gtk_ghostty_reload_config(
            &target,
            &GtkGhosttyActionScope::App
        ));

        assert_eq!(TEST_RELOAD_CONFIG_NEW.load(Ordering::SeqCst), 1);
        assert_eq!(TEST_RELOAD_CONFIG_LOAD_DEFAULT.load(Ordering::SeqCst), 1);
        assert_eq!(TEST_RELOAD_CONFIG_LOAD_RECURSIVE.load(Ordering::SeqCst), 1);
        assert_eq!(TEST_RELOAD_CONFIG_FINALIZE.load(Ordering::SeqCst), 1);
        assert_eq!(TEST_RELOAD_CONFIG_FREE.load(Ordering::SeqCst), 1);
        assert_eq!(counters.app_updates, 1);
        assert_eq!(counters.surface_updates, 0);
        assert_eq!(counters.last_config, test_reload_config_ptr());
    }

    #[test]
    fn gtk_ghostty_reload_config_reports_app_update_failure() {
        let _lock = TEST_RELOAD_LOCK.lock().expect("reload test lock");
        reset_test_reload_config_counters();
        let mut counters = ReloadUpdateCounters::default();
        let mut target = test_reload_target(Some(&mut counters as *mut _ as GhosttyApp), None);
        target.app_update_config = Some(test_reload_app_update_config_false);

        assert!(!gtk_ghostty_reload_config(
            &target,
            &GtkGhosttyActionScope::App
        ));

        assert_eq!(TEST_RELOAD_CONFIG_NEW.load(Ordering::SeqCst), 1);
        assert_eq!(TEST_RELOAD_CONFIG_LOAD_DEFAULT.load(Ordering::SeqCst), 1);
        assert_eq!(TEST_RELOAD_CONFIG_LOAD_RECURSIVE.load(Ordering::SeqCst), 1);
        assert_eq!(TEST_RELOAD_CONFIG_FINALIZE.load(Ordering::SeqCst), 1);
        assert_eq!(TEST_RELOAD_CONFIG_FREE.load(Ordering::SeqCst), 1);
        assert_eq!(counters.app_updates, 1);
        assert_eq!(counters.surface_updates, 0);
        assert_eq!(counters.last_config, test_reload_config_ptr());
    }

    #[test]
    fn gtk_ghostty_reload_config_reports_load_failure() {
        let _lock = TEST_RELOAD_LOCK.lock().expect("reload test lock");
        reset_test_reload_config_counters();
        let mut counters = ReloadUpdateCounters::default();
        let mut target = test_reload_target(Some(&mut counters as *mut _ as GhosttyApp), None);
        target.config_load_default_files = Some(test_reload_config_load_default_files_false);

        assert!(!gtk_ghostty_reload_config(
            &target,
            &GtkGhosttyActionScope::App
        ));

        assert_eq!(TEST_RELOAD_CONFIG_NEW.load(Ordering::SeqCst), 1);
        assert_eq!(TEST_RELOAD_CONFIG_LOAD_DEFAULT.load(Ordering::SeqCst), 1);
        assert_eq!(TEST_RELOAD_CONFIG_LOAD_RECURSIVE.load(Ordering::SeqCst), 0);
        assert_eq!(TEST_RELOAD_CONFIG_FINALIZE.load(Ordering::SeqCst), 0);
        assert_eq!(TEST_RELOAD_CONFIG_DIAGNOSTICS.load(Ordering::SeqCst), 1);
        assert_eq!(TEST_RELOAD_CONFIG_FREE.load(Ordering::SeqCst), 1);
        assert_eq!(counters.app_updates, 0);
        assert_eq!(counters.surface_updates, 0);
    }

    #[test]
    fn gtk_ghostty_reload_config_reports_recursive_load_failure() {
        let _lock = TEST_RELOAD_LOCK.lock().expect("reload test lock");
        reset_test_reload_config_counters();
        let mut counters = ReloadUpdateCounters::default();
        let mut target = test_reload_target(Some(&mut counters as *mut _ as GhosttyApp), None);
        target.config_load_recursive_files = Some(test_reload_config_load_recursive_files_false);

        assert!(!gtk_ghostty_reload_config(
            &target,
            &GtkGhosttyActionScope::App
        ));

        assert_eq!(TEST_RELOAD_CONFIG_NEW.load(Ordering::SeqCst), 1);
        assert_eq!(TEST_RELOAD_CONFIG_LOAD_DEFAULT.load(Ordering::SeqCst), 1);
        assert_eq!(TEST_RELOAD_CONFIG_LOAD_RECURSIVE.load(Ordering::SeqCst), 1);
        assert_eq!(TEST_RELOAD_CONFIG_FINALIZE.load(Ordering::SeqCst), 0);
        assert_eq!(TEST_RELOAD_CONFIG_DIAGNOSTICS.load(Ordering::SeqCst), 1);
        assert_eq!(TEST_RELOAD_CONFIG_FREE.load(Ordering::SeqCst), 1);
        assert_eq!(counters.app_updates, 0);
        assert_eq!(counters.surface_updates, 0);
    }

    #[test]
    fn gtk_ghostty_reload_config_reports_finalize_failure() {
        let _lock = TEST_RELOAD_LOCK.lock().expect("reload test lock");
        reset_test_reload_config_counters();
        let mut counters = ReloadUpdateCounters::default();
        let mut target = test_reload_target(Some(&mut counters as *mut _ as GhosttyApp), None);
        target.config_finalize = Some(test_reload_config_finalize_false);

        assert!(!gtk_ghostty_reload_config(
            &target,
            &GtkGhosttyActionScope::App
        ));

        assert_eq!(TEST_RELOAD_CONFIG_NEW.load(Ordering::SeqCst), 1);
        assert_eq!(TEST_RELOAD_CONFIG_LOAD_DEFAULT.load(Ordering::SeqCst), 1);
        assert_eq!(TEST_RELOAD_CONFIG_LOAD_RECURSIVE.load(Ordering::SeqCst), 1);
        assert_eq!(TEST_RELOAD_CONFIG_FINALIZE.load(Ordering::SeqCst), 1);
        assert_eq!(TEST_RELOAD_CONFIG_DIAGNOSTICS.load(Ordering::SeqCst), 1);
        assert_eq!(TEST_RELOAD_CONFIG_FREE.load(Ordering::SeqCst), 1);
        assert_eq!(counters.app_updates, 0);
        assert_eq!(counters.surface_updates, 0);
    }

    #[test]
    fn gtk_ghostty_reload_config_updates_surface_config() {
        let _lock = TEST_RELOAD_LOCK.lock().expect("reload test lock");
        reset_test_reload_config_counters();
        let mut counters = ReloadUpdateCounters::default();
        let target = test_reload_target(None, Some(&mut counters as *mut _ as GhosttySurface));

        assert!(gtk_ghostty_reload_config(
            &target,
            &GtkGhosttyActionScope::Surface
        ));

        assert_eq!(TEST_RELOAD_CONFIG_NEW.load(Ordering::SeqCst), 1);
        assert_eq!(TEST_RELOAD_CONFIG_LOAD_DEFAULT.load(Ordering::SeqCst), 1);
        assert_eq!(TEST_RELOAD_CONFIG_LOAD_RECURSIVE.load(Ordering::SeqCst), 1);
        assert_eq!(TEST_RELOAD_CONFIG_FINALIZE.load(Ordering::SeqCst), 1);
        assert_eq!(TEST_RELOAD_CONFIG_FREE.load(Ordering::SeqCst), 1);
        assert_eq!(counters.app_updates, 0);
        assert_eq!(counters.surface_updates, 1);
        assert_eq!(counters.last_config, test_reload_config_ptr());
    }

    #[test]
    fn gtk_ghostty_reload_config_reports_surface_update_failure() {
        let _lock = TEST_RELOAD_LOCK.lock().expect("reload test lock");
        reset_test_reload_config_counters();
        let mut counters = ReloadUpdateCounters::default();
        let mut target = test_reload_target(None, Some(&mut counters as *mut _ as GhosttySurface));
        target.surface_update_config = Some(test_reload_surface_update_config_false);

        assert!(!gtk_ghostty_reload_config(
            &target,
            &GtkGhosttyActionScope::Surface
        ));

        assert_eq!(TEST_RELOAD_CONFIG_NEW.load(Ordering::SeqCst), 1);
        assert_eq!(TEST_RELOAD_CONFIG_LOAD_DEFAULT.load(Ordering::SeqCst), 1);
        assert_eq!(TEST_RELOAD_CONFIG_LOAD_RECURSIVE.load(Ordering::SeqCst), 1);
        assert_eq!(TEST_RELOAD_CONFIG_FINALIZE.load(Ordering::SeqCst), 1);
        assert_eq!(TEST_RELOAD_CONFIG_FREE.load(Ordering::SeqCst), 1);
        assert_eq!(counters.app_updates, 0);
        assert_eq!(counters.surface_updates, 1);
        assert_eq!(counters.last_config, test_reload_config_ptr());
    }

    #[test]
    fn gtk_ghostty_im_rectangle_clamps_invalid_values() {
        let rect = gtk_ghostty_im_rectangle(GhosttyImePoint {
            x: f64::NAN,
            y: 12.6,
            width: 0.0,
            height: f64::INFINITY,
        });
        assert_eq!(rect.x(), 0);
        assert_eq!(rect.y(), 13);
        assert_eq!(rect.width(), 1);
        assert_eq!(rect.height(), 1);
    }

    #[test]
    fn gtk_ghostty_im_state_keeps_plain_commits_on_key_event() {
        let mut state = GhosttyImState::default();
        let prior = state.begin_key_event();
        assert_eq!(prior, GhosttyImKeyEvent::NotComposing);
        state.commit("a".to_string());

        assert!(!state.should_stop_key_event(prior, true));
        assert_eq!(state.take_key_commit().as_deref(), Some("a"));
        state.end_key_event();
        assert_eq!(state.in_key_event, GhosttyImKeyEvent::False);
    }

    #[test]
    fn gtk_ghostty_im_state_sends_composed_commits_directly() {
        let mut state = GhosttyImState::default();
        state.preedit_start();
        let prior = state.begin_key_event();
        state.commit("é".to_string());

        assert!(state.should_stop_key_event(prior, true));
        let (preedit, commits) = state.drain_effects();
        assert_eq!(preedit, Some(None));
        assert_eq!(commits, vec!["é".to_string()]);
    }

    #[test]
    fn gtk_ghostty_drop_path_text_shell_escapes_paths() {
        assert_eq!(
            ghostty_drop_path_text([
                PathBuf::from("/tmp/plain"),
                PathBuf::from("/tmp/space name"),
                PathBuf::from("/tmp/quote's")
            ])
            .as_deref(),
            Some("/tmp/plain\n/tmp/space\\ name\n/tmp/quote\\'s\n")
        );
    }

    #[test]
    fn gtk_ghostty_drop_path_text_quotes_multiline_paths() {
        assert_eq!(
            ghostty_drop_path_text([PathBuf::from("/tmp/line\nbreak")]).as_deref(),
            Some("'/tmp/line\nbreak'\n")
        );
    }

    #[test]
    fn gtk_ghostty_drop_path_text_ignores_empty_file_lists() {
        assert_eq!(ghostty_drop_path_text(std::iter::empty::<PathBuf>()), None);
    }

    #[test]
    fn gtk_ghostty_drop_uri_list_normalizes_file_urls() {
        assert_eq!(
            ghostty_drop_string_text(
                "# comment\r\nfile:///tmp/cmux%20drop/a.txt\r\nfile://localhost/tmp/two%20words\n"
            )
            .as_deref(),
            Some("/tmp/cmux\\ drop/a.txt\n/tmp/two\\ words\n")
        );
    }

    #[test]
    fn gtk_ghostty_drop_string_text_keeps_plain_text() {
        assert_eq!(
            ghostty_drop_string_text("literal\ntext").as_deref(),
            Some("literal\ntext")
        );
    }

    #[test]
    fn gtk_ghostty_callback_tokens_guard_stale_userdata() {
        let callbacks = Box::new(test_ghostty_callbacks(None));
        let ptr = ghostty_callback_ptr(callbacks.as_ref());

        assert_eq!(ghostty_callback_token(ptr), None);
        register_ghostty_callbacks(callbacks.as_ref());
        assert_eq!(ghostty_callback_token(ptr), Some(callbacks.token));
        assert_eq!(
            with_ghostty_callbacks(ptr, callbacks.token, |callbacks| callbacks.token),
            Some(callbacks.token)
        );

        unregister_ghostty_callbacks(callbacks.as_ref());
        assert_eq!(ghostty_callback_token(ptr), None);
        assert_eq!(
            with_ghostty_callbacks(ptr, callbacks.token, |_| "stale"),
            None
        );
    }

    #[test]
    fn gtk_ghostty_callback_rotation_invalidates_prior_surface_generation() {
        let mut callbacks = Box::new(test_ghostty_callbacks(None));
        let ptr = ghostty_callback_ptr(callbacks.as_ref());
        let previous_token = callbacks.token;
        register_ghostty_callbacks(callbacks.as_ref());

        rotate_ghostty_callback_registration(callbacks.as_mut());

        assert_ne!(callbacks.token, previous_token);
        assert_eq!(ghostty_callback_token(ptr), Some(callbacks.token));
        assert_eq!(
            with_ghostty_callbacks(ptr, previous_token, |_| "stale surface"),
            None
        );
        assert_eq!(
            with_ghostty_callbacks(ptr, callbacks.token, |_| "current surface"),
            Some("current surface")
        );
        unregister_ghostty_callbacks(callbacks.as_ref());
    }

    #[test]
    fn gtk_ghostty_callbacks_only_accept_redraw_with_live_surface() {
        let callbacks = test_ghostty_callbacks(None);
        assert!(!callbacks.has_live_surface());
        assert!(callbacks.redraw_area().is_none());

        let callbacks = test_ghostty_callbacks(Some(0x1234usize as GhosttySurface));
        assert!(callbacks.has_live_surface());
        assert!(callbacks.redraw_area().is_none());
    }

    #[test]
    fn gtk_ghostty_clipboard_confirm_completion_requires_original_surface() {
        let _guard = TEST_CLIPBOARD_LOCK.lock().expect("clipboard test lock");
        TEST_CLIPBOARD_COMPLETE_CALLS.store(0, Ordering::SeqCst);
        TEST_CLIPBOARD_COMPLETE_SURFACE.store(0, Ordering::SeqCst);
        TEST_CLIPBOARD_COMPLETE_REQUEST.store(0, Ordering::SeqCst);
        TEST_CLIPBOARD_COMPLETE_TEXT_LEN.store(0, Ordering::SeqCst);
        TEST_CLIPBOARD_COMPLETE_CONFIRMED.store(false, Ordering::SeqCst);

        let original_surface = 0x1234usize as GhosttySurface;
        let replacement_surface = 0x5678usize as GhosttySurface;
        let request = 0xabcdusize;
        let mut callbacks = test_ghostty_callbacks(Some(original_surface));
        callbacks.complete_clipboard_request = Some(test_complete_clipboard_request);
        let mut callbacks = Box::new(callbacks);
        let ptr = ghostty_callback_ptr(callbacks.as_ref());
        register_ghostty_callbacks(callbacks.as_ref());

        let initial_text = c"initial clipboard";
        assert!(gtk_ghostty_complete_initial_clipboard_read(
            test_complete_clipboard_request,
            original_surface,
            request - 1,
            initial_text,
        ));
        assert!(!TEST_CLIPBOARD_COMPLETE_CONFIRMED.load(Ordering::SeqCst));
        assert_eq!(TEST_CLIPBOARD_COMPLETE_TEXT_LEN.load(Ordering::SeqCst), 17);
        TEST_CLIPBOARD_COMPLETE_CALLS.store(0, Ordering::SeqCst);

        assert!(gtk_ghostty_complete_clipboard_confirmation(
            ptr,
            callbacks.token,
            original_surface as usize,
            request,
            "confirmed paste",
            true,
        ));
        assert_eq!(TEST_CLIPBOARD_COMPLETE_CALLS.load(Ordering::SeqCst), 1);
        assert_eq!(
            TEST_CLIPBOARD_COMPLETE_SURFACE.load(Ordering::SeqCst),
            original_surface as usize
        );
        assert_eq!(
            TEST_CLIPBOARD_COMPLETE_REQUEST.load(Ordering::SeqCst),
            request
        );
        assert_eq!(TEST_CLIPBOARD_COMPLETE_TEXT_LEN.load(Ordering::SeqCst), 15);
        assert!(TEST_CLIPBOARD_COMPLETE_CONFIRMED.load(Ordering::SeqCst));

        assert!(gtk_ghostty_complete_clipboard_confirmation(
            ptr,
            callbacks.token,
            original_surface as usize,
            request + 1,
            "denied paste",
            false,
        ));
        assert_eq!(TEST_CLIPBOARD_COMPLETE_CALLS.load(Ordering::SeqCst), 2);
        assert_eq!(TEST_CLIPBOARD_COMPLETE_TEXT_LEN.load(Ordering::SeqCst), 0);

        callbacks.surface = Some(replacement_surface);
        assert!(!gtk_ghostty_complete_clipboard_confirmation(
            ptr,
            callbacks.token,
            original_surface as usize,
            request,
            "stale paste",
            true,
        ));
        assert_eq!(TEST_CLIPBOARD_COMPLETE_CALLS.load(Ordering::SeqCst), 2);

        unregister_ghostty_callbacks(callbacks.as_ref());
    }

    #[test]
    fn gtk_ghostty_clipboard_confirmation_copy_matches_request_type() {
        let paste = gtk_ghostty_clipboard_confirmation_spec(GHOSTTY_CLIPBOARD_REQUEST_PASTE)
            .expect("paste confirmation");
        assert_eq!(paste.deny_label, "Cancel");
        assert_eq!(paste.allow_label, "Paste");

        let read = gtk_ghostty_clipboard_confirmation_spec(GHOSTTY_CLIPBOARD_REQUEST_OSC_52_READ)
            .expect("OSC 52 read confirmation");
        let write = gtk_ghostty_clipboard_confirmation_spec(GHOSTTY_CLIPBOARD_REQUEST_OSC_52_WRITE)
            .expect("OSC 52 write confirmation");
        assert_eq!(read.deny_label, "Deny");
        assert_eq!(read.allow_label, "Allow");
        assert_eq!(write.title, "Authorize Clipboard Access");
        assert!(gtk_ghostty_clipboard_confirmation_spec(c_int::MAX).is_none());
    }

    #[test]
    fn gtk_ghostty_app_callback_tokens_guard_stale_app_userdata() {
        let app = 0xfeedusize as GhosttyApp;
        let callbacks = Box::new(test_ghostty_app_callbacks(Some(app)));
        let ptr = ghostty_app_callback_ptr(callbacks.as_ref());

        assert_eq!(ghostty_app_callback_ref(app), None);
        register_ghostty_app(callbacks.as_ref());
        assert_eq!(ghostty_app_callback_ref(app), Some((ptr, callbacks.token)));

        unregister_ghostty_app(callbacks.as_ref());
        assert_eq!(ghostty_app_callback_ref(app), None);
    }

    #[test]
    fn gtk_ghostty_shared_app_routes_actions_to_target_and_focus() {
        let app = 0xfeedusize as GhosttyApp;
        let first_surface = 0x1000usize as GhosttySurface;
        let second_surface = 0x2000usize as GhosttySurface;
        let first = Box::new(test_ghostty_callbacks(Some(first_surface)));
        let second = Box::new(test_ghostty_callbacks(Some(second_surface)));
        register_ghostty_callbacks(first.as_ref());
        register_ghostty_callbacks(second.as_ref());

        let app_callbacks = Box::new(test_ghostty_app_callbacks(Some(app)));
        register_ghostty_app(app_callbacks.as_ref());
        app_callbacks
            .surfaces
            .lock()
            .expect("surface routes")
            .extend([
                (
                    first_surface as usize,
                    (ghostty_callback_ptr(first.as_ref()), first.token),
                ),
                (
                    second_surface as usize,
                    (ghostty_callback_ptr(second.as_ref()), second.token),
                ),
            ]);
        app_callbacks
            .focused_surface
            .store(second_surface as usize, Ordering::Release);

        let target = GhosttyTarget {
            tag: GHOSTTY_TARGET_SURFACE,
            target: crate::ghostty_embed::GhosttyTargetValue {
                surface: first_surface,
            },
        };
        assert_eq!(
            ghostty_callback_ref_for_action(app, target),
            Some((
                ghostty_callback_ptr(first.as_ref()),
                first.token,
                first_surface as usize
            ))
        );

        let app_target = GhosttyTarget {
            tag: GHOSTTY_TARGET_APP,
            target: crate::ghostty_embed::GhosttyTargetValue {
                surface: ptr::null_mut(),
            },
        };
        assert_eq!(
            ghostty_callback_ref_for_action(app, app_target),
            Some((
                ghostty_callback_ptr(second.as_ref()),
                second.token,
                second_surface as usize
            ))
        );

        unregister_ghostty_app(app_callbacks.as_ref());
        unregister_ghostty_callbacks(first.as_ref());
        unregister_ghostty_callbacks(second.as_ref());
    }

    #[test]
    fn gtk_ghostty_close_request_value_marks_confirmation_state() {
        assert_eq!(gtk_ghostty_close_request_value(false, false), None);
        assert_eq!(gtk_ghostty_close_request_value(true, false), None);
        assert_eq!(
            gtk_ghostty_close_request_value(false, true),
            Some("needs_confirm")
        );
        assert_eq!(
            gtk_ghostty_close_request_value(true, true),
            Some("needs_confirm:process_alive")
        );
    }

    #[test]
    fn gtk_ghostty_app_close_request_action_marks_confirmation_state() {
        assert_eq!(gtk_ghostty_app_close_request_action("quit", false), None);
        assert_eq!(
            gtk_ghostty_app_close_request_action("quit", true),
            Some("quit_requested")
        );
        assert_eq!(
            gtk_ghostty_app_close_request_action("close_all_windows", true),
            Some("close_all_windows_requested")
        );
        assert_eq!(
            gtk_ghostty_app_close_request_action("other", true),
            Some("close_requested")
        );
    }

    #[test]
    fn gtk_ghostty_window_and_tab_close_request_actions_mark_confirmation_state() {
        assert_eq!(
            gtk_ghostty_window_close_request_action("close_window", false),
            None
        );
        assert_eq!(
            gtk_ghostty_window_close_request_action("new_window", true),
            None
        );
        assert_eq!(
            gtk_ghostty_window_close_request_action("close_window", true),
            Some("close_window_requested")
        );
        assert_eq!(
            gtk_ghostty_current_tab_close_request_action("other", true),
            None
        );
        assert_eq!(
            gtk_ghostty_current_tab_close_request_action("right", true),
            None
        );
        assert_eq!(
            gtk_ghostty_current_tab_close_request_action("this", true),
            Some("close_tab_requested")
        );
    }

    #[test]
    fn gtk_ghostty_close_surface_context_requires_live_surface() {
        let callbacks = GtkGhosttyCallbacks {
            token: next_ghostty_callback_token(),
            app: None,
            app_tick: None,
            area: None,
            surface: None,
            config_new: None,
            config_free: None,
            config_load_default_files: None,
            config_load_recursive_files: None,
            config_finalize: None,
            config_diagnostics_count: None,
            config_get_diagnostic: None,
            config_open_path: None,
            string_free: None,
            app_update_config: None,
            surface_inherited_config: None,
            surface_inherited_config_free: None,
            surface_update_config: None,
            complete_clipboard_request: None,
            surface_needs_confirm_quit: None,
            close_surface_id: Some("surface:1".to_string()),
            app_state: None,
            cursor_state: Arc::new(Mutex::new(GhosttyCursorState::default())),
            selection_sync_requested: Arc::new(AtomicBool::new(false)),
            inspector_visible: Arc::new(AtomicBool::new(false)),
            rendering: AtomicBool::new(false),
        };

        assert!(gtk_ghostty_close_surface_context(&callbacks).is_none());
    }

    #[test]
    fn gtk_ghostty_next_inspector_visible_handles_modes() {
        assert_eq!(
            gtk_ghostty_next_inspector_visible(false, Some("show")),
            Some(true)
        );
        assert_eq!(
            gtk_ghostty_next_inspector_visible(true, Some("hide")),
            Some(false)
        );
        assert_eq!(
            gtk_ghostty_next_inspector_visible(false, Some("toggle")),
            Some(true)
        );
        assert_eq!(
            gtk_ghostty_next_inspector_visible(true, Some("toggle")),
            Some(false)
        );
        assert_eq!(gtk_ghostty_next_inspector_visible(true, None), None);
    }

    #[test]
    fn gtk_ghostty_fullscreen_mode_maps_known_ghostty_modes_to_linux_fullscreen() {
        assert!(gtk_ghostty_fullscreen_mode_is_supported(None));
        assert!(gtk_ghostty_fullscreen_mode_is_supported(Some("native")));
        assert!(gtk_ghostty_fullscreen_mode_is_supported(Some(
            "macos_non_native"
        )));
        assert!(gtk_ghostty_fullscreen_mode_is_supported(Some(
            "macos_non_native_visible_menu"
        )));
        assert!(gtk_ghostty_fullscreen_mode_is_supported(Some(
            "macos_non_native_padded_notch"
        )));
        assert!(!gtk_ghostty_fullscreen_mode_is_supported(Some("unknown")));
    }

    #[test]
    fn gtk_ghostty_render_guard_restores_flag_on_drop() {
        let rendering = AtomicBool::new(false);
        {
            let _guard = GhosttyRenderGuard::enter(&rendering);
            assert!(rendering.load(Ordering::SeqCst));
        }
        assert!(!rendering.load(Ordering::SeqCst));
    }

    #[test]
    fn gtk_ghostty_inspector_key_maps_common_navigation() {
        assert_eq!(
            ghostty_inspector_key(gdk::Key::BackSpace),
            Some(GHOSTTY_KEY_BACKSPACE)
        );
        assert_eq!(
            ghostty_inspector_key(gdk::Key::Return),
            Some(GHOSTTY_KEY_ENTER)
        );
        assert_eq!(
            ghostty_inspector_key(gdk::Key::Page_Down),
            Some(GHOSTTY_KEY_PAGE_DOWN)
        );
        assert_eq!(
            ghostty_inspector_key(gdk::Key::Up),
            Some(GHOSTTY_KEY_ARROW_UP)
        );
        assert_eq!(ghostty_inspector_key(gdk::Key::F1), Some(GHOSTTY_KEY_F1));
        assert_eq!(ghostty_inspector_key(gdk::Key::F12), Some(GHOSTTY_KEY_F12));
        assert_eq!(ghostty_inspector_key(gdk::Key::F24), Some(GHOSTTY_KEY_F24));
        assert_eq!(ghostty_inspector_key(gdk::Key::F25), Some(GHOSTTY_KEY_F25));
        assert_eq!(
            ghostty_inspector_key(gdk::Key::Print),
            Some(GHOSTTY_KEY_PRINT_SCREEN)
        );
        assert_eq!(
            ghostty_inspector_key(gdk::Key::Scroll_Lock),
            Some(GHOSTTY_KEY_SCROLL_LOCK)
        );
        assert_eq!(
            ghostty_inspector_key(gdk::Key::Pause),
            Some(GHOSTTY_KEY_PAUSE)
        );
        assert_eq!(ghostty_inspector_key(gdk::Key::a), None);
    }

    #[test]
    fn gtk_ghostty_action_event_extracts_owned_title_and_pwd() {
        let surface = 0xbeefusize as GhosttySurface;
        let target = ghostty_surface_target(surface);
        let title = CString::new("runtime title").expect("title");
        let event = ghostty_action_event(
            target,
            GhosttyAction {
                tag: GHOSTTY_ACTION_SET_TITLE,
                action: GhosttyActionPayload {
                    set_title: GhosttyActionSetTitle {
                        title: title.as_ptr(),
                    },
                },
            },
        )
        .expect("title event");
        assert_eq!(
            event,
            GtkGhosttyActionEvent::SetTitle {
                surface: surface as usize,
                title: "runtime title".to_string()
            }
        );

        let pwd = CString::new("/tmp/cmux").expect("pwd");
        let event = ghostty_action_event(
            target,
            GhosttyAction {
                tag: GHOSTTY_ACTION_PWD,
                action: GhosttyActionPayload {
                    pwd: GhosttyActionPwd { pwd: pwd.as_ptr() },
                },
            },
        )
        .expect("pwd event");
        assert_eq!(
            event,
            GtkGhosttyActionEvent::Pwd {
                surface: surface as usize,
                pwd: "/tmp/cmux".to_string()
            }
        );
    }

    #[test]
    fn gtk_ghostty_action_event_extracts_notification_and_bell() {
        let surface = 0xbeefusize as GhosttySurface;
        let target = ghostty_surface_target(surface);
        let title = CString::new("Build").expect("title");
        let body = CString::new("Done").expect("body");
        let event = ghostty_action_event(
            target,
            GhosttyAction {
                tag: GHOSTTY_ACTION_DESKTOP_NOTIFICATION,
                action: GhosttyActionPayload {
                    desktop_notification: GhosttyActionDesktopNotification {
                        title: title.as_ptr(),
                        body: body.as_ptr(),
                    },
                },
            },
        )
        .expect("notification event");
        assert_eq!(
            event,
            GtkGhosttyActionEvent::DesktopNotification {
                surface: surface as usize,
                title: "Build".to_string(),
                body: "Done".to_string()
            }
        );

        let event = ghostty_action_event(
            target,
            GhosttyAction {
                tag: GHOSTTY_ACTION_RING_BELL,
                action: GhosttyActionPayload { padding: [0; 3] },
            },
        )
        .expect("bell event");
        assert_eq!(
            event,
            GtkGhosttyActionEvent::RingBell {
                surface: surface as usize
            }
        );

        let event = ghostty_action_event(
            target,
            GhosttyAction {
                tag: GHOSTTY_ACTION_SHOW_CHILD_EXITED,
                action: GhosttyActionPayload {
                    child_exited: GhosttyActionChildExited {
                        exit_code: 7,
                        runtime_ms: 1250,
                    },
                },
            },
        )
        .expect("child-exited event");
        assert_eq!(
            event,
            GtkGhosttyActionEvent::ShowChildExited {
                surface: surface as usize,
                exit_code: 7,
                runtime_ms: 1250
            }
        );
    }

    #[test]
    fn gtk_ghostty_action_event_extracts_layout_actions() {
        let surface = 0xbeefusize as GhosttySurface;
        let target = ghostty_surface_target(surface);

        let event = ghostty_action_event(
            target,
            GhosttyAction {
                tag: GHOSTTY_ACTION_TOGGLE_COMMAND_PALETTE,
                action: GhosttyActionPayload { padding: [0; 3] },
            },
        )
        .expect("command palette event");
        assert_eq!(
            event,
            GtkGhosttyActionEvent::ToggleCommandPalette {
                surface: surface as usize
            }
        );

        let event = ghostty_action_event(
            target,
            GhosttyAction {
                tag: GHOSTTY_ACTION_OPEN_CONFIG,
                action: GhosttyActionPayload { padding: [0; 3] },
            },
        )
        .expect("open config event");
        assert_eq!(
            event,
            GtkGhosttyActionEvent::OpenConfig {
                surface: surface as usize
            }
        );

        let event = ghostty_action_event(
            target,
            GhosttyAction {
                tag: GHOSTTY_ACTION_RELOAD_CONFIG,
                action: GhosttyActionPayload {
                    reload_config: GhosttyActionReloadConfig { soft: true },
                },
            },
        )
        .expect("reload config event");
        assert_eq!(
            event,
            GtkGhosttyActionEvent::ReloadConfig {
                surface: surface as usize,
                scope: GtkGhosttyActionScope::Surface,
                soft: true
            }
        );

        let event = ghostty_action_event(
            target,
            GhosttyAction {
                tag: GHOSTTY_ACTION_READONLY,
                action: GhosttyActionPayload {
                    readonly: GHOSTTY_READONLY_ON,
                },
            },
        )
        .expect("readonly event");
        assert_eq!(
            event,
            GtkGhosttyActionEvent::Readonly {
                surface: surface as usize,
                readonly: true
            }
        );

        assert_eq!(
            ghostty_action_event(
                target,
                GhosttyAction {
                    tag: GHOSTTY_ACTION_READONLY,
                    action: GhosttyActionPayload { readonly: 99 },
                },
            ),
            None
        );

        let event = ghostty_action_event(
            target,
            GhosttyAction {
                tag: GHOSTTY_ACTION_COPY_TITLE_TO_CLIPBOARD,
                action: GhosttyActionPayload { padding: [0; 3] },
            },
        )
        .expect("copy-title event");
        assert_eq!(
            event,
            GtkGhosttyActionEvent::CopyTitleToClipboard {
                surface: surface as usize
            }
        );

        let event = ghostty_action_event(
            target,
            GhosttyAction {
                tag: GHOSTTY_ACTION_SELECTION_CHANGED,
                action: GhosttyActionPayload { padding: [0; 3] },
            },
        )
        .expect("selection changed event");
        assert_eq!(
            event,
            GtkGhosttyActionEvent::SelectionChanged {
                surface: surface as usize
            }
        );

        let event = ghostty_action_event(
            target,
            GhosttyAction {
                tag: GHOSTTY_ACTION_PRESENT_TERMINAL,
                action: GhosttyActionPayload { padding: [0; 3] },
            },
        )
        .expect("present event");
        assert_eq!(
            event,
            GtkGhosttyActionEvent::PresentTerminal {
                surface: surface as usize
            }
        );

        let event = ghostty_action_event(
            target,
            GhosttyAction {
                tag: GHOSTTY_ACTION_NEW_TAB,
                action: GhosttyActionPayload { padding: [0; 3] },
            },
        )
        .expect("new-tab event");
        assert_eq!(
            event,
            GtkGhosttyActionEvent::NewTab {
                surface: surface as usize
            }
        );

        let event = ghostty_action_event(
            target,
            GhosttyAction {
                tag: GHOSTTY_ACTION_NEW_SPLIT,
                action: GhosttyActionPayload {
                    new_split: GHOSTTY_SPLIT_DIRECTION_LEFT,
                },
            },
        )
        .expect("new-split event");
        assert_eq!(
            event,
            GtkGhosttyActionEvent::NewSplit {
                surface: surface as usize,
                direction: "left"
            }
        );

        assert_eq!(
            ghostty_action_event(
                target,
                GhosttyAction {
                    tag: GHOSTTY_ACTION_NEW_SPLIT,
                    action: GhosttyActionPayload { new_split: 99 },
                },
            ),
            None
        );

        let event = ghostty_action_event(
            target,
            GhosttyAction {
                tag: GHOSTTY_ACTION_CLOSE_TAB,
                action: GhosttyActionPayload {
                    close_tab_mode: GHOSTTY_CLOSE_TAB_MODE_OTHER,
                },
            },
        )
        .expect("close-tab event");
        assert_eq!(
            event,
            GtkGhosttyActionEvent::CloseTab {
                surface: surface as usize,
                mode: "other"
            }
        );

        let event = ghostty_action_event(
            target,
            GhosttyAction {
                tag: GHOSTTY_ACTION_GOTO_SPLIT,
                action: GhosttyActionPayload {
                    goto_split: GHOSTTY_GOTO_SPLIT_NEXT,
                },
            },
        )
        .expect("goto-split event");
        assert_eq!(
            event,
            GtkGhosttyActionEvent::GotoSplit {
                surface: surface as usize,
                direction: "next"
            }
        );

        let event = ghostty_action_event(
            target,
            GhosttyAction {
                tag: GHOSTTY_ACTION_RESIZE_SPLIT,
                action: GhosttyActionPayload {
                    resize_split: GhosttyActionResizeSplit {
                        amount: 12,
                        direction: GHOSTTY_RESIZE_SPLIT_RIGHT,
                    },
                },
            },
        )
        .expect("resize-split event");
        assert_eq!(
            event,
            GtkGhosttyActionEvent::ResizeSplit {
                surface: surface as usize,
                direction: "right",
                amount: 12
            }
        );

        let event = ghostty_action_event(
            target,
            GhosttyAction {
                tag: GHOSTTY_ACTION_EQUALIZE_SPLITS,
                action: GhosttyActionPayload { padding: [0; 3] },
            },
        )
        .expect("equalize event");
        assert_eq!(
            event,
            GtkGhosttyActionEvent::EqualizeSplits {
                surface: surface as usize
            }
        );

        let event = ghostty_action_event(
            target,
            GhosttyAction {
                tag: GHOSTTY_ACTION_TOGGLE_SPLIT_ZOOM,
                action: GhosttyActionPayload { padding: [0; 3] },
            },
        )
        .expect("toggle split zoom event");
        assert_eq!(
            event,
            GtkGhosttyActionEvent::ToggleSplitZoom {
                surface: surface as usize
            }
        );
    }

    #[test]
    fn gtk_ghostty_action_event_extracts_geometry_and_health_actions() {
        let surface = 0xbeefusize as GhosttySurface;
        let target = ghostty_surface_target(surface);

        let event = ghostty_action_event(
            target,
            GhosttyAction {
                tag: GHOSTTY_ACTION_SIZE_LIMIT,
                action: GhosttyActionPayload {
                    size_limit: GhosttyActionSizeLimit {
                        min_width: 100,
                        min_height: 40,
                        max_width: 2000,
                        max_height: 1200,
                    },
                },
            },
        )
        .expect("size-limit event");
        assert_eq!(
            event,
            GtkGhosttyActionEvent::SizeLimit {
                surface: surface as usize,
                min_width: 100,
                min_height: 40,
                max_width: 2000,
                max_height: 1200
            }
        );

        let event = ghostty_action_event(
            target,
            GhosttyAction {
                tag: GHOSTTY_ACTION_INITIAL_SIZE,
                action: GhosttyActionPayload {
                    initial_size: GhosttyActionInitialSize {
                        width: 900,
                        height: 600,
                    },
                },
            },
        )
        .expect("initial-size event");
        assert_eq!(
            event,
            GtkGhosttyActionEvent::InitialSize {
                surface: surface as usize,
                width: 900,
                height: 600
            }
        );

        let event = ghostty_action_event(
            target,
            GhosttyAction {
                tag: GHOSTTY_ACTION_CELL_SIZE,
                action: GhosttyActionPayload {
                    cell_size: GhosttyActionCellSize {
                        width: 10,
                        height: 20,
                    },
                },
            },
        )
        .expect("cell-size event");
        assert_eq!(
            event,
            GtkGhosttyActionEvent::CellSize {
                surface: surface as usize,
                width: 10,
                height: 20
            }
        );

        let event = ghostty_action_event(
            target,
            GhosttyAction {
                tag: GHOSTTY_ACTION_RENDERER_HEALTH,
                action: GhosttyActionPayload {
                    renderer_health: GHOSTTY_RENDERER_HEALTH_UNHEALTHY,
                },
            },
        )
        .expect("renderer-health event");
        assert_eq!(
            event,
            GtkGhosttyActionEvent::RendererHealth {
                surface: surface as usize,
                status: "unhealthy"
            }
        );

        assert_eq!(
            ghostty_action_event(
                target,
                GhosttyAction {
                    tag: GHOSTTY_ACTION_RENDERER_HEALTH,
                    action: GhosttyActionPayload {
                        renderer_health: 99,
                    },
                },
            ),
            None
        );
    }

    #[test]
    fn gtk_ghostty_action_event_extracts_lifecycle_state_actions() {
        let surface = 0xbeefusize as GhosttySurface;
        let target = ghostty_surface_target(surface);

        let event = ghostty_action_event(
            target,
            GhosttyAction {
                tag: GHOSTTY_ACTION_PROMPT_TITLE,
                action: GhosttyActionPayload {
                    prompt_title: GHOSTTY_PROMPT_TITLE_TAB,
                },
            },
        )
        .expect("prompt-title event");
        assert_eq!(
            event,
            GtkGhosttyActionEvent::PromptTitle {
                surface: surface as usize,
                target: "tab"
            }
        );

        let event = ghostty_action_event(
            target,
            GhosttyAction {
                tag: GHOSTTY_ACTION_QUIT_TIMER,
                action: GhosttyActionPayload {
                    quit_timer: GHOSTTY_QUIT_TIMER_START,
                },
            },
        )
        .expect("quit-timer event");
        assert_eq!(
            event,
            GtkGhosttyActionEvent::QuitTimer {
                surface: surface as usize,
                mode: "start"
            }
        );

        let event = ghostty_action_event(
            target,
            GhosttyAction {
                tag: GHOSTTY_ACTION_FLOAT_WINDOW,
                action: GhosttyActionPayload {
                    float_window: GHOSTTY_FLOAT_WINDOW_TOGGLE,
                },
            },
        )
        .expect("float-window event");
        assert_eq!(
            event,
            GtkGhosttyActionEvent::FloatWindow {
                surface: surface as usize,
                mode: "toggle"
            }
        );

        let event = ghostty_action_event(
            target,
            GhosttyAction {
                tag: GHOSTTY_ACTION_SECURE_INPUT,
                action: GhosttyActionPayload {
                    secure_input: GHOSTTY_SECURE_INPUT_ON,
                },
            },
        )
        .expect("secure-input event");
        assert_eq!(
            event,
            GtkGhosttyActionEvent::SecureInput {
                surface: surface as usize,
                mode: "on"
            }
        );

        let event = ghostty_action_event(
            target,
            GhosttyAction {
                tag: GHOSTTY_ACTION_COLOR_CHANGE,
                action: GhosttyActionPayload {
                    color_change: GhosttyActionColorChange {
                        kind: GHOSTTY_COLOR_KIND_FOREGROUND,
                        r: 1,
                        g: 2,
                        b: 3,
                    },
                },
            },
        )
        .expect("foreground color event");
        assert_eq!(
            event,
            GtkGhosttyActionEvent::ColorChange {
                surface: surface as usize,
                kind: "foreground",
                palette_index: None,
                r: 1,
                g: 2,
                b: 3
            }
        );

        let event = ghostty_action_event(
            target,
            GhosttyAction {
                tag: GHOSTTY_ACTION_COLOR_CHANGE,
                action: GhosttyActionPayload {
                    color_change: GhosttyActionColorChange {
                        kind: 12,
                        r: 4,
                        g: 5,
                        b: 6,
                    },
                },
            },
        )
        .expect("palette color event");
        assert_eq!(
            event,
            GtkGhosttyActionEvent::ColorChange {
                surface: surface as usize,
                kind: "palette",
                palette_index: Some(12),
                r: 4,
                g: 5,
                b: 6
            }
        );

        let event = ghostty_action_event(
            target,
            GhosttyAction {
                tag: GHOSTTY_ACTION_CONFIG_CHANGE,
                action: GhosttyActionPayload { padding: [0; 3] },
            },
        )
        .expect("config-change event");
        assert_eq!(
            event,
            GtkGhosttyActionEvent::ConfigChange {
                surface: surface as usize
            }
        );

        assert_eq!(
            ghostty_action_event(
                target,
                GhosttyAction {
                    tag: GHOSTTY_ACTION_FLOAT_WINDOW,
                    action: GhosttyActionPayload { float_window: 99 },
                },
            ),
            None
        );
        assert_eq!(
            ghostty_action_event(
                target,
                GhosttyAction {
                    tag: GHOSTTY_ACTION_COLOR_CHANGE,
                    action: GhosttyActionPayload {
                        color_change: GhosttyActionColorChange {
                            kind: -99,
                            r: 0,
                            g: 0,
                            b: 0,
                        },
                    },
                },
            ),
            None
        );
    }

    #[test]
    fn gtk_ghostty_action_event_extracts_window_tab_and_ui_actions() {
        let surface = 0xbeefusize as GhosttySurface;
        let target = ghostty_surface_target(surface);

        let event = ghostty_action_event(
            target,
            GhosttyAction {
                tag: GHOSTTY_ACTION_NEW_WINDOW,
                action: GhosttyActionPayload { padding: [0; 3] },
            },
        )
        .expect("new-window event");
        assert_eq!(
            event,
            GtkGhosttyActionEvent::WindowAction {
                surface: surface as usize,
                action: "new_window",
                value: None
            }
        );

        let event = ghostty_action_event(
            target,
            GhosttyAction {
                tag: GHOSTTY_ACTION_GOTO_WINDOW,
                action: GhosttyActionPayload {
                    goto_window: GHOSTTY_GOTO_WINDOW_NEXT,
                },
            },
        )
        .expect("goto-window event");
        assert_eq!(
            event,
            GtkGhosttyActionEvent::WindowAction {
                surface: surface as usize,
                action: "goto_window",
                value: Some("next")
            }
        );

        let event = ghostty_action_event(
            target,
            GhosttyAction {
                tag: GHOSTTY_ACTION_QUIT,
                action: GhosttyActionPayload { padding: [0; 3] },
            },
        )
        .expect("quit event");
        assert_eq!(
            event,
            GtkGhosttyActionEvent::Quit {
                surface: surface as usize
            }
        );

        let event = ghostty_action_event(
            target,
            GhosttyAction {
                tag: GHOSTTY_ACTION_CLOSE_ALL_WINDOWS,
                action: GhosttyActionPayload { padding: [0; 3] },
            },
        )
        .expect("close-all-windows event");
        assert_eq!(
            event,
            GtkGhosttyActionEvent::CloseAllWindows {
                surface: surface as usize
            }
        );

        let event = ghostty_action_event(
            target,
            GhosttyAction {
                tag: GHOSTTY_ACTION_MOVE_TAB,
                action: GhosttyActionPayload {
                    move_tab: GhosttyActionMoveTab { amount: -1 },
                },
            },
        )
        .expect("move-tab event");
        assert_eq!(
            event,
            GtkGhosttyActionEvent::TabAction {
                surface: surface as usize,
                action: "move_tab",
                amount: Some(-1)
            }
        );

        let event = ghostty_action_event(
            target,
            GhosttyAction {
                tag: GHOSTTY_ACTION_GOTO_TAB,
                action: GhosttyActionPayload {
                    goto_tab: crate::ghostty_embed::GHOSTTY_GOTO_TAB_NEXT,
                },
            },
        )
        .expect("goto-tab event");
        assert_eq!(
            event,
            GtkGhosttyActionEvent::TabAction {
                surface: surface as usize,
                action: "goto_tab",
                amount: Some(-2)
            }
        );

        let event = ghostty_action_event(
            target,
            GhosttyAction {
                tag: GHOSTTY_ACTION_TOGGLE_FULLSCREEN,
                action: GhosttyActionPayload {
                    toggle_fullscreen: GHOSTTY_FULLSCREEN_NATIVE,
                },
            },
        )
        .expect("fullscreen event");
        assert_eq!(
            event,
            GtkGhosttyActionEvent::UiAction {
                surface: surface as usize,
                action: "toggle_fullscreen",
                value: Some("native"),
                amount: None
            }
        );
        for mode in [
            GHOSTTY_FULLSCREEN_MACOS_NON_NATIVE,
            GHOSTTY_FULLSCREEN_MACOS_NON_NATIVE_VISIBLE_MENU,
            GHOSTTY_FULLSCREEN_MACOS_NON_NATIVE_PADDED_NOTCH,
        ] {
            let event = ghostty_action_event(
                target,
                GhosttyAction {
                    tag: GHOSTTY_ACTION_TOGGLE_FULLSCREEN,
                    action: GhosttyActionPayload {
                        toggle_fullscreen: mode,
                    },
                },
            )
            .expect("fullscreen compatibility event");
            assert_eq!(
                event,
                GtkGhosttyActionEvent::UiAction {
                    surface: surface as usize,
                    action: "toggle_fullscreen",
                    value: Some("native"),
                    amount: None
                }
            );
        }

        let event = ghostty_action_event(
            target,
            GhosttyAction {
                tag: GHOSTTY_ACTION_RENDER,
                action: GhosttyActionPayload { padding: [0; 3] },
            },
        )
        .expect("render event");
        assert_eq!(
            event,
            GtkGhosttyActionEvent::Render {
                surface: surface as usize
            }
        );

        let event = ghostty_action_event(
            target,
            GhosttyAction {
                tag: GHOSTTY_ACTION_INSPECTOR,
                action: GhosttyActionPayload {
                    inspector: GHOSTTY_INSPECTOR_SHOW,
                },
            },
        )
        .expect("inspector event");
        assert_eq!(
            event,
            GtkGhosttyActionEvent::UiAction {
                surface: surface as usize,
                action: "inspector",
                value: Some("show"),
                amount: None
            }
        );

        let event = ghostty_action_event(
            target,
            GhosttyAction {
                tag: GHOSTTY_ACTION_UNDO,
                action: GhosttyActionPayload { padding: [0; 3] },
            },
        )
        .expect("undo event");
        assert_eq!(
            event,
            GtkGhosttyActionEvent::UiAction {
                surface: surface as usize,
                action: "undo",
                value: None,
                amount: None
            }
        );

        let event = ghostty_action_event(
            target,
            GhosttyAction {
                tag: GHOSTTY_ACTION_SHOW_ON_SCREEN_KEYBOARD,
                action: GhosttyActionPayload { padding: [0; 3] },
            },
        )
        .expect("on-screen keyboard event");
        assert_eq!(
            event,
            GtkGhosttyActionEvent::ShowOnScreenKeyboard {
                surface: surface as usize
            }
        );

        assert_eq!(
            ghostty_action_event(
                target,
                GhosttyAction {
                    tag: GHOSTTY_ACTION_GOTO_WINDOW,
                    action: GhosttyActionPayload { goto_window: 99 },
                },
            ),
            None
        );
        assert_eq!(
            ghostty_action_event(
                target,
                GhosttyAction {
                    tag: GHOSTTY_ACTION_TOGGLE_FULLSCREEN,
                    action: GhosttyActionPayload {
                        toggle_fullscreen: 99,
                    },
                },
            ),
            None
        );
    }

    #[test]
    fn gtk_ghostty_action_event_extracts_key_sequence_and_table_actions() {
        let surface = 0xbeefusize as GhosttySurface;
        let target = ghostty_surface_target(surface);
        let table_name = b"leader\0";

        let event = ghostty_action_event(
            target,
            GhosttyAction {
                tag: GHOSTTY_ACTION_KEY_SEQUENCE,
                action: GhosttyActionPayload {
                    key_sequence: GhosttyActionKeySequence {
                        active: true,
                        trigger: GhosttyInputTrigger {
                            tag: GHOSTTY_TRIGGER_UNICODE,
                            key: GhosttyInputTriggerKey {
                                unicode: 'x' as u32,
                            },
                            mods: GHOSTTY_MODS_CTRL | GHOSTTY_MODS_SHIFT,
                        },
                    },
                },
            },
        )
        .expect("key-sequence event");
        assert_eq!(
            event,
            GtkGhosttyActionEvent::KeySequence {
                surface: surface as usize,
                active: true,
                trigger: "unicode:U+0078 mods=shift+ctrl".to_string()
            }
        );

        let event = ghostty_action_event(
            target,
            GhosttyAction {
                tag: GHOSTTY_ACTION_KEY_TABLE,
                action: GhosttyActionPayload {
                    key_table: GhosttyActionKeyTable {
                        tag: GHOSTTY_KEY_TABLE_ACTIVATE,
                        value: GhosttyActionKeyTableValue {
                            activate: GhosttyActionKeyTableActivate {
                                name: table_name.as_ptr().cast(),
                                len: "leader".len(),
                            },
                        },
                    },
                },
            },
        )
        .expect("key-table activate event");
        assert_eq!(
            event,
            GtkGhosttyActionEvent::KeyTable {
                surface: surface as usize,
                mode: "activate",
                name: Some("leader".to_string())
            }
        );

        let event = ghostty_action_event(
            target,
            GhosttyAction {
                tag: GHOSTTY_ACTION_KEY_TABLE,
                action: GhosttyActionPayload {
                    key_table: GhosttyActionKeyTable {
                        tag: GHOSTTY_KEY_TABLE_DEACTIVATE_ALL,
                        value: GhosttyActionKeyTableValue {
                            activate: GhosttyActionKeyTableActivate {
                                name: std::ptr::null(),
                                len: 0,
                            },
                        },
                    },
                },
            },
        )
        .expect("key-table deactivate-all event");
        assert_eq!(
            event,
            GtkGhosttyActionEvent::KeyTable {
                surface: surface as usize,
                mode: "deactivate_all",
                name: None
            }
        );

        assert_eq!(
            ghostty_action_event(
                target,
                GhosttyAction {
                    tag: GHOSTTY_ACTION_KEY_SEQUENCE,
                    action: GhosttyActionPayload {
                        key_sequence: GhosttyActionKeySequence {
                            active: false,
                            trigger: GhosttyInputTrigger {
                                tag: 99,
                                key: GhosttyInputTriggerKey { physical: 0 },
                                mods: 0,
                            },
                        },
                    },
                },
            ),
            None
        );
    }

    #[test]
    fn gtk_ghostty_action_event_uses_fallback_surface_for_app_target() {
        let surface = 0xbeefusize;
        let target = GhosttyTarget {
            tag: GHOSTTY_TARGET_APP,
            target: GhosttyTargetValue {
                surface: std::ptr::null_mut(),
            },
        };

        assert_eq!(
            ghostty_action_event(
                target,
                GhosttyAction {
                    tag: GHOSTTY_ACTION_OPEN_CONFIG,
                    action: GhosttyActionPayload { padding: [0; 3] },
                }
            ),
            None
        );

        let event = ghostty_action_event_with_fallback(
            target,
            GhosttyAction {
                tag: GHOSTTY_ACTION_OPEN_CONFIG,
                action: GhosttyActionPayload { padding: [0; 3] },
            },
            Some(surface),
        )
        .expect("fallback event");
        assert_eq!(event, GtkGhosttyActionEvent::OpenConfig { surface });

        let event = ghostty_action_event_with_fallback(
            target,
            GhosttyAction {
                tag: GHOSTTY_ACTION_RELOAD_CONFIG,
                action: GhosttyActionPayload {
                    reload_config: GhosttyActionReloadConfig { soft: false },
                },
            },
            Some(surface),
        )
        .expect("fallback reload event");
        assert_eq!(
            event,
            GtkGhosttyActionEvent::ReloadConfig {
                surface,
                scope: GtkGhosttyActionScope::App,
                soft: false
            }
        );
    }

    #[test]
    fn gtk_ghostty_action_event_extracts_open_url_and_link_hover() {
        let surface = 0xbeefusize as GhosttySurface;
        let target = ghostty_surface_target(surface);
        let url = "https://example.test/docs?via=ghostty";
        let event = ghostty_action_event(
            target,
            GhosttyAction {
                tag: GHOSTTY_ACTION_OPEN_URL,
                action: GhosttyActionPayload {
                    open_url: GhosttyActionOpenUrl {
                        kind: GHOSTTY_ACTION_OPEN_URL_KIND_HTML,
                        url: url.as_ptr() as *const c_char,
                        len: url.len(),
                    },
                },
            },
        )
        .expect("open-url event");
        assert_eq!(
            event,
            GtkGhosttyActionEvent::OpenUrl {
                surface: surface as usize,
                kind: GHOSTTY_ACTION_OPEN_URL_KIND_HTML,
                url: url.to_string()
            }
        );

        let event = ghostty_action_event(
            target,
            GhosttyAction {
                tag: GHOSTTY_ACTION_MOUSE_OVER_LINK,
                action: GhosttyActionPayload {
                    mouse_over_link: GhosttyActionMouseOverLink {
                        url: url.as_ptr() as *const c_char,
                        len: url.len(),
                    },
                },
            },
        )
        .expect("mouse-over-link event");
        assert_eq!(
            event,
            GtkGhosttyActionEvent::MouseOverLink {
                surface: surface as usize,
                url: Some(url.to_string())
            }
        );

        let event = ghostty_action_event(
            target,
            GhosttyAction {
                tag: GHOSTTY_ACTION_MOUSE_OVER_LINK,
                action: GhosttyActionPayload {
                    mouse_over_link: GhosttyActionMouseOverLink {
                        url: ptr::null(),
                        len: 0,
                    },
                },
            },
        )
        .expect("empty mouse-over-link event");
        assert_eq!(
            event,
            GtkGhosttyActionEvent::MouseOverLink {
                surface: surface as usize,
                url: None
            }
        );
    }

    #[test]
    fn gtk_ghostty_action_event_extracts_progress_and_command_finished() {
        let surface = 0xbeefusize as GhosttySurface;
        let target = ghostty_surface_target(surface);

        let event = ghostty_action_event(
            target,
            GhosttyAction {
                tag: GHOSTTY_ACTION_PROGRESS_REPORT,
                action: GhosttyActionPayload {
                    progress_report: GhosttyActionProgressReport {
                        state: GHOSTTY_PROGRESS_STATE_SET,
                        progress: 42,
                    },
                },
            },
        )
        .expect("progress event");
        assert_eq!(
            event,
            GtkGhosttyActionEvent::ProgressReport {
                surface: surface as usize,
                state: "set",
                progress: Some(42)
            }
        );

        let event = ghostty_action_event(
            target,
            GhosttyAction {
                tag: GHOSTTY_ACTION_PROGRESS_REPORT,
                action: GhosttyActionPayload {
                    progress_report: GhosttyActionProgressReport {
                        state: GHOSTTY_PROGRESS_STATE_INDETERMINATE,
                        progress: -1,
                    },
                },
            },
        )
        .expect("indeterminate progress event");
        assert_eq!(
            event,
            GtkGhosttyActionEvent::ProgressReport {
                surface: surface as usize,
                state: "indeterminate",
                progress: None
            }
        );

        assert_eq!(
            ghostty_action_event(
                target,
                GhosttyAction {
                    tag: GHOSTTY_ACTION_PROGRESS_REPORT,
                    action: GhosttyActionPayload {
                        progress_report: GhosttyActionProgressReport {
                            state: 99,
                            progress: 10,
                        },
                    },
                },
            ),
            None
        );

        let event = ghostty_action_event(
            target,
            GhosttyAction {
                tag: GHOSTTY_ACTION_COMMAND_FINISHED,
                action: GhosttyActionPayload {
                    command_finished: GhosttyActionCommandFinished {
                        exit_code: 7,
                        duration: 1_250_000_000,
                    },
                },
            },
        )
        .expect("command-finished event");
        assert_eq!(
            event,
            GtkGhosttyActionEvent::CommandFinished {
                surface: surface as usize,
                exit_code: Some(7),
                duration_ns: 1_250_000_000
            }
        );

        let event = ghostty_action_event(
            target,
            GhosttyAction {
                tag: GHOSTTY_ACTION_COMMAND_FINISHED,
                action: GhosttyActionPayload {
                    command_finished: GhosttyActionCommandFinished {
                        exit_code: -1,
                        duration: 500_000,
                    },
                },
            },
        )
        .expect("command-finished event without exit code");
        assert_eq!(
            event,
            GtkGhosttyActionEvent::CommandFinished {
                surface: surface as usize,
                exit_code: None,
                duration_ns: 500_000
            }
        );
    }

    #[test]
    fn gtk_ghostty_action_event_extracts_search_actions() {
        let surface = 0xbeefusize as GhosttySurface;
        let target = ghostty_surface_target(surface);
        let needle = CString::new("build failed").expect("needle");

        let event = ghostty_action_event(
            target,
            GhosttyAction {
                tag: GHOSTTY_ACTION_START_SEARCH,
                action: GhosttyActionPayload {
                    start_search: GhosttyActionStartSearch {
                        needle: needle.as_ptr(),
                    },
                },
            },
        )
        .expect("start-search event");
        assert_eq!(
            event,
            GtkGhosttyActionEvent::StartSearch {
                surface: surface as usize,
                needle: "build failed".to_string()
            }
        );

        let event = ghostty_action_event(
            target,
            GhosttyAction {
                tag: GHOSTTY_ACTION_SEARCH_TOTAL,
                action: GhosttyActionPayload {
                    search_total: GhosttyActionSearchTotal { total: 3 },
                },
            },
        )
        .expect("search-total event");
        assert_eq!(
            event,
            GtkGhosttyActionEvent::SearchTotal {
                surface: surface as usize,
                total: Some(3)
            }
        );

        let event = ghostty_action_event(
            target,
            GhosttyAction {
                tag: GHOSTTY_ACTION_SEARCH_SELECTED,
                action: GhosttyActionPayload {
                    search_selected: GhosttyActionSearchSelected { selected: 1 },
                },
            },
        )
        .expect("search-selected event");
        assert_eq!(
            event,
            GtkGhosttyActionEvent::SearchSelected {
                surface: surface as usize,
                selected: Some(1)
            }
        );

        let event = ghostty_action_event(
            target,
            GhosttyAction {
                tag: GHOSTTY_ACTION_SEARCH_TOTAL,
                action: GhosttyActionPayload {
                    search_total: GhosttyActionSearchTotal { total: -1 },
                },
            },
        )
        .expect("empty search-total event");
        assert_eq!(
            event,
            GtkGhosttyActionEvent::SearchTotal {
                surface: surface as usize,
                total: None
            }
        );

        let event = ghostty_action_event(
            target,
            GhosttyAction {
                tag: GHOSTTY_ACTION_END_SEARCH,
                action: GhosttyActionPayload { padding: [0; 3] },
            },
        )
        .expect("end-search event");
        assert_eq!(
            event,
            GtkGhosttyActionEvent::EndSearch {
                surface: surface as usize
            }
        );
    }

    #[test]
    fn gtk_ghostty_action_event_extracts_scrollbar_action() {
        let surface = 0xbeefusize as GhosttySurface;
        let target = ghostty_surface_target(surface);

        let event = ghostty_action_event(
            target,
            GhosttyAction {
                tag: GHOSTTY_ACTION_SCROLLBAR,
                action: GhosttyActionPayload {
                    scrollbar: GhosttyActionScrollbar {
                        total: 500,
                        offset: 120,
                        len: 40,
                    },
                },
            },
        )
        .expect("scrollbar event");
        assert_eq!(
            event,
            GtkGhosttyActionEvent::Scrollbar {
                surface: surface as usize,
                total: 500,
                offset: 120,
                len: 40
            }
        );
    }

    #[test]
    fn gtk_ghostty_action_event_extracts_cursor_actions() {
        let surface = 0xbeefusize as GhosttySurface;
        let target = ghostty_surface_target(surface);

        let event = ghostty_action_event(
            target,
            GhosttyAction {
                tag: GHOSTTY_ACTION_MOUSE_SHAPE,
                action: GhosttyActionPayload {
                    mouse_shape: GHOSTTY_MOUSE_SHAPE_POINTER,
                },
            },
        )
        .expect("mouse shape event");
        assert_eq!(
            event,
            GtkGhosttyActionEvent::MouseShape {
                surface: surface as usize,
                shape: GHOSTTY_MOUSE_SHAPE_POINTER
            }
        );

        let event = ghostty_action_event(
            target,
            GhosttyAction {
                tag: GHOSTTY_ACTION_MOUSE_VISIBILITY,
                action: GhosttyActionPayload {
                    mouse_visibility: GHOSTTY_MOUSE_HIDDEN,
                },
            },
        )
        .expect("mouse visibility event");
        assert_eq!(
            event,
            GtkGhosttyActionEvent::MouseVisibility {
                surface: surface as usize,
                visible: false
            }
        );

        assert_eq!(
            ghostty_action_event(
                target,
                GhosttyAction {
                    tag: GHOSTTY_ACTION_MOUSE_VISIBILITY,
                    action: GhosttyActionPayload {
                        mouse_visibility: 99,
                    },
                },
            ),
            None
        );
    }

    #[test]
    fn gtk_ghostty_cursor_state_composes_shape_and_visibility() {
        let mut state = GhosttyCursorState::default();
        assert_eq!(state.cursor_name(), "text");

        state.shape = GHOSTTY_MOUSE_SHAPE_WAIT;
        assert_eq!(state.cursor_name(), "wait");

        state.link_hover = true;
        assert_eq!(state.cursor_name(), "pointer");

        state.link_hover = false;
        assert_eq!(state.cursor_name(), "wait");

        state.visible = false;
        assert_eq!(state.cursor_name(), "none");

        state.link_hover = true;
        assert_eq!(state.cursor_name(), "none");

        state.visible = true;
        assert_eq!(state.cursor_name(), "pointer");

        state.reset();
        assert_eq!(state, GhosttyCursorState::default());
        assert_eq!(state.cursor_name(), "text");
    }

    #[test]
    fn gtk_ghostty_surface_binding_key_tracks_press_release_identity() {
        let press_text = CString::new("c").expect("text");
        let press = GhosttyInputKey {
            action: GHOSTTY_ACTION_PRESS,
            mods: GHOSTTY_MODS_SUPER | GHOSTTY_MODS_SHIFT,
            consumed_mods: GHOSTTY_MODS_SHIFT,
            keycode: 54,
            text: press_text.as_ptr(),
            unshifted_codepoint: 'c' as u32,
            composing: false,
        };
        let release = GhosttyInputKey {
            action: GHOSTTY_ACTION_RELEASE,
            text: ptr::null(),
            consumed_mods: 0,
            ..press
        };
        assert_eq!(
            GhosttySurfaceBindingKey::from_input(press),
            GhosttySurfaceBindingKey::from_input(release)
        );

        let mut active = HashSet::new();
        active.insert(GhosttySurfaceBindingKey::from_input(press));
        assert!(active.remove(&GhosttySurfaceBindingKey::from_input(release)));
        assert!(!active.remove(&GhosttySurfaceBindingKey::from_input(release)));

        let different_mods = GhosttyInputKey {
            mods: GHOSTTY_MODS_SUPER,
            ..press
        };
        assert_ne!(
            GhosttySurfaceBindingKey::from_input(press),
            GhosttySurfaceBindingKey::from_input(different_mods)
        );
    }

    #[test]
    fn gtk_ghostty_super_surface_binding_release_tracking_uses_binding_flags() {
        assert!(ghostty_should_track_super_surface_binding(
            GHOSTTY_BINDING_FLAGS_CONSUMED,
            true
        ));
        assert!(!ghostty_should_track_super_surface_binding(
            GHOSTTY_BINDING_FLAGS_CONSUMED,
            false
        ));
        assert!(ghostty_should_track_super_surface_binding(0, false));
        assert!(!ghostty_binding_flags_consume_input(
            GHOSTTY_BINDING_FLAGS_PERFORMABLE
        ));
        assert!(ghostty_binding_flags_consume_input(
            GHOSTTY_BINDING_FLAGS_ALL | GHOSTTY_BINDING_FLAGS_GLOBAL
        ));
    }

    #[test]
    fn gtk_ghostty_cursor_names_match_ghostty_gtk_mapping() {
        assert_eq!(ghostty_cursor_name(GHOSTTY_MOUSE_SHAPE_DEFAULT), "default");
        assert_eq!(
            ghostty_cursor_name(GHOSTTY_MOUSE_SHAPE_CONTEXT_MENU),
            "context-menu"
        );
        assert_eq!(
            ghostty_cursor_name(GHOSTTY_MOUSE_SHAPE_VERTICAL_TEXT),
            "vertical-text"
        );
        assert_eq!(ghostty_cursor_name(GHOSTTY_MOUSE_SHAPE_NO_DROP), "no-drop");
        assert_eq!(
            ghostty_cursor_name(GHOSTTY_MOUSE_SHAPE_NOT_ALLOWED),
            "not-allowed"
        );
        assert_eq!(
            ghostty_cursor_name(GHOSTTY_MOUSE_SHAPE_ALL_SCROLL),
            "all-scroll"
        );
        assert_eq!(
            ghostty_cursor_name(GHOSTTY_MOUSE_SHAPE_NESW_RESIZE),
            "nesw-resize"
        );
        assert_eq!(
            ghostty_cursor_name(GHOSTTY_MOUSE_SHAPE_NWSE_RESIZE),
            "nwse-resize"
        );
        assert_eq!(ghostty_cursor_name(GHOSTTY_MOUSE_SHAPE_ZOOM_IN), "zoom-in");
        assert_eq!(
            ghostty_cursor_name(GHOSTTY_MOUSE_SHAPE_ZOOM_OUT),
            "zoom-out"
        );
        assert_eq!(ghostty_cursor_name(999), "default");
    }

    fn ghostty_surface_target(surface: GhosttySurface) -> GhosttyTarget {
        GhosttyTarget {
            tag: GHOSTTY_TARGET_SURFACE,
            target: GhosttyTargetValue { surface },
        }
    }

    #[test]
    fn gtk_ghostty_modifiers_match_ghostty_abi() {
        let mods = ghostty_mods(
            gdk::ModifierType::SHIFT_MASK
                | gdk::ModifierType::CONTROL_MASK
                | gdk::ModifierType::ALT_MASK
                | gdk::ModifierType::SUPER_MASK
                | gdk::ModifierType::LOCK_MASK,
        );
        assert_eq!(
            mods,
            GHOSTTY_MODS_SHIFT
                | GHOSTTY_MODS_CTRL
                | GHOSTTY_MODS_ALT
                | GHOSTTY_MODS_SUPER
                | GHOSTTY_MODS_CAPS
        );
    }

    #[test]
    fn gtk_ghostty_direct_input_history_preserves_text_and_editing_keys() {
        assert_eq!(
            ghostty_input_history_bytes(gdk::Key::a, gdk::ModifierType::empty(), Some("a")),
            Some(b"a".to_vec())
        );
        assert_eq!(
            ghostty_input_history_bytes(
                gdk::Key::C,
                gdk::ModifierType::CONTROL_MASK | gdk::ModifierType::SHIFT_MASK,
                Some("C")
            ),
            Some(vec![0x03])
        );
        assert_eq!(
            ghostty_input_history_bytes(gdk::Key::Return, gdk::ModifierType::empty(), None),
            Some(vec![b'\r'])
        );
        assert_eq!(
            ghostty_input_history_bytes(gdk::Key::BackSpace, gdk::ModifierType::empty(), None),
            Some(vec![0x7f])
        );
    }

    #[test]
    fn gtk_ghostty_copy_mode_keys_and_modifiers_map_to_platform_neutral_input() {
        assert_eq!(gtk_copy_mode_key(gdk::Key::Escape), CopyModeKey::Escape);
        assert_eq!(gtk_copy_mode_key(gdk::Key::KP_Up), CopyModeKey::ArrowUp);
        assert_eq!(
            gtk_copy_mode_key(gdk::Key::Page_Down),
            CopyModeKey::PageDown
        );
        assert_eq!(gtk_copy_mode_key(gdk::Key::y), CopyModeKey::Character('y'));
        assert_eq!(
            gtk_copy_mode_modifiers(
                gdk::ModifierType::SUPER_MASK
                    | gdk::ModifierType::SHIFT_MASK
                    | gdk::ModifierType::CONTROL_MASK,
            ),
            CopyModeModifiers {
                super_key: true,
                shift: true,
                control: true,
                alt: false,
            }
        );
    }

    #[test]
    fn gtk_ghostty_trigger_mod_names_cover_full_ghostty_abi() {
        assert_eq!(
            ghostty_trigger_mod_names(
                GHOSTTY_MODS_CAPS
                    | GHOSTTY_MODS_NUM
                    | GHOSTTY_MODS_SHIFT_RIGHT
                    | GHOSTTY_MODS_CTRL_RIGHT
                    | GHOSTTY_MODS_ALT_RIGHT
                    | GHOSTTY_MODS_SUPER_RIGHT,
            ),
            "caps+num+shift-right+ctrl-right+alt-right+super-right"
        );
    }

    #[test]
    fn gtk_ghostty_queued_key_event_maps_common_keys_to_xkb() {
        let (enter, enter_text) =
            ghostty_queued_key_event(GHOSTTY_ACTION_PRESS, "enter").expect("enter event");
        assert_eq!(enter.action, GHOSTTY_ACTION_PRESS);
        assert_eq!(enter.keycode, 0x24);
        assert_eq!(enter.unshifted_codepoint, b'\r' as u32);
        assert!(enter.text.is_null());
        assert!(enter_text.is_none());

        let (left, left_text) =
            ghostty_queued_key_event(GHOSTTY_ACTION_PRESS, "ctrl-left").expect("left event");
        assert_eq!(left.keycode, 0x71);
        assert_eq!(left.mods, GHOSTTY_MODS_CTRL);
        assert!(left.text.is_null());
        assert!(left_text.is_none());

        let (f12, _) =
            ghostty_queued_key_event(GHOSTTY_ACTION_PRESS, "alt-f12").expect("f12 event");
        assert_eq!(f12.keycode, 0x60);
        assert_eq!(f12.mods, GHOSTTY_MODS_ALT);

        let (f13, f13_text) =
            ghostty_queued_key_event(GHOSTTY_ACTION_PRESS, "ctrl-f13").expect("f13 event");
        assert_eq!(f13.keycode, 0xbf);
        assert_eq!(f13.mods, GHOSTTY_MODS_CTRL);
        assert!(f13.text.is_null());
        assert!(f13_text.is_none());

        let (f24, f24_text) =
            ghostty_queued_key_event(GHOSTTY_ACTION_PRESS, "shift-f24").expect("f24 event");
        assert_eq!(f24.keycode, 0xca);
        assert_eq!(f24.mods, GHOSTTY_MODS_SHIFT);
        assert!(f24.text.is_null());
        assert!(f24_text.is_none());
        let (f25, f25_text) =
            ghostty_queued_key_event(GHOSTTY_ACTION_PRESS, "f25").expect("f25 event");
        assert_eq!(f25.keycode, ghostty_physical_keycode(GHOSTTY_KEY_F25));
        assert_eq!(f25.mods, 0);
        assert!(f25.text.is_null());
        assert!(f25_text.is_none());

        let (shift_f25, _) =
            ghostty_queued_key_event(GHOSTTY_ACTION_PRESS, "shift-f25").expect("shift f25 event");
        assert_eq!(shift_f25.keycode, ghostty_physical_keycode(GHOSTTY_KEY_F25));
        assert_eq!(shift_f25.mods, GHOSTTY_MODS_SHIFT);

        let (super_p, super_p_text) =
            ghostty_queued_key_event(GHOSTTY_ACTION_PRESS, "super-shift-p").expect("super p");
        assert_eq!(super_p.keycode, 0x21);
        assert_eq!(super_p.mods, GHOSTTY_MODS_SUPER | GHOSTTY_MODS_SHIFT);
        assert_eq!(super_p.unshifted_codepoint, 'p' as u32);
        assert_eq!(
            super_p_text.as_ref().map(|text| text.as_bytes()),
            Some(&b"P"[..])
        );

        let (cmd_enter, _) =
            ghostty_queued_key_event(GHOSTTY_ACTION_PRESS, "cmd-enter").expect("cmd enter");
        assert_eq!(cmd_enter.keycode, 0x24);
        assert_eq!(cmd_enter.mods, GHOSTTY_MODS_SUPER);

        let (command_enter, _) =
            ghostty_queued_key_event(GHOSTTY_ACTION_PRESS, "command-enter").expect("command enter");
        assert_eq!(command_enter.mods, GHOSTTY_MODS_SUPER);

        let (meta_a, _) = ghostty_queued_key_event(GHOSTTY_ACTION_PRESS, "meta-a").expect("meta a");
        assert_eq!(meta_a.mods, GHOSTTY_MODS_ALT);
    }

    #[test]
    fn gtk_ghostty_queued_key_event_keeps_printable_text_storage_alive() {
        let (event, text) =
            ghostty_queued_key_event(GHOSTTY_ACTION_PRESS, "shift-a").expect("shift a");
        let text = text.expect("text storage");
        assert_eq!(event.keycode, 0x26);
        assert_eq!(event.mods, GHOSTTY_MODS_SHIFT);
        assert_eq!(event.unshifted_codepoint, 'a' as u32);
        assert_eq!(text.as_bytes(), b"A");
        assert_eq!(event.text, text.as_ptr());
    }

    #[test]
    fn gtk_ghostty_queued_key_release_reuses_key_identity_without_text() {
        let (event, text) =
            ghostty_queued_key_event(GHOSTTY_ACTION_RELEASE, "shift-a").expect("shift a release");
        assert_eq!(event.action, GHOSTTY_ACTION_RELEASE);
        assert_eq!(event.keycode, 0x26);
        assert_eq!(event.mods, GHOSTTY_MODS_SHIFT);
        assert_eq!(event.unshifted_codepoint, 'a' as u32);
        assert!(event.text.is_null());
        assert!(text.is_none());
    }

    #[test]
    fn gtk_ghostty_queued_key_event_accepts_terminal_key_aliases() {
        let (page_down, _) =
            ghostty_queued_key_event(GHOSTTY_ACTION_PRESS, "PageDown").expect("page down");
        assert_eq!(page_down.keycode, 0x75);

        let (delete, _) = ghostty_queued_key_event(GHOSTTY_ACTION_PRESS, "del").expect("delete");
        assert_eq!(delete.keycode, 0x77);

        assert!(ghostty_queued_key_event(GHOSTTY_ACTION_PRESS, "ctrl-_").is_none());
    }

    #[test]
    fn gtk_ghostty_scroll_input_matches_native_gtk_direction_and_precision() {
        assert_eq!(
            ghostty_surface_scroll_input(2.0, -3.0, false, 1.0),
            (-2.0, 3.0, 0)
        );
        assert_eq!(
            ghostty_surface_scroll_input(2.0, -3.0, true, 1.0),
            (-20.0, 30.0, GHOSTTY_SCROLL_MOD_PRECISION)
        );
        assert_eq!(
            ghostty_surface_scroll_input(2.0, -3.0, true, 2.0),
            (-40.0, 60.0, GHOSTTY_SCROLL_MOD_PRECISION)
        );
        assert_eq!(
            ghostty_surface_scroll_input(2.0, -3.0, false, 2.0),
            (-4.0, 6.0, 0)
        );
        assert_eq!(
            ghostty_surface_scroll_input(2.0, -3.0, false, f64::NAN),
            (-2.0, 3.0, 0)
        );
        assert_eq!(
            ghostty_inspector_scroll_input(2.0, -3.0, true),
            (2.0, 3.0, GHOSTTY_SCROLL_MOD_PRECISION)
        );
    }

    #[test]
    fn gtk_ghostty_pointer_input_matches_native_gtk_device_scale() {
        assert_eq!(ghostty_pointer_input(12.5, 24.0, 1.0), (12.5, 24.0));
        assert_eq!(ghostty_pointer_input(12.5, 24.0, 2.0), (25.0, 48.0));
        assert_eq!(ghostty_pointer_input(-1.0, -1.0, 2.0), (-2.0, -2.0));
        assert_eq!(ghostty_pointer_input(12.5, 24.0, 0.0), (12.5, 24.0));
        assert_eq!(ghostty_pointer_input(12.5, 24.0, f64::NAN), (12.5, 24.0));
    }

    #[test]
    fn gtk_ghostty_scrollbar_visibility_requires_setting_and_scrollback() {
        assert!(!ghostty_scrollbar_visible(false, None));
        assert!(ghostty_scrollbar_visible(true, None));
        assert!(!ghostty_scrollbar_visible(
            true,
            Some(GhosttyScrollbarState {
                total: 40,
                offset: 0,
                len: 40,
            })
        ));
        assert!(ghostty_scrollbar_visible(
            true,
            Some(GhosttyScrollbarState {
                total: 500,
                offset: 120,
                len: 40,
            })
        ));
    }

    #[test]
    fn gtk_ghostty_status_hides_success_but_keeps_progress_and_errors_visible() {
        assert!(!ghostty_status_visible(GHOSTTY_RENDERER_ACTIVE_STATUS));
        assert!(ghostty_status_visible("Initializing Ghostty renderer"));
        assert!(ghostty_status_visible("Ghostty renderer draw failed"));
    }

    #[test]
    fn gtk_ghostty_mouse_buttons_map_gdk_button_numbers() {
        assert_eq!(ghostty_mouse_button(1), GHOSTTY_MOUSE_BUTTON_LEFT);
        assert_eq!(ghostty_mouse_button(2), GHOSTTY_MOUSE_BUTTON_MIDDLE);
        assert_eq!(ghostty_mouse_button(3), GHOSTTY_MOUSE_BUTTON_RIGHT);
        assert_eq!(ghostty_mouse_button(4), GHOSTTY_MOUSE_BUTTON_FOUR);
        assert_eq!(ghostty_mouse_button(5), GHOSTTY_MOUSE_BUTTON_FIVE);
        assert_eq!(ghostty_mouse_button(6), GHOSTTY_MOUSE_BUTTON_SIX);
        assert_eq!(ghostty_mouse_button(7), GHOSTTY_MOUSE_BUTTON_SEVEN);
        assert_eq!(ghostty_mouse_button(8), GHOSTTY_MOUSE_BUTTON_EIGHT);
        assert_eq!(ghostty_mouse_button(9), GHOSTTY_MOUSE_BUTTON_NINE);
        assert_eq!(ghostty_mouse_button(10), GHOSTTY_MOUSE_BUTTON_TEN);
        assert_eq!(ghostty_mouse_button(11), GHOSTTY_MOUSE_BUTTON_ELEVEN);
        assert_eq!(ghostty_mouse_button(99), GHOSTTY_MOUSE_BUTTON_UNKNOWN);
    }

    #[test]
    fn gtk_ghostty_pressure_values_are_clamped_for_ghostty() {
        assert_eq!(ghostty_pressure_value(f64::NAN), 0.0);
        assert_eq!(ghostty_pressure_value(f64::NEG_INFINITY), 0.0);
        assert_eq!(ghostty_pressure_value(-0.2), 0.0);
        assert_eq!(ghostty_pressure_value(0.5), 0.5);
        assert_eq!(ghostty_pressure_value(1.5), 1.0);
    }

    #[test]
    fn gtk_ghostty_gl_proc_resolver_finds_desktop_gl_entry_point() {
        assert!(!gtk_gl_proc_resolver()
            .resolve(c"glGetString".as_ptr())
            .is_null());
    }

    #[test]
    fn gtk_ghostty_pressure_stage_maps_to_ghostty_abi_values() {
        assert_eq!(ghostty_pressure_stage(0.0), GHOSTTY_MOUSE_PRESSURE_NONE);
        assert_eq!(ghostty_pressure_stage(0.01), GHOSTTY_MOUSE_PRESSURE_NORMAL);
        assert_eq!(
            ghostty_pressure_stage(GHOSTTY_STYLUS_DEEP_PRESSURE_THRESHOLD - 0.01),
            GHOSTTY_MOUSE_PRESSURE_NORMAL
        );
        assert_eq!(
            ghostty_pressure_stage(GHOSTTY_STYLUS_DEEP_PRESSURE_THRESHOLD),
            GHOSTTY_MOUSE_PRESSURE_DEEP
        );
        assert_eq!(ghostty_pressure_stage(2.0), GHOSTTY_MOUSE_PRESSURE_DEEP);
    }
}
