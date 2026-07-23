use anyhow::{anyhow, Context, Result};
use base64::{engine::general_purpose::STANDARD as BASE64_STANDARD, Engine as _};
use serde_json::{json, Value};
use std::collections::{HashMap, HashSet};
use std::io::{BufRead, BufReader, Read, Write};
use std::path::Path;
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;
use url::Url;
use uuid::Uuid;

const MAX_TRANSCRIPT_BYTES: usize = 1_000_000;
const RETAINED_TRANSCRIPT_BYTES: usize = 700_000;
const MAX_QUEUED_INPUT_BYTES: usize = 64 * 1024;
const MAX_OPENCODE_RESPONSE_BYTES: u64 = 1024 * 1024;

#[derive(Clone, Debug, Default)]
pub struct AgentSessionRuntimeSnapshot {
    pub status: String,
    pub ready: bool,
    pub turn_in_flight: bool,
    pub thread_id: Option<String>,
    pub transcript: String,
    pub activities: Vec<Value>,
    pub error: Option<String>,
}

pub struct CodexAppServerRuntime {
    child: Arc<Mutex<Child>>,
    writer: Arc<Mutex<ChildStdin>>,
    state: Arc<Mutex<CodexProtocolState>>,
}

struct CodexProtocolState {
    working_directory: Option<String>,
    next_request_id: u64,
    initialize_request_id: Option<u64>,
    thread_start_request_id: Option<u64>,
    turn_start_request_ids: HashSet<u64>,
    thread_id: Option<String>,
    queued_input: Option<(String, String)>,
    active_permission_mode: String,
    ready: bool,
    turn_in_flight: bool,
    failed: bool,
    error: Option<String>,
    transcript: String,
    activities: Vec<Value>,
}

impl CodexProtocolState {
    fn new(working_directory: Option<String>) -> Self {
        Self {
            working_directory,
            next_request_id: 1,
            initialize_request_id: None,
            thread_start_request_id: None,
            turn_start_request_ids: HashSet::new(),
            thread_id: None,
            queued_input: None,
            active_permission_mode: "default".to_string(),
            ready: false,
            turn_in_flight: false,
            failed: false,
            error: None,
            transcript: String::new(),
            activities: Vec::new(),
        }
    }

    fn request(&mut self, method: &str, params: Value) -> (u64, Value) {
        let id = self.next_request_id;
        self.next_request_id += 1;
        (id, json!({"id": id, "method": method, "params": params}))
    }

    fn append_transcript(&mut self, text: &str) {
        self.transcript.push_str(text);
        if self.transcript.len() > MAX_TRANSCRIPT_BYTES {
            let mut keep_from = self
                .transcript
                .len()
                .saturating_sub(RETAINED_TRANSCRIPT_BYTES);
            while keep_from < self.transcript.len() && !self.transcript.is_char_boundary(keep_from)
            {
                keep_from += 1;
            }
            self.transcript.replace_range(..keep_from, "");
        }
    }

    fn fail(&mut self, message: impl Into<String>) {
        let message = message.into();
        self.failed = true;
        self.ready = false;
        self.turn_in_flight = false;
        self.error = Some(message.clone());
        self.append_transcript(&format!("\nCodex app-server error: {message}\n"));
    }

    fn complete_turn(&mut self) {
        self.turn_in_flight = false;
        self.active_permission_mode = "default".to_string();
        if !self.transcript.ends_with('\n') {
            self.transcript.push('\n');
        }
    }
}

impl CodexAppServerRuntime {
    pub fn spawn(
        executable: &str,
        arguments: &[String],
        working_directory: Option<&str>,
        environment: &[(String, String)],
    ) -> Result<Self> {
        let mut command = Command::new(executable);
        command
            .args(arguments)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        if let Some(directory) = working_directory {
            command.current_dir(Path::new(directory));
        }
        for (key, value) in environment {
            command.env(key, value);
        }
        let mut child = command
            .spawn()
            .with_context(|| format!("failed to start Codex app-server: {executable}"))?;
        let stdin = child
            .stdin
            .take()
            .ok_or_else(|| anyhow!("Codex app-server stdin was unavailable"))?;
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| anyhow!("Codex app-server stdout was unavailable"))?;
        let stderr = child
            .stderr
            .take()
            .ok_or_else(|| anyhow!("Codex app-server stderr was unavailable"))?;
        let writer = Arc::new(Mutex::new(stdin));
        let state = Arc::new(Mutex::new(CodexProtocolState::new(
            working_directory.map(ToString::to_string),
        )));

        let stdout_state = Arc::clone(&state);
        let stdout_writer = Arc::clone(&writer);
        thread::spawn(move || {
            for line in BufReader::new(stdout).lines() {
                match line {
                    Ok(line) if !line.trim().is_empty() => {
                        process_codex_line(&line, &stdout_state, &stdout_writer)
                    }
                    Ok(_) => {}
                    Err(err) => {
                        if let Ok(mut state) = stdout_state.lock() {
                            state.fail(format!("failed to read app-server output: {err}"));
                        }
                        break;
                    }
                }
            }
        });

        let stderr_state = Arc::clone(&state);
        thread::spawn(move || {
            for line in BufReader::new(stderr).lines().map_while(|line| line.ok()) {
                if let Ok(mut state) = stderr_state.lock() {
                    state.append_transcript(&format!("{line}\n"));
                }
            }
        });

        let runtime = Self {
            child: Arc::new(Mutex::new(child)),
            writer,
            state,
        };
        runtime.initialize()?;
        Ok(runtime)
    }

    fn initialize(&self) -> Result<()> {
        let request = {
            let mut state = self
                .state
                .lock()
                .map_err(|_| anyhow!("Codex protocol state lock poisoned"))?;
            let (id, request) = state.request(
                "initialize",
                json!({
                    "clientInfo": {"name": "cmux", "title": "cmux", "version": env!("CARGO_PKG_VERSION")},
                    "capabilities": {"experimentalApi": true, "requestAttestation": false}
                }),
            );
            state.initialize_request_id = Some(id);
            request
        };
        write_json_line(&self.writer, &request)
    }

    pub fn submit(&self, text: &str, permission_mode: &str) -> Result<bool> {
        if text.as_bytes().len() > MAX_QUEUED_INPUT_BYTES {
            return Err(anyhow!("Codex prompt exceeds 64 KiB"));
        }
        let mut outgoing = None;
        let queued = {
            let mut state = self
                .state
                .lock()
                .map_err(|_| anyhow!("Codex protocol state lock poisoned"))?;
            if state.failed {
                return Err(anyhow!(
                    "Codex app-server is unavailable: {}",
                    state.error.as_deref().unwrap_or("startup failed")
                ));
            }
            if state.turn_in_flight {
                return Err(anyhow!("Codex already has a turn in flight"));
            }
            if let Some(thread_id) = state.thread_id.clone() {
                outgoing = Some(turn_start_request(
                    &mut state,
                    &thread_id,
                    text,
                    permission_mode,
                ));
                false
            } else {
                if state.queued_input.is_some() {
                    return Err(anyhow!("Codex already has a queued startup prompt"));
                }
                state.queued_input = Some((text.to_string(), permission_mode.to_string()));
                true
            }
        };
        if let Some(request) = outgoing {
            write_json_line(&self.writer, &request)?;
        }
        Ok(queued)
    }

    pub fn interrupt(&self) -> Result<()> {
        let request = {
            let mut state = self
                .state
                .lock()
                .map_err(|_| anyhow!("Codex protocol state lock poisoned"))?;
            let thread_id = state
                .thread_id
                .clone()
                .ok_or_else(|| anyhow!("Codex thread is not ready"))?;
            let (_, request) = state.request("turn/interrupt", json!({"threadId": thread_id}));
            request
        };
        write_json_line(&self.writer, &request)
    }

    pub fn snapshot(&self) -> AgentSessionRuntimeSnapshot {
        self.state
            .lock()
            .map(|state| AgentSessionRuntimeSnapshot {
                status: if state.failed {
                    "failed".to_string()
                } else if state.ready {
                    "running".to_string()
                } else {
                    "starting".to_string()
                },
                ready: state.ready,
                turn_in_flight: state.turn_in_flight,
                thread_id: state.thread_id.clone(),
                transcript: state.transcript.clone(),
                activities: state.activities.clone(),
                error: state.error.clone(),
            })
            .unwrap_or_else(|_| AgentSessionRuntimeSnapshot {
                status: "failed".to_string(),
                error: Some("Codex protocol state lock poisoned".to_string()),
                ..AgentSessionRuntimeSnapshot::default()
            })
    }

    pub fn try_wait(&self) -> Result<Option<u32>> {
        self.child
            .lock()
            .map_err(|_| anyhow!("Codex child lock poisoned"))?
            .try_wait()
            .map(|status| status.map(|status| status.code().unwrap_or(1) as u32))
            .context("failed to poll Codex app-server")
    }

    pub fn stop(&self) -> Result<()> {
        self.child
            .lock()
            .map_err(|_| anyhow!("Codex child lock poisoned"))?
            .kill()
            .context("failed to stop Codex app-server")
    }
}

