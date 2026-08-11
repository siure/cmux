use anyhow::{Context, Result};
use portable_pty::{native_pty_system, Child, CommandBuilder, MasterPty, PtySize};
use std::collections::HashMap;
use std::ffi::{CStr, CString, OsStr, OsString};
use std::fs;
use std::io::{Read, Write};
use std::os::unix::ffi::OsStrExt;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::{
    atomic::{AtomicU64, Ordering},
    Arc, Mutex,
};
use std::thread;
use std::time::{Duration, Instant};

#[derive(Clone)]
pub struct TerminalHandle {
    master: Arc<Mutex<Box<dyn MasterPty + Send>>>,
    writer: Arc<Mutex<Box<dyn Write + Send>>>,
    child: Arc<Mutex<Box<dyn Child + Send + Sync>>>,
    title_events: Arc<Mutex<Vec<String>>>,
}

#[derive(Clone, Default)]
pub struct TerminalOutputActivity {
    generation: Arc<AtomicU64>,
}

impl TerminalOutputActivity {
    pub fn generation(&self) -> u64 {
        self.generation.load(Ordering::Relaxed)
    }

    fn record_output(&self) {
        self.generation.fetch_add(1, Ordering::Relaxed);
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TerminalSize {
    pub rows: u16,
    pub cols: u16,
    pub pixel_width: u16,
    pub pixel_height: u16,
}

impl Default for TerminalSize {
    fn default() -> Self {
        Self {
            rows: 30,
            cols: 120,
            pixel_width: 0,
            pixel_height: 0,
        }
    }
}

impl TerminalHandle {
    pub fn send_text(&self, text: &str) -> Result<()> {
        let mut writer = self
            .writer
            .lock()
            .map_err(|_| anyhow::anyhow!("terminal writer lock poisoned"))?;
        writer
            .write_all(text.as_bytes())
            .context("failed to write text to terminal")?;
        writer.flush().context("failed to flush terminal writer")?;
        Ok(())
    }

    pub fn send_key(&self, key: &str) -> Result<()> {
        let mut writer = self
            .writer
            .lock()
            .map_err(|_| anyhow::anyhow!("terminal writer lock poisoned"))?;
        writer
            .write_all(&terminal_key_bytes(key)?)
            .context("failed to write key to terminal")?;
        writer.flush().context("failed to flush terminal writer")?;
        Ok(())
    }

    pub fn resize(&self, size: TerminalSize) -> Result<()> {
        let master = self
            .master
            .lock()
            .map_err(|_| anyhow::anyhow!("terminal master lock poisoned"))?;
        master
            .resize(size.to_pty_size())
            .context("failed to resize terminal PTY")
    }

    pub fn size(&self) -> Result<TerminalSize> {
        let master = self
            .master
            .lock()
            .map_err(|_| anyhow::anyhow!("terminal master lock poisoned"))?;
        master
            .get_size()
            .map(TerminalSize::from_pty_size)
            .context("failed to read terminal PTY size")
    }

    pub fn drain_title_events(&self) -> Vec<String> {
        self.title_events
            .lock()
            .map(|mut events| events.drain(..).collect())
            .unwrap_or_default()
    }

    pub fn pid(&self) -> Option<u32> {
        self.child.lock().ok().and_then(|child| child.process_id())
    }

    pub fn kill(&self) -> Result<()> {
        let mut child = self
            .child
            .lock()
            .map_err(|_| anyhow::anyhow!("terminal child lock poisoned"))?;
        child.kill().context("failed to kill terminal process")
    }

    pub fn try_wait_exit(&self) -> Result<Option<u32>> {
        let mut child = self
            .child
            .lock()
            .map_err(|_| anyhow::anyhow!("terminal child lock poisoned"))?;
        child
            .try_wait()
            .map(|status| status.map(|status| status.exit_code()))
            .context("failed to poll terminal process")
    }

    pub fn wait_for_exit(&self, timeout: Duration) -> Result<Option<u32>> {
        let deadline = Instant::now() + timeout;
        loop {
            if let Some(exit_code) = self.try_wait_exit()? {
                return Ok(Some(exit_code));
            }
            if Instant::now() >= deadline {
                return Ok(None);
            }
            thread::sleep(Duration::from_millis(10));
        }
    }
}

impl TerminalSize {
    fn to_pty_size(self) -> PtySize {
        PtySize {
            rows: self.rows.max(1),
            cols: self.cols.max(1),
            pixel_width: self.pixel_width,
            pixel_height: self.pixel_height,
        }
    }

    fn from_pty_size(size: PtySize) -> Self {
        Self {
            rows: size.rows.max(1),
            cols: size.cols.max(1),
            pixel_width: size.pixel_width,
            pixel_height: size.pixel_height,
        }
    }
}

pub(crate) fn terminal_key_bytes(key: &str) -> Result<Vec<u8>> {
    if let Some(text) = key.trim_start().strip_prefix("text:") {
        return Ok(text.as_bytes().to_vec());
    }
    let trimmed = key.trim();
    if trimmed.is_empty() {
        anyhow::bail!("unsupported key: {key}");
    }
    if trimmed.chars().count() == 1 {
        return Ok(trimmed.as_bytes().to_vec());
    }

    ParsedKey::parse(trimmed)?.sequence()
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ParsedKey {
    key: String,
    ctrl: bool,
    alt: bool,
    shift: bool,
}

impl ParsedKey {
    fn parse(key: &str) -> Result<Self> {
        let normalized = key
            .trim()
            .to_ascii_lowercase()
            .replace('+', "-")
            .split_whitespace()
            .collect::<Vec<_>>()
            .join("-");
        let parts = normalized
            .split('-')
            .filter(|part| !part.is_empty())
            .collect::<Vec<_>>();
        let mut ctrl = false;
        let mut alt = false;
        let mut shift = false;
        let mut index = 0;
        while index < parts.len() {
            match parts[index] {
                "ctrl" | "control" => ctrl = true,
                "alt" | "option" | "meta" => alt = true,
                "shift" => shift = true,
                "cmd" | "command" | "super" | "win" | "windows" => {}
                "c" if !ctrl && index + 1 < parts.len() => ctrl = true,
                "m" if !alt && index + 1 < parts.len() => alt = true,
                "s" if !shift && index + 1 < parts.len() => shift = true,
                _ => break,
            }
            index += 1;
        }
        if index >= parts.len() {
            anyhow::bail!("unsupported key: {key}");
        }
        let raw_key = parts[index..].join("-");
        Ok(Self {
            key: canonical_key_name(&raw_key),
            ctrl,
            alt,
            shift,
        })
    }

    fn sequence(&self) -> Result<Vec<u8>> {
        if self.ctrl {
            if let Some(byte) = terminal_control_byte(&self.key) {
                return Ok(maybe_alt_prefixed(vec![byte], self.alt));
            }
        }

        if self.alt && !self.ctrl && !self.shift {
            match self.key.as_str() {
                "left" => return Ok(maybe_alt_prefixed(b"b".to_vec(), true)),
                "right" => return Ok(maybe_alt_prefixed(b"f".to_vec(), true)),
                "backspace" => return Ok(maybe_alt_prefixed(vec![0x7f], true)),
                _ => {}
            }
        }

        if let Some(final_byte) = arrow_final(&self.key) {
            return Ok(modified_csi(final_byte, self.modifier_param()));
        }
        if let Some(final_byte) = home_end_final(&self.key) {
            return Ok(modified_csi(final_byte, self.modifier_param()));
        }
        if let Some(number) = tilde_key_number(&self.key) {
            return Ok(modified_tilde(number, self.modifier_param()));
        }
        if let Some(code) = function_key_code(&self.key) {
            return Ok(function_key_sequence(
                code.prefix,
                code.final_byte,
                self.modifier_param_with_base_mods(code.base_shift, code.base_ctrl),
            ));
        }

        match self.key.as_str() {
            "enter" => Ok(maybe_alt_prefixed(b"\r".to_vec(), self.alt)),
            "tab" if self.shift => Ok(b"\x1b[Z".to_vec()),
            "tab" => Ok(maybe_alt_prefixed(b"\t".to_vec(), self.alt)),
            "backspace" => Ok(maybe_alt_prefixed(vec![0x7f], self.alt)),
            "escape" => Ok(vec![0x1b]),
            "space" => Ok(maybe_alt_prefixed(b" ".to_vec(), self.alt)),
            other if other.chars().count() == 1 => {
                let mut ch = other.chars().next().unwrap();
                if self.shift && ch.is_ascii_lowercase() {
                    ch = ch.to_ascii_uppercase();
                }
                Ok(maybe_alt_prefixed(ch.to_string().into_bytes(), self.alt))
            }
            other => anyhow::bail!("unsupported key: {other}"),
        }
    }

    fn modifier_param(&self) -> Option<u8> {
        self.modifier_param_with_base_mods(false, false)
    }

    fn modifier_param_with_base_mods(&self, base_shift: bool, base_ctrl: bool) -> Option<u8> {
        let mut param = 1;
        if self.shift || base_shift {
            param += 1;
        }
        if self.alt {
            param += 2;
        }
        if self.ctrl || base_ctrl {
            param += 4;
        }
        (param > 1).then_some(param)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct FunctionKeyCode {
    prefix: Option<u8>,
    final_byte: u8,
    base_shift: bool,
    base_ctrl: bool,
}

fn canonical_key_name(key: &str) -> String {
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

pub(crate) fn terminal_control_byte(key: &str) -> Option<u8> {
    let mut chars = key.chars();
    let ch = chars.next()?;
    if chars.next().is_none() {
        if ch.is_ascii_lowercase() {
            return Some(ch as u8 - b'a' + 1);
        }
        return match ch {
            '2' => Some(0x00),
            '3' => Some(0x1b),
            '4' => Some(0x1c),
            '5' => Some(0x1d),
            '6' => Some(0x1e),
            '7' => Some(0x1f),
            '8' => Some(0x7f),
            '@' | ' ' => Some(0x00),
            '[' => Some(0x1b),
            '\\' => Some(0x1c),
            ']' => Some(0x1d),
            '^' => Some(0x1e),
            '_' => Some(0x1f),
            '?' => Some(0x7f),
            _ => None,
        };
    }
    match key {
        "space" => Some(0x00),
        "backspace" => Some(0x7f),
        "escape" => Some(0x1b),
        "enter" => Some(b'\r'),
        _ => None,
    }
}

fn arrow_final(key: &str) -> Option<u8> {
    match key {
        "up" => Some(b'A'),
        "down" => Some(b'B'),
        "right" => Some(b'C'),
        "left" => Some(b'D'),
        _ => None,
    }
}

fn home_end_final(key: &str) -> Option<u8> {
    match key {
        "home" => Some(b'H'),
        "end" => Some(b'F'),
        _ => None,
    }
}

fn tilde_key_number(key: &str) -> Option<u8> {
    match key {
        "insert" => Some(2),
        "delete" => Some(3),
        "page-up" => Some(5),
        "page-down" => Some(6),
        _ => None,
    }
}

fn function_key_code(key: &str) -> Option<FunctionKeyCode> {
    let number = key.strip_prefix('f')?.parse::<u8>().ok()?;
    let (number, base_shift, base_ctrl) = if (13..=24).contains(&number) {
        (number - 12, true, false)
    } else if number == 25 {
        (1, false, true)
    } else {
        (number, false, false)
    };
    let (prefix, final_byte) = match number {
        1 => (None, b'P'),
        2 => (None, b'Q'),
        3 => (None, b'R'),
        4 => (None, b'S'),
        5 => (Some(15), b'~'),
        6 => (Some(17), b'~'),
        7 => (Some(18), b'~'),
        8 => (Some(19), b'~'),
        9 => (Some(20), b'~'),
        10 => (Some(21), b'~'),
        11 => (Some(23), b'~'),
        12 => (Some(24), b'~'),
        _ => return None,
    };
    Some(FunctionKeyCode {
        prefix,
        final_byte,
        base_shift,
        base_ctrl,
    })
}

fn modified_csi(final_byte: u8, modifier_param: Option<u8>) -> Vec<u8> {
    if let Some(modifier_param) = modifier_param {
        format!("\x1b[1;{}{}", modifier_param, final_byte as char).into_bytes()
    } else {
        format!("\x1b[{}", final_byte as char).into_bytes()
    }
}

fn modified_tilde(number: u8, modifier_param: Option<u8>) -> Vec<u8> {
    if let Some(modifier_param) = modifier_param {
        format!("\x1b[{};{}~", number, modifier_param).into_bytes()
    } else {
        format!("\x1b[{}~", number).into_bytes()
    }
}

fn function_key_sequence(
    prefix: Option<u8>,
    final_byte: u8,
    modifier_param: Option<u8>,
) -> Vec<u8> {
    match (prefix, modifier_param) {
        (None, None) => vec![0x1b, b'O', final_byte],
        (None, Some(modifier_param)) => {
            format!("\x1b[1;{}{}", modifier_param, final_byte as char).into_bytes()
        }
        (Some(prefix), None) => format!("\x1b[{}~", prefix).into_bytes(),
        (Some(prefix), Some(modifier_param)) => {
            format!("\x1b[{};{}~", prefix, modifier_param).into_bytes()
        }
    }
}

fn maybe_alt_prefixed(mut bytes: Vec<u8>, alt: bool) -> Vec<u8> {
    if alt {
        let mut prefixed = vec![0x1b];
        prefixed.append(&mut bytes);
        prefixed
    } else {
        bytes
    }
}

pub fn spawn_terminal(
    cwd: Option<PathBuf>,
    env: HashMap<String, String>,
    command: Option<String>,
    buffer: Arc<Mutex<String>>,
    size: TerminalSize,
    output_activity: TerminalOutputActivity,
) -> Result<TerminalHandle> {
    spawn_terminal_inner(cwd, env, command, buffer, size, true, output_activity)
}

pub fn spawn_terminal_process(
    cwd: Option<PathBuf>,
    env: HashMap<String, String>,
    command: String,
    buffer: Arc<Mutex<String>>,
    size: TerminalSize,
    output_activity: TerminalOutputActivity,
) -> Result<TerminalHandle> {
    spawn_terminal_inner(
        cwd,
        env,
        Some(command),
        buffer,
        size,
        false,
        output_activity,
    )
}

fn spawn_terminal_inner(
    cwd: Option<PathBuf>,
    env: HashMap<String, String>,
    command: Option<String>,
    buffer: Arc<Mutex<String>>,
    size: TerminalSize,
    keep_shell_after_command: bool,
    output_activity: TerminalOutputActivity,
) -> Result<TerminalHandle> {
    let pty_system = native_pty_system();
    let pair = pty_system
        .openpty(size.to_pty_size())
        .context("failed to open PTY")?;

    let mut cmd = if let Some(command) = command {
        let mut builder = CommandBuilder::new("/bin/sh");
        builder.arg("-lc");
        if keep_shell_after_command {
            let shell = terminal_shell();
            builder.arg(format!("{command}; exec \"$1\" -i"));
            builder.arg("cmux");
            builder.arg(shell);
        } else {
            builder.arg(command);
        }
        builder
    } else {
        let shell = terminal_shell();
        let mut builder = CommandBuilder::new(shell);
        builder.arg("-i");
        builder
    };

    if let Some(cwd) = cwd {
        cmd.cwd(cwd);
    }
    for (key, value) in terminal_spawn_env(env) {
        cmd.env(key, value);
    }
    if let Ok((terminfo, resources)) = ensure_terminfo_resources() {
        cmd.env("TERMINFO", terminfo.display().to_string());
        let xdg = std::env::var("XDG_DATA_DIRS")
            .ok()
            .filter(|value| !value.trim().is_empty())
            .unwrap_or_else(|| "/usr/local/share:/usr/share".to_string());
        cmd.env("XDG_DATA_DIRS", format!("{}:{xdg}", resources.display()));
    }

    let child = pair
        .slave
        .spawn_command(cmd)
        .context("failed to spawn terminal shell")?;
    drop(pair.slave);

    let mut reader = pair
        .master
        .try_clone_reader()
        .context("failed to clone PTY reader")?;
    let writer = pair
        .master
        .take_writer()
        .context("failed to take PTY writer")?;
    let title_events = Arc::new(Mutex::new(Vec::new()));
    let reader_title_events = Arc::clone(&title_events);

    thread::spawn(move || {
        let mut chunk = [0_u8; 8192];
        loop {
            match reader.read(&mut chunk) {
                Ok(0) => break,
                Ok(n) => {
                    let text = String::from_utf8_lossy(&chunk[..n]);
                    let titles = terminal_title_events_from_text(&text);
                    if !titles.is_empty() {
                        if let Ok(mut events) = reader_title_events.lock() {
                            events.extend(titles);
                        }
                    }
                    if let Ok(mut out) = buffer.lock() {
                        out.push_str(&text);
                        if out.len() > 1_000_000 {
                            let keep_from = out.len().saturating_sub(700_000);
                            out.replace_range(..keep_from, "");
                        }
                        drop(out);
                        output_activity.record_output();
                    }
                }
                Err(_) => break,
            }
        }
    });

    Ok(TerminalHandle {
        master: Arc::new(Mutex::new(pair.master)),
        writer: Arc::new(Mutex::new(writer)),
        child: Arc::new(Mutex::new(child)),
        title_events,
    })
}

fn terminal_shell() -> PathBuf {
    terminal_shell_from_candidates(
        std::env::var_os("CMUX_SHELL"),
        std::env::var_os("SHELL"),
        passwd_shell_for_current_user(),
    )
}

fn terminal_shell_from_candidates(
    cmux_shell: Option<OsString>,
    environment_shell: Option<OsString>,
    passwd_shell: Option<PathBuf>,
) -> PathBuf {
    cmux_shell
        .map(PathBuf::from)
        .into_iter()
        .chain(environment_shell.map(PathBuf::from))
        .chain(passwd_shell)
        .find(|path| shell_path_is_executable(path))
        .unwrap_or_else(|| PathBuf::from("/bin/sh"))
}

fn passwd_shell_for_current_user() -> Option<PathBuf> {
    // SAFETY: getuid has no preconditions and does not dereference pointers.
    passwd_shell_for_uid(unsafe { libc::getuid() })
}

fn passwd_shell_for_uid(uid: libc::uid_t) -> Option<PathBuf> {
    const DEFAULT_PASSWD_BUFFER_SIZE: usize = 16 * 1024;
    const MAX_PASSWD_BUFFER_SIZE: usize = 1024 * 1024;

    // SAFETY: sysconf has no pointer arguments; failure is reported as -1.
    let configured_size = unsafe { libc::sysconf(libc::_SC_GETPW_R_SIZE_MAX) };
    let mut buffer_size = usize::try_from(configured_size)
        .ok()
        .filter(|size| *size > 0)
        .unwrap_or(DEFAULT_PASSWD_BUFFER_SIZE)
        .clamp(1024, MAX_PASSWD_BUFFER_SIZE);

    loop {
        // SAFETY: passwd is a C data carrier that getpwuid_r initializes before
        // any of its fields are read below.
        let mut passwd = unsafe { std::mem::zeroed::<libc::passwd>() };
        let mut result = std::ptr::null_mut();
        let mut buffer = vec![0_u8; buffer_size];
        // SAFETY: passwd, result, and the writable buffer remain alive for the
        // call. getpwuid_r writes at most buffer.len() bytes.
        let status = unsafe {
            libc::getpwuid_r(
                uid,
                &mut passwd,
                buffer.as_mut_ptr().cast(),
                buffer.len(),
                &mut result,
            )
        };
        if status == libc::ERANGE && buffer_size < MAX_PASSWD_BUFFER_SIZE {
            buffer_size = buffer_size.saturating_mul(2).min(MAX_PASSWD_BUFFER_SIZE);
            continue;
        }
        if status != 0 || result.is_null() || passwd.pw_shell.is_null() {
            return None;
        }

        // SAFETY: on success, pw_shell points to a NUL-terminated string whose
        // storage is owned by buffer until this scope ends.
        let shell = unsafe { CStr::from_ptr(passwd.pw_shell) }.to_bytes();
        return (!shell.is_empty()).then(|| PathBuf::from(OsStr::from_bytes(shell)));
    }
}

fn shell_path_is_executable(path: &Path) -> bool {
    if !path.is_absolute() || !fs::metadata(path).is_ok_and(|metadata| metadata.is_file()) {
        return false;
    }
    let Ok(path) = CString::new(path.as_os_str().as_bytes()) else {
        return false;
    };
    // SAFETY: path is NUL-terminated and remains alive for the call. AT_EACCESS
    // checks the effective credentials execve will use, including ACL/noexec
    // policy that cannot be inferred from mode bits alone.
    unsafe { libc::faccessat(libc::AT_FDCWD, path.as_ptr(), libc::X_OK, libc::AT_EACCESS) == 0 }
}

fn terminal_spawn_env(mut env: HashMap<String, String>) -> HashMap<String, String> {
    for (key, value) in terminal_identity_env() {
        env.insert(key.to_string(), value);
    }
    env
}

fn terminal_identity_env() -> [(&'static str, String); 4] {
    [
        ("TERM", "xterm-ghostty".to_string()),
        ("COLORTERM", "truecolor".to_string()),
        ("TERM_PROGRAM", "ghostty".to_string()),
        (
            "TERM_PROGRAM_VERSION",
            env!("CARGO_PKG_VERSION").to_string(),
        ),
    ]
}

pub(crate) fn terminal_title_events_from_text(text: &str) -> Vec<String> {
    let mut events = Vec::new();
    let mut rest = text;

    while let Some((_, tail)) = rest.split_once(']') {
        let (prefix_len, payload_tail) = if let Some(payload) = tail.strip_prefix("0;") {
            (2, payload)
        } else if let Some(payload) = tail.strip_prefix("1;") {
            (2, payload)
        } else if let Some(payload) = tail.strip_prefix("2;") {
            (2, payload)
        } else {
            rest = tail;
            continue;
        };
        let payload = terminal_osc_payload(payload_tail);
        let title = clean_terminal_osc_text(payload);
        if !title.is_empty() {
            events.push(title);
        }
        rest = if tail.len() > prefix_len + payload.len() {
            &tail[prefix_len + payload.len()..]
        } else {
            ""
        };
    }

    events
}

fn terminal_osc_payload(tail: &str) -> &str {
    const TERMINATORS: [&str; 12] = [
        "\u{1b}\\",
        "\u{7}",
        "\u{1b}",
        "\\x1b\\\\",
        "\\x1b\\",
        "\\033\\\\",
        "\\033\\",
        "\\e\\\\",
        "\\e\\",
        "\\x07",
        "\\007",
        "\n",
    ];

    let end = TERMINATORS
        .iter()
        .filter_map(|marker| tail.find(marker))
        .min()
        .unwrap_or(tail.len());
    &tail[..end]
}

fn clean_terminal_osc_text(value: &str) -> String {
    value
        .trim_matches(|ch| matches!(ch, '\'' | '"' | '\r' | '\n'))
        .trim()
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::{MetadataExt, PermissionsExt};

    const SHELL_PROBE_PREFIX: &str = "CMUX_SHELL_PROBE:";

    fn write_shell_probe(path: &Path) {
        fs::write(
            path,
            format!("#!/bin/sh\nprintf '%s%s\\n' '{SHELL_PROBE_PREFIX}' \"$0\"\n"),
        )
        .expect("write shell probe");
        let mut permissions = fs::metadata(path)
            .expect("shell probe metadata")
            .permissions();
        permissions.set_mode(0o755);
        fs::set_permissions(path, permissions).expect("make shell probe executable");
    }

    fn current_passwd_shell() -> String {
        let uid = fs::metadata("/proc/self")
            .expect("current process metadata")
            .uid();
        fs::read_to_string("/etc/passwd")
            .expect("read passwd database")
            .lines()
            .filter_map(|line| {
                let fields = line.split(':').collect::<Vec<_>>();
                (fields.len() >= 7 && fields[2].parse::<u32>().ok() == Some(uid))
                    .then(|| fields[6].trim().to_string())
            })
            .find(|shell| {
                !shell.is_empty()
                    && fs::metadata(shell).is_ok_and(|metadata| {
                        metadata.is_file() && metadata.permissions().mode() & 0o111 != 0
                    })
            })
            .unwrap_or_else(|| "/bin/sh".to_string())
    }

    fn run_shell_selection_probe(
        cmux_shell: Option<&str>,
        shell: Option<&str>,
        expected_shell: &str,
    ) {
        run_shell_selection_probe_with_command(cmux_shell, shell, expected_shell, None);
    }

    fn run_shell_selection_probe_with_command(
        cmux_shell: Option<&str>,
        shell: Option<&str>,
        expected_shell: &str,
        initial_command: Option<&str>,
    ) {
        let mut command =
            std::process::Command::new(std::env::current_exe().expect("test executable"));
        command
            .args([
                "--exact",
                "terminal::tests::terminal_shell_selection_probe_child",
                "--ignored",
                "--nocapture",
            ])
            .env_remove("CMUX_SHELL")
            .env_remove("SHELL")
            .envs(cmux_shell.map(|value| ("CMUX_SHELL", value)))
            .envs(shell.map(|value| ("SHELL", value)))
            .env("CMUX_TEST_EXPECTED_SHELL", expected_shell);
        if let Some(initial_command) = initial_command {
            command.env("CMUX_TEST_INITIAL_COMMAND", initial_command);
        }
        let output = command.output().expect("run isolated shell probe");

        assert!(
            output.status.success(),
            "shell probe failed:\nstdout:\n{}\nstderr:\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
    }

    fn wait_for_terminal_output(
        buffer: &Arc<Mutex<String>>,
        expected: &str,
        timeout: Duration,
    ) -> String {
        let deadline = Instant::now() + timeout;
        loop {
            let output = buffer.lock().expect("terminal buffer").clone();
            if output.contains(expected) || Instant::now() >= deadline {
                return output;
            }
            thread::sleep(Duration::from_millis(5));
        }
    }

    #[test]
    #[ignore = "helper executed in an isolated child process"]
    fn terminal_shell_selection_probe_child() {
        let expected_shell =
            std::env::var("CMUX_TEST_EXPECTED_SHELL").expect("expected shell path");
        let initial_command = std::env::var("CMUX_TEST_INITIAL_COMMAND").ok();
        let buffer = Arc::new(Mutex::new(String::new()));
        let terminal = spawn_terminal(
            None,
            HashMap::new(),
            initial_command.clone(),
            Arc::clone(&buffer),
            TerminalSize::default(),
            TerminalOutputActivity::default(),
        )
        .expect("spawn terminal shell");

        terminal
            .send_text(&format!(
                "printf '{SHELL_PROBE_PREFIX}%s\\n' \"$0\"\nexit\n"
            ))
            .expect("send shell identity probe");
        let exit = terminal
            .wait_for_exit(Duration::from_secs(3))
            .expect("wait for shell probe");
        if exit.is_none() {
            let _ = terminal.kill();
        }
        assert!(exit.is_some(), "shell probe timed out");

        // The PTY reader runs independently of the child handle. Process exit can
        // become visible just before the reader appends the shell's final bytes.
        let expected_probe = format!("{SHELL_PROBE_PREFIX}{expected_shell}");
        let output = wait_for_terminal_output(&buffer, &expected_probe, Duration::from_secs(3));
        assert!(
            output.contains(&expected_probe),
            "expected shell {expected_shell:?}, terminal output was {output:?}"
        );
        if initial_command.is_some() {
            assert!(
                output.contains("initial-command-path"),
                "initial command did not run before shell handoff: {output:?}"
            );
        }
    }

    #[test]
    fn terminal_shell_prefers_explicit_cmux_override() {
        let temp = tempfile::tempdir().expect("tempdir");
        let cmux_shell = temp.path().join("cmux-shell");
        let env_shell = temp.path().join("env-shell");
        write_shell_probe(&cmux_shell);
        write_shell_probe(&env_shell);

        run_shell_selection_probe(
            cmux_shell.to_str(),
            env_shell.to_str(),
            cmux_shell.to_str().expect("cmux shell path"),
        );
    }

    #[test]
    fn terminal_shell_runs_initial_command_before_interactive_handoff() {
        let temp = tempfile::tempdir().expect("tempdir");
        let cmux_shell = temp.path().join("cmux-shell");
        write_shell_probe(&cmux_shell);

        run_shell_selection_probe_with_command(
            cmux_shell.to_str(),
            None,
            cmux_shell.to_str().expect("cmux shell path"),
            Some("printf initial-command-path"),
        );
    }

    #[test]
    fn terminal_shell_uses_valid_shell_environment_fallback() {
        let temp = tempfile::tempdir().expect("tempdir");
        let env_shell = temp.path().join("env-shell");
        write_shell_probe(&env_shell);

        run_shell_selection_probe(
            None,
            env_shell.to_str(),
            env_shell.to_str().expect("environment shell path"),
        );
    }

    #[test]
    fn terminal_shell_skips_empty_and_invalid_overrides() {
        let temp = tempfile::tempdir().expect("tempdir");
        let env_shell = temp.path().join("env-shell");
        write_shell_probe(&env_shell);

        for cmux_shell in ["", "/definitely/missing/cmux-shell"] {
            run_shell_selection_probe(
                Some(cmux_shell),
                env_shell.to_str(),
                env_shell.to_str().expect("environment shell path"),
            );
        }
    }

    #[test]
    fn terminal_shell_skips_override_not_executable_by_current_user() {
        let temp = tempfile::tempdir().expect("tempdir");
        let blocked_shell = temp.path().join("blocked-shell");
        let env_shell = temp.path().join("env-shell");
        write_shell_probe(&blocked_shell);
        write_shell_probe(&env_shell);

        let mut permissions = fs::metadata(&blocked_shell)
            .expect("blocked shell metadata")
            .permissions();
        // The file has execute bits, but not for its owner (the test user).
        // A mode-bit-only check accepts it even though execve rejects it.
        permissions.set_mode(0o011);
        fs::set_permissions(&blocked_shell, permissions).expect("block owner execution");

        run_shell_selection_probe(
            blocked_shell.to_str(),
            env_shell.to_str(),
            env_shell.to_str().expect("environment shell path"),
        );
    }

    #[test]
    fn terminal_shell_uses_passwd_fallback_for_empty_or_invalid_shell_environment() {
        let expected = current_passwd_shell();

        for shell in [None, Some(""), Some("/definitely/missing/login-shell")] {
            run_shell_selection_probe(Some(""), shell, &expected);
        }
    }

    #[test]
    fn terminal_shell_uses_bin_sh_when_all_candidates_are_unusable() {
        assert_eq!(
            terminal_shell_from_candidates(
                Some(OsString::new()),
                Some(OsString::from("relative-shell")),
                Some(PathBuf::from("/definitely/missing/passwd-shell")),
            ),
            PathBuf::from("/bin/sh")
        );
    }

    #[test]
    fn explicit_terminal_process_command_does_not_require_a_valid_interactive_shell() {
        let buffer = Arc::new(Mutex::new(String::new()));
        let terminal = spawn_terminal_process(
            None,
            HashMap::new(),
            "printf explicit-command-path".to_string(),
            Arc::clone(&buffer),
            TerminalSize::default(),
            TerminalOutputActivity::default(),
        )
        .expect("spawn explicit terminal command");
        assert_eq!(
            terminal
                .wait_for_exit(Duration::from_secs(3))
                .expect("wait for explicit command"),
            Some(0)
        );
        let output = buffer.lock().expect("terminal buffer").clone();
        assert!(
            output.contains("explicit-command-path"),
            "output was {output:?}"
        );
    }

    #[test]
    fn terminal_key_bytes_cover_common_interactive_keys() {
        assert_eq!(terminal_key_bytes("enter").unwrap(), b"\r");
        assert_eq!(terminal_key_bytes("backspace").unwrap(), vec![0x7f]);
        assert_eq!(terminal_key_bytes("left").unwrap(), b"\x1b[D");
        assert_eq!(terminal_key_bytes("right").unwrap(), b"\x1b[C");
        assert_eq!(terminal_key_bytes("delete").unwrap(), b"\x1b[3~");
        assert_eq!(terminal_key_bytes("page-down").unwrap(), b"\x1b[6~");
        assert_eq!(terminal_key_bytes("ctrl-a").unwrap(), vec![0x01]);
        assert_eq!(terminal_key_bytes("ctrl-c").unwrap(), vec![0x03]);
        assert_eq!(terminal_key_bytes("ctrl-m").unwrap(), vec![0x0d]);
        assert_eq!(terminal_key_bytes("ctrl-s").unwrap(), vec![0x13]);
        assert_eq!(terminal_key_bytes("control-l").unwrap(), vec![0x0c]);
    }

    #[test]
    fn terminal_key_bytes_cover_numeric_control_aliases() {
        for (digit, byte) in [
            (2, 0x00),
            (3, 0x1b),
            (4, 0x1c),
            (5, 0x1d),
            (6, 0x1e),
            (7, 0x1f),
            (8, 0x7f),
        ] {
            assert_eq!(
                terminal_key_bytes(&format!("ctrl-{digit}")).unwrap(),
                vec![byte]
            );
        }
    }

    #[test]
    fn terminal_key_bytes_cover_tmux_and_ghostty_aliases() {
        assert_eq!(terminal_key_bytes("A").unwrap(), b"A");
        assert_eq!(terminal_key_bytes("C-d").unwrap(), vec![0x04]);
        assert_eq!(terminal_key_bytes("ctrl+space").unwrap(), vec![0x00]);
        assert_eq!(terminal_key_bytes("ctrl-_").unwrap(), vec![0x1f]);
        assert_eq!(terminal_key_bytes("BSpace").unwrap(), vec![0x7f]);
        assert_eq!(terminal_key_bytes("PageDown").unwrap(), b"\x1b[6~");
        assert_eq!(terminal_key_bytes("page_down").unwrap(), b"\x1b[6~");
        assert_eq!(terminal_key_bytes("insert").unwrap(), b"\x1b[2~");
        assert_eq!(terminal_key_bytes("F1").unwrap(), b"\x1bOP");
        assert_eq!(terminal_key_bytes("f12").unwrap(), b"\x1b[24~");
        assert_eq!(terminal_key_bytes("f13").unwrap(), b"\x1b[1;2P");
        assert_eq!(terminal_key_bytes("f16").unwrap(), b"\x1b[1;2S");
        assert_eq!(terminal_key_bytes("f17").unwrap(), b"\x1b[15;2~");
        assert_eq!(terminal_key_bytes("f20").unwrap(), b"\x1b[19;2~");
        assert_eq!(terminal_key_bytes("f21").unwrap(), b"\x1b[20;2~");
        assert_eq!(terminal_key_bytes("f24").unwrap(), b"\x1b[24;2~");
        assert_eq!(terminal_key_bytes("f25").unwrap(), b"\x1b[1;5P");
    }

    #[test]
    fn terminal_key_bytes_preserve_text_literal_payload() {
        assert_eq!(terminal_key_bytes("text: ").unwrap(), b" ");
        assert_eq!(terminal_key_bytes("text:\r").unwrap(), b"\r");
        assert_eq!(terminal_key_bytes("text:\n").unwrap(), b"\n");
        assert_eq!(terminal_key_bytes(" text: hi\t").unwrap(), b" hi\t");
    }

    #[test]
    fn terminal_key_bytes_cover_modified_keys() {
        assert_eq!(terminal_key_bytes("shift-tab").unwrap(), b"\x1b[Z");
        assert_eq!(terminal_key_bytes("ctrl-left").unwrap(), b"\x1b[1;5D");
        assert_eq!(terminal_key_bytes("shift-right").unwrap(), b"\x1b[1;2C");
        assert_eq!(terminal_key_bytes("option-left").unwrap(), b"\x1bb");
        assert_eq!(terminal_key_bytes("alt-right").unwrap(), b"\x1bf");
        assert_eq!(terminal_key_bytes("meta-a").unwrap(), b"\x1ba");
        assert_eq!(terminal_key_bytes("alt-up").unwrap(), b"\x1b[1;3A");
        assert_eq!(terminal_key_bytes("ctrl-alt-f5").unwrap(), b"\x1b[15;7~");
        assert_eq!(terminal_key_bytes("ctrl-f13").unwrap(), b"\x1b[1;6P");
        assert_eq!(terminal_key_bytes("alt-f24").unwrap(), b"\x1b[24;4~");
        assert_eq!(terminal_key_bytes("ctrl-f25").unwrap(), b"\x1b[1;5P");
        assert_eq!(terminal_key_bytes("shift-f25").unwrap(), b"\x1b[1;6P");
    }

    #[test]
    fn terminal_key_bytes_accept_linux_super_aliases_without_pty_mods() {
        assert_eq!(terminal_key_bytes("cmd-enter").unwrap(), b"\r");
        assert_eq!(terminal_key_bytes("command-enter").unwrap(), b"\r");
        assert_eq!(terminal_key_bytes("super-shift-p").unwrap(), b"P");
        assert_eq!(terminal_key_bytes("win-left").unwrap(), b"\x1b[D");
        assert_eq!(terminal_key_bytes("windows-alt-right").unwrap(), b"\x1bf");
        assert!(terminal_key_bytes("cmd").is_err());
    }

    #[test]
    fn terminal_size_clamps_cells_for_pty_size() {
        let size = TerminalSize {
            rows: 0,
            cols: 0,
            pixel_width: 800,
            pixel_height: 600,
        }
        .to_pty_size();
        assert_eq!(size.rows, 1);
        assert_eq!(size.cols, 1);
        assert_eq!(size.pixel_width, 800);
        assert_eq!(size.pixel_height, 600);
    }

    #[test]
    fn terminal_title_events_parse_osc_window_title_sequences() {
        assert_eq!(
            terminal_title_events_from_text("\x1b]0;Claude Code\x07"),
            vec!["Claude Code"]
        );
        assert_eq!(
            terminal_title_events_from_text("printf '\\033]2;Loading\\007'\n"),
            vec!["Loading"]
        );
        assert_eq!(
            terminal_title_events_from_text("\x1b]1;icon\x1b\\\x1b]2;window\x1b\\"),
            vec!["icon", "window"]
        );
    }

    #[test]
    fn terminal_spawn_env_protects_terminal_identity() {
        let env = terminal_spawn_env(HashMap::from([
            ("TERM".to_string(), "dumb".to_string()),
            ("COLORTERM".to_string(), "false".to_string()),
            ("TERM_PROGRAM".to_string(), "other".to_string()),
            ("TERM_PROGRAM_VERSION".to_string(), "0".to_string()),
            ("CUSTOM_ENV".to_string(), "kept".to_string()),
        ]));

        assert_eq!(env.get("TERM").map(String::as_str), Some("xterm-ghostty"));
        assert_eq!(env.get("COLORTERM").map(String::as_str), Some("truecolor"));
        assert_eq!(env.get("TERM_PROGRAM").map(String::as_str), Some("ghostty"));
        assert_eq!(
            env.get("TERM_PROGRAM_VERSION").map(String::as_str),
            Some(env!("CARGO_PKG_VERSION"))
        );
        assert_eq!(env.get("CUSTOM_ENV").map(String::as_str), Some("kept"));
    }

    #[test]
    fn terminfo_validation_rejects_placeholder_entry() {
        if !terminfo_tool_available("infocmp") {
            return;
        }
        let temp = tempfile::tempdir().expect("tempdir");
        let terminfo = temp.path().join("terminfo");
        let entry = terminfo_hex_entry_path(&terminfo);
        fs::create_dir_all(entry.parent().expect("entry parent")).expect("entry dir");
        fs::write(&entry, b"cmux-linux terminfo placeholder\n").expect("placeholder");

        assert!(!terminfo_entry_is_valid(&terminfo));
    }

    #[test]
    fn ensure_terminfo_resources_compiles_usable_ghostty_entry() {
        if !terminfo_tool_available("infocmp") || !terminfo_tool_available("tic") {
            return;
        }
        let temp = tempfile::tempdir().expect("tempdir");
        let resources = temp.path().join("resources");
        let terminfo = resources.join("terminfo");
        let stale_entry = terminfo_hex_entry_path(&terminfo);
        fs::create_dir_all(stale_entry.parent().expect("entry parent")).expect("entry dir");
        fs::write(&stale_entry, b"cmux-linux terminfo placeholder\n").expect("placeholder");

        let (actual_terminfo, actual_resources) =
            ensure_terminfo_resources_in(&resources).expect("terminfo resources");

        assert_eq!(actual_terminfo, terminfo);
        assert_eq!(actual_resources, resources);
        assert!(terminfo_entry_is_valid(&actual_terminfo));
        assert!(stale_entry.exists());
        assert_ne!(
            fs::read(&stale_entry).expect("compiled entry"),
            b"cmux-linux terminfo placeholder\n"
        );
    }
}

fn ensure_terminfo_resources() -> Result<(PathBuf, PathBuf)> {
    let resources = std::env::temp_dir().join("cmux-linux-resources");
    ensure_terminfo_resources_in(&resources)
}

fn ensure_terminfo_resources_in(resources: &Path) -> Result<(PathBuf, PathBuf)> {
    let terminfo = resources.join("terminfo");
    fs::create_dir_all(&terminfo).context("failed to create terminfo directory")?;

    if !terminfo_entry_is_valid(&terminfo) {
        compile_ghostty_terminfo(resources, &terminfo)?;
    }
    ensure_hex_terminfo_alias(&terminfo)?;
    if !terminfo_entry_is_valid(&terminfo) {
        anyhow::bail!("compiled xterm-ghostty terminfo entry could not be validated");
    }

    Ok((terminfo, resources.to_path_buf()))
}

fn compile_ghostty_terminfo(resources: &Path, terminfo: &Path) -> Result<()> {
    fs::create_dir_all(resources).context("failed to create terminfo resources directory")?;
    let source_path = resources.join("xterm-ghostty.terminfo");
    fs::write(&source_path, ghostty_terminfo_source()?)
        .context("failed to write xterm-ghostty terminfo source")?;

    let output = Command::new("tic")
        .arg("-x")
        .arg("-o")
        .arg(terminfo)
        .arg(&source_path)
        .output()
        .context("failed to run tic for xterm-ghostty terminfo")?;
    if !output.status.success() {
        anyhow::bail!(
            "tic failed while compiling xterm-ghostty terminfo: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }
    ensure_hex_terminfo_alias(terminfo)
}

fn ghostty_terminfo_source() -> Result<String> {
    if let Some(source) = infocmp_source("xterm-ghostty") {
        return Ok(source);
    }
    if let Some(source) = infocmp_source("xterm-256color") {
        return Ok(rename_terminfo_entry(
            &source,
            "xterm-ghostty|ghostty|Ghostty",
        ));
    }
    Ok("xterm-ghostty|ghostty|Ghostty,\n\tuse=xterm-256color,\n".to_string())
}

fn infocmp_source(term: &str) -> Option<String> {
    let output = Command::new("infocmp")
        .arg("-x")
        .arg(term)
        .env_remove("TERMINFO")
        .output()
        .ok()?;
    output
        .status
        .success()
        .then(|| String::from_utf8_lossy(&output.stdout).to_string())
        .filter(|source| !source.trim().is_empty())
}

fn rename_terminfo_entry(source: &str, names: &str) -> String {
    let mut renamed = String::new();
    let mut replaced = false;
    for line in source.lines() {
        if !replaced && !line.trim().is_empty() && !line.trim_start().starts_with('#') {
            renamed.push_str(names);
            renamed.push_str(",\n");
            replaced = true;
        } else {
            renamed.push_str(line);
            renamed.push('\n');
        }
    }
    if replaced {
        renamed
    } else {
        format!("{names},\n\tuse=xterm-256color,\n")
    }
}

fn terminfo_entry_is_valid(terminfo: &Path) -> bool {
    Command::new("infocmp")
        .arg("-A")
        .arg(terminfo)
        .arg("xterm-ghostty")
        .output()
        .map(|output| output.status.success())
        .unwrap_or(false)
}

fn ensure_hex_terminfo_alias(terminfo: &Path) -> Result<()> {
    let char_entry = terminfo_char_entry_path(terminfo);
    let hex_entry = terminfo_hex_entry_path(terminfo);
    if char_entry.exists() {
        if let Some(parent) = hex_entry.parent() {
            fs::create_dir_all(parent).context("failed to create hex terminfo directory")?;
        }
        fs::copy(&char_entry, &hex_entry).context("failed to mirror xterm-ghostty terminfo")?;
    }
    Ok(())
}

fn terminfo_char_entry_path(terminfo: &Path) -> PathBuf {
    terminfo.join("x").join("xterm-ghostty")
}

fn terminfo_hex_entry_path(terminfo: &Path) -> PathBuf {
    terminfo.join("78").join("xterm-ghostty")
}

#[cfg(test)]
fn terminfo_tool_available(tool: &str) -> bool {
    Command::new(tool).arg("-V").output().is_ok()
}
