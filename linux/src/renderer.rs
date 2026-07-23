use crate::app::{AppError, AppState};
use serde::Serialize;
use serde_json::{json, Value};
use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::OnceLock;

const GHOSTTY_VT_DEBUG_CELL_WIDTH: f64 = 10.0;
const GHOSTTY_VT_DEBUG_CELL_HEIGHT: f64 = 20.0;
const RENDER_GRID_SCROLLBACK_LINE_BUDGET: usize = 200;
const GTK4_DEVELOPMENT_PACKAGE_HINT: &str = "Install GTK4 development files: Fedora/RHEL `sudo dnf install gtk4-devel pkgconf-pkg-config`; Debian/Ubuntu `sudo apt install libgtk-4-dev pkg-config`; Arch `sudo pacman -S gtk4 pkgconf`; openSUSE `sudo zypper install gtk4-devel pkgconf-pkg-config`.";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
struct RenderGridRgb {
    r: u8,
    g: u8,
    b: u8,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
struct RenderGridStyle {
    fg: Option<RenderGridRgb>,
    bg: Option<RenderGridRgb>,
    bold: bool,
    italic: bool,
    faint: bool,
    blink: bool,
    inverse: bool,
    invisible: bool,
    underline: bool,
    strikethrough: bool,
    overline: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct RenderGridCell {
    ch: char,
    style: RenderGridStyle,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
enum RenderGridCursorStyle {
    #[default]
    Block,
    Underline,
    Bar,
}

impl RenderGridCursorStyle {
    fn as_str(self) -> &'static str {
        match self {
            Self::Block => "block",
            Self::Underline => "underline",
            Self::Bar => "bar",
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
enum RenderGridActiveScreen {
    #[default]
    Primary,
    Alternate,
}

impl RenderGridActiveScreen {
    fn as_str(self) -> &'static str {
        match self {
            Self::Primary => "primary",
            Self::Alternate => "alternate",
        }
    }
}

#[derive(Debug, Default)]
struct RenderGridBuffer {
    lines: Vec<Vec<Option<RenderGridCell>>>,
    row: usize,
    col: usize,
}

#[derive(Debug)]
struct RenderGridScreen {
    primary: RenderGridBuffer,
    alternate: RenderGridBuffer,
    active_screen: RenderGridActiveScreen,
    saved_primary_cursor: Option<(usize, usize)>,
    saved_alternate_cursor: Option<(usize, usize)>,
    cursor_visible: bool,
    cursor_style: RenderGridCursorStyle,
    cursor_blinking: bool,
    modes: HashSet<&'static str>,
    style: RenderGridStyle,
}

impl Default for RenderGridScreen {
    fn default() -> Self {
        Self {
            primary: RenderGridBuffer::default(),
            alternate: RenderGridBuffer::default(),
            active_screen: RenderGridActiveScreen::Primary,
            saved_primary_cursor: None,
            saved_alternate_cursor: None,
            cursor_visible: true,
            cursor_style: RenderGridCursorStyle::Block,
            cursor_blinking: false,
            modes: HashSet::new(),
            style: RenderGridStyle::default(),
        }
    }
}

#[derive(Debug, Clone, Serialize)]
struct RendererBackendStatus {
    name: &'static str,
    available: bool,
    active: bool,
    detail: String,
}

#[derive(Debug, Clone, Serialize)]
struct RendererDiagnostics {
    selected_backend: String,
    backends: Vec<RendererBackendStatus>,
    gtk4: GtkProbe,
    ghostty: GhosttyProbe,
    display: DisplayProbe,
}

#[derive(Debug, Clone, Serialize)]
struct GtkProbe {
    available: bool,
    feature_enabled: bool,
    pkg_config_available: bool,
    link_library_available: bool,
    development_files_available: bool,
    development_package_hint: &'static str,
    pkg_config_error: Option<String>,
    runtime_library: Option<String>,
    link_library: Option<String>,
    version: Option<String>,
    detail: String,
}

#[derive(Debug, Clone, Serialize)]
struct GhosttyProbe {
    header: Option<String>,
    internal_library: Option<String>,
    runtime_resources_dir: Option<String>,
    runtime_resources_source: Option<&'static str>,
    runtime_resources_present: bool,
    runtime_resources_missing: Vec<String>,
    runtime_themes_present: bool,
    runtime_i18n_present: bool,
    embedding_status: &'static str,
    embedding_header_has_linux_platform: bool,
    embedding_header_abi_version: Option<u32>,
    embedding_header_abi_version_matches: bool,
    embedding_header_linux_platform_value: Option<i32>,
    embedding_header_linux_platform_value_matches: bool,
    embedding_header_env_var_limit: Option<usize>,
    embedding_header_env_var_limit_matches: bool,
    embedding_header_keycode_native_mask: Option<u32>,
    embedding_header_keycode_native_mask_matches: bool,
    embedding_header_keycode_physical_key_flag: Option<u32>,
    embedding_header_keycode_physical_key_flag_matches: bool,
    embedding_header_has_app_thread_draw_contract: bool,
    embedding_header_has_redraw_surface_callback: bool,
    embedding_header_surface_env_vars_const: bool,
    embedding_header_init_argv_const: bool,
    embedding_header_ipc_new_window_arguments_const: bool,
    embedding_header_surface_metadata_returns_string: bool,
    embedding_library_present: bool,
    embedding_library_abi_version: Option<u32>,
    embedding_library_abi_version_matches: bool,
    embedding_library_platform: Option<i32>,
    embedding_library_platform_matches: bool,
    embedding_library_renderer_backend: Option<i32>,
    embedding_library_renderer_backend_matches: bool,
    embedding_library_env_var_limit: Option<usize>,
    embedding_library_env_var_limit_matches: bool,
    embedding_library_runtime_config_size: Option<usize>,
    embedding_library_surface_config_size: Option<usize>,
    embedding_library_platform_linux_size: Option<usize>,
    embedding_library_input_key_size: Option<usize>,
    embedding_library_target_size: Option<usize>,
    embedding_library_action_size: Option<usize>,
    embedding_library_text_size: Option<usize>,
    embedding_library_selection_size: Option<usize>,
    embedding_library_string_size: Option<usize>,
    embedding_library_surface_size_size: Option<usize>,
    embedding_library_diagnostic_size: Option<usize>,
    embedding_library_env_var_size: Option<usize>,
    embedding_library_clipboard_content_size: Option<usize>,
    embedding_library_input_trigger_size: Option<usize>,
    embedding_library_ipc_target_size: Option<usize>,
    embedding_library_ipc_action_size: Option<usize>,
    embedding_library_layout_sizes_match: bool,
    embedding_library_runtime_config_align: Option<usize>,
    embedding_library_surface_config_align: Option<usize>,
    embedding_library_platform_linux_align: Option<usize>,
    embedding_library_input_key_align: Option<usize>,
    embedding_library_target_align: Option<usize>,
    embedding_library_action_align: Option<usize>,
    embedding_library_text_align: Option<usize>,
    embedding_library_selection_align: Option<usize>,
    embedding_library_string_align: Option<usize>,
    embedding_library_surface_size_align: Option<usize>,
    embedding_library_diagnostic_align: Option<usize>,
    embedding_library_env_var_align: Option<usize>,
    embedding_library_clipboard_content_align: Option<usize>,
    embedding_library_input_trigger_align: Option<usize>,
    embedding_library_ipc_target_align: Option<usize>,
    embedding_library_ipc_action_align: Option<usize>,
    embedding_library_layout_alignments_match: bool,
    embedding_library_layout_fingerprint: Option<u64>,
    embedding_expected_layout_fingerprint: u64,
    embedding_library_layout_fingerprint_matches: bool,
    embedding_library_constants_fingerprint: Option<u64>,
    embedding_expected_constants_fingerprint: u64,
    embedding_library_constants_fingerprint_matches: bool,
    embedding_library_supports_linux_platform: Option<bool>,
    embedding_library_must_draw_from_app_thread: Option<bool>,
    embedding_library_info_query_error: Option<String>,
    embedding_library_info_direct_matches_query: bool,
    embedding_app_must_draw_from_app_thread: Option<bool>,
    embedding_app_must_draw_query_error: Option<String>,
    linux_embedding_supported: bool,
    embedding_symbols_verified: bool,
    embedding_missing_symbols: Vec<String>,
    embedding_darwin_symbols_hidden: bool,
    embedding_darwin_symbols_present: Vec<String>,
    embedding_internal_symbols_hidden: bool,
    embedding_internal_symbols_present: Vec<String>,
    embedding_unexpected_export_symbols_hidden: bool,
    embedding_unexpected_export_symbols_present: Vec<String>,
    embedding_unexpected_export_symbol_count: usize,
    embedding_library_loadable: bool,
    embedding_load_error: Option<String>,
    vt_header: Option<String>,
    vt_library: Option<String>,
    vt_pkg_config: Option<String>,
    vt_symbols_verified: bool,
    vt_missing_symbols: Vec<String>,
    vt_supported: bool,
    detail: String,
}

const REQUIRED_GHOSTTY_SYMBOLS: &[&str] = crate::ghostty_embed::REQUIRED_GHOSTTY_EMBED_SYMBOLS;

const DARWIN_ONLY_GHOSTTY_SYMBOLS: &[&str] = &[
    "ghostty_surface_set_display_id",
    "ghostty_surface_quicklook_font",
    "ghostty_surface_quicklook_word",
    "ghostty_inspector_metal_init",
    "ghostty_inspector_metal_render",
    "ghostty_inspector_metal_shutdown",
    "ghostty_set_window_background_blur",
];

const INTERNAL_GHOSTTY_SYMBOL_PREFIXES: &[&str] = &["ghostty_simd_"];

const OPTIONAL_GHOSTTY_EXPORT_SYMBOLS: &[&str] = &[
    "ghostty_benchmark_cli",
    "ghostty_cli_try_action",
    "ghostty_config_clone",
    "ghostty_config_key_is_binding",
    "ghostty_config_trigger",
    "ghostty_info",
    "ghostty_translate",
];

const MAX_UNEXPECTED_GHOSTTY_EXPORT_SYMBOLS: usize = 32;

const REQUIRED_GHOSTTY_VT_SYMBOLS: &[&str] = &[
    "ghostty_terminal_new",
    "ghostty_terminal_free",
    "ghostty_terminal_vt_write",
    "ghostty_render_state_new",
    "ghostty_render_state_update",
    "ghostty_render_state_free",
    "ghostty_render_state_get",
    "ghostty_render_state_row_iterator_new",
    "ghostty_render_state_row_iterator_free",
    "ghostty_render_state_row_iterator_next",
    "ghostty_render_state_row_get",
    "ghostty_render_state_row_cells_new",
    "ghostty_render_state_row_cells_free",
    "ghostty_render_state_row_cells_next",
    "ghostty_render_state_row_cells_get",
    "ghostty_formatter_terminal_new",
    "ghostty_formatter_format_alloc",
    "ghostty_formatter_free",
    "ghostty_free",
];

#[derive(Debug, Clone, Serialize)]
struct DisplayProbe {
    available: bool,
    wayland_display: Option<String>,
    x11_display: Option<String>,
}

#[derive(Debug, Clone)]
struct GhosttyRuntimeResources {
    dir: Option<PathBuf>,
    source: Option<GhosttyRuntimeResourceSource>,
    present: bool,
    missing: Vec<String>,
    themes_present: bool,
    i18n_present: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum GhosttyRuntimeResourceSource {
    Env,
    GhosttyReported,
    LibraryRelative,
    CheckoutRelative,
}

impl GhosttyRuntimeResourceSource {
    fn as_str(self) -> &'static str {
        match self {
            Self::Env => "GHOSTTY_RESOURCES_DIR",
            Self::GhosttyReported => "ghostty_resources_dir",
            Self::LibraryRelative => "library_relative",
            Self::CheckoutRelative => "checkout_relative",
        }
    }
}

#[derive(Debug, Clone)]
struct GhosttyRuntimeResourceCandidate {
    dir: PathBuf,
    source: GhosttyRuntimeResourceSource,
}

pub fn snapshot_value(app: &mut AppState, params: &Value) -> Result<Value, AppError> {
    if let Some(window) = params.get("window_id").or_else(|| params.get("window")) {
        return app.with_renderer_window(window, |app| snapshot_current_window_value(app, params));
    }
    snapshot_current_window_value(app, params)
}

fn snapshot_current_window_value(app: &mut AppState, params: &Value) -> Result<Value, AppError> {
    let backend = selected_backend(params)?;
    let window = app.handle("window.current", &json!({}))?;
    let focused = app.handle("system.identify", &json!({}))?;
    let workspaces = app.handle("workspace.list", &json!({}))?;
    let workspace_groups = app.handle("workspace.group.list", &json!({}))?;
    let panes = app.handle("pane.list", &json!({}))?;
    let surfaces = app.handle("surface.list", &json!({}))?;
    let tree = app.handle("system.tree", &json!({}))?;
    let window_surfaces = window_surface_inventory(&tree);
    let layout = app.handle("debug.layout", &json!({}))?;
    let canvas = app.handle("canvas.info", &json!({}))?;
    let sidebar = sidebar_snapshot(app)?;
    let custom_sidebar = app.custom_sidebar_snapshot();
    let right_sidebar = right_sidebar_snapshot(app)?;
    let notifications = app.handle("notification.list", &json!({}))?;
    let command_palette = app.handle("debug.command_palette.results", &json!({"limit": 10}))?;
    let shortcut_help = app.handle("help.shortcuts", &json!({}))?;
    let mut views = surface_views(&layout, &surfaces);
    if renderer_backend_uses_text_fallback(&backend) {
        attach_render_grid_fallbacks(app, &mut views)?;
    }
    if backend == "ghostty-vt" {
        attach_ghostty_vt_render_states(app, &mut views)?;
    }
    let diagnostics = cached_diagnostics_value_for_backend(&backend)?;

    Ok(json!({
        "renderer": {
            "backend": backend.clone(),
            "state": "model-ready",
            "frame_source": "debug.layout",
            "surface_view_count": views.as_array().map(Vec::len).unwrap_or(0)
        },
        "window": window,
        "focused": focused.get("focused").cloned().unwrap_or(focused),
        "workspaces": workspaces.get("workspaces").cloned().unwrap_or_else(|| json!([])),
        "workspace_groups": workspace_groups.get("groups").cloned().unwrap_or_else(|| json!([])),
        "panes": panes.get("panes").cloned().unwrap_or_else(|| json!([])),
        "surfaces": surfaces.get("surfaces").cloned().unwrap_or_else(|| json!([])),
        "window_surfaces": window_surfaces,
        "surface_views": views,
        "layout": layout.get("layout").cloned().unwrap_or(layout),
        "canvas": canvas,
        "sidebar": sidebar,
        "custom_sidebar": custom_sidebar,
        "right_sidebar": right_sidebar,
        "notifications": notifications.get("notifications").cloned().unwrap_or_else(|| json!([])),
        "command_palette": command_palette,
        "shortcut_help": shortcut_help,
        "close_confirmations": app.close_confirmation_requests_value(),
        "config": {
            "reload_generation": app.config_reload_generation(),
            "config_reload_generation": app.config_reload_generation(),
            "app": app.app_workspace_settings_value(),
            "terminal": app.terminal_interaction_settings_value()
        },
        "diagnostics": diagnostics,
    }))
}

fn window_surface_inventory(tree: &Value) -> Value {
    Value::Array(
        tree.get("windows")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .flat_map(|window| {
                window
                    .get("workspaces")
                    .and_then(Value::as_array)
                    .into_iter()
                    .flatten()
            })
            .flat_map(|workspace| {
                workspace
                    .get("panes")
                    .and_then(Value::as_array)
                    .into_iter()
                    .flatten()
            })
            .flat_map(|pane| {
                pane.get("surfaces")
                    .and_then(Value::as_array)
                    .into_iter()
                    .flatten()
            })
            .cloned()
            .collect(),
    )
}

pub fn diagnostics_value(params: &Value) -> Result<Value, AppError> {
    let backend = selected_backend(params)?;
    diagnostics_value_for_backend(&backend)
}

fn diagnostics_value_for_backend(backend: &str) -> Result<Value, AppError> {
    serde_json::to_value(diagnostics_for_backend(backend))
        .map_err(|err| AppError::internal(err.to_string()))
}

fn cached_diagnostics_value_for_backend(backend: &str) -> Result<Value, AppError> {
    static PROBES: OnceLock<(GtkProbe, GhosttyProbe, DisplayProbe)> = OnceLock::new();
    let (gtk4, ghostty, display) =
        PROBES.get_or_init(|| (probe_gtk4(), probe_ghostty(), probe_display()));
    serde_json::to_value(diagnostics_from_probes(
        backend,
        gtk4.clone(),
        ghostty.clone(),
        display.clone(),
    ))
    .map_err(|err| AppError::internal(err.to_string()))
}

fn sidebar_snapshot(app: &mut AppState) -> Result<Value, AppError> {
    let mut state = app.handle("sidebar.state", &json!({}))?;
    let statuses = app.handle("sidebar.status.list", &json!({}))?;
    let logs = app.handle("sidebar.log.list", &json!({"limit": 5}))?;

    if let Some(object) = state.as_object_mut() {
        object.insert(
            "statuses".to_string(),
            statuses
                .get("statuses")
                .cloned()
                .unwrap_or_else(|| json!([])),
        );
        object.insert(
            "logs".to_string(),
            logs.get("logs").cloned().unwrap_or_else(|| json!([])),
        );
    }

    Ok(state)
}

fn right_sidebar_snapshot(app: &mut AppState) -> Result<Value, AppError> {
    let mut state = app.handle("sidebar.right", &json!({"action": "mode"}))?;
    let include_feed = state.get("visible").and_then(Value::as_bool) == Some(true)
        && state.get("mode").and_then(Value::as_str) == Some("feed");
    let feed_items = if include_feed {
        app.handle("feed.list", &json!({"limit": 20}))?
            .get("items")
            .cloned()
            .unwrap_or_else(|| json!([]))
    } else {
        json!([])
    };
    if let Some(object) = state.as_object_mut() {
        object.insert("feed_items".to_string(), feed_items);
    }
    Ok(state)
}

fn diagnostics_for_backend(backend: &str) -> RendererDiagnostics {
    diagnostics_from_probes(backend, probe_gtk4(), probe_ghostty(), probe_display())
}

fn diagnostics_from_probes(
    backend: &str,
    gtk4: GtkProbe,
    ghostty: GhosttyProbe,
    display: DisplayProbe,
) -> RendererDiagnostics {
    let ghostty_available = ghostty_backend_available(
        ghostty.linux_embedding_supported,
        ghostty.runtime_resources_present,
    );
    let ghostty_vt_available = ghostty.vt_supported;
    let gtk_available = gtk4.available && display.available;
    let selected = backend.to_string();

    RendererDiagnostics {
        selected_backend: selected.clone(),
        backends: vec![
            RendererBackendStatus {
                name: "core",
                available: true,
                active: selected == "core",
                detail: "headless renderer model backed by the Rust app core".to_string(),
            },
            RendererBackendStatus {
                name: "gtk",
                available: gtk_available,
                active: selected == "gtk",
                detail: if gtk_available {
                    "GTK4 and a display session are available for the Linux app shell".to_string()
                } else if !gtk4.feature_enabled {
                    "cmux was built without the GTK renderer feature; rebuild with `--features gtk` to run the GTK shell".to_string()
                } else if gtk4.available {
                    "GTK4 is installed, but no DISPLAY or WAYLAND_DISPLAY is set".to_string()
                } else {
                    gtk4.detail.clone()
                },
            },
            RendererBackendStatus {
                name: "ghostty",
                available: ghostty_available,
                active: selected == "ghostty",
                detail: ghostty.detail.clone(),
            },
            RendererBackendStatus {
                name: "ghostty-vt",
                available: ghostty_vt_available,
                active: selected == "ghostty-vt",
                detail: if ghostty_vt_available {
                    "Ghostty portable VT core is available for parser/render-state integration"
                        .to_string()
                } else {
                    "libghostty-vt was not found; run `zig build -Demit-lib-vt=true` in the Ghostty checkout".to_string()
                },
            },
        ],
        gtk4,
        ghostty,
        display,
    }
}

fn surface_views(layout: &Value, surfaces: &Value) -> Value {
    let all_surface_rows = surfaces
        .get("surfaces")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .collect::<Vec<_>>();
    let surface_rows = all_surface_rows
        .iter()
        .copied()
        .filter_map(|surface| {
            let id = surface
                .get("surface_id")
                .or_else(|| surface.get("id"))?
                .as_str()?;
            Some((id.to_string(), surface))
        })
        .collect::<HashMap<_, _>>();

    let views = layout
        .pointer("/layout/selectedPanels")
        .or_else(|| layout.get("selectedPanels"))
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .map(|panel| {
            let surface_id = panel
                .get("surface_id")
                .or_else(|| panel.get("surfaceId"))
                .and_then(Value::as_str)
                .unwrap_or_default();
            let pane_id = panel
                .get("pane_id")
                .or_else(|| panel.get("paneId"))
                .and_then(Value::as_str)
                .unwrap_or_default();
            let surface = surface_rows.get(surface_id);
            let tabs = all_surface_rows
                .iter()
                .filter(|row| {
                    row.get("pane_id").and_then(Value::as_str) == Some(pane_id)
                })
                .enumerate()
                .map(|(index, row)| {
                    let tab_surface_id = row
                        .get("surface_id")
                        .or_else(|| row.get("id"))
                        .and_then(Value::as_str)
                        .unwrap_or_default();
                    json!({
                        "index": index,
                        "surface_id": row.get("surface_id").or_else(|| row.get("id")).cloned().unwrap_or(Value::Null),
                        "surface_ref": row.get("surface_ref").or_else(|| row.get("ref")).cloned().unwrap_or(Value::Null),
                        "title": row.get("title").cloned().unwrap_or_else(|| json!("Surface")),
                        "kind": row.get("type").or_else(|| row.get("kind")).cloned().unwrap_or_else(|| json!("terminal")),
                        "selected": tab_surface_id == surface_id,
                        "pinned": row.get("pinned").cloned().unwrap_or_else(|| json!(false)),
                        "unread": row.get("unread").cloned().unwrap_or_else(|| json!(false))
                    })
                })
                .collect::<Vec<_>>();
            let tab_count = tabs.len();
            let mut row = json!({
                "pane_id": panel.get("pane_id").or_else(|| panel.get("paneId")).cloned().unwrap_or(Value::Null),
                "pane_ref": panel.get("pane_ref").or_else(|| panel.get("paneRef")).cloned().unwrap_or(Value::Null),
                "surface_id": panel.get("surface_id").or_else(|| panel.get("surfaceId")).cloned().unwrap_or(Value::Null),
                "surface_ref": panel.get("surface_ref").or_else(|| panel.get("surfaceRef")).cloned().unwrap_or(Value::Null),
                "workspace_id": surface.and_then(|row| row.get("workspace_id")).cloned().unwrap_or(Value::Null),
                "workspace_ref": surface.and_then(|row| row.get("workspace_ref")).cloned().unwrap_or(Value::Null),
                "kind": panel.get("type").cloned().unwrap_or_else(|| json!("terminal")),
                "title": surface.and_then(|row| row.get("title")).cloned().unwrap_or(Value::Null),
                "url": surface.and_then(|row| row.get("url")).cloned().unwrap_or(Value::Null),
                "cwd": surface.and_then(|row| row.get("cwd")).cloned().unwrap_or(Value::Null),
                "current_directory": surface.and_then(|row| row.get("current_directory")).cloned().unwrap_or(Value::Null),
                "terminal_command": surface.and_then(|row| row.get("terminal_command")).cloned().unwrap_or(Value::Null),
                "terminal_initial_input": surface.and_then(|row| row.get("terminal_initial_input")).cloned().unwrap_or(Value::Null),
                "terminal_restore_output": surface.and_then(|row| row.get("terminal_restore_output")).cloned().unwrap_or(Value::Null),
                "terminal_env": surface.and_then(|row| row.get("terminal_env")).cloned().unwrap_or(Value::Null),
                "remote_tmux_manual_io": surface.and_then(|row| row.get("remote_tmux_manual_io")).cloned().unwrap_or_else(|| json!(false)),
                "embedded_terminal_size": surface.and_then(|row| row.get("embedded_terminal_size")).cloned().unwrap_or(Value::Null),
                "terminal_size": surface.and_then(|row| row.get("terminal_size")).cloned().unwrap_or(Value::Null),
                "terminal_size_limit": surface.and_then(|row| row.get("terminal_size_limit")).cloned().unwrap_or(Value::Null),
                "terminal_initial_size": surface.and_then(|row| row.get("terminal_initial_size")).cloned().unwrap_or(Value::Null),
                "terminal_cell_size": surface.and_then(|row| row.get("terminal_cell_size")).cloned().unwrap_or(Value::Null),
                "terminal_renderer_health": surface.and_then(|row| row.get("terminal_renderer_health")).cloned().unwrap_or(Value::Null),
                "terminal_prompt_title": surface.and_then(|row| row.get("terminal_prompt_title")).cloned().unwrap_or(Value::Null),
                "terminal_quit_timer": surface.and_then(|row| row.get("terminal_quit_timer")).cloned().unwrap_or(Value::Null),
                "terminal_float_window": surface.and_then(|row| row.get("terminal_float_window")).cloned().unwrap_or(Value::Null),
                "terminal_secure_input": surface.and_then(|row| row.get("terminal_secure_input")).cloned().unwrap_or(Value::Null),
                "terminal_color_change": surface.and_then(|row| row.get("terminal_color_change")).cloned().unwrap_or(Value::Null),
                "terminal_config_change_count": surface.and_then(|row| row.get("terminal_config_change_count")).cloned().unwrap_or(Value::Null),
                "terminal_progress": surface.and_then(|row| row.get("terminal_progress")).cloned().unwrap_or(Value::Null),
                "terminal_search": surface.and_then(|row| row.get("terminal_search")).cloned().unwrap_or(Value::Null),
                "terminal_scrollbar": surface.and_then(|row| row.get("terminal_scrollbar")).cloned().unwrap_or(Value::Null),
                "terminal_last_command": surface.and_then(|row| row.get("terminal_last_command")).cloned().unwrap_or(Value::Null),
                "resume_binding": surface.and_then(|row| row.get("resume_binding")).cloned().unwrap_or(Value::Null),
                "resume_restore_state": surface.and_then(|row| row.get("resume_restore_state")).cloned().unwrap_or(Value::Null),
                "agent_hibernation": surface.and_then(|row| row.get("agent_hibernation")).cloned().unwrap_or(Value::Null),
                "hibernated": surface.and_then(|row| row.get("hibernated")).cloned().unwrap_or_else(|| json!(false)),
                "preview": surface.and_then(|row| row.get("preview")).cloned().unwrap_or(Value::Null),
                "browser": surface.and_then(|row| row.get("browser")).cloned().unwrap_or(Value::Null),
                "project": surface.and_then(|row| row.get("project")).cloned().unwrap_or(Value::Null),
                "runtime_surface_ready": surface
                    .and_then(|row| row.get("runtime_surface_ready"))
                    .cloned()
                    .unwrap_or(Value::Null),
                "terminal_loading": surface
                    .and_then(|row| row.get("terminal_loading"))
                    .cloned()
                    .unwrap_or_else(|| json!(false)),
                "loading": surface
                    .and_then(|row| row.get("loading"))
                    .cloned()
                    .unwrap_or_else(|| json!(false)),
                "loading_message": surface
                    .and_then(|row| row.get("loading_message"))
                    .cloned()
                    .unwrap_or_else(|| json!("")),
                "state_seq": surface
                    .and_then(|row| row.get("state_seq").or_else(|| row.get("present_count")))
                    .cloned()
                    .unwrap_or_else(|| json!(0)),
                "focused": surface
                    .and_then(|row| row.get("focused"))
                    .and_then(Value::as_bool)
                    .unwrap_or(false),
                "frame": panel
                    .get("viewFrame")
                    .or_else(|| panel.get("paneFrame"))
                    .cloned()
                    .unwrap_or(Value::Null),
                "visible": !panel.get("hidden").and_then(Value::as_bool).unwrap_or(false)
            });
            if let Some(object) = row.as_object_mut() {
                object.insert(
                    "global_search_needle".to_string(),
                    surface
                        .and_then(|surface| surface.get("global_search_needle"))
                        .cloned()
                        .unwrap_or(Value::Null),
                );
                object.insert(
                    "settings".to_string(),
                    surface
                        .and_then(|surface| surface.get("settings"))
                        .cloned()
                        .unwrap_or(Value::Null),
                );
                object.insert(
                    "document".to_string(),
                    surface
                        .and_then(|surface| surface.get("document"))
                        .cloned()
                        .unwrap_or(Value::Null),
                );
                object.insert(
                    "agent_session".to_string(),
                    surface
                        .and_then(|surface| surface.get("agent_session"))
                        .cloned()
                        .unwrap_or(Value::Null),
                );
                object.insert(
                    "text_box".to_string(),
                    surface
                        .and_then(|surface| surface.get("text_box"))
                        .cloned()
                        .unwrap_or(Value::Null),
                );
                object.insert("tabs".to_string(), Value::Array(tabs));
                object.insert("tab_count".to_string(), json!(tab_count));
                for key in [
                    "terminal_last_app_action",
                    "terminal_last_window_action",
                    "terminal_last_tab_action",
                    "terminal_last_layout_action",
                    "terminal_last_ui_action",
                    "terminal_key_sequence",
                    "terminal_key_tables",
                    "terminal_key_table",
                    "terminal_on_screen_keyboard_requests",
                    "terminal_readonly",
                    "readonly",
                    "terminal_copy_mode_active",
                    "copy_mode_active",
                    "terminal_mouse_captured",
                    "mouse_captured",
                    "terminal_needs_confirm_quit",
                    "needs_confirm_quit",
                    "terminal_has_selection",
                    "has_selection",
                    "terminal_selection_text",
                    "terminal_cursor_shape",
                    "cursor_shape",
                    "terminal_cursor_visible",
                    "cursor_visible",
                    "terminal_mouse_over_link",
                    "mouse_over_link",
                    "terminal_mouse_over_link_url",
                    "mouse_over_link_url",
                    "terminal_config_reload_count",
                    "terminal_last_config_reload_soft",
                    "terminal_font_size",
                    "terminal_wait_after_command",
                ] {
                    object.insert(
                        key.to_string(),
                        surface
                            .and_then(|surface| surface.get(key))
                            .cloned()
                            .unwrap_or(Value::Null),
                    );
                }
            }
            row
        })
        .collect::<Vec<_>>();

    Value::Array(views)
}

fn attach_render_grid_fallbacks(app: &mut AppState, views: &mut Value) -> Result<(), AppError> {
    let Some(views) = views.as_array_mut() else {
        return Ok(());
    };
    for view in views {
        let is_terminal = view
            .get("kind")
            .and_then(Value::as_str)
            .map(|kind| kind == "terminal")
            .unwrap_or(false);
        if !is_terminal {
            continue;
        }
        let Some(surface_id) = view
            .get("surface_id")
            .and_then(Value::as_str)
            .map(str::to_string)
        else {
            continue;
        };
        let (cols, rows) = frame_terminal_size(view.get("frame"));
        let state_seq = view
            .get("state_seq")
            .or_else(|| view.get("present_count"))
            .and_then(Value::as_u64)
            .unwrap_or(0);
        let read = app.handle(
            "surface.read_text",
            &json!({"surface_id": surface_id.clone(), "raw": true}),
        );
        if let Some(object) = view.as_object_mut() {
            match read {
                Ok(value) => {
                    let text = value
                        .get("text")
                        .and_then(Value::as_str)
                        .unwrap_or_default();
                    object.insert(
                        "render_grid".to_string(),
                        render_grid_from_text(&surface_id, state_seq, cols, rows, text),
                    );
                }
                Err(err) => {
                    object.insert(
                        "render_grid_error".to_string(),
                        json!({"code": err.code, "message": err.message}),
                    );
                }
            }
        }
    }
    Ok(())
}

pub(crate) fn render_grid_from_text(
    surface_id: &str,
    state_seq: u64,
    columns: u16,
    rows: u16,
    text: &str,
) -> Value {
    render_grid_from_text_with_scrollback(
        surface_id,
        state_seq,
        columns,
        rows,
        text,
        RENDER_GRID_SCROLLBACK_LINE_BUDGET,
    )
}

pub(crate) fn render_grid_from_text_with_scrollback(
    surface_id: &str,
    state_seq: u64,
    columns: u16,
    rows: u16,
    text: &str,
    scrollback_budget: usize,
) -> Value {
    let columns = usize::from(columns.max(1));
    let rows = usize::from(rows.max(1));
    let screen = render_grid_screen(text);
    let lines = screen.render_lines();
    let viewport_start = lines.len().saturating_sub(rows);
    let viewport_end = viewport_start.saturating_add(rows);
    let mut styles = vec![json!({"id": 0})];
    let mut style_ids = HashMap::new();
    style_ids.insert(RenderGridStyle::default(), 0_u64);
    let row_spans = render_grid_spans_for_lines(
        &lines[viewport_start..lines.len().min(viewport_end)],
        columns,
        &mut styles,
        &mut style_ids,
    );
    let scrollback_start = viewport_start.saturating_sub(scrollback_budget);
    let scrollback_lines = &lines[scrollback_start..viewport_start];
    let scrollback_spans =
        render_grid_spans_for_lines(scrollback_lines, columns, &mut styles, &mut style_ids);
    let cursor_line = screen.cursor_row().min(lines.len().saturating_sub(1));
    let cursor = (cursor_line >= viewport_start && cursor_line < viewport_end).then(|| {
        json!({
            "row": cursor_line - viewport_start,
            "column": screen.cursor_col().min(columns.saturating_sub(1)),
            "visible": screen.cursor_visible,
            "style": screen.cursor_style.as_str(),
            "blinking": screen.cursor_blinking
        })
    });
    let modes = screen.modes_value();
    let mut value = json!({
        "format": "cmux.render-grid.v1",
        "parser": "renderer-text-fallback",
        "surface_id": surface_id,
        "state_seq": state_seq,
        "columns": columns,
        "rows": rows,
        "full": true,
        "cleared_rows": [],
        "styles": styles,
        "row_spans": row_spans,
        "active_screen": screen.active_screen.as_str(),
        "modes": modes,
        "scrollback_rows": scrollback_lines.len(),
        "scrollback_spans": scrollback_spans
    });
    if let Some(cursor) = cursor {
        if let Some(object) = value.as_object_mut() {
            object.insert("cursor".to_string(), cursor);
        }
    }
    value
}

fn render_grid_screen(text: &str) -> RenderGridScreen {
    let mut screen = RenderGridScreen::default();
    screen.current_line_mut();
    let chars = text.chars().collect::<Vec<_>>();
    let mut index = 0;
    while index < chars.len() {
        let ch = chars[index];
        if ch == '\u{1b}' {
            if chars.get(index + 1) == Some(&'[') {
                index += 2;
                let start = index;
                while index < chars.len() && !is_csi_final_byte(chars[index]) {
                    index += 1;
                }
                if index < chars.len() {
                    let sequence = chars[start..=index].iter().collect::<String>();
                    screen.apply_csi(&sequence);
                    index += 1;
                }
                continue;
            }
            if chars.get(index + 1) == Some(&']') {
                index += 2;
                while index < chars.len() {
                    if chars[index] == '\u{7}' {
                        index += 1;
                        break;
                    }
                    if chars[index] == '\u{1b}' && chars.get(index + 1) == Some(&'\\') {
                        index += 2;
                        break;
                    }
                    index += 1;
                }
                continue;
            }
            if let Some(next) = chars.get(index + 1).copied() {
                match next {
                    '7' => screen.save_cursor(),
                    '8' => screen.restore_cursor(),
                    'D' => screen.index(),
                    'E' => screen.next_line(),
                    'M' => screen.reverse_index(),
                    '=' => screen.set_mode("application_keypad", true),
                    '>' => screen.set_mode("application_keypad", false),
                    'c' => screen.reset(),
                    '(' | ')' | '*' | '+' | '-' | '.' | '/' => {
                        index += if chars.get(index + 2).is_some() { 3 } else { 2 };
                        continue;
                    }
                    _ => {}
                }
                index += 2;
            } else {
                index += 1;
            }
            continue;
        }
        match ch {
            '\r' => screen.carriage_return(),
            '\n' => screen.newline(),
            '\u{8}' => screen.backspace(),
            '\t' => screen.tab(),
            ch if ch.is_control() => {}
            ch => screen.put(ch),
        }
        index += 1;
    }
    screen
}

fn render_grid_spans_for_lines(
    lines: &[Vec<Option<RenderGridCell>>],
    columns: usize,
    styles: &mut Vec<Value>,
    style_ids: &mut HashMap<RenderGridStyle, u64>,
) -> Vec<Value> {
    let mut spans = Vec::new();
    for (row, line) in lines.iter().enumerate() {
        append_render_grid_line_spans(row, line, columns, styles, style_ids, &mut spans);
    }
    spans
}

fn append_render_grid_line_spans(
    row: usize,
    line: &[Option<RenderGridCell>],
    columns: usize,
    styles: &mut Vec<Value>,
    style_ids: &mut HashMap<RenderGridStyle, u64>,
    spans: &mut Vec<Value>,
) {
    let mut start_column = 0;
    let mut active_style: Option<RenderGridStyle> = None;
    let mut text = String::new();
    let limit = line.len().min(columns);
    for (column, cell) in line.iter().take(limit).enumerate() {
        let Some(cell) = cell else {
            flush_render_grid_span(
                row,
                start_column,
                active_style,
                &mut text,
                styles,
                style_ids,
                spans,
            );
            active_style = None;
            continue;
        };
        if active_style != Some(cell.style) {
            flush_render_grid_span(
                row,
                start_column,
                active_style,
                &mut text,
                styles,
                style_ids,
                spans,
            );
            active_style = Some(cell.style);
            start_column = column;
        }
        text.push(cell.ch);
    }
    flush_render_grid_span(
        row,
        start_column,
        active_style,
        &mut text,
        styles,
        style_ids,
        spans,
    );
}

fn flush_render_grid_span(
    row: usize,
    column: usize,
    style: Option<RenderGridStyle>,
    text: &mut String,
    styles: &mut Vec<Value>,
    style_ids: &mut HashMap<RenderGridStyle, u64>,
    spans: &mut Vec<Value>,
) {
    if text.is_empty() {
        return;
    }
    let style = style.unwrap_or_default();
    if text.trim_end().is_empty() && style == RenderGridStyle::default() {
        text.clear();
        return;
    }
    let cell_width = text.chars().count();
    let style_id = render_grid_style_id(style, styles, style_ids);
    spans.push(json!({
        "row": row,
        "column": column,
        "style_id": style_id,
        "text": std::mem::take(text),
        "cell_width": cell_width
    }));
}

fn render_grid_style_id(
    style: RenderGridStyle,
    styles: &mut Vec<Value>,
    style_ids: &mut HashMap<RenderGridStyle, u64>,
) -> u64 {
    if let Some(id) = style_ids.get(&style) {
        return *id;
    }
    let id = styles.len() as u64;
    style_ids.insert(style, id);
    styles.push(render_grid_style_value(id, style));
    id
}

fn render_grid_style_value(id: u64, style: RenderGridStyle) -> Value {
    let mut value = serde_json::Map::new();
    value.insert("id".to_string(), json!(id));
    if let Some(fg) = style.fg {
        value.insert("fg".to_string(), render_grid_rgb_value(fg));
    }
    if let Some(bg) = style.bg {
        value.insert("bg".to_string(), render_grid_rgb_value(bg));
    }
    for (key, enabled) in [
        ("bold", style.bold),
        ("italic", style.italic),
        ("faint", style.faint),
        ("blink", style.blink),
        ("inverse", style.inverse),
        ("invisible", style.invisible),
        ("underline", style.underline),
        ("strikethrough", style.strikethrough),
        ("overline", style.overline),
    ] {
        if enabled {
            value.insert(key.to_string(), json!(true));
        }
    }
    Value::Object(value)
}

fn render_grid_rgb_value(rgb: RenderGridRgb) -> Value {
    json!({"r": rgb.r, "g": rgb.g, "b": rgb.b})
}

impl RenderGridBuffer {
    fn current_line_mut(&mut self) -> &mut Vec<Option<RenderGridCell>> {
        if self.lines.is_empty() {
            self.lines.push(Vec::new());
        }
        while self.lines.len() <= self.row {
            self.lines.push(Vec::new());
        }
        &mut self.lines[self.row]
    }

    fn clear(&mut self) {
        self.lines.clear();
        self.row = 0;
        self.col = 0;
        self.current_line_mut();
    }

    fn render_lines(&self) -> Vec<Vec<Option<RenderGridCell>>> {
        let mut lines = self.lines.clone();
        while lines.len() <= self.row {
            lines.push(Vec::new());
        }
        if lines.is_empty() {
            lines.push(Vec::new());
        }
        for line in &mut lines {
            while line.last().is_some_and(|cell| match cell {
                Some(cell) => default_blank_cell(*cell),
                None => true,
            }) {
                line.pop();
            }
        }
        lines
    }
}

impl RenderGridScreen {
    fn active_buffer(&self) -> &RenderGridBuffer {
        match self.active_screen {
            RenderGridActiveScreen::Primary => &self.primary,
            RenderGridActiveScreen::Alternate => &self.alternate,
        }
    }

    fn active_buffer_mut(&mut self) -> &mut RenderGridBuffer {
        match self.active_screen {
            RenderGridActiveScreen::Primary => &mut self.primary,
            RenderGridActiveScreen::Alternate => &mut self.alternate,
        }
    }

    fn current_line_mut(&mut self) -> &mut Vec<Option<RenderGridCell>> {
        self.active_buffer_mut().current_line_mut()
    }

    fn cursor_row(&self) -> usize {
        self.active_buffer().row
    }

    fn cursor_col(&self) -> usize {
        self.active_buffer().col
    }

    fn put(&mut self, ch: char) {
        let style = self.style;
        let buffer = self.active_buffer_mut();
        let col = buffer.col;
        let line = buffer.current_line_mut();
        while line.len() < col {
            line.push(None);
        }
        let cell = Some(RenderGridCell { ch, style });
        if col < line.len() {
            line[col] = cell;
        } else {
            line.push(cell);
        }
        buffer.col += 1;
    }

    fn tab(&mut self) {
        let spaces = 4 - (self.cursor_col() % 4);
        for _ in 0..spaces {
            self.put(' ');
        }
    }

    fn backspace(&mut self) {
        let buffer = self.active_buffer_mut();
        buffer.col = buffer.col.saturating_sub(1);
    }

    fn carriage_return(&mut self) {
        self.active_buffer_mut().col = 0;
    }

    fn newline(&mut self) {
        let buffer = self.active_buffer_mut();
        buffer.row += 1;
        buffer.col = 0;
        self.current_line_mut();
    }

    fn index(&mut self) {
        let buffer = self.active_buffer_mut();
        buffer.row += 1;
        self.current_line_mut();
    }

    fn next_line(&mut self) {
        self.newline();
    }

    fn reverse_index(&mut self) {
        let buffer = self.active_buffer_mut();
        buffer.row = buffer.row.saturating_sub(1);
        self.current_line_mut();
    }

    fn reset(&mut self) {
        *self = Self::default();
        self.current_line_mut();
    }

    fn apply_csi(&mut self, sequence: &str) {
        let Some(final_byte) = sequence.chars().last() else {
            return;
        };
        let parameter_text = sequence.trim_end_matches(final_byte).trim_end();
        let private = parameter_text.starts_with('?');
        let params = csi_params(parameter_text);
        match final_byte {
            'm' => self.apply_sgr(&params),
            'q' => self.apply_cursor_style(&params),
            'A' => {
                let count = csi_count(&params);
                let buffer = self.active_buffer_mut();
                buffer.row = buffer.row.saturating_sub(count);
            }
            'B' => {
                let count = csi_count(&params);
                let buffer = self.active_buffer_mut();
                buffer.row += count;
                self.current_line_mut();
            }
            'C' => self.active_buffer_mut().col += csi_count(&params),
            'D' => {
                let count = csi_count(&params);
                let buffer = self.active_buffer_mut();
                buffer.col = buffer.col.saturating_sub(count);
            }
            'G' => self.active_buffer_mut().col = csi_position(params.first().copied()),
            'H' | 'f' => {
                let row = csi_position(params.first().copied());
                let col = csi_position(params.get(1).copied());
                let buffer = self.active_buffer_mut();
                buffer.row = row;
                buffer.col = col;
                self.current_line_mut();
            }
            'J' => {
                self.clear_display(params.first().copied().unwrap_or(0));
            }
            'K' => self.clear_line(params.first().copied().unwrap_or(0)),
            'h' if private => self.apply_private_mode(&params, true),
            'l' if private => self.apply_private_mode(&params, false),
            's' => self.save_cursor(),
            'u' => self.restore_cursor(),
            _ => {}
        }
    }

    fn apply_private_mode(&mut self, params: &[usize], enable: bool) {
        for mode in params {
            match (*mode, enable) {
                (1, _) => self.set_mode("application_cursor_keys", enable),
                (7, _) => self.set_mode("wraparound", enable),
                (12, _) => self.cursor_blinking = enable,
                (25, true) => self.cursor_visible = true,
                (25, false) => self.cursor_visible = false,
                (47 | 1047, true) => self.enter_alternate_screen(false),
                (47 | 1047, false) => self.leave_alternate_screen(false),
                (1048, true) => self.save_cursor(),
                (1048, false) => self.restore_cursor(),
                (1049, true) => self.enter_alternate_screen(true),
                (1049, false) => self.leave_alternate_screen(true),
                (1000, _) => self.set_mode("mouse_button_tracking", enable),
                (1002, _) => self.set_mode("mouse_drag_tracking", enable),
                (1003, _) => self.set_mode("mouse_any_tracking", enable),
                (1004, _) => self.set_mode("focus_events", enable),
                (1006, _) => self.set_mode("mouse_sgr", enable),
                (1015, _) => self.set_mode("mouse_urxvt", enable),
                (2004, _) => self.set_mode("bracketed_paste", enable),
                _ => {}
            }
        }
    }

    fn set_mode(&mut self, mode: &'static str, enable: bool) {
        if enable {
            self.modes.insert(mode);
        } else {
            self.modes.remove(mode);
        }
    }

    fn modes_value(&self) -> Vec<&'static str> {
        const ORDER: [&str; 10] = [
            "application_cursor_keys",
            "application_keypad",
            "wraparound",
            "bracketed_paste",
            "focus_events",
            "mouse_button_tracking",
            "mouse_drag_tracking",
            "mouse_any_tracking",
            "mouse_sgr",
            "mouse_urxvt",
        ];
        ORDER
            .iter()
            .copied()
            .filter(|mode| self.modes.contains(mode))
            .collect()
    }

    fn apply_cursor_style(&mut self, params: &[usize]) {
        let code = params.first().copied().unwrap_or(0);
        match code {
            0 | 1 => {
                self.cursor_style = RenderGridCursorStyle::Block;
                self.cursor_blinking = code == 1;
            }
            2 => {
                self.cursor_style = RenderGridCursorStyle::Block;
                self.cursor_blinking = false;
            }
            3 => {
                self.cursor_style = RenderGridCursorStyle::Underline;
                self.cursor_blinking = true;
            }
            4 => {
                self.cursor_style = RenderGridCursorStyle::Underline;
                self.cursor_blinking = false;
            }
            5 => {
                self.cursor_style = RenderGridCursorStyle::Bar;
                self.cursor_blinking = true;
            }
            6 => {
                self.cursor_style = RenderGridCursorStyle::Bar;
                self.cursor_blinking = false;
            }
            _ => {}
        }
    }

    fn enter_alternate_screen(&mut self, save_cursor: bool) {
        if save_cursor {
            self.save_cursor_for(RenderGridActiveScreen::Primary);
        }
        self.active_screen = RenderGridActiveScreen::Alternate;
        self.alternate.clear();
    }

    fn leave_alternate_screen(&mut self, restore_cursor: bool) {
        self.active_screen = RenderGridActiveScreen::Primary;
        if restore_cursor {
            self.restore_cursor_for(RenderGridActiveScreen::Primary);
        }
    }

    fn save_cursor(&mut self) {
        self.save_cursor_for(self.active_screen);
    }

    fn restore_cursor(&mut self) {
        self.restore_cursor_for(self.active_screen);
    }

    fn save_cursor_for(&mut self, screen: RenderGridActiveScreen) {
        let cursor = match screen {
            RenderGridActiveScreen::Primary => (self.primary.row, self.primary.col),
            RenderGridActiveScreen::Alternate => (self.alternate.row, self.alternate.col),
        };
        match screen {
            RenderGridActiveScreen::Primary => self.saved_primary_cursor = Some(cursor),
            RenderGridActiveScreen::Alternate => self.saved_alternate_cursor = Some(cursor),
        }
    }

    fn restore_cursor_for(&mut self, screen: RenderGridActiveScreen) {
        let cursor = match screen {
            RenderGridActiveScreen::Primary => self.saved_primary_cursor,
            RenderGridActiveScreen::Alternate => self.saved_alternate_cursor,
        };
        let Some((row, col)) = cursor else {
            return;
        };
        let buffer = match screen {
            RenderGridActiveScreen::Primary => &mut self.primary,
            RenderGridActiveScreen::Alternate => &mut self.alternate,
        };
        buffer.row = row;
        buffer.col = col;
        buffer.current_line_mut();
    }

    fn apply_sgr(&mut self, params: &[usize]) {
        let params = if params.is_empty() { &[0][..] } else { params };
        let mut index = 0;
        while index < params.len() {
            let code = params[index];
            match code {
                0 => self.style = RenderGridStyle::default(),
                1 => self.style.bold = true,
                2 => self.style.faint = true,
                3 => self.style.italic = true,
                4 => self.style.underline = true,
                5 => self.style.blink = true,
                7 => self.style.inverse = true,
                8 => self.style.invisible = true,
                9 => self.style.strikethrough = true,
                22 => {
                    self.style.bold = false;
                    self.style.faint = false;
                }
                23 => self.style.italic = false,
                24 => self.style.underline = false,
                25 => self.style.blink = false,
                27 => self.style.inverse = false,
                28 => self.style.invisible = false,
                29 => self.style.strikethrough = false,
                39 => self.style.fg = None,
                49 => self.style.bg = None,
                30..=37 => self.style.fg = Some(ansi_color((code - 30) as u8, false)),
                90..=97 => self.style.fg = Some(ansi_color((code - 90) as u8, true)),
                40..=47 => self.style.bg = Some(ansi_color((code - 40) as u8, false)),
                100..=107 => self.style.bg = Some(ansi_color((code - 100) as u8, true)),
                38 | 48 => {
                    if let Some((rgb, consumed)) = extended_ansi_color(&params[index + 1..]) {
                        if code == 38 {
                            self.style.fg = Some(rgb);
                        } else {
                            self.style.bg = Some(rgb);
                        }
                        index += consumed;
                    }
                }
                53 => self.style.overline = true,
                55 => self.style.overline = false,
                _ => {}
            }
            index += 1;
        }
    }

    fn clear_line(&mut self, mode: usize) {
        let col = self.cursor_col();
        let line = self.current_line_mut();
        match mode {
            1 => {
                let end = col.min(line.len().saturating_sub(1));
                for cell in line.iter_mut().take(end + 1) {
                    *cell = None;
                }
            }
            2 => line.clear(),
            _ => {
                if col < line.len() {
                    line.truncate(col);
                }
            }
        }
    }

    fn clear_display(&mut self, mode: usize) {
        let row = self.cursor_row();
        let col = self.cursor_col();
        let buffer = self.active_buffer_mut();
        buffer.current_line_mut();
        match mode {
            1 => {
                for line in buffer.lines.iter_mut().take(row) {
                    line.clear();
                }
                if let Some(line) = buffer.lines.get_mut(row) {
                    for cell in line.iter_mut().take(col.saturating_add(1)) {
                        *cell = None;
                    }
                }
            }
            2 | 3 => buffer.clear(),
            _ => {
                if let Some(line) = buffer.lines.get_mut(row) {
                    if col < line.len() {
                        line.truncate(col);
                    }
                }
                for line in buffer.lines.iter_mut().skip(row.saturating_add(1)) {
                    line.clear();
                }
            }
        }
    }

    fn render_lines(&self) -> Vec<Vec<Option<RenderGridCell>>> {
        self.active_buffer().render_lines()
    }
}

fn default_blank_cell(cell: RenderGridCell) -> bool {
    cell.ch == ' ' && cell.style == RenderGridStyle::default()
}

fn is_csi_final_byte(ch: char) -> bool {
    ('@'..='~').contains(&ch)
}

fn csi_params(sequence: &str) -> Vec<usize> {
    sequence
        .trim_start_matches('?')
        .split([';', ':'])
        .filter_map(|part| {
            if part.is_empty() {
                Some(0)
            } else {
                part.parse::<usize>().ok()
            }
        })
        .collect()
}

fn csi_count(params: &[usize]) -> usize {
    params
        .first()
        .copied()
        .filter(|count| *count > 0)
        .unwrap_or(1)
}

fn csi_position(value: Option<usize>) -> usize {
    value.unwrap_or(1).saturating_sub(1)
}

fn extended_ansi_color(params: &[usize]) -> Option<(RenderGridRgb, usize)> {
    match params.first().copied()? {
        2 => {
            let r = u8::try_from(*params.get(1)?).ok()?;
            let g = u8::try_from(*params.get(2)?).ok()?;
            let b = u8::try_from(*params.get(3)?).ok()?;
            Some((RenderGridRgb { r, g, b }, 4))
        }
        5 => {
            let index = u8::try_from(*params.get(1)?).ok()?;
            Some((ansi_256_color(index), 2))
        }
        _ => None,
    }
}

fn ansi_color(index: u8, bright: bool) -> RenderGridRgb {
    const NORMAL: [RenderGridRgb; 8] = [
        RenderGridRgb { r: 0, g: 0, b: 0 },
        RenderGridRgb {
            r: 205,
            g: 49,
            b: 49,
        },
        RenderGridRgb {
            r: 13,
            g: 188,
            b: 121,
        },
        RenderGridRgb {
            r: 229,
            g: 229,
            b: 16,
        },
        RenderGridRgb {
            r: 36,
            g: 114,
            b: 200,
        },
        RenderGridRgb {
            r: 188,
            g: 63,
            b: 188,
        },
        RenderGridRgb {
            r: 17,
            g: 168,
            b: 205,
        },
        RenderGridRgb {
            r: 229,
            g: 229,
            b: 229,
        },
    ];
    const BRIGHT: [RenderGridRgb; 8] = [
        RenderGridRgb {
            r: 102,
            g: 102,
            b: 102,
        },
        RenderGridRgb {
            r: 241,
            g: 76,
            b: 76,
        },
        RenderGridRgb {
            r: 35,
            g: 209,
            b: 139,
        },
        RenderGridRgb {
            r: 245,
            g: 245,
            b: 67,
        },
        RenderGridRgb {
            r: 59,
            g: 142,
            b: 234,
        },
        RenderGridRgb {
            r: 214,
            g: 112,
            b: 214,
        },
        RenderGridRgb {
            r: 41,
            g: 184,
            b: 219,
        },
        RenderGridRgb {
            r: 255,
            g: 255,
            b: 255,
        },
    ];
    let palette = if bright { BRIGHT } else { NORMAL };
    palette[usize::from(index.min(7))]
}

fn ansi_256_color(index: u8) -> RenderGridRgb {
    if index < 8 {
        return ansi_color(index, false);
    }
    if index < 16 {
        return ansi_color(index - 8, true);
    }
    if index < 232 {
        let idx = index - 16;
        let levels = [0, 95, 135, 175, 215, 255];
        return RenderGridRgb {
            r: levels[usize::from(idx / 36)],
            g: levels[usize::from((idx % 36) / 6)],
            b: levels[usize::from(idx % 6)],
        };
    }
    let level = 8 + (index - 232) * 10;
    RenderGridRgb {
        r: level,
        g: level,
        b: level,
    }
}

fn attach_ghostty_vt_render_states(app: &mut AppState, views: &mut Value) -> Result<(), AppError> {
    let Some(views) = views.as_array_mut() else {
        return Ok(());
    };
    for view in views {
        let is_terminal = view
            .get("kind")
            .and_then(Value::as_str)
            .map(|kind| kind == "terminal")
            .unwrap_or(false);
        if !is_terminal {
            continue;
        }
        let Some(surface_id) = view
            .get("surface_id")
            .and_then(Value::as_str)
            .map(str::to_string)
        else {
            continue;
        };
        let (cols, rows) = frame_terminal_size(view.get("frame"));
        let render_state = app.handle(
            "renderer.ghostty_vt.snapshot",
            &json!({
                "surface_id": surface_id,
                "cols": cols,
                "rows": rows
            }),
        );
        if let Some(object) = view.as_object_mut() {
            match render_state {
                Ok(value) => {
                    object.insert("ghostty_vt".to_string(), value);
                }
                Err(err) => {
                    object.insert(
                        "ghostty_vt_error".to_string(),
                        json!({"code": err.code, "message": err.message}),
                    );
                }
            }
        }
    }
    Ok(())
}

fn frame_terminal_size(frame: Option<&Value>) -> (u16, u16) {
    let width = frame
        .and_then(|frame| frame.get("width"))
        .and_then(Value::as_f64)
        .unwrap_or(1200.0);
    let height = frame
        .and_then(|frame| frame.get("height"))
        .and_then(Value::as_f64)
        .unwrap_or(600.0);
    (
        (width / GHOSTTY_VT_DEBUG_CELL_WIDTH)
            .floor()
            .clamp(1.0, u16::MAX as f64) as u16,
        (height / GHOSTTY_VT_DEBUG_CELL_HEIGHT)
            .floor()
            .clamp(1.0, u16::MAX as f64) as u16,
    )
}

fn selected_backend(params: &Value) -> Result<String, AppError> {
    let Some(backend) = params.get("backend").and_then(Value::as_str) else {
        return Ok("core".to_string());
    };
    normalize_backend(backend)
        .map(str::to_string)
        .ok_or_else(|| AppError::invalid_params(format!("unsupported renderer backend: {backend}")))
}

fn normalize_backend(backend: &str) -> Option<&'static str> {
    match backend {
        "core" => Some("core"),
        "gtk" | "gtk4" => Some("gtk"),
        "ghostty" | "libghostty" => Some("ghostty"),
        "ghostty-vt" | "libghostty-vt" | "vt" => Some("ghostty-vt"),
        _ => None,
    }
}

fn renderer_backend_uses_text_fallback(backend: &str) -> bool {
    backend != "ghostty"
}

fn probe_gtk4() -> GtkProbe {
    let feature_enabled = cfg!(feature = "gtk");
    let runtime_library = gtk4_runtime_library().map(|path| path.display().to_string());
    let link_library = gtk4_link_library().map(|path| path.display().to_string());
    let link_library_available = link_library.is_some();
    let pkg_config = std::env::var_os("PKG_CONFIG")
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| "pkg-config".into());
    match Command::new(&pkg_config)
        .arg("--modversion")
        .arg("gtk4")
        .output()
    {
        Ok(output) if output.status.success() => {
            let version = String::from_utf8_lossy(&output.stdout).trim().to_string();
            let development_files_available = link_library_available;
            GtkProbe {
                available: feature_enabled,
                feature_enabled,
                pkg_config_available: true,
                link_library_available,
                development_files_available,
                development_package_hint: GTK4_DEVELOPMENT_PACKAGE_HINT,
                pkg_config_error: None,
                runtime_library,
                link_library,
                version: Some(version.clone()),
                detail: if !development_files_available {
                    format!(
                        "gtk4 pkg-config version {version}, but libgtk-4.so was not found on common linker paths; {GTK4_DEVELOPMENT_PACKAGE_HINT}"
                    )
                } else if feature_enabled {
                    format!("gtk4 pkg-config version {version}")
                } else {
                    format!(
                        "gtk4 pkg-config version {version}, but this cmux binary was built without the GTK renderer feature"
                    )
                },
            }
        }
        Ok(output) => {
            let pkg_config_error = gtk4_pkg_config_error(&output.stderr);
            let runtime_available = runtime_library.is_some();
            let missing_development_detail = if !link_library_available {
                "GTK development files (`gtk4.pc` and `libgtk-4.so`) were not found"
            } else {
                "gtk4.pc was not found"
            };
            let detail = if !feature_enabled && runtime_available {
                format!(
                    "GTK4 runtime library found at {}, but this cmux binary was built without the GTK renderer feature; {missing_development_detail}",
                    runtime_library.as_deref().unwrap_or("unknown"),
                )
            } else if !feature_enabled {
                "cmux was built without the GTK renderer feature; rebuild with `--features gtk` to run the GTK shell".to_string()
            } else if runtime_available {
                format!(
                    "GTK4 runtime library found at {}, but {missing_development_detail}; {}",
                    runtime_library.as_deref().unwrap_or("unknown"),
                    GTK4_DEVELOPMENT_PACKAGE_HINT
                )
            } else if pkg_config_error.as_deref().unwrap_or_default().is_empty() {
                format!(
                    "{missing_development_detail}; {}",
                    GTK4_DEVELOPMENT_PACKAGE_HINT
                )
            } else {
                format!(
                    "{}; {}",
                    pkg_config_error.clone().unwrap_or_default(),
                    GTK4_DEVELOPMENT_PACKAGE_HINT
                )
            };
            GtkProbe {
                available: feature_enabled && runtime_available,
                feature_enabled,
                pkg_config_available: false,
                link_library_available,
                development_files_available: false,
                development_package_hint: GTK4_DEVELOPMENT_PACKAGE_HINT,
                pkg_config_error,
                runtime_library,
                link_library,
                version: None,
                detail,
            }
        }
        Err(err) => {
            let pkg_config_error =
                format!("failed to run {}: {err}", Path::new(&pkg_config).display());
            GtkProbe {
                available: feature_enabled && runtime_library.is_some(),
                feature_enabled,
                pkg_config_available: false,
                link_library_available,
                development_files_available: false,
                development_package_hint: GTK4_DEVELOPMENT_PACKAGE_HINT,
                pkg_config_error: Some(pkg_config_error.clone()),
                runtime_library,
                link_library,
                version: None,
                detail: if feature_enabled {
                    format!("{pkg_config_error}; {GTK4_DEVELOPMENT_PACKAGE_HINT}")
                } else {
                    "cmux was built without the GTK renderer feature; rebuild with `--features gtk` to run the GTK shell".to_string()
                },
            }
        }
    }
}

fn gtk4_pkg_config_error(stderr: &[u8]) -> Option<String> {
    let value = String::from_utf8_lossy(stderr).trim().to_string();
    if value.is_empty() {
        None
    } else {
        Some(value)
    }
}

fn gtk4_runtime_library() -> Option<PathBuf> {
    gtk4_runtime_library_in_dirs(gtk4_library_dirs())
}

fn gtk4_link_library() -> Option<PathBuf> {
    gtk4_link_library_in_dirs(gtk4_library_dirs())
}

fn gtk4_library_dirs() -> Vec<PathBuf> {
    let mut dirs = Vec::new();
    for key in ["LIBRARY_PATH", "LD_LIBRARY_PATH"] {
        if let Some(paths) = std::env::var_os(key) {
            for path in std::env::split_paths(&paths) {
                push_unique_path(&mut dirs, path);
            }
        }
    }
    for dir in [
        "/usr/lib64",
        "/usr/lib",
        "/lib64",
        "/lib",
        "/usr/local/lib64",
        "/usr/local/lib",
    ] {
        push_unique_path(&mut dirs, PathBuf::from(dir));
    }
    dirs
}

fn gtk4_runtime_library_in_dirs<I>(dirs: I) -> Option<PathBuf>
where
    I: IntoIterator<Item = PathBuf>,
{
    dirs.into_iter().find_map(|dir| {
        ["libgtk-4.so", "libgtk-4.so.1"]
            .iter()
            .map(|name| dir.join(name))
            .find(|path| path.exists())
    })
}

fn gtk4_link_library_in_dirs<I>(dirs: I) -> Option<PathBuf>
where
    I: IntoIterator<Item = PathBuf>,
{
    dirs.into_iter()
        .map(|dir| dir.join("libgtk-4.so"))
        .find(|path| path.exists())
}

fn push_unique_path(paths: &mut Vec<PathBuf>, path: PathBuf) {
    if !paths.iter().any(|existing| existing == &path) {
        paths.push(path);
    }
}

fn probe_ghostty() -> GhosttyProbe {
    let root = ghostty_root();
    let header = ghostty_embedding_header(root.as_deref());
    let header_text = header
        .as_ref()
        .and_then(|path| std::fs::read_to_string(path).ok())
        .unwrap_or_default();
    let header_linux_platform_value = ghostty_header_linux_platform_value(&header_text);
    let has_linux_platform = header_linux_platform_value.is_some();
    let header_abi_version = ghostty_header_embedding_abi_version(&header_text);
    let header_abi_version_matches =
        header_abi_version == Some(crate::ghostty_embed::GHOSTTY_EMBEDDING_ABI_VERSION);
    let header_linux_platform_value_matches =
        header_linux_platform_value == Some(crate::ghostty_embed::GHOSTTY_PLATFORM_LINUX);
    let header_env_var_limit = ghostty_header_env_var_limit(&header_text);
    let header_env_var_limit_matches =
        header_env_var_limit == Some(crate::ghostty_embed::GHOSTTY_SURFACE_MAX_ENV_VARS);
    let header_keycode_native_mask = ghostty_header_keycode_native_mask(&header_text);
    let header_keycode_native_mask_matches =
        header_keycode_native_mask == Some(crate::ghostty_embed::GHOSTTY_INPUT_KEYCODE_NATIVE_MASK);
    let header_keycode_physical_key_flag = ghostty_header_keycode_physical_key_flag(&header_text);
    let header_keycode_physical_key_flag_matches = header_keycode_physical_key_flag
        == Some(crate::ghostty_embed::GHOSTTY_INPUT_KEYCODE_PHYSICAL_KEY_FLAG);
    let header_has_app_thread_draw_contract =
        header_text.contains("ghostty_app_must_draw_from_app_thread");
    let header_has_redraw_surface_callback =
        ghostty_header_has_redraw_surface_callback(&header_text);
    let header_surface_env_vars_const = ghostty_header_surface_env_vars_const(&header_text);
    let header_init_argv_const = ghostty_header_init_argv_const(&header_text);
    let header_ipc_new_window_arguments_const =
        ghostty_header_ipc_new_window_arguments_const(&header_text);
    let header_surface_metadata_returns_string =
        ghostty_header_surface_metadata_returns_string(&header_text);
    let library = ghostty_library(root.as_deref());
    let library_present = library.is_some();
    let reported_resources_dir = library.as_deref().and_then(ghostty_reported_resources_dir);
    let runtime_resources = ghostty_runtime_resources(
        root.as_deref(),
        library.as_deref(),
        reported_resources_dir.as_deref(),
    );
    let vt_header = ghostty_vt_header(root.as_deref());
    let vt_library = ghostty_vt_library(root.as_deref());
    let vt_pkg_config = ghostty_vt_pkg_config(root.as_deref());
    let embedding_missing_symbols_result = library.as_deref().and_then(ghostty_missing_symbols);
    let embedding_symbols_verified = embedding_missing_symbols_result.is_some();
    let embedding_missing_symbols = embedding_missing_symbols_result.unwrap_or_default();
    let embedding_darwin_symbols_result =
        library.as_deref().and_then(ghostty_darwin_symbols_present);
    let embedding_darwin_symbols_hidden = embedding_darwin_symbols_result
        .as_ref()
        .map(Vec::is_empty)
        .unwrap_or(false);
    let embedding_darwin_symbols_present = embedding_darwin_symbols_result.unwrap_or_default();
    let embedding_internal_symbols_result = library
        .as_deref()
        .and_then(ghostty_internal_symbols_present);
    let embedding_internal_symbols_hidden = embedding_internal_symbols_result
        .as_ref()
        .map(Vec::is_empty)
        .unwrap_or(false);
    let embedding_internal_symbols_present = embedding_internal_symbols_result.unwrap_or_default();
    let embedding_unexpected_export_symbols_result = library
        .as_deref()
        .and_then(ghostty_unexpected_export_symbols_present);
    let embedding_unexpected_export_symbol_count = embedding_unexpected_export_symbols_result
        .as_ref()
        .map(|symbols| symbols.total)
        .unwrap_or(0);
    let embedding_unexpected_export_symbols_hidden = embedding_unexpected_export_symbols_result
        .as_ref()
        .map(|symbols| symbols.total == 0)
        .unwrap_or(false);
    let embedding_unexpected_export_symbols_present = embedding_unexpected_export_symbols_result
        .map(|symbols| symbols.sample)
        .unwrap_or_default();
    let embedding_load_result = library
        .as_deref()
        .map(crate::ghostty_embed::verify_library_loadable);
    let embedding_library_loadable = matches!(embedding_load_result, Some(Ok(())));
    let embedding_load_error =
        embedding_load_result.and_then(|result| result.err().map(|err| err.to_string()));
    let embedding_library_info_report_result = if embedding_library_loadable
        && embedding_symbols_verified
        && embedding_missing_symbols.is_empty()
    {
        library
            .as_deref()
            .map(crate::ghostty_embed::embedding_info_report_for_library)
    } else {
        None
    };
    let (
        embedding_library_info,
        embedding_library_info_query_succeeded,
        embedding_library_info_direct_matches_query,
        embedding_library_info_query_error,
    ) = match embedding_library_info_report_result {
        Some(Ok(report)) => (Some(report.info()), true, report.matches(), None),
        Some(Err(err)) => (None, false, false, Some(err.to_string())),
        None => (None, false, false, None),
    };
    let embedding_library_abi_version = embedding_library_info.map(|info| info.abi_version);
    let embedding_library_abi_version_matches =
        embedding_library_abi_version == Some(crate::ghostty_embed::GHOSTTY_EMBEDDING_ABI_VERSION);
    let embedding_library_platform = embedding_library_info.map(|info| info.platform);
    let embedding_library_platform_matches =
        embedding_library_platform == Some(crate::ghostty_embed::GHOSTTY_PLATFORM_LINUX);
    let embedding_library_renderer_backend =
        embedding_library_info.map(|info| info.renderer_backend);
    let embedding_library_renderer_backend_matches = embedding_library_renderer_backend
        == Some(crate::ghostty_embed::GHOSTTY_RENDERER_BACKEND_OPENGL);
    let embedding_library_env_var_limit =
        embedding_library_info.map(|info| info.surface_max_env_vars);
    let embedding_library_env_var_limit_matches =
        embedding_library_env_var_limit == Some(crate::ghostty_embed::GHOSTTY_SURFACE_MAX_ENV_VARS);
    let embedding_library_runtime_config_size =
        embedding_library_info.map(|info| info.runtime_config_size);
    let embedding_library_surface_config_size =
        embedding_library_info.map(|info| info.surface_config_size);
    let embedding_library_platform_linux_size =
        embedding_library_info.map(|info| info.platform_linux_size);
    let embedding_library_input_key_size = embedding_library_info.map(|info| info.input_key_size);
    let embedding_library_target_size = embedding_library_info.map(|info| info.target_size);
    let embedding_library_action_size = embedding_library_info.map(|info| info.action_size);
    let embedding_library_text_size = embedding_library_info.map(|info| info.text_size);
    let embedding_library_selection_size = embedding_library_info.map(|info| info.selection_size);
    let embedding_library_string_size = embedding_library_info.map(|info| info.string_size);
    let embedding_library_surface_size_size =
        embedding_library_info.map(|info| info.surface_size_size);
    let embedding_library_diagnostic_size = embedding_library_info.map(|info| info.diagnostic_size);
    let embedding_library_env_var_size = embedding_library_info.map(|info| info.env_var_size);
    let embedding_library_clipboard_content_size =
        embedding_library_info.map(|info| info.clipboard_content_size);
    let embedding_library_input_trigger_size =
        embedding_library_info.map(|info| info.input_trigger_size);
    let embedding_library_ipc_target_size = embedding_library_info.map(|info| info.ipc_target_size);
    let embedding_library_ipc_action_size = embedding_library_info.map(|info| info.ipc_action_size);
    let embedding_library_layout_sizes_match = embedding_library_info
        .as_ref()
        .map(crate::ghostty_embed::embedding_layout_sizes_match)
        .unwrap_or(false);
    let embedding_library_runtime_config_align =
        embedding_library_info.map(|info| info.runtime_config_align);
    let embedding_library_surface_config_align =
        embedding_library_info.map(|info| info.surface_config_align);
    let embedding_library_platform_linux_align =
        embedding_library_info.map(|info| info.platform_linux_align);
    let embedding_library_input_key_align = embedding_library_info.map(|info| info.input_key_align);
    let embedding_library_target_align = embedding_library_info.map(|info| info.target_align);
    let embedding_library_action_align = embedding_library_info.map(|info| info.action_align);
    let embedding_library_text_align = embedding_library_info.map(|info| info.text_align);
    let embedding_library_selection_align = embedding_library_info.map(|info| info.selection_align);
    let embedding_library_string_align = embedding_library_info.map(|info| info.string_align);
    let embedding_library_surface_size_align =
        embedding_library_info.map(|info| info.surface_size_align);
    let embedding_library_diagnostic_align =
        embedding_library_info.map(|info| info.diagnostic_align);
    let embedding_library_env_var_align = embedding_library_info.map(|info| info.env_var_align);
    let embedding_library_clipboard_content_align =
        embedding_library_info.map(|info| info.clipboard_content_align);
    let embedding_library_input_trigger_align =
        embedding_library_info.map(|info| info.input_trigger_align);
    let embedding_library_ipc_target_align =
        embedding_library_info.map(|info| info.ipc_target_align);
    let embedding_library_ipc_action_align =
        embedding_library_info.map(|info| info.ipc_action_align);
    let embedding_library_layout_alignments_match = embedding_library_info
        .as_ref()
        .map(crate::ghostty_embed::embedding_layout_alignments_match)
        .unwrap_or(false);
    let embedding_library_layout_fingerprint =
        embedding_library_info.map(|info| info.layout_fingerprint);
    let embedding_expected_layout_fingerprint =
        crate::ghostty_embed::embedding_layout_fingerprint();
    let embedding_library_layout_fingerprint_matches = embedding_library_info
        .as_ref()
        .map(crate::ghostty_embed::embedding_layout_fingerprint_matches)
        .unwrap_or(false);
    let embedding_library_constants_fingerprint =
        embedding_library_info.map(|info| info.constants_fingerprint);
    let embedding_expected_constants_fingerprint =
        crate::ghostty_embed::embedding_constants_fingerprint();
    let embedding_library_constants_fingerprint_matches = embedding_library_info
        .as_ref()
        .map(crate::ghostty_embed::embedding_constants_fingerprint_matches)
        .unwrap_or(false);
    let embedding_library_supports_linux_platform =
        embedding_library_info.map(|info| info.supports_linux_platform);
    let embedding_library_must_draw_from_app_thread =
        embedding_library_info.map(|info| info.must_draw_from_app_thread);
    let embedding_app_must_draw_result = if embedding_library_loadable
        && embedding_symbols_verified
        && embedding_missing_symbols.is_empty()
    {
        library
            .as_deref()
            .map(crate::ghostty_embed::host_check_for_library)
    } else {
        None
    };
    let (embedding_app_must_draw_from_app_thread, embedding_app_must_draw_query_error) =
        match embedding_app_must_draw_result {
            Some(Ok(check)) => (Some(check.must_draw_from_app_thread), None),
            Some(Err(err)) => (None, Some(err.to_string())),
            None => (None, None),
        };
    let vt_missing_symbols_result = vt_library.as_deref().and_then(ghostty_vt_missing_symbols);
    let vt_symbols_verified = vt_missing_symbols_result.is_some();
    let vt_missing_symbols = vt_missing_symbols_result.unwrap_or_default();
    let linux_embedding_supported = ghostty_embedding_supported(
        has_linux_platform,
        header_abi_version_matches,
        header_linux_platform_value_matches,
        header_env_var_limit_matches,
        header_keycode_native_mask_matches,
        header_keycode_physical_key_flag_matches,
        header_has_app_thread_draw_contract,
        header_has_redraw_surface_callback,
        header_surface_env_vars_const,
        header_init_argv_const,
        header_ipc_new_window_arguments_const,
        header_surface_metadata_returns_string,
        library_present,
        embedding_symbols_verified,
        &embedding_missing_symbols,
        embedding_darwin_symbols_hidden,
        embedding_internal_symbols_hidden,
        embedding_unexpected_export_symbols_hidden,
        embedding_library_loadable,
        embedding_library_info_query_succeeded,
        embedding_library_info_direct_matches_query,
        embedding_library_abi_version_matches,
        embedding_library_platform_matches,
        embedding_library_renderer_backend_matches,
        embedding_library_env_var_limit_matches,
        embedding_library_layout_sizes_match,
        embedding_library_layout_alignments_match,
        embedding_library_layout_fingerprint_matches,
        embedding_library_constants_fingerprint_matches,
        embedding_library_supports_linux_platform == Some(true),
    );
    let embedding_status = ghostty_embedding_status(
        header.is_some(),
        has_linux_platform,
        header_abi_version_matches,
        header_linux_platform_value_matches,
        header_env_var_limit_matches,
        header_keycode_native_mask_matches,
        header_keycode_physical_key_flag_matches,
        header_has_app_thread_draw_contract,
        header_has_redraw_surface_callback,
        header_surface_env_vars_const,
        header_init_argv_const,
        header_ipc_new_window_arguments_const,
        header_surface_metadata_returns_string,
        library_present,
        embedding_symbols_verified,
        &embedding_missing_symbols,
        embedding_darwin_symbols_hidden,
        embedding_internal_symbols_hidden,
        embedding_unexpected_export_symbols_hidden,
        embedding_library_loadable,
        embedding_library_info_query_succeeded,
        embedding_library_info_direct_matches_query,
        embedding_library_abi_version_matches,
        embedding_library_platform_matches,
        embedding_library_renderer_backend_matches,
        embedding_library_env_var_limit_matches,
        embedding_library_layout_sizes_match,
        embedding_library_layout_alignments_match,
        embedding_library_layout_fingerprint_matches,
        embedding_library_constants_fingerprint_matches,
        embedding_library_supports_linux_platform == Some(true),
    );
    let vt_supported = ghostty_vt_supported(
        vt_header.is_some(),
        vt_library.is_some(),
        vt_symbols_verified,
        &vt_missing_symbols,
    );
    let detail = if linux_embedding_supported {
        if runtime_resources.present {
            "Ghostty Linux embedding symbols and runtime resources are available".to_string()
        } else {
            format!(
                "Ghostty Linux embedding symbols are available, but runtime resources are incomplete: {}",
                runtime_resources.missing.join(", ")
            )
        }
    } else if has_linux_platform && !header_abi_version_matches {
        let expected = crate::ghostty_embed::GHOSTTY_EMBEDDING_ABI_VERSION;
        if let Some(actual) = header_abi_version {
            format!(
                "Ghostty embedding header exposes GHOSTTY_EMBEDDING_ABI_VERSION={actual}; cmux expects {expected}"
            )
        } else {
            format!(
                "Ghostty embedding header exposes the Linux platform tag, but does not define GHOSTTY_EMBEDDING_ABI_VERSION; cmux expects {expected}"
            )
        }
    } else if has_linux_platform && !header_linux_platform_value_matches {
        let expected = crate::ghostty_embed::GHOSTTY_PLATFORM_LINUX;
        if let Some(actual) = header_linux_platform_value {
            format!(
                "Ghostty embedding header exposes GHOSTTY_PLATFORM_LINUX={actual}; cmux expects {expected}"
            )
        } else {
            format!(
                "Ghostty embedding header exposes GHOSTTY_PLATFORM_LINUX but diagnostics could not resolve its enum value; cmux expects {expected}"
            )
        }
    } else if has_linux_platform && !header_env_var_limit_matches {
        let expected = crate::ghostty_embed::GHOSTTY_SURFACE_MAX_ENV_VARS;
        if let Some(actual) = header_env_var_limit {
            format!(
                "Ghostty embedding header exposes the Linux platform tag, but GHOSTTY_SURFACE_MAX_ENV_VARS is {actual}; cmux expects {expected}"
            )
        } else {
            format!(
                "Ghostty embedding header exposes the Linux platform tag, but does not define GHOSTTY_SURFACE_MAX_ENV_VARS; cmux expects {expected}"
            )
        }
    } else if has_linux_platform && !header_keycode_native_mask_matches {
        let expected = crate::ghostty_embed::GHOSTTY_INPUT_KEYCODE_NATIVE_MASK;
        if let Some(actual) = header_keycode_native_mask {
            format!(
                "Ghostty embedding header exposes the Linux platform tag, but GHOSTTY_INPUT_KEYCODE_NATIVE_MASK is {actual:#x}; cmux expects {expected:#x}"
            )
        } else {
            format!(
                "Ghostty embedding header exposes the Linux platform tag, but does not define GHOSTTY_INPUT_KEYCODE_NATIVE_MASK; cmux expects {expected:#x}"
            )
        }
    } else if has_linux_platform && !header_keycode_physical_key_flag_matches {
        let expected = crate::ghostty_embed::GHOSTTY_INPUT_KEYCODE_PHYSICAL_KEY_FLAG;
        if let Some(actual) = header_keycode_physical_key_flag {
            format!(
                "Ghostty embedding header exposes the Linux platform tag, but GHOSTTY_INPUT_KEYCODE_PHYSICAL_KEY_FLAG is {actual:#x}; cmux expects {expected:#x}"
            )
        } else {
            format!(
                "Ghostty embedding header exposes the Linux platform tag, but does not define GHOSTTY_INPUT_KEYCODE_PHYSICAL_KEY_FLAG; cmux expects {expected:#x}"
            )
        }
    } else if has_linux_platform && !header_has_app_thread_draw_contract {
        "Ghostty embedding header exposes the Linux platform tag, but does not declare ghostty_app_must_draw_from_app_thread".to_string()
    } else if has_linux_platform && !header_has_redraw_surface_callback {
        "Ghostty embedding header exposes the Linux platform tag, but ghostty_runtime_config_s.redraw_surface_cb is not declared".to_string()
    } else if has_linux_platform && !header_surface_env_vars_const {
        "Ghostty embedding header exposes the Linux platform tag, but ghostty_surface_config_s.env_vars is not declared as const ghostty_env_var_s *".to_string()
    } else if has_linux_platform && !header_init_argv_const {
        "Ghostty embedding header exposes the Linux platform tag, but ghostty_init argv is not declared as const char * const *".to_string()
    } else if has_linux_platform && !header_ipc_new_window_arguments_const {
        "Ghostty embedding header exposes the Linux platform tag, but ghostty_ipc_action_new_window_s.arguments is not declared as const char * const *".to_string()
    } else if has_linux_platform && !header_surface_metadata_returns_string {
        "Ghostty embedding header exposes the Linux platform tag, but surface metadata APIs are not declared as returning ghostty_string_s".to_string()
    } else if has_linux_platform && library_present && !embedding_library_loadable {
        format!(
            "ghostty-internal was found, but could not be loaded: {}",
            embedding_load_error
                .as_deref()
                .unwrap_or("unknown dynamic loader error")
        )
    } else if has_linux_platform
        && library_present
        && embedding_symbols_verified
        && !embedding_missing_symbols.is_empty()
    {
        format!(
            "ghostty-internal was found, but required Linux embedding symbols are missing: {}",
            embedding_missing_symbols.join(", ")
        )
    } else if has_linux_platform
        && library_present
        && embedding_symbols_verified
        && embedding_missing_symbols.is_empty()
        && !embedding_darwin_symbols_hidden
    {
        format!(
            "ghostty-internal was found, but its Linux build exports Darwin-only symbols: {}",
            embedding_darwin_symbols_present.join(", ")
        )
    } else if has_linux_platform
        && library_present
        && embedding_symbols_verified
        && embedding_missing_symbols.is_empty()
        && embedding_darwin_symbols_hidden
        && !embedding_internal_symbols_hidden
    {
        format!(
            "ghostty-internal was found, but its Linux build exports internal helper symbols: {}",
            embedding_internal_symbols_present.join(", ")
        )
    } else if has_linux_platform
        && library_present
        && embedding_symbols_verified
        && embedding_missing_symbols.is_empty()
        && embedding_darwin_symbols_hidden
        && embedding_internal_symbols_hidden
        && !embedding_unexpected_export_symbols_hidden
    {
        format!(
            "ghostty-internal was found, but its Linux build exports {embedding_unexpected_export_symbol_count} non-embedding symbol(s): {}",
            embedding_unexpected_export_symbols_present.join(", ")
        )
    } else if has_linux_platform
        && library_present
        && embedding_library_loadable
        && embedding_symbols_verified
        && embedding_missing_symbols.is_empty()
        && embedding_library_info_query_error.is_some()
    {
        format!(
            "ghostty-internal was found, but its embedding info could not be queried: {}",
            embedding_library_info_query_error
                .as_deref()
                .unwrap_or("unknown embedding info error")
        )
    } else if has_linux_platform
        && library_present
        && embedding_library_loadable
        && embedding_symbols_verified
        && embedding_missing_symbols.is_empty()
        && !embedding_library_info_direct_matches_query
    {
        "ghostty-internal direct and queried embedding info self-reports disagree".to_string()
    } else if has_linux_platform
        && library_present
        && embedding_library_loadable
        && embedding_symbols_verified
        && embedding_missing_symbols.is_empty()
        && !embedding_library_abi_version_matches
    {
        let expected = crate::ghostty_embed::GHOSTTY_EMBEDDING_ABI_VERSION;
        if let Some(actual) = embedding_library_abi_version {
            format!(
                "ghostty-internal reports embedding ABI version {actual}; cmux expects {expected}"
            )
        } else {
            format!(
                "ghostty-internal did not report an embedding ABI version; cmux expects {expected}"
            )
        }
    } else if has_linux_platform
        && library_present
        && embedding_library_loadable
        && embedding_symbols_verified
        && embedding_missing_symbols.is_empty()
        && !embedding_library_platform_matches
    {
        let expected = crate::ghostty_embed::GHOSTTY_PLATFORM_LINUX;
        if let Some(actual) = embedding_library_platform {
            format!(
                "ghostty-internal reports embedding platform {actual}; cmux expects Linux platform {expected}"
            )
        } else {
            format!(
                "ghostty-internal did not report an embedding platform; cmux expects Linux platform {expected}"
            )
        }
    } else if has_linux_platform
        && library_present
        && embedding_library_loadable
        && embedding_symbols_verified
        && embedding_missing_symbols.is_empty()
        && !embedding_library_renderer_backend_matches
    {
        let expected = crate::ghostty_embed::GHOSTTY_RENDERER_BACKEND_OPENGL;
        if let Some(actual) = embedding_library_renderer_backend {
            format!(
                "ghostty-internal reports renderer backend {actual}; cmux expects OpenGL backend {expected}"
            )
        } else {
            format!(
                "ghostty-internal did not report a renderer backend; cmux expects OpenGL backend {expected}"
            )
        }
    } else if has_linux_platform
        && library_present
        && embedding_library_loadable
        && embedding_symbols_verified
        && embedding_missing_symbols.is_empty()
        && !embedding_library_env_var_limit_matches
    {
        let expected = crate::ghostty_embed::GHOSTTY_SURFACE_MAX_ENV_VARS;
        if let Some(actual) = embedding_library_env_var_limit {
            format!(
                "ghostty-internal reports GHOSTTY_SURFACE_MAX_ENV_VARS={actual}; cmux expects {expected}"
            )
        } else {
            format!(
                "ghostty-internal did not report GHOSTTY_SURFACE_MAX_ENV_VARS; cmux expects {expected}"
            )
        }
    } else if has_linux_platform
        && library_present
        && embedding_library_loadable
        && embedding_symbols_verified
        && embedding_missing_symbols.is_empty()
        && (!embedding_library_layout_sizes_match || !embedding_library_layout_alignments_match)
    {
        ghostty_embedding_layout_mismatch_detail(embedding_library_info.as_ref())
    } else if has_linux_platform
        && library_present
        && embedding_library_loadable
        && embedding_symbols_verified
        && embedding_missing_symbols.is_empty()
        && !embedding_library_layout_fingerprint_matches
    {
        if let Some(actual) = embedding_library_layout_fingerprint {
            format!(
                "ghostty-internal reports embedding layout fingerprint {actual:#x}; cmux expects {embedding_expected_layout_fingerprint:#x}"
            )
        } else {
            format!(
                "ghostty-internal did not report an embedding layout fingerprint; cmux expects {embedding_expected_layout_fingerprint:#x}"
            )
        }
    } else if has_linux_platform
        && library_present
        && embedding_library_loadable
        && embedding_symbols_verified
        && embedding_missing_symbols.is_empty()
        && !embedding_library_constants_fingerprint_matches
    {
        if let Some(actual) = embedding_library_constants_fingerprint {
            format!(
                "ghostty-internal reports embedding constants fingerprint {actual:#x}; cmux expects {embedding_expected_constants_fingerprint:#x}"
            )
        } else {
            format!(
                "ghostty-internal did not report an embedding constants fingerprint; cmux expects {embedding_expected_constants_fingerprint:#x}"
            )
        }
    } else if has_linux_platform
        && library_present
        && embedding_library_loadable
        && embedding_symbols_verified
        && embedding_missing_symbols.is_empty()
        && embedding_library_supports_linux_platform != Some(true)
    {
        "ghostty-internal exposes the Linux embedding symbols but does not report Linux platform support".to_string()
    } else if has_linux_platform && library_present {
        "ghostty-internal was found, but required Linux embedding symbols could not be verified with nm".to_string()
    } else if has_linux_platform && !library_present {
        "Ghostty embedding header exposes the Linux platform tag, but ghostty-internal was not found; run `zig build -Dapp-runtime=none` in the Ghostty checkout".to_string()
    } else if vt_supported {
        "Ghostty portable libghostty-vt terminal core is available, but this checkout does not expose GHOSTTY_PLATFORM_LINUX for full embedding".to_string()
    } else if vt_header.is_some() && vt_library.is_some() && vt_symbols_verified {
        format!(
            "libghostty-vt was found, but required symbols are missing: {}",
            vt_missing_symbols.join(", ")
        )
    } else if vt_header.is_some() && vt_library.is_some() {
        "libghostty-vt was found, but required symbols could not be verified with nm".to_string()
    } else if header.is_some() {
        "Ghostty embedding header is present, but this checkout does not expose GHOSTTY_PLATFORM_LINUX; build or point CMUX_GHOSTTY_ROOT at the local Ghostty checkout with the Linux embedding ABI".to_string()
    } else {
        "Ghostty embedding header was not found near this checkout; libghostty-vt was also unavailable".to_string()
    };

    GhosttyProbe {
        header: header.map(display_path),
        internal_library: library.map(display_path),
        runtime_resources_dir: runtime_resources.dir.map(display_path),
        runtime_resources_source: runtime_resources
            .source
            .map(GhosttyRuntimeResourceSource::as_str),
        runtime_resources_present: runtime_resources.present,
        runtime_resources_missing: runtime_resources.missing,
        runtime_themes_present: runtime_resources.themes_present,
        runtime_i18n_present: runtime_resources.i18n_present,
        embedding_status,
        embedding_header_has_linux_platform: has_linux_platform,
        embedding_header_abi_version: header_abi_version,
        embedding_header_abi_version_matches: header_abi_version_matches,
        embedding_header_linux_platform_value: header_linux_platform_value,
        embedding_header_linux_platform_value_matches: header_linux_platform_value_matches,
        embedding_header_env_var_limit: header_env_var_limit,
        embedding_header_env_var_limit_matches: header_env_var_limit_matches,
        embedding_header_keycode_native_mask: header_keycode_native_mask,
        embedding_header_keycode_native_mask_matches: header_keycode_native_mask_matches,
        embedding_header_keycode_physical_key_flag: header_keycode_physical_key_flag,
        embedding_header_keycode_physical_key_flag_matches:
            header_keycode_physical_key_flag_matches,
        embedding_header_has_app_thread_draw_contract: header_has_app_thread_draw_contract,
        embedding_header_has_redraw_surface_callback: header_has_redraw_surface_callback,
        embedding_header_surface_env_vars_const: header_surface_env_vars_const,
        embedding_header_init_argv_const: header_init_argv_const,
        embedding_header_ipc_new_window_arguments_const: header_ipc_new_window_arguments_const,
        embedding_header_surface_metadata_returns_string: header_surface_metadata_returns_string,
        embedding_library_present: library_present,
        embedding_library_abi_version,
        embedding_library_abi_version_matches,
        embedding_library_platform,
        embedding_library_platform_matches,
        embedding_library_renderer_backend,
        embedding_library_renderer_backend_matches,
        embedding_library_env_var_limit,
        embedding_library_env_var_limit_matches,
        embedding_library_runtime_config_size,
        embedding_library_surface_config_size,
        embedding_library_platform_linux_size,
        embedding_library_input_key_size,
        embedding_library_target_size,
        embedding_library_action_size,
        embedding_library_text_size,
        embedding_library_selection_size,
        embedding_library_string_size,
        embedding_library_surface_size_size,
        embedding_library_diagnostic_size,
        embedding_library_env_var_size,
        embedding_library_clipboard_content_size,
        embedding_library_input_trigger_size,
        embedding_library_ipc_target_size,
        embedding_library_ipc_action_size,
        embedding_library_layout_sizes_match,
        embedding_library_runtime_config_align,
        embedding_library_surface_config_align,
        embedding_library_platform_linux_align,
        embedding_library_input_key_align,
        embedding_library_target_align,
        embedding_library_action_align,
        embedding_library_text_align,
        embedding_library_selection_align,
        embedding_library_string_align,
        embedding_library_surface_size_align,
        embedding_library_diagnostic_align,
        embedding_library_env_var_align,
        embedding_library_clipboard_content_align,
        embedding_library_input_trigger_align,
        embedding_library_ipc_target_align,
        embedding_library_ipc_action_align,
        embedding_library_layout_alignments_match,
        embedding_library_layout_fingerprint,
        embedding_expected_layout_fingerprint,
        embedding_library_layout_fingerprint_matches,
        embedding_library_constants_fingerprint,
        embedding_expected_constants_fingerprint,
        embedding_library_constants_fingerprint_matches,
        embedding_library_supports_linux_platform,
        embedding_library_must_draw_from_app_thread,
        embedding_library_info_query_error,
        embedding_library_info_direct_matches_query,
        embedding_app_must_draw_from_app_thread,
        embedding_app_must_draw_query_error,
        linux_embedding_supported,
        embedding_symbols_verified,
        embedding_missing_symbols,
        embedding_darwin_symbols_hidden,
        embedding_darwin_symbols_present,
        embedding_internal_symbols_hidden,
        embedding_internal_symbols_present,
        embedding_unexpected_export_symbols_hidden,
        embedding_unexpected_export_symbols_present,
        embedding_unexpected_export_symbol_count,
        embedding_library_loadable,
        embedding_load_error,
        vt_header: vt_header.map(display_path),
        vt_library: vt_library.map(display_path),
        vt_pkg_config: vt_pkg_config.map(display_path),
        vt_symbols_verified,
        vt_missing_symbols,
        vt_supported,
        detail,
    }
}

fn probe_display() -> DisplayProbe {
    let wayland_display = normalized_env("WAYLAND_DISPLAY");
    let x11_display = normalized_env("DISPLAY");
    DisplayProbe {
        available: wayland_display.is_some() || x11_display.is_some(),
        wayland_display,
        x11_display,
    }
}

fn ghostty_root() -> Option<PathBuf> {
    if let Some(root) = normalized_env("CMUX_GHOSTTY_ROOT").map(PathBuf::from) {
        if ghostty_embedding_header(Some(&root)).is_some()
            || ghostty_vt_header(Some(&root)).is_some()
        {
            return Some(root);
        }
    }

    for key in ["CMUX_GHOSTTY_LIBRARY", "CMUX_GHOSTTY_VT_LIBRARY"] {
        if let Some(path) = normalized_env(key).map(PathBuf::from) {
            if let Some(root) = ghostty_root_from_library_path(&path) {
                return Some(root);
            }
        }
    }

    let mut current = std::env::current_dir().ok()?;
    loop {
        let candidate = current.join("ghostty");
        if ghostty_embedding_header(Some(&candidate)).is_some()
            || ghostty_vt_header(Some(&candidate)).is_some()
        {
            return Some(candidate);
        }
        if !current.pop() {
            break;
        }
    }
    None
}

fn ghostty_embedding_header(root: Option<&Path>) -> Option<PathBuf> {
    let root = root?;
    [root.join("include/ghostty.h")]
        .into_iter()
        .find(|path| path.exists())
}

fn ghostty_library(root: Option<&Path>) -> Option<PathBuf> {
    if let Some(path) = normalized_env("CMUX_GHOSTTY_LIBRARY").map(PathBuf::from) {
        if path.exists() {
            return Some(path);
        }
    }
    let root = root?;
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
    if ghostty_embedding_header(Some(root)).is_some() || ghostty_vt_header(Some(root)).is_some() {
        Some(root.to_path_buf())
    } else {
        None
    }
}

fn ghostty_vt_header(root: Option<&Path>) -> Option<PathBuf> {
    let root = root?;
    [
        root.join("include/ghostty/vt.h"),
        root.join("zig-out/include/ghostty/vt.h"),
    ]
    .into_iter()
    .find(|path| path.exists())
}

fn ghostty_vt_library(root: Option<&Path>) -> Option<PathBuf> {
    if let Some(path) = normalized_env("CMUX_GHOSTTY_VT_LIBRARY").map(PathBuf::from) {
        if path.exists() {
            return Some(path);
        }
    }

    let root = root?;
    let direct = [
        root.join("zig-out/lib/libghostty-vt.so"),
        root.join("zig-out/lib/libghostty-vt.a"),
        root.join("lib/libghostty-vt.so"),
        root.join("lib/libghostty-vt.a"),
    ]
    .into_iter()
    .find(|path| path.exists());
    if direct.is_some() {
        return direct;
    }

    [root.join("zig-out/lib"), root.join("lib")]
        .into_iter()
        .find_map(find_versioned_ghostty_vt_library)
}

fn find_versioned_ghostty_vt_library(dir: PathBuf) -> Option<PathBuf> {
    let mut matches = fs::read_dir(dir)
        .ok()?
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| {
            path.file_name()
                .and_then(|name| name.to_str())
                .map(|name| name.starts_with("libghostty-vt.so."))
                .unwrap_or(false)
        })
        .collect::<Vec<_>>();
    matches.sort();
    matches.pop()
}