pub struct ClaudeStreamJsonRuntime {
    child: Arc<Mutex<Child>>,
    writer: Arc<Mutex<ChildStdin>>,
    state: Arc<Mutex<ClaudeProtocolState>>,
}

struct ClaudeProtocolState {
    ready: bool,
    turn_in_flight: bool,
    failed: bool,
    error: Option<String>,
    transcript: String,
    accumulator: ClaudeStreamJsonAccumulator,
}

impl ClaudeProtocolState {
    fn new() -> Self {
        Self {
            ready: true,
            turn_in_flight: false,
            failed: false,
            error: None,
            transcript: String::new(),
            accumulator: ClaudeStreamJsonAccumulator::default(),
        }
    }

    fn append_transcript(&mut self, text: &str) {
        self.transcript.push_str(text);
        if self.transcript.len() > MAX_TRANSCRIPT_BYTES {
            let mut keep_from = self
                .transcript
                .len()
                .saturating_sub(RETAINED_TRANSCRIPT_BYTES);
            while keep_from < self.transcript.len() && !self.transcript.is_char_boundary(keep_from)
            {
                keep_from += 1;
            }
            self.transcript.replace_range(..keep_from, "");
        }
    }

    fn fail(&mut self, message: impl Into<String>) {
        let message = message.into();
        self.failed = true;
        self.ready = false;
        self.turn_in_flight = false;
        self.error = Some(message.clone());
        self.append_transcript(&format!("\nClaude Code stream error: {message}\n"));
    }

    fn complete_turn(&mut self) {
        self.turn_in_flight = false;
        if !self.transcript.ends_with('\n') {
            self.transcript.push('\n');
        }
    }
}

impl ClaudeStreamJsonRuntime {
    pub fn spawn(
        executable: &str,
        arguments: &[String],
        working_directory: Option<&str>,
        environment: &[(String, String)],
    ) -> Result<Self> {
        let mut command = Command::new(executable);
        command
            .args(arguments)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        if let Some(directory) = working_directory {
            command.current_dir(Path::new(directory));
        }
        for (key, value) in environment {
            command.env(key, value);
        }
        let mut child = command
            .spawn()
            .with_context(|| format!("failed to start Claude Code stream: {executable}"))?;
        let stdin = child
            .stdin
            .take()
            .ok_or_else(|| anyhow!("Claude Code stream stdin was unavailable"))?;
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| anyhow!("Claude Code stream stdout was unavailable"))?;
        let stderr = child
            .stderr
            .take()
            .ok_or_else(|| anyhow!("Claude Code stream stderr was unavailable"))?;
        let state = Arc::new(Mutex::new(ClaudeProtocolState::new()));

        let stdout_state = Arc::clone(&state);
        thread::spawn(move || {
            for line in BufReader::new(stdout).lines() {
                match line {
                    Ok(line) if !line.trim().is_empty() => {
                        if let Ok(mut state) = stdout_state.lock() {
                            let update = state.accumulator.consume_line(&line);
                            for delta in update.output {
                                state.append_transcript(&delta);
                            }
                            if update.completed_turn {
                                state.complete_turn();
                            }
                        }
                    }
                    Ok(_) => {}
                    Err(err) => {
                        if let Ok(mut state) = stdout_state.lock() {
                            state.fail(format!("failed to read stream output: {err}"));
                        }
                        break;
                    }
                }
            }
        });

        let stderr_state = Arc::clone(&state);
        thread::spawn(move || {
            for line in BufReader::new(stderr).lines().map_while(|line| line.ok()) {
                if let Ok(mut state) = stderr_state.lock() {
                    state.append_transcript(&format!("{line}\n"));
                }
            }
        });

        Ok(Self {
            child: Arc::new(Mutex::new(child)),
            writer: Arc::new(Mutex::new(stdin)),
            state,
        })
    }

    pub fn submit(&self, text: &str) -> Result<bool> {
        if text.as_bytes().len() > MAX_QUEUED_INPUT_BYTES {
            return Err(anyhow!("Claude Code prompt exceeds 64 KiB"));
        }
        {
            let mut state = self
                .state
                .lock()
                .map_err(|_| anyhow!("Claude Code protocol state lock poisoned"))?;
            if state.failed {
                return Err(anyhow!(
                    "Claude Code stream is unavailable: {}",
                    state.error.as_deref().unwrap_or("startup failed")
                ));
            }
            if state.turn_in_flight {
                return Err(anyhow!("Claude Code already has a turn in flight"));
            }
            state.turn_in_flight = true;
        }
        let request = json!({
            "type": "user",
            "message": {
                "role": "user",
                "content": [{"type": "text", "text": text}]
            }
        });
        if let Err(err) = write_json_line(&self.writer, &request) {
            if let Ok(mut state) = self.state.lock() {
                state.fail(err.to_string());
            }
            return Err(err);
        }
        Ok(false)
    }

    pub fn snapshot(&self) -> AgentSessionRuntimeSnapshot {
        self.state
            .lock()
            .map(|state| AgentSessionRuntimeSnapshot {
                status: if state.failed {
                    "failed".to_string()
                } else {
                    "running".to_string()
                },
                ready: state.ready,
                turn_in_flight: state.turn_in_flight,
                transcript: state.transcript.clone(),
                error: state.error.clone(),
                ..AgentSessionRuntimeSnapshot::default()
            })
            .unwrap_or_else(|_| AgentSessionRuntimeSnapshot {
                status: "failed".to_string(),
                error: Some("Claude Code protocol state lock poisoned".to_string()),
                ..AgentSessionRuntimeSnapshot::default()
            })
    }

    pub fn try_wait(&self) -> Result<Option<u32>> {
        self.child
            .lock()
            .map_err(|_| anyhow!("Claude Code child lock poisoned"))?
            .try_wait()
            .map(|status| status.map(|status| status.code().unwrap_or(1) as u32))
            .context("failed to poll Claude Code stream")
    }

    pub fn interrupt(&self) -> Result<()> {
        let process_id = self
            .child
            .lock()
            .map_err(|_| anyhow!("Claude Code child lock poisoned"))?
            .id();
        send_sigint(process_id)
    }

    pub fn stop(&self) -> Result<()> {
        self.child
            .lock()
            .map_err(|_| anyhow!("Claude Code child lock poisoned"))?
            .kill()
            .context("failed to stop Claude Code stream")
    }
}

pub struct OpenCodeHttpRuntime {
    child: Arc<Mutex<Child>>,
    _writer: Arc<Mutex<ChildStdin>>,
    state: Arc<Mutex<OpenCodeProtocolState>>,
}

struct OpenCodeProtocolState {
    working_directory: Option<String>,
    authorization_header: String,
    base_url: Option<Url>,
    session_id: Option<String>,
    initializing: bool,
    ready: bool,
    turn_in_flight: bool,
    failed: bool,
    error: Option<String>,
    transcript: String,
    queued_input: Option<String>,
    accumulator: OpenCodeEventTextAccumulator,
}

impl OpenCodeProtocolState {
    fn new(working_directory: Option<String>, authorization_header: String) -> Self {
        Self {
            working_directory,
            authorization_header,
            base_url: None,
            session_id: None,
            initializing: false,
            ready: false,
            turn_in_flight: false,
            failed: false,
            error: None,
            transcript: String::new(),
            queued_input: None,
            accumulator: OpenCodeEventTextAccumulator::default(),
        }
    }

    fn append_transcript(&mut self, text: &str) {
        self.transcript.push_str(text);
        if self.transcript.len() > MAX_TRANSCRIPT_BYTES {
            let mut keep_from = self
                .transcript
                .len()
                .saturating_sub(RETAINED_TRANSCRIPT_BYTES);
            while keep_from < self.transcript.len() && !self.transcript.is_char_boundary(keep_from)
            {
                keep_from += 1;
            }
            self.transcript.replace_range(..keep_from, "");
        }
    }

    fn fail(&mut self, message: impl Into<String>) {
        let message = message.into();
        self.failed = true;
        self.ready = false;
        self.turn_in_flight = false;
        self.error = Some(message.clone());
        self.append_transcript(&format!("\nOpenCode session error: {message}\n"));
    }

    fn complete_turn(&mut self) {
        self.turn_in_flight = false;
        if !self.transcript.ends_with('\n') {
            self.transcript.push('\n');
        }
    }
}

