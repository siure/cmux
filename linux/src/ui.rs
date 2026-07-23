use crate::{
    app::{AppError, AppState, TerminalStartupMode},
    browser_environment::BrowserEnvironmentState,
    config, file_url, server,
};
use anyhow::{anyhow, Context, Result};
use serde_json::{json, Map, Value};
use std::collections::BTreeMap;
use std::fs;
use std::io::{self, BufRead, Write};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

const APP_RENDERER_ENV: &str = "CMUX_LINUX_RENDERER";
const GTK_SINGLE_INSTANCE_ENV: &str = "CMUX_LINUX_GTK_SINGLE_INSTANCE";

#[derive(Clone, Debug, PartialEq)]
#[cfg_attr(not(feature = "gtk"), allow(dead_code))]
pub(crate) struct BrowserNavigationState {
    pub surface_id: String,
    pub focused: bool,
    pub profile_id: String,
    pub profile_data_generation: u64,
    pub url: String,
    pub can_go_back: bool,
    pub can_go_forward: bool,
    pub page_zoom: f64,
    pub user_agent: String,
    pub init_scripts: Vec<String>,
    pub storage: BrowserStorageState,
    pub environment: BrowserEnvironmentState,
    pub request_configuration_generation: u64,
    pub developer_tools_visible: bool,
    pub focus_mode_active: bool,
    pub runtime_actions: Vec<BrowserRuntimeAction>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
#[cfg_attr(not(feature = "gtk"), allow(dead_code))]
pub(crate) struct BrowserStorageState {
    pub generation: u64,
    pub local: BTreeMap<String, String>,
    pub session: BTreeMap<String, String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
#[cfg_attr(not(feature = "gtk"), allow(dead_code))]
pub(crate) struct BrowserRuntimeAction {
    pub sequence: u64,
    pub script: String,
    pub focus_webview: bool,
    pub cookie: Option<BrowserRuntimeCookieAction>,
    pub upload: Option<BrowserRuntimeUploadAction>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
#[cfg_attr(not(feature = "gtk"), allow(dead_code))]
pub(crate) struct BrowserRuntimeCookieAction {
    pub operation: String,
    pub url: String,
    pub name: Option<String>,
    pub value: Option<String>,
    pub domain: Option<String>,
    pub path: Option<String>,
    pub max_age: Option<i32>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
#[cfg_attr(not(feature = "gtk"), allow(dead_code))]
pub(crate) struct BrowserRuntimeUploadAction {
    pub files: Vec<String>,
}

#[cfg_attr(not(feature = "gtk"), allow(dead_code))]
pub(crate) fn browser_navigation_state(view: &Value) -> Option<BrowserNavigationState> {
    if view.get("kind").and_then(Value::as_str) != Some("browser") {
        return None;
    }
    let surface_id = view
        .get("surface_id")
        .and_then(Value::as_str)
        .or_else(|| view.get("surface_ref").and_then(Value::as_str))
        .filter(|value| !value.trim().is_empty())?
        .to_string();
    let browser = view.get("browser").unwrap_or(&Value::Null);
    let url = browser
        .get("url")
        .and_then(Value::as_str)
        .or_else(|| view.get("url").and_then(Value::as_str))
        .unwrap_or_default()
        .to_string();
    Some(BrowserNavigationState {
        surface_id,
        focused: view
            .get("focused")
            .and_then(Value::as_bool)
            .unwrap_or(false),
        profile_id: browser
            .get("profile_id")
            .and_then(Value::as_str)
            .filter(|value| !value.trim().is_empty())
            .unwrap_or("52B43C05-4A1D-45D3-8FD5-9EF94952E445")
            .to_string(),
        profile_data_generation: browser
            .get("profile_data_generation")
            .and_then(Value::as_u64)
            .unwrap_or(0),
        url,
        can_go_back: browser
            .get("can_go_back")
            .and_then(Value::as_bool)
            .unwrap_or(false),
        can_go_forward: browser
            .get("can_go_forward")
            .and_then(Value::as_bool)
            .unwrap_or(false),
        page_zoom: browser
            .get("page_zoom")
            .and_then(Value::as_f64)
            .unwrap_or(1.0),
        user_agent: browser
            .get("user_agent")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string(),
        init_scripts: browser
            .get("init_scripts")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(Value::as_str)
            .map(ToString::to_string)
            .collect(),
        storage: browser_storage_state(browser.get("storage")),
        environment: BrowserEnvironmentState::from_snapshot(browser.get("environment")),
        request_configuration_generation: browser
            .get("request_configuration_generation")
            .and_then(Value::as_u64)
            .unwrap_or(0),
        developer_tools_visible: browser
            .get("developer_tools_visible")
            .and_then(Value::as_bool)
            .or_else(|| view.get("developer_tools_visible").and_then(Value::as_bool))
            .unwrap_or(false),
        focus_mode_active: browser
            .get("focus_mode_active")
            .and_then(Value::as_bool)
            .unwrap_or(false),
        runtime_actions: browser
            .get("runtime_actions")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(|action| {
                Some(BrowserRuntimeAction {
                    sequence: action.get("sequence")?.as_u64()?,
                    script: action
                        .get("script")
                        .and_then(Value::as_str)
                        .unwrap_or_default()
                        .to_string(),
                    focus_webview: action
                        .get("focus_webview")
                        .and_then(Value::as_bool)
                        .unwrap_or(false),
                    cookie: action.get("cookie").and_then(|cookie| {
                        let max_age = cookie
                            .get("max_age")
                            .and_then(Value::as_i64)
                            .and_then(|value| i32::try_from(value).ok());
                        Some(BrowserRuntimeCookieAction {
                            operation: cookie.get("operation")?.as_str()?.to_string(),
                            url: cookie
                                .get("url")
                                .and_then(Value::as_str)
                                .unwrap_or_default()
                                .to_string(),
                            name: cookie
                                .get("name")
                                .and_then(Value::as_str)
                                .map(ToString::to_string),
                            value: cookie
                                .get("value")
                                .and_then(Value::as_str)
                                .map(ToString::to_string),
                            domain: cookie
                                .get("domain")
                                .and_then(Value::as_str)
                                .map(ToString::to_string),
                            path: cookie
                                .get("path")
                                .and_then(Value::as_str)
                                .map(ToString::to_string),
                            max_age,
                        })
                    }),
                    upload: action.get("upload").and_then(|upload| {
                        Some(BrowserRuntimeUploadAction {
                            files: upload
                                .get("files")?
                                .as_array()?
                                .iter()
                                .filter_map(Value::as_str)
                                .map(ToString::to_string)
                                .collect(),
                        })
                    }),
                })
            })
            .collect(),
    })
}

fn browser_storage_state(value: Option<&Value>) -> BrowserStorageState {
    let value = value.unwrap_or(&Value::Null);
    BrowserStorageState {
        generation: value.get("generation").and_then(Value::as_u64).unwrap_or(0),
        local: browser_storage_entries(value.get("local")),
        session: browser_storage_entries(value.get("session")),
    }
}

fn browser_storage_entries(value: Option<&Value>) -> BTreeMap<String, String> {
    value
        .and_then(Value::as_object)
        .into_iter()
        .flatten()
        .filter_map(|(key, value)| value.as_str().map(|value| (key.clone(), value.to_string())))
        .collect()
}

pub fn run_app_command(command: &[String]) -> Result<()> {
    let mut script: Option<String> = None;
    let mut socket: Option<String> = None;
    let mut renderer_override: Option<String> = None;
    let mut open_targets = Vec::new();
    let mut literal_targets = false;
    let mut index = 1;
    while index < command.len() {
        if literal_targets {
            open_targets.push(command[index].clone());
            index += 1;
            continue;
        }
        match command[index].as_str() {
            "--" => {
                literal_targets = true;
            }
            "--socket" | "-s" => {
                index += 1;
                socket = Some(
                    command
                        .get(index)
                        .context("--socket requires a path")?
                        .clone(),
                );
            }
            "--script" => {
                index += 1;
                script = Some(
                    command
                        .get(index)
                        .context("--script requires a command string")?
                        .clone(),
                );
            }
            "--renderer" => {
                index += 1;
                renderer_override = Some(
                    command
                        .get(index)
                        .context("--renderer requires core, gtk, ghostty, or ghostty-vt")?
                        .clone(),
                );
            }
            "--script-file" => {
                index += 1;
                let path = command
                    .get(index)
                    .context("--script-file requires a path")?;
                script = Some(
                    fs::read_to_string(path)
                        .with_context(|| format!("failed to read script file {path}"))?,
                );
            }
            "--help" | "-h" => {
                print_app_help();
                return Ok(());
            }
            other if !other.starts_with('-') => {
                open_targets.push(other.to_string());
            }
            other => return Err(anyhow!("unknown app option: {other}")),
        }
        index += 1;
    }

    let env_renderer = std::env::var(APP_RENDERER_ENV).ok();
    let renderer =
        app_renderer_from_cli_or_env(renderer_override.as_deref(), env_renderer.as_deref())?;
    let gtk_single_instance = gtk_single_instance_mode(
        socket.is_some(),
        std::env::var(GTK_SINGLE_INSTANCE_ENV).ok().as_deref(),
    )?;
    #[cfg(feature = "gtk")]
    if matches!(renderer.as_str(), "gtk" | "ghostty") {
        crate::gtk_webkit::configure_environment();
    }
    let debug_log_path = socket.as_deref().map(server::debug_log_path_for_socket);
    if let Some(path) = &debug_log_path {
        let _ = fs::write(path, "");
    }
    let terminal_startup_mode = terminal_startup_mode_for_renderer(&renderer)?;
    let app = Arc::new(Mutex::new(AppState::with_paths_and_terminal_startup(
        debug_log_path,
        socket.clone(),
        terminal_startup_mode,
    )?));
    crate::mobile_host::start(Arc::clone(&app));
    let _server_thread = if let Some(socket) = socket.as_deref() {
        Some(server::spawn_server_with_state(socket, Arc::clone(&app))?)
    } else {
        None
    };
    if !open_targets.is_empty() {
        open_startup_targets(&app, &open_targets)?;
    }

    let default_renderer_backend = match renderer.as_str() {
        "core" => "core",
        "gtk" | "gtk4" => {
            if let Some(script) = script.as_deref() {
                if run_script(&app, script, "gtk")? {
                    return Ok(());
                }
            }
            return run_gtk_renderer(app, gtk_single_instance);
        }
        "ghostty-vt" | "libghostty-vt" | "vt" => "ghostty-vt",
        "ghostty" | "libghostty" => {
            if let Some(script) = script.as_deref() {
                if run_script(&app, script, "ghostty")? {
                    return Ok(());
                }
            }
            return run_ghostty_renderer(app, gtk_single_instance);
        }
        other => return Err(anyhow!("unknown app renderer: {other}")),
    };

    if let Some(script) = script {
        run_script(&app, &script, default_renderer_backend).map(|_| ())
    } else {
        run_repl(&app, default_renderer_backend)
    }
}

fn app_renderer_from_cli_or_env(
    cli_renderer: Option<&str>,
    env_renderer: Option<&str>,
) -> Result<String> {
    if let Some(renderer) = cli_renderer {
        return normalize_app_renderer(renderer)
            .map(str::to_string)
            .ok_or_else(|| anyhow!("unknown app renderer: {renderer}"));
    }

    if let Some(renderer) = env_renderer {
        return normalize_app_renderer(renderer)
            .map(str::to_string)
            .ok_or_else(|| {
                anyhow!("{APP_RENDERER_ENV} requires core, gtk, ghostty, or ghostty-vt")
            });
    }

    Ok("core".to_string())
}

fn gtk_single_instance_mode(socket_configured: bool, value: Option<&str>) -> Result<bool> {
    match value.map(str::trim).filter(|value| !value.is_empty()) {
        Some("1" | "true" | "yes" | "on") => Ok(true),
        Some("0" | "false" | "no" | "off") => Ok(false),
        Some(value) => Err(anyhow!(
            "{GTK_SINGLE_INSTANCE_ENV} must be true or false (got: {value})"
        )),
        None => Ok(!socket_configured),
    }
}

fn normalize_app_renderer(renderer: &str) -> Option<&'static str> {
    match renderer {
        "core" => Some("core"),
        "gtk" | "gtk4" => Some("gtk"),
        "ghostty-vt" | "libghostty-vt" | "vt" => Some("ghostty-vt"),
        "ghostty" | "libghostty" => Some("ghostty"),
        _ => None,
    }
}

fn open_startup_targets(app: &Arc<Mutex<AppState>>, targets: &[String]) -> Result<Value> {
    let payload = open_targets_payload(targets)?;
    let mut app = app.lock().map_err(|_| anyhow!("app state lock poisoned"))?;
    app_call(&mut app, "open.targets", payload)
}

fn terminal_startup_mode_for_renderer(renderer: &str) -> Result<TerminalStartupMode> {
    match renderer {
        "core" | "gtk" | "gtk4" | "ghostty-vt" | "libghostty-vt" | "vt" => {
            Ok(TerminalStartupMode::CorePty)
        }
        "ghostty" | "libghostty" => Ok(TerminalStartupMode::RendererOwned),
        other => Err(anyhow!("unknown app renderer: {other}")),
    }
}

fn run_script(
    app: &Arc<Mutex<AppState>>,
    script: &str,
    default_renderer_backend: &str,
) -> Result<bool> {
    for raw in script.lines() {
        let line = raw.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let outcome = execute_shared_command(app, line, default_renderer_backend)?;
        if !outcome.output.is_empty() {
            println!("{}", outcome.output);
        }
        if outcome.should_quit {
            return Ok(true);
        }
    }
    Ok(false)
}

fn run_repl(app: &Arc<Mutex<AppState>>, default_renderer_backend: &str) -> Result<()> {
    println!("cmux Linux app shell");
    println!("Type 'help' for commands. Type 'quit' to exit.");
    print_status(app)?;

    let stdin = io::stdin();
    let mut stdout = io::stdout();
    print!("cmux> ");
    stdout.flush()?;
    for line in stdin.lock().lines() {
        let line = line?;
        let outcome = execute_shared_command(app, line.trim(), default_renderer_backend)?;
        if !outcome.output.is_empty() {
            println!("{}", outcome.output);
        }
        if outcome.should_quit {
            break;
        }
        print!("cmux> ");
        stdout.flush()?;
    }
    Ok(())
}

#[cfg(feature = "gtk")]
fn run_gtk_renderer(app: Arc<Mutex<AppState>>, single_instance: bool) -> Result<()> {
    crate::gtk_ui::run_gtk_app(app, single_instance)
}

#[cfg(not(feature = "gtk"))]
fn run_gtk_renderer(_app: Arc<Mutex<AppState>>, _single_instance: bool) -> Result<()> {
    Err(anyhow!(
        "cmux was built without the GTK renderer feature; rebuild with `cargo run --features gtk -- app --renderer gtk`"
    ))
}

#[cfg(feature = "gtk")]
fn run_ghostty_renderer(app: Arc<Mutex<AppState>>, single_instance: bool) -> Result<()> {
    crate::gtk_ui::run_gtk_app_with_ghostty(app, single_instance)
}

#[cfg(not(feature = "gtk"))]
fn run_ghostty_renderer(_app: Arc<Mutex<AppState>>, _single_instance: bool) -> Result<()> {
    Err(anyhow!(
        "cmux has a Ghostty Linux embedding runtime loader, but hosting a Ghostty GL surface requires building with `cargo run --features gtk -- app --renderer ghostty`; use `--renderer ghostty-vt` for the portable Ghostty VT backend"
    ))
}

struct ShellCommandOutcome {
    output: String,
    should_quit: bool,
}

fn execute_shared_command(
    app: &Arc<Mutex<AppState>>,
    line: &str,
    default_renderer_backend: &str,
) -> Result<ShellCommandOutcome> {
    let trimmed = line.trim();
    if let Some(duration) = sleep_duration(trimmed)? {
        thread::sleep(duration);
        return Ok(shell_output(""));
    }
    let mut app = app.lock().map_err(|_| anyhow!("app state lock poisoned"))?;
    execute_shell_command(&mut app, trimmed, default_renderer_backend)
}

fn execute_shell_command(
    app: &mut AppState,
    line: &str,
    default_renderer_backend: &str,
) -> Result<ShellCommandOutcome> {
    let trimmed = line.trim();
    if trimmed.is_empty() {
        return Ok(shell_output(""));
    }

    let mut parts = trimmed.split_whitespace();
    let command = parts.next().unwrap_or_default();
    let rest = trimmed
        .get(command.len()..)
        .map(str::trim_start)
        .unwrap_or_default();

    match command {
        "help" | "?" => Ok(shell_output(app_help_text())),
        "quit" | "exit" => Ok(ShellCommandOutcome {
            output: "bye".to_string(),
            should_quit: true,
        }),
        "status" | "identify" => {
            let value = app_call(app, "system.identify", json!({}))?;
            Ok(shell_output(format_context(&value)))
        }
        "windows" => {
            let value = app_call(app, "window.list", json!({}))?;
            Ok(shell_output(format_list(&value["windows"], "window")))
        }
        "displays" | "window-displays" => {
            let value = app_call(app, "window.displays", json!({}))?;
            Ok(shell_output(format_displays(&value)))
        }
        "window-display" => {
            if shell_has_flag(rest, "--list") {
                let value = app_call(app, "window.displays", json!({}))?;
                return Ok(shell_output(format_displays(&value)));
            }
            let value = app_call(app, "window.display", window_display_shell_params(rest)?)?;
            Ok(shell_output(format_window_display(&value)))
        }
        "current-window" => {
            let value = app_call(app, "window.current", json!({}))?;
            Ok(shell_output(format!(
                "window={}",
                ref_or_id(&value, "window")
            )))
        }
        "new-window" => {
            let title = rest.trim();
            let params = if title.is_empty() {
                json!({})
            } else {
                json!({"title": title})
            };
            let value = app_call(app, "window.create", params)?;
            Ok(shell_output(format_window_created(&value)))
        }
        "focus-window" | "select-window" => {
            let target = parts
                .next()
                .ok_or_else(|| anyhow!("focus-window requires a window id, ref, or index"))?;
            app_call(app, "window.focus", json!({"window_id": target}))?;
            Ok(shell_output(format!("focused window={target}")))
        }
        "close-window" => {
            let target = parts
                .next()
                .ok_or_else(|| anyhow!("close-window requires a window id, ref, or index"))?;
            let value = app_call(app, "window.close", json!({"window_id": target}))?;
            Ok(shell_output(format_closed("window", Some(target), &value)))
        }
        "workspaces" | "tabs" => {
            let value = app_call(app, "workspace.list", json!({}))?;
            Ok(shell_output(format_list(&value["workspaces"], "workspace")))
        }
        "current-workspace" => {
            let value = app_call(app, "workspace.current", json!({}))?;
            Ok(shell_output(format!(
                "workspace={}",
                ref_or_id(&value, "workspace")
            )))
        }
        "panes" => {
            let value = app_call(app, "pane.list", json!({}))?;
            Ok(shell_output(format_list(&value["panes"], "pane")))
        }
        "focus-pane" => {
            let target = parts
                .next()
                .ok_or_else(|| anyhow!("focus-pane requires a pane id, ref, or index"))?;
            let value = app_call(app, "pane.focus", json!({"pane_id": target}))?;
            Ok(shell_output(format!(
                "focused pane={}",
                ref_or_id(&value, "pane")
            )))
        }
        "last-pane" => {
            let value = app_call(app, "pane.last", json!({}))?;
            Ok(shell_output(format!(
                "focused pane={}",
                ref_or_id(&value, "pane")
            )))
        }
        "surfaces" | "tabsurfaces" => {
            let value = app_call(app, "surface.list", json!({}))?;
            Ok(shell_output(format_list(&value["surfaces"], "surface")))
        }
        "new-workspace" | "new-tab" => {
            let title = rest.trim();
            let params = if title.is_empty() {
                json!({})
            } else {
                json!({"title": title})
            };
            let value = app_call(app, "workspace.create", params)?;
            let workspace_id = value
                .get("workspace_id")
                .and_then(Value::as_str)
                .ok_or_else(|| anyhow!("workspace.create returned no workspace_id"))?;
            app_call(
                app,
                "workspace.select",
                json!({"workspace_id": workspace_id}),
            )?;
            Ok(shell_output(format!(
                "workspace={} selected",
                value
                    .get("workspace_ref")
                    .and_then(Value::as_str)
                    .unwrap_or(workspace_id)
            )))
        }
        "select" => {
            let target = parts
                .next()
                .ok_or_else(|| anyhow!("select requires a workspace id, ref, or index"))?;
            app_call(app, "workspace.select", json!({"workspace_id": target}))?;
            Ok(shell_output("selected workspace"))
        }
        "next-workspace" | "next-tab" => {
            let value = app_call(app, "workspace.next", json!({}))?;
            Ok(shell_output(format_selected_workspace(&value)))
        }
        "previous-workspace" | "prev-workspace" | "previous-tab" | "prev-tab" => {
            let value = app_call(app, "workspace.previous", json!({}))?;
            Ok(shell_output(format_selected_workspace(&value)))
        }
        "last-workspace" | "last-tab" => {
            let value = app_call(app, "workspace.last", json!({}))?;
            Ok(shell_output(format_selected_workspace(&value)))
        }
        "close-workspace" => {
            let target = parts
                .next()
                .ok_or_else(|| anyhow!("close-workspace requires a workspace id, ref, or index"))?;
            let value = app_call(app, "workspace.close", json!({"workspace_id": target}))?;
            Ok(shell_output(format_closed(
                "workspace",
                Some(target),
                &value,
            )))
        }
        "rename-workspace" | "rename-window" => {
            let params = workspace_rename_shell_params(rest)?;
            let title = params
                .get("title")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string();
            let value = app_call(app, "workspace.rename", Value::Object(params))?;
            Ok(shell_output(format_workspace_renamed(&value, &title)))
        }
        "workspace-action" => {
            let value = app_call(
                app,
                "workspace.action",
                workspace_action_shell_params(rest)?,
            )?;
            Ok(shell_output(format_workspace_action(&value)))
        }
        "split" => {
            let direction = parts.next().unwrap_or("right");
            let value = app_call(app, "surface.split", json!({"direction": direction}))?;
            Ok(shell_output(format_created("surface", &value)))
        }
        "terminal" | "new-terminal" => {
            let value = app_call(app, "surface.create", json!({"type": "terminal"}))?;
            Ok(shell_output(format_created("surface", &value)))
        }
        "browser" => {
            let url = if rest.trim().is_empty() {
                "about:blank"
            } else {
                rest.trim()
            };
            let value = app_call(
                app,
                "browser.open_split",
                json!({"url": url, "focus": true}),
            )?;
            Ok(shell_output(format_created("browser", &value)))
        }
        "open" => execute_open_shell_command(app, rest),
        "focus-surface" | "focus-tab" => {
            let target = parts
                .next()
                .ok_or_else(|| anyhow!("focus-surface requires a surface id, ref, or index"))?;
            app_call(app, "surface.focus", json!({"surface_id": target}))?;
            Ok(shell_output("focused surface"))
        }
        "current-surface" | "current-tab" => {
            let value = app_call(app, "surface.current", json!({}))?;
            Ok(shell_output(format_created("surface", &value)))
        }
        "close-surface" | "close-tab" => {
            let target = parts.next();
            let params = target
                .map(|surface| json!({"surface_id": surface}))
                .unwrap_or_else(|| json!({}));
            let value = app_call(app, "surface.close", params)?;
            Ok(shell_output(format_closed("surface", target, &value)))
        }
        "rename-tab" | "rename-surface" => {
            let value = app_call(
                app,
                "tab.action",
                tab_action_shell_params(rest, Some("rename"))?,
            )?;
            Ok(shell_output(format_tab_action(&value)))
        }
        "tab-action" | "surface-action" => {
            let value = app_call(app, "tab.action", tab_action_shell_params(rest, None)?)?;
            Ok(shell_output(format_tab_action(&value)))
        }
        "move-tab-to-new-workspace" | "detach-tab" => {
            let value = app_call(
                app,
                "tab.action",
                tab_action_shell_params(rest, Some("move-to-new-workspace"))?,
            )?;
            Ok(shell_output(format_tab_action(&value)))
        }
        "pin-tab" | "pin-surface" => {
            let value = app_call(app, "tab.action", tab_action_alias_params(rest, "pin")?)?;
            Ok(shell_output(format_tab_action(&value)))
        }
        "unpin-tab" | "unpin-surface" => {
            let value = app_call(app, "tab.action", tab_action_alias_params(rest, "unpin")?)?;
            Ok(shell_output(format_tab_action(&value)))
        }
        "mark-tab-read" | "mark-surface-read" => {
            let value = app_call(
                app,
                "tab.action",
                tab_action_alias_params(rest, "mark-read")?,
            )?;
            Ok(shell_output(format_tab_action(&value)))
        }
        "mark-tab-unread" | "mark-surface-unread" => {
            let value = app_call(
                app,
                "tab.action",
                tab_action_alias_params(rest, "mark-unread")?,
            )?;
            Ok(shell_output(format_tab_action(&value)))
        }
        "send" => {
            if rest.is_empty() {
                return Err(anyhow!("send requires text"));
            }
            app_call(app, "surface.send_text", json!({"text": rest}))?;
            Ok(shell_output("sent"))
        }
        "enter" => {
            app_call(app, "surface.send_key", json!({"key": "enter"}))?;
            Ok(shell_output("sent enter"))
        }
        "read" => {
            let target = parts.next();
            let params = target
                .map(|surface| json!({"surface_id": surface}))
                .unwrap_or_else(|| json!({}));
            let value = app_call(app, "surface.read_text", params)?;
            Ok(shell_output(
                value
                    .get("text")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_string(),
            ))
        }
        "settings" => execute_settings_shell_command(app, rest),
        "config" => execute_config_shell_command(app, rest),
        "reload-config" => {
            let value = app_call(app, "config.reload", json!({}))?;
            Ok(shell_output(
                value
                    .get("text")
                    .and_then(Value::as_str)
                    .unwrap_or("OK Reloaded config")
                    .to_string(),
            ))
        }
        "themes" => execute_themes_shell_command(rest),
        "feed" => execute_feed_shell_command(app, rest),
        "notify" => {
            let title = rest.trim();
            if title.is_empty() {
                return Err(anyhow!("notify requires a title"));
            }
            let value = app_call(app, "notification.create", json!({"title": title}))?;
            let id = value
                .get("notification_id")
                .and_then(Value::as_str)
                .unwrap_or("suppressed");
            Ok(shell_output(format!("notification={id}")))
        }
        "notifications" | "list-notifications" => execute_notifications_shell_command(app, rest),
        "open-notification" => {
            let id = parts
                .next()
                .ok_or_else(|| anyhow!("open-notification requires a notification id"))?;
            let value = app_call(app, "notification.open", json!({"id": id}))?;
            Ok(shell_output(format_notification_action(
                "opened notification",
                &value,
            )))
        }
        "jump-unread" | "jump-to-unread" => {
            let value = app_call(app, "notification.jump_to_unread", json!({}))?;
            Ok(shell_output(format_notification_action(
                "jumped notification",
                &value,
            )))
        }
        "clear-notifications" => {
            let value = app_call(app, "notification.clear", json!({}))?;
            let cleared = value.get("cleared").and_then(Value::as_u64).unwrap_or(0);
            Ok(shell_output(format!(
                "cleared {cleared} notification{}",
                if cleared == 1 { "" } else { "s" }
            )))
        }
        "right-sidebar" | "sidebar" => execute_right_sidebar_shell_command(app, rest),
        "palette" | "command-palette" => execute_palette_shell_command(app, rest),
        "shortcuts" | "shortcut-help" => execute_shortcuts_shell_command(app, rest),
        "layout" => {
            let value = app_call(app, "debug.layout", json!({}))?;
            Ok(shell_output(serde_json::to_string_pretty(&value)?))
        }
        "renderer" | "render" => {
            let (method, params) = renderer_shell_request(rest, default_renderer_backend)?;
            let value = app_call(app, method, params)?;
            Ok(shell_output(serde_json::to_string_pretty(&value)?))
        }
        "renderer-diagnostics" | "render-diagnostics" => {
            let params = renderer_params(rest, default_renderer_backend)?;
            let value = app_call(app, "renderer.diagnostics", params)?;
            Ok(shell_output(serde_json::to_string_pretty(&value)?))
        }
        "sleep" => {
            let duration = sleep_duration(trimmed)?.unwrap_or(Duration::from_millis(0));
            thread::sleep(duration);
            Ok(shell_output(""))
        }
        other => Err(anyhow!("unknown app command: {other}")),
    }
}

fn execute_open_shell_command(app: &mut AppState, rest: &str) -> Result<ShellCommandOutcome> {
    let targets = shell_words(rest)?
        .into_iter()
        .filter(|target| !target.is_empty())
        .collect::<Vec<_>>();
    if targets.is_empty() {
        return Err(anyhow!("open requires a path or URL"));
    }
    let value = app_call(app, "open.targets", open_targets_payload(&targets)?)?;
    let count = value.get("count").and_then(Value::as_u64).unwrap_or(0);
    Ok(shell_output(format!(
        "opened {count} target{}",
        if count == 1 { "" } else { "s" }
    )))
}

fn open_targets_payload(targets: &[String]) -> Result<Value> {
    let targets = targets
        .iter()
        .map(|target| open_target_value(target))
        .collect::<Result<Vec<_>>>()?;
    Ok(json!({ "targets": targets }))
}

fn open_target_value(raw: &str) -> Result<Value> {
    if is_cmux_url_target(raw) {
        return Ok(json!({"kind": "cmux", "target": raw, "url": raw}));
    }
    if is_url_target(raw) {
        return Ok(json!({"kind": "url", "target": raw, "url": raw}));
    }
    let path = absolute_open_path(raw)?;
    let metadata =
        fs::metadata(&path).with_context(|| format!("open target does not exist: {raw}"))?;
    let path_text = path.to_string_lossy().to_string();
    if metadata.is_dir() {
        Ok(json!({"kind": "directory", "target": raw, "path": path_text}))
    } else {
        Ok(json!({
            "kind": "file",
            "target": raw,
            "path": path_text,
            "url": file_url::file_url_for_path(&path_text)
        }))
    }
}

fn absolute_open_path(raw: &str) -> Result<PathBuf> {
    let expanded = if raw == "~" {
        std::env::var("HOME")
            .map(PathBuf::from)
            .context("HOME is required to expand ~")?
    } else if let Some(rest) = raw.strip_prefix("~/") {
        std::env::var("HOME")
            .map(PathBuf::from)
            .context("HOME is required to expand ~/")?
            .join(rest)
    } else {
        PathBuf::from(raw)
    };
    let path = if expanded.is_absolute() {
        expanded
    } else {
        std::env::current_dir()?.join(expanded)
    };
    Ok(path.canonicalize().unwrap_or(path))
}

fn is_url_target(value: &str) -> bool {
    value.contains("://") || value.starts_with("about:")
}

fn is_cmux_url_target(value: &str) -> bool {
    value
        .get(..7)
        .is_some_and(|prefix| prefix.eq_ignore_ascii_case("cmux://"))
}

fn window_display_shell_params(rest: &str) -> Result<Value> {
    let mut display_parts = Vec::new();
    let mut window: Option<&str> = None;
    let parts = rest.split_whitespace().collect::<Vec<_>>();
    let mut index = 0;
    while index < parts.len() {
        match parts[index] {
            "--window" | "-w" => {
                index += 1;
                window = Some(parts.get(index).copied().ok_or_else(|| {
                    anyhow!("window-display --window requires a window id, ref, or index")
                })?);
            }
            other => display_parts.push(other),
        }
        index += 1;
    }
    let display = display_parts.join(" ");
    if display.trim().is_empty() {
        return Err(anyhow!("window-display requires a display name or index"));
    }

    let mut params = Map::new();
    params.insert("display".to_string(), json!(display));
    if let Some(window) = window {
        params.insert("window_id".to_string(), json!(window));
    }
    Ok(Value::Object(params))
}

fn workspace_rename_shell_params(rest: &str) -> Result<Map<String, Value>> {
    let mut params = workspace_action_shell_map(rest, Some("rename"))?;
    if params.get("title").is_none() {
        return Err(anyhow!("rename-workspace requires a title"));
    }
    params.remove("action");
    Ok(params)
}

fn workspace_action_shell_params(rest: &str) -> Result<Value> {
    workspace_action_shell_map(rest, None).map(Value::Object)
}

fn workspace_action_shell_map(
    rest: &str,
    forced_action: Option<&str>,
) -> Result<Map<String, Value>> {
    let parts = rest.split_whitespace().collect::<Vec<_>>();
    let mut params = Map::new();
    let mut positional = Vec::new();
    let mut index = 0;
    while index < parts.len() {
        match parts[index] {
            "--action" => {
                let value = shell_option_value(&parts, &mut index, "--action")?;
                params.insert("action".to_string(), json!(value));
            }
            "--workspace" | "--workspace-id" => {
                let flag = parts[index];
                let value = shell_option_value(&parts, &mut index, flag)?;
                params.insert("workspace_id".to_string(), shell_scalar(value));
            }
            "--window" | "--window-id" => {
                let flag = parts[index];
                let value = shell_option_value(&parts, &mut index, flag)?;
                params.insert("window_id".to_string(), shell_scalar(value));
            }
            "--title" => {
                let value = shell_option_text_value(&parts, &mut index, "--title")?;
                params.insert("title".to_string(), json!(value));
            }
            "--description" => {
                let value = shell_option_text_value(&parts, &mut index, "--description")?;
                params.insert("description".to_string(), json!(value));
            }
            "--color" => {
                let value = shell_option_value(&parts, &mut index, "--color")?;
                params.insert("color".to_string(), json!(value));
            }
            "--" => {
                positional.extend(parts[index + 1..].iter().map(|value| value.to_string()));
                break;
            }
            other if other.starts_with("--") => {
                return Err(anyhow!("workspace-action: unknown flag '{other}'"));
            }
            other => {
                positional.push(other.to_string());
                index += 1;
            }
        }
    }

    if let Some(action) = forced_action {
        if let Some(requested) = params.get("action").and_then(Value::as_str) {
            if requested != action {
                return Err(anyhow!(
                    "workspace action alias requires action {action}, got {requested}"
                ));
            }
        }
        params.insert("action".to_string(), json!(action));
    } else if params.get("action").is_none() && !positional.is_empty() {
        params.insert("action".to_string(), json!(positional.remove(0)));
    }

    let action = params
        .get("action")
        .and_then(Value::as_str)
        .map(|action| action.replace('-', "_"));
    match action.as_deref() {
        Some("rename") if params.get("title").is_none() => {
            if let Some(value) = positional_payload(&positional) {
                params.insert("title".to_string(), json!(value));
            }
        }
        Some("set_description") if params.get("description").is_none() => {
            if let Some(value) = positional_payload(&positional) {
                params.insert("description".to_string(), json!(value));
            }
        }
        Some("set_color") if params.get("color").is_none() => {
            if let Some(value) = positional_payload(&positional) {
                params.insert("color".to_string(), json!(value));
            }
        }
        _ => {}
    }

    if params.get("action").is_none() {
        return Err(anyhow!("workspace-action requires an action"));
    }
    add_shell_env_default(&mut params, "workspace_id", "CMUX_WORKSPACE_ID");
    Ok(params)
}

fn tab_action_alias_params(rest: &str, action: &str) -> Result<Value> {
    let mut params = tab_action_shell_map(rest, Some(action))?;
    if params.get("surface_id").is_none()
        && params.get("tab_id").is_none()
        && !rest.trim().is_empty()
        && !rest.trim_start().starts_with("--")
    {
        if let Some(target) = rest.split_whitespace().next() {
            params.insert("surface_id".to_string(), shell_scalar(target));
        }
    }
    Ok(Value::Object(params))
}

fn tab_action_shell_params(rest: &str, forced_action: Option<&str>) -> Result<Value> {
    tab_action_shell_map(rest, forced_action).map(Value::Object)
}

fn tab_action_shell_map(rest: &str, forced_action: Option<&str>) -> Result<Map<String, Value>> {
    let parts = rest.split_whitespace().collect::<Vec<_>>();
    let mut params = Map::new();
    let mut positional = Vec::new();
    let mut index = 0;
    while index < parts.len() {
        match parts[index] {
            "--action" => {
                let value = shell_option_value(&parts, &mut index, "--action")?;
                params.insert("action".to_string(), json!(value));
            }
            "--workspace" | "--workspace-id" => {
                let flag = parts[index];
                let value = shell_option_value(&parts, &mut index, flag)?;
                params.insert("workspace_id".to_string(), shell_scalar(value));
            }
            "--window" | "--window-id" => {
                let flag = parts[index];
                let value = shell_option_value(&parts, &mut index, flag)?;
                params.insert("window_id".to_string(), shell_scalar(value));
            }
            "--tab" | "--tab-id" => {
                let flag = parts[index];
                let value = shell_option_value(&parts, &mut index, flag)?;
                params.insert("tab_id".to_string(), shell_scalar(value));
            }
            "--surface" | "--surface-id" | "--panel" | "--panel-id" => {
                let flag = parts[index];
                let value = shell_option_value(&parts, &mut index, flag)?;
                params.insert("surface_id".to_string(), shell_scalar(value));
            }
            "--title" => {
                let value = shell_option_text_value(&parts, &mut index, "--title")?;
                params.insert("title".to_string(), json!(value));
            }
            "--url" => {
                let value = shell_option_text_value(&parts, &mut index, "--url")?;
                params.insert("url".to_string(), json!(value));
            }
            "--focus" => {
                if let Some(value) = parts
                    .get(index + 1)
                    .filter(|value| !value.starts_with("--"))
                {
                    params.insert("focus".to_string(), shell_scalar(value));
                    index += 2;
                } else {
                    params.insert("focus".to_string(), json!(true));
                    index += 1;
                }
            }
            "--no-focus" => {
                params.insert("focus".to_string(), json!(false));
                index += 1;
            }
            "--" => {
                positional.extend(parts[index + 1..].iter().map(|value| value.to_string()));
                break;
            }
            other if other.starts_with("--") => {
                return Err(anyhow!("tab-action: unknown flag '{other}'"));
            }
            other => {
                positional.push(other.to_string());
                index += 1;
            }
        }
    }

    if let Some(action) = forced_action {
        if let Some(requested) = params.get("action").and_then(Value::as_str) {
            if requested != action {
                return Err(anyhow!(
                    "tab action alias requires action {action}, got {requested}"
                ));
            }
        }
        params.insert("action".to_string(), json!(action));
    } else if params.get("action").is_none() && !positional.is_empty() {
        params.insert("action".to_string(), json!(positional.remove(0)));
    }

    let action = params
        .get("action")
        .and_then(Value::as_str)
        .map(|action| action.replace('-', "_"));
    match action.as_deref() {
        Some("rename") if params.get("title").is_none() => {
            if let Some(value) = positional_payload(&positional) {
                params.insert("title".to_string(), json!(value));
            }
        }
        Some("move_to_new_workspace") if params.get("title").is_none() => {
            if let Some(value) = positional_payload(&positional) {
                params.insert("title".to_string(), json!(value));
            }
        }
        _ => {}
    }

    if params.get("action").is_none() {
        return Err(anyhow!("tab-action requires an action"));
    }
    add_shell_env_default(&mut params, "workspace_id", "CMUX_WORKSPACE_ID");
    add_shell_env_default(&mut params, "tab_id", "CMUX_TAB_ID");
    add_shell_surface_env_default(&mut params);
    Ok(params)
}

fn shell_option_value<'a>(parts: &[&'a str], index: &mut usize, flag: &str) -> Result<&'a str> {
    *index += 1;
    let value = *parts
        .get(*index)
        .filter(|value| !value.starts_with("--"))
        .ok_or_else(|| anyhow!("{flag} requires a value"))?;
    *index += 1;
    Ok(value)
}