fn ghostty_vt_pkg_config(root: Option<&Path>) -> Option<PathBuf> {
    let root = root?;
    [
        root.join("zig-out/share/pkgconfig/libghostty-vt.pc"),
        root.join("share/pkgconfig/libghostty-vt.pc"),
    ]
    .into_iter()
    .find(|path| path.exists())
}

fn ghostty_runtime_resources(
    root: Option<&Path>,
    library: Option<&Path>,
    reported_resources_dir: Option<&Path>,
) -> GhosttyRuntimeResources {
    let resources_dir_env = normalized_env("GHOSTTY_RESOURCES_DIR").map(PathBuf::from);
    ghostty_runtime_resources_with_env(
        root,
        library,
        resources_dir_env.as_deref(),
        reported_resources_dir,
    )
}

fn ghostty_runtime_resources_with_env(
    root: Option<&Path>,
    library: Option<&Path>,
    resources_dir_env: Option<&Path>,
    reported_resources_dir: Option<&Path>,
) -> GhosttyRuntimeResources {
    let candidates = ghostty_runtime_resource_candidates(
        root,
        library,
        resources_dir_env,
        reported_resources_dir,
    );
    for candidate in &candidates {
        if candidate.dir.exists() {
            let mut resources = probe_ghostty_runtime_resources_dir(candidate.dir.clone());
            resources.source = Some(candidate.source);
            return resources;
        }
    }

    GhosttyRuntimeResources {
        dir: None,
        source: None,
        present: false,
        missing: vec!["resources_dir".to_string()],
        themes_present: false,
        i18n_present: false,
    }
}

