use crate::{
    browser_settings, config, custom_sidebar, diff_baseline, diff_viewer, file_url, linux_update,
    server, ui,
};
use anyhow::{bail, Context, Result};
use base64::Engine;
use serde_json::{json, Map, Value};
use sha2::{Digest, Sha256};
use std::fs;
use std::io::{self, BufRead, BufReader, IsTerminal, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::os::unix::fs::PermissionsExt;
use std::os::unix::net::UnixStream;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

#[derive(Clone, Copy, PartialEq, Eq)]
enum IdFormat {
    Refs,
    Uuids,
    Both,
}

struct GlobalOptions {
    socket: Option<String>,
    explicit_socket: bool,
    json: bool,
    id_format: IdFormat,
    command: Vec<String>,
}

pub fn run(args: Vec<String>) -> Result<()> {
    let mut args = args.into_iter().skip(1).collect::<Vec<_>>();
    if args.first().map(String::as_str) == Some("serve") {
        args.remove(0);
        let socket = parse_serve_socket(&args)?;
        return server::run_server(&socket);
    }

    let options = parse_global_options(args)?;
    if options.command.is_empty() || matches!(options.command[0].as_str(), "--help" | "-h" | "help")
    {
        print_help(&options.command);
        return Ok(());
    }
    if matches!(options.command[0].as_str(), "--version" | "-V" | "version") {
        println!("cmux-linux {}", linux_update::installed_version().0);
        return Ok(());
    }
    if options.command.first().map(String::as_str) == Some("__tmux-compat")
        && matches!(
            options.command.get(1).map(String::as_str),
            Some("--help" | "-h" | "help")
        )
    {
        print_command_help("__tmux-compat");
        return Ok(());
    }
    if options.command.first().map(String::as_str) == Some("__tmux-compat") {
        return run_tmux_compat(&options);
    }
    if command_has_help_flag(&options.command) {
        print_command_help(help_topic_for_command(&options.command));
        return Ok(());
    }
    if run_no_socket_command(&options)? {
        return Ok(());
    }

    let socket = resolve_socket_path(&options)?;
    if options.command.first().map(String::as_str) == Some("ssh-tmux") {
        return run_ssh_tmux_command(&socket, &options);
    }
    if options.command.first().map(String::as_str) == Some("events") {
        return run_events_command(&socket, &options.command);
    }
    if options.command.first().map(String::as_str) == Some("feed")
        && options.command.get(1).map(String::as_str) == Some("tui")
    {
        return run_feed_tui(
            &socket,
            &options.command,
            options.json || command_has_flag(&options.command, "--json"),
        );
    }
    if !confirm_feed_clear_if_needed(&options.command)? {
        return Ok(());
    }
    let (method, params, text_mode) = command_to_request(&options.command)?;
    let command_json = command_has_flag(&options.command, "--json");
    warn_legacy_browser_deprecation(&options.command, options.json || command_json);
    let response = call_socket(&socket, &method, params)?;
    let id_format = effective_id_format(&options.command, options.id_format)?;
    let formatted = format_ids(response, id_format);
    let text_mode_handles_json = matches!(
        text_mode,
        TextMode::BrowserScreenshot { .. } | TextMode::BrowserPdf { .. }
    );

    if options.json || (command_json && !text_mode_handles_json) {
        println!("{}", serde_json::to_string(&formatted)?);
    } else {
        print_text_response(&options.command[0], &formatted, text_mode)?;
    }
    Ok(())
}

fn parse_serve_socket(args: &[String]) -> Result<String> {
    let mut socket = default_socket_path();
    let mut index = 0;
    while index < args.len() {
        match args[index].as_str() {
            "--socket" | "-s" => {
                index += 1;
                socket = args.get(index).context("--socket requires a path")?.clone();
            }
            other => bail!("unknown serve option: {other}"),
        }
        index += 1;
    }
    Ok(socket)
}

fn parse_global_options(args: Vec<String>) -> Result<GlobalOptions> {
    let mut socket = None;
    let mut explicit_socket = false;
    let mut json = false;
    let mut id_format = IdFormat::Refs;
    let mut command = Vec::new();
    let mut index = 0;
    while index < args.len() {
        let arg = &args[index];
        match arg.as_str() {
            "--socket" | "-s" => {
                index += 1;
                socket = Some(args.get(index).context("--socket requires a path")?.clone());
                explicit_socket = true;
            }
            "--json" => json = true,
            "--password" => {
                index += 1;
                let _ = args.get(index).context("--password requires a value")?;
            }
            "--id-format" => {
                index += 1;
                id_format = match args.get(index).map(String::as_str) {
                    Some("refs") => IdFormat::Refs,
                    Some("uuids") => IdFormat::Uuids,
                    Some("both") => IdFormat::Both,
                    Some(other) => bail!("unknown --id-format {other}"),
                    None => bail!("--id-format requires refs, uuids, or both"),
                };
            }
            "--version" | "-V" | "--help" | "-h" => {
                command.extend_from_slice(&args[index..]);
                break;
            }
            _ if arg.starts_with("--") => bail!("unknown global option: {arg}"),
            _ => {
                command.extend_from_slice(&args[index..]);
                break;
            }
        }
        index += 1;
    }
    Ok(GlobalOptions {
        socket,
        explicit_socket,
        json,
        id_format,
        command,
    })
}

fn run_no_socket_command(options: &GlobalOptions) -> Result<bool> {
    if maybe_run_browser_availability_command(options)? {
        return Ok(true);
    }
    let command = options
        .command
        .first()
        .map(String::as_str)
        .unwrap_or("help");
    match command {
        "docs" => {
            if options.command.get(1).map(String::as_str) == Some("settings") {
                print_settings_docs(options.json || command_has_flag(&options.command, "--json"))?;
            } else if let Some(topic) = options.command.get(1).map(String::as_str) {
                print_docs_topic(topic, options.json)?;
            } else if options.json {
                println!(
                    "{}",
                    json!({"topics": ["settings", "shortcuts", "api", "browser", "agents", "dock", "sidebars"]})
                );
            } else {
                println!("Topics: settings, shortcuts, api, browser, agents, dock, sidebars");
            }
            Ok(true)
        }
        "settings" => {
            let sub = options.command.get(1).map(String::as_str).unwrap_or("path");
            if matches!(sub, "path" | "docs")
                || (sub == "--"
                    && matches!(
                        options.command.get(2).map(String::as_str),
                        Some("path" | "docs")
                    ))
            {
                print_settings_docs(options.json || command_has_flag(&options.command, "--json"))?;
                Ok(true)
            } else {
                Ok(false)
            }
        }
        "config" => {
            let sub = options.command.get(1).map(String::as_str).unwrap_or("help");
            if matches!(sub, "help" | "--help" | "-h") {
                print_command_help("config");
                Ok(true)
            } else if matches!(sub, "source" | "sources") {
                print_config_snapshot(options.json, false)?;
                Ok(true)
            } else if matches!(sub, "path" | "paths" | "docs" | "documentation") {
                print_settings_docs(options.json || command_has_flag(&options.command, "--json"))?;
                Ok(true)
            } else if matches!(sub, "doctor" | "check" | "validate") {
                run_config_doctor_command(options)?;
                Ok(true)
            } else if config_font_size_command_needs_no_socket(&options.command) {
                run_config_font_size_command(options)?;
                Ok(true)
            } else {
                Ok(false)
            }
        }
        "update" => {
            run_update_command(options)?;
            Ok(true)
        }
        "window" if options.command.get(1).map(String::as_str) == Some("default-display") => {
            run_window_default_display_command(options)?;
            Ok(true)
        }
        "themes" => {
            run_themes_command(options.json, &options.command)?;
            Ok(true)
        }
        "welcome" => {
            print_welcome();
            Ok(true)
        }
        "app" => {
            let command = app_command_with_global_socket(options);
            ui::run_app_command(&command)?;
            Ok(true)
        }
        "omo" | "omx" | "omc" => {
            run_agent_launcher_command(options)?;
            Ok(true)
        }
        "remote-daemon-status" => {
            print_remote_daemon_status(&options.command, options.json)?;
            Ok(true)
        }
        "hooks" | "setup-hooks" if maybe_run_hooks_installer_command(&options.command)? => Ok(true),
        "install-claude-code-integration" | "install-claude-integration" => {
            run_install_claude_code_integration(&options.command)?;
            Ok(true)
        }
        "install-codex-integration" => {
            run_install_codex_integration(&options.command)?;
            Ok(true)
        }
        "codex"
            if matches!(
                options.command.get(1).map(String::as_str),
                Some("install-hooks" | "install" | "setup")
            ) =>
        {
            run_install_codex_integration(&options.command)?;
            Ok(true)
        }
        "codex"
            if matches!(
                options.command.get(1).map(String::as_str),
                Some("uninstall-hooks" | "uninstall")
            ) =>
        {
            run_uninstall_codex_integration(&options.command)?;
            Ok(true)
        }
        "install-opencode-integration" | "install-opencode-plugin" => {
            run_install_opencode_integration(&options.command)?;
            Ok(true)
        }
        "opencode"
            if matches!(
                options.command.get(1).map(String::as_str),
                Some("install-hooks" | "install" | "setup")
            ) =>
        {
            run_install_opencode_integration(&options.command)?;
            Ok(true)
        }
        "opencode"
            if matches!(
                options.command.get(1).map(String::as_str),
                Some("uninstall-hooks" | "uninstall")
            ) =>
        {
            run_uninstall_opencode_integration(&options.command)?;
            Ok(true)
        }
        "wait-for" => {
            run_wait_for(&options.command)?;
            Ok(true)
        }
        "set-buffer" => {
            let name = option_value(&options.command, "--name").unwrap_or_else(|| "buffer".into());
            let text = last_positional(&options.command).unwrap_or_default();
            fs::write(buffer_path(&name), text)?;
            println!("OK");
            Ok(true)
        }
        "list-buffers" => {
            let dir = cmux_tmp_dir();
            if let Ok(entries) = fs::read_dir(dir) {
                for entry in entries.flatten() {
                    let file_name = entry.file_name().to_string_lossy().to_string();
                    if let Some(name) = file_name.strip_prefix("buffer-") {
                        println!("{name}");
                    }
                }
            }
            Ok(true)
        }
        "set-hook" => {
            run_set_hook(&options.command)?;
            Ok(true)
        }
        "display-message" => {
            if command_has_flag(&options.command, "-p") {
                println!("{}", last_positional(&options.command).unwrap_or_default());
            } else {
                println!("OK");
            }
            Ok(true)
        }
        "bind-key" => {
            run_bind_key(&options.command)?;
            Ok(true)
        }
        "unbind-key" => {
            run_unbind_key(&options.command)?;
            Ok(true)
        }
        "copy-mode" => {
            run_copy_mode(&options.command)?;
            Ok(true)
        }
        "popup" => {
            run_popup(options)?;
            Ok(true)
        }
        _ => Ok(false),
    }
}

fn run_update_command(options: &GlobalOptions) -> Result<()> {
    let mut index = 1;
    let subcommand = match options.command.get(index).map(String::as_str) {
        Some("check" | "status" | "install") => {
            let value = options.command[index].as_str();
            index += 1;
            value
        }
        Some(value) if !value.starts_with('-') => {
            bail!("unknown update subcommand: {value}");
        }
        _ => "check",
    };
    let json_output = options.json || command_has_flag(&options.command, "--json");
    if matches!(subcommand, "check" | "status") {
        for argument in &options.command[index..] {
            if argument != "--json" {
                bail!("unknown update option: {argument}");
            }
        }
        let status = linux_update::check_for_updates()?;
        if json_output {
            println!("{}", serde_json::to_string(&status)?);
        } else {
            println!("{}", linux_update::update_status_text(&status));
        }
        return Ok(());
    }

    let mut confirmed = false;
    let mut force = false;
    let mut prefix = None::<PathBuf>;
    while index < options.command.len() {
        match options.command[index].as_str() {
            "--yes" | "-y" => confirmed = true,
            "--force" => force = true,
            "--json" => {}
            "--prefix" => {
                index += 1;
                prefix = Some(PathBuf::from(
                    options
                        .command
                        .get(index)
                        .context("--prefix requires an absolute path")?,
                ));
            }
            argument => bail!("unknown update install option: {argument}"),
        }
        index += 1;
    }

    let status = linux_update::check_for_updates()?;
    if !force && status.get("update_available").and_then(Value::as_bool) != Some(true) {
        if !json_output {
            println!("{}", linux_update::update_status_text(&status));
        }
        bail!("no newer stable Linux release is available; pass --force to reinstall");
    }
    let latest_version = status
        .get("latest_version")
        .and_then(Value::as_str)
        .unwrap_or("unknown");
    if !confirmed {
        if !io::stdin().is_terminal() {
            bail!("update install requires --yes when not run from an interactive terminal");
        }
        eprint!(
            "Install cmux Linux {latest_version}{}? [y/N] ",
            prefix
                .as_ref()
                .map(|path| format!(" into {}", path.display()))
                .unwrap_or_default()
        );
        io::stderr().flush()?;
        let mut answer = String::new();
        io::stdin().read_line(&mut answer)?;
        if !matches!(answer.trim().to_ascii_lowercase().as_str(), "y" | "yes") {
            bail!("Linux update installation cancelled");
        }
    }
    let result = linux_update::install_checked_update(&status, prefix.as_deref(), force)?;
    if json_output {
        println!("{}", serde_json::to_string(&result)?);
    } else {
        println!(
            "Installed cmux Linux {} into {}.",
            result
                .get("latest_version")
                .and_then(Value::as_str)
                .unwrap_or("unknown"),
            result
                .get("installed_prefix")
                .and_then(Value::as_str)
                .unwrap_or("the selected prefix")
        );
        println!("Restart cmux to run the updated binaries.");
    }
    Ok(())
}

fn app_command_with_global_socket(options: &GlobalOptions) -> Vec<String> {
    let mut command = options.command.clone();
    if !options.explicit_socket || app_command_has_socket(&command) {
        return command;
    }
    if let Some(socket) = options.socket.as_ref() {
        command.insert(1, socket.clone());
        command.insert(1, "--socket".to_string());
    }
    command
}

fn app_command_has_socket(command: &[String]) -> bool {
    let mut index = 1;
    while index < command.len() {
        match command[index].as_str() {
            "--" => return false,
            "--socket" | "-s" => return true,
            _ => {}
        }
        index += 1;
    }
    false
}

struct AgentLauncherResolution {
    executable: PathBuf,
    child_path: std::ffi::OsString,
}

struct AgentLauncherEnvironment {
    child_path: std::ffi::OsString,
    envs: Vec<(String, String)>,
    args: Vec<String>,
}

fn run_agent_launcher_command(options: &GlobalOptions) -> Result<()> {
    let command = &options.command;
    let launcher = command.first().map(String::as_str).unwrap_or("omo");
    let executable_name = match launcher {
        "omo" => "opencode",
        "omx" => "omx",
        "omc" => "omc",
        other => bail!("unsupported agent launcher: {other}"),
    };
    let resolution = resolve_agent_launcher_executable(launcher, executable_name)?;
    let launcher_env = prepare_agent_launcher_environment(options, launcher, &resolution)?;
    let mut process = Command::new(&resolution.executable);
    process
        .args(&launcher_env.args)
        .env("PATH", &launcher_env.child_path)
        .stdin(Stdio::inherit())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit());
    for (key, value) in &launcher_env.envs {
        process.env(key, value);
    }
    process.env_remove("CMUX_SOCKET");
    let status = process.status().with_context(|| {
        format!(
            "failed to run cmux {launcher} launcher {}",
            resolution.executable.display()
        )
    })?;
    if !status.success() {
        std::process::exit(status.code().unwrap_or(1));
    }
    Ok(())
}

fn prepare_agent_launcher_environment(
    options: &GlobalOptions,
    launcher: &str,
    resolution: &AgentLauncherResolution,
) -> Result<AgentLauncherEnvironment> {
    let shim_dir = create_agent_launcher_shim_dir(launcher)?;
    let child_path = prepend_path_entry(&shim_dir, &resolution.child_path)?;
    let socket_path = resolve_socket_path(options)?;
    let cmux_bin = std::env::current_exe()
        .ok()
        .map(|path| path.display().to_string())
        .unwrap_or_else(|| "cmux".to_string());
    let launcher_upper = launcher.to_ascii_uppercase();
    let mut envs = vec![
        (format!("CMUX_{launcher_upper}_CMUX_BIN"), cmux_bin.clone()),
        ("CMUX_SOCKET_PATH".to_string(), socket_path),
        (
            "TMUX".to_string(),
            launcher_tmux_value(&format!("cmux-{launcher}")),
        ),
        ("TMUX_PANE".to_string(), launcher_tmux_pane_value()),
        ("TERM".to_string(), launcher_term_value(launcher)),
    ];
    let mut args = options.command.iter().skip(1).cloned().collect::<Vec<_>>();
    if launcher == "omo" {
        let config_dir = prepare_omo_launcher_environment(&child_path)?;
        let opencode_port = resolved_omo_port(&args);
        if !omo_command_has_port(&args) {
            args.push("--port".to_string());
            args.push(opencode_port.clone());
        }
        envs.push((
            "OPENCODE_CONFIG_DIR".to_string(),
            config_dir.display().to_string(),
        ));
        envs.push(("OPENCODE_PORT".to_string(), opencode_port));
        envs.push(("CMUX_OPENCODE_CMUX_BIN".to_string(), cmux_bin));
    }
    Ok(AgentLauncherEnvironment {
        child_path,
        envs,
        args,
    })
}

fn resolve_agent_launcher_executable(
    launcher: &str,
    executable_name: &str,
) -> Result<AgentLauncherResolution> {
    let search_dirs = agent_launcher_search_dirs();
    let executable = search_dirs
        .iter()
        .filter(|dir| !is_macos_app_bundle_launcher_dir(dir))
        .map(|dir| dir.join(executable_name))
        .find(|path| is_executable_file(path))
        .with_context(|| {
            format!(
                "cmux {launcher} requires {executable_name} on PATH, ~/.bun/bin, or ~/.local/bin"
            )
        })?;
    let executable_dir = executable
        .parent()
        .context("launcher executable has no parent directory")?
        .to_path_buf();
    let child_path = agent_launcher_child_path(&executable_dir, &search_dirs)?;
    Ok(AgentLauncherResolution {
        executable,
        child_path,
    })
}

fn agent_launcher_search_dirs() -> Vec<PathBuf> {
    let mut dirs = Vec::new();
    if let Some(path) = std::env::var_os("PATH") {
        for dir in std::env::split_paths(&path) {
            push_unique_path(&mut dirs, dir);
        }
    }
    if let Ok(home) = std::env::var("HOME") {
        let home = Path::new(&home);
        push_unique_path(&mut dirs, home.join(".bun/bin"));
        push_unique_path(&mut dirs, home.join(".local/bin"));
    }
    dirs
}

fn agent_launcher_child_path(
    executable_dir: &Path,
    search_dirs: &[PathBuf],
) -> Result<std::ffi::OsString> {
    let mut dirs = Vec::new();
    push_unique_path(&mut dirs, executable_dir.to_path_buf());
    for dir in search_dirs {
        if !is_macos_app_bundle_launcher_dir(dir) {
            push_unique_path(&mut dirs, dir.clone());
        }
    }
    std::env::join_paths(dirs).context("failed to build launcher PATH")
}

fn prepend_path_entry(entry: &Path, existing: &std::ffi::OsStr) -> Result<std::ffi::OsString> {
    let mut dirs = vec![entry.to_path_buf()];
    dirs.extend(std::env::split_paths(existing));
    std::env::join_paths(dirs).context("failed to build launcher PATH")
}

fn create_agent_launcher_shim_dir(launcher: &str) -> Result<PathBuf> {
    let dir = agent_launcher_cache_dir().join(format!("{launcher}-bin"));
    fs::create_dir_all(&dir)?;
    let tmux_script = launcher_tmux_shim_script(launcher)?;
    write_launcher_shim_if_changed(&dir.join("tmux"), &tmux_script)?;
    if launcher == "omo" {
        write_launcher_shim_if_changed(&dir.join("terminal-notifier"), omo_notifier_shim_script())?;
    }
    Ok(dir)
}

fn agent_launcher_cache_dir() -> PathBuf {
    cache_dir().join("agent-launchers")
}

fn launcher_tmux_shim_script(launcher: &str) -> Result<String> {
    let cmux_env = match launcher {
        "omo" => "CMUX_OMO_CMUX_BIN",
        "omx" => "CMUX_OMX_CMUX_BIN",
        "omc" => "CMUX_OMC_CMUX_BIN",
        other => bail!("unsupported agent launcher shim: {other}"),
    };
    if launcher == "omx" {
        return Ok(format!(
            r#"#!/usr/bin/env bash
set -euo pipefail
case "${{1:-}}" in
  -V|-v) echo "tmux 3.4"; exit 0 ;;
  show-options|show-option|show)
    shift
    value_only=0
    option_name=""
    while (($#)); do
      arg="$1"
      shift
      case "$arg" in
        --) ;;
        -t)
          if (($#)); then shift; fi
          ;;
        -t*) ;;
        -*)
          case "$arg" in
            *v*) value_only=1 ;;
          esac
          ;;
        *) option_name="$arg" ;;
      esac
    done
    case "$option_name" in
      extended-keys)
        if [[ "$value_only" == "1" ]]; then
          echo "on"
        else
          echo "extended-keys on"
        fi
        exit 0
        ;;
    esac
    ;;
esac
exec "${{{cmux_env}:-cmux}}" __tmux-compat "$@"
"#
        ));
    }
    Ok(format!(
        r#"#!/usr/bin/env bash
set -euo pipefail
case "${{1:-}}" in
  -V|-v) echo "tmux 3.4"; exit 0 ;;
esac
exec "${{{cmux_env}:-cmux}}" __tmux-compat "$@"
"#
    ))
}

fn omo_notifier_shim_script() -> &'static str {
    r#"#!/usr/bin/env bash
TITLE="" BODY=""
while [[ $# -gt 0 ]]; do
  case "$1" in
    -title) TITLE="${2:-}"; shift 2 ;;
    -message) BODY="${2:-}"; shift 2 ;;
    *) shift ;;
  esac
done
exec "${CMUX_OMO_CMUX_BIN:-cmux}" notify --title "${TITLE:-OpenCode}" --body "${BODY:-}"
"#
}

fn write_launcher_shim_if_changed(path: &Path, script: &str) -> Result<()> {
    let normalized = script.trim();
    let existing = fs::read_to_string(path).unwrap_or_default();
    if existing.trim() != normalized {
        fs::write(path, script).with_context(|| format!("failed to write {}", path.display()))?;
    }
    let mut permissions = fs::metadata(path)
        .with_context(|| format!("failed to stat {}", path.display()))?
        .permissions();
    permissions.set_mode(0o755);
    fs::set_permissions(path, permissions)
        .with_context(|| format!("failed to chmod {}", path.display()))?;
    Ok(())
}

fn launcher_tmux_value(prefix: &str) -> String {
    std::env::var("TMUX")
        .ok()
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| format!("/tmp/{prefix}/default,0,1"))
}

fn launcher_tmux_pane_value() -> String {
    std::env::var("TMUX_PANE")
        .ok()
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| "%1".to_string())
}

fn launcher_term_value(launcher: &str) -> String {
    let override_key = format!("CMUX_{}_TERM", launcher.to_ascii_uppercase());
    std::env::var(&override_key)
        .ok()
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| "screen-256color".to_string())
}

fn resolved_omo_port(args: &[String]) -> String {
    if let Some(port) = omo_requested_port(args) {
        return port;
    }
    if let Ok(port) = std::env::var("OPENCODE_PORT") {
        let trimmed = port.trim();
        if let Ok(parsed) = trimmed.parse::<u16>() {
            if parsed != 0 && omo_bindable_loopback_port(parsed).is_some() {
                return trimmed.to_string();
            }
        }
    }
    if let Some(port) = omo_bindable_loopback_port(4096) {
        return port.to_string();
    }
    omo_bindable_loopback_port(0)
        .map(|port| port.to_string())
        .unwrap_or_else(|| "4096".to_string())
}

fn omo_requested_port(args: &[String]) -> Option<String> {
    for (index, arg) in args.iter().enumerate() {
        if arg == "--port" {
            let value = args.get(index + 1)?.trim();
            return (!value.is_empty()).then(|| value.to_string());
        }
        if let Some(value) = arg.strip_prefix("--port=") {
            let value = value.trim();
            return (!value.is_empty()).then(|| value.to_string());
        }
    }
    None
}

fn omo_command_has_port(args: &[String]) -> bool {
    omo_requested_port(args).is_some()
}

fn omo_bindable_loopback_port(port: u16) -> Option<u16> {
    let listener = TcpListener::bind(("127.0.0.1", port)).ok()?;
    if port != 0 {
        return Some(port);
    }
    listener.local_addr().ok().map(|addr| addr.port())
}

fn push_unique_path(paths: &mut Vec<PathBuf>, path: PathBuf) {
    if !paths.iter().any(|existing| existing == &path) {
        paths.push(path);
    }
}

fn is_executable_file(path: &Path) -> bool {
    let Ok(metadata) = fs::metadata(path) else {
        return false;
    };
    metadata.is_file() && metadata.permissions().mode() & 0o111 != 0
}

fn is_macos_app_bundle_launcher_dir(path: &Path) -> bool {
    let text = path.to_string_lossy();
    text.contains(".app/Contents/Resources/bin") || text.contains(".app/Contents/MacOS")
}

const OMO_PLUGIN_NAME: &str = "oh-my-openagent";
const LEGACY_OMO_PLUGIN_NAME: &str = "oh-my-opencode";

fn prepare_omo_launcher_environment(child_path: &std::ffi::OsStr) -> Result<PathBuf> {
    let user_dir = omo_user_config_dir()?;
    let shadow_dir = omo_shadow_config_dir()?;
    fs::create_dir_all(&shadow_dir)?;

    let user_json = user_dir.join("opencode.json");
    let shadow_json = shadow_dir.join("opencode.json");
    let mut config = read_omo_opencode_config(&user_json)?;
    let plugins = config
        .remove("plugin")
        .and_then(|value| value.as_array().cloned())
        .unwrap_or_default();
    let mut plugins = normalize_omo_plugin_list(remove_opencode_session_plugins(plugins));
    if !opencode_plugin_list_contains_package(&plugins, OMO_PLUGIN_NAME) {
        plugins.push(json!(OMO_PLUGIN_NAME));
    }
    config.insert("plugin".to_string(), Value::Array(plugins));
    fs::write(
        &shadow_json,
        serde_json::to_string_pretty(&Value::Object(config))? + "\n",
    )
    .with_context(|| format!("failed to write {}", shadow_json.display()))?;

    let shadow_node_modules = shadow_dir.join("node_modules");
    let user_node_modules = user_dir.join("node_modules");
    ensure_shadow_node_modules_symlink(&shadow_node_modules, &user_node_modules)?;
    ensure_omo_shadow_package_manifest(&shadow_dir.join("package.json"))?;
    remove_if_symlink(&shadow_dir.join("bun.lock"))?;
    write_opencode_session_plugin_in(&shadow_dir)?;
    symlink_omo_config_files(&user_dir, &shadow_dir)?;
    ensure_omo_package_installed(
        &shadow_dir,
        &shadow_node_modules,
        &user_node_modules,
        child_path,
    )?;
    ensure_omo_tmux_config(&user_dir, &shadow_dir)?;
    Ok(shadow_dir)
}

fn omo_user_config_dir() -> Result<PathBuf> {
    let home = std::env::var("HOME").context("HOME is required for cmux omo")?;
    Ok(Path::new(&home).join(".config/opencode"))
}

fn omo_shadow_config_dir() -> Result<PathBuf> {
    Ok(agent_launcher_cache_dir().join("omo-config"))
}

fn read_omo_opencode_config(path: &Path) -> Result<Map<String, Value>> {
    let text = match fs::read_to_string(path) {
        Ok(text) => text,
        Err(err) if err.kind() == io::ErrorKind::NotFound => return Ok(Map::new()),
        Err(err) => return Err(err).with_context(|| format!("failed to read {}", path.display())),
    };
    if text.trim().is_empty() {
        return Ok(Map::new());
    }
    let value: Value = serde_json::from_str(&text).with_context(|| {
        format!(
            "Failed to parse {}. Fix the JSON syntax and retry.",
            path.display()
        )
    })?;
    value
        .as_object()
        .cloned()
        .with_context(|| format!("{} must contain a JSON object", path.display()))
}

fn remove_opencode_session_plugins(plugins: Vec<Value>) -> Vec<Value> {
    plugins
        .into_iter()
        .filter(|entry| !opencode_plugin_entry_is_session_plugin(entry))
        .collect()
}

fn normalize_omo_plugin_list(plugins: Vec<Value>) -> Vec<Value> {
    let Some(preferred) = preferred_omo_plugin_entry(&plugins) else {
        return plugins;
    };
    let mut inserted = false;
    let mut normalized = Vec::new();
    for entry in plugins {
        if opencode_plugin_entry_is_omo_package(&entry) {
            if !inserted {
                normalized.push(preferred.clone());
                inserted = true;
            }
        } else {
            normalized.push(entry);
        }
    }
    normalized
}

fn preferred_omo_plugin_entry(plugins: &[Value]) -> Option<Value> {
    let mut current_pinned = None;
    let mut current = None;
    let mut legacy = None;
    for entry in plugins {
        let Some(name) = opencode_plugin_entry_spec(entry) else {
            continue;
        };
        if opencode_plugin_spec_is_package(name, OMO_PLUGIN_NAME) {
            if opencode_plugin_spec_is_pinned_package(name, OMO_PLUGIN_NAME) {
                if current_pinned.is_none() {
                    current_pinned = Some(entry.clone());
                }
            } else if current.is_none() {
                current = Some(entry.clone());
            }
            continue;
        }
        if legacy.is_none() && opencode_plugin_spec_is_package(name, LEGACY_OMO_PLUGIN_NAME) {
            legacy = Some(opencode_plugin_entry_replacing_package(
                entry,
                LEGACY_OMO_PLUGIN_NAME,
                OMO_PLUGIN_NAME,
            ));
        }
    }
    current_pinned.or(current).or(legacy)
}

fn opencode_plugin_entry_replacing_package(
    entry: &Value,
    package_name: &str,
    replacement_package_name: &str,
) -> Value {
    let Some(name) = opencode_plugin_entry_spec(entry) else {
        return entry.clone();
    };
    if !opencode_plugin_spec_is_package(name, package_name) {
        return entry.clone();
    }
    let replacement = format!(
        "{}{}",
        replacement_package_name,
        name.strip_prefix(package_name).unwrap_or_default()
    );
    if entry.is_string() {
        return json!(replacement);
    }
    if let Some(values) = entry.as_array() {
        let mut values = values.clone();
        if let Some(first) = values.first_mut() {
            *first = json!(replacement);
        }
        return Value::Array(values);
    }
    entry.clone()
}

fn opencode_plugin_entry_is_omo_package(entry: &Value) -> bool {
    let Some(name) = opencode_plugin_entry_spec(entry) else {
        return false;
    };
    opencode_plugin_spec_is_package(name, OMO_PLUGIN_NAME)
        || opencode_plugin_spec_is_package(name, LEGACY_OMO_PLUGIN_NAME)
}

fn opencode_plugin_list_contains_package(plugins: &[Value], package_name: &str) -> bool {
    plugins.iter().any(|entry| {
        opencode_plugin_entry_spec(entry)
            .is_some_and(|spec| opencode_plugin_spec_is_package(spec, package_name))
    })
}

fn opencode_plugin_spec_is_package(value: &str, package_name: &str) -> bool {
    value == package_name || value.starts_with(&format!("{package_name}@"))
}

fn opencode_plugin_spec_is_pinned_package(value: &str, package_name: &str) -> bool {
    value.starts_with(&format!("{package_name}@"))
}

fn ensure_shadow_node_modules_symlink(
    shadow_node_modules: &Path,
    user_node_modules: &Path,
) -> Result<()> {
    if !user_node_modules.exists() {
        return Ok(());
    }
    if let Ok(metadata) = fs::symlink_metadata(shadow_node_modules) {
        if metadata.file_type().is_symlink() {
            if fs::read_link(shadow_node_modules).ok().as_deref() == Some(user_node_modules) {
                return Ok(());
            }
            fs::remove_file(shadow_node_modules).with_context(|| {
                format!(
                    "failed to remove stale symlink {}",
                    shadow_node_modules.display()
                )
            })?;
        } else {
            return Ok(());
        }
    }
    std::os::unix::fs::symlink(user_node_modules, shadow_node_modules).with_context(|| {
        format!(
            "failed to symlink {} to {}",
            shadow_node_modules.display(),
            user_node_modules.display()
        )
    })
}

fn ensure_omo_shadow_package_manifest(path: &Path) -> Result<()> {
    remove_if_symlink(path)?;
    let manifest = json!({
        "dependencies": {
            OMO_PLUGIN_NAME: "latest"
        },
        "name": "cmux-omo-shadow",
        "private": true
    });
    let text = serde_json::to_string_pretty(&manifest)? + "\n";
    let existing = fs::read_to_string(path).unwrap_or_default();
    if existing != text {
        fs::write(path, text).with_context(|| format!("failed to write {}", path.display()))?;
    }
    Ok(())
}

fn remove_if_symlink(path: &Path) -> Result<()> {
    if fs::symlink_metadata(path)
        .map(|metadata| metadata.file_type().is_symlink())
        .unwrap_or(false)
    {
        fs::remove_file(path).with_context(|| format!("failed to remove {}", path.display()))?;
    }
    Ok(())
}

fn write_opencode_session_plugin_in(config_dir: &Path) -> Result<()> {
    let plugin_dir = config_dir.join("plugins");
    fs::create_dir_all(&plugin_dir)?;
    fs::write(
        plugin_dir.join(OPENCODE_SESSION_PLUGIN_FILENAME),
        OPENCODE_SESSION_PLUGIN_SOURCE,
    )?;
    Ok(())
}

fn symlink_omo_config_files(user_dir: &Path, shadow_dir: &Path) -> Result<()> {
    for filename in [
        "oh-my-openagent.json",
        "oh-my-openagent.jsonc",
        "oh-my-opencode.json",
        "oh-my-opencode.jsonc",
    ] {
        let user_file = user_dir.join(filename);
        let shadow_file = shadow_dir.join(filename);
        if user_file.exists() && fs::symlink_metadata(&shadow_file).is_err() {
            std::os::unix::fs::symlink(&user_file, &shadow_file).with_context(|| {
                format!(
                    "failed to symlink {} to {}",
                    shadow_file.display(),
                    user_file.display()
                )
            })?;
        }
    }
    Ok(())
}

fn ensure_omo_package_installed(
    shadow_dir: &Path,
    shadow_node_modules: &Path,
    user_node_modules: &Path,
    child_path: &std::ffi::OsStr,
) -> Result<()> {
    let package_dir = shadow_node_modules.join(OMO_PLUGIN_NAME);
    if package_dir.exists() {
        return Ok(());
    }
    if let Some(bun) = executable_in_path("bun", child_path) {
        eprintln!("Installing {OMO_PLUGIN_NAME} plugin (this may take a minute on first run)...");
        let first_status =
            run_omo_package_manager(&bun, &["add", OMO_PLUGIN_NAME], shadow_dir, child_path)?;
        if first_status != 0 {
            eprintln!("Retrying {OMO_PLUGIN_NAME} install with a clean shadow package state...");
            let _ = fs::remove_file(shadow_dir.join("bun.lock"));
            if fs::symlink_metadata(shadow_node_modules).is_ok() {
                if fs::symlink_metadata(shadow_node_modules)
                    .map(|metadata| metadata.file_type().is_symlink())
                    .unwrap_or(false)
                {
                    let _ = fs::remove_file(shadow_node_modules);
                } else {
                    let _ = fs::remove_dir_all(shadow_node_modules);
                }
            }
            ensure_shadow_node_modules_symlink(shadow_node_modules, user_node_modules)?;
            let retry_status =
                run_omo_package_manager(&bun, &["add", OMO_PLUGIN_NAME], shadow_dir, child_path)?;
            if retry_status != 0 {
                bail!("Failed to install {OMO_PLUGIN_NAME}. Try manually: npm install -g {OMO_PLUGIN_NAME}");
            }
        }
        eprintln!("{OMO_PLUGIN_NAME} plugin installed");
        return Ok(());
    }
    if let Some(npm) = executable_in_path("npm", child_path) {
        eprintln!("Installing {OMO_PLUGIN_NAME} plugin (this may take a minute on first run)...");
        let status =
            run_omo_package_manager(&npm, &["install", OMO_PLUGIN_NAME], shadow_dir, child_path)?;
        if status != 0 {
            bail!("Failed to install {OMO_PLUGIN_NAME}. Try manually: npm install -g {OMO_PLUGIN_NAME}");
        }
        eprintln!("{OMO_PLUGIN_NAME} plugin installed");
        return Ok(());
    }
    bail!("Neither bun nor npm found in PATH. Install {OMO_PLUGIN_NAME} manually: bunx {OMO_PLUGIN_NAME} install");
}

fn executable_in_path(name: &str, path: &std::ffi::OsStr) -> Option<PathBuf> {
    std::env::split_paths(path)
        .filter(|dir| !is_macos_app_bundle_launcher_dir(dir))
        .map(|dir| dir.join(name))
        .find(|candidate| is_executable_file(candidate))
}

fn run_omo_package_manager(
    executable: &Path,
    args: &[&str],
    current_dir: &Path,
    child_path: &std::ffi::OsStr,
) -> Result<i32> {
    let output = Command::new(executable)
        .args(args)
        .current_dir(current_dir)
        .env("PATH", child_path)
        .output()
        .with_context(|| format!("failed to run {}", executable.display()))?;
    let mut stderr = io::stderr().lock();
    stderr.write_all(&output.stdout)?;
    stderr.write_all(&output.stderr)?;
    Ok(output.status.code().unwrap_or(1))
}

fn ensure_omo_tmux_config(user_dir: &Path, shadow_dir: &Path) -> Result<()> {
    let config_path = shadow_dir.join("oh-my-openagent.json");
    let mut config = if let Some(config) = read_optional_json_object(&config_path)? {
        config
    } else {
        read_first_json_object(&[
            user_dir.join("oh-my-openagent.json"),
            user_dir.join("oh-my-opencode.json"),
            shadow_dir.join("oh-my-opencode.json"),
        ])?
        .unwrap_or_default()
    };

    let mut tmux = config
        .remove("tmux")
        .and_then(|value| value.as_object().cloned())
        .unwrap_or_default();
    let mut changed = false;
    if tmux.get("enabled").and_then(Value::as_bool) != Some(true) {
        tmux.insert("enabled".to_string(), json!(true));
        changed = true;
    }
    for (key, value) in [
        ("main_pane_min_width", 60),
        ("agent_pane_min_width", 30),
        ("main_pane_size", 50),
    ] {
        if !tmux.contains_key(key) {
            tmux.insert(key.to_string(), json!(value));
            changed = true;
        }
    }
    if changed {
        config.insert("tmux".to_string(), Value::Object(tmux));
        remove_if_symlink(&config_path)?;
        fs::write(
            &config_path,
            serde_json::to_string_pretty(&Value::Object(config))? + "\n",
        )
        .with_context(|| format!("failed to write {}", config_path.display()))?;
    }
    Ok(())
}

fn read_first_json_object(paths: &[PathBuf]) -> Result<Option<Map<String, Value>>> {
    for path in paths {
        if let Some(value) = read_optional_json_object(path)? {
            return Ok(Some(value));
        }
    }
    Ok(None)
}

fn read_optional_json_object(path: &Path) -> Result<Option<Map<String, Value>>> {
    let text = match fs::read_to_string(path) {
        Ok(text) => text,
        Err(err) if err.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(err) => return Err(err).with_context(|| format!("failed to read {}", path.display())),
    };
    if text.trim().is_empty() {
        return Ok(Some(Map::new()));
    }
    let value: Value = serde_json::from_str(&text)
        .with_context(|| format!("{} is not valid JSON", path.display()))?;
    Ok(value.as_object().cloned())
}

fn print_docs_topic(topic: &str, json_output: bool) -> Result<()> {
    let normalized = match topic {
        "api" | "browser" | "agents" | "dock" | "sidebars" | "shortcuts" => topic,
        "sidebar" => "sidebars",
        "keyboard" | "keyboard-shortcuts" => "shortcuts",
        other => bail!("unknown docs topic: {other}"),
    };
    let payload = docs_topic_payload(normalized);
    if json_output {
        println!("{}", serde_json::to_string_pretty(&payload)?);
        return Ok(());
    }
    let title = payload
        .get("title")
        .and_then(Value::as_str)
        .unwrap_or(normalized);
    println!("{title}");
    if let Some(summary) = payload.get("summary").and_then(Value::as_str) {
        println!("{summary}");
    }
    if let Some(url) = payload.get("docs_url").and_then(Value::as_str) {
        println!("Docs: {url}");
    }
    if let Some(resources) = payload.get("resources").and_then(Value::as_array) {
        println!("Resources:");
        for resource in resources {
            if let Some(resource) = resource.as_str() {
                println!("  {resource}");
            }
        }
    }
    if let Some(commands) = payload.get("commands").and_then(Value::as_array) {
        println!("Commands:");
        for command in commands {
            if let Some(command) = command.as_str() {
                println!("  {command}");
            }
        }
    }
    if let Some(workflow) = payload.get("workflow").and_then(Value::as_array) {
        println!("Workflow:");
        for step in workflow {
            if let Some(step) = step.as_str() {
                println!("  {step}");
            }
        }
    }
    if let Some(concepts) = payload.get("concepts").and_then(Value::as_array) {
        println!("Concepts:");
        for concept in concepts {
            if let Some(concept) = concept.as_str() {
                println!("  {concept}");
            }
        }
    }
    Ok(())
}

fn docs_topic_payload(topic: &str) -> Value {
    match topic {
        "api" => json!({
            "topic": "api",
            "title": "API docs",
            "summary": "Socket and CLI contracts for Linux and macOS-compatible automation.",
            "docs_url": "web/app/[locale]/docs/api/page.tsx",
            "resources": [
                "docs/cli-contract.md",
                "docs/v2-api-migration.md",
                "docs/agent-browser-port-spec.md"
            ],
            "commands": [
                "cmux capabilities --json",
                "cmux identify --json",
                "cmux rpc <method> '<json-params>'"
            ]
        }),
        "browser" => json!({
            "topic": "browser",
            "title": "Browser automation docs",
            "summary": "Agent-facing browser workflow, command mapping, and raw skill resources.",
            "docs_url": "web/app/[locale]/docs/browser-automation/page.tsx",
            "resources": [
                "skills/cmux-browser/SKILL.md",
                "skills/cmux-browser/references/commands.md",
                "skills/cmux-browser/references/snapshot-refs.md",
                "skills/cmux-browser/references/session-management.md",
                "skills/cmux-browser/references/authentication.md",
                "docs/agent-browser-port-spec.md",
                "docs/cli-contract.md"
            ],
            "commands": [
                "cmux identify --json",
                "cmux browser open <url> --json",
                "cmux browser <surface> snapshot --interactive",
                "cmux browser <surface> click <selector-or-ref> --snapshot-after --json",
                "cmux browser <surface> get url --json"
            ],
            "workflow": [
                "identify current window/workspace/pane/surface",
                "open or choose a browser surface",
                "verify URL and wait for load state or content",
                "snapshot to get fresh element refs",
                "act, then request a post-action snapshot or re-snapshot"
            ],
            "concepts": [
                "window: top-level app window",
                "workspace: sidebar entry within a window",
                "pane: split region inside a workspace",
                "surface: tab within a pane; browser automation targets browser surfaces"
            ]
        }),
        "agents" => json!({
            "topic": "agents",
            "title": "Agent integration docs",
            "summary": "Hook installers, Feed routes, and cmux skills used by coding agents.",
            "docs_url": "web/app/[locale]/docs/skills/page.tsx",
            "resources": [
                "skills/cmux/SKILL.md",
                "skills/cmux-workspace/SKILL.md",
                "skills/cmux-browser/SKILL.md",
                "docs/feed.md",
                "docs/cli-contract.md"
            ],
            "commands": [
                "cmux hooks setup --agent <name>",
                "cmux feed list",
                "cmux feed tui",
                "cmux install-codex-integration",
                "cmux install-claude-code-integration"
            ]
        }),
        "dock" => json!({
            "topic": "dock",
            "title": "Dock and right-sidebar docs",
            "summary": "Right-sidebar terminal controls and mode switching.",
            "docs_url": "web/app/[locale]/docs/dock/page.tsx",
            "resources": [
                "docs/cli-contract.md",
                "skills/cmux/references/windows-workspaces.md"
            ],
            "commands": [
                "cmux right-sidebar show",
                "cmux right-sidebar set feed",
                "cmux right-sidebar mode --json"
            ]
        }),
        "sidebars" => json!({
            "topic": "sidebars",
            "title": "Sidebar docs",
            "summary": "Custom sidebar surfaces, metadata, status rows, progress, and logs.",
            "docs_url": "web/app/[locale]/docs/custom-commands/page.tsx",
            "resources": [
                "docs/custom-sidebars.md",
                "docs/data-driven-sidebar-plan.md",
                "docs/cli-contract.md"
            ],
            "commands": [
                "cmux set-status <key> <label>",
                "cmux set-progress <value>",
                "cmux log -- <message>",
                "cmux sidebar-state --json"
            ]
        }),
        "shortcuts" => json!({
            "topic": "shortcuts",
            "title": "Keyboard shortcut docs",
            "summary": "Shortcut reference and settings resources.",
            "docs_url": "web/app/[locale]/docs/keyboard-shortcuts/page.tsx",
            "resources": [
                "skills/cmux-settings/references/shortcut-actions.md",
                "skills/cmux-settings/references/all-keys.md",
                "docs/cli-contract.md"
            ],
            "commands": [
                "cmux shortcuts",
                "cmux settings keyboardShortcuts"
            ]
        }),
        _ => json!({"topic": topic}),
    }
}

fn maybe_run_browser_availability_command(options: &GlobalOptions) -> Result<bool> {
    let Some((action, command_json)) = browser_availability_action(&options.command)? else {
        return Ok(false);
    };
    let path = browser_settings::settings_path();
    let enabled = match action {
        "disable" => {
            browser_settings::save_enabled(&path, false)?;
            false
        }
        "enable" => {
            browser_settings::save_enabled(&path, true)?;
            true
        }
        "status" => browser_settings::load_enabled(&path),
        _ => return Ok(false),
    };
    let payload = browser_settings::payload(&path, enabled);
    if options.json || command_json {
        println!("{}", serde_json::to_string(&payload)?);
    } else if action == "status" {
        println!("{}", if enabled { "enabled" } else { "disabled" });
    } else {
        println!(
            "cmux browser {}",
            if enabled { "enabled" } else { "disabled" }
        );
    }
    Ok(true)
}

fn browser_availability_action(command: &[String]) -> Result<Option<(&'static str, bool)>> {
    let Some(name) = command.first().map(String::as_str) else {
        return Ok(None);
    };
    let mut args = command
        .iter()
        .skip(1)
        .map(String::as_str)
        .collect::<Vec<_>>();
    let command_json = args.iter().any(|arg| *arg == "--json");
    args.retain(|arg| *arg != "--json");
    match name {
        "disable-browser" => {
            ensure_no_browser_availability_args(&args)?;
            Ok(Some(("disable", command_json)))
        }
        "enable-browser" => {
            ensure_no_browser_availability_args(&args)?;
            Ok(Some(("enable", command_json)))
        }
        "browser-status" => {
            ensure_no_browser_availability_args(&args)?;
            Ok(Some(("status", command_json)))
        }
        "browser" => {
            let Some(action) = args.first().copied() else {
                return Ok(None);
            };
            let normalized = action.to_ascii_lowercase();
            let parsed = match normalized.as_str() {
                "disable" => "disable",
                "enable" => "enable",
                "status" => "status",
                _ => return Ok(None),
            };
            ensure_no_browser_availability_args(&args[1..])?;
            Ok(Some((parsed, command_json)))
        }
        _ => Ok(None),
    }
}

fn ensure_no_browser_availability_args(args: &[&str]) -> Result<()> {
    if let Some(arg) = args.first() {
        bail!("Unexpected argument: {arg}");
    }
    Ok(())
}

fn agent_hibernation_params(command: &[String]) -> Result<Value> {
    let args = command
        .iter()
        .skip(1)
        .filter(|arg| arg.as_str() != "--json")
        .map(String::as_str)
        .collect::<Vec<_>>();
    let Some(state) = args.first().copied() else {
        bail!("Usage: cmux agent-hibernation <on|off> [--json]");
    };
    if args.len() > 1 {
        bail!("Unexpected argument: {}", args[1]);
    }
    let enabled = match state.to_ascii_lowercase().as_str() {
        "on" | "enable" => true,
        "off" | "disable" => false,
        _ => bail!("Usage: cmux agent-hibernation <on|off> [--json]"),
    };
    Ok(json!({"enabled": enabled}))
}

fn print_remote_daemon_status(command: &[String], global_json: bool) -> Result<()> {
    let payload = remote_daemon_status_payload(command)?;
    if global_json || command_has_flag(command, "--json") {
        println!("{}", serde_json::to_string(&payload)?);
        return Ok(());
    }

    println!(
        "app version: {}",
        payload
            .get("app_version")
            .and_then(Value::as_str)
            .unwrap_or("unknown")
    );
    println!(
        "remote daemon version: {}",
        payload
            .get("remote_daemon_version")
            .and_then(Value::as_str)
            .unwrap_or("unknown")
    );
    println!(
        "manifest: {}",
        if payload
            .get("manifest_present")
            .and_then(Value::as_bool)
            .unwrap_or(false)
        {
            "present"
        } else {
            "missing"
        }
    );
    println!(
        "platform: {}/{}",
        payload
            .get("target_goos")
            .and_then(Value::as_str)
            .unwrap_or("unknown"),
        payload
            .get("target_goarch")
            .and_then(Value::as_str)
            .unwrap_or("unknown")
    );
    println!(
        "cache platform: {}",
        payload
            .get("cache_platform")
            .and_then(Value::as_str)
            .unwrap_or("unknown")
    );
    println!(
        "release: {}",
        payload
            .get("release_tag")
            .and_then(Value::as_str)
            .unwrap_or("unknown")
    );
    println!(
        "asset: {}",
        payload
            .get("asset_name")
            .and_then(Value::as_str)
            .unwrap_or("unknown")
    );
    println!(
        "cache: {}",
        payload
            .get("cache_path")
            .and_then(Value::as_str)
            .unwrap_or("unknown")
    );
    println!(
        "cache exists: {}",
        if payload
            .get("cache_exists")
            .and_then(Value::as_bool)
            .unwrap_or(false)
        {
            "yes"
        } else {
            "no"
        }
    );
    if let Some(cache_sha) = payload.get("cache_sha256").and_then(Value::as_str) {
        println!("cache sha256: {cache_sha}");
    }
    println!(
        "cache verified: {}",
        if payload
            .get("cache_verified")
            .and_then(Value::as_bool)
            .unwrap_or(false)
        {
            "yes"
        } else {
            "no"
        }
    );
    if let Some(local_binary) = payload.get("local_binary_path").and_then(Value::as_str) {
        println!("local binary: {local_binary}");
    }
    if let Some(local_sha) = payload.get("local_binary_sha256").and_then(Value::as_str) {
        println!("local binary sha256: {local_sha}");
    }
    println!(
        "download command: {}",
        payload
            .get("download_command")
            .and_then(Value::as_str)
            .unwrap_or("unknown")
    );
    println!(
        "download checksums: {}",
        payload
            .get("download_checksums_command")
            .and_then(Value::as_str)
            .unwrap_or("unknown")
    );
    println!(
        "verify checksum: {}",
        payload
            .get("checksum_verify_command")
            .and_then(Value::as_str)
            .unwrap_or("unknown")
    );
    println!(
        "attestation verify: {}",
        payload
            .get("attestation_verify_command")
            .and_then(Value::as_str)
            .unwrap_or("unknown")
    );
    println!("note: this build has no embedded remote daemon manifest. It uses the local cmuxd-remote binary for Linux SSH bootstrap.");
    Ok(())
}

fn remote_daemon_status_payload(command: &[String]) -> Result<Value> {
    let (target_goos, target_goarch) = remote_daemon_target(command)?;
    let version = remote_daemon_version();
    let cache_platform = remote_daemon_cache_platform(&target_goos, &target_goarch);
    let cache_path = std::env::temp_dir()
        .join("cmux-remote-daemon-build")
        .join(&version)
        .join(&cache_platform)
        .join("cmuxd-remote");
    let cache_exists = cache_path.is_file();
    let cache_sha256 = if cache_exists {
        Some(sha256_hex(&cache_path)?)
    } else {
        None
    };
    let local_binary_path = local_remote_daemon_binary_path();
    let local_binary_sha256 = local_binary_path.as_deref().map(sha256_hex).transpose()?;
    let asset_name = format!("cmuxd-remote-{target_goos}-{target_goarch}");
    let release_tag = "unknown";
    let checksums_asset_name = "unknown";
    let signer_workflow = "manaflow-ai/cmux/.github/workflows/release.yml";

    Ok(json!({
        "app_version": env!("CARGO_PKG_VERSION"),
        "remote_daemon_version": version,
        "build": Value::Null,
        "commit": Value::Null,
        "manifest_present": false,
        "release_tag": release_tag,
        "release_url": Value::Null,
        "target_goos": target_goos,
        "target_goarch": target_goarch,
        "cache_platform": cache_platform,
        "asset_name": asset_name,
        "download_url": "unknown",
        "checksums_asset_name": checksums_asset_name,
        "checksums_url": "unknown",
        "expected_sha256": Value::Null,
        "cache_path": cache_path.display().to_string(),
        "cache_exists": cache_exists,
        "cache_sha256": cache_sha256,
        "cache_verified": false,
        "local_binary_path": local_binary_path.map(|path| path.display().to_string()),
        "local_binary_exists": local_binary_sha256.is_some(),
        "local_binary_sha256": local_binary_sha256,
        "dev_local_build_fallback": true,
        "download_command": format!("gh release download {release_tag} --repo manaflow-ai/cmux --pattern {asset_name}"),
        "download_checksums_command": format!("gh release download {release_tag} --repo manaflow-ai/cmux --pattern {checksums_asset_name}"),
        "checksum_verify_command": format!("shasum -a 256 -c {checksums_asset_name} --ignore-missing"),
        "attestation_verify_command": format!("gh attestation verify ./{asset_name} --repo manaflow-ai/cmux --signer-workflow {signer_workflow}"),
    }))
}

fn remote_daemon_target(command: &[String]) -> Result<(String, String)> {
    let mut goos = None;
    let mut goarch = None;
    let mut index = 1;
    while index < command.len() {
        match command[index].as_str() {
            "--json" => {}
            "--os" => {
                index += 1;
                let value = command.get(index).context("--os requires a value")?;
                goos = Some(normalized_remote_daemon_os(value)?);
            }
            "--arch" => {
                index += 1;
                let value = command.get(index).context("--arch requires a value")?;
                goarch = Some(normalized_remote_daemon_arch(value)?);
            }
            other => bail!("remote-daemon-status: unexpected argument {other}"),
        }
        index += 1;
    }
    Ok((
        goos.unwrap_or_else(host_goos),
        goarch.unwrap_or_else(host_goarch),
    ))
}

fn normalized_remote_daemon_os(value: &str) -> Result<String> {
    match value.trim().to_ascii_lowercase().as_str() {
        "darwin" | "macos" | "mac" => Ok("darwin".to_string()),
        "linux" => Ok("linux".to_string()),
        other if other.is_empty() => bail!("--os requires a value"),
        other => bail!("unsupported remote daemon os: {other}"),
    }
}

fn normalized_remote_daemon_arch(value: &str) -> Result<String> {
    match value.trim().to_ascii_lowercase().as_str() {
        "amd64" | "x86_64" => Ok("amd64".to_string()),
        "arm64" | "aarch64" => Ok("arm64".to_string()),
        other if other.is_empty() => bail!("--arch requires a value"),
        other => bail!("unsupported remote daemon arch: {other}"),
    }
}

fn host_goos() -> String {
    match std::env::consts::OS {
        "macos" => "darwin".to_string(),
        other => other.to_string(),
    }
}

fn host_goarch() -> String {
    match std::env::consts::ARCH {
        "x86_64" => "amd64".to_string(),
        "aarch64" => "arm64".to_string(),
        other => other.to_string(),
    }
}

fn remote_daemon_cache_platform(goos: &str, goarch: &str) -> String {
    let rust_arch = match goarch {
        "amd64" => "x86_64",
        "arm64" => "aarch64",
        other => other,
    };
    format!("{goos}-{rust_arch}")
}

fn remote_daemon_version() -> String {
    format!("cmux-linux-{}", env!("CARGO_PKG_VERSION"))
}

fn local_remote_daemon_binary_path() -> Option<PathBuf> {
    if let Ok(path) = std::env::var("CMUX_LINUX_REMOTE_DAEMON_PATH") {
        let path = PathBuf::from(path);
        if path.is_file() {
            return Some(path);
        }
    }
    let mut sibling = std::env::current_exe().ok()?;
    sibling.set_file_name("cmuxd-remote");
    sibling.is_file().then_some(sibling)
}

fn sha256_hex(path: &Path) -> Result<String> {
    let bytes = fs::read(path).with_context(|| format!("failed to read {}", path.display()))?;
    let digest = Sha256::digest(&bytes);
    Ok(format!("{digest:x}"))
}

fn print_config_snapshot(json_output: bool, validation: bool) -> Result<()> {
    let snapshot = config::snapshot();
    if json_output {
        println!("{}", serde_json::to_string_pretty(&snapshot)?);
    } else {
        config::print_text(&snapshot, validation);
    }
    Ok(())
}

fn print_settings_docs(json_output: bool) -> Result<()> {
    let payload = config::settings_docs_payload();
    if json_output {
        println!("{}", serde_json::to_string_pretty(&payload)?);
    } else {
        config::print_settings_docs_text(&payload);
    }
    Ok(())
}

fn run_config_doctor_command(options: &GlobalOptions) -> Result<()> {
    let paths = config_doctor_paths(&options.command)?;
    let report = config::doctor(&paths).map_err(anyhow::Error::msg)?;
    let json_output = options.json || command_has_flag(&options.command, "--json");
    if json_output {
        println!("{}", serde_json::to_string_pretty(&report)?);
    } else {
        config::print_doctor_text(&report);
    }
    if report.error_count > 0 {
        bail!("cmux config doctor found {} error(s)", report.error_count);
    }
    Ok(())
}

fn config_doctor_paths(command: &[String]) -> Result<Vec<String>> {
    let mut paths = Vec::new();
    let mut index = 2;
    while index < command.len() {
        let arg = &command[index];
        match arg.as_str() {
            "--path" => {
                index += 1;
                paths.push(
                    command
                        .get(index)
                        .cloned()
                        .context("cmux config doctor --path requires a path")?,
                );
            }
            "--json" => {}
            "--" => {}
            other if other.starts_with("--path=") => {
                let raw = other.trim_start_matches("--path=");
                if raw.is_empty() {
                    bail!("cmux config doctor --path requires a path");
                }
                paths.push(raw.to_string());
            }
            other if other.starts_with('-') => {
                bail!("Unknown config doctor option '{other}'");
            }
            other => bail!("Unknown config doctor argument '{other}'. Use --path <path>."),
        }
        index += 1;
    }
    Ok(paths)
}

fn config_font_size_command_needs_no_socket(command: &[String]) -> bool {
    let subcommand = command.get(1).map(String::as_str).unwrap_or("help");
    if subcommand == "get" || subcommand == "set" {
        return true;
    }
    config::canonical_font_size_key(subcommand).is_some()
}

fn run_config_font_size_command(options: &GlobalOptions) -> Result<()> {
    let command = &options.command;
    let subcommand = command.get(1).map(String::as_str).unwrap_or("help");
    match subcommand {
        "get" => {
            if command.len() != 3 {
                bail!("Usage: cmux config get <sidebar-font-size|surface-tab-bar-font-size>");
            }
            let key = config::canonical_font_size_key(&command[2])
                .context("Usage: cmux config get <sidebar-font-size|surface-tab-bar-font-size>")?;
            let payload = config::get_font_size(key).map_err(anyhow::Error::msg)?;
            print_config_get_font_size(&payload, options.json)
        }
        "set" => {
            if command.len() != 4 {
                bail!(
                    "Usage: cmux config set <sidebar-font-size|surface-tab-bar-font-size> <points>"
                );
            }
            let key = config::canonical_font_size_key(&command[2]).context(
                "Usage: cmux config set <sidebar-font-size|surface-tab-bar-font-size> <points>",
            )?;
            run_config_set_font_size(options, key, &command[3])
        }
        other => {
            let key = config::canonical_font_size_key(other)
                .context("Unknown config subcommand. Run 'cmux config --help'.")?;
            match command.len() {
                2 => {
                    let payload = config::get_font_size(key).map_err(anyhow::Error::msg)?;
                    print_config_get_font_size(&payload, options.json)
                }
                3 => run_config_set_font_size(options, key, &command[2]),
                _ => bail!("Usage: cmux config {other} [points]"),
            }
        }
    }
}

fn run_config_set_font_size(options: &GlobalOptions, key: &str, raw_value: &str) -> Result<()> {
    let mut payload = config::set_font_size(key, raw_value, "skipped".to_string(), None)
        .map_err(anyhow::Error::msg)?;
    let (reload, reload_message) = reload_config_after_font_size_set(options);
    payload.reload = reload;
    payload.reload_message = reload_message;
    if options.json {
        println!("{}", serde_json::to_string_pretty(&payload)?);
        return Ok(());
    }

    match payload.reload.as_str() {
        "reloaded" => println!("OK {} = {} (reloaded)", payload.key, payload.formatted),
        "failed" => {
            println!(
                "OK {} = {} (saved; reload failed)",
                payload.key, payload.formatted
            );
            if let Some(message) = &payload.reload_message {
                println!("reload: {message}");
            }
            println!("Run `cmux config reload` after cmux is running to apply it.");
        }
        _ => {
            println!("OK {} = {} (saved)", payload.key, payload.formatted);
            println!("Run `cmux config reload` to apply it.");
        }
    }
    println!("path: {}", payload.path);
    Ok(())
}

fn print_config_get_font_size(
    payload: &config::FontSizeGetPayload,
    json_output: bool,
) -> Result<()> {
    if json_output {
        println!("{}", serde_json::to_string_pretty(payload)?);
    } else {
        println!("{} = {}", payload.key, payload.formatted);
        println!("path: {}", payload.path);
    }
    Ok(())
}

fn reload_config_after_font_size_set(options: &GlobalOptions) -> (String, Option<String>) {
    let Some(socket_path) = reload_socket_path_if_available(options) else {
        return ("skipped".to_string(), None);
    };
    match call_socket(&socket_path, "config.reload", json!({})) {
        Ok(value) => (
            "reloaded".to_string(),
            value
                .get("text")
                .and_then(Value::as_str)
                .map(ToString::to_string),
        ),
        Err(err) => ("failed".to_string(), Some(err.to_string())),
    }
}

fn reload_socket_path_if_available(options: &GlobalOptions) -> Option<String> {
    let socket = resolve_socket_path(options).ok()?;
    if parse_tcp_socket_addr(&socket).is_some() {
        if options.explicit_socket
            || normalized_env("CMUX_SOCKET_PATH").is_some()
            || normalized_env("CMUX_SOCKET").is_some()
        {
            return Some(socket);
        }
        return None;
    }
    if options.explicit_socket || Path::new(&socket).exists() {
        Some(socket)
    } else {
        None
    }
}

fn run_window_default_display_command(options: &GlobalOptions) -> Result<()> {
    let (display, clear) = window_default_display_args(&options.command)?;
    let payload = if clear {
        config::set_window_default_display(None).map_err(anyhow::Error::msg)?
    } else if let Some(display) = display {
        config::set_window_default_display(Some(display)).map_err(anyhow::Error::msg)?
    } else {
        config::get_window_default_display()
    };
    print_window_default_display(
        &payload,
        options.json || command_has_flag(&options.command, "--json"),
    )
}

fn window_default_display_args(command: &[String]) -> Result<(Option<String>, bool)> {
    let mut display = None;
    let mut clear = false;
    let mut literal = false;
    for arg in command.iter().skip(2) {
        if literal {
            if display.is_some() {
                bail!("Usage: cmux window default-display [<name>|--clear]");
            }
            display = Some(arg.clone());
            continue;
        }
        match arg.as_str() {
            "--" => literal = true,
            "--json" => {}
            "--clear" => clear = true,
            other if other.starts_with('-') => {
                bail!("Unknown window default-display option '{other}'");
            }
            other => {
                if display.is_some() {
                    bail!("Usage: cmux window default-display [<name>|--clear]");
                }
                display = Some(other.to_string());
            }
        }
    }
    if clear && display.is_some() {
        bail!("Usage: cmux window default-display [<name>|--clear]");
    }
    Ok((display, clear))
}

fn print_window_default_display(
    payload: &config::WindowDefaultDisplayPayload,
    json_output: bool,
) -> Result<()> {
    if json_output {
        println!("{}", serde_json::to_string_pretty(payload)?);
        return Ok(());
    }
    if payload.cleared {
        println!("OK default-display cleared");
    } else if let Some(display) = payload.display.as_deref() {
        println!("default-display = {display}");
    } else {
        println!("default-display = unset");
    }
    println!("path: {}", payload.path);
    Ok(())
}

fn run_themes_command(json_output: bool, command: &[String]) -> Result<()> {
    if command.len() == 1 {
        if json_output || !themes_picker_tty_available() {
            return print_themes_list(json_output);
        }
        return run_themes_picker();
    }

    let subcommand = command.get(1).map(String::as_str).unwrap_or("list");
    match subcommand {
        "list" => {
            if command.len() > 2 {
                bail!("themes list does not take any positional arguments");
            }
            print_themes_list(json_output)
        }
        "set" => run_themes_set(json_output, &command[2..]),
        "clear" => {
            if command.len() > 2 {
                bail!("themes clear does not take any positional arguments");
            }
            let payload = config::clear_theme_override().map_err(anyhow::Error::msg)?;
            if json_output {
                println!("{}", serde_json::to_string_pretty(&payload)?);
            } else {
                println!(
                    "OK cleared config={} reload={}",
                    payload.config_path,
                    if payload.reload_requested {
                        "requested"
                    } else {
                        "unavailable"
                    }
                );
            }
            Ok(())
        }
        other if other.starts_with('-') => {
            bail!("Unknown themes subcommand '{other}'. Run 'cmux themes --help'.")
        }
        _ => run_themes_set(json_output, &command[1..]),
    }
}

fn print_themes_list(json_output: bool) -> Result<()> {
    let payload = config::themes_list_payload();
    if json_output {
        println!("{}", serde_json::to_string_pretty(&payload)?);
        return Ok(());
    }
    println!(
        "Current light: {}",
        payload.current.light.as_deref().unwrap_or("inherit")
    );
    println!(
        "Current dark: {}",
        payload.current.dark.as_deref().unwrap_or("inherit")
    );
    println!("Config: {}", payload.config_path);
    if let Some(source) = payload.current.source_path.as_deref() {
        println!("Source: {source}");
    }
    println!();
    if payload.themes.is_empty() {
        println!("No themes found.");
        return Ok(());
    }
    for theme in payload.themes {
        let mut badges = Vec::new();
        if theme.current_light {
            badges.push("light");
        }
        if theme.current_dark {
            badges.push("dark");
        }
        if badges.is_empty() {
            println!("{}", theme.name);
        } else {
            println!("{}  [{}]", theme.name, badges.join(", "));
        }
    }
    Ok(())
}

fn run_themes_set(json_output: bool, args: &[String]) -> Result<()> {
    let mut light = None;
    let mut dark = None;
    let mut positional = Vec::new();
    let mut index = 0;
    while index < args.len() {
        let arg = &args[index];
        match arg.as_str() {
            "--light" => {
                index += 1;
                light = Some(
                    args.get(index)
                        .cloned()
                        .context("--light requires a value")?,
                );
            }
            "--dark" => {
                index += 1;
                dark = Some(
                    args.get(index)
                        .cloned()
                        .context("--dark requires a value")?,
                );
            }
            other if other.starts_with("--") => {
                bail!("themes set: unknown flag '{other}'. Known flags: --light <theme>, --dark <theme>");
            }
            other => positional.push(other.to_string()),
        }
        index += 1;
    }

    let payload = if light.is_none() && dark.is_none() {
        let theme = positional.join(" ").trim().to_string();
        if theme.is_empty() {
            bail!("themes set requires a theme name or --light/--dark flags");
        }
        config::set_theme_override(Some(theme.clone()), Some(theme)).map_err(anyhow::Error::msg)?
    } else {
        if !positional.is_empty() {
            bail!("themes set: unexpected argument '{}'", positional.join(" "));
        }
        config::set_theme_override(light, dark).map_err(anyhow::Error::msg)?
    };

    if json_output {
        println!("{}", serde_json::to_string_pretty(&payload)?);
    } else {
        println!(
            "OK light={} dark={} config={} reload={}",
            payload.light.as_deref().unwrap_or("-"),
            payload.dark.as_deref().unwrap_or("-"),
            payload.config_path,
            if payload.reload_requested {
                "requested"
            } else {
                "unavailable"
            }
        );
    }
    Ok(())
}

fn themes_picker_tty_available() -> bool {
    io::stdin().is_terminal() && io::stdout().is_terminal()
}

fn run_themes_picker() -> Result<()> {
    let payload = config::themes_list_payload();
    let Some(theme) = prompt_theme_picker(&payload)? else {
        return Ok(());
    };
    let result =
        config::set_theme_override(Some(theme.clone()), Some(theme)).map_err(anyhow::Error::msg)?;
    println!(
        "OK light={} dark={} config={} reload={}",
        result.light.as_deref().unwrap_or("-"),
        result.dark.as_deref().unwrap_or("-"),
        result.config_path,
        if result.reload_requested {
            "requested"
        } else {
            "unavailable"
        }
    );
    Ok(())
}

fn prompt_theme_picker(payload: &config::ThemeListPayload) -> Result<Option<String>> {
    if payload.themes.is_empty() {
        println!("No themes found.");
        return Ok(None);
    }
    print!("{}", themes_picker_text(payload));
    print!("Theme number/name (blank to cancel): ");
    io::stdout()
        .flush()
        .context("failed to flush theme prompt")?;

    let mut input = String::new();
    io::stdin()
        .read_line(&mut input)
        .context("failed to read theme selection")?;
    let selection = theme_picker_selection(payload, &input)?;
    if selection.is_none() {
        println!("Cancelled.");
    }
    Ok(selection)
}

fn themes_picker_text(payload: &config::ThemeListPayload) -> String {
    let mut out = String::new();
    out.push_str("cmux themes\n\n");
    out.push_str(&format!(
        "Current light: {}\n",
        payload.current.light.as_deref().unwrap_or("inherit")
    ));
    out.push_str(&format!(
        "Current dark: {}\n",
        payload.current.dark.as_deref().unwrap_or("inherit")
    ));
    out.push_str(&format!("Config: {}\n\n", payload.config_path));
    for (index, theme) in payload.themes.iter().enumerate() {
        let mut badges = Vec::new();
        if theme.current_light {
            badges.push("light");
        }
        if theme.current_dark {
            badges.push("dark");
        }
        if badges.is_empty() {
            out.push_str(&format!("{:>2}. {}\n", index + 1, theme.name));
        } else {
            out.push_str(&format!(
                "{:>2}. {}  [{}]\n",
                index + 1,
                theme.name,
                badges.join(", ")
            ));
        }
    }
    out.push('\n');
    out
}

fn theme_picker_selection(
    payload: &config::ThemeListPayload,
    input: &str,
) -> Result<Option<String>> {
    let trimmed = input.trim();
    if trimmed.is_empty() || matches!(trimmed, "q" | "quit" | "cancel") {
        return Ok(None);
    }
    if let Ok(index) = trimmed.parse::<usize>() {
        let Some(theme) = index
            .checked_sub(1)
            .and_then(|index| payload.themes.get(index))
        else {
            bail!(
                "theme selection {index} is out of range; choose 1-{}",
                payload.themes.len()
            );
        };
        return Ok(Some(theme.name.clone()));
    }
    payload
        .themes
        .iter()
        .find(|theme| theme.name.eq_ignore_ascii_case(trimmed))
        .map(|theme| Some(theme.name.clone()))
        .ok_or_else(|| {
            anyhow::anyhow!("Unknown theme '{trimmed}'. Enter a number, name, or blank to cancel.")
        })
}

fn run_set_hook(command: &[String]) -> Result<()> {
    let path = cmux_tmp_dir().join("hooks.txt");
    if command_has_flag(command, "--list") {
        if let Ok(text) = fs::read_to_string(path) {
            print!("{text}");
        }
        return Ok(());
    }

    let args = command
        .iter()
        .skip(1)
        .filter(|arg| !matches!(arg.as_str(), "--unset" | "--list"))
        .cloned()
        .collect::<Vec<_>>();
    let hook_name = args
        .first()
        .cloned()
        .context("set-hook requires a hook name")?;
    let mut hooks = fs::read_to_string(&path)
        .unwrap_or_default()
        .lines()
        .map(ToString::to_string)
        .collect::<Vec<_>>();
    hooks.retain(|line| !line.starts_with(&format!("{hook_name}\t")));
    if !command_has_flag(command, "--unset") {
        let body = args.get(1).cloned().unwrap_or_default();
        hooks.push(format!("{hook_name}\t{body}"));
    }
    fs::write(
        path,
        hooks.join("\n") + if hooks.is_empty() { "" } else { "\n" },
    )?;
    println!("OK");
    Ok(())
}

fn run_bind_key(command: &[String]) -> Result<()> {
    if command_has_flag(command, "--list") || command_has_flag(command, "-L") {
        if let Ok(text) = fs::read_to_string(bindings_path()) {
            print!("{text}");
        }
        return Ok(());
    }

    let args = tmux_positional_args(command);
    let key_index = args
        .iter()
        .position(|arg| !matches!(arg.as_str(), "-r" | "-n"))
        .context("bind-key requires a key")?;
    let key = args[key_index].clone();
    let action = args
        .iter()
        .skip(key_index + 1)
        .cloned()
        .collect::<Vec<_>>()
        .join(" ");
    if action.trim().is_empty() {
        bail!("bind-key requires a command");
    }

    let mut rows = load_bindings();
    rows.retain(|(existing_key, _)| existing_key != &key);
    rows.push((key.clone(), action.clone()));
    rows.sort_by(|left, right| left.0.cmp(&right.0));
    write_bindings(&rows)?;
    println!("bind-key {key} {action}");
    Ok(())
}

fn run_unbind_key(command: &[String]) -> Result<()> {
    let rows = if command_has_flag(command, "-a") {
        Vec::new()
    } else {
        let args = tmux_positional_args(command);
        let key = args
            .iter()
            .find(|arg| !matches!(arg.as_str(), "-n"))
            .cloned()
            .context("unbind-key requires a key or -a")?;
        let mut rows = load_bindings();
        rows.retain(|(existing_key, _)| existing_key != &key);
        rows
    };
    write_bindings(&rows)?;
    println!("OK");
    Ok(())
}

fn run_copy_mode(command: &[String]) -> Result<()> {
    let state = if command_has_flag(command, "-q") || command_has_flag(command, "--cancel") {
        "inactive"
    } else {
        "active"
    };
    fs::write(copy_mode_path(), state)?;
    println!("copy-mode {state}");
    Ok(())
}

fn run_popup(options: &GlobalOptions) -> Result<()> {
    let socket = resolve_socket_path(options)?;
    let command = &options.command;
    let title = option_value(command, "--title")
        .or_else(|| option_value(command, "-T"))
        .unwrap_or_else(|| "tmux popup".to_string());
    let shell_command = option_value(command, "--command")
        .or_else(|| option_value(command, "-E"))
        .or_else(|| {
            let args = tmux_positional_args(command);
            if args.is_empty() {
                None
            } else {
                Some(args.join(" "))
            }
        });

    if let Some(shell_command) = shell_command.filter(|value| !value.trim().is_empty()) {
        let mut params = option_params(command, &[("--workspace", "workspace_id")])?;
        add_env_default(&mut params, "workspace_id", "CMUX_WORKSPACE_ID");
        params["kind"] = json!("terminal");
        params["title"] = json!(title);
        params["command"] = json!(shell_command);
        let response = call_socket(&socket, "pane.create", params)?;
        if options.json || command_has_flag(command, "--json") {
            println!("{}", serde_json::to_string_pretty(&response)?);
        } else {
            let surface_ref = response
                .get("surface_ref")
                .and_then(Value::as_str)
                .or_else(|| response.get("surface_id").and_then(Value::as_str))
                .unwrap_or("surface");
            println!("popup {surface_ref}");
        }
    } else {
        let body = last_positional(command).unwrap_or_else(|| title.clone());
        let response = call_socket(
            &socket,
            "notification.create",
            json!({
                "title": title,
                "body": body
            }),
        )?;
        if options.json || command_has_flag(command, "--json") {
            println!("{}", serde_json::to_string_pretty(&response)?);
        } else {
            println!("popup notification");
        }
    }
    Ok(())
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum HookInstallerAgent {
    Claude,
    Codex,
    OpenCode,
}

fn maybe_run_hooks_installer_command(command: &[String]) -> Result<bool> {
    match command.first().map(String::as_str) {
        Some("setup-hooks") => {
            let agents = hook_setup_agents(command, 1)?;
            run_hook_installers(command, &agents)?;
            Ok(true)
        }
        Some("hooks") => {
            let sub = command.get(1).map(String::as_str).unwrap_or("help");
            match sub {
                "setup" | "install" => {
                    let agents = hook_setup_agents(command, 2)?;
                    run_hook_installers(command, &agents)?;
                    Ok(true)
                }
                "uninstall" | "uninstall-hooks" => {
                    let agents = hook_setup_agents(command, 2)?;
                    run_hook_uninstallers(command, &agents)?;
                    Ok(true)
                }
                agent => {
                    let Some(action) = command.get(2).map(String::as_str) else {
                        return Ok(false);
                    };
                    if matches!(action, "install" | "install-hooks" | "setup") {
                        let agent = parse_hook_installer_agent(agent)?;
                        run_hook_installer(command, agent)?;
                        return Ok(true);
                    }
                    if matches!(action, "uninstall" | "uninstall-hooks") {
                        let agent = parse_hook_installer_agent(agent)?;
                        run_hook_uninstaller(command, agent)?;
                        return Ok(true);
                    }
                    Ok(false)
                }
            }
        }
        _ => Ok(false),
    }
}

fn hook_setup_agents(command: &[String], start: usize) -> Result<Vec<HookInstallerAgent>> {
    let mut agents = Vec::new();
    for name in hook_agent_option_values(command) {
        push_unique_hook_agent(&mut agents, parse_hook_installer_agent(&name)?);
    }
    for name in positional_args_after_skipping_options(
        command,
        start,
        &[
            "--agent",
            "--path",
            "--codex-home",
            "--hooks-path",
            "--config-path",
            "--config-dir",
        ],
    ) {
        push_unique_hook_agent(&mut agents, parse_hook_installer_agent(&name)?);
    }
    if agents.is_empty() {
        agents = hook_agents_on_path();
    }
    Ok(agents)
}

fn hook_agent_option_values(command: &[String]) -> Vec<String> {
    let mut out = Vec::new();
    let mut index = 0;
    while index < command.len() {
        match command[index].as_str() {
            "--" => break,
            "--agent" => {
                if let Some(value) = command.get(index + 1) {
                    out.push(value.clone());
                }
                index += 2;
            }
            _ => index += 1,
        }
    }
    out
}

fn parse_hook_installer_agent(value: &str) -> Result<HookInstallerAgent> {
    match normalize_hook_source(value).as_str() {
        "claude" => Ok(HookInstallerAgent::Claude),
        "codex" => Ok(HookInstallerAgent::Codex),
        "opencode" | "open_code" => Ok(HookInstallerAgent::OpenCode),
        other => bail!("unsupported hook installer agent: {other}"),
    }
}

fn push_unique_hook_agent(agents: &mut Vec<HookInstallerAgent>, agent: HookInstallerAgent) {
    if !agents.contains(&agent) {
        agents.push(agent);
    }
}

fn hook_agents_on_path() -> Vec<HookInstallerAgent> {
    let mut agents = Vec::new();
    if command_on_path("claude") {
        agents.push(HookInstallerAgent::Claude);
    }
    if command_on_path("codex") {
        agents.push(HookInstallerAgent::Codex);
    }
    if command_on_path("opencode") {
        agents.push(HookInstallerAgent::OpenCode);
    }
    agents
}

fn command_on_path(name: &str) -> bool {
    let Some(path) = std::env::var_os("PATH") else {
        return false;
    };
    std::env::split_paths(&path).any(|dir| dir.join(name).is_file())
}

fn run_hook_installers(command: &[String], agents: &[HookInstallerAgent]) -> Result<()> {
    if agents.is_empty() {
        println!("No supported hook agents found on PATH.");
        return Ok(());
    }
    for agent in agents {
        run_hook_installer(command, *agent)?;
    }
    Ok(())
}

fn run_hook_installer(command: &[String], agent: HookInstallerAgent) -> Result<()> {
    match agent {
        HookInstallerAgent::Claude => run_install_claude_code_integration(command),
        HookInstallerAgent::Codex => run_install_codex_integration(command),
        HookInstallerAgent::OpenCode => run_install_opencode_integration(command),
    }
}

fn run_hook_uninstallers(command: &[String], agents: &[HookInstallerAgent]) -> Result<()> {
    if agents.is_empty() {
        println!("No supported hook agents found on PATH.");
        return Ok(());
    }
    for agent in agents {
        run_hook_uninstaller(command, *agent)?;
    }
    Ok(())
}

fn run_hook_uninstaller(command: &[String], agent: HookInstallerAgent) -> Result<()> {
    match agent {
        HookInstallerAgent::Claude => run_uninstall_claude_code_integration(command),
        HookInstallerAgent::Codex => run_uninstall_codex_integration(command),
        HookInstallerAgent::OpenCode => run_uninstall_opencode_integration(command),
    }
}

fn run_install_claude_code_integration(command: &[String]) -> Result<()> {
    let path = claude_settings_path(command)?;
    let current_text = fs::read_to_string(&path).unwrap_or_default();
    let current_json = if current_text.trim().is_empty() {
        json!({})
    } else {
        serde_json::from_str(&current_text)
            .with_context(|| format!("{} is not valid JSON", path.display()))?
    };
    let updated_json = merged_claude_code_settings(current_json)?;
    let updated_text = serde_json::to_string_pretty(&updated_json)? + "\n";

    if normalize_json_text(&current_text) == normalize_json_text(&updated_text) {
        println!(
            "Claude Code integration is already installed in {}",
            path.display()
        );
        return Ok(());
    }

    println!("Claude Code integration config: {}", path.display());
    println!(
        "{}",
        simple_unified_diff(
            &format!("a/{}", path.display()),
            &format!("b/{}", path.display()),
            &current_text,
            &updated_text,
        )
    );

    if command_has_flag(command, "--dry-run") {
        println!("Dry run only; no files were changed.");
        return Ok(());
    }

    if !command_has_flag(command, "--yes") && !command_has_flag(command, "-y") {
        print!("Apply this change? [y/N] ");
        io::stdout().flush()?;
        let mut answer = String::new();
        io::stdin().read_line(&mut answer)?;
        if !matches!(answer.trim().to_ascii_lowercase().as_str(), "y" | "yes") {
            println!("Canceled.");
            return Ok(());
        }
    }

    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(&path, updated_text)?;
    println!("Installed Claude Code integration in {}", path.display());
    Ok(())
}

fn run_uninstall_claude_code_integration(command: &[String]) -> Result<()> {
    let path = claude_settings_path(command)?;
    let current_text = fs::read_to_string(&path).unwrap_or_default();
    let current_json = if current_text.trim().is_empty() {
        json!({})
    } else {
        serde_json::from_str(&current_text)
            .with_context(|| format!("{} is not valid JSON", path.display()))?
    };
    let updated_json = claude_code_settings_without_cmux_hooks(current_json.clone())?;
    if current_json == updated_json {
        println!(
            "Claude Code integration is already uninstalled from {}",
            path.display()
        );
        return Ok(());
    }
    let updated_text = serde_json::to_string_pretty(&updated_json)? + "\n";

    println!("Claude Code integration config: {}", path.display());
    println!(
        "{}",
        simple_unified_diff(
            &format!("a/{}", path.display()),
            &format!("b/{}", path.display()),
            &current_text,
            &updated_text,
        )
    );

    if command_has_flag(command, "--dry-run") {
        println!("Dry run only; no files were changed.");
        return Ok(());
    }
    if !confirm_hook_config_change(command, false)? {
        return Ok(());
    }

    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(&path, updated_text)?;
    println!(
        "Uninstalled Claude Code integration from {}",
        path.display()
    );
    Ok(())
}

fn claude_settings_path(command: &[String]) -> Result<PathBuf> {
    if let Some(path) = option_value(command, "--path") {
        return Ok(PathBuf::from(path));
    }
    if let Ok(path) = std::env::var("CMUX_CLAUDE_SETTINGS_PATH") {
        let trimmed = path.trim();
        if !trimmed.is_empty() {
            return Ok(PathBuf::from(trimmed));
        }
    }
    let home = std::env::var("HOME").context("HOME is required for Claude settings path")?;
    Ok(Path::new(&home).join(".claude/settings.json"))
}

fn merged_claude_code_settings(mut root: Value) -> Result<Value> {
    if !root.is_object() {
        bail!("Claude settings root must be a JSON object");
    }
    if root.get("hooks").is_none() {
        root["hooks"] = json!({});
    }
    if !root["hooks"].is_object() {
        bail!("Claude settings hooks field must be a JSON object");
    }

    for (event, command) in claude_code_hook_commands() {
        if root["hooks"].get(event).is_none() {
            root["hooks"][event] = json!([]);
        }
        let entries = root["hooks"][event]
            .as_array_mut()
            .with_context(|| format!("Claude settings hooks.{event} must be an array"))?;
        if !entries
            .iter()
            .any(|entry| hook_entry_runs_command(entry, command))
        {
            entries.push(json!({
                "matcher": "",
                "hooks": [{
                    "type": "command",
                    "command": command
                }]
            }));
        }
    }
    Ok(root)
}

fn claude_code_settings_without_cmux_hooks(mut root: Value) -> Result<Value> {
    if !root.is_object() {
        bail!("Claude settings root must be a JSON object");
    }
    let remove_hooks_field = if let Some(hooks) = root.get_mut("hooks") {
        let hooks = hooks
            .as_object_mut()
            .context("Claude settings hooks field must be a JSON object")?;
        for (event, command) in claude_code_hook_commands()
            .into_iter()
            .chain(claude_code_legacy_hook_commands())
        {
            remove_command_hook_from_event(hooks, event, command)?;
        }
        hooks.is_empty()
    } else {
        false
    };
    if remove_hooks_field {
        root.as_object_mut().unwrap().remove("hooks");
    }
    Ok(root)
}

fn claude_code_hook_commands() -> [(&'static str, &'static str); 3] {
    [
        (
            "Notification",
            "cmux notify --title 'Claude Code' --subtitle 'Input needed' --body 'Claude Code needs your attention'",
        ),
        (
            "UserPromptSubmit",
            "cmux hooks claude UserPromptSubmit; cmux set-agent-lifecycle claude_code running",
        ),
        ("Stop", "cmux set-agent-lifecycle claude_code idle"),
    ]
}

fn claude_code_legacy_hook_commands() -> [(&'static str, &'static str); 1] {
    [(
        "UserPromptSubmit",
        "cmux set-agent-lifecycle claude_code running",
    )]
}

fn hook_entry_runs_command(entry: &Value, command: &str) -> bool {
    entry
        .get("hooks")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .any(|hook| {
            hook.get("type").and_then(Value::as_str) == Some("command")
                && hook.get("command").and_then(Value::as_str) == Some(command)
        })
}

fn remove_command_hook_from_event(
    hooks: &mut Map<String, Value>,
    event: &str,
    command: &str,
) -> Result<()> {
    let remove_event = if let Some(value) = hooks.get_mut(event) {
        let entries = value
            .as_array_mut()
            .with_context(|| format!("hook event {event} must be an array"))?;
        let old_entries = std::mem::take(entries);
        let mut new_entries = Vec::new();
        for mut entry in old_entries {
            if remove_command_hook_from_entry(&mut entry, command) {
                new_entries.push(entry);
            }
        }
        *entries = new_entries;
        entries.is_empty()
    } else {
        false
    };
    if remove_event {
        hooks.remove(event);
    }
    Ok(())
}

fn remove_command_hook_from_entry(entry: &mut Value, command: &str) -> bool {
    let Some(object) = entry.as_object_mut() else {
        return true;
    };
    let Some(hooks) = object.get_mut("hooks").and_then(Value::as_array_mut) else {
        return true;
    };
    hooks.retain(|hook| {
        !(hook.get("type").and_then(Value::as_str) == Some("command")
            && hook.get("command").and_then(Value::as_str) == Some(command))
    });
    !hooks.is_empty()
}

fn confirm_hook_config_change(command: &[String], plural: bool) -> Result<bool> {
    if command_has_flag(command, "--yes") || command_has_flag(command, "-y") {
        return Ok(true);
    }
    if plural {
        print!("Apply these changes? [y/N] ");
    } else {
        print!("Apply this change? [y/N] ");
    }
    io::stdout().flush()?;
    let mut answer = String::new();
    io::stdin().read_line(&mut answer)?;
    if matches!(answer.trim().to_ascii_lowercase().as_str(), "y" | "yes") {
        Ok(true)
    } else {
        println!("Canceled.");
        Ok(false)
    }
}

fn normalize_json_text(text: &str) -> String {
    serde_json::from_str::<Value>(text)
        .ok()
        .and_then(|value| serde_json::to_string_pretty(&value).ok())
        .unwrap_or_else(|| text.trim_end().to_string())
}

fn simple_unified_diff(old_label: &str, new_label: &str, old_text: &str, new_text: &str) -> String {
    let mut out = String::new();
    out.push_str(&format!("--- {old_label}\n+++ {new_label}\n@@\n"));
    if !old_text.trim().is_empty() {
        for line in old_text.lines() {
            out.push('-');
            out.push_str(line);
            out.push('\n');
        }
    }
    for line in new_text.lines() {
        out.push('+');
        out.push_str(line);
        out.push('\n');
    }
    out
}

fn run_install_codex_integration(command: &[String]) -> Result<()> {
    let codex_home = codex_home_path(command)?;
    let hooks_path = option_value(command, "--hooks-path")
        .map(PathBuf::from)
        .unwrap_or_else(|| codex_home.join("hooks.json"));
    let config_path = option_value(command, "--config-path")
        .map(PathBuf::from)
        .unwrap_or_else(|| codex_home.join("config.toml"));

    let current_hooks_text = fs::read_to_string(&hooks_path).unwrap_or_default();
    let current_hooks = if current_hooks_text.trim().is_empty() {
        json!({})
    } else {
        serde_json::from_str(&current_hooks_text)
            .with_context(|| format!("{} is not valid JSON", hooks_path.display()))?
    };
    let updated_hooks = merged_codex_hooks(current_hooks)?;
    let updated_hooks_text = serde_json::to_string_pretty(&updated_hooks)? + "\n";

    let current_config_text = fs::read_to_string(&config_path).unwrap_or_default();
    let updated_config_text = codex_config_with_notify(&current_config_text);
    let hooks_changed =
        normalize_json_text(&current_hooks_text) != normalize_json_text(&updated_hooks_text);
    let config_changed = current_config_text != updated_config_text;

    if !hooks_changed && !config_changed {
        println!(
            "Codex integration is already installed in {}",
            codex_home.display()
        );
        return Ok(());
    }

    if hooks_changed {
        println!("Codex hooks config: {}", hooks_path.display());
        println!(
            "{}",
            simple_unified_diff(
                &format!("a/{}", hooks_path.display()),
                &format!("b/{}", hooks_path.display()),
                &current_hooks_text,
                &updated_hooks_text,
            )
        );
    }
    if config_changed {
        println!("Codex notification config: {}", config_path.display());
        println!(
            "{}",
            simple_unified_diff(
                &format!("a/{}", config_path.display()),
                &format!("b/{}", config_path.display()),
                &current_config_text,
                &updated_config_text,
            )
        );
    }

    if command_has_flag(command, "--dry-run") {
        println!("Dry run only; no files were changed.");
        return Ok(());
    }

    if !command_has_flag(command, "--yes") && !command_has_flag(command, "-y") {
        print!("Apply these changes? [y/N] ");
        io::stdout().flush()?;
        let mut answer = String::new();
        io::stdin().read_line(&mut answer)?;
        if !matches!(answer.trim().to_ascii_lowercase().as_str(), "y" | "yes") {
            println!("Canceled.");
            return Ok(());
        }
    }

    if hooks_changed {
        if let Some(parent) = hooks_path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(&hooks_path, updated_hooks_text)?;
    }
    if config_changed {
        if let Some(parent) = config_path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(&config_path, updated_config_text)?;
    }
    println!("Installed Codex integration in {}", codex_home.display());
    Ok(())
}

fn run_uninstall_codex_integration(command: &[String]) -> Result<()> {
    let codex_home = codex_home_path(command)?;
    let hooks_path = option_value(command, "--hooks-path")
        .map(PathBuf::from)
        .unwrap_or_else(|| codex_home.join("hooks.json"));
    let config_path = option_value(command, "--config-path")
        .map(PathBuf::from)
        .unwrap_or_else(|| codex_home.join("config.toml"));

    let current_hooks_text = fs::read_to_string(&hooks_path).unwrap_or_default();
    let current_hooks = if current_hooks_text.trim().is_empty() {
        json!({})
    } else {
        serde_json::from_str(&current_hooks_text)
            .with_context(|| format!("{} is not valid JSON", hooks_path.display()))?
    };
    let updated_hooks = codex_hooks_without_cmux_commands(current_hooks.clone())?;
    let hooks_changed = current_hooks != updated_hooks;
    let updated_hooks_text = serde_json::to_string_pretty(&updated_hooks)? + "\n";

    let current_config_text = fs::read_to_string(&config_path).unwrap_or_default();
    let updated_config_text = codex_config_without_notify(&current_config_text);
    let config_changed = current_config_text != updated_config_text;

    if !hooks_changed && !config_changed {
        println!(
            "Codex integration is already uninstalled from {}",
            codex_home.display()
        );
        return Ok(());
    }

    if hooks_changed {
        println!("Codex hooks config: {}", hooks_path.display());
        println!(
            "{}",
            simple_unified_diff(
                &format!("a/{}", hooks_path.display()),
                &format!("b/{}", hooks_path.display()),
                &current_hooks_text,
                &updated_hooks_text,
            )
        );
    }
    if config_changed {
        println!("Codex notification config: {}", config_path.display());
        println!(
            "{}",
            simple_unified_diff(
                &format!("a/{}", config_path.display()),
                &format!("b/{}", config_path.display()),
                &current_config_text,
                &updated_config_text,
            )
        );
    }

    if command_has_flag(command, "--dry-run") {
        println!("Dry run only; no files were changed.");
        return Ok(());
    }
    if !confirm_hook_config_change(command, hooks_changed && config_changed)? {
        return Ok(());
    }

    if hooks_changed {
        if let Some(parent) = hooks_path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(&hooks_path, updated_hooks_text)?;
    }
    if config_changed {
        if let Some(parent) = config_path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(&config_path, updated_config_text)?;
    }
    println!(
        "Uninstalled Codex integration from {}",
        codex_home.display()
    );
    Ok(())
}

fn codex_home_path(command: &[String]) -> Result<PathBuf> {
    if let Some(path) = option_value(command, "--codex-home") {
        return Ok(PathBuf::from(path));
    }
    if let Ok(path) = std::env::var("CODEX_HOME") {
        let trimmed = path.trim();
        if !trimmed.is_empty() {
            return Ok(PathBuf::from(trimmed));
        }
    }
    let home = std::env::var("HOME").context("HOME is required for Codex home path")?;
    Ok(Path::new(&home).join(".codex"))
}

fn merged_codex_hooks(mut root: Value) -> Result<Value> {
    if !root.is_object() {
        bail!("Codex hooks root must be a JSON object");
    }
    if root.get("hooks").is_none() {
        root["hooks"] = json!({});
    }
    if !root["hooks"].is_object() {
        bail!("Codex hooks field must be a JSON object");
    }
    for (event, matcher, command, status_message) in codex_hook_commands() {
        if root["hooks"].get(event).is_none() {
            root["hooks"][event] = json!([]);
        }
        let entries = root["hooks"][event]
            .as_array_mut()
            .with_context(|| format!("Codex hooks.{event} must be an array"))?;
        if !entries
            .iter()
            .any(|entry| hook_entry_runs_command(entry, command))
        {
            let mut entry = json!({
                "hooks": [{
                    "type": "command",
                    "command": command,
                    "statusMessage": status_message
                }]
            });
            if !matcher.is_empty() {
                entry["matcher"] = json!(matcher);
            }
            entries.push(entry);
        }
    }
    Ok(root)
}

fn codex_hooks_without_cmux_commands(mut root: Value) -> Result<Value> {
    if !root.is_object() {
        bail!("Codex hooks root must be a JSON object");
    }
    let remove_hooks_field = if let Some(hooks) = root.get_mut("hooks") {
        let hooks = hooks
            .as_object_mut()
            .context("Codex hooks field must be a JSON object")?;
        for (event, _matcher, command, _status_message) in codex_hook_commands()
            .into_iter()
            .chain(codex_legacy_hook_commands())
        {
            remove_command_hook_from_event(hooks, event, command)?;
        }
        hooks.is_empty()
    } else {
        false
    };
    if remove_hooks_field {
        root.as_object_mut().unwrap().remove("hooks");
    }
    Ok(root)
}

fn codex_hook_commands() -> [(&'static str, &'static str, &'static str, &'static str); 4] {
    [
        (
            "UserPromptSubmit",
            "",
            "cmux hooks codex UserPromptSubmit; cmux set-agent-lifecycle codex running",
            "cmux: Codex running",
        ),
        (
            "PreToolUse",
            "",
            "cmux hooks codex PreToolUse; cmux set-agent-lifecycle codex running",
            "cmux: Codex using a tool",
        ),
        (
            "PermissionRequest",
            "",
            "cmux notify --title Codex --subtitle Permission --body 'Codex requested approval'",
            "cmux: Codex approval notification",
        ),
        (
            "Stop",
            "",
            "cmux set-agent-lifecycle codex idle",
            "cmux: Codex idle",
        ),
    ]
}

fn codex_legacy_hook_commands() -> [(&'static str, &'static str, &'static str, &'static str); 2] {
    [
        (
            "UserPromptSubmit",
            "",
            "cmux set-agent-lifecycle codex running",
            "cmux: Codex running",
        ),
        (
            "PreToolUse",
            "",
            "cmux set-agent-lifecycle codex running",
            "cmux: Codex using a tool",
        ),
    ]
}

const CODEX_NOTIFY_MARKER: &str = "# cmux Codex integration";
const CODEX_NOTIFY_LINE: &str = "notify = [\"bash\", \"-lc\", \"command -v cmux >/dev/null 2>&1 && cmux notify --title Codex --body 'Turn complete'\", \"--\"]";

fn codex_config_with_notify(current: &str) -> String {
    if current
        .lines()
        .any(|line| line.trim_start().starts_with("notify ="))
    {
        return current.to_string();
    }
    let mut out = current.trim_end().to_string();
    if !out.is_empty() {
        out.push_str("\n\n");
    }
    out.push_str(CODEX_NOTIFY_MARKER);
    out.push('\n');
    out.push_str(CODEX_NOTIFY_LINE);
    out.push('\n');
    out
}

fn codex_config_without_notify(current: &str) -> String {
    let lines = current.lines().collect::<Vec<_>>();
    let mut out = Vec::new();
    let mut index = 0;
    while index < lines.len() {
        let line = lines[index];
        let next = lines.get(index + 1).copied();
        if line.trim() == CODEX_NOTIFY_MARKER
            && next.is_some_and(|value| value.trim() == CODEX_NOTIFY_LINE)
        {
            index += 2;
            continue;
        }
        if line.trim() == CODEX_NOTIFY_LINE {
            index += 1;
            continue;
        }
        out.push(line);
        index += 1;
    }
    let mut text = out.join("\n");
    if !text.is_empty() && current.ends_with('\n') {
        text.push('\n');
    }
    text
}

const OPENCODE_SESSION_PLUGIN_MARKER: &str = "cmux-opencode-session-plugin-marker";
const OPENCODE_SESSION_PLUGIN_FILENAME: &str = "cmux-session.js";
const OPENCODE_SESSION_PLUGIN_CONFIG_SPEC: &str = "./plugins/cmux-session.js";
const OPENCODE_FEED_PLUGIN_MARKER: &str = "cmux-feed-plugin-marker";
const OPENCODE_FEED_PLUGIN_FILENAME: &str = "cmux-feed.js";
const OPENCODE_FEED_PLUGIN_SOURCE: &str = include_str!("../../Resources/opencode-plugin.js");
const OPENCODE_SESSION_PLUGIN_SOURCE: &str = r#"// cmux-opencode-session-plugin-marker v1
// Bridges OpenCode session lifecycle events into cmux's restorable session store.
// Installed by `cmux hooks opencode install` or `cmux hooks setup`.
// DO NOT EDIT MANUALLY. cmux upgrades this file in place.

import { spawnSync } from "node:child_process";
import * as fs from "node:fs";
import * as path from "node:path";

const CMUX_PLUGIN_INSTALLED_KEY = Symbol.for("cmux.session.restore.plugin.installed");
const messageRoles = new Map();
const sessions = new Map();

function firstString(...values) {
  for (const value of values) {
    if (typeof value === "string" && value.trim().length > 0) return value.trim();
  }
  return null;
}

function eventProperties(event) {
  return (event && typeof event === "object" && event.properties) || {};
}

function normalizeText(value, max = 1000) {
  if (typeof value !== "string") return null;
  const normalized = value.replace(/\s+/g, " ").trim();
  if (!normalized) return null;
  return normalized.length > max ? `${normalized.slice(0, max - 3)}...` : normalized;
}

function sessionState(sessionId) {
  const key = sessionId || "unknown";
  if (!sessions.has(key)) {
    sessions.set(key, {
      lastUserMessage: null,
      assistantPreamble: null,
      cwd: null,
    });
  }
  return sessions.get(key);
}

function contextForSession(sessionId) {
  const state = sessionState(sessionId);
  const context = {};
  if (state.lastUserMessage) context.lastUserMessage = state.lastUserMessage;
  if (state.assistantPreamble) context.assistantPreamble = state.assistantPreamble;
  return Object.keys(context).length > 0 ? context : undefined;
}

function sessionIdFor(event) {
  const props = eventProperties(event);
  return firstString(
    props.info && props.info.id,
    props.sessionID,
    props.sessionId,
    props.session_id,
    props.session && props.session.id,
    event && event.sessionID,
    event && event.sessionId,
    event && event.id
  );
}

function cwdFor(ctx, event) {
  const props = eventProperties(event);
  return firstString(
    props.info && props.info.directory,
    props.cwd,
    props.directory,
    ctx && ctx.directory,
    process.cwd()
  );
}

function resolveExecutable(name) {
  const pathEnv = process.env.PATH || "";
  for (const dir of pathEnv.split(path.delimiter)) {
    if (!dir) continue;
    const candidate = path.join(dir, name);
    try {
      fs.accessSync(candidate, fs.constants.X_OK);
      return candidate;
    } catch (_) {}
  }
  return name;
}

function looksLikeOpenCodeScript(value) {
  if (!value) return false;
  const lower = String(value).toLowerCase();
  return lower.includes("opencode") || lower.includes("open-code");
}

function isOpenCodeInternalWorkerArg(value) {
  if (!value) return false;
  const normalized = String(value).replaceAll("\\", "/");
  return normalized.includes("/$bunfs/") && normalized.includes("/src/cli/cmd/tui/worker.js");
}

function withoutOpenCodeInternalWorkerArgs(argv) {
  const result = [];
  for (let i = 0; i < argv.length; i += 1) {
    const value = argv[i];
    if (i > 0 && isOpenCodeInternalWorkerArg(value)) continue;
    result.push(value);
  }
  return result.length > 0 ? result : [resolveExecutable("opencode")];
}

function normalizedLaunchArgv() {
  const raw = Array.isArray(process.argv) ? process.argv.map((value) => String(value)) : [];
  if (raw.length === 0) return [resolveExecutable("opencode")];

  const firstBase = path.basename(raw[0]).toLowerCase();
  if (looksLikeOpenCodeScript(firstBase)) return withoutOpenCodeInternalWorkerArgs(raw);

  let tail = raw.slice(1);
  if (tail.length > 0 && looksLikeOpenCodeScript(tail[0])) {
    tail = tail.slice(1);
  }
  return withoutOpenCodeInternalWorkerArgs([resolveExecutable("opencode"), ...tail]);
}

function base64NulSeparated(values) {
  const bytes = [];
  for (const value of values) {
    bytes.push(Buffer.from(String(value), "utf8"));
    bytes.push(Buffer.from([0]));
  }
  return Buffer.concat(bytes).toString("base64");
}

function hookEnvironment(cwd) {
  const env = { ...process.env };
  delete env.AMP_API_KEY;
  if (!env.CMUX_AGENT_LAUNCH_ARGV_B64) {
    const argv = normalizedLaunchArgv();
    env.CMUX_AGENT_LAUNCH_KIND = "opencode";
    env.CMUX_AGENT_LAUNCH_EXECUTABLE = argv[0] || resolveExecutable("opencode");
    env.CMUX_AGENT_LAUNCH_ARGV_B64 = base64NulSeparated(argv);
    env.CMUX_AGENT_LAUNCH_CWD = cwd || process.cwd();
  }
  return env;
}

function hasCmuxSurfaceContext() {
  return Boolean(process.env.CMUX_SURFACE_ID || process.env.CMUX_PANEL_ID);
}

function sendHook(subcommand, ctx, event, extra = {}) {
  if (process.env.CMUX_OPENCODE_HOOKS_DISABLED === "1") return;
  if (!hasCmuxSurfaceContext()) return;

  const sessionId = sessionIdFor(event);
  if (!sessionId) return;

  const cwd = cwdFor(ctx, event);
  const state = sessionState(sessionId);
  state.cwd = cwd || state.cwd;
  const payload = {
    session_id: sessionId,
    cwd,
    event: event && event.type,
    hook_event_name: event && event.type,
    ...extra,
  };
  const context = extra.context || contextForSession(sessionId);
  if (context) payload.context = context;
  const cmux = process.env.CMUX_OPENCODE_CMUX_BIN || "cmux";
  try {
    spawnSync(cmux, ["hooks", "opencode", subcommand], {
      input: JSON.stringify(payload),
      encoding: "utf8",
      env: hookEnvironment(cwd),
      stdio: ["pipe", "ignore", "ignore"],
      timeout: 5000,
    });
  } catch (_) {}
}

function trackMessage(event) {
  const props = eventProperties(event);
  if (event && event.type === "message.updated") {
    const info = props.info || props.message || {};
    const messageId = info.id || props.messageID;
    const sessionId = info.sessionID || props.sessionID;
    const role = info.role || props.role;
    if (messageId && sessionId && role) {
      messageRoles.set(messageId, { sessionId, role });
      if (messageRoles.size > 300) {
        messageRoles.delete(messageRoles.keys().next().value);
      }
    }
    return;
  }

  if (!event || event.type !== "message.part.updated") return;
  const part = props.part || {};
  if (part.type !== "text" || !part.messageID) return;
  const meta = messageRoles.get(part.messageID);
  if (!meta) return;
  const text = normalizeText(part.text || part.textDelta || part.content);
  if (!text) return;
  const state = sessionState(meta.sessionId);
  if (meta.role === "user") {
    state.lastUserMessage = text;
  } else if (meta.role === "assistant") {
    state.assistantPreamble = text;
  }
}

const CMUXSessionRestore = async (ctx) => {
  if (globalThis[CMUX_PLUGIN_INSTALLED_KEY]) return {};
  globalThis[CMUX_PLUGIN_INSTALLED_KEY] = true;
  return {
    event: async ({ event }) => {
      trackMessage(event);
      const props = eventProperties(event);
      switch (event && event.type) {
        case "session.created":
          sendHook("session-start", ctx, event);
          break;
        case "session.updated":
          if (props.info && props.info.time && props.info.time.archived) {
            sendHook("session-end", ctx, event);
            sessions.delete(sessionIdFor(event));
          } else {
            sendHook("session-start", ctx, event);
          }
          break;
        case "session.status":
          if (props.status && props.status.type === "idle") {
            sendHook("stop", ctx, event);
          }
          break;
        case "session.idle":
          sendHook("stop", ctx, event);
          break;
        case "session.deleted":
          sendHook("session-end", ctx, event);
          sessions.delete(sessionIdFor(event));
          break;
        default:
          break;
      }
    },
  };
};

export { CMUXSessionRestore };
export default CMUXSessionRestore;
"#;

fn run_install_opencode_integration(command: &[String]) -> Result<()> {
    let config_dir = opencode_config_dir_path(command)?;
    let config_path = config_dir.join("opencode.json");
    let plugin_dir = config_dir.join("plugins");
    let session_plugin_path = plugin_dir.join(OPENCODE_SESSION_PLUGIN_FILENAME);
    let feed_plugin_path = plugin_dir.join(OPENCODE_FEED_PLUGIN_FILENAME);

    let current_config_text = fs::read_to_string(&config_path).unwrap_or_default();
    let current_config = if current_config_text.trim().is_empty() {
        json!({})
    } else {
        serde_json::from_str(&current_config_text)
            .with_context(|| format!("{} is not valid JSON", config_path.display()))?
    };
    let updated_config = merged_opencode_config(current_config)?;
    let updated_config_text = serde_json::to_string_pretty(&updated_config)? + "\n";
    let config_changed =
        normalize_json_text(&current_config_text) != normalize_json_text(&updated_config_text);

    let current_session_plugin = fs::read_to_string(&session_plugin_path).unwrap_or_default();
    if !current_session_plugin.is_empty()
        && !current_session_plugin.contains(OPENCODE_SESSION_PLUGIN_MARKER)
    {
        bail!(
            "{} exists and is not a cmux OpenCode session plugin; leaving it alone",
            session_plugin_path.display()
        );
    }
    let session_plugin_changed = current_session_plugin != OPENCODE_SESSION_PLUGIN_SOURCE;

    let current_feed_plugin = fs::read_to_string(&feed_plugin_path).unwrap_or_default();
    if !current_feed_plugin.is_empty() && !current_feed_plugin.contains(OPENCODE_FEED_PLUGIN_MARKER)
    {
        bail!(
            "{} exists and is not a cmux OpenCode feed plugin; leaving it alone",
            feed_plugin_path.display()
        );
    }
    let feed_plugin_changed = current_feed_plugin != OPENCODE_FEED_PLUGIN_SOURCE;

    if !config_changed && !session_plugin_changed && !feed_plugin_changed {
        println!(
            "OpenCode integration is already installed in {}",
            config_dir.display()
        );
        return Ok(());
    }

    if config_changed {
        println!("OpenCode config: {}", config_path.display());
        println!(
            "{}",
            simple_unified_diff(
                &format!("a/{}", config_path.display()),
                &format!("b/{}", config_path.display()),
                &current_config_text,
                &updated_config_text,
            )
        );
    }
    if session_plugin_changed {
        println!("OpenCode session plugin: {}", session_plugin_path.display());
        println!(
            "{}",
            simple_unified_diff(
                &format!("a/{}", session_plugin_path.display()),
                &format!("b/{}", session_plugin_path.display()),
                &current_session_plugin,
                OPENCODE_SESSION_PLUGIN_SOURCE,
            )
        );
    }
    if feed_plugin_changed {
        println!("OpenCode feed plugin: {}", feed_plugin_path.display());
        println!(
            "{}",
            simple_unified_diff(
                &format!("a/{}", feed_plugin_path.display()),
                &format!("b/{}", feed_plugin_path.display()),
                &current_feed_plugin,
                OPENCODE_FEED_PLUGIN_SOURCE,
            )
        );
    }

    if command_has_flag(command, "--dry-run") {
        println!("Dry run only; no files were changed.");
        return Ok(());
    }

    if !command_has_flag(command, "--yes") && !command_has_flag(command, "-y") {
        print!("Apply these changes? [y/N] ");
        io::stdout().flush()?;
        let mut answer = String::new();
        io::stdin().read_line(&mut answer)?;
        if !matches!(answer.trim().to_ascii_lowercase().as_str(), "y" | "yes") {
            println!("Canceled.");
            return Ok(());
        }
    }

    if config_changed {
        fs::create_dir_all(&config_dir)?;
        fs::write(&config_path, updated_config_text)?;
    }
    if session_plugin_changed {
        fs::create_dir_all(&plugin_dir)?;
        fs::write(&session_plugin_path, OPENCODE_SESSION_PLUGIN_SOURCE)?;
    }
    if feed_plugin_changed {
        fs::create_dir_all(&plugin_dir)?;
        fs::write(&feed_plugin_path, OPENCODE_FEED_PLUGIN_SOURCE)?;
    }
    println!("Installed OpenCode integration in {}", config_dir.display());
    Ok(())
}

fn run_uninstall_opencode_integration(command: &[String]) -> Result<()> {
    let config_dir = opencode_config_dir_path(command)?;
    let config_path = config_dir.join("opencode.json");
    let plugin_dir = config_dir.join("plugins");
    let session_plugin_path = plugin_dir.join(OPENCODE_SESSION_PLUGIN_FILENAME);
    let feed_plugin_path = plugin_dir.join(OPENCODE_FEED_PLUGIN_FILENAME);

    let current_session_plugin = fs::read_to_string(&session_plugin_path).unwrap_or_default();
    let session_plugin_owned = current_session_plugin.contains(OPENCODE_SESSION_PLUGIN_MARKER);
    let session_plugin_foreign = !current_session_plugin.is_empty() && !session_plugin_owned;

    let current_feed_plugin = fs::read_to_string(&feed_plugin_path).unwrap_or_default();
    let feed_plugin_owned = current_feed_plugin.contains(OPENCODE_FEED_PLUGIN_MARKER);
    let feed_plugin_foreign = !current_feed_plugin.is_empty() && !feed_plugin_owned;

    let current_config_text = fs::read_to_string(&config_path).unwrap_or_default();
    let current_config = if current_config_text.trim().is_empty() {
        json!({})
    } else {
        serde_json::from_str(&current_config_text)
            .with_context(|| format!("{} is not valid JSON", config_path.display()))?
    };
    let updated_config =
        opencode_config_without_session_plugin(current_config.clone(), !session_plugin_foreign)?;
    let config_changed = current_config != updated_config;
    let updated_config_text = serde_json::to_string_pretty(&updated_config)? + "\n";
    let session_plugin_changed = session_plugin_owned;
    let feed_plugin_changed = feed_plugin_owned;

    if session_plugin_foreign {
        println!(
            "Refusing to remove {}; it is not a cmux OpenCode session plugin.",
            session_plugin_path.display()
        );
    }
    if feed_plugin_foreign {
        println!(
            "Refusing to remove {}; it is not a cmux OpenCode feed plugin.",
            feed_plugin_path.display()
        );
    }

    if !config_changed && !session_plugin_changed && !feed_plugin_changed {
        println!(
            "OpenCode integration is already uninstalled from {}",
            config_dir.display()
        );
        return Ok(());
    }

    if config_changed {
        println!("OpenCode config: {}", config_path.display());
        println!(
            "{}",
            simple_unified_diff(
                &format!("a/{}", config_path.display()),
                &format!("b/{}", config_path.display()),
                &current_config_text,
                &updated_config_text,
            )
        );
    }
    if session_plugin_changed {
        println!("OpenCode session plugin: {}", session_plugin_path.display());
        println!(
            "{}",
            simple_unified_diff(
                &format!("a/{}", session_plugin_path.display()),
                &format!("b/{}", session_plugin_path.display()),
                &current_session_plugin,
                "",
            )
        );
    }
    if feed_plugin_changed {
        println!("OpenCode feed plugin: {}", feed_plugin_path.display());
        println!(
            "{}",
            simple_unified_diff(
                &format!("a/{}", feed_plugin_path.display()),
                &format!("b/{}", feed_plugin_path.display()),
                &current_feed_plugin,
                "",
            )
        );
    }

    if command_has_flag(command, "--dry-run") {
        println!("Dry run only; no files were changed.");
        return Ok(());
    }
    let multiple_changes = [config_changed, session_plugin_changed, feed_plugin_changed]
        .into_iter()
        .filter(|changed| *changed)
        .count()
        > 1;
    if !confirm_hook_config_change(command, multiple_changes)? {
        return Ok(());
    }

    if config_changed {
        fs::create_dir_all(&config_dir)?;
        fs::write(&config_path, updated_config_text)?;
    }
    if session_plugin_changed {
        fs::remove_file(&session_plugin_path).with_context(|| {
            format!(
                "failed to remove OpenCode session plugin {}",
                session_plugin_path.display()
            )
        })?;
    }
    if feed_plugin_changed {
        fs::remove_file(&feed_plugin_path).with_context(|| {
            format!(
                "failed to remove OpenCode feed plugin {}",
                feed_plugin_path.display()
            )
        })?;
    }
    println!(
        "Uninstalled OpenCode integration from {}",
        config_dir.display()
    );
    Ok(())
}

fn opencode_config_dir_path(command: &[String]) -> Result<PathBuf> {
    if let Some(path) = option_value(command, "--config-dir") {
        return Ok(PathBuf::from(path));
    }
    if let Ok(path) = std::env::var("OPENCODE_CONFIG_DIR") {
        let trimmed = path.trim();
        if !trimmed.is_empty() {
            return Ok(PathBuf::from(trimmed));
        }
    }
    let home = std::env::var("HOME").context("HOME is required for OpenCode config path")?;
    Ok(Path::new(&home).join(".config/opencode"))
}

fn merged_opencode_config(mut root: Value) -> Result<Value> {
    if !root.is_object() {
        bail!("OpenCode config root must be a JSON object");
    }
    if root.get("plugin").is_none() {
        root["plugin"] = json!([]);
    }
    let plugins = root["plugin"]
        .as_array_mut()
        .context("OpenCode config plugin field must be an array")?;
    plugins.retain(|entry| !opencode_plugin_entry_is_session_plugin(entry));
    if !plugins
        .iter()
        .any(|entry| opencode_plugin_entry_spec(entry) == Some(OPENCODE_SESSION_PLUGIN_CONFIG_SPEC))
    {
        plugins.push(json!(OPENCODE_SESSION_PLUGIN_CONFIG_SPEC));
    }
    Ok(root)
}

fn opencode_config_without_session_plugin(
    mut root: Value,
    remove_local_plugin_entry: bool,
) -> Result<Value> {
    if !root.is_object() {
        bail!("OpenCode config root must be a JSON object");
    }
    let remove_plugin_field = if let Some(plugin) = root.get_mut("plugin") {
        let plugins = plugin
            .as_array_mut()
            .context("OpenCode config plugin field must be an array")?;
        plugins.retain(|entry| {
            let spec = opencode_plugin_entry_spec(entry);
            if spec == Some(OPENCODE_SESSION_PLUGIN_CONFIG_SPEC) && !remove_local_plugin_entry {
                return true;
            }
            !opencode_plugin_entry_is_session_plugin(entry)
        });
        plugins.is_empty()
    } else {
        false
    };
    if remove_plugin_field {
        root.as_object_mut().unwrap().remove("plugin");
    }
    Ok(root)
}

fn opencode_plugin_entry_spec(entry: &Value) -> Option<&str> {
    entry
        .as_str()
        .or_else(|| entry.as_array()?.first()?.as_str())
}

fn opencode_plugin_entry_is_session_plugin(entry: &Value) -> bool {
    let Some(value) = opencode_plugin_entry_spec(entry) else {
        return false;
    };
    value == OPENCODE_SESSION_PLUGIN_CONFIG_SPEC
        || value == "cmux-session"
        || value.ends_with(&format!("/plugins/{OPENCODE_SESSION_PLUGIN_FILENAME}"))
        || value.ends_with(&format!("/{OPENCODE_SESSION_PLUGIN_FILENAME}"))
}

fn run_wait_for(command: &[String]) -> Result<()> {
    let name = positional_args(command)
        .into_iter()
        .find(|arg| arg != "-S")
        .context("wait-for requires a name")?;
    let path = wait_path(&name);
    if command_has_flag(command, "-S") {
        fs::write(path, "1")?;
        println!("OK");
        return Ok(());
    }

    let timeout = option_value(command, "--timeout")
        .and_then(|value| value.parse::<f64>().ok())
        .unwrap_or(1.0)
        .max(0.0);
    let deadline = Instant::now() + Duration::from_secs_f64(timeout);
    while Instant::now() < deadline {
        if path.exists() {
            let _ = fs::remove_file(path);
            println!("OK");
            return Ok(());
        }
        thread::sleep(Duration::from_millis(25));
    }
    bail!("wait-for timed out")
}

fn run_tmux_compat(options: &GlobalOptions) -> Result<()> {
    let args = options.command.iter().skip(1).cloned().collect::<Vec<_>>();
    if args
        .iter()
        .any(|arg| matches!(arg.as_str(), "-V" | "--version"))
    {
        println!("tmux 3.4-cmux");
        return Ok(());
    }

    let command = args.first().map(String::as_str).unwrap_or("help");
    match command {
        "list-panes" | "listp" => {
            let socket = resolve_socket_path(options)?;
            let response = call_socket(&socket, "pane.list", json!({}))?;
            let format = option_value(&args, "-F").unwrap_or_else(|| "#{pane_id}".to_string());
            for pane in response
                .get("panes")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
            {
                println!("{}", render_tmux_format(&format, pane, &response));
            }
            Ok(())
        }
        "display" | "display-message" => {
            let socket = resolve_socket_path(options)?;
            let response = call_socket(&socket, "pane.list", json!({}))?;
            let format = option_value(&args, "-p")
                .or_else(|| last_positional(&args))
                .unwrap_or_default();
            let pane = response
                .get("panes")
                .and_then(Value::as_array)
                .and_then(|panes| {
                    panes
                        .iter()
                        .find(|pane| pane.get("focused").and_then(Value::as_bool) == Some(true))
                        .or_else(|| panes.first())
                })
                .context("no panes")?;
            println!("{}", render_tmux_format(&format, pane, &response));
            Ok(())
        }
        "split-window" | "splitw" => run_tmux_compat_split_window(options, &args),
        "new-window" | "neww" | "new-session" | "new" => run_tmux_compat_new_window(options, &args),
        "select-pane" | "selectp" => run_tmux_compat_select_pane(options, &args),
        "capture-pane" | "capturep" => run_tmux_compat_capture_pane(options, &args),
        "resize-pane" | "resizep" => run_tmux_compat_resize_pane(options, &args),
        "pipe-pane" | "pipep" => run_tmux_compat_pipe_pane(options, &args),
        "clear-history" | "clearhist" => run_tmux_compat_clear_history(options, &args),
        "paste-buffer" | "pasteb" => run_tmux_compat_paste_buffer(options, &args),
        "respawn-pane" | "respawnp" => run_tmux_compat_respawn_pane(options, &args),
        "last-pane" | "lastp" => {
            let socket = resolve_socket_path(options)?;
            let _ = call_socket(&socket, "pane.last", json!({}))?;
            Ok(())
        }
        "swap-pane" | "swapp" => run_tmux_compat_swap_pane(options, &args),
        "break-pane" | "breakp" => run_tmux_compat_break_pane(options, &args),
        "join-pane" | "joinp" => run_tmux_compat_join_pane(options, &args),
        "next-window" | "next" => run_tmux_compat_workspace_nav(options, "workspace.next"),
        "previous-window" | "prev" => run_tmux_compat_workspace_nav(options, "workspace.previous"),
        "last-window" | "last" => run_tmux_compat_workspace_nav(options, "workspace.last"),
        "show-options" | "show-option" | "show" => run_tmux_compat_show_options(&args),
        "wait-for" => run_wait_for(&args),
        "set-hook" => run_set_hook(&args),
        "bind-key" => run_bind_key(&args),
        "unbind-key" => run_unbind_key(&args),
        "copy-mode" => run_copy_mode(&args),
        "set-buffer" | "setb" => run_tmux_compat_set_buffer(&args),
        "list-buffers" | "listb" => run_tmux_compat_list_buffers(),
        "popup" => {
            let compat_options = tmux_compat_options(options, &args);
            run_popup(&compat_options)
        }
        other => bail!(
            "unsupported tmux compatibility command: {other}; run `cmux help __tmux-compat` or `cmux docs api` for supported commands"
        ),
    }
}

#[derive(Default)]
struct TmuxCompatArgs {
    flags: Vec<String>,
    values: Vec<(String, String)>,
    positionals: Vec<String>,
}

impl TmuxCompatArgs {
    fn has_flag(&self, flag: &str) -> bool {
        self.flags
            .iter()
            .any(|candidate| candidate == flag || compact_short_flag_contains(candidate, flag))
    }

    fn value(&self, flag: &str) -> Option<String> {
        self.values
            .iter()
            .rev()
            .find(|(key, _)| key == flag)
            .map(|(_, value)| value.clone())
    }
}

fn run_tmux_compat_split_window(options: &GlobalOptions, args: &[String]) -> Result<()> {
    let parsed = parse_tmux_compat_args(args, &["-c", "-e", "-F", "-l", "-t"])?;
    let socket = resolve_socket_path(options)?;
    let panes_response = call_socket(&socket, "pane.list", json!({}))?;
    let target = select_tmux_pane(panes_response, parsed.value("-t").as_deref())?;
    let workspace_id = tmux_pane_field(&target, &["workspace_id", "workspace"])
        .context("target pane has no workspace_id")?
        .to_string();
    let pane_id = tmux_pane_field(&target, &["pane_id", "id"])
        .context("target pane has no pane_id")?
        .to_string();
    let surface_id = selected_surface_for_pane(&socket, Some(&workspace_id), &pane_id)?;
    let direction = if parsed.has_flag("-h") {
        if parsed.has_flag("-b") {
            "left"
        } else {
            "right"
        }
    } else if parsed.has_flag("-b") {
        "up"
    } else {
        "down"
    };
    let mut params = json!({
        "workspace_id": workspace_id,
        "direction": direction,
        "focus": !parsed.has_flag("-d"),
        "type": "terminal"
    });
    if let Some(surface_id) = surface_id {
        params["surface_id"] = json!(surface_id);
    }
    if let Some(command) = tmux_shell_command(&parsed.positionals) {
        params["command"] = json!(command);
    }
    let created = call_socket(&socket, "surface.split", params)?;
    if parsed.has_flag("-P") {
        let format = parsed
            .value("-F")
            .unwrap_or_else(|| "#{pane_id}".to_string());
        let workspace_id = created
            .get("workspace_id")
            .and_then(Value::as_str)
            .context("surface.split returned no workspace_id")?;
        let pane_id = created.get("pane_id").and_then(Value::as_str);
        print_tmux_pane_format(&socket, workspace_id, pane_id, &format)?;
    }
    Ok(())
}

fn run_tmux_compat_new_window(options: &GlobalOptions, args: &[String]) -> Result<()> {
    let parsed = parse_tmux_compat_args(
        args,
        &["-c", "-e", "-F", "-f", "-n", "-s", "-t", "-x", "-y"],
    )?;
    let socket = resolve_socket_path(options)?;
    let mut params = json!({
        "focus": !parsed.has_flag("-d")
    });
    if let Some(title) = parsed.value("-n").or_else(|| parsed.value("-s")) {
        params["title"] = json!(title);
    }
    if let Some(cwd) = parsed.value("-c") {
        params["cwd"] = json!(cwd);
    }
    if let Some(command) = tmux_shell_command(&parsed.positionals) {
        params["command"] = json!(command);
    }
    let created = call_socket(&socket, "workspace.create", params)?;
    if parsed.has_flag("-P") {
        let format = parsed
            .value("-F")
            .unwrap_or_else(|| "#{pane_id}".to_string());
        let workspace_id = created
            .get("workspace_id")
            .and_then(Value::as_str)
            .context("workspace.create returned no workspace_id")?;
        print_tmux_pane_format(&socket, workspace_id, None, &format)?;
    }
    Ok(())
}

fn run_tmux_compat_select_pane(options: &GlobalOptions, args: &[String]) -> Result<()> {
    let parsed = parse_tmux_compat_args(args, &["-t", "-T"])?;
    let socket = resolve_socket_path(options)?;
    if parsed.has_flag("-l") {
        let _ = call_socket(&socket, "pane.last", json!({}))?;
        return Ok(());
    }
    if let Some(target) = parsed
        .value("-t")
        .or_else(|| parsed.positionals.first().cloned())
    {
        let pane_id = tmux_rpc_pane_target(&target);
        let _ = call_socket(&socket, "pane.focus", json!({"pane_id": pane_id}))?;
    }
    Ok(())
}

fn run_tmux_compat_capture_pane(options: &GlobalOptions, args: &[String]) -> Result<()> {
    let parsed = parse_tmux_compat_args(args, &["-b", "-E", "-S", "-t"])?;
    let socket = resolve_socket_path(options)?;
    let mut params = json!({});
    if let Some(target) = parsed
        .value("-t")
        .or_else(|| parsed.positionals.first().cloned())
    {
        let pane_id = tmux_rpc_pane_target(&target);
        if let Some(surface_id) = selected_surface_for_pane(&socket, None, &pane_id)? {
            params["surface_id"] = json!(surface_id);
        }
    }
    let response = call_socket(&socket, "surface.read_text", params)?;
    if parsed.has_flag("-p") {
        let text = response
            .get("text")
            .and_then(Value::as_str)
            .unwrap_or_default();
        println!("{text}");
    }
    Ok(())
}

fn run_tmux_compat_resize_pane(options: &GlobalOptions, args: &[String]) -> Result<()> {
    let parsed = parse_tmux_compat_args(args, &["-t", "-x", "-y"])?;
    let socket = resolve_socket_path(options)?;
    let pane_id = tmux_compat_target_pane_id(&socket, parsed.value("-t").as_deref())?;
    let direction = ["-R", "-L", "-U", "-D"]
        .iter()
        .find(|flag| parsed.has_flag(flag))
        .copied()
        .unwrap_or("-R");
    let mut params = json!({
        "pane_id": pane_id,
        "direction": direction
    });
    if let Some(amount) = parsed
        .positionals
        .iter()
        .find_map(|arg| arg.parse::<f64>().ok())
    {
        params["amount"] = json!(amount);
    }
    let _ = call_socket(&socket, "pane.resize", params)?;
    Ok(())
}

fn run_tmux_compat_pipe_pane(options: &GlobalOptions, args: &[String]) -> Result<()> {
    let parsed = parse_tmux_compat_args(args, &["-t", "--command"])?;
    let Some(command) = parsed
        .value("--command")
        .or_else(|| tmux_shell_command(&parsed.positionals))
    else {
        return Ok(());
    };
    let socket = resolve_socket_path(options)?;
    let mut params = tmux_compat_target_surface_params(&socket, parsed.value("-t").as_deref())?;
    params["command"] = json!(command);
    let _ = call_socket(&socket, "surface.pipe_pane", params)?;
    Ok(())
}

fn run_tmux_compat_clear_history(options: &GlobalOptions, args: &[String]) -> Result<()> {
    let parsed = parse_tmux_compat_args(args, &["-t"])?;
    let socket = resolve_socket_path(options)?;
    let params = tmux_compat_target_surface_params(&socket, parsed.value("-t").as_deref())?;
    let _ = call_socket(&socket, "surface.clear_history", params)?;
    Ok(())
}

fn run_tmux_compat_paste_buffer(options: &GlobalOptions, args: &[String]) -> Result<()> {
    let parsed = parse_tmux_compat_args(args, &["-b", "-t", "--name"])?;
    let name = parsed
        .value("-b")
        .or_else(|| parsed.value("--name"))
        .unwrap_or_else(|| "buffer".to_string());
    let text = fs::read_to_string(buffer_path(&name)).unwrap_or_default();
    let socket = resolve_socket_path(options)?;
    let mut params = tmux_compat_target_surface_params(&socket, parsed.value("-t").as_deref())?;
    params["text"] = json!(text);
    let _ = call_socket(&socket, "surface.send_text", params)?;
    Ok(())
}

fn run_tmux_compat_respawn_pane(options: &GlobalOptions, args: &[String]) -> Result<()> {
    let parsed = parse_tmux_compat_args(args, &["-t", "--command"])?;
    let command = parsed
        .value("--command")
        .or_else(|| tmux_shell_command(&parsed.positionals))
        .unwrap_or_default();
    let socket = resolve_socket_path(options)?;
    let mut params = tmux_compat_target_surface_params(&socket, parsed.value("-t").as_deref())?;
    params["text"] = if command.trim().is_empty() {
        json!("")
    } else {
        json!(format!("{command}\n"))
    };
    let _ = call_socket(&socket, "surface.send_text", params)?;
    Ok(())
}

fn run_tmux_compat_swap_pane(options: &GlobalOptions, args: &[String]) -> Result<()> {
    let parsed = parse_tmux_compat_args(args, &["-s", "-t"])?;
    let socket = resolve_socket_path(options)?;
    let pane_id = tmux_compat_target_pane_id(&socket, parsed.value("-s").as_deref())?;
    let target_pane_id = tmux_compat_target_pane_id(&socket, parsed.value("-t").as_deref())?;
    let _ = call_socket(
        &socket,
        "pane.swap",
        json!({"pane_id": pane_id, "target_pane_id": target_pane_id}),
    )?;
    Ok(())
}

fn run_tmux_compat_break_pane(options: &GlobalOptions, args: &[String]) -> Result<()> {
    let parsed = parse_tmux_compat_args(args, &["-F", "-s", "-t"])?;
    let socket = resolve_socket_path(options)?;
    let source = parsed.value("-s").or_else(|| parsed.value("-t"));
    let pane_id = tmux_compat_target_pane_id(&socket, source.as_deref())?;
    let surface_id = selected_surface_for_pane(&socket, None, &pane_id)?
        .context("target pane has no selected surface")?;
    let created = call_socket(&socket, "pane.break", json!({"surface_id": surface_id}))?;
    if parsed.has_flag("-P") {
        let format = parsed
            .value("-F")
            .unwrap_or_else(|| "#{pane_id}".to_string());
        let workspace_id = created
            .get("workspace_id")
            .and_then(Value::as_str)
            .context("pane.break returned no workspace_id")?;
        let pane_id = created.get("pane_id").and_then(Value::as_str);
        print_tmux_pane_format(&socket, workspace_id, pane_id, &format)?;
    }
    Ok(())
}

fn run_tmux_compat_join_pane(options: &GlobalOptions, args: &[String]) -> Result<()> {
    let parsed = parse_tmux_compat_args(args, &["-s", "-t"])?;
    let socket = resolve_socket_path(options)?;
    let source_pane_id = tmux_compat_target_pane_id(&socket, parsed.value("-s").as_deref())?;
    let target_pane_id = tmux_compat_target_pane_id(&socket, parsed.value("-t").as_deref())?;
    let surface_id = selected_surface_for_pane(&socket, None, &source_pane_id)?
        .context("source pane has no selected surface")?;
    let _ = call_socket(
        &socket,
        "pane.join",
        json!({"surface_id": surface_id, "target_pane_id": target_pane_id}),
    )?;
    Ok(())
}

fn run_tmux_compat_workspace_nav(options: &GlobalOptions, method: &str) -> Result<()> {
    let socket = resolve_socket_path(options)?;
    let _ = call_socket(&socket, method, json!({}))?;
    Ok(())
}

fn run_tmux_compat_show_options(args: &[String]) -> Result<()> {
    let parsed = parse_tmux_compat_args(args, &["-t"])?;
    let option = parsed.positionals.last().map(String::as_str).unwrap_or("");
    let value = match option {
        "extended-keys" => "on",
        "status" => "on",
        _ => "",
    };
    if parsed.has_flag("-v") || parsed.has_flag("-q") {
        if !value.is_empty() {
            println!("{value}");
        }
    } else if !option.is_empty() {
        println!("{option} {value}");
    }
    Ok(())
}

fn run_tmux_compat_set_buffer(args: &[String]) -> Result<()> {
    let parsed = parse_tmux_compat_args(args, &["-b", "--name"])?;
    let name = parsed
        .value("-b")
        .or_else(|| parsed.value("--name"))
        .unwrap_or_else(|| "buffer".to_string());
    let text = parsed.positionals.join(" ");
    fs::write(buffer_path(&name), text)?;
    println!("OK");
    Ok(())
}

fn run_tmux_compat_list_buffers() -> Result<()> {
    let dir = cmux_tmp_dir();
    if let Ok(entries) = fs::read_dir(dir) {
        for entry in entries.flatten() {
            let file_name = entry.file_name().to_string_lossy().to_string();
            if let Some(name) = file_name.strip_prefix("buffer-") {
                println!("{name}");
            }
        }
    }
    Ok(())
}

fn tmux_compat_options(options: &GlobalOptions, args: &[String]) -> GlobalOptions {
    GlobalOptions {
        socket: options.socket.clone(),
        explicit_socket: options.explicit_socket,
        json: options.json,
        id_format: options.id_format,
        command: args.to_vec(),
    }
}

fn tmux_shell_command(args: &[String]) -> Option<String> {
    match args {
        [] => None,
        [single] => Some(single.clone()),
        many => Some(shell_join_args(many)),
    }
}

fn parse_tmux_compat_args(args: &[String], value_flags: &[&str]) -> Result<TmuxCompatArgs> {
    let mut parsed = TmuxCompatArgs::default();
    let mut index = 1;
    let mut literal = false;
    'outer: while index < args.len() {
        let arg = &args[index];
        if literal {
            parsed.positionals.push(arg.clone());
            index += 1;
            continue;
        }
        if arg == "--" {
            literal = true;
            index += 1;
            continue;
        }
        if value_flags.contains(&arg.as_str()) {
            let value = args
                .get(index + 1)
                .with_context(|| format!("{arg} requires a value"))?;
            parsed.values.push((arg.clone(), value.clone()));
            index += 2;
            continue;
        }
        if let Some((flag, value)) = arg.split_once('=') {
            if value_flags.contains(&flag) {
                parsed.values.push((flag.to_string(), value.to_string()));
                index += 1;
                continue;
            }
        }
        if arg.starts_with('-') && !arg.starts_with("--") && arg.len() > 2 {
            for value_flag in value_flags {
                if value_flag.len() == 2 && arg.starts_with(value_flag) {
                    parsed
                        .values
                        .push(((*value_flag).to_string(), arg[2..].to_string()));
                    index += 1;
                    continue 'outer;
                }
            }
            for ch in arg[1..].chars() {
                parsed.flags.push(format!("-{ch}"));
            }
            index += 1;
            continue;
        }
        if arg.starts_with('-') {
            parsed.flags.push(arg.clone());
            index += 1;
            continue;
        }
        parsed.positionals.push(arg.clone());
        index += 1;
    }
    Ok(parsed)
}

fn compact_short_flag_contains(candidate: &str, flag: &str) -> bool {
    if !flag.starts_with('-')
        || flag.len() != 2
        || !candidate.starts_with('-')
        || candidate.starts_with("--")
    {
        return false;
    }
    candidate[1..] == flag[1..] || candidate[1..].contains(&flag[1..])
}

fn select_tmux_pane(response: Value, target: Option<&str>) -> Result<Value> {
    let panes = response
        .get("panes")
        .and_then(Value::as_array)
        .context("no panes")?;
    if let Some(target) = target {
        return panes
            .iter()
            .find(|pane| tmux_pane_matches_target(pane, target))
            .cloned()
            .with_context(|| format!("pane target {target} not found"));
    }
    panes
        .iter()
        .find(|pane| pane.get("focused").and_then(Value::as_bool) == Some(true))
        .or_else(|| panes.first())
        .cloned()
        .context("no panes")
}

fn tmux_pane_matches_target(pane: &Value, target: &str) -> bool {
    let normalized = tmux_rpc_pane_target(target);
    ["id", "pane_id", "ref", "pane_ref"]
        .iter()
        .filter_map(|key| pane.get(*key).and_then(Value::as_str))
        .any(|candidate| {
            candidate == target
                || candidate == normalized
                || format!("%{candidate}") == target
                || candidate
                    .strip_prefix("pane:")
                    .is_some_and(|suffix| suffix == normalized || format!("%{suffix}") == target)
        })
}

fn tmux_rpc_pane_target(target: &str) -> String {
    let target = target.trim();
    let target = target.strip_prefix('%').unwrap_or(target);
    if target.starts_with("pane:") {
        target.to_string()
    } else if target.chars().all(|ch| ch.is_ascii_digit()) {
        format!("pane:{target}")
    } else {
        target.to_string()
    }
}

fn tmux_compat_target_pane_id(socket: &str, target: Option<&str>) -> Result<String> {
    let response = call_socket(socket, "pane.list", json!({}))?;
    let pane = select_tmux_pane(response, target)?;
    tmux_pane_field(&pane, &["pane_ref", "pane_id", "id"])
        .context("target pane has no pane_id")
        .map(tmux_rpc_pane_target)
}

fn tmux_compat_target_surface_params(socket: &str, target: Option<&str>) -> Result<Value> {
    let Some(target) = target else {
        return Ok(json!({}));
    };
    let pane_id = tmux_compat_target_pane_id(socket, Some(target))?;
    let surface_id =
        selected_surface_for_pane(socket, None, &pane_id)?.context("target pane has no surface")?;
    Ok(json!({"surface_id": surface_id}))
}

fn tmux_pane_field<'a>(pane: &'a Value, keys: &[&str]) -> Option<&'a str> {
    keys.iter()
        .find_map(|key| pane.get(*key).and_then(Value::as_str))
}

fn selected_surface_for_pane(
    socket: &str,
    workspace_id: Option<&str>,
    pane_id: &str,
) -> Result<Option<String>> {
    let mut params = json!({"pane_id": pane_id});
    if let Some(workspace_id) = workspace_id {
        params["workspace_id"] = json!(workspace_id);
    }
    let response = call_socket(socket, "pane.surfaces", params)?;
    Ok(response
        .get("surfaces")
        .and_then(Value::as_array)
        .and_then(|surfaces| {
            surfaces
                .iter()
                .find(|surface| surface.get("selected").and_then(Value::as_bool) == Some(true))
                .or_else(|| surfaces.first())
        })
        .and_then(|surface| {
            surface
                .get("surface_id")
                .or_else(|| surface.get("id"))
                .and_then(Value::as_str)
        })
        .map(ToString::to_string))
}

fn print_tmux_pane_format(
    socket: &str,
    workspace_id: &str,
    pane_id: Option<&str>,
    format: &str,
) -> Result<()> {
    let response = call_socket(socket, "pane.list", json!({"workspace_id": workspace_id}))?;
    let panes = response
        .get("panes")
        .and_then(Value::as_array)
        .context("no panes")?;
    let pane = pane_id
        .and_then(|pane_id| {
            panes
                .iter()
                .find(|pane| tmux_pane_matches_target(pane, pane_id))
        })
        .or_else(|| {
            panes
                .iter()
                .find(|pane| pane.get("focused").and_then(Value::as_bool) == Some(true))
        })
        .or_else(|| panes.first())
        .context("no panes")?;
    println!("{}", render_tmux_format(format, pane, &response));
    Ok(())
}

fn render_tmux_format(format: &str, pane: &Value, response: &Value) -> String {
    let frame = pane
        .get("pixel_frame")
        .or_else(|| pane.get("frame"))
        .unwrap_or(&Value::Null);
    let container = response
        .get("container_frame")
        .or_else(|| response.get("containerFrame"))
        .unwrap_or(&Value::Null);
    let id = pane
        .get("pane_ref")
        .or_else(|| pane.get("ref"))
        .or_else(|| pane.get("pane_id"))
        .or_else(|| pane.get("id"))
        .and_then(Value::as_str)
        .unwrap_or_default();
    let pane_id = if id.starts_with('%') {
        id.to_string()
    } else {
        format!("%{id}")
    };
    let replacements = [
        ("pane_id", pane_id),
        (
            "pane_width",
            integer_string(pane.get("columns").unwrap_or(&Value::Null)),
        ),
        (
            "pane_height",
            integer_string(pane.get("rows").unwrap_or(&Value::Null)),
        ),
        (
            "pane_left",
            integer_string(frame.get("x").unwrap_or(&Value::Null)),
        ),
        (
            "pane_top",
            integer_string(frame.get("y").unwrap_or(&Value::Null)),
        ),
        (
            "pane_active",
            if pane.get("focused").and_then(Value::as_bool) == Some(true) {
                "1".to_string()
            } else {
                "0".to_string()
            },
        ),
        (
            "window_width",
            integer_string(container.get("width").unwrap_or(&Value::Null)),
        ),
        (
            "window_height",
            integer_string(container.get("height").unwrap_or(&Value::Null)),
        ),
        (
            "pane_title",
            pane.get("title")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string(),
        ),
    ];
    let mut out = format.to_string();
    for (key, value) in replacements {
        out = out.replace(&format!("#{{{key}}}"), &value);
    }
    out
}

fn integer_string(value: &Value) -> String {
    if let Some(value) = value.as_i64() {
        return value.to_string();
    }
    if let Some(value) = value.as_u64() {
        return value.to_string();
    }
    if let Some(value) = value.as_f64() {
        return (value.round() as i64).to_string();
    }
    "0".to_string()
}

fn cmux_tmp_dir() -> PathBuf {
    let dir = std::env::temp_dir().join("cmux-linux");
    let _ = fs::create_dir_all(&dir);
    dir
}

fn wait_path(name: &str) -> PathBuf {
    cmux_tmp_dir().join(format!("wait-{}", safe_file_token(name)))
}

fn buffer_path(name: &str) -> PathBuf {
    cmux_tmp_dir().join(format!("buffer-{}", safe_file_token(name)))
}

fn bindings_path() -> PathBuf {
    cmux_tmp_dir().join("bindings.txt")
}

fn copy_mode_path() -> PathBuf {
    cmux_tmp_dir().join("copy-mode.txt")
}

fn load_bindings() -> Vec<(String, String)> {
    fs::read_to_string(bindings_path())
        .unwrap_or_default()
        .lines()
        .filter_map(|line| {
            let line = line.strip_prefix("bind-key ")?;
            let (key, action) = line.split_once(' ')?;
            Some((key.to_string(), action.to_string()))
        })
        .collect()
}

fn write_bindings(rows: &[(String, String)]) -> Result<()> {
    let text = rows
        .iter()
        .map(|(key, action)| format!("bind-key {key} {action}"))
        .collect::<Vec<_>>()
        .join("\n");
    fs::write(
        bindings_path(),
        if text.is_empty() {
            String::new()
        } else {
            format!("{text}\n")
        },
    )?;
    Ok(())
}

fn safe_file_token(value: &str) -> String {
    let token = value
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_') {
                ch
            } else {
                '_'
            }
        })
        .collect::<String>();
    if token.is_empty() {
        "default".to_string()
    } else {
        token
    }
}

fn resolve_socket_path(options: &GlobalOptions) -> Result<String> {
    if let Some(socket) = &options.socket {
        return Ok(socket.clone());
    }
    let env_socket = normalized_env("CMUX_SOCKET_PATH");
    let legacy_socket = normalized_env("CMUX_SOCKET");
    if let (Some(a), Some(b)) = (&env_socket, &legacy_socket) {
        if a != b && !options.explicit_socket {
            bail!("Refusing to choose socket: CMUX_SOCKET_PATH and CMUX_SOCKET differ. Use CMUX_SOCKET_PATH or unset CMUX_SOCKET.");
        }
    }
    if let Some(socket) = env_socket.or(legacy_socket) {
        return Ok(socket);
    }
    let marker_paths = socket_marker_paths();
    let default_socket = default_socket_path();
    Ok(socket_from_markers_or_default(
        &marker_paths,
        &default_socket,
    ))
}

fn socket_from_markers_or_default(paths: &[PathBuf], default_socket: &str) -> String {
    if let Some(socket) = read_socket_marker(paths) {
        return socket;
    }
    if socket_endpoint_reachable(default_socket) {
        if let Some(marker) = paths.first() {
            if let Some(parent) = marker.parent() {
                let _ = fs::create_dir_all(parent);
            }
            let _ = fs::write(marker, format!("{default_socket}\n"));
        }
    }
    default_socket.to_string()
}

fn read_socket_marker(paths: &[PathBuf]) -> Option<String> {
    for marker in paths {
        let Ok(path) = fs::read_to_string(marker) else {
            continue;
        };
        let trimmed = path.trim();
        if !trimmed.is_empty() && socket_endpoint_reachable(trimmed) {
            return Some(trimmed.to_string());
        }
    }
    None
}

fn socket_endpoint_reachable(socket_path: &str) -> bool {
    if let Some((host, port)) = parse_tcp_socket_addr(socket_path) {
        return TcpStream::connect((host.as_str(), port)).is_ok();
    }
    UnixStream::connect(socket_path).is_ok()
}

fn socket_marker_paths() -> Vec<PathBuf> {
    let xdg_state_home = normalized_env("XDG_STATE_HOME");
    let home = normalized_env("HOME");
    socket_marker_paths_from_env(
        xdg_state_home.as_deref(),
        home.as_deref(),
        &std::env::temp_dir(),
    )
}

fn socket_marker_paths_from_env(
    xdg_state_home: Option<&str>,
    home: Option<&str>,
    temp_dir: &Path,
) -> Vec<PathBuf> {
    let mut paths = Vec::new();
    if let Some(path) = xdg_state_home {
        push_unique_path(
            &mut paths,
            PathBuf::from(path).join("cmux/last-socket-path"),
        );
    }
    if let Some(home) = home {
        push_unique_path(
            &mut paths,
            PathBuf::from(home).join(".local/state/cmux/last-socket-path"),
        );
    }
    push_unique_path(&mut paths, temp_dir.join("cmux/last-socket-path"));
    push_unique_path(&mut paths, PathBuf::from("/tmp/cmux-last-socket-path"));
    paths
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

fn normalized_surface_env() -> Option<String> {
    normalized_env("CMUX_PANEL_ID").or_else(|| normalized_env("CMUX_SURFACE_ID"))
}

fn effective_id_format(command: &[String], default: IdFormat) -> Result<IdFormat> {
    match option_value(command, "--id-format").as_deref() {
        Some("refs") => Ok(IdFormat::Refs),
        Some("uuids") => Ok(IdFormat::Uuids),
        Some("both") => Ok(IdFormat::Both),
        Some(other) => bail!("unknown --id-format {other}"),
        None if command.first().map(String::as_str) == Some("ssh-session-attach") => {
            Ok(IdFormat::Both)
        }
        None => Ok(default),
    }
}

fn command_has_flag(command: &[String], flag: &str) -> bool {
    command
        .iter()
        .take_while(|arg| arg.as_str() != "--")
        .any(|arg| arg == flag)
}

fn command_has_help_flag(command: &[String]) -> bool {
    command
        .iter()
        .skip(1)
        .take_while(|arg| arg.as_str() != "--")
        .any(|arg| matches!(arg.as_str(), "--help" | "-h"))
}

fn default_socket_path() -> String {
    default_socket_path_in_state_dir(&state_dir())
}

fn default_socket_path_in_state_dir(state_dir: &Path) -> String {
    state_dir.join("cmux.sock").display().to_string()
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

fn cache_dir() -> PathBuf {
    let xdg_cache_home = normalized_env("XDG_CACHE_HOME");
    let home = normalized_env("HOME");
    cache_dir_from_env(
        xdg_cache_home.as_deref(),
        home.as_deref(),
        &std::env::temp_dir(),
    )
}

fn cache_dir_from_env(
    xdg_cache_home: Option<&str>,
    home: Option<&str>,
    temp_dir: &Path,
) -> PathBuf {
    if let Some(path) = xdg_cache_home {
        return PathBuf::from(path).join("cmux");
    }
    if let Some(home) = home {
        return PathBuf::from(home).join(".cache/cmux");
    }
    temp_dir.join("cmux-cache")
}

enum TextMode {
    Pong,
    Ok,
    OkRef(&'static str),
    MarkdownOpen,
    TabAction,
    Text,
    BrowserSnapshot,
    BrowserScreenshot {
        out: Option<String>,
        json_output: bool,
    },
    BrowserPdf {
        out: Option<String>,
        json_output: bool,
    },
    BrowserAvailability {
        status_only: bool,
    },
    AgentHibernation,
    AuthStatus,
    AuthLogin,
    AuthLogout,
    FeedList,
    FeedClear,
    SettingsOpen,
    SurfaceResumeGet,
    DiffOpen,
    RightSidebar,
    CustomSidebar {
        action: String,
    },
    RemoteTmuxWindow,
    Jsonish,
}

fn command_to_request(command: &[String]) -> Result<(String, Value, TextMode)> {
    let name = command.first().map(String::as_str).unwrap_or("help");
    if command_has_help_flag(command) {
        print_command_help(name);
        std::process::exit(0);
    }

    match name {
        "ping" => Ok(("system.ping".to_string(), json!({}), TextMode::Pong)),
        "capabilities" => Ok((
            "system.capabilities".to_string(),
            json!({}),
            TextMode::Jsonish,
        )),
        "auth" | "login" | "logout" => auth_command_to_request(command),
        "disable-browser" => Ok((
            "browser.disable".to_string(),
            json!({}),
            TextMode::BrowserAvailability { status_only: false },
        )),
        "enable-browser" => Ok((
            "browser.enable".to_string(),
            json!({}),
            TextMode::BrowserAvailability { status_only: false },
        )),
        "browser-status" => Ok((
            "browser.status".to_string(),
            json!({}),
            TextMode::BrowserAvailability { status_only: true },
        )),
        "agent-hibernation" => Ok((
            "agent.hibernation.set".to_string(),
            agent_hibernation_params(command)?,
            TextMode::AgentHibernation,
        )),
        "rpc" => {
            let method = command.get(1).context("rpc requires a method")?.clone();
            let params = command
                .get(2)
                .map(|s| serde_json::from_str::<Value>(s))
                .transpose()?
                .unwrap_or_else(|| json!({}));
            Ok((method, params, TextMode::Jsonish))
        }
        "identify" => {
            let mut caller = Map::new();
            collect_option(command, "--workspace", "workspace_id", &mut caller)?;
            collect_option(command, "--surface", "surface_id", &mut caller)?;
            collect_option(command, "--window", "window_id", &mut caller)?;
            let params = if caller.is_empty() {
                json!({})
            } else {
                json!({"caller": caller})
            };
            Ok(("system.identify".to_string(), params, TextMode::Jsonish))
        }
        "tree" => Ok((
            "system.tree".to_string(),
            tree_params(command)?,
            TextMode::Jsonish,
        )),
        "top" => Ok((
            "system.top".to_string(),
            system_top_params(command)?,
            TextMode::Jsonish,
        )),
        "memory" => Ok((
            "system.memory".to_string(),
            system_memory_params(command)?,
            TextMode::Jsonish,
        )),
        "open" => open_command_to_request(command),
        "markdown" => markdown_command_to_request(command),
        "diff" => diff_command_to_request(command),
        "feed" => feed_command_to_request(command),
        "hooks" | "claude-hook" => hook_command_to_request(command),
        "feedback" => feedback_command_to_request(command),
        "settings" => settings_command_to_request(command),
        "shortcuts" => shortcuts_command_to_request(command),
        "config" => {
            if command.get(1).map(String::as_str) == Some("reload") && command.len() == 2 {
                Ok(("config.reload".to_string(), json!({}), TextMode::Text))
            } else {
                bail!("Unknown config subcommand. Run 'cmux config --help'.")
            }
        }
        "reload-config" => {
            if command.len() == 1 {
                Ok(("config.reload".to_string(), json!({}), TextMode::Text))
            } else {
                bail!("Usage: cmux reload-config")
            }
        }
        "remotes" | "remote" => remotes_command_to_request(command),
        "mobile" => mobile_command_to_request(command),
        "vm" | "cloud" => vm_command_to_request(command),
        "restore-session" => Ok((
            "session.restore_previous".to_string(),
            json!({}),
            TextMode::Ok,
        )),
        "window" => window_command_to_request(command),
        "surface" => surface_command_to_request(command),
        "workspace" => workspace_command_to_request(command),
        "workspace-group" => workspace_group_command_to_request(command),
        "list-windows" => Ok(("window.list".to_string(), json!({}), TextMode::Jsonish)),
        "current-window" => Ok(("window.current".to_string(), json!({}), TextMode::Jsonish)),
        "new-window" => Ok((
            "window.create".to_string(),
            json!({}),
            TextMode::OkRef("window_ref"),
        )),
        "focus-window" => Ok((
            "window.focus".to_string(),
            option_params(command, &[("--window", "window_id")])?,
            TextMode::Jsonish,
        )),
        "close-window" => Ok((
            "window.close".to_string(),
            option_params(command, &[("--window", "window_id")])?,
            TextMode::Jsonish,
        )),
        "move-workspace-to-window" => Ok((
            "workspace.move_to_window".to_string(),
            move_workspace_to_window_params(command)?,
            TextMode::Jsonish,
        )),
        "list-workspaces" => Ok((
            "workspace.list".to_string(),
            option_params(command, &[("--window", "window_id")])?,
            TextMode::Jsonish,
        )),
        "current-workspace" => Ok((
            "workspace.current".to_string(),
            json!({}),
            TextMode::Jsonish,
        )),
        "new-workspace" => Ok((
            "workspace.create".to_string(),
            workspace_create_params(command)?,
            TextMode::OkRef("workspace_ref"),
        )),
        "select-workspace" => Ok((
            "workspace.select".to_string(),
            select_workspace_params(command, 1)?,
            TextMode::Jsonish,
        )),
        "close-workspace" => Ok((
            "workspace.close".to_string(),
            close_workspace_params(command)?,
            TextMode::Jsonish,
        )),
        "reorder-workspace" => Ok((
            "workspace.reorder".to_string(),
            reorder_workspace_params(command)?,
            TextMode::Jsonish,
        )),
        "reorder-workspaces" => Ok((
            "workspace.reorder_many".to_string(),
            reorder_workspaces_params(command)?,
            TextMode::Jsonish,
        )),
        "rename-workspace" | "rename-window" => {
            let mut params = option_params(command, &[("--workspace", "workspace_id")])?;
            add_env_default(&mut params, "workspace_id", "CMUX_WORKSPACE_ID");
            let title =
                trailing_title(command).with_context(|| format!("{name} requires a title"))?;
            params["title"] = json!(title);
            Ok(("workspace.rename".to_string(), params, TextMode::Jsonish))
        }
        "find-window" => Ok(("workspace.list".to_string(), json!({}), TextMode::Jsonish)),
        "next-window" => Ok(("workspace.next".to_string(), json!({}), TextMode::Jsonish)),
        "previous-window" => Ok((
            "workspace.previous".to_string(),
            json!({}),
            TextMode::Jsonish,
        )),
        "last-window" => Ok(("workspace.last".to_string(), json!({}), TextMode::Jsonish)),
        "list-panes" => Ok((
            "pane.list".to_string(),
            workspace_option_params(command, &[("--workspace", "workspace_id")])?,
            TextMode::Jsonish,
        )),
        "list-pane-surfaces" => Ok((
            "pane.surfaces".to_string(),
            workspace_option_params(
                command,
                &[("--pane", "pane_id"), ("--workspace", "workspace_id")],
            )?,
            TextMode::Jsonish,
        )),
        "debug-terminals" => Ok(("debug.terminals".to_string(), json!({}), TextMode::Jsonish)),
        "focus-pane" => Ok((
            "pane.focus".to_string(),
            workspace_option_params(
                command,
                &[("--pane", "pane_id"), ("--workspace", "workspace_id")],
            )?,
            TextMode::Jsonish,
        )),
        "last-pane" => Ok(("pane.last".to_string(), json!({}), TextMode::Jsonish)),
        "swap-pane" => Ok((
            "pane.swap".to_string(),
            option_params(
                command,
                &[
                    ("--pane", "pane_id"),
                    ("--target-pane", "target_pane_id"),
                    ("--workspace", "workspace_id"),
                ],
            )?,
            TextMode::Jsonish,
        )),
        "break-pane" => Ok((
            "pane.break".to_string(),
            option_params(
                command,
                &[("--surface", "surface_id"), ("--workspace", "workspace_id")],
            )?,
            TextMode::Jsonish,
        )),
        "join-pane" => Ok((
            "pane.join".to_string(),
            option_params(
                command,
                &[
                    ("--surface", "surface_id"),
                    ("--target-pane", "target_pane_id"),
                    ("--pane", "pane_id"),
                    ("--workspace", "workspace_id"),
                ],
            )?,
            TextMode::Jsonish,
        )),
        "resize-pane" => {
            let mut params = option_params(
                command,
                &[
                    ("--pane", "pane_id"),
                    ("--workspace", "workspace_id"),
                    ("--amount", "amount"),
                ],
            )?;
            if let Some(direction) = command
                .iter()
                .find(|arg| matches!(arg.as_str(), "-R" | "-L" | "-U" | "-D"))
            {
                params["direction"] = json!(direction);
            }
            Ok(("pane.resize".to_string(), params, TextMode::Jsonish))
        }
        "equalize-splits" => Ok((
            "workspace.equalize_splits".to_string(),
            equalize_splits_params(command)?,
            TextMode::Jsonish,
        )),
        "new-split" | "drag-surface-to-split" => {
            let mut params = option_params(
                command,
                &[
                    ("--workspace", "workspace_id"),
                    ("--surface", "surface_id"),
                    ("--panel", "surface_id"),
                    ("--direction", "direction"),
                ],
            )?;
            add_env_default(&mut params, "workspace_id", "CMUX_WORKSPACE_ID");
            add_env_surface_default(&mut params);
            if params.get("direction").is_none() {
                if let Some(direction) = first_positional_after(command, 1) {
                    params["direction"] = json!(direction);
                }
            }
            Ok((
                if name == "new-split" {
                    "surface.split"
                } else {
                    "surface.drag_to_split"
                }
                .to_string(),
                params,
                TextMode::OkRef("surface_ref"),
            ))
        }
        "move-surface" => Ok((
            "surface.move".to_string(),
            surface_move_params(command)?,
            TextMode::Jsonish,
        )),
        "split-off" => Ok((
            "surface.split_off".to_string(),
            split_off_params(command)?,
            TextMode::Jsonish,
        )),
        "reorder-surface" => Ok((
            "surface.reorder".to_string(),
            surface_reorder_params(command)?,
            TextMode::Jsonish,
        )),
        "new-pane" => Ok((
            "pane.create".to_string(),
            new_surface_params(command)?,
            TextMode::OkRef("surface_ref"),
        )),
        "new-surface" => Ok((
            "surface.create".to_string(),
            new_surface_params(command)?,
            TextMode::OkRef("surface_ref"),
        )),
        "close-surface" => Ok((
            "surface.close".to_string(),
            surface_option_params(command)?,
            TextMode::Jsonish,
        )),
        "list-panels" | "list-surfaces" => Ok((
            "surface.list".to_string(),
            workspace_option_params(command, &[("--workspace", "workspace_id")])?,
            TextMode::Jsonish,
        )),
        "renderer" => renderer_command_to_request(command),
        "move-tab-to-new-workspace" | "detach-tab" => Ok((
            "tab.action".to_string(),
            move_tab_to_new_workspace_params(command)?,
            TextMode::TabAction,
        )),
        "rename-tab" => {
            let mut params = option_params(
                command,
                &[
                    ("--workspace", "workspace_id"),
                    ("--window", "window_id"),
                    ("--tab", "surface_id"),
                    ("--surface", "surface_id"),
                    ("--panel", "surface_id"),
                ],
            )?;
            add_env_default(&mut params, "workspace_id", "CMUX_WORKSPACE_ID");
            add_env_surface_default(&mut params);
            let title = trailing_title(command).context("rename-tab requires a title")?;
            params["action"] = json!("rename");
            params["title"] = json!(title);
            Ok(("tab.action".to_string(), params, TextMode::TabAction))
        }
        "tab-action" => Ok((
            "tab.action".to_string(),
            tab_action_params(command)?,
            TextMode::TabAction,
        )),
        "workspace-action" => Ok((
            "workspace.action".to_string(),
            workspace_action_params(command)?,
            TextMode::Jsonish,
        )),
        "focus-panel" | "focus-surface" => Ok((
            "surface.focus".to_string(),
            surface_option_params(command)?,
            TextMode::Jsonish,
        )),
        "surface-health" => Ok((
            "surface.health".to_string(),
            workspace_option_params(command, &[("--workspace", "workspace_id")])?,
            TextMode::Jsonish,
        )),
        "trigger-flash" => Ok((
            "surface.trigger_flash".to_string(),
            surface_option_params(command)?,
            TextMode::Jsonish,
        )),
        "refresh-surfaces" => Ok(("surface.refresh".to_string(), json!({}), TextMode::Jsonish)),
        "send" | "send-panel" => {
            let mut params = option_params(
                command,
                &[
                    ("--workspace", "workspace_id"),
                    ("--surface", "surface_id"),
                    ("--panel", "surface_id"),
                ],
            )?;
            add_env_default(&mut params, "workspace_id", "CMUX_WORKSPACE_ID");
            add_env_surface_default(&mut params);
            params["text"] = json!(last_positional(command).context("send requires text")?);
            Ok(("surface.send_text".to_string(), params, TextMode::Jsonish))
        }
        "send-key" | "send-key-panel" => {
            let mut params = option_params(
                command,
                &[
                    ("--workspace", "workspace_id"),
                    ("--surface", "surface_id"),
                    ("--panel", "surface_id"),
                ],
            )?;
            add_env_default(&mut params, "workspace_id", "CMUX_WORKSPACE_ID");
            add_env_surface_default(&mut params);
            params["key"] = json!(last_positional(command).context("send-key requires key")?);
            Ok(("surface.send_key".to_string(), params, TextMode::Jsonish))
        }
        "read-screen" | "capture-pane" => Ok((
            "surface.read_text".to_string(),
            read_screen_params(command)?,
            TextMode::Jsonish,
        )),
        "clear-history" => Ok((
            "surface.clear_history".to_string(),
            read_screen_params(command)?,
            TextMode::Jsonish,
        )),
        "pipe-pane" => Ok((
            "surface.pipe_pane".to_string(),
            {
                let mut params = read_screen_params(command)?;
                if let Some(pipe_command) = option_value(command, "--command") {
                    params["command"] = json!(pipe_command);
                }
                params
            },
            TextMode::Jsonish,
        )),
        "paste-buffer" => {
            let name = option_value(command, "--name").unwrap_or_else(|| "buffer".into());
            let text = fs::read_to_string(buffer_path(&name)).unwrap_or_default();
            let mut params = read_screen_params(command)?;
            params["text"] = json!(text);
            Ok(("surface.send_text".to_string(), params, TextMode::Jsonish))
        }
        "respawn-pane" => {
            let mut params = read_screen_params(command)?;
            let text = option_value(command, "--command")
                .map(|command| format!("{command}\n"))
                .unwrap_or_default();
            params["text"] = json!(text);
            Ok(("surface.send_text".to_string(), params, TextMode::Jsonish))
        }
        "notify" => Ok((
            "notification.create".to_string(),
            notify_params(command)?,
            TextMode::Jsonish,
        )),
        "list-notifications" => Ok((
            "notification.list".to_string(),
            json!({}),
            TextMode::Jsonish,
        )),
        "dismiss-notification" => Ok((
            "notification.dismiss".to_string(),
            dismiss_notification_params(command)?,
            TextMode::Jsonish,
        )),
        "mark-notification-read" => Ok((
            "notification.mark_read".to_string(),
            mark_notification_read_params(command)?,
            TextMode::Jsonish,
        )),
        "open-notification" => Ok((
            "notification.open".to_string(),
            open_notification_params(command)?,
            TextMode::Jsonish,
        )),
        "jump-to-unread" => Ok((
            "notification.jump_to_unread".to_string(),
            json!({}),
            TextMode::Jsonish,
        )),
        "clear-notifications" => Ok((
            "notification.clear".to_string(),
            json!({}),
            TextMode::Jsonish,
        )),
        "set-status" => Ok((
            "sidebar.status.set".to_string(),
            sidebar_status_set_params(command)?,
            TextMode::Ok,
        )),
        "set-agent-lifecycle" | "set_agent_lifecycle" => Ok((
            "sidebar.agent_lifecycle.set".to_string(),
            sidebar_agent_lifecycle_set_params(command)?,
            TextMode::Ok,
        )),
        "report-meta" | "report_meta" => Ok((
            "sidebar.meta.set".to_string(),
            sidebar_status_set_params(command)?,
            TextMode::Ok,
        )),
        "clear-status" => Ok((
            "sidebar.status.clear".to_string(),
            sidebar_key_params(command, "clear-status requires a key")?,
            TextMode::Ok,
        )),
        "clear-meta" | "clear_meta" => Ok((
            "sidebar.meta.clear".to_string(),
            sidebar_key_params(command, "clear-meta requires a key")?,
            TextMode::Ok,
        )),
        "list-status" => Ok((
            "sidebar.status.list".to_string(),
            sidebar_target_params(command)?,
            TextMode::Text,
        )),
        "list-meta" | "list_meta" => Ok((
            "sidebar.meta.list".to_string(),
            sidebar_target_params(command)?,
            TextMode::Text,
        )),
        "report-meta-block" | "report_meta_block" => Ok((
            "sidebar.meta_block.set".to_string(),
            sidebar_metadata_block_set_params(command)?,
            TextMode::Ok,
        )),
        "clear-meta-block" | "clear_meta_block" => Ok((
            "sidebar.meta_block.clear".to_string(),
            sidebar_key_params(command, "clear-meta-block requires a key")?,
            TextMode::Ok,
        )),
        "list-meta-blocks" | "list_meta_blocks" => Ok((
            "sidebar.meta_block.list".to_string(),
            sidebar_target_params(command)?,
            TextMode::Text,
        )),
        "set-progress" => Ok((
            "sidebar.progress.set".to_string(),
            sidebar_progress_set_params(command)?,
            TextMode::Ok,
        )),
        "clear-progress" => Ok((
            "sidebar.progress.clear".to_string(),
            sidebar_target_params(command)?,
            TextMode::Ok,
        )),
        "log" => Ok((
            "sidebar.log.append".to_string(),
            sidebar_log_append_params(command)?,
            TextMode::Ok,
        )),
        "clear-log" => Ok((
            "sidebar.log.clear".to_string(),
            sidebar_target_params(command)?,
            TextMode::Ok,
        )),
        "list-log" => Ok((
            "sidebar.log.list".to_string(),
            sidebar_log_list_params(command)?,
            TextMode::Text,
        )),
        "sidebar-state" => Ok((
            "sidebar.state".to_string(),
            sidebar_target_params(command)?,
            TextMode::Text,
        )),
        "set-app-focus" => Ok((
            "app.focus_override.set".to_string(),
            app_focus_override_params(command)?,
            TextMode::Jsonish,
        )),
        "simulate-app-active" => Ok((
            "app.simulate_active".to_string(),
            json!({}),
            TextMode::Jsonish,
        )),
        "right-sidebar" => Ok((
            "sidebar.right".to_string(),
            right_sidebar_params(command)?,
            TextMode::RightSidebar,
        )),
        "sidebar" => {
            let (method, params, action) = custom_sidebar_params(command)?;
            Ok((method, params, TextMode::CustomSidebar { action }))
        }
        "ssh" => Ok((
            "workspace.remote.create".to_string(),
            ssh_params(command)?,
            TextMode::Jsonish,
        )),
        "ssh-tmux" => Ok((
            "remote.tmux.window".to_string(),
            ssh_tmux_params(command)?,
            TextMode::RemoteTmuxWindow,
        )),
        "ssh-session-list" => Ok((
            "ssh.session.list".to_string(),
            ssh_session_params(command, false)?,
            TextMode::Jsonish,
        )),
        "ssh-session-attach" => Ok((
            "ssh.session.attach".to_string(),
            ssh_session_params(command, true)?,
            TextMode::Jsonish,
        )),
        "ssh-session-cleanup" => Ok((
            "ssh.session.cleanup".to_string(),
            ssh_session_params(command, true)?,
            TextMode::Jsonish,
        )),
        "ssh-session-snapshot" => Ok((
            "ssh.session.snapshot".to_string(),
            ssh_session_snapshot_params(command)?,
            TextMode::Jsonish,
        )),
        "ssh-session-restore" | "ssh-session-restore-snapshot" => Ok((
            "ssh.session.restore_snapshot".to_string(),
            ssh_session_restore_snapshot_params(command)?,
            TextMode::Jsonish,
        )),
        "open-browser" | "open_browser" => browser_legacy_command_to_request(command, "open"),
        "navigate" => browser_legacy_command_to_request(command, "navigate"),
        "browser-back" | "browser_back" => browser_legacy_command_to_request(command, "back"),
        "browser-forward" | "browser_forward" => {
            browser_legacy_command_to_request(command, "forward")
        }
        "browser-reload" | "browser_reload" => browser_legacy_command_to_request(command, "reload"),
        "get-url" | "get_url" => browser_legacy_command_to_request(command, "get-url"),
        "focus-webview" | "focus_webview" => {
            browser_legacy_command_to_request(command, "focus-webview")
        }
        "is-webview-focused" | "is_webview_focused" => {
            browser_legacy_command_to_request(command, "is-webview-focused")
        }
        "browser" => browser_command_to_request(command),
        other if command.len() == 1 && looks_like_open_target(other) => {
            open_command_to_request(&["open".to_string(), other.to_string()])
        }
        other => bail!("unknown command: {other}"),
    }
}

fn surface_command_to_request(command: &[String]) -> Result<(String, Value, TextMode)> {
    let subcommand = command.get(1).map(String::as_str).unwrap_or("help");
    match subcommand {
        "resume" => surface_resume_command_to_request(command),
        "help" => {
            print_command_help("surface");
            std::process::exit(0);
        }
        other => bail!("Unknown surface subcommand: {other}"),
    }
}

fn surface_resume_command_to_request(command: &[String]) -> Result<(String, Value, TextMode)> {
    let raw_action = command.get(2).map(String::as_str);
    let (action, mut index) = match raw_action {
        Some("set" | "get" | "show" | "clear" | "approve" | "run") => (raw_action.unwrap(), 3),
        Some(value) if value.starts_with('-') => ("get", 2),
        Some("help") | None => ("get", 3),
        Some(other) => bail!("Unknown surface resume subcommand: {other}"),
    };
    if raw_action == Some("help") {
        print_command_help("surface");
        std::process::exit(0);
    }

    let mut params = Map::new();
    let mut environment = Map::new();
    let mut trailing_command = Vec::new();
    while index < command.len() {
        match command[index].as_str() {
            "--json" => {}
            "--surface" | "--surface-id" | "--tab" | "--tab-id" => {
                index += 1;
                params.insert(
                    "surface_id".to_string(),
                    scalar_value(command.get(index).context("--surface requires a value")?),
                );
            }
            "--workspace" | "--workspace-id" => {
                index += 1;
                params.insert(
                    "workspace_id".to_string(),
                    scalar_value(command.get(index).context("--workspace requires a value")?),
                );
            }
            "--pane" | "--pane-id" => {
                index += 1;
                params.insert(
                    "pane_id".to_string(),
                    scalar_value(command.get(index).context("--pane requires a value")?),
                );
            }
            "--name" => {
                index += 1;
                params.insert(
                    "name".to_string(),
                    json!(command.get(index).context("--name requires a value")?),
                );
            }
            "--kind" => {
                index += 1;
                params.insert(
                    "kind".to_string(),
                    json!(command.get(index).context("--kind requires a value")?),
                );
            }
            "--checkpoint" | "--checkpoint-id" => {
                index += 1;
                params.insert(
                    "checkpoint_id".to_string(),
                    json!(command
                        .get(index)
                        .context("--checkpoint requires a value")?),
                );
            }
            "--source" => {
                index += 1;
                params.insert(
                    "source".to_string(),
                    json!(command.get(index).context("--source requires a value")?),
                );
            }
            "--cwd" => {
                index += 1;
                params.insert(
                    "cwd".to_string(),
                    json!(command.get(index).context("--cwd requires a value")?),
                );
            }
            "--shell" | "--command" => {
                index += 1;
                params.insert(
                    "command".to_string(),
                    json!(command.get(index).context("--shell requires a value")?),
                );
            }
            "--env" => {
                index += 1;
                let entry = command.get(index).context("--env requires KEY=VALUE")?;
                let (key, value) = entry
                    .split_once('=')
                    .with_context(|| format!("--env requires KEY=VALUE, got {entry}"))?;
                if key.trim().is_empty() {
                    bail!("--env requires a non-empty key");
                }
                environment.insert(key.to_string(), json!(value));
            }
            "--auto-resume" => {
                params.insert("auto_resume".to_string(), json!(true));
            }
            "--no-auto-resume" => {
                params.insert("auto_resume".to_string(), json!(false));
            }
            "--policy" => {
                index += 1;
                params.insert(
                    "policy".to_string(),
                    json!(command.get(index).context("--policy requires a value")?),
                );
            }
            "--skip" => {
                params.insert("run".to_string(), json!(false));
            }
            "--" => {
                trailing_command.extend(command.iter().skip(index + 1).cloned());
                break;
            }
            other if !other.starts_with('-') && action == "set" => {
                trailing_command.push(other.to_string());
            }
            other => bail!("surface resume {action}: unknown argument '{other}'"),
        }
        index += 1;
    }

    if !environment.is_empty() {
        params.insert("environment".to_string(), Value::Object(environment));
    }
    if action == "set" && !params.contains_key("command") && !trailing_command.is_empty() {
        params.insert(
            "command".to_string(),
            json!(shell_join_args(&trailing_command)),
        );
    }
    if action == "set" && !params.contains_key("source") {
        params.insert("source".to_string(), json!("cli"));
    }

    match action {
        "set" => Ok((
            "surface.resume.set".to_string(),
            Value::Object(params),
            TextMode::Ok,
        )),
        "get" | "show" => Ok((
            "surface.resume.get".to_string(),
            Value::Object(params),
            TextMode::SurfaceResumeGet,
        )),
        "clear" => Ok((
            "surface.resume.clear".to_string(),
            Value::Object(params),
            TextMode::Ok,
        )),
        "approve" => {
            if !params.contains_key("policy") {
                bail!("surface resume approve requires --policy manual|prompt|auto");
            }
            Ok((
                "surface.resume.approve".to_string(),
                Value::Object(params),
                TextMode::Ok,
            ))
        }
        "run" => Ok((
            "surface.resume.run".to_string(),
            Value::Object(params),
            TextMode::Ok,
        )),
        _ => unreachable!("surface resume action was already validated"),
    }
}

fn workspace_command_to_request(command: &[String]) -> Result<(String, Value, TextMode)> {
    let subcommand = command.get(1).map(String::as_str).unwrap_or("list");
    match subcommand {
        "list" | "ls" => Ok((
            "workspace.list".to_string(),
            option_params(command, &[("--window", "window_id")])?,
            TextMode::Jsonish,
        )),
        "current" | "show" => Ok((
            "workspace.current".to_string(),
            json!({}),
            TextMode::Jsonish,
        )),
        "create" | "new" => {
            let delegated = delegated_command("new-workspace", command, 2);
            Ok((
                "workspace.create".to_string(),
                workspace_create_params(&delegated)?,
                TextMode::OkRef("workspace_ref"),
            ))
        }
        "select" | "focus" => Ok((
            "workspace.select".to_string(),
            select_workspace_params(command, 2)?,
            TextMode::Jsonish,
        )),
        "close" | "rm" | "delete" => {
            let delegated = delegated_command("close-workspace", command, 2);
            Ok((
                "workspace.close".to_string(),
                close_workspace_params(&delegated)?,
                TextMode::Jsonish,
            ))
        }
        "rename" => Ok((
            "workspace.rename".to_string(),
            workspace_namespace_rename_params(command)?,
            TextMode::Jsonish,
        )),
        "env" => Ok((
            "workspace.env".to_string(),
            workspace_env_params(command)?,
            TextMode::Jsonish,
        )),
        "reconnect" => Ok((
            "workspace.remote.reconnect".to_string(),
            workspace_remote_namespace_params(command)?,
            TextMode::Jsonish,
        )),
        "disconnect" => Ok((
            "workspace.remote.disconnect".to_string(),
            workspace_remote_namespace_params(command)?,
            TextMode::Jsonish,
        )),
        "group" => {
            let delegated = delegated_command("workspace-group", command, 2);
            workspace_group_command_to_request(&delegated)
        }
        "help" => {
            print_command_help("workspace");
            std::process::exit(0);
        }
        other => bail!("Unknown workspace subcommand: {other}"),
    }
}

fn delegated_command(name: &str, command: &[String], start: usize) -> Vec<String> {
    let mut out = Vec::with_capacity(command.len().saturating_sub(start) + 1);
    out.push(name.to_string());
    out.extend(command.iter().skip(start).cloned());
    out
}

fn workspace_group_command_to_request(command: &[String]) -> Result<(String, Value, TextMode)> {
    let subcommand = command.get(1).map(String::as_str).unwrap_or("list");
    match subcommand {
        "list" => Ok((
            "workspace.group.list".to_string(),
            option_params(command, &[("--window", "window_id")])?,
            TextMode::Jsonish,
        )),
        "create" => {
            let mut params = Map::new();
            collect_option(command, "--name", "name", &mut params)?;
            collect_option(command, "--cwd", "cwd", &mut params)?;
            collect_option(command, "--window", "window_id", &mut params)?;
            if let Some(from) = option_value(command, "--from") {
                params.insert(
                    "child_workspace_ids".to_string(),
                    json!(from
                        .split(',')
                        .map(str::trim)
                        .filter(|part| !part.is_empty())
                        .collect::<Vec<_>>()),
                );
            }
            Ok((
                "workspace.group.create".to_string(),
                Value::Object(params),
                TextMode::Jsonish,
            ))
        }
        "ungroup" | "delete" | "collapse" | "expand" | "pin" | "unpin" | "focus" => {
            let mut params = workspace_group_target_params(command)?;
            collect_option(command, "--window", "window_id", &mut params)?;
            Ok((
                format!("workspace.group.{subcommand}"),
                Value::Object(params),
                TextMode::Jsonish,
            ))
        }
        "rename" => {
            let mut params = workspace_group_target_params(command)?;
            let name = option_value(command, "--name")
                .or_else(|| workspace_group_positional(command, 3))
                .context("workspace-group rename requires --name <name>")?;
            params.insert("name".to_string(), json!(name));
            Ok((
                "workspace.group.rename".to_string(),
                Value::Object(params),
                TextMode::Jsonish,
            ))
        }
        "add" => {
            let mut params = Map::new();
            let group = option_value(command, "--group")
                .or_else(|| option_value(command, "--group-id"))
                .or_else(|| workspace_group_positional(command, 2))
                .context("workspace-group add requires --group <group-id>")?;
            let workspace = option_value(command, "--workspace")
                .or_else(|| option_value(command, "--workspace-id"))
                .context("workspace-group add requires --workspace <workspace-id>")?;
            params.insert("group_id".to_string(), scalar_value(&group));
            params.insert("workspace_id".to_string(), scalar_value(&workspace));
            Ok((
                "workspace.group.add".to_string(),
                Value::Object(params),
                TextMode::Jsonish,
            ))
        }
        "remove" => {
            let mut params = Map::new();
            let workspace = option_value(command, "--workspace")
                .or_else(|| option_value(command, "--workspace-id"))
                .or_else(|| workspace_group_positional(command, 2))
                .context("workspace-group remove requires --workspace <workspace-id>")?;
            params.insert("workspace_id".to_string(), scalar_value(&workspace));
            Ok((
                "workspace.group.remove".to_string(),
                Value::Object(params),
                TextMode::Jsonish,
            ))
        }
        "set-anchor" => {
            let mut params = Map::new();
            let group = option_value(command, "--group")
                .or_else(|| option_value(command, "--group-id"))
                .or_else(|| workspace_group_positional(command, 2))
                .context("workspace-group set-anchor requires --group <group-id>")?;
            let workspace = option_value(command, "--workspace")
                .or_else(|| option_value(command, "--workspace-id"))
                .context("workspace-group set-anchor requires --workspace <workspace-id>")?;
            params.insert("group_id".to_string(), scalar_value(&group));
            params.insert("workspace_id".to_string(), scalar_value(&workspace));
            Ok((
                "workspace.group.set_anchor".to_string(),
                Value::Object(params),
                TextMode::Jsonish,
            ))
        }
        "new-workspace" => {
            let mut params = workspace_group_target_params(command)?;
            if let Some(placement) = option_value(command, "--placement") {
                params.insert("placement".to_string(), json!(placement));
            }
            Ok((
                "workspace.group.new_workspace".to_string(),
                Value::Object(params),
                TextMode::Jsonish,
            ))
        }
        "set-color" => {
            let mut params = workspace_group_target_params(command)?;
            let hex = option_value(command, "--hex")
                .or_else(|| option_value(command, "--color"))
                .unwrap_or_default();
            params.insert("hex".to_string(), json!(hex));
            Ok((
                "workspace.group.set_color".to_string(),
                Value::Object(params),
                TextMode::Jsonish,
            ))
        }
        "set-icon" => {
            let mut params = workspace_group_target_params(command)?;
            let symbol = option_value(command, "--symbol")
                .or_else(|| option_value(command, "--icon"))
                .unwrap_or_default();
            params.insert("symbol".to_string(), json!(symbol));
            Ok((
                "workspace.group.set_icon".to_string(),
                Value::Object(params),
                TextMode::Jsonish,
            ))
        }
        "move" => {
            let mut params = workspace_group_target_params(command)?;
            collect_option(command, "--index", "to_index", &mut params)?;
            collect_option(command, "--to-index", "to_index", &mut params)?;
            collect_option(command, "--before-group", "before_group_id", &mut params)?;
            collect_option(command, "--after-group", "after_group_id", &mut params)?;
            Ok((
                "workspace.group.move".to_string(),
                Value::Object(params),
                TextMode::Jsonish,
            ))
        }
        "help" => {
            print_command_help("workspace-group");
            std::process::exit(0);
        }
        other => bail!("Unknown workspace-group subcommand: {other}"),
    }
}

fn workspace_group_target_params(command: &[String]) -> Result<Map<String, Value>> {
    let group = option_value(command, "--group")
        .or_else(|| option_value(command, "--group-id"))
        .or_else(|| workspace_group_positional(command, 2))
        .context("workspace-group command requires a group id")?;
    let mut params = Map::new();
    params.insert("group_id".to_string(), scalar_value(&group));
    Ok(params)
}

fn workspace_group_positional(command: &[String], start: usize) -> Option<String> {
    let value_flags = [
        "--name",
        "--cwd",
        "--window",
        "--from",
        "--group",
        "--group-id",
        "--workspace",
        "--workspace-id",
        "--placement",
        "--hex",
        "--color",
        "--symbol",
        "--icon",
        "--index",
        "--to-index",
        "--before-group",
        "--after-group",
    ];
    let mut index = start;
    while index < command.len() {
        let arg = &command[index];
        if arg == "--" {
            return command.get(index + 1).cloned();
        }
        if value_flags.contains(&arg.as_str()) {
            index += 2;
            continue;
        }
        if arg.starts_with('-') {
            index += 1;
            continue;
        }
        return Some(arg.clone());
    }
    None
}

fn open_command_to_request(command: &[String]) -> Result<(String, Value, TextMode)> {
    let targets = open_targets(command)?;
    if targets.is_empty() {
        bail!("open requires at least one path or URL");
    }
    let mut params = Map::new();
    params.insert("targets".to_string(), Value::Array(targets));
    collect_option(command, "--workspace", "workspace_id", &mut params)?;
    collect_option(command, "--workspace-id", "workspace_id", &mut params)?;
    collect_option(command, "--surface", "surface_id", &mut params)?;
    collect_option(command, "--surface-id", "surface_id", &mut params)?;
    collect_option(command, "--panel", "surface_id", &mut params)?;
    collect_option(command, "--pane", "pane_id", &mut params)?;
    collect_option(command, "--window", "window_id", &mut params)?;
    if command_has_flag(command, "--no-focus") {
        params.insert("focus".to_string(), json!(false));
    } else if let Some(focus) = option_value(command, "--focus") {
        params.insert("focus".to_string(), scalar_value(&focus));
    }
    Ok((
        "open.targets".to_string(),
        Value::Object(params),
        TextMode::Jsonish,
    ))
}

fn auth_command_to_request(command: &[String]) -> Result<(String, Value, TextMode)> {
    let name = command.first().map(String::as_str).unwrap_or("auth");
    let (subcommand, args): (&str, &[String]) = match name {
        "login" => ("login", &command[1..]),
        "logout" => ("logout", &command[1..]),
        "auth" => (
            command.get(1).map(String::as_str).unwrap_or("status"),
            if command.len() > 2 {
                &command[2..]
            } else {
                &[]
            },
        ),
        _ => unreachable!("auth_command_to_request called for non-auth command"),
    };

    match subcommand {
        "status" => {
            if !args.is_empty() {
                bail!("auth status does not take positional arguments");
            }
            Ok(("auth.status".to_string(), json!({}), TextMode::AuthStatus))
        }
        "login" => {
            let mut params = Map::new();
            let mut index = 0;
            while index < args.len() {
                match args[index].as_str() {
                    "--timeout" | "--timeout-seconds" => {
                        index += 1;
                        let raw = args.get(index).context("--timeout requires a value")?;
                        let timeout = raw
                            .parse::<f64>()
                            .with_context(|| format!("invalid auth login timeout: {raw}"))?;
                        params.insert("timeout_seconds".to_string(), json!(timeout));
                    }
                    other if other.starts_with('-') => {
                        bail!("auth login: unknown flag '{other}'");
                    }
                    other => {
                        bail!("auth login: unexpected argument '{other}'");
                    }
                }
                index += 1;
            }
            Ok((
                "auth.begin_sign_in".to_string(),
                Value::Object(params),
                TextMode::AuthLogin,
            ))
        }
        "logout" => {
            if !args.is_empty() {
                bail!("auth logout does not take positional arguments");
            }
            Ok(("auth.sign_out".to_string(), json!({}), TextMode::AuthLogout))
        }
        "team" | "select-team" => {
            let team_id = args
                .first()
                .context("auth team requires a team ID or `none`")?;
            if args.len() > 1 {
                bail!("auth team takes exactly one team ID or `none`");
            }
            let team_id = if matches!(
                team_id.trim().to_ascii_lowercase().as_str(),
                "none" | "clear" | "null"
            ) {
                Value::Null
            } else {
                json!(team_id)
            };
            Ok((
                "auth.team.select".to_string(),
                json!({"team_id": team_id}),
                TextMode::AuthStatus,
            ))
        }
        other => {
            bail!("Unknown auth subcommand '{other}'. Usage: cmux auth <status|login|logout|team>")
        }
    }
}

fn feed_command_to_request(command: &[String]) -> Result<(String, Value, TextMode)> {
    let subcommand = command.get(1).map(String::as_str).unwrap_or("list");
    match subcommand {
        "list" => {
            let mut params = Map::new();
            let mut index = 2;
            while index < command.len() {
                match command[index].as_str() {
                    "--pending-only" | "--pending" => {
                        params.insert("pending_only".to_string(), json!(true));
                    }
                    other => bail!("feed list: unknown argument '{other}'"),
                }
                index += 1;
            }
            Ok((
                "feed.list".to_string(),
                Value::Object(params),
                TextMode::FeedList,
            ))
        }
        "clear" => {
            let mut index = 2;
            while index < command.len() {
                match command[index].as_str() {
                    "--yes" | "-y" => {}
                    other => bail!("feed clear: unknown argument '{other}'"),
                }
                index += 1;
            }
            Ok(("feed.clear".to_string(), json!({}), TextMode::FeedClear))
        }
        "tui" => Ok(("feed.list".to_string(), json!({}), TextMode::FeedList)),
        "help" => {
            print_command_help("feed");
            std::process::exit(0);
        }
        other => bail!("Unknown feed subcommand: {other}"),
    }
}

fn hook_command_to_request(command: &[String]) -> Result<(String, Value, TextMode)> {
    let (source, event_name) = hook_source_and_event(command)?;
    let event = normalized_hook_event_from_stdin(&source, event_name.as_deref())?;
    Ok((
        "feed.push".to_string(),
        json!({ "event": event }),
        TextMode::Jsonish,
    ))
}

fn hook_source_and_event(command: &[String]) -> Result<(String, Option<String>)> {
    match command.first().map(String::as_str) {
        Some("claude-hook") => {
            let args = positional_args_after_skipping_options(command, 1, &["--event"]);
            let event = option_value(command, "--event").or_else(|| args.first().cloned());
            Ok(("claude".to_string(), event))
        }
        Some("hooks") => {
            let sub = command.get(1).map(String::as_str).unwrap_or("help");
            if sub == "help" {
                print_command_help("hooks");
                std::process::exit(0);
            }
            if matches!(sub, "setup" | "uninstall" | "install") {
                bail!(
                    "hooks {sub} must name an agent or use --agent <name>; try `cmux hooks setup --agent claude`, `cmux hooks claude install`, or `cmux help hooks`"
                );
            }
            if sub == "feed" {
                let source = option_value(command, "--source")
                    .or_else(|| option_value(command, "--agent"))
                    .context("hooks feed requires --source <agent>")?;
                let args = positional_args_after_skipping_options(
                    command,
                    2,
                    &["--source", "--agent", "--event"],
                );
                let event = option_value(command, "--event").or_else(|| args.first().cloned());
                return Ok((normalize_hook_source(&source), event));
            }
            let args = positional_args_after_skipping_options(command, 2, &["--event"]);
            let event = option_value(command, "--event").or_else(|| args.first().cloned());
            Ok((normalize_hook_source(sub), event))
        }
        _ => bail!("unknown hook command"),
    }
}

fn normalized_hook_event_from_stdin(source: &str, event_name: Option<&str>) -> Result<Value> {
    let mut text = String::new();
    if !io::stdin().is_terminal() {
        io::stdin().read_to_string(&mut text)?;
    }
    let raw = if text.trim().is_empty() {
        json!({})
    } else {
        serde_json::from_str::<Value>(&text).context("hook payload must be JSON")?
    };
    let raw = raw.get("event").cloned().unwrap_or(raw);
    let mut object = match raw {
        Value::Object(object) => object,
        other => {
            let mut object = Map::new();
            object.insert("tool_input".to_string(), other);
            object
        }
    };

    copy_hook_alias(&mut object, "sessionId", "session_id");
    copy_hook_alias(&mut object, "workspaceId", "workspace_id");
    copy_hook_alias(&mut object, "surfaceId", "surface_id");
    copy_hook_alias(&mut object, "turnId", "turn_id");
    copy_hook_alias(&mut object, "hookEventName", "hook_event_name");
    copy_hook_alias(&mut object, "hookEvent", "hook_event_name");
    copy_hook_alias(&mut object, "eventName", "hook_event_name");
    copy_hook_alias(&mut object, "requestId", "_opencode_request_id");
    copy_hook_alias(&mut object, "request_id", "_opencode_request_id");

    if let Some(event_name) = event_name
        .and_then(normalize_hook_event_name)
        .or_else(|| hook_event_name_from_object(&object))
    {
        object.insert("hook_event_name".to_string(), json!(event_name));
    }
    if object.get("_source").is_none() {
        object.insert("_source".to_string(), json!(source));
    }
    if object.get("session_id").is_none() {
        let fallback = normalized_env("CMUX_SESSION_ID")
            .or_else(|| normalized_env("CLAUDE_SESSION_ID"))
            .unwrap_or_else(|| format!("{source}-cli-session"));
        object.insert("session_id".to_string(), json!(fallback));
    }
    if object.get("workspace_id").is_none() {
        if let Some(workspace_id) = normalized_env("CMUX_WORKSPACE_ID") {
            object.insert("workspace_id".to_string(), json!(workspace_id));
        }
    }
    if object.get("surface_id").is_none() {
        if let Some(surface_id) = normalized_surface_env() {
            object.insert("surface_id".to_string(), json!(surface_id));
        }
    }
    if object.get("cwd").is_none() {
        if let Ok(cwd) = std::env::current_dir() {
            object.insert("cwd".to_string(), json!(cwd.display().to_string()));
        }
    }
    if let Some(baseline) = record_hook_turn_baseline(source, &object) {
        object.insert("_cmux_last_turn_baseline".to_string(), baseline);
    }
    Ok(Value::Object(object))
}

fn record_hook_turn_baseline(source: &str, object: &Map<String, Value>) -> Option<Value> {
    let event_name = object.get("hook_event_name").and_then(Value::as_str)?;
    let preserve_existing = match event_name {
        "UserPromptSubmit" => false,
        "PreToolUse" => true,
        _ => return None,
    };
    let session_id = hook_object_string(object, "session_id")?;
    let workspace_id = hook_object_string(object, "workspace_id");
    let surface_id = hook_object_string(object, "surface_id");
    let turn_id = hook_object_string(object, "turn_id")
        .or_else(|| hook_object_string(object, "message_id"))
        .or_else(|| hook_object_string(object, "request_id"));
    let cwd = hook_object_string(object, "cwd")
        .map(PathBuf::from)
        .or_else(|| std::env::current_dir().ok())?;

    match diff_baseline::record_turn_baseline(
        source,
        &session_id,
        turn_id.as_deref(),
        &cwd,
        workspace_id.as_deref(),
        surface_id.as_deref(),
        preserve_existing,
    ) {
        Ok(Some(record)) => Some(json!({
            "recorded": true,
            "repo_root": record.repo_root.display().to_string(),
            "store_path": record.store_path.display().to_string(),
            "base_commit": record.base_commit,
            "replaced": record.replaced
        })),
        Ok(None) => Some(json!({"recorded": false, "reason": "missing_context_or_preserved"})),
        Err(err) => Some(json!({"recorded": false, "error": err.to_string()})),
    }
}

fn hook_object_string(object: &Map<String, Value>, key: &str) -> Option<String> {
    object
        .get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToString::to_string)
}

fn copy_hook_alias(object: &mut Map<String, Value>, from: &str, to: &str) {
    if object.get(to).is_none() {
        if let Some(value) = object.get(from).cloned() {
            object.insert(to.to_string(), value);
        }
    }
}

fn hook_event_name_from_object(object: &Map<String, Value>) -> Option<String> {
    ["hook_event_name", "type", "event", "event_name"]
        .iter()
        .filter_map(|key| object.get(*key).and_then(Value::as_str))
        .find_map(normalize_hook_event_name)
}

fn normalize_hook_event_name(value: &str) -> Option<String> {
    let normalized = value
        .chars()
        .filter(|ch| ch.is_ascii_alphanumeric())
        .flat_map(char::to_lowercase)
        .collect::<String>();
    let event = match normalized.as_str() {
        "permissionrequest" => "PermissionRequest",
        "askuserquestion" => "AskUserQuestion",
        "exitplanmode" => "ExitPlanMode",
        "pretooluse" => "PreToolUse",
        "posttooluse" => "PostToolUse",
        "notification" => "Notification",
        "userpromptsubmit" => "UserPromptSubmit",
        "sessionstart" => "SessionStart",
        "sessionend" => "SessionEnd",
        "stop" => "Stop",
        "subagentstop" => "SubagentStop",
        "todowrite" => "TodoWrite",
        _ => return None,
    };
    Some(event.to_string())
}

fn normalize_hook_source(value: &str) -> String {
    match value.trim().to_ascii_lowercase().replace('-', "_").as_str() {
        "claude_code" | "claudecode" => "claude".to_string(),
        "openai" => "codex".to_string(),
        other => other.to_string(),
    }
}

fn confirm_feed_clear_if_needed(command: &[String]) -> Result<bool> {
    if command.first().map(String::as_str) != Some("feed")
        || command.get(1).map(String::as_str) != Some("clear")
        || command_has_flag(command, "--yes")
        || command_has_flag(command, "-y")
    {
        return Ok(true);
    }
    if !io::stdin().is_terminal() {
        bail!("feed clear requires --yes when not run from an interactive terminal");
    }
    print!("Clear all Feed items? Type 'yes' to continue: ");
    io::stdout().flush()?;
    let mut answer = String::new();
    io::stdin().read_line(&mut answer)?;
    if answer.trim().eq_ignore_ascii_case("yes") {
        Ok(true)
    } else {
        println!("Feed clear cancelled.");
        Ok(false)
    }
}

struct FeedTuiOptions {
    once: bool,
    pending_only: bool,
}

fn run_feed_tui(socket_path: &str, command: &[String], json_output: bool) -> Result<()> {
    let options = parse_feed_tui_options(command)?;
    let params = json!({"pending_only": options.pending_only});
    if json_output {
        let value = call_socket(socket_path, "feed.list", params)?;
        println!("{}", serde_json::to_string(&value)?);
        return Ok(());
    }

    let interactive = !options.once && io::stdin().is_terminal() && io::stdout().is_terminal();
    let mut selected = 0_usize;
    loop {
        let value = call_socket(socket_path, "feed.list", params.clone())?;
        let items = feed_tui_items(&value);
        if items.is_empty() {
            selected = 0;
        } else if selected >= items.len() {
            selected = items.len().saturating_sub(1);
        }

        if interactive {
            print!("\x1b[2J\x1b[H");
        }
        print!("{}", render_feed_tui(&value, selected, interactive));
        io::stdout().flush()?;

        if !interactive {
            break;
        }

        print!("feed> ");
        io::stdout().flush()?;
        let mut input = String::new();
        if io::stdin().read_line(&mut input)? == 0 {
            break;
        }
        let input = input.trim();
        if input.is_empty() || matches!(input, "r" | "refresh") {
            continue;
        }
        match input {
            "q" | "quit" | "exit" => break,
            "j" | "down" | "n" => {
                if selected + 1 < items.len() {
                    selected += 1;
                }
            }
            "k" | "up" | "p" => {
                selected = selected.saturating_sub(1);
            }
            _ => {
                let Some(item) = items.get(selected) else {
                    continue;
                };
                let message = run_feed_tui_action(socket_path, item, input)?;
                println!("{message}");
                io::stdout().flush()?;
                thread::sleep(Duration::from_millis(700));
            }
        }
    }

    Ok(())
}

fn parse_feed_tui_options(command: &[String]) -> Result<FeedTuiOptions> {
    let mut options = FeedTuiOptions {
        once: false,
        pending_only: false,
    };
    let mut index = 2;
    while index < command.len() {
        match command[index].as_str() {
            "--once" => options.once = true,
            "--pending-only" | "--pending" => options.pending_only = true,
            "--all" => options.pending_only = false,
            "--json" => {}
            "--legacy" | "--opentui" => {}
            other => bail!("feed tui: unknown argument '{other}'"),
        }
        index += 1;
    }
    Ok(options)
}

fn feed_tui_items(value: &Value) -> Vec<Value> {
    value
        .get("items")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default()
}

fn render_feed_tui(value: &Value, selected: usize, interactive: bool) -> String {
    let items = feed_tui_items(value);
    let pending_count = items
        .iter()
        .filter(|item| item.get("status").and_then(Value::as_str) == Some("pending"))
        .count();
    let mut out = String::new();
    out.push_str("cmux Feed (Linux)\n");
    out.push_str(&format!(
        "{} item{}, {} pending\n",
        items.len(),
        if items.len() == 1 { "" } else { "s" },
        pending_count
    ));
    out.push('\n');

    if items.is_empty() {
        out.push_str("No feed items.\n");
    } else {
        for (index, item) in items.iter().enumerate() {
            let marker = if index == selected { ">" } else { " " };
            let status = item
                .get("status")
                .and_then(Value::as_str)
                .unwrap_or("unknown");
            let source = item.get("source").and_then(Value::as_str).unwrap_or("?");
            let kind = item.get("kind").and_then(Value::as_str).unwrap_or("?");
            let request_id = feed_tui_request_id(item).unwrap_or_else(|| {
                item.get("id")
                    .and_then(Value::as_str)
                    .unwrap_or("?")
                    .to_string()
            });
            let title = feed_tui_title(item);
            out.push_str(&format!(
                "{marker} {:>2}. {status:<9} {source:<8} {kind:<18} {title}\n",
                index + 1
            ));
            out.push_str(&format!("      request: {request_id}\n"));
            if let Some(workstream) = item.get("workstream_id").and_then(Value::as_str) {
                out.push_str(&format!("      workstream: {workstream}\n"));
            }
            if let Some(summary) = feed_tui_summary(item) {
                out.push_str(&format!("      {summary}\n"));
            }
            if index == selected {
                out.push_str(&format!("      {}\n", feed_tui_action_help(item)));
            }
        }
    }

    if interactive {
        out.push_str("\nUse j/k to move, r to refresh, q to quit.\n");
    } else {
        out.push_str(
            "\nRun in a terminal for interactive actions, or use `cmux feed tui --once` for this snapshot.\n",
        );
    }
    out
}

fn feed_tui_title(item: &Value) -> String {
    item.get("title")
        .and_then(Value::as_str)
        .or_else(|| item.get("tool_name").and_then(Value::as_str))
        .or_else(|| item.get("question_prompt").and_then(Value::as_str))
        .or_else(|| item.get("plan_summary").and_then(Value::as_str))
        .unwrap_or("")
        .to_string()
}

fn feed_tui_request_id(item: &Value) -> Option<String> {
    item.get("request_id")
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty())
        .map(ToString::to_string)
}

fn feed_tui_summary(item: &Value) -> Option<String> {
    match item.get("kind").and_then(Value::as_str).unwrap_or("") {
        "permissionRequest" => item
            .get("tool_input")
            .and_then(Value::as_str)
            .map(|input| format!("input: {}", truncate_one_line(input, 140))),
        "question" => {
            let prompt = item
                .get("question_prompt")
                .and_then(Value::as_str)
                .unwrap_or("");
            let options = item
                .get("question_options")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
                .enumerate()
                .filter_map(|(index, option)| {
                    feed_tui_question_option_label(option)
                        .map(|label| format!("{}={label}", index + 1))
                })
                .collect::<Vec<_>>()
                .join(", ");
            Some(if options.is_empty() {
                format!("question: {}", truncate_one_line(prompt, 140))
            } else {
                format!("question: {} [{}]", truncate_one_line(prompt, 100), options)
            })
        }
        "exitPlan" => item
            .get("plan_summary")
            .and_then(Value::as_str)
            .map(|plan| format!("plan: {}", truncate_one_line(plan, 140))),
        "toolUse" | "toolResult" => item
            .get("tool_input")
            .or_else(|| item.get("tool_result"))
            .and_then(Value::as_str)
            .map(|text| format!("details: {}", truncate_one_line(text, 140))),
        "userPrompt" => item
            .get("text")
            .and_then(Value::as_str)
            .map(|text| format!("prompt: {}", truncate_one_line(text, 140))),
        _ => None,
    }
}

fn feed_tui_action_help(item: &Value) -> &'static str {
    if item.get("status").and_then(Value::as_str) != Some("pending") {
        return "Actions: o jump, r refresh, q quit";
    }
    match item.get("kind").and_then(Value::as_str).unwrap_or("") {
        "permissionRequest" => {
            "Actions: a allow once, A always, l all tools, b bypass, d deny, o jump"
        }
        "exitPlan" => "Actions: u ultraplan, m manual, y auto, d deny, o jump",
        "question" => "Actions: 1-N answer, comma-list for multiple answers, o jump",
        _ => "Actions: o jump, r refresh, q quit",
    }
}

fn run_feed_tui_action(socket_path: &str, item: &Value, input: &str) -> Result<String> {
    if matches!(input, "o" | "open" | "jump") {
        let workstream_id = item
            .get("workstream_id")
            .and_then(Value::as_str)
            .context("selected feed item has no workstream_id")?;
        let result = call_socket(
            socket_path,
            "feed.jump",
            json!({"workstream_id": workstream_id}),
        )?;
        return Ok(
            if result
                .get("matched")
                .and_then(Value::as_bool)
                .unwrap_or(false)
            {
                format!("Jumped to workstream {workstream_id}.")
            } else {
                format!("No cmux workspace matched workstream {workstream_id}.")
            },
        );
    }

    if item.get("status").and_then(Value::as_str) != Some("pending") {
        return Ok("Selected feed item is not pending.".to_string());
    }
    let request_id =
        feed_tui_request_id(item).context("selected pending feed item has no request_id")?;
    match item.get("kind").and_then(Value::as_str).unwrap_or("") {
        "permissionRequest" => {
            let mode = match input {
                "a" | "once" | "allow" => "once",
                "A" | "always" => "always",
                "l" | "all" | "all-tools" => "all",
                "b" | "bypass" => "bypass",
                "d" | "deny" | "n" | "no" => "deny",
                _ => return Ok("Unknown permission action.".to_string()),
            };
            call_socket(
                socket_path,
                "feed.permission.reply",
                json!({"request_id": request_id, "mode": mode}),
            )?;
            Ok(format!("Sent permission decision: {mode}."))
        }
        "exitPlan" => {
            let mode = match input {
                "u" | "ultraplan" => "ultraplan",
                "m" | "manual" => "manual",
                "y" | "auto" | "autoaccept" | "auto-accept" => "autoAccept",
                "d" | "deny" | "n" | "no" => "deny",
                _ => return Ok("Unknown plan action.".to_string()),
            };
            call_socket(
                socket_path,
                "feed.exit_plan.reply",
                json!({"request_id": request_id, "mode": mode}),
            )?;
            Ok(format!("Sent plan decision: {mode}."))
        }
        "question" => {
            let selections = feed_tui_question_selections(item, input);
            if selections.is_empty() {
                return Ok("Unknown question action.".to_string());
            }
            call_socket(
                socket_path,
                "feed.question.reply",
                json!({"request_id": request_id, "selections": selections}),
            )?;
            Ok("Sent question answer.".to_string())
        }
        _ => Ok("Selected feed item is not actionable.".to_string()),
    }
}

fn feed_tui_question_selections(item: &Value, input: &str) -> Vec<String> {
    let options = item
        .get("question_options")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    input
        .split([',', ' '])
        .map(str::trim)
        .filter(|part| !part.is_empty())
        .filter_map(|part| {
            if let Ok(index) = part.parse::<usize>() {
                return index
                    .checked_sub(1)
                    .and_then(|index| options.get(index))
                    .and_then(feed_tui_question_option_label);
            }
            Some(part.to_string())
        })
        .collect()
}

fn feed_tui_question_option_label(option: &Value) -> Option<String> {
    option
        .get("label")
        .and_then(Value::as_str)
        .or_else(|| option.get("id").and_then(Value::as_str))
        .filter(|value| !value.trim().is_empty())
        .map(ToString::to_string)
}

fn truncate_one_line(value: &str, max_chars: usize) -> String {
    let mut text = value
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .collect::<Vec<_>>()
        .join(" ");
    if text.chars().count() <= max_chars {
        return text;
    }
    text = text.chars().take(max_chars.saturating_sub(3)).collect();
    text.push_str("...");
    text
}

fn markdown_command_to_request(command: &[String]) -> Result<(String, Value, TextMode)> {
    let sub_args = markdown_sub_args(command)?;
    let raw_path = sub_args
        .first()
        .filter(|path| !path.is_empty())
        .context("markdown open requires a file path. Usage: cmux markdown open <path>")?;
    if let Some(extra) = sub_args.get(1) {
        if extra.starts_with('-') {
            bail!("markdown open: unknown flag '{extra}'. Usage: cmux markdown open <path> [--workspace <id|ref|index>] [--surface <id|ref|index>] [--window <id|ref|index>] [--direction right|down|left|up] [--focus <true|false>] [--font-size <points>]");
        }
        bail!("markdown open: unexpected argument '{extra}'. Usage: cmux markdown open <path> [--workspace <id|ref|index>] [--surface <id|ref|index>] [--window <id|ref|index>] [--direction right|down|left|up] [--focus <true|false>] [--font-size <points>]");
    }

    let path = absolute_open_path(raw_path)?;
    let metadata = fs::metadata(&path)
        .with_context(|| format!("markdown open target does not exist: {raw_path}"))?;
    if !metadata.is_file() {
        bail!("markdown open target is not a file: {raw_path}");
    }

    let mut params = Map::new();
    params.insert(
        "path".to_string(),
        json!(path.to_string_lossy().to_string()),
    );
    params.insert(
        "direction".to_string(),
        json!(option_value(command, "--direction").unwrap_or_else(|| "right".to_string())),
    );
    if let Some(workspace) =
        option_value(command, "--workspace").or_else(|| option_value(command, "--workspace-id"))
    {
        params.insert("workspace_id".to_string(), scalar_value(&workspace));
    } else if option_value(command, "--window").is_none() {
        if let Some(workspace) = normalized_env("CMUX_WORKSPACE_ID") {
            params.insert("workspace_id".to_string(), json!(workspace));
        }
    }
    collect_option(command, "--surface", "surface_id", &mut params)?;
    collect_option(command, "--surface-id", "surface_id", &mut params)?;
    collect_option(command, "--pane", "pane_id", &mut params)?;
    collect_option(command, "--window", "window_id", &mut params)?;
    if let Some(focus) = option_value(command, "--focus") {
        params.insert("focus".to_string(), scalar_value(&focus));
    } else if command_has_flag(command, "--no-focus") {
        params.insert("focus".to_string(), json!(false));
    } else {
        params.insert("focus".to_string(), json!(false));
    }
    if let Some(font_size) = option_value(command, "--font-size") {
        params.insert(
            "font_size".to_string(),
            json!(parse_markdown_font_size(&font_size)?),
        );
    }

    Ok((
        "markdown.open".to_string(),
        Value::Object(params),
        TextMode::MarkdownOpen,
    ))
}

fn diff_command_to_request(command: &[String]) -> Result<(String, Value, TextMode)> {
    let options = parse_diff_options(command)?;
    let (diff_text, source_label) = diff_input_text(&options)?;
    let title = options
        .title
        .clone()
        .unwrap_or_else(|| format!("Diff - {source_label}"));
    let review_comments = diff_viewer::review_comments_from_cli(
        options.review_comments_file.as_deref(),
        options.review_comments_json.as_deref(),
    )
    .map_err(anyhow::Error::msg)?;
    let mut params = Map::new();
    params.insert("diff".to_string(), json!(diff_text));
    params.insert("title".to_string(), json!(title));
    params.insert("source_label".to_string(), json!(source_label));
    params.insert("font_size".to_string(), json!(options.font_size));
    if let Some(layout) = options.layout {
        params.insert("layout".to_string(), json!(layout));
    }
    params.insert(
        "comments".to_string(),
        Value::Array(
            review_comments
                .iter()
                .map(|comment| {
                    json!({
                        "id": comment.id,
                        "filePath": comment.file_path,
                        "side": comment.side,
                        "startLine": comment.start_line,
                        "endLine": comment.end_line,
                        "endSide": comment.end_side,
                        "lineText": comment.line_text,
                        "message": comment.message,
                        "submissionText": comment.submission_text,
                        "author": comment.author,
                        "createdAt": comment.created_at,
                        "outdated": comment.outdated,
                        "resolved": comment.resolved
                    })
                })
                .collect(),
        ),
    );
    params.insert("direction".to_string(), json!("right"));
    if let Some(workspace) = options.workspace {
        params.insert("workspace_id".to_string(), scalar_value(&workspace));
    } else if options.window.is_none() {
        if let Some(workspace) = normalized_env("CMUX_WORKSPACE_ID") {
            params.insert("workspace_id".to_string(), json!(workspace));
        }
    }
    if let Some(surface) = options.surface {
        params.insert("surface_id".to_string(), scalar_value(&surface));
    } else if options.window.is_none() {
        if let Some(surface) = normalized_surface_env() {
            params.insert("surface_id".to_string(), json!(surface));
        }
    }
    if let Some(pane) = options.pane {
        params.insert("pane_id".to_string(), scalar_value(&pane));
    }
    if let Some(window) = options.window {
        params.insert("window_id".to_string(), scalar_value(&window));
    }
    params.insert("focus".to_string(), json!(options.focus));

    Ok((
        "diff.open".to_string(),
        Value::Object(params),
        TextMode::DiffOpen,
    ))
}

#[derive(Default)]
struct DiffOptions {
    inputs: Vec<String>,
    source: Option<String>,
    cwd: Option<String>,
    base: Option<String>,
    workspace: Option<String>,
    surface: Option<String>,
    pane: Option<String>,
    window: Option<String>,
    title: Option<String>,
    layout: Option<String>,
    font_size: f64,
    review_comments_file: Option<String>,
    review_comments_json: Option<String>,
    focus: bool,
}

fn parse_diff_options(command: &[String]) -> Result<DiffOptions> {
    let mut options = DiffOptions {
        font_size: 10.0,
        ..DiffOptions::default()
    };
    let mut index = 1;
    while index < command.len() {
        let arg = &command[index];
        match arg.as_str() {
            "--json" => index += 1,
            "--source" => {
                index += 1;
                options.source = Some(
                    command
                        .get(index)
                        .context("diff --source requires a value")?
                        .clone(),
                );
                index += 1;
            }
            "--unstaged" => {
                options.source = Some("unstaged".to_string());
                index += 1;
            }
            "--staged" => {
                options.source = Some("staged".to_string());
                index += 1;
            }
            "--branch" => {
                options.source = Some("branch".to_string());
                index += 1;
            }
            "--last-turn" => {
                options.source = Some("last-turn".to_string());
                index += 1;
            }
            "--cwd" | "--repo" => {
                index += 1;
                options.cwd = Some(
                    command
                        .get(index)
                        .context("diff --cwd requires a path")?
                        .clone(),
                );
                index += 1;
            }
            "--base" => {
                index += 1;
                options.base = Some(
                    command
                        .get(index)
                        .context("diff --base requires a ref")?
                        .clone(),
                );
                index += 1;
            }
            "--workspace" | "--workspace-id" => {
                index += 1;
                options.workspace = Some(
                    command
                        .get(index)
                        .context("diff --workspace requires a value")?
                        .clone(),
                );
                index += 1;
            }
            "--surface" | "--surface-id" | "--tab" | "--tab-id" => {
                index += 1;
                options.surface = Some(
                    command
                        .get(index)
                        .context("diff --surface requires a value")?
                        .clone(),
                );
                index += 1;
            }
            "--pane" | "--pane-id" => {
                index += 1;
                options.pane = Some(
                    command
                        .get(index)
                        .context("diff --pane requires a value")?
                        .clone(),
                );
                index += 1;
            }
            "--window" | "--window-id" => {
                index += 1;
                options.window = Some(
                    command
                        .get(index)
                        .context("diff --window requires a value")?
                        .clone(),
                );
                index += 1;
            }
            "--focus" => {
                index += 1;
                let raw = command
                    .get(index)
                    .context("diff --focus requires true|false")?;
                options.focus = parse_bool_cli(raw).context("diff --focus must be true|false")?;
                index += 1;
            }
            "--no-focus" => {
                options.focus = false;
                index += 1;
            }
            "--title" => {
                index += 1;
                options.title = Some(
                    command
                        .get(index)
                        .context("diff --title requires text")?
                        .clone(),
                );
                index += 1;
            }
            "--layout" => {
                index += 1;
                let layout = command
                    .get(index)
                    .context("diff --layout requires split or unified")?
                    .clone();
                if !matches!(layout.as_str(), "split" | "unified") {
                    bail!("diff --layout must be split or unified");
                }
                options.layout = Some(layout);
                index += 1;
            }
            "--font-size" => {
                index += 1;
                let raw = command
                    .get(index)
                    .context("diff --font-size requires points")?;
                options.font_size = parse_diff_font_size(raw)?;
                index += 1;
            }
            "--comments" | "--review-comments" => {
                index += 1;
                options.review_comments_file = Some(
                    command
                        .get(index)
                        .context("diff --comments requires a JSON file path")?
                        .clone(),
                );
                index += 1;
            }
            "--comments-json" | "--review-comments-json" => {
                index += 1;
                options.review_comments_json = Some(
                    command
                        .get(index)
                        .context("diff --comments-json requires a JSON value")?
                        .clone(),
                );
                index += 1;
            }
            "--" => {
                options
                    .inputs
                    .extend(command.iter().skip(index + 1).cloned());
                break;
            }
            value if value.starts_with('-') && value != "-" => {
                bail!("diff: unknown flag '{value}'. Usage: cmux diff [patch-file|-] [options]");
            }
            value => {
                options.inputs.push(value.to_string());
                index += 1;
            }
        }
    }
    if options.inputs.len() > 1 {
        bail!("diff accepts at most one patch file. Usage: cmux diff [patch-file|-] [options]");
    }
    if options.source.is_some() && !options.inputs.is_empty() {
        bail!("diff accepts either a patch file or a git source, not both");
    }
    Ok(options)
}

fn diff_input_text(options: &DiffOptions) -> Result<(String, String)> {
    if let Some(source) = options.source.as_deref() {
        return diff_git_source_text(options, source);
    }
    if let Some(input) = options.inputs.first() {
        if input == "-" {
            let mut text = String::new();
            io::stdin().read_to_string(&mut text)?;
            if text.trim().is_empty() {
                bail!("diff stdin was empty");
            }
            return Ok((text, "stdin".to_string()));
        }
        let path = absolute_open_path(input)?;
        let text = fs::read_to_string(&path)
            .with_context(|| format!("failed to read diff patch {}", path.display()))?;
        if text.trim().is_empty() {
            bail!("diff patch is empty: {}", path.display());
        }
        return Ok((text, path.display().to_string()));
    }
    if !io::stdin().is_terminal() {
        let mut text = String::new();
        io::stdin().read_to_string(&mut text)?;
        if !text.trim().is_empty() {
            return Ok((text, "stdin".to_string()));
        }
    }
    bail!(
        "diff requires a patch file, '-', piped stdin, or --unstaged/--staged/--branch/--last-turn"
    );
}

fn diff_git_source_text(options: &DiffOptions, source: &str) -> Result<(String, String)> {
    match source {
        "unstaged" => run_git_diff(options, &["diff"], "unstaged"),
        "staged" => run_git_diff(options, &["diff", "--staged"], "staged"),
        "branch" => {
            let base = options.base.as_deref().unwrap_or("origin/HEAD");
            run_git_diff(options, &["diff", base], &format!("branch {base}"))
        }
        "last-turn" | "last" | "lastturn" => {
            let cwd = if let Some(cwd) = options.cwd.as_deref() {
                absolute_open_path(cwd)?
            } else {
                std::env::current_dir().context("failed to resolve current directory")?
            };
            let env_workspace = normalized_env("CMUX_WORKSPACE_ID");
            let env_surface = normalized_surface_env();
            let workspace = options.workspace.as_deref().or(env_workspace.as_deref());
            let surface = options.surface.as_deref().or(env_surface.as_deref());
            let diff = diff_baseline::last_turn_diff(&cwd, workspace, surface)?;
            Ok((diff.patch, diff.source_label))
        }
        other => bail!("unknown diff source: {other}"),
    }
}

fn run_git_diff(options: &DiffOptions, args: &[&str], label: &str) -> Result<(String, String)> {
    let mut command = Command::new("git");
    command.args(args);
    if let Some(cwd) = options.cwd.as_deref() {
        command.current_dir(absolute_open_path(cwd)?);
    }
    let output = command
        .output()
        .with_context(|| format!("failed to run git {}", args.join(" ")))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        bail!("git {} failed: {}", args.join(" "), stderr.trim());
    }
    let text = String::from_utf8(output.stdout).context("git diff output was not UTF-8")?;
    let rendered = if text.trim().is_empty() {
        format!("diff --git a/.cmux-empty-diff b/.cmux-empty-diff\n--- a/.cmux-empty-diff\n+++ b/.cmux-empty-diff\n@@ -1 +1 @@\n-No {label} changes\n+No {label} changes\n")
    } else {
        text
    };
    Ok((rendered, label.to_string()))
}

fn parse_diff_font_size(raw: &str) -> Result<f64> {
    let size = raw
        .trim()
        .parse::<f64>()
        .with_context(|| format!("invalid diff font size: {raw}"))?;
    if !(8.0..=48.0).contains(&size) {
        bail!("diff font size must be between 8 and 48 points");
    }
    Ok((size * 100.0).round() / 100.0)
}

fn parse_bool_cli(raw: &str) -> Option<bool> {
    match raw {
        "true" | "1" | "yes" | "on" => Some(true),
        "false" | "0" | "no" | "off" => Some(false),
        _ => None,
    }
}

fn markdown_sub_args(command: &[String]) -> Result<Vec<String>> {
    let args = markdown_positional_args(command)?;
    if args.first().map(String::as_str) == Some("open") {
        return Ok(args.into_iter().skip(1).collect());
    }
    if args.len() == 1 {
        return Ok(args);
    }
    if let Some(first) = args.first() {
        if first.starts_with('-') {
            bail!("markdown open: unknown flag '{first}'. Usage: cmux markdown open <path> [--workspace <id|ref|index>] [--surface <id|ref|index>] [--window <id|ref|index>] [--direction right|down|left|up] [--focus <true|false>] [--font-size <points>]");
        }
        if looks_like_path(first) || first.contains('.') {
            return Ok(args);
        }
        bail!("Unknown markdown subcommand: {first}. Usage: cmux markdown open <path>");
    }
    Ok(Vec::new())
}

fn markdown_positional_args(command: &[String]) -> Result<Vec<String>> {
    let value_flags = [
        "--workspace",
        "--workspace-id",
        "--surface",
        "--surface-id",
        "--pane",
        "--window",
        "--direction",
        "--focus",
        "--font-size",
    ];
    let mut out = Vec::new();
    let mut skip_next = false;
    let mut literal = false;
    for arg in command.iter().skip(1) {
        if literal {
            out.push(arg.clone());
            continue;
        }
        if skip_next {
            skip_next = false;
            continue;
        }
        if arg == "--" {
            literal = true;
            continue;
        }
        if value_flags.contains(&arg.as_str()) {
            skip_next = true;
            continue;
        }
        if arg == "--no-focus" || arg == "--json" {
            continue;
        }
        if arg.starts_with("--") {
            bail!("markdown open: unknown flag '{arg}'. Usage: cmux markdown open <path> [--workspace <id|ref|index>] [--surface <id|ref|index>] [--window <id|ref|index>] [--direction right|down|left|up] [--focus <true|false>] [--font-size <points>]");
        }
        out.push(arg.clone());
    }
    Ok(out)
}

fn parse_markdown_font_size(raw: &str) -> Result<f64> {
    let value = raw
        .parse::<f64>()
        .with_context(|| format!("invalid markdown font size: {raw}"))?;
    if !(8.0..=96.0).contains(&value) {
        bail!("markdown font size must be between 8 and 96 points");
    }
    Ok(value)
}

fn looks_like_path(value: &str) -> bool {
    value.starts_with('/')
        || value.starts_with("./")
        || value.starts_with("../")
        || value.starts_with("~/")
        || value == "~"
}

fn feedback_command_to_request(command: &[String]) -> Result<(String, Value, TextMode)> {
    if command.get(1).map(String::as_str) == Some("retry") {
        let mut limit = None;
        let mut index = 2;
        while index < command.len() {
            match command[index].as_str() {
                "--limit" => {
                    index += 1;
                    let raw = command.get(index).context("--limit requires a value")?;
                    let parsed = raw
                        .parse::<u64>()
                        .with_context(|| format!("invalid feedback retry limit: {raw}"))?;
                    limit = Some(parsed);
                }
                "--json" => {}
                other => {
                    bail!("feedback retry: unknown flag '{other}'. Known flags: --limit <count>")
                }
            }
            index += 1;
        }
        let mut params = Map::new();
        if let Some(limit) = limit {
            params.insert("limit".to_string(), json!(limit));
        }
        return Ok((
            "feedback.retry".to_string(),
            Value::Object(params),
            TextMode::Ok,
        ));
    }

    let mut email = None;
    let mut body = None;
    let mut image_paths = Vec::new();
    let mut index = 1;
    while index < command.len() {
        let arg = &command[index];
        match arg.as_str() {
            "--email" => {
                index += 1;
                email = Some(
                    command
                        .get(index)
                        .cloned()
                        .context("--email requires a value")?,
                );
            }
            "--body" => {
                index += 1;
                body = Some(
                    command
                        .get(index)
                        .cloned()
                        .context("--body requires a value")?,
                );
            }
            "--image" => {
                index += 1;
                let raw = command
                    .get(index)
                    .cloned()
                    .context("--image requires a value")?;
                image_paths.push(absolute_open_path(&raw)?.to_string_lossy().to_string());
            }
            "--" => {}
            "--json" => {}
            other => bail!(
                "feedback: unknown flag '{other}'. Known flags: --email <email>, --body <text>, --image <path>"
            ),
        }
        index += 1;
    }

    if email.is_none() && body.is_none() && image_paths.is_empty() {
        let mut params = Map::new();
        if let Some(workspace) = normalized_env("CMUX_WORKSPACE_ID") {
            params.insert("workspace_id".to_string(), json!(workspace));
            params.insert("activate".to_string(), json!(false));
        } else {
            params.insert("activate".to_string(), json!(true));
        }
        return Ok((
            "feedback.open".to_string(),
            Value::Object(params),
            TextMode::Ok,
        ));
    }

    let email = email
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .context("feedback requires --email <email> when sending feedback")?;
    let body = body
        .filter(|value| !value.trim().is_empty())
        .context("feedback requires --body <text> when sending feedback")?;
    Ok((
        "feedback.submit".to_string(),
        json!({
            "email": email,
            "body": body,
            "image_paths": image_paths
        }),
        TextMode::Ok,
    ))
}

fn settings_command_to_request(command: &[String]) -> Result<(String, Value, TextMode)> {
    let mut params = Map::new();
    let mut target = None;
    let mut index = 1;
    if command.get(index).map(String::as_str) == Some("open") {
        index += 1;
    }

    while index < command.len() {
        let arg = &command[index];
        match arg.as_str() {
            "--target" => {
                index += 1;
                let raw = command.get(index).context("--target requires a value")?;
                target = Some(canonical_settings_target(raw).ok_or_else(|| {
                    anyhow::anyhow!("Unknown settings target '{raw}'. Run 'cmux settings --help'.")
                })?);
            }
            "--workspace" | "--workspace-id" => {
                index += 1;
                params.insert(
                    "workspace_id".to_string(),
                    scalar_value(command.get(index).context("--workspace requires a value")?),
                );
            }
            "--surface" | "--surface-id" => {
                index += 1;
                params.insert(
                    "surface_id".to_string(),
                    scalar_value(command.get(index).context("--surface requires a value")?),
                );
            }
            "--pane" | "--pane-id" => {
                index += 1;
                params.insert(
                    "pane_id".to_string(),
                    scalar_value(command.get(index).context("--pane requires a value")?),
                );
            }
            "--window" | "--window-id" => {
                index += 1;
                params.insert(
                    "window_id".to_string(),
                    scalar_value(command.get(index).context("--window requires a value")?),
                );
            }
            "--focus" => {
                let value = command
                    .get(index + 1)
                    .filter(|value| !value.starts_with("--"));
                if let Some(value) = value {
                    index += 1;
                    params.insert("focus".to_string(), scalar_value(value));
                } else {
                    params.insert("focus".to_string(), json!(true));
                }
            }
            "--no-focus" => {
                params.insert("focus".to_string(), json!(false));
            }
            "--activate" => {
                let value = command
                    .get(index + 1)
                    .filter(|value| !value.starts_with("--"));
                if let Some(value) = value {
                    index += 1;
                    params.insert("activate".to_string(), scalar_value(value));
                } else {
                    params.insert("activate".to_string(), json!(true));
                }
            }
            "--no-activate" => {
                params.insert("activate".to_string(), json!(false));
            }
            "--json" => {}
            "--" => {}
            other if other.starts_with("--") => bail!(
                "settings open: unknown flag '{other}'. Usage: cmux settings open [target] [--target <target>] [--workspace <id|ref|index>] [--surface <id|ref|index>] [--pane <id|ref|index>] [--window <id|ref|index>] [--focus <true|false>] [--no-focus]"
            ),
            other if target.is_none() => {
                target = Some(canonical_settings_target(other).ok_or_else(|| {
                    anyhow::anyhow!("Unknown settings target '{other}'. Run 'cmux settings --help'.")
                })?);
            }
            other => bail!("settings open: unexpected argument '{other}'"),
        }
        index += 1;
    }

    if let Some(target) = target {
        params.insert("target".to_string(), json!(target));
    }
    if !params.contains_key("workspace_id") && !params.contains_key("window_id") {
        if let Some(workspace) = normalized_env("CMUX_WORKSPACE_ID") {
            params.insert("workspace_id".to_string(), json!(workspace));
        }
    }

    Ok((
        "settings.open".to_string(),
        Value::Object(params),
        TextMode::SettingsOpen,
    ))
}

fn shortcuts_command_to_request(command: &[String]) -> Result<(String, Value, TextMode)> {
    if let Some(unknown) = command.iter().skip(1).find(|arg| arg.as_str() != "--") {
        bail!("shortcuts: unknown flag '{unknown}'");
    }
    Ok((
        "settings.open".to_string(),
        json!({
            "target": "keyboardShortcuts",
            "activate": true,
        }),
        TextMode::Ok,
    ))
}

fn canonical_settings_target(raw: &str) -> Option<String> {
    let normalized = raw.trim().to_ascii_lowercase().replace('_', "-");
    let target = match normalized.as_str() {
        "account" => "account",
        "app" | "general" => "app",
        "terminal" => "terminal",
        "textbox" | "text-box" => "textBox",
        "mobile" => "mobile",
        "sidebar" | "sidebar-appearance" | "sidebarappearance" => "sidebarAppearance",
        "custom-sidebars" | "customsidebars" => "customSidebars",
        "beta-features" | "betafeatures" => "betaFeatures",
        "automation" => "automation",
        "browser" => "browser",
        "browser-import" | "browserimport" | "import-browser-data" => "browserImport",
        "global-hotkey" | "globalhotkey" | "hotkey" => "globalHotkey",
        "keyboard-shortcuts" | "keyboardshortcuts" | "shortcuts" | "keys" | "keybindings" => {
            "keyboardShortcuts"
        }
        "workspace-colors" | "workspacecolors" | "colors" => "workspaceColors",
        "cmux-json" | "cmuxjson" | "settings-json" | "settingsjson" | "json" | "file"
        | "settings-file" => "settingsJSON",
        "reset" => "reset",
        _ => return None,
    };
    Some(target.to_string())
}

fn remotes_command_to_request(command: &[String]) -> Result<(String, Value, TextMode)> {
    let sub = command.get(1).map(String::as_str).unwrap_or("list");
    match sub {
        "list" | "ls" => Ok(("remotes.list".to_string(), json!({}), TextMode::Jsonish)),
        "add" => {
            let mut name = None;
            let mut routes = Vec::new();
            let mut tag = None;
            let mut index = 2;
            while index < command.len() {
                match command[index].as_str() {
                    "--route" => {
                        index += 1;
                        routes.push(
                            command
                                .get(index)
                                .context("--route requires host:port")?
                                .to_string(),
                        );
                    }
                    "--tag" => {
                        index += 1;
                        tag = Some(
                            command
                                .get(index)
                                .context("--tag requires a value")?
                                .to_string(),
                        );
                    }
                    "--json" => {}
                    "--" => {
                        index += 1;
                        if name.is_none() {
                            name = command.get(index).cloned();
                        }
                    }
                    value if value.starts_with("--route=") => {
                        routes.push(value.trim_start_matches("--route=").to_string());
                    }
                    value if value.starts_with("--tag=") => {
                        tag = Some(value.trim_start_matches("--tag=").to_string());
                    }
                    value if value.starts_with('-') => bail!("unknown remotes add option: {value}"),
                    value if name.is_none() => name = Some(value.to_string()),
                    value => bail!("unexpected remotes add argument: {value}"),
                }
                index += 1;
            }
            let name = name.context("remotes add requires a name")?;
            let mut params = Map::new();
            params.insert("name".to_string(), json!(name));
            params.insert("routes".to_string(), json!(routes));
            if let Some(tag) = tag {
                params.insert("tag".to_string(), json!(tag));
            }
            Ok((
                "remotes.add".to_string(),
                Value::Object(params),
                TextMode::Jsonish,
            ))
        }
        "remove" | "rm" | "delete" => {
            let target = command
                .iter()
                .skip(2)
                .find(|arg| !arg.starts_with('-'))
                .context("remotes remove requires a name or deviceId")?
                .to_string();
            Ok((
                "remotes.remove".to_string(),
                json!({"target": target}),
                TextMode::Jsonish,
            ))
        }
        other => bail!(
            "Unknown remotes subcommand '{other}'. Usage: cmux remotes <list|add|remove> [options]"
        ),
    }
}

fn mobile_command_to_request(command: &[String]) -> Result<(String, Value, TextMode)> {
    let mut index = 1;
    let subcommand = command.get(index).map(String::as_str).unwrap_or("status");
    if subcommand == "host" {
        index += 1;
        let nested = command.get(index).map(String::as_str).unwrap_or("status");
        if nested == "status" {
            index += 1;
            let params = mobile_status_params(command, index)?;
            return Ok(("mobile.host.status".to_string(), params, TextMode::Jsonish));
        }
        if is_mobile_attach_ticket_subcommand(nested) {
            index += 1;
            let params = mobile_attach_ticket_params(command, index)?;
            return Ok((
                "mobile.attach_ticket.create".to_string(),
                params,
                TextMode::Jsonish,
            ));
        }
        bail!(
            "Unknown mobile host subcommand: {nested}. Usage: cmux mobile host status | cmux mobile attach-ticket create"
        );
    } else if matches!(subcommand, "status" | "host-status") {
        index += 1;
        let params = mobile_status_params(command, index)?;
        return Ok(("mobile.host.status".to_string(), params, TextMode::Jsonish));
    } else if is_mobile_workspace_subcommand(subcommand) {
        index += 1;
        return mobile_workspace_command_to_request(command, index);
    } else if is_mobile_terminal_subcommand(subcommand) {
        index += 1;
        return mobile_terminal_command_to_request(command, index);
    } else if is_mobile_chat_subcommand(subcommand) {
        index += 1;
        return mobile_chat_command_to_request(command, index);
    } else if is_mobile_attach_ticket_subcommand(subcommand) {
        index += 1;
        let params = mobile_attach_ticket_params(command, index)?;
        return Ok((
            "mobile.attach_ticket.create".to_string(),
            params,
            TextMode::Jsonish,
        ));
    } else if subcommand.starts_with("--") {
        index = 1;
        let params = mobile_status_params(command, index)?;
        return Ok(("mobile.host.status".to_string(), params, TextMode::Jsonish));
    } else {
        bail!(
            "Unknown mobile subcommand: {subcommand}. Usage: cmux mobile status | cmux mobile attach-ticket create"
        );
    }
}

fn is_mobile_attach_ticket_subcommand(value: &str) -> bool {
    matches!(
        value,
        "attach-ticket" | "attach_ticket" | "attach" | "ticket" | "pair" | "pairing-ticket"
    )
}

fn is_mobile_workspace_subcommand(value: &str) -> bool {
    matches!(value, "workspace" | "workspaces")
}

fn is_mobile_terminal_subcommand(value: &str) -> bool {
    matches!(value, "terminal" | "term" | "terminals")
}

fn is_mobile_chat_subcommand(value: &str) -> bool {
    matches!(value, "chat" | "chats" | "agent-chat" | "agent_chat")
}

fn mobile_status_params(command: &[String], mut index: usize) -> Result<Value> {
    let mut params = Map::new();
    while index < command.len() {
        let arg = &command[index];
        match arg.as_str() {
            "--workspace" | "--workspace-id" => {
                index += 1;
                params.insert(
                    "workspace_id".to_string(),
                    scalar_value(command.get(index).context("--workspace requires a value")?),
                );
            }
            "--window" | "--window-id" => {
                index += 1;
                params.insert(
                    "window_id".to_string(),
                    scalar_value(command.get(index).context("--window requires a value")?),
                );
            }
            "--json" => {}
            "--" => {}
            other => bail!(
                "mobile status: unknown argument '{other}'. Usage: cmux mobile status [--workspace <id|ref|index>] [--window <id|ref|index>]"
            ),
        }
        index += 1;
    }

    Ok(Value::Object(params))
}

fn mobile_workspace_command_to_request(
    command: &[String],
    mut index: usize,
) -> Result<(String, Value, TextMode)> {
    if matches!(
        command.get(index).map(String::as_str),
        Some("list" | "ls" | "all")
    ) {
        index += 1;
    }
    let mut params = Map::new();
    while index < command.len() {
        let arg = &command[index];
        match arg.as_str() {
            "--workspace" | "--workspace-id" => {
                index += 1;
                params.insert(
                    "workspace_id".to_string(),
                    scalar_value(command.get(index).context("--workspace requires a value")?),
                );
            }
            "--json" => {}
            "--" => {}
            other => bail!(
                "mobile workspace list: unknown argument '{other}'. Usage: cmux mobile workspace list [--workspace <id|ref|index>]"
            ),
        }
        index += 1;
    }
    Ok((
        "mobile.workspace.list".to_string(),
        Value::Object(params),
        TextMode::Jsonish,
    ))
}

fn mobile_chat_command_to_request(
    command: &[String],
    index: usize,
) -> Result<(String, Value, TextMode)> {
    let action = command.get(index).map(String::as_str).unwrap_or("sessions");
    let mut params = mobile_chat_params(command)?;
    match action {
        "sessions" | "list" | "ls" => Ok((
            "mobile.chat.sessions".to_string(),
            params,
            TextMode::Jsonish,
        )),
        "dump" => Ok(("chat.sessions.dump".to_string(), json!({}), TextMode::Jsonish)),
        "history" | "messages" => {
            mobile_chat_insert_session(&mut params, command, index + 1)?;
            Ok((
                "mobile.chat.history".to_string(),
                params,
                TextMode::Jsonish,
            ))
        }
        "send" => {
            let session_was_option = params
                .get("session_id")
                .is_some_and(|value| !value.is_null());
            mobile_chat_insert_session(&mut params, command, index + 1)?;
            mobile_chat_insert_text(&mut params, command, index + 1, session_was_option);
            let attachments = mobile_chat_attachments(command)?;
            if !attachments.is_empty() {
                params["attachments"] = json!(attachments);
            }
            Ok(("mobile.chat.send".to_string(), params, TextMode::Jsonish))
        }
        "interrupt" | "cancel" => {
            mobile_chat_insert_session(&mut params, command, index + 1)?;
            if command_has_flag(command, "--soft") {
                params["hard"] = json!(false);
            } else if command_has_flag(command, "--hard") && params.get("hard").is_none() {
                params["hard"] = json!(true);
            }
            Ok((
                "mobile.chat.interrupt".to_string(),
                params,
                TextMode::Jsonish,
            ))
        }
        "answer" => {
            let session_was_option = params
                .get("session_id")
                .is_some_and(|value| !value.is_null());
            mobile_chat_insert_session(&mut params, command, index + 1)?;
            if !params
                .get("option_index")
                .is_some_and(|value| !value.is_null())
            {
                let positionals =
                    positional_args_after_skipping_options(command, index + 1, mobile_chat_flags());
                let option_index = if session_was_option {
                    positionals.first()
                } else {
                    positionals.get(1)
                }
                .context("answer requires option index")?;
                params["option_index"] = scalar_value(option_index);
            }
            Ok(("mobile.chat.answer".to_string(), params, TextMode::Jsonish))
        }
        other => bail!(
            "Unknown mobile chat subcommand: {other}. Usage: cmux mobile chat <sessions|history|send|interrupt|answer|dump>"
        ),
    }
}

fn mobile_chat_params(command: &[String]) -> Result<Value> {
    option_params(
        command,
        &[
            ("--workspace", "workspace_id"),
            ("--workspace-id", "workspace_id"),
            ("--session", "session_id"),
            ("--session-id", "session_id"),
            ("--text", "text"),
            ("--submit-key", "submit_key"),
            ("--limit", "limit"),
            ("--before-seq", "before_seq"),
            ("--before", "before_seq"),
            ("--option-index", "option_index"),
            ("--option", "option_index"),
        ],
    )
}

fn mobile_chat_insert_session(params: &mut Value, command: &[String], start: usize) -> Result<()> {
    if params
        .get("session_id")
        .is_some_and(|value| !value.is_null())
    {
        return Ok(());
    }
    let session_id =
        first_positional_after(command, start).context("chat command requires session id")?;
    params["session_id"] = json!(session_id);
    Ok(())
}

fn mobile_chat_insert_text(
    params: &mut Value,
    command: &[String],
    start: usize,
    session_was_option: bool,
) {
    if params.get("text").is_some_and(|value| !value.is_null()) {
        return;
    }
    let positionals = positional_args_after_skipping_options(command, start, mobile_chat_flags());
    let text_parts = if session_was_option {
        positionals.as_slice()
    } else {
        &positionals[1.min(positionals.len())..]
    };
    let text = text_parts.join(" ").trim().to_string();
    if !text.is_empty() {
        params["text"] = json!(text);
    }
}

fn mobile_chat_attachments(command: &[String]) -> Result<Vec<Value>> {
    let mut paths = Vec::new();
    for flag in ["--image", "--file", "--attachment"] {
        paths.extend(all_option_values(command, flag));
    }
    paths
        .into_iter()
        .map(|path| {
            let bytes = fs::read(&path)
                .with_context(|| format!("failed to read chat attachment {path}"))?;
            if bytes.is_empty() {
                bail!("chat attachment was empty: {path}");
            }
            Ok(json!({
                "data_b64": base64::engine::general_purpose::STANDARD.encode(bytes),
                "format": image_format_from_path(&path)
            }))
        })
        .collect()
}

fn mobile_chat_flags() -> &'static [&'static str] {
    &[
        "--workspace",
        "--workspace-id",
        "--session",
        "--session-id",
        "--text",
        "--submit-key",
        "--limit",
        "--before-seq",
        "--before",
        "--option-index",
        "--option",
        "--image",
        "--file",
        "--attachment",
    ]
}

fn mobile_terminal_command_to_request(
    command: &[String],
    index: usize,
) -> Result<(String, Value, TextMode)> {
    let action = command.get(index).map(String::as_str).unwrap_or("replay");
    let mut params = mobile_terminal_params(command)?;
    match action {
        "create" | "new" => {
            if !params.get("title").is_some_and(|value| !value.is_null()) {
                if let Some(title) = first_positional_after(command, index + 1) {
                    params["title"] = json!(title);
                }
            }
            Ok((
                "mobile.terminal.create".to_string(),
                params,
                TextMode::Jsonish,
            ))
        }
        "input" | "send" => {
            mobile_terminal_insert_text(&mut params, command, index + 1, "input requires text")?;
            Ok((
                "mobile.terminal.input".to_string(),
                params,
                TextMode::Jsonish,
            ))
        }
        "paste" => {
            mobile_terminal_insert_text(&mut params, command, index + 1, "paste requires text")?;
            Ok((
                "mobile.terminal.paste".to_string(),
                params,
                TextMode::Jsonish,
            ))
        }
        "paste-image" | "paste_image" | "image" => {
            mobile_terminal_insert_image(&mut params, command, index + 1)?;
            Ok((
                "mobile.terminal.paste_image".to_string(),
                params,
                TextMode::Jsonish,
            ))
        }
        "replay" | "snapshot" => Ok((
            "mobile.terminal.replay".to_string(),
            params,
            TextMode::Jsonish,
        )),
        "viewport" | "size" => Ok((
            "mobile.terminal.viewport".to_string(),
            params,
            TextMode::Jsonish,
        )),
        "scroll" => {
            if !params
                .get("delta_lines")
                .is_some_and(|value| !value.is_null())
            {
                if let Some(delta) = first_positional_after(command, index + 1) {
                    params["delta_lines"] = scalar_value(&delta);
                }
            }
            Ok((
                "mobile.terminal.scroll".to_string(),
                params,
                TextMode::Jsonish,
            ))
        }
        "mouse" => {
            let positionals =
                positional_args_after_skipping_options(command, index + 1, mobile_terminal_flags());
            if !params.get("col").is_some_and(|value| !value.is_null()) {
                if let Some(col) = positionals.first() {
                    params["col"] = scalar_value(col);
                }
            }
            if !params.get("row").is_some_and(|value| !value.is_null()) {
                if let Some(row) = positionals.get(1) {
                    params["row"] = scalar_value(row);
                }
            }
            Ok((
                "mobile.terminal.mouse".to_string(),
                params,
                TextMode::Jsonish,
            ))
        }
        other => bail!(
            "Unknown mobile terminal subcommand: {other}. Usage: cmux mobile terminal <create|input|paste|paste-image|replay|viewport|scroll|mouse>"
        ),
    }
}

fn mobile_terminal_params(command: &[String]) -> Result<Value> {
    option_params(
        command,
        &[
            ("--workspace", "workspace_id"),
            ("--workspace-id", "workspace_id"),
            ("--pane", "pane_id"),
            ("--pane-id", "pane_id"),
            ("--terminal", "terminal_id"),
            ("--terminal-id", "terminal_id"),
            ("--surface", "surface_id"),
            ("--surface-id", "surface_id"),
            ("--title", "title"),
            ("--command", "command"),
            ("--initial-command", "initial_command"),
            ("--text", "text"),
            ("--submit-key", "submit_key"),
            ("--image-base64", "image_base64"),
            ("--image_base64", "image_base64"),
            ("--image-format", "image_format"),
            ("--image_format", "image_format"),
            ("--format", "image_format"),
            ("--file", "image_path"),
            ("--path", "image_path"),
            ("--delta-lines", "delta_lines"),
            ("--delta", "delta_lines"),
            ("--max-scrollback-rows", "max_scrollback_rows"),
            ("--col", "col"),
            ("--column", "col"),
            ("--row", "row"),
        ],
    )
}

fn mobile_terminal_insert_text(
    params: &mut Value,
    command: &[String],
    start: usize,
    missing_message: &'static str,
) -> Result<()> {
    if params.get("text").is_some_and(|value| !value.is_null()) {
        return Ok(());
    }
    let text = positional_args_after_skipping_options(command, start, mobile_terminal_flags())
        .join(" ")
        .trim()
        .to_string();
    if text.is_empty() {
        bail!("{missing_message}");
    }
    params["text"] = json!(text);
    Ok(())
}

fn mobile_terminal_insert_image(
    params: &mut Value,
    command: &[String],
    start: usize,
) -> Result<()> {
    if params
        .get("image_base64")
        .is_some_and(|value| !value.is_null())
    {
        return Ok(());
    }

    let path = params
        .get("image_path")
        .and_then(Value::as_str)
        .map(ToString::to_string)
        .or_else(|| first_positional_after(command, start))
        .context("paste-image requires --file <path> or --image-base64 <data>")?;
    let bytes = fs::read(&path).with_context(|| format!("failed to read image file {path}"))?;
    if bytes.is_empty() {
        bail!("image file was empty: {path}");
    }
    params["image_base64"] = json!(base64::engine::general_purpose::STANDARD.encode(bytes));
    if !params
        .get("image_format")
        .is_some_and(|value| !value.is_null())
    {
        params["image_format"] = json!(image_format_from_path(&path));
    }
    Ok(())
}

fn image_format_from_path(path: &str) -> String {
    Path::new(path)
        .extension()
        .and_then(|extension| extension.to_str())
        .map(str::trim)
        .filter(|extension| !extension.is_empty())
        .unwrap_or("png")
        .trim_start_matches('.')
        .to_ascii_lowercase()
}

fn mobile_terminal_flags() -> &'static [&'static str] {
    &[
        "--workspace",
        "--workspace-id",
        "--pane",
        "--pane-id",
        "--terminal",
        "--terminal-id",
        "--surface",
        "--surface-id",
        "--title",
        "--command",
        "--initial-command",
        "--text",
        "--submit-key",
        "--image-base64",
        "--image_base64",
        "--image-format",
        "--image_format",
        "--format",
        "--file",
        "--path",
        "--delta-lines",
        "--delta",
        "--max-scrollback-rows",
        "--col",
        "--column",
        "--row",
    ]
}

fn mobile_attach_ticket_params(command: &[String], mut index: usize) -> Result<Value> {
    if matches!(
        command.get(index).map(String::as_str),
        Some("create" | "new" | "mint")
    ) {
        index += 1;
    }

    let mut params = Map::new();
    while index < command.len() {
        let arg = &command[index];
        match arg.as_str() {
            "--scope" => {
                index += 1;
                params.insert(
                    "scope".to_string(),
                    scalar_value(command.get(index).context("--scope requires a value")?),
                );
            }
            "--ttl" | "--ttl-seconds" | "--ttl_seconds" => {
                index += 1;
                params.insert(
                    "ttl_seconds".to_string(),
                    scalar_value(command.get(index).context("--ttl-seconds requires a value")?),
                );
            }
            "--route-id" | "--route_id" => {
                index += 1;
                params.insert(
                    "route_id".to_string(),
                    scalar_value(command.get(index).context("--route-id requires a value")?),
                );
            }
            "--route-kind" | "--route_kind" => {
                index += 1;
                params.insert(
                    "route_kind".to_string(),
                    scalar_value(command.get(index).context("--route-kind requires a value")?),
                );
            }
            "--workspace" | "--workspace-id" => {
                index += 1;
                params.insert(
                    "workspace_id".to_string(),
                    scalar_value(command.get(index).context("--workspace requires a value")?),
                );
            }
            "--terminal" | "--terminal-id" => {
                index += 1;
                params.insert(
                    "terminal_id".to_string(),
                    scalar_value(command.get(index).context("--terminal requires a value")?),
                );
            }
            "--surface" | "--surface-id" => {
                index += 1;
                params.insert(
                    "surface_id".to_string(),
                    scalar_value(command.get(index).context("--surface requires a value")?),
                );
            }
            "--json" => {}
            "--" => {}
            other => bail!(
                "mobile attach-ticket: unknown argument '{other}'. Usage: cmux mobile attach-ticket create [--scope <linux|mac|workspace>] [--route-id <id>] [--route-kind <kind>] [--ttl-seconds <seconds>] [--workspace <id|ref|index>] [--terminal <id|ref|index>]"
            ),
        }
        index += 1;
    }

    Ok(Value::Object(params))
}

fn open_targets(command: &[String]) -> Result<Vec<Value>> {
    let mut targets = Vec::new();
    let value_flags = [
        "--workspace",
        "--workspace-id",
        "--surface",
        "--surface-id",
        "--pane",
        "--window",
        "--focus",
    ];
    let mut skip_next = false;
    let mut literal = false;
    for arg in command.iter().skip(1) {
        if literal {
            targets.push(open_target(arg)?);
            continue;
        }
        if skip_next {
            skip_next = false;
            continue;
        }
        if arg == "--" {
            literal = true;
            continue;
        }
        if value_flags.contains(&arg.as_str()) {
            skip_next = true;
            continue;
        }
        if arg == "--no-focus" || arg == "--json" {
            continue;
        }
        if arg.starts_with('-') {
            bail!("open: unknown flag {arg}");
        }
        targets.push(open_target(arg)?);
    }
    Ok(targets)
}

fn open_target(raw: &str) -> Result<Value> {
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

fn looks_like_open_target(value: &str) -> bool {
    is_cmux_url_target(value) || is_url_target(value) || Path::new(value).exists()
}

fn renderer_command_to_request(command: &[String]) -> Result<(String, Value, TextMode)> {
    let subcommand = command
        .get(1)
        .filter(|value| !value.starts_with('-'))
        .map(String::as_str)
        .unwrap_or("snapshot");
    let (method, params) = match subcommand {
        "diagnostics" | "doctor" => ("renderer.diagnostics", renderer_backend_params(command)?),
        "snapshot" | "state" => ("renderer.snapshot", renderer_snapshot_params(command)?),
        "apply-size" | "resize" => ("renderer.apply_size", renderer_apply_size_params(command)?),
        other => bail!("unknown renderer subcommand: {other}"),
    };
    Ok((method.to_string(), params, TextMode::Jsonish))
}

fn renderer_apply_size_params(command: &[String]) -> Result<Value> {
    let mut params = Map::new();
    let mut index = 2;
    while index < command.len() {
        let arg = &command[index];
        match arg.as_str() {
            "--pane" | "--pane-id" => {
                let value = renderer_apply_size_option_value(command, &mut index, arg)?;
                params.insert("pane_id".to_string(), json!(value));
            }
            "--cols" | "--columns" => {
                let value = renderer_apply_size_positive_u32(command, &mut index, arg)?;
                params.insert("cols".to_string(), json!(value));
            }
            "--rows" => {
                let value = renderer_apply_size_positive_u32(command, &mut index, arg)?;
                params.insert("rows".to_string(), json!(value));
            }
            "--pixel-width" | "--width" => {
                let value = renderer_apply_size_positive_u32(command, &mut index, arg)?;
                params.insert("pixel_width".to_string(), json!(value));
            }
            "--pixel-height" | "--height" => {
                let value = renderer_apply_size_positive_u32(command, &mut index, arg)?;
                params.insert("pixel_height".to_string(), json!(value));
            }
            "--attachment" | "--attachment-id" => {
                let value = renderer_apply_size_option_value(command, &mut index, arg)?;
                params.insert("attachment_id".to_string(), json!(value));
            }
            "--client" | "--client-id" => {
                let value = renderer_apply_size_option_value(command, &mut index, arg)?;
                params.insert("client_id".to_string(), json!(value));
            }
            "--json" => {}
            "--" => {
                if let Some(extra) = command.get(index + 1) {
                    bail!("renderer apply-size: unexpected positional argument '{extra}'");
                }
                break;
            }
            other if other.starts_with('-') => {
                bail!("renderer apply-size: unknown argument '{other}'");
            }
            other => bail!("renderer apply-size: unexpected positional argument '{other}'"),
        }
        index += 1;
    }

    for (key, message) in [
        ("pane_id", "renderer apply-size requires --pane <pane>"),
        ("cols", "renderer apply-size requires --cols <n>"),
        ("rows", "renderer apply-size requires --rows <n>"),
        (
            "pixel_width",
            "renderer apply-size requires --pixel-width <px>",
        ),
        (
            "pixel_height",
            "renderer apply-size requires --pixel-height <px>",
        ),
    ] {
        if !params.contains_key(key) {
            bail!("{message}");
        }
    }

    Ok(Value::Object(params))
}

fn renderer_apply_size_option_value(
    command: &[String],
    index: &mut usize,
    flag: &str,
) -> Result<String> {
    *index += 1;
    command
        .get(*index)
        .filter(|value| !value.starts_with('-'))
        .cloned()
        .with_context(|| format!("renderer apply-size: {flag} requires a value"))
}

fn renderer_apply_size_positive_u32(
    command: &[String],
    index: &mut usize,
    flag: &str,
) -> Result<u32> {
    let value = renderer_apply_size_option_value(command, index, flag)?;
    let parsed = value
        .parse::<u32>()
        .with_context(|| format!("renderer apply-size: {flag} must be a positive integer"))?;
    if parsed == 0 {
        bail!("renderer apply-size: {flag} must be greater than 0");
    }
    Ok(parsed)
}

fn renderer_backend_params(command: &[String]) -> Result<Value> {
    let mut params = Map::new();
    if let Some(value) = required_option_value(command, "--backend")? {
        params.insert("backend".to_string(), scalar_value(&value));
    }
    Ok(Value::Object(params))
}

fn renderer_snapshot_params(command: &[String]) -> Result<Value> {
    let mut params = renderer_backend_params(command)?;
    if let Some(value) = required_option_value(command, "--window")?
        .or(required_option_value(command, "--window-id")?)
    {
        params["window_id"] = scalar_value(&value);
    }
    Ok(params)
}

fn workspace_create_params(command: &[String]) -> Result<Value> {
    let mut params = option_params(
        command,
        &[
            ("--name", "title"),
            ("--title", "title"),
            ("--description", "description"),
            ("--cwd", "cwd"),
            ("--command", "command"),
            ("--window", "window_id"),
            ("--focus", "focus"),
        ],
    )?;
    if let Some(layout) = option_value(command, "--layout") {
        params["layout"] = serde_json::from_str(&layout).context("--layout must be JSON")?;
    }
    let workspace_env = workspace_env_from_cli(command)?;
    if !workspace_env.is_empty() {
        params["workspace_env"] = Value::Object(workspace_env);
    }
    if params.get("focus").is_none() {
        params["focus"] = json!(false);
    }
    Ok(params)
}

fn select_workspace_params(command: &[String], positional_start: usize) -> Result<Value> {
    let mut params = option_params(command, &[("--workspace", "workspace_id")])?;
    add_env_default(&mut params, "workspace_id", "CMUX_WORKSPACE_ID");
    if params.get("workspace_id").is_none() {
        if let Some(workspace) = first_positional_after(command, positional_start) {
            params["workspace_id"] = json!(workspace);
        }
    }
    if params.get("workspace_id").is_none() {
        bail!("select-workspace requires --workspace <id|ref|index>");
    }
    Ok(params)
}

fn close_workspace_params(command: &[String]) -> Result<Value> {
    let mut params = option_params(command, &[("--workspace", "workspace_id")])?;
    if params.get("workspace_id").is_none() {
        if let Some(workspace) = first_positional_after(command, 1) {
            params["workspace_id"] = json!(workspace);
        }
    }
    if params.get("workspace_id").is_none() {
        bail!("close-workspace requires an explicit workspace ref or UUID");
    }
    Ok(params)
}

fn workspace_namespace_rename_params(command: &[String]) -> Result<Value> {
    let mut params = option_params(
        command,
        &[("--workspace", "workspace_id"), ("--title", "title")],
    )?;
    add_env_default(&mut params, "workspace_id", "CMUX_WORKSPACE_ID");
    let positionals =
        positional_args_after_skipping_options(command, 2, &["--workspace", "--title", "--window"]);
    let mut workspace_from_positional = false;
    if params.get("workspace_id").is_none() {
        if let Some(workspace) = positionals.first() {
            params["workspace_id"] = json!(workspace);
            workspace_from_positional = true;
        }
    }
    if params.get("title").is_none() {
        let title_parts = if workspace_from_positional && !positionals.is_empty() {
            &positionals[1..]
        } else {
            positionals.as_slice()
        };
        let title = if title_parts.is_empty() {
            trailing_title(command)
        } else {
            Some(title_parts.join(" "))
        };
        if let Some(title) = title
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty())
        {
            params["title"] = json!(title);
        }
    }
    if params.get("workspace_id").is_none() {
        bail!("workspace rename requires a workspace handle or --workspace");
    }
    if params.get("title").is_none() {
        bail!("workspace rename requires a title");
    }
    Ok(params)
}

fn workspace_env_params(command: &[String]) -> Result<Value> {
    let mut params = option_params(
        command,
        &[
            ("--workspace", "workspace_id"),
            ("--surface", "surface_id"),
            ("--tab", "surface_id"),
            ("--panel", "surface_id"),
            ("--pane", "pane_id"),
        ],
    )?;
    add_env_default(&mut params, "workspace_id", "CMUX_WORKSPACE_ID");
    add_env_surface_default(&mut params);
    if command_has_flag(command, "--mask") {
        params["mask"] = json!(true);
    }
    if params.get("workspace_id").is_none()
        && params.get("surface_id").is_none()
        && params.get("pane_id").is_none()
    {
        let positionals = positional_args_after_skipping_options(
            command,
            2,
            &["--workspace", "--surface", "--tab", "--pane"],
        );
        if let Some(workspace) = positionals.first() {
            params["workspace_id"] = json!(workspace);
        }
    }
    Ok(params)
}

fn workspace_remote_namespace_params(command: &[String]) -> Result<Value> {
    let mut params = option_params(
        command,
        &[
            ("--workspace", "workspace_id"),
            ("--surface", "surface_id"),
            ("--tab", "surface_id"),
            ("--panel", "surface_id"),
            ("--pane", "pane_id"),
        ],
    )?;
    add_env_default(&mut params, "workspace_id", "CMUX_WORKSPACE_ID");
    add_env_surface_default(&mut params);
    if command_has_flag(command, "--clear") {
        params["clear"] = json!(true);
    }
    if params.get("workspace_id").is_none()
        && params.get("surface_id").is_none()
        && params.get("pane_id").is_none()
    {
        let positionals = positional_args_after_skipping_options(
            command,
            2,
            &["--workspace", "--surface", "--tab", "--pane"],
        );
        if let Some(workspace) = positionals.first() {
            params["workspace_id"] = json!(workspace);
        }
    }
    Ok(params)
}

fn workspace_env_from_cli(command: &[String]) -> Result<Map<String, Value>> {
    let mut env = Map::new();
    for path in all_option_values(command, "--env-file") {
        let text = fs::read_to_string(&path)
            .with_context(|| format!("failed to read workspace env file {path}"))?;
        for (line_index, line) in text.lines().enumerate() {
            let trimmed = line.trim();
            if trimmed.is_empty() || trimmed.starts_with('#') {
                continue;
            }
            let entry = trimmed.strip_prefix("export ").unwrap_or(trimmed);
            let (key, value) = entry.split_once('=').with_context(|| {
                format!(
                    "invalid workspace env file entry at {}:{}; expected KEY=VALUE",
                    path,
                    line_index + 1
                )
            })?;
            insert_workspace_env_cli_value(&mut env, key, value)?;
        }
    }
    for entry in all_option_values(command, "--env") {
        let (key, value) = entry
            .split_once('=')
            .with_context(|| format!("invalid --env {entry:?}; expected KEY=VALUE"))?;
        insert_workspace_env_cli_value(&mut env, key, value)?;
    }
    Ok(env)
}

fn insert_workspace_env_cli_value(
    env: &mut Map<String, Value>,
    key: &str,
    value: &str,
) -> Result<()> {
    let key = key.trim();
    if key.is_empty() || key.contains('=') || key.contains('\0') {
        bail!("workspace env key must be non-empty and must not contain '=' or NUL");
    }
    let value = value.trim();
    if value.is_empty() || value.contains('\0') {
        return Ok(());
    }
    env.insert(key.to_string(), json!(value));
    Ok(())
}

fn reorder_workspace_params(command: &[String]) -> Result<Value> {
    let mut params = option_params(
        command,
        &[
            ("--workspace", "workspace_id"),
            ("--before", "before_workspace_id"),
            ("--before-workspace", "before_workspace_id"),
            ("--after", "after_workspace_id"),
            ("--after-workspace", "after_workspace_id"),
            ("--window", "window_id"),
            ("--index", "index"),
        ],
    )?;
    if params.get("workspace_id").is_none() {
        if let Some(workspace) = first_positional_after(command, 1) {
            params["workspace_id"] = json!(workspace);
        }
    }
    if params.get("workspace_id").is_none() {
        bail!("reorder-workspace requires --workspace <id|ref|index>");
    }
    if params.get("before_workspace_id").is_none()
        && params.get("after_workspace_id").is_none()
        && params.get("index").is_none()
    {
        bail!("reorder-workspace requires --index, --before, or --after");
    }
    if command_has_flag(command, "--dry-run") {
        params["dry_run"] = json!(true);
    }
    Ok(params)
}

fn reorder_workspaces_params(command: &[String]) -> Result<Value> {
    let mut params = option_params(command, &[("--order", "order"), ("--window", "window_id")])?;
    if params.get("order").is_none() {
        bail!("reorder-workspaces requires --order <id|ref|index>,<id|ref|index>,...");
    }
    if command_has_flag(command, "--dry-run") {
        params["dry_run"] = json!(true);
    }
    Ok(params)
}

fn move_workspace_to_window_params(command: &[String]) -> Result<Value> {
    let params = option_params(
        command,
        &[
            ("--workspace", "workspace_id"),
            ("--window", "window_id"),
            ("--focus", "focus"),
        ],
    )?;
    if params.get("workspace_id").is_none() {
        bail!("move-workspace-to-window requires --workspace");
    }
    if params.get("window_id").is_none() {
        bail!("move-workspace-to-window requires --window");
    }
    Ok(params)
}

fn new_surface_params(command: &[String]) -> Result<Value> {
    workspace_option_params(
        command,
        &[
            ("--type", "type"),
            ("--direction", "direction"),
            ("--workspace", "workspace_id"),
            ("--pane", "pane_id"),
            ("--window", "window_id"),
            ("--url", "url"),
            ("--focus", "focus"),
        ],
    )
}

fn surface_move_params(command: &[String]) -> Result<Value> {
    let mut params = option_params(
        command,
        &[
            ("--surface", "surface_id"),
            ("--panel", "surface_id"),
            ("--pane", "pane_id"),
            ("--workspace", "workspace_id"),
            ("--window", "window_id"),
            ("--before", "before_surface_id"),
            ("--before-surface", "before_surface_id"),
            ("--after", "after_surface_id"),
            ("--after-surface", "after_surface_id"),
            ("--index", "index"),
            ("--focus", "focus"),
        ],
    )?;
    add_env_default(&mut params, "workspace_id", "CMUX_WORKSPACE_ID");
    if params.get("surface_id").is_none() {
        if let Some(surface) = first_positional_after(command, 1) {
            params["surface_id"] = json!(surface);
        }
    }
    add_env_surface_default(&mut params);
    if params.get("surface_id").is_none() {
        bail!("move-surface requires --surface <id|ref|index>");
    }
    Ok(params)
}

fn split_off_params(command: &[String]) -> Result<Value> {
    let mut params = option_params(
        command,
        &[
            ("--surface", "surface_id"),
            ("--panel", "surface_id"),
            ("--workspace", "workspace_id"),
            ("--window", "window_id"),
            ("--focus", "focus"),
        ],
    )?;
    add_env_default(&mut params, "workspace_id", "CMUX_WORKSPACE_ID");
    add_env_surface_default(&mut params);
    if params.get("surface_id").is_none() {
        bail!("split-off requires --surface <id|ref|index>");
    }
    let direction = first_positional_after(command, 1)
        .context("split-off requires a direction: left, right, up, or down")?;
    if !matches!(direction.as_str(), "left" | "right" | "up" | "down") {
        bail!("split-off direction must be left, right, up, or down");
    }
    params["direction"] = json!(direction);
    Ok(params)
}

fn surface_reorder_params(command: &[String]) -> Result<Value> {
    let mut params = option_params(
        command,
        &[
            ("--surface", "surface_id"),
            ("--panel", "surface_id"),
            ("--workspace", "workspace_id"),
            ("--window", "window_id"),
            ("--before", "before_surface_id"),
            ("--before-surface", "before_surface_id"),
            ("--after", "after_surface_id"),
            ("--after-surface", "after_surface_id"),
            ("--index", "index"),
            ("--focus", "focus"),
        ],
    )?;
    add_env_default(&mut params, "workspace_id", "CMUX_WORKSPACE_ID");
    if params.get("surface_id").is_none() {
        if let Some(surface) = first_positional_after(command, 1) {
            params["surface_id"] = json!(surface);
        }
    }
    add_env_surface_default(&mut params);
    if params.get("surface_id").is_none() {
        bail!("reorder-surface requires --surface <id|ref|index>");
    }
    if params.get("before_surface_id").is_none()
        && params.get("after_surface_id").is_none()
        && params.get("index").is_none()
    {
        bail!("reorder-surface requires --index, --before, or --after");
    }
    Ok(params)
}

fn tab_action_params(command: &[String]) -> Result<Value> {
    let mut params = option_params(
        command,
        &[
            ("--action", "action"),
            ("--workspace", "workspace_id"),
            ("--window", "window_id"),
            ("--tab", "tab_id"),
            ("--surface", "surface_id"),
            ("--panel", "surface_id"),
            ("--title", "title"),
            ("--url", "url"),
            ("--focus", "focus"),
        ],
    )?;
    add_env_default(&mut params, "workspace_id", "CMUX_WORKSPACE_ID");
    add_env_default(&mut params, "tab_id", "CMUX_TAB_ID");
    add_env_surface_default(&mut params);
    if params.get("action").is_none() {
        if let Some(action) = first_positional_after(command, 1) {
            params["action"] = json!(action);
        }
    }
    if params.get("title").is_none() {
        if let Some(title) = trailing_title(command) {
            if params.get("action").and_then(Value::as_str) == Some("rename") {
                params["title"] = json!(title);
            }
        }
    }
    if params.get("action").is_none() {
        bail!("tab-action requires --action <name>");
    }
    Ok(params)
}

fn move_tab_to_new_workspace_params(command: &[String]) -> Result<Value> {
    if command_has_flag(command, "--action") {
        bail!("move-tab-to-new-workspace does not accept --action");
    }
    let mut params = option_params(
        command,
        &[
            ("--workspace", "workspace_id"),
            ("--window", "window_id"),
            ("--tab", "tab_id"),
            ("--surface", "surface_id"),
            ("--panel", "surface_id"),
            ("--title", "title"),
            ("--focus", "focus"),
        ],
    )?;
    add_env_default(&mut params, "workspace_id", "CMUX_WORKSPACE_ID");
    add_env_default(&mut params, "tab_id", "CMUX_TAB_ID");
    add_env_surface_default(&mut params);
    params["action"] = json!("move-to-new-workspace");
    Ok(params)
}

fn workspace_action_params(command: &[String]) -> Result<Value> {
    let mut params = option_params(
        command,
        &[
            ("--action", "action"),
            ("--workspace", "workspace_id"),
            ("--window", "window_id"),
            ("--title", "title"),
            ("--description", "description"),
            ("--color", "color"),
        ],
    )?;
    add_env_default(&mut params, "workspace_id", "CMUX_WORKSPACE_ID");
    if params.get("action").is_none() {
        if let Some(action) = first_positional_after(command, 1) {
            params["action"] = json!(action);
        }
    }
    let action = params
        .get("action")
        .and_then(Value::as_str)
        .map(|action| action.replace('-', "_"));
    if params.get("title").is_none() && action.as_deref() == Some("rename") {
        if let Some(title) = workspace_action_positional_payload(command, action.as_deref()) {
            params["title"] = json!(title);
        }
    }
    if params.get("description").is_none() && action.as_deref() == Some("set_description") {
        if let Some(description) = workspace_action_positional_payload(command, action.as_deref()) {
            params["description"] = json!(description);
        }
    }
    if params.get("color").is_none() && action.as_deref() == Some("set_color") {
        if let Some(color) = workspace_action_positional_payload(command, action.as_deref()) {
            params["color"] = json!(color);
        }
    }
    if params.get("action").is_none() {
        bail!("workspace-action requires --action <name>");
    }
    Ok(params)
}

fn workspace_action_positional_payload(command: &[String], action: Option<&str>) -> Option<String> {
    let mut args = positional_args_after_skipping_options(
        command,
        1,
        &[
            "--action",
            "--workspace",
            "--window",
            "--title",
            "--description",
            "--color",
        ],
    );
    if let (Some(action), Some(first)) = (action, args.first()) {
        if first.replace('-', "_") == action {
            args.remove(0);
        }
    }
    if args.is_empty() {
        trailing_title(command)
    } else {
        Some(args.join(" "))
    }
    .map(|value| value.trim().to_string())
    .filter(|value| !value.is_empty())
}

fn ssh_params(command: &[String]) -> Result<Value> {
    let destination = ssh_destination(command).context("ssh requires a destination host")?;
    let mut params = Map::new();
    params.insert("destination".to_string(), json!(destination));
    if let Some(name) = option_value(command, "--name") {
        params.insert("title".to_string(), json!(name));
    }
    if let Some(port) = option_value(command, "--port") {
        params.insert("port".to_string(), scalar_value(&port));
    }
    if let Some(identity) = option_value(command, "--identity") {
        params.insert("identity_file".to_string(), json!(identity));
    }
    let ssh_options = all_option_values(command, "--ssh-option");
    if !ssh_options.is_empty() {
        params.insert("ssh_options".to_string(), json!(ssh_options));
    }
    if let Some(forward_agent) = ssh_forward_agent_flag(command) {
        params.insert("forward_agent".to_string(), json!(forward_agent));
    }
    if let Ok(features) = std::env::var("GHOSTTY_SHELL_FEATURES") {
        params.insert("ghostty_shell_features".to_string(), json!(features));
    }
    Ok(Value::Object(params))
}

fn ssh_tmux_params(command: &[String]) -> Result<Value> {
    let mut destination = None;
    let mut params = Map::new();
    let mut index = 1;
    let mut literal = false;
    while index < command.len() {
        let arg = &command[index];
        if literal {
            if destination.is_none() {
                destination = Some(arg.clone());
            } else {
                bail!("ssh-tmux: unexpected extra argument '{arg}'");
            }
            index += 1;
            continue;
        }
        match arg.as_str() {
            "--" => {
                literal = true;
                index += 1;
            }
            "--json" => {
                index += 1;
            }
            "--id-format" => {
                index += 2;
            }
            "--port" => {
                index += 1;
                let raw = command
                    .get(index)
                    .context("ssh-tmux: --port requires a value")?;
                let port = raw
                    .parse::<i64>()
                    .ok()
                    .filter(|port| (1..=65_535).contains(port))
                    .context("ssh-tmux: --port must be 1-65535")?;
                params.insert("port".to_string(), json!(port));
                index += 1;
            }
            "--identity" | "--identity-file" => {
                index += 1;
                let identity = command
                    .get(index)
                    .context("ssh-tmux: --identity requires a path")?;
                params.insert("identity_file".to_string(), json!(identity));
                index += 1;
            }
            "--no-focus" => {
                params.insert("activate".to_string(), json!(false));
                index += 1;
            }
            "--live" | "--probe" => {
                params.insert("live".to_string(), json!(true));
                index += 1;
            }
            value if value.starts_with('-') => {
                bail!("ssh-tmux: destination must be <user@host> or an ssh alias. Use --port/--identity for SSH flags.");
            }
            value => {
                if destination.is_none() {
                    destination = Some(value.to_string());
                } else {
                    bail!("ssh-tmux: unexpected extra argument '{value}'");
                }
                index += 1;
            }
        }
    }
    let destination = destination
        .context("ssh-tmux requires a destination (example: cmux ssh-tmux user@host)")?;
    params.insert("host".to_string(), json!(destination));
    Ok(Value::Object(params))
}

fn run_ssh_tmux_command(socket: &str, options: &GlobalOptions) -> Result<()> {
    let params = ssh_tmux_params(&options.command)?;
    let result = run_ssh_tmux_flow(
        &params,
        |params| call_socket(socket, "remote.tmux.window", params.clone()),
        run_interactive_auth_ssh,
    )?;
    let formatted = format_ids(
        result,
        effective_id_format(&options.command, options.id_format)?,
    );
    if options.json || command_has_flag(&options.command, "--json") {
        println!("{}", serde_json::to_string(&formatted)?);
    } else {
        print_text_response("ssh-tmux", &formatted, TextMode::RemoteTmuxWindow)?;
    }
    Ok(())
}

fn run_ssh_tmux_flow<C, A>(params: &Value, mut call_socket: C, mut authenticate: A) -> Result<Value>
where
    C: FnMut(&Value) -> Result<Value>,
    A: FnMut(&[String], &str) -> Result<()>,
{
    let destination = params
        .get("host")
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty())
        .unwrap_or("unknown")
        .to_string();
    let mut did_authenticate = false;
    loop {
        let result = call_socket(params)?;
        if result
            .get("mirrored")
            .and_then(Value::as_bool)
            .unwrap_or(false)
        {
            return Ok(result);
        }
        if result
            .get("auth_required")
            .and_then(Value::as_bool)
            .unwrap_or(false)
        {
            if did_authenticate {
                bail!("ssh-tmux: authentication did not open the connection to {destination}");
            }
            let ssh_argv = remote_tmux_auth_ssh_argv(&result)?;
            authenticate(&ssh_argv, &destination)?;
            did_authenticate = true;
            continue;
        }
        bail!("ssh-tmux: unexpected response from cmux");
    }
}

fn remote_tmux_auth_ssh_argv(result: &Value) -> Result<Vec<String>> {
    let values = result
        .get("ssh_argv")
        .and_then(Value::as_array)
        .context("ssh-tmux: cmux did not return an ssh command for authentication")?;
    let argv = values
        .iter()
        .map(|value| {
            value
                .as_str()
                .map(ToString::to_string)
                .context("ssh-tmux: cmux returned a malformed ssh command")
        })
        .collect::<Result<Vec<_>>>()?;
    if argv.is_empty() {
        bail!("ssh-tmux: cmux did not return an ssh command for authentication");
    }
    Ok(argv)
}

fn run_interactive_auth_ssh(ssh_argv: &[String], destination: &str) -> Result<()> {
    validate_interactive_auth_ssh_argv(ssh_argv)?;
    if !io::stdin().is_terminal() {
        bail!(
            "ssh-tmux: {destination} needs interactive authentication, which requires a terminal. Run `cmux ssh-tmux {destination}` directly from an interactive shell."
        );
    }
    let status = Command::new(&ssh_argv[0])
        .args(&ssh_argv[1..])
        .stdin(Stdio::inherit())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .status()
        .with_context(|| format!("ssh-tmux: failed to launch ssh: {}", ssh_argv[0]))?;
    if !status.success() {
        let code = status
            .code()
            .map(|code| code.to_string())
            .unwrap_or_else(|| "signal".to_string());
        bail!("ssh-tmux: ssh authentication to {destination} failed (exit {code})");
    }
    Ok(())
}

fn validate_interactive_auth_ssh_argv(ssh_argv: &[String]) -> Result<()> {
    let executable = ssh_argv
        .first()
        .map(String::as_str)
        .filter(|value| !value.trim().is_empty())
        .context("ssh-tmux: cmux did not return an ssh command for authentication")?;
    if executable != "/usr/bin/ssh" {
        bail!("ssh-tmux: refusing to run a non-standard ssh path for authentication");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::{Cell, RefCell};

    fn theme_picker_payload_fixture() -> config::ThemeListPayload {
        config::ThemeListPayload {
            themes: vec![
                config::ThemeListEntry {
                    name: "Ghostty Base".to_string(),
                    current_light: true,
                    current_dark: true,
                },
                config::ThemeListEntry {
                    name: "Catppuccin Latte".to_string(),
                    current_light: false,
                    current_dark: false,
                },
                config::ThemeListEntry {
                    name: "Catppuccin Mocha".to_string(),
                    current_light: false,
                    current_dark: false,
                },
            ],
            current: config::ThemeSelection {
                raw_value: Some("Ghostty Base".to_string()),
                light: Some("Ghostty Base".to_string()),
                dark: Some("Ghostty Base".to_string()),
                source_path: Some("/tmp/ghostty/config".to_string()),
            },
            config_path: "/tmp/cmux/config.ghostty".to_string(),
        }
    }

    #[test]
    fn theme_picker_selection_accepts_number_name_and_cancel() {
        let payload = theme_picker_payload_fixture();

        assert_eq!(
            theme_picker_selection(&payload, "2").expect("number selection"),
            Some("Catppuccin Latte".to_string())
        );
        assert_eq!(
            theme_picker_selection(&payload, "catppuccin mocha").expect("name selection"),
            Some("Catppuccin Mocha".to_string())
        );
        assert_eq!(theme_picker_selection(&payload, "").unwrap(), None);
        assert_eq!(theme_picker_selection(&payload, "q").unwrap(), None);

        let err = theme_picker_selection(&payload, "9").expect_err("out of range");
        assert!(
            err.to_string().contains("out of range"),
            "unexpected error: {err}"
        );
    }

    #[test]
    fn themes_picker_text_marks_current_theme() {
        let payload = theme_picker_payload_fixture();
        let text = themes_picker_text(&payload);

        assert!(text.contains("Current light: Ghostty Base"));
        assert!(text.contains("Config: /tmp/cmux/config.ghostty"));
        assert!(text.contains(" 1. Ghostty Base  [light, dark]"));
        assert!(text.contains(" 3. Catppuccin Mocha"));
    }

    #[test]
    fn ssh_tmux_flow_retries_once_after_auth_required() {
        let calls = Cell::new(0);
        let auth_calls = RefCell::new(Vec::<(String, Vec<String>)>::new());
        let params = json!({"host": "auth.example"});
        let result = run_ssh_tmux_flow(
            &params,
            |_| {
                let next = calls.get() + 1;
                calls.set(next);
                if next == 1 {
                    Ok(json!({
                        "host": "auth.example",
                        "auth_required": true,
                        "ssh_argv": ["/usr/bin/ssh", "-T", "--", "auth.example", "true"]
                    }))
                } else {
                    Ok(json!({
                        "host": "auth.example",
                        "mirrored": true,
                        "window_id": "window-auth"
                    }))
                }
            },
            |argv, destination| {
                auth_calls
                    .borrow_mut()
                    .push((destination.to_string(), argv.to_vec()));
                Ok(())
            },
        )
        .expect("ssh-tmux flow");

        assert_eq!(calls.get(), 2);
        assert_eq!(result["mirrored"], true);
        let auth_calls = auth_calls.borrow();
        assert_eq!(auth_calls.len(), 1);
        assert_eq!(auth_calls[0].0, "auth.example");
        assert_eq!(auth_calls[0].1[0], "/usr/bin/ssh");
    }

    #[test]
    fn ssh_tmux_flow_refuses_repeated_auth_required() {
        let calls = Cell::new(0);
        let params = json!({"host": "still-auth.example"});
        let err = run_ssh_tmux_flow(
            &params,
            |_| {
                calls.set(calls.get() + 1);
                Ok(json!({
                    "host": "still-auth.example",
                    "auth_required": true,
                    "ssh_argv": ["/usr/bin/ssh", "-T", "--", "still-auth.example", "true"]
                }))
            },
            |_, _| Ok(()),
        )
        .expect_err("repeated auth should fail");

        assert_eq!(calls.get(), 2);
        assert!(
            err.to_string()
                .contains("authentication did not open the connection"),
            "unexpected error: {err}"
        );
    }

    #[test]
    fn ssh_tmux_auth_argv_must_use_system_ssh() {
        validate_interactive_auth_ssh_argv(&["/usr/bin/ssh".to_string(), "-T".to_string()])
            .expect("system ssh is accepted");
        let err = validate_interactive_auth_ssh_argv(&["/tmp/ssh".to_string()])
            .expect_err("non-standard ssh should fail");
        assert!(
            err.to_string()
                .contains("refusing to run a non-standard ssh path"),
            "unexpected error: {err}"
        );
    }

    fn cli_args(values: &[&str]) -> Vec<String> {
        values.iter().map(|value| value.to_string()).collect()
    }

    #[test]
    fn app_command_uses_explicit_global_socket_when_app_socket_is_absent() {
        let options = parse_global_options(cli_args(&[
            "--socket",
            "/tmp/cmux-global.sock",
            "app",
            "--script",
            "quit",
        ]))
        .expect("global options");

        assert_eq!(
            app_command_with_global_socket(&options),
            cli_args(&[
                "app",
                "--socket",
                "/tmp/cmux-global.sock",
                "--script",
                "quit",
            ])
        );
    }

    #[test]
    fn app_command_socket_option_overrides_global_socket() {
        let options = parse_global_options(cli_args(&[
            "--socket",
            "/tmp/cmux-global.sock",
            "app",
            "--socket",
            "/tmp/cmux-local.sock",
            "--script",
            "quit",
        ]))
        .expect("global options");

        assert_eq!(
            app_command_with_global_socket(&options),
            cli_args(&[
                "app",
                "--socket",
                "/tmp/cmux-local.sock",
                "--script",
                "quit",
            ])
        );
    }

    #[test]
    fn app_command_without_explicit_global_socket_stays_local() {
        let options =
            parse_global_options(cli_args(&["app", "--script", "quit"])).expect("global options");

        assert_eq!(
            app_command_with_global_socket(&options),
            cli_args(&["app", "--script", "quit"])
        );
    }

    #[test]
    fn socket_marker_paths_prefer_xdg_and_keep_legacy_tmp_as_fallback() {
        let paths = socket_marker_paths_from_env(
            Some("/run/user/1000/state"),
            Some("/home/me"),
            Path::new("/tmp/codex"),
        );

        assert_eq!(
            paths,
            vec![
                PathBuf::from("/run/user/1000/state/cmux/last-socket-path"),
                PathBuf::from("/home/me/.local/state/cmux/last-socket-path"),
                PathBuf::from("/tmp/codex/cmux/last-socket-path"),
                PathBuf::from("/tmp/cmux-last-socket-path"),
            ]
        );
    }

    #[test]
    fn read_socket_marker_skips_missing_and_empty_candidates() {
        let tmp = tempfile::tempdir().expect("socket marker tempdir");
        let empty = tmp.path().join("empty");
        let stale = tmp.path().join("stale");
        let valid = tmp.path().join("valid");
        let socket = tmp.path().join("cmux.sock");
        let _listener = std::os::unix::net::UnixListener::bind(&socket).expect("bind test socket");
        fs::write(&empty, "\n").expect("empty marker");
        fs::write(
            &stale,
            format!("{}\n", tmp.path().join("gone.sock").display()),
        )
        .expect("stale marker");
        fs::write(&valid, format!(" {} \n", socket.display())).expect("valid marker");

        assert_eq!(
            read_socket_marker(&[tmp.path().join("missing"), empty, stale, valid]),
            Some(socket.display().to_string())
        );
    }

    #[test]
    fn stale_socket_marker_falls_back_to_live_default_and_repairs_marker() {
        let tmp = tempfile::tempdir().expect("socket marker tempdir");
        let marker = tmp.path().join("last-socket-path");
        let default_socket = tmp.path().join("cmux.sock");
        let _listener = std::os::unix::net::UnixListener::bind(&default_socket)
            .expect("bind default test socket");
        fs::write(
            &marker,
            format!("{}\n", tmp.path().join("gone.sock").display()),
        )
        .expect("write stale marker");

        assert_eq!(
            socket_from_markers_or_default(
                &[marker.clone()],
                &default_socket.display().to_string()
            ),
            default_socket.display().to_string()
        );
        assert_eq!(
            fs::read_to_string(marker).expect("read repaired marker"),
            format!("{}\n", default_socket.display())
        );
    }

    #[test]
    fn default_socket_path_uses_cmux_state_directory() {
        assert_eq!(
            default_socket_path_in_state_dir(Path::new("/run/user/1000/state/cmux")),
            "/run/user/1000/state/cmux/cmux.sock"
        );
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
    fn cli_cache_dir_prefers_xdg_then_home_then_temp() {
        assert_eq!(
            cache_dir_from_env(
                Some("/run/user/1000/cache"),
                Some("/home/me"),
                Path::new("/tmp")
            ),
            PathBuf::from("/run/user/1000/cache/cmux")
        );
        assert_eq!(
            cache_dir_from_env(None, Some("/home/me"), Path::new("/tmp")),
            PathBuf::from("/home/me/.cache/cmux")
        );
        assert_eq!(
            cache_dir_from_env(None, None, Path::new("/tmp/codex")),
            PathBuf::from("/tmp/codex/cmux-cache")
        );
    }

    #[test]
    fn browser_artifact_defaults_live_under_cmux_cache() {
        assert_eq!(
            browser_artifact_path_for(
                Path::new("/home/me/.cache/cmux"),
                &["browser", "screenshots"],
                "cmux-browser-screenshot",
                "png",
                42,
                99
            ),
            PathBuf::from(
                "/home/me/.cache/cmux/browser/screenshots/cmux-browser-screenshot-42-99.png"
            )
        );
        assert_eq!(
            browser_artifact_path_for(
                Path::new("/home/me/.cache/cmux"),
                &["browser", "pdf"],
                "cmux-browser-pdf",
                "pdf",
                42,
                100
            ),
            PathBuf::from("/home/me/.cache/cmux/browser/pdf/cmux-browser-pdf-42-100.pdf")
        );
    }

    #[test]
    fn browser_screenshot_cli_forwards_full_page_capture() {
        let (method, params, _) = browser_command_to_request(&cli_args(&[
            "browser",
            "screenshot",
            "--surface",
            "surface-live",
            "--full-page",
        ]))
        .expect("browser screenshot command");

        assert_eq!(method, "browser.screenshot");
        assert_eq!(params["surface_id"], "surface-live");
        assert_eq!(params["full_page"], true);
    }

    #[test]
    fn custom_sidebar_cli_routes_validation_reload_selection_and_default() {
        let (method, params, action) =
            custom_sidebar_params(&cli_args(&["sidebar", "validate", "status"]))
                .expect("validate custom sidebar");
        assert_eq!(method, "sidebar.custom.validate");
        assert_eq!(params["name"], "status");
        assert_eq!(action, "validate");

        let (method, params, action) =
            custom_sidebar_params(&cli_args(&["sidebar", "reload", "--all", "--json"]))
                .expect("reload custom sidebars");
        assert_eq!(method, "sidebar.custom.reload");
        assert_eq!(params, json!({}));
        assert_eq!(action, "reload");

        let (method, params, action) =
            custom_sidebar_params(&cli_args(&["sidebar", "select", "finder"]))
                .expect("select custom sidebar");
        assert_eq!(method, "sidebar.custom.select");
        assert_eq!(params["name"], "finder");
        assert_eq!(action, "select");

        let (_, params, _) = custom_sidebar_params(&cli_args(&["sidebar", "select", "workspaces"]))
            .expect("restore workspace sidebar");
        assert_eq!(params["provider_id"], "cmux.sidebar.workspaces");

        let (method, params, action) =
            custom_sidebar_params(&cli_args(&["sidebar", "clear-state", "finder"]))
                .expect("clear custom sidebar state");
        assert_eq!(method, "sidebar.custom.state.clear");
        assert_eq!(params["provider_id"], "cmux.sidebar.custom.finder");
        assert_eq!(action, "clear-state");

        let (method, params, _) = custom_sidebar_params(&cli_args(&["sidebar", "clear-state"]))
            .expect("clear selected custom sidebar state");
        assert_eq!(method, "sidebar.custom.state.clear");
        assert_eq!(params, json!({}));
    }

    #[test]
    fn custom_sidebar_cli_rejects_ambiguous_and_unknown_arguments() {
        for args in [
            &["sidebar", "validate", "--all", "status"][..],
            &["sidebar", "select", "--all"][..],
            &["sidebar", "select"][..],
            &["sidebar", "reload", "--bogus"][..],
            &["sidebar", "clear-state", "--all"][..],
            &["sidebar", "clear-state", "one", "two"][..],
        ] {
            assert!(
                custom_sidebar_params(&cli_args(args)).is_err(),
                "expected custom sidebar parse failure for {args:?}"
            );
        }
    }

    fn expect_cli_parse_error(
        result: Result<(String, Value, TextMode)>,
        context: &str,
    ) -> anyhow::Error {
        match result {
            Ok((method, params, _)) => {
                panic!("{context}: unexpectedly succeeded with method={method} params={params}");
            }
            Err(err) => err,
        }
    }

    #[test]
    fn renderer_apply_size_params_are_strict_and_canonical() {
        let (method, params, _) = renderer_command_to_request(&cli_args(&[
            "renderer",
            "resize",
            "--pane-id",
            "pane:2",
            "--columns",
            "100",
            "--rows",
            "30",
            "--width",
            "1000",
            "--height",
            "600",
            "--attachment-id",
            "gtk",
        ]))
        .expect("renderer resize request");

        assert_eq!(method, "renderer.apply_size");
        assert_eq!(params["pane_id"].as_str(), Some("pane:2"));
        assert_eq!(params["cols"].as_u64(), Some(100));
        assert_eq!(params["rows"].as_u64(), Some(30));
        assert_eq!(params["pixel_width"].as_u64(), Some(1000));
        assert_eq!(params["pixel_height"].as_u64(), Some(600));
        assert_eq!(params["attachment_id"].as_str(), Some("gtk"));
    }

    #[test]
    fn renderer_snapshot_params_include_explicit_window() {
        let (method, params, _) = renderer_command_to_request(&cli_args(&[
            "renderer",
            "snapshot",
            "--backend",
            "ghostty",
            "--window",
            "window:2",
        ]))
        .expect("renderer window snapshot request");

        assert_eq!(method, "renderer.snapshot");
        assert_eq!(params["backend"], "ghostty");
        assert_eq!(params["window_id"], "window:2");
    }

    #[test]
    fn renderer_apply_size_params_reject_missing_unknown_and_invalid_values() {
        let missing = expect_cli_parse_error(
            renderer_command_to_request(&cli_args(&[
                "renderer",
                "apply-size",
                "--pane",
                "pane:1",
                "--cols",
                "80",
                "--rows",
                "24",
                "--pixel-width",
                "800",
            ])),
            "missing pixel height should fail",
        );
        assert!(
            missing.to_string().contains("--pixel-height"),
            "unexpected error: {missing}"
        );

        let unknown = expect_cli_parse_error(
            renderer_command_to_request(&cli_args(&[
                "renderer",
                "apply-size",
                "--pane",
                "pane:1",
                "--cols",
                "80",
                "--rows",
                "24",
                "--pixel-width",
                "800",
                "--pixel-height",
                "480",
                "--bogus",
                "value",
            ])),
            "unknown flag should fail",
        );
        assert!(
            unknown.to_string().contains("unknown argument '--bogus'"),
            "unexpected error: {unknown}"
        );

        let invalid = expect_cli_parse_error(
            renderer_command_to_request(&cli_args(&[
                "renderer",
                "apply-size",
                "--pane",
                "pane:1",
                "--cols",
                "0",
                "--rows",
                "24",
                "--pixel-width",
                "800",
                "--pixel-height",
                "480",
            ])),
            "zero cols should fail",
        );
        assert!(
            invalid
                .to_string()
                .contains("--cols must be greater than 0"),
            "unexpected error: {invalid}"
        );
    }
}

fn ssh_destination(command: &[String]) -> Option<String> {
    let mut index = 1;
    let mut literal = false;
    while index < command.len() {
        let arg = &command[index];
        if literal {
            return Some(arg.clone());
        }
        match arg.as_str() {
            "--" => literal = true,
            "-A" | "--forward-agent" | "-a" | "--no-forward-agent" => {}
            "--name" | "--port" | "--identity" | "--ssh-option" => index += 1,
            value
                if value.starts_with("--name=")
                    || value.starts_with("--port=")
                    || value.starts_with("--identity=")
                    || value.starts_with("--ssh-option=") => {}
            _ => return Some(arg.clone()),
        }
        index += 1;
    }
    None
}

fn ssh_forward_agent_flag(command: &[String]) -> Option<bool> {
    let mut result = None;
    let mut index = 1;
    while index < command.len() {
        match command[index].as_str() {
            "--" => break,
            "-A" | "--forward-agent" => result = Some(true),
            "-a" | "--no-forward-agent" => result = Some(false),
            "--name" | "--port" | "--identity" | "--ssh-option" => index += 1,
            _ => {}
        }
        index += 1;
    }
    result
}

fn all_option_values(command: &[String], flag: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut iter = command.iter().peekable();
    while let Some(arg) = iter.next() {
        if arg == "--" {
            break;
        }
        if arg == flag {
            if let Some(value) = iter.peek() {
                out.push((*value).clone().to_string());
            }
        }
    }
    out
}

fn read_screen_params(command: &[String]) -> Result<Value> {
    let mut params = workspace_option_params(
        command,
        &[
            ("--workspace", "workspace_id"),
            ("--surface", "surface_id"),
            ("--panel", "surface_id"),
        ],
    )?;
    add_env_surface_default(&mut params);
    if command.iter().any(|arg| arg == "--scrollback") {
        params["scrollback"] = json!(true);
    }
    if let Some(lines) = option_value(command, "--lines") {
        let parsed = lines.parse::<i64>().context("--lines must be an integer")?;
        if parsed <= 0 {
            bail!("--lines must be greater than 0");
        }
        params["lines"] = json!(parsed);
    }
    Ok(params)
}

fn notify_params(command: &[String]) -> Result<Value> {
    option_params(
        command,
        &[
            ("--title", "title"),
            ("--subtitle", "subtitle"),
            ("--body", "body"),
            ("--workspace", "workspace_id"),
            ("--surface", "surface_id"),
            ("--window", "window_id"),
        ],
    )
}

fn dismiss_notification_params(command: &[String]) -> Result<Value> {
    let id = option_value(command, "--id");
    let all_read = command_has_flag(command, "--all-read");
    if usize::from(id.is_some()) + usize::from(all_read) != 1 {
        bail!("dismiss-notification requires exactly one of --id or --all-read");
    }
    let mut params = Map::new();
    if let Some(id) = id {
        params.insert("id".to_string(), json!(id));
    }
    if all_read {
        params.insert("all_read".to_string(), json!(true));
    }
    Ok(Value::Object(params))
}

fn mark_notification_read_params(command: &[String]) -> Result<Value> {
    let id = option_value(command, "--id");
    let workspace = option_value(command, "--workspace");
    let surface = option_value(command, "--surface");
    let all = command_has_flag(command, "--all");
    if usize::from(id.is_some()) + usize::from(workspace.is_some()) + usize::from(all) != 1 {
        bail!("mark-notification-read requires exactly one selector: --id, --workspace, or --all");
    }
    if surface.is_some() && workspace.is_none() {
        bail!("--surface requires --workspace");
    }

    let mut params = Map::new();
    if let Some(id) = id {
        params.insert("id".to_string(), json!(id));
    }
    if let Some(workspace) = workspace {
        params.insert("workspace_id".to_string(), scalar_value(&workspace));
    }
    if let Some(surface) = surface {
        params.insert("surface_id".to_string(), scalar_value(&surface));
    }
    if all {
        params.insert("all".to_string(), json!(true));
    }
    collect_option(command, "--window", "window_id", &mut params)?;
    Ok(Value::Object(params))
}

fn open_notification_params(command: &[String]) -> Result<Value> {
    let id = option_value(command, "--id").context("open-notification requires --id")?;
    Ok(json!({"id": id}))
}

fn right_sidebar_params(command: &[String]) -> Result<Value> {
    let mut positional = Vec::new();
    let mut workspace = None;
    let mut window = None;
    let mut no_focus = false;
    let mut index = 1;
    while index < command.len() {
        let arg = &command[index];
        match arg.as_str() {
            "--workspace" => {
                index += 1;
                workspace = Some(
                    command
                        .get(index)
                        .context("right-sidebar: --workspace requires an id")?
                        .clone(),
                );
            }
            "--window" => {
                index += 1;
                window = Some(
                    command
                        .get(index)
                        .context("right-sidebar: --window requires an id")?
                        .clone(),
                );
            }
            "--no-focus" => {
                no_focus = true;
            }
            "--" => {
                positional.extend(command.iter().skip(index + 1).cloned());
                break;
            }
            _ if arg.starts_with("--workspace=") => {
                workspace = Some(arg["--workspace=".len()..].to_string());
            }
            _ if arg.starts_with("--window=") => {
                window = Some(arg["--window=".len()..].to_string());
            }
            _ if arg.starts_with("--") => bail!("right-sidebar: unknown flag '{arg}'"),
            _ => positional.push(arg.clone()),
        }
        index += 1;
    }

    let action = positional
        .first()
        .map(|value| value.trim().to_ascii_lowercase())
        .filter(|value| !value.is_empty())
        .context("right-sidebar requires a subcommand")?;
    let mut params = Map::new();
    if let Some(workspace) = workspace {
        params.insert("workspace_id".to_string(), scalar_value(&workspace));
    }
    if let Some(window) = window {
        params.insert("window_id".to_string(), scalar_value(&window));
    }

    match action.as_str() {
        "toggle" | "show" | "hide" | "focus" | "mode" => {
            if positional.len() != 1 {
                bail!("right-sidebar {action} received unexpected arguments");
            }
            if no_focus {
                bail!("right-sidebar: --no-focus is only valid with set");
            }
            params.insert("action".to_string(), json!(action));
        }
        "set" => {
            if positional.len() != 2 {
                bail!("right-sidebar set requires a mode: files, find, vault, sessions, feed, or dock");
            }
            let mode = positional[1].trim().to_ascii_lowercase();
            if !is_right_sidebar_cli_mode(&mode) {
                bail!("Unknown right-sidebar mode '{}'", positional[1]);
            }
            params.insert("action".to_string(), json!("set"));
            params.insert("mode".to_string(), json!(mode));
            if no_focus {
                params.insert("no_focus".to_string(), json!(true));
            }
        }
        "files" | "find" | "vault" | "sessions" | "feed" | "dock" => {
            if positional.len() != 1 {
                bail!("right-sidebar {action} received unexpected arguments");
            }
            if no_focus {
                bail!("right-sidebar: --no-focus is only valid with set");
            }
            params.insert("action".to_string(), json!("set"));
            params.insert("mode".to_string(), json!(action));
        }
        other => bail!("Unknown right-sidebar command '{other}'"),
    }

    Ok(Value::Object(params))
}

fn custom_sidebar_params(command: &[String]) -> Result<(String, Value, String)> {
    let action = command
        .get(1)
        .map(|value| value.trim().to_ascii_lowercase())
        .filter(|value| !value.is_empty())
        .context("sidebar requires a subcommand: validate, reload, select, or clear-state")?;
    let mut explicit_all = false;
    let mut names = Vec::new();
    for value in command.iter().skip(2) {
        match value.as_str() {
            "--all" => explicit_all = true,
            "--json" => {}
            _ if value.starts_with("--") => bail!("sidebar: unknown flag '{value}'"),
            _ => names.push(value.clone()),
        }
    }
    match action.as_str() {
        "validate" | "reload" => {
            if names.len() > 1 {
                bail!("sidebar {action} accepts at most one sidebar name");
            }
            if explicit_all && !names.is_empty() {
                bail!("sidebar {action}: use either --all or a sidebar name, not both");
            }
            let mut params = Map::new();
            if let Some(name) = names.first() {
                params.insert("name".to_string(), json!(name));
            }
            Ok((
                if action == "validate" {
                    "sidebar.custom.validate"
                } else {
                    "sidebar.custom.reload"
                }
                .to_string(),
                Value::Object(params),
                action,
            ))
        }
        "select" => {
            if explicit_all {
                bail!("sidebar select does not support --all");
            }
            if names.len() != 1 {
                bail!("sidebar select requires one sidebar name");
            }
            let name = names[0].trim();
            let params = if matches!(name.to_ascii_lowercase().as_str(), "default" | "workspaces") {
                json!({"provider_id": "cmux.sidebar.workspaces"})
            } else {
                json!({"name": name})
            };
            Ok(("sidebar.custom.select".to_string(), params, action))
        }
        "clear-state" | "reset-state" => {
            if explicit_all {
                bail!("sidebar {action} does not support --all");
            }
            if names.len() > 1 {
                bail!("sidebar {action} accepts at most one sidebar name");
            }
            let params = names.first().map_or_else(
                || json!({}),
                |name| json!({"provider_id": custom_sidebar::provider_id(name.trim())}),
            );
            Ok((
                "sidebar.custom.state.clear".to_string(),
                params,
                "clear-state".to_string(),
            ))
        }
        other => bail!("Unknown sidebar command '{other}'"),
    }
}

fn is_right_sidebar_cli_mode(value: &str) -> bool {
    matches!(
        value,
        "files" | "find" | "vault" | "sessions" | "feed" | "dock"
    )
}

fn sidebar_target_params(command: &[String]) -> Result<Value> {
    let mut params = option_params(
        command,
        &[
            ("--workspace", "workspace_id"),
            ("--window", "window_id"),
            ("--surface", "surface_id"),
            ("--panel", "surface_id"),
            ("--tab", "workspace_id"),
            ("--limit", "limit"),
        ],
    )?;
    add_env_default(&mut params, "workspace_id", "CMUX_WORKSPACE_ID");
    Ok(params)
}

fn sidebar_status_set_params(command: &[String]) -> Result<Value> {
    let mut params = sidebar_target_params(command)?;
    let args = positional_args(command);
    let key = args.first().context("set-status requires a key")?;
    let value = args.get(1).context("set-status requires a value")?;
    params["key"] = json!(key);
    params["value"] = json!(value);
    if let Some(priority) = option_value(command, "--priority") {
        params["priority"] = scalar_value(&priority);
    }
    if let Some(icon) = option_value(command, "--icon") {
        params["icon"] = json!(icon);
    }
    if let Some(color) = option_value(command, "--color") {
        params["color"] = json!(color);
    }
    if let Some(url) = option_value(command, "--url").or_else(|| option_value(command, "--link")) {
        params["url"] = json!(url);
    }
    if let Some(format) = option_value(command, "--format") {
        params["format"] = json!(format);
    }
    Ok(params)
}

fn sidebar_agent_lifecycle_set_params(command: &[String]) -> Result<Value> {
    let mut params = sidebar_target_params(command)?;
    let args = positional_args(command);
    let key = args.first().context("set-agent-lifecycle requires a key")?;
    let lifecycle = args
        .get(1)
        .context("set-agent-lifecycle requires a lifecycle")?;
    params["key"] = json!(key);
    params["lifecycle"] = json!(lifecycle);
    Ok(params)
}

fn sidebar_key_params(command: &[String], missing: &'static str) -> Result<Value> {
    let mut params = sidebar_target_params(command)?;
    let args = positional_args(command);
    let key = args.first().context(missing)?;
    params["key"] = json!(key);
    Ok(params)
}

fn app_focus_override_params(command: &[String]) -> Result<Value> {
    let state = option_value(command, "--state")
        .or_else(|| positional_args(command).first().cloned())
        .unwrap_or_else(|| "clear".to_string());
    Ok(json!({ "state": state }))
}

fn sidebar_progress_set_params(command: &[String]) -> Result<Value> {
    let mut params = sidebar_target_params(command)?;
    let args = positional_args(command);
    let value = args.first().context("set-progress requires a value")?;
    params["value"] = scalar_value(value);
    if let Some(label) = option_value(command, "--label") {
        params["label"] = json!(label);
    }
    Ok(params)
}

fn sidebar_log_append_params(command: &[String]) -> Result<Value> {
    let mut params = sidebar_target_params(command)?;
    if let Some(level) = option_value(command, "--level") {
        params["level"] = json!(level);
    }
    if let Some(source) = option_value(command, "--source") {
        params["source"] = json!(source);
    }
    let message = trailing_title(command).context("log requires a message")?;
    params["message"] = json!(message);
    Ok(params)
}

fn sidebar_log_list_params(command: &[String]) -> Result<Value> {
    sidebar_target_params(command)
}

fn sidebar_metadata_block_set_params(command: &[String]) -> Result<Value> {
    let mut params = sidebar_target_params(command)?;
    let args = positional_args(command);
    let key = args.first().context("report-meta-block requires a key")?;
    let markdown = if let Some(literal_index) = command.iter().position(|arg| arg == "--") {
        command
            .iter()
            .skip(literal_index + 1)
            .cloned()
            .collect::<Vec<_>>()
            .join(" ")
    } else {
        args.iter().skip(1).cloned().collect::<Vec<_>>().join(" ")
    };
    if markdown.trim().is_empty() {
        anyhow::bail!("report-meta-block requires markdown after the key");
    }
    params["key"] = json!(key);
    params["markdown"] = json!(markdown);
    if let Some(priority) = option_value(command, "--priority") {
        params["priority"] = scalar_value(&priority);
    }
    Ok(params)
}

fn browser_legacy_command_to_request(
    command: &[String],
    subcommand: &str,
) -> Result<(String, Value, TextMode)> {
    let target_index = browser_legacy_positional_target_index(command, subcommand);
    let mut delegated = vec!["browser".to_string()];
    if let Some(index) = target_index {
        delegated.push(command[index].clone());
    }
    delegated.push(subcommand.to_string());
    delegated.extend(
        command
            .iter()
            .enumerate()
            .skip(1)
            .filter_map(|(index, arg)| {
                if Some(index) == target_index {
                    None
                } else {
                    Some(arg.clone())
                }
            }),
    );
    browser_command_to_request(&delegated)
}

fn warn_legacy_browser_deprecation(command: &[String], json_output: bool) {
    if json_output {
        return;
    }
    if let Some((alias, replacement)) = legacy_browser_alias_replacement(command) {
        eprintln!(
            "warning: legacy browser CLI alias `{alias}` is deprecated; use `cmux {replacement}` instead"
        );
    }
}

fn legacy_browser_alias_replacement(command: &[String]) -> Option<(&str, &'static str)> {
    let alias = command.first()?.as_str();
    let replacement = match alias {
        "open-browser" | "open_browser" => "browser open",
        "navigate" => "browser navigate",
        "browser-back" | "browser_back" => "browser back",
        "browser-forward" | "browser_forward" => "browser forward",
        "browser-reload" | "browser_reload" => "browser reload",
        "get-url" | "get_url" => "browser get-url",
        "focus-webview" | "focus_webview" => "browser focus-webview",
        "is-webview-focused" | "is_webview_focused" => "browser is-webview-focused",
        _ => return None,
    };
    Some((alias, replacement))
}

fn browser_legacy_positional_target_index(command: &[String], subcommand: &str) -> Option<usize> {
    if subcommand == "open"
        || option_value(command, "--surface").is_some()
        || option_value(command, "--surface-id").is_some()
    {
        return None;
    }
    let positions = positional_arg_indices(command);
    match subcommand {
        "navigate" => positions.get(1).and_then(|_| positions.first()).copied(),
        _ => positions.first().copied(),
    }
}

fn browser_command_to_request(command: &[String]) -> Result<(String, Value, TextMode)> {
    let first = command.get(1).map(String::as_str).unwrap_or("status");
    let (target, sub_index, sub) = if is_browser_subcommand(first) || command.len() <= 2 {
        (None, 1, first)
    } else {
        (
            command.get(1).cloned(),
            2,
            command.get(2).map(String::as_str).unwrap_or("status"),
        )
    };
    match sub {
        "disable" | "enable" | "status" => {
            if let Some(arg) = command
                .iter()
                .skip(sub_index + 1)
                .find(|arg| arg.as_str() != "--json")
            {
                bail!("Unexpected argument: {arg}");
            }
            let method = match sub {
                "disable" => "browser.disable",
                "enable" => "browser.enable",
                _ => "browser.status",
            };
            Ok((
                method.to_string(),
                json!({}),
                TextMode::BrowserAvailability {
                    status_only: sub == "status",
                },
            ))
        }
        "connect" => {
            let mut params = browser_target_params(
                command,
                target.as_deref(),
                &[
                    ("--url", "url"),
                    ("--workspace", "workspace_id"),
                    ("--workspace-id", "workspace_id"),
                    ("--profile", "profile_id"),
                    ("--profile-id", "profile_id"),
                ],
            )?;
            if !params.get("url").is_some_and(|value| !value.is_null()) {
                if let Some(url) = first_positional_after(command, sub_index + 1) {
                    params["url"] = json!(url);
                }
            }
            if command_has_flag(command, "--no-create") {
                params["create"] = json!(false);
            }
            if command_has_flag(command, "--create") {
                params["create"] = json!(true);
            }
            if command_has_flag(command, "--focus") {
                params["focus"] = json!(true);
            }
            Ok(("browser.connect".to_string(), params, TextMode::Jsonish))
        }
        "open" | "open-split" | "new" => {
            let mut params = Map::new();
            if let Some(url) = first_positional_after(command, sub_index + 1) {
                params.insert("url".to_string(), json!(url));
            }
            if let Some(workspace) = option_value(command, "--workspace")
                .or_else(|| option_value(command, "--workspace-id"))
            {
                params.insert("workspace_id".to_string(), json!(workspace));
            }
            if let Some(profile) = option_value(command, "--profile")
                .or_else(|| option_value(command, "--profile-id"))
            {
                params.insert("profile_id".to_string(), json!(profile));
            }
            Ok((
                "browser.open_split".to_string(),
                Value::Object(params),
                TextMode::OkRef("surface_ref"),
            ))
        }
        "window" => {
            let action = command
                .get(sub_index + 1)
                .map(String::as_str)
                .unwrap_or("new");
            let mut params = Map::new();
            if let Some(url) = first_positional_after(command, sub_index + 2) {
                params.insert("url".to_string(), json!(url));
            }
            if let Some(title) =
                option_value(command, "--window-title").or_else(|| option_value(command, "--title"))
            {
                params.insert("title".to_string(), json!(title));
            }
            if let Some(workspace_title) = option_value(command, "--workspace-title") {
                params.insert("workspace_title".to_string(), json!(workspace_title));
            }
            if let Some(profile) = option_value(command, "--profile")
                .or_else(|| option_value(command, "--profile-id"))
            {
                params.insert("profile_id".to_string(), json!(profile));
            }
            if command_has_flag(command, "--no-focus") {
                params.insert("focus".to_string(), json!(false));
            }
            match action {
                "new" | "create" => Ok((
                    "browser.window.new".to_string(),
                    Value::Object(params),
                    TextMode::Jsonish,
                )),
                other => bail!("unsupported browser window action: {other}"),
            }
        }
        "close" | "quit" | "exit" => Ok((
            format!("browser.{sub}"),
            browser_target_params(command, target.as_deref(), &[])?,
            TextMode::Jsonish,
        )),
        "identify" => Ok((
            "browser.identify".to_string(),
            browser_target_params(
                command,
                target.as_deref(),
                &[
                    ("--workspace", "workspace_id"),
                    ("--workspace-id", "workspace_id"),
                ],
            )?,
            TextMode::Jsonish,
        )),
        "focus-webview" | "focus_webview" => Ok((
            "browser.focus_webview".to_string(),
            browser_target_params(command, target.as_deref(), &[])?,
            TextMode::Jsonish,
        )),
        "is-webview-focused" | "is_webview_focused" => Ok((
            "browser.is_webview_focused".to_string(),
            browser_target_params(command, target.as_deref(), &[])?,
            TextMode::Jsonish,
        )),
        "goto" | "navigate" => {
            let mut params = browser_target_params(command, target.as_deref(), &[])?;
            params["url"] =
                json!(last_positional(command).context("browser navigate requires url")?);
            if command_has_flag(command, "--snapshot-after") {
                params["snapshot_after"] = json!(true);
            }
            Ok(("browser.navigate".to_string(), params, TextMode::Jsonish))
        }
        "find" => {
            let family = command
                .get(sub_index + 1)
                .context("browser find requires type")?;
            let value = command.get(sub_index + 2).cloned().unwrap_or_default();
            let mut params = browser_target_params(
                command,
                target.as_deref(),
                &[("--role", "role"), ("--name", "name")],
            )?;
            match family.as_str() {
                "role" => {
                    if !params.get("role").is_some_and(|value| !value.is_null()) {
                        params["role"] = json!(value);
                    }
                    if !params.get("name").is_some_and(|value| !value.is_null()) {
                        if let Some(name) = command.get(sub_index + 3) {
                            params["name"] = json!(name);
                        }
                    }
                }
                "text" => params["text"] = json!(value),
                "label" => params["label"] = json!(value),
                "placeholder" => params["placeholder"] = json!(value),
                "alt" => params["alt"] = json!(value),
                "title" => params["title"] = json!(value),
                "testid" => params["testid"] = json!(value),
                "first" | "last" | "nth" => params["selector"] = json!(value),
                other => bail!("unsupported browser find type: {other}"),
            }
            Ok((format!("browser.find.{family}"), params, TextMode::Jsonish))
        }
        "frame" => {
            let frame_target = command
                .get(sub_index + 1)
                .map(String::as_str)
                .unwrap_or("main");
            let mut params = browser_target_params(command, target.as_deref(), &[])?;
            if frame_target == "main" {
                Ok(("browser.frame.main".to_string(), params, TextMode::Jsonish))
            } else {
                params["selector"] = json!(frame_target);
                Ok((
                    "browser.frame.select".to_string(),
                    params,
                    TextMode::Jsonish,
                ))
            }
        }
        "dialog" => {
            let action = command
                .get(sub_index + 1)
                .map(String::as_str)
                .unwrap_or("accept");
            let mut params = browser_target_params(
                command,
                target.as_deref(),
                &[
                    ("--text", "text"),
                    ("--prompt-text", "prompt_text"),
                    ("--response", "response"),
                    ("--action", "action"),
                ],
            )?;
            if command_has_flag(command, "--accept") {
                params["accept"] = json!(true);
                params["action"] = json!("accept");
            }
            if command_has_flag(command, "--dismiss") || command_has_flag(command, "--cancel") {
                params["accept"] = json!(false);
                params["action"] = json!("dismiss");
            }
            if !params.get("text").is_some_and(|value| !value.is_null())
                && !params
                    .get("prompt_text")
                    .is_some_and(|value| !value.is_null())
                && !params.get("response").is_some_and(|value| !value.is_null())
            {
                if let Some(text) = first_positional_after(command, sub_index + 2) {
                    params["text"] = scalar_value(&text);
                }
            }
            let method = match action {
                "accept" | "ok" => "browser.dialog.accept",
                "dismiss" | "cancel" => "browser.dialog.dismiss",
                "respond" | "response" => "browser.dialog.respond",
                other => bail!("unsupported browser dialog action: {other}"),
            };
            Ok((method.to_string(), params, TextMode::Jsonish))
        }
        "profiles" | "profile" => browser_profiles_command_to_request(command, sub_index),
        "import" => {
            let mut params = Map::new();
            if let Some(source) = option_value(command, "--from")
                .or_else(|| option_value(command, "--browser"))
                .or_else(|| option_value(command, "--source"))
            {
                params.insert("browser".to_string(), json!(source));
            }
            if let Some(profile) = option_value(command, "--profile")
                .or_else(|| option_value(command, "--source-profile"))
            {
                params.insert("source_profile".to_string(), json!(profile));
            }
            if command_has_flag(command, "--all-profiles") {
                params.insert("all_profiles".to_string(), json!(true));
            }
            if let Some(destination) = option_value(command, "--to-profile")
                .or_else(|| option_value(command, "--destination-profile"))
                .or_else(|| option_value(command, "--to"))
            {
                params.insert("destination_profile".to_string(), json!(destination));
            }
            if command_has_flag(command, "--create-profile") {
                params.insert("create_profile".to_string(), json!(true));
            }
            if let Some(domain) = option_value(command, "--domain") {
                params.insert("domain".to_string(), json!(domain));
            }
            if let Some(scope) = option_value(command, "--scope") {
                params.insert("scope".to_string(), json!(scope));
            }
            if let Some(workspace) = option_value(command, "--workspace")
                .or_else(|| option_value(command, "--workspace-id"))
            {
                params.insert("workspace_id".to_string(), json!(workspace));
            }
            if !params.contains_key("workspace_id") {
                if let Some(workspace) = normalized_env("CMUX_WORKSPACE_ID") {
                    params.insert("workspace_id".to_string(), json!(workspace));
                }
            }
            if let Some(path) = option_value(command, "--cookies-file")
                .or_else(|| option_value(command, "--cookie-file"))
                .or_else(|| option_value(command, "--cookies-path"))
                .or_else(|| option_value(command, "--path"))
            {
                params.insert("cookies_file".to_string(), json!(path));
            }
            if let Some(path) = option_value(command, "--bookmarks-file")
                .or_else(|| option_value(command, "--bookmark-file"))
                .or_else(|| option_value(command, "--bookmarks-path"))
            {
                params.insert("bookmarks_file".to_string(), json!(path));
            }
            if let Some(path) = option_value(command, "--settings-file")
                .or_else(|| option_value(command, "--browser-settings-file"))
                .or_else(|| option_value(command, "--settings-path"))
            {
                params.insert("settings_file".to_string(), json!(path));
            }
            if command_has_flag(command, "--interactive") {
                Ok((
                    "browser.import.dialog".to_string(),
                    Value::Object(params),
                    TextMode::Jsonish,
                ))
            } else {
                Ok((
                    "browser.import.data".to_string(),
                    Value::Object(params),
                    TextMode::Jsonish,
                ))
            }
        }
        "cookies" => {
            let action = command
                .get(sub_index + 1)
                .map(String::as_str)
                .unwrap_or("get");
            let mut params = browser_target_params(
                command,
                target.as_deref(),
                &[("--name", "name"), ("--url", "url")],
            )?;
            match action {
                "set" => {
                    params["name"] = json!(command
                        .get(sub_index + 2)
                        .context("cookie name is required")?);
                    params["value"] = json!(command
                        .get(sub_index + 3)
                        .context("cookie value is required")?);
                    Ok(("browser.cookies.set".to_string(), params, TextMode::Jsonish))
                }
                "get" => Ok(("browser.cookies.get".to_string(), params, TextMode::Jsonish)),
                "clear" => Ok((
                    "browser.cookies.clear".to_string(),
                    params,
                    TextMode::Jsonish,
                )),
                other => bail!("unsupported browser cookies action: {other}"),
            }
        }
        "storage" => {
            let storage_type = command
                .get(sub_index + 1)
                .map(String::as_str)
                .unwrap_or("local");
            let action = command
                .get(sub_index + 2)
                .map(String::as_str)
                .unwrap_or("get");
            let mut params =
                browser_target_params(command, target.as_deref(), &[("--key", "key")])?;
            params["type"] = json!(storage_type);
            match action {
                "set" => {
                    params["key"] = json!(command
                        .get(sub_index + 3)
                        .context("storage key is required")?);
                    params["value"] = json!(command
                        .get(sub_index + 4)
                        .context("storage value is required")?);
                    Ok(("browser.storage.set".to_string(), params, TextMode::Jsonish))
                }
                "get" => {
                    if let Some(key) = command.get(sub_index + 3) {
                        params["key"] = json!(key);
                    }
                    Ok(("browser.storage.get".to_string(), params, TextMode::Jsonish))
                }
                "clear" => Ok((
                    "browser.storage.clear".to_string(),
                    params,
                    TextMode::Jsonish,
                )),
                other => bail!("unsupported browser storage action: {other}"),
            }
        }
        "set" => browser_set_command_to_request(command, target.as_deref(), sub_index),
        "fill" => {
            let mut params =
                browser_target_params(command, target.as_deref(), &[("--text", "text")])?;
            params["selector"] = json!(command
                .get(sub_index + 1)
                .context("fill requires selector")?);
            if command_has_flag(command, "--snapshot-after") {
                params["snapshot_after"] = json!(true);
            }
            Ok(("browser.fill".to_string(), params, TextMode::Jsonish))
        }
        "click" => {
            let mut params = browser_target_params(command, target.as_deref(), &[])?;
            params["selector"] = json!(command
                .get(sub_index + 1)
                .context("click requires selector")?);
            Ok(("browser.click".to_string(), params, TextMode::Jsonish))
        }
        "dblclick" | "double-click" | "hover" | "focus" | "check" | "uncheck"
        | "scroll-into-view" | "scrollintoview" | "scrollinto" => {
            let mut params = browser_target_params(command, target.as_deref(), &[])?;
            params["selector"] = json!(command
                .get(sub_index + 1)
                .with_context(|| format!("{sub} requires selector"))?);
            let method = match sub {
                "dblclick" | "double-click" => "browser.dblclick",
                "scroll-into-view" | "scrollintoview" | "scrollinto" => "browser.scroll_into_view",
                other => return Ok((format!("browser.{other}"), params, TextMode::Jsonish)),
            };
            Ok((method.to_string(), params, TextMode::Jsonish))
        }
        "drag" => {
            let mut params = browser_target_params(
                command,
                target.as_deref(),
                &[
                    ("--selector", "selector"),
                    ("--source", "source"),
                    ("--from", "source"),
                    ("--target", "target"),
                    ("--to", "target"),
                    ("--start-x", "start_x"),
                    ("--start-y", "start_y"),
                    ("--end-x", "end_x"),
                    ("--end-y", "end_y"),
                    ("--x", "x"),
                    ("--y", "y"),
                ],
            )?;
            if !params.get("selector").is_some_and(|value| !value.is_null())
                && !params.get("source").is_some_and(|value| !value.is_null())
            {
                params["selector"] = scalar_value(
                    command
                        .get(sub_index + 1)
                        .context("drag requires source selector")?,
                );
            }
            if !params.get("target").is_some_and(|value| !value.is_null()) {
                if let Some(target) = command
                    .get(sub_index + 2)
                    .filter(|arg| !arg.starts_with("--"))
                {
                    params["target"] = scalar_value(target);
                }
            }
            Ok(("browser.drag".to_string(), params, TextMode::Jsonish))
        }
        "upload" => {
            let mut params = browser_target_params(
                command,
                target.as_deref(),
                &[
                    ("--selector", "selector"),
                    ("--target", "selector"),
                    ("--input", "selector"),
                    ("--file", "path"),
                    ("--path", "path"),
                    ("--files", "files"),
                    ("--paths", "files"),
                ],
            )?;
            if !params.get("selector").is_some_and(|value| !value.is_null()) {
                params["selector"] = scalar_value(
                    command
                        .get(sub_index + 1)
                        .context("upload requires selector")?,
                );
            }
            if !params.get("path").is_some_and(|value| !value.is_null())
                && !params.get("files").is_some_and(|value| !value.is_null())
            {
                if let Some(path) = first_positional_after(command, sub_index + 2) {
                    params["path"] = scalar_value(&path);
                }
            }
            Ok(("browser.upload".to_string(), params, TextMode::Jsonish))
        }
        "type" => {
            let mut params =
                browser_target_params(command, target.as_deref(), &[("--text", "text")])?;
            params["selector"] = json!(command
                .get(sub_index + 1)
                .context("type requires selector")?);
            if !params.get("text").is_some_and(|value| !value.is_null()) {
                let text = command
                    .iter()
                    .skip(sub_index + 2)
                    .take_while(|arg| !arg.starts_with("--"))
                    .cloned()
                    .collect::<Vec<_>>()
                    .join(" ");
                params["text"] = json!(text);
            }
            if command_has_flag(command, "--snapshot-after") {
                params["snapshot_after"] = json!(true);
            }
            Ok(("browser.type".to_string(), params, TextMode::Jsonish))
        }
        "select" => {
            let mut params = browser_target_params(
                command,
                target.as_deref(),
                &[("--selector", "selector"), ("--value", "value")],
            )?;
            if !params.get("selector").is_some_and(|value| !value.is_null()) {
                params["selector"] = scalar_value(
                    command
                        .get(sub_index + 1)
                        .context("select requires selector")?,
                );
            }
            if !params.get("value").is_some_and(|value| !value.is_null()) {
                params["value"] = scalar_value(
                    command
                        .get(sub_index + 2)
                        .context("select requires value")?,
                );
            }
            Ok(("browser.select".to_string(), params, TextMode::Jsonish))
        }
        "press" | "key" | "keydown" | "keyup" => {
            let mut params =
                browser_target_params(command, target.as_deref(), &[("--key", "key")])?;
            if !params.get("key").is_some_and(|value| !value.is_null()) {
                if let Some(key) = command.get(sub_index + 1) {
                    params["key"] = scalar_value(key);
                }
            }
            let method = match sub {
                "key" => "browser.press",
                other => return Ok((format!("browser.{other}"), params, TextMode::Jsonish)),
            };
            Ok((method.to_string(), params, TextMode::Jsonish))
        }
        "scroll" => {
            let mut params = browser_target_params(
                command,
                target.as_deref(),
                &[("--dx", "dx"), ("--dy", "dy")],
            )?;
            if !params.get("dy").is_some_and(|value| !value.is_null()) {
                if let Some(dy) = command.get(sub_index + 1) {
                    params["dy"] = scalar_value(dy);
                }
            }
            Ok(("browser.scroll".to_string(), params, TextMode::Jsonish))
        }
        "bringtofront" | "bring-to-front" | "front" => Ok((
            "browser.bringtofront".to_string(),
            browser_target_params(command, target.as_deref(), &[])?,
            TextMode::Jsonish,
        )),
        "get" => {
            let family = command
                .get(sub_index + 1)
                .map(String::as_str)
                .unwrap_or("text");
            let mut params = browser_target_params(
                command,
                target.as_deref(),
                &[
                    ("--property", "property"),
                    ("--attr", "attr"),
                    ("--name", "attr"),
                ],
            )?;
            if let Some(selector) = command.get(sub_index + 2) {
                params["selector"] = json!(selector);
            }
            let method = match family {
                "url" => "browser.url.get",
                "title" => "browser.get.title",
                "count" => "browser.get.count",
                "box" => "browser.get.box",
                "attr" => {
                    if !params.get("attr").is_some_and(|value| !value.is_null()) {
                        params["attr"] = scalar_value(
                            command
                                .get(sub_index + 3)
                                .context("browser get attr requires attr name")?,
                        );
                    }
                    "browser.get.attr"
                }
                "value" => "browser.get.value",
                "styles" => "browser.get.styles",
                "text" => "browser.get.text",
                "html" => "browser.get.html",
                other => bail!("unsupported browser get type: {other}"),
            };
            Ok((method.to_string(), params, TextMode::Jsonish))
        }
        "is" => {
            let family = command
                .get(sub_index + 1)
                .map(String::as_str)
                .unwrap_or("visible");
            let mut params =
                browser_target_params(command, target.as_deref(), &[("--selector", "selector")])?;
            if !params.get("selector").is_some_and(|value| !value.is_null()) {
                if let Some(selector) = command.get(sub_index + 2) {
                    params["selector"] = scalar_value(selector);
                }
            }
            let method = match family {
                "visible" => "browser.is.visible",
                "enabled" => "browser.is.enabled",
                "checked" => "browser.is.checked",
                other => bail!("unsupported browser is type: {other}"),
            };
            Ok((method.to_string(), params, TextMode::Jsonish))
        }
        "download" => {
            let action = command
                .get(sub_index + 1)
                .map(String::as_str)
                .unwrap_or("wait");
            let mut params = browser_target_params(
                command,
                target.as_deref(),
                &[
                    ("--path", "path"),
                    ("--out", "path"),
                    ("--timeout-ms", "timeout_ms"),
                ],
            )?;
            if !params.get("path").is_some_and(|value| !value.is_null()) {
                if !matches!(action, "wait" | "save") && action.starts_with("--") {
                    bail!("download requires path");
                }
                let path_index = if matches!(action, "wait" | "save") {
                    sub_index + 2
                } else {
                    sub_index + 1
                };
                params["path"] = scalar_value(
                    command
                        .get(path_index)
                        .context("download wait requires path")?,
                );
            }
            Ok((
                "browser.download.wait".to_string(),
                params,
                TextMode::Jsonish,
            ))
        }
        "content" => {
            let action = command
                .get(sub_index + 1)
                .map(String::as_str)
                .unwrap_or("get");
            let mut params = browser_target_params(
                command,
                target.as_deref(),
                &[
                    ("--html", "html"),
                    ("--content", "content"),
                    ("--body", "body"),
                    ("--text", "text"),
                    ("--url", "url"),
                ],
            )?;
            if action == "set" {
                if !params.get("html").is_some_and(|value| !value.is_null())
                    && !params.get("content").is_some_and(|value| !value.is_null())
                    && !params.get("body").is_some_and(|value| !value.is_null())
                    && !params.get("text").is_some_and(|value| !value.is_null())
                {
                    let html = command
                        .iter()
                        .skip(sub_index + 2)
                        .cloned()
                        .collect::<Vec<_>>()
                        .join(" ");
                    if html.trim().is_empty() {
                        bail!("browser content set requires html");
                    }
                    params["html"] = json!(html);
                }
                Ok(("browser.setcontent".to_string(), params, TextMode::Jsonish))
            } else {
                Ok(("browser.content".to_string(), params, TextMode::Jsonish))
            }
        }
        "innertext" | "inner-text" => {
            let mut params = browser_target_params(command, target.as_deref(), &[])?;
            if let Some(selector) = command.get(sub_index + 1) {
                params["selector"] = json!(selector);
            }
            Ok(("browser.innertext".to_string(), params, TextMode::Jsonish))
        }
        "setcontent" | "set-content" => {
            let mut params = browser_target_params(
                command,
                target.as_deref(),
                &[
                    ("--html", "html"),
                    ("--content", "content"),
                    ("--body", "body"),
                    ("--text", "text"),
                    ("--url", "url"),
                ],
            )?;
            if !params.get("html").is_some_and(|value| !value.is_null())
                && !params.get("content").is_some_and(|value| !value.is_null())
                && !params.get("body").is_some_and(|value| !value.is_null())
                && !params.get("text").is_some_and(|value| !value.is_null())
            {
                let html = command
                    .iter()
                    .skip(sub_index + 1)
                    .cloned()
                    .collect::<Vec<_>>()
                    .join(" ");
                if html.trim().is_empty() {
                    bail!("setcontent requires html");
                }
                params["html"] = json!(html);
            }
            Ok(("browser.setcontent".to_string(), params, TextMode::Jsonish))
        }
        "setvalue" | "set-value" => {
            let mut params = browser_target_params(
                command,
                target.as_deref(),
                &[
                    ("--selector", "selector"),
                    ("--text", "text"),
                    ("--value", "value"),
                ],
            )?;
            if !params.get("selector").is_some_and(|value| !value.is_null()) {
                params["selector"] = scalar_value(
                    command
                        .get(sub_index + 1)
                        .context("setvalue requires selector")?,
                );
            }
            if !params.get("text").is_some_and(|value| !value.is_null())
                && !params.get("value").is_some_and(|value| !value.is_null())
            {
                if let Some(value) = command.get(sub_index + 2) {
                    params["value"] = scalar_value(value);
                }
            }
            Ok(("browser.setvalue".to_string(), params, TextMode::Jsonish))
        }
        "inserttext" | "insert-text" => {
            let mut params = browser_target_params(
                command,
                target.as_deref(),
                &[
                    ("--selector", "selector"),
                    ("--text", "text"),
                    ("--value", "value"),
                ],
            )?;
            if !params.get("selector").is_some_and(|value| !value.is_null()) {
                if let Some(selector) = command
                    .get(sub_index + 1)
                    .filter(|arg| !arg.starts_with("--") && command.get(sub_index + 2).is_some())
                {
                    params["selector"] = scalar_value(selector);
                }
            }
            if !params.get("text").is_some_and(|value| !value.is_null())
                && !params.get("value").is_some_and(|value| !value.is_null())
            {
                let start = if params.get("selector").is_some_and(|value| !value.is_null()) {
                    sub_index + 2
                } else {
                    sub_index + 1
                };
                let text = command
                    .iter()
                    .skip(start)
                    .cloned()
                    .collect::<Vec<_>>()
                    .join(" ");
                params["text"] = json!(text);
            }
            Ok(("browser.inserttext".to_string(), params, TextMode::Jsonish))
        }
        "selectall" | "select-all" => {
            let mut params = browser_target_params(command, target.as_deref(), &[])?;
            if let Some(selector) = command.get(sub_index + 1) {
                params["selector"] = json!(selector);
            }
            Ok(("browser.selectall".to_string(), params, TextMode::Jsonish))
        }
        "multiselect" | "multi-select" => {
            let mut params = browser_target_params(
                command,
                target.as_deref(),
                &[
                    ("--selector", "selector"),
                    ("--value", "value"),
                    ("--values", "values"),
                ],
            )?;
            if !params.get("selector").is_some_and(|value| !value.is_null()) {
                params["selector"] = scalar_value(
                    command
                        .get(sub_index + 1)
                        .context("multiselect requires selector")?,
                );
            }
            if !params.get("value").is_some_and(|value| !value.is_null())
                && !params.get("values").is_some_and(|value| !value.is_null())
            {
                let values = command
                    .iter()
                    .skip(sub_index + 2)
                    .filter(|arg| !arg.starts_with("--"))
                    .cloned()
                    .collect::<Vec<_>>();
                if values.is_empty() {
                    bail!("multiselect requires at least one value");
                }
                params["values"] = json!(values);
            }
            Ok(("browser.multiselect".to_string(), params, TextMode::Jsonish))
        }
        "clear" => {
            let mut params = browser_target_params(command, target.as_deref(), &[])?;
            if let Some(selector) = command.get(sub_index + 1) {
                params["selector"] = json!(selector);
            }
            Ok(("browser.clear".to_string(), params, TextMode::Jsonish))
        }
        "clipboard" => {
            let mut params = browser_target_params(
                command,
                target.as_deref(),
                &[
                    ("--action", "action"),
                    ("--operation", "operation"),
                    ("--text", "text"),
                    ("--value", "value"),
                ],
            )?;
            let action = command
                .get(sub_index + 1)
                .filter(|arg| {
                    matches!(
                        arg.as_str(),
                        "read" | "get" | "write" | "set" | "copy" | "clear" | "paste"
                    )
                })
                .cloned();
            if !params.get("action").is_some_and(|value| !value.is_null()) {
                if let Some(action) = action.clone() {
                    params["action"] = json!(action);
                }
            }
            if !params.get("text").is_some_and(|value| !value.is_null())
                && !params.get("value").is_some_and(|value| !value.is_null())
            {
                let start = if action.is_some() {
                    sub_index + 2
                } else {
                    sub_index + 1
                };
                let text = command
                    .iter()
                    .skip(start)
                    .cloned()
                    .collect::<Vec<_>>()
                    .join(" ");
                if !text.trim().is_empty() {
                    params["text"] = json!(text);
                }
            }
            Ok(("browser.clipboard".to_string(), params, TextMode::Jsonish))
        }
        "eval" => {
            let mut params =
                browser_target_params(command, target.as_deref(), &[("--script", "script")])?;
            if !params.get("script").is_some_and(|value| !value.is_null()) {
                let script = command
                    .iter()
                    .skip(sub_index + 1)
                    .cloned()
                    .collect::<Vec<_>>()
                    .join(" ");
                if script.trim().is_empty() {
                    bail!("eval requires script");
                }
                params["script"] = json!(script);
            }
            Ok(("browser.eval".to_string(), params, TextMode::Jsonish))
        }
        "evalhandle" | "eval-handle" => {
            let mut params =
                browser_target_params(command, target.as_deref(), &[("--script", "script")])?;
            if !params.get("script").is_some_and(|value| !value.is_null()) {
                let script = command
                    .iter()
                    .skip(sub_index + 1)
                    .cloned()
                    .collect::<Vec<_>>()
                    .join(" ");
                if script.trim().is_empty() {
                    bail!("evalhandle requires script");
                }
                params["script"] = json!(script);
            }
            Ok(("browser.evalhandle".to_string(), params, TextMode::Jsonish))
        }
        "expose" => {
            let mut params = browser_target_params(
                command,
                target.as_deref(),
                &[
                    ("--name", "name"),
                    ("--function-name", "name"),
                    ("--script", "script"),
                    ("--function", "function"),
                    ("--body", "body"),
                ],
            )?;
            if !params.get("name").is_some_and(|value| !value.is_null()) {
                params["name"] = scalar_value(
                    command
                        .get(sub_index + 1)
                        .context("expose requires function name")?,
                );
            }
            Ok(("browser.expose".to_string(), params, TextMode::Jsonish))
        }
        "dispatch" => {
            let mut params = browser_target_params(
                command,
                target.as_deref(),
                &[
                    ("--selector", "selector"),
                    ("--target", "selector"),
                    ("--event", "type"),
                    ("--type", "type"),
                    ("--value", "value"),
                ],
            )?;
            if !params.get("selector").is_some_and(|value| !value.is_null()) {
                params["selector"] = scalar_value(
                    command
                        .get(sub_index + 1)
                        .context("dispatch requires selector")?,
                );
            }
            if !params.get("type").is_some_and(|value| !value.is_null()) {
                if let Some(event_type) = command.get(sub_index + 2) {
                    params["type"] = scalar_value(event_type);
                }
            }
            if command_has_flag(command, "--snapshot-after") {
                params["snapshot_after"] = json!(true);
            }
            Ok(("browser.dispatch".to_string(), params, TextMode::Jsonish))
        }
        "tab" => {
            let action = command
                .get(sub_index + 1)
                .map(String::as_str)
                .unwrap_or("list");
            let mut params = browser_target_params(command, target.as_deref(), &[])?;
            match action {
                "list" => Ok(("browser.tab.list".to_string(), params, TextMode::Jsonish)),
                "new" => {
                    if let Some(url) = command.get(sub_index + 2) {
                        params["url"] = json!(url);
                    }
                    Ok(("browser.tab.new".to_string(), params, TextMode::Jsonish))
                }
                "switch" => {
                    params["target_surface_id"] = json!(command
                        .get(sub_index + 2)
                        .context("tab switch requires target")?);
                    Ok(("browser.tab.switch".to_string(), params, TextMode::Jsonish))
                }
                "close" => {
                    params["target_surface_id"] = json!(command
                        .get(sub_index + 2)
                        .context("tab close requires target")?);
                    Ok(("browser.tab.close".to_string(), params, TextMode::Jsonish))
                }
                other => bail!("unsupported browser tab action: {other}"),
            }
        }
        "addscript" | "addinitscript" | "addstyle" => {
            let mut params = browser_target_params(command, target.as_deref(), &[])?;
            let text = command.get(sub_index + 1).cloned().unwrap_or_default();
            let method = match sub {
                "addstyle" => {
                    params["css"] = json!(text);
                    "browser.addstyle"
                }
                "addinitscript" => {
                    params["script"] = json!(text);
                    "browser.addinitscript"
                }
                _ => {
                    params["script"] = json!(text);
                    "browser.addscript"
                }
            };
            Ok((method.to_string(), params, TextMode::Jsonish))
        }
        "console" => {
            let action = command
                .get(sub_index + 1)
                .map(String::as_str)
                .unwrap_or("list");
            let params = browser_target_params(command, target.as_deref(), &[])?;
            let method = match action {
                "clear" => "browser.console.clear",
                "show" | "open" => "browser.console.show",
                "list" | "ls" => "browser.console.list",
                other => bail!("unsupported browser console action: {other}"),
            };
            Ok((method.to_string(), params, TextMode::Jsonish))
        }
        "errors" => Ok((
            "browser.errors.list".to_string(),
            browser_target_params(command, target.as_deref(), &[])?,
            TextMode::Jsonish,
        )),
        "devtools" | "dev-tools" | "developer-tools" => Ok((
            "browser.devtools.toggle".to_string(),
            browser_target_params(command, target.as_deref(), &[])?,
            TextMode::Jsonish,
        )),
        "react-grab" | "react_grab" | "reactgrab" => Ok((
            "browser.react_grab.toggle".to_string(),
            browser_target_params(command, target.as_deref(), &[])?,
            TextMode::Jsonish,
        )),
        "focus-mode" | "focus_mode" | "focusmode" => {
            let mut params =
                browser_target_params(command, target.as_deref(), &[("--mode", "mode")])?;
            if !params.get("mode").is_some_and(|value| !value.is_null()) {
                if let Some(mode) = command
                    .get(sub_index + 1)
                    .filter(|arg| !arg.starts_with("--"))
                {
                    params["mode"] = json!(mode);
                }
            }
            Ok((
                "browser.focus_mode.set".to_string(),
                params,
                TextMode::Jsonish,
            ))
        }
        "zoom" => {
            let mut params =
                browser_target_params(command, target.as_deref(), &[("--direction", "direction")])?;
            if !params
                .get("direction")
                .is_some_and(|value| !value.is_null())
            {
                if let Some(direction) = command
                    .get(sub_index + 1)
                    .filter(|arg| !arg.starts_with("--"))
                {
                    params["direction"] = json!(direction);
                }
            }
            Ok(("browser.zoom.set".to_string(), params, TextMode::Jsonish))
        }
        "history" => {
            let action = command
                .get(sub_index + 1)
                .map(String::as_str)
                .unwrap_or("list");
            match action {
                "list" | "ls" | "search" => {
                    let mut params = Map::new();
                    if let Some(profile) = option_value(command, "--profile")
                        .or_else(|| option_value(command, "--profile-id"))
                    {
                        params.insert("profile".to_string(), scalar_value(&profile));
                    }
                    if let Some(limit) = option_value(command, "--limit") {
                        params.insert("limit".to_string(), scalar_value(&limit));
                    }
                    if let Some(query) = option_value(command, "--query")
                        .or_else(|| option_value(command, "--search"))
                        .or_else(|| {
                            (action == "search")
                                .then(|| command.get(sub_index + 2).cloned())
                                .flatten()
                        })
                    {
                        params.insert("query".to_string(), json!(query));
                    }
                    Ok((
                        "browser.history.list".to_string(),
                        Value::Object(params),
                        TextMode::Jsonish,
                    ))
                }
                "clear" => {
                    let mut params = browser_target_params(command, target.as_deref(), &[])?;
                    if command_has_flag(command, "--force") {
                        params["force"] = json!(true);
                    }
                    if let Some(profile) = option_value(command, "--profile")
                        .or_else(|| option_value(command, "--profile-id"))
                    {
                        params["profile"] = scalar_value(&profile);
                    }
                    Ok((
                        "browser.history.clear".to_string(),
                        params,
                        TextMode::Jsonish,
                    ))
                }
                other => bail!("unsupported browser history action: {other}"),
            }
        }
        "bookmarks" | "bookmark" => {
            let action = command
                .get(sub_index + 1)
                .map(String::as_str)
                .unwrap_or("list");
            match action {
                "list" | "ls" | "search" => {
                    let mut params = Map::new();
                    if let Some(profile) = option_value(command, "--profile")
                        .or_else(|| option_value(command, "--profile-id"))
                    {
                        params.insert("profile".to_string(), scalar_value(&profile));
                    }
                    if let Some(limit) = option_value(command, "--limit") {
                        params.insert("limit".to_string(), scalar_value(&limit));
                    }
                    if let Some(query) = option_value(command, "--query")
                        .or_else(|| option_value(command, "--search"))
                        .or_else(|| {
                            (action == "search")
                                .then(|| command.get(sub_index + 2).cloned())
                                .flatten()
                        })
                    {
                        params.insert("query".to_string(), json!(query));
                    }
                    Ok((
                        "browser.bookmarks.list".to_string(),
                        Value::Object(params),
                        TextMode::Jsonish,
                    ))
                }
                "clear" => {
                    let mut params = browser_target_params(command, target.as_deref(), &[])?;
                    if command_has_flag(command, "--force") {
                        params["force"] = json!(true);
                    }
                    if let Some(profile) = option_value(command, "--profile")
                        .or_else(|| option_value(command, "--profile-id"))
                    {
                        params["profile"] = scalar_value(&profile);
                    }
                    Ok((
                        "browser.bookmarks.clear".to_string(),
                        params,
                        TextMode::Jsonish,
                    ))
                }
                other => bail!("unsupported browser bookmarks action: {other}"),
            }
        }
        "highlight" => {
            let mut params = browser_target_params(command, target.as_deref(), &[])?;
            params["selector"] = json!(command
                .get(sub_index + 1)
                .context("highlight requires selector")?);
            Ok(("browser.highlight".to_string(), params, TextMode::Jsonish))
        }
        "state" => {
            let action = command
                .get(sub_index + 1)
                .map(String::as_str)
                .unwrap_or("save");
            let mut params = browser_target_params(command, target.as_deref(), &[])?;
            params["path"] = json!(command
                .get(sub_index + 2)
                .context("state path is required")?);
            let method = if action == "load" {
                "browser.state.load"
            } else {
                "browser.state.save"
            };
            Ok((method.to_string(), params, TextMode::Jsonish))
        }
        "viewport" => {
            let mut params = browser_target_params(
                command,
                target.as_deref(),
                &[
                    ("--width", "width"),
                    ("--height", "height"),
                    ("--scale", "device_scale_factor"),
                    ("--device-scale-factor", "device_scale_factor"),
                ],
            )?;
            if !params.get("width").is_some_and(|value| !value.is_null()) {
                if let Some(width) = command.get(sub_index + 1) {
                    params["width"] = scalar_value(width);
                }
            }
            if !params.get("height").is_some_and(|value| !value.is_null()) {
                if let Some(height) = command.get(sub_index + 2) {
                    params["height"] = scalar_value(height);
                }
            }
            Ok((
                "browser.viewport.set".to_string(),
                params,
                TextMode::Jsonish,
            ))
        }
        "device" => {
            if matches!(
                command.get(sub_index + 1).map(String::as_str),
                Some("list" | "devices" | "presets")
            ) {
                return Ok((
                    "browser.device.list".to_string(),
                    json!({}),
                    TextMode::Jsonish,
                ));
            }
            let mut params = browser_target_params(
                command,
                target.as_deref(),
                &[
                    ("--device", "device"),
                    ("--name", "device"),
                    ("--width", "width"),
                    ("--height", "height"),
                    ("--scale", "device_scale_factor"),
                    ("--device-scale-factor", "device_scale_factor"),
                    ("--user-agent", "user_agent"),
                    ("--useragent", "user_agent"),
                    ("--mobile", "mobile"),
                    ("--touch", "touch"),
                ],
            )?;
            if !params.get("device").is_some_and(|value| !value.is_null()) {
                if let Some(device) = command
                    .get(sub_index + 1)
                    .filter(|arg| !arg.starts_with("--"))
                {
                    params["device"] = scalar_value(device);
                }
            }
            if command_has_flag(command, "--mobile") {
                params["mobile"] = json!(true);
            }
            if command_has_flag(command, "--no-mobile") {
                params["mobile"] = json!(false);
            }
            if command_has_flag(command, "--touch") {
                params["touch"] = json!(true);
            }
            if command_has_flag(command, "--no-touch") {
                params["touch"] = json!(false);
            }
            Ok(("browser.device.set".to_string(), params, TextMode::Jsonish))
        }
        "offline" => {
            let mut params =
                browser_target_params(command, target.as_deref(), &[("--enabled", "enabled")])?;
            if !params.get("enabled").is_some_and(|value| !value.is_null()) {
                if let Some(enabled) = command.get(sub_index + 1) {
                    params["enabled"] = scalar_value(enabled);
                }
            }
            Ok(("browser.offline.set".to_string(), params, TextMode::Jsonish))
        }
        "media" => {
            let mut params = browser_target_params(
                command,
                target.as_deref(),
                &[
                    ("--type", "media_type"),
                    ("--media", "media_type"),
                    ("--color-scheme", "color_scheme"),
                    ("--scheme", "color_scheme"),
                    ("--reduced-motion", "reduced_motion"),
                    ("--motion", "reduced_motion"),
                ],
            )?;
            if !params
                .get("media_type")
                .is_some_and(|value| !value.is_null())
            {
                if let Some(media_type) = command
                    .get(sub_index + 1)
                    .filter(|arg| !arg.starts_with("--"))
                {
                    params["media_type"] = scalar_value(media_type);
                }
            }
            Ok(("browser.media.set".to_string(), params, TextMode::Jsonish))
        }
        "headers" => {
            let mut params = browser_target_params(
                command,
                target.as_deref(),
                &[
                    ("--header", "header"),
                    ("--name", "name"),
                    ("--key", "name"),
                    ("--value", "value"),
                ],
            )?;
            if command_has_flag(command, "--clear")
                || command.get(sub_index + 1).map(String::as_str) == Some("clear")
            {
                params["clear"] = json!(true);
            }
            if !params.get("header").is_some_and(|value| !value.is_null()) {
                if let Some(header) = command
                    .get(sub_index + 1)
                    .filter(|arg| !arg.starts_with("--") && arg.as_str() != "clear")
                {
                    params["header"] = scalar_value(header);
                }
            }
            Ok(("browser.headers.set".to_string(), params, TextMode::Jsonish))
        }
        "credentials" | "auth" => {
            let mut params = browser_target_params(
                command,
                target.as_deref(),
                &[
                    ("--username", "username"),
                    ("--user", "username"),
                    ("--name", "username"),
                    ("--password", "password"),
                    ("--pass", "password"),
                ],
            )?;
            if command_has_flag(command, "--clear")
                || command.get(sub_index + 1).map(String::as_str) == Some("clear")
            {
                params["clear"] = json!(true);
            }
            if !params.get("username").is_some_and(|value| !value.is_null()) {
                if let Some(username) = command
                    .get(sub_index + 1)
                    .filter(|arg| !arg.starts_with("--") && arg.as_str() != "clear")
                {
                    params["username"] = scalar_value(username);
                }
            }
            if !params.get("password").is_some_and(|value| !value.is_null()) {
                if let Some(password) = command
                    .get(sub_index + 2)
                    .filter(|arg| !arg.starts_with("--"))
                {
                    params["password"] = scalar_value(password);
                }
            }
            Ok((
                "browser.credentials.set".to_string(),
                params,
                TextMode::Jsonish,
            ))
        }
        "permissions" | "permission" => {
            let mut params = browser_target_params(
                command,
                target.as_deref(),
                &[
                    ("--permission", "permission"),
                    ("--name", "permission"),
                    ("--state", "state"),
                    ("--value", "state"),
                ],
            )?;
            if command_has_flag(command, "--clear")
                || command.get(sub_index + 1).map(String::as_str) == Some("clear")
            {
                params["clear"] = json!(true);
            }
            if !params
                .get("permission")
                .is_some_and(|value| !value.is_null())
            {
                if let Some(permission) = command
                    .get(sub_index + 1)
                    .filter(|arg| !arg.starts_with("--") && arg.as_str() != "clear")
                {
                    params["permission"] = scalar_value(permission);
                }
            }
            if !params.get("state").is_some_and(|value| !value.is_null()) {
                if let Some(state) = command
                    .get(sub_index + 2)
                    .filter(|arg| !arg.starts_with("--"))
                {
                    params["state"] = scalar_value(state);
                }
            }
            Ok((
                "browser.permissions.set".to_string(),
                params,
                TextMode::Jsonish,
            ))
        }
        "geolocation" | "geo" => {
            let mut params = browser_target_params(
                command,
                target.as_deref(),
                &[
                    ("--latitude", "latitude"),
                    ("--lat", "latitude"),
                    ("--longitude", "longitude"),
                    ("--lon", "longitude"),
                    ("--lng", "longitude"),
                    ("--accuracy", "accuracy"),
                ],
            )?;
            if !params.get("latitude").is_some_and(|value| !value.is_null()) {
                if let Some(latitude) = command.get(sub_index + 1) {
                    params["latitude"] = scalar_value(latitude);
                }
            }
            if !params
                .get("longitude")
                .is_some_and(|value| !value.is_null())
            {
                if let Some(longitude) = command.get(sub_index + 2) {
                    params["longitude"] = scalar_value(longitude);
                }
            }
            Ok((
                "browser.geolocation.set".to_string(),
                params,
                TextMode::Jsonish,
            ))
        }
        "useragent" | "user-agent" | "ua" => {
            let mut params = browser_target_params(
                command,
                target.as_deref(),
                &[
                    ("--user-agent", "user_agent"),
                    ("--useragent", "user_agent"),
                    ("--value", "user_agent"),
                ],
            )?;
            if !params
                .get("user_agent")
                .is_some_and(|value| !value.is_null())
            {
                params["user_agent"] = scalar_value(
                    command
                        .get(sub_index + 1)
                        .context("useragent requires value")?,
                );
            }
            Ok((
                "browser.useragent.set".to_string(),
                params,
                TextMode::Jsonish,
            ))
        }
        "locale" | "language" => {
            let mut params = browser_target_params(
                command,
                target.as_deref(),
                &[
                    ("--locale", "locale"),
                    ("--language", "locale"),
                    ("--value", "locale"),
                ],
            )?;
            if !params.get("locale").is_some_and(|value| !value.is_null()) {
                params["locale"] = scalar_value(
                    command
                        .get(sub_index + 1)
                        .context("locale requires value")?,
                );
            }
            Ok(("browser.locale.set".to_string(), params, TextMode::Jsonish))
        }
        "timezone" | "time-zone" | "tz" => {
            let mut params = browser_target_params(
                command,
                target.as_deref(),
                &[
                    ("--timezone", "timezone"),
                    ("--time-zone", "timezone"),
                    ("--value", "timezone"),
                ],
            )?;
            if !params.get("timezone").is_some_and(|value| !value.is_null()) {
                params["timezone"] = scalar_value(
                    command
                        .get(sub_index + 1)
                        .context("timezone requires value")?,
                );
            }
            Ok((
                "browser.timezone.set".to_string(),
                params,
                TextMode::Jsonish,
            ))
        }
        "network" => {
            let action = command
                .get(sub_index + 1)
                .map(String::as_str)
                .unwrap_or("requests");
            let mut params = browser_target_params(
                command,
                target.as_deref(),
                &[
                    ("--url", "url"),
                    ("--pattern", "url"),
                    ("--body", "body"),
                    ("--request-id", "request_id"),
                    ("--id", "request_id"),
                ],
            )?;
            match action {
                "route" => {
                    if !params.get("url").is_some_and(|value| !value.is_null()) {
                        params["url"] = json!(command
                            .get(sub_index + 2)
                            .context("network route requires url")?);
                    }
                    if command.iter().any(|arg| arg == "--abort") {
                        params["abort"] = json!(true);
                    }
                    Ok((
                        "browser.network.route".to_string(),
                        params,
                        TextMode::Jsonish,
                    ))
                }
                "unroute" => {
                    if !params.get("url").is_some_and(|value| !value.is_null()) {
                        params["url"] = json!(command
                            .get(sub_index + 2)
                            .context("network unroute requires url")?);
                    }
                    Ok((
                        "browser.network.unroute".to_string(),
                        params,
                        TextMode::Jsonish,
                    ))
                }
                "requests" | "list" => {
                    if command.iter().any(|arg| arg == "--clear") {
                        params["clear"] = json!(true);
                    }
                    Ok((
                        "browser.network.requests".to_string(),
                        params,
                        TextMode::Jsonish,
                    ))
                }
                "responsebody" | "response-body" | "body" => {
                    if !params
                        .get("request_id")
                        .is_some_and(|value| !value.is_null())
                    {
                        params["request_id"] = scalar_value(
                            command
                                .get(sub_index + 2)
                                .context("network responsebody requires request id")?,
                        );
                    }
                    Ok((
                        "browser.network.responsebody".to_string(),
                        params,
                        TextMode::Jsonish,
                    ))
                }
                other => bail!("unsupported browser network action: {other}"),
            }
        }
        "trace" => {
            let action = command
                .get(sub_index + 1)
                .map(String::as_str)
                .unwrap_or("start");
            let mut params = browser_target_params(command, target.as_deref(), &[])?;
            if let Some(path) =
                browser_artifact_path_arg(command, &["--path"], Some(sub_index + 2))?
            {
                params["path"] = json!(path);
            }
            let method = match action {
                "start" => "browser.trace.start",
                "stop" => "browser.trace.stop",
                other => bail!("unsupported browser trace action: {other}"),
            };
            Ok((method.to_string(), params, TextMode::Jsonish))
        }
        "har" => {
            let action = command
                .get(sub_index + 1)
                .map(String::as_str)
                .unwrap_or("start");
            let mut params = browser_target_params(command, target.as_deref(), &[])?;
            if let Some(path) =
                browser_artifact_path_arg(command, &["--path"], Some(sub_index + 2))?
            {
                params["path"] = json!(path);
            }
            let method = match action {
                "start" => "browser.har.start",
                "stop" => "browser.har.stop",
                other => bail!("unsupported browser har action: {other}"),
            };
            Ok((method.to_string(), params, TextMode::Jsonish))
        }
        "screencast" => {
            let action = command
                .get(sub_index + 1)
                .map(String::as_str)
                .unwrap_or("start");
            let method = match action {
                "start" => "browser.screencast.start",
                "stop" => "browser.screencast.stop",
                other => bail!("unsupported browser screencast action: {other}"),
            };
            Ok((
                method.to_string(),
                browser_target_params(command, target.as_deref(), &[])?,
                TextMode::Jsonish,
            ))
        }
        "input" => {
            let device = command
                .get(sub_index + 1)
                .map(String::as_str)
                .unwrap_or("keyboard");
            let method = match device {
                "mouse" => "browser.input_mouse",
                "keyboard" => "browser.input_keyboard",
                "touch" => "browser.input_touch",
                other => bail!("unsupported browser input device: {other}"),
            };
            let mut params = browser_target_params(command, target.as_deref(), &[])?;
            let args = browser_input_args_after(command, sub_index + 2);
            if !args.is_empty() {
                params["args"] = json!(args);
            }
            Ok((method.to_string(), params, TextMode::Jsonish))
        }
        "input_mouse" | "input_keyboard" | "input_touch" => {
            let mut params = browser_target_params(command, target.as_deref(), &[])?;
            let args = browser_input_args_after(command, sub_index + 1);
            if !args.is_empty() {
                params["args"] = json!(args);
            }
            Ok((format!("browser.{sub}"), params, TextMode::Jsonish))
        }
        "mouse" => {
            let mut params = browser_target_params(command, target.as_deref(), &[])?;
            let args = browser_input_args_after(command, sub_index + 1);
            if !args.is_empty() {
                params["args"] = json!(args);
            }
            Ok(("browser.input_mouse".to_string(), params, TextMode::Jsonish))
        }
        "tap" | "swipe" => {
            let mut params = browser_target_params(command, target.as_deref(), &[])?;
            let mut args = vec![sub.to_string()];
            args.extend(browser_input_args_after(command, sub_index + 1));
            params["args"] = json!(args);
            Ok(("browser.input_touch".to_string(), params, TextMode::Jsonish))
        }
        "keyboard" => {
            let mut params = browser_target_params(command, target.as_deref(), &[])?;
            let args = browser_input_args_after(command, sub_index + 1);
            if !args.is_empty() {
                params["args"] = json!(args);
            }
            Ok(("browser.keyboard".to_string(), params, TextMode::Jsonish))
        }
        "pause" => {
            let mut params = browser_target_params(
                command,
                target.as_deref(),
                &[
                    ("--duration-ms", "duration_ms"),
                    ("--timeout-ms", "timeout_ms"),
                    ("--ms", "ms"),
                ],
            )?;
            if !params
                .get("duration_ms")
                .is_some_and(|value| !value.is_null())
                && !params
                    .get("timeout_ms")
                    .is_some_and(|value| !value.is_null())
                && !params.get("ms").is_some_and(|value| !value.is_null())
            {
                if let Some(duration_ms) = command.get(sub_index + 1) {
                    params["duration_ms"] = scalar_value(duration_ms);
                }
            }
            Ok(("browser.pause".to_string(), params, TextMode::Jsonish))
        }
        "video" | "record" => {
            let action = command
                .get(sub_index + 1)
                .map(String::as_str)
                .unwrap_or("start");
            let mut params = browser_target_params(command, target.as_deref(), &[])?;
            if let Some(path) =
                browser_artifact_path_arg(command, &["--out", "--path"], Some(sub_index + 2))?
            {
                params["path"] = json!(path);
            }
            let method = match action {
                "start" if sub == "record" => "browser.record.start",
                "stop" if sub == "record" => "browser.record.stop",
                "restart" if sub == "record" => "browser.record.restart",
                "start" => "browser.video.start",
                "stop" => "browser.video.stop",
                other => bail!("unsupported browser {sub} action: {other}"),
            };
            Ok((method.to_string(), params, TextMode::Jsonish))
        }
        "wait" => Ok((
            "browser.wait".to_string(),
            browser_wait_params(command, target.as_deref(), sub_index)?,
            TextMode::Ok,
        )),
        "snapshot" => Ok((
            "browser.snapshot".to_string(),
            browser_target_params(command, target.as_deref(), &[])?,
            TextMode::BrowserSnapshot,
        )),
        "screenshot" => {
            let mut params = browser_target_params(command, target.as_deref(), &[])?;
            if command
                .iter()
                .any(|arg| matches!(arg.as_str(), "--full-page" | "--fullPage"))
            {
                params["full_page"] = json!(true);
            }
            Ok((
                "browser.screenshot".to_string(),
                params,
                TextMode::BrowserScreenshot {
                    out: browser_artifact_path_arg(
                        command,
                        &["--out", "--path"],
                        Some(sub_index + 1),
                    )?,
                    json_output: command.iter().any(|arg| arg == "--json"),
                },
            ))
        }
        "pdf" => {
            let out =
                browser_artifact_path_arg(command, &["--out", "--path"], Some(sub_index + 1))?;
            let mut params = browser_target_params(command, target.as_deref(), &[])?;
            if let Some(path) = &out {
                params["path"] = json!(path);
            }
            Ok((
                "browser.pdf".to_string(),
                params,
                TextMode::BrowserPdf {
                    out,
                    json_output: command.iter().any(|arg| arg == "--json"),
                },
            ))
        }
        "url" | "get-url" => Ok((
            "browser.url.get".to_string(),
            browser_target_params(command, target.as_deref(), &[])?,
            TextMode::Jsonish,
        )),
        "back" => Ok((
            "browser.back".to_string(),
            browser_target_params(command, target.as_deref(), &[])?,
            TextMode::Jsonish,
        )),
        "forward" => Ok((
            "browser.forward".to_string(),
            browser_target_params(command, target.as_deref(), &[])?,
            TextMode::Jsonish,
        )),
        "reload" => Ok((
            "browser.reload".to_string(),
            browser_target_params(command, target.as_deref(), &[])?,
            TextMode::Jsonish,
        )),
        other => bail!(
            "unsupported browser subcommand: {other}; run `cmux help browser` or `cmux docs browser` for supported Linux browser commands"
        ),
    }
}

fn browser_profiles_command_to_request(
    command: &[String],
    sub_index: usize,
) -> Result<(String, Value, TextMode)> {
    let action = command
        .get(sub_index + 1)
        .map(String::as_str)
        .unwrap_or("list");
    let positional = positional_args_after_skipping_options(
        command,
        sub_index + 2,
        &[
            "--name",
            "--profile",
            "--id",
            "--new-name",
            "--to",
            "--surface",
            "--surface-id",
        ],
    );
    match action {
        "list" | "ls" => Ok((
            "browser.profiles.list".to_string(),
            json!({}),
            TextMode::Jsonish,
        )),
        "create" | "add" | "new" => {
            let name = option_value(command, "--name")
                .or_else(|| positional.first().cloned())
                .context("browser profiles create requires a name")?;
            Ok((
                "browser.profiles.create".to_string(),
                json!({"name": name}),
                TextMode::Jsonish,
            ))
        }
        "select" | "switch" | "use" => {
            let profile = option_value(command, "--profile")
                .or_else(|| option_value(command, "--id"))
                .or_else(|| option_value(command, "--name"))
                .or_else(|| positional.first().cloned())
                .context("browser profiles select requires a profile")?;
            let mut params = Map::new();
            params.insert("profile".to_string(), scalar_value(&profile));
            if let Some(surface) =
                option_value(command, "--surface").or_else(|| option_value(command, "--surface-id"))
            {
                params.insert("surface_id".to_string(), scalar_value(&surface));
            }
            Ok((
                "browser.profiles.select".to_string(),
                Value::Object(params),
                TextMode::Jsonish,
            ))
        }
        "rename" | "mv" => {
            let profile = option_value(command, "--profile")
                .or_else(|| option_value(command, "--id"))
                .or_else(|| positional.first().cloned())
                .context("browser profiles rename requires a profile")?;
            let new_name = option_value(command, "--new-name")
                .or_else(|| option_value(command, "--to"))
                .or_else(|| option_value(command, "--name"))
                .or_else(|| positional.get(1).cloned())
                .context("browser profiles rename requires a new name")?;
            Ok((
                "browser.profiles.rename".to_string(),
                json!({"profile": profile, "new_name": new_name}),
                TextMode::Jsonish,
            ))
        }
        "clear" => {
            let mut params = Map::new();
            if command_has_flag(command, "--all") || command_has_flag(command, "--all-profiles") {
                params.insert("all".to_string(), json!(true));
                params.insert("all_profiles".to_string(), json!(true));
            }
            if command_has_flag(command, "--force") {
                params.insert("force".to_string(), json!(true));
            }
            if let Some(profile) = option_value(command, "--profile")
                .or_else(|| option_value(command, "--id"))
                .or_else(|| option_value(command, "--name"))
                .or_else(|| positional.first().cloned())
            {
                params.insert("profile".to_string(), scalar_value(&profile));
            }
            Ok((
                "browser.profiles.clear".to_string(),
                Value::Object(params),
                TextMode::Jsonish,
            ))
        }
        "delete" | "remove" | "rm" => {
            let profile = option_value(command, "--profile")
                .or_else(|| option_value(command, "--id"))
                .or_else(|| option_value(command, "--name"))
                .or_else(|| positional.first().cloned())
                .context("browser profiles delete requires a profile")?;
            Ok((
                "browser.profiles.delete".to_string(),
                json!({"profile": profile}),
                TextMode::Jsonish,
            ))
        }
        other => bail!("unsupported browser profiles action: {other}"),
    }
}

fn browser_set_command_to_request(
    command: &[String],
    target: Option<&str>,
    sub_index: usize,
) -> Result<(String, Value, TextMode)> {
    let family = command
        .get(sub_index + 1)
        .map(String::as_str)
        .context("browser set requires a family")?;
    let delegated_family = match family {
        "geo" => "geolocation",
        "auth" => "credentials",
        "user-agent" => "useragent",
        "time-zone" => "timezone",
        "set" => bail!("browser set requires a setting family"),
        other => other,
    };
    let mut delegated = vec!["browser".to_string()];
    if let Some(target) = target {
        delegated.push(target.to_string());
    }
    delegated.push(delegated_family.to_string());
    delegated.extend(command.iter().skip(sub_index + 2).cloned());
    browser_command_to_request(&delegated)
}

fn is_browser_subcommand(value: &str) -> bool {
    matches!(
        value,
        "open"
            | "open-split"
            | "new"
            | "connect"
            | "goto"
            | "navigate"
            | "window"
            | "find"
            | "frame"
            | "dialog"
            | "profiles"
            | "profile"
            | "import"
            | "cookies"
            | "storage"
            | "set"
            | "fill"
            | "type"
            | "click"
            | "dblclick"
            | "double-click"
            | "hover"
            | "focus"
            | "check"
            | "uncheck"
            | "drag"
            | "upload"
            | "scroll-into-view"
            | "scrollintoview"
            | "scrollinto"
            | "select"
            | "press"
            | "key"
            | "keydown"
            | "keyup"
            | "scroll"
            | "bringtofront"
            | "bring-to-front"
            | "front"
            | "get"
            | "is"
            | "download"
            | "content"
            | "innertext"
            | "inner-text"
            | "setcontent"
            | "set-content"
            | "setvalue"
            | "set-value"
            | "inserttext"
            | "insert-text"
            | "selectall"
            | "select-all"
            | "multiselect"
            | "multi-select"
            | "clear"
            | "clipboard"
            | "eval"
            | "evalhandle"
            | "eval-handle"
            | "expose"
            | "dispatch"
            | "tab"
            | "addscript"
            | "addinitscript"
            | "addstyle"
            | "console"
            | "errors"
            | "devtools"
            | "dev-tools"
            | "developer-tools"
            | "react-grab"
            | "react_grab"
            | "reactgrab"
            | "focus-mode"
            | "focus_mode"
            | "focusmode"
            | "zoom"
            | "history"
            | "bookmarks"
            | "bookmark"
            | "highlight"
            | "state"
            | "viewport"
            | "device"
            | "offline"
            | "media"
            | "geolocation"
            | "geo"
            | "useragent"
            | "user-agent"
            | "ua"
            | "locale"
            | "language"
            | "timezone"
            | "time-zone"
            | "tz"
            | "headers"
            | "credentials"
            | "auth"
            | "permissions"
            | "permission"
            | "network"
            | "trace"
            | "har"
            | "screencast"
            | "input"
            | "input_mouse"
            | "input_keyboard"
            | "input_touch"
            | "mouse"
            | "tap"
            | "swipe"
            | "keyboard"
            | "pause"
            | "video"
            | "record"
            | "pdf"
            | "wait"
            | "snapshot"
            | "screenshot"
            | "url"
            | "get-url"
            | "back"
            | "forward"
            | "reload"
            | "disable"
            | "enable"
            | "status"
            | "close"
            | "quit"
            | "exit"
            | "identify"
            | "focus-webview"
            | "focus_webview"
            | "is-webview-focused"
            | "is_webview_focused"
    )
}

fn browser_input_args_after(command: &[String], start: usize) -> Vec<String> {
    let mut out = Vec::new();
    let mut skip_next = false;
    let mut literal = false;
    for arg in command.iter().skip(start) {
        if literal {
            out.push(arg.clone());
            continue;
        }
        if skip_next {
            skip_next = false;
            continue;
        }
        if arg == "--" {
            literal = true;
            continue;
        }
        if arg == "--surface" {
            skip_next = true;
            continue;
        }
        if arg.starts_with("--") {
            continue;
        }
        out.push(arg.clone());
    }
    out
}

fn browser_target_params(
    command: &[String],
    target: Option<&str>,
    specs: &[(&str, &str)],
) -> Result<Value> {
    let mut combined = vec![("--surface", "surface_id"), ("--surface-id", "surface_id")];
    combined.extend_from_slice(specs);
    let mut params = option_params(command, &combined)?;
    if let Some(target) = target {
        params["surface_id"] = json!(target);
    }
    Ok(params)
}

fn browser_wait_params(
    command: &[String],
    target: Option<&str>,
    sub_index: usize,
) -> Result<Value> {
    let mut params = browser_target_params(
        command,
        target,
        &[
            ("--selector", "selector"),
            ("--text", "text_contains"),
            ("--text-contains", "text_contains"),
            ("--contains", "text_contains"),
            ("--url", "url_contains"),
            ("--url-contains", "url_contains"),
            ("--function", "function"),
            ("--predicate", "function"),
            ("--script", "function"),
            ("--load-state", "load_state"),
            ("--timeout-ms", "timeout_ms"),
        ],
    )?;
    if !params.get("selector").is_some_and(|value| !value.is_null()) {
        if let Some(selector) = first_positional_after(command, sub_index + 1) {
            params["selector"] = json!(selector);
        }
    }
    Ok(params)
}

fn option_params(command: &[String], specs: &[(&str, &str)]) -> Result<Value> {
    let mut params = Map::new();
    for (flag, key) in specs {
        if let Some(value) = option_value(command, flag) {
            params.insert((*key).to_string(), scalar_value(&value));
        }
    }
    Ok(Value::Object(params))
}

fn ssh_session_params(command: &[String], require_session: bool) -> Result<Value> {
    let mut params = option_params(
        command,
        &[
            ("--workspace", "workspace_id"),
            ("--workspace-id", "workspace_id"),
            ("--session", "session_id"),
            ("--session-id", "session_id"),
        ],
    )?;
    if command_has_flag(command, "--all") || command_has_flag(command, "--all-workspaces") {
        if require_session {
            bail!("--all is only supported by ssh-session-list");
        }
        params["all_workspaces"] = json!(true);
    }
    if !params
        .get("session_id")
        .is_some_and(|value| !value.is_null())
    {
        let positionals = positional_args_after_skipping_options(
            command,
            1,
            &[
                "--workspace",
                "--workspace-id",
                "--session",
                "--session-id",
                "--socket",
                "--password",
                "--id-format",
            ],
        );
        if let Some(session_id) = positionals.into_iter().find(|arg| {
            !matches!(
                arg.as_str(),
                "ssh-session-list" | "ssh-session-attach" | "ssh-session-cleanup"
            )
        }) {
            params["session_id"] = json!(session_id);
        }
    }
    if require_session
        && !params
            .get("session_id")
            .is_some_and(|value| !value.is_null())
    {
        bail!("session_id is required");
    }
    Ok(params)
}

fn ssh_session_snapshot_params(command: &[String]) -> Result<Value> {
    option_params(
        command,
        &[
            ("--workspace", "workspace_id"),
            ("--workspace-id", "workspace_id"),
        ],
    )
}

fn ssh_session_restore_snapshot_params(command: &[String]) -> Result<Value> {
    let snapshot = if let Some(raw) = option_value(command, "--snapshot") {
        serde_json::from_str::<Value>(&raw).context("--snapshot must be JSON")?
    } else if let Some(path) =
        option_value(command, "--file").or_else(|| option_value(command, "--path"))
    {
        let text = if path == "-" {
            let mut text = String::new();
            io::stdin().read_to_string(&mut text)?;
            text
        } else {
            fs::read_to_string(&path)
                .with_context(|| format!("failed to read SSH session snapshot {path}"))?
        };
        serde_json::from_str::<Value>(&text)
            .with_context(|| format!("SSH session snapshot JSON was invalid: {path}"))?
    } else if !io::stdin().is_terminal() {
        let mut text = String::new();
        io::stdin().read_to_string(&mut text)?;
        serde_json::from_str::<Value>(&text).context("SSH session snapshot stdin must be JSON")?
    } else {
        bail!("ssh-session-restore requires --file <path>, --snapshot <json>, or JSON on stdin");
    };
    Ok(json!({ "snapshot": snapshot }))
}

fn system_top_params(command: &[String]) -> Result<Value> {
    let mut params = option_params(
        command,
        &[
            ("--window", "window_id"),
            ("--workspace", "workspace_id"),
            ("--tab", "tab_id"),
        ],
    )?;
    if command_has_flag(command, "--all-windows") {
        params["all_windows"] = json!(true);
    }
    if command_has_flag(command, "--include-processes") || command_has_flag(command, "--processes")
    {
        params["include_processes"] = json!(true);
    }
    Ok(params)
}

fn tree_params(command: &[String]) -> Result<Value> {
    let mut params = option_params(
        command,
        &[
            ("--window", "window_id"),
            ("--workspace", "workspace_id"),
            ("--tab", "tab_id"),
        ],
    )?;
    if command_has_flag(command, "--all-windows") || command_has_flag(command, "--all") {
        params["all_windows"] = json!(true);
    }
    Ok(params)
}

fn system_memory_params(command: &[String]) -> Result<Value> {
    let mut params = option_params(
        command,
        &[
            ("--window", "window_id"),
            ("--workspace", "workspace_id"),
            ("--tab", "tab_id"),
            ("--top-group-limit", "top_group_limit"),
            ("--group-limit", "group_limit"),
        ],
    )?;
    if command_has_flag(command, "--all-windows") {
        params["all_windows"] = json!(true);
    }
    Ok(params)
}

fn equalize_splits_params(command: &[String]) -> Result<Value> {
    option_params(
        command,
        &[
            ("--workspace", "workspace_id"),
            ("--surface", "surface_id"),
            ("--panel", "surface_id"),
            ("--pane", "pane_id"),
            ("--orientation", "orientation"),
        ],
    )
}

fn window_command_to_request(command: &[String]) -> Result<(String, Value, TextMode)> {
    match command.get(1).map(String::as_str).unwrap_or("help") {
        "displays" => Ok(("window.displays".to_string(), json!({}), TextMode::Jsonish)),
        "default-display" => {
            bail!("cmux window default-display is handled without a socket")
        }
        "display" => {
            if command_has_flag(command, "--list") || command_has_flag(command, "-l") {
                return Ok(("window.displays".to_string(), json!({}), TextMode::Jsonish));
            }
            let mut params = option_params(
                command,
                &[("--window", "window_id"), ("--display", "display")],
            )?;
            if params.get("display").is_none() {
                if let Some(display) = first_positional_after(command, 2) {
                    params["display"] = scalar_value(&display);
                }
            }
            Ok(("window.display".to_string(), params, TextMode::Jsonish))
        }
        "list" => Ok(("window.list".to_string(), json!({}), TextMode::Jsonish)),
        "current" => Ok(("window.current".to_string(), json!({}), TextMode::Jsonish)),
        other => bail!("unknown window command: {other}"),
    }
}

fn vm_command_to_request(command: &[String]) -> Result<(String, Value, TextMode)> {
    let subcommand = command.get(1).map(String::as_str).unwrap_or("help");
    match subcommand {
        "ls" | "list" => Ok(("vm.list".to_string(), json!({}), TextMode::Jsonish)),
        "new" | "create" => Ok((
            "vm.create".to_string(),
            vm_create_params(command)?,
            TextMode::Jsonish,
        )),
        "rm" | "destroy" | "delete" => Ok((
            "vm.destroy".to_string(),
            vm_id_params(command, 2, "cmux vm rm requires a VM id")?,
            TextMode::Jsonish,
        )),
        "exec" => Ok((
            "vm.exec".to_string(),
            vm_exec_params(command)?,
            TextMode::Jsonish,
        )),
        "ssh-info" => Ok((
            "vm.ssh_info".to_string(),
            vm_id_params(command, 2, "cmux vm ssh-info requires a VM id")?,
            TextMode::Jsonish,
        )),
        "shell" | "attach" | "ssh" => Ok((
            "vm.attach_info".to_string(),
            vm_attach_params(command)?,
            TextMode::Jsonish,
        )),
        "help" => {
            print_command_help(command.first().map(String::as_str).unwrap_or("vm"));
            std::process::exit(0);
        }
        other => bail!("Unknown vm subcommand: {other}"),
    }
}

fn vm_create_params(command: &[String]) -> Result<Value> {
    let mut params = option_params(
        command,
        &[
            ("--provider", "provider"),
            ("--image", "image"),
            ("--idempotency-key", "idempotency_key"),
        ],
    )?;
    if params.get("idempotency_key").is_none() {
        params["idempotency_key"] = json!(generated_vm_idempotency_key());
    }
    if command_has_flag(command, "--detach") || command_has_flag(command, "-d") {
        params["detach"] = json!(true);
    }
    Ok(params)
}

fn generated_vm_idempotency_key() -> String {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or(0);
    format!("linux-cli-{}-{nanos}", std::process::id())
}

fn vm_id_params(command: &[String], start: usize, missing_message: &'static str) -> Result<Value> {
    let id = option_value(command, "--id")
        .or_else(|| option_value(command, "--vm"))
        .or_else(|| first_positional_after(command, start))
        .context(missing_message)?;
    Ok(json!({"id": id}))
}

fn vm_exec_params(command: &[String]) -> Result<Value> {
    let mut params = vm_id_params(command, 2, "cmux vm exec requires a VM id")?;
    if let Some(timeout_ms) =
        option_value(command, "--timeout-ms").or_else(|| option_value(command, "--timeout"))
    {
        params["timeout_ms"] = scalar_value(&timeout_ms);
    }
    let command_text =
        vm_exec_command_text(command).context("cmux vm exec requires a command after --")?;
    params["command"] = json!(command_text);
    Ok(params)
}

fn vm_exec_command_text(command: &[String]) -> Option<String> {
    if let Some(index) = command.iter().position(|arg| arg == "--") {
        let args = command.iter().skip(index + 1).cloned().collect::<Vec<_>>();
        if !args.is_empty() {
            return Some(shell_join_args(&args));
        }
    }
    let id = option_value(command, "--id")
        .or_else(|| option_value(command, "--vm"))
        .or_else(|| first_positional_after(command, 2))?;
    let mut args = Vec::new();
    let mut skip_next = false;
    for arg in command.iter().skip(2) {
        if skip_next {
            skip_next = false;
            continue;
        }
        if arg == &id {
            continue;
        }
        match arg.as_str() {
            "--id" | "--vm" | "--timeout-ms" | "--timeout" => {
                skip_next = true;
            }
            "--json" => {}
            other if other.starts_with('-') => {}
            other => args.push(other.to_string()),
        }
    }
    (!args.is_empty()).then(|| shell_join_args(&args))
}

fn vm_attach_params(command: &[String]) -> Result<Value> {
    let mut params = vm_id_params(command, 2, "cmux vm attach requires a VM id")?;
    if command_has_flag(command, "--require-daemon")
        || command.get(1).map(String::as_str) == Some("ssh")
    {
        params["require_daemon"] = json!(true);
    }
    Ok(params)
}

fn workspace_option_params(command: &[String], specs: &[(&str, &str)]) -> Result<Value> {
    let mut params = option_params(command, specs)?;
    add_env_default(&mut params, "workspace_id", "CMUX_WORKSPACE_ID");
    Ok(params)
}

fn surface_option_params(command: &[String]) -> Result<Value> {
    let mut params = option_params(
        command,
        &[
            ("--workspace", "workspace_id"),
            ("--surface", "surface_id"),
            ("--panel", "surface_id"),
        ],
    )?;
    add_env_default(&mut params, "workspace_id", "CMUX_WORKSPACE_ID");
    add_env_surface_default(&mut params);
    Ok(params)
}

fn add_env_default(params: &mut Value, key: &str, env_key: &str) {
    let Some(obj) = params.as_object_mut() else {
        return;
    };
    if obj.contains_key(key) {
        return;
    }
    if let Some(value) = normalized_env(env_key) {
        obj.insert(key.to_string(), json!(value));
    }
}

fn add_env_surface_default(params: &mut Value) {
    add_env_default(params, "surface_id", "CMUX_TAB_ID");
    add_env_default(params, "surface_id", "CMUX_PANEL_ID");
    add_env_default(params, "surface_id", "CMUX_SURFACE_ID");
}

fn trailing_title(command: &[String]) -> Option<String> {
    let mut out = None;
    let mut skip_next = false;
    let mut literal = false;
    for arg in command.iter().skip(1) {
        if literal {
            out = Some(arg.clone());
            continue;
        }
        if skip_next {
            skip_next = false;
            continue;
        }
        if arg == "--" {
            literal = true;
            continue;
        }
        if arg.starts_with("--") {
            skip_next = true;
            continue;
        }
        out = Some(arg.clone());
    }
    out.map(|title| title.trim().to_string())
        .filter(|title| !title.is_empty())
}

fn positional_args(command: &[String]) -> Vec<String> {
    positional_arg_indices(command)
        .into_iter()
        .map(|index| command[index].clone())
        .collect()
}

fn positional_args_after_skipping_options(
    command: &[String],
    start: usize,
    value_flags: &[&str],
) -> Vec<String> {
    let mut out = Vec::new();
    let mut index = start;
    let mut literal = false;
    while index < command.len() {
        let arg = &command[index];
        if literal {
            out.push(arg.clone());
            index += 1;
            continue;
        }
        if arg == "--" {
            literal = true;
            index += 1;
            continue;
        }
        if value_flags.contains(&arg.as_str()) {
            index += 2;
            continue;
        }
        if arg.starts_with("--") {
            index += 1;
            continue;
        }
        out.push(arg.clone());
        index += 1;
    }
    out
}

fn positional_arg_indices(command: &[String]) -> Vec<usize> {
    let mut out = Vec::new();
    let mut skip_next = false;
    let mut literal = false;
    for (index, arg) in command.iter().enumerate().skip(1) {
        if literal {
            out.push(index);
            continue;
        }
        if skip_next {
            skip_next = false;
            continue;
        }
        if arg == "--" {
            literal = true;
            continue;
        }
        if arg.starts_with("--") {
            skip_next = true;
            continue;
        }
        out.push(index);
    }
    out
}

fn tmux_positional_args(command: &[String]) -> Vec<String> {
    let mut out = Vec::new();
    let mut skip_next = false;
    let mut literal = false;
    for arg in command.iter().skip(1) {
        if literal {
            out.push(arg.clone());
            continue;
        }
        if skip_next {
            skip_next = false;
            continue;
        }
        if arg == "--" {
            literal = true;
            continue;
        }
        if matches!(
            arg.as_str(),
            "--command"
                | "--title"
                | "--workspace"
                | "-T"
                | "-E"
                | "-t"
                | "-d"
                | "-w"
                | "-h"
                | "-x"
                | "-y"
        ) {
            skip_next = true;
            continue;
        }
        if arg.starts_with("--") {
            skip_next = true;
            continue;
        }
        out.push(arg.clone());
    }
    out
}

fn collect_option(
    command: &[String],
    flag: &str,
    key: &str,
    params: &mut Map<String, Value>,
) -> Result<()> {
    if let Some(value) = option_value(command, flag) {
        params.insert(key.to_string(), scalar_value(&value));
    }
    Ok(())
}

fn option_value(command: &[String], flag: &str) -> Option<String> {
    let mut iter = command.iter().peekable();
    while let Some(arg) = iter.next() {
        if arg == "--" {
            return None;
        }
        if arg == flag {
            return iter.peek().map(|value| (*value).clone().to_string());
        }
    }
    None
}

fn required_option_value(command: &[String], flag: &str) -> Result<Option<String>> {
    for (index, arg) in command.iter().enumerate() {
        if arg == "--" {
            return Ok(None);
        }
        if arg == flag {
            let Some(value) = command
                .get(index + 1)
                .filter(|value| !value.starts_with("--"))
            else {
                bail!("{flag} requires core, gtk, ghostty, or ghostty-vt");
            };
            return Ok(Some(value.clone()));
        }
    }
    Ok(None)
}

fn scalar_value(value: &str) -> Value {
    match value {
        "true" => Value::Bool(true),
        "false" => Value::Bool(false),
        _ => value
            .parse::<i64>()
            .map(Value::from)
            .unwrap_or_else(|_| json!(value)),
    }
}

fn shell_join_args(args: &[String]) -> String {
    args.iter()
        .map(|arg| shell_quote_arg(arg))
        .collect::<Vec<_>>()
        .join(" ")
}

fn shell_quote_arg(value: &str) -> String {
    if value.chars().all(|ch| {
        ch.is_ascii_alphanumeric() || matches!(ch, '/' | '.' | '_' | '-' | ':' | '=' | '%' | '@')
    }) {
        return value.to_string();
    }
    format!("'{}'", value.replace('\'', "'\\''"))
}

fn first_positional_after(command: &[String], start: usize) -> Option<String> {
    let mut skip_next = false;
    let mut literal = false;
    for arg in command.iter().skip(start) {
        if literal {
            return Some(arg.clone());
        }
        if skip_next {
            skip_next = false;
            continue;
        }
        if arg == "--" {
            literal = true;
            continue;
        }
        if arg.starts_with("--") {
            skip_next = true;
            continue;
        }
        return Some(arg.clone());
    }
    None
}

fn browser_artifact_path_arg(
    command: &[String],
    flags: &[&str],
    positional_start: Option<usize>,
) -> Result<Option<String>> {
    for flag in flags {
        if let Some(value) = browser_artifact_option_value(command, flag)? {
            return Ok(Some(value));
        }
    }
    Ok(positional_start.and_then(|start| first_positional_after(command, start)))
}

fn browser_artifact_option_value(command: &[String], flag: &str) -> Result<Option<String>> {
    let equals_prefix = format!("{flag}=");
    let mut index = 0;
    while index < command.len() {
        let arg = &command[index];
        if arg == "--" {
            return Ok(None);
        }
        if let Some(value) = arg.strip_prefix(&equals_prefix) {
            if value.is_empty() {
                bail!("{flag} requires a value");
            }
            return Ok(Some(value.to_string()));
        }
        if arg == flag {
            let value = command
                .get(index + 1)
                .with_context(|| format!("{flag} requires a value"))?;
            if value == "--" || value.starts_with("--") {
                bail!("{flag} requires a value");
            }
            return Ok(Some(value.clone()));
        }
        index += 1;
    }
    Ok(None)
}

fn last_positional(command: &[String]) -> Option<String> {
    let mut out = None;
    let mut skip_next = false;
    let mut literal = false;
    for arg in command.iter().skip(1) {
        if literal {
            out = Some(arg.clone());
            continue;
        }
        if skip_next {
            skip_next = false;
            continue;
        }
        if arg == "--" {
            literal = true;
            continue;
        }
        if arg.starts_with("--") {
            skip_next = true;
            continue;
        }
        out = Some(arg.clone());
    }
    out
}

fn call_socket(socket_path: &str, method: &str, params: Value) -> Result<Value> {
    if let Some((host, port)) = parse_tcp_socket_addr(socket_path) {
        let stream = TcpStream::connect((host.as_str(), port))
            .with_context(|| format!("failed to connect to cmux TCP socket {socket_path}"))?;
        return call_stream(stream, method, params);
    }
    let stream = UnixStream::connect(socket_path)
        .with_context(|| format!("failed to connect to cmux socket {socket_path}"))?;
    call_stream(stream, method, params)
}

fn parse_tcp_socket_addr(socket_path: &str) -> Option<(String, u16)> {
    let (host, port) = socket_path.rsplit_once(':')?;
    if host.is_empty() || host.contains('/') {
        return None;
    }
    let port = port.parse::<u16>().ok()?;
    if port == 0 {
        return None;
    }
    Some((host.to_string(), port))
}

fn call_stream<S>(mut stream: S, method: &str, params: Value) -> Result<Value>
where
    S: ReadWrite,
{
    let request = json!({"id": 1, "method": method, "params": params});
    serde_json::to_writer(&mut stream, &request)?;
    stream.write_all(b"\n")?;
    stream.flush()?;

    let mut reader = BufReader::new(stream);
    let mut line = String::new();
    reader.read_line(&mut line)?;
    let response: Value = serde_json::from_str(&line).context("invalid socket response JSON")?;
    if response.get("ok").and_then(Value::as_bool) == Some(true) {
        Ok(response.get("result").cloned().unwrap_or_else(|| json!({})))
    } else {
        let err = response.get("error").unwrap_or(&Value::Null);
        let code = err.get("code").and_then(Value::as_str).unwrap_or("error");
        let message = err
            .get("message")
            .and_then(Value::as_str)
            .unwrap_or("Unknown error");
        bail!("{code}: {message}");
    }
}

struct EventsOptions {
    after_seq: Option<i64>,
    cursor_file: Option<String>,
    names: Vec<String>,
    categories: Vec<String>,
    reconnect: bool,
    limit: Option<usize>,
    print_ack: bool,
    print_heartbeats: bool,
}

impl Default for EventsOptions {
    fn default() -> Self {
        Self {
            after_seq: None,
            cursor_file: None,
            names: Vec::new(),
            categories: Vec::new(),
            reconnect: false,
            limit: None,
            print_ack: true,
            print_heartbeats: true,
        }
    }
}

fn run_events_command(socket_path: &str, command: &[String]) -> Result<()> {
    let mut options = parse_events_options(command)?;
    if options.after_seq.is_none() {
        if let Some(cursor_file) = options.cursor_file.as_deref() {
            options.after_seq = read_events_cursor(cursor_file)?;
        }
    }

    loop {
        let result = if let Some((host, port)) = parse_tcp_socket_addr(socket_path) {
            let stream = TcpStream::connect((host.as_str(), port))
                .with_context(|| format!("failed to connect to cmux TCP socket {socket_path}"))?;
            stream_events_once(stream, &options)
        } else {
            let stream = UnixStream::connect(socket_path)
                .with_context(|| format!("failed to connect to cmux socket {socket_path}"))?;
            stream_events_once(stream, &options)
        };
        match result {
            Ok(()) => return Ok(()),
            Err(err) if options.reconnect && is_transient_events_error(&err) => {
                thread::sleep(Duration::from_secs(1));
                continue;
            }
            Err(err) => return Err(err),
        }
    }
}

fn stream_events_once<S>(mut stream: S, options: &EventsOptions) -> Result<()>
where
    S: ReadWrite,
{
    let mut params = Map::new();
    params.insert("include_heartbeats".to_string(), json!(true));
    if let Some(after_seq) = options.after_seq {
        params.insert("after_seq".to_string(), json!(after_seq));
    }
    if !options.names.is_empty() {
        params.insert("names".to_string(), json!(options.names));
    }
    if !options.categories.is_empty() {
        params.insert("categories".to_string(), json!(options.categories));
    }

    let request = json!({
        "id": events_request_id(),
        "method": "events.stream",
        "params": Value::Object(params)
    });
    serde_json::to_writer(&mut stream, &request).context("failed to write stream request")?;
    stream
        .write_all(b"\n")
        .context("failed to write stream newline")?;
    stream.flush().context("failed to flush stream request")?;

    let mut reader = BufReader::new(stream);
    let mut line = String::new();
    let mut emitted_events = 0_usize;
    loop {
        line.clear();
        let read = reader
            .read_line(&mut line)
            .context("event stream socket read error")?;
        if read == 0 {
            bail!("event stream closed");
        }
        let trimmed = line.trim_end_matches(['\r', '\n']);
        if trimmed.is_empty() {
            continue;
        }
        let frame: Value = serde_json::from_str(trimmed)
            .with_context(|| format!("Invalid event stream frame: {trimmed}"))?;
        if frame.get("ok").and_then(Value::as_bool) == Some(false) {
            let message = frame
                .get("error")
                .and_then(|error| error.get("message"))
                .and_then(Value::as_str)
                .unwrap_or("event stream error");
            bail!("{message}");
        }
        let frame_type = frame.get("type").and_then(Value::as_str).unwrap_or("");
        if frame_type == "ack" && !options.print_ack {
            continue;
        }
        if frame_type == "heartbeat" && !options.print_heartbeats {
            continue;
        }
        println!("{trimmed}");
        io::stdout().flush().ok();
        if frame_type == "event" {
            let seq = frame
                .get("seq")
                .and_then(Value::as_i64)
                .context("Invalid event stream frame: event missing numeric seq")?;
            if let Some(cursor_file) = options.cursor_file.as_deref() {
                write_events_cursor(seq, cursor_file)?;
            }
            emitted_events += 1;
            if options.limit.is_some_and(|limit| emitted_events >= limit) {
                return Ok(());
            }
        }
    }
}

fn parse_events_options(command: &[String]) -> Result<EventsOptions> {
    let mut options = EventsOptions::default();
    let mut index = 1;
    while index < command.len() {
        let arg = &command[index];
        match arg.as_str() {
            "--after" | "--after-seq" => {
                let raw = events_option_value(command, &mut index, arg)?;
                let seq = raw
                    .parse::<i64>()
                    .with_context(|| format!("{arg} must be a non-negative integer"))?;
                if seq < 0 {
                    bail!("{arg} must be a non-negative integer");
                }
                options.after_seq = Some(seq);
            }
            "--cursor-file" => {
                options.cursor_file = Some(events_option_value(command, &mut index, arg)?);
            }
            "--name" => {
                options
                    .names
                    .push(events_option_value(command, &mut index, arg)?);
            }
            "--category" => {
                options
                    .categories
                    .push(events_option_value(command, &mut index, arg)?);
            }
            "--reconnect" => {
                options.reconnect = true;
            }
            "--limit" => {
                let raw = events_option_value(command, &mut index, arg)?;
                let limit = raw
                    .parse::<usize>()
                    .context("--limit must be greater than 0")?;
                if limit == 0 {
                    bail!("--limit must be greater than 0");
                }
                options.limit = Some(limit);
            }
            "--no-ack" => {
                options.print_ack = false;
            }
            "--no-heartbeat" | "--no-heartbeats" => {
                options.print_heartbeats = false;
            }
            "--json" => {}
            other => bail!("Unknown events option: {other}"),
        }
        index += 1;
    }
    Ok(options)
}

fn events_option_value(command: &[String], index: &mut usize, arg: &str) -> Result<String> {
    *index += 1;
    command
        .get(*index)
        .cloned()
        .with_context(|| format!("{arg} requires a value"))
}

fn events_request_id() -> String {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or(0);
    format!("events-{nanos}")
}

fn read_events_cursor(path: &str) -> Result<Option<i64>> {
    let path = expand_tilde_path(path)?;
    if !path.exists() {
        return Ok(None);
    }
    let text = fs::read_to_string(&path)
        .with_context(|| format!("Failed to read events cursor file {}", path.display()))?;
    let trimmed = text.trim();
    let sequence = trimmed.parse::<i64>().with_context(|| {
        format!(
            "Malformed events cursor file {}: expected a non-negative sequence number",
            path.display()
        )
    })?;
    if sequence < 0 {
        bail!(
            "Malformed events cursor file {}: expected a non-negative sequence number",
            path.display()
        );
    }
    Ok(Some(sequence))
}

fn write_events_cursor(seq: i64, path: &str) -> Result<()> {
    let path = expand_tilde_path(path)?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("failed to create cursor parent {}", parent.display()))?;
    }
    fs::write(&path, format!("{seq}\n"))
        .with_context(|| format!("failed to write events cursor file {}", path.display()))?;
    Ok(())
}

fn expand_tilde_path(path: &str) -> Result<PathBuf> {
    if path == "~" {
        return std::env::var("HOME")
            .map(PathBuf::from)
            .context("HOME is required to expand ~");
    }
    if let Some(rest) = path.strip_prefix("~/") {
        return std::env::var("HOME")
            .map(|home| PathBuf::from(home).join(rest))
            .context("HOME is required to expand ~/");
    }
    Ok(PathBuf::from(path))
}

fn is_transient_events_error(err: &anyhow::Error) -> bool {
    let message = err.to_string().to_ascii_lowercase();
    [
        "socket not found",
        "failed to connect",
        "event stream closed",
        "event stream socket read error",
        "timed out",
        "broken pipe",
        "connection reset",
        "connection refused",
    ]
    .iter()
    .any(|marker| message.contains(marker))
}

trait ReadWrite: std::io::Read + std::io::Write {}
impl<T: std::io::Read + std::io::Write> ReadWrite for T {}

fn format_ids(value: Value, format: IdFormat) -> Value {
    match value {
        Value::Array(items) => {
            Value::Array(items.into_iter().map(|v| format_ids(v, format)).collect())
        }
        Value::Object(obj) => {
            let original = obj.clone();
            let mut out = Map::new();
            for (key, value) in obj {
                let has_ref_twin = key == "id" && original.contains_key("ref")
                    || key.ends_with("_id")
                        && original.contains_key(&format!("{}_ref", key.trim_end_matches("_id")));
                let has_id_twin = key == "ref" && original.contains_key("id")
                    || key.ends_with("_ref")
                        && original.contains_key(&format!("{}_id", key.trim_end_matches("_ref")));
                if format == IdFormat::Refs && has_ref_twin {
                    continue;
                }
                if format == IdFormat::Uuids && has_id_twin {
                    continue;
                }
                out.insert(key, format_ids(value, format));
            }
            Value::Object(out)
        }
        other => other,
    }
}

fn print_text_response(command: &str, value: &Value, mode: TextMode) -> Result<()> {
    match mode {
        TextMode::Pong => {
            println!("pong");
        }
        TextMode::Ok => {
            println!("OK");
        }
        TextMode::OkRef(key) => {
            let text = value
                .get(key)
                .and_then(Value::as_str)
                .or_else(|| value.get("ref").and_then(Value::as_str))
                .or_else(|| value.get("id").and_then(Value::as_str))
                .unwrap_or("ok");
            println!("OK {text}");
        }
        TextMode::MarkdownOpen => {
            let surface = value
                .get("surface_ref")
                .and_then(Value::as_str)
                .or_else(|| value.get("surface_id").and_then(Value::as_str))
                .unwrap_or("unknown");
            let pane = value
                .get("pane_ref")
                .and_then(Value::as_str)
                .or_else(|| value.get("pane_id").and_then(Value::as_str))
                .unwrap_or("unknown");
            let path = value.get("path").and_then(Value::as_str).unwrap_or("");
            println!("OK surface={surface} pane={pane} path={path}");
        }
        TextMode::DiffOpen => {
            let surface = value
                .get("surface_ref")
                .and_then(Value::as_str)
                .or_else(|| value.get("surface_id").and_then(Value::as_str))
                .unwrap_or("unknown");
            let pane = value
                .get("pane_ref")
                .and_then(Value::as_str)
                .or_else(|| value.get("pane_id").and_then(Value::as_str))
                .unwrap_or("unknown");
            println!("OK surface={surface} pane={pane}");
        }
        TextMode::TabAction => {
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
            let title = value.get("title").and_then(Value::as_str).unwrap_or("");
            let mut parts = vec![format!("OK action={action}"), format!("tab={tab}")];
            if !title.is_empty() {
                parts.push(format!("title={title}"));
            }
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
            if let Some(closed) = value.get("closed").and_then(Value::as_i64) {
                parts.push(format!("closed={closed}"));
            }
            if let Some(skipped) = value.get("skipped_pinned").and_then(Value::as_i64) {
                parts.push(format!("skipped_pinned={skipped}"));
            }
            println!("{}", parts.join(" "));
        }
        TextMode::Text => {
            print!(
                "{}",
                value
                    .get("text")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
            );
            if value
                .get("text")
                .and_then(Value::as_str)
                .is_some_and(|text| !text.ends_with('\n'))
            {
                println!();
            }
        }
        TextMode::BrowserSnapshot => {
            println!("- document");
            println!("  - ref=e1");
            println!("  - action get url");
            for line in value
                .get("snapshot")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .lines()
                .map(str::trim)
                .filter(|line| !line.is_empty())
            {
                println!("  - text \"{}\"", line.replace('"', "\\\""));
            }
        }
        TextMode::BrowserScreenshot { out, json_output } => {
            let (path, url) = write_browser_screenshot(value, out.as_deref())?;
            if json_output {
                println!(
                    "{}",
                    serde_json::to_string(&json!({
                        "path": path,
                        "url": url,
                        "width": value.get("width").cloned().unwrap_or(json!(800)),
                        "height": value.get("height").cloned().unwrap_or(json!(600))
                    }))?
                );
            } else {
                println!("OK {path}");
            }
        }
        TextMode::BrowserPdf { out, json_output } => {
            let (path, url) = write_browser_pdf(value, out.as_deref())?;
            if json_output {
                println!(
                    "{}",
                    serde_json::to_string(&json!({
                        "path": path,
                        "url": url,
                        "mime_type": value.get("mime_type").cloned().unwrap_or(json!("application/pdf")),
                        "bytes": value.get("bytes").cloned().unwrap_or(json!(0)),
                        "page_count": value.get("page_count").cloned().unwrap_or(json!(1))
                    }))?
                );
            } else {
                println!("OK {path}");
            }
        }
        TextMode::AuthStatus => {
            print_auth_status(value);
        }
        TextMode::AuthLogin => {
            if value
                .get("already_signed_in")
                .and_then(Value::as_bool)
                .unwrap_or(false)
            {
                let email = auth_response_email(value);
                println!(
                    "Already signed in{}. Use `cmux auth logout` to sign out first.",
                    email
                        .as_ref()
                        .map(|email| format!(" as {email}"))
                        .unwrap_or_default()
                );
            } else if value
                .get("signed_in")
                .and_then(Value::as_bool)
                .unwrap_or(false)
            {
                let email = auth_response_email(value);
                println!(
                    "Signed in{}.",
                    email
                        .as_ref()
                        .map(|email| format!(" as {email}"))
                        .unwrap_or_default()
                );
            } else if value
                .get("sign_in_started")
                .and_then(Value::as_bool)
                .unwrap_or(false)
            {
                let url = value
                    .get("sign_in_url")
                    .and_then(Value::as_str)
                    .unwrap_or("the sign-in URL");
                println!("Opened {url} for sign-in. Run `cmux auth status` after completing it.");
            } else if value
                .get("timed_out")
                .and_then(Value::as_bool)
                .unwrap_or(false)
            {
                println!(
                    "Timed out waiting for sign-in. Run `cmux auth status` once you've finished in the popup."
                );
            } else {
                println!("Sign-in did not complete. Run `cmux auth status` to check.");
            }
        }
        TextMode::AuthLogout => {
            if value
                .get("already_signed_out")
                .and_then(Value::as_bool)
                .unwrap_or(false)
            {
                println!("Already signed out.");
            } else if value
                .get("signed_in")
                .and_then(Value::as_bool)
                .unwrap_or(false)
            {
                println!("Sign-out requested but state hasn't cleared yet. Run `cmux auth status` to confirm.");
            } else {
                println!("Signed out.");
            }
        }
        TextMode::FeedList => {
            let items = value
                .get("items")
                .and_then(Value::as_array)
                .cloned()
                .unwrap_or_default();
            if items.is_empty() {
                println!("No feed items.");
            } else {
                for item in items {
                    let status = item
                        .get("status")
                        .and_then(Value::as_str)
                        .unwrap_or("unknown");
                    let source = item.get("source").and_then(Value::as_str).unwrap_or("?");
                    let kind = item.get("kind").and_then(Value::as_str).unwrap_or("?");
                    let id = item.get("id").and_then(Value::as_str).unwrap_or("?");
                    let title = item
                        .get("title")
                        .and_then(Value::as_str)
                        .or_else(|| item.get("tool_name").and_then(Value::as_str))
                        .or_else(|| item.get("question_prompt").and_then(Value::as_str))
                        .or_else(|| item.get("plan_summary").and_then(Value::as_str))
                        .unwrap_or("");
                    if title.is_empty() {
                        println!("{status}\t{source}\t{kind}\t{id}");
                    } else {
                        println!("{status}\t{source}\t{kind}\t{id}\t{title}");
                    }
                }
            }
        }
        TextMode::FeedClear => {
            let removed = value.get("removed").and_then(Value::as_u64).unwrap_or(0);
            println!(
                "Cleared {removed} feed item{}.",
                if removed == 1 { "" } else { "s" }
            );
        }
        TextMode::SettingsOpen => {
            let target = value
                .get("target")
                .and_then(Value::as_str)
                .unwrap_or("general");
            let surface = value
                .get("surface_ref")
                .and_then(Value::as_str)
                .or_else(|| value.get("surface_id").and_then(Value::as_str));
            if let Some(surface) = surface {
                println!("OK target={target} surface={surface}");
            } else {
                println!("OK target={target}");
            }
        }
        TextMode::SurfaceResumeGet => {
            let command = value
                .get("resume_binding")
                .and_then(|binding| binding.get("command"))
                .and_then(Value::as_str);
            if let Some(command) = command {
                println!("{command}");
            } else {
                println!("No resume binding.");
            }
        }
        TextMode::BrowserAvailability { status_only } => {
            let enabled = value
                .get("enabled")
                .and_then(Value::as_bool)
                .unwrap_or(true);
            if status_only {
                println!("{}", if enabled { "enabled" } else { "disabled" });
            } else {
                println!(
                    "cmux browser {}",
                    if enabled { "enabled" } else { "disabled" }
                );
            }
        }
        TextMode::AgentHibernation => {
            println!("OK");
        }
        TextMode::RightSidebar => {
            if value.get("action").and_then(Value::as_str) == Some("mode") {
                println!(
                    "{}",
                    serde_json::to_string(&json!({
                        "visible": value.get("visible").cloned().unwrap_or(json!(true)),
                        "mode": value.get("mode").cloned().unwrap_or(json!("files"))
                    }))?
                );
            }
        }
        TextMode::CustomSidebar { action } => {
            let sidebars = value
                .get("sidebars")
                .and_then(Value::as_array)
                .cloned()
                .unwrap_or_default();
            if sidebars.is_empty() && matches!(action.as_str(), "validate" | "reload") {
                println!("No custom sidebars found.");
            }
            for sidebar in sidebars {
                let name = sidebar
                    .get("name")
                    .and_then(Value::as_str)
                    .unwrap_or("(unknown)");
                let kind = sidebar
                    .get("kind")
                    .and_then(Value::as_str)
                    .unwrap_or("unknown");
                let path = sidebar.get("path").and_then(Value::as_str).unwrap_or("");
                if sidebar.get("ok").and_then(Value::as_bool).unwrap_or(false) {
                    println!("OK {name} ({kind}) {path}");
                } else {
                    let error = sidebar
                        .get("error")
                        .and_then(Value::as_str)
                        .unwrap_or("validation failed");
                    println!("ERROR {name} ({kind}) {path}: {error}");
                }
            }
            if action == "reload" {
                let count = value
                    .get("reloaded_count")
                    .and_then(Value::as_u64)
                    .unwrap_or(0);
                println!(
                    "Reloaded {count} sidebar{}.",
                    if count == 1 { "" } else { "s" }
                );
            } else if action == "select" {
                let title = value
                    .get("selected_name")
                    .and_then(Value::as_str)
                    .unwrap_or("Workspaces");
                if value
                    .get("error_count")
                    .and_then(Value::as_u64)
                    .unwrap_or(0)
                    == 0
                {
                    println!("Selected {title}.");
                }
            }
        }
        TextMode::RemoteTmuxWindow => {
            let host = value
                .get("host")
                .and_then(Value::as_str)
                .unwrap_or("unknown");
            let window = value
                .get("window_ref")
                .and_then(Value::as_str)
                .or_else(|| value.get("window_id").and_then(Value::as_str))
                .unwrap_or("unknown");
            if value
                .get("mirrored")
                .and_then(Value::as_bool)
                .unwrap_or(false)
            {
                println!("OK host={host} window={window}");
            } else if value
                .get("auth_required")
                .and_then(Value::as_bool)
                .unwrap_or(false)
            {
                println!("Authentication required for {host}. Run this command from an interactive terminal.");
            } else {
                println!("{}", serde_json::to_string_pretty(value)?);
            }
        }
        TextMode::Jsonish => {
            if command == "read-screen" || command == "capture-pane" {
                print!(
                    "{}",
                    value
                        .get("text")
                        .and_then(Value::as_str)
                        .unwrap_or_default()
                );
            } else {
                println!("{}", serde_json::to_string_pretty(value)?);
            }
        }
    }
    Ok(())
}

fn write_browser_screenshot(value: &Value, out: Option<&str>) -> Result<(String, String)> {
    let path = out
        .map(PathBuf::from)
        .unwrap_or_else(default_screenshot_path);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).with_context(|| {
            format!("failed to create screenshot directory {}", parent.display())
        })?;
    }
    let bytes = value
        .get("png_base64")
        .and_then(Value::as_str)
        .filter(|data| !data.is_empty())
        .map(|data| {
            base64::engine::general_purpose::STANDARD
                .decode(data)
                .context("failed to decode screenshot PNG")
        })
        .transpose()?
        .unwrap_or_default();
    fs::write(&path, bytes)
        .with_context(|| format!("failed to write screenshot {}", path.display()))?;
    let path = path.to_string_lossy().to_string();
    Ok((path.clone(), file_url::file_url_for_path(&path)))
}

fn write_browser_pdf(value: &Value, out: Option<&str>) -> Result<(String, String)> {
    let path = out
        .map(PathBuf::from)
        .or_else(|| value.get("path").and_then(Value::as_str).map(PathBuf::from))
        .unwrap_or_else(default_pdf_path);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("failed to create PDF directory {}", parent.display()))?;
    }
    let bytes = value
        .get("pdf_base64")
        .and_then(Value::as_str)
        .filter(|data| !data.is_empty())
        .map(|data| {
            base64::engine::general_purpose::STANDARD
                .decode(data)
                .context("failed to decode browser PDF")
        })
        .transpose()?
        .unwrap_or_default();
    fs::write(&path, bytes).with_context(|| format!("failed to write PDF {}", path.display()))?;
    let path = path.to_string_lossy().to_string();
    Ok((path.clone(), file_url::file_url_for_path(&path)))
}

fn default_screenshot_path() -> PathBuf {
    browser_artifact_path(
        &cache_dir(),
        &["browser", "screenshots"],
        "cmux-browser-screenshot",
        "png",
    )
}

fn default_pdf_path() -> PathBuf {
    browser_artifact_path(&cache_dir(), &["browser", "pdf"], "cmux-browser-pdf", "pdf")
}

fn browser_artifact_path(
    cache_dir: &Path,
    subdirs: &[&str],
    prefix: &str,
    extension: &str,
) -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or(0);
    browser_artifact_path_for(
        cache_dir,
        subdirs,
        prefix,
        extension,
        std::process::id(),
        nanos,
    )
}

fn browser_artifact_path_for(
    cache_dir: &Path,
    subdirs: &[&str],
    prefix: &str,
    extension: &str,
    process_id: u32,
    nanos: u128,
) -> PathBuf {
    let mut dir = cache_dir.to_path_buf();
    for subdir in subdirs {
        dir.push(subdir);
    }
    dir.join(format!("{prefix}-{process_id}-{nanos}.{extension}"))
}

fn print_auth_status(value: &Value) {
    let signed_in = value
        .get("signed_in")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    if !signed_in {
        println!("Not signed in.");
        println!("Run: cmux auth login");
        return;
    }

    println!("Signed in.");
    if let Some(email) = auth_response_email(value) {
        println!("  email:    {email}");
    }
    if let Some(display_name) = value
        .get("user")
        .and_then(|user| user.get("display_name"))
        .and_then(Value::as_str)
    {
        println!("  name:     {display_name}");
    }
    if let Some(user_id) = value
        .get("user")
        .and_then(|user| user.get("id"))
        .and_then(Value::as_str)
    {
        println!("  user_id:  {user_id}");
    }
    if let Some(team_id) = value.get("selected_team_id").and_then(Value::as_str) {
        println!("  team_id:  {team_id}");
    }
}

fn auth_response_email(value: &Value) -> Option<String> {
    value
        .get("user")
        .and_then(|user| user.get("email"))
        .and_then(Value::as_str)
        .map(ToString::to_string)
}

fn print_welcome() {
    println!();
    println!("  cmux");
    println!("  the open source terminal");
    println!("  built for coding agents");
    println!();
    println!("  Shortcuts");
    println!();
    println!("  Ctrl+N                 New workspace");
    println!("  Ctrl+T                 New tab");
    println!("  Ctrl+P                 Go to workspace");
    println!("  Ctrl+B                 Toggle Left Sidebar");
    println!("  Ctrl+Alt+B             Toggle Right Sidebar");
    println!("  Ctrl+D                 Split right");
    println!("  Ctrl+Shift+D           Split down");
    println!("  Ctrl+Shift+P           Command palette");
    println!("  Ctrl+Shift+R           Rename workspace");
    println!("  Ctrl+Shift+L           New browser");
    println!("  Ctrl+Shift+U           Jump to latest unread");
    println!("  Ctrl+Alt+U             Toggle unread");
    println!();
    println!("  Docs                   https://cmux.com/docs");
    println!("  Discord                https://discord.gg/xsgFEVrWCZ");
}

fn print_help(command: &[String]) {
    if command.first().map(String::as_str) == Some("help") {
        if command
            .iter()
            .skip(1)
            .any(|arg| matches!(arg.as_str(), "--help" | "-h"))
        {
            println!("Usage: cmux help [command]");
            return;
        }
        if let Some(topic) = command.get(1).map(String::as_str) {
            print_command_help(canonical_help_command(topic));
            return;
        }
    }
    println!(
        "{}",
        concat!(
            "cmux - control cmux Linux via Unix socket\n\n",
            "Usage:\n  cmux [global-options] <command> [options]\n\n",
            "Global options:\n",
            "  --socket <path>\n",
            "  --json\n",
            "  --id-format <refs|uuids|both>\n",
            "  --password <value>\n\n",
            "Commands:\n",
            "  open <path-or-url>..., markdown, diff, events, app, serve, ping, version, capabilities,\n",
            "  identify, top, memory, vm, cloud, window, workspace, list-workspaces, new-workspace, list-panes, list-panels, new-split,\n",
            "  equalize-splits, resize-pane,\n",
            "  restore-session, workspace-group, surface resume, renderer, tab-action, rename-tab,\n",
            "  move-tab-to-new-workspace, workspace-action,\n",
            "  tree, debug-terminals, set-app-focus, simulate-app-active,\n",
            "  send, send-key, read-screen, browser,\n",
            "  disable-browser, enable-browser, browser-status, agent-hibernation, remotes, remote, ssh, ssh-tmux,\n",
            "  ssh-session-list, ssh-session-attach, ssh-session-cleanup,\n",
            "  ssh-session-snapshot, ssh-session-restore, remote-daemon-status,\n",
            "  auth, login, logout, feed, feedback, mobile, themes, notify, sidebar, right-sidebar, docs, settings, config, update,\n",
            "  shortcuts,\n",
            "  hooks, codex, claude-hook, omo, omx, omc,\n",
            "  install-claude-code-integration,\n",
            "  install-codex-integration, install-opencode-integration, welcome, help"
        )
    );
}

fn canonical_help_command(command: &str) -> &str {
    match command {
        "keyboard" | "keyboard-shortcuts" | "keyboardshortcuts" | "keys" | "keybindings" => {
            "shortcuts"
        }
        "browser-import" | "browserimport" => "browser-import",
        "dock" => "right-sidebar",
        "sidebars" => "sidebar",
        "agents" => "hooks",
        other => other,
    }
}

fn help_topic_for_command(command: &[String]) -> &str {
    match (
        command.first().map(String::as_str),
        command.get(1).map(String::as_str),
    ) {
        (Some("browser"), Some("import")) => "browser-import",
        _ => command.first().map(String::as_str).unwrap_or("help"),
    }
}

fn print_command_help(command: &str) {
    match command {
        "open" => {
            println!(
                "cmux open\n\nUsage: cmux open <path-or-url>... [--workspace <id|ref>] [--surface <id|ref>] [--pane <id|ref>] [--window <id|ref>] [--focus true|false] [--no-focus]\n\nOpen files as browser file:// surfaces, directories as workspaces rooted at that path, web URLs as browser surfaces, and cmux://settings[/target] app links as settings surfaces."
            );
        }
        "markdown" => {
            println!(
                "Usage: cmux markdown open <path> [options]\n       cmux markdown <path>       (shorthand for 'open')\n\nOpen a markdown file in the native Linux document viewer.\n\nOptions:\n  --workspace <id|ref|index>   Target workspace (default: $CMUX_WORKSPACE_ID)\n  --surface <id|ref|index>     Source surface to split from (default: focused surface)\n  --window <id|ref|index>      Target window\n  --direction <left|right|up|down>  Split direction (default: right)\n  --focus <true|false>         Focus the markdown panel (default: false)\n  --font-size <points>         Viewer font size, 8-96 points\n\nExamples:\n  cmux markdown open plan.md\n  cmux markdown ~/project/CHANGELOG.md\n  cmux markdown open ./docs/design.md --workspace 0\n  cmux markdown open plan.md --direction down"
            );
        }
        "diff" => {
            println!(
                "Usage: cmux diff [patch-file|-] [options]\n\nRender a diff or patch in the native Linux diff viewer. With no patch file or source, cmux diff reads piped stdin.\n\nOptions:\n  --source <unstaged|staged|branch|last-turn>  Git diff source\n  --unstaged | --staged | --branch | --last-turn  Git source shortcuts\n  --cwd, --repo <path>               Git repository or worktree path\n  --base <ref>                       Base ref for --branch (default: origin/HEAD)\n  --workspace <id|ref|index>         Target workspace\n  --surface <id|ref|index>           Source surface to split from\n  --window <id|ref|index>            Target window\n  --focus <true|false> | --no-focus  Focus behavior\n  --title <text>                     Viewer title\n  --layout <split|unified>           Override the configured diff layout\n  --font-size <points>               Diff font size, 8-48 points\n  --comments <json-file>             Render read-only review comments from JSON\n  --comments-json <json>             Render read-only review comments from inline JSON"
            );
        }
        "events" => {
            println!(
                "Usage: cmux events [--after <seq>] [--cursor-file <path>] [--name <event>] [--category <category>] [--limit <n>] [--no-ack] [--no-heartbeat] [--reconnect]\n\nStream cmux domain events as newline-delimited JSON frames."
            );
        }
        "vm" | "cloud" => {
            println!(
                "Usage: cmux {command} <new|ls|rm|exec|shell|attach|ssh|ssh-info> [args...]\n\nManage Linux cloud VM state through the cmux socket. `cloud` is an alias for `vm`.\n\nSubcommands:\n  new|create [--image <image>] [--provider <provider>] [--detach]\n  ls|list\n  rm|destroy|delete <id>\n  exec <id> -- <command>\n  shell|attach <id>\n  ssh <id>\n  ssh-info <id>"
            );
        }
        "browser" => {
            println!(
                "cmux browser\n\nUsage: cmux browser [<surface-id|surface:ref>] <command> [options]\n\nCore commands:\n  open, connect, identify, snapshot, wait, click, type, fill, get, is, screenshot, pdf\n\nState and profile commands:\n  profiles list|create|select|rename|clear|delete\n  import, cookies, storage, tab, console, errors, state\n  history list|search|clear\n\nEmulation and panel commands:\n  viewport, device, geolocation, useragent, locale, timezone, media, headers, credentials, permissions, offline\n  devtools, react-grab, focus-mode, zoom\n\nNetwork and recording commands:\n  network, trace, har, screencast, video, record, input\n\nRun 'cmux docs browser' for the browser automation workflow and raw contract resources."
            );
        }
        "browser-import" => {
            println!(
                "Usage: cmux browser import [options]\n\nImport browser data into the Linux browser model. Non-interactive import reads cookies, history, bookmarks, and portable settings from detected Firefox and Chromium-family profiles. Use --interactive to open the browser-backed wizard instead, creating a browser surface when needed.\n\nOptions:\n  --from, --browser, --source <name>       Source browser filter, such as Firefox, Chrome, Chromium, Brave, or Edge\n  --profile, --source-profile <name>      Source profile name or qualified browser/profile name\n  --all-profiles                          Import from every matching source profile\n  --to-profile, --destination-profile <name>  Destination cmux browser profile\n  --create-profile                        Create the destination profile if it does not exist\n  --scope <cookies|history|bookmarks|settings|additional|all>  Data to import\n  --workspace <id|ref|index>              Target workspace for the interactive import wizard\n  --domain <domain>                       Import only matching cookie/history/bookmark domains\n  --cookies-file, --cookie-file <path>    Import cookies from JSON or Netscape cookie file\n  --bookmarks-file, --bookmark-file <path>  Import normalized JSON or a Chromium Bookmarks file\n  --settings-file, --browser-settings-file <path>  Import portable JSON or Firefox prefs.js settings\n  --interactive                           Open the browser import wizard instead of importing immediately"
            );
        }
        "__tmux-compat" => {
            println!(
                "Usage: cmux __tmux-compat <command> [options]\n\nInternal tmux compatibility dispatcher used by tmux-control shims and shell integrations.\n\nSupported commands:\n  list-panes, display-message, split-window, new-window, new-session\n  select-pane, capture-pane, resize-pane, pipe-pane, clear-history\n  paste-buffer, set-buffer, list-buffers, respawn-pane\n  last-pane, swap-pane, break-pane, join-pane\n  next-window, previous-window, last-window\n  show-options, wait-for, set-hook, bind-key, unbind-key, copy-mode, popup"
            );
        }
        "docs" => {
            println!(
                "Usage: cmux docs [settings|shortcuts|api|browser|agents|dock|sidebars]\n\nPrint canonical Linux docs, local raw resources, and useful commands for a topic without connecting to a cmux socket."
            );
        }
        "feedback" => {
            println!(
                "Usage: cmux feedback\n       cmux feedback --email <email> --body <text> [--image <path> ...]\n       cmux feedback retry [--limit <count>]\n\nWithout args, open the feedback composer. With email and body, queue and submit feedback to cmux.com. Network and service failures remain in the private local queue; `feedback retry` retries pending reports."
            );
        }
        "settings" => {
            println!(
                "Usage: cmux settings [open [target]|path|docs|<target>]\n\nOpen cmux Settings, print cmux.json paths, or show settings documentation.\n\nSubcommands:\n  open [target]       Open Settings, optionally to a target section.\n  path                Print cmux.json paths, docs URL, and schema URL.\n  docs                Print the same output as `cmux docs settings`.\n\nTargets: account, app, terminal, sidebar-appearance, custom-sidebars, automation, browser, browser-import, global-hotkey, keyboard-shortcuts, shortcuts, workspace-colors, cmux-json, json, reset\n\nOptions for open/targets:\n  --target <target>             Settings pane target\n  --workspace <id|ref|index>    Target workspace (default: $CMUX_WORKSPACE_ID)\n  --surface <id|ref|index>      Source surface to split from\n  --pane <id|ref|index>         Target pane\n  --window <id|ref|index>       Target window\n  --focus <true|false>          Focus the settings panel\n  --no-focus                    Do not focus the settings panel"
            );
        }
        "config" => {
            println!(
                "Usage: cmux config <doctor|check|validate|path|paths|docs|documentation|reload|get|set|sidebar-font-size|surface-tab-bar-font-size>\n\nValidate cmux.json syntax, print config references, update selected Ghostty config keys, or reload the running app.\n\nSubcommands:\n  doctor|check|validate [--path <path>]   Validate JSONC syntax for cmux config files.\n  path|paths                              Print cmux.json paths, docs URL, and schema URL.\n  docs|documentation                      Print the same output as `cmux docs settings`.\n  reload                                  Reload Ghostty config and cmux settings through the running app.\n  get <key>                               Print sidebar-font-size or surface-tab-bar-font-size.\n  set <key> <points>                      Set sidebar-font-size (10-20 pt) or surface-tab-bar-font-size (8-14 pt), then reload if cmux is running.\n  sidebar-font-size [points]              Get or set the left sidebar text size.\n  surface-tab-bar-font-size [points]      Get or set the workspace tab bar text size."
            );
        }
        "welcome" => {
            println!(
                "Usage: cmux welcome\n\nShow a welcome screen with the cmux logo and useful shortcuts."
            );
        }
        "shortcuts" => {
            println!("Usage: cmux shortcuts\n\nOpen Settings to Keyboard Shortcuts.");
        }
        "reload-config" => {
            println!(
                "Usage: cmux reload-config\n\nAsk the running cmux app to reload configuration."
            );
        }
        "mobile" => {
            println!(
                "Usage: cmux mobile status [--workspace <id|ref|index>] [--window <id|ref|index>]\n       cmux mobile workspace list [--workspace <id|ref|index>]\n       cmux mobile terminal <create|input|paste|paste-image|replay|viewport|scroll|mouse> [options]\n       cmux mobile chat <sessions|history|send|interrupt|answer|dump> [options]\n       cmux mobile host status [options]\n       cmux mobile attach-ticket create [--scope <linux|mac|workspace>] [--route-id <id>] [--route-kind <kind>] [--ttl-seconds <seconds>] [--workspace <id|ref|index>] [--terminal <id|ref|index>]\n\nShow the Linux mobile host status, inspect mobile workspaces, drive terminal replay/input flows, drive mobile agent chat sessions, paste image files, or mint an iOS-compatible attach ticket for configured Linux host routes."
            );
        }
        "remotes" | "remote" => {
            println!(
                "Usage: cmux remotes <list|add|remove> [options]\n\nManage the Linux local registry of iOS-visible remote hosts.\n\nCommands:\n  remotes list|ls [--json]\n  remotes add <name> --route <host:port> [--route <host:port> ...] [--tag <tag>] [--json]\n  remotes remove|rm|delete <name-or-deviceId> [--json]\n\nRoutes must use a Tailscale CGNAT host in 100.64.0.0/10 or a *.ts.net name."
            );
        }
        "ssh-tmux" => {
            println!(
                "Usage: cmux ssh-tmux <destination> [--port <n>] [--identity <path>] [--no-focus] [--live]\n\nOpen a dedicated Linux cmux window that mirrors the remote host's tmux sessions through the remote tmux socket model. With --live, probe tmux sessions over SSH first. If the app reports auth_required with an ssh_argv, the CLI runs the vetted /usr/bin/ssh command in the caller's terminal and retries once."
            );
        }
        "feed" => {
            println!(
                "Usage: cmux feed list [--pending-only]\n       cmux feed clear [--yes|-y]\n       cmux feed tui [--once] [--pending-only|--all] [--legacy|--opentui]\n\nList, clear, or open the built-in Linux Feed terminal UI. In non-terminal output or with --once, feed tui renders one snapshot and exits."
            );
        }
        "top" => {
            println!(
                "Usage: cmux top [--window <id|ref|index> | --all-windows] [--workspace <id|ref|index>] [--include-processes]\n\nShow Linux process resource summaries for cmux windows, workspaces, panes, and surfaces."
            );
        }
        "memory" => {
            println!(
                "Usage: cmux memory [--window <id|ref|index> | --all-windows] [--workspace <id|ref|index>] [--top-group-limit <1-100>]\n\nShow Linux memory diagnostics for cmux and child terminal processes."
            );
        }
        "tree" => {
            println!(
                "Usage: cmux tree [--window <id|ref|index> | --all-windows] [--workspace <id|ref|index>] [--tab <id|ref|index>]\n\nPrint the Linux window, workspace, pane, and surface tree."
            );
        }
        "debug-terminals" => {
            println!("Usage: cmux debug-terminals\n\nPrint Linux terminal surface debug state.");
        }
        "set-app-focus" => {
            println!(
                "Usage: cmux set-app-focus <active|inactive|clear>\n\nOverride app focus state for Linux tests."
            );
        }
        "simulate-app-active" => {
            println!(
                "Usage: cmux simulate-app-active\n\nTrigger Linux app-active handling for tests."
            );
        }
        "equalize-splits" => {
            println!(
                "Usage: cmux equalize-splits [--workspace <id|ref|index>] [--surface <id|ref|index>] [--pane <id|ref|index>] [--orientation <horizontal|vertical|all>]\n\nEqualize split sizes in a workspace."
            );
        }
        "copy-mode" => {
            println!(
                "Usage: cmux copy-mode [--cancel|-q]\n\nToggle the Linux tmux-compatible copy-mode marker."
            );
        }
        "set-buffer" => {
            println!(
                "Usage: cmux set-buffer [--name <name>] <text>\n\nStore text in a Linux tmux-compatible paste buffer."
            );
        }
        "paste-buffer" => {
            println!(
                "Usage: cmux paste-buffer [--name <name>] [--workspace <id|ref|index>] [--surface <id|ref|index>]\n\nPaste a Linux tmux-compatible buffer into a terminal surface."
            );
        }
        "list-buffers" => {
            println!("Usage: cmux list-buffers\n\nList Linux tmux-compatible paste buffers.");
        }
        "respawn-pane" => {
            println!(
                "Usage: cmux respawn-pane [--workspace <id|ref|index>] [--surface <id|ref|index>] [--command <shell-command>]\n\nSend a restart command to a terminal surface."
            );
        }
        "display-message" => {
            println!(
                "Usage: cmux display-message [-p] [message]\n\nPrint or display a Linux tmux-compatible message."
            );
        }
        "window" => {
            println!(
                "Usage: cmux window displays\n       cmux window display <name|index> [--window <id|ref|index>]\n       cmux window display --list\n       cmux window default-display [<name>|--clear]\n\nList Linux displays, assign cmux window state to a display, or configure the default display for new windows."
            );
        }
        "restore-session" => {
            println!(
                "Usage: cmux restore-session [--json]\n\nRestore the newest in-memory session resume binding into a new workspace."
            );
        }
        "move-workspace-to-window" => {
            println!(
                "Usage: cmux move-workspace-to-window --workspace <id|ref|index> --window <id|ref|index>\n\nMove a workspace to a different window."
            );
        }
        "reorder-workspace" => {
            println!(
                "Usage: cmux reorder-workspace [--workspace <id|ref|index> | <id|ref|index>] [flags]\n\nReorder a workspace within its window.\n\nFlags:\n  --workspace <id|ref|index>\n  --index <n>\n  --before <id|ref|index>\n  --before-workspace <id|ref|index>\n  --after <id|ref|index>\n  --after-workspace <id|ref|index>\n  --window <id|ref|index>\n  --dry-run"
            );
        }
        "reorder-workspaces" => {
            println!(
                "Usage: cmux reorder-workspaces --order <id|ref|index>,<id|ref|index>,... [flags]\n\nReorder workspaces within a window as one batch.\n\nFlags:\n  --order <refs>\n  --window <id|ref|index>\n  --dry-run"
            );
        }
        "move-surface" => {
            println!(
                "Usage: cmux move-surface [--surface <id|ref|index> | <id|ref|index>] [flags]\n\nMove a surface to a different pane, workspace, or window.\n\nFlags:\n  --surface <id|ref|index>\n  --pane <id|ref|index>\n  --workspace <id|ref|index>\n  --window <id|ref|index>\n  --before <id|ref|index>\n  --after <id|ref|index>\n  --index <n>\n  --focus <true|false>"
            );
        }
        "split-off" => {
            println!(
                "Usage: cmux split-off --surface <id|ref|index> <left|right|up|down> [flags]\n\nMove an existing surface into a new split without changing focus by default."
            );
        }
        "reorder-surface" => {
            println!(
                "Usage: cmux reorder-surface [--surface <id|ref|index> | <id|ref|index>] [flags]\n\nReorder a surface within its pane. Provide --index, --before, or --after."
            );
        }
        "surface" => {
            println!(
                "Usage: cmux surface resume get [--surface <id|ref>] [--workspace <id|ref>] [--pane <id|ref>]\n       cmux surface resume set [target] --shell <command> [--name <name>] [--kind <kind>] [--cwd <path>] [--checkpoint <id>] [--source <source>] [--env KEY=VALUE ...]\n       cmux surface resume approve --surface <id|ref> --policy manual|prompt|auto\n       cmux surface resume run --surface <id|ref> [--skip]\n       cmux surface resume clear [target] [--checkpoint <id>] [--source <source>]\n\nManage the resume binding and signed restore policy attached to a surface. CLI-created bindings start as manual approvals; --auto-resume is honored only for agent-hook sources."
            );
        }
        "workspace" => {
            println!(
                "Usage: cmux workspace <list|create|env|close|rename|select|reconnect|disconnect|group> [flags]\n\nNamespace for workspace operations. Top-level compatibility commands such as list-workspaces, new-workspace, select-workspace, close-workspace, and rename-workspace use the same socket methods.\n\nExamples:\n  cmux workspace list\n  cmux workspace create --name build --env KEY=VALUE --env-file .env\n  cmux workspace env workspace:1 --mask\n  cmux workspace group list"
            );
        }
        "workspace-group" => {
            println!(
                "Usage: cmux workspace-group list [--json]\n       cmux workspace-group create --name <name> [--cwd <path>] [--from <workspace>,<workspace>]\n       cmux workspace-group ungroup <group-id>\n       cmux workspace-group delete <group-id>\n       cmux workspace-group rename <group-id> --name <name>\n       cmux workspace-group collapse|expand|pin|unpin|focus <group-id>\n       cmux workspace-group add --group <group-id> --workspace <workspace-id>\n       cmux workspace-group remove --workspace <workspace-id>\n       cmux workspace-group set-anchor --group <group-id> --workspace <workspace-id>\n       cmux workspace-group new-workspace <group-id> [--placement afterCurrent|top|end]\n\nManage workspace groups."
            );
        }
        "themes" => {
            println!(
                "Usage: cmux themes\n       cmux themes list\n       cmux themes set <theme>\n       cmux themes set --light <theme> [--dark <theme>]\n       cmux themes set --dark <theme> [--light <theme>]\n       cmux themes clear\n\nOpen an interactive theme picker in a TTY, or list themes when output is captured. Set or clear the cmux Ghostty theme override."
            );
        }
        "auth" | "login" | "logout" => {
            println!(
                "Usage: cmux auth [status]\n       cmux auth login [--timeout <seconds>]\n       cmux auth logout\n       cmux auth team <team-id|none>\n       cmux login [--timeout <seconds>]\n       cmux logout\n\nSign in through the hosted browser flow, inspect the current account session, or select the active team."
            );
        }
        "list-notifications" => {
            println!("Usage: cmux list-notifications\n\nList queued notifications.");
        }
        "dismiss-notification" => {
            println!(
                "Usage: cmux dismiss-notification (--id <uuid> | --all-read)\n\nRemove one notification, or remove every already-read notification."
            );
        }
        "mark-notification-read" => {
            println!(
                "Usage: cmux mark-notification-read (--id <uuid> | --workspace <id|ref> [--surface <id|ref>] [--window <id|ref>] | --all)\n\nMark notifications read without opening them."
            );
        }
        "open-notification" => {
            println!(
                "Usage: cmux open-notification --id <uuid>\n\nFocus the notification target and mark it read."
            );
        }
        "jump-to-unread" => {
            println!("Usage: cmux jump-to-unread\n\nFocus the latest unread notification.");
        }
        "clear-notifications" => {
            println!("Usage: cmux clear-notifications\n\nClear queued notifications.");
        }
        "right-sidebar" => {
            println!(
                "Usage: cmux right-sidebar <command> [flags]\n\nCommands:\n  toggle\n  show\n  hide\n  focus\n  set <files|find|vault|sessions|feed|dock>\n  mode\n  files|find|vault|sessions|feed|dock\n\nFlags:\n  --workspace <id|ref>\n  --window <id|ref>\n  --no-focus"
            );
        }
        "sidebar" => {
            println!(
                "Usage: cmux sidebar <validate|reload|select|clear-state> [name|--all] [--json]\n\nValidate, reload, select, or reset custom left sidebars from ~/.config/cmux/sidebars. Swift files win over JSON files with the same base name. Linux interprets the supported SwiftUI-style subset with live workspace data, persisted @State bindings, and cmux actions; customSidebars.renderer selects in-process or isolated-worker evaluation.\n\nCommands:\n  validate [name]\n  reload [name]\n  select <name|workspaces>\n  clear-state [name]"
            );
        }
        "disable-browser" => {
            println!(
                "Usage: cmux disable-browser [--json]\n\nDisable cmux browser creation and link interception. This overrides browser settings until re-enabled."
            );
        }
        "enable-browser" => {
            println!(
                "Usage: cmux enable-browser [--json]\n\nRe-enable cmux browser creation and link interception."
            );
        }
        "browser-status" => {
            println!(
                "Usage: cmux browser-status [--json]\n\nPrint whether cmux browser creation and link interception are enabled."
            );
        }
        "agent-hibernation" => {
            println!(
                "Usage: cmux agent-hibernation <on|off> [--json]\n\nEnable or disable Agent Hibernation."
            );
        }
        "remote-daemon-status" => {
            println!(
                "Usage: cmux remote-daemon-status [--os <darwin|linux>] [--arch <arm64|amd64>]\n\nShow the cmuxd-remote version, local binary, cache status, checksum state, and release verification commands."
            );
        }
        "update" => {
            println!(
                "Usage: cmux update [check|status] [--json]\n       cmux update install [--yes|-y] [--prefix <absolute-path>] [--force] [--json]\n\nCheck the latest stable cmux Linux release, select the archive for this architecture, and validate its published SHA-256 metadata. `install` downloads the bounded archive, verifies that checksum, rejects unsafe archive entries, and runs the bundled installer. Non-interactive installation requires --yes."
            );
        }
        "tab-action" => {
            println!(
                "Usage: cmux tab-action --action <name> [flags]\n\nTarget tab:\n  --tab <id|ref|index> accepts tab:<n>, surface:<n>, UUID, or index.\n  --surface <id|ref|index> is an alias for --tab.\n\nActions:\n  rename, clear-name, close-left, close-right, close-others,\n  new-terminal-right, new-browser-right, reload, duplicate,\n  move-to-new-workspace, pin, unpin, mark-read, mark-unread\n\nExamples:\n  cmux tab-action --tab tab:3 --action pin\n  cmux tab-action --workspace workspace:2 --tab tab:1 --action rename --title logs\n  cmux tab-action --tab tab:2 --action duplicate"
            );
        }
        "move-tab-to-new-workspace" | "detach-tab" => {
            println!(
                "Usage: cmux move-tab-to-new-workspace [--tab <id|ref|index>] [--surface <id|ref|index>] [--workspace <id|ref|index>] [--window <id|ref|index>] [--title <text>] [--focus <true|false>]\n\nMove a tab into a newly created workspace in the same window."
            );
        }
        "workspace-action" => {
            println!(
                "Usage: cmux workspace-action --action <name> [flags]\n\nTarget workspace:\n  --workspace <id|ref|index>\n  --window <id|ref|index> scopes workspace:<n> refs.\n\nActions:\n  pin, unpin, rename, clear-name, set-description, clear-description,\n  move-up, move-down, move-top, close-others, close-above, close-below,\n  mark-read, mark-unread,\n  set-color, clear-color\n\nExamples:\n  cmux workspace-action --workspace workspace:2 --action pin\n  cmux workspace-action --action rename --title infra\n  cmux workspace-action set-color blue"
            );
        }
        "ssh" => {
            println!(
                "cmux ssh\n\nUsage: cmux ssh <destination> [--name <title>] [--port <port>] [--identity <path>] [--ssh-option <option>] [-A|--forward-agent] [-a|--no-forward-agent]\n\nCreate a new workspace backed by an SSH remote session. Agent forwarding inherits ssh_config by default; use -A to request it or -a to disable it for this workspace."
            );
        }
        "ssh-session-list" => {
            println!("Usage: cmux ssh-session-list [--workspace <id|ref|index>|--all]");
        }
        "ssh-session-attach" => {
            println!(
                "Usage: cmux ssh-session-attach [--workspace <id|ref|index>] (--session-id <id>|<id>)"
            );
        }
        "ssh-session-cleanup" => {
            println!(
                "Usage: cmux ssh-session-cleanup [--workspace <id|ref|index>] (--session-id <id>|<id>)"
            );
        }
        "ssh-session-snapshot" => {
            println!("Usage: cmux ssh-session-snapshot [--workspace <id|ref|index>]");
        }
        "ssh-session-restore" | "ssh-session-restore-snapshot" => {
            println!("Usage: cmux ssh-session-restore (--file <path>|--snapshot <json>|- < stdin)");
        }
        "open-browser" | "open_browser" => {
            println!("Legacy alias for 'cmux browser open'");
        }
        "navigate" => {
            println!("Legacy alias for 'cmux browser navigate'");
        }
        "browser-back" | "browser_back" => {
            println!("Legacy alias for 'cmux browser back'");
        }
        "browser-forward" | "browser_forward" => {
            println!("Legacy alias for 'cmux browser forward'");
        }
        "browser-reload" | "browser_reload" => {
            println!("Legacy alias for 'cmux browser reload'");
        }
        "get-url" | "get_url" => {
            println!("Legacy alias for 'cmux browser get-url'");
        }
        "focus-webview" | "focus_webview" => {
            println!("Legacy alias for 'cmux browser focus-webview'");
        }
        "is-webview-focused" | "is_webview_focused" => {
            println!("Legacy alias for 'cmux browser is-webview-focused'");
        }
        "hooks" => {
            println!(
                "Usage: cmux hooks setup [agent] [--agent <name>] [--yes|-y]\n       cmux hooks uninstall [agent] [--agent <name>] [--yes|-y]\n       cmux hooks <agent> <event>\n       cmux hooks feed --source <agent> [--event <event>]\n\nInstall agent hooks or convert stdin hook JSON into the Linux Feed event stream."
            );
        }
        "codex" => {
            println!(
                "Usage: cmux codex <install-hooks|uninstall-hooks>\n\nCompatibility alias for Codex hook integration commands."
            );
        }
        "claude-hook" => {
            println!(
                "Usage: cmux claude-hook <event>\n\nCompatibility alias for `cmux hooks claude <event>`; reads hook JSON from stdin."
            );
        }
        "omo" => {
            println!("Usage: cmux omo [opencode-args...]");
        }
        "omx" => {
            println!("Usage: cmux omx [omx-args...]");
        }
        "omc" => {
            println!("Usage: cmux omc [omc-args...]");
        }
        "install-claude-code-integration" | "install-claude-integration" => {
            println!(
                "cmux install-claude-code-integration\n\nUsage: cmux install-claude-code-integration [--path <settings.json>] [--yes|-y] [--dry-run]\n\nShow the proposed Claude Code hooks diff, prompt for confirmation, then update the Claude settings file."
            );
        }
        "install-codex-integration" => {
            println!(
                "cmux install-codex-integration\n\nUsage: cmux install-codex-integration [--codex-home <dir>] [--hooks-path <hooks.json>] [--config-path <config.toml>] [--yes|-y] [--dry-run]\n\nShow the proposed Codex hooks and notification config diffs, prompt for confirmation, then update the Codex config files."
            );
        }
        "install-opencode-integration" | "install-opencode-plugin" | "opencode" => {
            println!(
                "cmux install-opencode-integration\n\nUsage: cmux install-opencode-integration [--config-dir <dir>] [--yes|-y] [--dry-run]\n\nShow the proposed OpenCode opencode.json and cmux plugin diffs, prompt for confirmation, then update the OpenCode config directory."
            );
        }
        "app" => {
            println!(
                "cmux app\n\nUsage: cmux app [--renderer core|gtk|ghostty|ghostty-vt] [--socket <path>] [--script <commands>] [path-or-url ...]\n\nRun the local Linux app shell backed by the Rust cmux core. Positional paths or URLs are opened at startup for desktop/file-manager launches. When --socket is set, the shell also exposes the normal cmux control socket over the same app state. The GTK renderer requires building with --features gtk; ghostty uses the initial full Ghostty GLArea host with keyboard, keyboard-map changes, app/surface keybinds, mouse, scroll modifiers, GTK IME preedit/commit, file/text drops, app and surface focus, visibility, color-scheme, clipboard, close-request/confirmation, stable refresh, cwd, startup-command, wait-after-command, startup-input, environment, inherited-font-size forwarding, renderer-owned initial terminal startup plus queued socket/app text-key input, embedded viewport readback, event-driven selection readback, and TTY/process/grid/cell-size metadata sync, Ghostty runtime title/cwd/size-limit/initial-size/cell-size/renderer-health/prompt-title/quit-timer/float-window/secure-input/color-change/config-change/notification/bell/open-url/progress/command-finished/child-exited/search/scrollbar/command-palette/config-open/config-reload/readonly/copy-title/cursor actions, Ghostty new-window/close-window/goto-window/new-tab/move-tab/goto-tab/new-split/present-terminal/close-tab/goto-split/resize-split/equalize-splits/toggle-split-zoom layout actions with structured terminal metadata, host GTK handling for the GTK inspector, render requests, quit/close-all-windows with structured app-action metadata, Ghostty fullscreen compatibility requests as native fullscreen on Linux, maximize, visibility/quick-terminal toggles, reset-window-size, window decorations, tab overview as the Linux command switcher, and on-screen keyboard focus requests, an OpenGL Ghostty inspector overlay with focus/mouse/scroll/text/common-key routing, structured key-sequence/key-table terminal state, and UI-only Ghostty action metadata for background opacity, undo/redo, and update checks inside terminal surface cards, and ghostty-vt selects the portable Ghostty VT renderer snapshot backend.\n\nCommands inside the shell: help, status, windows, displays, window-display, current-window, new-window, focus-window, close-window, workspaces, current-workspace, next-workspace, previous-workspace, last-workspace, panes, focus-pane, last-pane, surfaces, current-surface, split, new-workspace, select, close-workspace, rename-workspace, workspace-action, open, focus-surface, close-surface, rename-tab, tab-action, send, enter, read, settings, config, themes, feed, notify, notifications, right-sidebar, palette, shortcuts, sleep, layout, renderer, quit"
            );
        }
        "renderer" => {
            println!(
                "cmux renderer\n\nUsage:\n  cmux renderer [snapshot|diagnostics] [--backend core|gtk|ghostty|ghostty-vt]\n  cmux renderer apply-size --pane <pane> --cols <n> --rows <n> --pixel-width <px> --pixel-height <px>\n\nInspect the renderer-facing app snapshot, platform/backend diagnostics, or apply a renderer allocation to a pane PTY."
            );
        }
        _ => println!("Usage: cmux {command} [options]"),
    }
}