fn shell_option_text_value(parts: &[&str], index: &mut usize, flag: &str) -> Result<String> {
    *index += 1;
    let mut words = Vec::new();
    while let Some(value) = parts.get(*index) {
        if value.starts_with("--") {
            break;
        }
        words.push((*value).to_string());
        *index += 1;
    }
    if words.is_empty() {
        return Err(anyhow!("{flag} requires a value"));
    }
    Ok(words.join(" "))
}

fn positional_payload(parts: &[String]) -> Option<String> {
    (!parts.is_empty())
        .then(|| parts.join(" "))
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

fn add_shell_env_default(params: &mut Map<String, Value>, key: &str, env_key: &str) {
    if params.get(key).is_none() {
        if let Ok(value) = std::env::var(env_key) {
            if !value.trim().is_empty() {
                params.insert(key.to_string(), json!(value));
            }
        }
    }
}

fn add_shell_surface_env_default(params: &mut Map<String, Value>) {
    add_shell_env_default(params, "surface_id", "CMUX_PANEL_ID");
    add_shell_env_default(params, "surface_id", "CMUX_SURFACE_ID");
}

fn execute_right_sidebar_shell_command(
    app: &mut AppState,
    rest: &str,
) -> Result<ShellCommandOutcome> {
    let mut parts = rest.split_whitespace();
    let subcommand = parts.next().unwrap_or("mode");
    let params = match subcommand {
        "toggle" | "show" | "hide" | "focus" | "mode" => json!({"action": subcommand}),
        "set" => {
            let mode = parts
                .next()
                .ok_or_else(|| anyhow!("right-sidebar set requires a mode"))?;
            let no_focus = parts.any(|part| part == "--no-focus");
            json!({"action": "set", "mode": mode, "no_focus": no_focus})
        }
        "files" | "find" | "vault" | "sessions" | "feed" | "dock" => {
            json!({"action": "set", "mode": subcommand})
        }
        other => return Err(anyhow!("unknown right-sidebar command: {other}")),
    };
    let value = app_call(app, "sidebar.right", params)?;
    Ok(shell_output(format_right_sidebar_state(&value)))
}

fn execute_palette_shell_command(app: &mut AppState, rest: &str) -> Result<ShellCommandOutcome> {
    let mut parts = rest.split_whitespace();
    let subcommand = parts.next().unwrap_or("results");
    match subcommand {
        "toggle" | "switcher" => {
            let value = app_call(app, "debug.shortcut.simulate", json!({"combo": "cmd+p"}))?;
            Ok(shell_output(format_palette_visibility(&value)))
        }
        "commands" => {
            let value = app_call(
                app,
                "debug.shortcut.simulate",
                json!({"combo": "cmd+shift+p"}),
            )?;
            Ok(shell_output(format_palette_visibility(&value)))
        }
        "type" => {
            let text = parts.collect::<Vec<_>>().join(" ");
            if text.is_empty() {
                return Err(anyhow!("palette type requires text"));
            }
            let value = app_call(app, "debug.type", json!({"text": text}))?;
            Ok(shell_output(format_palette_visibility(&value)))
        }
        "enter" => {
            let value = app_call(app, "debug.shortcut.simulate", json!({"combo": "enter"}))?;
            Ok(shell_output(serde_json::to_string_pretty(&value)?))
        }
        "results" => {
            let value = app_call(app, "debug.command_palette.results", json!({"limit": 8}))?;
            Ok(shell_output(format_palette_results(&value)))
        }
        other => Err(anyhow!("unknown palette command: {other}")),
    }
}

fn execute_shortcuts_shell_command(app: &mut AppState, rest: &str) -> Result<ShellCommandOutcome> {
    let subcommand = rest.split_whitespace().next().unwrap_or("toggle");
    let value = match subcommand {
        "state" => app_call(app, "help.shortcuts", json!({}))?,
        "toggle" => app_call(app, "help.shortcuts.toggle", json!({}))?,
        "show" => app_call(app, "help.shortcuts.toggle", json!({"visible": true}))?,
        "hide" => app_call(app, "help.shortcuts.toggle", json!({"visible": false}))?,
        other => return Err(anyhow!("unknown shortcuts command: {other}")),
    };
    let visible = value
        .get("visible")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let row_count = value
        .get("rows")
        .and_then(Value::as_array)
        .map(Vec::len)
        .unwrap_or(0);
    Ok(shell_output(format_shortcut_help(
        &value, visible, row_count,
    )))
}

fn execute_feed_shell_command(app: &mut AppState, rest: &str) -> Result<ShellCommandOutcome> {
    let mut parts = rest.split_whitespace();
    let subcommand = parts.next().unwrap_or("list");
    match subcommand {
        "list" | "pending" | "--pending-only" | "--pending" => {
            let pending_only = subcommand == "pending"
                || subcommand == "--pending-only"
                || subcommand == "--pending"
                || parts.any(|part| matches!(part, "--pending-only" | "--pending"));
            let value = app_call(app, "feed.list", json!({"pending_only": pending_only}))?;
            Ok(shell_output(format_feed_items(&value)))
        }
        "clear" => {
            let confirmed = parts.any(|part| matches!(part, "--yes" | "-y"));
            if !confirmed {
                return Err(anyhow!("feed clear requires --yes"));
            }
            let value = app_call(app, "feed.clear", json!({}))?;
            let removed = value.get("removed").and_then(Value::as_u64).unwrap_or(0);
            Ok(shell_output(format!(
                "cleared {removed} feed item{}",
                if removed == 1 { "" } else { "s" }
            )))
        }
        "jump" | "open" => {
            let workstream_id = parts
                .next()
                .ok_or_else(|| anyhow!("feed jump requires a workstream id"))?;
            let value = app_call(app, "feed.jump", json!({"workstream_id": workstream_id}))?;
            let opened = value
                .get("opened")
                .and_then(Value::as_bool)
                .unwrap_or(false);
            Ok(shell_output(if opened {
                format!("opened feed workstream {workstream_id}")
            } else {
                format!("no workspace matched feed workstream {workstream_id}")
            }))
        }
        other => Err(anyhow!("unknown feed command: {other}")),
    }
}

fn execute_notifications_shell_command(
    app: &mut AppState,
    rest: &str,
) -> Result<ShellCommandOutcome> {
    let mut parts = rest.split_whitespace();
    let subcommand = parts.next().unwrap_or("list");
    match subcommand {
        "list" => {
            let value = app_call(app, "notification.list", json!({}))?;
            Ok(shell_output(format_notifications(&value)))
        }
        "open" => {
            let id = parts
                .next()
                .ok_or_else(|| anyhow!("notifications open requires a notification id"))?;
            let value = app_call(app, "notification.open", json!({"id": id}))?;
            Ok(shell_output(format_notification_action(
                "opened notification",
                &value,
            )))
        }
        "jump" | "jump-unread" => {
            let value = app_call(app, "notification.jump_to_unread", json!({}))?;
            Ok(shell_output(format_notification_action(
                "jumped notification",
                &value,
            )))
        }
        "clear" => {
            let confirmed = parts.any(|part| matches!(part, "--yes" | "-y"));
            if !confirmed {
                return Err(anyhow!("notifications clear requires --yes"));
            }
            let value = app_call(app, "notification.clear", json!({}))?;
            let cleared = value.get("cleared").and_then(Value::as_u64).unwrap_or(0);
            Ok(shell_output(format!(
                "cleared {cleared} notification{}",
                if cleared == 1 { "" } else { "s" }
            )))
        }
        other => Err(anyhow!("unknown notifications command: {other}")),
    }
}

fn execute_settings_shell_command(app: &mut AppState, rest: &str) -> Result<ShellCommandOutcome> {
    let subcommand = rest.split_whitespace().next().unwrap_or("open");
    if matches!(subcommand, "path" | "paths" | "docs" | "documentation") {
        let payload = config::settings_docs_payload();
        return if shell_has_flag(rest, "--json") {
            Ok(shell_output(serde_json::to_string_pretty(&payload)?))
        } else {
            Ok(shell_output(format_settings_docs(&payload)))
        };
    }

    let params = settings_shell_params(rest)?;
    let value = app_call(app, "settings.open", params)?;
    Ok(shell_output(format_settings_opened(&value)))
}

fn settings_shell_params(rest: &str) -> Result<Value> {
    let parts = rest.split_whitespace().collect::<Vec<_>>();
    let mut params = Map::new();
    let mut target = None;
    let mut index = usize::from(parts.first() == Some(&"open"));

    while index < parts.len() {
        let arg = parts[index];
        match arg {
            "--target" => {
                index += 1;
                let raw = parts
                    .get(index)
                    .ok_or_else(|| anyhow!("settings --target requires a value"))?;
                target = Some(canonical_settings_target(raw).ok_or_else(|| {
                    anyhow!("Unknown settings target '{raw}'. Run 'cmux app --help'.")
                })?);
            }
            "--workspace" | "--workspace-id" => {
                index += 1;
                params.insert(
                    "workspace_id".to_string(),
                    shell_scalar(
                        *parts
                            .get(index)
                            .ok_or_else(|| anyhow!("settings --workspace requires a value"))?,
                    ),
                );
            }
            "--surface" | "--surface-id" => {
                index += 1;
                params.insert(
                    "surface_id".to_string(),
                    shell_scalar(
                        *parts
                            .get(index)
                            .ok_or_else(|| anyhow!("settings --surface requires a value"))?,
                    ),
                );
            }
            "--pane" | "--pane-id" => {
                index += 1;
                params.insert(
                    "pane_id".to_string(),
                    shell_scalar(
                        *parts
                            .get(index)
                            .ok_or_else(|| anyhow!("settings --pane requires a value"))?,
                    ),
                );
            }
            "--window" | "--window-id" => {
                index += 1;
                params.insert(
                    "window_id".to_string(),
                    shell_scalar(
                        *parts
                            .get(index)
                            .ok_or_else(|| anyhow!("settings --window requires a value"))?,
                    ),
                );
            }
            "--focus" => {
                if let Some(value) = parts
                    .get(index + 1)
                    .filter(|value| !value.starts_with("--"))
                {
                    index += 1;
                    params.insert("focus".to_string(), shell_scalar(value));
                } else {
                    params.insert("focus".to_string(), json!(true));
                }
            }
            "--no-focus" => {
                params.insert("focus".to_string(), json!(false));
            }
            "--activate" => {
                if let Some(value) = parts
                    .get(index + 1)
                    .filter(|value| !value.starts_with("--"))
                {
                    index += 1;
                    params.insert("activate".to_string(), shell_scalar(value));
                } else {
                    params.insert("activate".to_string(), json!(true));
                }
            }
            "--no-activate" => {
                params.insert("activate".to_string(), json!(false));
            }
            "--json" | "--" => {}
            other if other.starts_with("--") => {
                return Err(anyhow!("settings: unknown flag '{other}'"));
            }
            other if target.is_none() => {
                target = Some(canonical_settings_target(other).ok_or_else(|| {
                    anyhow!("Unknown settings target '{other}'. Run 'cmux app --help'.")
                })?);
            }
            other => return Err(anyhow!("settings: unexpected argument '{other}'")),
        }
        index += 1;
    }

    if let Some(target) = target {
        params.insert("target".to_string(), json!(target));
    }
    Ok(Value::Object(params))
}

fn execute_config_shell_command(app: &mut AppState, rest: &str) -> Result<ShellCommandOutcome> {
    let parts = rest.split_whitespace().collect::<Vec<_>>();
    let subcommand = parts
        .iter()
        .copied()
        .find(|part| !part.starts_with("--"))
        .unwrap_or("path");
    match subcommand {
        "path" | "paths" | "snapshot" | "status" | "" => {
            let snapshot = config::snapshot();
            if shell_has_flag(rest, "--json") {
                Ok(shell_output(serde_json::to_string_pretty(&snapshot)?))
            } else {
                Ok(shell_output(format_config_snapshot(&snapshot, false)))
            }
        }
        "docs" | "documentation" => {
            let payload = config::settings_docs_payload();
            if shell_has_flag(rest, "--json") {
                Ok(shell_output(serde_json::to_string_pretty(&payload)?))
            } else {
                Ok(shell_output(format_settings_docs(&payload)))
            }
        }
        "check" | "doctor" | "validate" => {
            let (paths, json_output) = config_doctor_shell_args(&parts[1..])?;
            let report = config::doctor(&paths).map_err(anyhow::Error::msg)?;
            if json_output {
                Ok(shell_output(serde_json::to_string_pretty(&report)?))
            } else {
                Ok(shell_output(format_config_doctor(&report)))
            }
        }
        "reload" => {
            let value = app_call(app, "config.reload", json!({}))?;
            Ok(shell_output(
                value
                    .get("text")
                    .and_then(Value::as_str)
                    .unwrap_or("OK Reloaded config")
                    .to_string(),
            ))
        }
        "get" => {
            let key = parts
                .get(1)
                .and_then(|raw| config::canonical_font_size_key(raw))
                .ok_or_else(|| {
                    anyhow!("config get requires sidebar-font-size or surface-tab-bar-font-size")
                })?;
            let payload = config::get_font_size(key).map_err(anyhow::Error::msg)?;
            if shell_has_flag(rest, "--json") {
                Ok(shell_output(serde_json::to_string_pretty(&payload)?))
            } else {
                Ok(shell_output(format_font_size_get(&payload)))
            }
        }
        "set" => {
            let key = parts
                .get(1)
                .and_then(|raw| config::canonical_font_size_key(raw))
                .ok_or_else(|| {
                    anyhow!("config set requires sidebar-font-size or surface-tab-bar-font-size")
                })?;
            let value = parts
                .get(2)
                .ok_or_else(|| anyhow!("config set {key} requires a value"))?;
            let payload = set_font_size_and_reload(app, key, value)?;
            if shell_has_flag(rest, "--json") {
                Ok(shell_output(serde_json::to_string_pretty(&payload)?))
            } else {
                Ok(shell_output(format_font_size_set(&payload)))
            }
        }
        other if config::canonical_font_size_key(other).is_some() => {
            let key = config::canonical_font_size_key(other).unwrap();
            if let Some(value) = parts.get(1) {
                let payload = set_font_size_and_reload(app, key, value)?;
                Ok(shell_output(format_font_size_set(&payload)))
            } else {
                let payload = config::get_font_size(key).map_err(anyhow::Error::msg)?;
                Ok(shell_output(format_font_size_get(&payload)))
            }
        }
        other => Err(anyhow!("unknown config command: {other}")),
    }
}

fn execute_themes_shell_command(rest: &str) -> Result<ShellCommandOutcome> {
    let parts = rest.split_whitespace().collect::<Vec<_>>();
    let json_output = shell_has_flag(rest, "--json");
    let subcommand = parts
        .first()
        .copied()
        .filter(|part| *part != "--json")
        .unwrap_or("list");
    match subcommand {
        "list" => {
            let payload = config::themes_list_payload();
            if json_output {
                Ok(shell_output(serde_json::to_string_pretty(&payload)?))
            } else {
                Ok(shell_output(format_themes_list(&payload)))
            }
        }
        "set" => {
            let payload = parse_theme_set_shell_args(&parts[1..])
                .and_then(|(light, dark)| config::set_theme_override(light, dark))
                .map_err(anyhow::Error::msg)?;
            if json_output {
                Ok(shell_output(serde_json::to_string_pretty(&payload)?))
            } else {
                Ok(shell_output(format_theme_set(&payload)))
            }
        }
        "clear" => {
            if parts
                .iter()
                .skip(1)
                .any(|part| *part != "--json" && *part != "--")
            {
                return Err(anyhow!("themes clear does not take positional arguments"));
            }
            let payload = config::clear_theme_override().map_err(anyhow::Error::msg)?;
            if json_output {
                Ok(shell_output(serde_json::to_string_pretty(&payload)?))
            } else {
                Ok(shell_output(format_theme_clear(&payload)))
            }
        }
        other if other.starts_with("--") => {
            let payload = config::themes_list_payload();
            if json_output {
                Ok(shell_output(serde_json::to_string_pretty(&payload)?))
            } else {
                Ok(shell_output(format_themes_list(&payload)))
            }
        }
        _ => {
            let payload = parse_theme_set_shell_args(&parts)
                .and_then(|(light, dark)| config::set_theme_override(light, dark))
                .map_err(anyhow::Error::msg)?;
            if json_output {
                Ok(shell_output(serde_json::to_string_pretty(&payload)?))
            } else {
                Ok(shell_output(format_theme_set(&payload)))
            }
        }
    }
}

fn set_font_size_and_reload(
    app: &mut AppState,
    key: &str,
    raw_value: &str,
) -> Result<config::FontSizeSetPayload> {
    let mut payload = config::set_font_size(key, raw_value, "skipped".to_string(), None)
        .map_err(anyhow::Error::msg)?;
    match app_call(app, "config.reload", json!({})) {
        Ok(value) => {
            payload.reload = "reloaded".to_string();
            payload.reload_message = value
                .get("text")
                .and_then(Value::as_str)
                .map(ToString::to_string);
        }
        Err(err) => {
            payload.reload = "failed".to_string();
            payload.reload_message = Some(err.to_string());
        }
    }
    Ok(payload)
}

fn config_doctor_shell_args(parts: &[&str]) -> Result<(Vec<String>, bool)> {
    let mut paths = Vec::new();
    let mut json_output = false;
    let mut index = 0;
    while index < parts.len() {
        let arg = parts[index];
        match arg {
            "--json" => json_output = true,
            "--path" => {
                index += 1;
                paths.push(
                    parts
                        .get(index)
                        .ok_or_else(|| anyhow!("config doctor --path requires a path"))?
                        .to_string(),
                );
            }
            "--" => {}
            other if other.starts_with("--path=") => {
                let path = other.trim_start_matches("--path=");
                if path.is_empty() {
                    return Err(anyhow!("config doctor --path requires a path"));
                }
                paths.push(path.to_string());
            }
            other if other.starts_with('-') => {
                return Err(anyhow!("unknown config doctor option '{other}'"));
            }
            other => {
                return Err(anyhow!(
                    "unknown config doctor argument '{other}'. Use --path <path>."
                ));
            }
        }
        index += 1;
    }
    Ok((paths, json_output))
}

fn parse_theme_set_shell_args(parts: &[&str]) -> Result<(Option<String>, Option<String>), String> {
    let mut light = None;
    let mut dark = None;
    let mut positional = Vec::new();
    let mut index = 0;
    while index < parts.len() {
        let arg = parts[index];
        match arg {
            "--json" | "--" => {}
            "--light" => {
                index += 1;
                light = Some(
                    parts
                        .get(index)
                        .ok_or_else(|| "--light requires a value".to_string())?
                        .to_string(),
                );
            }
            "--dark" => {
                index += 1;
                dark = Some(
                    parts
                        .get(index)
                        .ok_or_else(|| "--dark requires a value".to_string())?
                        .to_string(),
                );
            }
            other if other.starts_with("--") => {
                return Err(format!(
                    "themes set: unknown flag '{other}'. Known flags: --light <theme>, --dark <theme>"
                ));
            }
            other => positional.push(other.to_string()),
        }
        index += 1;
    }

    if light.is_none() && dark.is_none() {
        let theme = positional.join(" ").trim().to_string();
        if theme.is_empty() {
            return Err("themes set requires a theme name or --light/--dark flags".to_string());
        }
        Ok((Some(theme.clone()), Some(theme)))
    } else if positional.is_empty() {
        Ok((light, dark))
    } else {
        Err(format!(
            "themes set: unexpected argument '{}'",
            positional.join(" ")
        ))
    }
}

fn app_call(app: &mut AppState, method: &str, params: Value) -> Result<Value> {
    app.handle(method, &params)
        .map_err(|err: AppError| anyhow!("{err}"))
}

fn shell_output(output: impl Into<String>) -> ShellCommandOutcome {
    ShellCommandOutcome {
        output: output.into(),
        should_quit: false,
    }
}

fn format_context(value: &Value) -> String {
    let focused = value.get("focused").unwrap_or(value);
    format!(
        "window={} workspace={} pane={} surface={}",
        ref_or_id(focused, "window"),
        ref_or_id(focused, "workspace"),
        ref_or_id(focused, "pane"),
        ref_or_id(focused, "surface")
    )
}

fn format_created(kind: &str, value: &Value) -> String {
    let workspace = ref_or_id(value, "workspace");
    let pane = ref_or_id(value, "pane");
    let surface = ref_or_id(value, "surface");
    match kind {
        "browser" => format!("browser={surface} workspace={workspace} pane={pane}"),
        _ => format!("{kind}={surface} workspace={workspace} pane={pane}"),
    }
}

fn format_window_created(value: &Value) -> String {
    let window = ref_or_id(value, "window");
    let workspace = ref_or_id(value, "workspace");
    format!("window={window} workspace={workspace} selected")
}

fn format_displays(value: &Value) -> String {
    let displays = value
        .get("displays")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    if displays.is_empty() {
        return "no displays".to_string();
    }
    displays
        .iter()
        .map(|display| {
            let index = display.get("index").and_then(Value::as_i64).unwrap_or(0);
            let selected = if display
                .get("main")
                .and_then(Value::as_bool)
                .unwrap_or(false)
            {
                "*"
            } else {
                " "
            };
            let name = display
                .get("name")
                .and_then(Value::as_str)
                .unwrap_or("Unnamed");
            let frame = display.get("frame").unwrap_or(display);
            let x = frame.get("x").and_then(Value::as_i64).unwrap_or(0);
            let y = frame.get("y").and_then(Value::as_i64).unwrap_or(0);
            let width = frame.get("width").and_then(Value::as_i64).unwrap_or(0);
            let height = frame.get("height").and_then(Value::as_i64).unwrap_or(0);
            let id = display
                .get("display_id")
                .and_then(Value::as_i64)
                .map(|id| format!(" id={id}"))
                .unwrap_or_default();
            format!("{selected} display:{index} {name} {width}x{height}+{x}+{y}{id}")
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn format_window_display(value: &Value) -> String {
    let display = value
        .get("display")
        .and_then(Value::as_str)
        .unwrap_or("unknown");
    let window = ref_or_id(value, "window");
    if window != "-" {
        format!("display={display} window={window}")
    } else {
        let moved = value
            .get("moved")
            .and_then(Value::as_array)
            .map(Vec::len)
            .unwrap_or(0);
        format!("display={display} moved={moved}")
    }
}

fn format_selected_workspace(value: &Value) -> String {
    format!("workspace={} selected", ref_or_id(value, "workspace"))
}

fn format_closed(kind: &str, requested: Option<&str>, value: &Value) -> String {
    let closed = ref_or_id(value, kind);
    if closed == "-" {
        requested
            .map(|target| format!("closed {kind}={target}"))
            .unwrap_or_else(|| format!("closed {kind}"))
    } else {
        format!("closed {kind}={closed}")
    }
}

fn format_workspace_renamed(value: &Value, title: &str) -> String {
    format!(
        "OK workspace={} title={}",
        ref_or_id(value, "workspace"),
        title
    )
}

fn format_tab_action(value: &Value) -> String {
    let action = value
        .get("action")
        .and_then(Value::as_str)
        .unwrap_or("unknown");
    let tab = value
        .get("tab_ref")
        .and_then(Value::as_str)
        .or_else(|| value.get("surface_ref").and_then(Value::as_str))
        .or_else(|| value.get("tab_id").and_then(Value::as_str))
        .or_else(|| value.get("surface_id").and_then(Value::as_str))
        .unwrap_or("");
    let mut parts = vec![format!("OK action={action}"), format!("tab={tab}")];
    push_string_field(&mut parts, value, "title", "title");
    push_bool_field(&mut parts, value, "pinned", "pinned");
    push_bool_field(&mut parts, value, "unread", "unread");
    if let Some(workspace) = value
        .get("created_workspace_ref")
        .and_then(Value::as_str)
        .or_else(|| value.get("created_workspace_id").and_then(Value::as_str))
    {
        parts.push(format!("created_workspace={workspace}"));
    }
    if let Some(created) = value
        .get("created_tab_ref")
        .and_then(Value::as_str)
        .or_else(|| value.get("created_surface_ref").and_then(Value::as_str))
        .or_else(|| value.get("created_tab_id").and_then(Value::as_str))
        .or_else(|| value.get("created_surface_id").and_then(Value::as_str))
    {
        parts.push(format!("created={created}"));
    }
    push_i64_field(&mut parts, value, "closed", "closed");
    push_i64_field(&mut parts, value, "skipped_pinned", "skipped_pinned");
    parts.join(" ")
}

fn format_workspace_action(value: &Value) -> String {
    let action = value
        .get("action")
        .and_then(Value::as_str)
        .unwrap_or("unknown");
    let workspace = ref_or_id(value, "workspace");
    let mut parts = vec![
        format!("OK action={action}"),
        format!("workspace={workspace}"),
    ];
    push_i64_field(&mut parts, value, "index", "index");
    push_string_field(&mut parts, value, "title", "title");
    push_string_field(&mut parts, value, "description", "description");
    push_string_field(&mut parts, value, "custom_color", "color");
    push_bool_field(&mut parts, value, "pinned", "pinned");
    push_bool_field(&mut parts, value, "unread", "unread");
    push_i64_field(&mut parts, value, "closed", "closed");
    push_i64_field(&mut parts, value, "skipped_pinned", "skipped_pinned");
    parts.join(" ")
}

fn push_string_field(parts: &mut Vec<String>, value: &Value, key: &str, label: &str) {
    if let Some(text) = value
        .get(key)
        .and_then(Value::as_str)
        .filter(|text| !text.is_empty())
    {
        parts.push(format!("{label}={text}"));
    }
}

fn push_bool_field(parts: &mut Vec<String>, value: &Value, key: &str, label: &str) {
    if let Some(flag) = value.get(key).and_then(Value::as_bool) {
        parts.push(format!("{label}={flag}"));
    }
}

fn push_i64_field(parts: &mut Vec<String>, value: &Value, key: &str, label: &str) {
    if let Some(number) = value.get(key).and_then(Value::as_i64) {
        parts.push(format!("{label}={number}"));
    }
}

fn format_list(value: &Value, kind: &str) -> String {
    let Some(rows) = value.as_array() else {
        return format!("no {kind}s");
    };
    if rows.is_empty() {
        return format!("no {kind}s");
    }
    rows.iter()
        .map(|row| {
            let index = row.get("index").and_then(Value::as_i64).unwrap_or(0);
            let selected = if row
                .get("selected")
                .or_else(|| row.get("focused"))
                .and_then(Value::as_bool)
                .unwrap_or(false)
            {
                "*"
            } else {
                " "
            };
            let title = row
                .get("title")
                .or_else(|| row.get("name"))
                .and_then(Value::as_str)
                .unwrap_or_default();
            let display = if kind == "window" {
                row.get("display")
                    .and_then(Value::as_str)
                    .map(|display| format!(" display={display}"))
                    .unwrap_or_default()
            } else {
                String::new()
            };
            format!(
                "{selected} {kind}:{index} {} {title}{display}",
                ref_or_id(row, kind)
            )
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn format_feed_items(value: &Value) -> String {
    let items = value
        .get("items")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    if items.is_empty() {
        return "No feed items.".to_string();
    }
    items
        .iter()
        .map(|item| {
            let status = item
                .get("status")
                .and_then(Value::as_str)
                .unwrap_or("unknown");
            let source = item.get("source").and_then(Value::as_str).unwrap_or("?");
            let kind = item.get("kind").and_then(Value::as_str).unwrap_or("?");
            let request = item
                .get("request_id")
                .or_else(|| item.get("id"))
                .and_then(Value::as_str)
                .unwrap_or("?");
            let title = item
                .get("title")
                .and_then(Value::as_str)
                .or_else(|| item.get("tool_name").and_then(Value::as_str))
                .or_else(|| item.get("question_prompt").and_then(Value::as_str))
                .or_else(|| item.get("plan_summary").and_then(Value::as_str))
                .unwrap_or("");
            if title.is_empty() {
                format!("{status}\t{source}\t{kind}\t{request}")
            } else {
                format!("{status}\t{source}\t{kind}\t{request}\t{title}")
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn format_notifications(value: &Value) -> String {
    let notifications = value
        .get("notifications")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    if notifications.is_empty() {
        return "No notifications.".to_string();
    }
    notifications
        .iter()
        .map(|notification| {
            let read = if notification
                .get("is_read")
                .or_else(|| notification.get("read"))
                .and_then(Value::as_bool)
                .unwrap_or(false)
            {
                "read"
            } else {
                "unread"
            };
            let title = notification
                .get("title")
                .and_then(Value::as_str)
                .unwrap_or("Notification");
            let id = notification
                .get("id")
                .or_else(|| notification.get("notification_id"))
                .and_then(Value::as_str)
                .unwrap_or("?");
            let workspace = ref_or_id(notification, "workspace");
            let surface = ref_or_id(notification, "surface");
            format!("{read}\t{id}\tworkspace={workspace}\tsurface={surface}\t{title}")
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn format_notification_action(prefix: &str, value: &Value) -> String {
    let id = value
        .get("id")
        .or_else(|| value.get("notification_id"))
        .and_then(Value::as_str)
        .unwrap_or("none");
    let opened = value
        .get("opened")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    format!("{prefix} {id} opened={opened}")
}

fn format_settings_opened(value: &Value) -> String {
    let target = value
        .get("target")
        .and_then(Value::as_str)
        .unwrap_or("general");
    let surface = value
        .get("surface_ref")
        .and_then(Value::as_str)
        .or_else(|| value.get("surface_id").and_then(Value::as_str));
    if let Some(surface) = surface {
        format!("OK target={target} surface={surface}")
    } else {
        format!("OK target={target}")
    }
}

fn format_config_snapshot(snapshot: &config::ConfigSnapshot, validation: bool) -> String {
    let mut out = String::new();
    out.push_str("Config files:\n");
    out.push_str(&format_config_source_line("cmux", &snapshot.cmux));
    out.push('\n');
    out.push_str(&format_config_source_line("ghostty", &snapshot.ghostty));
    out.push('\n');
    out.push_str(&format_config_source_line("synced", &snapshot.synced));
    out.push('\n');
    if snapshot.load_paths.is_empty() {
        out.push_str("Load paths: none\n");
    } else {
        out.push_str("Load paths:\n");
        for path in &snapshot.load_paths {
            out.push_str(&format!("  - {path}\n"));
        }
    }
    if validation {
        out.push_str("Validation: OK\n");
    }
    out.trim_end().to_string()
}

fn format_config_source_line(label: &str, source: &config::ConfigSourceSnapshot) -> String {
    let state = if source.has_backing_file {
        "exists"
    } else {
        "missing"
    };
    format!("  {label}: {} ({state})", source.path)
}

fn format_settings_docs(payload: &config::SettingsDocsPayload) -> String {
    [
        "Config files:".to_string(),
        format!("  primary: {}", payload.settings_files.primary),
        format!("  legacy config: {}", payload.settings_files.legacy),
        format!(
            "  {}: {}",
            config::MACOS_MIGRATION_FALLBACK_LABEL,
            payload.settings_files.fallback
        ),
        String::new(),
        "Related (not cmux-owned, but cmux reads it for terminal behavior):".to_string(),
        format!("  {}", payload.ghostty_config.path),
        format!("  {}", payload.ghostty_config.note),
        String::new(),
        "Docs:".to_string(),
        format!("  {}", payload.docs_url),
        String::new(),
        "Schema:".to_string(),
        format!("  {}", payload.schema_url),
        String::new(),
        "Before editing cmux.json:".to_string(),
        format!("  {}", payload.backup),
        String::new(),
        "Reload after editing cmux.json or Ghostty config:".to_string(),
        format!("  {}   ({})", payload.reload_command, payload.reload_scope),
    ]
    .join("\n")
}

fn format_config_doctor(report: &config::ConfigDoctorReport) -> String {
    let mut out = String::from("cmux config doctor\n");
    for finding in &report.findings {
        out.push_str(&format!(
            "{} {}: {}\n",
            finding.status.to_ascii_uppercase(),
            finding.label,
            finding.display_path
        ));
        out.push_str(&format!("  path: {}\n", finding.path));
        if let Some(bytes) = finding.bytes {
            out.push_str(&format!("  bytes: {bytes}\n"));
        }
        if !finding.keys.is_empty() {
            out.push_str(&format!("  keys: {}\n", finding.keys.join(", ")));
        }
        if let Some(message) = &finding.message {
            out.push_str(&format!("  {message}\n"));
        }
    }
    out.push('\n');
    out.push_str(&format!("Docs: {}\n", report.docs_url));
    out.push_str(&format!("Schema: {}\n", report.schema_url));
    out.push_str(&format!("Reload: {}", report.reload_command));
    out
}

fn format_font_size_get(payload: &config::FontSizeGetPayload) -> String {
    format!(
        "{} = {}\npath: {}",
        payload.key, payload.formatted, payload.path
    )
}

fn format_font_size_set(payload: &config::FontSizeSetPayload) -> String {
    let status = match payload.reload.as_str() {
        "reloaded" => "reloaded",
        "failed" => "saved; reload failed",
        _ => "saved",
    };
    let mut out = format!("OK {} = {} ({status})\n", payload.key, payload.formatted);
    if let Some(message) = &payload.reload_message {
        out.push_str(&format!("reload: {message}\n"));
    }
    out.push_str(&format!("path: {}", payload.path));
    out
}

fn format_themes_list(payload: &config::ThemeListPayload) -> String {
    let mut out = String::new();
    out.push_str(&format!(
        "Current light: {}\n",
        payload.current.light.as_deref().unwrap_or("inherit")
    ));
    out.push_str(&format!(
        "Current dark: {}\n",
        payload.current.dark.as_deref().unwrap_or("inherit")
    ));
    out.push_str(&format!("Config: {}\n", payload.config_path));
    if let Some(source) = payload.current.source_path.as_deref() {
        out.push_str(&format!("Source: {source}\n"));
    }
    out.push('\n');
    if payload.themes.is_empty() {
        out.push_str("No themes found.");
        return out;
    }
    for theme in &payload.themes {
        let mut badges = Vec::new();
        if theme.current_light {
            badges.push("light");
        }
        if theme.current_dark {
            badges.push("dark");
        }
        if badges.is_empty() {
            out.push_str(&theme.name);
            out.push('\n');
        } else {
            out.push_str(&format!("{}  [{}]\n", theme.name, badges.join(", ")));
        }
    }
    out.trim_end().to_string()
}

fn format_theme_set(payload: &config::ThemeSetPayload) -> String {
    format!(
        "OK light={} dark={} config={} reload={}",
        payload.light.as_deref().unwrap_or("-"),
        payload.dark.as_deref().unwrap_or("-"),
        payload.config_path,
        if payload.reload_requested {
            "requested"
        } else {
            "unavailable"
        }
    )
}

fn format_theme_clear(payload: &config::ThemeClearPayload) -> String {
    format!(
        "OK cleared config={} reload={}",
        payload.config_path,
        if payload.reload_requested {
            "requested"
        } else {
            "unavailable"
        }
    )
}

fn format_right_sidebar_state(value: &Value) -> String {
    let visible = value
        .get("visible")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let mode = value.get("mode").and_then(Value::as_str).unwrap_or("files");
    let focused = value
        .get("focused")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let action = value
        .get("action")
        .and_then(Value::as_str)
        .unwrap_or("mode");
    format!("right-sidebar action={action} visible={visible} mode={mode} focused={focused}")
}

fn format_palette_visibility(value: &Value) -> String {
    let visible = value
        .get("visible")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let mode = value.get("mode").and_then(Value::as_str).unwrap_or("-");
    format!("palette visible={visible} mode={mode}")
}

fn format_palette_results(value: &Value) -> String {
    let visible = value
        .get("visible")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let mode = value.get("mode").and_then(Value::as_str).unwrap_or("-");
    let query = value.get("query").and_then(Value::as_str).unwrap_or("");
    let rows = value
        .get("results")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    if rows.is_empty() {
        return format!("palette visible={visible} mode={mode} query={query} results=0");
    }
    let mut lines = vec![format!(
        "palette visible={visible} mode={mode} query={query} results={}",
        rows.len()
    )];
    lines.extend(rows.iter().map(|row| {
        let command_id = row.get("command_id").and_then(Value::as_str).unwrap_or("-");
        let title = row.get("title").and_then(Value::as_str).unwrap_or("");
        let shortcut_label = shortcut_label_for_row(row);
        if shortcut_label.is_empty() {
            format!("{command_id}\t{title}")
        } else {
            format!("{command_id}\t{title}\t{shortcut_label}")
        }
    }));
    lines.join("\n")
}

fn format_shortcut_help(value: &Value, visible: bool, row_count: usize) -> String {
    let rows = value
        .get("rows")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let mut lines = vec![format!("shortcuts visible={visible} rows={row_count}")];
    lines.extend(rows.iter().map(|row| {
        let name = row.get("name").and_then(Value::as_str).unwrap_or("-");
        let title = row.get("title").and_then(Value::as_str).unwrap_or("");
        let shortcut_label = shortcut_label_for_row(row);
        let description = row.get("description").and_then(Value::as_str).unwrap_or("");
        if description.is_empty() {
            format!("{name}\t{title}\t{shortcut_label}")
        } else {
            format!("{name}\t{title}\t{shortcut_label}\t{description}")
        }
    }));
    lines.join("\n")
}

fn shortcut_label_for_row(row: &Value) -> String {
    row.get("shortcut_label")
        .and_then(Value::as_str)
        .filter(|label| !label.is_empty())
        .map(ToOwned::to_owned)
        .or_else(|| {
            row.get("shortcut_hint")
                .and_then(Value::as_str)
                .map(linux_shortcut_label_from_hint)
        })
        .unwrap_or_default()
}

fn linux_shortcut_label_from_hint(hint: &str) -> String {
    let mut parts = Vec::new();
    let mut key = String::new();
    for ch in hint.chars() {
        match ch {
            '\u{21e7}' => parts.push("Shift".to_string()),
            '\u{2303}' => parts.push("Ctrl".to_string()),
            '\u{2325}' => parts.push("Alt".to_string()),
            '\u{2318}' => parts.push("Super".to_string()),
            _ if ch.is_whitespace() => {}
            _ => key.push(ch),
        }
    }
    if !key.is_empty() {
        parts.push(key);
    }
    parts.join("+")
}

fn ref_or_id(value: &Value, kind: &str) -> String {
    let ref_key = format!("{kind}_ref");
    let id_key = format!("{kind}_id");
    value
        .get(&ref_key)
        .or_else(|| value.get("ref"))
        .and_then(Value::as_str)
        .or_else(|| value.get(&id_key).and_then(Value::as_str))
        .or_else(|| value.get("id").and_then(Value::as_str))
        .unwrap_or("-")
        .to_string()
}

fn sleep_duration(line: &str) -> Result<Option<Duration>> {
    let mut parts = line.split_whitespace();
    if parts.next() != Some("sleep") {
        return Ok(None);
    }
    let raw = parts.next().unwrap_or("1000");
    let millis = raw
        .parse::<u64>()
        .with_context(|| format!("invalid sleep duration: {raw}"))?;
    Ok(Some(Duration::from_millis(millis)))
}

fn renderer_shell_request(
    rest: &str,
    default_renderer_backend: &str,
) -> Result<(&'static str, Value)> {
    let subcommand = rest
        .split_whitespace()
        .next()
        .filter(|part| !part.starts_with('-'))
        .unwrap_or("snapshot");
    let method = match subcommand {
        "diagnostics" | "doctor" => "renderer.diagnostics",
        "snapshot" | "state" => "renderer.snapshot",
        other => return Err(anyhow!("unknown renderer subcommand: {other}")),
    };
    Ok((method, renderer_params(rest, default_renderer_backend)?))
}

fn renderer_params(rest: &str, default_renderer_backend: &str) -> Result<Value> {
    let parts = rest.split_whitespace().collect::<Vec<_>>();
    let mut backend = None;
    for (index, part) in parts.iter().enumerate() {
        if *part != "--backend" {
            continue;
        }
        let Some(value) = parts.get(index + 1).filter(|value| !value.starts_with('-')) else {
            return Err(anyhow!(
                "--backend requires core, gtk, ghostty, or ghostty-vt"
            ));
        };
        backend = Some(*value);
        break;
    }

    Ok(backend
        .or_else(|| (default_renderer_backend != "core").then_some(default_renderer_backend))
        .map(|backend| json!({"backend": backend}))
        .unwrap_or_else(|| json!({})))
}

fn shell_has_flag(line: &str, flag: &str) -> bool {
    line.split_whitespace().any(|part| part == flag)
}

fn shell_words(line: &str) -> Result<Vec<String>> {
    let mut words = Vec::new();
    let mut current = String::new();
    let mut chars = line.chars().peekable();
    let mut quote: Option<char> = None;
    let mut in_word = false;

    while let Some(ch) = chars.next() {
        match (quote, ch) {
            (None, ch) if ch.is_whitespace() => {
                if in_word {
                    words.push(std::mem::take(&mut current));
                    in_word = false;
                }
            }
            (None, '\'' | '"') => {
                quote = Some(ch);
                in_word = true;
            }
            (Some(active), ch) if ch == active => {
                quote = None;
                in_word = true;
            }
            (None, '\\') | (Some('"'), '\\') => {
                let Some(next) = chars.next() else {
                    return Err(anyhow!("unterminated escape in app command"));
                };
                current.push(next);
                in_word = true;
            }
            _ => {
                current.push(ch);
                in_word = true;
            }
        }
    }

    if let Some(active) = quote {
        return Err(anyhow!("unterminated {active} quote in app command"));
    }
    if in_word {
        words.push(current);
    }
    Ok(words)
}

fn shell_scalar(value: &str) -> Value {
    match value {
        "true" => Value::Bool(true),
        "false" => Value::Bool(false),
        _ => value
            .parse::<i64>()
            .map(Value::from)
            .unwrap_or_else(|_| json!(value)),
    }
}

fn canonical_settings_target(raw: &str) -> Option<&'static str> {
    let normalized = raw.trim().to_ascii_lowercase().replace('_', "-");
    match normalized.as_str() {
        "account" => Some("account"),
        "app" | "general" => Some("app"),
        "terminal" => Some("terminal"),
        "textbox" | "text-box" => Some("textBox"),
        "mobile" => Some("mobile"),
        "sidebar" | "sidebar-appearance" | "sidebarappearance" => Some("sidebarAppearance"),
        "custom-sidebars" | "customsidebars" => Some("customSidebars"),
        "beta-features" | "betafeatures" => Some("betaFeatures"),
        "automation" => Some("automation"),
        "browser" => Some("browser"),
        "browser-import" | "browserimport" | "import-browser-data" => Some("browserImport"),
        "global-hotkey" | "globalhotkey" | "hotkey" => Some("globalHotkey"),
        "keyboard-shortcuts" | "keyboardshortcuts" | "shortcuts" | "keys" | "keybindings" => {
            Some("keyboardShortcuts")
        }
        "workspace-colors" | "workspacecolors" | "colors" => Some("workspaceColors"),
        "cmux-json" | "cmuxjson" | "settings-json" | "settingsjson" | "json" | "file"
        | "settings-file" => Some("settingsJSON"),
        "reset" => Some("reset"),
        _ => None,
    }
}

fn print_status(app: &Arc<Mutex<AppState>>) -> Result<()> {
    let mut app = app.lock().map_err(|_| anyhow!("app state lock poisoned"))?;
    let value = app_call(&mut app, "system.identify", json!({}))?;
    println!("{}", format_context(&value));
    Ok(())
}

fn print_app_help() {
    println!("{}", app_help_text());
}

fn app_help_text() -> &'static str {
    "Commands:
  status                         Show focused window/workspace/pane/surface
  windows | workspaces           List windows or workspaces
  displays                       List Linux displays
  window-display <display> [--window <window>]
                                 Assign all or one window to a display
  panes | surfaces               List panes or surfaces
  current-window                 Print the selected window
  new-window [title]             Create and focus a window
  focus-window <window>          Focus a window by UUID/ref/index
  close-window <window>          Close a window by UUID/ref/index
  new-workspace [title]          Create and select a workspace
  current-workspace              Print the selected workspace
  select <workspace>             Select workspace by UUID/ref/index
  next-workspace                 Select the next workspace
  previous-workspace             Select the previous workspace
  last-workspace                 Select the last focused workspace
  close-workspace <workspace>    Close workspace by UUID/ref/index
  rename-workspace [flags] <title>
                                 Rename the focused or --workspace workspace
  workspace-action <action> [flags]
                                 Run workspace context actions
  split [right|down|left|up]     Split the focused surface
  focus-pane <pane>              Focus a pane by UUID/ref/index
  last-pane                      Focus the previously selected pane
  terminal                       Create a terminal surface
  browser [url]                  Open a browser surface
  open <path-or-url>...          Open files, directories, or URLs; quote paths with spaces
  current-surface                Print the selected surface
  focus-surface <surface>        Focus a surface by UUID/ref/index
  close-surface [surface]        Close focused or named surface
  rename-tab [flags] <title>     Rename the focused or --surface tab
  tab-action <action> [flags]    Run tab context actions
  send <text>                    Send text to the focused terminal
  enter                          Send Enter to the focused terminal
  read [surface]                 Print terminal text
  settings [open [target]|path]  Open Settings or print config docs
  config [path|docs|doctor|reload]
                                 Inspect or reload Linux config
  themes [list|set|clear]        List or change Ghostty theme overrides
  feed [list|pending]            List Feed workstream items
  feed clear --yes               Clear Feed items
  feed jump <workstream-id>      Focus a Feed workstream target
  notify <title>                 Create a notification in the current workspace
  notifications [list]           List notifications
  notifications open <id>        Open and mark a notification read
  jump-unread                    Focus the newest unread notification
  right-sidebar [mode|show|hide|toggle|set <mode>]
                                 Control the right sidebar
  palette [commands|type|results|enter]
                                 Open/search/execute command palette rows
  shortcuts [state|toggle|show|hide]
                                 Inspect or toggle shortcut help
  sleep [milliseconds]           Pause scripts without blocking socket clients
  layout                         Print debug layout JSON
  renderer [snapshot|diagnostics] [--backend core|gtk|ghostty|ghostty-vt]
                                 Print renderer snapshot or diagnostics JSON
  quit                           Exit"
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn browser_navigation_state_extracts_browser_controls() {
        let state = browser_navigation_state(&json!({
            "kind": "browser",
            "surface_id": "surface-browser",
            "focused": true,
            "url": "https://fallback.test",
            "browser": {
                "url": "https://example.test/docs",
                "profile_id": "11111111-2222-3333-4444-555555555555",
                "profile_data_generation": 7,
                "can_go_back": true,
                "can_go_forward": false,
                "page_zoom": 1.25,
                "user_agent": "cmux-native/1.0",
                "request_configuration_generation": 9,
                "init_scripts": ["window.__first = true", "window.__second = true"],
                "storage": {
                    "generation": 4,
                    "local": {"alpha": "one"},
                    "session": {"beta": "two"}
                },
                "environment": {
                    "locale": "nl-NL",
                    "timezone": "Europe/Amsterdam",
                    "media_type": "print",
                    "color_scheme": "dark",
                    "reduced_motion": "reduce",
                    "offline": true,
                    "geolocation": {
                        "latitude": 52.37,
                        "longitude": 4.90,
                        "accuracy": 8.0
                    },
                    "mobile": true,
                    "touch": true,
                    "device_scale_factor": 2.0,
                    "permissions": {"geolocation": "granted"}
                },
                "developer_tools_visible": true,
                "focus_mode_active": true,
                "runtime_actions": [{
                    "sequence": 7,
                    "script": "document.body.dataset.ready = 'yes'",
                    "focus_webview": true
                }, {
                    "sequence": 8,
                    "focus_webview": false,
                    "cookie": {
                        "operation": "set",
                        "url": "https://example.test/docs",
                        "name": "session",
                        "value": "native",
                        "domain": "example.test",
                        "path": "/",
                        "max_age": 60
                    }
                }]
            }
        }))
        .expect("browser navigation state");

        assert_eq!(state.surface_id, "surface-browser");
        assert!(state.focused);
        assert_eq!(state.profile_id, "11111111-2222-3333-4444-555555555555");
        assert_eq!(state.profile_data_generation, 7);
        assert_eq!(state.url, "https://example.test/docs");
        assert!(state.can_go_back);
        assert!(!state.can_go_forward);
        assert_eq!(state.page_zoom, 1.25);
        assert_eq!(state.user_agent, "cmux-native/1.0");
        assert_eq!(state.request_configuration_generation, 9);
        assert_eq!(
            state.init_scripts,
            ["window.__first = true", "window.__second = true"]
        );
        assert_eq!(state.storage.generation, 4);
        assert_eq!(
            state.storage.local.get("alpha").map(String::as_str),
            Some("one")
        );
        assert_eq!(
            state.storage.session.get("beta").map(String::as_str),
            Some("two")
        );
        assert_eq!(state.environment.locale, "nl-NL");
        assert_eq!(state.environment.timezone, "Europe/Amsterdam");
        assert_eq!(state.environment.media_type, "print");
        assert_eq!(state.environment.color_scheme, "dark");
        assert_eq!(state.environment.reduced_motion, "reduce");
        assert!(state.environment.offline);
        assert!(state.environment.mobile);
        assert!(state.environment.touch);
        assert_eq!(state.environment.device_scale_factor, 2.0);
        assert_eq!(
            state
                .environment
                .permissions
                .get("geolocation")
                .map(String::as_str),
            Some("granted")
        );
        assert!(state.developer_tools_visible);
        assert!(state.focus_mode_active);
        assert_eq!(state.runtime_actions.len(), 2);
        assert_eq!(state.runtime_actions[0].sequence, 7);
        assert!(state.runtime_actions[0].focus_webview);
        assert!(state.runtime_actions[0].cookie.is_none());
        assert_eq!(state.runtime_actions[1].script, "");
        let cookie = state.runtime_actions[1]
            .cookie
            .as_ref()
            .expect("cookie action");
        assert_eq!(cookie.operation, "set");
        assert_eq!(cookie.name.as_deref(), Some("session"));
        assert_eq!(cookie.max_age, Some(60));
        assert!(browser_navigation_state(&json!({
            "kind": "terminal",
            "surface_id": "surface-terminal"
        }))
        .is_none());

        let fallback = browser_navigation_state(&json!({
            "kind": "browser",
            "surface_id": null,
            "surface_ref": "surface:2",
            "url": "https://fallback.test",
            "browser": {"url": null}
        }))
        .expect("fallback browser navigation state");
        assert_eq!(fallback.surface_id, "surface:2");
        assert!(!fallback.focused);
        assert_eq!(fallback.url, "https://fallback.test");
        assert_eq!(fallback.profile_id, "52B43C05-4A1D-45D3-8FD5-9EF94952E445");
        assert_eq!(fallback.profile_data_generation, 0);
        assert_eq!(fallback.page_zoom, 1.0);
        assert_eq!(fallback.user_agent, "");
        assert_eq!(fallback.request_configuration_generation, 0);
        assert!(fallback.init_scripts.is_empty());
        assert_eq!(fallback.storage, BrowserStorageState::default());
        assert_eq!(fallback.environment, BrowserEnvironmentState::default());
        assert!(!fallback.developer_tools_visible);
        assert!(!fallback.focus_mode_active);
        assert!(fallback.runtime_actions.is_empty());
    }

    #[test]
    fn app_renderer_defaults_to_core_without_cli_or_env() {
        assert_eq!(
            app_renderer_from_cli_or_env(None, None).expect("default renderer"),
            "core"
        );
    }

    #[test]
    fn app_renderer_uses_env_when_cli_is_absent() {
        assert_eq!(
            app_renderer_from_cli_or_env(None, Some("ghostty-vt")).expect("env renderer"),
            "ghostty-vt"
        );
        assert_eq!(
            app_renderer_from_cli_or_env(None, Some("vt")).expect("env alias"),
            "ghostty-vt"
        );
        assert_eq!(
            app_renderer_from_cli_or_env(None, Some("gtk4")).expect("gtk alias"),
            "gtk"
        );
    }

    #[test]
    fn app_renderer_cli_overrides_env() {
        assert_eq!(
            app_renderer_from_cli_or_env(Some("ghostty"), Some("not-a-renderer"))
                .expect("cli renderer wins"),
            "ghostty"
        );
    }

    #[test]
    fn app_renderer_rejects_invalid_env_default() {
        let err = app_renderer_from_cli_or_env(None, Some("not-a-renderer"))
            .expect_err("invalid env renderer");
        assert!(
            err.to_string()
                .contains("CMUX_LINUX_RENDERER requires core, gtk, ghostty, or ghostty-vt"),
            "error was {err}"
        );
    }

    #[test]
    fn gtk_single_instance_mode_defaults_private_sockets_to_local_processes() {
        assert!(gtk_single_instance_mode(false, None).unwrap());
        assert!(!gtk_single_instance_mode(true, None).unwrap());
        assert!(gtk_single_instance_mode(true, Some("true")).unwrap());
        assert!(!gtk_single_instance_mode(false, Some("0")).unwrap());

        let error = gtk_single_instance_mode(true, Some("sometimes")).unwrap_err();
        assert!(error.to_string().contains(GTK_SINGLE_INSTANCE_ENV));
    }

    #[test]
    fn palette_results_falls_back_to_linux_shortcut_label() {
        let output = format_palette_results(&json!({
            "visible": true,
            "mode": "commands",
            "query": "",
            "results": [{
                "command_id": "palette.newTerminal",
                "title": "New Terminal",
                "shortcut_hint": "\u{2318}T"
            }]
        }));

        assert!(output.contains("palette.newTerminal\tNew Terminal\tSuper+T"));
    }

    #[test]
    fn shortcut_help_prefers_label_and_converts_legacy_hint() {
        let output = format_shortcut_help(
            &json!({
                "rows": [
                    {
                        "name": "new_terminal",
                        "title": "New Terminal",
                        "shortcut_label": "Alt+Super+T",
                        "shortcut_hint": "\u{2318}T",
                        "description": "Create a terminal"
                    },
                    {
                        "name": "open_settings",
                        "title": "Open Settings",
                        "shortcut_hint": "\u{2318},",
                        "description": "Open settings"
                    }
                ]
            }),
            true,
            2,
        );

        assert!(output.contains("new_terminal\tNew Terminal\tAlt+Super+T\tCreate a terminal"));
        assert!(output.contains("open_settings\tOpen Settings\tSuper+,\tOpen settings"));
    }

    #[test]
    fn legacy_shortcut_hint_preserves_multi_character_keys() {
        assert_eq!(linux_shortcut_label_from_hint("\u{2318}F1"), "Super+F1");
        assert_eq!(
            linux_shortcut_label_from_hint("\u{21e7}\u{2318}PageDown"),
            "Shift+Super+PageDown"
        );
        assert_eq!(linux_shortcut_label_from_hint("F12"), "F12");
    }

    #[test]
    fn tab_action_shell_params_defaults_to_panel_env_surface_context() {
        let old_workspace = std::env::var("CMUX_WORKSPACE_ID").ok();
        let old_tab = std::env::var("CMUX_TAB_ID").ok();
        let old_panel = std::env::var("CMUX_PANEL_ID").ok();
        let old_surface = std::env::var("CMUX_SURFACE_ID").ok();
        std::env::set_var("CMUX_WORKSPACE_ID", "workspace-env");
        std::env::set_var("CMUX_TAB_ID", "tab-env");
        std::env::set_var("CMUX_PANEL_ID", "panel-env");
        std::env::set_var("CMUX_SURFACE_ID", "surface-env");

        let params = tab_action_shell_params("rename Shell Title", None).expect("tab params");

        restore_test_env("CMUX_WORKSPACE_ID", old_workspace);
        restore_test_env("CMUX_TAB_ID", old_tab);
        restore_test_env("CMUX_PANEL_ID", old_panel);
        restore_test_env("CMUX_SURFACE_ID", old_surface);

        assert_eq!(params["workspace_id"], "workspace-env");
        assert_eq!(params["tab_id"], "tab-env");
        assert_eq!(params["surface_id"], "panel-env");
        assert_eq!(params["action"], "rename");
        assert_eq!(params["title"], "Shell Title");
    }

    fn restore_test_env(key: &str, value: Option<String>) {
        if let Some(value) = value {
            std::env::set_var(key, value);
        } else {
            std::env::remove_var(key);
        }
    }

    #[test]
    fn renderer_params_require_backend_value_when_flag_is_present() {
        assert_eq!(
            renderer_params("diagnostics", "ghostty-vt").expect("default backend"),
            json!({"backend": "ghostty-vt"})
        );
        assert_eq!(
            renderer_params("diagnostics --backend gtk", "ghostty-vt").expect("explicit backend"),
            json!({"backend": "gtk"})
        );

        let err = renderer_params("diagnostics --backend", "core").expect_err("missing backend");
        assert!(
            err.to_string()
                .contains("--backend requires core, gtk, ghostty, or ghostty-vt"),
            "error was {err}"
        );
    }

    #[test]
    fn app_open_target_values_classify_files_directories_and_urls() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let file = tmp.path().join("launch file.txt");
        fs::write(&file, "desktop handoff").expect("write launch file");

        let file_value = open_target_value(&file.display().to_string()).expect("file target");
        assert_eq!(file_value["kind"], "file");
        assert_eq!(file_value["path"], file.display().to_string());
        assert_eq!(
            file_value["url"],
            file_url::file_url_for_path(&file.display().to_string())
        );

        let dir_value = open_target_value(&tmp.path().display().to_string()).expect("dir target");
        assert_eq!(dir_value["kind"], "directory");
        assert_eq!(dir_value["path"], tmp.path().display().to_string());

        let url_value = open_target_value("https://example.test").expect("url target");
        assert_eq!(url_value["kind"], "url");
        assert_eq!(url_value["url"], "https://example.test");
    }

    #[test]
    fn app_shell_words_parse_quoted_targets() {
        assert_eq!(
            shell_words(r#"one "two words" 'three four' five\ six"#).expect("shell words"),
            vec!["one", "two words", "three four", "five six"]
        );
        let err = shell_words("\"unterminated").expect_err("unterminated quote");
        assert!(err.to_string().contains("unterminated"), "error was {err}");
    }

    #[test]
    fn app_open_shell_command_accepts_quoted_paths_with_spaces() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let file = tmp.path().join("launch file.md");
        fs::write(&file, "# desktop handoff\n").expect("write launch file");
        let mut app =
            AppState::with_paths_and_terminal_startup(None, None, TerminalStartupMode::CorePty)
                .expect("app state");

        let outcome =
            execute_shell_command(&mut app, &format!("open \"{}\"", file.display()), "core")
                .expect("open quoted file path");
        assert_eq!(outcome.output, "opened 1 target");
    }

    #[test]
    fn app_startup_open_targets_seed_app_state() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let project = tmp.path().join("project");
        fs::create_dir_all(&project).expect("project dir");
        let file = tmp.path().join("launch.html");
        fs::write(&file, "<title>Launch File</title>").expect("launch file");
        let app = Arc::new(Mutex::new(
            AppState::with_paths_and_terminal_startup(
                None,
                None,
                TerminalStartupMode::RendererOwned,
            )
            .expect("app state"),
        ));

        let opened = open_startup_targets(
            &app,
            &[file.display().to_string(), project.display().to_string()],
        )
        .expect("open startup targets");
        assert_eq!(opened["count"], 2);
        let opened_rows = opened["opened"].as_array().expect("opened rows");
        assert!(opened_rows.iter().any(|row| row["kind"] == "file"));
        assert!(opened_rows.iter().any(|row| row["kind"] == "directory"));

        let mut app = app.lock().expect("app lock");
        let workspaces = app_call(&mut app, "workspace.list", json!({})).expect("workspace list");
        assert!(
            workspaces["workspaces"]
                .as_array()
                .unwrap()
                .iter()
                .any(|workspace| workspace["cwd"] == project.display().to_string()),
            "directory target did not create workspace: {workspaces}"
        );
    }
}
