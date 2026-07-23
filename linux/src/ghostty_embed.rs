#![allow(dead_code)]

use anyhow::{anyhow, Context, Result};
use std::ffi::{c_char, c_int, c_void, CStr, CString};
use std::os::unix::ffi::OsStrExt;
use std::path::{Path, PathBuf};
use std::ptr;

const RTLD_NOW: c_int = 2;
pub const GHOSTTY_EMBEDDING_ABI_VERSION: u32 = 15;
pub const GHOSTTY_PLATFORM_LINUX: c_int = 3;
pub const GHOSTTY_RENDERER_BACKEND_UNKNOWN: c_int = 0;
pub const GHOSTTY_RENDERER_BACKEND_OPENGL: c_int = 1;
pub const GHOSTTY_RENDERER_BACKEND_METAL: c_int = 2;
pub const GHOSTTY_RENDERER_BACKEND_WEBGL: c_int = 3;
pub const GHOSTTY_CLIPBOARD_STANDARD: c_int = 0;
pub const GHOSTTY_CLIPBOARD_SELECTION: c_int = 1;
pub const GHOSTTY_CLIPBOARD_PRIMARY: c_int = 2;
pub const GHOSTTY_CLIPBOARD_REQUEST_PASTE: c_int = 0;
pub const GHOSTTY_CLIPBOARD_REQUEST_OSC_52_READ: c_int = 1;
pub const GHOSTTY_CLIPBOARD_REQUEST_OSC_52_WRITE: c_int = 2;
pub const GHOSTTY_SURFACE_CONTEXT_WINDOW: c_int = 0;
pub const GHOSTTY_SURFACE_CONTEXT_TAB: c_int = 1;
pub const GHOSTTY_SURFACE_CONTEXT_SPLIT: c_int = 2;
pub const GHOSTTY_SURFACE_IO_EXEC: c_int = 0;
pub const GHOSTTY_SURFACE_IO_MANUAL: c_int = 1;
pub const GHOSTTY_SURFACE_MAX_ENV_VARS: usize = 4096;
pub const GHOSTTY_ACTION_RELEASE: c_int = 0;
pub const GHOSTTY_ACTION_PRESS: c_int = 1;
pub const GHOSTTY_ACTION_REPEAT: c_int = 2;
pub const GHOSTTY_INPUT_KEYCODE_NATIVE_MASK: u32 = 0x7fff_ffff;
pub const GHOSTTY_INPUT_KEYCODE_PHYSICAL_KEY_FLAG: u32 = 0x8000_0000;
pub const GHOSTTY_TRIGGER_PHYSICAL: c_int = 0;
pub const GHOSTTY_TRIGGER_UNICODE: c_int = 1;
pub const GHOSTTY_TRIGGER_CATCH_ALL: c_int = 2;
pub const GHOSTTY_MODS_SHIFT: c_int = 1 << 0;
pub const GHOSTTY_MODS_CTRL: c_int = 1 << 1;
pub const GHOSTTY_MODS_ALT: c_int = 1 << 2;
pub const GHOSTTY_MODS_SUPER: c_int = 1 << 3;
pub const GHOSTTY_MODS_CAPS: c_int = 1 << 4;
pub const GHOSTTY_MODS_NUM: c_int = 1 << 5;
pub const GHOSTTY_MODS_SHIFT_RIGHT: c_int = 1 << 6;
pub const GHOSTTY_MODS_CTRL_RIGHT: c_int = 1 << 7;
pub const GHOSTTY_MODS_ALT_RIGHT: c_int = 1 << 8;
pub const GHOSTTY_MODS_SUPER_RIGHT: c_int = 1 << 9;
pub const GHOSTTY_BINDING_FLAGS_CONSUMED: c_int = 1 << 0;
pub const GHOSTTY_BINDING_FLAGS_ALL: c_int = 1 << 1;
pub const GHOSTTY_BINDING_FLAGS_GLOBAL: c_int = 1 << 2;
pub const GHOSTTY_BINDING_FLAGS_PERFORMABLE: c_int = 1 << 3;
pub const GHOSTTY_MOUSE_RELEASE: c_int = 0;
pub const GHOSTTY_MOUSE_PRESS: c_int = 1;
pub const GHOSTTY_MOUSE_BUTTON_UNKNOWN: c_int = 0;
pub const GHOSTTY_MOUSE_BUTTON_LEFT: c_int = 1;
pub const GHOSTTY_MOUSE_BUTTON_RIGHT: c_int = 2;
pub const GHOSTTY_MOUSE_BUTTON_MIDDLE: c_int = 3;
pub const GHOSTTY_MOUSE_BUTTON_FOUR: c_int = 4;
pub const GHOSTTY_MOUSE_BUTTON_FIVE: c_int = 5;
pub const GHOSTTY_MOUSE_BUTTON_SIX: c_int = 6;
pub const GHOSTTY_MOUSE_BUTTON_SEVEN: c_int = 7;
pub const GHOSTTY_MOUSE_BUTTON_EIGHT: c_int = 8;
pub const GHOSTTY_MOUSE_BUTTON_NINE: c_int = 9;
pub const GHOSTTY_MOUSE_BUTTON_TEN: c_int = 10;
pub const GHOSTTY_MOUSE_BUTTON_ELEVEN: c_int = 11;
pub const GHOSTTY_MOUSE_PRESSURE_NONE: c_int = 0;
pub const GHOSTTY_MOUSE_PRESSURE_NORMAL: c_int = 1;
pub const GHOSTTY_MOUSE_PRESSURE_DEEP: c_int = 2;
pub const GHOSTTY_COLOR_SCHEME_LIGHT: c_int = 0;
pub const GHOSTTY_COLOR_SCHEME_DARK: c_int = 1;
pub const GHOSTTY_TARGET_APP: c_int = 0;
pub const GHOSTTY_TARGET_SURFACE: c_int = 1;
pub const GHOSTTY_ACTION_QUIT: c_int = 0;
pub const GHOSTTY_ACTION_NEW_WINDOW: c_int = 1;
pub const GHOSTTY_ACTION_TOGGLE_COMMAND_PALETTE: c_int = 11;
pub const GHOSTTY_ACTION_NEW_TAB: c_int = 2;
pub const GHOSTTY_ACTION_CLOSE_TAB: c_int = 3;
pub const GHOSTTY_ACTION_NEW_SPLIT: c_int = 4;
pub const GHOSTTY_ACTION_CLOSE_ALL_WINDOWS: c_int = 5;
pub const GHOSTTY_ACTION_TOGGLE_MAXIMIZE: c_int = 6;
pub const GHOSTTY_ACTION_TOGGLE_FULLSCREEN: c_int = 7;
pub const GHOSTTY_ACTION_TOGGLE_TAB_OVERVIEW: c_int = 8;
pub const GHOSTTY_ACTION_TOGGLE_WINDOW_DECORATIONS: c_int = 9;
pub const GHOSTTY_ACTION_TOGGLE_QUICK_TERMINAL: c_int = 10;
pub const GHOSTTY_ACTION_TOGGLE_VISIBILITY: c_int = 12;
pub const GHOSTTY_ACTION_TOGGLE_BACKGROUND_OPACITY: c_int = 13;
pub const GHOSTTY_ACTION_MOVE_TAB: c_int = 14;
pub const GHOSTTY_ACTION_GOTO_TAB: c_int = 15;
pub const GHOSTTY_ACTION_GOTO_SPLIT: c_int = 16;
pub const GHOSTTY_ACTION_GOTO_WINDOW: c_int = 17;
pub const GHOSTTY_ACTION_RESIZE_SPLIT: c_int = 18;
pub const GHOSTTY_ACTION_EQUALIZE_SPLITS: c_int = 19;
pub const GHOSTTY_ACTION_TOGGLE_SPLIT_ZOOM: c_int = 20;
pub const GHOSTTY_ACTION_PRESENT_TERMINAL: c_int = 21;
pub const GHOSTTY_ACTION_SIZE_LIMIT: c_int = 22;
pub const GHOSTTY_ACTION_RESET_WINDOW_SIZE: c_int = 23;
pub const GHOSTTY_ACTION_INITIAL_SIZE: c_int = 24;
pub const GHOSTTY_ACTION_CELL_SIZE: c_int = 25;
pub const GHOSTTY_ACTION_SCROLLBAR: c_int = 26;
pub const GHOSTTY_ACTION_RENDER: c_int = 27;
pub const GHOSTTY_ACTION_INSPECTOR: c_int = 28;
pub const GHOSTTY_ACTION_SHOW_GTK_INSPECTOR: c_int = 29;
pub const GHOSTTY_ACTION_RENDER_INSPECTOR: c_int = 30;
pub const GHOSTTY_ACTION_DESKTOP_NOTIFICATION: c_int = 31;
pub const GHOSTTY_ACTION_SET_TITLE: c_int = 32;
pub const GHOSTTY_ACTION_SET_TAB_TITLE: c_int = 33;
pub const GHOSTTY_ACTION_PROMPT_TITLE: c_int = 34;
pub const GHOSTTY_ACTION_PWD: c_int = 35;
pub const GHOSTTY_ACTION_MOUSE_SHAPE: c_int = 36;
pub const GHOSTTY_ACTION_MOUSE_VISIBILITY: c_int = 37;
pub const GHOSTTY_ACTION_MOUSE_OVER_LINK: c_int = 38;
pub const GHOSTTY_ACTION_RENDERER_HEALTH: c_int = 39;
pub const GHOSTTY_ACTION_OPEN_CONFIG: c_int = 40;
pub const GHOSTTY_ACTION_QUIT_TIMER: c_int = 41;
pub const GHOSTTY_ACTION_FLOAT_WINDOW: c_int = 42;
pub const GHOSTTY_ACTION_SECURE_INPUT: c_int = 43;
pub const GHOSTTY_ACTION_KEY_SEQUENCE: c_int = 44;
pub const GHOSTTY_ACTION_KEY_TABLE: c_int = 45;
pub const GHOSTTY_ACTION_COLOR_CHANGE: c_int = 46;
pub const GHOSTTY_ACTION_RELOAD_CONFIG: c_int = 47;
pub const GHOSTTY_ACTION_CONFIG_CHANGE: c_int = 48;
pub const GHOSTTY_ACTION_CLOSE_WINDOW: c_int = 49;
pub const GHOSTTY_ACTION_RING_BELL: c_int = 50;
pub const GHOSTTY_ACTION_SELECTION_CHANGED: c_int = 51;
pub const GHOSTTY_ACTION_UNDO: c_int = 52;
pub const GHOSTTY_ACTION_REDO: c_int = 53;
pub const GHOSTTY_ACTION_CHECK_FOR_UPDATES: c_int = 54;
pub const GHOSTTY_ACTION_OPEN_URL: c_int = 55;
pub const GHOSTTY_ACTION_SHOW_CHILD_EXITED: c_int = 56;
pub const GHOSTTY_ACTION_PROGRESS_REPORT: c_int = 57;
pub const GHOSTTY_ACTION_SHOW_ON_SCREEN_KEYBOARD: c_int = 58;
pub const GHOSTTY_ACTION_COMMAND_FINISHED: c_int = 59;
pub const GHOSTTY_ACTION_START_SEARCH: c_int = 60;
pub const GHOSTTY_ACTION_END_SEARCH: c_int = 61;
pub const GHOSTTY_ACTION_SEARCH_TOTAL: c_int = 62;
pub const GHOSTTY_ACTION_SEARCH_SELECTED: c_int = 63;
pub const GHOSTTY_ACTION_READONLY: c_int = 64;
pub const GHOSTTY_ACTION_COPY_TITLE_TO_CLIPBOARD: c_int = 65;
pub const GHOSTTY_IPC_TARGET_CLASS: c_int = 0;
pub const GHOSTTY_IPC_TARGET_DETECT: c_int = 1;
pub const GHOSTTY_IPC_ACTION_NEW_WINDOW: c_int = 0;
pub const GHOSTTY_IPC_ACTION_TOGGLE_QUICK_TERMINAL: c_int = 1;
pub const GHOSTTY_ACTION_OPEN_URL_KIND_UNKNOWN: c_int = 0;
pub const GHOSTTY_ACTION_OPEN_URL_KIND_TEXT: c_int = 1;
pub const GHOSTTY_ACTION_OPEN_URL_KIND_HTML: c_int = 2;
pub const GHOSTTY_SPLIT_DIRECTION_RIGHT: c_int = 0;
pub const GHOSTTY_SPLIT_DIRECTION_DOWN: c_int = 1;
pub const GHOSTTY_SPLIT_DIRECTION_LEFT: c_int = 2;
pub const GHOSTTY_SPLIT_DIRECTION_UP: c_int = 3;
pub const GHOSTTY_FULLSCREEN_NATIVE: c_int = 0;
pub const GHOSTTY_FULLSCREEN_MACOS_NON_NATIVE: c_int = 1;
pub const GHOSTTY_FULLSCREEN_MACOS_NON_NATIVE_VISIBLE_MENU: c_int = 2;
pub const GHOSTTY_FULLSCREEN_MACOS_NON_NATIVE_PADDED_NOTCH: c_int = 3;
pub const GHOSTTY_CLOSE_TAB_MODE_THIS: c_int = 0;
pub const GHOSTTY_CLOSE_TAB_MODE_OTHER: c_int = 1;
pub const GHOSTTY_CLOSE_TAB_MODE_RIGHT: c_int = 2;
pub const GHOSTTY_GOTO_TAB_PREVIOUS: c_int = -1;
pub const GHOSTTY_GOTO_TAB_NEXT: c_int = -2;
pub const GHOSTTY_GOTO_TAB_LAST: c_int = -3;
pub const GHOSTTY_GOTO_SPLIT_PREVIOUS: c_int = 0;
pub const GHOSTTY_GOTO_SPLIT_NEXT: c_int = 1;
pub const GHOSTTY_GOTO_SPLIT_UP: c_int = 2;
pub const GHOSTTY_GOTO_SPLIT_LEFT: c_int = 3;
pub const GHOSTTY_GOTO_SPLIT_DOWN: c_int = 4;
pub const GHOSTTY_GOTO_SPLIT_RIGHT: c_int = 5;
pub const GHOSTTY_GOTO_WINDOW_PREVIOUS: c_int = 0;
pub const GHOSTTY_GOTO_WINDOW_NEXT: c_int = 1;
pub const GHOSTTY_RESIZE_SPLIT_UP: c_int = 0;
pub const GHOSTTY_RESIZE_SPLIT_DOWN: c_int = 1;
pub const GHOSTTY_RESIZE_SPLIT_LEFT: c_int = 2;
pub const GHOSTTY_RESIZE_SPLIT_RIGHT: c_int = 3;
pub const GHOSTTY_READONLY_OFF: c_int = 0;
pub const GHOSTTY_READONLY_ON: c_int = 1;
pub const GHOSTTY_PROGRESS_STATE_REMOVE: c_int = 0;
pub const GHOSTTY_PROGRESS_STATE_SET: c_int = 1;
pub const GHOSTTY_PROGRESS_STATE_ERROR: c_int = 2;
pub const GHOSTTY_PROGRESS_STATE_INDETERMINATE: c_int = 3;
pub const GHOSTTY_PROGRESS_STATE_PAUSE: c_int = 4;
pub const GHOSTTY_RENDERER_HEALTH_HEALTHY: c_int = 0;
pub const GHOSTTY_RENDERER_HEALTH_UNHEALTHY: c_int = 1;
pub const GHOSTTY_PROMPT_TITLE_SURFACE: c_int = 0;
pub const GHOSTTY_PROMPT_TITLE_TAB: c_int = 1;
pub const GHOSTTY_QUIT_TIMER_START: c_int = 0;
pub const GHOSTTY_QUIT_TIMER_STOP: c_int = 1;
pub const GHOSTTY_FLOAT_WINDOW_ON: c_int = 0;
pub const GHOSTTY_FLOAT_WINDOW_OFF: c_int = 1;
pub const GHOSTTY_FLOAT_WINDOW_TOGGLE: c_int = 2;
pub const GHOSTTY_SECURE_INPUT_ON: c_int = 0;
pub const GHOSTTY_SECURE_INPUT_OFF: c_int = 1;
pub const GHOSTTY_SECURE_INPUT_TOGGLE: c_int = 2;
pub const GHOSTTY_INSPECTOR_TOGGLE: c_int = 0;
pub const GHOSTTY_INSPECTOR_SHOW: c_int = 1;
pub const GHOSTTY_INSPECTOR_HIDE: c_int = 2;
pub const GHOSTTY_KEY_UNIDENTIFIED: c_int = 0;
pub const GHOSTTY_KEY_BACKSPACE: c_int = 53;
pub const GHOSTTY_KEY_ENTER: c_int = 58;
pub const GHOSTTY_KEY_SPACE: c_int = 63;
pub const GHOSTTY_KEY_TAB: c_int = 64;
pub const GHOSTTY_KEY_DELETE: c_int = 68;
pub const GHOSTTY_KEY_END: c_int = 69;
pub const GHOSTTY_KEY_HOME: c_int = 71;
pub const GHOSTTY_KEY_INSERT: c_int = 72;
pub const GHOSTTY_KEY_PAGE_DOWN: c_int = 73;
pub const GHOSTTY_KEY_PAGE_UP: c_int = 74;
pub const GHOSTTY_KEY_ARROW_DOWN: c_int = 75;
pub const GHOSTTY_KEY_ARROW_LEFT: c_int = 76;
pub const GHOSTTY_KEY_ARROW_RIGHT: c_int = 77;
pub const GHOSTTY_KEY_ARROW_UP: c_int = 78;
pub const GHOSTTY_KEY_ESCAPE: c_int = 120;
pub const GHOSTTY_KEY_F1: c_int = 121;
pub const GHOSTTY_KEY_F2: c_int = 122;
pub const GHOSTTY_KEY_F3: c_int = 123;
pub const GHOSTTY_KEY_F4: c_int = 124;
pub const GHOSTTY_KEY_F5: c_int = 125;
pub const GHOSTTY_KEY_F6: c_int = 126;
pub const GHOSTTY_KEY_F7: c_int = 127;
pub const GHOSTTY_KEY_F8: c_int = 128;
pub const GHOSTTY_KEY_F9: c_int = 129;
pub const GHOSTTY_KEY_F10: c_int = 130;
pub const GHOSTTY_KEY_F11: c_int = 131;
pub const GHOSTTY_KEY_F12: c_int = 132;
pub const GHOSTTY_KEY_F13: c_int = 133;
pub const GHOSTTY_KEY_F14: c_int = 134;
pub const GHOSTTY_KEY_F15: c_int = 135;
pub const GHOSTTY_KEY_F16: c_int = 136;
pub const GHOSTTY_KEY_F17: c_int = 137;
pub const GHOSTTY_KEY_F18: c_int = 138;
pub const GHOSTTY_KEY_F19: c_int = 139;
pub const GHOSTTY_KEY_F20: c_int = 140;
pub const GHOSTTY_KEY_F21: c_int = 141;
pub const GHOSTTY_KEY_F22: c_int = 142;
pub const GHOSTTY_KEY_F23: c_int = 143;
pub const GHOSTTY_KEY_F24: c_int = 144;
pub const GHOSTTY_KEY_F25: c_int = 145;
pub const GHOSTTY_KEY_FN: c_int = 146;
pub const GHOSTTY_KEY_FN_LOCK: c_int = 147;
pub const GHOSTTY_KEY_PRINT_SCREEN: c_int = 148;
pub const GHOSTTY_KEY_SCROLL_LOCK: c_int = 149;
pub const GHOSTTY_KEY_PAUSE: c_int = 150;
pub const GHOSTTY_KEY_TABLE_ACTIVATE: c_int = 0;
pub const GHOSTTY_KEY_TABLE_DEACTIVATE: c_int = 1;
pub const GHOSTTY_KEY_TABLE_DEACTIVATE_ALL: c_int = 2;
pub const GHOSTTY_COLOR_KIND_FOREGROUND: c_int = -1;
pub const GHOSTTY_COLOR_KIND_BACKGROUND: c_int = -2;
pub const GHOSTTY_COLOR_KIND_CURSOR: c_int = -3;
pub const GHOSTTY_MOUSE_SHAPE_DEFAULT: c_int = 0;
pub const GHOSTTY_MOUSE_SHAPE_CONTEXT_MENU: c_int = 1;
pub const GHOSTTY_MOUSE_SHAPE_HELP: c_int = 2;
pub const GHOSTTY_MOUSE_SHAPE_POINTER: c_int = 3;
pub const GHOSTTY_MOUSE_SHAPE_PROGRESS: c_int = 4;
pub const GHOSTTY_MOUSE_SHAPE_WAIT: c_int = 5;
pub const GHOSTTY_MOUSE_SHAPE_CELL: c_int = 6;
pub const GHOSTTY_MOUSE_SHAPE_CROSSHAIR: c_int = 7;
pub const GHOSTTY_MOUSE_SHAPE_TEXT: c_int = 8;
pub const GHOSTTY_MOUSE_SHAPE_VERTICAL_TEXT: c_int = 9;
pub const GHOSTTY_MOUSE_SHAPE_ALIAS: c_int = 10;
pub const GHOSTTY_MOUSE_SHAPE_COPY: c_int = 11;
pub const GHOSTTY_MOUSE_SHAPE_MOVE: c_int = 12;
pub const GHOSTTY_MOUSE_SHAPE_NO_DROP: c_int = 13;
pub const GHOSTTY_MOUSE_SHAPE_NOT_ALLOWED: c_int = 14;
pub const GHOSTTY_MOUSE_SHAPE_GRAB: c_int = 15;
pub const GHOSTTY_MOUSE_SHAPE_GRABBING: c_int = 16;
pub const GHOSTTY_MOUSE_SHAPE_ALL_SCROLL: c_int = 17;
pub const GHOSTTY_MOUSE_SHAPE_COL_RESIZE: c_int = 18;
pub const GHOSTTY_MOUSE_SHAPE_ROW_RESIZE: c_int = 19;
pub const GHOSTTY_MOUSE_SHAPE_N_RESIZE: c_int = 20;
pub const GHOSTTY_MOUSE_SHAPE_E_RESIZE: c_int = 21;
pub const GHOSTTY_MOUSE_SHAPE_S_RESIZE: c_int = 22;
pub const GHOSTTY_MOUSE_SHAPE_W_RESIZE: c_int = 23;
pub const GHOSTTY_MOUSE_SHAPE_NE_RESIZE: c_int = 24;
pub const GHOSTTY_MOUSE_SHAPE_NW_RESIZE: c_int = 25;
pub const GHOSTTY_MOUSE_SHAPE_SE_RESIZE: c_int = 26;
pub const GHOSTTY_MOUSE_SHAPE_SW_RESIZE: c_int = 27;
pub const GHOSTTY_MOUSE_SHAPE_EW_RESIZE: c_int = 28;
pub const GHOSTTY_MOUSE_SHAPE_NS_RESIZE: c_int = 29;
pub const GHOSTTY_MOUSE_SHAPE_NESW_RESIZE: c_int = 30;
pub const GHOSTTY_MOUSE_SHAPE_NWSE_RESIZE: c_int = 31;
pub const GHOSTTY_MOUSE_SHAPE_ZOOM_IN: c_int = 32;
pub const GHOSTTY_MOUSE_SHAPE_ZOOM_OUT: c_int = 33;
pub const GHOSTTY_MOUSE_VISIBLE: c_int = 0;
pub const GHOSTTY_MOUSE_HIDDEN: c_int = 1;
pub const GHOSTTY_POINT_VIEWPORT: c_int = 1;
pub const GHOSTTY_POINT_COORD_TOP_LEFT: c_int = 1;
pub const GHOSTTY_POINT_COORD_BOTTOM_RIGHT: c_int = 2;

pub const fn ghostty_physical_keycode(key: c_int) -> u32 {
    GHOSTTY_INPUT_KEYCODE_PHYSICAL_KEY_FLAG | ((key as u32) & GHOSTTY_INPUT_KEYCODE_NATIVE_MASK)
}

pub const REQUIRED_GHOSTTY_EMBED_SYMBOLS: &[&str] = &[
    "ghostty_init",
    "ghostty_string_free",
    "ghostty_embedding_info",
    "ghostty_embedding_info_query",
    "ghostty_config_new",
    "ghostty_config_free",
    "ghostty_config_load_cli_args",
    "ghostty_config_load_file",
    "ghostty_config_load_string",
    "ghostty_config_load_default_files",
    "ghostty_config_load_recursive_files",
    "ghostty_config_finalize",
    "ghostty_config_get",
    "ghostty_config_diagnostics_count",
    "ghostty_config_get_diagnostic",
    "ghostty_config_open_path",
    "ghostty_resources_dir",
    "ghostty_app_new",
    "ghostty_app_free",
    "ghostty_app_tick",
    "ghostty_app_userdata",
    "ghostty_app_set_focus",
    "ghostty_app_key",
    "ghostty_app_keyboard_changed",
    "ghostty_app_open_config",
    "ghostty_app_reload_config",
    "ghostty_app_update_config",
    "ghostty_app_needs_confirm_quit",
    "ghostty_app_has_global_keybinds",
    "ghostty_app_must_draw_from_app_thread",
    "ghostty_app_set_color_scheme",
    "ghostty_surface_config_new",
    "ghostty_surface_new",
    "ghostty_surface_free",
    "ghostty_surface_userdata",
    "ghostty_surface_app",
    "ghostty_surface_inherited_config",
    "ghostty_surface_inherited_config_free",
    "ghostty_surface_update_config",
    "ghostty_surface_refresh",
    "ghostty_surface_draw",
    "ghostty_surface_display_realized",
    "ghostty_surface_display_unrealized",
    "ghostty_surface_set_renderer_realized",
    "ghostty_surface_set_content_scale",
    "ghostty_surface_set_focus",
    "ghostty_surface_set_visible",
    "ghostty_surface_set_occlusion",
    "ghostty_surface_set_size",
    "ghostty_surface_set_color_scheme",
    "ghostty_surface_needs_confirm_quit",
    "ghostty_surface_size",
    "ghostty_surface_process_exited",
    "ghostty_surface_foreground_pid",
    "ghostty_surface_tty_name",
    "ghostty_surface_title",
    "ghostty_surface_pwd",
    "ghostty_surface_key_translation_mods",
    "ghostty_surface_key",
    "ghostty_surface_key_is_binding",
    "ghostty_surface_text",
    "ghostty_surface_process_output",
    "ghostty_surface_preedit",
    "ghostty_surface_mouse_captured",
    "ghostty_surface_mouse_button",
    "ghostty_surface_mouse_pos",
    "ghostty_surface_mouse_scroll",
    "ghostty_surface_mouse_pressure",
    "ghostty_surface_ime_point",
    "ghostty_surface_request_close",
    "ghostty_surface_split",
    "ghostty_surface_split_focus",
    "ghostty_surface_split_resize",
    "ghostty_surface_split_equalize",
    "ghostty_surface_split_toggle_zoom",
    "ghostty_surface_binding_action",
    "ghostty_surface_has_selection",
    "ghostty_surface_select_cursor_cell",
    "ghostty_surface_select_viewport_rows",
    "ghostty_surface_clear_selection",
    "ghostty_surface_read_selection",
    "ghostty_surface_complete_clipboard_request",
    "ghostty_surface_read_text",
    "ghostty_surface_read_scrollback",
    "ghostty_surface_free_text",
    "ghostty_surface_inspector",
    "ghostty_inspector_free",
    "ghostty_inspector_set_focus",
    "ghostty_inspector_set_content_scale",
    "ghostty_inspector_set_size",
    "ghostty_inspector_mouse_button",
    "ghostty_inspector_mouse_pos",
    "ghostty_inspector_mouse_scroll",
    "ghostty_inspector_key",
    "ghostty_inspector_text",
    "ghostty_inspector_opengl_init",
    "ghostty_inspector_opengl_render",
    "ghostty_inspector_opengl_shutdown",
];

#[link(name = "dl")]
extern "C" {
    fn dlopen(filename: *const c_char, flags: c_int) -> *mut c_void;
    fn dlsym(handle: *mut c_void, symbol: *const c_char) -> *mut c_void;
    fn dlclose(handle: *mut c_void) -> c_int;
    fn dlerror() -> *const c_char;
}

pub type GhosttyApp = *mut c_void;
pub type GhosttyConfig = *mut c_void;
pub type GhosttySurface = *mut c_void;
pub type GhosttyInspector = *mut c_void;
pub type GhosttyLinuxMakeCurrent = unsafe extern "C" fn(*mut c_void) -> bool;
pub type GhosttyLinuxGetProcAddress =
    unsafe extern "C" fn(*mut c_void, *const c_char) -> *mut c_void;
pub type GhosttyLinuxDoneCurrent = unsafe extern "C" fn(*mut c_void);
pub type GhosttyIoWriteCb = unsafe extern "C" fn(*mut c_void, *const c_char, usize);

type GhosttyInit = unsafe extern "C" fn(usize, *const *const c_char) -> c_int;
type GhosttyEmbeddingInfoFn = unsafe extern "C" fn() -> GhosttyEmbeddingInfo;
type GhosttyEmbeddingInfoQuery = unsafe extern "C" fn(*mut GhosttyEmbeddingInfo, usize) -> bool;
pub type GhosttyConfigNew = unsafe extern "C" fn() -> GhosttyConfig;
pub type GhosttyConfigFree = unsafe extern "C" fn(GhosttyConfig);
type GhosttyConfigLoadCliArgs = unsafe extern "C" fn(GhosttyConfig) -> bool;
type GhosttyConfigLoadFile = unsafe extern "C" fn(GhosttyConfig, *const c_char) -> bool;
type GhosttyConfigLoadString = unsafe extern "C" fn(GhosttyConfig, *const c_char, usize) -> bool;
pub type GhosttyConfigLoadDefaultFiles = unsafe extern "C" fn(GhosttyConfig) -> bool;
pub type GhosttyConfigLoadRecursiveFiles = unsafe extern "C" fn(GhosttyConfig) -> bool;
pub type GhosttyConfigFinalize = unsafe extern "C" fn(GhosttyConfig) -> bool;
pub type GhosttyConfigGet =
    unsafe extern "C" fn(GhosttyConfig, *mut c_void, *const c_char, usize) -> bool;
pub type GhosttyConfigDiagnosticsCount = unsafe extern "C" fn(GhosttyConfig) -> u32;
pub type GhosttyConfigGetDiagnostic = unsafe extern "C" fn(GhosttyConfig, u32) -> GhosttyDiagnostic;
pub type GhosttyConfigOpenPath = unsafe extern "C" fn() -> GhosttyString;
type GhosttyResourcesDir = unsafe extern "C" fn() -> GhosttyString;
type GhosttyAppNew = unsafe extern "C" fn(*const GhosttyRuntimeConfig, GhosttyConfig) -> GhosttyApp;
type GhosttyAppFree = unsafe extern "C" fn(GhosttyApp);
pub type GhosttyAppTick = unsafe extern "C" fn(GhosttyApp) -> bool;
type GhosttyAppUserdata = unsafe extern "C" fn(GhosttyApp) -> *mut c_void;
pub type GhosttyAppSetFocus = unsafe extern "C" fn(GhosttyApp, bool) -> bool;
type GhosttyAppKey = unsafe extern "C" fn(GhosttyApp, GhosttyInputKey) -> bool;
pub type GhosttyAppKeyboardChanged = unsafe extern "C" fn(GhosttyApp) -> bool;
type GhosttyAppOpenConfig = unsafe extern "C" fn(GhosttyApp) -> bool;
type GhosttyAppReloadConfig = unsafe extern "C" fn(GhosttyApp, bool) -> bool;
pub type GhosttyAppUpdateConfig = unsafe extern "C" fn(GhosttyApp, GhosttyConfig) -> bool;
type GhosttyAppNeedsConfirmQuit = unsafe extern "C" fn(GhosttyApp) -> bool;
type GhosttyAppHasGlobalKeybinds = unsafe extern "C" fn(GhosttyApp) -> bool;
type GhosttyAppMustDrawFromAppThread = unsafe extern "C" fn(GhosttyApp) -> bool;
type GhosttyAppSetColorScheme = unsafe extern "C" fn(GhosttyApp, c_int) -> bool;
type GhosttySurfaceConfigNew = unsafe extern "C" fn() -> GhosttySurfaceConfig;
type GhosttySurfaceNew =
    unsafe extern "C" fn(GhosttyApp, *const GhosttySurfaceConfig) -> GhosttySurface;
type GhosttySurfaceFree = unsafe extern "C" fn(GhosttySurface);
type GhosttySurfaceUserdata = unsafe extern "C" fn(GhosttySurface) -> *mut c_void;
type GhosttySurfaceApp = unsafe extern "C" fn(GhosttySurface) -> GhosttyApp;
pub type GhosttySurfaceInheritedConfig =
    unsafe extern "C" fn(GhosttySurface, c_int) -> GhosttySurfaceConfig;
pub type GhosttySurfaceInheritedConfigFree =
    unsafe extern "C" fn(GhosttySurface, *mut GhosttySurfaceConfig);
