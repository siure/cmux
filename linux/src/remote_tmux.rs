use anyhow::{anyhow, Context, Result};
use std::collections::VecDeque;
use std::io::{Read, Write};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::{Arc, Mutex};
use std::thread;

const ENTER_SEQUENCE: &[u8] = b"\x1bP1000p";
const OUTPUT_PREFIX: &[u8] = b"%output ";
const DEFAULT_MAX_LINE_BYTES: usize = 1024 * 1024;
const DEFAULT_MAX_BLOCK_BYTES: usize = 16 * 1024 * 1024;
const MAX_QUEUED_EVENT_BYTES: usize = 16 * 1024 * 1024;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum ControlMessage {
    Enter,
    Exit(Option<String>),
    Output {
        pane_id: u64,
        data: Vec<u8>,
    },
    SessionChanged {
        session_id: u64,
        name: String,
    },
    SessionsChanged,
    WindowAdd {
        window_id: u64,
    },
    WindowClose {
        window_id: u64,
    },
    WindowRenamed {
        window_id: u64,
        name: String,
    },
    LayoutChange {
        window_id: u64,
        layout: String,
    },
    WindowPaneChanged {
        window_id: u64,
        pane_id: u64,
    },
    SessionWindowChanged {
        session_id: u64,
        window_id: u64,
    },
    SubscriptionChanged {
        name: String,
        value: String,
    },
    CommandResult {
        command_number: u64,
        lines: Vec<String>,
        is_error: bool,
    },
    StreamError(String),
    IgnoredNotification(String),
    Unparsed(String),
}

impl ControlMessage {
    fn estimated_bytes(&self) -> usize {
        match self {
            Self::Output { data, .. } => data.len(),
            Self::CommandResult { lines, .. } => lines.iter().map(String::len).sum(),
            Self::Exit(Some(value))
            | Self::StreamError(value)
            | Self::IgnoredNotification(value)
            | Self::Unparsed(value) => value.len(),
            Self::SessionChanged { name, .. }
            | Self::WindowRenamed { name, .. }
            | Self::LayoutChange { layout: name, .. } => name.len(),
            Self::SubscriptionChanged { name, value } => name.len() + value.len(),
            _ => 32,
        }
    }
}

pub(crate) struct ControlStreamParser {
    max_line_bytes: usize,
    max_block_bytes: usize,
    buffer: Vec<u8>,
    block_number: Option<u64>,
    block_lines: Vec<String>,
    block_bytes: usize,
}

impl Default for ControlStreamParser {
    fn default() -> Self {
        Self::new(DEFAULT_MAX_LINE_BYTES, DEFAULT_MAX_BLOCK_BYTES)
    }
}

impl ControlStreamParser {
    pub(crate) fn new(max_line_bytes: usize, max_block_bytes: usize) -> Self {
        Self {
            max_line_bytes: max_line_bytes.max(1),
            max_block_bytes: max_block_bytes.max(1),
            buffer: Vec::new(),
            block_number: None,
            block_lines: Vec::new(),
            block_bytes: 0,
        }
    }

    pub(crate) fn feed(&mut self, bytes: &[u8]) -> Vec<ControlMessage> {
        let mut messages = Vec::new();
        for &byte in bytes {
            if byte == b'\n' {
                let mut line = std::mem::take(&mut self.buffer);
                if line.last() == Some(&b'\r') {
                    line.pop();
                }
                for message in self.parse_line(line) {
                    let failed = matches!(message, ControlMessage::StreamError(_));
                    messages.push(message);
                    if failed {
                        return messages;
                    }
                }
            } else {
                self.buffer.push(byte);
                if self.buffer.len() > self.max_line_bytes {
                    messages
                        .push(self.fail(format!("line exceeded {} bytes", self.max_line_bytes)));
                    return messages;
                }
            }
        }
        messages
    }