impl OpenCodeHttpRuntime {
    pub fn spawn(
        executable: &str,
        arguments: &[String],
        working_directory: Option<&str>,
        environment: &[(String, String)],
    ) -> Result<Self> {
        let mut launch_environment = environment.to_vec();
        let username = environment_value(&launch_environment, "OPENCODE_SERVER_USERNAME")
            .filter(|value| !value.is_empty())
            .unwrap_or_else(|| "opencode".to_string());
        let password = environment_value(&launch_environment, "OPENCODE_SERVER_PASSWORD")
            .filter(|value| !value.is_empty())
            .unwrap_or_else(|| format!("{}-{}", Uuid::new_v4(), Uuid::new_v4()));
        set_environment_value(
            &mut launch_environment,
            "OPENCODE_SERVER_USERNAME",
            &username,
        );
        set_environment_value(
            &mut launch_environment,
            "OPENCODE_SERVER_PASSWORD",
            &password,
        );
        let authorization_header = format!(
            "Basic {}",
            BASE64_STANDARD.encode(format!("{username}:{password}"))
        );

        let mut command = Command::new(executable);
        command
            .args(arguments)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        if let Some(directory) = working_directory {
            command.current_dir(Path::new(directory));
        }
        for (key, value) in &launch_environment {
            command.env(key, value);
        }
        let mut child = command
            .spawn()
            .with_context(|| format!("failed to start OpenCode server: {executable}"))?;
        let stdin = child
            .stdin
            .take()
            .ok_or_else(|| anyhow!("OpenCode server stdin was unavailable"))?;
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| anyhow!("OpenCode server stdout was unavailable"))?;
        let stderr = child
            .stderr
            .take()
            .ok_or_else(|| anyhow!("OpenCode server stderr was unavailable"))?;
        let child = Arc::new(Mutex::new(child));
        let state = Arc::new(Mutex::new(OpenCodeProtocolState::new(
            working_directory.map(ToString::to_string),
            authorization_header,
        )));

        let stdout_state = Arc::clone(&state);
        let stdout_child = Arc::clone(&child);
        thread::spawn(move || {
            for line in BufReader::new(stdout).lines() {
                match line {
                    Ok(line) => {
                        process_opencode_process_line(&line, true, &stdout_state, &stdout_child)
                    }
                    Err(err) => {
                        fail_opencode_runtime(
                            &stdout_state,
                            &stdout_child,
                            format!("failed to read server output: {err}"),
                        );
                        break;
                    }
                }
            }
        });

        let stderr_state = Arc::clone(&state);
        let stderr_child = Arc::clone(&child);
        thread::spawn(move || {
            for line in BufReader::new(stderr).lines() {
                match line {
                    Ok(line) => {
                        process_opencode_process_line(&line, false, &stderr_state, &stderr_child)
                    }
                    Err(err) => {
                        fail_opencode_runtime(
                            &stderr_state,
                            &stderr_child,
                            format!("failed to read server error output: {err}"),
                        );
                        break;
                    }
                }
            }
        });

        Ok(Self {
            child,
            _writer: Arc::new(Mutex::new(stdin)),
            state,
        })
    }

    pub fn submit(&self, text: &str) -> Result<bool> {
        if text.as_bytes().len() > MAX_QUEUED_INPUT_BYTES {
            return Err(anyhow!("OpenCode prompt exceeds 64 KiB"));
        }
        let target = {
            let mut state = self
                .state
                .lock()
                .map_err(|_| anyhow!("OpenCode protocol state lock poisoned"))?;
            if state.failed {
                return Err(anyhow!(
                    "OpenCode session is unavailable: {}",
                    state.error.as_deref().unwrap_or("startup failed")
                ));
            }
            if state.turn_in_flight {
                return Err(anyhow!("OpenCode already has a turn in flight"));
            }
            match (state.base_url.clone(), state.session_id.clone()) {
                (Some(base_url), Some(session_id)) if state.ready => {
                    state.turn_in_flight = true;
                    Some((
                        base_url,
                        session_id,
                        state.working_directory.clone(),
                        state.authorization_header.clone(),
                    ))
                }
                _ => {
                    if state.queued_input.is_some() {
                        return Err(anyhow!("OpenCode already has a queued startup prompt"));
                    }
                    state.queued_input = Some(text.to_string());
                    None
                }
            }
        };
        let Some((base_url, session_id, working_directory, authorization_header)) = target else {
            return Ok(true);
        };
        if let Err(err) = opencode_post_prompt(
            &base_url,
            &session_id,
            working_directory.as_deref(),
            &authorization_header,
            text,
        ) {
            if let Ok(mut state) = self.state.lock() {
                state.turn_in_flight = false;
            }
            return Err(err);
        }
        Ok(false)
    }

    pub fn snapshot(&self) -> AgentSessionRuntimeSnapshot {
        self.state
            .lock()
            .map(|state| AgentSessionRuntimeSnapshot {
                status: if state.failed {
                    "failed".to_string()
                } else if state.ready {
                    "running".to_string()
                } else {
                    "starting".to_string()
                },
                ready: state.ready,
                turn_in_flight: state.turn_in_flight,
                thread_id: state.session_id.clone(),
                transcript: state.transcript.clone(),
                error: state.error.clone(),
                ..AgentSessionRuntimeSnapshot::default()
            })
            .unwrap_or_else(|_| AgentSessionRuntimeSnapshot {
                status: "failed".to_string(),
                error: Some("OpenCode protocol state lock poisoned".to_string()),
                ..AgentSessionRuntimeSnapshot::default()
            })
    }

    pub fn try_wait(&self) -> Result<Option<u32>> {
        self.child
            .lock()
            .map_err(|_| anyhow!("OpenCode child lock poisoned"))?
            .try_wait()
            .map(|status| status.map(|status| status.code().unwrap_or(1) as u32))
            .context("failed to poll OpenCode server")
    }

    pub fn interrupt(&self) -> Result<()> {
        let (base_url, session_id, working_directory, authorization_header) = {
            let state = self
                .state
                .lock()
                .map_err(|_| anyhow!("OpenCode protocol state lock poisoned"))?;
            (
                state
                    .base_url
                    .clone()
                    .ok_or_else(|| anyhow!("OpenCode server is not ready"))?,
                state
                    .session_id
                    .clone()
                    .ok_or_else(|| anyhow!("OpenCode session is not ready"))?,
                state.working_directory.clone(),
                state.authorization_header.clone(),
            )
        };
        let url = opencode_url(
            &base_url,
            &format!("session/{session_id}/abort"),
            working_directory.as_deref(),
        )?;
        opencode_post_json(&url, &authorization_header, &json!({}))?;
        if let Ok(mut state) = self.state.lock() {
            state.complete_turn();
        }
        Ok(())
    }

    pub fn stop(&self) -> Result<()> {
        self.child
            .lock()
            .map_err(|_| anyhow!("OpenCode child lock poisoned"))?
            .kill()
            .context("failed to stop OpenCode server")
    }
}

pub enum AgentSessionRuntime {
    Codex(CodexAppServerRuntime),
    Claude(ClaudeStreamJsonRuntime),
    OpenCode(OpenCodeHttpRuntime),
}

impl AgentSessionRuntime {
    pub fn spawn_codex(
        executable: &str,
        arguments: &[String],
        working_directory: Option<&str>,
        environment: &[(String, String)],
    ) -> Result<Self> {
        CodexAppServerRuntime::spawn(executable, arguments, working_directory, environment)
            .map(Self::Codex)
    }

    pub fn spawn_claude(
        executable: &str,
        arguments: &[String],
        working_directory: Option<&str>,
        environment: &[(String, String)],
    ) -> Result<Self> {
        ClaudeStreamJsonRuntime::spawn(executable, arguments, working_directory, environment)
            .map(Self::Claude)
    }

    pub fn spawn_opencode(
        executable: &str,
        arguments: &[String],
        working_directory: Option<&str>,
        environment: &[(String, String)],
    ) -> Result<Self> {
        OpenCodeHttpRuntime::spawn(executable, arguments, working_directory, environment)
            .map(Self::OpenCode)
    }

    pub fn submit(&self, text: &str, permission_mode: &str) -> Result<bool> {
        match self {
            Self::Codex(runtime) => runtime.submit(text, permission_mode),
            Self::Claude(runtime) => runtime.submit(text),
            Self::OpenCode(runtime) => runtime.submit(text),
        }
    }

    pub fn snapshot(&self) -> AgentSessionRuntimeSnapshot {
        match self {
            Self::Codex(runtime) => runtime.snapshot(),
            Self::Claude(runtime) => runtime.snapshot(),
            Self::OpenCode(runtime) => runtime.snapshot(),
        }
    }

    pub fn try_wait(&self) -> Result<Option<u32>> {
        match self {
            Self::Codex(runtime) => runtime.try_wait(),
            Self::Claude(runtime) => runtime.try_wait(),
            Self::OpenCode(runtime) => runtime.try_wait(),
        }
    }

    pub fn interrupt(&self) -> Result<()> {
        match self {
            Self::Codex(runtime) => runtime.interrupt(),
            Self::Claude(runtime) => runtime.interrupt(),
            Self::OpenCode(runtime) => runtime.interrupt(),
        }
    }