pub type GhosttySurfaceUpdateConfig = unsafe extern "C" fn(GhosttySurface, GhosttyConfig) -> bool;
type GhosttySurfaceRefresh = unsafe extern "C" fn(GhosttySurface) -> bool;
type GhosttySurfaceDraw = unsafe extern "C" fn(GhosttySurface) -> bool;
type GhosttySurfaceDisplayRealized = unsafe extern "C" fn(GhosttySurface) -> bool;
type GhosttySurfaceDisplayUnrealized = unsafe extern "C" fn(GhosttySurface) -> bool;
type GhosttySurfaceSetRendererRealized = unsafe extern "C" fn(GhosttySurface, bool) -> bool;
type GhosttySurfaceSetContentScale = unsafe extern "C" fn(GhosttySurface, f64, f64) -> bool;
type GhosttySurfaceSetFocus = unsafe extern "C" fn(GhosttySurface, bool) -> bool;
type GhosttySurfaceSetVisible = unsafe extern "C" fn(GhosttySurface, bool) -> bool;
type GhosttySurfaceSetOcclusion = unsafe extern "C" fn(GhosttySurface, bool) -> bool;
type GhosttySurfaceSetSize = unsafe extern "C" fn(GhosttySurface, u32, u32) -> bool;
type GhosttySurfaceSetColorScheme = unsafe extern "C" fn(GhosttySurface, c_int) -> bool;
pub type GhosttySurfaceNeedsConfirmQuit = unsafe extern "C" fn(GhosttySurface) -> bool;
type GhosttySurfaceSize = unsafe extern "C" fn(GhosttySurface) -> GhosttySurfaceSizeResult;
type GhosttySurfaceProcessExited = unsafe extern "C" fn(GhosttySurface) -> bool;
type GhosttySurfaceForegroundPid = unsafe extern "C" fn(GhosttySurface) -> u64;
type GhosttySurfaceTtyName = unsafe extern "C" fn(GhosttySurface) -> GhosttyString;
type GhosttySurfaceTitle = unsafe extern "C" fn(GhosttySurface) -> GhosttyString;
type GhosttySurfacePwd = unsafe extern "C" fn(GhosttySurface) -> GhosttyString;
type GhosttySurfaceKeyTranslationMods = unsafe extern "C" fn(GhosttySurface, c_int) -> c_int;
type GhosttySurfaceKey = unsafe extern "C" fn(GhosttySurface, GhosttyInputKey) -> bool;
type GhosttySurfaceKeyIsBinding =
    unsafe extern "C" fn(GhosttySurface, GhosttyInputKey, *mut c_int) -> bool;
type GhosttySurfaceText = unsafe extern "C" fn(GhosttySurface, *const c_char, usize) -> bool;
type GhosttySurfaceProcessOutput =
    unsafe extern "C" fn(GhosttySurface, *const c_char, usize) -> bool;
type GhosttySurfacePreedit = unsafe extern "C" fn(GhosttySurface, *const c_char, usize) -> bool;
type GhosttySurfaceMouseCaptured = unsafe extern "C" fn(GhosttySurface) -> bool;
type GhosttySurfaceMouseButton = unsafe extern "C" fn(GhosttySurface, c_int, c_int, c_int) -> bool;
type GhosttySurfaceMousePos = unsafe extern "C" fn(GhosttySurface, f64, f64, c_int) -> bool;
type GhosttySurfaceMouseScroll = unsafe extern "C" fn(GhosttySurface, f64, f64, c_int) -> bool;
type GhosttySurfaceMousePressure = unsafe extern "C" fn(GhosttySurface, c_int, f64) -> bool;
type GhosttySurfaceImePoint =
    unsafe extern "C" fn(GhosttySurface, *mut f64, *mut f64, *mut f64, *mut f64) -> bool;
type GhosttySurfaceRequestClose = unsafe extern "C" fn(GhosttySurface) -> bool;
type GhosttySurfaceSplit = unsafe extern "C" fn(GhosttySurface, c_int) -> bool;
type GhosttySurfaceSplitFocus = unsafe extern "C" fn(GhosttySurface, c_int) -> bool;
type GhosttySurfaceSplitResize = unsafe extern "C" fn(GhosttySurface, c_int, u16) -> bool;
type GhosttySurfaceSplitEqualize = unsafe extern "C" fn(GhosttySurface) -> bool;
type GhosttySurfaceSplitToggleZoom = unsafe extern "C" fn(GhosttySurface) -> bool;
type GhosttySurfaceBindingAction =
    unsafe extern "C" fn(GhosttySurface, *const c_char, usize) -> bool;
type GhosttySurfaceHasSelection = unsafe extern "C" fn(GhosttySurface) -> bool;
type GhosttySurfaceSelectCursorCell = unsafe extern "C" fn(GhosttySurface) -> bool;
type GhosttySurfaceSelectViewportRows = unsafe extern "C" fn(GhosttySurface, u32, u32) -> bool;
type GhosttySurfaceClearSelection = unsafe extern "C" fn(GhosttySurface) -> bool;
type GhosttySurfaceReadSelection = unsafe extern "C" fn(GhosttySurface, *mut GhosttyText) -> bool;
type GhosttySurfaceReadText =
    unsafe extern "C" fn(GhosttySurface, GhosttySelection, *mut GhosttyText) -> bool;
type GhosttySurfaceReadScrollback =
    unsafe extern "C" fn(GhosttySurface, usize, *mut GhosttyText) -> bool;
type GhosttySurfaceFreeText = unsafe extern "C" fn(GhosttySurface, *mut GhosttyText);
pub type GhosttySurfaceCompleteClipboardRequest =
    unsafe extern "C" fn(GhosttySurface, *const c_char, *mut c_void, bool) -> bool;
type GhosttySurfaceInspector = unsafe extern "C" fn(GhosttySurface) -> GhosttyInspector;
type GhosttyInspectorFree = unsafe extern "C" fn(GhosttyInspector);
type GhosttyInspectorSetFocus = unsafe extern "C" fn(GhosttyInspector, bool) -> bool;
type GhosttyInspectorSetContentScale = unsafe extern "C" fn(GhosttyInspector, f64, f64) -> bool;
type GhosttyInspectorSetSize = unsafe extern "C" fn(GhosttyInspector, u32, u32) -> bool;
type GhosttyInspectorMouseButton =
    unsafe extern "C" fn(GhosttyInspector, c_int, c_int, c_int) -> bool;
type GhosttyInspectorMousePos = unsafe extern "C" fn(GhosttyInspector, f64, f64) -> bool;
type GhosttyInspectorMouseScroll = unsafe extern "C" fn(GhosttyInspector, f64, f64, c_int) -> bool;
type GhosttyInspectorKey = unsafe extern "C" fn(GhosttyInspector, c_int, c_int, c_int) -> bool;
type GhosttyInspectorText = unsafe extern "C" fn(GhosttyInspector, *const c_char) -> bool;
type GhosttyInspectorOpenGLInit = unsafe extern "C" fn(GhosttyInspector, *const c_char) -> bool;
type GhosttyInspectorOpenGLRender = unsafe extern "C" fn(GhosttyInspector) -> bool;
type GhosttyInspectorOpenGLShutdown = unsafe extern "C" fn(GhosttyInspector) -> bool;
pub type GhosttyStringFree = unsafe extern "C" fn(GhosttyString);

#[repr(C)]
#[derive(Clone, Copy)]
pub struct GhosttyRuntimeConfig {
    userdata: *mut c_void,
    supports_selection_clipboard: bool,
    wakeup_cb: GhosttyRuntimeWakeupCb,
    action_cb: GhosttyRuntimeActionCb,
    read_clipboard_cb: GhosttyRuntimeReadClipboardCb,
    confirm_read_clipboard_cb: GhosttyRuntimeConfirmReadClipboardCb,
    write_clipboard_cb: GhosttyRuntimeWriteClipboardCb,
    close_surface_cb: Option<GhosttyRuntimeCloseSurfaceCb>,
    redraw_surface_cb: Option<GhosttyRuntimeRedrawSurfaceCb>,
}

pub type GhosttyRuntimeWakeupCb = unsafe extern "C" fn(*mut c_void);
pub type GhosttyRuntimeActionCb =
    unsafe extern "C" fn(GhosttyApp, GhosttyTarget, GhosttyAction) -> bool;
pub type GhosttyRuntimeReadClipboardCb =
    unsafe extern "C" fn(*mut c_void, c_int, *mut c_void) -> bool;
pub type GhosttyRuntimeConfirmReadClipboardCb =
    unsafe extern "C" fn(*mut c_void, *const c_char, *mut c_void, c_int);
pub type GhosttyRuntimeWriteClipboardCb =
    unsafe extern "C" fn(*mut c_void, c_int, *const GhosttyClipboardContent, usize, bool);
pub type GhosttyRuntimeCloseSurfaceCb = unsafe extern "C" fn(*mut c_void, bool);
pub type GhosttyRuntimeRedrawSurfaceCb = unsafe extern "C" fn(*mut c_void);

#[derive(Clone, Copy)]
pub struct GhosttyRuntimeCallbacks {
    pub userdata: *mut c_void,
    pub wakeup: GhosttyRuntimeWakeupCb,
    pub action: GhosttyRuntimeActionCb,
    pub read_clipboard: GhosttyRuntimeReadClipboardCb,
    pub confirm_read_clipboard: GhosttyRuntimeConfirmReadClipboardCb,
    pub write_clipboard: GhosttyRuntimeWriteClipboardCb,
    pub close_surface: Option<GhosttyRuntimeCloseSurfaceCb>,
    pub redraw_surface: GhosttyRuntimeRedrawSurfaceCb,
    pub supports_selection_clipboard: bool,
}

impl GhosttyRuntimeConfig {
    pub fn with_redraw_surface(redraw_surface_cb: GhosttyRuntimeRedrawSurfaceCb) -> Self {
        let mut config = default_runtime_config();
        config.redraw_surface_cb = Some(redraw_surface_cb);
        config
    }

    pub fn with_callbacks(callbacks: GhosttyRuntimeCallbacks) -> Self {
        let mut config = default_runtime_config();
        config.userdata = callbacks.userdata;
        config.supports_selection_clipboard = callbacks.supports_selection_clipboard;
        config.wakeup_cb = callbacks.wakeup;
        config.action_cb = callbacks.action;
        config.read_clipboard_cb = callbacks.read_clipboard;
        config.confirm_read_clipboard_cb = callbacks.confirm_read_clipboard;
        config.write_clipboard_cb = callbacks.write_clipboard;
        config.close_surface_cb = callbacks.close_surface;
        config.redraw_surface_cb = Some(callbacks.redraw_surface);
        config
    }
}