    fn parse_line(&mut self, mut bytes: Vec<u8>) -> Vec<ControlMessage> {
        let mut messages = Vec::new();
        if bytes.starts_with(ENTER_SEQUENCE) {
            messages.push(ControlMessage::Enter);
            bytes.drain(..ENTER_SEQUENCE.len());
        }
        if self.block_number.is_none() {
            remove_string_terminators(&mut bytes);
        }
        if bytes.is_empty() {
            return messages;
        }

        if self.block_number.is_none() {
            if let Some(output) = parse_output(&bytes) {
                messages.push(output);
                return messages;
            }
        }

        let line = String::from_utf8_lossy(&bytes).into_owned();
        if let Some(block_number) = self.block_number {
            if (line.starts_with("%end ") || line.starts_with("%error "))
                && field_u64(&line, 2) == Some(block_number)
            {
                messages.push(ControlMessage::CommandResult {
                    command_number: block_number,
                    lines: std::mem::take(&mut self.block_lines),
                    is_error: line.starts_with("%error "),
                });
                self.block_number = None;
                self.block_bytes = 0;
                return messages;
            }
            if self.block_bytes + bytes.len() + 1 > self.max_block_bytes {
                messages.push(self.fail(format!(
                    "command block exceeded {} bytes",
                    self.max_block_bytes
                )));
                return messages;
            }
            self.block_bytes += bytes.len() + 1;
            self.block_lines.push(line);
            return messages;
        }

        if line.starts_with("%begin ") {
            if let Some(command_number) = field_u64(&line, 2) {
                self.block_number = Some(command_number);
                self.block_lines.clear();
                self.block_bytes = 0;
                return messages;
            }
        }
        messages.push(parse_notification(line));
        messages
    }

    fn fail(&mut self, reason: String) -> ControlMessage {
        self.buffer.clear();
        self.block_number = None;
        self.block_lines.clear();
        self.block_bytes = 0;
        ControlMessage::StreamError(reason)
    }
}

fn parse_output(bytes: &[u8]) -> Option<ControlMessage> {
    let mut index = OUTPUT_PREFIX.len();
    if !bytes.starts_with(OUTPUT_PREFIX) || bytes.get(index) != Some(&b'%') {
        return None;
    }
    index += 1;
    let digits_start = index;
    while bytes.get(index).is_some_and(u8::is_ascii_digit) {
        index += 1;
    }
    if digits_start == index || bytes.get(index) != Some(&b' ') {
        return None;
    }
    let pane_id = std::str::from_utf8(&bytes[digits_start..index])
        .ok()?
        .parse()
        .ok()?;
    Some(ControlMessage::Output {
        pane_id,
        data: unescape_output(&bytes[index + 1..]),
    })
}

fn parse_notification(line: String) -> ControlMessage {
    if line == "%exit" || line.starts_with("%exit ") {
        return ControlMessage::Exit(
            line.strip_prefix("%exit ")
                .map(ToString::to_string)
                .filter(|value| !value.is_empty()),
        );
    }
    if line.starts_with("%session-changed ") {
        return match field_id(&line, 1, '$') {
            Some(session_id) => ControlMessage::SessionChanged {
                session_id,
                name: fields_from(&line, 2),
            },
            None => ControlMessage::Unparsed(line),
        };
    }
    if line == "%sessions-changed" {
        return ControlMessage::SessionsChanged;
    }
    if line.starts_with("%window-add ") {
        return field_id(&line, 1, '@')
            .map(|window_id| ControlMessage::WindowAdd { window_id })
            .unwrap_or(ControlMessage::Unparsed(line));
    }
    if line.starts_with("%window-close ") || line.starts_with("%unlinked-window-close ") {
        return field_id(&line, 1, '@')
            .map(|window_id| ControlMessage::WindowClose { window_id })
            .unwrap_or(ControlMessage::Unparsed(line));
    }
    if line.starts_with("%window-renamed ") {
        return field_id(&line, 1, '@')
            .map(|window_id| ControlMessage::WindowRenamed {
                window_id,
                name: fields_from(&line, 2),
            })
            .unwrap_or(ControlMessage::Unparsed(line));
    }
    if line.starts_with("%layout-change ") {
        return match (field_id(&line, 1, '@'), field(&line, 2)) {
            (Some(window_id), Some(layout)) => ControlMessage::LayoutChange {
                window_id,
                layout: layout.to_string(),
            },
            _ => ControlMessage::Unparsed(line),
        };
    }
    if line.starts_with("%window-pane-changed ") {
        return match (field_id(&line, 1, '@'), field_id(&line, 2, '%')) {
            (Some(window_id), Some(pane_id)) => {
                ControlMessage::WindowPaneChanged { window_id, pane_id }
            }
            _ => ControlMessage::Unparsed(line),
        };
    }
    if line.starts_with("%session-window-changed ") {
        return match (field_id(&line, 1, '$'), field_id(&line, 2, '@')) {
            (Some(session_id), Some(window_id)) => ControlMessage::SessionWindowChanged {
                session_id,
                window_id,
            },
            _ => ControlMessage::Unparsed(line),
        };
    }
    if line.starts_with("%subscription-changed ") {
        return match field(&line, 1) {
            Some(name) => ControlMessage::SubscriptionChanged {
                name: name.to_string(),
                value: line
                    .split_once(" : ")
                    .map(|(_, value)| value.to_string())
                    .unwrap_or_default(),
            },
            None => ControlMessage::IgnoredNotification(line),
        };
    }
    if line.starts_with('%') {
        ControlMessage::IgnoredNotification(line)
    } else {
        ControlMessage::Unparsed(line)
    }
}