    pub fn stop(&self) -> Result<()> {
        match self {
            Self::Codex(runtime) => runtime.stop(),
            Self::Claude(runtime) => runtime.stop(),
            Self::OpenCode(runtime) => runtime.stop(),
        }
    }
}

#[derive(Default)]
struct ClaudeStreamJsonAccumulator {
    emitted_character_count_by_message_id: HashMap<String, usize>,
    message_id_order: Vec<String>,
    current_message_id: Option<String>,
    pending_delta_character_count: usize,
    emitted_any_assistant_text: bool,
}

struct ClaudeStreamUpdate {
    output: Vec<String>,
    completed_turn: bool,
}

impl ClaudeStreamJsonAccumulator {
    const MAX_TRACKED_MESSAGES: usize = 16;

    fn consume_line(&mut self, line: &str) -> ClaudeStreamUpdate {
        let Ok(object) = serde_json::from_str::<Value>(line.trim()) else {
            return ClaudeStreamUpdate {
                output: Vec::new(),
                completed_turn: false,
            };
        };

        if let Some(message_id) = assistant_message_id_from_start(&object) {
            self.remember_message_id(&message_id);
            self.current_message_id = Some(message_id);
            self.pending_delta_character_count = 0;
            return ClaudeStreamUpdate {
                output: Vec::new(),
                completed_turn: false,
            };
        }

        if let Some(delta) = self
            .assistant_text_delta(&object)
            .filter(|delta| !delta.is_empty())
        {
            self.emitted_any_assistant_text = true;
            if let Some(message_id) = self.current_message_id.clone() {
                self.remember_message_id(&message_id);
                *self
                    .emitted_character_count_by_message_id
                    .entry(message_id)
                    .or_default() += delta.chars().count();
            } else {
                self.pending_delta_character_count += delta.chars().count();
            }
            return ClaudeStreamUpdate {
                output: vec![delta],
                completed_turn: false,
            };
        }

        let completed_turn = claude_completes_turn(&object);
        if !self.emitted_any_assistant_text
            && object.get("type").and_then(Value::as_str) == Some("result")
        {
            if let Some(result) = object
                .get("result")
                .and_then(Value::as_str)
                .filter(|result| !result.is_empty())
            {
                self.emitted_any_assistant_text = true;
                self.reset_turn_tracking();
                return ClaudeStreamUpdate {
                    output: vec![result.to_string()],
                    completed_turn: true,
                };
            }
        }
        if completed_turn {
            self.reset_turn_tracking();
        }
        ClaudeStreamUpdate {
            output: Vec::new(),
            completed_turn,
        }
    }

    fn assistant_text_delta(&mut self, object: &Value) -> Option<String> {
        if object.get("type").and_then(Value::as_str) == Some("content_block_delta") {
            return object
                .pointer("/delta/text")
                .and_then(Value::as_str)
                .map(ToString::to_string);
        }
        if object.get("type").and_then(Value::as_str) != Some("assistant") {
            return None;
        }
        let message = object.get("message").unwrap_or(object);
        let full_text = claude_content_text(message.get("content"));
        if full_text.is_empty() {
            return None;
        }
        let message_id = message
            .get("id")
            .and_then(Value::as_str)
            .unwrap_or("assistant")
            .to_string();
        self.remember_message_id(&message_id);
        let previous_character_count = self
            .emitted_character_count_by_message_id
            .get(&message_id)
            .copied()
            .unwrap_or_else(|| {
                self.pending_delta_character_count
                    .min(full_text.chars().count())
            });
        self.emitted_character_count_by_message_id
            .insert(message_id.clone(), full_text.chars().count());
        if self.current_message_id.as_deref() == Some(&message_id) {
            self.current_message_id = None;
        }
        self.pending_delta_character_count = 0;
        if previous_character_count > 0 {
            return Some(full_text.chars().skip(previous_character_count).collect());
        }
        Some(full_text)
    }

    fn remember_message_id(&mut self, message_id: &str) {
        if !self
            .message_id_order
            .iter()
            .any(|existing| existing == message_id)
        {
            self.message_id_order.push(message_id.to_string());
        }
        while self.message_id_order.len() > Self::MAX_TRACKED_MESSAGES {
            let removed = self.message_id_order.remove(0);
            self.emitted_character_count_by_message_id.remove(&removed);
        }
    }

    fn reset_turn_tracking(&mut self) {
        self.emitted_character_count_by_message_id.clear();
        self.message_id_order.clear();
        self.current_message_id = None;
        self.pending_delta_character_count = 0;
        self.emitted_any_assistant_text = false;
    }
}

fn assistant_message_id_from_start(object: &Value) -> Option<String> {
    (object.get("type").and_then(Value::as_str) == Some("message_start")
        && object.pointer("/message/role").and_then(Value::as_str) == Some("assistant"))
    .then(|| {
        object
            .pointer("/message/id")
            .and_then(Value::as_str)
            .filter(|id| !id.is_empty())
            .map(ToString::to_string)
    })
    .flatten()
}

fn claude_completes_turn(object: &Value) -> bool {
    matches!(
        object.get("type").and_then(Value::as_str),
        Some("result" | "message_stop" | "done")
    )
}

fn claude_content_text(content: Option<&Value>) -> String {
    match content {
        Some(Value::String(text)) => text.clone(),
        Some(Value::Object(part)) => {
            if part
                .get("type")
                .and_then(Value::as_str)
                .is_some_and(|kind| kind != "text")
            {
                String::new()
            } else {
                part.get("text")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_string()
            }
        }
        Some(Value::Array(parts)) => parts
            .iter()
            .map(|part| claude_content_text(Some(part)))
            .collect(),
        _ => String::new(),
    }
}

fn environment_value(environment: &[(String, String)], key: &str) -> Option<String> {
    environment
        .iter()
        .rev()
        .find(|(candidate, _)| candidate == key)
        .map(|(_, value)| value.clone())
}

fn set_environment_value(environment: &mut Vec<(String, String)>, key: &str, value: &str) {
    environment.retain(|(candidate, _)| candidate != key);
    environment.push((key.to_string(), value.to_string()));
}

fn process_opencode_process_line(
    line: &str,
    stdout: bool,
    state: &Arc<Mutex<OpenCodeProtocolState>>,
    child: &Arc<Mutex<Child>>,
) {
    if let Some(base_url) = opencode_server_url(line) {
        let should_initialize = state
            .lock()
            .map(|mut state| {
                if state.base_url.is_some() || state.initializing || state.failed {
                    return false;
                }
                state.base_url = Some(base_url.clone());
                state.initializing = true;
                true
            })
            .unwrap_or(false);
        if should_initialize {
            let state = Arc::clone(state);
            let child = Arc::clone(child);
            thread::spawn(move || initialize_opencode_session(base_url, state, child));
        }
        return;
    }
    if !stdout && !line.is_empty() {
        if let Ok(mut state) = state.lock() {
            state.append_transcript(&format!("{line}\n"));
        }
    }
}

fn opencode_server_url(line: &str) -> Option<Url> {
    let marker = "opencode server listening on ";
    let raw_url = line
        .split_once(marker)?
        .1
        .split_whitespace()
        .next()
        .filter(|value| !value.is_empty())?;
    let url = Url::parse(raw_url).ok()?;
    if !matches!(url.scheme(), "http" | "https") {
        return None;
    }
    let host = url.host_str()?.to_ascii_lowercase();
    matches!(host.as_str(), "localhost" | "127.0.0.1" | "::1").then_some(url)
}

fn initialize_opencode_session(
    base_url: Url,
    state: Arc<Mutex<OpenCodeProtocolState>>,
    child: Arc<Mutex<Child>>,
) {
    let (working_directory, authorization_header) = match state.lock() {
        Ok(state) => (
            state.working_directory.clone(),
            state.authorization_header.clone(),
        ),
        Err(_) => return,
    };
    let result = (|| -> Result<String> {
        let url = opencode_url(&base_url, "session", working_directory.as_deref())?;
        let response = opencode_post_json(&url, &authorization_header, &json!({}))?;
        response
            .get("id")
            .and_then(Value::as_str)
            .filter(|id| !id.is_empty())
            .map(ToString::to_string)
            .ok_or_else(|| anyhow!("OpenCode session response did not include an id"))
    })();
    let session_id = match result {
        Ok(session_id) => session_id,
        Err(err) => {
            fail_opencode_runtime(&state, &child, err.to_string());
            return;
        }
    };

    let queued_input = match state.lock() {
        Ok(mut state) => {
            state.session_id = Some(session_id.clone());
            state.initializing = false;
            state.ready = true;
            let queued_input = state.queued_input.take();
            if queued_input.is_some() {
                state.turn_in_flight = true;
            }
            queued_input
        }
        Err(_) => return,
    };

    let event_state = Arc::clone(&state);
    let event_child = Arc::clone(&child);
    let event_base_url = base_url.clone();
    let event_session_id = session_id.clone();
    let event_working_directory = working_directory.clone();
    let event_authorization_header = authorization_header.clone();
    thread::spawn(move || {
        consume_opencode_event_stream(
            event_base_url,
            event_session_id,
            event_working_directory,
            event_authorization_header,
            event_state,
            event_child,
        )
    });

    if let Some(text) = queued_input {
        if let Err(err) = opencode_post_prompt(
            &base_url,
            &session_id,
            working_directory.as_deref(),
            &authorization_header,
            &text,
        ) {
            fail_opencode_runtime(&state, &child, err.to_string());
        }
    }
}

