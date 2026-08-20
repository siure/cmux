#![recursion_limit = "256"]

mod agent_hibernation_settings;
mod agent_session;
mod app;
mod browser_environment;
mod browser_omnibar;
mod browser_runtime;
mod browser_settings;
mod cli;
mod config;
mod custom_sidebar;
mod diff_baseline;
mod diff_viewer;
mod file_url;
mod ghostty_embed;
mod ghostty_vt;
mod global_shortcuts;
#[cfg(feature = "gtk")]
mod gtk_ghostty;
#[cfg(feature = "gtk")]
mod gtk_ui;
#[cfg(feature = "gtk")]
mod gtk_webkit;
mod linux_update;
mod mobile_host;
mod project;
mod remote_tmux;
mod renderer;
mod resume_approval;
mod server;
mod shortcut_when;
mod sidebar_extension;
mod swift_sidebar;
mod terminal;
#[cfg_attr(not(feature = "gtk"), allow(dead_code))]
mod terminal_copy_mode;
mod ui;

pub fn run(args: Vec<String>) -> anyhow::Result<()> {
    if args.get(1).map(String::as_str) == Some("__sidebar-interpreter-worker") {
        swift_sidebar::run_worker().map_err(anyhow::Error::msg)
    } else {
        cli::run(args)
    }
}
