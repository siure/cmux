use serde::Serialize;
use serde_json::{json, Map, Value};
use std::collections::{HashMap, HashSet};
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

const RELEASE_BUNDLE_ID: &str = "com.cmuxterm.app";
const CMUX_THEMES_BLOCK_START: &str = "# cmux themes start";
const CMUX_THEMES_BLOCK_END: &str = "# cmux themes end";
const SIDEBAR_FONT_SIZE_KEY: &str = "sidebar-font-size";
const SURFACE_TAB_BAR_FONT_SIZE_KEY: &str = "surface-tab-bar-font-size";
const SETTINGS_DOCS_URL: &str = "https://cmux.com/docs/configuration#cmux-json";
const SETTINGS_SCHEMA_URL: &str =
    "https://raw.githubusercontent.com/manaflow-ai/cmux/main/web/data/cmux.schema.json";
pub const MACOS_MIGRATION_FALLBACK_LABEL: &str = "macOS migration fallback";
pub const WORKSPACE_COLOR_DEFAULT_PALETTE: &[(&str, &str)] = &[
    ("Red", "#C0392B"),
    ("Crimson", "#922B21"),
    ("Orange", "#A04000"),
    ("Amber", "#7D6608"),
    ("Olive", "#4A5C18"),
    ("Green", "#196F3D"),
    ("Teal", "#006B6B"),
    ("Aqua", "#0E6B8C"),
    ("Blue", "#1565C0"),
    ("Navy", "#1A5276"),
    ("Indigo", "#283593"),
    ("Purple", "#6A1B9A"),
    ("Magenta", "#AD1457"),
    ("Rose", "#880E4F"),
    ("Brown", "#7B3F00"),
    ("Charcoal", "#3E4B5E"),
];