fn opencode_url(base_url: &Url, path: &str, working_directory: Option<&str>) -> Result<Url> {
    let mut base = base_url.clone();
    if !base.path().ends_with('/') {
        base.set_path(&format!("{}/", base.path()));
    }
    let mut url = base
        .join(path)
        .with_context(|| format!("invalid OpenCode URL path: {path}"))?;
    if let Some(directory) = working_directory {
        url.query_pairs_mut().append_pair("directory", directory);
    }
    Ok(url)
}

fn opencode_post_prompt(
    base_url: &Url,
    session_id: &str,
    working_directory: Option<&str>,
    authorization_header: &str,
    text: &str,
) -> Result<()> {
    let url = opencode_url(
        base_url,
        &format!("session/{session_id}/prompt_async"),
        working_directory,
    )?;
    opencode_post_json(
        &url,
        authorization_header,
        &json!({"parts": [{"type": "text", "text": text}]}),
    )?;
    Ok(())
}

fn opencode_post_json(url: &Url, authorization_header: &str, body: &Value) -> Result<Value> {
    let client = reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(30))
        .build()
        .context("failed to create OpenCode HTTP client")?;
    let response = client
        .post(url.clone())
        .header(reqwest::header::AUTHORIZATION, authorization_header)
        .header(reqwest::header::CONTENT_TYPE, "application/json")
        .json(body)
        .send()
        .with_context(|| format!("OpenCode request failed: {url}"))?;
    if !response.status().is_success() {
        return Err(anyhow!(
            "OpenCode request returned HTTP {}",
            response.status()
        ));
    }
    let mut bytes = Vec::new();
    response
        .take(MAX_OPENCODE_RESPONSE_BYTES + 1)
        .read_to_end(&mut bytes)
        .context("failed to read OpenCode response")?;
    if bytes.len() as u64 > MAX_OPENCODE_RESPONSE_BYTES {
        return Err(anyhow!("OpenCode response exceeded 1 MiB"));
    }
    if bytes.is_empty() {
        return Ok(json!({}));
    }
    serde_json::from_slice(&bytes).context("OpenCode response was not valid JSON")
}

fn consume_opencode_event_stream(
    base_url: Url,
    session_id: String,
    working_directory: Option<String>,
    authorization_header: String,
    state: Arc<Mutex<OpenCodeProtocolState>>,
    child: Arc<Mutex<Child>>,
) {
    let result = (|| -> Result<()> {
        let url = opencode_url(&base_url, "event", working_directory.as_deref())?;
        let client = reqwest::blocking::Client::builder()
            .timeout(Duration::from_secs(3600))
            .build()
            .context("failed to create OpenCode event client")?;
        let response = client
            .get(url.clone())
            .header(reqwest::header::AUTHORIZATION, authorization_header)
            .header(reqwest::header::ACCEPT, "text/event-stream")
            .send()
            .with_context(|| format!("OpenCode event stream failed: {url}"))?;
        if !response.status().is_success() {
            return Err(anyhow!(
                "OpenCode event stream returned HTTP {}",
                response.status()
            ));
        }
        let mut parser = OpenCodeEventStreamParser::default();
        for line in BufReader::new(response).lines() {
            let line = line.context("failed to read OpenCode event stream")?;
            for event in parser.consume_line(&line) {
                handle_opencode_event(&state, &session_id, &event);
            }
        }
        for event in parser.flush() {
            handle_opencode_event(&state, &session_id, &event);
        }
        Ok(())
    })();
    if let Err(err) = result {
        if opencode_child_is_running(&child) {
            fail_opencode_runtime(&state, &child, err.to_string());
        }
    } else if opencode_child_is_running(&child) {
        fail_opencode_runtime(&state, &child, "OpenCode event stream disconnected");
    }
}

fn handle_opencode_event(
    state: &Arc<Mutex<OpenCodeProtocolState>>,
    session_id: &str,
    event: &Value,
) {
    if let Ok(mut state) = state.lock() {
        if state.session_id.as_deref() != Some(session_id) {
            return;
        }
        let completed_turn =
            OpenCodeEventTextAccumulator::completes_assistant_turn(event, session_id);
        let output = state.accumulator.consume_event(event, session_id);
        for delta in output {
            state.append_transcript(&delta);
        }
        if completed_turn {
            state.complete_turn();
        }
    }
}

fn opencode_child_is_running(child: &Arc<Mutex<Child>>) -> bool {
    child
        .lock()
        .ok()
        .and_then(|mut child| child.try_wait().ok())
        .flatten()
        .is_none()
}

fn fail_opencode_runtime(
    state: &Arc<Mutex<OpenCodeProtocolState>>,
    child: &Arc<Mutex<Child>>,
    message: impl Into<String>,
) {
    if let Ok(mut state) = state.lock() {
        if state.failed {
            return;
        }
        state.fail(message);
    }
    if let Ok(mut child) = child.lock() {
        let _ = child.kill();
    }
}

#[derive(Default)]
struct OpenCodeEventStreamParser {
    data_lines: Vec<String>,
    data_byte_count: usize,
}

impl OpenCodeEventStreamParser {
    const MAX_EVENT_DATA_BYTES: usize = 1024 * 1024;

    fn consume_line(&mut self, line: &str) -> Vec<Value> {
        let line = line.trim_end_matches('\r');
        if line.is_empty() {
            return self.flush();
        }
        let Some(mut data) = line.strip_prefix("data:") else {
            return Vec::new();
        };
        if let Some(stripped) = data.strip_prefix(' ') {
            data = stripped;
        }
        let separator_bytes = usize::from(!self.data_lines.is_empty());
        self.data_byte_count = self
            .data_byte_count
            .saturating_add(data.len())
            .saturating_add(separator_bytes);
        if self.data_byte_count > Self::MAX_EVENT_DATA_BYTES {
            self.reset();
            return Vec::new();
        }
        self.data_lines.push(data.to_string());
        Vec::new()
    }

    fn flush(&mut self) -> Vec<Value> {
        if self.data_lines.is_empty() {
            return Vec::new();
        }
        let data = self.data_lines.join("\n");
        self.reset();
        serde_json::from_str::<Value>(&data)
            .ok()
            .filter(Value::is_object)
            .into_iter()
            .collect()
    }

    fn reset(&mut self) {
        self.data_lines.clear();
        self.data_byte_count = 0;
    }
}

#[derive(Default)]
struct OpenCodeEventTextAccumulator {
    message_role_by_id: HashMap<String, String>,
    message_id_order: Vec<String>,
    message_id_by_part_id: HashMap<String, String>,
    is_text_part_by_id: HashMap<String, bool>,
    text_by_part_id: HashMap<String, String>,
    stored_text_start_offset_by_part_id: HashMap<String, usize>,
    emitted_character_count_by_part_id: HashMap<String, usize>,
}

impl OpenCodeEventTextAccumulator {
    const MAX_TRACKED_MESSAGES: usize = 16;
    const MAX_TRACKED_PART_TEXT_CHARACTERS: usize = 256 * 1024;

    fn consume_event(&mut self, event: &Value, session_id: &str) -> Vec<String> {
        let Some(kind) = event.get("type").and_then(Value::as_str) else {
            return Vec::new();
        };
        let Some(properties) = event.get("properties").and_then(Value::as_object) else {
            return Vec::new();
        };
        if opencode_event_session_id(properties).as_deref() != Some(session_id) {
            return Vec::new();
        }
        match kind {
            "message.updated" => self.consume_message_updated(properties),
            "message.part.updated" => self.consume_part_updated(properties),
            "message.part.delta" => self.consume_part_delta(properties),
            _ => Vec::new(),
        }
    }

    fn completes_assistant_turn(event: &Value, session_id: &str) -> bool {
        let Some(kind) = event.get("type").and_then(Value::as_str) else {
            return false;
        };
        let Some(properties) = event.get("properties").and_then(Value::as_object) else {
            return false;
        };
        if opencode_event_session_id(properties).as_deref() != Some(session_id) {
            return false;
        }
        match kind {
            "session.idle" => true,
            "session.status" => opencode_status_is_idle(properties.get("status")),
            "message.updated" => {
                let info = properties
                    .get("info")
                    .and_then(Value::as_object)
                    .or_else(|| properties.get("message").and_then(Value::as_object));
                info.is_some_and(|info| {
                    opencode_first_string([info.get("role"), properties.get("role")]).as_deref()
                        == Some("assistant")
                        && (opencode_message_has_completed_time(info)
                            || opencode_first_string([
                                info.get("finish"),
                                info.get("finishedReason"),
                                properties.get("finish"),
                            ])
                            .is_some()
                            || info.get("error").is_some())
                })
            }
            _ => false,
        }
    }