fn ghostty_runtime_resource_candidates(
    root: Option<&Path>,
    library: Option<&Path>,
    resources_dir_env: Option<&Path>,
    reported_resources_dir: Option<&Path>,
) -> Vec<GhosttyRuntimeResourceCandidate> {
    let mut candidates = Vec::new();
    if let Some(dir) = reported_resources_dir {
        candidates.push(GhosttyRuntimeResourceCandidate {
            dir: dir.to_path_buf(),
            source: GhosttyRuntimeResourceSource::GhosttyReported,
        });
    }

    if let Some(library) = library {
        for ancestor in library.ancestors() {
            candidates.push(GhosttyRuntimeResourceCandidate {
                dir: ancestor.join("share/ghostty"),
                source: GhosttyRuntimeResourceSource::LibraryRelative,
            });
        }
    }

    if let Some(dir) = resources_dir_env {
        candidates.push(GhosttyRuntimeResourceCandidate {
            dir: dir.to_path_buf(),
            source: GhosttyRuntimeResourceSource::Env,
        });
    }

    if let Some(root) = root {
        candidates.push(GhosttyRuntimeResourceCandidate {
            dir: root.join("zig-out/share/ghostty"),
            source: GhosttyRuntimeResourceSource::CheckoutRelative,
        });
        candidates.push(GhosttyRuntimeResourceCandidate {
            dir: root.join("share/ghostty"),
            source: GhosttyRuntimeResourceSource::CheckoutRelative,
        });
    }

    dedupe_resource_candidates(candidates)
}