fn field(line: &str, index: usize) -> Option<&str> {
    line.split(' ').nth(index)
}

fn fields_from(line: &str, index: usize) -> String {
    line.split(' ').skip(index).collect::<Vec<_>>().join(" ")
}

fn field_u64(line: &str, index: usize) -> Option<u64> {
    field(line, index)?.parse().ok()
}

fn field_id(line: &str, index: usize, sigil: char) -> Option<u64> {
    field(line, index)?.strip_prefix(sigil)?.parse::<u64>().ok()
}

fn remove_string_terminators(bytes: &mut Vec<u8>) {
    let mut read = 0;
    let mut write = 0;
    while read < bytes.len() {
        if bytes[read] == 0x1b && bytes.get(read + 1) == Some(&b'\\') {
            read += 2;
            continue;
        }
        bytes[write] = bytes[read];
        write += 1;
        read += 1;
    }
    bytes.truncate(write);
}

pub(crate) fn unescape_output(bytes: &[u8]) -> Vec<u8> {
    let mut output = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'\\'
            && index + 3 < bytes.len()
            && bytes[index + 1..=index + 3]
                .iter()
                .all(|byte| matches!(byte, b'0'..=b'7'))
        {
            let value = usize::from(bytes[index + 1] - b'0') * 64
                + usize::from(bytes[index + 2] - b'0') * 8
                + usize::from(bytes[index + 3] - b'0');
            if value <= u8::MAX as usize {
                output.push(value as u8);
                index += 4;
                continue;
            }
        }
        output.push(bytes[index]);
        index += 1;
    }
    output
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum RuntimeEvent {
    Message(ControlMessage),
    Stderr(String),
    StdoutClosed,
    StderrClosed,
    QueueOverflow,
}

struct EventQueue {
    events: VecDeque<RuntimeEvent>,
    estimated_bytes: usize,
    overflowed: bool,
}

impl EventQueue {
    fn push(&mut self, event: RuntimeEvent) {
        let bytes = match &event {
            RuntimeEvent::Message(message) => message.estimated_bytes(),
            RuntimeEvent::Stderr(stderr) => stderr.len(),
            _ => 32,
        };
        if self.estimated_bytes.saturating_add(bytes) > MAX_QUEUED_EVENT_BYTES {
            if !self.overflowed {
                self.events.clear();
                self.estimated_bytes = 0;
                self.events.push_back(RuntimeEvent::QueueOverflow);
                self.overflowed = true;
            }
            return;
        }
        self.estimated_bytes += bytes;
        self.events.push_back(event);
    }

    fn drain(&mut self) -> Vec<RuntimeEvent> {
        self.estimated_bytes = 0;
        self.overflowed = false;
        self.events.drain(..).collect()
    }
}

pub(crate) struct RemoteTmuxRuntime {
    child: Mutex<Child>,
    stdin: Mutex<ChildStdin>,
    events: Arc<Mutex<EventQueue>>,
}