    fn consume_message_updated(
        &mut self,
        properties: &serde_json::Map<String, Value>,
    ) -> Vec<String> {
        let Some(info) = properties
            .get("info")
            .and_then(Value::as_object)
            .or_else(|| properties.get("message").and_then(Value::as_object))
        else {
            return Vec::new();
        };
        let Some(message_id) = opencode_first_string([
            info.get("id"),
            properties.get("messageID"),
            properties.get("messageId"),
        ]) else {
            return Vec::new();
        };
        let Some(role) = opencode_first_string([info.get("role"), properties.get("role")]) else {
            return Vec::new();
        };
        self.remember_message_id(&message_id);
        self.message_role_by_id
            .insert(message_id.clone(), role.clone());
        if role != "assistant" {
            return Vec::new();
        }
        let part_ids = self
            .message_id_by_part_id
            .iter()
            .filter(|(_, candidate)| *candidate == &message_id)
            .map(|(part_id, _)| part_id.clone())
            .collect::<Vec<_>>();
        let output = part_ids
            .iter()
            .flat_map(|part_id| self.flush_part(part_id))
            .collect::<Vec<_>>();
        if opencode_message_has_completed_time(info)
            || opencode_first_string([
                info.get("finish"),
                info.get("finishedReason"),
                properties.get("finish"),
            ])
            .is_some()
            || info.get("error").is_some()
        {
            self.prune_message(&message_id);
        }
        output
    }

    fn consume_part_updated(&mut self, properties: &serde_json::Map<String, Value>) -> Vec<String> {
        let Some(part) = properties.get("part").and_then(Value::as_object) else {
            return Vec::new();
        };
        let (Some(part_id), Some(message_id)) = (
            part.get("id").and_then(Value::as_str),
            part.get("messageID").and_then(Value::as_str),
        ) else {
            return Vec::new();
        };
        let part_id = part_id.to_string();
        let message_id = message_id.to_string();
        self.message_id_by_part_id
            .insert(part_id.clone(), message_id.clone());
        self.remember_message_id(&message_id);
        if part.get("type").and_then(Value::as_str) != Some("text")
            || part.get("ignored").and_then(Value::as_bool) == Some(true)
        {
            self.prune_part(&part_id);
            return Vec::new();
        }
        self.is_text_part_by_id.insert(part_id.clone(), true);
        let Some(text) = opencode_first_content_string([
            part.get("text"),
            part.get("textDelta"),
            part.get("content"),
        ]) else {
            return Vec::new();
        };
        if text.chars().count() >= self.source_character_count(&part_id) {
            self.store_bounded_text(&part_id, &text, 0);
        }
        self.flush_full_text(&part_id, &text)
    }

    fn consume_part_delta(&mut self, properties: &serde_json::Map<String, Value>) -> Vec<String> {
        if properties.get("field").and_then(Value::as_str) != Some("text") {
            return Vec::new();
        }
        let (Some(part_id), Some(message_id), Some(delta)) = (
            properties.get("partID").and_then(Value::as_str),
            properties.get("messageID").and_then(Value::as_str),
            properties.get("delta").and_then(Value::as_str),
        ) else {
            return Vec::new();
        };
        if delta.is_empty() {
            return Vec::new();
        }
        let part_id = part_id.to_string();
        let message_id = message_id.to_string();
        self.message_id_by_part_id
            .insert(part_id.clone(), message_id.clone());
        self.remember_message_id(&message_id);
        if self.is_text_part_by_id.get(&part_id) == Some(&true)
            && self.message_role_by_id.get(&message_id).map(String::as_str) == Some("assistant")
        {
            *self
                .emitted_character_count_by_part_id
                .entry(part_id)
                .or_default() += delta.chars().count();
            return vec![delta.to_string()];
        }
        let source_start = self
            .stored_text_start_offset_by_part_id
            .get(&part_id)
            .copied()
            .unwrap_or(0);
        let combined = format!(
            "{}{delta}",
            self.text_by_part_id
                .get(&part_id)
                .map(String::as_str)
                .unwrap_or_default()
        );
        self.store_bounded_text(&part_id, &combined, source_start);
        self.flush_part(&part_id)
    }

    fn flush_full_text(&mut self, part_id: &str, text: &str) -> Vec<String> {
        let Some(message_id) = self.message_id_by_part_id.get(part_id) else {
            return Vec::new();
        };
        if self.is_text_part_by_id.get(part_id) != Some(&true)
            || self.message_role_by_id.get(message_id).map(String::as_str) != Some("assistant")
            || text.is_empty()
        {
            return Vec::new();
        }
        let emitted = self
            .emitted_character_count_by_part_id
            .get(part_id)
            .copied()
            .unwrap_or(0);
        let text_count = text.chars().count();
        if text_count <= emitted {
            return Vec::new();
        }
        self.emitted_character_count_by_part_id
            .insert(part_id.to_string(), text_count);
        vec![text.chars().skip(emitted).collect()]
    }

    fn flush_part(&mut self, part_id: &str) -> Vec<String> {
        let Some(message_id) = self.message_id_by_part_id.get(part_id) else {
            return Vec::new();
        };
        if self.is_text_part_by_id.get(part_id) != Some(&true)
            || self.message_role_by_id.get(message_id).map(String::as_str) != Some("assistant")
        {
            return Vec::new();
        }
        let Some(text) = self.text_by_part_id.get(part_id) else {
            return Vec::new();
        };
        if text.is_empty() {
            return Vec::new();
        }
        let emitted = self
            .emitted_character_count_by_part_id
            .get(part_id)
            .copied()
            .unwrap_or(0);
        let stored_start = self
            .stored_text_start_offset_by_part_id
            .get(part_id)
            .copied()
            .unwrap_or(0);
        let stored_end = stored_start + text.chars().count();
        if stored_end <= emitted {
            return Vec::new();
        }
        let relative_start = emitted.saturating_sub(stored_start);
        if relative_start >= text.chars().count() {
            return Vec::new();
        }
        self.emitted_character_count_by_part_id
            .insert(part_id.to_string(), stored_end);
        vec![text.chars().skip(relative_start).collect()]
    }

    fn source_character_count(&self, part_id: &str) -> usize {
        self.emitted_character_count_by_part_id
            .get(part_id)
            .copied()
            .unwrap_or(0)
            .max(
                self.stored_text_start_offset_by_part_id
                    .get(part_id)
                    .copied()
                    .unwrap_or(0)
                    + self
                        .text_by_part_id
                        .get(part_id)
                        .map(|text| text.chars().count())
                        .unwrap_or(0),
            )
    }

    fn store_bounded_text(&mut self, part_id: &str, text: &str, source_start: usize) {
        let count = text.chars().count();
        let dropped = count.saturating_sub(Self::MAX_TRACKED_PART_TEXT_CHARACTERS);
        let bounded = if dropped == 0 {
            text.to_string()
        } else {
            text.chars().skip(dropped).collect()
        };
        self.text_by_part_id.insert(part_id.to_string(), bounded);
        self.stored_text_start_offset_by_part_id
            .insert(part_id.to_string(), source_start + dropped);
    }

    fn remember_message_id(&mut self, message_id: &str) {
        if !self
            .message_id_order
            .iter()
            .any(|existing| existing == message_id)
        {
            self.message_id_order.push(message_id.to_string());
        }
        while self.message_id_order.len() > Self::MAX_TRACKED_MESSAGES {
            let removed = self.message_id_order[0].clone();
            self.prune_message(&removed);
        }
    }

    fn prune_message(&mut self, message_id: &str) {
        self.message_role_by_id.remove(message_id);
        self.message_id_order
            .retain(|candidate| candidate != message_id);
        let part_ids = self
            .message_id_by_part_id
            .iter()
            .filter(|(_, candidate)| candidate.as_str() == message_id)
            .map(|(part_id, _)| part_id.clone())
            .collect::<Vec<_>>();
        for part_id in part_ids {
            self.prune_part(&part_id);
        }
    }

    fn prune_part(&mut self, part_id: &str) {
        self.message_id_by_part_id.remove(part_id);
        self.is_text_part_by_id.remove(part_id);
        self.text_by_part_id.remove(part_id);
        self.stored_text_start_offset_by_part_id.remove(part_id);
        self.emitted_character_count_by_part_id.remove(part_id);
    }
}

