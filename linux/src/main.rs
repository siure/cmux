use std::any::Any;
use std::panic::{self, AssertUnwindSafe};

fn main() {
    let args = std::env::args().collect::<Vec<_>>();
    let default_panic_hook = panic::take_hook();
    panic::set_hook(Box::new(move |info| {
        if !stdout_broken_pipe_panic(info.payload()) {
            default_panic_hook(info);
        }
    }));
    let result = panic::catch_unwind(AssertUnwindSafe(|| cmux_linux::run(args)));
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