impl RemoteTmuxRuntime {
    pub(crate) fn spawn(mut command: Command) -> Result<Self> {
        let mut child = command
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .context("failed to start remote tmux control process")?;
        let stdin = child
            .stdin
            .take()
            .ok_or_else(|| anyhow!("remote tmux stdin was unavailable"))?;
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| anyhow!("remote tmux stdout was unavailable"))?;
        let stderr = child
            .stderr
            .take()
            .ok_or_else(|| anyhow!("remote tmux stderr was unavailable"))?;
        let events = Arc::new(Mutex::new(EventQueue {
            events: VecDeque::new(),
            estimated_bytes: 0,
            overflowed: false,
        }));
        spawn_stdout_reader(stdout, Arc::clone(&events));
        spawn_stderr_reader(stderr, Arc::clone(&events));
        Ok(Self {
            child: Mutex::new(child),
            stdin: Mutex::new(stdin),
            events,
        })
    }

    pub(crate) fn send_command(&self, command: &str) -> Result<()> {
        if command
            .bytes()
            .any(|byte| matches!(byte, b'\r' | b'\n' | 0))
        {
            return Err(anyhow!("remote tmux command contains a line terminator"));
        }
        let mut stdin = self
            .stdin
            .lock()
            .map_err(|_| anyhow!("remote tmux stdin lock poisoned"))?;
        stdin
            .write_all(command.as_bytes())
            .and_then(|_| stdin.write_all(b"\n"))
            .and_then(|_| stdin.flush())
            .context("failed to write remote tmux command")
    }

    pub(crate) fn send_keys(&self, pane_id: u64, bytes: &[u8]) -> Result<()> {
        if bytes.is_empty() {
            return Ok(());
        }
        let mut command = format!("send-keys -t %{pane_id} -H");
        for byte in bytes {
            use std::fmt::Write as _;
            write!(&mut command, " {byte:02x}").expect("write to String");
        }
        self.send_command(&command)
    }

    pub(crate) fn drain_events(&self) -> Vec<RuntimeEvent> {
        self.events
            .lock()
            .map(|mut queue| queue.drain())
            .unwrap_or_else(|_| {
                vec![RuntimeEvent::Message(ControlMessage::StreamError(
                    "remote tmux event queue lock poisoned".to_string(),
                ))]
            })
    }

    pub(crate) fn try_wait(&self) -> Result<Option<i32>> {
        self.child
            .lock()
            .map_err(|_| anyhow!("remote tmux child lock poisoned"))?
            .try_wait()
            .map(|status| status.map(|status| status.code().unwrap_or(-1)))
            .context("failed to poll remote tmux control process")
    }

    pub(crate) fn stop(&self) {
        if let Ok(mut child) = self.child.lock() {
            if child.try_wait().ok().flatten().is_none() {
                let _ = child.kill();
                let _ = child.wait();
            }
        }
    }
}

impl Drop for RemoteTmuxRuntime {
    fn drop(&mut self) {
        self.stop();
    }
}

fn spawn_stdout_reader(mut stdout: impl Read + Send + 'static, events: Arc<Mutex<EventQueue>>) {
    thread::spawn(move || {
        let mut parser = ControlStreamParser::default();
        let mut buffer = [0_u8; 8192];
        loop {
            match stdout.read(&mut buffer) {
                Ok(0) => {
                    if let Ok(mut queue) = events.lock() {
                        queue.push(RuntimeEvent::StdoutClosed);
                    }
                    return;
                }
                Ok(count) => {
                    let messages = parser.feed(&buffer[..count]);
                    if let Ok(mut queue) = events.lock() {
                        for message in messages {
                            queue.push(RuntimeEvent::Message(message));
                        }
                    } else {
                        return;
                    }
                }
                Err(error) => {
                    if let Ok(mut queue) = events.lock() {
                        queue.push(RuntimeEvent::Message(ControlMessage::StreamError(format!(
                            "failed to read remote tmux stdout: {error}"
                        ))));
                    }
                    return;
                }
            }
        }
    });
}