fn opencode_event_session_id(properties: &serde_json::Map<String, Value>) -> Option<String> {
    opencode_first_string([
        properties.get("sessionID"),
        properties.get("sessionId"),
        properties.get("session_id"),
        opencode_nested_value(properties, "info", "sessionID"),
        opencode_nested_value(properties, "info", "sessionId"),
        opencode_nested_value(properties, "info", "session_id"),
        opencode_nested_value(properties, "message", "sessionID"),
        opencode_nested_value(properties, "message", "sessionId"),
        opencode_nested_value(properties, "message", "session_id"),
        opencode_nested_value(properties, "part", "sessionID"),
        opencode_nested_value(properties, "part", "sessionId"),
        opencode_nested_value(properties, "part", "session_id"),
    ])
}

fn opencode_nested_value<'a>(
    properties: &'a serde_json::Map<String, Value>,
    key: &str,
    nested_key: &str,
) -> Option<&'a Value> {
    properties
        .get(key)
        .and_then(Value::as_object)
        .and_then(|nested| nested.get(nested_key))
}

fn opencode_first_string<'a>(
    values: impl IntoIterator<Item = Option<&'a Value>>,
) -> Option<String> {
    values.into_iter().flatten().find_map(|value| {
        value.as_str().and_then(|value| {
            let trimmed = value.trim();
            (!trimmed.is_empty()).then(|| trimmed.to_string())
        })
    })
}

fn opencode_first_content_string<'a>(
    values: impl IntoIterator<Item = Option<&'a Value>>,
) -> Option<String> {
    let mut empty = None;
    for value in values.into_iter().flatten() {
        let Some(value) = value.as_str() else {
            continue;
        };
        if !value.is_empty() {
            return Some(value.to_string());
        }
        empty.get_or_insert_with(String::new);
    }
    empty
}

fn opencode_status_is_idle(value: Option<&Value>) -> bool {
    match value {
        Some(Value::String(status)) => status.trim() == "idle",
        Some(Value::Object(status)) => {
            opencode_first_string([
                status.get("type"),
                status.get("status"),
                status.get("state"),
            ])
            .as_deref()
                == Some("idle")
        }
        _ => false,
    }
}

fn opencode_message_has_completed_time(info: &serde_json::Map<String, Value>) -> bool {
    info.get("time")
        .and_then(Value::as_object)
        .is_some_and(|time| {
            ["completed", "completedAt", "end", "ended"]
                .into_iter()
                .any(|key| time.contains_key(key))
        })
}

fn send_sigint(process_id: u32) -> Result<()> {
    const SIGINT: i32 = 2;
    unsafe extern "C" {
        fn kill(process_id: i32, signal: i32) -> i32;
    }
    let result = unsafe { kill(process_id as i32, SIGINT) };
    if result == 0 {
        Ok(())
    } else {
        Err(std::io::Error::last_os_error()).context("failed to interrupt Claude Code stream")
    }
}

fn turn_start_request(
    state: &mut CodexProtocolState,
    thread_id: &str,
    text: &str,
    permission_mode: &str,
) -> Value {
    let mut params = json!({
        "threadId": thread_id,
        "input": [{"type": "text", "text": text, "text_elements": []}]
    });
    let params_object = params.as_object_mut().expect("turn params object");
    match permission_mode {
        "full-access" => {
            params_object.insert("approvalPolicy".to_string(), json!("never"));
            params_object.insert("approvalsReviewer".to_string(), json!("user"));
            params_object.insert(
                "sandboxPolicy".to_string(),
                json!({"type": "dangerFullAccess"}),
            );
        }
        "auto-review" => {
            params_object.insert("approvalPolicy".to_string(), json!("on-request"));
            params_object.insert("approvalsReviewer".to_string(), json!("auto_review"));
            params_object.insert("sandboxPolicy".to_string(), Value::Null);
        }
        "custom" => {}
        _ => {
            params_object.insert("approvalPolicy".to_string(), json!("never"));
            params_object.insert("approvalsReviewer".to_string(), Value::Null);
            params_object.insert("sandboxPolicy".to_string(), Value::Null);
        }
    }
    state.active_permission_mode = permission_mode.to_string();
    state.turn_in_flight = true;
    let (id, request) = state.request("turn/start", params);
    state.turn_start_request_ids.insert(id);
    request
}

fn process_codex_line(
    line: &str,
    state: &Arc<Mutex<CodexProtocolState>>,
    writer: &Arc<Mutex<ChildStdin>>,
) {
    let value = match serde_json::from_str::<Value>(line) {
        Ok(value) => value,
        Err(_) => {
            if let Ok(mut state) = state.lock() {
                state.fail("Codex app-server response was not valid JSON");
            }
            return;
        }
    };
    let mut outgoing = Vec::new();
    if let Ok(mut state) = state.lock() {
        let method = value.get("method").and_then(Value::as_str);
        if let (Some(method), Some(id)) = (method, value.get("id")) {
            outgoing.push(server_request_response(&state, id.clone(), method, &value));
        } else if let Some(method) = method {
            handle_notification(&mut state, method, value.get("params"));
            if method == "thread/started" {
                if let (Some(thread_id), Some((text, permission_mode))) =
                    (state.thread_id.clone(), state.queued_input.take())
                {
                    outgoing.push(turn_start_request(
                        &mut state,
                        &thread_id,
                        &text,
                        &permission_mode,
                    ));
                }
            }
        } else if let Some(id) = request_id(value.get("id")) {
            if let Some(error) = value.get("error") {
                let message = error
                    .get("message")
                    .and_then(Value::as_str)
                    .unwrap_or("Codex app-server request failed");
                if state.initialize_request_id == Some(id)
                    || state.thread_start_request_id == Some(id)
                {
                    state.fail(message);
                } else {
                    state.turn_start_request_ids.remove(&id);
                    state.turn_in_flight = false;
                    state.append_transcript(&format!("\nCodex request failed: {message}\n"));
                }
            } else if state.initialize_request_id == Some(id) {
                state.initialize_request_id = None;
                outgoing.push(json!({"method": "initialized"}));
                let mut params = json!({"serviceName": "cmux", "threadSource": "user"});
                if let Some(directory) = state.working_directory.as_deref() {
                    params["cwd"] = json!(directory);
                }
                let (id, request) = state.request("thread/start", params);
                state.thread_start_request_id = Some(id);
                outgoing.push(request);
            } else if state.thread_start_request_id == Some(id) {
                let thread_id = value
                    .pointer("/result/thread/id")
                    .and_then(Value::as_str)
                    .map(ToString::to_string);
                if let Some(thread_id) = thread_id {
                    state.thread_id = Some(thread_id.clone());
                    state.thread_start_request_id = None;
                    state.ready = true;
                    if let Some((text, permission_mode)) = state.queued_input.take() {
                        outgoing.push(turn_start_request(
                            &mut state,
                            &thread_id,
                            &text,
                            &permission_mode,
                        ));
                    }
                } else {
                    state.fail("Codex thread/start response did not include a thread id");
                }
            } else {
                state.turn_start_request_ids.remove(&id);
            }
        }
    }
    for message in outgoing {
        if let Err(err) = write_json_line(writer, &message) {
            if let Ok(mut state) = state.lock() {
                state.fail(err.to_string());
            }
            break;
        }
    }
}

fn handle_notification(state: &mut CodexProtocolState, method: &str, params: Option<&Value>) {
    match method {
        "thread/started" => {
            if state.thread_id.is_none() {
                state.thread_id = params
                    .and_then(|params| params.pointer("/thread/id"))
                    .and_then(Value::as_str)
                    .map(ToString::to_string);
                state.ready = state.thread_id.is_some();
            }
        }
        "item/agentMessage/delta" => {
            if let Some(delta) = params
                .and_then(|params| params.get("delta"))
                .and_then(Value::as_str)
            {
                state.append_transcript(delta);
            }
        }
        "item/agentMessage/completed"
        | "item/agentMessage/complete"
        | "item/agentMessage/finished"
        | "turn/completed"
        | "turn/complete"
        | "turn/finished"
        | "turn/end"
        | "turn/ended"
        | "turn/stopped"
        | "turn/failed"
        | "turn/canceled"
        | "turn/cancelled" => state.complete_turn(),
        "item/started" | "item/completed" => {
            if let Some(item) = params.and_then(|params| params.get("item")) {
                if item
                    .get("type")
                    .and_then(Value::as_str)
                    .is_some_and(|kind| {
                        matches!(kind, "agentMessage" | "assistantMessage" | "message")
                    })
                {
                    if method == "item/completed" {
                        state.complete_turn();
                    }
                } else if let Some(activity) = codex_activity(item, method == "item/completed") {
                    let action = activity
                        .get("action")
                        .and_then(Value::as_str)
                        .unwrap_or("Activity");
                    let detail = activity
                        .get("detail")
                        .and_then(Value::as_str)
                        .unwrap_or_default();
                    state.append_transcript(&format!(
                        "\n{action}{}\n",
                        if detail.is_empty() {
                            String::new()
                        } else {
                            format!(": {detail}")
                        }
                    ));
                    state.activities.push(activity);
                    if state.activities.len() > 100 {
                        state.activities.drain(..state.activities.len() - 100);
                    }
                }
            }
        }
        "item/commandExecution/outputDelta" => {
            if let Some(delta) = params
                .and_then(|params| params.get("delta"))
                .and_then(Value::as_str)
            {
                state.append_transcript(delta);
            }
        }
        "error" => {
            let message = params
                .and_then(|params| params.pointer("/error/message"))
                .and_then(Value::as_str)
                .unwrap_or("Codex app-server reported an error");
            state.fail(message);
        }
        "warning" | "guardianWarning" | "configWarning" | "deprecationNotice" => {
            let message = params
                .and_then(|params| params.get("message"))
                .and_then(Value::as_str)
                .unwrap_or("Codex app-server warning");
            state.append_transcript(&format!("\n{message}\n"));
        }
        _ => {}
    }
}