#[derive(Debug, Clone, Serialize)]
pub struct ConfigSnapshot {
    pub sources: Vec<&'static str>,
    pub cmux: ConfigSourceSnapshot,
    pub ghostty: ConfigSourceSnapshot,
    pub synced: ConfigSourceSnapshot,
    pub load_paths: Vec<String>,
    pub editor_paths: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ConfigSourceSnapshot {
    pub path: String,
    pub display_paths: Vec<String>,
    pub contents: String,
    pub is_editable: bool,
    pub has_backing_file: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct ThemeSelection {
    pub raw_value: Option<String>,
    pub light: Option<String>,
    pub dark: Option<String>,
    pub source_path: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ThemeListEntry {
    pub name: String,
    pub current_light: bool,
    pub current_dark: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct ThemeListPayload {
    pub themes: Vec<ThemeListEntry>,
    pub current: ThemeSelection,
    pub config_path: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct ThemeSetPayload {
    pub ok: bool,
    pub light: Option<String>,
    pub dark: Option<String>,
    pub raw_value: String,
    pub config_path: String,
    pub reload_requested: bool,
    pub reload_target_bundle_id: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct ThemeClearPayload {
    pub ok: bool,
    pub cleared: bool,
    pub config_path: String,
    pub reload_requested: bool,
    pub reload_target_bundle_id: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct FontSizeGetPayload {
    pub key: String,
    pub value: f64,
    pub formatted: String,
    pub path: String,
    pub configured: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub configured_value: Option<f64>,
}

#[derive(Debug, Clone, Serialize)]
pub struct FontSizeSetPayload {
    pub ok: bool,
    pub key: String,
    pub value: f64,
    pub formatted: String,
    pub path: String,
    pub clamped: bool,
    pub reload: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reload_message: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct WindowDefaultDisplayPayload {
    pub ok: bool,
    pub display: Option<String>,
    pub configured: bool,
    pub cleared: bool,
    pub path: String,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct CanvasSettingsPayload {
    pub pane_gap: f64,
    pub snapping_enabled: bool,
    pub snap_threshold: f64,
    pub min_pane_width: f64,
    pub min_pane_height: f64,
    pub path: String,
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
pub struct TerminalTextBoxSettings {
    pub show_on_new_terminals: bool,
    pub focus_on_new_terminals: bool,
    pub max_lines: u32,
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
pub enum WorkspacePlacement {
    Top,
    AfterCurrent,
    End,
}

impl WorkspacePlacement {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Top => "top",
            Self::AfterCurrent => "afterCurrent",
            Self::End => "end",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value.trim() {
            "top" => Some(Self::Top),
            "afterCurrent" | "after-current" | "after_current" => Some(Self::AfterCurrent),
            "end" => Some(Self::End),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
pub enum ConfirmQuitPolicy {
    Always,
    DirtyOnly,
    Never,
}

impl ConfirmQuitPolicy {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Always => "always",
            Self::DirtyOnly => "dirty-only",
            Self::Never => "never",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value.trim() {
            "always" => Some(Self::Always),
            "dirty-only" | "dirtyOnly" | "dirty_only" => Some(Self::DirtyOnly),
            "never" => Some(Self::Never),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
pub enum DiffViewerLayout {
    Unified,
    Split,
}

impl DiffViewerLayout {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Unified => "unified",
            Self::Split => "split",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value.trim() {
            "unified" => Some(Self::Unified),
            "split" => Some(Self::Split),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
pub struct AppWorkspaceSettings {
    pub new_workspace_placement: WorkspacePlacement,
    pub workspace_inherit_working_directory: bool,
    pub keep_workspace_open_when_closing_last_surface: bool,
    pub confirm_quit: ConfirmQuitPolicy,
    pub warn_before_closing_tab: bool,
    pub warn_before_closing_tab_x_button: bool,
    pub hide_tab_close_button: bool,
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
pub struct TerminalInteractionSettings {
    pub show_scroll_bar: bool,
    pub copy_on_select: bool,
    pub auto_resume_agent_sessions: bool,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct BrowserSearchSettings {
    pub engine: String,
    pub custom_name: String,
    pub custom_url_template: String,
    pub show_search_suggestions: bool,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct BetaFeatureSettings {
    pub right_sidebar_feed: bool,
    pub right_sidebar_dock: bool,
    pub extensions: bool,
    pub custom_sidebars: bool,
    pub remote_tmux: bool,
    pub path: String,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct WorkspaceColorSettings {
    pub indicator_style: String,
    pub selection_color: Option<String>,
    pub notification_badge_color: Option<String>,
    pub colors: Vec<(String, String)>,
    pub path: String,
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
pub enum SidebarBranchLayout {
    Vertical,
    Inline,
}

impl SidebarBranchLayout {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Vertical => "vertical",
            Self::Inline => "inline",
        }
    }

    fn parse(value: &str) -> Option<Self> {
        match value.trim() {
            "vertical" => Some(Self::Vertical),
            "inline" => Some(Self::Inline),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct SidebarSettings {
    pub match_terminal_background: bool,
    pub hide_all_details: bool,
    pub wrap_workspace_titles: bool,
    pub show_workspace_description: bool,
    pub branch_layout: SidebarBranchLayout,
    pub stack_branch_directory: bool,
    pub path_last_segment_only: bool,
    pub show_notification_message: bool,
    pub show_branch_directory: bool,
    pub show_pull_requests: bool,
    pub watch_git_status: bool,
    pub make_pull_requests_clickable: bool,
    pub open_pull_request_links_in_cmux_browser: bool,
    pub open_port_links_in_cmux_browser: bool,
    pub show_ssh: bool,
    pub show_ports: bool,
    pub show_log: bool,
    pub show_progress: bool,
    pub show_custom_metadata: bool,
    pub right_max_width: Option<f64>,
    pub path: String,
}

impl Default for SidebarSettings {
    fn default() -> Self {
        Self {
            match_terminal_background: false,
            hide_all_details: false,
            wrap_workspace_titles: false,
            show_workspace_description: true,
            branch_layout: SidebarBranchLayout::Vertical,
            stack_branch_directory: false,
            path_last_segment_only: false,
            show_notification_message: true,
            show_branch_directory: true,
            show_pull_requests: true,
            watch_git_status: true,
            make_pull_requests_clickable: true,
            open_pull_request_links_in_cmux_browser: true,
            open_port_links_in_cmux_browser: true,
            show_ssh: true,
            show_ports: true,
            show_log: true,
            show_progress: true,
            show_custom_metadata: true,
            right_max_width: None,
            path: String::new(),
        }
    }
}

impl Default for BrowserSearchSettings {
    fn default() -> Self {
        Self {
            engine: "google".to_string(),
            custom_name: String::new(),
            custom_url_template: "https://www.google.com/search?q={query}".to_string(),
            show_search_suggestions: true,
        }
    }
}

impl Default for BetaFeatureSettings {
    fn default() -> Self {
        Self {
            right_sidebar_feed: false,
            right_sidebar_dock: false,
            extensions: false,
            custom_sidebars: true,
            remote_tmux: false,
            path: String::new(),
        }
    }
}

impl Default for TerminalInteractionSettings {
    fn default() -> Self {
        Self {
            show_scroll_bar: true,
            copy_on_select: false,
            auto_resume_agent_sessions: true,
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
pub enum CustomSidebarRendererMode {
    InProcess,
    Remote,
}

impl CustomSidebarRendererMode {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::InProcess => "inProcess",
            Self::Remote => "remote",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value.trim() {
            "inProcess" | "in-process" | "in_process" => Some(Self::InProcess),
            "remote" => Some(Self::Remote),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ShortcutBinding {
    Unbound,
    Single(String),
    Chord(String, String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ShortcutBindingUpdate {
    Set(Vec<String>),
    Unbind,
    Reset,
}

#[derive(Debug, Clone, Serialize)]
pub struct ConfigDoctorReport {
    pub ok: bool,
    pub error_count: usize,
    pub findings: Vec<ConfigDoctorFinding>,
    pub reload_command: String,
    pub docs_url: String,
    pub schema_url: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct ConfigDoctorFinding {
    pub label: String,
    pub display_path: String,
    pub path: String,
    pub status: String,
    pub ok: bool,
    pub keys: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bytes: Option<usize>,
}

#[derive(Debug, Clone, Serialize)]
pub struct SettingsDocsPayload {
    pub topic: String,
    pub title: String,
    pub summary: String,
    pub docs_url: String,
    pub schema_url: String,
    pub settings_files: SettingsFilesPayload,
    pub primary: String,
    pub legacy: String,
    pub fallback: String,
    pub ghostty_config: GhosttyConfigPayload,
    pub backup: String,
    pub reload_command: String,
    pub reload_scope: String,
    pub resources: Vec<String>,
    pub commands: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct SettingsFilesPayload {
    pub primary: String,
    pub legacy: String,
    pub fallback: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct GhosttyConfigPayload {
    pub path: String,
    pub note: String,
}

#[derive(Debug, Clone, Default)]
pub struct WorkspaceGroupConfig {
    pub color: Option<String>,
    pub icon_symbol: Option<String>,
    pub new_workspace_placement: Option<String>,
    pub context_menu_items: Vec<Value>,
}

#[derive(Debug, Clone)]
struct ConfigEnvironment {
    home: PathBuf,
    xdg_config_home: PathBuf,
    xdg_cache_home: PathBuf,
    bundle_id: String,
}

#[derive(Debug, Clone)]
struct ParsedEntry {
    key: String,
    value: String,
    source: PathBuf,
    line_number: usize,
}

struct ConfigDoctorTarget {
    label: String,
    display_path: String,
    path: PathBuf,
    missing_is_error: bool,
}

pub fn snapshot() -> ConfigSnapshot {
    let env = ConfigEnvironment::live();
    let cmux_path = cmux_config_path(&env);
    let ghostty_path = ghostty_config_path(&env);
    let load_paths = cmux_load_paths(&env, &cmux_path);
    let synced_path = env.xdg_cache_home.join("cmux/config.synced-preview");
    let synced_contents = render_synced_preview(
        regular_file(&ghostty_path).then_some(ghostty_path.as_path()),
        &load_paths,
        &env,
    );
    let _ = materialize_synced_preview(&synced_path, &synced_contents);
    let editor_paths = editor_paths(&env, &cmux_path, &ghostty_path, &load_paths);

    ConfigSnapshot {
        sources: vec!["cmux", "synced"],
        cmux: source_snapshot(cmux_path, true, &env),
        ghostty: source_snapshot(ghostty_path, false, &env),
        synced: ConfigSourceSnapshot {
            display_paths: vec![abbreviated_path(&synced_path, &env)],
            path: synced_path.display().to_string(),
            contents: synced_contents,
            is_editable: false,
            has_backing_file: regular_file(&synced_path),
        },
        load_paths: load_paths
            .iter()
            .map(|path| path.display().to_string())
            .collect(),
        editor_paths: editor_paths
            .iter()
            .map(|path| path.display().to_string())
            .collect(),
    }
}

pub fn themes_list_payload() -> ThemeListPayload {
    let env = ConfigEnvironment::live();
    let current = current_theme_selection(&env);
    let themes = available_theme_names_with_env(&env)
        .into_iter()
        .map(|name| ThemeListEntry {
            current_light: current
                .light
                .as_ref()
                .is_some_and(|value| value.eq_ignore_ascii_case(&name)),
            current_dark: current
                .dark
                .as_ref()
                .is_some_and(|value| value.eq_ignore_ascii_case(&name)),
            name,
        })
        .collect();
    ThemeListPayload {
        themes,
        current,
        config_path: cmux_config_path(&env).display().to_string(),
    }
}

pub fn set_theme_override(
    light: Option<String>,
    dark: Option<String>,
) -> Result<ThemeSetPayload, String> {
    let env = ConfigEnvironment::live();
    let available = available_theme_names_with_env(&env);
    let current = current_theme_selection(&env);
    let resolved_light = match light {
        Some(value) => Some(validated_theme_name(&value, &available)?),
        None => current.light,
    };
    let resolved_dark = match dark {
        Some(value) => Some(validated_theme_name(&value, &available)?),
        None => current.dark,
    };
    let raw_value = encoded_theme_value(resolved_light.as_deref(), resolved_dark.as_deref())
        .ok_or_else(|| "themes set requires at least one theme".to_string())?;
    let config_path = write_managed_theme_override(&env, &raw_value)?;
    Ok(ThemeSetPayload {
        ok: true,
        light: resolved_light,
        dark: resolved_dark,
        raw_value,
        config_path: config_path.display().to_string(),
        reload_requested: false,
        reload_target_bundle_id: "linux".to_string(),
    })
}

pub fn clear_theme_override() -> Result<ThemeClearPayload, String> {
    let env = ConfigEnvironment::live();
    let config_path = clear_managed_theme_override(&env)?;
    Ok(ThemeClearPayload {
        ok: true,
        cleared: true,
        config_path: config_path.display().to_string(),
        reload_requested: false,
        reload_target_bundle_id: "linux".to_string(),
    })
}

pub fn canonical_font_size_key(raw: &str) -> Option<&'static str> {
    match raw.trim().to_ascii_lowercase().as_str() {
        SIDEBAR_FONT_SIZE_KEY => Some(SIDEBAR_FONT_SIZE_KEY),
        SURFACE_TAB_BAR_FONT_SIZE_KEY => Some(SURFACE_TAB_BAR_FONT_SIZE_KEY),
        _ => None,
    }
}

pub fn get_font_size(key: &str) -> Result<FontSizeGetPayload, String> {
    let descriptor =
        font_size_descriptor(key).ok_or_else(|| format!("Unknown font size key '{key}'"))?;
    let env = ConfigEnvironment::live();
    let cmux_path = cmux_config_path(&env);
    let value = effective_font_size_value(&env, &cmux_path, descriptor);
    let effective_value = value.unwrap_or(descriptor.default_value);
    Ok(FontSizeGetPayload {
        key: descriptor.key.to_string(),
        value: effective_value,
        formatted: format_font_size(effective_value),
        path: cmux_path.display().to_string(),
        configured: value.is_some(),
        configured_value: value,
    })
}

pub fn set_font_size(
    key: &str,
    raw_value: &str,
    reload: String,
    reload_message: Option<String>,
) -> Result<FontSizeSetPayload, String> {
    let descriptor =
        font_size_descriptor(key).ok_or_else(|| format!("Unknown font size key '{key}'"))?;
    let requested = raw_value
        .trim()
        .parse::<f64>()
        .map_err(|_| format!("{} requires a numeric point size", descriptor.key))?;
    if !requested.is_finite() {
        return Err(format!("{} requires a numeric point size", descriptor.key));
    }
    let value = clamp_font_size(requested, descriptor);
    let formatted = format_font_size(value);
    let env = ConfigEnvironment::live();
    let path = write_font_size_setting(&env, descriptor.key, &formatted)?;
    Ok(FontSizeSetPayload {
        ok: true,
        key: descriptor.key.to_string(),
        value,
        formatted,
        path: path.display().to_string(),
        clamped: value != requested,
        reload,
        reload_message,
    })
}

pub fn settings_docs_payload() -> SettingsDocsPayload {
    let env = ConfigEnvironment::live();
    let primary = abbreviated_path(&env.xdg_config_home.join("cmux/cmux.json"), &env);
    let legacy = abbreviated_path(&env.xdg_config_home.join("cmux/settings.json"), &env);
    let fallback = abbreviated_path(
        &env.app_support(RELEASE_BUNDLE_ID).join("settings.json"),
        &env,
    );
    let ghostty_config = abbreviated_path(&ghostty_config_path(&env), &env);
    SettingsDocsPayload {
        topic: "settings".to_string(),
        title: "Configuration docs".to_string(),
        summary: "cmux-owned settings, cmux.json locations, schema, and reload flow."
            .to_string(),
        docs_url: SETTINGS_DOCS_URL.to_string(),
        schema_url: SETTINGS_SCHEMA_URL.to_string(),
        settings_files: SettingsFilesPayload {
            primary: primary.clone(),
            legacy: legacy.clone(),
            fallback: fallback.clone(),
        },
        primary,
        legacy,
        fallback,
        ghostty_config: GhosttyConfigPayload {
            path: ghostty_config,
            note: "Not cmux-owned, but cmux reads it. Use for terminal transparency (background-opacity), blur, font, theme, etc.".to_string(),
        },
        backup: "Back up any existing cmux.json file to a timestamped .bak copy before editing so the user can revert.".to_string(),
        reload_command: "cmux reload-config".to_string(),
        reload_scope: "Reloads Ghostty config + cmux.json and refreshes terminals in place. No app restart needed.".to_string(),
        resources: vec![
            SETTINGS_SCHEMA_URL.to_string(),
            "docs/cli-contract.md".to_string(),
        ],
        commands: vec![
            "cmux settings path".to_string(),
            "cmux config doctor".to_string(),
            "cmux reload-config".to_string(),
        ],
    }
}

pub fn workspace_group_new_workspace_placement(anchor_cwd: Option<&str>) -> Option<String> {
    workspace_group_config_for_cwd(anchor_cwd).new_workspace_placement
}

pub fn default_window_display_name() -> Option<String> {
    let env = ConfigEnvironment::live();
    cmux_json_config_paths(&env)
        .into_iter()
        .filter_map(|path| {
            read_jsonc_object(&path)?
                .get("app")
                .and_then(Value::as_object)?
                .get("devWindowDisplay")
                .and_then(trimmed_json_string)
        })
        .last()
}

pub fn get_window_default_display() -> WindowDefaultDisplayPayload {
    let env = ConfigEnvironment::live();
    let path = primary_cmux_json_path(&env);
    let display = default_window_display_name();
    WindowDefaultDisplayPayload {
        ok: true,
        configured: display.is_some(),
        display,
        cleared: false,
        path: path.display().to_string(),
    }
}

pub fn set_window_default_display(
    display: Option<String>,
) -> Result<WindowDefaultDisplayPayload, String> {
    let env = ConfigEnvironment::live();
    let path = primary_cmux_json_path(&env);
    let mut root = read_primary_cmux_json_for_write(&path)?;
    let Some(display) = display
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
    else {
        if let Some(app) = root.get_mut("app").and_then(Value::as_object_mut) {
            app.remove("devWindowDisplay");
            if app.is_empty() {
                root.remove("app");
            }
        }
        write_primary_cmux_json(&path, &root)?;
        return Ok(WindowDefaultDisplayPayload {
            ok: true,
            display: None,
            configured: false,
            cleared: true,
            path: path.display().to_string(),
        });
    };

    let app = root
        .entry("app".to_string())
        .or_insert_with(|| Value::Object(Map::new()));
    if !app.is_object() {
        *app = Value::Object(Map::new());
    }
    if let Some(app) = app.as_object_mut() {
        app.insert("devWindowDisplay".to_string(), json!(display));
    }
    write_primary_cmux_json(&path, &root)?;
    Ok(WindowDefaultDisplayPayload {
        ok: true,
        display: Some(display),
        configured: true,
        cleared: false,
        path: path.display().to_string(),
    })
}

pub fn canvas_settings() -> CanvasSettingsPayload {
    canvas_settings_with_env(&ConfigEnvironment::live())
}

pub fn terminal_text_box_settings() -> TerminalTextBoxSettings {
    terminal_text_box_settings_with_env(&ConfigEnvironment::live())
}

pub fn terminal_interaction_settings() -> TerminalInteractionSettings {
    terminal_interaction_settings_with_env(&ConfigEnvironment::live())
}

pub(crate) fn agent_hibernation_settings() -> crate::agent_hibernation_settings::Settings {
    agent_hibernation_settings_with_env(&ConfigEnvironment::live())
}

pub(crate) fn agent_hibernation_settings_path() -> PathBuf {
    primary_cmux_json_path(&ConfigEnvironment::live())
}

pub(crate) fn set_agent_hibernation_settings(
    settings: crate::agent_hibernation_settings::Settings,
) -> Result<String, String> {
    set_agent_hibernation_settings_with_env(&ConfigEnvironment::live(), settings)
}

pub fn browser_search_settings() -> BrowserSearchSettings {
    browser_search_settings_with_env(&ConfigEnvironment::live())
}

pub fn beta_feature_settings() -> BetaFeatureSettings {
    beta_feature_settings_with_env(&ConfigEnvironment::live())
}

pub fn workspace_color_settings() -> WorkspaceColorSettings {
    workspace_color_settings_with_env(&ConfigEnvironment::live())
}

pub fn sidebar_settings() -> SidebarSettings {
    sidebar_settings_with_env(&ConfigEnvironment::live())
}

pub fn set_sidebar_setting(key: &str, value: Value) -> Result<String, String> {
    set_sidebar_setting_with_env(&ConfigEnvironment::live(), key, value)
}

pub fn set_beta_feature_setting(key: &str, value: Value) -> Result<String, String> {
    set_beta_feature_setting_with_env(&ConfigEnvironment::live(), key, value)
}

pub fn set_workspace_color_setting(key: &str, value: Value) -> Result<String, String> {
    let env = ConfigEnvironment::live();
    set_workspace_color_setting_with_env(&env, key, value)
}

pub fn set_workspace_palette_color(name: &str, color: &str) -> Result<String, String> {
    let env = ConfigEnvironment::live();
    let mut settings = workspace_color_settings_with_env(&env);
    let name = normalized_workspace_color_name(name)?;
    let color = normalize_workspace_color_hex(color)?;
    if let Some(entry) = settings.colors.iter_mut().find(|entry| entry.0 == name) {
        entry.1 = color;
    } else {
        settings.colors.push((name, color));
    }
    settings.colors = ordered_workspace_palette(
        settings
            .colors
            .into_iter()
            .collect::<HashMap<String, String>>(),
    );
    write_workspace_palette(&env, &settings.colors)
}

pub fn remove_workspace_palette_color(name: &str) -> Result<String, String> {
    let env = ConfigEnvironment::live();
    let name = normalized_workspace_color_name(name)?;
    let mut settings = workspace_color_settings_with_env(&env);
    settings.colors.retain(|entry| entry.0 != name);
    write_workspace_palette(&env, &settings.colors)
}

pub fn reset_workspace_color_palette() -> Result<String, String> {
    let env = ConfigEnvironment::live();
    let colors = default_workspace_palette();
    write_workspace_palette(&env, &colors)
}

fn workspace_color_settings_with_env(env: &ConfigEnvironment) -> WorkspaceColorSettings {
    let mut indicator_style = "leftRail".to_string();
    let mut selection_color = None;
    let mut notification_badge_color = None;
    let mut palette = default_workspace_palette()
        .into_iter()
        .collect::<HashMap<_, _>>();

    for path in cmux_json_config_paths(env) {
        let Some(section) = read_jsonc_object(&path).and_then(|root| {
            root.get("workspaceColors")
                .and_then(Value::as_object)
                .cloned()
        }) else {
            continue;
        };
        if let Some(style) = section
            .get("indicatorStyle")
            .and_then(Value::as_str)
            .and_then(normalize_workspace_indicator_style)
        {
            indicator_style = style.to_string();
        }
        if section.contains_key("selectionColor") {
            selection_color = section
                .get("selectionColor")
                .and_then(Value::as_str)
                .and_then(|color| normalize_workspace_color_hex(color).ok());
        }
        if section.contains_key("notificationBadgeColor") {
            notification_badge_color = section
                .get("notificationBadgeColor")
                .and_then(Value::as_str)
                .and_then(|color| normalize_workspace_color_hex(color).ok());
        }
        if let Some(colors) = section.get("colors").and_then(Value::as_object) {
            palette = colors
                .iter()
                .filter_map(|(name, color)| {
                    let name = normalized_workspace_color_name(name).ok()?;
                    let color =
                        normalize_workspace_color_hex(color.as_str().unwrap_or_default()).ok()?;
                    Some((name, color))
                })
                .collect();
        } else {
            if let Some(overrides) = section.get("paletteOverrides").and_then(Value::as_object) {
                for (name, color) in overrides {
                    let Ok(name) = normalized_workspace_color_name(name) else {
                        continue;
                    };
                    let Ok(color) =
                        normalize_workspace_color_hex(color.as_str().unwrap_or_default())
                    else {
                        continue;
                    };
                    palette.insert(name, color);
                }
            }
            if let Some(custom_colors) = section.get("customColors").and_then(Value::as_array) {
                for color in custom_colors {
                    let Ok(color) =
                        normalize_workspace_color_hex(color.as_str().unwrap_or_default())
                    else {
                        continue;
                    };
                    if palette.values().any(|existing| existing == &color) {
                        continue;
                    }
                    let mut index = 1;
                    loop {
                        let name = format!("Custom {index}");
                        if !palette.contains_key(&name) {
                            palette.insert(name, color.clone());
                            break;
                        }
                        index += 1;
                    }
                }
            }
        }
    }

    WorkspaceColorSettings {
        indicator_style,
        selection_color,
        notification_badge_color,
        colors: ordered_workspace_palette(palette),
        path: primary_cmux_json_path(env).display().to_string(),
    }
}

fn set_workspace_color_setting_with_env(
    env: &ConfigEnvironment,
    key: &str,
    value: Value,
) -> Result<String, String> {
    let path = primary_cmux_json_path(env);
    let mut root = read_primary_cmux_json_for_write(&path)?;
    let section = root
        .entry("workspaceColors".to_string())
        .or_insert_with(|| Value::Object(Map::new()));
    if !section.is_object() {
        *section = Value::Object(Map::new());
    }
    let normalized = match key {
        "indicatorStyle" => {
            let style = value
                .as_str()
                .and_then(normalize_workspace_indicator_style)
                .ok_or_else(|| "indicatorStyle must be leftRail or solidFill".to_string())?;
            json!(style)
        }
        "selectionColor" | "notificationBadgeColor" => {
            if value.is_null() {
                Value::Null
            } else {
                json!(normalize_workspace_color_hex(value.as_str().ok_or_else(
                    || format!("{key} must be a hex color or null")
                )?)?)
            }
        }
        _ => return Err(format!("unsupported workspace color setting: {key}")),
    };
    section
        .as_object_mut()
        .expect("workspaceColors setting object")
        .insert(key.to_string(), normalized);
    write_primary_cmux_json(&path, &root)?;
    Ok(path.display().to_string())
}

fn write_workspace_palette(
    env: &ConfigEnvironment,
    colors: &[(String, String)],
) -> Result<String, String> {
    let path = primary_cmux_json_path(env);
    let mut root = read_primary_cmux_json_for_write(&path)?;
    let section = root
        .entry("workspaceColors".to_string())
        .or_insert_with(|| Value::Object(Map::new()));
    if !section.is_object() {
        *section = Value::Object(Map::new());
    }
    let section = section
        .as_object_mut()
        .expect("workspaceColors setting object");
    section.insert(
        "colors".to_string(),
        Value::Object(
            colors
                .iter()
                .map(|(name, color)| (name.clone(), json!(color)))
                .collect(),
        ),
    );
    section.remove("paletteOverrides");
    section.remove("customColors");
    write_primary_cmux_json(&path, &root)?;
    Ok(path.display().to_string())
}

fn default_workspace_palette() -> Vec<(String, String)> {
    WORKSPACE_COLOR_DEFAULT_PALETTE
        .iter()
        .map(|(name, color)| ((*name).to_string(), (*color).to_string()))
        .collect()
}

fn ordered_workspace_palette(mut colors: HashMap<String, String>) -> Vec<(String, String)> {
    let mut ordered = Vec::new();
    for (name, _) in WORKSPACE_COLOR_DEFAULT_PALETTE {
        if let Some(color) = colors.remove(*name) {
            ordered.push(((*name).to_string(), color));
        }
    }
    let mut custom = colors.into_iter().collect::<Vec<_>>();
    custom.sort_by(|left, right| left.0.to_lowercase().cmp(&right.0.to_lowercase()));
    ordered.extend(custom);
    ordered
}

fn normalize_workspace_indicator_style(value: &str) -> Option<&'static str> {
    match value.trim() {
        "leftRail" | "rail" | "border" | "washRail" | "blueWashColorRail" => Some("leftRail"),
        "solidFill" | "wash" | "lift" | "typography" => Some("solidFill"),
        _ => None,
    }
}

fn normalized_workspace_color_name(value: &str) -> Result<String, String> {
    let value = value.trim();
    if value.is_empty() {
        Err("workspace color name cannot be empty".to_string())
    } else {
        Ok(value.to_string())
    }
}

fn normalize_workspace_color_hex(value: &str) -> Result<String, String> {
    let value = value.trim();
    let body = value.strip_prefix('#').unwrap_or(value);
    if body.len() != 6 || !body.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err("workspace color must be a six-digit hex value".to_string());
    }
    Ok(format!("#{}", body.to_ascii_uppercase()))
}

fn beta_feature_settings_with_env(env: &ConfigEnvironment) -> BetaFeatureSettings {
    let mut settings = BetaFeatureSettings {
        path: primary_cmux_json_path(env).display().to_string(),
        ..BetaFeatureSettings::default()
    };
    for path in cmux_json_config_paths(env) {
        let Some(root) = read_jsonc_object(&path) else {
            continue;
        };
        apply_root_bool(
            &root,
            "rightSidebar.beta.feed.enabled",
            &mut settings.right_sidebar_feed,
        );
        apply_root_bool(
            &root,
            "rightSidebar.beta.dock.enabled",
            &mut settings.right_sidebar_dock,
        );
        apply_root_bool(&root, "extensions.beta.enabled", &mut settings.extensions);
        apply_root_bool(
            &root,
            "customSidebars.beta.enabled",
            &mut settings.custom_sidebars,
        );
        apply_root_bool(&root, "remoteTmux.beta.enabled", &mut settings.remote_tmux);
    }
    settings
}

fn apply_root_bool(root: &Map<String, Value>, key: &str, target: &mut bool) {
    if let Some(value) = root.get(key).and_then(Value::as_bool) {
        *target = value;
    }
}

fn set_beta_feature_setting_with_env(
    env: &ConfigEnvironment,
    key: &str,
    value: Value,
) -> Result<String, String> {
    let canonical = match key.trim() {
        "rightSidebar.beta.feed.enabled" | "rightSidebarFeed" | "feed" => {
            "rightSidebar.beta.feed.enabled"
        }
        "rightSidebar.beta.dock.enabled" | "rightSidebarDock" | "dock" => {
            "rightSidebar.beta.dock.enabled"
        }
        "extensions.beta.enabled" | "extensions" => "extensions.beta.enabled",
        "customSidebars.beta.enabled" | "customSidebars" => "customSidebars.beta.enabled",
        "remoteTmux.beta.enabled" | "remoteTmux" => "remoteTmux.beta.enabled",
        _ => return Err(format!("unsupported beta feature setting: {key}")),
    };
    let enabled = value
        .as_bool()
        .ok_or_else(|| format!("{canonical} must be true or false"))?;
    let path = primary_cmux_json_path(env);
    let mut root = read_primary_cmux_json_for_write(&path)?;
    root.insert(canonical.to_string(), json!(enabled));
    write_primary_cmux_json(&path, &root)?;
    Ok(path.display().to_string())
}

fn sidebar_settings_with_env(env: &ConfigEnvironment) -> SidebarSettings {
    let mut settings = SidebarSettings {
        path: primary_cmux_json_path(env).display().to_string(),
        ..SidebarSettings::default()
    };
    for path in cmux_json_config_paths(env) {
        let Some(root) = read_jsonc_object(&path) else {
            continue;
        };
        if let Some(appearance) = root.get("sidebarAppearance").and_then(Value::as_object) {
            if let Some(value) = appearance
                .get("matchTerminalBackground")
                .and_then(Value::as_bool)
            {
                settings.match_terminal_background = value;
            }
        }
        let Some(sidebar) = root.get("sidebar").and_then(Value::as_object) else {
            continue;
        };
        apply_sidebar_bool(sidebar, "hideAllDetails", &mut settings.hide_all_details);
        apply_sidebar_bool(
            sidebar,
            "wrapWorkspaceTitles",
            &mut settings.wrap_workspace_titles,
        );
        apply_sidebar_bool(
            sidebar,
            "showWorkspaceDescription",
            &mut settings.show_workspace_description,
        );
        if let Some(layout) = sidebar
            .get("branchLayout")
            .and_then(Value::as_str)
            .and_then(SidebarBranchLayout::parse)
        {
            settings.branch_layout = layout;
        } else if let Some(vertical) = sidebar.get("branchVerticalLayout").and_then(Value::as_bool)
        {
            settings.branch_layout = if vertical {
                SidebarBranchLayout::Vertical
            } else {
                SidebarBranchLayout::Inline
            };
        }
        apply_sidebar_bool(
            sidebar,
            "stackBranchDirectory",
            &mut settings.stack_branch_directory,
        );
        apply_sidebar_bool(
            sidebar,
            "pathLastSegmentOnly",
            &mut settings.path_last_segment_only,
        );
        apply_sidebar_bool(
            sidebar,
            "showNotificationMessage",
            &mut settings.show_notification_message,
        );
        apply_sidebar_bool(
            sidebar,
            "showBranchDirectory",
            &mut settings.show_branch_directory,
        );
        apply_sidebar_bool(
            sidebar,
            "showPullRequests",
            &mut settings.show_pull_requests,
        );
        apply_sidebar_bool(sidebar, "watchGitStatus", &mut settings.watch_git_status);
        apply_sidebar_bool(
            sidebar,
            "makePullRequestsClickable",
            &mut settings.make_pull_requests_clickable,
        );
        apply_sidebar_bool(
            sidebar,
            "openPullRequestLinksInCmuxBrowser",
            &mut settings.open_pull_request_links_in_cmux_browser,
        );
        apply_sidebar_bool(
            sidebar,
            "openPortLinksInCmuxBrowser",
            &mut settings.open_port_links_in_cmux_browser,
        );
        apply_sidebar_bool(sidebar, "showSSH", &mut settings.show_ssh);
        apply_sidebar_bool(sidebar, "showPorts", &mut settings.show_ports);
        apply_sidebar_bool(sidebar, "showLog", &mut settings.show_log);
        apply_sidebar_bool(sidebar, "showProgress", &mut settings.show_progress);
        apply_sidebar_bool(
            sidebar,
            "showCustomMetadata",
            &mut settings.show_custom_metadata,
        );
        if sidebar.contains_key("rightMaxWidth") {
            settings.right_max_width = sidebar
                .get("rightMaxWidth")
                .and_then(Value::as_f64)
                .filter(|value| value.is_finite() && *value > 0.0)
                .map(|value| value.clamp(276.0, 4096.0).round());
        }
    }
    settings
}

fn apply_sidebar_bool(section: &Map<String, Value>, key: &str, target: &mut bool) {
    if let Some(value) = section.get(key).and_then(Value::as_bool) {
        *target = value;
    }
}

fn set_sidebar_setting_with_env(
    env: &ConfigEnvironment,
    key: &str,
    value: Value,
) -> Result<String, String> {
    let path = primary_cmux_json_path(env);
    let mut root = read_primary_cmux_json_for_write(&path)?;
    let normalized_key = key
        .strip_prefix("sidebar.")
        .or_else(|| key.strip_prefix("sidebarAppearance."))
        .unwrap_or(key);
    if normalized_key == "matchTerminalBackground" {
        let enabled = value
            .as_bool()
            .ok_or_else(|| "matchTerminalBackground must be true or false".to_string())?;
        let section = root
            .entry("sidebarAppearance".to_string())
            .or_insert_with(|| Value::Object(Map::new()));
        if !section.is_object() {
            *section = Value::Object(Map::new());
        }
        section
            .as_object_mut()
            .expect("sidebarAppearance object")
            .insert(normalized_key.to_string(), json!(enabled));
        write_primary_cmux_json(&path, &root)?;
        return Ok(path.display().to_string());
    }

    let section = root
        .entry("sidebar".to_string())
        .or_insert_with(|| Value::Object(Map::new()));
    if !section.is_object() {
        *section = Value::Object(Map::new());
    }
    let normalized = match normalized_key {
        "branchLayout" => {
            let layout = value
                .as_str()
                .and_then(SidebarBranchLayout::parse)
                .ok_or_else(|| "branchLayout must be vertical or inline".to_string())?;
            json!(layout.as_str())
        }
        "hideAllDetails"
        | "wrapWorkspaceTitles"
        | "showWorkspaceDescription"
        | "stackBranchDirectory"
        | "pathLastSegmentOnly"
        | "showNotificationMessage"
        | "showBranchDirectory"
        | "showPullRequests"
        | "watchGitStatus"
        | "makePullRequestsClickable"
        | "openPullRequestLinksInCmuxBrowser"
        | "openPortLinksInCmuxBrowser"
        | "showSSH"
        | "showPorts"
        | "showLog"
        | "showProgress"
        | "showCustomMetadata" => json!(value
            .as_bool()
            .ok_or_else(|| format!("{normalized_key} must be true or false"))?),
        "rightMaxWidth" => {
            if value.is_null() {
                section
                    .as_object_mut()
                    .expect("sidebar object")
                    .insert(normalized_key.to_string(), Value::Null);
                write_primary_cmux_json(&path, &root)?;
                return Ok(path.display().to_string());
            }
            let width = value
                .as_f64()
                .filter(|width| width.is_finite() && *width > 0.0)
                .ok_or_else(|| "rightMaxWidth must be a positive number or null".to_string())?;
            json!(width.clamp(276.0, 4096.0).round())
        }
        _ => return Err(format!("unsupported sidebar setting: {key}")),
    };
    section
        .as_object_mut()
        .expect("sidebar object")
        .insert(normalized_key.to_string(), normalized);
    write_primary_cmux_json(&path, &root)?;
    Ok(path.display().to_string())
}

pub fn set_browser_search_setting(key: &str, value: Value) -> Result<String, String> {
    let env = ConfigEnvironment::live();
    let path = primary_cmux_json_path(&env);
    let mut root = read_primary_cmux_json_for_write(&path)?;
    let browser = root
        .entry("browser".to_string())
        .or_insert_with(|| Value::Object(Map::new()));
    if !browser.is_object() {
        *browser = Value::Object(Map::new());
    }
    let normalized = match key {
        "defaultSearchEngine" => {
            let engine = value
                .as_str()
                .map(str::trim)
                .filter(|engine| crate::browser_omnibar::valid_browser_search_engine(engine))
                .ok_or_else(|| {
                    "defaultSearchEngine is not a supported search engine".to_string()
                })?;
            json!(engine)
        }
        "customSearchEngineName" => {
            let name = value
                .as_str()
                .ok_or_else(|| "customSearchEngineName must be a string".to_string())?;
            json!(name.trim())
        }
        "customSearchEngineURLTemplate" => {
            let template = value
                .as_str()
                .map(str::trim)
                .filter(|template| !template.is_empty())
                .ok_or_else(|| {
                    "customSearchEngineURLTemplate must be a non-empty string".to_string()
                })?;
            let settings = BrowserSearchSettings {
                engine: "custom".to_string(),
                custom_url_template: template.to_string(),
                ..BrowserSearchSettings::default()
            };
            if crate::browser_omnibar::browser_search_url(&settings, "cmux search").is_none() {
                return Err(
                    "customSearchEngineURLTemplate must produce an HTTP or HTTPS URL".to_string(),
                );
            }
            json!(template)
        }
        "showSearchSuggestions" => {
            let enabled = value
                .as_bool()
                .ok_or_else(|| "showSearchSuggestions must be true or false".to_string())?;
            json!(enabled)
        }
        _ => return Err(format!("unsupported browser setting: {key}")),
    };
    browser
        .as_object_mut()
        .expect("browser settings object")
        .insert(key.to_string(), normalized);
    write_primary_cmux_json(&path, &root)?;
    Ok(path.display().to_string())
}

fn browser_search_settings_with_env(env: &ConfigEnvironment) -> BrowserSearchSettings {
    let mut settings = BrowserSearchSettings::default();
    for path in cmux_json_config_paths(env) {
        let Some(browser) = read_jsonc_object(&path)
            .and_then(|root| root.get("browser").and_then(Value::as_object).cloned())
        else {
            continue;
        };
        if let Some(engine) = browser
            .get("defaultSearchEngine")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|engine| crate::browser_omnibar::valid_browser_search_engine(engine))
        {
            settings.engine = engine.to_string();
        }
        if let Some(name) = browser
            .get("customSearchEngineName")
            .and_then(Value::as_str)
        {
            settings.custom_name = name.to_string();
        }
        if let Some(template) = browser
            .get("customSearchEngineURLTemplate")
            .and_then(Value::as_str)
            .filter(|template| {
                let mut custom = settings.clone();
                custom.engine = "custom".to_string();
                custom.custom_url_template = (*template).to_string();
                crate::browser_omnibar::browser_search_url(&custom, "cmux search").is_some()
            })
        {
            settings.custom_url_template = template.to_string();
        }
        if let Some(enabled) = browser
            .get("showSearchSuggestions")
            .and_then(Value::as_bool)
        {
            settings.show_search_suggestions = enabled;
        }
    }
    settings
}

pub fn diff_viewer_default_layout() -> DiffViewerLayout {
    diff_viewer_default_layout_with_env(&ConfigEnvironment::live())
}

pub fn set_diff_viewer_default_layout(layout: DiffViewerLayout) -> Result<String, String> {
    let env = ConfigEnvironment::live();
    set_diff_viewer_default_layout_with_env(&env, layout)
}

fn set_diff_viewer_default_layout_with_env(
    env: &ConfigEnvironment,
    layout: DiffViewerLayout,
) -> Result<String, String> {
    let path = primary_cmux_json_path(env);
    let mut root = read_primary_cmux_json_for_write(&path)?;
    let section = root
        .entry("diffViewer".to_string())
        .or_insert_with(|| Value::Object(Map::new()));
    if !section.is_object() {
        *section = Value::Object(Map::new());
    }
    section
        .as_object_mut()
        .expect("diffViewer settings object")
        .insert("defaultLayout".to_string(), json!(layout.as_str()));
    write_primary_cmux_json(&path, &root)?;
    Ok(path.display().to_string())
}

fn diff_viewer_default_layout_with_env(env: &ConfigEnvironment) -> DiffViewerLayout {
    let mut layout = DiffViewerLayout::Unified;
    for path in cmux_json_config_paths(env) {
        if let Some(value) = read_jsonc_object(&path)
            .and_then(|root| root.get("diffViewer").and_then(Value::as_object).cloned())
            .and_then(|section| {
                section
                    .get("defaultLayout")
                    .and_then(Value::as_str)
                    .and_then(DiffViewerLayout::parse)
            })
        {
            layout = value;
        }
    }
    layout
}

#[cfg_attr(not(feature = "gtk"), allow(dead_code))]
pub fn terminal_managed_ghostty_config() -> String {
    terminal_managed_ghostty_config_for(terminal_interaction_settings())
}

#[cfg_attr(not(any(test, feature = "gtk")), allow(dead_code))]
fn terminal_managed_ghostty_config_for(settings: TerminalInteractionSettings) -> String {
    format!(
        "copy-on-select = {}",
        if settings.copy_on_select {
            "clipboard"
        } else {
            "false"
        }
    )
}

pub fn custom_sidebar_renderer_mode() -> CustomSidebarRendererMode {
    if let Some(mode) = std::env::var("CMUX_CUSTOM_SIDEBAR_RENDERER")
        .ok()
        .and_then(|value| CustomSidebarRendererMode::parse(&value))
    {
        return mode;
    }
    custom_sidebar_renderer_mode_with_env(&ConfigEnvironment::live())
}

#[cfg_attr(not(feature = "gtk"), allow(dead_code))]
pub fn set_custom_sidebar_renderer_mode(mode: CustomSidebarRendererMode) -> Result<String, String> {
    let env = ConfigEnvironment::live();
    set_custom_sidebar_renderer_mode_with_env(&env, mode)
}

fn set_custom_sidebar_renderer_mode_with_env(
    env: &ConfigEnvironment,
    mode: CustomSidebarRendererMode,
) -> Result<String, String> {
    let path = primary_cmux_json_path(env);
    let mut root = read_primary_cmux_json_for_write(&path)?;
    let section = root
        .entry("customSidebars".to_string())
        .or_insert_with(|| Value::Object(Map::new()));
    if !section.is_object() {
        *section = Value::Object(Map::new());
    }
    section
        .as_object_mut()
        .expect("customSidebars settings object")
        .insert("renderer".to_string(), json!(mode.as_str()));
    write_primary_cmux_json(&path, &root)?;
    Ok(path.display().to_string())
}

fn custom_sidebar_renderer_mode_with_env(env: &ConfigEnvironment) -> CustomSidebarRendererMode {
    let mut mode = CustomSidebarRendererMode::InProcess;
    for path in cmux_json_config_paths(env) {
        if let Some(value) = read_jsonc_object(&path)
            .and_then(|root| {
                root.get("customSidebars")
                    .and_then(Value::as_object)
                    .cloned()
            })
            .and_then(|section| {
                section
                    .get("renderer")
                    .and_then(Value::as_str)
                    .and_then(CustomSidebarRendererMode::parse)
            })
        {
            mode = value;
        }
    }
    mode
}

#[cfg_attr(not(feature = "gtk"), allow(dead_code))]
pub fn set_terminal_text_box_setting(key: &str, value: Value) -> Result<String, String> {
    let env = ConfigEnvironment::live();
    set_terminal_text_box_setting_with_env(&env, key, value)
}

fn set_terminal_text_box_setting_with_env(
    env: &ConfigEnvironment,
    key: &str,
    value: Value,
) -> Result<String, String> {
    let value = match key {
        "showTextBoxOnNewTerminals" | "focusTextBoxOnNewTerminals" => Value::Bool(
            value
                .as_bool()
                .ok_or_else(|| format!("{key} requires a boolean"))?,
        ),
        "textBoxMaxLines" => json!(value
            .as_u64()
            .ok_or_else(|| "textBoxMaxLines requires an integer".to_string())?
            .clamp(1, 20)),
        _ => return Err(format!("Unknown terminal TextBox setting '{key}'")),
    };
    let path = primary_cmux_json_path(env);
    let mut root = read_primary_cmux_json_for_write(&path)?;
    let terminal = root
        .entry("terminal".to_string())
        .or_insert_with(|| Value::Object(Map::new()));
    if !terminal.is_object() {
        *terminal = Value::Object(Map::new());
    }
    terminal
        .as_object_mut()
        .expect("terminal settings object")
        .insert(key.to_string(), value);
    write_primary_cmux_json(&path, &root)?;
    Ok(path.display().to_string())
}

fn terminal_text_box_settings_with_env(env: &ConfigEnvironment) -> TerminalTextBoxSettings {
    let mut settings = TerminalTextBoxSettings {
        show_on_new_terminals: false,
        focus_on_new_terminals: false,
        max_lines: 10,
    };
    for path in cmux_json_config_paths(env) {
        let Some(terminal) = read_jsonc_object(&path)
            .and_then(|root| root.get("terminal").and_then(Value::as_object).cloned())
        else {
            continue;
        };
        if let Some(value) = terminal
            .get("showTextBoxOnNewTerminals")
            .and_then(Value::as_bool)
        {
            settings.show_on_new_terminals = value;
        }
        if let Some(value) = terminal
            .get("focusTextBoxOnNewTerminals")
            .and_then(Value::as_bool)
        {
            settings.focus_on_new_terminals = value;
        }
        if let Some(value) = terminal.get("textBoxMaxLines").and_then(Value::as_u64) {
            settings.max_lines = value.clamp(1, 20) as u32;
        }
    }
    settings
}

#[cfg_attr(not(feature = "gtk"), allow(dead_code))]
pub fn set_terminal_interaction_setting(key: &str, value: Value) -> Result<String, String> {
    let env = ConfigEnvironment::live();
    set_terminal_interaction_setting_with_env(&env, key, value)
}

fn set_terminal_interaction_setting_with_env(
    env: &ConfigEnvironment,
    key: &str,
    value: Value,
) -> Result<String, String> {
    let value = match key {
        "showScrollBar" | "copyOnSelect" | "autoResumeAgentSessions" => Value::Bool(
            value
                .as_bool()
                .ok_or_else(|| format!("{key} requires a boolean"))?,
        ),
        _ => return Err(format!("Unknown terminal interaction setting '{key}'")),
    };
    let path = primary_cmux_json_path(env);
    let mut root = read_primary_cmux_json_for_write(&path)?;
    let terminal = root
        .entry("terminal".to_string())
        .or_insert_with(|| Value::Object(Map::new()));
    if !terminal.is_object() {
        *terminal = Value::Object(Map::new());
    }
    terminal
        .as_object_mut()
        .expect("terminal settings object")
        .insert(key.to_string(), value);
    write_primary_cmux_json(&path, &root)?;
    Ok(path.display().to_string())
}

fn terminal_interaction_settings_with_env(env: &ConfigEnvironment) -> TerminalInteractionSettings {
    let mut settings = TerminalInteractionSettings::default();
    for path in cmux_json_config_paths(env) {
        let Some(terminal) = read_jsonc_object(&path)
            .and_then(|root| root.get("terminal").and_then(Value::as_object).cloned())
        else {
            continue;
        };
        if let Some(value) = terminal.get("showScrollBar").and_then(Value::as_bool) {
            settings.show_scroll_bar = value;
        }
        if let Some(value) = terminal.get("copyOnSelect").and_then(Value::as_bool) {
            settings.copy_on_select = value;
        }
        if let Some(value) = terminal
            .get("autoResumeAgentSessions")
            .and_then(Value::as_bool)
        {
            settings.auto_resume_agent_sessions = value;
        }
    }
    settings
}

fn agent_hibernation_settings_with_env(
    env: &ConfigEnvironment,
) -> crate::agent_hibernation_settings::Settings {
    let mut settings = crate::agent_hibernation_settings::Settings::default();
    for path in cmux_json_config_paths(env) {
        let Some(agent_hibernation) = read_jsonc_object(&path)
            .and_then(|root| root.get("terminal").and_then(Value::as_object).cloned())
            .and_then(|terminal| {
                terminal
                    .get("agentHibernation")
                    .and_then(Value::as_object)
                    .cloned()
            })
        else {
            continue;
        };
        if let Some(value) = agent_hibernation.get("enabled").and_then(Value::as_bool) {
            settings.enabled = value;
        }
        if let Some(value) = agent_hibernation.get("idleSeconds").and_then(Value::as_u64) {
            settings.idle_seconds = value;
        }
        if let Some(value) = agent_hibernation
            .get("maxLiveTerminals")
            .and_then(Value::as_u64)
        {
            settings.max_live_terminals = value;
        }
        if let Some(value) = agent_hibernation
            .get("confirmationSeconds")
            .and_then(Value::as_u64)
        {
            settings.confirmation_seconds = value;
        }
    }
    settings.sanitized()
}

fn set_agent_hibernation_settings_with_env(
    env: &ConfigEnvironment,
    settings: crate::agent_hibernation_settings::Settings,
) -> Result<String, String> {
    let settings = settings.sanitized();
    let path = primary_cmux_json_path(env);
    let mut root = read_primary_cmux_json_for_write(&path)?;
    let terminal = root
        .entry("terminal".to_string())
        .or_insert_with(|| Value::Object(Map::new()));
    if !terminal.is_object() {
        *terminal = Value::Object(Map::new());
    }
    terminal
        .as_object_mut()
        .expect("terminal settings object")
        .insert(
            "agentHibernation".to_string(),
            json!({
                "enabled": settings.enabled,
                "idleSeconds": settings.idle_seconds,
                "maxLiveTerminals": settings.max_live_terminals,
                "confirmationSeconds": settings.confirmation_seconds
            }),
        );
    write_primary_cmux_json(&path, &root)?;
    Ok(path.display().to_string())
}

pub(crate) fn primary_cmux_json_path_live() -> PathBuf {
    primary_cmux_json_path(&ConfigEnvironment::live())
}

pub(crate) fn terminal_resume_commands() -> Result<Vec<Value>, String> {
    let path = primary_cmux_json_path_live();
    let root = read_primary_cmux_json_for_write(&path)?;
    Ok(root
        .get("terminal")
        .and_then(Value::as_object)
        .and_then(|terminal| terminal.get("resumeCommands"))
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default())
}

pub(crate) fn set_terminal_resume_commands(records: &[Value]) -> Result<String, String> {
    let path = primary_cmux_json_path_live();
    let mut root = read_primary_cmux_json_for_write(&path)?;
    let terminal = root
        .entry("terminal".to_string())
        .or_insert_with(|| Value::Object(Map::new()));
    if !terminal.is_object() {
        *terminal = Value::Object(Map::new());
    }
    terminal
        .as_object_mut()
        .expect("terminal settings object")
        .insert("resumeCommands".to_string(), Value::Array(records.to_vec()));
    write_primary_cmux_json(&path, &root)?;
    let _ = fs::set_permissions(&path, fs::Permissions::from_mode(0o600));
    Ok(path.display().to_string())
}

pub fn system_wide_hotkey_enabled() -> bool {
    system_wide_hotkey_enabled_with_env(&ConfigEnvironment::live())
}

#[cfg_attr(not(feature = "gtk"), allow(dead_code))]
pub fn set_system_wide_hotkey_enabled(enabled: bool) -> Result<String, String> {
    let env = ConfigEnvironment::live();
    set_system_wide_hotkey_enabled_with_env(&env, enabled)
}

fn set_system_wide_hotkey_enabled_with_env(
    env: &ConfigEnvironment,
    enabled: bool,
) -> Result<String, String> {
    let path = primary_cmux_json_path(&env);
    let mut root = read_primary_cmux_json_for_write(&path)?;
    let app = root
        .entry("app".to_string())
        .or_insert_with(|| Value::Object(Map::new()));
    if !app.is_object() {
        *app = Value::Object(Map::new());
    }
    app.as_object_mut()
        .expect("app settings object")
        .insert("systemWideHotkeyEnabled".to_string(), json!(enabled));
    write_primary_cmux_json(&path, &root)?;
    Ok(path.display().to_string())
}

fn system_wide_hotkey_enabled_with_env(env: &ConfigEnvironment) -> bool {
    let mut enabled = false;
    for path in cmux_json_config_paths(env) {
        if let Some(value) = read_jsonc_object(&path)
            .and_then(|root| root.get("app").and_then(Value::as_object).cloned())
            .and_then(|app| app.get("systemWideHotkeyEnabled").and_then(Value::as_bool))
        {
            enabled = value;
        }
    }
    enabled
}

pub fn shortcut_bindings() -> HashMap<String, ShortcutBinding> {
    shortcut_bindings_with_env(&ConfigEnvironment::live())
}

pub fn shortcut_when_clauses() -> HashMap<String, String> {
    shortcut_when_clauses_with_env(&ConfigEnvironment::live())
}

pub fn set_shortcut_binding(
    action_id: &str,
    update: ShortcutBindingUpdate,
) -> Result<String, String> {
    let env = ConfigEnvironment::live();
    set_shortcut_binding_with_env(&env, action_id, update)
}

fn shortcut_bindings_with_env(env: &ConfigEnvironment) -> HashMap<String, ShortcutBinding> {
    let mut bindings = HashMap::new();
    for path in cmux_json_config_paths(env) {
        let Some(root) = read_jsonc_object(&path) else {
            continue;
        };
        let Some(shortcuts) = root.get("shortcuts").and_then(Value::as_object) else {
            continue;
        };
        if let Some(nested) = shortcuts.get("bindings").and_then(Value::as_object) {
            merge_shortcut_bindings(&mut bindings, nested);
        }
        let direct = shortcuts
            .iter()
            .filter(|(key, _)| {
                !matches!(key.as_str(), "bindings" | "showModifierHoldHints" | "when")
            })
            .map(|(key, value)| (key.clone(), value.clone()))
            .collect::<Map<_, _>>();
        merge_shortcut_bindings(&mut bindings, &direct);
    }
    bindings
}

fn shortcut_when_clauses_with_env(env: &ConfigEnvironment) -> HashMap<String, String> {
    let mut clauses = HashMap::new();
    for path in cmux_json_config_paths(env) {
        let Some(root) = read_jsonc_object(&path) else {
            continue;
        };
        let Some(when) = root
            .get("shortcuts")
            .and_then(Value::as_object)
            .and_then(|shortcuts| shortcuts.get("when"))
            .and_then(Value::as_object)
        else {
            continue;
        };
        for (action_id, value) in when {
            if let Some(expression) = value.as_str() {
                clauses.insert(action_id.clone(), expression.to_string());
            }
        }
    }
    clauses
}

fn merge_shortcut_bindings(
    bindings: &mut HashMap<String, ShortcutBinding>,
    values: &Map<String, Value>,
) {
    for (action_id, value) in values {
        if let Some(binding) = parse_shortcut_binding_value(value) {
            bindings.insert(action_id.clone(), binding);
        }
    }
}

fn parse_shortcut_binding_value(value: &Value) -> Option<ShortcutBinding> {
    match value {
        Value::Null => Some(ShortcutBinding::Unbound),
        Value::String(raw) => parse_shortcut_binding_string(raw),
        Value::Array(values) if values.is_empty() => Some(ShortcutBinding::Unbound),
        Value::Array(values) if values.len() == 1 => values
            .first()
            .and_then(Value::as_str)
            .and_then(parse_shortcut_binding_string),
        Value::Array(values) if values.len() == 2 => {
            let first = values.first()?.as_str()?.trim();
            let second = values.get(1)?.as_str()?.trim();
            if first.is_empty() || second.is_empty() {
                None
            } else {
                Some(ShortcutBinding::Chord(
                    first.to_string(),
                    second.to_string(),
                ))
            }
        }
        Value::Object(object) => parse_shortcut_binding_object(object),
        _ => None,
    }
}

fn parse_shortcut_binding_string(raw: &str) -> Option<ShortcutBinding> {
    let trimmed = raw.trim();
    if trimmed.is_empty()
        || matches!(
            trimmed.to_ascii_lowercase().as_str(),
            "none" | "clear" | "unbound" | "disabled"
        )
    {
        Some(ShortcutBinding::Unbound)
    } else {
        Some(ShortcutBinding::Single(trimmed.to_string()))
    }
}

fn parse_shortcut_binding_object(object: &Map<String, Value>) -> Option<ShortcutBinding> {
    let first = object.get("first")?.as_object()?;
    let Some(first) = shortcut_stroke_object_combo(first) else {
        return Some(ShortcutBinding::Unbound);
    };
    if let Some(second) = object.get("second").filter(|value| !value.is_null()) {
        let second = shortcut_stroke_object_combo(second.as_object()?)?;
        return Some(ShortcutBinding::Chord(first, second));
    }
    Some(ShortcutBinding::Single(first))
}

fn shortcut_stroke_object_combo(first: &Map<String, Value>) -> Option<String> {
    let key = first.get("key")?.as_str()?.trim();
    if key.is_empty() {
        return None;
    }
    let mut parts = Vec::new();
    for (field, token) in [
        ("command", "cmd"),
        ("control", "ctrl"),
        ("option", "opt"),
        ("shift", "shift"),
    ] {
        if first.get(field).and_then(Value::as_bool).unwrap_or(false) {
            parts.push(token.to_string());
        }
    }
    let key = match key {
        "\r" | "return" => "enter",
        "←" => "left",
        "→" => "right",
        "↑" => "up",
        "↓" => "down",
        other => other,
    };
    parts.push(key.to_string());
    Some(parts.join("+"))
}

fn set_shortcut_binding_with_env(
    env: &ConfigEnvironment,
    action_id: &str,
    update: ShortcutBindingUpdate,
) -> Result<String, String> {
    let action_id = action_id.trim();
    if action_id.is_empty() {
        return Err("shortcut action id must not be empty".to_string());
    }
    let path = primary_cmux_json_path(env);
    let mut root = read_primary_cmux_json_for_write(&path)?;
    let shortcuts = root
        .entry("shortcuts".to_string())
        .or_insert_with(|| Value::Object(Map::new()));
    if !shortcuts.is_object() {
        *shortcuts = Value::Object(Map::new());
    }
    let shortcuts = shortcuts.as_object_mut().expect("shortcuts object");
    let bindings = shortcuts
        .entry("bindings".to_string())
        .or_insert_with(|| Value::Object(Map::new()));
    if !bindings.is_object() {
        *bindings = Value::Object(Map::new());
    }
    let bindings = bindings.as_object_mut().expect("shortcut bindings object");
    match update {
        ShortcutBindingUpdate::Set(strokes) if strokes.len() == 1 => {
            bindings.insert(action_id.to_string(), Value::String(strokes[0].clone()));
        }
        ShortcutBindingUpdate::Set(strokes) if strokes.len() == 2 => {
            bindings.insert(
                action_id.to_string(),
                Value::Array(strokes.into_iter().map(Value::String).collect()),
            );
        }
        ShortcutBindingUpdate::Set(_) => {
            return Err("shortcut bindings must contain one or two strokes".to_string());
        }
        ShortcutBindingUpdate::Unbind => {
            bindings.insert(action_id.to_string(), Value::Null);
        }
        ShortcutBindingUpdate::Reset => {
            bindings.remove(action_id);
        }
    }
    if bindings.is_empty() {
        shortcuts.remove("bindings");
    }
    if shortcuts.is_empty() {
        root.remove("shortcuts");
    }
    write_primary_cmux_json(&path, &root)?;
    Ok(path.display().to_string())
}

#[cfg_attr(not(feature = "gtk"), allow(dead_code))]
pub fn set_canvas_pane_gap(value: f64) -> Result<CanvasSettingsPayload, String> {
    let env = ConfigEnvironment::live();
    set_canvas_setting_with_env(&env, "paneGap", json!(value.clamp(0.0, 64.0)))?;
    Ok(canvas_settings_with_env(&env))
}

#[cfg_attr(not(feature = "gtk"), allow(dead_code))]
pub fn set_canvas_snapping_enabled(enabled: bool) -> Result<CanvasSettingsPayload, String> {
    let env = ConfigEnvironment::live();
    set_canvas_setting_with_env(&env, "snappingEnabled", json!(enabled))?;
    Ok(canvas_settings_with_env(&env))
}

pub fn app_workspace_settings() -> AppWorkspaceSettings {
    app_workspace_settings_with_env(&ConfigEnvironment::live())
}

pub fn set_app_workspace_placement(value: WorkspacePlacement) -> Result<String, String> {
    let env = ConfigEnvironment::live();
    set_app_workspace_setting_with_env(&env, "newWorkspacePlacement", json!(value.as_str()))?;
    Ok(primary_cmux_json_path(&env).display().to_string())
}

pub fn set_workspace_inherit_working_directory(enabled: bool) -> Result<String, String> {
    let env = ConfigEnvironment::live();
    set_app_workspace_setting_with_env(&env, "workspaceInheritWorkingDirectory", json!(enabled))?;
    Ok(primary_cmux_json_path(&env).display().to_string())
}

pub fn set_keep_workspace_open_when_closing_last_surface(enabled: bool) -> Result<String, String> {
    let env = ConfigEnvironment::live();
    set_app_workspace_setting_with_env(
        &env,
        "keepWorkspaceOpenWhenClosingLastSurface",
        json!(enabled),
    )?;
    Ok(primary_cmux_json_path(&env).display().to_string())
}

pub fn set_app_behavior_setting(key: &str, value: Value) -> Result<String, String> {
    let env = ConfigEnvironment::live();
    set_app_workspace_setting_with_env(&env, key, value)?;
    Ok(primary_cmux_json_path(&env).display().to_string())
}

fn app_workspace_settings_with_env(env: &ConfigEnvironment) -> AppWorkspaceSettings {
    let mut settings = AppWorkspaceSettings {
        new_workspace_placement: WorkspacePlacement::AfterCurrent,
        workspace_inherit_working_directory: true,
        keep_workspace_open_when_closing_last_surface: false,
        confirm_quit: ConfirmQuitPolicy::Always,
        warn_before_closing_tab: true,
        warn_before_closing_tab_x_button: false,
        hide_tab_close_button: false,
    };
    for path in cmux_json_config_paths(env) {
        let Some(app) = read_jsonc_object(&path)
            .and_then(|root| root.get("app").and_then(Value::as_object).cloned())
        else {
            continue;
        };
        if let Some(placement) = app
            .get("newWorkspacePlacement")
            .and_then(Value::as_str)
            .and_then(WorkspacePlacement::parse)
        {
            settings.new_workspace_placement = placement;
        }
        if let Some(enabled) = app
            .get("workspaceInheritWorkingDirectory")
            .and_then(Value::as_bool)
        {
            settings.workspace_inherit_working_directory = enabled;
        }
        if let Some(enabled) = app
            .get("keepWorkspaceOpenWhenClosingLastSurface")
            .and_then(Value::as_bool)
        {
            settings.keep_workspace_open_when_closing_last_surface = enabled;
        }
        if let Some(policy) = app
            .get("confirmQuit")
            .and_then(Value::as_str)
            .and_then(ConfirmQuitPolicy::parse)
        {
            settings.confirm_quit = policy;
        } else if let Some(enabled) = app.get("warnBeforeQuit").and_then(Value::as_bool) {
            settings.confirm_quit = if enabled {
                ConfirmQuitPolicy::Always
            } else {
                ConfirmQuitPolicy::Never
            };
        }
        if let Some(enabled) = app.get("warnBeforeClosingTab").and_then(Value::as_bool) {
            settings.warn_before_closing_tab = enabled;
        }
        if let Some(enabled) = app
            .get("warnBeforeClosingTabXButton")
            .and_then(Value::as_bool)
        {
            settings.warn_before_closing_tab_x_button = enabled;
        }
        if let Some(enabled) = app.get("hideTabCloseButton").and_then(Value::as_bool) {
            settings.hide_tab_close_button = enabled;
        }
    }
    settings
}

fn set_app_workspace_setting_with_env(
    env: &ConfigEnvironment,
    key: &str,
    value: Value,
) -> Result<(), String> {
    let path = primary_cmux_json_path(env);
    let mut root = read_primary_cmux_json_for_write(&path)?;
    let app = root
        .entry("app".to_string())
        .or_insert_with(|| Value::Object(Map::new()));
    if !app.is_object() {
        *app = Value::Object(Map::new());
    }
    app.as_object_mut()
        .expect("app setting object")
        .insert(key.to_string(), value);
    write_primary_cmux_json(&path, &root)
}

fn canvas_settings_with_env(env: &ConfigEnvironment) -> CanvasSettingsPayload {
    let mut pane_gap = 16.0;
    let mut snapping_enabled = true;
    for path in cmux_json_config_paths(env) {
        let Some(canvas) = read_jsonc_object(&path)
            .and_then(|root| root.get("canvas").and_then(Value::as_object).cloned())
        else {
            continue;
        };
        if let Some(value) = canvas
            .get("paneGap")
            .and_then(Value::as_f64)
            .filter(|value| value.is_finite())
        {
            pane_gap = value.clamp(0.0, 64.0);
        }
        if let Some(value) = canvas.get("snappingEnabled").and_then(Value::as_bool) {
            snapping_enabled = value;
        }
    }
    CanvasSettingsPayload {
        pane_gap,
        snapping_enabled,
        snap_threshold: if snapping_enabled { 8.0 } else { 0.0 },
        min_pane_width: 200.0,
        min_pane_height: 120.0,
        path: primary_cmux_json_path(env).display().to_string(),
    }
}

#[cfg_attr(not(any(feature = "gtk", test)), allow(dead_code))]
fn set_canvas_setting_with_env(
    env: &ConfigEnvironment,
    key: &str,
    value: Value,
) -> Result<(), String> {
    let path = primary_cmux_json_path(env);
    let mut root = read_primary_cmux_json_for_write(&path)?;
    let canvas = root
        .entry("canvas".to_string())
        .or_insert_with(|| Value::Object(Map::new()));
    if !canvas.is_object() {
        *canvas = Value::Object(Map::new());
    }
    canvas
        .as_object_mut()
        .expect("canvas setting object")
        .insert(key.to_string(), value);
    write_primary_cmux_json(&path, &root)
}

pub fn workspace_group_config_for_cwd(anchor_cwd: Option<&str>) -> WorkspaceGroupConfig {
    let env = ConfigEnvironment::live();
    let anchor_cwd = anchor_cwd
        .map(|cwd| normalize_settings_path(cwd, &env, false))
        .filter(|cwd| !cwd.is_empty());
    let mut configured_default = None;
    let mut configured_for_cwd: Option<(usize, WorkspaceGroupConfig)> = None;

    for path in cmux_json_config_paths(&env) {
        let Some(root) = read_jsonc_object(&path) else {
            continue;
        };
        let Some(workspace_groups) = root.get("workspaceGroups").and_then(Value::as_object) else {
            continue;
        };
        if let Some(placement) = workspace_groups
            .get("newWorkspacePlacement")
            .and_then(valid_workspace_group_placement)
        {
            configured_default = Some(placement);
        }
        let Some(anchor_cwd) = anchor_cwd.as_deref() else {
            continue;
        };
        let Some(by_cwd) = workspace_groups.get("byCwd").and_then(Value::as_object) else {
            continue;
        };
        for (key, entry) in by_cwd {
            let Some(entry) = entry.as_object() else {
                continue;
            };
            let normalized_key =
                normalize_settings_path(key, &env, key.contains('*') || key.contains('?'));
            if workspace_group_cwd_key_matches(
                &normalized_key,
                anchor_cwd,
                key.contains('*') || key.contains('?'),
            ) {
                let score = normalized_key.len();
                let resolved = WorkspaceGroupConfig {
                    color: entry.get("color").and_then(trimmed_json_string),
                    icon_symbol: entry
                        .get("icon")
                        .or_else(|| entry.get("iconSymbol"))
                        .and_then(trimmed_json_string),
                    new_workspace_placement: entry
                        .get("newWorkspacePlacement")
                        .and_then(valid_workspace_group_placement),
                    context_menu_items: workspace_group_context_menu_items(
                        entry.get("contextMenu"),
                        root.get("actions"),
                        root.get("commands"),
                    ),
                };
                if configured_for_cwd
                    .as_ref()
                    .is_none_or(|(best_score, _)| score >= *best_score)
                {
                    configured_for_cwd = Some((score, resolved));
                }
            }
        }
    }

    let mut resolved = WorkspaceGroupConfig {
        color: None,
        icon_symbol: None,
        new_workspace_placement: configured_default,
        context_menu_items: Vec::new(),
    };
    if let Some((_, by_cwd)) = configured_for_cwd {
        if by_cwd.color.is_some() {
            resolved.color = by_cwd.color;
        }
        if by_cwd.icon_symbol.is_some() {
            resolved.icon_symbol = by_cwd.icon_symbol;
        }
        if by_cwd.new_workspace_placement.is_some() {
            resolved.new_workspace_placement = by_cwd.new_workspace_placement;
        }
        resolved.context_menu_items = by_cwd.context_menu_items;
    }
    resolved
}

pub fn doctor(paths: &[String]) -> Result<ConfigDoctorReport, String> {
    let env = ConfigEnvironment::live();
    let targets = if paths.is_empty() {
        default_config_doctor_targets(&env)?
    } else {
        paths
            .iter()
            .enumerate()
            .map(|(index, raw)| {
                let path = absolute_config_path(raw, &env)?;
                Ok(ConfigDoctorTarget {
                    label: format!("custom {}", index + 1),
                    display_path: abbreviated_path(&path, &env),
                    path,
                    missing_is_error: true,
                })
            })
            .collect::<Result<Vec<_>, String>>()?
    };
    let findings = targets
        .iter()
        .map(config_doctor_finding)
        .collect::<Vec<_>>();
    let error_count = findings
        .iter()
        .filter(|finding| finding.status == "error")
        .count();
    Ok(ConfigDoctorReport {
        ok: error_count == 0,
        error_count,
        findings,
        reload_command: "cmux reload-config".to_string(),
        docs_url: SETTINGS_DOCS_URL.to_string(),
        schema_url: SETTINGS_SCHEMA_URL.to_string(),
    })
}

pub fn print_doctor_text(report: &ConfigDoctorReport) {
    println!("cmux config doctor");
    for finding in &report.findings {
        println!(
            "{} {}: {}",
            finding.status.to_ascii_uppercase(),
            finding.label,
            finding.display_path
        );
        println!("  path: {}", finding.path);
        if let Some(bytes) = finding.bytes {
            println!("  bytes: {bytes}");
        }
        if !finding.keys.is_empty() {
            println!("  keys: {}", finding.keys.join(", "));
        }
        if let Some(message) = &finding.message {
            println!("  {message}");
        }
    }
    println!();
    println!("Docs: {}", report.docs_url);
    println!("Schema: {}", report.schema_url);
    println!("Reload: {}", report.reload_command);
}

pub fn print_settings_docs_text(payload: &SettingsDocsPayload) {
    println!("Config files:");
    println!("  primary: {}", payload.settings_files.primary);
    println!("  legacy config: {}", payload.settings_files.legacy);
    println!(
        "  {}: {}",
        MACOS_MIGRATION_FALLBACK_LABEL, payload.settings_files.fallback
    );
    println!();
    println!("Related (not cmux-owned, but cmux reads it for terminal behavior):");
    println!("  {}", payload.ghostty_config.path);
    println!("  {}", payload.ghostty_config.note);
    println!();
    println!("Docs:");
    println!("  {}", payload.docs_url);
    println!();
    println!("Schema:");
    println!("  {}", payload.schema_url);
    println!();
    println!("Before editing cmux.json:");
    println!("  {}", payload.backup);
    println!();
    println!("Reload after editing cmux.json or Ghostty config:");
    println!("  {}   ({})", payload.reload_command, payload.reload_scope);
}

pub fn print_text(snapshot: &ConfigSnapshot, validation: bool) {
    println!("Config files:");
    print_source_line("cmux", &snapshot.cmux);
    print_source_line("ghostty", &snapshot.ghostty);
    print_source_line("synced", &snapshot.synced);
    if snapshot.load_paths.is_empty() {
        println!("Load paths: none");
    } else {
        println!("Load paths:");
        for path in &snapshot.load_paths {
            println!("  - {path}");
        }
    }
    if validation {
        println!("Validation: OK");
    }
}

fn print_source_line(label: &str, source: &ConfigSourceSnapshot) {
    let state = if source.has_backing_file {
        "exists"
    } else {
        "missing"
    };
    println!("  {label}: {} ({state})", source.path);
}

fn default_config_doctor_targets(
    env: &ConfigEnvironment,
) -> Result<Vec<ConfigDoctorTarget>, String> {
    let primary = env.xdg_config_home.join("cmux/cmux.json");
    let mut targets = vec![ConfigDoctorTarget {
        label: "primary".to_string(),
        display_path: "~/.config/cmux/cmux.json".to_string(),
        path: primary.clone(),
        missing_is_error: false,
    }];

    if let Some(project_path) = find_project_config_path(env)? {
        if project_path != primary {
            targets.push(ConfigDoctorTarget {
                label: "project".to_string(),
                display_path: abbreviated_path(&project_path, env),
                path: project_path,
                missing_is_error: false,
            });
        }
    }

    let optional_paths = [
        (
            "legacy config",
            env.xdg_config_home.join("cmux/settings.json"),
        ),
        (
            MACOS_MIGRATION_FALLBACK_LABEL,
            env.app_support(RELEASE_BUNDLE_ID).join("settings.json"),
        ),
    ];
    for (label, path) in optional_paths {
        if path != primary
            && regular_file(&path)
            && !targets.iter().any(|target| target.path == path)
        {
            targets.push(ConfigDoctorTarget {
                label: label.to_string(),
                display_path: abbreviated_path(&path, env),
                path,
                missing_is_error: false,
            });
        }
    }
    Ok(targets)
}

fn find_project_config_path(env: &ConfigEnvironment) -> Result<Option<PathBuf>, String> {
    let mut current = std::env::current_dir()
        .map_err(|err| format!("failed to read current directory: {err}"))?
        .canonicalize()
        .unwrap_or_else(|_| PathBuf::from("."));
    let home = env.home.canonicalize().unwrap_or_else(|_| env.home.clone());
    loop {
        if current == home {
            return Ok(None);
        }
        for candidate in [current.join(".cmux/cmux.json"), current.join("cmux.json")] {
            if regular_file(&candidate) {
                return Ok(Some(candidate));
            }
        }
        let Some(parent) = current.parent().map(Path::to_path_buf) else {
            return Ok(None);
        };
        if parent == current {
            return Ok(None);
        }
        current = parent;
    }
}

fn cmux_json_config_paths(env: &ConfigEnvironment) -> Vec<PathBuf> {
    let mut paths = vec![
        env.app_support(RELEASE_BUNDLE_ID).join("settings.json"),
        env.xdg_config_home.join("cmux/settings.json"),
        primary_cmux_json_path(env),
    ];
    if let Ok(Some(project_path)) = find_project_config_path(env) {
        if !paths.iter().any(|path| path == &project_path) {
            paths.push(project_path);
        }
    }
    paths
        .into_iter()
        .filter(|path| regular_file(path))
        .collect()
}

fn primary_cmux_json_path(env: &ConfigEnvironment) -> PathBuf {
    env.xdg_config_home.join("cmux/cmux.json")
}

fn read_jsonc_object(path: &Path) -> Option<serde_json::Map<String, Value>> {
    let contents = fs::read_to_string(path).ok()?;
    let preprocessed = preprocess_jsonc(&contents).ok()?;
    match serde_json::from_str::<Value>(&preprocessed).ok()? {
        Value::Object(object) => Some(object),
        _ => None,
    }
}

fn read_primary_cmux_json_for_write(path: &Path) -> Result<Map<String, Value>, String> {
    let Ok(contents) = fs::read_to_string(path) else {
        return Ok(Map::new());
    };
    if contents.trim().is_empty() {
        return Ok(Map::new());
    }
    let preprocessed = preprocess_jsonc(&contents)
        .map_err(|err| format!("failed to parse {}: {err}", path.display()))?;
    match serde_json::from_str::<Value>(&preprocessed)
        .map_err(|err| format!("{} is not valid JSON: {err}", path.display()))?
    {
        Value::Object(object) => Ok(object),
        _ => Err(format!("{} must contain a JSON object", path.display())),
    }
}

fn write_primary_cmux_json(path: &Path, root: &Map<String, Value>) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|err| {
            format!(
                "failed to create config directory {}: {err}",
                parent.display()
            )
        })?;
    }
    let text = serde_json::to_string_pretty(&Value::Object(root.clone()))
        .map_err(|err| format!("failed to encode {}: {err}", path.display()))?
        + "\n";
    fs::write(path, text)
        .map_err(|err| format!("failed to write cmux config {}: {err}", path.display()))
}

fn valid_workspace_group_placement(value: &Value) -> Option<String> {
    let value = value.as_str()?.trim();
    match value {
        "afterCurrent" | "after-current" => Some("afterCurrent".to_string()),
        "top" => Some("top".to_string()),
        "end" => Some("end".to_string()),
        _ => None,
    }
}

fn trimmed_json_string(value: &Value) -> Option<String> {
    value
        .as_str()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToString::to_string)
}

#[derive(Debug, Clone)]
struct WorkspaceGroupActionMetadata {
    id: String,
    title: String,
    tooltip: Option<String>,
    icon_symbol: Option<String>,
    action: Value,
}

fn workspace_group_context_menu_items(
    configured: Option<&Value>,
    actions: Option<&Value>,
    commands: Option<&Value>,
) -> Vec<Value> {
    let Some(configured_items) = configured.and_then(Value::as_array) else {
        return Vec::new();
    };
    let action_lookup = workspace_group_action_lookup(actions, commands);
    let mut resolved = Vec::new();
    let mut last_was_separator = false;

    for (index, item) in configured_items.iter().enumerate() {
        if workspace_group_context_menu_separator(item) {
            if resolved.is_empty() || last_was_separator {
                continue;
            }
            resolved.push(json!({
                "type": "separator",
                "id": format!("workspaceGroups.contextMenu.separator.{index}")
            }));
            last_was_separator = true;
            continue;
        }

        let Some((raw_action_id, title_override, tooltip_override, icon_override)) =
            workspace_group_context_menu_action(item)
        else {
            continue;
        };
        let lookup_id =
            canonical_workspace_group_action_id(&raw_action_id).unwrap_or(raw_action_id.as_str());
        let Some(action) = action_lookup.get(lookup_id) else {
            continue;
        };
        let title = title_override.unwrap_or_else(|| action.title.clone());
        let tooltip = tooltip_override.or_else(|| action.tooltip.clone());
        let icon_symbol = icon_override.or_else(|| action.icon_symbol.clone());
        resolved.push(json!({
            "type": "action",
            "id": format!("workspaceGroups.contextMenu.{index}.{}", action.id),
            "action_id": action.id,
            "title": title,
            "tooltip": tooltip,
            "icon_symbol": icon_symbol,
            "action": action.action
        }));
        last_was_separator = false;
    }

    resolved
}

fn workspace_group_context_menu_separator(item: &Value) -> bool {
    if let Some(text) = item.as_str().map(str::trim) {
        return text == "-" || text.eq_ignore_ascii_case("separator");
    }
    item.as_object()
        .and_then(|object| object.get("type"))
        .and_then(Value::as_str)
        .is_some_and(|kind| kind.trim().eq_ignore_ascii_case("separator"))
}

fn workspace_group_context_menu_action(
    item: &Value,
) -> Option<(String, Option<String>, Option<String>, Option<String>)> {
    if item.as_str().is_some() {
        return trimmed_json_string(item).map(|action| (action, None, None, None));
    }
    let object = item.as_object()?;
    let action = object.get("action").and_then(trimmed_json_string)?;
    let title = object.get("title").and_then(trimmed_json_string);
    let tooltip = object
        .get("tooltip")
        .or_else(|| object.get("help"))
        .and_then(trimmed_json_string);
    let icon = object.get("icon").and_then(workspace_group_icon_symbol);
    Some((action, title, tooltip, icon))
}

fn workspace_group_action_lookup(
    actions: Option<&Value>,
    commands: Option<&Value>,
) -> HashMap<String, WorkspaceGroupActionMetadata> {
    let mut lookup = HashMap::new();
    let command_lookup = workspace_group_workspace_command_lookup(commands);
    for action in workspace_group_builtin_actions() {
        lookup.insert(action.id.clone(), action);
    }

    if let Some(actions) = actions.and_then(Value::as_object) {
        for (raw_id, definition) in actions {
            let Some(raw_id) = non_empty_str(raw_id) else {
                continue;
            };
            let registry_id = canonical_workspace_group_action_id(raw_id)
                .unwrap_or(raw_id)
                .to_string();
            let existing = lookup.get(&registry_id);
            if let Some(action) = workspace_group_action_from_definition(
                &registry_id,
                definition,
                existing,
                &command_lookup,
            ) {
                lookup.insert(action.id.clone(), action);
            }
        }
    }

    if let Some(commands) = commands.and_then(Value::as_array) {
        for command in commands {
            if let Some(action) = workspace_group_command_action(command) {
                lookup.entry(action.id.clone()).or_insert(action);
            }
        }
    }

    lookup
}

fn workspace_group_action_from_definition(
    fallback_id: &str,
    definition: &Value,
    existing: Option<&WorkspaceGroupActionMetadata>,
    commands: &HashMap<String, Value>,
) -> Option<WorkspaceGroupActionMetadata> {
    let object = definition.as_object()?;
    let mut id = fallback_id.to_string();
    let mut action = existing.map(|existing| existing.action.clone());
    let mut default_title = existing.map(|existing| existing.title.clone());
    let mut default_icon = existing.and_then(|existing| existing.icon_symbol.clone());
    let mut default_tooltip = existing.and_then(|existing| existing.tooltip.clone());

    if let Some(raw_builtin) = object.get("builtin").and_then(trimmed_json_string) {
        let builtin = workspace_group_builtin_action(&raw_builtin)?;
        id = builtin.id;
        action = Some(builtin.action);
        default_title = Some(builtin.title);
        default_icon = builtin.icon_symbol;
        default_tooltip = builtin.tooltip;
    } else if let Some(command) = object.get("command").and_then(trimmed_json_string) {
        action = Some(json!({
            "kind": "command",
            "command": command
        }));
    } else if let Some(command_name) = object
        .get("commandName")
        .or_else(|| object.get("name"))
        .and_then(trimmed_json_string)
    {
        let mut payload = json!({
            "kind": "workspace_command",
            "command_name": command_name
        });
        if let Some(workspace) = commands.get(&command_name) {
            payload["workspace"] = workspace.clone();
        }
        action = Some(payload);
    } else if let Some(agent) = object.get("agent").and_then(trimmed_json_string) {
        let mut payload = json!({
            "kind": "agent",
            "agent": agent
        });
        if let Some(args) = object.get("args").and_then(trimmed_json_string) {
            payload["args"] = json!(args);
        }
        action = Some(payload);
    }

    let action = action?;
    let title = object
        .get("title")
        .or_else(|| object.get("tooltip"))
        .and_then(trimmed_json_string)
        .or(default_title)
        .unwrap_or_else(|| id.clone());
    let tooltip = object
        .get("tooltip")
        .or_else(|| object.get("description"))
        .and_then(trimmed_json_string)
        .or(default_tooltip);
    let icon_symbol = object
        .get("icon")
        .and_then(workspace_group_icon_symbol)
        .or(default_icon);

    Some(WorkspaceGroupActionMetadata {
        id,
        title,
        tooltip,
        icon_symbol,
        action,
    })
}

fn workspace_group_command_action(command: &Value) -> Option<WorkspaceGroupActionMetadata> {
    let object = command.as_object()?;
    let name = object.get("name").and_then(trimmed_json_string)?;
    let id = format!(
        "cmux.config.command.{}",
        workspace_group_percent_encode_id(&name)
    );
    let description = object.get("description").and_then(trimmed_json_string);
    let workspace = object.get("workspace").cloned();
    let workspace_command = workspace.is_some();
    let command_text = object.get("command").and_then(trimmed_json_string);
    let action = if let Some(workspace) = workspace {
        json!({
            "kind": "workspace_command",
            "command_name": name,
            "workspace": workspace
        })
    } else {
        json!({
            "kind": "command",
            "command": command_text?
        })
    };
    Some(WorkspaceGroupActionMetadata {
        id,
        title: format!("Custom: {name}"),
        tooltip: description,
        icon_symbol: Some(if workspace_command {
            "rectangle.stack.badge.plus".to_string()
        } else {
            "terminal".to_string()
        }),
        action,
    })
}

fn workspace_group_workspace_command_lookup(commands: Option<&Value>) -> HashMap<String, Value> {
    let mut lookup = HashMap::new();
    if let Some(commands) = commands.and_then(Value::as_array) {
        for command in commands {
            let Some(object) = command.as_object() else {
                continue;
            };
            let Some(name) = object.get("name").and_then(trimmed_json_string) else {
                continue;
            };
            let Some(workspace) = object.get("workspace").cloned() else {
                continue;
            };
            lookup.entry(name).or_insert(workspace);
        }
    }
    lookup
}

fn workspace_group_builtin_actions() -> Vec<WorkspaceGroupActionMetadata> {
    [
        (
            "cmux.newWorkspace",
            "New Workspace",
            "plus.square",
            json!({"kind": "builtin", "builtin": "cmux.newWorkspace"}),
        ),
        (
            "cmux.cloudvm",
            "Start Cloud VM",
            "cloud",
            json!({"kind": "builtin", "builtin": "cmux.cloudvm"}),
        ),
        (
            "cmux.newTerminal",
            "New Terminal Tab",
            "terminal",
            json!({"kind": "builtin", "builtin": "cmux.newTerminal"}),
        ),
        (
            "cmux.newBrowser",
            "New Browser Tab",
            "globe",
            json!({"kind": "builtin", "builtin": "cmux.newBrowser"}),
        ),
        (
            "cmux.splitRight",
            "Split Right",
            "square.split.2x1",
            json!({"kind": "builtin", "builtin": "cmux.splitRight"}),
        ),
        (
            "cmux.splitDown",
            "Split Down",
            "square.split.1x2",
            json!({"kind": "builtin", "builtin": "cmux.splitDown"}),
        ),
    ]
    .into_iter()
    .map(|(id, title, icon, action)| WorkspaceGroupActionMetadata {
        id: id.to_string(),
        title: title.to_string(),
        tooltip: None,
        icon_symbol: Some(icon.to_string()),
        action,
    })
    .collect()
}

fn workspace_group_builtin_action(raw_id: &str) -> Option<WorkspaceGroupActionMetadata> {
    let id = canonical_workspace_group_action_id(raw_id)?;
    workspace_group_builtin_actions()
        .into_iter()
        .find(|action| action.id == id)
}

fn canonical_workspace_group_action_id(raw_id: &str) -> Option<&'static str> {
    match raw_id.trim() {
        "cmux.newWorkspace" | "newWorkspace" => Some("cmux.newWorkspace"),
        "cmux.cloudvm" | "cmux.cloudVM" | "cloudVM" | "cloudvm" | "cmux.newCloudVM"
        | "cmux.newCloudVm" | "newCloudVM" | "newCloudVm" | "cmux.startCloudVM"
        | "cmux.startCloudVm" | "startCloudVM" | "startCloudVm" => Some("cmux.cloudvm"),
        "cmux.newTerminal" | "newTerminal" => Some("cmux.newTerminal"),
        "cmux.newBrowser" | "newBrowser" => Some("cmux.newBrowser"),
        "cmux.splitRight" | "splitRight" => Some("cmux.splitRight"),
        "cmux.splitDown" | "splitDown" => Some("cmux.splitDown"),
        _ => None,
    }
}

fn workspace_group_icon_symbol(value: &Value) -> Option<String> {
    if let Some(symbol) = trimmed_json_string(value) {
        return Some(symbol);
    }
    let object = value.as_object()?;
    let icon_type = object
        .get("type")
        .and_then(Value::as_str)
        .map(str::trim)
        .unwrap_or("symbol");
    if !matches!(icon_type, "symbol" | "sfSymbol" | "systemImage") {
        return None;
    }
    object
        .get("name")
        .or_else(|| object.get("symbol"))
        .and_then(trimmed_json_string)
}

fn workspace_group_percent_encode_id(value: &str) -> String {
    let mut out = String::new();
    for byte in value.as_bytes() {
        if byte.is_ascii_alphanumeric() {
            out.push(*byte as char);
        } else {
            out.push_str(&format!("%{byte:02X}"));
        }
    }
    out
}

fn non_empty_str(value: &str) -> Option<&str> {
    let value = value.trim();
    (!value.is_empty()).then_some(value)
}

fn normalize_settings_path(raw: &str, env: &ConfigEnvironment, preserve_glob: bool) -> String {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return String::new();
    }
    let expanded = if let Some(suffix) = trimmed.strip_prefix('~') {
        if suffix.is_empty() {
            env.home.display().to_string()
        } else if suffix.starts_with('/') {
            format!("{}{}", env.home.display(), suffix)
        } else {
            trimmed.to_string()
        }
    } else {
        trimmed.to_string()
    };
    if preserve_glob {
        return expanded;
    }
    let mut normalized = Path::new(&expanded)
        .components()
        .collect::<PathBuf>()
        .display()
        .to_string();
    while normalized.len() > 1 && normalized.ends_with('/') {
        normalized.pop();
    }
    normalized
}

fn workspace_group_cwd_key_matches(key: &str, cwd: &str, is_glob: bool) -> bool {
    if key.is_empty() {
        return false;
    }
    if is_glob {
        return fnmatch_style(key, cwd);
    }
    if cwd == key {
        return true;
    }
    if key == "/" {
        return cwd.starts_with('/');
    }
    cwd.strip_prefix(key)
        .is_some_and(|suffix| suffix.starts_with('/'))
}

fn fnmatch_style(pattern: &str, candidate: &str) -> bool {
    let pattern = pattern.chars().collect::<Vec<_>>();
    let candidate = candidate.chars().collect::<Vec<_>>();
    let mut pattern_index = 0usize;
    let mut candidate_index = 0usize;
    let mut star_pattern_index = None;
    let mut star_candidate_index = 0usize;

    while candidate_index < candidate.len() {
        if pattern_index < pattern.len()
            && (pattern[pattern_index] == '?'
                || pattern[pattern_index] == candidate[candidate_index])
        {
            pattern_index += 1;
            candidate_index += 1;
        } else if pattern_index < pattern.len() && pattern[pattern_index] == '*' {
            star_pattern_index = Some(pattern_index);
            star_candidate_index = candidate_index;
            pattern_index += 1;
        } else if let Some(star_index) = star_pattern_index {
            pattern_index = star_index + 1;
            star_candidate_index += 1;
            candidate_index = star_candidate_index;
        } else {
            return false;
        }
    }
    while pattern_index < pattern.len() && pattern[pattern_index] == '*' {
        pattern_index += 1;
    }
    pattern_index == pattern.len()
}

fn config_doctor_finding(target: &ConfigDoctorTarget) -> ConfigDoctorFinding {
    let path_text = target.path.display().to_string();
    let metadata = match fs::metadata(&target.path) {
        Ok(metadata) => metadata,
        Err(_) => {
            let status = if target.missing_is_error {
                "error"
            } else {
                "missing"
            };
            return ConfigDoctorFinding {
                label: target.label.clone(),
                display_path: target.display_path.clone(),
                path: path_text,
                status: status.to_string(),
                ok: status != "error",
                keys: Vec::new(),
                message: Some(if target.missing_is_error {
                    "file not found".to_string()
                } else {
                    "not found; cmux will use defaults until this file exists".to_string()
                }),
                bytes: None,
            };
        }
    };
    if metadata.is_dir() {
        return ConfigDoctorFinding {
            label: target.label.clone(),
            display_path: target.display_path.clone(),
            path: path_text,
            status: "error".to_string(),
            ok: false,
            keys: Vec::new(),
            message: Some("path is a directory, expected a file".to_string()),
            bytes: None,
        };
    }

    let bytes = match fs::read(&target.path) {
        Ok(bytes) => bytes,
        Err(err) => {
            return ConfigDoctorFinding {
                label: target.label.clone(),
                display_path: target.display_path.clone(),
                path: path_text,
                status: "error".to_string(),
                ok: false,
                keys: Vec::new(),
                message: Some(err.to_string()),
                bytes: None,
            };
        }
    };
    if bytes.is_empty() {
        return ConfigDoctorFinding {
            label: target.label.clone(),
            display_path: target.display_path.clone(),
            path: path_text,
            status: "error".to_string(),
            ok: false,
            keys: Vec::new(),
            message: Some("file is empty".to_string()),
            bytes: Some(0),
        };
    }

    let parsed = std::str::from_utf8(&bytes)
        .map_err(|err| err.to_string())
        .and_then(preprocess_jsonc)
        .and_then(|text| {
            serde_json::from_str::<serde_json::Value>(&text).map_err(|err| err.to_string())
        });
    match parsed {
        Ok(serde_json::Value::Object(object)) => {
            let mut keys = object.keys().cloned().collect::<Vec<_>>();
            keys.sort();
            ConfigDoctorFinding {
                label: target.label.clone(),
                display_path: target.display_path.clone(),
                path: path_text,
                status: "ok".to_string(),
                ok: true,
                keys,
                message: Some("JSONC syntax is valid".to_string()),
                bytes: Some(bytes.len()),
            }
        }
        Ok(_) => ConfigDoctorFinding {
            label: target.label.clone(),
            display_path: target.display_path.clone(),
            path: path_text,
            status: "error".to_string(),
            ok: false,
            keys: Vec::new(),
            message: Some("top-level value must be a JSON object".to_string()),
            bytes: Some(bytes.len()),
        },
        Err(message) => ConfigDoctorFinding {
            label: target.label.clone(),
            display_path: target.display_path.clone(),
            path: path_text,
            status: "error".to_string(),
            ok: false,
            keys: Vec::new(),
            message: Some(message),
            bytes: None,
        },
    }
}

fn available_theme_names_with_env(env: &ConfigEnvironment) -> Vec<String> {
    let mut themes = Vec::new();
    let mut seen = HashSet::new();
    for dir in theme_directories(env) {
        let Ok(entries) = fs::read_dir(dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                continue;
            }
            if !path.is_file() {
                continue;
            }
            let Some(name) = path.file_name().and_then(|value| value.to_str()) else {
                continue;
            };
            let folded = name.to_ascii_lowercase();
            if seen.insert(folded) {
                themes.push(name.to_string());
            }
        }
    }
    themes.sort_by_key(|name| name.to_ascii_lowercase());
    themes
}

fn theme_directories(env: &ConfigEnvironment) -> Vec<PathBuf> {
    let mut dirs = Vec::new();
    let mut seen = HashSet::new();
    let mut push = |path: PathBuf| {
        let canonical = fs::canonicalize(&path).unwrap_or_else(|_| path.clone());
        if canonical.is_dir() && seen.insert(canonical) {
            dirs.push(path);
        }
    };
    if let Some(resources) = std::env::var("GHOSTTY_RESOURCES_DIR")
        .ok()
        .filter(|value| !value.trim().is_empty())
    {
        push(PathBuf::from(resources).join("themes"));
    }
    push(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../Resources/ghostty/themes"));
    if let Ok(data_dirs) = std::env::var("XDG_DATA_DIRS") {
        for data_dir in data_dirs.split(':').filter(|value| !value.is_empty()) {
            push(PathBuf::from(data_dir).join("ghostty/themes"));
        }
    }
    push(env.xdg_config_home.join("ghostty/themes"));
    push(env.home.join(".local/share/ghostty/themes"));
    push(
        env.home
            .join("Library/Application Support/com.mitchellh.ghostty/themes"),
    );
    dirs
}

fn current_theme_selection(env: &ConfigEnvironment) -> ThemeSelection {
    let mut raw_value = None;
    let mut source_path = None;
    let mut sources = Vec::new();
    let ghostty = ghostty_config_path(env);
    if regular_file(&ghostty) {
        sources.push(ghostty);
    }
    let cmux = cmux_config_path(env);
    if regular_file(&cmux) || !sources.iter().any(|path| path == &cmux) {
        sources.push(cmux);
    }
    for path in sources {
        let Ok(contents) = fs::read_to_string(&path) else {
            continue;
        };
        if let Some(value) = last_theme_directive(&contents) {
            raw_value = Some(value);
            source_path = Some(path.display().to_string());
        }
    }
    parse_theme_selection(raw_value, source_path)
}

fn last_theme_directive(contents: &str) -> Option<String> {
    let mut last = None;
    for line in contents.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        let Some((key, value)) = trimmed.split_once('=') else {
            continue;
        };
        if key.trim() != "theme" {
            continue;
        }
        let value = value.trim().trim_matches('"').to_string();
        if !value.is_empty() {
            last = Some(value);
        }
    }
    last
}

fn parse_theme_selection(raw_value: Option<String>, source_path: Option<String>) -> ThemeSelection {
    let Some(raw_value) = raw_value
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
    else {
        return ThemeSelection {
            raw_value: None,
            light: None,
            dark: None,
            source_path,
        };
    };

    let mut fallback = None;
    let mut light = None;
    let mut dark = None;
    for token in raw_value.split(',') {
        let entry = token.trim();
        if entry.is_empty() {
            continue;
        }
        if let Some((key, value)) = entry.split_once(':') {
            let value = value.trim();
            if value.is_empty() {
                continue;
            }
            match key.trim().to_ascii_lowercase().as_str() {
                "light" if light.is_none() => light = Some(value.to_string()),
                "dark" if dark.is_none() => dark = Some(value.to_string()),
                _ if fallback.is_none() => fallback = Some(value.to_string()),
                _ => {}
            }
        } else if fallback.is_none() {
            fallback = Some(entry.to_string());
        }
    }
    let resolved_light = light.or_else(|| fallback.clone());
    let resolved_dark = dark.or_else(|| fallback.clone());
    ThemeSelection {
        raw_value: Some(raw_value),
        light: resolved_light,
        dark: resolved_dark,
        source_path,
    }
}

fn validated_theme_name(raw: &str, available: &[String]) -> Result<String, String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Err("Theme name cannot be empty".to_string());
    }
    if let Some(matched) = available
        .iter()
        .find(|theme| theme.eq_ignore_ascii_case(trimmed))
    {
        return Ok(matched.clone());
    }
    if available.is_empty() {
        return Ok(trimmed.to_string());
    }
    Err(format!(
        "Unknown theme '{trimmed}'. Run 'cmux themes' to list available themes."
    ))
}

fn encoded_theme_value(light: Option<&str>, dark: Option<&str>) -> Option<String> {
    let light = light.map(str::trim).filter(|value| !value.is_empty());
    let dark = dark.map(str::trim).filter(|value| !value.is_empty());
    match (light, dark) {
        (Some(light), Some(dark)) => Some(format!("light:{light},dark:{dark}")),
        (Some(light), None) => Some(format!("light:{light}")),
        (None, Some(dark)) => Some(format!("dark:{dark}")),
        (None, None) => None,
    }
}

fn write_managed_theme_override(
    env: &ConfigEnvironment,
    raw_value: &str,
) -> Result<PathBuf, String> {
    let path = cmux_config_path(env);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|err| {
            format!(
                "failed to create config directory {}: {err}",
                parent.display()
            )
        })?;
    }
    let existing = fs::read_to_string(&path).unwrap_or_default();
    let stripped = remove_managed_theme_override(&existing).trim().to_string();
    let block = format!("{CMUX_THEMES_BLOCK_START}\ntheme = {raw_value}\n{CMUX_THEMES_BLOCK_END}");
    let next = if stripped.is_empty() {
        format!("{block}\n")
    } else {
        format!("{stripped}\n\n{block}\n")
    };
    fs::write(&path, next)
        .map_err(|err| format!("failed to write theme config {}: {err}", path.display()))?;
    Ok(path)
}

fn clear_managed_theme_override(env: &ConfigEnvironment) -> Result<PathBuf, String> {
    let path = cmux_config_path(env);
    let Ok(existing) = fs::read_to_string(&path) else {
        return Ok(path);
    };
    let stripped = remove_managed_theme_override(&existing).trim().to_string();
    if stripped.is_empty() {
        if let Err(err) = fs::remove_file(&path) {
            if err.kind() != std::io::ErrorKind::NotFound {
                return Err(format!(
                    "failed to remove theme config {}: {err}",
                    path.display()
                ));
            }
        }
    } else {
        fs::write(&path, format!("{stripped}\n"))
            .map_err(|err| format!("failed to write theme config {}: {err}", path.display()))?;
    }
    Ok(path)
}

#[derive(Clone, Copy)]
struct FontSizeDescriptor {
    key: &'static str,
    default_value: f64,
    min: f64,
    max: f64,
}

fn font_size_descriptor(key: &str) -> Option<FontSizeDescriptor> {
    match key {
        SIDEBAR_FONT_SIZE_KEY => Some(FontSizeDescriptor {
            key: SIDEBAR_FONT_SIZE_KEY,
            default_value: 12.5,
            min: 10.0,
            max: 20.0,
        }),
        SURFACE_TAB_BAR_FONT_SIZE_KEY => Some(FontSizeDescriptor {
            key: SURFACE_TAB_BAR_FONT_SIZE_KEY,
            default_value: 11.0,
            min: 8.0,
            max: 14.0,
        }),
        _ => None,
    }
}

fn effective_font_size_value(
    env: &ConfigEnvironment,
    cmux_path: &Path,
    descriptor: FontSizeDescriptor,
) -> Option<f64> {
    let ghostty_path = ghostty_config_path(env);
    let mut sources = Vec::new();
    if regular_file(&ghostty_path) {
        sources.push(ghostty_path);
    }
    if regular_file(cmux_path) || !sources.iter().any(|path| path == cmux_path) {
        sources.push(cmux_path.to_path_buf());
    }
    sources
        .into_iter()
        .filter_map(|path| parsed_font_size_from_file(&path, descriptor))
        .last()
}

fn parsed_font_size_from_file(path: &Path, descriptor: FontSizeDescriptor) -> Option<f64> {
    let contents = fs::read_to_string(path).ok()?;
    parsed_font_size(&contents, descriptor)
}

fn parsed_font_size(contents: &str, descriptor: FontSizeDescriptor) -> Option<f64> {
    parsed_config_value(contents, descriptor.key)
        .and_then(|raw| parse_font_size_value(&raw))
        .map(|value| clamp_font_size(value, descriptor))
}

fn parsed_config_value(contents: &str, key: &str) -> Option<String> {
    let mut latest = None;
    for line in contents.lines() {
        let Some((parsed_key, mut value, quoted)) = parse_config_line(line) else {
            continue;
        };
        if parsed_key != key {
            continue;
        }
        if !quoted {
            value = strip_inline_comment(&value);
        }
        if value.len() >= 2 && value.starts_with('"') && value.ends_with('"') {
            value.remove(0);
            value.pop();
        }
        latest = Some(value);
    }
    latest
}

fn parse_font_size_value(raw: &str) -> Option<f64> {
    let value = raw.trim().trim_matches('"').parse::<f64>().ok()?;
    value.is_finite().then_some(value)
}

fn clamp_font_size(value: f64, descriptor: FontSizeDescriptor) -> f64 {
    value.max(descriptor.min).min(descriptor.max)
}

fn format_font_size(value: f64) -> String {
    let scaled = (value * 100.0).round() as i64;
    let whole = scaled / 100;
    let fraction = (scaled % 100).abs();
    if fraction == 0 {
        return whole.to_string();
    }
    if fraction % 10 == 0 {
        return format!("{}.{}", whole, fraction / 10);
    }
    format!("{}.{:02}", whole, fraction)
}

fn write_font_size_setting(
    env: &ConfigEnvironment,
    key: &str,
    formatted_value: &str,
) -> Result<PathBuf, String> {
    let path = cmux_config_path(env);
    let write_path = fs::read_link(&path)
        .map(|target| {
            if target.is_absolute() {
                target
            } else {
                path.parent().unwrap_or_else(|| Path::new(".")).join(target)
            }
        })
        .unwrap_or_else(|_| path.clone());
    if let Some(parent) = write_path.parent() {
        fs::create_dir_all(parent).map_err(|err| {
            format!(
                "failed to create config directory {}: {err}",
                parent.display()
            )
        })?;
    }
    let existing = fs::read_to_string(&write_path)
        .or_else(|_| fs::read_to_string(&path))
        .unwrap_or_default();
    let next = updated_config_contents(&existing, key, formatted_value);
    fs::write(&write_path, next).map_err(|err| {
        format!(
            "failed to write config setting {}: {err}",
            write_path.display()
        )
    })?;
    Ok(path)
}

fn updated_config_contents(contents: &str, key: &str, value: &str) -> String {
    let mut lines = contents
        .split('\n')
        .map(ToString::to_string)
        .collect::<Vec<_>>();
    if contents.ends_with('\n') {
        lines.pop();
    }
    if lines.len() == 1 && lines[0].is_empty() {
        lines.clear();
    }

    let mut did_replace = false;
    for line in &mut lines {
        let Some((parsed_key, _, _)) = parse_config_line(line) else {
            continue;
        };
        if parsed_key != key {
            continue;
        }
        *line = format!("{key} = {value}");
        did_replace = true;
    }
    if !did_replace {
        lines.push(format!("{key} = {value}"));
    }
    format!("{}\n", lines.join("\n"))
}

fn remove_managed_theme_override(contents: &str) -> String {
    let mut out = Vec::new();
    let mut in_block = false;
    for line in contents.lines() {
        if line.trim() == CMUX_THEMES_BLOCK_START {
            in_block = true;
            continue;
        }
        if in_block {
            if line.trim() == CMUX_THEMES_BLOCK_END {
                in_block = false;
            }
            continue;
        }
        out.push(line);
    }
    out.join("\n")
}

fn source_snapshot(
    path: PathBuf,
    is_editable: bool,
    env: &ConfigEnvironment,
) -> ConfigSourceSnapshot {
    ConfigSourceSnapshot {
        display_paths: vec![abbreviated_path(&path, env)],
        contents: fs::read_to_string(&path).unwrap_or_default(),
        has_backing_file: regular_file(&path),
        is_editable,
        path: path.display().to_string(),
    }
}

impl ConfigEnvironment {
    fn live() -> Self {
        let home = std::env::var("HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|_| PathBuf::from("."));
        let xdg_config_home = std::env::var("XDG_CONFIG_HOME")
            .ok()
            .filter(|value| !value.trim().is_empty())
            .map(PathBuf::from)
            .unwrap_or_else(|| home.join(".config"));
        let xdg_cache_home = std::env::var("XDG_CACHE_HOME")
            .ok()
            .filter(|value| !value.trim().is_empty())
            .map(PathBuf::from)
            .unwrap_or_else(|| home.join(".cache"));
        let bundle_id = std::env::var("CMUX_BUNDLE_IDENTIFIER")
            .ok()
            .filter(|value| !value.trim().is_empty())
            .unwrap_or_else(|| RELEASE_BUNDLE_ID.to_string());
        Self {
            home,
            xdg_config_home,
            xdg_cache_home,
            bundle_id,
        }
    }

    fn app_support(&self, bundle_id: &str) -> PathBuf {
        self.home
            .join("Library")
            .join("Application Support")
            .join(bundle_id)
    }
}

fn cmux_config_path(env: &ConfigEnvironment) -> PathBuf {
    if let Some(path) = std::env::var("CMUX_CONFIG_PATH")
        .ok()
        .filter(|value| !value.trim().is_empty())
        .map(PathBuf::from)
    {
        return path;
    }

    let linux_dir = env.xdg_config_home.join("cmux");
    let linux_config = linux_dir.join("config.ghostty");
    let linux_legacy = linux_dir.join("config");
    let mac_current = env.app_support(&env.bundle_id).join("config.ghostty");
    let mac_release = env.app_support(RELEASE_BUNDLE_ID).join("config.ghostty");
    let mac_legacy = env.app_support(RELEASE_BUNDLE_ID).join("config");

    let current_is_release = env.bundle_id == RELEASE_BUNDLE_ID;
    let active_candidates = if current_is_release {
        vec![
            linux_config.clone(),
            linux_legacy,
            mac_release.clone(),
            mac_legacy,
        ]
    } else {
        vec![
            linux_config.clone(),
            linux_legacy,
            mac_current.clone(),
            mac_release,
            mac_legacy,
        ]
    };

    active_candidates
        .into_iter()
        .find(|path| non_empty_regular_file(path))
        .unwrap_or(linux_config)
}

fn cmux_load_paths(_env: &ConfigEnvironment, cmux_path: &Path) -> Vec<PathBuf> {
    non_empty_regular_file(cmux_path)
        .then(|| cmux_path.to_path_buf())
        .into_iter()
        .collect()
}

fn ghostty_config_path(env: &ConfigEnvironment) -> PathBuf {
    let mac_dir = env
        .home
        .join("Library")
        .join("Application Support")
        .join("com.mitchellh.ghostty");
    let candidates = [
        env.xdg_config_home.join("ghostty/config"),
        env.xdg_config_home.join("ghostty/config.ghostty"),
        mac_dir.join("config"),
        mac_dir.join("config.ghostty"),
    ];
    candidates
        .iter()
        .find(|path| regular_file(path))
        .cloned()
        .unwrap_or_else(|| candidates[0].clone())
}

fn editor_paths(
    env: &ConfigEnvironment,
    cmux_path: &Path,
    ghostty_path: &Path,
    load_paths: &[PathBuf],
) -> Vec<PathBuf> {
    let mut collector = PathCollector::default();
    collector.append(cmux_path.to_path_buf());
    if regular_file(ghostty_path) {
        collector.append(ghostty_path.to_path_buf());
    }
    for path in load_paths {
        collector.append(path.clone());
    }
    collector.append_recursive_includes(&env.home);
    collector.paths
}

fn render_synced_preview(
    ghostty_path: Option<&Path>,
    cmux_paths: &[PathBuf],
    env: &ConfigEnvironment,
) -> String {
    let mut entries: HashMap<String, ParsedEntry> = HashMap::new();
    let mut keys = Vec::new();
    let mut sources = Vec::new();
    if let Some(path) = ghostty_path {
        sources.push(path.to_path_buf());
    }
    sources.extend(cmux_paths.iter().cloned());

    for source in sources {
        for entry in parsed_entries(&source) {
            if !entries.contains_key(&entry.key) {
                keys.push(entry.key.clone());
            }
            entries.insert(entry.key.clone(), entry);
        }
    }

    keys.into_iter()
        .filter_map(|key| entries.get(&key).cloned())
        .map(|entry| {
            format!(
                "{} = {}  # from: {}:{}",
                entry.key,
                entry.value,
                abbreviated_path(&entry.source, env),
                entry.line_number
            )
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn parsed_entries(path: &Path) -> Vec<ParsedEntry> {
    let Ok(contents) = fs::read_to_string(path) else {
        return Vec::new();
    };
    contents
        .lines()
        .enumerate()
        .filter_map(|(index, line)| {
            let trimmed = line.trim();
            if trimmed.is_empty() || trimmed.starts_with('#') {
                return None;
            }
            let (key, value) = trimmed.split_once('=')?;
            let key = key.trim();
            if key.is_empty() {
                return None;
            }
            Some(ParsedEntry {
                key: key.to_string(),
                value: value.trim().to_string(),
                source: path.to_path_buf(),
                line_number: index + 1,
            })
        })
        .collect()
}

fn materialize_synced_preview(path: &Path, contents: &str) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(path, contents)
}

#[derive(Default)]
struct PathCollector {
    paths: Vec<PathBuf>,
    seen: HashSet<PathBuf>,
}

impl PathCollector {
    fn append(&mut self, path: PathBuf) {
        let canonical = fs::canonicalize(&path).unwrap_or_else(|_| path.clone());
        if self.seen.insert(canonical) {
            self.paths.push(path);
        }
    }

    fn append_recursive_includes(&mut self, home: &Path) {
        let mut queue = self.paths.clone();
        let mut scanned = HashSet::new();
        while let Some(path) = queue.pop() {
            let canonical = fs::canonicalize(&path).unwrap_or_else(|_| path.clone());
            if !scanned.insert(canonical) || !regular_file(&path) {
                continue;
            }
            let Ok(contents) = fs::read_to_string(&path) else {
                continue;
            };
            let parent = path.parent().unwrap_or_else(|| Path::new("."));
            for include in config_file_includes(&contents, parent, home) {
                if regular_file(&include) {
                    let before = self.paths.len();
                    self.append(include.clone());
                    if self.paths.len() > before {
                        queue.push(include);
                    }
                }
            }
        }
    }
}

fn config_file_includes(contents: &str, parent: &Path, home: &Path) -> Vec<PathBuf> {
    let mut includes = Vec::new();
    for line in contents.lines() {
        let Some((key, mut value, quoted)) = parse_config_line(line) else {
            continue;
        };
        if key != "config-file" {
            continue;
        }
        if !quoted {
            value = strip_inline_comment(&value);
        }
        if value.is_empty() {
            includes.clear();
            continue;
        }
        if !quoted && value.starts_with('?') {
            value.remove(0);
        }
        if value.is_empty() {
            continue;
        }
        let expanded = expand_tilde(&value, home);
        let path = PathBuf::from(&expanded);
        includes.push(if path.is_absolute() {
            path
        } else {
            parent.join(path)
        });
    }
    includes
}

fn parse_config_line(line: &str) -> Option<(String, String, bool)> {
    let trimmed = line.trim().trim_start_matches('\u{feff}').trim();
    if trimmed.is_empty() || trimmed.starts_with('#') {
        return None;
    }
    let Some((key, value)) = trimmed.split_once('=') else {
        return Some((trimmed.to_string(), String::new(), false));
    };
    let key = key.trim().to_string();
    let mut value = value.trim().to_string();
    let quoted = value.len() >= 2 && value.starts_with('"') && value.ends_with('"');
    if quoted {
        value.remove(0);
        value.pop();
    }
    Some((key, value, quoted))
}

fn strip_inline_comment(value: &str) -> String {
    let mut result = String::new();
    let mut escaped = false;
    for ch in value.chars() {
        if escaped {
            result.push(ch);
            escaped = false;
            continue;
        }
        if ch == '\\' {
            result.push(ch);
            escaped = true;
            continue;
        }
        if ch == '#' {
            break;
        }
        result.push(ch);
    }
    result.trim().to_string()
}

fn expand_tilde(path: &str, home: &Path) -> String {
    if path == "~" {
        return home.display().to_string();
    }
    if let Some(rest) = path.strip_prefix("~/") {
        return home.join(rest).display().to_string();
    }
    path.to_string()
}

fn absolute_config_path(raw_path: &str, env: &ConfigEnvironment) -> Result<PathBuf, String> {
    let expanded = expand_tilde(raw_path, &env.home);
    let path = PathBuf::from(expanded);
    let absolute = if path.is_absolute() {
        path
    } else {
        std::env::current_dir()
            .map_err(|err| format!("failed to read current directory: {err}"))?
            .join(path)
    };
    Ok(absolute
        .canonicalize()
        .unwrap_or_else(|_| normalize_path(absolute)))
}

fn normalize_path(path: PathBuf) -> PathBuf {
    let mut normalized = PathBuf::new();
    for component in path.components() {
        match component {
            std::path::Component::CurDir => {}
            std::path::Component::ParentDir => {
                normalized.pop();
            }
            other => normalized.push(other.as_os_str()),
        }
    }
    normalized
}

fn preprocess_jsonc(source: &str) -> Result<String, String> {
    let without_bom = source.strip_prefix('\u{feff}').unwrap_or(source);
    let stripped = strip_jsonc_comments(without_bom)?;
    Ok(strip_jsonc_trailing_commas(&stripped))
}

fn strip_jsonc_comments(source: &str) -> Result<String, String> {
    let chars = source.chars().collect::<Vec<_>>();
    let mut result = String::new();
    let mut index = 0;
    let mut in_string = false;
    let mut escaped = false;

    while index < chars.len() {
        let ch = chars[index];
        if in_string {
            result.push(ch);
            if escaped {
                escaped = false;
            } else if ch == '\\' {
                escaped = true;
            } else if ch == '"' {
                in_string = false;
            }
            index += 1;
            continue;
        }

        if ch == '"' {
            in_string = true;
            result.push(ch);
            index += 1;
            continue;
        }

        if ch == '/' && chars.get(index + 1) == Some(&'/') {
            index += 2;
            while index < chars.len() && !matches!(chars[index], '\n' | '\r') {
                index += 1;
            }
            if index < chars.len() {
                result.push(chars[index]);
                index += 1;
            }
            continue;
        }

        if ch == '/' && chars.get(index + 1) == Some(&'*') {
            index += 2;
            let mut closed = false;
            while index < chars.len() {
                if chars[index] == '*' && chars.get(index + 1) == Some(&'/') {
                    index += 2;
                    closed = true;
                    break;
                }
                if matches!(chars[index], '\n' | '\r') {
                    result.push(chars[index]);
                }
                index += 1;
            }
            if !closed {
                return Err("unterminated block comment".to_string());
            }
            continue;
        }

        result.push(ch);
        index += 1;
    }

    if in_string {
        return Err("unterminated string literal".to_string());
    }
    Ok(result)
}

fn strip_jsonc_trailing_commas(source: &str) -> String {
    let chars = source.chars().collect::<Vec<_>>();
    let mut result = String::new();
    let mut index = 0;
    let mut in_string = false;
    let mut escaped = false;

    while index < chars.len() {
        let ch = chars[index];
        if in_string {
            result.push(ch);
            if escaped {
                escaped = false;
            } else if ch == '\\' {
                escaped = true;
            } else if ch == '"' {
                in_string = false;
            }
            index += 1;
            continue;
        }

        if ch == '"' {
            in_string = true;
            result.push(ch);
            index += 1;
            continue;
        }

        if ch == ',' {
            let mut probe = index + 1;
            while probe < chars.len() && chars[probe].is_whitespace() {
                probe += 1;
            }
            if matches!(chars.get(probe), Some('}' | ']')) {
                index += 1;
                continue;
            }
        }

        result.push(ch);
        index += 1;
    }
    result
}

fn regular_file(path: &Path) -> bool {
    fs::metadata(path)
        .map(|metadata| metadata.is_file())
        .unwrap_or(false)
}

fn non_empty_regular_file(path: &Path) -> bool {
    fs::metadata(path)
        .map(|metadata| metadata.is_file() && metadata.len() > 0)
        .unwrap_or(false)
}

fn abbreviated_path(path: &Path, env: &ConfigEnvironment) -> String {
    let path = path.display().to_string();
    let home = env.home.display().to_string();
    if path == home {
        return "~".to_string();
    }
    let prefix = format!("{home}/");
    if let Some(rest) = path.strip_prefix(&prefix) {
        format!("~/{rest}")
    } else {
        path
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn synced_preview_overlays_cmux_on_ghostty_entries() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let home = tmp.path().to_path_buf();
        let env = ConfigEnvironment {
            xdg_config_home: home.join(".config"),
            xdg_cache_home: home.join(".cache"),
            home: home.clone(),
            bundle_id: RELEASE_BUNDLE_ID.to_string(),
        };
        let ghostty = home.join(".config/ghostty/config");
        let cmux = home.join(".config/cmux/config.ghostty");
        fs::create_dir_all(ghostty.parent().unwrap()).unwrap();
        fs::create_dir_all(cmux.parent().unwrap()).unwrap();
        fs::write(
            &ghostty,
            "theme = Solarized Light\nbackground = #111111\nfont-size = 13\n",
        )
        .unwrap();
        fs::write(&cmux, "background = #222222\ncopy-on-select = clipboard\n").unwrap();

        let preview = render_synced_preview(Some(&ghostty), &[cmux], &env);

        assert!(preview.contains("theme = Solarized Light  # from: ~/.config/ghostty/config:1"));
        assert!(preview.contains("background = #222222  # from: ~/.config/cmux/config.ghostty:1"));
        assert!(
            preview.contains("copy-on-select = clipboard  # from: ~/.config/cmux/config.ghostty:2")
        );
        assert!(!preview.contains("background = #111111"));
    }

    #[test]
    fn cmux_config_path_defaults_to_linux_xdg_for_fresh_nonrelease_bundle() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let home = tmp.path().to_path_buf();
        let env = ConfigEnvironment {
            xdg_config_home: home.join(".config"),
            xdg_cache_home: home.join(".cache"),
            home: home.clone(),
            bundle_id: "ai.cmux.dev".to_string(),
        };

        assert_eq!(
            cmux_config_path(&env),
            home.join(".config/cmux/config.ghostty")
        );
    }

    #[test]
    fn cmux_config_path_still_migrates_existing_nonrelease_macos_config() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let home = tmp.path().to_path_buf();
        let env = ConfigEnvironment {
            xdg_config_home: home.join(".config"),
            xdg_cache_home: home.join(".cache"),
            home: home.clone(),
            bundle_id: "ai.cmux.dev".to_string(),
        };
        let legacy = home.join("Library/Application Support/ai.cmux.dev/config.ghostty");
        fs::create_dir_all(legacy.parent().unwrap()).unwrap();
        fs::write(&legacy, "theme = Dev\n").unwrap();

        assert_eq!(cmux_config_path(&env), legacy);
    }

    #[test]
    fn include_collector_handles_optional_relative_and_clear_entries() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let home = tmp.path();
        let dir = home.join("config");
        let root = dir.join("root");
        let nested = dir.join("nested");
        fs::create_dir_all(&dir).unwrap();
        fs::write(
            &root,
            "config-file = ?nested # optional include\nconfig-file = \"~/quoted\"\n",
        )
        .unwrap();
        fs::write(&nested, "font-size = 12\n").unwrap();
        fs::write(home.join("quoted"), "theme = dark\n").unwrap();

        let includes = config_file_includes(&fs::read_to_string(root).unwrap(), &dir, home);

        assert_eq!(includes, vec![nested, home.join("quoted")]);
    }

    #[test]
    fn canvas_settings_layer_clamp_and_write_primary_cmux_json() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let home = tmp.path().to_path_buf();
        let env = ConfigEnvironment {
            xdg_config_home: home.join(".config"),
            xdg_cache_home: home.join(".cache"),
            home: home.clone(),
            bundle_id: RELEASE_BUNDLE_ID.to_string(),
        };
        let path = primary_cmux_json_path(&env);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(
            &path,
            r#"{
                // Canvas values are layered and bounded on read.
                "canvas": {"paneGap": 99, "snappingEnabled": false},
                "unrelated": {"preserved": true}
            }"#,
        )
        .unwrap();

        let settings = canvas_settings_with_env(&env);
        assert_eq!(settings.pane_gap, 64.0);
        assert!(!settings.snapping_enabled);
        assert_eq!(settings.snap_threshold, 0.0);
        assert_eq!(settings.min_pane_width, 200.0);
        assert_eq!(settings.min_pane_height, 120.0);

        set_canvas_setting_with_env(&env, "paneGap", json!(24.0)).unwrap();
        set_canvas_setting_with_env(&env, "snappingEnabled", json!(true)).unwrap();
        let written = read_jsonc_object(&path).expect("written settings");
        assert_eq!(written["canvas"]["paneGap"], 24.0);
        assert_eq!(written["canvas"]["snappingEnabled"], true);
        assert_eq!(written["unrelated"]["preserved"], true);
    }

    #[test]
    fn diff_viewer_layout_layers_and_writes_primary_cmux_json() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let home = tmp.path().to_path_buf();
        let env = ConfigEnvironment {
            xdg_config_home: home.join(".config"),
            xdg_cache_home: home.join(".cache"),
            home: home.clone(),
            bundle_id: RELEASE_BUNDLE_ID.to_string(),
        };
        let legacy = env.xdg_config_home.join("cmux/settings.json");
        let primary = primary_cmux_json_path(&env);
        fs::create_dir_all(primary.parent().unwrap()).unwrap();
        fs::write(&legacy, r#"{"diffViewer":{"defaultLayout":"split"}}"#).unwrap();
        fs::write(
            &primary,
            r#"{"diffViewer":{"defaultLayout":"invalid"},"keep":true}"#,
        )
        .unwrap();

        assert_eq!(
            diff_viewer_default_layout_with_env(&env),
            DiffViewerLayout::Split
        );
        set_diff_viewer_default_layout_with_env(&env, DiffViewerLayout::Unified).unwrap();
        assert_eq!(
            diff_viewer_default_layout_with_env(&env),
            DiffViewerLayout::Unified
        );
        let written = read_jsonc_object(&primary).expect("written config");
        assert_eq!(written["diffViewer"]["defaultLayout"], "unified");
        assert_eq!(written["keep"], true);
    }

    #[test]
    fn app_workspace_settings_layer_and_write_primary_cmux_json() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let home = tmp.path().to_path_buf();
        let env = ConfigEnvironment {
            xdg_config_home: home.join(".config"),
            xdg_cache_home: home.join(".cache"),
            home: home.clone(),
            bundle_id: RELEASE_BUNDLE_ID.to_string(),
        };
        let legacy = env.xdg_config_home.join("cmux/settings.json");
        let primary = primary_cmux_json_path(&env);
        fs::create_dir_all(primary.parent().unwrap()).unwrap();
        fs::write(
            &legacy,
            r#"{"app":{"newWorkspacePlacement":"top","workspaceInheritWorkingDirectory":false,"keepWorkspaceOpenWhenClosingLastSurface":true,"warnBeforeQuit":false,"warnBeforeClosingTab":false,"warnBeforeClosingTabXButton":true,"hideTabCloseButton":true}}"#,
        )
        .unwrap();
        fs::write(
            &primary,
            r#"{"app":{"newWorkspacePlacement":"invalid","confirmQuit":"dirty-only","warnBeforeClosingTab":true},"keep":true}"#,
        )
        .unwrap();

        assert_eq!(
            app_workspace_settings_with_env(&env),
            AppWorkspaceSettings {
                new_workspace_placement: WorkspacePlacement::Top,
                workspace_inherit_working_directory: false,
                keep_workspace_open_when_closing_last_surface: true,
                confirm_quit: ConfirmQuitPolicy::DirtyOnly,
                warn_before_closing_tab: true,
                warn_before_closing_tab_x_button: true,
                hide_tab_close_button: true,
            }
        );

        set_app_workspace_setting_with_env(&env, "newWorkspacePlacement", json!("afterCurrent"))
            .unwrap();
        set_app_workspace_setting_with_env(&env, "workspaceInheritWorkingDirectory", json!(true))
            .unwrap();
        set_app_workspace_setting_with_env(
            &env,
            "keepWorkspaceOpenWhenClosingLastSurface",
            json!(false),
        )
        .unwrap();
        set_app_workspace_setting_with_env(&env, "confirmQuit", json!("never")).unwrap();
        set_app_workspace_setting_with_env(&env, "warnBeforeClosingTab", json!(false)).unwrap();
        set_app_workspace_setting_with_env(&env, "warnBeforeClosingTabXButton", json!(false))
            .unwrap();
        set_app_workspace_setting_with_env(&env, "hideTabCloseButton", json!(false)).unwrap();
        assert_eq!(
            app_workspace_settings_with_env(&env),
            AppWorkspaceSettings {
                new_workspace_placement: WorkspacePlacement::AfterCurrent,
                workspace_inherit_working_directory: true,
                keep_workspace_open_when_closing_last_surface: false,
                confirm_quit: ConfirmQuitPolicy::Never,
                warn_before_closing_tab: false,
                warn_before_closing_tab_x_button: false,
                hide_tab_close_button: false,
            }
        );
        let written = read_jsonc_object(&primary).expect("written config");
        assert_eq!(written["app"]["newWorkspacePlacement"], "afterCurrent");
        assert_eq!(written["app"]["workspaceInheritWorkingDirectory"], true);
        assert_eq!(
            written["app"]["keepWorkspaceOpenWhenClosingLastSurface"],
            false
        );
        assert_eq!(written["app"]["confirmQuit"], "never");
        assert_eq!(written["app"]["warnBeforeClosingTab"], false);
        assert_eq!(written["app"]["warnBeforeClosingTabXButton"], false);
        assert_eq!(written["app"]["hideTabCloseButton"], false);
        assert_eq!(written["keep"], true);
    }

    #[test]
    fn system_wide_hotkey_layers_and_writes_the_primary_app_setting() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let home = tmp.path().to_path_buf();
        let env = ConfigEnvironment {
            xdg_config_home: home.join(".config"),
            xdg_cache_home: home.join(".cache"),
            home: home.clone(),
            bundle_id: RELEASE_BUNDLE_ID.to_string(),
        };
        let legacy = env.xdg_config_home.join("cmux/settings.json");
        let primary = primary_cmux_json_path(&env);
        fs::create_dir_all(primary.parent().unwrap()).unwrap();
        fs::write(&legacy, r#"{"app":{"systemWideHotkeyEnabled":true}}"#).unwrap();
        fs::write(
            &primary,
            r#"{"app":{"systemWideHotkeyEnabled":false},"keep":true}"#,
        )
        .unwrap();

        assert!(!system_wide_hotkey_enabled_with_env(&env));

        set_system_wide_hotkey_enabled_with_env(&env, true).unwrap();
        assert!(system_wide_hotkey_enabled_with_env(&env));
        assert_eq!(read_jsonc_object(&primary).unwrap()["keep"], true);
    }

    #[test]
    fn terminal_text_box_settings_layer_clamp_and_preserve_primary_json() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let home = tmp.path().to_path_buf();
        let env = ConfigEnvironment {
            xdg_config_home: home.join(".config"),
            xdg_cache_home: home.join(".cache"),
            home: home.clone(),
            bundle_id: RELEASE_BUNDLE_ID.to_string(),
        };
        let legacy = env.xdg_config_home.join("cmux/settings.json");
        let primary = primary_cmux_json_path(&env);
        fs::create_dir_all(primary.parent().unwrap()).unwrap();
        fs::write(
            &legacy,
            r#"{"terminal":{"showTextBoxOnNewTerminals":true,"textBoxMaxLines":4}}"#,
        )
        .unwrap();
        fs::write(
            &primary,
            r#"{"terminal":{"focusTextBoxOnNewTerminals":true,"textBoxMaxLines":40},"keep":true}"#,
        )
        .unwrap();

        let settings = terminal_text_box_settings_with_env(&env);
        assert!(settings.show_on_new_terminals);
        assert!(settings.focus_on_new_terminals);
        assert_eq!(settings.max_lines, 20);

        set_terminal_text_box_setting_with_env(&env, "textBoxMaxLines", json!(0)).unwrap();
        let written = read_jsonc_object(&primary).expect("written config");
        assert_eq!(written["terminal"]["textBoxMaxLines"], 1);
        assert_eq!(written["keep"], true);
    }

    #[test]
    fn browser_search_settings_layer_and_validate_custom_template() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let home = tmp.path().to_path_buf();
        let env = ConfigEnvironment {
            xdg_config_home: home.join(".config"),
            xdg_cache_home: home.join(".cache"),
            home: home.clone(),
            bundle_id: RELEASE_BUNDLE_ID.to_string(),
        };
        let legacy = env.xdg_config_home.join("cmux/settings.json");
        let primary = primary_cmux_json_path(&env);
        fs::create_dir_all(primary.parent().unwrap()).unwrap();
        fs::write(
            &legacy,
            r#"{"browser":{"defaultSearchEngine":"duckduckgo","showSearchSuggestions":false}}"#,
        )
        .unwrap();
        fs::write(
            &primary,
            r#"{"browser":{"defaultSearchEngine":"custom","customSearchEngineName":"Docs","customSearchEngineURLTemplate":"https://docs.example.test/find/{query}","showSearchSuggestions":true}}"#,
        )
        .unwrap();

        assert_eq!(
            browser_search_settings_with_env(&env),
            BrowserSearchSettings {
                engine: "custom".to_string(),
                custom_name: "Docs".to_string(),
                custom_url_template: "https://docs.example.test/find/{query}".to_string(),
                show_search_suggestions: true,
            }
        );

        fs::write(
            &primary,
            r#"{"browser":{"defaultSearchEngine":"unknown","customSearchEngineURLTemplate":"javascript:{query}"}}"#,
        )
        .unwrap();
        assert_eq!(
            browser_search_settings_with_env(&env),
            BrowserSearchSettings {
                engine: "duckduckgo".to_string(),
                custom_name: String::new(),
                custom_url_template: "https://www.google.com/search?q={query}".to_string(),
                show_search_suggestions: false,
            }
        );
    }

    #[test]
    fn workspace_color_settings_layer_normalize_and_write_primary_palette() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let home = tmp.path().to_path_buf();
        let env = ConfigEnvironment {
            xdg_config_home: home.join(".config"),
            xdg_cache_home: home.join(".cache"),
            home: home.clone(),
            bundle_id: RELEASE_BUNDLE_ID.to_string(),
        };
        let legacy = env.xdg_config_home.join("cmux/settings.json");
        let primary = primary_cmux_json_path(&env);
        fs::create_dir_all(primary.parent().unwrap()).unwrap();
        fs::write(
            &legacy,
            r##"{
                "workspaceColors": {
                    "indicatorStyle": "wash",
                    "selectionColor": "#123abc",
                    "paletteOverrides": {"Blue": "#010203"},
                    "customColors": ["#AABBCC"]
                }
            }"##,
        )
        .unwrap();
        fs::write(
            &primary,
            r##"{
                "workspaceColors": {
                    "indicatorStyle": "rail",
                    "notificationBadgeColor": "#fefefe",
                    "colors": {
                        "Blue": "#1565c0",
                        "Custom Team": "#0a1b2c"
                    }
                },
                "keep": true
            }"##,
        )
        .unwrap();

        let settings = workspace_color_settings_with_env(&env);
        assert_eq!(settings.indicator_style, "leftRail");
        assert_eq!(settings.selection_color.as_deref(), Some("#123ABC"));
        assert_eq!(
            settings.notification_badge_color.as_deref(),
            Some("#FEFEFE")
        );
        assert_eq!(
            settings.colors,
            vec![
                ("Blue".to_string(), "#1565C0".to_string()),
                ("Custom Team".to_string(), "#0A1B2C".to_string())
            ]
        );

        set_workspace_color_setting_with_env(&env, "selectionColor", Value::Null).unwrap();
        set_workspace_color_setting_with_env(&env, "indicatorStyle", json!("solidFill")).unwrap();
        write_workspace_palette(&env, &default_workspace_palette()).unwrap();
        let written = read_jsonc_object(&primary).expect("written config");
        assert!(written["workspaceColors"]["selectionColor"].is_null());
        assert_eq!(written["workspaceColors"]["indicatorStyle"], "solidFill");
        assert_eq!(
            written["workspaceColors"]["colors"]["Red"],
            WORKSPACE_COLOR_DEFAULT_PALETTE[0].1
        );
        assert!(written["workspaceColors"].get("paletteOverrides").is_none());
        assert!(written["workspaceColors"].get("customColors").is_none());
        assert_eq!(written["keep"], true);
    }

    #[test]
    fn beta_feature_settings_layer_shared_defaults_and_preserve_primary_json() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let home = tmp.path().to_path_buf();
        let env = ConfigEnvironment {
            xdg_config_home: home.join(".config"),
            xdg_cache_home: home.join(".cache"),
            home: home.clone(),
            bundle_id: RELEASE_BUNDLE_ID.to_string(),
        };
        let legacy = env.xdg_config_home.join("cmux/settings.json");
        let primary = primary_cmux_json_path(&env);
        fs::create_dir_all(primary.parent().unwrap()).unwrap();
        fs::write(
            &legacy,
            r#"{
                "rightSidebar.beta.feed.enabled": true,
                "rightSidebar.beta.dock.enabled": true,
                "extensions.beta.enabled": true,
                "customSidebars.beta.enabled": false,
                "remoteTmux.beta.enabled": true
            }"#,
        )
        .unwrap();
        fs::write(
            &primary,
            r#"{
                "rightSidebar.beta.dock.enabled": false,
                "customSidebars.beta.enabled": true,
                "keep": {"value": 1}
            }"#,
        )
        .unwrap();

        let settings = beta_feature_settings_with_env(&env);
        assert!(settings.right_sidebar_feed);
        assert!(!settings.right_sidebar_dock);
        assert!(settings.extensions);
        assert!(settings.custom_sidebars);
        assert!(settings.remote_tmux);

        set_beta_feature_setting_with_env(&env, "feed", json!(false)).unwrap();
        set_beta_feature_setting_with_env(&env, "rightSidebarDock", json!(true)).unwrap();
        set_beta_feature_setting_with_env(&env, "customSidebars", json!(false)).unwrap();
        let written = read_jsonc_object(&primary).expect("written config");
        assert_eq!(written["rightSidebar.beta.feed.enabled"], false);
        assert_eq!(written["rightSidebar.beta.dock.enabled"], true);
        assert_eq!(written["customSidebars.beta.enabled"], false);
        assert_eq!(written["keep"]["value"], 1);
    }

    #[test]
    fn sidebar_settings_layer_legacy_layout_and_write_primary() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let home = tmp.path().to_path_buf();
        let env = ConfigEnvironment {
            xdg_config_home: home.join(".config"),
            xdg_cache_home: home.join(".cache"),
            home: home.clone(),
            bundle_id: RELEASE_BUNDLE_ID.to_string(),
        };
        let legacy = env.xdg_config_home.join("cmux/settings.json");
        let primary = primary_cmux_json_path(&env);
        fs::create_dir_all(primary.parent().unwrap()).unwrap();
        fs::write(
            &legacy,
            r#"{
                "sidebarAppearance": {"matchTerminalBackground": true},
                "sidebar": {
                    "hideAllDetails": true,
                    "branchVerticalLayout": false,
                    "showPorts": false,
                    "rightMaxWidth": 99999
                }
            }"#,
        )
        .unwrap();
        fs::write(
            &primary,
            r#"{
                "sidebar": {
                    "hideAllDetails": false,
                    "branchLayout": "vertical",
                    "stackBranchDirectory": true,
                    "pathLastSegmentOnly": true,
                    "showNotificationMessage": false
                },
                "keep": {"value": 1}
            }"#,
        )
        .unwrap();

        let settings = sidebar_settings_with_env(&env);
        assert!(settings.match_terminal_background);
        assert!(!settings.hide_all_details);
        assert_eq!(settings.branch_layout, SidebarBranchLayout::Vertical);
        assert!(settings.stack_branch_directory);
        assert!(settings.path_last_segment_only);
        assert!(!settings.show_notification_message);
        assert!(!settings.show_ports);
        assert_eq!(settings.right_max_width, Some(4096.0));

        set_sidebar_setting_with_env(&env, "sidebar.branchLayout", json!("inline")).unwrap();
        set_sidebar_setting_with_env(&env, "showPorts", json!(true)).unwrap();
        set_sidebar_setting_with_env(&env, "matchTerminalBackground", json!(false)).unwrap();
        set_sidebar_setting_with_env(&env, "rightMaxWidth", Value::Null).unwrap();
        let written = read_jsonc_object(&primary).expect("written config");
        assert_eq!(written["sidebar"]["branchLayout"], "inline");
        assert_eq!(written["sidebar"]["showPorts"], true);
        assert!(written["sidebar"]["rightMaxWidth"].is_null());
        assert_eq!(
            written["sidebarAppearance"]["matchTerminalBackground"],
            false
        );
        assert_eq!(written["keep"]["value"], 1);
        assert_eq!(sidebar_settings_with_env(&env).right_max_width, None);
    }

    #[test]
    fn terminal_interaction_settings_layer_and_preserve_primary_json() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let home = tmp.path().to_path_buf();
        let env = ConfigEnvironment {
            xdg_config_home: home.join(".config"),
            xdg_cache_home: home.join(".cache"),
            home: home.clone(),
            bundle_id: RELEASE_BUNDLE_ID.to_string(),
        };
        let legacy = env.xdg_config_home.join("cmux/settings.json");
        let primary = primary_cmux_json_path(&env);
        fs::create_dir_all(primary.parent().unwrap()).unwrap();
        fs::write(
            &legacy,
            r#"{"terminal":{"showScrollBar":false,"copyOnSelect":true,"autoResumeAgentSessions":false}}"#,
        )
        .unwrap();
        fs::write(
            &primary,
            r#"{"terminal":{"showScrollBar":true,"autoResumeAgentSessions":true},"keep":true}"#,
        )
        .unwrap();

        let settings = terminal_interaction_settings_with_env(&env);
        assert!(settings.show_scroll_bar);
        assert!(settings.copy_on_select);
        assert!(settings.auto_resume_agent_sessions);

        set_terminal_interaction_setting_with_env(&env, "copyOnSelect", json!(false)).unwrap();
        set_terminal_interaction_setting_with_env(&env, "autoResumeAgentSessions", json!(false))
            .unwrap();
        let written = read_jsonc_object(&primary).expect("written config");
        assert_eq!(written["terminal"]["showScrollBar"], true);
        assert_eq!(written["terminal"]["copyOnSelect"], false);
        assert_eq!(written["terminal"]["autoResumeAgentSessions"], false);
        assert_eq!(written["keep"], true);
        assert_eq!(
            terminal_managed_ghostty_config_for(TerminalInteractionSettings {
                show_scroll_bar: true,
                copy_on_select: true,
                auto_resume_agent_sessions: true,
            }),
            "copy-on-select = clipboard"
        );
        assert_eq!(
            terminal_managed_ghostty_config_for(TerminalInteractionSettings::default()),
            "copy-on-select = false"
        );
    }

    #[test]
    fn agent_hibernation_settings_layer_clamp_and_preserve_primary_json() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let home = tmp.path().to_path_buf();
        let env = ConfigEnvironment {
            xdg_config_home: home.join(".config"),
            xdg_cache_home: home.join(".cache"),
            home: home.clone(),
            bundle_id: RELEASE_BUNDLE_ID.to_string(),
        };
        let legacy = env.xdg_config_home.join("cmux/settings.json");
        let primary = primary_cmux_json_path(&env);
        fs::create_dir_all(primary.parent().unwrap()).unwrap();
        fs::write(
            &legacy,
            r#"{"terminal":{"agentHibernation":{"enabled":true,"idleSeconds":30,"maxLiveTerminals":8,"confirmationSeconds":90}}}"#,
        )
        .unwrap();
        fs::write(
            &primary,
            r#"{"terminal":{"agentHibernation":{"idleSeconds":1,"maxLiveTerminals":999}},"keep":true}"#,
        )
        .unwrap();

        assert_eq!(
            agent_hibernation_settings_with_env(&env),
            crate::agent_hibernation_settings::Settings {
                enabled: true,
                idle_seconds: crate::agent_hibernation_settings::MIN_IDLE_SECONDS,
                max_live_terminals: crate::agent_hibernation_settings::MAX_LIVE_TERMINALS,
                confirmation_seconds: 90,
            }
        );

        set_agent_hibernation_settings_with_env(
            &env,
            crate::agent_hibernation_settings::Settings {
                enabled: false,
                idle_seconds: 120,
                max_live_terminals: 4,
                confirmation_seconds: 45,
            },
        )
        .unwrap();
        let written = read_jsonc_object(&primary).expect("written config");
        assert_eq!(
            written["terminal"]["agentHibernation"],
            json!({
                "enabled": false,
                "idleSeconds": 120,
                "maxLiveTerminals": 4,
                "confirmationSeconds": 45
            })
        );
        assert_eq!(written["keep"], true);
    }

    #[test]
    fn custom_sidebar_renderer_layers_and_writes_primary_json() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let home = tmp.path().to_path_buf();
        let env = ConfigEnvironment {
            xdg_config_home: home.join(".config"),
            xdg_cache_home: home.join(".cache"),
            home: home.clone(),
            bundle_id: RELEASE_BUNDLE_ID.to_string(),
        };
        let legacy = env.xdg_config_home.join("cmux/settings.json");
        let primary = primary_cmux_json_path(&env);
        fs::create_dir_all(primary.parent().unwrap()).unwrap();
        fs::write(&legacy, r#"{"customSidebars":{"renderer":"remote"}}"#).unwrap();
        fs::write(
            &primary,
            r#"{"customSidebars":{"renderer":"invalid"},"keep":true}"#,
        )
        .unwrap();

        assert_eq!(
            custom_sidebar_renderer_mode_with_env(&env),
            CustomSidebarRendererMode::Remote
        );

        set_custom_sidebar_renderer_mode_with_env(&env, CustomSidebarRendererMode::InProcess)
            .unwrap();
        assert_eq!(
            custom_sidebar_renderer_mode_with_env(&env),
            CustomSidebarRendererMode::InProcess
        );
        let written = read_jsonc_object(&primary).expect("written config");
        assert_eq!(written["customSidebars"]["renderer"], "inProcess");
        assert_eq!(written["keep"], true);
    }

    #[test]
    fn shortcut_bindings_layer_parse_recorder_forms_and_write_primary_json() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let home = tmp.path().to_path_buf();
        let env = ConfigEnvironment {
            xdg_config_home: home.join(".config"),
            xdg_cache_home: home.join(".cache"),
            home: home.clone(),
            bundle_id: RELEASE_BUNDLE_ID.to_string(),
        };
        let legacy = env.xdg_config_home.join("cmux/settings.json");
        let primary = primary_cmux_json_path(&env);
        fs::create_dir_all(primary.parent().unwrap()).unwrap();
        fs::write(
            &legacy,
            r#"{
                "shortcuts": {
                    "bindings": {
                        "newSurface": "cmd+opt+t",
                        "focusLeft": {
                            "first": {
                                "key": "←",
                                "command": true,
                                "option": true
                            }
                        }
                    },
                    "when": {
                        "newSurface": "terminalFocus",
                        "openFolder": "workspaceCount > 1"
                    }
                }
            }"#,
        )
        .unwrap();
        fs::write(
            &primary,
            r#"{
                // The primary file overrides the legacy layer.
                "shortcuts": {
                    "bindings": {
                        "newSurface": null,
                        "canvasAlignLeft": "super+ctrl+1",
                        "openFolder": ["ctrl+b", "o"],
                    },
                    "when": {
                        "newSurface": "!sidebarFocus",
                        "openFolder": 42,
                        "focusLeft": "paneCount > 1"
                    }
                },
            }"#,
        )
        .unwrap();