fn spawn_stderr_reader(mut stderr: impl Read + Send + 'static, events: Arc<Mutex<EventQueue>>) {
    thread::spawn(move || {
        let mut buffer = [0_u8; 4096];
        loop {
            match stderr.read(&mut buffer) {
                Ok(0) => {
                    if let Ok(mut queue) = events.lock() {
                        queue.push(RuntimeEvent::StderrClosed);
                    }
                    return;
                }
                Ok(count) => {
                    if let Ok(mut queue) = events.lock() {
                        queue.push(RuntimeEvent::Stderr(
                            String::from_utf8_lossy(&buffer[..count]).into_owned(),
                        ));
                    } else {
                        return;
                    }
                }
                Err(error) => {
                    if let Ok(mut queue) = events.lock() {
                        queue.push(RuntimeEvent::Stderr(format!(
                            "failed to read remote tmux stderr: {error}"
                        )));
                    }
                    return;
                }
            }
        }
    });
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum LayoutContent {
    Pane(u64),
    Horizontal(Vec<LayoutNode>),
    Vertical(Vec<LayoutNode>),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct LayoutNode {
    pub width: u32,
    pub height: u32,
    pub x: u32,
    pub y: u32,
    pub content: LayoutContent,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct LayoutPane {
    pub pane_id: u64,
    pub width: u32,
    pub height: u32,
    pub x: u32,
    pub y: u32,
}

impl LayoutNode {
    pub(crate) fn pane_ids(&self) -> Vec<u64> {
        match &self.content {
            LayoutContent::Pane(pane_id) => vec![*pane_id],
            LayoutContent::Horizontal(children) | LayoutContent::Vertical(children) => {
                children.iter().flat_map(Self::pane_ids).collect()
            }
        }
    }

    pub(crate) fn panes(&self) -> Vec<LayoutPane> {
        let mut panes = Vec::new();
        self.collect_panes(&mut panes);
        panes
    }

    fn collect_panes(&self, panes: &mut Vec<LayoutPane>) {
        match &self.content {
            LayoutContent::Pane(pane_id) => panes.push(LayoutPane {
                pane_id: *pane_id,
                width: self.width,
                height: self.height,
                x: self.x,
                y: self.y,
            }),
            LayoutContent::Horizontal(children) | LayoutContent::Vertical(children) => {
                for child in children {
                    child.collect_panes(panes);
                }
            }
        }
    }
}

pub(crate) fn parse_layout(raw: &str) -> Option<LayoutNode> {
    let mut text = raw.trim();
    if text.len() > 5
        && text.as_bytes().get(4) == Some(&b',')
        && text[..4].bytes().all(|byte| byte.is_ascii_hexdigit())
    {
        text = &text[5..];
    }
    let bytes = text.as_bytes();
    let mut index = 0;
    let node = parse_layout_node(bytes, &mut index)?;
    (index == bytes.len()).then_some(node)
}

fn parse_layout_node(bytes: &[u8], index: &mut usize) -> Option<LayoutNode> {
    let width = parse_layout_u32(bytes, index)?;
    consume_layout(bytes, index, b'x')?;
    let height = parse_layout_u32(bytes, index)?;
    consume_layout(bytes, index, b',')?;
    let x = parse_layout_u32(bytes, index)?;
    consume_layout(bytes, index, b',')?;
    let y = parse_layout_u32(bytes, index)?;
    let content = match bytes.get(*index)? {
        b',' => {
            *index += 1;
            LayoutContent::Pane(u64::from(parse_layout_u32(bytes, index)?))
        }
        b'{' => LayoutContent::Horizontal(parse_layout_children(bytes, index, b'{', b'}')?),
        b'[' => LayoutContent::Vertical(parse_layout_children(bytes, index, b'[', b']')?),
        _ => return None,
    };
    Some(LayoutNode {
        width,
        height,
        x,
        y,
        content,
    })
}

fn parse_layout_children(
    bytes: &[u8],
    index: &mut usize,
    open: u8,
    close: u8,
) -> Option<Vec<LayoutNode>> {
    consume_layout(bytes, index, open)?;
    let mut children = Vec::new();
    loop {
        children.push(parse_layout_node(bytes, index)?);
        match bytes.get(*index)? {
            byte if *byte == close => {
                *index += 1;
                break;
            }
            b',' => *index += 1,
            _ => return None,
        }
    }
    (children.len() >= 2).then_some(children)
}

fn parse_layout_u32(bytes: &[u8], index: &mut usize) -> Option<u32> {
    let start = *index;
    while bytes.get(*index).is_some_and(u8::is_ascii_digit) {
        *index += 1;
    }
    (start < *index)
        .then(|| {
            std::str::from_utf8(&bytes[start..*index])
                .ok()?
                .parse()
                .ok()
        })
        .flatten()
}

fn consume_layout(bytes: &[u8], index: &mut usize, expected: u8) -> Option<()> {
    (bytes.get(*index) == Some(&expected)).then(|| *index += 1)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{Duration, Instant};

    #[test]
    fn parser_preserves_split_utf8_and_octal_output() {
        let mut parser = ControlStreamParser::default();
        let mut first = b"\x1bP1000p%begin 1 7 0\r\n%end 1 7 0\r\n%output %4 ".to_vec();
        first.extend_from_slice(&[0xe2]);
        first.push(b'\n');
        let messages = parser.feed(&first);
        assert!(messages.contains(&ControlMessage::Enter));
        assert!(messages.contains(&ControlMessage::CommandResult {
            command_number: 7,
            lines: Vec::new(),
            is_error: false,
        }));
        assert!(messages.contains(&ControlMessage::Output {
            pane_id: 4,
            data: vec![0xe2],
        }));
        assert_eq!(
            parser.feed(b"%output %4 \\200\\254 text\\015\\012\n"),
            vec![ControlMessage::Output {
                pane_id: 4,
                data: vec![0x80, 0xac, b' ', b't', b'e', b'x', b't', b'\r', b'\n'],
            }]
        );
    }

    #[test]
    fn parser_does_not_end_block_on_captured_end_line() {
        let mut parser = ControlStreamParser::default();
        let messages = parser.feed(b"%begin 9 42 0\ncaptured\n%end 1 41 0\n%end 9 42 0\n");
        assert_eq!(
            messages,
            vec![ControlMessage::CommandResult {
                command_number: 42,
                lines: vec!["captured".to_string(), "%end 1 41 0".to_string()],
                is_error: false,
            }]
        );
    }

    #[test]
    fn parser_bounds_lines_and_blocks() {
        let mut parser = ControlStreamParser::new(4, 8);
        assert!(matches!(
            parser.feed(b"12345"),
            ref messages if matches!(messages.as_slice(), [ControlMessage::StreamError(_)])
        ));
        let mut parser = ControlStreamParser::new(64, 4);
        assert!(matches!(
            parser.feed(b"%begin 1 2 0\n12345\n"),
            ref messages if matches!(messages.as_slice(), [ControlMessage::StreamError(_)])
        ));
    }

    #[test]
    fn parser_classifies_topology_notifications() {
        let mut parser = ControlStreamParser::default();
        assert_eq!(
            parser.feed(
                b"%session-changed $2 work space\n%window-add @4\n%window-renamed @4 editor tab\n%layout-change @4 abcd,80x24,0,0,7\n%window-pane-changed @4 %7\n%session-window-changed $2 @4\n"
            ),
            vec![
                ControlMessage::SessionChanged {
                    session_id: 2,
                    name: "work space".to_string(),
                },
                ControlMessage::WindowAdd { window_id: 4 },
                ControlMessage::WindowRenamed {
                    window_id: 4,
                    name: "editor tab".to_string(),
                },
                ControlMessage::LayoutChange {
                    window_id: 4,
                    layout: "abcd,80x24,0,0,7".to_string(),
                },
                ControlMessage::WindowPaneChanged {
                    window_id: 4,
                    pane_id: 7,
                },
                ControlMessage::SessionWindowChanged {
                    session_id: 2,
                    window_id: 4,
                },
            ]
        );
    }

    #[test]
    fn raw_layout_parser_builds_nested_tree() {
        let layout =
            parse_layout("f92f,120x40,0,0{60x40,0,0,4,59x40,61,0[59x20,61,0,5,59x19,61,21,8]}")
                .expect("layout");
        assert_eq!(layout.pane_ids(), vec![4, 5, 8]);
        assert_eq!(
            layout.panes(),
            vec![
                LayoutPane {
                    pane_id: 4,
                    width: 60,
                    height: 40,
                    x: 0,
                    y: 0,
                },
                LayoutPane {
                    pane_id: 5,
                    width: 59,
                    height: 20,
                    x: 61,
                    y: 0,
                },
                LayoutPane {
                    pane_id: 8,
                    width: 59,
                    height: 19,
                    x: 61,
                    y: 21,
                },
            ]
        );
        assert!(matches!(layout.content, LayoutContent::Horizontal(_)));
        assert!(parse_layout("80x24,0,0,1 trailing").is_none());
    }

    #[test]
    fn runtime_streams_messages_and_writes_commands() {
        let mut command = Command::new("/bin/sh");
        command.args([
            "-c",
            "printf '\\033P1000p%%begin 1 1 0\\n%%end 1 1 0\\n%%output %%3 hello\\\\015\\\\012\\n'; IFS= read -r line; printf 'seen:%s\\n' \"$line\" >&2",
        ]);
        let runtime = RemoteTmuxRuntime::spawn(command).expect("runtime");
        runtime.send_keys(3, b"A\n").expect("send keys");

        let deadline = Instant::now() + Duration::from_secs(2);
        let mut events = Vec::new();
        while Instant::now() < deadline {
            events.extend(runtime.drain_events());
            if runtime.try_wait().expect("wait").is_some() {
                events.extend(runtime.drain_events());
                break;
            }
            thread::sleep(Duration::from_millis(10));
        }
        assert!(events.contains(&RuntimeEvent::Message(ControlMessage::Enter)));
        assert!(
            events.contains(&RuntimeEvent::Message(ControlMessage::Output {
                pane_id: 3,
                data: b"hello\r\n".to_vec(),
            }))
        );
        assert!(events.iter().any(|event| {
            matches!(event, RuntimeEvent::Stderr(text) if text.contains("seen:send-keys -t %3 -H 41 0a"))
        }));
    }
}