#[repr(C)]
#[derive(Clone, Copy)]
pub union GhosttyTargetValue {
    pub surface: GhosttySurface,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct GhosttyTarget {
    pub tag: c_int,
    pub target: GhosttyTargetValue,
}

impl GhosttyTarget {
    pub fn surface(&self) -> Option<GhosttySurface> {
        if self.tag == GHOSTTY_TARGET_SURFACE {
            Some(unsafe { self.target.surface })
        } else {
            None
        }
    }
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct GhosttyActionDesktopNotification {
    pub title: *const c_char,
    pub body: *const c_char,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct GhosttyActionSetTitle {
    pub title: *const c_char,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct GhosttyActionPwd {
    pub pwd: *const c_char,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct GhosttyActionMouseOverLink {
    pub url: *const c_char,
    pub len: usize,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct GhosttyActionOpenUrl {
    pub kind: c_int,
    pub url: *const c_char,
    pub len: usize,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct GhosttyActionProgressReport {
    pub state: c_int,
    pub progress: i8,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct GhosttyActionChildExited {
    pub exit_code: u32,
    pub runtime_ms: u64,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct GhosttyActionCommandFinished {
    pub exit_code: i16,
    pub duration: u64,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct GhosttyActionStartSearch {
    pub needle: *const c_char,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct GhosttyActionSearchTotal {
    pub total: isize,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct GhosttyActionSearchSelected {
    pub selected: isize,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct GhosttyActionScrollbar {
    pub total: u64,
    pub offset: u64,
    pub len: u64,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub union GhosttyInputTriggerKey {
    pub physical: c_int,
    pub unicode: u32,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct GhosttyInputTrigger {
    pub tag: c_int,
    pub key: GhosttyInputTriggerKey,
    pub mods: c_int,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct GhosttyActionKeySequence {
    pub active: bool,
    pub trigger: GhosttyInputTrigger,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct GhosttyActionKeyTableActivate {
    pub name: *const c_char,
    pub len: usize,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub union GhosttyActionKeyTableValue {
    pub activate: GhosttyActionKeyTableActivate,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct GhosttyActionKeyTable {
    pub tag: c_int,
    pub value: GhosttyActionKeyTableValue,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct GhosttyActionMoveTab {
    pub amount: isize,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct GhosttyActionSizeLimit {
    pub min_width: u32,
    pub min_height: u32,
    pub max_width: u32,
    pub max_height: u32,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct GhosttyActionInitialSize {
    pub width: u32,
    pub height: u32,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct GhosttyActionCellSize {
    pub width: u32,
    pub height: u32,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct GhosttyActionColorChange {
    pub kind: c_int,
    pub r: u8,
    pub g: u8,
    pub b: u8,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct GhosttyActionConfigChange {
    pub config: GhosttyConfig,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct GhosttyActionReloadConfig {
    pub soft: bool,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct GhosttyActionResizeSplit {
    pub amount: u16,
    pub direction: c_int,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub union GhosttyActionPayload {
    pub new_split: c_int,
    pub toggle_fullscreen: c_int,
    pub move_tab: GhosttyActionMoveTab,
    pub goto_tab: c_int,
    pub goto_window: c_int,
    pub close_tab_mode: c_int,
    pub goto_split: c_int,
    pub resize_split: GhosttyActionResizeSplit,
    pub size_limit: GhosttyActionSizeLimit,
    pub initial_size: GhosttyActionInitialSize,
    pub cell_size: GhosttyActionCellSize,
    pub desktop_notification: GhosttyActionDesktopNotification,
    pub set_title: GhosttyActionSetTitle,
    pub set_tab_title: GhosttyActionSetTitle,
    pub prompt_title: c_int,
    pub pwd: GhosttyActionPwd,
    pub mouse_shape: c_int,
    pub mouse_visibility: c_int,
    pub mouse_over_link: GhosttyActionMouseOverLink,
    pub renderer_health: c_int,
    pub inspector: c_int,
    pub quit_timer: c_int,
    pub float_window: c_int,
    pub secure_input: c_int,
    pub key_sequence: GhosttyActionKeySequence,
    pub key_table: GhosttyActionKeyTable,
    pub color_change: GhosttyActionColorChange,
    pub config_change: GhosttyActionConfigChange,
    pub open_url: GhosttyActionOpenUrl,
    pub child_exited: GhosttyActionChildExited,
    pub progress_report: GhosttyActionProgressReport,
    pub command_finished: GhosttyActionCommandFinished,
    pub start_search: GhosttyActionStartSearch,
    pub search_total: GhosttyActionSearchTotal,
    pub search_selected: GhosttyActionSearchSelected,
    pub scrollbar: GhosttyActionScrollbar,
    pub reload_config: GhosttyActionReloadConfig,
    pub readonly: c_int,
    pub padding: [usize; 3],
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct GhosttyAction {
    pub tag: c_int,
    pub action: GhosttyActionPayload,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub union GhosttyIpcTargetPayload {
    pub klass: *const c_char,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct GhosttyIpcTarget {
    pub tag: c_int,
    pub target: GhosttyIpcTargetPayload,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct GhosttyIpcActionNewWindow {
    pub arguments: *const *const c_char,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub union GhosttyIpcActionPayload {
    pub new_window: GhosttyIpcActionNewWindow,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct GhosttyIpcAction {
    pub tag: c_int,
    pub action: GhosttyIpcActionPayload,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct GhosttyClipboardContent {
    pub mime: *const c_char,
    pub data: *const c_char,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct GhosttyDiagnostic {
    pub message: *const c_char,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct GhosttyPlatformLinux {
    userdata: *mut c_void,
    make_current: GhosttyLinuxMakeCurrent,
    get_proc_address: GhosttyLinuxGetProcAddress,
    done_current: Option<GhosttyLinuxDoneCurrent>,
}

impl GhosttyPlatformLinux {
    pub fn new(
        userdata: *mut c_void,
        make_current: GhosttyLinuxMakeCurrent,
        get_proc_address: GhosttyLinuxGetProcAddress,
        done_current: Option<GhosttyLinuxDoneCurrent>,
    ) -> Self {
        Self {
            userdata,
            make_current,
            get_proc_address,
            done_current,
        }
    }
}

#[repr(C)]
#[derive(Clone, Copy)]
pub union GhosttyPlatform {
    linux_gl: GhosttyPlatformLinux,
    padding: [usize; 4],
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct GhosttyEnvVar {
    pub key: *const c_char,
    pub value: *const c_char,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct GhosttySurfaceConfig {
    platform_tag: c_int,
    platform: GhosttyPlatform,
    userdata: *mut c_void,
    scale_factor: f64,
    font_size: f32,
    working_directory: *const c_char,
    command: *const c_char,
    env_vars: *const GhosttyEnvVar,
    env_var_count: usize,
    initial_input: *const c_char,
    wait_after_command: bool,
    context: c_int,
    io_mode: c_int,
    io_write_cb: Option<GhosttyIoWriteCb>,
    io_write_userdata: *mut c_void,
    initial_output: *const c_char,
    initial_output_len: usize,
    initial_width_px: u32,
    initial_height_px: u32,
}

impl GhosttySurfaceConfig {
    pub fn configure_linux_platform(&mut self, platform: GhosttyPlatformLinux) {
        self.platform_tag = GHOSTTY_PLATFORM_LINUX;
        self.platform = GhosttyPlatform { linux_gl: platform };
    }

    pub fn set_userdata(&mut self, userdata: *mut c_void) {
        self.userdata = userdata;
    }

    pub fn set_scale_factor(&mut self, scale_factor: f64) {
        self.scale_factor = scale_factor;
    }

    pub fn set_font_size(&mut self, font_size: f32) {
        self.font_size = font_size;
    }

    pub fn set_context(&mut self, context: c_int) {
        self.context = context;
    }

    pub fn set_manual_io(&mut self, write_cb: GhosttyIoWriteCb, userdata: *mut c_void) {
        self.io_mode = GHOSTTY_SURFACE_IO_MANUAL;
        self.io_write_cb = Some(write_cb);
        self.io_write_userdata = userdata;
    }

    pub fn set_working_directory(&mut self, working_directory: *const c_char) {
        self.working_directory = working_directory;
    }

    pub fn set_command(&mut self, command: *const c_char) {
        self.command = command;
    }

    pub fn set_initial_input(&mut self, initial_input: *const c_char) {
        self.initial_input = initial_input;
    }

    pub fn set_initial_output(&mut self, initial_output: &[u8]) {
        self.initial_output = initial_output.as_ptr() as *const c_char;
        self.initial_output_len = initial_output.len();
    }

    pub fn set_initial_size(&mut self, width: u32, height: u32) {
        self.initial_width_px = width;
        self.initial_height_px = height;
    }

    pub fn set_wait_after_command(&mut self, wait_after_command: bool) {
        self.wait_after_command = wait_after_command;
    }

    pub fn set_env_vars(&mut self, env_vars: *const GhosttyEnvVar, env_var_count: usize) {
        self.env_vars = env_vars;
        self.env_var_count = env_var_count;
    }

    pub fn font_size(&self) -> Option<f32> {
        (self.font_size.is_finite() && self.font_size > 0.0)
            .then_some(self.font_size.clamp(1.0, 255.0))
    }

    pub fn working_directory(&self) -> Option<String> {
        if self.working_directory.is_null() {
            return None;
        }
        let text = unsafe { CStr::from_ptr(self.working_directory) }
            .to_string_lossy()
            .into_owned();
        (!text.trim().is_empty()).then_some(text)
    }
}

#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct GhosttySurfaceSizeResult {
    pub columns: u16,
    pub rows: u16,
    pub width_px: u32,
    pub height_px: u32,
    pub cell_width_px: u32,
    pub cell_height_px: u32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GhosttyImePoint {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct GhosttyInputKey {
    pub action: c_int,
    pub mods: c_int,
    pub consumed_mods: c_int,
    pub keycode: u32,
    pub text: *const c_char,
    pub unshifted_codepoint: u32,
    pub composing: bool,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct GhosttyPoint {
    tag: c_int,
    coord: c_int,
    x: u32,
    y: u32,
}

impl GhosttyPoint {
    fn viewport(coord: c_int) -> Self {
        Self {
            tag: GHOSTTY_POINT_VIEWPORT,
            coord,
            x: 0,
            y: 0,
        }
    }
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct GhosttySelection {
    top_left: GhosttyPoint,
    bottom_right: GhosttyPoint,
    rectangle: bool,
}

impl GhosttySelection {
    fn viewport() -> Self {
        Self {
            top_left: GhosttyPoint::viewport(GHOSTTY_POINT_COORD_TOP_LEFT),
            bottom_right: GhosttyPoint::viewport(GHOSTTY_POINT_COORD_BOTTOM_RIGHT),
            rectangle: false,
        }
    }
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct GhosttyText {
    tl_px_x: f64,
    tl_px_y: f64,
    offset_start: u32,
    offset_len: u32,
    text: *const c_char,
    text_len: usize,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct GhosttyString {
    ptr: *const c_char,
    len: usize,
    sentinel: bool,
}

#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GhosttyEmbeddingInfo {
    pub abi_version: u32,
    pub platform: c_int,
    pub renderer_backend: c_int,
    pub surface_max_env_vars: usize,
    pub supports_linux_platform: bool,
    pub must_draw_from_app_thread: bool,
    pub runtime_config_size: usize,
    pub surface_config_size: usize,
    pub platform_linux_size: usize,
    pub input_key_size: usize,
    pub target_size: usize,
    pub action_size: usize,
    pub text_size: usize,
    pub selection_size: usize,
    pub string_size: usize,
    pub surface_size_size: usize,
    pub diagnostic_size: usize,
    pub env_var_size: usize,
    pub clipboard_content_size: usize,
    pub input_trigger_size: usize,
    pub ipc_target_size: usize,
    pub ipc_action_size: usize,
    pub runtime_config_align: usize,
    pub surface_config_align: usize,
    pub platform_linux_align: usize,
    pub input_key_align: usize,
    pub target_align: usize,
    pub action_align: usize,
    pub text_align: usize,
    pub selection_align: usize,
    pub string_align: usize,
    pub surface_size_align: usize,
    pub diagnostic_align: usize,
    pub env_var_align: usize,
    pub clipboard_content_align: usize,
    pub input_trigger_align: usize,
    pub ipc_target_align: usize,
    pub ipc_action_align: usize,
    pub layout_fingerprint: u64,
    pub constants_fingerprint: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GhosttyEmbeddingInfoReport {
    pub direct: GhosttyEmbeddingInfo,
    pub query: GhosttyEmbeddingInfo,
}

impl GhosttyEmbeddingInfoReport {
    pub fn matches(&self) -> bool {
        self.direct == self.query
    }

    pub fn info(&self) -> GhosttyEmbeddingInfo {
        self.query
    }
}

pub fn embedding_layout_sizes_match(info: &GhosttyEmbeddingInfo) -> bool {
    info.runtime_config_size == std::mem::size_of::<GhosttyRuntimeConfig>()
        && info.surface_config_size == std::mem::size_of::<GhosttySurfaceConfig>()
        && info.platform_linux_size == std::mem::size_of::<GhosttyPlatformLinux>()
        && info.input_key_size == std::mem::size_of::<GhosttyInputKey>()
        && info.target_size == std::mem::size_of::<GhosttyTarget>()
        && info.action_size == std::mem::size_of::<GhosttyAction>()
        && info.text_size == std::mem::size_of::<GhosttyText>()
        && info.selection_size == std::mem::size_of::<GhosttySelection>()
        && info.string_size == std::mem::size_of::<GhosttyString>()
        && info.surface_size_size == std::mem::size_of::<GhosttySurfaceSizeResult>()
        && info.diagnostic_size == std::mem::size_of::<GhosttyDiagnostic>()
        && info.env_var_size == std::mem::size_of::<GhosttyEnvVar>()
        && info.clipboard_content_size == std::mem::size_of::<GhosttyClipboardContent>()
        && info.input_trigger_size == std::mem::size_of::<GhosttyInputTrigger>()
        && info.ipc_target_size == std::mem::size_of::<GhosttyIpcTarget>()
        && info.ipc_action_size == std::mem::size_of::<GhosttyIpcAction>()
}

pub fn embedding_layout_alignments_match(info: &GhosttyEmbeddingInfo) -> bool {
    info.runtime_config_align == std::mem::align_of::<GhosttyRuntimeConfig>()
        && info.surface_config_align == std::mem::align_of::<GhosttySurfaceConfig>()
        && info.platform_linux_align == std::mem::align_of::<GhosttyPlatformLinux>()
        && info.input_key_align == std::mem::align_of::<GhosttyInputKey>()
        && info.target_align == std::mem::align_of::<GhosttyTarget>()
        && info.action_align == std::mem::align_of::<GhosttyAction>()
        && info.text_align == std::mem::align_of::<GhosttyText>()
        && info.selection_align == std::mem::align_of::<GhosttySelection>()
        && info.string_align == std::mem::align_of::<GhosttyString>()
        && info.surface_size_align == std::mem::align_of::<GhosttySurfaceSizeResult>()
        && info.diagnostic_align == std::mem::align_of::<GhosttyDiagnostic>()
        && info.env_var_align == std::mem::align_of::<GhosttyEnvVar>()
        && info.clipboard_content_align == std::mem::align_of::<GhosttyClipboardContent>()
        && info.input_trigger_align == std::mem::align_of::<GhosttyInputTrigger>()
        && info.ipc_target_align == std::mem::align_of::<GhosttyIpcTarget>()
        && info.ipc_action_align == std::mem::align_of::<GhosttyIpcAction>()
}

const LAYOUT_HASH_OFFSET: u64 = 0xcbf29ce484222325;
const LAYOUT_HASH_PRIME: u64 = 0x100000001b3;

fn layout_hash_bytes(mut hash: u64, bytes: &[u8]) -> u64 {
    for byte in bytes {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(LAYOUT_HASH_PRIME);
    }
    hash
}

fn layout_hash_usize(mut hash: u64, value: usize) -> u64 {
    let mut remaining = value as u64;
    for _ in 0..8 {
        hash ^= remaining & 0xff;
        hash = hash.wrapping_mul(LAYOUT_HASH_PRIME);
        remaining >>= 8;
    }
    hash
}

fn layout_hash_i64(mut hash: u64, value: i64) -> u64 {
    let mut remaining = value as u64;
    for _ in 0..8 {
        hash ^= remaining & 0xff;
        hash = hash.wrapping_mul(LAYOUT_HASH_PRIME);
        remaining >>= 8;
    }
    hash
}

fn layout_hash_type<T>(mut hash: u64, name: &str, fields: &[(&str, usize)]) -> u64 {
    hash = layout_hash_bytes(hash, name.as_bytes());
    hash = layout_hash_usize(hash, std::mem::size_of::<T>());
    hash = layout_hash_usize(hash, std::mem::align_of::<T>());
    for (field, offset) in fields {
        hash = layout_hash_bytes(hash, field.as_bytes());
        hash = layout_hash_usize(hash, *offset);
    }
    hash
}

fn constants_hash_i64(mut hash: u64, name: &str, value: i64) -> u64 {
    hash = layout_hash_bytes(hash, name.as_bytes());
    layout_hash_i64(hash, value)
}

pub fn embedding_constants_fingerprint() -> u64 {
    macro_rules! constant {
        ($hash:ident, $name:ident) => {
            $hash = constants_hash_i64($hash, stringify!($name), $name as i64);
        };
    }

    let mut hash = LAYOUT_HASH_OFFSET;
    constant!(hash, GHOSTTY_PLATFORM_LINUX);
    constant!(hash, GHOSTTY_RENDERER_BACKEND_UNKNOWN);
    constant!(hash, GHOSTTY_RENDERER_BACKEND_OPENGL);
    constant!(hash, GHOSTTY_RENDERER_BACKEND_METAL);
    constant!(hash, GHOSTTY_RENDERER_BACKEND_WEBGL);
    constant!(hash, GHOSTTY_CLIPBOARD_STANDARD);
    constant!(hash, GHOSTTY_CLIPBOARD_SELECTION);
    constant!(hash, GHOSTTY_CLIPBOARD_PRIMARY);
    constant!(hash, GHOSTTY_CLIPBOARD_REQUEST_PASTE);
    constant!(hash, GHOSTTY_CLIPBOARD_REQUEST_OSC_52_READ);
    constant!(hash, GHOSTTY_CLIPBOARD_REQUEST_OSC_52_WRITE);
    constant!(hash, GHOSTTY_SURFACE_CONTEXT_WINDOW);
    constant!(hash, GHOSTTY_SURFACE_CONTEXT_TAB);
    constant!(hash, GHOSTTY_SURFACE_CONTEXT_SPLIT);
    constant!(hash, GHOSTTY_SURFACE_IO_EXEC);
    constant!(hash, GHOSTTY_SURFACE_IO_MANUAL);
    constant!(hash, GHOSTTY_SURFACE_MAX_ENV_VARS);
    constant!(hash, GHOSTTY_ACTION_RELEASE);
    constant!(hash, GHOSTTY_ACTION_PRESS);
    constant!(hash, GHOSTTY_ACTION_REPEAT);
    constant!(hash, GHOSTTY_INPUT_KEYCODE_NATIVE_MASK);
    constant!(hash, GHOSTTY_INPUT_KEYCODE_PHYSICAL_KEY_FLAG);
    constant!(hash, GHOSTTY_TRIGGER_PHYSICAL);
    constant!(hash, GHOSTTY_TRIGGER_UNICODE);
    constant!(hash, GHOSTTY_TRIGGER_CATCH_ALL);
    constant!(hash, GHOSTTY_MODS_SHIFT);
    constant!(hash, GHOSTTY_MODS_CTRL);
    constant!(hash, GHOSTTY_MODS_ALT);
    constant!(hash, GHOSTTY_MODS_SUPER);
    constant!(hash, GHOSTTY_MODS_CAPS);
    constant!(hash, GHOSTTY_MODS_NUM);
    constant!(hash, GHOSTTY_MODS_SHIFT_RIGHT);
    constant!(hash, GHOSTTY_MODS_CTRL_RIGHT);
    constant!(hash, GHOSTTY_MODS_ALT_RIGHT);
    constant!(hash, GHOSTTY_MODS_SUPER_RIGHT);
    constant!(hash, GHOSTTY_BINDING_FLAGS_CONSUMED);
    constant!(hash, GHOSTTY_BINDING_FLAGS_ALL);
    constant!(hash, GHOSTTY_BINDING_FLAGS_GLOBAL);
    constant!(hash, GHOSTTY_BINDING_FLAGS_PERFORMABLE);
    constant!(hash, GHOSTTY_MOUSE_RELEASE);
    constant!(hash, GHOSTTY_MOUSE_PRESS);
    constant!(hash, GHOSTTY_MOUSE_BUTTON_UNKNOWN);
    constant!(hash, GHOSTTY_MOUSE_BUTTON_LEFT);
    constant!(hash, GHOSTTY_MOUSE_BUTTON_RIGHT);
    constant!(hash, GHOSTTY_MOUSE_BUTTON_MIDDLE);
    constant!(hash, GHOSTTY_MOUSE_BUTTON_FOUR);
    constant!(hash, GHOSTTY_MOUSE_BUTTON_FIVE);
    constant!(hash, GHOSTTY_MOUSE_BUTTON_SIX);
    constant!(hash, GHOSTTY_MOUSE_BUTTON_SEVEN);
    constant!(hash, GHOSTTY_MOUSE_BUTTON_EIGHT);
    constant!(hash, GHOSTTY_MOUSE_BUTTON_NINE);
    constant!(hash, GHOSTTY_MOUSE_BUTTON_TEN);
    constant!(hash, GHOSTTY_MOUSE_BUTTON_ELEVEN);
    constant!(hash, GHOSTTY_MOUSE_PRESSURE_NONE);
    constant!(hash, GHOSTTY_MOUSE_PRESSURE_NORMAL);
    constant!(hash, GHOSTTY_MOUSE_PRESSURE_DEEP);
    constant!(hash, GHOSTTY_COLOR_SCHEME_LIGHT);
    constant!(hash, GHOSTTY_COLOR_SCHEME_DARK);
    constant!(hash, GHOSTTY_TARGET_APP);
    constant!(hash, GHOSTTY_TARGET_SURFACE);
    constant!(hash, GHOSTTY_ACTION_QUIT);
    constant!(hash, GHOSTTY_ACTION_NEW_WINDOW);
    constant!(hash, GHOSTTY_ACTION_TOGGLE_COMMAND_PALETTE);
    constant!(hash, GHOSTTY_ACTION_NEW_TAB);
    constant!(hash, GHOSTTY_ACTION_CLOSE_TAB);
    constant!(hash, GHOSTTY_ACTION_NEW_SPLIT);
    constant!(hash, GHOSTTY_ACTION_CLOSE_ALL_WINDOWS);
    constant!(hash, GHOSTTY_ACTION_TOGGLE_MAXIMIZE);
    constant!(hash, GHOSTTY_ACTION_TOGGLE_FULLSCREEN);
    constant!(hash, GHOSTTY_ACTION_TOGGLE_TAB_OVERVIEW);
    constant!(hash, GHOSTTY_ACTION_TOGGLE_WINDOW_DECORATIONS);
    constant!(hash, GHOSTTY_ACTION_TOGGLE_QUICK_TERMINAL);
    constant!(hash, GHOSTTY_ACTION_TOGGLE_VISIBILITY);
    constant!(hash, GHOSTTY_ACTION_TOGGLE_BACKGROUND_OPACITY);
    constant!(hash, GHOSTTY_ACTION_MOVE_TAB);
    constant!(hash, GHOSTTY_ACTION_GOTO_TAB);
    constant!(hash, GHOSTTY_ACTION_GOTO_SPLIT);
    constant!(hash, GHOSTTY_ACTION_GOTO_WINDOW);
    constant!(hash, GHOSTTY_ACTION_RESIZE_SPLIT);
    constant!(hash, GHOSTTY_ACTION_EQUALIZE_SPLITS);
    constant!(hash, GHOSTTY_ACTION_TOGGLE_SPLIT_ZOOM);
    constant!(hash, GHOSTTY_ACTION_PRESENT_TERMINAL);
    constant!(hash, GHOSTTY_ACTION_SIZE_LIMIT);
    constant!(hash, GHOSTTY_ACTION_RESET_WINDOW_SIZE);
    constant!(hash, GHOSTTY_ACTION_INITIAL_SIZE);
    constant!(hash, GHOSTTY_ACTION_CELL_SIZE);
    constant!(hash, GHOSTTY_ACTION_SCROLLBAR);
    constant!(hash, GHOSTTY_ACTION_RENDER);
    constant!(hash, GHOSTTY_ACTION_INSPECTOR);
    constant!(hash, GHOSTTY_ACTION_SHOW_GTK_INSPECTOR);
    constant!(hash, GHOSTTY_ACTION_RENDER_INSPECTOR);
    constant!(hash, GHOSTTY_ACTION_DESKTOP_NOTIFICATION);
    constant!(hash, GHOSTTY_ACTION_SET_TITLE);
    constant!(hash, GHOSTTY_ACTION_SET_TAB_TITLE);
    constant!(hash, GHOSTTY_ACTION_PROMPT_TITLE);
    constant!(hash, GHOSTTY_ACTION_PWD);
    constant!(hash, GHOSTTY_ACTION_MOUSE_SHAPE);
    constant!(hash, GHOSTTY_ACTION_MOUSE_VISIBILITY);
    constant!(hash, GHOSTTY_ACTION_MOUSE_OVER_LINK);
    constant!(hash, GHOSTTY_ACTION_RENDERER_HEALTH);
    constant!(hash, GHOSTTY_ACTION_OPEN_CONFIG);
    constant!(hash, GHOSTTY_ACTION_QUIT_TIMER);
    constant!(hash, GHOSTTY_ACTION_FLOAT_WINDOW);
    constant!(hash, GHOSTTY_ACTION_SECURE_INPUT);
    constant!(hash, GHOSTTY_ACTION_KEY_SEQUENCE);
    constant!(hash, GHOSTTY_ACTION_KEY_TABLE);
    constant!(hash, GHOSTTY_ACTION_COLOR_CHANGE);
    constant!(hash, GHOSTTY_ACTION_RELOAD_CONFIG);
    constant!(hash, GHOSTTY_ACTION_CONFIG_CHANGE);
    constant!(hash, GHOSTTY_ACTION_CLOSE_WINDOW);
    constant!(hash, GHOSTTY_ACTION_RING_BELL);
    constant!(hash, GHOSTTY_ACTION_SELECTION_CHANGED);
    constant!(hash, GHOSTTY_ACTION_UNDO);
    constant!(hash, GHOSTTY_ACTION_REDO);
    constant!(hash, GHOSTTY_ACTION_CHECK_FOR_UPDATES);
    constant!(hash, GHOSTTY_ACTION_OPEN_URL);
    constant!(hash, GHOSTTY_ACTION_SHOW_CHILD_EXITED);
    constant!(hash, GHOSTTY_ACTION_PROGRESS_REPORT);
    constant!(hash, GHOSTTY_ACTION_SHOW_ON_SCREEN_KEYBOARD);
    constant!(hash, GHOSTTY_ACTION_COMMAND_FINISHED);
    constant!(hash, GHOSTTY_ACTION_START_SEARCH);
    constant!(hash, GHOSTTY_ACTION_END_SEARCH);
    constant!(hash, GHOSTTY_ACTION_SEARCH_TOTAL);
    constant!(hash, GHOSTTY_ACTION_SEARCH_SELECTED);
    constant!(hash, GHOSTTY_ACTION_READONLY);
    constant!(hash, GHOSTTY_ACTION_COPY_TITLE_TO_CLIPBOARD);
    constant!(hash, GHOSTTY_ACTION_OPEN_URL_KIND_UNKNOWN);
    constant!(hash, GHOSTTY_ACTION_OPEN_URL_KIND_TEXT);
    constant!(hash, GHOSTTY_ACTION_OPEN_URL_KIND_HTML);
    constant!(hash, GHOSTTY_SPLIT_DIRECTION_RIGHT);
    constant!(hash, GHOSTTY_SPLIT_DIRECTION_DOWN);
    constant!(hash, GHOSTTY_SPLIT_DIRECTION_LEFT);
    constant!(hash, GHOSTTY_SPLIT_DIRECTION_UP);
    constant!(hash, GHOSTTY_FULLSCREEN_NATIVE);
    constant!(hash, GHOSTTY_FULLSCREEN_MACOS_NON_NATIVE);
    constant!(hash, GHOSTTY_FULLSCREEN_MACOS_NON_NATIVE_VISIBLE_MENU);
    constant!(hash, GHOSTTY_FULLSCREEN_MACOS_NON_NATIVE_PADDED_NOTCH);
    constant!(hash, GHOSTTY_CLOSE_TAB_MODE_THIS);
    constant!(hash, GHOSTTY_CLOSE_TAB_MODE_OTHER);
    constant!(hash, GHOSTTY_CLOSE_TAB_MODE_RIGHT);
    constant!(hash, GHOSTTY_GOTO_TAB_PREVIOUS);
    constant!(hash, GHOSTTY_GOTO_TAB_NEXT);
    constant!(hash, GHOSTTY_GOTO_TAB_LAST);
    constant!(hash, GHOSTTY_GOTO_SPLIT_PREVIOUS);
    constant!(hash, GHOSTTY_GOTO_SPLIT_NEXT);
    constant!(hash, GHOSTTY_GOTO_SPLIT_UP);
    constant!(hash, GHOSTTY_GOTO_SPLIT_LEFT);
    constant!(hash, GHOSTTY_GOTO_SPLIT_DOWN);
    constant!(hash, GHOSTTY_GOTO_SPLIT_RIGHT);
    constant!(hash, GHOSTTY_GOTO_WINDOW_PREVIOUS);
    constant!(hash, GHOSTTY_GOTO_WINDOW_NEXT);
    constant!(hash, GHOSTTY_RESIZE_SPLIT_UP);
    constant!(hash, GHOSTTY_RESIZE_SPLIT_DOWN);
    constant!(hash, GHOSTTY_RESIZE_SPLIT_LEFT);
    constant!(hash, GHOSTTY_RESIZE_SPLIT_RIGHT);
    constant!(hash, GHOSTTY_READONLY_OFF);
    constant!(hash, GHOSTTY_READONLY_ON);
    constant!(hash, GHOSTTY_PROGRESS_STATE_REMOVE);
    constant!(hash, GHOSTTY_PROGRESS_STATE_SET);
    constant!(hash, GHOSTTY_PROGRESS_STATE_ERROR);
    constant!(hash, GHOSTTY_PROGRESS_STATE_INDETERMINATE);
    constant!(hash, GHOSTTY_PROGRESS_STATE_PAUSE);
    constant!(hash, GHOSTTY_RENDERER_HEALTH_HEALTHY);
    constant!(hash, GHOSTTY_RENDERER_HEALTH_UNHEALTHY);
    constant!(hash, GHOSTTY_PROMPT_TITLE_SURFACE);
    constant!(hash, GHOSTTY_PROMPT_TITLE_TAB);
    constant!(hash, GHOSTTY_QUIT_TIMER_START);
    constant!(hash, GHOSTTY_QUIT_TIMER_STOP);
    constant!(hash, GHOSTTY_FLOAT_WINDOW_ON);
    constant!(hash, GHOSTTY_FLOAT_WINDOW_OFF);
    constant!(hash, GHOSTTY_FLOAT_WINDOW_TOGGLE);
    constant!(hash, GHOSTTY_SECURE_INPUT_ON);
    constant!(hash, GHOSTTY_SECURE_INPUT_OFF);
    constant!(hash, GHOSTTY_SECURE_INPUT_TOGGLE);
    constant!(hash, GHOSTTY_INSPECTOR_TOGGLE);
    constant!(hash, GHOSTTY_INSPECTOR_SHOW);
    constant!(hash, GHOSTTY_INSPECTOR_HIDE);
    constant!(hash, GHOSTTY_KEY_UNIDENTIFIED);
    constant!(hash, GHOSTTY_KEY_BACKSPACE);
    constant!(hash, GHOSTTY_KEY_ENTER);
    constant!(hash, GHOSTTY_KEY_SPACE);
    constant!(hash, GHOSTTY_KEY_TAB);
    constant!(hash, GHOSTTY_KEY_DELETE);
    constant!(hash, GHOSTTY_KEY_END);
    constant!(hash, GHOSTTY_KEY_HOME);
    constant!(hash, GHOSTTY_KEY_INSERT);
    constant!(hash, GHOSTTY_KEY_PAGE_DOWN);
    constant!(hash, GHOSTTY_KEY_PAGE_UP);
    constant!(hash, GHOSTTY_KEY_ARROW_DOWN);
    constant!(hash, GHOSTTY_KEY_ARROW_LEFT);
    constant!(hash, GHOSTTY_KEY_ARROW_RIGHT);
    constant!(hash, GHOSTTY_KEY_ARROW_UP);
    constant!(hash, GHOSTTY_KEY_ESCAPE);
    constant!(hash, GHOSTTY_KEY_F1);
    constant!(hash, GHOSTTY_KEY_F2);
    constant!(hash, GHOSTTY_KEY_F3);
    constant!(hash, GHOSTTY_KEY_F4);
    constant!(hash, GHOSTTY_KEY_F5);
    constant!(hash, GHOSTTY_KEY_F6);
    constant!(hash, GHOSTTY_KEY_F7);
    constant!(hash, GHOSTTY_KEY_F8);
    constant!(hash, GHOSTTY_KEY_F9);
    constant!(hash, GHOSTTY_KEY_F10);
    constant!(hash, GHOSTTY_KEY_F11);
    constant!(hash, GHOSTTY_KEY_F12);
    constant!(hash, GHOSTTY_KEY_F13);
    constant!(hash, GHOSTTY_KEY_F14);
    constant!(hash, GHOSTTY_KEY_F15);
    constant!(hash, GHOSTTY_KEY_F16);
    constant!(hash, GHOSTTY_KEY_F17);
    constant!(hash, GHOSTTY_KEY_F18);
    constant!(hash, GHOSTTY_KEY_F19);
    constant!(hash, GHOSTTY_KEY_F20);
    constant!(hash, GHOSTTY_KEY_F21);
    constant!(hash, GHOSTTY_KEY_F22);
    constant!(hash, GHOSTTY_KEY_F23);
    constant!(hash, GHOSTTY_KEY_F24);
    constant!(hash, GHOSTTY_KEY_F25);
    constant!(hash, GHOSTTY_KEY_FN);
    constant!(hash, GHOSTTY_KEY_FN_LOCK);
    constant!(hash, GHOSTTY_KEY_PRINT_SCREEN);
    constant!(hash, GHOSTTY_KEY_SCROLL_LOCK);
    constant!(hash, GHOSTTY_KEY_PAUSE);
    constant!(hash, GHOSTTY_KEY_TABLE_ACTIVATE);
    constant!(hash, GHOSTTY_KEY_TABLE_DEACTIVATE);
    constant!(hash, GHOSTTY_KEY_TABLE_DEACTIVATE_ALL);
    constant!(hash, GHOSTTY_COLOR_KIND_FOREGROUND);
    constant!(hash, GHOSTTY_COLOR_KIND_BACKGROUND);
    constant!(hash, GHOSTTY_COLOR_KIND_CURSOR);
    constant!(hash, GHOSTTY_MOUSE_SHAPE_DEFAULT);
    constant!(hash, GHOSTTY_MOUSE_SHAPE_CONTEXT_MENU);
    constant!(hash, GHOSTTY_MOUSE_SHAPE_HELP);
    constant!(hash, GHOSTTY_MOUSE_SHAPE_POINTER);
    constant!(hash, GHOSTTY_MOUSE_SHAPE_PROGRESS);
    constant!(hash, GHOSTTY_MOUSE_SHAPE_WAIT);
    constant!(hash, GHOSTTY_MOUSE_SHAPE_CELL);
    constant!(hash, GHOSTTY_MOUSE_SHAPE_CROSSHAIR);
    constant!(hash, GHOSTTY_MOUSE_SHAPE_TEXT);
    constant!(hash, GHOSTTY_MOUSE_SHAPE_VERTICAL_TEXT);
    constant!(hash, GHOSTTY_MOUSE_SHAPE_ALIAS);
    constant!(hash, GHOSTTY_MOUSE_SHAPE_COPY);
    constant!(hash, GHOSTTY_MOUSE_SHAPE_MOVE);
    constant!(hash, GHOSTTY_MOUSE_SHAPE_NO_DROP);
    constant!(hash, GHOSTTY_MOUSE_SHAPE_NOT_ALLOWED);
    constant!(hash, GHOSTTY_MOUSE_SHAPE_GRAB);
    constant!(hash, GHOSTTY_MOUSE_SHAPE_GRABBING);
    constant!(hash, GHOSTTY_MOUSE_SHAPE_ALL_SCROLL);
    constant!(hash, GHOSTTY_MOUSE_SHAPE_COL_RESIZE);
    constant!(hash, GHOSTTY_MOUSE_SHAPE_ROW_RESIZE);
    constant!(hash, GHOSTTY_MOUSE_SHAPE_N_RESIZE);
    constant!(hash, GHOSTTY_MOUSE_SHAPE_E_RESIZE);
    constant!(hash, GHOSTTY_MOUSE_SHAPE_S_RESIZE);
    constant!(hash, GHOSTTY_MOUSE_SHAPE_W_RESIZE);
    constant!(hash, GHOSTTY_MOUSE_SHAPE_NE_RESIZE);
    constant!(hash, GHOSTTY_MOUSE_SHAPE_NW_RESIZE);
    constant!(hash, GHOSTTY_MOUSE_SHAPE_SE_RESIZE);
    constant!(hash, GHOSTTY_MOUSE_SHAPE_SW_RESIZE);
    constant!(hash, GHOSTTY_MOUSE_SHAPE_EW_RESIZE);
    constant!(hash, GHOSTTY_MOUSE_SHAPE_NS_RESIZE);
    constant!(hash, GHOSTTY_MOUSE_SHAPE_NESW_RESIZE);
    constant!(hash, GHOSTTY_MOUSE_SHAPE_NWSE_RESIZE);
    constant!(hash, GHOSTTY_MOUSE_SHAPE_ZOOM_IN);
    constant!(hash, GHOSTTY_MOUSE_SHAPE_ZOOM_OUT);
    constant!(hash, GHOSTTY_MOUSE_VISIBLE);
    constant!(hash, GHOSTTY_MOUSE_HIDDEN);
    constant!(hash, GHOSTTY_POINT_VIEWPORT);
    constant!(hash, GHOSTTY_POINT_COORD_TOP_LEFT);
    constant!(hash, GHOSTTY_POINT_COORD_BOTTOM_RIGHT);
    constant!(hash, GHOSTTY_IPC_TARGET_CLASS);
    constant!(hash, GHOSTTY_IPC_TARGET_DETECT);
    constant!(hash, GHOSTTY_IPC_ACTION_NEW_WINDOW);
    constant!(hash, GHOSTTY_IPC_ACTION_TOGGLE_QUICK_TERMINAL);
    hash
}

pub fn embedding_layout_fingerprint() -> u64 {
    let mut hash = LAYOUT_HASH_OFFSET;
    hash = layout_hash_type::<GhosttyRuntimeConfig>(
        hash,
        "runtime_config",
        &[
            (
                "userdata",
                std::mem::offset_of!(GhosttyRuntimeConfig, userdata),
            ),
            (
                "supports_selection_clipboard",
                std::mem::offset_of!(GhosttyRuntimeConfig, supports_selection_clipboard),
            ),
            (
                "wakeup_cb",
                std::mem::offset_of!(GhosttyRuntimeConfig, wakeup_cb),
            ),
            (
                "action_cb",
                std::mem::offset_of!(GhosttyRuntimeConfig, action_cb),
            ),
            (
                "read_clipboard_cb",
                std::mem::offset_of!(GhosttyRuntimeConfig, read_clipboard_cb),
            ),
            (
                "confirm_read_clipboard_cb",
                std::mem::offset_of!(GhosttyRuntimeConfig, confirm_read_clipboard_cb),
            ),
            (
                "write_clipboard_cb",
                std::mem::offset_of!(GhosttyRuntimeConfig, write_clipboard_cb),
            ),
            (
                "close_surface_cb",
                std::mem::offset_of!(GhosttyRuntimeConfig, close_surface_cb),
            ),
            (
                "redraw_surface_cb",
                std::mem::offset_of!(GhosttyRuntimeConfig, redraw_surface_cb),
            ),
        ],
    );
    hash = layout_hash_type::<GhosttySurfaceConfig>(
        hash,
        "surface_config",
        &[
            (
                "platform_tag",
                std::mem::offset_of!(GhosttySurfaceConfig, platform_tag),
            ),
            (
                "platform",
                std::mem::offset_of!(GhosttySurfaceConfig, platform),
            ),
            (
                "userdata",
                std::mem::offset_of!(GhosttySurfaceConfig, userdata),
            ),
            (
                "scale_factor",
                std::mem::offset_of!(GhosttySurfaceConfig, scale_factor),
            ),
            (
                "font_size",
                std::mem::offset_of!(GhosttySurfaceConfig, font_size),
            ),
            (
                "working_directory",
                std::mem::offset_of!(GhosttySurfaceConfig, working_directory),
            ),
            (
                "command",
                std::mem::offset_of!(GhosttySurfaceConfig, command),
            ),
            (
                "env_vars",
                std::mem::offset_of!(GhosttySurfaceConfig, env_vars),
            ),
            (
                "env_var_count",
                std::mem::offset_of!(GhosttySurfaceConfig, env_var_count),
            ),
            (
                "initial_input",
                std::mem::offset_of!(GhosttySurfaceConfig, initial_input),
            ),
            (
                "wait_after_command",
                std::mem::offset_of!(GhosttySurfaceConfig, wait_after_command),
            ),
            (
                "context",
                std::mem::offset_of!(GhosttySurfaceConfig, context),
            ),
            (
                "io_mode",
                std::mem::offset_of!(GhosttySurfaceConfig, io_mode),
            ),
            (
                "io_write_cb",
                std::mem::offset_of!(GhosttySurfaceConfig, io_write_cb),
            ),
            (
                "io_write_userdata",
                std::mem::offset_of!(GhosttySurfaceConfig, io_write_userdata),
            ),
            (
                "initial_output",
                std::mem::offset_of!(GhosttySurfaceConfig, initial_output),
            ),
            (
                "initial_output_len",
                std::mem::offset_of!(GhosttySurfaceConfig, initial_output_len),
            ),
            (
                "initial_width_px",
                std::mem::offset_of!(GhosttySurfaceConfig, initial_width_px),
            ),
            (
                "initial_height_px",
                std::mem::offset_of!(GhosttySurfaceConfig, initial_height_px),
            ),
        ],
    );
    hash = layout_hash_type::<GhosttyPlatformLinux>(
        hash,
        "platform_linux",
        &[
            (
                "userdata",
                std::mem::offset_of!(GhosttyPlatformLinux, userdata),
            ),
            (
                "make_current",
                std::mem::offset_of!(GhosttyPlatformLinux, make_current),
            ),
            (
                "get_proc_address",
                std::mem::offset_of!(GhosttyPlatformLinux, get_proc_address),
            ),
            (
                "done_current",
                std::mem::offset_of!(GhosttyPlatformLinux, done_current),
            ),
        ],
    );
    hash = layout_hash_type::<GhosttyInputKey>(
        hash,
        "input_key",
        &[
            ("action", std::mem::offset_of!(GhosttyInputKey, action)),
            ("mods", std::mem::offset_of!(GhosttyInputKey, mods)),
            (
                "consumed_mods",
                std::mem::offset_of!(GhosttyInputKey, consumed_mods),
            ),
            ("keycode", std::mem::offset_of!(GhosttyInputKey, keycode)),
            ("text", std::mem::offset_of!(GhosttyInputKey, text)),
            (
                "unshifted_codepoint",
                std::mem::offset_of!(GhosttyInputKey, unshifted_codepoint),
            ),
            (
                "composing",
                std::mem::offset_of!(GhosttyInputKey, composing),
            ),
        ],
    );
    hash = layout_hash_type::<GhosttyTarget>(
        hash,
        "target",
        &[
            ("tag", std::mem::offset_of!(GhosttyTarget, tag)),
            ("target", std::mem::offset_of!(GhosttyTarget, target)),
        ],
    );
    hash = layout_hash_type::<GhosttyAction>(
        hash,
        "action",
        &[
            ("tag", std::mem::offset_of!(GhosttyAction, tag)),
            ("action", std::mem::offset_of!(GhosttyAction, action)),
        ],
    );
    hash = layout_hash_type::<GhosttyActionResizeSplit>(
        hash,
        "action_resize_split",
        &[
            (
                "amount",
                std::mem::offset_of!(GhosttyActionResizeSplit, amount),
            ),
            (
                "direction",
                std::mem::offset_of!(GhosttyActionResizeSplit, direction),
            ),
        ],
    );
    hash = layout_hash_type::<GhosttyActionMoveTab>(
        hash,
        "action_move_tab",
        &[("amount", std::mem::offset_of!(GhosttyActionMoveTab, amount))],
    );
    hash = layout_hash_type::<GhosttyActionSizeLimit>(
        hash,
        "action_size_limit",
        &[
            (
                "min_width",
                std::mem::offset_of!(GhosttyActionSizeLimit, min_width),
            ),
            (
                "min_height",
                std::mem::offset_of!(GhosttyActionSizeLimit, min_height),
            ),
            (
                "max_width",
                std::mem::offset_of!(GhosttyActionSizeLimit, max_width),
            ),
            (
                "max_height",
                std::mem::offset_of!(GhosttyActionSizeLimit, max_height),
            ),
        ],
    );
    hash = layout_hash_type::<GhosttyActionInitialSize>(
        hash,
        "action_initial_size",
        &[
            (
                "width",
                std::mem::offset_of!(GhosttyActionInitialSize, width),
            ),
            (
                "height",
                std::mem::offset_of!(GhosttyActionInitialSize, height),
            ),
        ],
    );
    hash = layout_hash_type::<GhosttyActionCellSize>(
        hash,
        "action_cell_size",
        &[
            ("width", std::mem::offset_of!(GhosttyActionCellSize, width)),
            (
                "height",
                std::mem::offset_of!(GhosttyActionCellSize, height),
            ),
        ],
    );
    hash = layout_hash_type::<GhosttyActionMouseOverLink>(
        hash,
        "action_mouse_over_link",
        &[
            ("url", std::mem::offset_of!(GhosttyActionMouseOverLink, url)),
            ("len", std::mem::offset_of!(GhosttyActionMouseOverLink, len)),
        ],
    );
    hash = layout_hash_type::<GhosttyActionSetTitle>(
        hash,
        "action_set_title",
        &[("title", std::mem::offset_of!(GhosttyActionSetTitle, title))],
    );
    hash = layout_hash_type::<GhosttyActionPwd>(
        hash,
        "action_pwd",
        &[("pwd", std::mem::offset_of!(GhosttyActionPwd, pwd))],
    );
    hash = layout_hash_type::<GhosttyActionDesktopNotification>(
        hash,
        "action_desktop_notification",
        &[
            (
                "title",
                std::mem::offset_of!(GhosttyActionDesktopNotification, title),
            ),
            (
                "body",
                std::mem::offset_of!(GhosttyActionDesktopNotification, body),
            ),
        ],
    );
    hash = layout_hash_type::<GhosttyActionKeySequence>(
        hash,
        "action_key_sequence",
        &[
            (
                "active",
                std::mem::offset_of!(GhosttyActionKeySequence, active),
            ),
            (
                "trigger",
                std::mem::offset_of!(GhosttyActionKeySequence, trigger),
            ),
        ],
    );
    hash = layout_hash_type::<GhosttyActionKeyTableActivate>(
        hash,
        "action_key_table_activate",
        &[
            (
                "name",
                std::mem::offset_of!(GhosttyActionKeyTableActivate, name),
            ),
            (
                "len",
                std::mem::offset_of!(GhosttyActionKeyTableActivate, len),
            ),
        ],
    );
    hash = layout_hash_type::<GhosttyActionKeyTable>(
        hash,
        "action_key_table",
        &[
            ("tag", std::mem::offset_of!(GhosttyActionKeyTable, tag)),
            ("value", std::mem::offset_of!(GhosttyActionKeyTable, value)),
        ],
    );
    hash = layout_hash_type::<GhosttyActionColorChange>(
        hash,
        "action_color_change",
        &[
            ("kind", std::mem::offset_of!(GhosttyActionColorChange, kind)),
            ("r", std::mem::offset_of!(GhosttyActionColorChange, r)),
            ("g", std::mem::offset_of!(GhosttyActionColorChange, g)),
            ("b", std::mem::offset_of!(GhosttyActionColorChange, b)),
        ],
    );
    hash = layout_hash_type::<GhosttyActionConfigChange>(
        hash,
        "action_config_change",
        &[(
            "config",
            std::mem::offset_of!(GhosttyActionConfigChange, config),
        )],
    );
    hash = layout_hash_type::<GhosttyActionReloadConfig>(
        hash,
        "action_reload_config",
        &[(
            "soft",
            std::mem::offset_of!(GhosttyActionReloadConfig, soft),
        )],
    );
    hash = layout_hash_type::<GhosttyActionOpenUrl>(
        hash,
        "action_open_url",
        &[
            ("kind", std::mem::offset_of!(GhosttyActionOpenUrl, kind)),
            ("url", std::mem::offset_of!(GhosttyActionOpenUrl, url)),
            ("len", std::mem::offset_of!(GhosttyActionOpenUrl, len)),
        ],
    );
    hash = layout_hash_type::<GhosttyActionChildExited>(
        hash,
        "action_child_exited",
        &[
            (
                "exit_code",
                std::mem::offset_of!(GhosttyActionChildExited, exit_code),
            ),
            (
                "runtime_ms",
                std::mem::offset_of!(GhosttyActionChildExited, runtime_ms),
            ),
        ],
    );
    hash = layout_hash_type::<GhosttyActionProgressReport>(
        hash,
        "action_progress_report",
        &[
            (
                "state",
                std::mem::offset_of!(GhosttyActionProgressReport, state),
            ),
            (
                "progress",
                std::mem::offset_of!(GhosttyActionProgressReport, progress),
            ),
        ],
    );
    hash = layout_hash_type::<GhosttyActionCommandFinished>(
        hash,
        "action_command_finished",
        &[
            (
                "exit_code",
                std::mem::offset_of!(GhosttyActionCommandFinished, exit_code),
            ),
            (
                "duration",
                std::mem::offset_of!(GhosttyActionCommandFinished, duration),
            ),
        ],
    );
    hash = layout_hash_type::<GhosttyActionStartSearch>(
        hash,
        "action_start_search",
        &[(
            "needle",
            std::mem::offset_of!(GhosttyActionStartSearch, needle),
        )],
    );
    hash = layout_hash_type::<GhosttyActionSearchTotal>(
        hash,
        "action_search_total",
        &[(
            "total",
            std::mem::offset_of!(GhosttyActionSearchTotal, total),
        )],
    );
    hash = layout_hash_type::<GhosttyActionSearchSelected>(
        hash,
        "action_search_selected",
        &[(
            "selected",
            std::mem::offset_of!(GhosttyActionSearchSelected, selected),
        )],
    );
    hash = layout_hash_type::<GhosttyActionScrollbar>(
        hash,
        "action_scrollbar",
        &[
            ("total", std::mem::offset_of!(GhosttyActionScrollbar, total)),
            (
                "offset",
                std::mem::offset_of!(GhosttyActionScrollbar, offset),
            ),
            ("len", std::mem::offset_of!(GhosttyActionScrollbar, len)),
        ],
    );
    hash = layout_hash_type::<GhosttyText>(
        hash,
        "text",
        &[
            ("tl_px_x", std::mem::offset_of!(GhosttyText, tl_px_x)),
            ("tl_px_y", std::mem::offset_of!(GhosttyText, tl_px_y)),
            (
                "offset_start",
                std::mem::offset_of!(GhosttyText, offset_start),
            ),
            ("offset_len", std::mem::offset_of!(GhosttyText, offset_len)),
            ("text", std::mem::offset_of!(GhosttyText, text)),
            ("text_len", std::mem::offset_of!(GhosttyText, text_len)),
        ],
    );
    hash = layout_hash_type::<GhosttyPoint>(
        hash,
        "point",
        &[
            ("tag", std::mem::offset_of!(GhosttyPoint, tag)),
            ("coord", std::mem::offset_of!(GhosttyPoint, coord)),
            ("x", std::mem::offset_of!(GhosttyPoint, x)),
            ("y", std::mem::offset_of!(GhosttyPoint, y)),
        ],
    );
    hash = layout_hash_type::<GhosttySelection>(
        hash,
        "selection",
        &[
            ("top_left", std::mem::offset_of!(GhosttySelection, top_left)),
            (
                "bottom_right",
                std::mem::offset_of!(GhosttySelection, bottom_right),
            ),
            (
                "rectangle",
                std::mem::offset_of!(GhosttySelection, rectangle),
            ),
        ],
    );
    hash = layout_hash_type::<GhosttyString>(
        hash,
        "string",
        &[
            ("ptr", std::mem::offset_of!(GhosttyString, ptr)),
            ("len", std::mem::offset_of!(GhosttyString, len)),
            ("sentinel", std::mem::offset_of!(GhosttyString, sentinel)),
        ],
    );
    hash = layout_hash_type::<GhosttySurfaceSizeResult>(
        hash,
        "surface_size",
        &[
            (
                "columns",
                std::mem::offset_of!(GhosttySurfaceSizeResult, columns),
            ),
            ("rows", std::mem::offset_of!(GhosttySurfaceSizeResult, rows)),
            (
                "width_px",
                std::mem::offset_of!(GhosttySurfaceSizeResult, width_px),
            ),
            (
                "height_px",
                std::mem::offset_of!(GhosttySurfaceSizeResult, height_px),
            ),
            (
                "cell_width_px",
                std::mem::offset_of!(GhosttySurfaceSizeResult, cell_width_px),
            ),
            (
                "cell_height_px",
                std::mem::offset_of!(GhosttySurfaceSizeResult, cell_height_px),
            ),
        ],
    );
    hash = layout_hash_type::<GhosttyDiagnostic>(
        hash,
        "diagnostic",
        &[("message", std::mem::offset_of!(GhosttyDiagnostic, message))],
    );
    hash = layout_hash_type::<GhosttyEnvVar>(
        hash,
        "env_var",
        &[
            ("key", std::mem::offset_of!(GhosttyEnvVar, key)),
            ("value", std::mem::offset_of!(GhosttyEnvVar, value)),
        ],
    );
    hash = layout_hash_type::<GhosttyClipboardContent>(
        hash,
        "clipboard_content",
        &[
            ("mime", std::mem::offset_of!(GhosttyClipboardContent, mime)),
            ("data", std::mem::offset_of!(GhosttyClipboardContent, data)),
        ],
    );
    hash = layout_hash_type::<GhosttyInputTrigger>(
        hash,
        "input_trigger",
        &[
            ("tag", std::mem::offset_of!(GhosttyInputTrigger, tag)),
            ("key", std::mem::offset_of!(GhosttyInputTrigger, key)),
            ("mods", std::mem::offset_of!(GhosttyInputTrigger, mods)),
        ],
    );
    hash = layout_hash_type::<GhosttyIpcTargetPayload>(hash, "ipc_target_payload", &[]);
    hash = layout_hash_type::<GhosttyIpcTarget>(
        hash,
        "ipc_target",
        &[
            ("tag", std::mem::offset_of!(GhosttyIpcTarget, tag)),
            ("target", std::mem::offset_of!(GhosttyIpcTarget, target)),
        ],
    );
    hash = layout_hash_type::<GhosttyIpcActionNewWindow>(
        hash,
        "ipc_action_new_window",
        &[(
            "arguments",
            std::mem::offset_of!(GhosttyIpcActionNewWindow, arguments),
        )],
    );
    hash = layout_hash_type::<GhosttyIpcActionPayload>(hash, "ipc_action_payload", &[]);
    hash = layout_hash_type::<GhosttyIpcAction>(
        hash,
        "ipc_action",
        &[
            ("tag", std::mem::offset_of!(GhosttyIpcAction, tag)),
            ("action", std::mem::offset_of!(GhosttyIpcAction, action)),
        ],
    );
    hash
}

pub fn embedding_layout_fingerprint_matches(info: &GhosttyEmbeddingInfo) -> bool {
    info.layout_fingerprint == embedding_layout_fingerprint()
}

pub fn embedding_constants_fingerprint_matches(info: &GhosttyEmbeddingInfo) -> bool {
    info.constants_fingerprint == embedding_constants_fingerprint()
}

pub fn ghostty_string_to_string(
    value: GhosttyString,
    string_free: GhosttyStringFree,
) -> Option<String> {
    if value.ptr.is_null() {
        return None;
    }
    let bytes = unsafe { std::slice::from_raw_parts(value.ptr as *const u8, value.len) };
    let text = String::from_utf8_lossy(bytes).into_owned();
    unsafe {
        string_free(value);
    }
    if text.trim().is_empty() {
        None
    } else {
        Some(text)
    }
}

pub struct GhosttyLibrary {
    handle: *mut c_void,
    path: PathBuf,
    ghostty_init: GhosttyInit,
    ghostty_string_free: GhosttyStringFree,
    embedding_info_direct: GhosttyEmbeddingInfoFn,
    embedding_info_query: GhosttyEmbeddingInfoQuery,
    config_new: GhosttyConfigNew,
    config_free: GhosttyConfigFree,
    config_load_cli_args: GhosttyConfigLoadCliArgs,
    config_load_file: GhosttyConfigLoadFile,
    config_load_string: GhosttyConfigLoadString,
    config_load_default_files: GhosttyConfigLoadDefaultFiles,
    config_load_recursive_files: GhosttyConfigLoadRecursiveFiles,
    config_finalize: GhosttyConfigFinalize,
    config_get: GhosttyConfigGet,
    config_diagnostics_count: GhosttyConfigDiagnosticsCount,
    config_get_diagnostic: GhosttyConfigGetDiagnostic,
    config_open_path: GhosttyConfigOpenPath,
    resources_dir: GhosttyResourcesDir,
    app_new: GhosttyAppNew,
    app_free: GhosttyAppFree,
    app_tick: GhosttyAppTick,
    app_userdata: GhosttyAppUserdata,
    app_set_focus: GhosttyAppSetFocus,
    app_key: GhosttyAppKey,
    app_keyboard_changed: GhosttyAppKeyboardChanged,
    app_open_config: GhosttyAppOpenConfig,
    app_reload_config: GhosttyAppReloadConfig,
    app_update_config: GhosttyAppUpdateConfig,
    app_needs_confirm_quit: GhosttyAppNeedsConfirmQuit,
    app_has_global_keybinds: GhosttyAppHasGlobalKeybinds,
    app_must_draw_from_app_thread: GhosttyAppMustDrawFromAppThread,
    app_set_color_scheme: GhosttyAppSetColorScheme,
    surface_config_new: GhosttySurfaceConfigNew,
    surface_new: GhosttySurfaceNew,
    surface_free: GhosttySurfaceFree,
    surface_userdata: GhosttySurfaceUserdata,
    surface_app: GhosttySurfaceApp,
    surface_inherited_config: GhosttySurfaceInheritedConfig,
    surface_inherited_config_free: GhosttySurfaceInheritedConfigFree,
    surface_update_config: GhosttySurfaceUpdateConfig,
    surface_refresh: GhosttySurfaceRefresh,
    surface_draw: GhosttySurfaceDraw,
    surface_display_realized: GhosttySurfaceDisplayRealized,
    surface_display_unrealized: GhosttySurfaceDisplayUnrealized,
    surface_set_renderer_realized: GhosttySurfaceSetRendererRealized,
    surface_set_content_scale: GhosttySurfaceSetContentScale,
    surface_set_focus: GhosttySurfaceSetFocus,
    surface_set_visible: GhosttySurfaceSetVisible,
    surface_set_occlusion: GhosttySurfaceSetOcclusion,
    surface_set_size: GhosttySurfaceSetSize,
    surface_set_color_scheme: GhosttySurfaceSetColorScheme,
    surface_needs_confirm_quit: GhosttySurfaceNeedsConfirmQuit,
    surface_size: GhosttySurfaceSize,
    surface_process_exited: GhosttySurfaceProcessExited,
    surface_foreground_pid: GhosttySurfaceForegroundPid,
    surface_tty_name: GhosttySurfaceTtyName,
    surface_title: GhosttySurfaceTitle,
    surface_pwd: GhosttySurfacePwd,
    surface_key_translation_mods: GhosttySurfaceKeyTranslationMods,
    surface_key: GhosttySurfaceKey,
    surface_key_is_binding: GhosttySurfaceKeyIsBinding,
    surface_text: GhosttySurfaceText,
    surface_process_output: GhosttySurfaceProcessOutput,
    surface_preedit: GhosttySurfacePreedit,
    surface_mouse_captured: GhosttySurfaceMouseCaptured,
    surface_mouse_button: GhosttySurfaceMouseButton,
    surface_mouse_pos: GhosttySurfaceMousePos,
    surface_mouse_scroll: GhosttySurfaceMouseScroll,
    surface_mouse_pressure: GhosttySurfaceMousePressure,
    surface_ime_point: GhosttySurfaceImePoint,
    surface_request_close: GhosttySurfaceRequestClose,
    surface_split: GhosttySurfaceSplit,
    surface_split_focus: GhosttySurfaceSplitFocus,
    surface_split_resize: GhosttySurfaceSplitResize,
    surface_split_equalize: GhosttySurfaceSplitEqualize,
    surface_split_toggle_zoom: GhosttySurfaceSplitToggleZoom,
    surface_binding_action: GhosttySurfaceBindingAction,
    surface_has_selection: GhosttySurfaceHasSelection,
    surface_select_cursor_cell: GhosttySurfaceSelectCursorCell,
    surface_select_viewport_rows: GhosttySurfaceSelectViewportRows,
    surface_clear_selection: GhosttySurfaceClearSelection,
    surface_read_selection: GhosttySurfaceReadSelection,
    surface_read_text: GhosttySurfaceReadText,
    surface_read_scrollback: GhosttySurfaceReadScrollback,
    surface_free_text: GhosttySurfaceFreeText,
    surface_complete_clipboard_request: GhosttySurfaceCompleteClipboardRequest,
    surface_inspector: GhosttySurfaceInspector,
    inspector_free: GhosttyInspectorFree,
    inspector_set_focus: GhosttyInspectorSetFocus,
    inspector_set_content_scale: GhosttyInspectorSetContentScale,
    inspector_set_size: GhosttyInspectorSetSize,
    inspector_mouse_button: GhosttyInspectorMouseButton,
    inspector_mouse_pos: GhosttyInspectorMousePos,
    inspector_mouse_scroll: GhosttyInspectorMouseScroll,
    inspector_key: GhosttyInspectorKey,
    inspector_text: GhosttyInspectorText,
    inspector_opengl_init: GhosttyInspectorOpenGLInit,
    inspector_opengl_render: GhosttyInspectorOpenGLRender,
    inspector_opengl_shutdown: GhosttyInspectorOpenGLShutdown,
}

impl GhosttyLibrary {
    pub fn open_discovered() -> Result<Self> {
        let path = discover_library().ok_or_else(|| {
            anyhow!("ghostty-internal was not found; set CMUX_GHOSTTY_LIBRARY or run `zig build -Dapp-runtime=none` in the Ghostty checkout")
        })?;
        unsafe { Self::open(&path) }
    }

    pub unsafe fn open(path: &Path) -> Result<Self> {
        let raw_path = CString::new(path.as_os_str().as_encoded_bytes())
            .context("Ghostty library path contained an interior NUL")?;
        let handle = dlopen(raw_path.as_ptr(), RTLD_NOW);
        if handle.is_null() {
            return Err(anyhow!("failed to open {}: {}", path.display(), dl_error()));
        }
        let mut handle_guard = DlopenHandleGuard::new(handle);

        let library = Self {
            handle: handle_guard.handle(),
            path: path.to_path_buf(),
            ghostty_init: load_symbol(handle, "ghostty_init")?,
            ghostty_string_free: load_symbol(handle, "ghostty_string_free")?,
            embedding_info_direct: load_symbol(handle, "ghostty_embedding_info")?,
            embedding_info_query: load_symbol(handle, "ghostty_embedding_info_query")?,
            config_new: load_symbol(handle, "ghostty_config_new")?,
            config_free: load_symbol(handle, "ghostty_config_free")?,
            config_load_cli_args: load_symbol(handle, "ghostty_config_load_cli_args")?,
            config_load_file: load_symbol(handle, "ghostty_config_load_file")?,
            config_load_string: load_symbol(handle, "ghostty_config_load_string")?,
            config_load_default_files: load_symbol(handle, "ghostty_config_load_default_files")?,
            config_load_recursive_files: load_symbol(
                handle,
                "ghostty_config_load_recursive_files",
            )?,
            config_finalize: load_symbol(handle, "ghostty_config_finalize")?,
            config_get: load_symbol(handle, "ghostty_config_get")?,
            config_diagnostics_count: load_symbol(handle, "ghostty_config_diagnostics_count")?,
            config_get_diagnostic: load_symbol(handle, "ghostty_config_get_diagnostic")?,
            config_open_path: load_symbol(handle, "ghostty_config_open_path")?,
            resources_dir: load_symbol(handle, "ghostty_resources_dir")?,
            app_new: load_symbol(handle, "ghostty_app_new")?,
            app_free: load_symbol(handle, "ghostty_app_free")?,
            app_tick: load_symbol(handle, "ghostty_app_tick")?,
            app_userdata: load_symbol(handle, "ghostty_app_userdata")?,
            app_set_focus: load_symbol(handle, "ghostty_app_set_focus")?,
            app_key: load_symbol(handle, "ghostty_app_key")?,
            app_keyboard_changed: load_symbol(handle, "ghostty_app_keyboard_changed")?,
            app_open_config: load_symbol(handle, "ghostty_app_open_config")?,
            app_reload_config: load_symbol(handle, "ghostty_app_reload_config")?,
            app_update_config: load_symbol(handle, "ghostty_app_update_config")?,
            app_needs_confirm_quit: load_symbol(handle, "ghostty_app_needs_confirm_quit")?,
            app_has_global_keybinds: load_symbol(handle, "ghostty_app_has_global_keybinds")?,
            app_must_draw_from_app_thread: load_symbol(
                handle,
                "ghostty_app_must_draw_from_app_thread",
            )?,
            app_set_color_scheme: load_symbol(handle, "ghostty_app_set_color_scheme")?,
            surface_config_new: load_symbol(handle, "ghostty_surface_config_new")?,
            surface_new: load_symbol(handle, "ghostty_surface_new")?,
            surface_free: load_symbol(handle, "ghostty_surface_free")?,
            surface_userdata: load_symbol(handle, "ghostty_surface_userdata")?,
            surface_app: load_symbol(handle, "ghostty_surface_app")?,
            surface_inherited_config: load_symbol(handle, "ghostty_surface_inherited_config")?,
            surface_inherited_config_free: load_symbol(
                handle,
                "ghostty_surface_inherited_config_free",
            )?,
            surface_update_config: load_symbol(handle, "ghostty_surface_update_config")?,
            surface_refresh: load_symbol(handle, "ghostty_surface_refresh")?,
            surface_draw: load_symbol(handle, "ghostty_surface_draw")?,
            surface_display_realized: load_symbol(handle, "ghostty_surface_display_realized")?,
            surface_display_unrealized: load_symbol(handle, "ghostty_surface_display_unrealized")?,
            surface_set_renderer_realized: load_symbol(
                handle,
                "ghostty_surface_set_renderer_realized",
            )?,
            surface_set_content_scale: load_symbol(handle, "ghostty_surface_set_content_scale")?,
            surface_set_focus: load_symbol(handle, "ghostty_surface_set_focus")?,
            surface_set_visible: load_symbol(handle, "ghostty_surface_set_visible")?,
            surface_set_occlusion: load_symbol(handle, "ghostty_surface_set_occlusion")?,
            surface_set_size: load_symbol(handle, "ghostty_surface_set_size")?,
            surface_set_color_scheme: load_symbol(handle, "ghostty_surface_set_color_scheme")?,
            surface_needs_confirm_quit: load_symbol(handle, "ghostty_surface_needs_confirm_quit")?,
            surface_size: load_symbol(handle, "ghostty_surface_size")?,
            surface_process_exited: load_symbol(handle, "ghostty_surface_process_exited")?,
            surface_foreground_pid: load_symbol(handle, "ghostty_surface_foreground_pid")?,
            surface_tty_name: load_symbol(handle, "ghostty_surface_tty_name")?,
            surface_title: load_symbol(handle, "ghostty_surface_title")?,
            surface_pwd: load_symbol(handle, "ghostty_surface_pwd")?,
            surface_key_translation_mods: load_symbol(
                handle,
                "ghostty_surface_key_translation_mods",
            )?,
            surface_key: load_symbol(handle, "ghostty_surface_key")?,
            surface_key_is_binding: load_symbol(handle, "ghostty_surface_key_is_binding")?,
            surface_text: load_symbol(handle, "ghostty_surface_text")?,
            surface_process_output: load_symbol(handle, "ghostty_surface_process_output")?,
            surface_preedit: load_symbol(handle, "ghostty_surface_preedit")?,
            surface_mouse_captured: load_symbol(handle, "ghostty_surface_mouse_captured")?,
            surface_mouse_button: load_symbol(handle, "ghostty_surface_mouse_button")?,
            surface_mouse_pos: load_symbol(handle, "ghostty_surface_mouse_pos")?,
            surface_mouse_scroll: load_symbol(handle, "ghostty_surface_mouse_scroll")?,
            surface_mouse_pressure: load_symbol(handle, "ghostty_surface_mouse_pressure")?,
            surface_ime_point: load_symbol(handle, "ghostty_surface_ime_point")?,
            surface_request_close: load_symbol(handle, "ghostty_surface_request_close")?,
            surface_split: load_symbol(handle, "ghostty_surface_split")?,
            surface_split_focus: load_symbol(handle, "ghostty_surface_split_focus")?,
            surface_split_resize: load_symbol(handle, "ghostty_surface_split_resize")?,
            surface_split_equalize: load_symbol(handle, "ghostty_surface_split_equalize")?,
            surface_split_toggle_zoom: load_symbol(handle, "ghostty_surface_split_toggle_zoom")?,
            surface_binding_action: load_symbol(handle, "ghostty_surface_binding_action")?,
            surface_has_selection: load_symbol(handle, "ghostty_surface_has_selection")?,
            surface_select_cursor_cell: load_symbol(handle, "ghostty_surface_select_cursor_cell")?,
            surface_select_viewport_rows: load_symbol(
                handle,
                "ghostty_surface_select_viewport_rows",
            )?,
            surface_clear_selection: load_symbol(handle, "ghostty_surface_clear_selection")?,
            surface_read_selection: load_symbol(handle, "ghostty_surface_read_selection")?,
            surface_read_text: load_symbol(handle, "ghostty_surface_read_text")?,
            surface_read_scrollback: load_symbol(handle, "ghostty_surface_read_scrollback")?,
            surface_free_text: load_symbol(handle, "ghostty_surface_free_text")?,
            surface_complete_clipboard_request: load_symbol(
                handle,
                "ghostty_surface_complete_clipboard_request",
            )?,
            surface_inspector: load_symbol(handle, "ghostty_surface_inspector")?,
            inspector_free: load_symbol(handle, "ghostty_inspector_free")?,
            inspector_set_focus: load_symbol(handle, "ghostty_inspector_set_focus")?,
            inspector_set_content_scale: load_symbol(
                handle,
                "ghostty_inspector_set_content_scale",
            )?,
            inspector_set_size: load_symbol(handle, "ghostty_inspector_set_size")?,
            inspector_mouse_button: load_symbol(handle, "ghostty_inspector_mouse_button")?,
            inspector_mouse_pos: load_symbol(handle, "ghostty_inspector_mouse_pos")?,
            inspector_mouse_scroll: load_symbol(handle, "ghostty_inspector_mouse_scroll")?,
            inspector_key: load_symbol(handle, "ghostty_inspector_key")?,
            inspector_text: load_symbol(handle, "ghostty_inspector_text")?,
            inspector_opengl_init: load_symbol(handle, "ghostty_inspector_opengl_init")?,
            inspector_opengl_render: load_symbol(handle, "ghostty_inspector_opengl_render")?,
            inspector_opengl_shutdown: load_symbol(handle, "ghostty_inspector_opengl_shutdown")?,
        };
        handle_guard.disarm();
        Ok(library)
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn config_new_fn(&self) -> GhosttyConfigNew {
        self.config_new
    }

    pub fn config_free_fn(&self) -> GhosttyConfigFree {
        self.config_free
    }

    pub fn config_load_default_files_fn(&self) -> GhosttyConfigLoadDefaultFiles {
        self.config_load_default_files
    }

    pub fn config_load_recursive_files_fn(&self) -> GhosttyConfigLoadRecursiveFiles {
        self.config_load_recursive_files
    }

    pub fn config_finalize_fn(&self) -> GhosttyConfigFinalize {
        self.config_finalize
    }

    pub fn config_get_fn(&self) -> GhosttyConfigGet {
        self.config_get
    }

    pub fn config_diagnostics_count_fn(&self) -> GhosttyConfigDiagnosticsCount {
        self.config_diagnostics_count
    }

    pub fn config_get_diagnostic_fn(&self) -> GhosttyConfigGetDiagnostic {
        self.config_get_diagnostic
    }

    pub fn config_open_path_fn(&self) -> GhosttyConfigOpenPath {
        self.config_open_path
    }

    pub fn string_free_fn(&self) -> GhosttyStringFree {
        self.ghostty_string_free
    }

    pub fn resources_dir(&self) -> Result<Option<String>> {
        self.initialize()?;
        let value = unsafe { (self.resources_dir)() };
        Ok(ghostty_string_to_string(value, self.ghostty_string_free))
    }

    pub fn embedding_info(&self) -> Result<GhosttyEmbeddingInfo> {
        Ok(self.embedding_info_report()?.info())
    }

    pub fn embedding_info_report(&self) -> Result<GhosttyEmbeddingInfoReport> {
        let direct = unsafe { (self.embedding_info_direct)() };
        let mut info = std::mem::MaybeUninit::<GhosttyEmbeddingInfo>::zeroed();
        let len = std::mem::size_of::<GhosttyEmbeddingInfo>();
        let ok = unsafe { (self.embedding_info_query)(info.as_mut_ptr(), len) };
        if !ok {
            return Err(anyhow!(
                "ghostty_embedding_info_query returned false for {len}-byte buffer"
            ));
        }
        Ok(GhosttyEmbeddingInfoReport {
            direct,
            query: unsafe { info.assume_init() },
        })
    }

    pub fn app_update_config_fn(&self) -> GhosttyAppUpdateConfig {
        self.app_update_config
    }

    pub fn surface_update_config_fn(&self) -> GhosttySurfaceUpdateConfig {
        self.surface_update_config
    }

    pub fn surface_inherited_config_fn(&self) -> GhosttySurfaceInheritedConfig {
        self.surface_inherited_config
    }

    pub fn surface_inherited_config_free_fn(&self) -> GhosttySurfaceInheritedConfigFree {
        self.surface_inherited_config_free
    }

    pub fn initialize(&self) -> Result<()> {
        initialize_ghostty(self.ghostty_init)
    }

    pub fn load_default_config(&self) -> Result<GhosttyConfigGuard> {
        self.load_default_config_with_override(None)
    }

    pub fn load_default_config_with_string(&self, contents: &str) -> Result<GhosttyConfigGuard> {
        self.load_default_config_with_override(Some(contents))
    }

    fn load_default_config_with_override(
        &self,
        contents: Option<&str>,
    ) -> Result<GhosttyConfigGuard> {
        let config = unsafe { (self.config_new)() };
        if config.is_null() {
            return Err(anyhow!("ghostty_config_new returned null"));
        }
        let guard = GhosttyConfigGuard {
            config,
            free: self.config_free,
        };
        load_and_finalize_default_config_with_override(
            guard.config,
            self.config_load_default_files,
            contents,
            Some(self.config_load_string),
            self.config_load_recursive_files,
            self.config_finalize,
            self.config_diagnostics_count,
            self.config_get_diagnostic,
        )?;
        Ok(guard)
    }

    pub fn config_string(&self, config: &GhosttyConfigGuard, key: &str) -> Option<String> {
        let key = CString::new(key).ok()?;
        let mut value: *const c_char = ptr::null();
        let found = unsafe {
            (self.config_get)(
                config.config,
                &mut value as *mut *const c_char as *mut c_void,
                key.as_ptr(),
                key.as_bytes().len(),
            )
        };
        (found && !value.is_null())
            .then(|| unsafe { CStr::from_ptr(value) }.to_string_lossy().into())
    }

    pub fn load_config_file(&self, path: &Path) -> Result<GhosttyConfigGuard> {
        let config = unsafe { (self.config_new)() };
        if config.is_null() {
            return Err(anyhow!("ghostty_config_new returned null"));
        }
        let guard = GhosttyConfigGuard {
            config,
            free: self.config_free,
        };
        load_and_finalize_config_file(
            guard.config,
            path,
            self.config_load_file,
            self.config_load_recursive_files,
            self.config_finalize,
            self.config_diagnostics_count,
            self.config_get_diagnostic,
        )?;
        Ok(guard)
    }

    pub fn load_config_string(&self, contents: &str) -> Result<GhosttyConfigGuard> {
        let config = unsafe { (self.config_new)() };
        if config.is_null() {
            return Err(anyhow!("ghostty_config_new returned null"));
        }
        let guard = GhosttyConfigGuard {
            config,
            free: self.config_free,
        };
        load_and_finalize_config_string(
            guard.config,
            contents,
            self.config_load_string,
            self.config_load_recursive_files,
            self.config_finalize,
            self.config_diagnostics_count,
            self.config_get_diagnostic,
        )?;
        Ok(guard)
    }

    pub fn create_app(&self, config: &GhosttyConfigGuard) -> Result<GhosttyAppGuard> {
        self.create_app_with_runtime(config, default_runtime_config())
    }

    pub fn create_app_with_runtime(
        &self,
        config: &GhosttyConfigGuard,
        runtime: GhosttyRuntimeConfig,
    ) -> Result<GhosttyAppGuard> {
        let app = unsafe { (self.app_new)(&runtime, config.config) };
        if app.is_null() {
            return Err(anyhow!("ghostty_app_new returned null"));
        }
        Ok(GhosttyAppGuard {
            app,
            free: self.app_free,
            tick: self.app_tick,
            userdata: self.app_userdata,
            set_focus: self.app_set_focus,
            key: self.app_key,
            keyboard_changed: self.app_keyboard_changed,
            open_config: self.app_open_config,
            reload_config: self.app_reload_config,
            update_config: self.app_update_config,
            needs_confirm_quit: self.app_needs_confirm_quit,
            has_global_keybinds: self.app_has_global_keybinds,
            must_draw_from_app_thread: self.app_must_draw_from_app_thread,
            set_color_scheme: self.app_set_color_scheme,
        })
    }

    pub fn create_surface(
        &self,
        app: &GhosttyAppGuard,
        config: &GhosttySurfaceConfig,
    ) -> Result<GhosttySurfaceGuard> {
        let surface = unsafe { (self.surface_new)(app.app, config) };
        if surface.is_null() {
            return Err(anyhow!("ghostty_surface_new returned null"));
        }
        Ok(GhosttySurfaceGuard {
            surface,
            display_unrealized_complete: false,
            free: self.surface_free,
            userdata: self.surface_userdata,
            surface_app: self.surface_app,
            inherited_config: self.surface_inherited_config,
            inherited_config_free: self.surface_inherited_config_free,
            update_config: self.surface_update_config,
            string_free: self.ghostty_string_free,
            refresh: self.surface_refresh,
            draw: self.surface_draw,
            display_realized: self.surface_display_realized,
            display_unrealized: self.surface_display_unrealized,
            set_renderer_realized: self.surface_set_renderer_realized,
            set_content_scale: self.surface_set_content_scale,
            set_focus: self.surface_set_focus,
            set_visible: self.surface_set_visible,
            set_occlusion: self.surface_set_occlusion,
            set_size: self.surface_set_size,
            set_color_scheme: self.surface_set_color_scheme,
            needs_confirm_quit: self.surface_needs_confirm_quit,
            surface_size: self.surface_size,
            process_exited: self.surface_process_exited,
            foreground_pid: self.surface_foreground_pid,
            tty_name: self.surface_tty_name,
            title: self.surface_title,
            pwd: self.surface_pwd,
            key_translation_mods: self.surface_key_translation_mods,
            key: self.surface_key,
            key_is_binding: self.surface_key_is_binding,
            text: self.surface_text,
            process_output: self.surface_process_output,
            preedit: self.surface_preedit,
            mouse_captured: self.surface_mouse_captured,
            mouse_button: self.surface_mouse_button,
            mouse_pos: self.surface_mouse_pos,
            mouse_scroll: self.surface_mouse_scroll,
            mouse_pressure: self.surface_mouse_pressure,
            ime_point: self.surface_ime_point,
            request_close: self.surface_request_close,
            split: self.surface_split,
            split_focus: self.surface_split_focus,
            split_resize: self.surface_split_resize,
            split_equalize: self.surface_split_equalize,
            split_toggle_zoom: self.surface_split_toggle_zoom,
            binding_action: self.surface_binding_action,
            has_selection: self.surface_has_selection,
            select_cursor_cell: self.surface_select_cursor_cell,
            select_viewport_rows: self.surface_select_viewport_rows,
            clear_selection: self.surface_clear_selection,
            read_selection: self.surface_read_selection,
            read_text: self.surface_read_text,
            read_scrollback: self.surface_read_scrollback,
            free_text: self.surface_free_text,
            surface_inspector: self.surface_inspector,
            inspector_free: self.inspector_free,
            inspector_set_focus: self.inspector_set_focus,
            inspector_set_content_scale: self.inspector_set_content_scale,
            inspector_set_size: self.inspector_set_size,
            inspector_mouse_button: self.inspector_mouse_button,
            inspector_mouse_pos: self.inspector_mouse_pos,
            inspector_mouse_scroll: self.inspector_mouse_scroll,
            inspector_key: self.inspector_key,
            inspector_text: self.inspector_text,
            inspector_opengl_init: self.inspector_opengl_init,
            inspector_opengl_render: self.inspector_opengl_render,
            inspector_opengl_shutdown: self.inspector_opengl_shutdown,
        })
    }

    pub fn linux_surface_config(&self, platform: GhosttyPlatformLinux) -> GhosttySurfaceConfig {
        let mut config = unsafe { (self.surface_config_new)() };
        config.configure_linux_platform(platform);
        config
    }

    pub fn complete_clipboard_request_fn(&self) -> GhosttySurfaceCompleteClipboardRequest {
        self.surface_complete_clipboard_request
    }

    pub fn surface_needs_confirm_quit_fn(&self) -> GhosttySurfaceNeedsConfirmQuit {
        self.surface_needs_confirm_quit
    }
}

impl Drop for GhosttyLibrary {
    fn drop(&mut self) {
        if !self.handle.is_null() {
            unsafe {
                dlclose(self.handle);
            }
        }
    }
}

struct DlopenHandleGuard {
    handle: *mut c_void,
}

impl DlopenHandleGuard {
    fn new(handle: *mut c_void) -> Self {
        Self { handle }
    }

    fn handle(&self) -> *mut c_void {
        self.handle
    }

    fn disarm(&mut self) {
        self.handle = ptr::null_mut();
    }
}

impl Drop for DlopenHandleGuard {
    fn drop(&mut self) {
        if !self.handle.is_null() {
            unsafe {
                dlclose(self.handle);
            }
        }
    }
}

pub struct GhosttyConfigGuard {
    config: GhosttyConfig,
    free: GhosttyConfigFree,
}

impl Drop for GhosttyConfigGuard {
    fn drop(&mut self) {
        if !self.config.is_null() {
            unsafe {
                (self.free)(self.config);
            }
        }
    }
}

pub struct GhosttyAppGuard {
    app: GhosttyApp,
    free: GhosttyAppFree,
    tick: GhosttyAppTick,
    userdata: GhosttyAppUserdata,
    set_focus: GhosttyAppSetFocus,
    key: GhosttyAppKey,
    keyboard_changed: GhosttyAppKeyboardChanged,
    open_config: GhosttyAppOpenConfig,
    reload_config: GhosttyAppReloadConfig,
    update_config: GhosttyAppUpdateConfig,
    needs_confirm_quit: GhosttyAppNeedsConfirmQuit,
    has_global_keybinds: GhosttyAppHasGlobalKeybinds,
    must_draw_from_app_thread: GhosttyAppMustDrawFromAppThread,
    set_color_scheme: GhosttyAppSetColorScheme,
}

impl GhosttyAppGuard {
    pub fn raw(&self) -> GhosttyApp {
        self.app
    }

    pub fn userdata(&self) -> *mut c_void {
        unsafe { (self.userdata)(self.app) }
    }

    pub fn tick_fn(&self) -> GhosttyAppTick {
        self.tick
    }

    pub fn keyboard_changed_fn(&self) -> GhosttyAppKeyboardChanged {
        self.keyboard_changed
    }

    pub fn tick(&self) -> bool {
        unsafe { (self.tick)(self.app) }
    }

    pub fn set_focus(&self, focused: bool) -> bool {
        unsafe { (self.set_focus)(self.app, focused) }
    }

    pub fn key(&self, event: GhosttyInputKey) -> bool {
        unsafe { (self.key)(self.app, event) }
    }

    pub fn keyboard_changed(&self) -> bool {
        unsafe { (self.keyboard_changed)(self.app) }
    }

    pub fn open_config(&self) -> bool {
        unsafe { (self.open_config)(self.app) }
    }

    pub fn reload_config(&self, soft: bool) -> bool {
        unsafe { (self.reload_config)(self.app, soft) }
    }

    pub fn update_config(&self, config: &GhosttyConfigGuard) -> bool {
        unsafe { (self.update_config)(self.app, config.config) }
    }

    pub fn needs_confirm_quit(&self) -> bool {
        unsafe { (self.needs_confirm_quit)(self.app) }
    }

    pub fn has_global_keybinds(&self) -> bool {
        unsafe { (self.has_global_keybinds)(self.app) }
    }

    pub fn must_draw_from_app_thread(&self) -> bool {
        unsafe { (self.must_draw_from_app_thread)(self.app) }
    }

    pub fn set_color_scheme(&self, color_scheme: c_int) -> bool {
        unsafe { (self.set_color_scheme)(self.app, color_scheme) }
    }
}

impl Drop for GhosttyAppGuard {
    fn drop(&mut self) {
        if !self.app.is_null() {
            unsafe {
                (self.free)(self.app);
            }
        }
    }
}

pub struct GhosttySurfaceGuard {
    surface: GhosttySurface,
    display_unrealized_complete: bool,
    free: GhosttySurfaceFree,
    userdata: GhosttySurfaceUserdata,
    surface_app: GhosttySurfaceApp,
    inherited_config: GhosttySurfaceInheritedConfig,
    inherited_config_free: GhosttySurfaceInheritedConfigFree,
    update_config: GhosttySurfaceUpdateConfig,
    string_free: GhosttyStringFree,
    refresh: GhosttySurfaceRefresh,
    draw: GhosttySurfaceDraw,
    display_realized: GhosttySurfaceDisplayRealized,
    display_unrealized: GhosttySurfaceDisplayUnrealized,
    set_renderer_realized: GhosttySurfaceSetRendererRealized,
    set_content_scale: GhosttySurfaceSetContentScale,
    set_focus: GhosttySurfaceSetFocus,
    set_visible: GhosttySurfaceSetVisible,
    set_occlusion: GhosttySurfaceSetOcclusion,
    set_size: GhosttySurfaceSetSize,
    set_color_scheme: GhosttySurfaceSetColorScheme,
    needs_confirm_quit: GhosttySurfaceNeedsConfirmQuit,
    surface_size: GhosttySurfaceSize,
    process_exited: GhosttySurfaceProcessExited,
    foreground_pid: GhosttySurfaceForegroundPid,
    tty_name: GhosttySurfaceTtyName,
    title: GhosttySurfaceTitle,
    pwd: GhosttySurfacePwd,
    key_translation_mods: GhosttySurfaceKeyTranslationMods,
    key: GhosttySurfaceKey,
    key_is_binding: GhosttySurfaceKeyIsBinding,
    text: GhosttySurfaceText,
    process_output: GhosttySurfaceProcessOutput,
    preedit: GhosttySurfacePreedit,
    mouse_captured: GhosttySurfaceMouseCaptured,
    mouse_button: GhosttySurfaceMouseButton,
    mouse_pos: GhosttySurfaceMousePos,
    mouse_scroll: GhosttySurfaceMouseScroll,
    mouse_pressure: GhosttySurfaceMousePressure,
    ime_point: GhosttySurfaceImePoint,
    request_close: GhosttySurfaceRequestClose,
    split: GhosttySurfaceSplit,
    split_focus: GhosttySurfaceSplitFocus,
    split_resize: GhosttySurfaceSplitResize,
    split_equalize: GhosttySurfaceSplitEqualize,
    split_toggle_zoom: GhosttySurfaceSplitToggleZoom,
    binding_action: GhosttySurfaceBindingAction,
    has_selection: GhosttySurfaceHasSelection,
    select_cursor_cell: GhosttySurfaceSelectCursorCell,
    select_viewport_rows: GhosttySurfaceSelectViewportRows,
    clear_selection: GhosttySurfaceClearSelection,
    read_selection: GhosttySurfaceReadSelection,
    read_text: GhosttySurfaceReadText,
    read_scrollback: GhosttySurfaceReadScrollback,
    free_text: GhosttySurfaceFreeText,
    surface_inspector: GhosttySurfaceInspector,
    inspector_free: GhosttyInspectorFree,
    inspector_set_focus: GhosttyInspectorSetFocus,
    inspector_set_content_scale: GhosttyInspectorSetContentScale,
    inspector_set_size: GhosttyInspectorSetSize,
    inspector_mouse_button: GhosttyInspectorMouseButton,
    inspector_mouse_pos: GhosttyInspectorMousePos,
    inspector_mouse_scroll: GhosttyInspectorMouseScroll,
    inspector_key: GhosttyInspectorKey,
    inspector_text: GhosttyInspectorText,
    inspector_opengl_init: GhosttyInspectorOpenGLInit,
    inspector_opengl_render: GhosttyInspectorOpenGLRender,
    inspector_opengl_shutdown: GhosttyInspectorOpenGLShutdown,
}

impl GhosttySurfaceGuard {
    pub fn raw(&self) -> GhosttySurface {
        self.surface
    }

    pub fn userdata(&self) -> *mut c_void {
        unsafe { (self.userdata)(self.surface) }
    }

    pub fn app(&self) -> GhosttyApp {
        unsafe { (self.surface_app)(self.surface) }
    }

    pub fn inherited_config(&self, context: c_int) -> GhosttySurfaceConfig {
        unsafe { (self.inherited_config)(self.surface, context) }
    }

    pub fn free_inherited_config(&self, config: &mut GhosttySurfaceConfig) {
        unsafe { (self.inherited_config_free)(self.surface, config) }
    }

    pub fn update_config(&self, config: &GhosttyConfigGuard) -> bool {
        unsafe { (self.update_config)(self.surface, config.config) }
    }

    pub fn refresh(&self) -> bool {
        unsafe { (self.refresh)(self.surface) }
    }

    pub fn draw(&self) -> bool {
        unsafe { (self.draw)(self.surface) }
    }

    pub fn display_realized(&mut self) -> bool {
        let realized = unsafe { (self.display_realized)(self.surface) };
        if realized {
            self.display_unrealized_complete = false;
        }
        realized
    }

    pub fn display_unrealized(&mut self) -> bool {
        if self.display_unrealized_complete {
            return true;
        }
        let unrealized = unsafe { (self.display_unrealized)(self.surface) };
        if unrealized {
            self.display_unrealized_complete = true;
        }
        unrealized
    }

    pub fn set_renderer_realized(&mut self, realized: bool) -> bool {
        let updated = unsafe { (self.set_renderer_realized)(self.surface, realized) };
        if updated {
            self.display_unrealized_complete = !realized;
        }
        updated
    }

    pub fn set_content_scale(&self, x: f64, y: f64) -> bool {
        unsafe { (self.set_content_scale)(self.surface, x, y) }
    }

    pub fn set_focus(&self, focused: bool) -> bool {
        unsafe { (self.set_focus)(self.surface, focused) }
    }

    pub fn set_visible(&self, visible: bool) -> bool {
        unsafe { (self.set_visible)(self.surface, visible) }
    }

    pub fn set_occlusion(&self, visible: bool) -> bool {
        unsafe { (self.set_occlusion)(self.surface, visible) }
    }

    pub fn set_size(&self, width: u32, height: u32) -> bool {
        unsafe { (self.set_size)(self.surface, width, height) }
    }

    pub fn set_color_scheme(&self, color_scheme: c_int) -> bool {
        unsafe { (self.set_color_scheme)(self.surface, color_scheme) }
    }

    pub fn needs_confirm_quit(&self) -> bool {
        unsafe { (self.needs_confirm_quit)(self.surface) }
    }

    pub fn size(&self) -> GhosttySurfaceSizeResult {
        unsafe { (self.surface_size)(self.surface) }
    }

    pub fn process_exited(&self) -> bool {
        unsafe { (self.process_exited)(self.surface) }
    }

    pub fn foreground_pid(&self) -> Option<u32> {
        let pid = unsafe { (self.foreground_pid)(self.surface) };
        u32::try_from(pid).ok().filter(|pid| *pid > 0)
    }

    pub fn tty_name(&self) -> Option<String> {
        let tty = unsafe { (self.tty_name)(self.surface) };
        ghostty_string_to_string(tty, self.string_free)
    }

    pub fn title(&self) -> Option<String> {
        let title = unsafe { (self.title)(self.surface) };
        ghostty_string_to_string(title, self.string_free)
    }

    pub fn pwd(&self) -> Option<String> {
        let pwd = unsafe { (self.pwd)(self.surface) };
        ghostty_string_to_string(pwd, self.string_free)
    }

    pub fn key_translation_mods(&self, mods: c_int) -> c_int {
        unsafe { (self.key_translation_mods)(self.surface, mods) }
    }

    pub fn key(&self, event: GhosttyInputKey) -> bool {
        unsafe { (self.key)(self.surface, event) }
    }

    pub fn key_binding_flags(&self, event: GhosttyInputKey) -> Option<c_int> {
        let mut flags = 0;
        let is_binding = unsafe { (self.key_is_binding)(self.surface, event, &mut flags) };
        if is_binding {
            Some(flags)
        } else {
            None
        }
    }

    pub fn key_is_binding(&self, event: GhosttyInputKey) -> bool {
        self.key_binding_flags(event).is_some()
    }

    pub fn text(&self, text: &str) -> bool {
        unsafe { (self.text)(self.surface, text.as_ptr() as *const c_char, text.len()) }
    }

    pub fn process_output(&self, bytes: &[u8]) -> bool {
        unsafe { (self.process_output)(self.surface, bytes.as_ptr() as *const c_char, bytes.len()) }
    }

    pub fn preedit(&self, text: Option<&str>) -> bool {
        unsafe {
            if let Some(text) = text {
                (self.preedit)(self.surface, text.as_ptr() as *const c_char, text.len())
            } else {
                (self.preedit)(self.surface, ptr::null(), 0)
            }
        }
    }

    pub fn mouse_captured(&self) -> bool {
        unsafe { (self.mouse_captured)(self.surface) }
    }

    pub fn mouse_button(&self, action: c_int, button: c_int, mods: c_int) -> bool {
        unsafe { (self.mouse_button)(self.surface, action, button, mods) }
    }

    pub fn mouse_pos(&self, x: f64, y: f64, mods: c_int) -> bool {
        unsafe { (self.mouse_pos)(self.surface, x, y, mods) }
    }

    pub fn mouse_scroll(&self, x: f64, y: f64, scroll_mods: c_int) -> bool {
        unsafe { (self.mouse_scroll)(self.surface, x, y, scroll_mods) }
    }

    pub fn mouse_pressure(&self, stage: c_int, pressure: f64) -> bool {
        unsafe { (self.mouse_pressure)(self.surface, stage, pressure) }
    }

    pub fn ime_point(&self) -> Option<GhosttyImePoint> {
        let mut x = 0.0;
        let mut y = 0.0;
        let mut width = 1.0;
        let mut height = 1.0;
        let ok = unsafe { (self.ime_point)(self.surface, &mut x, &mut y, &mut width, &mut height) };
        ok.then_some(GhosttyImePoint {
            x,
            y,
            width,
            height,
        })
    }

    pub fn request_close(&self) -> bool {
        unsafe { (self.request_close)(self.surface) }
    }

    pub fn split(&self, direction: c_int) -> bool {
        unsafe { (self.split)(self.surface, direction) }
    }

    pub fn split_focus(&self, direction: c_int) -> bool {
        unsafe { (self.split_focus)(self.surface, direction) }
    }

    pub fn split_resize(&self, direction: c_int, amount: u16) -> bool {
        unsafe { (self.split_resize)(self.surface, direction, amount) }
    }

    pub fn split_equalize(&self) -> bool {
        unsafe { (self.split_equalize)(self.surface) }
    }

    pub fn split_toggle_zoom(&self) -> bool {
        unsafe { (self.split_toggle_zoom)(self.surface) }
    }

    pub fn binding_action(&self, action: &str) -> Result<bool> {
        let action = binding_action_c_string(action)?;
        Ok(
            unsafe {
                (self.binding_action)(self.surface, action.as_ptr(), action.as_bytes().len())
            },
        )
    }

    pub fn has_selection(&self) -> bool {
        unsafe { (self.has_selection)(self.surface) }
    }

    pub fn select_cursor_cell(&self) -> bool {
        unsafe { (self.select_cursor_cell)(self.surface) }
    }

    pub fn select_viewport_rows(&self, start_row: u32, end_row: u32) -> bool {
        unsafe { (self.select_viewport_rows)(self.surface, start_row, end_row) }
    }

    pub fn clear_selection(&self) -> bool {
        unsafe { (self.clear_selection)(self.surface) }
    }

    pub fn read_selection_text(&self) -> Option<String> {
        let mut text = empty_ghostty_text();
        let ok = unsafe { (self.read_selection)(self.surface, &mut text) };
        self.ghostty_text_result(ok, &mut text)
    }

    pub fn read_viewport_text(&self) -> Option<String> {
        let mut text = empty_ghostty_text();
        let ok = unsafe { (self.read_text)(self.surface, GhosttySelection::viewport(), &mut text) };
        self.ghostty_text_result(ok, &mut text)
    }

    pub fn read_scrollback_text(&self, max_bytes: usize) -> Option<String> {
        let mut text = empty_ghostty_text();
        let ok = unsafe { (self.read_scrollback)(self.surface, max_bytes, &mut text) };
        self.ghostty_text_result(ok, &mut text)
    }

    pub fn create_inspector(&self) -> Result<GhosttyInspectorGuard> {
        let inspector = unsafe { (self.surface_inspector)(self.surface) };
        if inspector.is_null() {
            return Err(anyhow!("ghostty_surface_inspector returned null"));
        }
        Ok(GhosttyInspectorGuard {
            inspector,
            free: self.inspector_free,
            set_focus: self.inspector_set_focus,
            set_content_scale: self.inspector_set_content_scale,
            set_size: self.inspector_set_size,
            mouse_button: self.inspector_mouse_button,
            mouse_pos: self.inspector_mouse_pos,
            mouse_scroll: self.inspector_mouse_scroll,
            key: self.inspector_key,
            text: self.inspector_text,
            opengl_init: self.inspector_opengl_init,
            opengl_render: self.inspector_opengl_render,
            opengl_shutdown: self.inspector_opengl_shutdown,
            opengl_shutdown_complete: false,
        })
    }

    fn ghostty_text_result(&self, ok: bool, text: &mut GhosttyText) -> Option<String> {
        if !ok || text.text.is_null() {
            if !text.text.is_null() {
                unsafe {
                    (self.free_text)(self.surface, text);
                }
            }
            return None;
        }
        let bytes = unsafe { std::slice::from_raw_parts(text.text as *const u8, text.text_len) };
        let value = String::from_utf8_lossy(bytes).into_owned();
        unsafe {
            (self.free_text)(self.surface, text);
        }
        Some(value)
    }
}

fn empty_ghostty_text() -> GhosttyText {
    GhosttyText {
        tl_px_x: 0.0,
        tl_px_y: 0.0,
        offset_start: 0,
        offset_len: 0,
        text: ptr::null(),
        text_len: 0,
    }
}

pub struct GhosttyInspectorGuard {
    inspector: GhosttyInspector,
    free: GhosttyInspectorFree,
    set_focus: GhosttyInspectorSetFocus,
    set_content_scale: GhosttyInspectorSetContentScale,
    set_size: GhosttyInspectorSetSize,
    mouse_button: GhosttyInspectorMouseButton,
    mouse_pos: GhosttyInspectorMousePos,
    mouse_scroll: GhosttyInspectorMouseScroll,
    key: GhosttyInspectorKey,
    text: GhosttyInspectorText,
    opengl_init: GhosttyInspectorOpenGLInit,
    opengl_render: GhosttyInspectorOpenGLRender,
    opengl_shutdown: GhosttyInspectorOpenGLShutdown,
    opengl_shutdown_complete: bool,
}

impl GhosttyInspectorGuard {
    pub fn init_opengl(&self) -> bool {
        let glsl_version = CString::new("#version 430").expect("static GLSL version has no NUL");
        unsafe { (self.opengl_init)(self.inspector, glsl_version.as_ptr()) }
    }

    pub fn render(&self) -> bool {
        unsafe { (self.opengl_render)(self.inspector) }
    }

    pub fn shutdown_opengl(&mut self) -> bool {
        if self.opengl_shutdown_complete {
            return true;
        }
        let shutdown = unsafe { (self.opengl_shutdown)(self.inspector) };
        if shutdown {
            self.opengl_shutdown_complete = true;
        }
        shutdown
    }

    pub fn set_focus(&self, focused: bool) -> bool {
        unsafe { (self.set_focus)(self.inspector, focused) }
    }

    pub fn set_content_scale(&self, x: f64, y: f64) -> bool {
        unsafe { (self.set_content_scale)(self.inspector, x, y) }
    }

    pub fn set_size(&self, width: u32, height: u32) -> bool {
        unsafe { (self.set_size)(self.inspector, width, height) }
    }

    pub fn mouse_button(&self, action: c_int, button: c_int, mods: c_int) -> bool {
        unsafe { (self.mouse_button)(self.inspector, action, button, mods) }
    }

    pub fn mouse_pos(&self, x: f64, y: f64) -> bool {
        unsafe { (self.mouse_pos)(self.inspector, x, y) }
    }

    pub fn mouse_scroll(&self, x: f64, y: f64, scroll_mods: c_int) -> bool {
        unsafe { (self.mouse_scroll)(self.inspector, x, y, scroll_mods) }
    }

    pub fn key(&self, action: c_int, key: c_int, mods: c_int) -> bool {
        unsafe { (self.key)(self.inspector, action, key, mods) }
    }

    pub fn text(&self, text: &str) -> bool {
        let Ok(text) = CString::new(text) else {
            return false;
        };
        unsafe { (self.text)(self.inspector, text.as_ptr()) }
    }
}

impl Drop for GhosttyInspectorGuard {
    fn drop(&mut self) {
        if !self.inspector.is_null() {
            unsafe {
                let _ = self.shutdown_opengl();
                (self.free)(self.inspector);
            }
        }
    }
}

impl Drop for GhosttySurfaceGuard {
    fn drop(&mut self) {
        if !self.surface.is_null() {
            self.display_unrealized();
            unsafe {
                (self.free)(self.surface);
            }
        }
    }
}

#[derive(Debug, Clone)]
pub struct GhosttyHostCheck {
    pub library: PathBuf,
    pub must_draw_from_app_thread: bool,
}

pub fn host_check() -> Result<GhosttyHostCheck> {
    let library = GhosttyLibrary::open_discovered()?;
    host_check_with_library(library)
}

pub fn host_check_for_library(path: &Path) -> Result<GhosttyHostCheck> {
    let library = unsafe { GhosttyLibrary::open(path)? };
    host_check_with_library(library)
}

pub fn embedding_info_report_for_library(path: &Path) -> Result<GhosttyEmbeddingInfoReport> {
    let library = unsafe { GhosttyLibrary::open(path)? };
    library.embedding_info_report()
}

pub fn embedding_info_for_library(path: &Path) -> Result<GhosttyEmbeddingInfo> {
    Ok(embedding_info_report_for_library(path)?.info())
}

fn host_check_with_library(library: GhosttyLibrary) -> Result<GhosttyHostCheck> {
    library.initialize()?;
    let config = library.load_default_config()?;
    let app = library.create_app(&config)?;
    let must_draw_from_app_thread = app.must_draw_from_app_thread();
    if !app.tick() {
        return Err(anyhow!("ghostty_app_tick returned false"));
    }
    Ok(GhosttyHostCheck {
        library: library.path().to_path_buf(),
        must_draw_from_app_thread,
    })
}

pub fn verify_library_loadable(path: &Path) -> Result<()> {
    let _library = unsafe { GhosttyLibrary::open(path)? };
    Ok(())
}

fn default_runtime_config() -> GhosttyRuntimeConfig {
    GhosttyRuntimeConfig {
        userdata: ptr::null_mut(),
        supports_selection_clipboard: false,
        wakeup_cb: runtime_wakeup,
        action_cb: runtime_action,
        read_clipboard_cb: runtime_read_clipboard,
        confirm_read_clipboard_cb: runtime_confirm_read_clipboard,
        write_clipboard_cb: runtime_write_clipboard,
        close_surface_cb: Some(runtime_close_surface),
        redraw_surface_cb: Some(runtime_redraw_surface),
    }
}

pub fn validate_surface_env_var_count(count: usize) -> Result<()> {
    if count > GHOSTTY_SURFACE_MAX_ENV_VARS {
        return Err(anyhow!(
            "Ghostty surface environment variable count {count} exceeds maximum {GHOSTTY_SURFACE_MAX_ENV_VARS}"
        ));
    }
    Ok(())
}

fn binding_action_c_string(action: &str) -> Result<CString> {
    CString::new(action).context("Ghostty binding action contained NUL")
}

fn initialize_ghostty(ghostty_init: GhosttyInit) -> Result<()> {
    let result = unsafe { ghostty_init(0, ptr::null()) };
    if result == 0 {
        Ok(())
    } else {
        Err(anyhow!("ghostty_init failed with status {result}"))
    }
}

unsafe extern "C" fn runtime_wakeup(_userdata: *mut c_void) {}

unsafe extern "C" fn runtime_action(
    _app: GhosttyApp,
    _target: GhosttyTarget,
    _action: GhosttyAction,
) -> bool {
    false
}

unsafe extern "C" fn runtime_read_clipboard(
    _userdata: *mut c_void,
    _clipboard: c_int,
    _request: *mut c_void,
) -> bool {
    false
}

unsafe extern "C" fn runtime_confirm_read_clipboard(
    _userdata: *mut c_void,
    _text: *const c_char,
    _request: *mut c_void,
    _request_type: c_int,
) {
}

unsafe extern "C" fn runtime_write_clipboard(
    _userdata: *mut c_void,
    _clipboard: c_int,
    _contents: *const GhosttyClipboardContent,
    _len: usize,
    _confirm: bool,
) {
}

unsafe extern "C" fn runtime_close_surface(_userdata: *mut c_void, _process_alive: bool) {}

unsafe extern "C" fn runtime_redraw_surface(_userdata: *mut c_void) {}

unsafe fn load_symbol<T: Copy>(handle: *mut c_void, name: &str) -> Result<T> {
    let raw_name = CString::new(name).expect("symbol names are static");
    let symbol = dlsym(handle, raw_name.as_ptr());
    if symbol.is_null() {
        return Err(anyhow!("missing symbol {name}: {}", dl_error()));
    }
    Ok(std::mem::transmute_copy(&symbol))
}

pub fn load_and_finalize_default_config(
    config: GhosttyConfig,
    load_default_files: GhosttyConfigLoadDefaultFiles,
    load_recursive_files: GhosttyConfigLoadRecursiveFiles,
    finalize: GhosttyConfigFinalize,
    diagnostics_count: GhosttyConfigDiagnosticsCount,
    get_diagnostic: GhosttyConfigGetDiagnostic,
) -> Result<()> {
    load_and_finalize_default_config_with_override(
        config,
        load_default_files,
        None,
        None,
        load_recursive_files,
        finalize,
        diagnostics_count,
        get_diagnostic,
    )
}

fn load_and_finalize_default_config_with_override(
    config: GhosttyConfig,
    load_default_files: GhosttyConfigLoadDefaultFiles,
    override_contents: Option<&str>,
    load_string: Option<GhosttyConfigLoadString>,
    load_recursive_files: GhosttyConfigLoadRecursiveFiles,
    finalize: GhosttyConfigFinalize,
    diagnostics_count: GhosttyConfigDiagnosticsCount,
    get_diagnostic: GhosttyConfigGetDiagnostic,
) -> Result<()> {
    unsafe {
        if !load_default_files(config) {
            return Err(ghostty_config_error(
                "ghostty_config_load_default_files failed",
                config,
                diagnostics_count,
                get_diagnostic,
            ));
        }
        if let Some(contents) = override_contents {
            let load_string = load_string
                .ok_or_else(|| anyhow!("Ghostty config override loader is unavailable"))?;
            if !load_string(config, contents.as_ptr() as *const c_char, contents.len()) {
                return Err(ghostty_config_error(
                    "ghostty_config_load_string failed",
                    config,
                    diagnostics_count,
                    get_diagnostic,
                ));
            }
        }
        if !load_recursive_files(config) {
            return Err(ghostty_config_error(
                "ghostty_config_load_recursive_files failed",
                config,
                diagnostics_count,
                get_diagnostic,
            ));
        }
        if !finalize(config) {
            return Err(ghostty_config_error(
                "ghostty_config_finalize failed",
                config,
                diagnostics_count,
                get_diagnostic,
            ));
        }
    }
    Ok(())
}

fn load_and_finalize_config_file(
    config: GhosttyConfig,
    path: &Path,
    load_file: GhosttyConfigLoadFile,
    load_recursive_files: GhosttyConfigLoadRecursiveFiles,
    finalize: GhosttyConfigFinalize,
    diagnostics_count: GhosttyConfigDiagnosticsCount,
    get_diagnostic: GhosttyConfigGetDiagnostic,
) -> Result<()> {
    let path = CString::new(path.as_os_str().as_bytes())
        .context("ghostty config file path contains an interior NUL byte")?;
    unsafe {
        if !load_file(config, path.as_ptr()) {
            return Err(ghostty_config_error(
                "ghostty_config_load_file failed",
                config,
                diagnostics_count,
                get_diagnostic,
            ));
        }
        if !load_recursive_files(config) {
            return Err(ghostty_config_error(
                "ghostty_config_load_recursive_files failed",
                config,
                diagnostics_count,
                get_diagnostic,
            ));
        }
        if !finalize(config) {
            return Err(ghostty_config_error(
                "ghostty_config_finalize failed",
                config,
                diagnostics_count,
                get_diagnostic,
            ));
        }
    }
    Ok(())
}

fn load_and_finalize_config_string(
    config: GhosttyConfig,
    contents: &str,
    load_string: GhosttyConfigLoadString,
    load_recursive_files: GhosttyConfigLoadRecursiveFiles,
    finalize: GhosttyConfigFinalize,
    diagnostics_count: GhosttyConfigDiagnosticsCount,
    get_diagnostic: GhosttyConfigGetDiagnostic,
) -> Result<()> {
    unsafe {
        if !load_string(config, contents.as_ptr() as *const c_char, contents.len()) {
            return Err(ghostty_config_error(
                "ghostty_config_load_string failed",
                config,
                diagnostics_count,
                get_diagnostic,
            ));
        }
        if !load_recursive_files(config) {
            return Err(ghostty_config_error(
                "ghostty_config_load_recursive_files failed",
                config,
                diagnostics_count,
                get_diagnostic,
            ));
        }
        if !finalize(config) {
            return Err(ghostty_config_error(
                "ghostty_config_finalize failed",
                config,
                diagnostics_count,
                get_diagnostic,
            ));
        }
    }
    Ok(())
}

fn ghostty_config_error(
    message: &str,
    config: GhosttyConfig,
    diagnostics_count: GhosttyConfigDiagnosticsCount,
    get_diagnostic: GhosttyConfigGetDiagnostic,
) -> anyhow::Error {
    let diagnostics = ghostty_config_diagnostics(config, diagnostics_count, get_diagnostic);
    if diagnostics.is_empty() {
        anyhow!("{message}")
    } else {
        anyhow!("{message}: {}", diagnostics.join("; "))
    }
}

fn ghostty_config_diagnostics(
    config: GhosttyConfig,
    diagnostics_count: GhosttyConfigDiagnosticsCount,
    get_diagnostic: GhosttyConfigGetDiagnostic,
) -> Vec<String> {
    let count = unsafe { diagnostics_count(config) };
    let capped = count.min(32);
    let mut diagnostics = Vec::with_capacity(capped as usize);
    for idx in 0..capped {
        let diagnostic = unsafe { get_diagnostic(config, idx) };
        if diagnostic.message.is_null() {
            continue;
        }
        let message = unsafe { CStr::from_ptr(diagnostic.message) }
            .to_string_lossy()
            .trim()
            .to_string();
        if !message.is_empty() {
            diagnostics.push(message);
        }
    }
    diagnostics
}

fn dl_error() -> String {
    let error = unsafe { dlerror() };
    if error.is_null() {
        "unknown dynamic loader error".to_string()
    } else {
        unsafe { CStr::from_ptr(error) }
            .to_string_lossy()
            .to_string()
    }
}

pub fn discover_library() -> Option<PathBuf> {
    if let Some(path) = normalized_env_path("CMUX_GHOSTTY_LIBRARY") {
        if path.exists() {
            return Some(path);
        }
    }
    let root = ghostty_root()?;
    ghostty_library(&root)
}

fn ghostty_root() -> Option<PathBuf> {
    if let Some(root) = normalized_env_path("CMUX_GHOSTTY_ROOT") {
        if root.exists() {
            return Some(root);
        }
    }

    if let Some(path) = normalized_env_path("CMUX_GHOSTTY_LIBRARY") {
        if let Some(root) = ghostty_root_from_library_path(&path) {
            return Some(root);
        }
    }

    let mut current = std::env::current_dir().ok()?;
    loop {
        let candidate = current.join("ghostty");
        if candidate.join("include/ghostty.h").exists() {
            return Some(candidate);
        }
        if !current.pop() {
            break;
        }
    }
    None
}

fn ghostty_library(root: &Path) -> Option<PathBuf> {
    [
        root.join("zig-out/lib/libghostty-internal.so"),
        root.join("zig-out/lib/ghostty-internal.so"),
        root.join("zig-out/lib/libghostty.so"),
        root.join("zig-out/lib/ghostty-internal.dylib"),
        root.join("lib/libghostty-internal.so"),
        root.join("lib/ghostty-internal.so"),
    ]
    .into_iter()
    .find(|path| path.exists())
}

fn ghostty_root_from_library_path(path: &Path) -> Option<PathBuf> {
    let root = path.parent()?.parent()?;
    ghostty_root_candidate(root).or_else(|| {
        if root.file_name().and_then(|name| name.to_str()) == Some("zig-out") {
            root.parent().and_then(ghostty_root_candidate)
        } else {
            None
        }
    })
}

fn ghostty_root_candidate(root: &Path) -> Option<PathBuf> {
    if root.join("include/ghostty.h").exists() {
        Some(root.to_path_buf())
    } else {
        None
    }
}

fn normalized_env_path(key: &str) -> Option<PathBuf> {
    let value = std::env::var(key).ok()?;
    let trimmed = value.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(PathBuf::from(trimmed))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeSet;
    use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
    use std::sync::Mutex;

    static TEST_STRING_FREE_COUNT: AtomicUsize = AtomicUsize::new(0);
    static TEST_GHOSTTY_INIT_LOCK: Mutex<()> = Mutex::new(());
    static TEST_GHOSTTY_INIT_COUNT: AtomicUsize = AtomicUsize::new(0);
    static TEST_GHOSTTY_INIT_ARGC: AtomicUsize = AtomicUsize::new(usize::MAX);
    static TEST_GHOSTTY_INIT_ARGV_NULL: AtomicBool = AtomicBool::new(false);

    unsafe extern "C" fn test_ghostty_init_success(
        argc: usize,
        argv: *const *const c_char,
    ) -> c_int {
        TEST_GHOSTTY_INIT_COUNT.fetch_add(1, Ordering::SeqCst);
        TEST_GHOSTTY_INIT_ARGC.store(argc, Ordering::SeqCst);
        TEST_GHOSTTY_INIT_ARGV_NULL.store(argv.is_null(), Ordering::SeqCst);
        0
    }

    unsafe extern "C" fn test_ghostty_init_failure(
        argc: usize,
        argv: *const *const c_char,
    ) -> c_int {
        TEST_GHOSTTY_INIT_COUNT.fetch_add(1, Ordering::SeqCst);
        TEST_GHOSTTY_INIT_ARGC.store(argc, Ordering::SeqCst);
        TEST_GHOSTTY_INIT_ARGV_NULL.store(argv.is_null(), Ordering::SeqCst);
        7
    }

    fn reset_ghostty_init_test_state() {
        TEST_GHOSTTY_INIT_COUNT.store(0, Ordering::SeqCst);
        TEST_GHOSTTY_INIT_ARGC.store(usize::MAX, Ordering::SeqCst);
        TEST_GHOSTTY_INIT_ARGV_NULL.store(false, Ordering::SeqCst);
    }

    #[test]
    fn ghostty_initialize_uses_empty_argv_fallback() {
        let _lock = TEST_GHOSTTY_INIT_LOCK
            .lock()
            .expect("ghostty init test lock");
        reset_ghostty_init_test_state();

        initialize_ghostty(test_ghostty_init_success).expect("ghostty init");

        assert_eq!(TEST_GHOSTTY_INIT_COUNT.load(Ordering::SeqCst), 1);
        assert_eq!(TEST_GHOSTTY_INIT_ARGC.load(Ordering::SeqCst), 0);
        assert!(TEST_GHOSTTY_INIT_ARGV_NULL.load(Ordering::SeqCst));
    }

    #[test]
    fn ghostty_initialize_does_not_cache_failures() {
        let _lock = TEST_GHOSTTY_INIT_LOCK
            .lock()
            .expect("ghostty init test lock");
        reset_ghostty_init_test_state();

        let err = initialize_ghostty(test_ghostty_init_failure).expect_err("failed init");
        assert!(err.to_string().contains("status 7"));
        initialize_ghostty(test_ghostty_init_success).expect("retry succeeds");

        assert_eq!(TEST_GHOSTTY_INIT_COUNT.load(Ordering::SeqCst), 2);
        assert_eq!(TEST_GHOSTTY_INIT_ARGC.load(Ordering::SeqCst), 0);
        assert!(TEST_GHOSTTY_INIT_ARGV_NULL.load(Ordering::SeqCst));
    }

    #[test]
    fn ghostty_required_symbols_match_dynamic_loader_symbols() {
        let required = REQUIRED_GHOSTTY_EMBED_SYMBOLS
            .iter()
            .copied()
            .collect::<BTreeSet<_>>();
        let loaded = ghostty_load_symbol_names_from_source(include_str!("ghostty_embed.rs"));

        assert_eq!(
            loaded, required,
            "Ghostty diagnostics must require every symbol loaded by GhosttyLibrary::open"
        );
    }

    #[test]
    fn gtk_ghostty_shared_app_keeps_callback_userdata_alive_through_app_drop() {
        let source = include_str!("gtk_ghostty.rs");
        let fields = source
            .split_once("struct GtkGhosttyApp {")
            .and_then(|(_, body)| body.split_once("\n}\n\nimpl GtkGhosttyApp"))
            .map(|(fields, _)| fields)
            .expect("GtkGhosttyApp declaration should be present");
        let app = fields
            .find("    app: GhosttyAppGuard,")
            .expect("app field should be present");
        let config = fields
            .find("    _config: GhosttyConfigGuard,")
            .expect("config field should be present");
        let callbacks = fields
            .find("    callbacks: Box<GtkGhosttyAppCallbacks>,")
            .expect("callback userdata field should be present");
        let library = fields
            .find("    library: GhosttyLibrary,")
            .expect("library field should be present");

        assert!(app < config);
        assert!(config < callbacks);
        assert!(callbacks < library);
    }

    #[test]
    fn gtk_ghostty_host_rotates_tokens_after_surface_teardown() {
        let source = include_str!("gtk_ghostty.rs");
        assert_eq!(
            source
                .matches("rotate_ghostty_callback_registration(self.callbacks.as_mut());")
                .count(),
            2,
            "failed realization and normal surface release must invalidate queued callbacks"
        );
        let rotate = source
            .split_once("fn rotate_ghostty_callback_registration(")
            .and_then(|(_, body)| body.split_once("\n}"))
            .map(|(body, _)| body)
            .expect("callback registration rotation helper should be present");
        let unregister = rotate
            .find("\n    unregister_ghostty_callbacks(callbacks);")
            .expect("old callback generation should be unregistered");
        let token = rotate
            .find("\n    callbacks.token = next_ghostty_callback_token();")
            .expect("callback generation should receive a fresh token");
        let register = rotate
            .find("\n    register_ghostty_callbacks(callbacks);")
            .expect("new callback generation should be registered");
        assert!(unregister < token);
        assert!(token < register);
    }

    fn ghostty_load_symbol_names_from_source(source: &'static str) -> BTreeSet<&'static str> {
        let mut names = BTreeSet::new();
        let mut rest = source
            .split_once("let library = Self {")
            .and_then(|(_, loader)| loader.split_once("handle_guard.disarm();"))
            .map(|(loader, _)| loader)
            .expect("GhosttyLibrary::open loader block should be present");
        while let Some(index) = rest.find("load_symbol(") {
            rest = &rest[index + "load_symbol(".len()..];
            let Some(quote_start) = rest.find('"') else {
                break;
            };
            let value = &rest[quote_start + 1..];
            let Some(quote_end) = value.find('"') else {
                break;
            };
            let symbol = &value[..quote_end];
            if symbol.starts_with("ghostty_") {
                names.insert(symbol);
            }
            rest = &value[quote_end + 1..];
        }
        names
    }

    #[test]
    fn ghostty_surface_config_forwards_initial_output_and_size() {
        let mut config = unsafe { std::mem::zeroed::<GhosttySurfaceConfig>() };
        let output = b"restored terminal output\n";
        config.set_initial_output(output);
        config.set_initial_size(640, 480);
        assert_eq!(config.initial_output, output.as_ptr() as *const c_char);
        assert_eq!(config.initial_output_len, output.len());
        assert_eq!(config.initial_width_px, 640);
        assert_eq!(config.initial_height_px, 480);
    }

    #[test]
    fn ghostty_embed_c_layout_matches_header() {
        assert_eq!(std::mem::size_of::<GhosttyTarget>(), 16);
        assert_eq!(std::mem::align_of::<GhosttyTarget>(), 8);
        assert_eq!(std::mem::size_of::<GhosttyActionDesktopNotification>(), 16);
        assert_eq!(std::mem::size_of::<GhosttyActionSetTitle>(), 8);
        assert_eq!(std::mem::size_of::<GhosttyActionPwd>(), 8);
        assert_eq!(std::mem::size_of::<GhosttyActionMouseOverLink>(), 16);
        assert_eq!(std::mem::size_of::<GhosttyActionOpenUrl>(), 24);
        assert_eq!(std::mem::size_of::<GhosttyActionProgressReport>(), 8);
        assert_eq!(std::mem::align_of::<GhosttyActionProgressReport>(), 4);
        assert_eq!(std::mem::size_of::<GhosttyActionChildExited>(), 16);
        assert_eq!(std::mem::align_of::<GhosttyActionChildExited>(), 8);
        assert_eq!(std::mem::size_of::<GhosttyActionCommandFinished>(), 16);
        assert_eq!(std::mem::align_of::<GhosttyActionCommandFinished>(), 8);
        assert_eq!(std::mem::size_of::<GhosttyActionStartSearch>(), 8);
        assert_eq!(std::mem::align_of::<GhosttyActionStartSearch>(), 8);
        assert_eq!(std::mem::size_of::<GhosttyActionSearchTotal>(), 8);
        assert_eq!(std::mem::align_of::<GhosttyActionSearchTotal>(), 8);
        assert_eq!(std::mem::size_of::<GhosttyActionSearchSelected>(), 8);
        assert_eq!(std::mem::align_of::<GhosttyActionSearchSelected>(), 8);
        assert_eq!(std::mem::size_of::<GhosttyActionScrollbar>(), 24);
        assert_eq!(std::mem::align_of::<GhosttyActionScrollbar>(), 8);
        assert_eq!(std::mem::size_of::<GhosttyInputTriggerKey>(), 4);
        assert_eq!(std::mem::align_of::<GhosttyInputTriggerKey>(), 4);
        assert_eq!(std::mem::size_of::<GhosttyInputTrigger>(), 12);
        assert_eq!(std::mem::align_of::<GhosttyInputTrigger>(), 4);
        assert_eq!(std::mem::size_of::<GhosttyActionKeySequence>(), 16);
        assert_eq!(std::mem::align_of::<GhosttyActionKeySequence>(), 4);
        assert_eq!(std::mem::size_of::<GhosttyActionKeyTableActivate>(), 16);
        assert_eq!(std::mem::align_of::<GhosttyActionKeyTableActivate>(), 8);
        assert_eq!(std::mem::size_of::<GhosttyActionKeyTableValue>(), 16);
        assert_eq!(std::mem::align_of::<GhosttyActionKeyTableValue>(), 8);
        assert_eq!(std::mem::size_of::<GhosttyActionKeyTable>(), 24);
        assert_eq!(std::mem::align_of::<GhosttyActionKeyTable>(), 8);
        assert_eq!(std::mem::size_of::<GhosttyActionMoveTab>(), 8);
        assert_eq!(std::mem::align_of::<GhosttyActionMoveTab>(), 8);
        assert_eq!(std::mem::size_of::<GhosttyActionSizeLimit>(), 16);
        assert_eq!(std::mem::align_of::<GhosttyActionSizeLimit>(), 4);
        assert_eq!(std::mem::size_of::<GhosttyActionInitialSize>(), 8);
        assert_eq!(std::mem::align_of::<GhosttyActionInitialSize>(), 4);
        assert_eq!(std::mem::size_of::<GhosttyActionCellSize>(), 8);
        assert_eq!(std::mem::align_of::<GhosttyActionCellSize>(), 4);
        assert_eq!(std::mem::size_of::<GhosttyActionColorChange>(), 8);
        assert_eq!(std::mem::align_of::<GhosttyActionColorChange>(), 4);
        assert_eq!(std::mem::size_of::<GhosttyActionConfigChange>(), 8);
        assert_eq!(std::mem::align_of::<GhosttyActionConfigChange>(), 8);
        assert_eq!(std::mem::size_of::<GhosttyActionResizeSplit>(), 8);
        assert_eq!(std::mem::align_of::<GhosttyActionResizeSplit>(), 4);
        assert_eq!(std::mem::size_of::<GhosttyActionPayload>(), 24);
        assert_eq!(std::mem::align_of::<GhosttyActionPayload>(), 8);
        assert_eq!(std::mem::size_of::<GhosttyAction>(), 32);
        assert_eq!(std::mem::align_of::<GhosttyAction>(), 8);
        assert_eq!(std::mem::size_of::<GhosttyIpcTargetPayload>(), 8);
        assert_eq!(std::mem::align_of::<GhosttyIpcTargetPayload>(), 8);
        assert_eq!(std::mem::size_of::<GhosttyIpcTarget>(), 16);
        assert_eq!(std::mem::align_of::<GhosttyIpcTarget>(), 8);
        assert_eq!(std::mem::offset_of!(GhosttyIpcTarget, tag), 0);
        assert_eq!(std::mem::offset_of!(GhosttyIpcTarget, target), 8);
        assert_eq!(std::mem::size_of::<GhosttyIpcActionNewWindow>(), 8);
        assert_eq!(std::mem::align_of::<GhosttyIpcActionNewWindow>(), 8);
        assert_eq!(
            std::mem::offset_of!(GhosttyIpcActionNewWindow, arguments),
            0
        );
        assert_eq!(std::mem::size_of::<GhosttyIpcActionPayload>(), 8);
        assert_eq!(std::mem::align_of::<GhosttyIpcActionPayload>(), 8);
        assert_eq!(std::mem::size_of::<GhosttyIpcAction>(), 16);
        assert_eq!(std::mem::align_of::<GhosttyIpcAction>(), 8);
        assert_eq!(std::mem::offset_of!(GhosttyIpcAction, tag), 0);
        assert_eq!(std::mem::offset_of!(GhosttyIpcAction, action), 8);
        assert_eq!(std::mem::size_of::<GhosttyRuntimeConfig>(), 72);
        assert_eq!(std::mem::align_of::<GhosttyRuntimeConfig>(), 8);
        assert_eq!(std::mem::offset_of!(GhosttyRuntimeConfig, userdata), 0);
        assert_eq!(
            std::mem::offset_of!(GhosttyRuntimeConfig, supports_selection_clipboard),
            8
        );
        assert_eq!(std::mem::offset_of!(GhosttyRuntimeConfig, wakeup_cb), 16);
        assert_eq!(std::mem::offset_of!(GhosttyRuntimeConfig, action_cb), 24);
        assert_eq!(
            std::mem::offset_of!(GhosttyRuntimeConfig, read_clipboard_cb),
            32
        );
        assert_eq!(
            std::mem::offset_of!(GhosttyRuntimeConfig, confirm_read_clipboard_cb),
            40
        );
        assert_eq!(
            std::mem::offset_of!(GhosttyRuntimeConfig, write_clipboard_cb),
            48
        );
        assert_eq!(
            std::mem::offset_of!(GhosttyRuntimeConfig, close_surface_cb),
            56
        );
        assert_eq!(
            std::mem::offset_of!(GhosttyRuntimeConfig, redraw_surface_cb),
            64
        );
        assert_eq!(std::mem::size_of::<GhosttyDiagnostic>(), 8);
        assert_eq!(std::mem::align_of::<GhosttyDiagnostic>(), 8);
        assert_eq!(std::mem::size_of::<GhosttyPlatformLinux>(), 32);
        assert_eq!(std::mem::align_of::<GhosttyPlatformLinux>(), 8);
        assert_eq!(std::mem::offset_of!(GhosttyPlatformLinux, userdata), 0);
        assert_eq!(std::mem::offset_of!(GhosttyPlatformLinux, make_current), 8);
        assert_eq!(
            std::mem::offset_of!(GhosttyPlatformLinux, get_proc_address),
            16
        );
        assert_eq!(std::mem::offset_of!(GhosttyPlatformLinux, done_current), 24);
        assert_eq!(std::mem::size_of::<GhosttySurfaceConfig>(), 160);
        assert_eq!(std::mem::align_of::<GhosttySurfaceConfig>(), 8);
        assert_eq!(std::mem::offset_of!(GhosttySurfaceConfig, platform_tag), 0);
        assert_eq!(std::mem::offset_of!(GhosttySurfaceConfig, platform), 8);
        assert_eq!(std::mem::offset_of!(GhosttySurfaceConfig, userdata), 40);
        assert_eq!(std::mem::offset_of!(GhosttySurfaceConfig, scale_factor), 48);
        assert_eq!(std::mem::offset_of!(GhosttySurfaceConfig, font_size), 56);
        assert_eq!(
            std::mem::offset_of!(GhosttySurfaceConfig, working_directory),
            64
        );
        assert_eq!(std::mem::offset_of!(GhosttySurfaceConfig, command), 72);
        assert_eq!(std::mem::offset_of!(GhosttySurfaceConfig, env_vars), 80);
        assert_eq!(
            std::mem::offset_of!(GhosttySurfaceConfig, env_var_count),
            88
        );
        assert_eq!(
            std::mem::offset_of!(GhosttySurfaceConfig, initial_input),
            96
        );
        assert_eq!(
            std::mem::offset_of!(GhosttySurfaceConfig, wait_after_command),
            104
        );
        assert_eq!(std::mem::offset_of!(GhosttySurfaceConfig, context), 108);
        assert_eq!(std::mem::offset_of!(GhosttySurfaceConfig, io_mode), 112);
        assert_eq!(std::mem::offset_of!(GhosttySurfaceConfig, io_write_cb), 120);
        assert_eq!(
            std::mem::offset_of!(GhosttySurfaceConfig, io_write_userdata),
            128
        );
        assert_eq!(
            std::mem::offset_of!(GhosttySurfaceConfig, initial_output),
            136
        );
        assert_eq!(
            std::mem::offset_of!(GhosttySurfaceConfig, initial_output_len),
            144
        );
        assert_eq!(
            std::mem::offset_of!(GhosttySurfaceConfig, initial_width_px),
            152
        );
        assert_eq!(
            std::mem::offset_of!(GhosttySurfaceConfig, initial_height_px),
            156
        );
        assert_eq!(std::mem::size_of::<GhosttyInputKey>(), 32);
        assert_eq!(std::mem::align_of::<GhosttyInputKey>(), 8);
        assert_eq!(std::mem::offset_of!(GhosttyInputKey, action), 0);
        assert_eq!(std::mem::offset_of!(GhosttyInputKey, mods), 4);
        assert_eq!(std::mem::offset_of!(GhosttyInputKey, consumed_mods), 8);
        assert_eq!(std::mem::offset_of!(GhosttyInputKey, keycode), 12);
        assert_eq!(std::mem::offset_of!(GhosttyInputKey, text), 16);
        assert_eq!(
            std::mem::offset_of!(GhosttyInputKey, unshifted_codepoint),
            24
        );
        assert_eq!(std::mem::offset_of!(GhosttyInputKey, composing), 28);
        assert_eq!(std::mem::size_of::<GhosttySurfaceSizeResult>(), 20);
        assert_eq!(std::mem::align_of::<GhosttySurfaceSizeResult>(), 4);
        assert_eq!(std::mem::offset_of!(GhosttySurfaceSizeResult, columns), 0);
        assert_eq!(std::mem::offset_of!(GhosttySurfaceSizeResult, rows), 2);
        assert_eq!(std::mem::offset_of!(GhosttySurfaceSizeResult, width_px), 4);
        assert_eq!(std::mem::offset_of!(GhosttySurfaceSizeResult, height_px), 8);
        assert_eq!(
            std::mem::offset_of!(GhosttySurfaceSizeResult, cell_width_px),
            12
        );
        assert_eq!(
            std::mem::offset_of!(GhosttySurfaceSizeResult, cell_height_px),
            16
        );
        assert_eq!(std::mem::size_of::<GhosttyPoint>(), 16);
        assert_eq!(std::mem::align_of::<GhosttyPoint>(), 4);
        assert_eq!(std::mem::offset_of!(GhosttyPoint, tag), 0);
        assert_eq!(std::mem::offset_of!(GhosttyPoint, coord), 4);
        assert_eq!(std::mem::offset_of!(GhosttyPoint, x), 8);
        assert_eq!(std::mem::offset_of!(GhosttyPoint, y), 12);
        assert_eq!(std::mem::size_of::<GhosttySelection>(), 36);
        assert_eq!(std::mem::align_of::<GhosttySelection>(), 4);
        assert_eq!(std::mem::offset_of!(GhosttySelection, top_left), 0);
        assert_eq!(std::mem::offset_of!(GhosttySelection, bottom_right), 16);
        assert_eq!(std::mem::offset_of!(GhosttySelection, rectangle), 32);
        assert_eq!(std::mem::size_of::<GhosttyText>(), 40);
        assert_eq!(std::mem::align_of::<GhosttyText>(), 8);
        assert_eq!(std::mem::offset_of!(GhosttyText, tl_px_x), 0);
        assert_eq!(std::mem::offset_of!(GhosttyText, tl_px_y), 8);
        assert_eq!(std::mem::offset_of!(GhosttyText, offset_start), 16);
        assert_eq!(std::mem::offset_of!(GhosttyText, offset_len), 20);
        assert_eq!(std::mem::offset_of!(GhosttyText, text), 24);
        assert_eq!(std::mem::offset_of!(GhosttyText, text_len), 32);
        assert_eq!(std::mem::size_of::<GhosttyClipboardContent>(), 16);
        assert_eq!(std::mem::align_of::<GhosttyClipboardContent>(), 8);
        assert_eq!(std::mem::size_of::<GhosttyEmbeddingInfo>(), 304);
        assert_eq!(std::mem::align_of::<GhosttyEmbeddingInfo>(), 8);
        assert_eq!(std::mem::offset_of!(GhosttyEmbeddingInfo, abi_version), 0);
        assert_eq!(std::mem::offset_of!(GhosttyEmbeddingInfo, platform), 4);
        assert_eq!(
            std::mem::offset_of!(GhosttyEmbeddingInfo, renderer_backend),
            8
        );
        assert_eq!(
            std::mem::offset_of!(GhosttyEmbeddingInfo, surface_max_env_vars),
            16
        );
        assert_eq!(
            std::mem::offset_of!(GhosttyEmbeddingInfo, supports_linux_platform),
            24
        );
        assert_eq!(
            std::mem::offset_of!(GhosttyEmbeddingInfo, must_draw_from_app_thread),
            25
        );
        assert_eq!(
            std::mem::offset_of!(GhosttyEmbeddingInfo, ipc_target_size),
            144
        );
        assert_eq!(
            std::mem::offset_of!(GhosttyEmbeddingInfo, ipc_action_size),
            152
        );
        assert_eq!(
            std::mem::offset_of!(GhosttyEmbeddingInfo, ipc_target_align),
            272
        );
        assert_eq!(
            std::mem::offset_of!(GhosttyEmbeddingInfo, ipc_action_align),
            280
        );
        assert_eq!(
            std::mem::offset_of!(GhosttyEmbeddingInfo, layout_fingerprint),
            288
        );
        assert_eq!(
            std::mem::offset_of!(GhosttyEmbeddingInfo, constants_fingerprint),
            296
        );
    }

    #[test]
    fn ghostty_surface_env_var_count_matches_header_bound() {
        assert!(validate_surface_env_var_count(0).is_ok());
        assert!(validate_surface_env_var_count(GHOSTTY_SURFACE_MAX_ENV_VARS).is_ok());

        let error = validate_surface_env_var_count(GHOSTTY_SURFACE_MAX_ENV_VARS + 1).unwrap_err();
        let message = error.to_string();
        assert!(message.contains("environment variable count"));
        assert!(message.contains(&GHOSTTY_SURFACE_MAX_ENV_VARS.to_string()));
    }

    #[test]
    fn ghostty_cursor_constants_match_header_order() {
        assert_eq!(GHOSTTY_EMBEDDING_ABI_VERSION, 15);
        assert_eq!(GHOSTTY_RENDERER_BACKEND_UNKNOWN, 0);
        assert_eq!(GHOSTTY_RENDERER_BACKEND_OPENGL, 1);
        assert_eq!(GHOSTTY_RENDERER_BACKEND_METAL, 2);
        assert_eq!(GHOSTTY_RENDERER_BACKEND_WEBGL, 3);
        assert_eq!(GHOSTTY_CLIPBOARD_REQUEST_PASTE, 0);
        assert_eq!(GHOSTTY_CLIPBOARD_REQUEST_OSC_52_READ, 1);
        assert_eq!(GHOSTTY_CLIPBOARD_REQUEST_OSC_52_WRITE, 2);
        assert_eq!(GHOSTTY_SURFACE_CONTEXT_WINDOW, 0);
        assert_eq!(GHOSTTY_SURFACE_CONTEXT_TAB, 1);
        assert_eq!(GHOSTTY_SURFACE_CONTEXT_SPLIT, 2);
        assert_eq!(GHOSTTY_SURFACE_IO_EXEC, 0);
        assert_eq!(GHOSTTY_SURFACE_IO_MANUAL, 1);
        assert_eq!(GHOSTTY_ACTION_QUIT, 0);
        assert_eq!(GHOSTTY_ACTION_NEW_WINDOW, 1);
        assert_eq!(GHOSTTY_IPC_TARGET_CLASS, 0);
        assert_eq!(GHOSTTY_IPC_TARGET_DETECT, 1);
        assert_eq!(GHOSTTY_IPC_ACTION_NEW_WINDOW, 0);
        assert_eq!(GHOSTTY_IPC_ACTION_TOGGLE_QUICK_TERMINAL, 1);
        assert_eq!(GHOSTTY_ACTION_REPEAT, 2);
        assert_eq!(GHOSTTY_INPUT_KEYCODE_NATIVE_MASK, 0x7fff_ffff);
        assert_eq!(GHOSTTY_INPUT_KEYCODE_PHYSICAL_KEY_FLAG, 0x8000_0000);
        assert_eq!(
            ghostty_physical_keycode(GHOSTTY_KEY_F25),
            GHOSTTY_INPUT_KEYCODE_PHYSICAL_KEY_FLAG | GHOSTTY_KEY_F25 as u32
        );
        assert_eq!(GHOSTTY_ACTION_NEW_TAB, 2);
        assert_eq!(GHOSTTY_ACTION_CLOSE_TAB, 3);
        assert_eq!(GHOSTTY_MODS_SHIFT, 1 << 0);
        assert_eq!(GHOSTTY_MODS_CTRL, 1 << 1);
        assert_eq!(GHOSTTY_MODS_ALT, 1 << 2);
        assert_eq!(GHOSTTY_MODS_SUPER, 1 << 3);
        assert_eq!(GHOSTTY_MODS_CAPS, 1 << 4);
        assert_eq!(GHOSTTY_MODS_NUM, 1 << 5);
        assert_eq!(GHOSTTY_MODS_SHIFT_RIGHT, 1 << 6);
        assert_eq!(GHOSTTY_MODS_CTRL_RIGHT, 1 << 7);
        assert_eq!(GHOSTTY_MODS_ALT_RIGHT, 1 << 8);
        assert_eq!(GHOSTTY_MODS_SUPER_RIGHT, 1 << 9);
        assert_eq!(GHOSTTY_BINDING_FLAGS_CONSUMED, 1 << 0);
        assert_eq!(GHOSTTY_BINDING_FLAGS_ALL, 1 << 1);
        assert_eq!(GHOSTTY_BINDING_FLAGS_GLOBAL, 1 << 2);
        assert_eq!(GHOSTTY_BINDING_FLAGS_PERFORMABLE, 1 << 3);
        assert_eq!(GHOSTTY_ACTION_NEW_SPLIT, 4);
        assert_eq!(GHOSTTY_ACTION_CLOSE_ALL_WINDOWS, 5);
        assert_eq!(GHOSTTY_ACTION_TOGGLE_MAXIMIZE, 6);
        assert_eq!(GHOSTTY_ACTION_TOGGLE_FULLSCREEN, 7);
        assert_eq!(GHOSTTY_ACTION_TOGGLE_TAB_OVERVIEW, 8);
        assert_eq!(GHOSTTY_ACTION_TOGGLE_WINDOW_DECORATIONS, 9);
        assert_eq!(GHOSTTY_ACTION_TOGGLE_QUICK_TERMINAL, 10);
        assert_eq!(GHOSTTY_ACTION_TOGGLE_COMMAND_PALETTE, 11);
        assert_eq!(GHOSTTY_ACTION_TOGGLE_VISIBILITY, 12);
        assert_eq!(GHOSTTY_ACTION_TOGGLE_BACKGROUND_OPACITY, 13);
        assert_eq!(GHOSTTY_ACTION_MOVE_TAB, 14);
        assert_eq!(GHOSTTY_ACTION_GOTO_TAB, 15);
        assert_eq!(GHOSTTY_ACTION_GOTO_SPLIT, 16);
        assert_eq!(GHOSTTY_ACTION_GOTO_WINDOW, 17);
        assert_eq!(GHOSTTY_ACTION_RESIZE_SPLIT, 18);
        assert_eq!(GHOSTTY_ACTION_EQUALIZE_SPLITS, 19);
        assert_eq!(GHOSTTY_ACTION_TOGGLE_SPLIT_ZOOM, 20);
        assert_eq!(GHOSTTY_ACTION_PRESENT_TERMINAL, 21);
        assert_eq!(GHOSTTY_ACTION_SIZE_LIMIT, 22);
        assert_eq!(GHOSTTY_ACTION_RESET_WINDOW_SIZE, 23);
        assert_eq!(GHOSTTY_ACTION_INITIAL_SIZE, 24);
        assert_eq!(GHOSTTY_ACTION_CELL_SIZE, 25);
        assert_eq!(GHOSTTY_ACTION_SCROLLBAR, 26);
        assert_eq!(GHOSTTY_ACTION_RENDER, 27);
        assert_eq!(GHOSTTY_ACTION_INSPECTOR, 28);
        assert_eq!(GHOSTTY_ACTION_SHOW_GTK_INSPECTOR, 29);
        assert_eq!(GHOSTTY_ACTION_RENDER_INSPECTOR, 30);
        assert_eq!(GHOSTTY_ACTION_DESKTOP_NOTIFICATION, 31);
        assert_eq!(GHOSTTY_ACTION_SET_TITLE, 32);
        assert_eq!(GHOSTTY_ACTION_SET_TAB_TITLE, 33);
        assert_eq!(GHOSTTY_ACTION_PROMPT_TITLE, 34);
        assert_eq!(GHOSTTY_ACTION_PWD, 35);
        assert_eq!(GHOSTTY_ACTION_MOUSE_SHAPE, 36);
        assert_eq!(GHOSTTY_ACTION_MOUSE_VISIBILITY, 37);
        assert_eq!(GHOSTTY_ACTION_MOUSE_OVER_LINK, 38);
        assert_eq!(GHOSTTY_ACTION_RENDERER_HEALTH, 39);
        assert_eq!(GHOSTTY_ACTION_OPEN_CONFIG, 40);
        assert_eq!(GHOSTTY_ACTION_QUIT_TIMER, 41);
        assert_eq!(GHOSTTY_ACTION_FLOAT_WINDOW, 42);
        assert_eq!(GHOSTTY_ACTION_SECURE_INPUT, 43);
        assert_eq!(GHOSTTY_ACTION_KEY_SEQUENCE, 44);
        assert_eq!(GHOSTTY_ACTION_KEY_TABLE, 45);
        assert_eq!(GHOSTTY_ACTION_COLOR_CHANGE, 46);
        assert_eq!(GHOSTTY_ACTION_RELOAD_CONFIG, 47);
        assert_eq!(GHOSTTY_ACTION_CONFIG_CHANGE, 48);
        assert_eq!(GHOSTTY_ACTION_CLOSE_WINDOW, 49);
        assert_eq!(GHOSTTY_ACTION_RING_BELL, 50);
        assert_eq!(GHOSTTY_ACTION_SELECTION_CHANGED, 51);
        assert_eq!(GHOSTTY_ACTION_UNDO, 52);
        assert_eq!(GHOSTTY_ACTION_REDO, 53);
        assert_eq!(GHOSTTY_ACTION_CHECK_FOR_UPDATES, 54);
        assert_eq!(GHOSTTY_ACTION_OPEN_URL, 55);
        assert_eq!(GHOSTTY_ACTION_SHOW_CHILD_EXITED, 56);
        assert_eq!(GHOSTTY_ACTION_PROGRESS_REPORT, 57);
        assert_eq!(GHOSTTY_ACTION_SHOW_ON_SCREEN_KEYBOARD, 58);
        assert_eq!(GHOSTTY_ACTION_COMMAND_FINISHED, 59);
        assert_eq!(GHOSTTY_ACTION_START_SEARCH, 60);
        assert_eq!(GHOSTTY_ACTION_END_SEARCH, 61);
        assert_eq!(GHOSTTY_ACTION_SEARCH_TOTAL, 62);
        assert_eq!(GHOSTTY_ACTION_SEARCH_SELECTED, 63);
        assert_eq!(GHOSTTY_ACTION_READONLY, 64);
        assert_eq!(GHOSTTY_ACTION_COPY_TITLE_TO_CLIPBOARD, 65);
        assert_eq!(GHOSTTY_TRIGGER_PHYSICAL, 0);
        assert_eq!(GHOSTTY_TRIGGER_UNICODE, 1);
        assert_eq!(GHOSTTY_TRIGGER_CATCH_ALL, 2);
        assert_eq!(GHOSTTY_COLOR_SCHEME_LIGHT, 0);
        assert_eq!(GHOSTTY_COLOR_SCHEME_DARK, 1);
        assert_eq!(GHOSTTY_RENDERER_HEALTH_HEALTHY, 0);
        assert_eq!(GHOSTTY_RENDERER_HEALTH_UNHEALTHY, 1);
        assert_eq!(GHOSTTY_PROMPT_TITLE_SURFACE, 0);
        assert_eq!(GHOSTTY_PROMPT_TITLE_TAB, 1);
        assert_eq!(GHOSTTY_QUIT_TIMER_START, 0);
        assert_eq!(GHOSTTY_QUIT_TIMER_STOP, 1);
        assert_eq!(GHOSTTY_FLOAT_WINDOW_ON, 0);
        assert_eq!(GHOSTTY_FLOAT_WINDOW_OFF, 1);
        assert_eq!(GHOSTTY_FLOAT_WINDOW_TOGGLE, 2);
        assert_eq!(GHOSTTY_SECURE_INPUT_ON, 0);
        assert_eq!(GHOSTTY_SECURE_INPUT_OFF, 1);
        assert_eq!(GHOSTTY_SECURE_INPUT_TOGGLE, 2);
        assert_eq!(GHOSTTY_COLOR_KIND_FOREGROUND, -1);
        assert_eq!(GHOSTTY_COLOR_KIND_BACKGROUND, -2);
        assert_eq!(GHOSTTY_COLOR_KIND_CURSOR, -3);
        assert_eq!(GHOSTTY_FULLSCREEN_NATIVE, 0);
        assert_eq!(GHOSTTY_FULLSCREEN_MACOS_NON_NATIVE, 1);
        assert_eq!(GHOSTTY_FULLSCREEN_MACOS_NON_NATIVE_VISIBLE_MENU, 2);
        assert_eq!(GHOSTTY_FULLSCREEN_MACOS_NON_NATIVE_PADDED_NOTCH, 3);
        assert_eq!(GHOSTTY_GOTO_TAB_PREVIOUS, -1);
        assert_eq!(GHOSTTY_GOTO_TAB_NEXT, -2);
        assert_eq!(GHOSTTY_GOTO_TAB_LAST, -3);
        assert_eq!(GHOSTTY_GOTO_WINDOW_PREVIOUS, 0);
        assert_eq!(GHOSTTY_GOTO_WINDOW_NEXT, 1);
        assert_eq!(GHOSTTY_INSPECTOR_TOGGLE, 0);
        assert_eq!(GHOSTTY_INSPECTOR_SHOW, 1);
        assert_eq!(GHOSTTY_INSPECTOR_HIDE, 2);
        assert_eq!(GHOSTTY_KEY_BACKSPACE, 53);
        assert_eq!(GHOSTTY_KEY_ENTER, 58);
        assert_eq!(GHOSTTY_KEY_SPACE, 63);
        assert_eq!(GHOSTTY_KEY_TAB, 64);
        assert_eq!(GHOSTTY_KEY_DELETE, 68);
        assert_eq!(GHOSTTY_KEY_PAGE_DOWN, 73);
        assert_eq!(GHOSTTY_KEY_PAGE_UP, 74);
        assert_eq!(GHOSTTY_KEY_ARROW_DOWN, 75);
        assert_eq!(GHOSTTY_KEY_ARROW_LEFT, 76);
        assert_eq!(GHOSTTY_KEY_ARROW_RIGHT, 77);
        assert_eq!(GHOSTTY_KEY_ARROW_UP, 78);
        assert_eq!(GHOSTTY_KEY_ESCAPE, 120);
        assert_eq!(GHOSTTY_KEY_F1, 121);
        assert_eq!(GHOSTTY_KEY_F12, 132);
        assert_eq!(GHOSTTY_KEY_F13, 133);
        assert_eq!(GHOSTTY_KEY_F24, 144);
        assert_eq!(GHOSTTY_KEY_F25, 145);
        assert_eq!(GHOSTTY_KEY_FN, 146);
        assert_eq!(GHOSTTY_KEY_FN_LOCK, 147);
        assert_eq!(GHOSTTY_KEY_PRINT_SCREEN, 148);
        assert_eq!(GHOSTTY_KEY_SCROLL_LOCK, 149);
        assert_eq!(GHOSTTY_KEY_PAUSE, 150);
        assert_eq!(GHOSTTY_KEY_TABLE_ACTIVATE, 0);
        assert_eq!(GHOSTTY_KEY_TABLE_DEACTIVATE, 1);
        assert_eq!(GHOSTTY_KEY_TABLE_DEACTIVATE_ALL, 2);
        assert_eq!(GHOSTTY_SPLIT_DIRECTION_RIGHT, 0);
        assert_eq!(GHOSTTY_SPLIT_DIRECTION_DOWN, 1);
        assert_eq!(GHOSTTY_SPLIT_DIRECTION_LEFT, 2);
        assert_eq!(GHOSTTY_SPLIT_DIRECTION_UP, 3);
        assert_eq!(GHOSTTY_CLOSE_TAB_MODE_THIS, 0);
        assert_eq!(GHOSTTY_CLOSE_TAB_MODE_OTHER, 1);
        assert_eq!(GHOSTTY_CLOSE_TAB_MODE_RIGHT, 2);
        assert_eq!(GHOSTTY_GOTO_SPLIT_PREVIOUS, 0);
        assert_eq!(GHOSTTY_GOTO_SPLIT_NEXT, 1);
        assert_eq!(GHOSTTY_GOTO_SPLIT_UP, 2);
        assert_eq!(GHOSTTY_GOTO_SPLIT_LEFT, 3);
        assert_eq!(GHOSTTY_GOTO_SPLIT_DOWN, 4);
        assert_eq!(GHOSTTY_GOTO_SPLIT_RIGHT, 5);
        assert_eq!(GHOSTTY_RESIZE_SPLIT_UP, 0);
        assert_eq!(GHOSTTY_RESIZE_SPLIT_DOWN, 1);
        assert_eq!(GHOSTTY_RESIZE_SPLIT_LEFT, 2);
        assert_eq!(GHOSTTY_RESIZE_SPLIT_RIGHT, 3);
        assert_eq!(GHOSTTY_MOUSE_SHAPE_DEFAULT, 0);
        assert_eq!(GHOSTTY_MOUSE_SHAPE_POINTER, 3);
        assert_eq!(GHOSTTY_MOUSE_SHAPE_TEXT, 8);
        assert_eq!(GHOSTTY_MOUSE_SHAPE_ZOOM_OUT, 33);
        assert_eq!(GHOSTTY_MOUSE_VISIBLE, 0);
        assert_eq!(GHOSTTY_MOUSE_HIDDEN, 1);
        assert_eq!(GHOSTTY_MOUSE_BUTTON_UNKNOWN, 0);
        assert_eq!(GHOSTTY_MOUSE_BUTTON_LEFT, 1);
        assert_eq!(GHOSTTY_MOUSE_BUTTON_RIGHT, 2);
        assert_eq!(GHOSTTY_MOUSE_BUTTON_MIDDLE, 3);
        assert_eq!(GHOSTTY_MOUSE_BUTTON_FOUR, 4);
        assert_eq!(GHOSTTY_MOUSE_BUTTON_FIVE, 5);
        assert_eq!(GHOSTTY_MOUSE_BUTTON_SIX, 6);
        assert_eq!(GHOSTTY_MOUSE_BUTTON_SEVEN, 7);
        assert_eq!(GHOSTTY_MOUSE_BUTTON_EIGHT, 8);
        assert_eq!(GHOSTTY_MOUSE_BUTTON_NINE, 9);
        assert_eq!(GHOSTTY_MOUSE_BUTTON_TEN, 10);
        assert_eq!(GHOSTTY_MOUSE_BUTTON_ELEVEN, 11);
        assert_eq!(GHOSTTY_MOUSE_PRESSURE_NONE, 0);
        assert_eq!(GHOSTTY_MOUSE_PRESSURE_NORMAL, 1);
        assert_eq!(GHOSTTY_MOUSE_PRESSURE_DEEP, 2);
        assert_eq!(GHOSTTY_POINT_VIEWPORT, 1);
        assert_eq!(GHOSTTY_POINT_COORD_TOP_LEFT, 1);
        assert_eq!(GHOSTTY_POINT_COORD_BOTTOM_RIGHT, 2);
    }

    #[test]
    fn ghostty_viewport_selection_targets_visible_screen() {
        let selection = GhosttySelection::viewport();
        assert_eq!(selection.top_left.tag, GHOSTTY_POINT_VIEWPORT);
        assert_eq!(selection.top_left.coord, GHOSTTY_POINT_COORD_TOP_LEFT);
        assert_eq!(selection.bottom_right.tag, GHOSTTY_POINT_VIEWPORT);
        assert_eq!(
            selection.bottom_right.coord,
            GHOSTTY_POINT_COORD_BOTTOM_RIGHT
        );
        assert!(!selection.rectangle);
    }

    #[test]
    fn ghostty_binding_action_c_string_rejects_nul() {
        let action = binding_action_c_string("copy_title_to_clipboard").expect("binding action");
        assert_eq!(action.as_bytes(), b"copy_title_to_clipboard");
        assert!(binding_action_c_string("bad\0action").is_err());
    }

    #[derive(Default)]
    struct AppDropCounters {
        freed: usize,
        userdata: *mut c_void,
        updated_config: GhosttyConfig,
    }

    #[derive(Default)]
    struct GuardDropCounters {
        unrealized: usize,
        freed: usize,
        userdata: *mut c_void,
        app: GhosttyApp,
        inherited_context: c_int,
        inherited_config_frees: usize,
        updated_config: GhosttyConfig,
        mouse_captured: bool,
        mouse_pressure_stage: c_int,
        mouse_pressure: f64,
        refreshes: usize,
        refresh_result: bool,
        draws: usize,
        draw_result: bool,
        display_unrealized_result: bool,
        renderer_realized: Option<bool>,
        renderer_realized_result: bool,
    }

    #[derive(Default)]
    struct ConfigDropCounters {
        freed: usize,
    }

    #[derive(Default)]
    struct InspectorDropCounters {
        render: usize,
        render_result: bool,
        shutdown: usize,
        freed: usize,
    }

    fn ghostty_surface_guard_for_drop_test(
        surface: GhosttySurface,
        display_unrealized_complete: bool,
    ) -> GhosttySurfaceGuard {
        GhosttySurfaceGuard {
            surface,
            display_unrealized_complete,
            free: test_surface_free,
            userdata: test_surface_userdata,
            surface_app: test_surface_app,
            inherited_config: test_surface_inherited_config,
            inherited_config_free: test_surface_inherited_config_free,
            update_config: test_surface_update_config,
            string_free: test_string_free,
            refresh: test_surface_refresh,
            draw: test_surface_draw,
            display_realized: test_surface_display_realized,
            display_unrealized: test_surface_display_unrealized,
            set_renderer_realized: test_surface_set_renderer_realized,
            set_content_scale: test_surface_set_content_scale,
            set_focus: test_surface_set_bool,
            set_visible: test_surface_set_bool,
            set_occlusion: test_surface_set_bool,
            set_size: test_surface_set_size,
            set_color_scheme: test_surface_set_int,
            needs_confirm_quit: test_surface_bool,
            surface_size: test_surface_size,
            process_exited: test_surface_bool,
            foreground_pid: test_surface_pid,
            tty_name: test_surface_string,
            title: test_surface_string,
            pwd: test_surface_string,
            key_translation_mods: test_surface_key_translation_mods,
            key: test_surface_key,
            key_is_binding: test_surface_key_is_binding,
            text: test_surface_text,
            process_output: test_surface_text,
            preedit: test_surface_text,
            mouse_captured: test_surface_mouse_captured,
            mouse_button: test_surface_mouse_button,
            mouse_pos: test_surface_mouse_pos,
            mouse_scroll: test_surface_mouse_scroll,
            mouse_pressure: test_surface_mouse_pressure,
            ime_point: test_surface_ime_point,
            request_close: test_surface_void,
            split: test_surface_set_int_void,
            split_focus: test_surface_set_int_void,
            split_resize: test_surface_split_resize,
            split_equalize: test_surface_void,
            split_toggle_zoom: test_surface_void,
            binding_action: test_surface_binding_action,
            has_selection: test_surface_bool,
            select_cursor_cell: test_surface_bool,
            select_viewport_rows: test_surface_select_viewport_rows,
            clear_selection: test_surface_bool,
            read_selection: test_surface_read_text,
            read_text: test_surface_read_text_selection,
            read_scrollback: test_surface_read_scrollback,
            free_text: test_surface_free_text,
            surface_inspector: test_surface_inspector,
            inspector_free: test_inspector_free,
            inspector_set_focus: test_inspector_set_bool,
            inspector_set_content_scale: test_inspector_set_content_scale,
            inspector_set_size: test_inspector_set_size,
            inspector_mouse_button: test_inspector_mouse_button,
            inspector_mouse_pos: test_inspector_mouse_pos,
            inspector_mouse_scroll: test_inspector_mouse_scroll,
            inspector_key: test_inspector_key,
            inspector_text: test_inspector_text,
            inspector_opengl_init: test_inspector_opengl_init,
            inspector_opengl_render: test_inspector_render,
            inspector_opengl_shutdown: test_inspector_void,
        }
    }

    unsafe extern "C" fn test_app_free(app: GhosttyApp) {
        unsafe {
            (*(app as *mut AppDropCounters)).freed += 1;
        }
    }
    unsafe extern "C" fn test_app_void(_app: GhosttyApp) -> bool {
        true
    }
    unsafe extern "C" fn test_app_void_false(_app: GhosttyApp) -> bool {
        false
    }
    unsafe extern "C" fn test_app_bool_void(_app: GhosttyApp) -> bool {
        true
    }
    unsafe extern "C" fn test_app_bool_void_false(_app: GhosttyApp) -> bool {
        false
    }
    unsafe extern "C" fn test_app_userdata(app: GhosttyApp) -> *mut c_void {
        unsafe { (*(app as *mut AppDropCounters)).userdata }
    }
    unsafe extern "C" fn test_app_set_bool(_app: GhosttyApp, _value: bool) -> bool {
        true
    }
    unsafe extern "C" fn test_app_set_bool_false(_app: GhosttyApp, _value: bool) -> bool {
        false
    }
    unsafe extern "C" fn test_app_key(_app: GhosttyApp, _event: GhosttyInputKey) -> bool {
        false
    }
    unsafe extern "C" fn test_app_reload_config(_app: GhosttyApp, _soft: bool) -> bool {
        true
    }
    unsafe extern "C" fn test_app_reload_config_false(_app: GhosttyApp, _soft: bool) -> bool {
        false
    }
    unsafe extern "C" fn test_app_update_config(app: GhosttyApp, config: GhosttyConfig) -> bool {
        unsafe {
            (*(app as *mut AppDropCounters)).updated_config = config;
        }
        true
    }
    unsafe extern "C" fn test_app_update_config_false(
        _app: GhosttyApp,
        _config: GhosttyConfig,
    ) -> bool {
        false
    }
    unsafe extern "C" fn test_app_bool(_app: GhosttyApp) -> bool {
        false
    }
    unsafe extern "C" fn test_app_set_int(_app: GhosttyApp, _value: c_int) -> bool {
        true
    }
    unsafe extern "C" fn test_app_set_int_false(_app: GhosttyApp, _value: c_int) -> bool {
        false
    }
    unsafe extern "C" fn test_config_free(config: GhosttyConfig) {
        unsafe {
            (*(config as *mut ConfigDropCounters)).freed += 1;
        }
    }
    unsafe extern "C" fn test_config_load_default_files(_config: GhosttyConfig) -> bool {
        true
    }
    unsafe extern "C" fn test_config_load_default_files_false(_config: GhosttyConfig) -> bool {
        false
    }
    unsafe extern "C" fn test_config_load_file(
        _config: GhosttyConfig,
        _path: *const c_char,
    ) -> bool {
        true
    }
    unsafe extern "C" fn test_config_load_file_false(
        _config: GhosttyConfig,
        _path: *const c_char,
    ) -> bool {
        false
    }
    unsafe extern "C" fn test_config_load_string(
        _config: GhosttyConfig,
        contents: *const c_char,
        len: usize,
    ) -> bool {
        !contents.is_null() && len > 0
    }
    unsafe extern "C" fn test_config_load_string_false(
        _config: GhosttyConfig,
        _contents: *const c_char,
        _len: usize,
    ) -> bool {
        false
    }
    unsafe extern "C" fn test_config_load_recursive_files(_config: GhosttyConfig) -> bool {
        true
    }
    unsafe extern "C" fn test_config_load_recursive_files_false(_config: GhosttyConfig) -> bool {
        false
    }
    unsafe extern "C" fn test_config_finalize(_config: GhosttyConfig) -> bool {
        true
    }
    unsafe extern "C" fn test_config_finalize_false(_config: GhosttyConfig) -> bool {
        false
    }
    unsafe extern "C" fn test_config_diagnostics_count_empty(_config: GhosttyConfig) -> u32 {
        0
    }
    unsafe extern "C" fn test_config_diagnostics_count_one(_config: GhosttyConfig) -> u32 {
        1
    }
    unsafe extern "C" fn test_config_get_diagnostic_empty(
        _config: GhosttyConfig,
        _idx: u32,
    ) -> GhosttyDiagnostic {
        GhosttyDiagnostic {
            message: ptr::null(),
        }
    }
    unsafe extern "C" fn test_config_get_diagnostic_message(
        _config: GhosttyConfig,
        _idx: u32,
    ) -> GhosttyDiagnostic {
        static MESSAGE: &[u8] = b"font-family: invalid font\0";
        GhosttyDiagnostic {
            message: MESSAGE.as_ptr() as *const c_char,
        }
    }

    unsafe extern "C" fn test_surface_free(surface: GhosttySurface) {
        unsafe {
            (*(surface as *mut GuardDropCounters)).freed += 1;
        }
    }
    unsafe extern "C" fn test_surface_userdata(surface: GhosttySurface) -> *mut c_void {
        unsafe { (*(surface as *mut GuardDropCounters)).userdata }
    }
    unsafe extern "C" fn test_surface_app(surface: GhosttySurface) -> GhosttyApp {
        unsafe { (*(surface as *mut GuardDropCounters)).app }
    }
    unsafe extern "C" fn test_surface_inherited_config(
        surface: GhosttySurface,
        context: c_int,
    ) -> GhosttySurfaceConfig {
        unsafe {
            (*(surface as *mut GuardDropCounters)).inherited_context = context;
        }
        GhosttySurfaceConfig {
            platform_tag: 0,
            platform: GhosttyPlatform { padding: [0; 4] },
            userdata: ptr::null_mut(),
            scale_factor: 0.0,
            font_size: 0.0,
            working_directory: ptr::null(),
            command: ptr::null(),
            env_vars: ptr::null(),
            env_var_count: 0,
            initial_input: ptr::null(),
            wait_after_command: false,
            context,
            io_mode: GHOSTTY_SURFACE_IO_EXEC,
            io_write_cb: None,
            io_write_userdata: ptr::null_mut(),
            initial_output: ptr::null(),
            initial_output_len: 0,
            initial_width_px: 0,
            initial_height_px: 0,
        }
    }
    unsafe extern "C" fn test_surface_inherited_config_free(
        surface: GhosttySurface,
        config: *mut GhosttySurfaceConfig,
    ) {
        unsafe {
            (*(surface as *mut GuardDropCounters)).inherited_config_frees += 1;
            if !config.is_null() {
                (*config).working_directory = ptr::null();
            }
        }
    }
    unsafe extern "C" fn test_surface_update_config(
        surface: GhosttySurface,
        config: GhosttyConfig,
    ) -> bool {
        unsafe {
            (*(surface as *mut GuardDropCounters)).updated_config = config;
        }
        true
    }
    unsafe extern "C" fn test_surface_update_config_false(
        _surface: GhosttySurface,
        _config: GhosttyConfig,
    ) -> bool {
        false
    }

    unsafe extern "C" fn test_surface_display_unrealized(surface: GhosttySurface) -> bool {
        unsafe {
            let counters = &mut *(surface as *mut GuardDropCounters);
            counters.unrealized += 1;
            counters.display_unrealized_result
        }
    }

    unsafe extern "C" fn test_surface_display_realized(_surface: GhosttySurface) -> bool {
        true
    }

    unsafe extern "C" fn test_surface_set_renderer_realized(
        surface: GhosttySurface,
        realized: bool,
    ) -> bool {
        unsafe {
            let counters = &mut *(surface as *mut GuardDropCounters);
            counters.renderer_realized = Some(realized);
            counters.renderer_realized_result
        }
    }

    unsafe extern "C" fn test_string_free(_value: GhosttyString) {}
    unsafe extern "C" fn test_counting_string_free(value: GhosttyString) {
        if !value.ptr.is_null() {
            TEST_STRING_FREE_COUNT.fetch_add(1, Ordering::SeqCst);
        }
    }
    unsafe extern "C" fn test_surface_void(_surface: GhosttySurface) -> bool {
        true
    }
    unsafe extern "C" fn test_surface_void_false(_surface: GhosttySurface) -> bool {
        false
    }
    unsafe extern "C" fn test_surface_refresh(surface: GhosttySurface) -> bool {
        unsafe {
            let counters = &mut *(surface as *mut GuardDropCounters);
            counters.refreshes += 1;
            counters.refresh_result
        }
    }
    unsafe extern "C" fn test_surface_draw(surface: GhosttySurface) -> bool {
        unsafe {
            let counters = &mut *(surface as *mut GuardDropCounters);
            counters.draws += 1;
            counters.draw_result
        }
    }
    unsafe extern "C" fn test_surface_set_bool(_surface: GhosttySurface, _value: bool) -> bool {
        true
    }
    unsafe extern "C" fn test_surface_set_bool_false(
        _surface: GhosttySurface,
        _value: bool,
    ) -> bool {
        false
    }
    unsafe extern "C" fn test_surface_set_int(_surface: GhosttySurface, _value: c_int) -> bool {
        true
    }
    unsafe extern "C" fn test_surface_set_int_void(
        _surface: GhosttySurface,
        _value: c_int,
    ) -> bool {
        true
    }
    unsafe extern "C" fn test_surface_set_int_void_false(
        _surface: GhosttySurface,
        _value: c_int,
    ) -> bool {
        false
    }
    unsafe extern "C" fn test_surface_set_int_false(
        _surface: GhosttySurface,
        _value: c_int,
    ) -> bool {
        false
    }
    unsafe extern "C" fn test_surface_set_content_scale(
        _surface: GhosttySurface,
        _x: f64,
        _y: f64,
    ) -> bool {
        true
    }
    unsafe extern "C" fn test_surface_set_content_scale_false(
        _surface: GhosttySurface,
        _x: f64,
        _y: f64,
    ) -> bool {
        false
    }
    unsafe extern "C" fn test_surface_set_size(
        _surface: GhosttySurface,
        _width: u32,
        _height: u32,
    ) -> bool {
        true
    }
    unsafe extern "C" fn test_surface_set_size_false(
        _surface: GhosttySurface,
        _width: u32,
        _height: u32,
    ) -> bool {
        false
    }
    unsafe extern "C" fn test_surface_bool(_surface: GhosttySurface) -> bool {
        false
    }
    unsafe extern "C" fn test_surface_bool_true(_surface: GhosttySurface) -> bool {
        true
    }
    unsafe extern "C" fn test_surface_select_viewport_rows(
        _surface: GhosttySurface,
        _start_row: u32,
        _end_row: u32,
    ) -> bool {
        false
    }
    unsafe extern "C" fn test_surface_select_viewport_rows_true(
        _surface: GhosttySurface,
        _start_row: u32,
        _end_row: u32,
    ) -> bool {
        true
    }
    unsafe extern "C" fn test_surface_size(_surface: GhosttySurface) -> GhosttySurfaceSizeResult {
        GhosttySurfaceSizeResult {
            columns: 0,
            rows: 0,
            width_px: 0,
            height_px: 0,
            cell_width_px: 0,
            cell_height_px: 0,
        }
    }
    unsafe extern "C" fn test_surface_pid(_surface: GhosttySurface) -> u64 {
        0
    }
    unsafe extern "C" fn test_surface_string(_surface: GhosttySurface) -> GhosttyString {
        GhosttyString {
            ptr: ptr::null(),
            len: 0,
            sentinel: false,
        }
    }
    unsafe extern "C" fn test_surface_title_string(_surface: GhosttySurface) -> GhosttyString {
        static TITLE: &[u8] = b"embedded title\0";
        GhosttyString {
            ptr: TITLE.as_ptr() as *const c_char,
            len: TITLE.len() - 1,
            sentinel: true,
        }
    }
    unsafe extern "C" fn test_surface_pwd_string(_surface: GhosttySurface) -> GhosttyString {
        static PWD: &[u8] = b"/tmp/cmux-embedded\0";
        GhosttyString {
            ptr: PWD.as_ptr() as *const c_char,
            len: PWD.len() - 1,
            sentinel: true,
        }
    }
    unsafe extern "C" fn test_surface_key_translation_mods(
        _surface: GhosttySurface,
        mods: c_int,
    ) -> c_int {
        mods
    }
    unsafe extern "C" fn test_surface_key(
        _surface: GhosttySurface,
        _event: GhosttyInputKey,
    ) -> bool {
        false
    }
    unsafe extern "C" fn test_surface_key_is_binding(
        _surface: GhosttySurface,
        _event: GhosttyInputKey,
        _flags: *mut c_int,
    ) -> bool {
        false
    }
    unsafe extern "C" fn test_surface_key_is_binding_flags(
        _surface: GhosttySurface,
        _event: GhosttyInputKey,
        flags: *mut c_int,
    ) -> bool {
        unsafe {
            *flags = GHOSTTY_BINDING_FLAGS_CONSUMED | GHOSTTY_BINDING_FLAGS_PERFORMABLE;
        }
        true
    }
    unsafe extern "C" fn test_surface_text(
        _surface: GhosttySurface,
        _text: *const c_char,
        _len: usize,
    ) -> bool {
        true
    }
    unsafe extern "C" fn test_surface_text_false(
        _surface: GhosttySurface,
        _text: *const c_char,
        _len: usize,
    ) -> bool {
        false
    }
    unsafe extern "C" fn test_surface_mouse_captured(surface: GhosttySurface) -> bool {
        unsafe { (*(surface as *mut GuardDropCounters)).mouse_captured }
    }
    unsafe extern "C" fn test_surface_mouse_button(
        _surface: GhosttySurface,
        _state: c_int,
        _button: c_int,
        _mods: c_int,
    ) -> bool {
        false
    }
    unsafe extern "C" fn test_surface_mouse_pos(
        _surface: GhosttySurface,
        _x: f64,
        _y: f64,
        _mods: c_int,
    ) -> bool {
        true
    }
    unsafe extern "C" fn test_surface_mouse_scroll(
        _surface: GhosttySurface,
        _x: f64,
        _y: f64,
        _mods: c_int,
    ) -> bool {
        true
    }
    unsafe extern "C" fn test_surface_pointer_false(
        _surface: GhosttySurface,
        _x: f64,
        _y: f64,
        _mods: c_int,
    ) -> bool {
        false
    }
    unsafe extern "C" fn test_surface_mouse_pressure(
        surface: GhosttySurface,
        stage: c_int,
        pressure: f64,
    ) -> bool {
        unsafe {
            let counters = &mut *(surface as *mut GuardDropCounters);
            counters.mouse_pressure_stage = stage;
            counters.mouse_pressure = pressure;
        }
        true
    }
    unsafe extern "C" fn test_surface_mouse_pressure_false(
        _surface: GhosttySurface,
        _stage: c_int,
        _pressure: f64,
    ) -> bool {
        false
    }
    unsafe extern "C" fn test_surface_ime_point(
        _surface: GhosttySurface,
        _x: *mut f64,
        _y: *mut f64,
        _width: *mut f64,
        _height: *mut f64,
    ) -> bool {
        true
    }
    unsafe extern "C" fn test_surface_ime_point_false(
        _surface: GhosttySurface,
        _x: *mut f64,
        _y: *mut f64,
        _width: *mut f64,
        _height: *mut f64,
    ) -> bool {
        false
    }
    unsafe extern "C" fn test_surface_split_resize(
        _surface: GhosttySurface,
        _direction: c_int,
        _amount: u16,
    ) -> bool {
        true
    }
    unsafe extern "C" fn test_surface_split_resize_false(
        _surface: GhosttySurface,
        _direction: c_int,
        _amount: u16,
    ) -> bool {
        false
    }
    unsafe extern "C" fn test_surface_binding_action(
        _surface: GhosttySurface,
        _action: *const c_char,
        _len: usize,
    ) -> bool {
        false
    }
    unsafe extern "C" fn test_surface_read_text(
        _surface: GhosttySurface,
        _text: *mut GhosttyText,
    ) -> bool {
        false
    }
    unsafe extern "C" fn test_surface_read_text_selection(
        _surface: GhosttySurface,
        _selection: GhosttySelection,
        _text: *mut GhosttyText,
    ) -> bool {
        false
    }
    unsafe extern "C" fn test_surface_read_scrollback(
        _surface: GhosttySurface,
        _max_bytes: usize,
        _text: *mut GhosttyText,
    ) -> bool {
        false
    }
    unsafe extern "C" fn test_surface_free_text(_surface: GhosttySurface, _text: *mut GhosttyText) {
    }
    unsafe extern "C" fn test_surface_inspector(_surface: GhosttySurface) -> GhosttyInspector {
        ptr::null_mut()
    }
    unsafe extern "C" fn test_inspector_free(_inspector: GhosttyInspector) {}
    unsafe extern "C" fn test_inspector_set_bool(
        _inspector: GhosttyInspector,
        _value: bool,
    ) -> bool {
        true
    }
    unsafe extern "C" fn test_inspector_set_bool_false(
        _inspector: GhosttyInspector,
        _value: bool,
    ) -> bool {
        false
    }
    unsafe extern "C" fn test_inspector_set_content_scale(
        _inspector: GhosttyInspector,
        _x: f64,
        _y: f64,
    ) -> bool {
        true
    }
    unsafe extern "C" fn test_inspector_set_content_scale_false(
        _inspector: GhosttyInspector,
        _x: f64,
        _y: f64,
    ) -> bool {
        false
    }
    unsafe extern "C" fn test_inspector_set_size(
        _inspector: GhosttyInspector,
        _width: u32,
        _height: u32,
    ) -> bool {
        true
    }
    unsafe extern "C" fn test_inspector_set_size_false(
        _inspector: GhosttyInspector,
        _width: u32,
        _height: u32,
    ) -> bool {
        false
    }
    unsafe extern "C" fn test_inspector_mouse_button(
        _inspector: GhosttyInspector,
        _action: c_int,
        _button: c_int,
        _mods: c_int,
    ) -> bool {
        true
    }
    unsafe extern "C" fn test_inspector_mouse_button_false(
        _inspector: GhosttyInspector,
        _action: c_int,
        _button: c_int,
        _mods: c_int,
    ) -> bool {
        false
    }
    unsafe extern "C" fn test_inspector_mouse_pos(
        _inspector: GhosttyInspector,
        _x: f64,
        _y: f64,
    ) -> bool {
        true
    }
    unsafe extern "C" fn test_inspector_mouse_pos_false(
        _inspector: GhosttyInspector,
        _x: f64,
        _y: f64,
    ) -> bool {
        false
    }
    unsafe extern "C" fn test_inspector_mouse_scroll(
        _inspector: GhosttyInspector,
        _x: f64,
        _y: f64,
        _mods: c_int,
    ) -> bool {
        true
    }
    unsafe extern "C" fn test_inspector_mouse_scroll_false(
        _inspector: GhosttyInspector,
        _x: f64,
        _y: f64,
        _mods: c_int,
    ) -> bool {
        false
    }
    unsafe extern "C" fn test_inspector_key(
        _inspector: GhosttyInspector,
        _action: c_int,
        _key: c_int,
        _mods: c_int,
    ) -> bool {
        true
    }
    unsafe extern "C" fn test_inspector_key_false(
        _inspector: GhosttyInspector,
        _action: c_int,
        _key: c_int,
        _mods: c_int,
    ) -> bool {
        false
    }
    unsafe extern "C" fn test_inspector_text(
        _inspector: GhosttyInspector,
        _text: *const c_char,
    ) -> bool {
        true
    }
    unsafe extern "C" fn test_inspector_text_false(
        _inspector: GhosttyInspector,
        _text: *const c_char,
    ) -> bool {
        false
    }
    unsafe extern "C" fn test_inspector_opengl_init(
        _inspector: GhosttyInspector,
        _glsl_version: *const c_char,
    ) -> bool {
        false
    }
    unsafe extern "C" fn test_inspector_render(inspector: GhosttyInspector) -> bool {
        unsafe {
            let counters = &mut *(inspector as *mut InspectorDropCounters);
            counters.render += 1;
            counters.render_result
        }
    }
    unsafe extern "C" fn test_inspector_void(_inspector: GhosttyInspector) -> bool {
        true
    }
    unsafe extern "C" fn test_counting_inspector_free(inspector: GhosttyInspector) {
        unsafe {
            (*(inspector as *mut InspectorDropCounters)).freed += 1;
        }
    }
    unsafe extern "C" fn test_counting_inspector_shutdown(inspector: GhosttyInspector) -> bool {
        unsafe {
            (*(inspector as *mut InspectorDropCounters)).shutdown += 1;
        }
        true
    }
    unsafe extern "C" fn test_counting_inspector_shutdown_false(
        inspector: GhosttyInspector,
    ) -> bool {
        unsafe {
            (*(inspector as *mut InspectorDropCounters)).shutdown += 1;
        }
        false
    }

    #[test]
    fn ghostty_surface_guard_unrealizes_before_free_when_realized() {
        let mut counters = GuardDropCounters::default();
        counters.display_unrealized_result = true;
        let surface = &mut counters as *mut GuardDropCounters as GhosttySurface;
        {
            let mut guard = ghostty_surface_guard_for_drop_test(surface, false);
            assert!(guard.display_realized());
        }
        assert_eq!(counters.unrealized, 1);
        assert_eq!(counters.freed, 1);
    }

    #[test]
    fn ghostty_surface_guard_does_not_unrealize_twice() {
        let mut counters = GuardDropCounters::default();
        counters.display_unrealized_result = true;
        let surface = &mut counters as *mut GuardDropCounters as GhosttySurface;
        {
            let mut guard = ghostty_surface_guard_for_drop_test(surface, false);
            assert!(guard.display_realized());
            assert!(guard.display_unrealized());
        }
        assert_eq!(counters.unrealized, 1);
        assert_eq!(counters.freed, 1);
    }

    #[test]
    fn ghostty_surface_guard_keeps_realized_state_when_unrealize_fails() {
        let mut counters = GuardDropCounters::default();
        let surface = &mut counters as *mut GuardDropCounters as GhosttySurface;
        {
            let mut guard = ghostty_surface_guard_for_drop_test(surface, false);
            assert!(guard.display_realized());
            assert!(!guard.display_unrealized());
        }
        assert_eq!(counters.unrealized, 2);
        assert_eq!(counters.freed, 1);
    }

    #[test]
    fn ghostty_surface_guard_reports_draw_result() {
        let mut counters = GuardDropCounters {
            draw_result: true,
            ..GuardDropCounters::default()
        };
        let surface = &mut counters as *mut GuardDropCounters as GhosttySurface;
        let guard = ghostty_surface_guard_for_drop_test(surface, false);

        assert!(guard.draw());
        assert_eq!(counters.draws, 1);

        counters.draw_result = false;
        assert!(!guard.draw());
        assert_eq!(counters.draws, 2);
    }

    #[test]
    fn ghostty_surface_guard_reports_refresh_result() {
        let mut counters = GuardDropCounters {
            refresh_result: true,
            ..GuardDropCounters::default()
        };
        let surface = &mut counters as *mut GuardDropCounters as GhosttySurface;
        let guard = ghostty_surface_guard_for_drop_test(surface, false);

        assert!(guard.refresh());
        assert_eq!(counters.refreshes, 1);

        counters.refresh_result = false;
        assert!(!guard.refresh());
        assert_eq!(counters.refreshes, 2);
    }

    #[test]
    fn ghostty_surface_guard_tracks_renderer_realized_alias() {
        let mut counters = GuardDropCounters {
            display_unrealized_result: true,
            renderer_realized_result: true,
            ..GuardDropCounters::default()
        };
        let surface = &mut counters as *mut GuardDropCounters as GhosttySurface;

        {
            let mut guard = ghostty_surface_guard_for_drop_test(surface, false);
            assert!(guard.set_renderer_realized(false));
            assert_eq!(counters.renderer_realized, Some(false));
        }
        assert_eq!(counters.unrealized, 0);

        {
            let mut guard = ghostty_surface_guard_for_drop_test(surface, false);
            assert!(guard.set_renderer_realized(true));
            assert_eq!(counters.renderer_realized, Some(true));
        }
        assert_eq!(counters.unrealized, 1);
    }

    #[test]
    fn ghostty_surface_guard_unrealizes_even_when_never_realized() {
        let mut counters = GuardDropCounters::default();
        counters.display_unrealized_result = true;
        let surface = &mut counters as *mut GuardDropCounters as GhosttySurface;
        {
            let _guard = ghostty_surface_guard_for_drop_test(surface, false);
        }
        assert_eq!(counters.unrealized, 1);
        assert_eq!(counters.freed, 1);
    }

    #[test]
    fn ghostty_app_guard_returns_userdata() {
        let mut token = 7_u8;
        let mut counters = AppDropCounters {
            userdata: &mut token as *mut u8 as *mut c_void,
            ..Default::default()
        };
        let app = &mut counters as *mut AppDropCounters as GhosttyApp;
        {
            let guard = GhosttyAppGuard {
                app,
                free: test_app_free,
                tick: test_app_bool_void,
                userdata: test_app_userdata,
                set_focus: test_app_set_bool,
                key: test_app_key,
                keyboard_changed: test_app_bool_void,
                open_config: test_app_void,
                reload_config: test_app_reload_config,
                update_config: test_app_update_config,
                needs_confirm_quit: test_app_bool,
                has_global_keybinds: test_app_bool,
                must_draw_from_app_thread: test_app_bool_void,
                set_color_scheme: test_app_set_int,
            };
            assert_eq!(guard.userdata(), counters.userdata);
        }
        assert_eq!(counters.freed, 1);
    }

    #[test]
    fn ghostty_app_guard_reports_tick_and_keyboard_changed_results() {
        let mut counters = AppDropCounters::default();
        let app = &mut counters as *mut AppDropCounters as GhosttyApp;
        {
            let mut guard = GhosttyAppGuard {
                app,
                free: test_app_free,
                tick: test_app_bool_void,
                userdata: test_app_userdata,
                set_focus: test_app_set_bool,
                key: test_app_key,
                keyboard_changed: test_app_bool_void,
                open_config: test_app_void,
                reload_config: test_app_reload_config,
                update_config: test_app_update_config,
                needs_confirm_quit: test_app_bool,
                has_global_keybinds: test_app_bool,
                must_draw_from_app_thread: test_app_bool_void,
                set_color_scheme: test_app_set_int,
            };

            assert!(guard.tick());
            guard.tick = test_app_bool_void_false;
            assert!(!guard.tick());

            assert!(guard.keyboard_changed());
            guard.keyboard_changed = test_app_bool_void_false;
            assert!(!guard.keyboard_changed());

            assert!(guard.must_draw_from_app_thread());
            guard.must_draw_from_app_thread = test_app_bool_void_false;
            assert!(!guard.must_draw_from_app_thread());
        }
        assert_eq!(counters.freed, 1);
    }

    #[test]
    fn ghostty_app_guard_reports_focus_result() {
        let mut counters = AppDropCounters::default();
        let app = &mut counters as *mut AppDropCounters as GhosttyApp;
        {
            let mut guard = GhosttyAppGuard {
                app,
                free: test_app_free,
                tick: test_app_bool_void,
                userdata: test_app_userdata,
                set_focus: test_app_set_bool,
                key: test_app_key,
                keyboard_changed: test_app_bool_void,
                open_config: test_app_void,
                reload_config: test_app_reload_config,
                update_config: test_app_update_config,
                needs_confirm_quit: test_app_bool,
                has_global_keybinds: test_app_bool,
                must_draw_from_app_thread: test_app_bool_void,
                set_color_scheme: test_app_set_int,
            };
            assert!(guard.set_focus(true));
            guard.set_focus = test_app_set_bool_false;
            assert!(!guard.set_focus(false));
        }
        assert_eq!(counters.freed, 1);
    }

    #[test]
    fn ghostty_app_guard_reports_config_action_results() {
        let mut counters = AppDropCounters::default();
        let app = &mut counters as *mut AppDropCounters as GhosttyApp;
        {
            let mut guard = GhosttyAppGuard {
                app,
                free: test_app_free,
                tick: test_app_bool_void,
                userdata: test_app_userdata,
                set_focus: test_app_set_bool,
                key: test_app_key,
                keyboard_changed: test_app_bool_void,
                open_config: test_app_void,
                reload_config: test_app_reload_config,
                update_config: test_app_update_config,
                needs_confirm_quit: test_app_bool,
                has_global_keybinds: test_app_bool,
                must_draw_from_app_thread: test_app_bool_void,
                set_color_scheme: test_app_set_int,
            };

            assert!(guard.open_config());
            guard.open_config = test_app_void_false;
            assert!(!guard.open_config());

            assert!(guard.reload_config(true));
            guard.reload_config = test_app_reload_config_false;
            assert!(!guard.reload_config(false));
        }
        assert_eq!(counters.freed, 1);
    }

    #[test]
    fn ghostty_default_config_load_reports_load_and_finalize_results() {
        let mut config_counters = ConfigDropCounters::default();
        let config = &mut config_counters as *mut ConfigDropCounters as GhosttyConfig;

        assert!(load_and_finalize_default_config(
            config,
            test_config_load_default_files,
            test_config_load_recursive_files,
            test_config_finalize,
            test_config_diagnostics_count_empty,
            test_config_get_diagnostic_empty
        )
        .is_ok());

        assert!(load_and_finalize_default_config(
            config,
            test_config_load_default_files_false,
            test_config_load_recursive_files,
            test_config_finalize,
            test_config_diagnostics_count_empty,
            test_config_get_diagnostic_empty
        )
        .is_err());

        assert!(load_and_finalize_default_config(
            config,
            test_config_load_default_files,
            test_config_load_recursive_files_false,
            test_config_finalize,
            test_config_diagnostics_count_empty,
            test_config_get_diagnostic_empty
        )
        .is_err());

        assert!(load_and_finalize_default_config(
            config,
            test_config_load_default_files,
            test_config_load_recursive_files,
            test_config_finalize_false,
            test_config_diagnostics_count_empty,
            test_config_get_diagnostic_empty
        )
        .is_err());

        let err = load_and_finalize_default_config(
            config,
            test_config_load_default_files,
            test_config_load_recursive_files,
            test_config_finalize_false,
            test_config_diagnostics_count_one,
            test_config_get_diagnostic_message,
        )
        .unwrap_err()
        .to_string();
        assert!(err.contains("ghostty_config_finalize failed"), "{err}");
        assert!(err.contains("font-family: invalid font"), "{err}");

        assert!(load_and_finalize_default_config_with_override(
            config,
            test_config_load_default_files,
            Some("copy-on-select = false"),
            Some(test_config_load_string),
            test_config_load_recursive_files,
            test_config_finalize,
            test_config_diagnostics_count_empty,
            test_config_get_diagnostic_empty,
        )
        .is_ok());
        assert!(load_and_finalize_default_config_with_override(
            config,
            test_config_load_default_files,
            Some("copy-on-select = false"),
            Some(test_config_load_string_false),
            test_config_load_recursive_files,
            test_config_finalize,
            test_config_diagnostics_count_empty,
            test_config_get_diagnostic_empty,
        )
        .is_err());
    }

    #[test]
    fn ghostty_file_config_load_reports_load_and_finalize_results() {
        let mut config_counters = ConfigDropCounters::default();
        let config = &mut config_counters as *mut ConfigDropCounters as GhosttyConfig;
        let path = Path::new("/tmp/ghostty/config");

        assert!(load_and_finalize_config_file(
            config,
            path,
            test_config_load_file,
            test_config_load_recursive_files,
            test_config_finalize,
            test_config_diagnostics_count_empty,
            test_config_get_diagnostic_empty
        )
        .is_ok());

        assert!(load_and_finalize_config_file(
            config,
            path,
            test_config_load_file_false,
            test_config_load_recursive_files,
            test_config_finalize,
            test_config_diagnostics_count_empty,
            test_config_get_diagnostic_empty
        )
        .is_err());

        assert!(load_and_finalize_config_file(
            config,
            path,
            test_config_load_file,
            test_config_load_recursive_files_false,
            test_config_finalize,
            test_config_diagnostics_count_empty,
            test_config_get_diagnostic_empty
        )
        .is_err());

        assert!(load_and_finalize_config_file(
            config,
            path,
            test_config_load_file,
            test_config_load_recursive_files,
            test_config_finalize_false,
            test_config_diagnostics_count_empty,
            test_config_get_diagnostic_empty
        )
        .is_err());

        let err = load_and_finalize_config_file(
            config,
            path,
            test_config_load_file_false,
            test_config_load_recursive_files,
            test_config_finalize,
            test_config_diagnostics_count_one,
            test_config_get_diagnostic_message,
        )
        .unwrap_err()
        .to_string();
        assert!(err.contains("ghostty_config_load_file failed"), "{err}");
        assert!(err.contains("font-family: invalid font"), "{err}");
    }

    #[test]
    fn ghostty_string_config_load_reports_load_and_finalize_results() {
        let mut config_counters = ConfigDropCounters::default();
        let config = &mut config_counters as *mut ConfigDropCounters as GhosttyConfig;
        let contents = "font-family = JetBrains Mono\n";

        assert!(load_and_finalize_config_string(
            config,
            contents,
            test_config_load_string,
            test_config_load_recursive_files,
            test_config_finalize,
            test_config_diagnostics_count_empty,
            test_config_get_diagnostic_empty
        )
        .is_ok());

        assert!(load_and_finalize_config_string(
            config,
            contents,
            test_config_load_string_false,
            test_config_load_recursive_files,
            test_config_finalize,
            test_config_diagnostics_count_empty,
            test_config_get_diagnostic_empty
        )
        .is_err());

        assert!(load_and_finalize_config_string(
            config,
            contents,
            test_config_load_string,
            test_config_load_recursive_files_false,
            test_config_finalize,
            test_config_diagnostics_count_empty,
            test_config_get_diagnostic_empty
        )
        .is_err());

        assert!(load_and_finalize_config_string(
            config,
            contents,
            test_config_load_string,
            test_config_load_recursive_files,
            test_config_finalize_false,
            test_config_diagnostics_count_empty,
            test_config_get_diagnostic_empty
        )
        .is_err());

        let err = load_and_finalize_config_string(
            config,
            contents,
            test_config_load_string_false,
            test_config_load_recursive_files,
            test_config_finalize,
            test_config_diagnostics_count_one,
            test_config_get_diagnostic_message,
        )
        .unwrap_err()
        .to_string();
        assert!(err.contains("ghostty_config_load_string failed"), "{err}");
        assert!(err.contains("font-family: invalid font"), "{err}");
    }

    #[test]
    fn ghostty_app_guard_updates_config() {
        let mut counters = AppDropCounters::default();
        let app = &mut counters as *mut AppDropCounters as GhosttyApp;
        let mut config_counters = ConfigDropCounters::default();
        let config = &mut config_counters as *mut ConfigDropCounters as GhosttyConfig;
        {
            let guard = GhosttyAppGuard {
                app,
                free: test_app_free,
                tick: test_app_bool_void,
                userdata: test_app_userdata,
                set_focus: test_app_set_bool,
                key: test_app_key,
                keyboard_changed: test_app_bool_void,
                open_config: test_app_void,
                reload_config: test_app_reload_config,
                update_config: test_app_update_config,
                needs_confirm_quit: test_app_bool,
                has_global_keybinds: test_app_bool,
                must_draw_from_app_thread: test_app_bool_void,
                set_color_scheme: test_app_set_int,
            };
            let config_guard = GhosttyConfigGuard {
                config,
                free: test_config_free,
            };
            assert!(guard.update_config(&config_guard));
            assert_eq!(counters.updated_config, config);

            let failing_guard = GhosttyAppGuard {
                app,
                free: test_app_free,
                tick: test_app_bool_void,
                userdata: test_app_userdata,
                set_focus: test_app_set_bool,
                key: test_app_key,
                keyboard_changed: test_app_bool_void,
                open_config: test_app_void,
                reload_config: test_app_reload_config,
                update_config: test_app_update_config_false,
                needs_confirm_quit: test_app_bool,
                has_global_keybinds: test_app_bool,
                must_draw_from_app_thread: test_app_bool_void,
                set_color_scheme: test_app_set_int,
            };
            assert!(!failing_guard.update_config(&config_guard));
        }
        assert_eq!(counters.freed, 2);
        assert_eq!(config_counters.freed, 1);
    }

    #[test]
    fn ghostty_app_guard_reports_color_scheme_result() {
        let mut counters = AppDropCounters::default();
        let app = &mut counters as *mut AppDropCounters as GhosttyApp;
        {
            let mut guard = GhosttyAppGuard {
                app,
                free: test_app_free,
                tick: test_app_bool_void,
                userdata: test_app_userdata,
                set_focus: test_app_set_bool,
                key: test_app_key,
                keyboard_changed: test_app_bool_void,
                open_config: test_app_void,
                reload_config: test_app_reload_config,
                update_config: test_app_update_config,
                needs_confirm_quit: test_app_bool,
                has_global_keybinds: test_app_bool,
                must_draw_from_app_thread: test_app_bool_void,
                set_color_scheme: test_app_set_int,
            };
            assert!(guard.set_color_scheme(GHOSTTY_COLOR_SCHEME_DARK));
            guard.set_color_scheme = test_app_set_int_false;
            assert!(!guard.set_color_scheme(GHOSTTY_COLOR_SCHEME_LIGHT));
        }
        assert_eq!(counters.freed, 1);
    }

    #[test]
    fn ghostty_surface_guard_returns_userdata() {
        let mut token = 9_u8;
        let mut counters = GuardDropCounters {
            userdata: &mut token as *mut u8 as *mut c_void,
            ..Default::default()
        };
        let surface = &mut counters as *mut GuardDropCounters as GhosttySurface;
        {
            let guard = ghostty_surface_guard_for_drop_test(surface, false);
            assert_eq!(guard.userdata(), counters.userdata);
        }
        assert_eq!(counters.freed, 1);
    }

    #[test]
    fn ghostty_surface_guard_reads_title_and_pwd_strings() {
        TEST_STRING_FREE_COUNT.store(0, Ordering::SeqCst);

        let mut counters = GuardDropCounters::default();
        let surface = &mut counters as *mut GuardDropCounters as GhosttySurface;
        {
            let mut guard = ghostty_surface_guard_for_drop_test(surface, false);
            guard.string_free = test_counting_string_free;
            guard.title = test_surface_title_string;
            guard.pwd = test_surface_pwd_string;

            assert_eq!(guard.title().as_deref(), Some("embedded title"));
            assert_eq!(guard.pwd().as_deref(), Some("/tmp/cmux-embedded"));
            assert_eq!(TEST_STRING_FREE_COUNT.load(Ordering::SeqCst), 2);
        }
        assert_eq!(counters.freed, 1);
    }

    #[test]
    fn ghostty_surface_guard_reports_app_inherited_config_and_update_config() {
        let mut app_counters = AppDropCounters::default();
        let app = &mut app_counters as *mut AppDropCounters as GhosttyApp;
        let mut counters = GuardDropCounters {
            app,
            ..Default::default()
        };
        let surface = &mut counters as *mut GuardDropCounters as GhosttySurface;
        let mut config_counters = ConfigDropCounters::default();
        let config = &mut config_counters as *mut ConfigDropCounters as GhosttyConfig;
        {
            let guard = ghostty_surface_guard_for_drop_test(surface, false);
            assert_eq!(guard.app(), app);

            let mut inherited = guard.inherited_config(GHOSTTY_SURFACE_CONTEXT_SPLIT);
            assert_eq!(inherited.context, GHOSTTY_SURFACE_CONTEXT_SPLIT);
            assert_eq!(counters.inherited_context, GHOSTTY_SURFACE_CONTEXT_SPLIT);
            inherited.working_directory = b"/tmp/cmux\0".as_ptr() as *const c_char;
            guard.free_inherited_config(&mut inherited);
            assert!(inherited.working_directory.is_null());
            assert_eq!(counters.inherited_config_frees, 1);

            let config_guard = GhosttyConfigGuard {
                config,
                free: test_config_free,
            };
            assert!(guard.update_config(&config_guard));
            assert_eq!(counters.updated_config, config);

            let mut failing_guard = ghostty_surface_guard_for_drop_test(surface, false);
            failing_guard.update_config = test_surface_update_config_false;
            assert!(!failing_guard.update_config(&config_guard));
        }
        assert_eq!(counters.freed, 2);
        assert_eq!(config_counters.freed, 1);
    }

    #[test]
    fn ghostty_surface_guard_reports_state_update_results() {
        let mut counters = GuardDropCounters::default();
        let surface = &mut counters as *mut GuardDropCounters as GhosttySurface;
        {
            let mut guard = ghostty_surface_guard_for_drop_test(surface, false);

            assert!(guard.set_content_scale(2.0, 2.0));
            guard.set_content_scale = test_surface_set_content_scale_false;
            assert!(!guard.set_content_scale(2.0, 2.0));

            assert!(guard.set_focus(true));
            guard.set_focus = test_surface_set_bool_false;
            assert!(!guard.set_focus(false));

            assert!(guard.set_visible(true));
            guard.set_visible = test_surface_set_bool_false;
            assert!(!guard.set_visible(false));

            assert!(guard.set_occlusion(true));
            guard.set_occlusion = test_surface_set_bool_false;
            assert!(!guard.set_occlusion(false));

            assert!(guard.set_size(120, 40));
            guard.set_size = test_surface_set_size_false;
            assert!(!guard.set_size(120, 40));

            assert!(guard.set_color_scheme(GHOSTTY_COLOR_SCHEME_DARK));
            guard.set_color_scheme = test_surface_set_int_false;
            assert!(!guard.set_color_scheme(GHOSTTY_COLOR_SCHEME_LIGHT));
        }
        assert_eq!(counters.freed, 1);
    }

    #[test]
    fn ghostty_surface_guard_reports_text_and_preedit_results() {
        let mut counters = GuardDropCounters::default();
        let surface = &mut counters as *mut GuardDropCounters as GhosttySurface;
        {
            let mut guard = ghostty_surface_guard_for_drop_test(surface, false);

            assert!(guard.text("hello"));
            guard.text = test_surface_text_false;
            assert!(!guard.text("hello"));

            assert!(guard.preedit(Some("compose")));
            assert!(guard.preedit(None));
            guard.preedit = test_surface_text_false;
            assert!(!guard.preedit(Some("compose")));
            assert!(!guard.preedit(None));
        }
        assert_eq!(counters.freed, 1);
    }

    #[test]
    fn ghostty_surface_guard_reports_binding_flags() {
        let mut counters = GuardDropCounters::default();
        let surface = &mut counters as *mut GuardDropCounters as GhosttySurface;
        {
            let mut guard = ghostty_surface_guard_for_drop_test(surface, false);
            let event = GhosttyInputKey {
                action: GHOSTTY_ACTION_PRESS,
                mods: GHOSTTY_MODS_SUPER,
                consumed_mods: 0,
                keycode: 54,
                text: ptr::null(),
                unshifted_codepoint: 'c' as u32,
                composing: false,
            };

            assert_eq!(guard.key_binding_flags(event), None);
            assert!(!guard.key_is_binding(event));
            guard.key_is_binding = test_surface_key_is_binding_flags;
            assert_eq!(
                guard.key_binding_flags(event),
                Some(GHOSTTY_BINDING_FLAGS_CONSUMED | GHOSTTY_BINDING_FLAGS_PERFORMABLE)
            );
            assert!(guard.key_is_binding(event));
        }
        assert_eq!(counters.freed, 1);
    }

    #[test]
    fn ghostty_surface_guard_reports_mouse_capture() {
        let mut counters = GuardDropCounters {
            mouse_captured: true,
            ..Default::default()
        };
        let surface = &mut counters as *mut GuardDropCounters as GhosttySurface;
        {
            let guard = ghostty_surface_guard_for_drop_test(surface, false);
            assert!(guard.mouse_captured());
        }
        assert_eq!(counters.freed, 1);
    }

    #[test]
    fn ghostty_surface_guard_forwards_mouse_pressure() {
        let mut counters = GuardDropCounters::default();
        let surface = &mut counters as *mut GuardDropCounters as GhosttySurface;
        {
            let guard = ghostty_surface_guard_for_drop_test(surface, false);
            assert!(guard.mouse_pressure(2, 0.75));
        }
        assert_eq!(counters.mouse_pressure_stage, 2);
        assert_eq!(counters.mouse_pressure, 0.75);
        assert_eq!(counters.freed, 1);
    }

    #[test]
    fn ghostty_surface_guard_reports_pointer_input_results() {
        let mut counters = GuardDropCounters::default();
        let surface = &mut counters as *mut GuardDropCounters as GhosttySurface;
        {
            let mut guard = ghostty_surface_guard_for_drop_test(surface, false);

            assert!(guard.mouse_pos(10.0, 20.0, GHOSTTY_MODS_SHIFT));
            guard.mouse_pos = test_surface_pointer_false;
            assert!(!guard.mouse_pos(10.0, 20.0, GHOSTTY_MODS_SHIFT));

            assert!(guard.mouse_scroll(1.0, -1.0, 0));
            guard.mouse_scroll = test_surface_pointer_false;
            assert!(!guard.mouse_scroll(1.0, -1.0, 0));

            assert!(guard.mouse_pressure(2, 0.75));
            guard.mouse_pressure = test_surface_mouse_pressure_false;
            assert!(!guard.mouse_pressure(2, 0.75));
        }
        assert_eq!(counters.freed, 1);
    }

    #[test]
    fn ghostty_surface_guard_reports_ime_point_result() {
        let mut counters = GuardDropCounters::default();
        let surface = &mut counters as *mut GuardDropCounters as GhosttySurface;
        {
            let mut guard = ghostty_surface_guard_for_drop_test(surface, false);
            assert!(guard.ime_point().is_some());
            guard.ime_point = test_surface_ime_point_false;
            assert!(guard.ime_point().is_none());
        }
        assert_eq!(counters.freed, 1);
    }

    #[test]
    fn ghostty_surface_guard_reports_close_and_split_results() {
        let mut counters = GuardDropCounters::default();
        let surface = &mut counters as *mut GuardDropCounters as GhosttySurface;
        {
            let mut guard = ghostty_surface_guard_for_drop_test(surface, false);

            assert!(guard.request_close());
            guard.request_close = test_surface_void_false;
            assert!(!guard.request_close());

            assert!(guard.split(GHOSTTY_SPLIT_DIRECTION_RIGHT));
            guard.split = test_surface_set_int_void_false;
            assert!(!guard.split(GHOSTTY_SPLIT_DIRECTION_RIGHT));

            assert!(guard.split_focus(GHOSTTY_GOTO_SPLIT_NEXT));
            guard.split_focus = test_surface_set_int_void_false;
            assert!(!guard.split_focus(GHOSTTY_GOTO_SPLIT_NEXT));

            assert!(guard.split_resize(GHOSTTY_RESIZE_SPLIT_RIGHT, 25));
            guard.split_resize = test_surface_split_resize_false;
            assert!(!guard.split_resize(GHOSTTY_RESIZE_SPLIT_RIGHT, 25));

            assert!(guard.split_equalize());
            guard.split_equalize = test_surface_void_false;
            assert!(!guard.split_equalize());

            assert!(guard.split_toggle_zoom());
            guard.split_toggle_zoom = test_surface_void_false;
            assert!(!guard.split_toggle_zoom());
        }
        assert_eq!(counters.freed, 1);
    }

    #[test]
    fn ghostty_surface_guard_reports_selection_results() {
        let mut counters = GuardDropCounters::default();
        let surface = &mut counters as *mut GuardDropCounters as GhosttySurface;
        {
            let mut guard = ghostty_surface_guard_for_drop_test(surface, false);

            assert!(!guard.has_selection());
            guard.has_selection = test_surface_bool_true;
            assert!(guard.has_selection());

            assert!(!guard.select_cursor_cell());
            guard.select_cursor_cell = test_surface_bool_true;
            assert!(guard.select_cursor_cell());

            assert!(!guard.select_viewport_rows(1, 2));
            guard.select_viewport_rows = test_surface_select_viewport_rows_true;
            assert!(guard.select_viewport_rows(1, 2));

            assert!(!guard.clear_selection());
            guard.clear_selection = test_surface_bool_true;
            assert!(guard.clear_selection());
        }
        assert_eq!(counters.freed, 1);
    }

    #[test]
    fn ghostty_inspector_guard_shutdowns_and_frees_inspector_handle() {
        let mut counters = InspectorDropCounters::default();
        let inspector = &mut counters as *mut InspectorDropCounters as GhosttyInspector;
        {
            let _guard = GhosttyInspectorGuard {
                inspector,
                free: test_counting_inspector_free,
                set_focus: test_inspector_set_bool,
                set_content_scale: test_inspector_set_content_scale,
                set_size: test_inspector_set_size,
                mouse_button: test_inspector_mouse_button,
                mouse_pos: test_inspector_mouse_pos,
                mouse_scroll: test_inspector_mouse_scroll,
                key: test_inspector_key,
                text: test_inspector_text,
                opengl_init: test_inspector_opengl_init,
                opengl_render: test_inspector_render,
                opengl_shutdown: test_counting_inspector_shutdown,
                opengl_shutdown_complete: false,
            };
        }
        assert_eq!(counters.shutdown, 1);
        assert_eq!(counters.freed, 1);
    }

    #[test]
    fn ghostty_inspector_guard_reports_shutdown_result_and_skips_successful_retry() {
        let mut counters = InspectorDropCounters::default();
        let inspector = &mut counters as *mut InspectorDropCounters as GhosttyInspector;
        {
            let mut guard = GhosttyInspectorGuard {
                inspector,
                free: test_counting_inspector_free,
                set_focus: test_inspector_set_bool,
                set_content_scale: test_inspector_set_content_scale,
                set_size: test_inspector_set_size,
                mouse_button: test_inspector_mouse_button,
                mouse_pos: test_inspector_mouse_pos,
                mouse_scroll: test_inspector_mouse_scroll,
                key: test_inspector_key,
                text: test_inspector_text,
                opengl_init: test_inspector_opengl_init,
                opengl_render: test_inspector_render,
                opengl_shutdown: test_counting_inspector_shutdown,
                opengl_shutdown_complete: false,
            };
            assert!(guard.shutdown_opengl());
        }
        assert_eq!(counters.shutdown, 1);
        assert_eq!(counters.freed, 1);
    }

    #[test]
    fn ghostty_inspector_guard_retries_failed_shutdown_on_drop() {
        let mut counters = InspectorDropCounters::default();
        let inspector = &mut counters as *mut InspectorDropCounters as GhosttyInspector;
        {
            let mut guard = GhosttyInspectorGuard {
                inspector,
                free: test_counting_inspector_free,
                set_focus: test_inspector_set_bool,
                set_content_scale: test_inspector_set_content_scale,
                set_size: test_inspector_set_size,
                mouse_button: test_inspector_mouse_button,
                mouse_pos: test_inspector_mouse_pos,
                mouse_scroll: test_inspector_mouse_scroll,
                key: test_inspector_key,
                text: test_inspector_text,
                opengl_init: test_inspector_opengl_init,
                opengl_render: test_inspector_render,
                opengl_shutdown: test_counting_inspector_shutdown_false,
                opengl_shutdown_complete: false,
            };
            assert!(!guard.shutdown_opengl());
        }
        assert_eq!(counters.shutdown, 2);
        assert_eq!(counters.freed, 1);
    }

    #[test]
    fn ghostty_inspector_guard_reports_render_result() {
        let mut counters = InspectorDropCounters {
            render_result: true,
            ..Default::default()
        };
        let inspector = &mut counters as *mut InspectorDropCounters as GhosttyInspector;
        let guard = GhosttyInspectorGuard {
            inspector,
            free: test_inspector_free,
            set_focus: test_inspector_set_bool,
            set_content_scale: test_inspector_set_content_scale,
            set_size: test_inspector_set_size,
            mouse_button: test_inspector_mouse_button,
            mouse_pos: test_inspector_mouse_pos,
            mouse_scroll: test_inspector_mouse_scroll,
            key: test_inspector_key,
            text: test_inspector_text,
            opengl_init: test_inspector_opengl_init,
            opengl_render: test_inspector_render,
            opengl_shutdown: test_inspector_void,
            opengl_shutdown_complete: false,
        };

        assert!(guard.render());
        assert_eq!(counters.render, 1);

        counters.render_result = false;
        assert!(!guard.render());
        assert_eq!(counters.render, 2);
    }

    #[test]
    fn ghostty_inspector_guard_reports_setter_results() {
        let mut counters = InspectorDropCounters::default();
        let inspector = &mut counters as *mut InspectorDropCounters as GhosttyInspector;
        let mut guard = GhosttyInspectorGuard {
            inspector,
            free: test_inspector_free,
            set_focus: test_inspector_set_bool,
            set_content_scale: test_inspector_set_content_scale,
            set_size: test_inspector_set_size,
            mouse_button: test_inspector_mouse_button,
            mouse_pos: test_inspector_mouse_pos,
            mouse_scroll: test_inspector_mouse_scroll,
            key: test_inspector_key,
            text: test_inspector_text,
            opengl_init: test_inspector_opengl_init,
            opengl_render: test_inspector_render,
            opengl_shutdown: test_inspector_void,
            opengl_shutdown_complete: false,
        };

        assert!(guard.set_focus(true));
        guard.set_focus = test_inspector_set_bool_false;
        assert!(!guard.set_focus(false));

        assert!(guard.set_content_scale(2.0, 2.0));
        guard.set_content_scale = test_inspector_set_content_scale_false;
        assert!(!guard.set_content_scale(1.0, 1.0));

        assert!(guard.set_size(120, 40));
        guard.set_size = test_inspector_set_size_false;
        assert!(!guard.set_size(80, 24));
    }

    #[test]
    fn ghostty_inspector_guard_reports_key_result() {
        let mut counters = InspectorDropCounters::default();
        let inspector = &mut counters as *mut InspectorDropCounters as GhosttyInspector;
        let mut guard = GhosttyInspectorGuard {
            inspector,
            free: test_inspector_free,
            set_focus: test_inspector_set_bool,
            set_content_scale: test_inspector_set_content_scale,
            set_size: test_inspector_set_size,
            mouse_button: test_inspector_mouse_button,
            mouse_pos: test_inspector_mouse_pos,
            mouse_scroll: test_inspector_mouse_scroll,
            key: test_inspector_key,
            text: test_inspector_text,
            opengl_init: test_inspector_opengl_init,
            opengl_render: test_inspector_render,
            opengl_shutdown: test_inspector_void,
            opengl_shutdown_complete: false,
        };

        assert!(guard.key(GHOSTTY_ACTION_PRESS, GHOSTTY_KEY_ENTER, GHOSTTY_MODS_CTRL));
        guard.key = test_inspector_key_false;
        assert!(!guard.key(GHOSTTY_ACTION_PRESS, GHOSTTY_KEY_ENTER, GHOSTTY_MODS_CTRL));
    }

    #[test]
    fn ghostty_inspector_guard_reports_pointer_input_results() {
        let mut counters = InspectorDropCounters::default();
        let inspector = &mut counters as *mut InspectorDropCounters as GhosttyInspector;
        let mut guard = GhosttyInspectorGuard {
            inspector,
            free: test_inspector_free,
            set_focus: test_inspector_set_bool,
            set_content_scale: test_inspector_set_content_scale,
            set_size: test_inspector_set_size,
            mouse_button: test_inspector_mouse_button,
            mouse_pos: test_inspector_mouse_pos,
            mouse_scroll: test_inspector_mouse_scroll,
            key: test_inspector_key,
            text: test_inspector_text,
            opengl_init: test_inspector_opengl_init,
            opengl_render: test_inspector_render,
            opengl_shutdown: test_inspector_void,
            opengl_shutdown_complete: false,
        };

        assert!(guard.mouse_pos(10.0, 20.0));
        guard.mouse_pos = test_inspector_mouse_pos_false;
        assert!(!guard.mouse_pos(10.0, 20.0));

        assert!(guard.mouse_button(
            GHOSTTY_ACTION_PRESS,
            GHOSTTY_MOUSE_BUTTON_LEFT,
            GHOSTTY_MODS_SHIFT
        ));
        guard.mouse_button = test_inspector_mouse_button_false;
        assert!(!guard.mouse_button(
            GHOSTTY_ACTION_PRESS,
            GHOSTTY_MOUSE_BUTTON_LEFT,
            GHOSTTY_MODS_SHIFT
        ));

        assert!(guard.mouse_scroll(1.0, -1.0, 0));
        guard.mouse_scroll = test_inspector_mouse_scroll_false;
        assert!(!guard.mouse_scroll(1.0, -1.0, 0));
    }

    #[test]
    fn ghostty_inspector_guard_reports_text_result() {
        let mut counters = InspectorDropCounters::default();
        let inspector = &mut counters as *mut InspectorDropCounters as GhosttyInspector;
        let mut guard = GhosttyInspectorGuard {
            inspector,
            free: test_inspector_free,
            set_focus: test_inspector_set_bool,
            set_content_scale: test_inspector_set_content_scale,
            set_size: test_inspector_set_size,
            mouse_button: test_inspector_mouse_button,
            mouse_pos: test_inspector_mouse_pos,
            mouse_scroll: test_inspector_mouse_scroll,
            key: test_inspector_key,
            text: test_inspector_text,
            opengl_init: test_inspector_opengl_init,
            opengl_render: test_inspector_render,
            opengl_shutdown: test_inspector_void,
            opengl_shutdown_complete: false,
        };

        assert!(guard.text("hello"));
        guard.text = test_inspector_text_false;
        assert!(!guard.text("hello"));
        assert!(!guard.text("bad\0text"));
    }

    #[test]
    fn ghostty_embed_discovers_zig_out_internal_library() {
        let dir = tempfile::tempdir().expect("tempdir");
        let lib_dir = dir.path().join("zig-out/lib");
        std::fs::create_dir_all(&lib_dir).expect("lib dir");
        std::fs::write(lib_dir.join("libghostty-internal.so"), "").expect("so");

        assert_eq!(
            ghostty_library(dir.path()).as_deref(),
            Some(lib_dir.join("libghostty-internal.so").as_path())
        );
    }

    #[test]
    fn ghostty_embed_discovers_legacy_zig_out_internal_library() {
        let dir = tempfile::tempdir().expect("tempdir");
        let lib_dir = dir.path().join("zig-out/lib");
        std::fs::create_dir_all(&lib_dir).expect("lib dir");
        std::fs::write(lib_dir.join("ghostty-internal.so"), "").expect("so");

        assert_eq!(
            ghostty_library(dir.path()).as_deref(),
            Some(lib_dir.join("ghostty-internal.so").as_path())
        );
    }

    #[test]
    fn ghostty_embed_discovers_installed_internal_library() {
        let dir = tempfile::tempdir().expect("tempdir");
        let lib_dir = dir.path().join("lib");
        std::fs::create_dir_all(&lib_dir).expect("lib dir");
        std::fs::write(lib_dir.join("libghostty-internal.so"), "").expect("so");

        assert_eq!(
            ghostty_library(dir.path()).as_deref(),
            Some(lib_dir.join("libghostty-internal.so").as_path())
        );
    }

    #[test]
    fn ghostty_embed_infers_installed_root_from_library_path() {
        let dir = tempfile::tempdir().expect("tempdir");
        let include_dir = dir.path().join("include");
        let lib_dir = dir.path().join("lib");
        std::fs::create_dir_all(&include_dir).expect("include dir");
        std::fs::create_dir_all(&lib_dir).expect("lib dir");
        std::fs::write(include_dir.join("ghostty.h"), "/* ghostty */").expect("header");
        let library = lib_dir.join("libghostty-internal.so");
        std::fs::write(&library, "").expect("so");

        assert_eq!(
            ghostty_root_from_library_path(&library).as_deref(),
            Some(dir.path())
        );
    }

    #[test]
    fn ghostty_embed_infers_checkout_root_from_zig_out_library_path() {
        let dir = tempfile::tempdir().expect("tempdir");
        let include_dir = dir.path().join("include");
        let lib_dir = dir.path().join("zig-out/lib");
        std::fs::create_dir_all(&include_dir).expect("include dir");
        std::fs::create_dir_all(&lib_dir).expect("lib dir");
        std::fs::write(include_dir.join("ghostty.h"), "/* ghostty */").expect("header");
        let library = lib_dir.join("libghostty-internal.so");
        std::fs::write(&library, "").expect("so");

        assert_eq!(
            ghostty_root_from_library_path(&library).as_deref(),
            Some(dir.path())
        );
    }

    #[test]
    fn ghostty_runtime_callbacks_preserve_wakeup_userdata() {
        unsafe extern "C" fn wakeup(_userdata: *mut c_void) {}
        unsafe extern "C" fn action(
            _app: GhosttyApp,
            _target: GhosttyTarget,
            _action: GhosttyAction,
        ) -> bool {
            true
        }
        unsafe extern "C" fn read_clipboard(
            _userdata: *mut c_void,
            _clipboard: c_int,
            _request: *mut c_void,
        ) -> bool {
            false
        }
        unsafe extern "C" fn confirm_read_clipboard(
            _userdata: *mut c_void,
            _text: *const c_char,
            _request: *mut c_void,
            _request_type: c_int,
        ) {
        }
        unsafe extern "C" fn write_clipboard(
            _userdata: *mut c_void,
            _clipboard: c_int,
            _contents: *const GhosttyClipboardContent,
            _len: usize,
            _confirm: bool,
        ) {
        }
        unsafe extern "C" fn close_surface(_userdata: *mut c_void, _process_alive: bool) {}
        unsafe extern "C" fn redraw_surface(_userdata: *mut c_void) {}

        let userdata = 0x1234usize as *mut c_void;
        let config = GhosttyRuntimeConfig::with_callbacks(GhosttyRuntimeCallbacks {
            userdata,
            wakeup,
            action,
            read_clipboard,
            confirm_read_clipboard,
            write_clipboard,
            close_surface: Some(close_surface),
            redraw_surface,
            supports_selection_clipboard: true,
        });

        assert_eq!(config.userdata, userdata);
        assert_eq!(config.wakeup_cb as usize, wakeup as *const () as usize);
        assert_eq!(config.action_cb as usize, action as *const () as usize);
        assert_eq!(
            config.read_clipboard_cb as usize,
            read_clipboard as *const () as usize
        );
        assert_eq!(
            config.confirm_read_clipboard_cb as usize,
            confirm_read_clipboard as *const () as usize
        );
        assert_eq!(
            config.write_clipboard_cb as usize,
            write_clipboard as *const () as usize
        );
        assert_eq!(
            config.close_surface_cb.map(|callback| callback as usize),
            Some(close_surface as *const () as usize)
        );
        assert_eq!(
            config.redraw_surface_cb.map(|callback| callback as usize),
            Some(redraw_surface as *const () as usize)
        );
        assert!(config.supports_selection_clipboard);
    }

    #[test]
    fn ghostty_linux_platform_config_preserves_gl_callbacks() {
        unsafe extern "C" fn make_current(_userdata: *mut c_void) -> bool {
            true
        }
        unsafe extern "C" fn get_proc_address(
            _userdata: *mut c_void,
            _name: *const c_char,
        ) -> *mut c_void {
            ptr::null_mut()
        }
        unsafe extern "C" fn done_current(_userdata: *mut c_void) {}

        let userdata = 0x5678usize as *mut c_void;
        let platform =
            GhosttyPlatformLinux::new(userdata, make_current, get_proc_address, Some(done_current));
        assert_eq!(platform.userdata, userdata);
        assert_eq!(
            platform.make_current as usize,
            make_current as *const () as usize
        );
        assert_eq!(
            platform.get_proc_address as usize,
            get_proc_address as *const () as usize
        );
        assert_eq!(
            platform.done_current.map(|callback| callback as usize),
            Some(done_current as *const () as usize)
        );

        let mut config = unsafe { std::mem::zeroed::<GhosttySurfaceConfig>() };
        config.configure_linux_platform(platform);
        assert_eq!(config.platform_tag, GHOSTTY_PLATFORM_LINUX);
        config.set_wait_after_command(true);
        assert!(config.wait_after_command);

        let configured = unsafe { config.platform.linux_gl };
        assert_eq!(configured.userdata, userdata);
        assert_eq!(
            configured.make_current as usize,
            make_current as *const () as usize
        );
        assert_eq!(
            configured.get_proc_address as usize,
            get_proc_address as *const () as usize
        );
        assert_eq!(
            configured.done_current.map(|callback| callback as usize),
            Some(done_current as *const () as usize)
        );
    }

    #[test]
    fn ghostty_embed_host_check_smoke_with_real_library_when_requested() {
        if std::env::var("CMUX_GHOSTTY_EMBED_SMOKE").ok().as_deref() != Some("1") {
            eprintln!("SKIP: set CMUX_GHOSTTY_EMBED_SMOKE=1 to exercise ghostty-internal");
            return;
        }

        let check = host_check().expect("host check");
        assert!(check.library.exists(), "library was {:?}", check.library);
        assert_eq!(check.must_draw_from_app_thread, cfg!(target_os = "linux"));
    }

    #[test]
    fn ghostty_managed_config_override_loads_with_real_library_when_requested() {
        if std::env::var("CMUX_GHOSTTY_EMBED_SMOKE").ok().as_deref() != Some("1") {
            eprintln!("SKIP: set CMUX_GHOSTTY_EMBED_SMOKE=1 to exercise ghostty-internal");
            return;
        }

        let library = GhosttyLibrary::open_discovered().expect("open Ghostty");
        library.initialize().expect("initialize Ghostty");
        let config = library
            .load_default_config_with_string("copy-on-select = false")
            .expect("load managed config");
        assert_eq!(
            library.config_string(&config, "copy-on-select").as_deref(),
            Some("false")
        );
        let config = library
            .load_default_config_with_string("copy-on-select = clipboard")
            .expect("load clipboard config");
        assert_eq!(
            library.config_string(&config, "copy-on-select").as_deref(),
            Some("clipboard")
        );
        assert!(
            library.config_string(&config, "scrollbar").is_some(),
            "Ghostty scrollbar config must be queryable by the GTK host"
        );
    }
}