fn ghostty_reported_resources_dir(library: &Path) -> Option<PathBuf> {
    // ghostty_resources_dir uses Ghostty global state. Keep the DSO pinned
    // after initializing it so later embedded surfaces use the same live state.
    let lib = unsafe { crate::ghostty_embed::GhosttyLibrary::open(library).ok()? };
    let dir = lib.resources_dir().ok().flatten().map(PathBuf::from);
    std::mem::forget(lib);
    dir
}

fn probe_ghostty_runtime_resources_dir(dir: PathBuf) -> GhosttyRuntimeResources {
    let share_dir = dir.parent().unwrap_or(dir.as_path());
    let mut missing = Vec::new();
    if !ghostty_terminfo_present(share_dir) {
        missing.push("terminfo".to_string());
    }
    if !dir.join("shell-integration").is_dir() {
        missing.push("shell-integration".to_string());
    }

    GhosttyRuntimeResources {
        themes_present: dir.join("themes").is_dir(),
        i18n_present: ghostty_i18n_present(share_dir),
        dir: Some(dir),
        source: None,
        present: missing.is_empty(),
        missing,
    }
}

fn ghostty_terminfo_present(share_dir: &Path) -> bool {
    [
        share_dir.join("terminfo/g/ghostty"),
        share_dir.join("terminfo/x/xterm-ghostty"),
        share_dir.join("site-terminfo/g/ghostty"),
        share_dir.join("site-terminfo/x/xterm-ghostty"),
    ]
    .into_iter()
    .any(|path| path.exists())
}

fn ghostty_i18n_present(share_dir: &Path) -> bool {
    let locale_dir = share_dir.join("locale");
    fs::read_dir(locale_dir)
        .ok()
        .into_iter()
        .flatten()
        .filter_map(Result::ok)
        .any(|entry| {
            entry
                .path()
                .join("LC_MESSAGES/com.mitchellh.ghostty.mo")
                .exists()
        })
}

fn dedupe_resource_candidates(
    candidates: Vec<GhosttyRuntimeResourceCandidate>,
) -> Vec<GhosttyRuntimeResourceCandidate> {
    let mut seen = HashSet::new();
    let mut deduped = Vec::new();
    for candidate in candidates {
        if seen.insert(candidate.dir.clone()) {
            deduped.push(candidate);
        }
    }
    deduped
}

fn ghostty_missing_symbols(library: &Path) -> Option<Vec<String>> {
    let symbols = defined_symbols(library)?;
    Some(missing_required_ghostty_symbols(&symbols))
}

fn missing_required_ghostty_symbols(symbols: &HashSet<String>) -> Vec<String> {
    REQUIRED_GHOSTTY_SYMBOLS
        .iter()
        .filter(|symbol| !symbols.contains(**symbol))
        .map(|symbol| (*symbol).to_string())
        .collect()
}