        let bindings = shortcut_bindings_with_env(&env);
        assert_eq!(bindings.get("newSurface"), Some(&ShortcutBinding::Unbound));
        assert_eq!(
            bindings.get("focusLeft"),
            Some(&ShortcutBinding::Single("cmd+opt+left".to_string()))
        );
        assert_eq!(
            bindings.get("canvasAlignLeft"),
            Some(&ShortcutBinding::Single("super+ctrl+1".to_string()))
        );
        assert_eq!(
            bindings.get("openFolder"),
            Some(&ShortcutBinding::Chord(
                "ctrl+b".to_string(),
                "o".to_string()
            ))
        );

        let when = shortcut_when_clauses_with_env(&env);
        assert_eq!(
            when.get("newSurface").map(String::as_str),
            Some("!sidebarFocus")
        );
        assert_eq!(
            when.get("openFolder").map(String::as_str),
            Some("workspaceCount > 1")
        );
        assert_eq!(
            when.get("focusLeft").map(String::as_str),
            Some("paneCount > 1")
        );

        set_shortcut_binding_with_env(
            &env,
            "newSurface",
            ShortcutBindingUpdate::Set(vec!["cmd+ctrl+t".to_string()]),
        )
        .unwrap();
        let written = read_jsonc_object(&primary).expect("written config");
        assert_eq!(written["shortcuts"]["bindings"]["newSurface"], "cmd+ctrl+t");

        set_shortcut_binding_with_env(
            &env,
            "newSurface",
            ShortcutBindingUpdate::Set(vec!["ctrl+b".to_string(), "c".to_string()]),
        )
        .unwrap();
        let written = read_jsonc_object(&primary).expect("written chord");
        assert_eq!(
            written["shortcuts"]["bindings"]["newSurface"],
            json!(["ctrl+b", "c"])
        );

        set_shortcut_binding_with_env(&env, "canvasAlignLeft", ShortcutBindingUpdate::Reset)
            .unwrap();
        let written = read_jsonc_object(&primary).expect("reset config");
        assert!(written["shortcuts"]["bindings"]
            .get("canvasAlignLeft")
            .is_none());
    }
}
