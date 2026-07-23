#![recursion_limit = "256"]

use std::any::Any;
use std::panic::{self, AssertUnwindSafe};

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

fn main() {
    let args = std::env::args().collect::<Vec<_>>();
    let default_panic_hook = panic::take_hook();
    panic::set_hook(Box::new(move |info| {
        if !stdout_broken_pipe_panic(info.payload()) {
            default_panic_hook(info);
        }
    }));
    let result = panic::catch_unwind(AssertUnwindSafe(|| {
        if args.get(1).map(String::as_str) == Some("__sidebar-interpreter-worker") {
            swift_sidebar::run_worker().map_err(anyhow::Error::msg)
        } else {
            cli::run(args)
        }
    }));
    let result = match result {
        Ok(result) => result,
        Err(payload) if stdout_broken_pipe_panic(payload.as_ref()) => return,
        Err(payload) => panic::resume_unwind(payload),
    };
    if let Err(err) = result {
        eprintln!("error: {err:#}");
        std::process::exit(1);
    }
}

fn stdout_broken_pipe_panic(payload: &(dyn Any + Send)) -> bool {
    let message = payload
        .downcast_ref::<String>()
        .map(String::as_str)
        .or_else(|| payload.downcast_ref::<&str>().copied());
    message.is_some_and(|message| {
        message.starts_with("failed printing to stdout:")
            && (message.contains("Broken pipe") || message.contains("broken pipe"))
    })
}

#[cfg(test)]
mod tests {
    use super::stdout_broken_pipe_panic;

    #[test]
    fn recognizes_only_stdout_broken_pipe_panics() {
        assert!(stdout_broken_pipe_panic(
            &"failed printing to stdout: Broken pipe (os error 32)"
        ));
        assert!(stdout_broken_pipe_panic(&String::from(
            "failed printing to stdout: broken pipe"
        )));
        assert!(!stdout_broken_pipe_panic(
            &"failed printing to stderr: Broken pipe (os error 32)"
        ));
        assert!(!stdout_broken_pipe_panic(
            &"failed printing to stdout: permission denied"
        ));
    }
}