fn ghostty_darwin_symbols_present(library: &Path) -> Option<Vec<String>> {
    let symbols = defined_symbols(library)?;
    Some(present_darwin_only_ghostty_symbols(&symbols))
}

fn present_darwin_only_ghostty_symbols(symbols: &HashSet<String>) -> Vec<String> {
    DARWIN_ONLY_GHOSTTY_SYMBOLS
        .iter()
        .filter(|symbol| symbols.contains(**symbol))
        .map(|symbol| (*symbol).to_string())
        .collect()
}

fn ghostty_internal_symbols_present(library: &Path) -> Option<Vec<String>> {
    let symbols = defined_symbols(library)?;
    Some(present_internal_ghostty_symbols(&symbols))
}

fn present_internal_ghostty_symbols(symbols: &HashSet<String>) -> Vec<String> {
    let mut present = symbols
        .iter()
        .filter(|symbol| {
            INTERNAL_GHOSTTY_SYMBOL_PREFIXES
                .iter()
                .any(|prefix| symbol.starts_with(prefix))
        })
        .cloned()
        .collect::<Vec<_>>();
    present.sort();
    present
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct UnexpectedGhosttyExportSymbols {
    sample: Vec<String>,
    total: usize,
}

fn ghostty_unexpected_export_symbols_present(
    library: &Path,
) -> Option<UnexpectedGhosttyExportSymbols> {
    let symbols = defined_symbols(library)?;
    Some(unexpected_ghostty_export_symbols(&symbols))
}

fn unexpected_ghostty_export_symbols(symbols: &HashSet<String>) -> UnexpectedGhosttyExportSymbols {
    let allowed = allowed_ghostty_export_symbols();
    let mut unexpected = symbols
        .iter()
        .filter(|symbol| !allowed.contains(symbol.as_str()))
        .cloned()
        .collect::<Vec<_>>();
    unexpected.sort();
    let total = unexpected.len();
    let sample = unexpected
        .into_iter()
        .take(MAX_UNEXPECTED_GHOSTTY_EXPORT_SYMBOLS)
        .collect::<Vec<_>>();
    UnexpectedGhosttyExportSymbols { sample, total }
}

fn allowed_ghostty_export_symbols() -> HashSet<&'static str> {
    REQUIRED_GHOSTTY_SYMBOLS
        .iter()
        .chain(OPTIONAL_GHOSTTY_EXPORT_SYMBOLS.iter())
        .copied()
        .collect()
}

fn ghostty_header_linux_platform_value(header_text: &str) -> Option<i32> {
    ghostty_header_enum_value(header_text, "ghostty_platform_e", "GHOSTTY_PLATFORM_LINUX")
}

fn ghostty_header_embedding_abi_version(header_text: &str) -> Option<u32> {
    ghostty_header_define_value(header_text, "GHOSTTY_EMBEDDING_ABI_VERSION")
        .and_then(parse_c_u32_define_value)
}

fn ghostty_header_init_argv_const(header_text: &str) -> bool {
    let compact = header_text
        .chars()
        .filter(|ch| !ch.is_whitespace())
        .collect::<String>();
    compact.contains("ghostty_init(uintptr_t,constchar*const*)")
}

fn ghostty_header_ipc_new_window_arguments_const(header_text: &str) -> bool {
    let compact = header_text
        .chars()
        .filter(|ch| !ch.is_whitespace())
        .collect::<String>();
    compact.contains("constchar*const*arguments;")
}

fn ghostty_header_surface_env_vars_const(header_text: &str) -> bool {
    let compact = header_text
        .chars()
        .filter(|ch| !ch.is_whitespace())
        .collect::<String>();
    compact.contains("constghostty_env_var_s*env_vars;")
}

fn ghostty_header_has_redraw_surface_callback(header_text: &str) -> bool {
    let compact = header_text
        .chars()
        .filter(|ch| !ch.is_whitespace())
        .collect::<String>();
    compact.contains("ghostty_runtime_redraw_surface_cbredraw_surface_cb;")
}

fn ghostty_header_surface_metadata_returns_string(header_text: &str) -> bool {
    let compact = header_text
        .chars()
        .filter(|ch| !ch.is_whitespace())
        .collect::<String>();
    [
        "ghostty_string_sghostty_surface_tty_name(ghostty_surface_t);",
        "ghostty_string_sghostty_surface_title(ghostty_surface_t);",
        "ghostty_string_sghostty_surface_pwd(ghostty_surface_t);",
    ]
    .iter()
    .all(|signature| compact.contains(signature))
}

fn ghostty_header_enum_value(header_text: &str, enum_name: &str, variant: &str) -> Option<i32> {
    let mut in_enum = false;
    let mut variants = Vec::new();
    for line in header_text.lines() {
        let trimmed = line.trim();
        if !in_enum {
            if trimmed.starts_with("typedef enum") {
                in_enum = true;
                variants.clear();
            }
            continue;
        }

        if trimmed.starts_with('}') {
            if trimmed.contains(enum_name) {
                return ghostty_c_enum_variant_value(&variants, variant);
            }
            in_enum = false;
            variants.clear();
            continue;
        }

        variants.push(line.to_string());
    }
    None
}

fn ghostty_c_enum_variant_value(lines: &[String], variant: &str) -> Option<i32> {
    let mut next_value = 0;
    for line in lines {
        let declaration = line
            .split_once("//")
            .map(|(declaration, _)| declaration)
            .unwrap_or(line)
            .trim()
            .trim_end_matches(',')
            .trim();
        if declaration.is_empty() || declaration.starts_with('{') {
            continue;
        }

        let (name, value) = if let Some((name, value)) = declaration.split_once('=') {
            let value = value.trim().trim_end_matches(',').trim();
            (name.trim(), value.parse::<i32>().ok()?)
        } else {
            (declaration, next_value)
        };
        if name == variant {
            return Some(value);
        }
        next_value = value.checked_add(1)?;
    }
    None
}

fn ghostty_header_env_var_limit(header_text: &str) -> Option<usize> {
    ghostty_header_define_value(header_text, "GHOSTTY_SURFACE_MAX_ENV_VARS")
        .and_then(|value| parse_c_u32_define_value(value).map(|value| value as usize))
}

fn ghostty_header_keycode_native_mask(header_text: &str) -> Option<u32> {
    ghostty_header_define_value(header_text, "GHOSTTY_INPUT_KEYCODE_NATIVE_MASK")
        .and_then(parse_c_u32_define_value)
}

fn ghostty_header_keycode_physical_key_flag(header_text: &str) -> Option<u32> {
    ghostty_header_define_value(header_text, "GHOSTTY_INPUT_KEYCODE_PHYSICAL_KEY_FLAG")
        .and_then(parse_c_u32_define_value)
}

fn ghostty_header_define_value<'a>(header_text: &'a str, name: &str) -> Option<&'a str> {
    header_text.lines().find_map(|line| {
        let declaration = line
            .split_once("//")
            .map(|(declaration, _)| declaration)
            .unwrap_or(line)
            .trim();
        let mut parts = declaration.split_whitespace();
        if parts.next()? != "#define" {
            return None;
        }
        if parts.next()? != name {
            return None;
        }
        parts.next()
    })
}

fn parse_c_u32_define_value(value: &str) -> Option<u32> {
    let mut value = value.trim();
    if let Some(inner) = value
        .strip_prefix("UINT32_C(")
        .and_then(|value| value.strip_suffix(')'))
    {
        value = inner.trim();
    }
    value = value.trim_end_matches(|ch| matches!(ch, 'u' | 'U' | 'l' | 'L'));
    if let Some(hex) = value
        .strip_prefix("0x")
        .or_else(|| value.strip_prefix("0X"))
    {
        u32::from_str_radix(hex, 16).ok()
    } else {
        value.parse::<u32>().ok()
    }
}

fn ghostty_embedding_supported(
    header_has_linux_platform: bool,
    header_abi_version_matches: bool,
    header_linux_platform_value_matches: bool,
    header_env_var_limit_matches: bool,
    header_keycode_native_mask_matches: bool,
    header_keycode_physical_key_flag_matches: bool,
    header_has_app_thread_draw_contract: bool,
    header_has_redraw_surface_callback: bool,
    header_surface_env_vars_const: bool,
    header_init_argv_const: bool,
    header_ipc_new_window_arguments_const: bool,
    header_surface_metadata_returns_string: bool,
    library_present: bool,
    symbols_verified: bool,
    missing_symbols: &[String],
    darwin_symbols_hidden: bool,
    internal_symbols_hidden: bool,
    unexpected_export_symbols_hidden: bool,
    library_loadable: bool,
    library_info_query_succeeded: bool,
    library_info_direct_matches_query: bool,
    library_abi_version_matches: bool,
    library_platform_matches: bool,
    library_renderer_backend_matches: bool,
    library_env_var_limit_matches: bool,
    library_layout_sizes_match: bool,
    library_layout_alignments_match: bool,
    library_layout_fingerprint_matches: bool,
    library_constants_fingerprint_matches: bool,
    library_supports_linux_platform: bool,
) -> bool {
    header_has_linux_platform
        && header_abi_version_matches
        && header_linux_platform_value_matches
        && header_env_var_limit_matches
        && header_keycode_native_mask_matches
        && header_keycode_physical_key_flag_matches
        && header_has_app_thread_draw_contract
        && header_has_redraw_surface_callback
        && header_surface_env_vars_const
        && header_init_argv_const
        && header_ipc_new_window_arguments_const
        && header_surface_metadata_returns_string
        && library_present
        && symbols_verified
        && missing_symbols.is_empty()
        && darwin_symbols_hidden
        && internal_symbols_hidden
        && unexpected_export_symbols_hidden
        && library_loadable
        && library_info_query_succeeded
        && library_info_direct_matches_query
        && library_abi_version_matches
        && library_platform_matches
        && library_renderer_backend_matches
        && library_env_var_limit_matches
        && library_layout_sizes_match
        && library_layout_alignments_match
        && library_layout_fingerprint_matches
        && library_constants_fingerprint_matches
        && library_supports_linux_platform
}

fn ghostty_backend_available(
    linux_embedding_supported: bool,
    runtime_resources_present: bool,
) -> bool {
    linux_embedding_supported && runtime_resources_present
}

fn ghostty_embedding_status(
    header_present: bool,
    header_has_linux_platform: bool,
    header_abi_version_matches: bool,
    header_linux_platform_value_matches: bool,
    header_env_var_limit_matches: bool,
    header_keycode_native_mask_matches: bool,
    header_keycode_physical_key_flag_matches: bool,
    header_has_app_thread_draw_contract: bool,
    header_has_redraw_surface_callback: bool,
    header_surface_env_vars_const: bool,
    header_init_argv_const: bool,
    header_ipc_new_window_arguments_const: bool,
    header_surface_metadata_returns_string: bool,
    library_present: bool,
    symbols_verified: bool,
    missing_symbols: &[String],
    darwin_symbols_hidden: bool,
    internal_symbols_hidden: bool,
    unexpected_export_symbols_hidden: bool,
    library_loadable: bool,
    library_info_query_succeeded: bool,
    library_info_direct_matches_query: bool,
    library_abi_version_matches: bool,
    library_platform_matches: bool,
    library_renderer_backend_matches: bool,
    library_env_var_limit_matches: bool,
    library_layout_sizes_match: bool,
    library_layout_alignments_match: bool,
    library_layout_fingerprint_matches: bool,
    library_constants_fingerprint_matches: bool,
    library_supports_linux_platform: bool,
) -> &'static str {
    if !header_present {
        return "missing_header";
    }
    if !header_has_linux_platform {
        return "missing_linux_platform";
    }
    if !header_abi_version_matches {
        return "embedding_header_abi_version_mismatch";
    }
    if !header_linux_platform_value_matches {
        return "linux_platform_value_mismatch";
    }
    if !header_env_var_limit_matches {
        return "env_var_limit_mismatch";
    }
    if !header_keycode_native_mask_matches {
        return "input_keycode_native_mask_mismatch";
    }
    if !header_keycode_physical_key_flag_matches {
        return "input_keycode_physical_key_flag_mismatch";
    }
    if !header_has_app_thread_draw_contract {
        return "missing_app_thread_draw_contract";
    }
    if !header_has_redraw_surface_callback {
        return "missing_redraw_surface_callback";
    }
    if !header_surface_env_vars_const {
        return "surface_env_vars_not_const";
    }
    if !header_init_argv_const {
        return "init_argv_not_const";
    }
    if !header_ipc_new_window_arguments_const {
        return "ipc_new_window_arguments_not_const";
    }
    if !header_surface_metadata_returns_string {
        return "surface_metadata_not_string";
    }
    if !library_present {
        return "missing_library";
    }
    if !library_loadable {
        return "load_error";
    }
    if !symbols_verified {
        return "symbols_unverified";
    }
    if !missing_symbols.is_empty() {
        return "missing_symbols";
    }
    if !darwin_symbols_hidden {
        return "darwin_symbols_exported";
    }
    if !internal_symbols_hidden {
        return "internal_symbols_exported";
    }
    if !unexpected_export_symbols_hidden {
        return "unexpected_export_symbols";
    }
    if !library_info_query_succeeded {
        return "embedding_info_query_error";
    }
    if !library_info_direct_matches_query {
        return "embedding_info_direct_mismatch";
    }
    if !library_abi_version_matches {
        return "embedding_abi_version_mismatch";
    }
    if !library_platform_matches {
        return "embedding_platform_mismatch";
    }
    if !library_renderer_backend_matches {
        return "embedding_renderer_backend_mismatch";
    }
    if !library_env_var_limit_matches {
        return "embedding_env_var_limit_mismatch";
    }
    if !library_layout_sizes_match {
        return "embedding_layout_size_mismatch";
    }
    if !library_layout_alignments_match {
        return "embedding_layout_alignment_mismatch";
    }
    if !library_layout_fingerprint_matches {
        return "embedding_layout_fingerprint_mismatch";
    }
    if !library_constants_fingerprint_matches {
        return "embedding_constants_fingerprint_mismatch";
    }
    if !library_supports_linux_platform {
        return "embedding_linux_platform_unsupported";
    }
    "available"
}

fn ghostty_embedding_layout_mismatch_detail(
    info: Option<&crate::ghostty_embed::GhosttyEmbeddingInfo>,
) -> String {
    let Some(info) = info else {
        return "ghostty-internal did not report embedding layout sizes/alignments".to_string();
    };
    let expected = [
        (
            "runtime_config_size",
            info.runtime_config_size,
            std::mem::size_of::<crate::ghostty_embed::GhosttyRuntimeConfig>(),
        ),
        (
            "surface_config_size",
            info.surface_config_size,
            std::mem::size_of::<crate::ghostty_embed::GhosttySurfaceConfig>(),
        ),
        (
            "platform_linux_size",
            info.platform_linux_size,
            std::mem::size_of::<crate::ghostty_embed::GhosttyPlatformLinux>(),
        ),
        (
            "input_key_size",
            info.input_key_size,
            std::mem::size_of::<crate::ghostty_embed::GhosttyInputKey>(),
        ),
        (
            "target_size",
            info.target_size,
            std::mem::size_of::<crate::ghostty_embed::GhosttyTarget>(),
        ),
        (
            "action_size",
            info.action_size,
            std::mem::size_of::<crate::ghostty_embed::GhosttyAction>(),
        ),
        (
            "text_size",
            info.text_size,
            std::mem::size_of::<crate::ghostty_embed::GhosttyText>(),
        ),
        (
            "selection_size",
            info.selection_size,
            std::mem::size_of::<crate::ghostty_embed::GhosttySelection>(),
        ),
        (
            "string_size",
            info.string_size,
            std::mem::size_of::<crate::ghostty_embed::GhosttyString>(),
        ),
        (
            "surface_size_size",
            info.surface_size_size,
            std::mem::size_of::<crate::ghostty_embed::GhosttySurfaceSizeResult>(),
        ),
        (
            "diagnostic_size",
            info.diagnostic_size,
            std::mem::size_of::<crate::ghostty_embed::GhosttyDiagnostic>(),
        ),
        (
            "env_var_size",
            info.env_var_size,
            std::mem::size_of::<crate::ghostty_embed::GhosttyEnvVar>(),
        ),
        (
            "clipboard_content_size",
            info.clipboard_content_size,
            std::mem::size_of::<crate::ghostty_embed::GhosttyClipboardContent>(),
        ),
        (
            "input_trigger_size",
            info.input_trigger_size,
            std::mem::size_of::<crate::ghostty_embed::GhosttyInputTrigger>(),
        ),
        (
            "ipc_target_size",
            info.ipc_target_size,
            std::mem::size_of::<crate::ghostty_embed::GhosttyIpcTarget>(),
        ),
        (
            "ipc_action_size",
            info.ipc_action_size,
            std::mem::size_of::<crate::ghostty_embed::GhosttyIpcAction>(),
        ),
        (
            "runtime_config_align",
            info.runtime_config_align,
            std::mem::align_of::<crate::ghostty_embed::GhosttyRuntimeConfig>(),
        ),
        (
            "surface_config_align",
            info.surface_config_align,
            std::mem::align_of::<crate::ghostty_embed::GhosttySurfaceConfig>(),
        ),
        (
            "platform_linux_align",
            info.platform_linux_align,
            std::mem::align_of::<crate::ghostty_embed::GhosttyPlatformLinux>(),
        ),
        (
            "input_key_align",
            info.input_key_align,
            std::mem::align_of::<crate::ghostty_embed::GhosttyInputKey>(),
        ),
        (
            "target_align",
            info.target_align,
            std::mem::align_of::<crate::ghostty_embed::GhosttyTarget>(),
        ),
        (
            "action_align",
            info.action_align,
            std::mem::align_of::<crate::ghostty_embed::GhosttyAction>(),
        ),
        (
            "text_align",
            info.text_align,
            std::mem::align_of::<crate::ghostty_embed::GhosttyText>(),
        ),
        (
            "selection_align",
            info.selection_align,
            std::mem::align_of::<crate::ghostty_embed::GhosttySelection>(),
        ),
        (
            "string_align",
            info.string_align,
            std::mem::align_of::<crate::ghostty_embed::GhosttyString>(),
        ),
        (
            "surface_size_align",
            info.surface_size_align,
            std::mem::align_of::<crate::ghostty_embed::GhosttySurfaceSizeResult>(),
        ),
        (
            "diagnostic_align",
            info.diagnostic_align,
            std::mem::align_of::<crate::ghostty_embed::GhosttyDiagnostic>(),
        ),
        (
            "env_var_align",
            info.env_var_align,
            std::mem::align_of::<crate::ghostty_embed::GhosttyEnvVar>(),
        ),
        (
            "clipboard_content_align",
            info.clipboard_content_align,
            std::mem::align_of::<crate::ghostty_embed::GhosttyClipboardContent>(),
        ),
        (
            "input_trigger_align",
            info.input_trigger_align,
            std::mem::align_of::<crate::ghostty_embed::GhosttyInputTrigger>(),
        ),
        (
            "ipc_target_align",
            info.ipc_target_align,
            std::mem::align_of::<crate::ghostty_embed::GhosttyIpcTarget>(),
        ),
        (
            "ipc_action_align",
            info.ipc_action_align,
            std::mem::align_of::<crate::ghostty_embed::GhosttyIpcAction>(),
        ),
    ];
    let mismatches = expected
        .into_iter()
        .filter(|(_, actual, expected)| actual != expected)
        .map(|(name, actual, expected)| format!("{name} library={actual} cmux={expected}"))
        .collect::<Vec<_>>();
    if mismatches.is_empty() {
        "ghostty-internal reports embedding layout sizes/alignments that cmux could not validate"
            .to_string()
    } else {
        format!(
            "ghostty-internal reports embedding layout sizes/alignments that do not match cmux's Rust FFI mirror: {}",
            mismatches.join(", ")
        )
    }
}

fn ghostty_vt_missing_symbols(library: &Path) -> Option<Vec<String>> {
    let symbols = defined_symbols(library)?;
    Some(missing_required_ghostty_vt_symbols(&symbols))
}

fn missing_required_ghostty_vt_symbols(symbols: &HashSet<String>) -> Vec<String> {
    REQUIRED_GHOSTTY_VT_SYMBOLS
        .iter()
        .filter(|symbol| !symbols.contains(**symbol))
        .map(|symbol| (*symbol).to_string())
        .collect()
}

fn ghostty_vt_supported(
    vt_header_present: bool,
    vt_library_present: bool,
    vt_symbols_verified: bool,
    vt_missing_symbols: &[String],
) -> bool {
    vt_header_present && vt_library_present && vt_symbols_verified && vt_missing_symbols.is_empty()
}

fn defined_symbols(library: &Path) -> Option<HashSet<String>> {
    let dynamic = Command::new("nm")
        .arg("-D")
        .arg("--defined-only")
        .arg(library)
        .output()
        .ok()
        .filter(|output| output.status.success());
    let output = match dynamic {
        Some(output) => output,
        None => Command::new("nm")
            .arg("-g")
            .arg("--defined-only")
            .arg(library)
            .output()
            .ok()
            .filter(|output| output.status.success())?,
    };
    Some(parse_nm_symbols(&String::from_utf8_lossy(&output.stdout)))
}