fn codex_activity(item: &Value, completed: bool) -> Option<Value> {
    let id = item.get("id")?.as_str()?;
    let kind = item.get("type")?.as_str()?;
    let status = if completed { "completed" } else { "inProgress" };
    match kind {
        "commandExecution" => Some(json!({
            "activity_id": id,
            "kind": "command",
            "status": status,
            "action": if completed { "Ran" } else { "Running" },
            "detail": command_detail(item)
        })),
        "fileChange" => Some(json!({
            "activity_id": id,
            "kind": "fileChange",
            "status": status,
            "action": if completed { "Edited" } else { "Editing" },
            "detail": file_change_path(item.get("changes"))
        })),
        _ => None,
    }
}

fn command_detail(item: &Value) -> Option<String> {
    ["command", "cmd", "commandText", "name"]
        .into_iter()
        .find_map(|key| item.get(key).and_then(Value::as_str))
        .map(ToString::to_string)
}

fn file_change_path(changes: Option<&Value>) -> Option<String> {
    match changes? {
        Value::Object(changes) => changes.keys().next().cloned(),
        Value::Array(changes) => changes.first().and_then(|change| {
            ["path", "filePath", "name"]
                .into_iter()
                .find_map(|key| change.get(key).and_then(Value::as_str))
                .map(ToString::to_string)
        }),
        _ => None,
    }
}

fn server_request_response(
    state: &CodexProtocolState,
    id: Value,
    method: &str,
    request: &Value,
) -> Value {
    let full_access = state.active_permission_mode == "full-access";
    let result = match method {
        "item/commandExecution/requestApproval" | "item/fileChange/requestApproval" => {
            json!({"decision": if full_access { "acceptForSession" } else { "decline" }})
        }
        "item/permissions/requestApproval" => json!({
            "permissions": if full_access {
                request.pointer("/params/permissions").cloned().unwrap_or_else(|| json!({}))
            } else {
                json!({})
            },
            "scope": "turn"
        }),
        "execCommandApproval" | "applyPatchApproval" => json!({
            "decision": if full_access { "approved_for_session" } else { "denied" }
        }),
        _ => {
            return json!({
                "id": id,
                "error": {"code": -32601, "message": format!("Unsupported Codex server request: {method}")}
            });
        }
    };
    json!({"id": id, "result": result})
}

fn request_id(value: Option<&Value>) -> Option<u64> {
    value.and_then(|value| {
        value
            .as_u64()
            .or_else(|| value.as_str().and_then(|value| value.parse().ok()))
    })
}

fn write_json_line(writer: &Arc<Mutex<ChildStdin>>, value: &Value) -> Result<()> {
    let mut writer = writer
        .lock()
        .map_err(|_| anyhow!("agent session writer lock poisoned"))?;
    serde_json::to_writer(&mut *writer, value).context("failed to encode agent session request")?;
    writer
        .write_all(b"\n")
        .context("failed to write agent session request")?;
    writer
        .flush()
        .context("failed to flush agent session request")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn claude_stream_accumulator_emits_deltas_without_duplicate_final_text() {
        let mut accumulator = ClaudeStreamJsonAccumulator::default();
        let started = accumulator.consume_line(
            r#"{"type":"message_start","message":{"id":"message-1","role":"assistant"}}"#,
        );
        assert!(started.output.is_empty());
        assert!(!started.completed_turn);

        let first = accumulator.consume_line(
            r#"{"type":"content_block_delta","delta":{"type":"text_delta","text":"Hello "}}"#,
        );
        assert_eq!(first.output, vec!["Hello "]);

        let second = accumulator.consume_line(
            r#"{"type":"assistant","message":{"id":"message-1","role":"assistant","content":[{"type":"text","text":"Hello world"}]}}"#,
        );
        assert_eq!(second.output, vec!["world"]);

        let completed = accumulator.consume_line(r#"{"type":"result","result":"Hello world"}"#);
        assert!(completed.output.is_empty());
        assert!(completed.completed_turn);
    }

    #[test]
    fn claude_stream_accumulator_uses_result_when_no_assistant_text_arrives() {
        let mut accumulator = ClaudeStreamJsonAccumulator::default();
        let completed =
            accumulator.consume_line(r#"{"type":"result","result":"Fallback response"}"#);
        assert_eq!(completed.output, vec!["Fallback response"]);
        assert!(completed.completed_turn);
        let next_turn = accumulator.consume_line(r#"{"type":"result","result":"Second fallback"}"#);
        assert_eq!(next_turn.output, vec!["Second fallback"]);
        assert!(next_turn.completed_turn);
    }

    #[test]
    fn claude_stream_accumulator_ignores_tool_blocks_and_completes_done() {
        let mut accumulator = ClaudeStreamJsonAccumulator::default();
        let tool = accumulator.consume_line(
            r#"{"type":"assistant","message":{"id":"message-2","content":[{"type":"tool_use","text":"hidden"},{"type":"text","text":"Visible"}]}}"#,
        );
        assert_eq!(tool.output, vec!["Visible"]);
        let done = accumulator.consume_line(r#"{"type":"done"}"#);
        assert!(done.completed_turn);
    }

    #[test]
    fn opencode_event_stream_parser_decodes_and_bounds_data_events() {
        let mut parser = OpenCodeEventStreamParser::default();
        assert!(parser.consume_line("event: message").is_empty());
        assert!(parser
            .consume_line(r#"data: {"type":"server.connected","properties":{}}"#)
            .is_empty());
        let events = parser.consume_line("");
        assert_eq!(events.len(), 1);
        assert_eq!(events[0]["type"], "server.connected");

        assert!(parser
            .consume_line(&format!(
                "data: {}",
                "a".repeat(OpenCodeEventStreamParser::MAX_EVENT_DATA_BYTES + 1)
            ))
            .is_empty());
        assert!(parser.consume_line("").is_empty());
    }

    #[test]
    fn opencode_text_accumulator_orders_role_part_and_delta_events() {
        let mut accumulator = OpenCodeEventTextAccumulator::default();
        let early_delta = json!({
            "type": "message.part.delta",
            "properties": {
                "sessionID": "session-1",
                "messageID": "message-1",
                "partID": "part-1",
                "field": "text",
                "delta": "hel"
            }
        });
        assert!(accumulator
            .consume_event(&early_delta, "session-1")
            .is_empty());
        let part = json!({
            "type": "message.part.updated",
            "properties": {
                "part": {
                    "id": "part-1",
                    "sessionID": "session-1",
                    "messageID": "message-1",
                    "type": "text",
                    "text": "hel"
                }
            }
        });
        assert!(accumulator.consume_event(&part, "session-1").is_empty());
        let role = json!({
            "type": "message.updated",
            "properties": {
                "info": {
                    "id": "message-1",
                    "sessionID": "session-1",
                    "role": "assistant"
                }
            }
        });
        assert_eq!(accumulator.consume_event(&role, "session-1"), vec!["hel"]);
        let next_delta = json!({
            "type": "message.part.delta",
            "properties": {
                "sessionID": "session-1",
                "messageID": "message-1",
                "partID": "part-1",
                "field": "text",
                "delta": "lo"
            }
        });
        assert_eq!(
            accumulator.consume_event(&next_delta, "session-1"),
            vec!["lo"]
        );
        assert!(accumulator
            .consume_event(&next_delta, "other-session")
            .is_empty());
    }

    #[test]
    fn opencode_completion_accepts_idle_and_completed_assistant_messages() {
        assert!(OpenCodeEventTextAccumulator::completes_assistant_turn(
            &json!({
                "type": "session.status",
                "properties": {
                    "sessionID": "session-1",
                    "status": {"type": "idle"}
                }
            }),
            "session-1"
        ));
        assert!(OpenCodeEventTextAccumulator::completes_assistant_turn(
            &json!({
                "type": "message.updated",
                "properties": {
                    "sessionID": "session-1",
                    "info": {
                        "id": "message-1",
                        "role": "assistant",
                        "time": {"completed": 2}
                    }
                }
            }),
            "session-1"
        ));
    }
}