fn parse_nm_symbols(output: &str) -> HashSet<String> {
    output
        .lines()
        .filter_map(|line| line.split_whitespace().last())
        .map(ToString::to_string)
        .collect()
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

fn display_path(path: PathBuf) -> String {
    path.to_string_lossy().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn renderer_snapshot_exposes_beta_gated_right_sidebar_modes_and_feed_data() {
        let mut app = AppState::with_paths(None, None).expect("app state");
        app.set_beta_feature_settings_for_test(crate::config::BetaFeatureSettings::default());
        app.handle(
            "feed.push",
            &json!({
                "event": {
                    "session_id": "renderer-feed-session",
                    "hook_event_name": "Stop",
                    "_source": "codex",
                    "cwd": "/tmp/project"
                }
            }),
        )
        .expect("push feed item");

        let snapshot = snapshot_value(&mut app, &json!({})).expect("snapshot");
        assert_eq!(snapshot["right_sidebar"]["visible"], true);
        assert_eq!(snapshot["right_sidebar"]["mode"], "files");
        assert_eq!(
            snapshot["right_sidebar"]["available_modes"],
            json!(["files", "find", "sessions"])
        );
        assert_eq!(snapshot["right_sidebar"]["focus_generation"], 0);
        assert_eq!(snapshot["right_sidebar"]["feed_items"], json!([]));

        app.handle("sidebar.right", &json!({"action": "hide"}))
            .expect("hide right sidebar");
        let hidden = snapshot_value(&mut app, &json!({})).expect("hidden snapshot");
        assert_eq!(hidden["right_sidebar"]["visible"], false);
        assert_eq!(hidden["right_sidebar"]["feed_items"], json!([]));

        app.handle("sidebar.right", &json!({"action": "focus"}))
            .expect("focus right sidebar");
        let focused = snapshot_value(&mut app, &json!({})).expect("focused snapshot");
        assert_eq!(focused["right_sidebar"]["focus_generation"], 1);

        app.set_beta_feature_settings_for_test(crate::config::BetaFeatureSettings {
            right_sidebar_feed: true,
            ..crate::config::BetaFeatureSettings::default()
        });
        app.handle("sidebar.right", &json!({"action": "set", "mode": "feed"}))
            .expect("show feed sidebar");
        let feed = snapshot_value(&mut app, &json!({})).expect("feed snapshot");
        assert_eq!(feed["right_sidebar"]["mode"], "feed");
        assert_eq!(feed["right_sidebar"]["feed_items"][0]["source"], "codex");
    }

    #[test]
    fn renderer_snapshot_exposes_canvas_mode_frames_and_viewport() {
        let mut app = AppState::with_paths(None, None).expect("app state");
        let focused = app
            .handle("system.identify", &json!({}))
            .expect("focused state");
        let workspace_id = focused["focused"]["workspace_id"]
            .as_str()
            .expect("workspace id")
            .to_string();
        let surface_id = focused["focused"]["surface_id"]
            .as_str()
            .expect("surface id")
            .to_string();
        app.handle(
            "canvas.set_mode",
            &json!({"workspace_id": workspace_id, "mode": "canvas"}),
        )
        .expect("canvas mode");
        app.handle(
            "canvas.set_frame",
            &json!({
                "surface_id": surface_id,
                "x": -120,
                "y": 40,
                "width": 640,
                "height": 360
            }),
        )
        .expect("canvas frame");
        app.handle(
            "canvas.set_viewport",
            &json!({"workspace_id": workspace_id, "x": 80, "y": 120, "zoom": 1.5}),
        )
        .expect("canvas viewport");

        let snapshot = snapshot_value(&mut app, &json!({})).expect("snapshot");
        assert_eq!(snapshot["canvas"]["workspace_id"], workspace_id);
        assert_eq!(snapshot["canvas"]["mode"], "canvas");
        assert_eq!(snapshot["canvas"]["magnification"], 1.5);
        assert_eq!(snapshot["canvas"]["viewport_center"]["x"], 80.0);
        assert_eq!(snapshot["canvas"]["viewport_center"]["y"], 120.0);
        assert_eq!(
            snapshot["canvas"]["panes"][0]["pane_id"],
            snapshot["surface_views"][0]["pane_id"]
        );
        assert_eq!(snapshot["canvas"]["panes"][0]["x"], -120.0);
        assert_eq!(snapshot["canvas"]["panes"][0]["width"], 640.0);
    }

    #[test]
    fn renderer_snapshot_scopes_to_window_and_restores_current_window() {
        let mut app = AppState::with_paths(None, None).expect("app state");
        let first_window = app
            .handle("window.current", &json!({}))
            .expect("initial window")["window_id"]
            .as_str()
            .expect("initial window id")
            .to_string();
        let second_window = app
            .handle("window.create", &json!({"title": "Second"}))
            .expect("second window")["window_id"]
            .as_str()
            .expect("second window id")
            .to_string();

        let first = snapshot_value(
            &mut app,
            &json!({"window_id": first_window, "backend": "ghostty-vt"}),
        )
        .expect("first window snapshot");
        assert_eq!(first["window"]["window_id"], first_window);
        assert!(first["workspaces"]
            .as_array()
            .is_some_and(|workspaces| !workspaces.is_empty()));
        assert!(first["workspaces"].as_array().is_some_and(|workspaces| {
            workspaces
                .iter()
                .all(|workspace| workspace["window_id"].as_str() == Some(first_window.as_str()))
        }));
        assert_eq!(
            app.handle("window.current", &json!({}))
                .expect("restored current window")["window_id"],
            second_window
        );

        assert!(snapshot_value(
            &mut app,
            &json!({"window_id": first_window, "backend": "invalid"}),
        )
        .is_err());
        assert_eq!(
            app.handle("window.current", &json!({}))
                .expect("restored after error")["window_id"],
            second_window
        );
    }

    struct SupportArgs {
        header_has_linux_platform: bool,
        header_abi_version_matches: bool,
        header_linux_platform_value_matches: bool,
        header_env_var_limit_matches: bool,
        header_keycode_native_mask_matches: bool,
        header_keycode_physical_key_flag_matches: bool,
        header_has_app_thread_draw_contract: bool,
        header_has_redraw_surface_callback: bool,
        header_surface_env_vars_const: bool,
        header_init_argv_const: bool,
        header_ipc_new_window_arguments_const: bool,
        header_surface_metadata_returns_string: bool,
        library_present: bool,
        symbols_verified: bool,
        missing_symbols: Vec<String>,
        darwin_symbols_hidden: bool,
        internal_symbols_hidden: bool,
        unexpected_export_symbols_hidden: bool,
        library_loadable: bool,
        library_info_query_succeeded: bool,
        library_info_direct_matches_query: bool,
        library_abi_version_matches: bool,
        library_platform_matches: bool,
        library_renderer_backend_matches: bool,
        library_env_var_limit_matches: bool,
        library_layout_sizes_match: bool,
        library_layout_alignments_match: bool,
        library_layout_fingerprint_matches: bool,
        library_constants_fingerprint_matches: bool,
        library_supports_linux_platform: bool,
    }

    impl Default for SupportArgs {
        fn default() -> Self {
            Self {
                header_has_linux_platform: true,
                header_abi_version_matches: true,
                header_linux_platform_value_matches: true,
                header_env_var_limit_matches: true,
                header_keycode_native_mask_matches: true,
                header_keycode_physical_key_flag_matches: true,
                header_has_app_thread_draw_contract: true,
                header_has_redraw_surface_callback: true,
                header_surface_env_vars_const: true,
                header_init_argv_const: true,
                header_ipc_new_window_arguments_const: true,
                header_surface_metadata_returns_string: true,
                library_present: true,
                symbols_verified: true,
                missing_symbols: Vec::new(),
                darwin_symbols_hidden: true,
                internal_symbols_hidden: true,
                unexpected_export_symbols_hidden: true,
                library_loadable: true,
                library_info_query_succeeded: true,
                library_info_direct_matches_query: true,
                library_abi_version_matches: true,
                library_platform_matches: true,
                library_renderer_backend_matches: true,
                library_env_var_limit_matches: true,
                library_layout_sizes_match: true,
                library_layout_alignments_match: true,
                library_layout_fingerprint_matches: true,
                library_constants_fingerprint_matches: true,
                library_supports_linux_platform: true,
            }
        }
    }

    fn ghostty_embedding_supported_with_args(args: SupportArgs) -> bool {
        ghostty_embedding_supported(
            args.header_has_linux_platform,
            args.header_abi_version_matches,
            args.header_linux_platform_value_matches,
            args.header_env_var_limit_matches,
            args.header_keycode_native_mask_matches,
            args.header_keycode_physical_key_flag_matches,
            args.header_has_app_thread_draw_contract,
            args.header_has_redraw_surface_callback,
            args.header_surface_env_vars_const,
            args.header_init_argv_const,
            args.header_ipc_new_window_arguments_const,
            args.header_surface_metadata_returns_string,
            args.library_present,
            args.symbols_verified,
            &args.missing_symbols,
            args.darwin_symbols_hidden,
            args.internal_symbols_hidden,
            args.unexpected_export_symbols_hidden,
            args.library_loadable,
            args.library_info_query_succeeded,
            args.library_info_direct_matches_query,
            args.library_abi_version_matches,
            args.library_platform_matches,
            args.library_renderer_backend_matches,
            args.library_env_var_limit_matches,
            args.library_layout_sizes_match,
            args.library_layout_alignments_match,
            args.library_layout_fingerprint_matches,
            args.library_constants_fingerprint_matches,
            args.library_supports_linux_platform,
        )
    }

    struct StatusArgs {
        header_present: bool,
        support: SupportArgs,
    }

    impl Default for StatusArgs {
        fn default() -> Self {
            Self {
                header_present: true,
                support: SupportArgs::default(),
            }
        }
    }

    fn ghostty_embedding_status_with_args(args: StatusArgs) -> &'static str {
        ghostty_embedding_status(
            args.header_present,
            args.support.header_has_linux_platform,
            args.support.header_abi_version_matches,
            args.support.header_linux_platform_value_matches,
            args.support.header_env_var_limit_matches,
            args.support.header_keycode_native_mask_matches,
            args.support.header_keycode_physical_key_flag_matches,
            args.support.header_has_app_thread_draw_contract,
            args.support.header_has_redraw_surface_callback,
            args.support.header_surface_env_vars_const,
            args.support.header_init_argv_const,
            args.support.header_ipc_new_window_arguments_const,
            args.support.header_surface_metadata_returns_string,
            args.support.library_present,
            args.support.symbols_verified,
            &args.support.missing_symbols,
            args.support.darwin_symbols_hidden,
            args.support.internal_symbols_hidden,
            args.support.unexpected_export_symbols_hidden,
            args.support.library_loadable,
            args.support.library_info_query_succeeded,
            args.support.library_info_direct_matches_query,
            args.support.library_abi_version_matches,
            args.support.library_platform_matches,
            args.support.library_renderer_backend_matches,
            args.support.library_env_var_limit_matches,
            args.support.library_layout_sizes_match,
            args.support.library_layout_alignments_match,
            args.support.library_layout_fingerprint_matches,
            args.support.library_constants_fingerprint_matches,
            args.support.library_supports_linux_platform,
        )
    }

    fn ghostty_embedding_supported_with_valid_library_info(
        header_has_linux_platform: bool,
        header_linux_platform_value_matches: bool,
        header_env_var_limit_matches: bool,
        header_has_app_thread_draw_contract: bool,
        library_present: bool,
        symbols_verified: bool,
        missing_symbols: &[String],
        library_loadable: bool,
    ) -> bool {
        ghostty_embedding_supported_with_args(SupportArgs {
            header_has_linux_platform,
            header_linux_platform_value_matches,
            header_env_var_limit_matches,
            header_has_app_thread_draw_contract,
            library_present,
            symbols_verified,
            missing_symbols: missing_symbols.to_vec(),
            library_loadable,
            ..SupportArgs::default()
        })
    }

    fn ghostty_embedding_supported_with_contract_checks(
        header_abi_version_matches: bool,
        library_abi_version_matches: bool,
        library_platform_matches: bool,
        library_env_var_limit_matches: bool,
        library_layout_sizes_match: bool,
        library_layout_alignments_match: bool,
        library_layout_fingerprint_matches: bool,
        library_constants_fingerprint_matches: bool,
        library_supports_linux_platform: bool,
    ) -> bool {
        ghostty_embedding_supported_with_args(SupportArgs {
            header_abi_version_matches,
            library_abi_version_matches,
            library_platform_matches,
            library_env_var_limit_matches,
            library_layout_sizes_match,
            library_layout_alignments_match,
            library_layout_fingerprint_matches,
            library_constants_fingerprint_matches,
            library_supports_linux_platform,
            ..SupportArgs::default()
        })
    }

    fn ghostty_embedding_supported_with_renderer_backend(
        library_renderer_backend_matches: bool,
    ) -> bool {
        ghostty_embedding_supported_with_args(SupportArgs {
            library_renderer_backend_matches,
            ..SupportArgs::default()
        })
    }

    fn ghostty_embedding_supported_with_header_keycode_contract(
        header_keycode_native_mask_matches: bool,
        header_keycode_physical_key_flag_matches: bool,
    ) -> bool {
        ghostty_embedding_supported_with_args(SupportArgs {
            header_keycode_native_mask_matches,
            header_keycode_physical_key_flag_matches,
            ..SupportArgs::default()
        })
    }

    fn ghostty_embedding_supported_with_header_init_argv_const(
        header_init_argv_const: bool,
    ) -> bool {
        ghostty_embedding_supported_with_args(SupportArgs {
            header_init_argv_const,
            ..SupportArgs::default()
        })
    }

    fn ghostty_embedding_supported_with_header_surface_env_vars_const(
        header_surface_env_vars_const: bool,
    ) -> bool {
        ghostty_embedding_supported_with_args(SupportArgs {
            header_surface_env_vars_const,
            ..SupportArgs::default()
        })
    }

    fn ghostty_embedding_supported_with_header_redraw_surface_callback(
        header_has_redraw_surface_callback: bool,
    ) -> bool {
        ghostty_embedding_supported_with_args(SupportArgs {
            header_has_redraw_surface_callback,
            ..SupportArgs::default()
        })
    }

    fn ghostty_embedding_supported_with_header_ipc_new_window_arguments_const(
        header_ipc_new_window_arguments_const: bool,
    ) -> bool {
        ghostty_embedding_supported_with_args(SupportArgs {
            header_ipc_new_window_arguments_const,
            ..SupportArgs::default()
        })
    }

    fn ghostty_embedding_supported_with_header_surface_metadata_returns_string(
        header_surface_metadata_returns_string: bool,
    ) -> bool {
        ghostty_embedding_supported_with_args(SupportArgs {
            header_surface_metadata_returns_string,
            ..SupportArgs::default()
        })
    }

    fn ghostty_embedding_supported_with_darwin_symbols_hidden(darwin_symbols_hidden: bool) -> bool {
        ghostty_embedding_supported_with_args(SupportArgs {
            darwin_symbols_hidden,
            ..SupportArgs::default()
        })
    }

    fn ghostty_embedding_supported_with_internal_symbols_hidden(
        internal_symbols_hidden: bool,
    ) -> bool {
        ghostty_embedding_supported_with_args(SupportArgs {
            internal_symbols_hidden,
            ..SupportArgs::default()
        })
    }

    fn ghostty_embedding_supported_with_unexpected_export_symbols_hidden(
        unexpected_export_symbols_hidden: bool,
    ) -> bool {
        ghostty_embedding_supported_with_args(SupportArgs {
            unexpected_export_symbols_hidden,
            ..SupportArgs::default()
        })
    }

    fn ghostty_embedding_status_with_internal_symbols_hidden(
        internal_symbols_hidden: bool,
    ) -> &'static str {
        ghostty_embedding_status_with_args(StatusArgs {
            support: SupportArgs {
                internal_symbols_hidden,
                ..SupportArgs::default()
            },
            ..StatusArgs::default()
        })
    }

    fn ghostty_embedding_status_with_unexpected_export_symbols_hidden(
        unexpected_export_symbols_hidden: bool,
    ) -> &'static str {
        ghostty_embedding_status_with_args(StatusArgs {
            support: SupportArgs {
                unexpected_export_symbols_hidden,
                ..SupportArgs::default()
            },
            ..StatusArgs::default()
        })
    }

    fn ghostty_embedding_supported_with_library_info_report(
        library_info_query_succeeded: bool,
        library_info_direct_matches_query: bool,
    ) -> bool {
        ghostty_embedding_supported_with_args(SupportArgs {
            library_info_query_succeeded,
            library_info_direct_matches_query,
            ..SupportArgs::default()
        })
    }

    fn ghostty_embedding_status_with_valid_library_info(
        header_present: bool,
        header_has_linux_platform: bool,
        header_linux_platform_value_matches: bool,
        header_env_var_limit_matches: bool,
        header_has_app_thread_draw_contract: bool,
        library_present: bool,
        symbols_verified: bool,
        missing_symbols: &[String],
        library_loadable: bool,
    ) -> &'static str {
        ghostty_embedding_status_with_args(StatusArgs {
            header_present,
            support: SupportArgs {
                header_has_linux_platform,
                header_linux_platform_value_matches,
                header_env_var_limit_matches,
                header_has_app_thread_draw_contract,
                library_present,
                symbols_verified,
                missing_symbols: missing_symbols.to_vec(),
                library_loadable,
                ..SupportArgs::default()
            },
        })
    }

    fn ghostty_embedding_status_with_contract_checks(
        header_abi_version_matches: bool,
        library_abi_version_matches: bool,
        library_platform_matches: bool,
        library_env_var_limit_matches: bool,
        library_layout_sizes_match: bool,
        library_layout_alignments_match: bool,
        library_layout_fingerprint_matches: bool,
        library_constants_fingerprint_matches: bool,
        library_supports_linux_platform: bool,
    ) -> &'static str {
        ghostty_embedding_status_with_args(StatusArgs {
            support: SupportArgs {
                header_abi_version_matches,
                library_abi_version_matches,
                library_platform_matches,
                library_env_var_limit_matches,
                library_layout_sizes_match,
                library_layout_alignments_match,
                library_layout_fingerprint_matches,
                library_constants_fingerprint_matches,
                library_supports_linux_platform,
                ..SupportArgs::default()
            },
            ..StatusArgs::default()
        })
    }

    fn ghostty_embedding_status_with_renderer_backend(
        library_renderer_backend_matches: bool,
    ) -> &'static str {
        ghostty_embedding_status_with_args(StatusArgs {
            support: SupportArgs {
                library_renderer_backend_matches,
                ..SupportArgs::default()
            },
            ..StatusArgs::default()
        })
    }

    fn ghostty_embedding_status_with_header_keycode_contract(
        header_keycode_native_mask_matches: bool,
        header_keycode_physical_key_flag_matches: bool,
    ) -> &'static str {
        ghostty_embedding_status_with_args(StatusArgs {
            support: SupportArgs {
                header_keycode_native_mask_matches,
                header_keycode_physical_key_flag_matches,
                ..SupportArgs::default()
            },
            ..StatusArgs::default()
        })
    }

    fn ghostty_embedding_status_with_header_init_argv_const(
        header_init_argv_const: bool,
    ) -> &'static str {
        ghostty_embedding_status_with_args(StatusArgs {
            support: SupportArgs {
                header_init_argv_const,
                ..SupportArgs::default()
            },
            ..StatusArgs::default()
        })
    }

    fn ghostty_embedding_status_with_header_surface_env_vars_const(
        header_surface_env_vars_const: bool,
    ) -> &'static str {
        ghostty_embedding_status_with_args(StatusArgs {
            support: SupportArgs {
                header_surface_env_vars_const,
                ..SupportArgs::default()
            },
            ..StatusArgs::default()
        })
    }

    fn ghostty_embedding_status_with_header_redraw_surface_callback(
        header_has_redraw_surface_callback: bool,
    ) -> &'static str {
        ghostty_embedding_status_with_args(StatusArgs {
            support: SupportArgs {
                header_has_redraw_surface_callback,
                ..SupportArgs::default()
            },
            ..StatusArgs::default()
        })
    }

    fn ghostty_embedding_status_with_header_ipc_new_window_arguments_const(
        header_ipc_new_window_arguments_const: bool,
    ) -> &'static str {
        ghostty_embedding_status_with_args(StatusArgs {
            support: SupportArgs {
                header_ipc_new_window_arguments_const,
                ..SupportArgs::default()
            },
            ..StatusArgs::default()
        })
    }

    fn ghostty_embedding_status_with_header_surface_metadata_returns_string(
        header_surface_metadata_returns_string: bool,
    ) -> &'static str {
        ghostty_embedding_status_with_args(StatusArgs {
            support: SupportArgs {
                header_surface_metadata_returns_string,
                ..SupportArgs::default()
            },
            ..StatusArgs::default()
        })
    }

    fn ghostty_embedding_status_with_darwin_symbols_hidden(
        darwin_symbols_hidden: bool,
    ) -> &'static str {
        ghostty_embedding_status_with_args(StatusArgs {
            support: SupportArgs {
                darwin_symbols_hidden,
                ..SupportArgs::default()
            },
            ..StatusArgs::default()
        })
    }

    #[test]
    fn normalize_backend_accepts_known_aliases() {
        assert_eq!(normalize_backend("core"), Some("core"));
        assert_eq!(normalize_backend("gtk4"), Some("gtk"));
        assert_eq!(normalize_backend("libghostty"), Some("ghostty"));
        assert_eq!(normalize_backend("libghostty-vt"), Some("ghostty-vt"));
        assert_eq!(normalize_backend("vt"), Some("ghostty-vt"));
        assert_eq!(normalize_backend("unknown"), None);
    }

    #[test]
    fn selected_backend_rejects_unknown_explicit_backend() {
        let err = selected_backend(&json!({"backend": "bad"})).expect_err("invalid backend");
        assert_eq!(err.code, "invalid_params");
        assert!(
            err.message.contains("unsupported renderer backend: bad"),
            "message was {}",
            err.message
        );
    }

    #[test]
    fn full_ghostty_backend_skips_core_text_fallback() {
        assert!(!renderer_backend_uses_text_fallback("ghostty"));
        assert!(renderer_backend_uses_text_fallback("core"));
        assert!(renderer_backend_uses_text_fallback("gtk"));
        assert!(renderer_backend_uses_text_fallback("ghostty-vt"));
    }

    #[test]
    fn gtk4_runtime_library_probe_finds_versioned_shared_library() {
        let dir = tempfile::tempdir().expect("tempdir");
        let lib = dir.path().join("libgtk-4.so.1");
        std::fs::write(&lib, "").expect("gtk runtime lib");

        assert_eq!(
            gtk4_runtime_library_in_dirs(vec![PathBuf::from("/missing"), dir.path().to_path_buf()])
                .as_deref(),
            Some(lib.as_path())
        );
    }

    #[test]
    fn gtk4_link_library_probe_requires_unversioned_shared_library() {
        let dir = tempfile::tempdir().expect("tempdir");
        std::fs::write(dir.path().join("libgtk-4.so.1"), "").expect("gtk runtime lib");
        assert_eq!(
            gtk4_link_library_in_dirs(vec![dir.path().to_path_buf()]).as_deref(),
            None
        );

        let link_lib = dir.path().join("libgtk-4.so");
        std::fs::write(&link_lib, "").expect("gtk link lib");
        assert_eq!(
            gtk4_link_library_in_dirs(vec![dir.path().to_path_buf()]).as_deref(),
            Some(link_lib.as_path())
        );
    }

    #[test]
    fn gtk4_pkg_config_error_trims_empty_and_nonempty_stderr() {
        assert_eq!(gtk4_pkg_config_error(b""), None);
        assert_eq!(
            gtk4_pkg_config_error(b" Package 'gtk4' not found\n\n").as_deref(),
            Some("Package 'gtk4' not found")
        );
    }

    #[cfg(not(feature = "gtk"))]
    #[test]
    fn gtk_backend_reports_missing_feature_in_default_build() {
        let diagnostics = diagnostics_for_backend("gtk");
        let gtk_backend = diagnostics
            .backends
            .iter()
            .find(|backend| backend.name == "gtk")
            .expect("gtk backend");

        assert_eq!(diagnostics.selected_backend, "gtk");
        assert!(!diagnostics.gtk4.feature_enabled);
        assert!(!diagnostics.gtk4.available);
        assert!(!gtk_backend.available);
        assert!(
            gtk_backend
                .detail
                .contains("without the GTK renderer feature"),
            "backend detail was {}",
            gtk_backend.detail
        );
    }

    #[test]
    fn surface_views_merge_layout_and_surface_rows() {
        let layout = json!({
            "layout": {
                "selectedPanels": [{
                    "pane_id": "pane-a",
                    "pane_ref": "pane:1",
                    "surface_id": "surface-a",
                    "surface_ref": "surface:1",
                    "type": "terminal",
                    "viewFrame": {"x": 1, "y": 2, "width": 3, "height": 4}
                }]
            }
        });
        let mut surfaces = json!({
            "surfaces": [{
                "surface_id": "surface-a",
                "workspace_id": "workspace-a",
                "workspace_ref": "workspace:1",
                "title": "shell",
                "cwd": "/tmp/cmux-workspace",
                "current_directory": "/tmp/cmux-workspace",
                "terminal_command": "echo ready",
                "terminal_initial_input": "echo boot\n",
                "terminal_env": {
                    "CMUX_WORKSPACE_ID": "workspace-a",
                    "CMUX_SURFACE_ID": "surface-a"
                },
                "terminal_size": {
                    "columns": 100,
                    "rows": 32,
                    "width_px": 1000,
                    "height_px": 640,
                    "cell_width_px": 10,
                    "cell_height_px": 20
                },
                "embedded_terminal_size": {
                    "columns": 100,
                    "rows": 32,
                    "width_px": 1000,
                    "height_px": 640,
                    "cell_width_px": 10,
                    "cell_height_px": 20
                },
                "terminal_size_limit": {
                    "min_width_px": 100,
                    "min_height_px": 40,
                    "max_width_px": 2000,
                    "max_height_px": 1200
                },
                "terminal_initial_size": {
                    "width_px": 900,
                    "height_px": 600
                },
                "terminal_cell_size": {
                    "width_px": 10,
                    "height_px": 20
                },
                "terminal_renderer_health": "healthy",
                "terminal_prompt_title": "tab",
                "terminal_quit_timer": "start",
                "terminal_float_window": "toggle",
                "terminal_secure_input": "on",
                "terminal_color_change": {
                    "kind": "palette",
                    "palette_index": 12,
                    "r": 1,
                    "g": 2,
                    "b": 3
                },
                "terminal_key_sequence": {
                    "active": true,
                    "trigger": "unicode:U+0078 mods=shift+ctrl"
                },
                "terminal_key_tables": ["leader", "resize"],
                "terminal_key_table": "resize",
                "terminal_on_screen_keyboard_requests": 2,
                "terminal_config_change_count": 1,
                "terminal_last_window_action": {
                    "sequence": 2,
                    "action": "new_window",
                    "value": null,
                    "amount": null
                },
                "terminal_last_tab_action": {
                    "sequence": 3,
                    "action": "goto_tab",
                    "value": null,
                    "amount": -2
                },
                "terminal_last_ui_action": {
                    "sequence": 4,
                    "action": "toggle_fullscreen",
                    "value": "native",
                    "amount": null
                },
                "terminal_progress": {"state": "set", "percent": 50},
                "terminal_last_command": {"exit_code": 0, "duration_ms": 800},
                "preview": "ready",
                "browser": {"title": "shell browser"},
                "project": null,
                "focused": true,
                "runtime_surface_ready": false,
                "terminal_loading": true,
                "loading": true,
                "loading_message": "Loading terminal..."
            }]
        });

        if let Some(surface) = surfaces["surfaces"][0].as_object_mut() {
            surface.insert("pane_id".to_string(), json!("pane-a"));
            surface.insert("pane_ref".to_string(), json!("pane:1"));
            surface.insert("terminal_font_size".to_string(), json!(15.5));
            surface.insert("terminal_wait_after_command".to_string(), json!(true));
            surface.insert("terminal_mouse_captured".to_string(), json!(true));
            surface.insert("mouse_captured".to_string(), json!(true));
            surface.insert("terminal_readonly".to_string(), json!(true));
            surface.insert("readonly".to_string(), json!(true));
            surface.insert("terminal_config_reload_count".to_string(), json!(3));
            surface.insert("terminal_last_config_reload_soft".to_string(), json!(true));
            surface.insert("terminal_needs_confirm_quit".to_string(), json!(true));
            surface.insert("needs_confirm_quit".to_string(), json!(true));
            surface.insert("terminal_has_selection".to_string(), json!(true));
            surface.insert("has_selection".to_string(), json!(true));
            surface.insert(
                "terminal_selection_text".to_string(),
                json!("selected text"),
            );
            surface.insert("terminal_cursor_shape".to_string(), json!("pointer"));
            surface.insert("cursor_shape".to_string(), json!("pointer"));
            surface.insert("terminal_cursor_visible".to_string(), json!(false));
            surface.insert("cursor_visible".to_string(), json!(false));
            surface.insert("terminal_mouse_over_link".to_string(), json!(true));
            surface.insert("mouse_over_link".to_string(), json!(true));
            surface.insert(
                "terminal_mouse_over_link_url".to_string(),
                json!("https://example.test/docs"),
            );
            surface.insert(
                "mouse_over_link_url".to_string(),
                json!("https://example.test/docs"),
            );
            surface.insert(
                "terminal_last_layout_action".to_string(),
                json!({
                    "sequence": 5,
                    "action": "toggle_split_zoom",
                    "value": null,
                    "amount": null
                }),
            );
            surface.insert(
                "terminal_last_app_action".to_string(),
                json!({
                    "sequence": 1,
                    "action": "close_all_windows",
                    "value": null,
                    "amount": null
                }),
            );
        }
        surfaces["surfaces"].as_array_mut().unwrap().push(json!({
            "surface_id": "surface-b",
            "surface_ref": "surface:2",
            "pane_id": "pane-a",
            "pane_ref": "pane:1",
            "workspace_id": "workspace-a",
            "workspace_ref": "workspace:1",
            "title": "docs",
            "type": "browser",
            "url": "https://example.test",
            "pinned": true,
            "unread": true,
            "focused": false
        }));

        let views = surface_views(&layout, &surfaces);
        assert_eq!(views[0]["title"], "shell");
        assert_eq!(views[0]["workspace_id"], "workspace-a");
        assert_eq!(views[0]["workspace_ref"], "workspace:1");
        assert_eq!(views[0]["cwd"], "/tmp/cmux-workspace");
        assert_eq!(views[0]["current_directory"], "/tmp/cmux-workspace");
        assert_eq!(views[0]["terminal_command"], "echo ready");
        assert_eq!(views[0]["terminal_initial_input"], "echo boot\n");
        assert_eq!(views[0]["terminal_wait_after_command"], true);
        assert_eq!(views[0]["terminal_font_size"], 15.5);
        assert_eq!(views[0]["terminal_env"]["CMUX_WORKSPACE_ID"], "workspace-a");
        assert_eq!(views[0]["terminal_env"]["CMUX_SURFACE_ID"], "surface-a");
        assert_eq!(views[0]["terminal_size"]["columns"], 100);
        assert_eq!(views[0]["terminal_size"]["cell_height_px"], 20);
        assert_eq!(views[0]["embedded_terminal_size"]["rows"], 32);
        assert_eq!(views[0]["terminal_size_limit"]["max_width_px"], 2000);
        assert_eq!(views[0]["terminal_initial_size"]["width_px"], 900);
        assert_eq!(views[0]["terminal_cell_size"]["height_px"], 20);
        assert_eq!(views[0]["terminal_renderer_health"], "healthy");
        assert_eq!(views[0]["terminal_mouse_captured"], true);
        assert_eq!(views[0]["mouse_captured"], true);
        assert_eq!(views[0]["terminal_readonly"], true);
        assert_eq!(views[0]["readonly"], true);
        assert_eq!(views[0]["terminal_needs_confirm_quit"], true);
        assert_eq!(views[0]["needs_confirm_quit"], true);
        assert_eq!(views[0]["terminal_has_selection"], true);
        assert_eq!(views[0]["has_selection"], true);
        assert_eq!(views[0]["terminal_selection_text"], "selected text");
        assert_eq!(views[0]["terminal_cursor_shape"], "pointer");
        assert_eq!(views[0]["cursor_shape"], "pointer");
        assert_eq!(views[0]["terminal_cursor_visible"], false);
        assert_eq!(views[0]["cursor_visible"], false);
        assert_eq!(views[0]["terminal_mouse_over_link"], true);
        assert_eq!(views[0]["mouse_over_link"], true);
        assert_eq!(
            views[0]["terminal_mouse_over_link_url"],
            "https://example.test/docs"
        );
        assert_eq!(views[0]["mouse_over_link_url"], "https://example.test/docs");
        assert_eq!(views[0]["terminal_prompt_title"], "tab");
        assert_eq!(views[0]["terminal_quit_timer"], "start");
        assert_eq!(views[0]["terminal_float_window"], "toggle");
        assert_eq!(views[0]["terminal_secure_input"], "on");
        assert_eq!(views[0]["terminal_color_change"]["palette_index"], 12);
        assert_eq!(views[0]["terminal_color_change"]["g"], 2);
        assert_eq!(views[0]["terminal_key_sequence"]["active"], true);
        assert_eq!(views[0]["terminal_key_table"], "resize");
        assert_eq!(views[0]["terminal_key_tables"], json!(["leader", "resize"]));
        assert_eq!(views[0]["terminal_on_screen_keyboard_requests"], 2);
        assert_eq!(views[0]["terminal_config_change_count"], 1);
        assert_eq!(views[0]["terminal_config_reload_count"], 3);
        assert_eq!(views[0]["terminal_last_config_reload_soft"], true);
        assert_eq!(
            views[0]["terminal_last_app_action"]["action"],
            "close_all_windows"
        );
        assert_eq!(
            views[0]["terminal_last_window_action"]["action"],
            "new_window"
        );
        assert_eq!(views[0]["terminal_last_tab_action"]["amount"], -2);
        assert_eq!(
            views[0]["terminal_last_layout_action"]["action"],
            "toggle_split_zoom"
        );
        assert_eq!(views[0]["terminal_last_ui_action"]["value"], "native");
        assert_eq!(views[0]["terminal_progress"]["state"], "set");
        assert_eq!(views[0]["terminal_last_command"]["duration_ms"], 800);
        assert_eq!(views[0]["preview"], "ready");
        assert_eq!(views[0]["browser"]["title"], "shell browser");
        assert_eq!(views[0]["focused"], true);
        assert_eq!(views[0]["runtime_surface_ready"], false);
        assert_eq!(views[0]["terminal_loading"], true);
        assert_eq!(views[0]["loading"], true);
        assert_eq!(views[0]["loading_message"], "Loading terminal...");
        assert_eq!(views[0]["tab_count"], 2);
        assert_eq!(views[0]["tabs"][0]["surface_id"], "surface-a");
        assert_eq!(views[0]["tabs"][0]["selected"], true);
        assert_eq!(views[0]["tabs"][1]["surface_id"], "surface-b");
        assert_eq!(views[0]["tabs"][1]["title"], "docs");
        assert_eq!(views[0]["tabs"][1]["kind"], "browser");
        assert_eq!(views[0]["tabs"][1]["pinned"], true);
        assert_eq!(views[0]["tabs"][1]["unread"], true);
        assert_eq!(views[0]["tabs"][1]["selected"], false);
        assert_eq!(views[0]["visible"], true);
        assert_eq!(views[0]["frame"]["width"], 3);
    }

    #[test]
    fn window_surface_inventory_flattens_every_workspace_in_the_window() {
        let tree = json!({
            "windows": [{
                "workspaces": [
                    {"panes": [{"surfaces": [{"id": "surface-a", "type": "terminal"}]}]},
                    {"panes": [{"surfaces": [{"id": "surface-b", "type": "browser"}]}]}
                ]
            }]
        });

        let inventory = window_surface_inventory(&tree);
        assert_eq!(inventory.as_array().unwrap().len(), 2);
        assert_eq!(inventory[0]["id"], "surface-a");
        assert_eq!(inventory[1]["id"], "surface-b");
    }

    #[test]
    fn frame_terminal_size_uses_debug_cell_geometry() {
        let frame = json!({"width": 400.0, "height": 100.0});
        assert_eq!(frame_terminal_size(Some(&frame)), (40, 5));
        let tiny = json!({"width": 1.0, "height": 1.0});
        assert_eq!(frame_terminal_size(Some(&tiny)), (1, 1));
    }

    #[test]
    fn renderer_text_fallback_builds_render_grid() {
        let grid = render_grid_from_text("surface-a", 42, 8, 2, "one\ntwo\nthree");
        assert_eq!(grid["format"], "cmux.render-grid.v1");
        assert_eq!(grid["parser"], "renderer-text-fallback");
        assert_eq!(grid["surface_id"], "surface-a");
        assert_eq!(grid["state_seq"], 42);
        assert_eq!(grid["columns"], 8);
        assert_eq!(grid["rows"], 2);
        assert_eq!(grid["scrollback_rows"], 1);
        assert_eq!(grid["scrollback_spans"][0]["row"], 0);
        assert_eq!(grid["scrollback_spans"][0]["text"], "one");
        assert_eq!(grid["row_spans"][0]["row"], 0);
        assert_eq!(grid["row_spans"][0]["text"], "two");
        assert_eq!(grid["row_spans"][1]["row"], 1);
        assert_eq!(grid["row_spans"][1]["text"], "three");
        assert_eq!(grid["cursor"]["row"], 1);
        assert_eq!(grid["cursor"]["column"], 5);
    }

    #[test]
    fn renderer_text_fallback_keeps_cursor_only_empty_grid() {
        let grid = render_grid_from_text("surface-a", 43, 8, 2, "");
        assert_eq!(grid["format"], "cmux.render-grid.v1");
        assert_eq!(grid["parser"], "renderer-text-fallback");
        assert_eq!(grid["columns"], 8);
        assert_eq!(grid["rows"], 2);
        assert_eq!(grid["row_spans"].as_array().unwrap().len(), 0);
        assert_eq!(grid["scrollback_rows"], 0);
        assert_eq!(grid["cursor"]["row"], 0);
        assert_eq!(grid["cursor"]["column"], 0);
        assert_eq!(grid["cursor"]["visible"], true);
    }

    #[test]
    fn renderer_text_fallback_places_cursor_on_trailing_blank_line() {
        let grid = render_grid_from_text("surface-a", 44, 8, 2, "ready\n");
        assert_eq!(grid["row_spans"][0]["row"], 0);
        assert_eq!(grid["row_spans"][0]["text"], "ready");
        assert_eq!(grid["cursor"]["row"], 1);
        assert_eq!(grid["cursor"]["column"], 0);
        assert_eq!(grid["cursor"]["visible"], true);
    }

    #[test]
    fn renderer_text_fallback_tracks_active_alternate_screen() {
        let grid = render_grid_from_text(
            "surface-a",
            45,
            20,
            3,
            "primary\x1b[?1049hALT\x1b[2;5Hrow\x1b[?25l",
        );
        let spans = grid["row_spans"].as_array().expect("row spans");
        assert_eq!(grid["active_screen"], "alternate");
        assert_eq!(spans.len(), 2, "grid was {grid}");
        assert_eq!(spans[0]["row"], 0);
        assert_eq!(spans[0]["text"], "ALT");
        assert_eq!(spans[1]["row"], 1);
        assert_eq!(spans[1]["column"], 4);
        assert_eq!(spans[1]["text"], "row");
        assert_eq!(grid["cursor"]["row"], 1);
        assert_eq!(grid["cursor"]["column"], 7);
        assert_eq!(grid["cursor"]["visible"], false);
    }

    #[test]
    fn renderer_text_fallback_restores_primary_after_alternate_screen() {
        let grid = render_grid_from_text(
            "surface-a",
            46,
            30,
            2,
            "primary\x1b[?1049hALT\x1b[2;5Hrow\x1b[?1049l-back",
        );
        let spans = grid["row_spans"].as_array().expect("row spans");
        assert_eq!(grid["active_screen"], "primary");
        assert_eq!(spans.len(), 1, "grid was {grid}");
        assert_eq!(spans[0]["text"], "primary-back");
        assert_eq!(grid["cursor"]["row"], 0);
        assert_eq!(grid["cursor"]["column"], 12);
        assert_eq!(grid["cursor"]["visible"], true);
    }

    #[test]
    fn renderer_text_fallback_models_partial_display_erase() {
        let erase_to_end = render_grid_from_text("surface-a", 47, 20, 2, "abcdef\x1b[1;4H\x1b[JZ");
        let spans = erase_to_end["row_spans"].as_array().expect("row spans");
        assert_eq!(spans.len(), 1, "grid was {erase_to_end}");
        assert_eq!(spans[0]["column"], 0);
        assert_eq!(spans[0]["text"], "abcZ");
        assert_eq!(erase_to_end["cursor"]["column"], 4);

        let erase_to_start =
            render_grid_from_text("surface-a", 48, 20, 2, "abcdef\x1b[1;4H\x1b[1JZ");
        let spans = erase_to_start["row_spans"].as_array().expect("row spans");
        assert_eq!(spans.len(), 1, "grid was {erase_to_start}");
        assert_eq!(spans[0]["column"], 3);
        assert_eq!(spans[0]["text"], "Zef");
        assert_eq!(erase_to_start["cursor"]["column"], 4);
    }

    #[test]
    fn renderer_text_fallback_models_common_esc_controls() {
        let grid = render_grid_from_text(
            "surface-a",
            49,
            20,
            3,
            "abc\x1b7XYZ\x1b8!\x1bEline\x1bDtail",
        );
        let spans = grid["row_spans"].as_array().expect("row spans");
        assert_eq!(spans.len(), 3, "grid was {grid}");
        assert_eq!(spans[0]["row"], 0);
        assert_eq!(spans[0]["text"], "abc!YZ");
        assert_eq!(spans[1]["row"], 1);
        assert_eq!(spans[1]["text"], "line");
        assert_eq!(spans[2]["row"], 2);
        assert_eq!(spans[2]["column"], 4);
        assert_eq!(spans[2]["text"], "tail");
        assert_eq!(grid["cursor"]["row"], 2);
        assert_eq!(grid["cursor"]["column"], 8);
    }

    #[test]
    fn renderer_text_fallback_reset_clears_screen_and_modes() {
        let grid = render_grid_from_text(
            "surface-a",
            50,
            20,
            2,
            "before\x1b[?1049hALT\x1b[?25l\x1bcafter",
        );
        let spans = grid["row_spans"].as_array().expect("row spans");
        assert_eq!(grid["active_screen"], "primary");
        assert_eq!(spans.len(), 1, "grid was {grid}");
        assert_eq!(spans[0]["text"], "after");
        assert_eq!(grid["cursor"]["visible"], true);
    }

    #[test]
    fn renderer_text_fallback_tracks_cursor_shape_and_modes() {
        let grid = render_grid_from_text(
            "surface-a",
            51,
            40,
            2,
            "ready\x1b[5 q\x1b[?1;7;1000;1004;1006;2004h\x1b=",
        );
        assert_eq!(grid["cursor"]["style"], "bar");
        assert_eq!(grid["cursor"]["blinking"], true);
        assert_eq!(
            grid["modes"],
            json!([
                "application_cursor_keys",
                "application_keypad",
                "wraparound",
                "bracketed_paste",
                "focus_events",
                "mouse_button_tracking",
                "mouse_sgr"
            ])
        );
    }

    #[test]
    fn renderer_text_fallback_clears_cursor_shape_and_modes() {
        let grid = render_grid_from_text(
            "surface-a",
            52,
            40,
            2,
            "ready\x1b[3 q\x1b[?2004;1006h\x1b=\x1b[6 q\x1b[?2004;1006l\x1b>",
        );
        assert_eq!(grid["cursor"]["style"], "bar");
        assert_eq!(grid["cursor"]["blinking"], false);
        assert_eq!(grid["modes"], json!([]));
    }

    #[test]
    fn renderer_text_fallback_preserves_ansi_sgr_styles() {
        let grid = render_grid_from_text(
            "surface-a",
            53,
            80,
            2,
            "plain \u{1b}[31;48;2;1;2;3;1;3;4mRED\u{1b}[0m done",
        );
        let spans = grid["row_spans"].as_array().expect("row spans");
        assert_eq!(spans.len(), 3, "grid was {grid}");
        assert_eq!(spans[0]["text"], "plain ");
        assert_eq!(spans[1]["text"], "RED");
        assert_eq!(spans[2]["text"], " done");
        let style_id = spans[1]["style_id"].as_u64().expect("style id");
        let style = grid["styles"]
            .as_array()
            .unwrap()
            .iter()
            .find(|style| style["id"].as_u64() == Some(style_id))
            .expect("styled span style");
        assert_eq!(style["fg"], json!({"r": 205, "g": 49, "b": 49}));
        assert_eq!(style["bg"], json!({"r": 1, "g": 2, "b": 3}));
        assert_eq!(style["bold"], true);
        assert_eq!(style["italic"], true);
        assert_eq!(style["underline"], true);
    }

    #[test]
    fn renderer_text_fallback_models_control_sequences_with_styles() {
        let grid = render_grid_from_text(
            "surface-a",
            54,
            20,
            2,
            "old value\r\u{1b}[2K\u{1b}[38;5;196mnew value\u{1b}[0m\n",
        );
        let spans = grid["row_spans"].as_array().expect("row spans");
        assert_eq!(spans.len(), 1, "grid was {grid}");
        assert_eq!(spans[0]["row"], 0);
        assert_eq!(spans[0]["text"], "new value");
        let style_id = spans[0]["style_id"].as_u64().expect("style id");
        let style = grid["styles"]
            .as_array()
            .unwrap()
            .iter()
            .find(|style| style["id"].as_u64() == Some(style_id))
            .expect("styled span style");
        assert_eq!(style["fg"], json!({"r": 255, "g": 0, "b": 0}));
        assert_eq!(grid["cursor"]["row"], 1);
        assert_eq!(grid["cursor"]["column"], 0);
    }

    #[test]
    fn ghostty_vt_probe_finds_zig_out_headers_and_versioned_library() {
        let dir = tempfile::tempdir().expect("tempdir");
        let root = dir.path();
        let include_dir = root.join("zig-out/include/ghostty");
        let lib_dir = root.join("zig-out/lib");
        let pkg_dir = root.join("zig-out/share/pkgconfig");
        std::fs::create_dir_all(&include_dir).expect("include dir");
        std::fs::create_dir_all(&lib_dir).expect("lib dir");
        std::fs::create_dir_all(&pkg_dir).expect("pkg dir");
        std::fs::write(include_dir.join("vt.h"), "/* libghostty-vt */").expect("vt header");
        std::fs::write(lib_dir.join("libghostty-vt.so.0.1.0"), "").expect("vt so");
        std::fs::write(pkg_dir.join("libghostty-vt.pc"), "").expect("pc");

        assert_eq!(
            ghostty_vt_header(Some(root)).as_deref(),
            Some(include_dir.join("vt.h").as_path())
        );
        assert_eq!(
            ghostty_vt_library(Some(root)).as_deref(),
            Some(lib_dir.join("libghostty-vt.so.0.1.0").as_path())
        );
        assert_eq!(
            ghostty_vt_pkg_config(Some(root)).as_deref(),
            Some(pkg_dir.join("libghostty-vt.pc").as_path())
        );
    }

    #[test]
    fn ghostty_vt_probe_infers_installed_root_from_library_path() {
        let dir = tempfile::tempdir().expect("tempdir");
        let root = dir.path();
        let include_dir = root.join("include/ghostty");
        let lib_dir = root.join("lib");
        std::fs::create_dir_all(&include_dir).expect("include dir");
        std::fs::create_dir_all(&lib_dir).expect("lib dir");
        std::fs::write(include_dir.join("vt.h"), "/* libghostty-vt */").expect("vt header");
        let library = lib_dir.join("libghostty-vt.so.0.1.0");
        std::fs::write(&library, "").expect("vt so");

        assert_eq!(
            ghostty_root_from_library_path(&library).as_deref(),
            Some(root)
        );
    }

    #[test]
    fn ghostty_vt_probe_infers_checkout_root_from_zig_out_library_path() {
        let dir = tempfile::tempdir().expect("tempdir");
        let root = dir.path();
        let include_dir = root.join("include/ghostty");
        let lib_dir = root.join("zig-out/lib");
        std::fs::create_dir_all(&include_dir).expect("include dir");
        std::fs::create_dir_all(&lib_dir).expect("lib dir");
        std::fs::write(include_dir.join("vt.h"), "/* libghostty-vt */").expect("vt header");
        let library = lib_dir.join("libghostty-vt.so.0.1.0");
        std::fs::write(&library, "").expect("vt so");

        assert_eq!(
            ghostty_root_from_library_path(&library).as_deref(),
            Some(root)
        );
    }

    #[test]
    fn ghostty_embedding_probe_finds_linux_header_and_internal_library() {
        let dir = tempfile::tempdir().expect("tempdir");
        let root = dir.path();
        let include_dir = root.join("include");
        let lib_dir = root.join("zig-out/lib");
        std::fs::create_dir_all(&include_dir).expect("include dir");
        std::fs::create_dir_all(&lib_dir).expect("lib dir");
        std::fs::write(
            include_dir.join("ghostty.h"),
            "typedef enum { GHOSTTY_PLATFORM_LINUX } ghostty_platform_e;",
        )
        .expect("ghostty header");
        std::fs::write(lib_dir.join("libghostty-internal.so"), "").expect("ghostty so");

        let header = ghostty_embedding_header(Some(root)).expect("embedding header");
        let header_text = std::fs::read_to_string(header).expect("header text");
        assert!(header_text.contains("GHOSTTY_PLATFORM_LINUX"));
        assert_eq!(
            ghostty_library(Some(root)).as_deref(),
            Some(lib_dir.join("libghostty-internal.so").as_path())
        );
    }

    #[test]
    fn ghostty_embedding_probe_finds_installed_internal_library() {
        let dir = tempfile::tempdir().expect("tempdir");
        let root = dir.path();
        let include_dir = root.join("include");
        let lib_dir = root.join("lib");
        std::fs::create_dir_all(&include_dir).expect("include dir");
        std::fs::create_dir_all(&lib_dir).expect("lib dir");
        std::fs::write(
            include_dir.join("ghostty.h"),
            "typedef enum { GHOSTTY_PLATFORM_LINUX } ghostty_platform_e;",
        )
        .expect("ghostty header");
        std::fs::write(lib_dir.join("libghostty-internal.so"), "").expect("ghostty so");

        assert_eq!(
            ghostty_library(Some(root)).as_deref(),
            Some(lib_dir.join("libghostty-internal.so").as_path())
        );
    }

    #[test]
    fn ghostty_probe_infers_installed_root_from_internal_library_path() {
        let dir = tempfile::tempdir().expect("tempdir");
        let root = dir.path();
        let include_dir = root.join("include");
        let lib_dir = root.join("lib");
        std::fs::create_dir_all(&include_dir).expect("include dir");
        std::fs::create_dir_all(&lib_dir).expect("lib dir");
        std::fs::write(
            include_dir.join("ghostty.h"),
            "typedef enum { GHOSTTY_PLATFORM_LINUX } ghostty_platform_e;",
        )
        .expect("ghostty header");
        let library = lib_dir.join("libghostty-internal.so");
        std::fs::write(&library, "").expect("ghostty so");

        assert_eq!(
            ghostty_root_from_library_path(&library).as_deref(),
            Some(root)
        );
    }

    #[test]
    fn ghostty_probe_infers_checkout_root_from_zig_out_internal_library_path() {
        let dir = tempfile::tempdir().expect("tempdir");
        let root = dir.path();
        let include_dir = root.join("include");
        let lib_dir = root.join("zig-out/lib");
        std::fs::create_dir_all(&include_dir).expect("include dir");
        std::fs::create_dir_all(&lib_dir).expect("lib dir");
        std::fs::write(
            include_dir.join("ghostty.h"),
            "typedef enum { GHOSTTY_PLATFORM_LINUX } ghostty_platform_e;",
        )
        .expect("ghostty header");
        let library = lib_dir.join("libghostty-internal.so");
        std::fs::write(&library, "").expect("ghostty so");

        assert_eq!(
            ghostty_root_from_library_path(&library).as_deref(),
            Some(root)
        );
    }

    #[test]
    fn ghostty_embedding_probe_keeps_legacy_internal_library_fallback() {
        let dir = tempfile::tempdir().expect("tempdir");
        let root = dir.path();
        let lib_dir = root.join("zig-out/lib");
        std::fs::create_dir_all(&lib_dir).expect("lib dir");
        std::fs::write(lib_dir.join("ghostty-internal.so"), "").expect("ghostty so");

        assert_eq!(
            ghostty_library(Some(root)).as_deref(),
            Some(lib_dir.join("ghostty-internal.so").as_path())
        );
    }

    #[test]
    fn ghostty_runtime_resource_probe_finds_zig_out_resources_from_library() {
        let dir = tempfile::tempdir().expect("tempdir");
        let root = dir.path();
        let lib_dir = root.join("zig-out/lib");
        let share_dir = root.join("zig-out/share");
        let resources_dir = share_dir.join("ghostty");
        std::fs::create_dir_all(&lib_dir).expect("lib dir");
        std::fs::create_dir_all(share_dir.join("terminfo/g")).expect("terminfo dir");
        std::fs::create_dir_all(resources_dir.join("shell-integration/bash"))
            .expect("shell integration dir");
        std::fs::create_dir_all(resources_dir.join("themes")).expect("themes dir");
        std::fs::create_dir_all(share_dir.join("locale/en/LC_MESSAGES")).expect("locale dir");
        let library = lib_dir.join("libghostty-internal.so");
        std::fs::write(&library, "").expect("ghostty so");
        std::fs::write(share_dir.join("terminfo/g/ghostty"), "").expect("terminfo");
        std::fs::write(
            share_dir.join("locale/en/LC_MESSAGES/com.mitchellh.ghostty.mo"),
            "",
        )
        .expect("message catalog");

        let resources = ghostty_runtime_resources_with_env(Some(root), Some(&library), None, None);

        assert_eq!(resources.dir.as_deref(), Some(resources_dir.as_path()));
        assert_eq!(
            resources.source,
            Some(GhosttyRuntimeResourceSource::LibraryRelative)
        );
        assert!(resources.present);
        assert!(resources.missing.is_empty());
        assert!(resources.themes_present);
        assert!(resources.i18n_present);
    }

    #[test]
    fn ghostty_runtime_resource_probe_reports_missing_required_resources() {
        let dir = tempfile::tempdir().expect("tempdir");
        let root = dir.path();
        let resources_dir = root.join("zig-out/share/ghostty");
        std::fs::create_dir_all(&resources_dir).expect("resources dir");

        let resources = ghostty_runtime_resources_with_env(Some(root), None, None, None);

        assert_eq!(resources.dir.as_deref(), Some(resources_dir.as_path()));
        assert_eq!(
            resources.source,
            Some(GhosttyRuntimeResourceSource::CheckoutRelative)
        );
        assert!(!resources.present);
        assert_eq!(
            resources.missing,
            vec!["terminfo".to_string(), "shell-integration".to_string()]
        );
        assert!(!resources.themes_present);
        assert!(!resources.i18n_present);
    }

    #[test]
    fn ghostty_runtime_resource_probe_prefers_reported_resources_dir() {
        let dir = tempfile::tempdir().expect("tempdir");
        let root = dir.path();
        let reported_share = root.join("reported/share");
        let reported_resources = reported_share.join("ghostty");
        let env_share = root.join("env/share");
        let env_resources = env_share.join("ghostty");
        let fallback_resources = root.join("zig-out/share/ghostty");
        std::fs::create_dir_all(reported_share.join("terminfo/g")).expect("terminfo dir");
        std::fs::create_dir_all(reported_resources.join("shell-integration"))
            .expect("reported shell dir");
        std::fs::create_dir_all(env_share.join("terminfo/g")).expect("env terminfo dir");
        std::fs::create_dir_all(env_resources.join("shell-integration")).expect("env shell dir");
        std::fs::create_dir_all(&fallback_resources).expect("fallback dir");
        std::fs::write(reported_share.join("terminfo/g/ghostty"), "").expect("terminfo");
        std::fs::write(env_share.join("terminfo/g/ghostty"), "").expect("env terminfo");

        let resources = ghostty_runtime_resources_with_env(
            Some(root),
            None,
            Some(&env_resources),
            Some(&reported_resources),
        );

        assert_eq!(resources.dir.as_deref(), Some(reported_resources.as_path()));
        assert_eq!(
            resources.source,
            Some(GhosttyRuntimeResourceSource::GhosttyReported)
        );
        assert!(resources.present);
        assert!(resources.missing.is_empty());
    }

    #[test]
    fn ghostty_runtime_resource_probe_prefers_library_relative_over_env() {
        let dir = tempfile::tempdir().expect("tempdir");
        let root = dir.path();
        let lib_dir = root.join("install/lib");
        let library = lib_dir.join("libghostty-internal.so");
        let installed_share = root.join("install/share");
        let installed_resources = installed_share.join("ghostty");
        let env_share = root.join("env/share");
        let env_resources = env_share.join("ghostty");
        std::fs::create_dir_all(&lib_dir).expect("lib dir");
        std::fs::create_dir_all(installed_share.join("terminfo/g"))
            .expect("installed terminfo dir");
        std::fs::create_dir_all(installed_resources.join("shell-integration"))
            .expect("installed shell dir");
        std::fs::create_dir_all(env_share.join("terminfo/g")).expect("env terminfo dir");
        std::fs::create_dir_all(env_resources.join("shell-integration")).expect("env shell dir");
        std::fs::write(&library, "").expect("ghostty so");
        std::fs::write(installed_share.join("terminfo/g/ghostty"), "").expect("installed terminfo");
        std::fs::write(env_share.join("terminfo/g/ghostty"), "").expect("env terminfo");

        let resources =
            ghostty_runtime_resources_with_env(None, Some(&library), Some(&env_resources), None);

        assert_eq!(
            resources.dir.as_deref(),
            Some(installed_resources.as_path())
        );
        assert_eq!(
            resources.source,
            Some(GhosttyRuntimeResourceSource::LibraryRelative)
        );
        assert!(resources.present);
        assert!(resources.missing.is_empty());
    }

    #[test]
    fn parse_nm_symbols_uses_last_column_names() {
        let symbols = parse_nm_symbols(
            "0000000000001110 T ghostty_terminal_new\n\
             0000000000001120 T ghostty_formatter_format_alloc\n",
        );
        assert!(symbols.contains("ghostty_terminal_new"));
        assert!(symbols.contains("ghostty_formatter_format_alloc"));
    }

    #[test]
    fn ghostty_required_symbols_cover_linux_embedding_loader() {
        let symbols = REQUIRED_GHOSTTY_SYMBOLS
            .iter()
            .map(|symbol| (*symbol).to_string())
            .collect::<HashSet<_>>();
        assert_eq!(
            symbols.len(),
            REQUIRED_GHOSTTY_SYMBOLS.len(),
            "required Ghostty embedding symbols must be unique"
        );
        assert!(missing_required_ghostty_symbols(&symbols).is_empty());
        assert!(
            symbols.contains("ghostty_surface_set_occlusion"),
            "Ghostty embedding diagnostics must require the visibility compatibility alias"
        );
        assert!(
            symbols.contains("ghostty_app_must_draw_from_app_thread"),
            "Ghostty embedding diagnostics must require the Linux app-thread draw contract"
        );
        assert!(
            symbols.contains("ghostty_embedding_info_query"),
            "Ghostty embedding diagnostics must require the loaded library ABI info query"
        );
        assert!(
            symbols.contains("ghostty_embedding_info"),
            "Ghostty embedding diagnostics must require the direct library ABI info self-report"
        );
        assert!(
            symbols.contains("ghostty_config_get"),
            "Ghostty embedding diagnostics must require every symbol loaded by the Linux Ghostty loader"
        );
        for darwin_symbol in DARWIN_ONLY_GHOSTTY_SYMBOLS {
            assert!(
                !symbols.contains(*darwin_symbol),
                "Linux embedding diagnostics must not require Darwin-only symbol {darwin_symbol}"
            );
        }
        for internal_prefix in INTERNAL_GHOSTTY_SYMBOL_PREFIXES {
            assert!(
                !symbols.iter().any(|symbol| symbol.starts_with(internal_prefix)),
                "Linux embedding diagnostics must not require internal helper symbols with prefix {internal_prefix}"
            );
        }
        for optional in OPTIONAL_GHOSTTY_EXPORT_SYMBOLS {
            assert!(
                !symbols.contains(*optional),
                "optional Ghostty export {optional} should be listed only once"
            );
        }
        let allowed = allowed_ghostty_export_symbols();
        assert_eq!(
            allowed.len(),
            REQUIRED_GHOSTTY_SYMBOLS.len() + OPTIONAL_GHOSTTY_EXPORT_SYMBOLS.len(),
            "allowed Ghostty export symbols must be unique"
        );

        for required in REQUIRED_GHOSTTY_SYMBOLS {
            let mut missing_symbols = symbols.clone();
            assert!(
                missing_symbols.remove(*required),
                "test fixture did not contain required symbol {required}"
            );
            let missing = missing_required_ghostty_symbols(&missing_symbols);
            assert_eq!(
                missing,
                vec![(*required).to_string()],
                "diagnostics should report exactly the absent Ghostty embedding symbol"
            );
        }
    }

    #[test]
    fn ghostty_darwin_only_symbols_are_rejected_for_linux_embedding() {
        let symbols = HashSet::from([
            "ghostty_init".to_string(),
            "ghostty_surface_quicklook_word".to_string(),
            "ghostty_set_window_background_blur".to_string(),
        ]);

        assert_eq!(
            present_darwin_only_ghostty_symbols(&symbols),
            vec![
                "ghostty_surface_quicklook_word".to_string(),
                "ghostty_set_window_background_blur".to_string()
            ]
        );
    }

    #[test]
    fn ghostty_internal_symbols_are_rejected_for_linux_embedding() {
        let symbols = HashSet::from([
            "ghostty_init".to_string(),
            "ghostty_simd_codepoint_width".to_string(),
            "ghostty_surface_draw".to_string(),
            "ghostty_simd_base64_decode".to_string(),
        ]);

        assert_eq!(
            present_internal_ghostty_symbols(&symbols),
            vec![
                "ghostty_simd_base64_decode".to_string(),
                "ghostty_simd_codepoint_width".to_string(),
            ]
        );
    }

    #[test]
    fn ghostty_unexpected_export_symbols_are_rejected_for_linux_embedding() {
        let symbols = REQUIRED_GHOSTTY_SYMBOLS
            .iter()
            .chain(OPTIONAL_GHOSTTY_EXPORT_SYMBOLS.iter())
            .map(|symbol| (*symbol).to_string())
            .chain([
                "FT_Init_FreeType".to_string(),
                "hb_buffer_create".to_string(),
                "xmlReadMemory".to_string(),
            ])
            .collect::<HashSet<_>>();

        assert_eq!(
            unexpected_ghostty_export_symbols(&symbols),
            UnexpectedGhosttyExportSymbols {
                sample: vec![
                    "FT_Init_FreeType".to_string(),
                    "hb_buffer_create".to_string(),
                    "xmlReadMemory".to_string(),
                ],
                total: 3,
            }
        );
    }

    #[test]
    fn ghostty_unexpected_export_symbol_sample_is_bounded() {
        let symbols = (0..(MAX_UNEXPECTED_GHOSTTY_EXPORT_SYMBOLS + 2))
            .map(|index| format!("leaked_symbol_{index:02}"))
            .collect::<HashSet<_>>();

        let unexpected = unexpected_ghostty_export_symbols(&symbols);

        assert_eq!(unexpected.total, MAX_UNEXPECTED_GHOSTTY_EXPORT_SYMBOLS + 2);
        assert_eq!(
            unexpected.sample.len(),
            MAX_UNEXPECTED_GHOSTTY_EXPORT_SYMBOLS
        );
        assert_eq!(
            unexpected.sample.first().map(String::as_str),
            Some("leaked_symbol_00")
        );
    }

    #[test]
    fn ghostty_header_linux_platform_value_parses_implicit_c_enum() {
        let header = "\
typedef enum {
  GHOSTTY_PLATFORM_INVALID,
  GHOSTTY_PLATFORM_MACOS,
  GHOSTTY_PLATFORM_IOS,
  GHOSTTY_PLATFORM_LINUX,
} ghostty_platform_e;
";
        assert_eq!(
            ghostty_header_linux_platform_value(header),
            Some(crate::ghostty_embed::GHOSTTY_PLATFORM_LINUX)
        );

        let shifted_header = "\
typedef enum {
  GHOSTTY_PLATFORM_INVALID = 2,
  GHOSTTY_PLATFORM_MACOS,
  GHOSTTY_PLATFORM_IOS,
  GHOSTTY_PLATFORM_LINUX,
} ghostty_platform_e;
";
        assert_eq!(ghostty_header_linux_platform_value(shifted_header), Some(5));
        assert_eq!(
            ghostty_header_linux_platform_value(
                "typedef enum { GHOSTTY_PLATFORM_LINUX } other_enum_e;"
            ),
            None
        );
    }

    #[test]
    fn ghostty_header_embedding_abi_version_parses_c_define() {
        assert_eq!(
            ghostty_header_embedding_abi_version(
                "\n#define GHOSTTY_EMBEDDING_ABI_VERSION 15\n#define GHOSTTY_PLATFORM_LINUX 3\n"
            ),
            Some(crate::ghostty_embed::GHOSTTY_EMBEDDING_ABI_VERSION)
        );
        assert_eq!(
            ghostty_header_embedding_abi_version(
                "#define GHOSTTY_EMBEDDING_ABI_VERSION 15 /* local Linux embedding ABI */"
            ),
            Some(crate::ghostty_embed::GHOSTTY_EMBEDDING_ABI_VERSION)
        );
        assert_eq!(
            ghostty_header_embedding_abi_version("#define GHOSTTY_SURFACE_MAX_ENV_VARS 4096"),
            None
        );
        assert_eq!(
            ghostty_header_embedding_abi_version("#define GHOSTTY_EMBEDDING_ABI_VERSION many"),
            None
        );
    }

    #[test]
    fn ghostty_header_init_argv_const_rejects_mutable_argv() {
        assert!(ghostty_header_init_argv_const(
            "GHOSTTY_API int ghostty_init(uintptr_t, const char * const *);"
        ));
        assert!(ghostty_header_init_argv_const(
            "GHOSTTY_API int ghostty_init(uintptr_t,\n  const char* const*);"
        ));
        assert!(!ghostty_header_init_argv_const(
            "GHOSTTY_API int ghostty_init(uintptr_t, char**);"
        ));
        assert!(!ghostty_header_init_argv_const(
            "GHOSTTY_API int ghostty_init(uintptr_t, const char**);"
        ));
        assert!(!ghostty_header_init_argv_const(
            "GHOSTTY_API int ghostty_init(uintptr_t, char* const*);"
        ));
    }

    #[test]
    fn ghostty_header_ipc_new_window_arguments_const_rejects_mutable_argv() {
        assert!(ghostty_header_ipc_new_window_arguments_const(
            "typedef struct { const char * const *arguments; } ghostty_ipc_action_new_window_s;"
        ));
        assert!(ghostty_header_ipc_new_window_arguments_const(
            "typedef struct {\n  const char* const* arguments;\n} ghostty_ipc_action_new_window_s;"
        ));
        assert!(!ghostty_header_ipc_new_window_arguments_const(
            "typedef struct { const char **arguments; } ghostty_ipc_action_new_window_s;"
        ));
        assert!(!ghostty_header_ipc_new_window_arguments_const(
            "typedef struct { char * const *arguments; } ghostty_ipc_action_new_window_s;"
        ));
    }

    #[test]
    fn ghostty_header_surface_env_vars_const_rejects_mutable_array() {
        assert!(ghostty_header_surface_env_vars_const(
            "typedef struct { const ghostty_env_var_s *env_vars; } ghostty_surface_config_s;"
        ));
        assert!(ghostty_header_surface_env_vars_const(
            "typedef struct {\n  const ghostty_env_var_s* env_vars;\n} ghostty_surface_config_s;"
        ));
        assert!(!ghostty_header_surface_env_vars_const(
            "typedef struct { ghostty_env_var_s *env_vars; } ghostty_surface_config_s;"
        ));
        assert!(!ghostty_header_surface_env_vars_const(
            "typedef struct { ghostty_env_var_s* env_vars; } ghostty_surface_config_s;"
        ));
    }

    #[test]
    fn ghostty_header_redraw_surface_callback_requires_runtime_config_field() {
        assert!(ghostty_header_has_redraw_surface_callback(
            "typedef struct { ghostty_runtime_redraw_surface_cb redraw_surface_cb; } ghostty_runtime_config_s;"
        ));
        assert!(ghostty_header_has_redraw_surface_callback(
            "typedef struct {\n  ghostty_runtime_redraw_surface_cb\n    redraw_surface_cb;\n} ghostty_runtime_config_s;"
        ));
        assert!(!ghostty_header_has_redraw_surface_callback(
            "typedef struct { ghostty_runtime_wakeup_cb wakeup_cb; } ghostty_runtime_config_s;"
        ));
        assert!(!ghostty_header_has_redraw_surface_callback(
            "typedef struct { void* redraw_surface_cb; } ghostty_runtime_config_s;"
        ));
    }

    #[test]
    fn ghostty_header_env_var_limit_parses_c_define() {
        assert_eq!(
            ghostty_header_env_var_limit(
                "\n#define GHOSTTY_PLATFORM_LINUX 3\n#define GHOSTTY_SURFACE_MAX_ENV_VARS 4096\n"
            ),
            Some(crate::ghostty_embed::GHOSTTY_SURFACE_MAX_ENV_VARS)
        );
        assert_eq!(
            ghostty_header_env_var_limit(
                "#define GHOSTTY_SURFACE_MAX_ENV_VARS 4096 /* bounded by embedded runtime */"
            ),
            Some(crate::ghostty_embed::GHOSTTY_SURFACE_MAX_ENV_VARS)
        );
        assert_eq!(ghostty_header_env_var_limit("#define OTHER 4096"), None);
        assert_eq!(
            ghostty_header_env_var_limit("#define GHOSTTY_SURFACE_MAX_ENV_VARS many"),
            None
        );
    }

    #[test]
    fn ghostty_header_keycode_markers_parse_c_defines() {
        let header = "\
#define GHOSTTY_INPUT_KEYCODE_NATIVE_MASK UINT32_C(0x7fffffff)
#define GHOSTTY_INPUT_KEYCODE_PHYSICAL_KEY_FLAG UINT32_C(0x80000000)
";
        assert_eq!(
            ghostty_header_keycode_native_mask(header),
            Some(crate::ghostty_embed::GHOSTTY_INPUT_KEYCODE_NATIVE_MASK)
        );
        assert_eq!(
            ghostty_header_keycode_physical_key_flag(header),
            Some(crate::ghostty_embed::GHOSTTY_INPUT_KEYCODE_PHYSICAL_KEY_FLAG)
        );
        assert_eq!(
            ghostty_header_keycode_native_mask(
                "#define GHOSTTY_INPUT_KEYCODE_NATIVE_MASK 2147483647U"
            ),
            Some(crate::ghostty_embed::GHOSTTY_INPUT_KEYCODE_NATIVE_MASK)
        );
        assert_eq!(
            ghostty_header_keycode_physical_key_flag(
                "#define GHOSTTY_INPUT_KEYCODE_PHYSICAL_KEY_FLAG 0x80000000UL"
            ),
            Some(crate::ghostty_embed::GHOSTTY_INPUT_KEYCODE_PHYSICAL_KEY_FLAG)
        );
        assert_eq!(
            ghostty_header_keycode_native_mask(
                "#define GHOSTTY_INPUT_KEYCODE_NATIVE_MASKED UINT32_C(0x7fffffff)"
            ),
            None
        );
        assert_eq!(
            ghostty_header_keycode_physical_key_flag(
                "#define GHOSTTY_INPUT_KEYCODE_PHYSICAL_KEY(key) ((uint32_t)(key))"
            ),
            None
        );
    }

    #[test]
    fn ghostty_embedding_support_requires_verified_complete_symbols() {
        assert!(ghostty_embedding_supported_with_valid_library_info(
            true,
            true,
            true,
            true,
            true,
            true,
            &[],
            true
        ));
        assert!(!ghostty_embedding_supported_with_valid_library_info(
            false,
            true,
            true,
            true,
            true,
            true,
            &[],
            true
        ));
        assert!(!ghostty_embedding_supported_with_valid_library_info(
            true,
            false,
            true,
            true,
            true,
            true,
            &[],
            true
        ));
        assert!(!ghostty_embedding_supported_with_valid_library_info(
            true,
            true,
            false,
            true,
            true,
            true,
            &[],
            true
        ));
        assert!(!ghostty_embedding_supported_with_valid_library_info(
            true,
            true,
            true,
            false,
            true,
            true,
            &[],
            true
        ));
        assert!(!ghostty_embedding_supported_with_valid_library_info(
            true,
            true,
            true,
            true,
            false,
            true,
            &[],
            true
        ));
        assert!(!ghostty_embedding_supported_with_valid_library_info(
            true,
            true,
            true,
            true,
            true,
            false,
            &[],
            true
        ));
        assert!(!ghostty_embedding_supported_with_valid_library_info(
            true,
            true,
            true,
            true,
            true,
            true,
            &[],
            false
        ));
        assert!(!ghostty_embedding_supported_with_valid_library_info(
            true,
            true,
            true,
            true,
            true,
            true,
            &["ghostty_surface_display_realized".to_string()],
            true
        ));
        assert!(!ghostty_embedding_supported_with_darwin_symbols_hidden(
            false
        ));
        assert!(!ghostty_embedding_supported_with_internal_symbols_hidden(
            false
        ));
        assert!(!ghostty_embedding_supported_with_unexpected_export_symbols_hidden(false));
        assert!(!ghostty_embedding_supported_with_library_info_report(
            false, true
        ));
        assert!(!ghostty_embedding_supported_with_library_info_report(
            true, false
        ));
        assert!(!ghostty_embedding_supported_with_header_keycode_contract(
            false, true
        ));
        assert!(!ghostty_embedding_supported_with_header_keycode_contract(
            true, false
        ));
        assert!(!ghostty_embedding_supported_with_header_redraw_surface_callback(false));
        assert!(!ghostty_embedding_supported_with_header_surface_env_vars_const(false));
        assert!(!ghostty_embedding_supported_with_header_init_argv_const(
            false
        ));
        assert!(!ghostty_embedding_supported_with_header_ipc_new_window_arguments_const(false));
        assert!(!ghostty_embedding_supported_with_header_surface_metadata_returns_string(false));
        assert!(!ghostty_embedding_supported_with_contract_checks(
            false, true, true, true, true, true, true, true, true
        ));
        assert!(!ghostty_embedding_supported_with_contract_checks(
            true, false, true, true, true, true, true, true, true
        ));
        assert!(!ghostty_embedding_supported_with_contract_checks(
            true, true, false, true, true, true, true, true, true
        ));
        assert!(!ghostty_embedding_supported_with_renderer_backend(false));
        assert!(!ghostty_embedding_supported_with_contract_checks(
            true, true, true, false, true, true, true, true, true
        ));
        assert!(!ghostty_embedding_supported_with_contract_checks(
            true, true, true, true, false, true, true, true, true
        ));
        assert!(!ghostty_embedding_supported_with_contract_checks(
            true, true, true, true, true, false, true, true, true
        ));
        assert!(!ghostty_embedding_supported_with_contract_checks(
            true, true, true, true, true, true, false, true, true
        ));
        assert!(!ghostty_embedding_supported_with_contract_checks(
            true, true, true, true, true, true, true, false, true
        ));
        assert!(!ghostty_embedding_supported_with_contract_checks(
            true, true, true, true, true, true, true, true, false
        ));
    }

    #[test]
    fn full_ghostty_backend_requires_embedding_and_runtime_resources() {
        assert!(ghostty_backend_available(true, true));
        assert!(!ghostty_backend_available(true, false));
        assert!(!ghostty_backend_available(false, true));
        assert!(!ghostty_backend_available(false, false));
    }

    #[test]
    fn ghostty_embedding_status_reports_first_actionable_state() {
        assert_eq!(
            ghostty_embedding_status_with_valid_library_info(
                false,
                false,
                false,
                false,
                false,
                false,
                false,
                &[],
                false
            ),
            "missing_header"
        );
        assert_eq!(
            ghostty_embedding_status_with_valid_library_info(
                true,
                false,
                false,
                false,
                false,
                false,
                false,
                &[],
                false
            ),
            "missing_linux_platform"
        );
        assert_eq!(
            ghostty_embedding_status_with_valid_library_info(
                true,
                true,
                false,
                false,
                false,
                false,
                false,
                &[],
                false
            ),
            "linux_platform_value_mismatch"
        );
        assert_eq!(
            ghostty_embedding_status_with_valid_library_info(
                true,
                true,
                true,
                false,
                false,
                false,
                false,
                &[],
                false
            ),
            "env_var_limit_mismatch"
        );
        assert_eq!(
            ghostty_embedding_status_with_header_keycode_contract(false, true),
            "input_keycode_native_mask_mismatch"
        );
        assert_eq!(
            ghostty_embedding_status_with_header_keycode_contract(true, false),
            "input_keycode_physical_key_flag_mismatch"
        );
        assert_eq!(
            ghostty_embedding_status_with_valid_library_info(
                true,
                true,
                true,
                true,
                false,
                false,
                false,
                &[],
                false
            ),
            "missing_app_thread_draw_contract"
        );
        assert_eq!(
            ghostty_embedding_status_with_header_redraw_surface_callback(false),
            "missing_redraw_surface_callback"
        );
        assert_eq!(
            ghostty_embedding_status_with_header_surface_env_vars_const(false),
            "surface_env_vars_not_const"
        );
        assert_eq!(
            ghostty_embedding_status_with_header_init_argv_const(false),
            "init_argv_not_const"
        );
        assert_eq!(
            ghostty_embedding_status_with_header_ipc_new_window_arguments_const(false),
            "ipc_new_window_arguments_not_const"
        );
        assert_eq!(
            ghostty_embedding_status_with_header_surface_metadata_returns_string(false),
            "surface_metadata_not_string"
        );
        assert_eq!(
            ghostty_embedding_status_with_valid_library_info(
                true,
                true,
                true,
                true,
                true,
                false,
                false,
                &[],
                false
            ),
            "missing_library"
        );
        assert_eq!(
            ghostty_embedding_status_with_valid_library_info(
                true,
                true,
                true,
                true,
                true,
                true,
                false,
                &[],
                false
            ),
            "load_error"
        );
        assert_eq!(
            ghostty_embedding_status_with_valid_library_info(
                true,
                true,
                true,
                true,
                true,
                true,
                false,
                &[],
                true
            ),
            "symbols_unverified"
        );
        assert_eq!(
            ghostty_embedding_status_with_valid_library_info(
                true,
                true,
                true,
                true,
                true,
                true,
                true,
                &["ghostty_surface_display_realized".to_string()],
                true
            ),
            "missing_symbols"
        );
        assert_eq!(
            ghostty_embedding_status_with_darwin_symbols_hidden(false),
            "darwin_symbols_exported"
        );
        assert_eq!(
            ghostty_embedding_status_with_internal_symbols_hidden(false),
            "internal_symbols_exported"
        );
        assert_eq!(
            ghostty_embedding_status_with_unexpected_export_symbols_hidden(false),
            "unexpected_export_symbols"
        );
        assert_eq!(
            ghostty_embedding_status_with_args(StatusArgs {
                support: SupportArgs {
                    library_info_query_succeeded: false,
                    ..SupportArgs::default()
                },
                ..StatusArgs::default()
            }),
            "embedding_info_query_error"
        );
        assert_eq!(
            ghostty_embedding_status_with_args(StatusArgs {
                support: SupportArgs {
                    library_info_direct_matches_query: false,
                    ..SupportArgs::default()
                },
                ..StatusArgs::default()
            }),
            "embedding_info_direct_mismatch"
        );
        assert_eq!(
            ghostty_embedding_status_with_contract_checks(
                false, true, true, true, true, true, true, true, true
            ),
            "embedding_header_abi_version_mismatch"
        );
        assert_eq!(
            ghostty_embedding_status_with_contract_checks(
                true, false, true, true, true, true, true, true, true
            ),
            "embedding_abi_version_mismatch"
        );
        assert_eq!(
            ghostty_embedding_status_with_contract_checks(
                true, true, false, true, true, true, true, true, true
            ),
            "embedding_platform_mismatch"
        );
        assert_eq!(
            ghostty_embedding_status_with_renderer_backend(false),
            "embedding_renderer_backend_mismatch"
        );
        assert_eq!(
            ghostty_embedding_status_with_contract_checks(
                true, true, true, false, true, true, true, true, true
            ),
            "embedding_env_var_limit_mismatch"
        );
        assert_eq!(
            ghostty_embedding_status_with_contract_checks(
                true, true, true, true, false, true, true, true, true
            ),
            "embedding_layout_size_mismatch"
        );
        assert_eq!(
            ghostty_embedding_status_with_contract_checks(
                true, true, true, true, true, false, true, true, true
            ),
            "embedding_layout_alignment_mismatch"
        );
        assert_eq!(
            ghostty_embedding_status_with_contract_checks(
                true, true, true, true, true, true, false, true, true
            ),
            "embedding_layout_fingerprint_mismatch"
        );
        assert_eq!(
            ghostty_embedding_status_with_contract_checks(
                true, true, true, true, true, true, true, false, true
            ),
            "embedding_constants_fingerprint_mismatch"
        );
        assert_eq!(
            ghostty_embedding_status_with_contract_checks(
                true, true, true, true, true, true, true, true, false
            ),
            "embedding_linux_platform_unsupported"
        );
        assert_eq!(
            ghostty_embedding_status_with_valid_library_info(
                true,
                true,
                true,
                true,
                true,
                true,
                true,
                &[],
                true
            ),
            "available"
        );
    }

    #[test]
    fn ghostty_vt_required_symbols_cover_runtime_snapshot_loader() {
        let mut symbols = REQUIRED_GHOSTTY_VT_SYMBOLS
            .iter()
            .map(|symbol| (*symbol).to_string())
            .collect::<HashSet<_>>();
        assert!(missing_required_ghostty_vt_symbols(&symbols).is_empty());

        symbols.remove("ghostty_render_state_row_cells_get");
        let missing = missing_required_ghostty_vt_symbols(&symbols);
        assert_eq!(
            missing,
            vec!["ghostty_render_state_row_cells_get".to_string()]
        );
    }

    #[test]
    fn ghostty_vt_support_requires_verified_complete_symbols() {
        assert!(ghostty_vt_supported(true, true, true, &[]));
        assert!(!ghostty_vt_supported(false, true, true, &[]));
        assert!(!ghostty_vt_supported(true, false, true, &[]));
        assert!(!ghostty_vt_supported(true, true, false, &[]));
        assert!(!ghostty_vt_supported(
            true,
            true,
            true,
            &["ghostty_render_state_row_cells_get".to_string()]
        ));
    }
}
