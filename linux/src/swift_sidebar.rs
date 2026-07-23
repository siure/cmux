use crate::custom_sidebar::{
    SidebarAction, SidebarActionCommand, SidebarBinding, SidebarDocument, SidebarEvent,
    SidebarNode, SidebarNodeKind, SidebarOption, SidebarReorder, SidebarState,
};
use serde::{Deserialize, Serialize};
use serde_json::{Number, Value};
use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::io::{Read, Write};
use std::process::{Command, Stdio};
use std::rc::Rc;
use std::thread;
use std::time::{Duration, Instant};
use tree_sitter::{Node, Parser};

const MAX_EXPRESSION_DEPTH: usize = 96;
const MAX_FUNCTION_CALLS: usize = 256;
const MAX_ITERATION_VALUES: usize = 4096;
const MAX_STATE_IDENTITIES: usize = 256;
const MAX_STATE_IDENTITY_DEPTH: usize = 32;
const INSTANCE_STATE_PREFIX: &str = "__cmux_instance_";
const ENUM_TYPE_KEY: &str = "__cmux_enum_type";
const ENUM_CASE_KEY: &str = "__cmux_enum_case";
const ENUM_VALUES_KEY: &str = "__cmux_enum_values";
const ENUM_LABELS_KEY: &str = "__cmux_enum_labels";
const ENUM_RAW_VALUE_KEY: &str = "__cmux_enum_raw_value";
const WORKER_PROTOCOL_VERSION: u32 = 3;
const MAX_WORKER_REQUEST_BYTES: usize = 8 * 1024 * 1024;
const MAX_WORKER_RESPONSE_BYTES: usize = 32 * 1024 * 1024;
const DEFAULT_WORKER_TIMEOUT: Duration = Duration::from_secs(2);

#[derive(Debug, Deserialize, Serialize)]
struct WorkerRequest {
    version: u32,
    source: String,
    context: Value,
    state: SidebarState,
    event: Option<SidebarEvaluationEvent>,
}

#[derive(Debug, Deserialize, Serialize)]
struct WorkerResponse {
    version: u32,
    document: Option<SidebarDocument>,
    state: Option<SidebarState>,
    error: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct SidebarEvaluationEvent {
    pub id: String,
    pub old_value: Value,
    pub new_value: Value,
}

#[derive(Clone, Debug, PartialEq)]
enum EvalValue {
    Null,
    Optional(Option<Box<EvalValue>>),
    Bool(bool),
    Int(i64),
    Double(f64),
    String(String),
    Array(Vec<EvalValue>),
    Object(HashMap<String, EvalValue>),
    EnumCase {
        type_name: String,
        case_name: String,
        values: Vec<EvalValue>,
        labels: Vec<Option<String>>,
        raw_value: Option<Box<EvalValue>>,
    },
    Range(i64, i64, bool),
    NumericRange(f64, f64, bool),
    Binding(String),
}

enum EvalFlow<T> {
    Normal(T),
    Return(T),
    Break(T),
    Continue(T),
}

impl<T> EvalFlow<T> {
    fn into_value(self) -> T {
        match self {
            Self::Normal(value)
            | Self::Return(value)
            | Self::Break(value)
            | Self::Continue(value) => value,
        }
    }
}

impl EvalValue {
    fn from_json(value: &Value) -> Self {
        match value {
            Value::Null => Self::Optional(None),
            Value::Bool(value) => Self::Bool(*value),
            Value::Number(value) => value
                .as_i64()
                .map(Self::Int)
                .or_else(|| value.as_f64().map(Self::Double))
                .unwrap_or(Self::Null),
            Value::String(value) => Self::String(value.clone()),
            Value::Array(values) => Self::Array(values.iter().map(Self::from_json).collect()),
            Value::Object(values) => {
                let enum_type = values.get(ENUM_TYPE_KEY).and_then(Value::as_str);
                let enum_case = values.get(ENUM_CASE_KEY).and_then(Value::as_str);
                if let (Some(type_name), Some(case_name)) = (enum_type, enum_case) {
                    let enum_values = values
                        .get(ENUM_VALUES_KEY)
                        .and_then(Value::as_array)
                        .map(|values| values.iter().map(Self::from_json).collect())
                        .unwrap_or_default();
                    let labels = values
                        .get(ENUM_LABELS_KEY)
                        .and_then(Value::as_array)
                        .map(|labels| {
                            labels
                                .iter()
                                .map(|label| label.as_str().map(str::to_string))
                                .collect()
                        })
                        .unwrap_or_default();
                    let raw_value = values
                        .get(ENUM_RAW_VALUE_KEY)
                        .filter(|value| !value.is_null())
                        .map(Self::from_json)
                        .map(Box::new);
                    Self::EnumCase {
                        type_name: type_name.to_string(),
                        case_name: case_name.to_string(),
                        values: enum_values,
                        labels,
                        raw_value,
                    }
                } else {
                    Self::Object(
                        values
                            .iter()
                            .map(|(key, value)| (key.clone(), Self::from_json(value)))
                            .collect(),
                    )
                }
            }
        }
    }

    fn display_string(&self) -> String {
        match self {
            Self::Null => String::new(),
            Self::Optional(value) => value
                .as_deref()
                .map(Self::display_string)
                .unwrap_or_default(),
            Self::Bool(value) => value.to_string(),
            Self::Int(value) => value.to_string(),
            Self::Double(value) => {
                if value.is_finite() && value.fract() == 0.0 {
                    format!("{value:.0}")
                } else {
                    value.to_string()
                }
            }
            Self::String(value) => value.clone(),
            Self::Array(values) => format!(
                "[{}]",
                values
                    .iter()
                    .map(Self::display_string)
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
            Self::Object(values) => {
                let mut keys = values.keys().collect::<Vec<_>>();
                keys.sort();
                format!(
                    "{{{}}}",
                    keys.into_iter()
                        .map(|key| format!("{key}: {}", values[key].display_string()))
                        .collect::<Vec<_>>()
                        .join(", ")
                )
            }
            Self::EnumCase {
                case_name,
                values,
                raw_value,
                ..
            } => raw_value
                .as_deref()
                .map(Self::display_string)
                .unwrap_or_else(|| {
                    if values.is_empty() {
                        case_name.clone()
                    } else {
                        format!(
                            "{case_name}({})",
                            values
                                .iter()
                                .map(Self::display_string)
                                .collect::<Vec<_>>()
                                .join(", ")
                        )
                    }
                }),
            Self::Range(lower, upper, inclusive) => {
                format!("{lower}{}{upper}", if *inclusive { "..." } else { "..<" })
            }
            Self::NumericRange(lower, upper, inclusive) => format!(
                "{}{}{}",
                display_number(*lower),
                if *inclusive { "..." } else { "..<" },
                display_number(*upper)
            ),
            Self::Binding(_) => String::new(),
        }
    }

    fn to_json(&self) -> Option<Value> {
        match self {
            Self::Null => Some(Value::Null),
            Self::Optional(value) => value
                .as_deref()
                .map(Self::to_json)
                .unwrap_or(Some(Value::Null)),
            Self::Bool(value) => Some(Value::Bool(*value)),
            Self::Int(value) => Some(Value::Number(Number::from(*value))),
            Self::Double(value) => Number::from_f64(*value).map(Value::Number),
            Self::String(value) => Some(Value::String(value.clone())),
            Self::Array(values) => values
                .iter()
                .map(Self::to_json)
                .collect::<Option<Vec<_>>>()
                .map(Value::Array),
            Self::Object(values) => values
                .iter()
                .map(|(key, value)| Some((key.clone(), value.to_json()?)))
                .collect::<Option<serde_json::Map<_, _>>>()
                .map(Value::Object),
            Self::EnumCase {
                type_name,
                case_name,
                values,
                labels,
                raw_value,
            } => {
                let values = values
                    .iter()
                    .map(Self::to_json)
                    .collect::<Option<Vec<_>>>()?;
                let labels = labels
                    .iter()
                    .map(|label| label.clone().map(Value::String).unwrap_or(Value::Null))
                    .collect();
                let raw_value = raw_value
                    .as_deref()
                    .and_then(Self::to_json)
                    .unwrap_or(Value::Null);
                Some(Value::Object(serde_json::Map::from_iter([
                    (ENUM_TYPE_KEY.to_string(), Value::String(type_name.clone())),
                    (ENUM_CASE_KEY.to_string(), Value::String(case_name.clone())),
                    (ENUM_VALUES_KEY.to_string(), Value::Array(values)),
                    (ENUM_LABELS_KEY.to_string(), Value::Array(labels)),
                    (ENUM_RAW_VALUE_KEY.to_string(), raw_value),
                ])))
            }
            Self::Range(_, _, _) | Self::NumericRange(_, _, _) | Self::Binding(_) => None,
        }
    }

    fn is_truthy(&self) -> bool {
        matches!(self, Self::Bool(true))
    }

    fn optional(value: Option<Self>) -> Self {
        match value {
            Some(Self::Optional(value)) => Self::Optional(value),
            Some(value) => Self::Optional(Some(Box::new(value))),
            None => Self::Optional(None),
        }
    }

    fn optional_binding_value(self) -> Option<Self> {
        match self {
            Self::Null | Self::Optional(None) => None,
            Self::Optional(Some(value)) => Some(*value),
            value => Some(value),
        }
    }

    fn member(&self, name: &str) -> Option<Self> {
        match self {
            Self::Object(values) => values.get(name).cloned(),
            Self::Array(values) => match name {
                "count" => Some(Self::Int(values.len() as i64)),
                "isEmpty" => Some(Self::Bool(values.is_empty())),
                "first" => Some(Self::optional(values.first().cloned())),
                "last" => Some(Self::optional(values.last().cloned())),
                "indices" => Some(Self::Array(
                    (0..values.len())
                        .map(|index| Self::Int(index as i64))
                        .collect(),
                )),
                _ => None,
            },
            Self::String(value) => match name {
                "count" => Some(Self::Int(value.chars().count() as i64)),
                "isEmpty" => Some(Self::Bool(value.is_empty())),
                "capitalized" => Some(Self::String(capitalize(value))),
                "uppercased" => Some(Self::String(value.to_uppercase())),
                "lowercased" => Some(Self::String(value.to_lowercase())),
                _ => None,
            },
            Self::EnumCase {
                values,
                labels,
                raw_value,
                ..
            } => {
                if name == "rawValue" {
                    return raw_value.as_deref().cloned();
                }
                labels
                    .iter()
                    .position(|label| label.as_deref() == Some(name))
                    .and_then(|index| values.get(index).cloned())
            }
            Self::Optional(None) => Some(Self::Optional(None)),
            Self::Optional(Some(value)) => {
                value.member(name).map(|value| Self::optional(Some(value)))
            }
            Self::Binding(_) => None,
            _ => None,
        }
    }

    fn iteration_values(&self) -> Vec<Self> {
        match self {
            Self::Array(values) => values.iter().take(MAX_ITERATION_VALUES).cloned().collect(),
            Self::Range(lower, upper, inclusive) => {
                let end = if *inclusive {
                    upper.saturating_add(1)
                } else {
                    *upper
                };
                if end < *lower || end.saturating_sub(*lower) > MAX_ITERATION_VALUES as i64 {
                    Vec::new()
                } else {
                    (*lower..end).map(Self::Int).collect()
                }
            }
            Self::NumericRange(_, _, _) => Vec::new(),
            Self::Optional(Some(value)) => value.iteration_values(),
            _ => Vec::new(),
        }
    }

    fn as_f64(&self) -> Option<f64> {
        match self {
            Self::Int(value) => Some(*value as f64),
            Self::Double(value) if value.is_finite() => Some(*value),
            Self::Optional(Some(value)) => value.as_f64(),
            _ => None,
        }
    }
}

#[derive(Clone)]
struct Environment {
    values: HashMap<String, EvalValue>,
    state: Rc<RefCell<SidebarState>>,
    state_bindings: HashMap<String, String>,
    identity_path: Vec<String>,
    declared_state_keys: Rc<RefCell<HashSet<String>>>,
    state_error: Rc<RefCell<Option<String>>>,
}

impl Environment {
    fn new(context: &Value, state: &SidebarState) -> Self {
        let values = context
            .as_object()
            .map(|values| {
                values
                    .iter()
                    .map(|(key, value)| (key.clone(), EvalValue::from_json(value)))
                    .collect()
            })
            .unwrap_or_default();
        Self {
            values,
            state: Rc::new(RefCell::new(state.clone())),
            state_bindings: HashMap::new(),
            identity_path: Vec::new(),
            declared_state_keys: Rc::new(RefCell::new(HashSet::new())),
            state_error: Rc::new(RefCell::new(None)),
        }
    }

    fn child(&self) -> Self {
        self.clone()
    }

    fn define(&mut self, name: impl Into<String>, value: EvalValue) {
        self.values.insert(name.into(), value);
    }

    fn define_state(&mut self, name: String, key: String, value: EvalValue) {
        self.state_bindings.insert(name.clone(), key);
        self.values.insert(name, value);
    }

    fn push_identity(&mut self, kind: &str, site: usize, value: &EvalValue) {
        if self.identity_path.len() >= MAX_STATE_IDENTITY_DEPTH {
            *self.state_error.borrow_mut() = Some(format!(
                "Custom sidebar state identity exceeds {MAX_STATE_IDENTITY_DEPTH} nested instances."
            ));
            return;
        }
        self.identity_path.push(format!(
            "{kind}:{site}:{:016x}",
            stable_identity_hash(&identity_value_bytes(value))
        ));
    }

    fn push_call_identity(&mut self, site: usize, name: &str) {
        if self.identity_path.len() >= MAX_STATE_IDENTITY_DEPTH {
            *self.state_error.borrow_mut() = Some(format!(
                "Custom sidebar state identity exceeds {MAX_STATE_IDENTITY_DEPTH} nested instances."
            ));
            return;
        }
        self.identity_path.push(format!("call:{site}:{name}"));
    }

    fn state_key(&self, name: &str, declaration_site: usize) -> Option<String> {
        let key = if self.identity_path.is_empty() {
            name.to_string()
        } else {
            let mut bytes = Vec::new();
            for segment in &self.identity_path {
                bytes.extend_from_slice(&(segment.len() as u64).to_le_bytes());
                bytes.extend_from_slice(segment.as_bytes());
            }
            bytes.extend_from_slice(&(declaration_site as u64).to_le_bytes());
            bytes.extend_from_slice(name.as_bytes());
            let suffix_limit = 128usize
                .saturating_sub(INSTANCE_STATE_PREFIX.len())
                .saturating_sub(17);
            let suffix = name.chars().take(suffix_limit).collect::<String>();
            format!(
                "{INSTANCE_STATE_PREFIX}{:016x}_{suffix}",
                stable_identity_hash(&bytes)
            )
        };
        let mut declared = self.declared_state_keys.borrow_mut();
        if !declared.contains(&key) && declared.len() >= MAX_STATE_IDENTITIES {
            *self.state_error.borrow_mut() = Some(format!(
                "Custom sidebar state is limited to {MAX_STATE_IDENTITIES} active values."
            ));
            return None;
        }
        declared.insert(key.clone());
        Some(key)
    }

    fn lookup(&self, name: &str) -> Option<EvalValue> {
        if let Some(value) = self.values.get(name) {
            return Some(value.clone());
        }
        if let Some(binding_name) = name.strip_prefix('$') {
            return self
                .state_bindings
                .get(binding_name)
                .cloned()
                .map(EvalValue::Binding);
        }
        None
    }

    fn state_snapshot(&self) -> SidebarState {
        let declared = self.declared_state_keys.borrow();
        let mut state = self.state.borrow().clone();
        state.retain(|key, _| !key.starts_with(INSTANCE_STATE_PREFIX) || declared.contains(key));
        state
    }

    fn state_error(&self) -> Option<String> {
        self.state_error.borrow().clone()
    }
}

#[derive(Clone, Copy)]
struct FunctionDefinition<'tree> {
    node: Node<'tree>,
    body: Node<'tree>,
}

#[derive(Clone, Copy)]
struct ViewDefinition<'tree> {
    body: Node<'tree>,
    members: Node<'tree>,
}

#[derive(Clone)]
struct EnumCaseDefinition<'tree> {
    raw_value: Option<Node<'tree>>,
    associated_labels: Vec<Option<String>>,
}

#[derive(Clone)]
struct EnumDefinition<'tree> {
    cases: HashMap<String, EnumCaseDefinition<'tree>>,
    raw_type: Option<String>,
}

struct SwiftSidebarInterpreter<'source, 'tree> {
    source: &'source str,
    functions: HashMap<String, FunctionDefinition<'tree>>,
    views: HashMap<String, ViewDefinition<'tree>>,
    enums: HashMap<String, EnumDefinition<'tree>>,
    produced_nodes: usize,
    function_calls: usize,
    event: Option<SidebarEvaluationEvent>,
}

#[cfg_attr(not(test), allow(dead_code))]
pub fn evaluate(source: &str, context: &Value) -> Result<SidebarDocument, String> {
    evaluate_with_state(source, context, &mut SidebarState::new())
}

pub fn evaluate_with_state(
    source: &str,
    context: &Value,
    state: &mut SidebarState,
) -> Result<SidebarDocument, String> {
    evaluate_with_state_and_event(source, context, state, None)
}

pub fn evaluate_with_state_and_event(
    source: &str,
    context: &Value,
    state: &mut SidebarState,
    event: Option<&SidebarEvaluationEvent>,
) -> Result<SidebarDocument, String> {
    if source.len() > 1024 * 1024 {
        return Err("Sidebar file exceeds the 1048576 byte limit.".to_string());
    }
    let mut parser = Parser::new();
    parser
        .set_language(&tree_sitter_swift::LANGUAGE.into())
        .map_err(|error| format!("Failed to load the Swift sidebar parser: {error}"))?;
    let tree = parser
        .parse(source, None)
        .ok_or_else(|| "Failed to parse interpreted Swift sidebar source.".to_string())?;
    let root = tree.root_node();
    let mut interpreter = SwiftSidebarInterpreter {
        source,
        functions: HashMap::new(),
        views: HashMap::new(),
        enums: HashMap::new(),
        produced_nodes: 0,
        function_calls: 0,
        event: event.cloned(),
    };
    interpreter.register_declarations(root);
    let mut environment = Environment::new(context, state);
    let nodes = interpreter.eval_statements(root, &mut environment, 0);
    if let Some(error) = environment.state_error() {
        return Err(error);
    }
    let root = match nodes.len() {
        0 => {
            return Err(if root.has_error() {
                "No supported SwiftUI view found; the source also contains Swift syntax errors."
                    .to_string()
            } else {
                "No supported SwiftUI view found.".to_string()
            });
        }
        1 => nodes.into_iter().next().unwrap(),
        _ => SidebarNode::container(SidebarNodeKind::VStack, nodes),
    };
    *state = environment.state_snapshot();
    Ok(SidebarDocument { version: 1, root })
}

#[allow(dead_code)]
pub fn evaluate_isolated(source: &str, context: &Value) -> Result<SidebarDocument, String> {
    evaluate_isolated_with_state(source, context, &mut SidebarState::new())
}

pub fn evaluate_isolated_with_state(
    source: &str,
    context: &Value,
    state: &mut SidebarState,
) -> Result<SidebarDocument, String> {
    evaluate_isolated_with_state_and_event(source, context, state, None)
}

pub fn evaluate_isolated_with_state_and_event(
    source: &str,
    context: &Value,
    state: &mut SidebarState,
    event: Option<&SidebarEvaluationEvent>,
) -> Result<SidebarDocument, String> {
    let request = serde_json::to_vec(&WorkerRequest {
        version: WORKER_PROTOCOL_VERSION,
        source: source.to_string(),
        context: context.clone(),
        state: state.clone(),
        event: event.cloned(),
    })
    .map_err(|error| format!("Failed to encode Swift sidebar worker request: {error}"))?;
    if request.len() > MAX_WORKER_REQUEST_BYTES {
        return Err(format!(
            "Swift sidebar worker request exceeds the {MAX_WORKER_REQUEST_BYTES} byte limit."
        ));
    }

    let executable = std::env::var_os("CMUX_CUSTOM_SIDEBAR_WORKER")
        .map(std::path::PathBuf::from)
        .or_else(|| std::env::current_exe().ok())
        .ok_or_else(|| "Failed to resolve the cmux sidebar worker executable.".to_string())?;
    let mut child = Command::new(&executable)
        .arg("__sidebar-interpreter-worker")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|error| {
            format!(
                "Failed to start Swift sidebar worker {}: {error}",
                executable.display()
            )
        })?;

    let stdin = child
        .stdin
        .take()
        .ok_or_else(|| "Swift sidebar worker stdin was unavailable.".to_string())?;

    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| "Swift sidebar worker stdout was unavailable.".to_string())?;
    let stderr = child
        .stderr
        .take()
        .ok_or_else(|| "Swift sidebar worker stderr was unavailable.".to_string())?;
    let stdin_writer = thread::spawn(move || {
        let mut stdin = stdin;
        stdin
            .write_all(&request)
            .map_err(|error| format!("Failed to write Swift sidebar worker request: {error}"))
    });
    let stdout_reader = thread::spawn(move || read_bounded(stdout, MAX_WORKER_RESPONSE_BYTES));
    let stderr_reader = thread::spawn(move || read_bounded(stderr, 64 * 1024));

    let timeout = worker_timeout();
    let deadline = Instant::now() + timeout;
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) if Instant::now() < deadline => thread::sleep(Duration::from_millis(10)),
            Ok(None) => {
                let _ = child.kill();
                let _ = child.wait();
                let _ = stdin_writer.join();
                let _ = stdout_reader.join();
                let _ = stderr_reader.join();
                return Err(format!(
                    "Swift sidebar worker timed out after {} milliseconds.",
                    timeout.as_millis()
                ));
            }
            Err(error) => {
                let _ = child.kill();
                let _ = child.wait();
                let _ = stdin_writer.join();
                let _ = stdout_reader.join();
                let _ = stderr_reader.join();
                return Err(format!("Failed to wait for Swift sidebar worker: {error}"));
            }
        }
    };
    let stdin_result = stdin_writer
        .join()
        .map_err(|_| "Swift sidebar worker stdin writer panicked.".to_string())?;
    let stdout = stdout_reader
        .join()
        .map_err(|_| "Swift sidebar worker stdout reader panicked.".to_string())??;
    let stderr = stderr_reader
        .join()
        .map_err(|_| "Swift sidebar worker stderr reader panicked.".to_string())??;
    if !status.success() {
        let detail = String::from_utf8_lossy(&stderr);
        return Err(format!(
            "Swift sidebar worker exited with {status}{}",
            if detail.trim().is_empty() {
                String::new()
            } else {
                format!(": {}", detail.trim())
            }
        ));
    }
    stdin_result?;
    let response = serde_json::from_slice::<WorkerResponse>(&stdout)
        .map_err(|error| format!("Swift sidebar worker returned invalid JSON: {error}"))?;
    if response.version != WORKER_PROTOCOL_VERSION {
        return Err(format!(
            "Swift sidebar worker protocol version {} is unsupported; expected {}.",
            response.version, WORKER_PROTOCOL_VERSION
        ));
    }
    match (response.document, response.state, response.error) {
        (Some(document), Some(response_state), None) => {
            *state = response_state;
            Ok(document)
        }
        (None, _, Some(error)) => Err(error),
        _ => Err("Swift sidebar worker returned an invalid response.".to_string()),
    }
}

pub fn run_worker() -> Result<(), String> {
    let mut input = Vec::new();
    std::io::stdin()
        .take(MAX_WORKER_REQUEST_BYTES as u64 + 1)
        .read_to_end(&mut input)
        .map_err(|error| format!("Failed to read Swift sidebar worker request: {error}"))?;
    if input.len() > MAX_WORKER_REQUEST_BYTES {
        return Err(format!(
            "Swift sidebar worker request exceeds the {MAX_WORKER_REQUEST_BYTES} byte limit."
        ));
    }
    let request = serde_json::from_slice::<WorkerRequest>(&input)
        .map_err(|error| format!("Invalid Swift sidebar worker request: {error}"))?;
    if request.version != WORKER_PROTOCOL_VERSION {
        return Err(format!(
            "Swift sidebar worker protocol version {} is unsupported; expected {}.",
            request.version, WORKER_PROTOCOL_VERSION
        ));
    }
    let mut state = request.state;
    let response = match evaluate_with_state_and_event(
        &request.source,
        &request.context,
        &mut state,
        request.event.as_ref(),
    ) {
        Ok(document) => WorkerResponse {
            version: WORKER_PROTOCOL_VERSION,
            document: Some(document),
            state: Some(state),
            error: None,
        },
        Err(error) => WorkerResponse {
            version: WORKER_PROTOCOL_VERSION,
            document: None,
            state: None,
            error: Some(error),
        },
    };
    serde_json::to_writer(std::io::stdout().lock(), &response)
        .map_err(|error| format!("Failed to write Swift sidebar worker response: {error}"))
}

fn worker_timeout() -> Duration {
    std::env::var("CMUX_CUSTOM_SIDEBAR_WORKER_TIMEOUT_MS")
        .ok()
        .and_then(|value| value.trim().parse::<u64>().ok())
        .map(|milliseconds| Duration::from_millis(milliseconds.clamp(50, 10_000)))
        .unwrap_or(DEFAULT_WORKER_TIMEOUT)
}

fn read_bounded(reader: impl Read, limit: usize) -> Result<Vec<u8>, String> {
    let mut output = Vec::new();
    reader
        .take(limit as u64 + 1)
        .read_to_end(&mut output)
        .map_err(|error| format!("Failed to read Swift sidebar worker output: {error}"))?;
    if output.len() > limit {
        return Err(format!(
            "Swift sidebar worker output exceeds the {limit} byte limit."
        ));
    }
    Ok(output)
}

impl<'source, 'tree> SwiftSidebarInterpreter<'source, 'tree> {
    fn register_declarations(&mut self, root: Node<'tree>) {
        let mut cursor = root.walk();
        let mut stack = vec![root];
        while let Some(node) = stack.pop() {
            if node.kind() == "class_declaration" {
                match node
                    .child_by_field_name("declaration_kind")
                    .map(|kind| self.text(kind))
                {
                    Some("struct") => self.register_view(node),
                    Some("enum") => self.register_enum(node),
                    _ => {}
                }
                continue;
            }
            if node.kind() == "function_declaration" {
                if let (Some(name), Some(body)) = (
                    node.child_by_field_name("name"),
                    node.child_by_field_name("body"),
                ) {
                    self.functions.insert(
                        self.text(name).to_string(),
                        FunctionDefinition { node, body },
                    );
                }
                continue;
            }
            stack.extend(node.named_children(&mut cursor));
        }
    }

    fn register_enum(&mut self, declaration: Node<'tree>) {
        let Some(name) = declaration.child_by_field_name("name") else {
            return;
        };
        let Some(body) = declaration.child_by_field_name("body") else {
            return;
        };
        let raw_type = declaration
            .named_children(&mut declaration.walk())
            .find(|child| child.kind() == "inheritance_specifier")
            .and_then(|inheritance| {
                self.text(inheritance)
                    .trim()
                    .trim_start_matches(':')
                    .split(',')
                    .next()
                    .map(str::trim)
                    .filter(|name| !name.is_empty())
                    .map(str::to_string)
            });
        let mut cases = HashMap::new();
        for entry in body
            .named_children(&mut body.walk())
            .filter(|entry| entry.kind() == "enum_entry")
        {
            let case_names = entry
                .children_by_field_name("name", &mut entry.walk())
                .collect::<Vec<_>>();
            if case_names.is_empty() {
                continue;
            }
            let associated_labels: Vec<Option<String>> = entry
                .child_by_field_name("data_contents")
                .map(|contents| {
                    let source = self
                        .text(contents)
                        .trim()
                        .trim_start_matches('(')
                        .trim_end_matches(')');
                    split_top_level(source, ',')
                        .into_iter()
                        .filter(|parameter| !parameter.trim().is_empty())
                        .map(|parameter| {
                            parameter
                                .split_once(':')
                                .map(|(label, _)| {
                                    label
                                        .trim()
                                        .trim_matches(|character: char| {
                                            !is_identifier_char(character)
                                        })
                                        .to_string()
                                })
                                .filter(|label| !label.is_empty() && label != "_")
                        })
                        .collect()
                })
                .unwrap_or_default();
            let last_index = case_names.len().saturating_sub(1);
            for (index, case_name) in case_names.into_iter().enumerate() {
                cases.insert(
                    self.text(case_name).to_string(),
                    EnumCaseDefinition {
                        raw_value: (index == last_index)
                            .then(|| entry.child_by_field_name("raw_value"))
                            .flatten(),
                        associated_labels: associated_labels.clone(),
                    },
                );
            }
        }
        if !cases.is_empty() {
            self.enums.insert(
                self.text(name).to_string(),
                EnumDefinition { cases, raw_type },
            );
        }
    }

    fn register_view(&mut self, declaration: Node<'tree>) {
        let is_struct = declaration
            .child_by_field_name("declaration_kind")
            .is_some_and(|kind| self.text(kind) == "struct");
        if !is_struct
            || !declaration
                .named_children(&mut declaration.walk())
                .any(|child| {
                    child.kind() == "inheritance_specifier"
                        && self.text(child).split(',').any(|name| {
                            let name = name
                                .trim()
                                .trim_start_matches(':')
                                .trim()
                                .split_whitespace()
                                .last()
                                .unwrap_or_default();
                            name == "View" || name.ends_with(".View")
                        })
                })
        {
            return;
        }
        let Some(name) = declaration.child_by_field_name("name") else {
            return;
        };
        let Some(members) = declaration.child_by_field_name("body") else {
            return;
        };
        let body = members
            .named_children(&mut members.walk())
            .filter(|member| member.kind() == "property_declaration")
            .find_map(|property| {
                (self.property_name(property).as_deref() == Some("body"))
                    .then(|| property.child_by_field_name("computed_value"))
                    .flatten()
            });
        if let Some(body) = body {
            self.views.insert(
                self.text(name).to_string(),
                ViewDefinition { body, members },
            );
        }
    }

    fn eval_statements(
        &mut self,
        container: Node<'tree>,
        environment: &mut Environment,
        depth: usize,
    ) -> Vec<SidebarNode> {
        self.eval_statements_flow(container, environment, depth)
            .into_value()
    }

    fn eval_statements_flow(
        &mut self,
        container: Node<'tree>,
        environment: &mut Environment,
        depth: usize,
    ) -> EvalFlow<Vec<SidebarNode>> {
        if depth > MAX_EXPRESSION_DEPTH || self.produced_nodes >= 4096 {
            return EvalFlow::Normal(Vec::new());
        }
        let statements = if container.kind() == "source_file" || container.kind() == "statements" {
            container
        } else {
            self.first_named_child(container, "statements")
                .unwrap_or(container)
        };
        let mut cursor = statements.walk();
        let mut nodes = Vec::new();
        for child in statements.named_children(&mut cursor) {
            if self.produced_nodes >= 4096 {
                break;
            }
            match child.kind() {
                "class_declaration" | "function_declaration" | "comment" => {}
                "property_declaration" => self.apply_binding(child, environment, depth + 1),
                "if_statement" => match self.eval_if_flow(child, environment, depth + 1) {
                    EvalFlow::Normal(values) => nodes.extend(values),
                    EvalFlow::Return(values) => {
                        nodes.extend(values);
                        return EvalFlow::Return(nodes);
                    }
                    EvalFlow::Break(values) => {
                        nodes.extend(values);
                        return EvalFlow::Break(nodes);
                    }
                    EvalFlow::Continue(values) => {
                        nodes.extend(values);
                        return EvalFlow::Continue(nodes);
                    }
                },
                "guard_statement" => {
                    if let Some(passed_environment) =
                        self.eval_condition_environment(child, environment, depth + 1)
                    {
                        *environment = passed_environment;
                    } else {
                        let branch = self.first_named_child(child, "statements");
                        let mut branch_environment = environment.child();
                        let flow = branch
                            .map(|branch| {
                                self.eval_statements_flow(
                                    branch,
                                    &mut branch_environment,
                                    depth + 1,
                                )
                            })
                            .unwrap_or_else(|| EvalFlow::Normal(Vec::new()));
                        let values = flow.into_value();
                        nodes.extend(values);
                        return EvalFlow::Return(nodes);
                    }
                }
                "switch_statement" => match self.eval_switch_flow(child, environment, depth + 1) {
                    EvalFlow::Normal(values) => nodes.extend(values),
                    EvalFlow::Return(values) => {
                        nodes.extend(values);
                        return EvalFlow::Return(nodes);
                    }
                    EvalFlow::Break(values) => {
                        nodes.extend(values);
                        return EvalFlow::Break(nodes);
                    }
                    EvalFlow::Continue(values) => {
                        nodes.extend(values);
                        return EvalFlow::Continue(nodes);
                    }
                },
                "for_statement" => match self.eval_for_flow(child, environment, depth + 1) {
                    EvalFlow::Normal(values) => nodes.extend(values),
                    EvalFlow::Return(values) => {
                        nodes.extend(values);
                        return EvalFlow::Return(nodes);
                    }
                    EvalFlow::Break(values) | EvalFlow::Continue(values) => nodes.extend(values),
                },
                "control_transfer_statement" => {
                    let transfer = self.text(child).trim().to_string();
                    if let Some(result) = child.child_by_field_name("result") {
                        nodes.extend(self.eval_view_or_expansion(result, environment, depth + 1));
                    }
                    if transfer.starts_with("return") {
                        return EvalFlow::Return(nodes);
                    }
                    if transfer.starts_with("break") {
                        return EvalFlow::Break(nodes);
                    }
                    if transfer.starts_with("continue") {
                        return EvalFlow::Continue(nodes);
                    }
                }
                _ => nodes.extend(self.eval_view_or_expansion(child, environment, depth + 1)),
            }
        }
        EvalFlow::Normal(nodes)
    }

    fn eval_view_or_expansion(
        &mut self,
        node: Node<'tree>,
        environment: &Environment,
        depth: usize,
    ) -> Vec<SidebarNode> {
        if node.kind() == "call_expression" {
            if self.call_name(node).as_deref() == Some("ForEach") {
                return self.eval_for_each(node, environment, depth + 1);
            }
            if self.call_name(node).as_deref() == Some("Reorderable") {
                return self
                    .eval_reorderable(node, environment, depth + 1)
                    .into_iter()
                    .collect();
            }
        }
        self.eval_view(node, environment, depth)
            .into_iter()
            .collect()
    }

    fn eval_view(
        &mut self,
        node: Node<'tree>,
        environment: &Environment,
        depth: usize,
    ) -> Option<SidebarNode> {
        if depth > MAX_EXPRESSION_DEPTH || self.produced_nodes >= 4096 {
            return None;
        }
        if node.kind() != "call_expression" {
            return None;
        }
        let callee = node.named_child(0)?;
        if callee.kind() == "navigation_expression" {
            let target = callee.child_by_field_name("target")?;
            if target.kind() == "call_expression" {
                let mut rendered = self.eval_view(target, environment, depth + 1)?;
                let modifier = self.navigation_suffix(&callee)?;
                self.apply_modifier(&mut rendered, &modifier, node, environment, depth + 1);
                return Some(rendered);
            }
        }
        let name = self.call_name(node)?;
        let args = self.call_arguments(node);
        let closure = self.call_closure(node);
        let mut rendered = match name.as_str() {
            "Text" => SidebarNode::text(
                args.first()
                    .and_then(|arg| self.eval_expr(arg.value, environment, depth + 1))
                    .map(|value| value.display_string())
                    .unwrap_or_default(),
            ),
            "Image" => {
                let system_name = args
                    .iter()
                    .find(|arg| arg.label.as_deref() == Some("systemName"))
                    .or_else(|| args.first())
                    .and_then(|arg| self.eval_expr(arg.value, environment, depth + 1))
                    .map(|value| value.display_string())
                    .unwrap_or_default();
                SidebarNode::image(system_name)
            }
            "Label" => {
                let title = args
                    .first()
                    .and_then(|arg| self.eval_expr(arg.value, environment, depth + 1))
                    .map(|value| value.display_string())
                    .unwrap_or_default();
                let system_name = args
                    .iter()
                    .find(|arg| arg.label.as_deref() == Some("systemImage"))
                    .and_then(|arg| self.eval_expr(arg.value, environment, depth + 1))
                    .map(|value| value.display_string())
                    .unwrap_or_default();
                SidebarNode::container(
                    SidebarNodeKind::HStack,
                    vec![SidebarNode::image(system_name), SidebarNode::text(title)],
                )
            }
            "Button" => self.eval_button(node, &args, closure, environment, depth + 1),
            "Toggle" => self.eval_toggle(&args, closure, environment, depth + 1)?,
            "TextField" => self.eval_text_field(&args, environment, depth + 1)?,
            "Slider" => self.eval_slider(&args, closure, environment, depth + 1)?,
            "Picker" => self.eval_picker(&args, closure, environment, depth + 1)?,
            "Stepper" => self.eval_stepper(&args, closure, environment, depth + 1)?,
            "Spacer" => SidebarNode::simple(SidebarNodeKind::Spacer),
            "Divider" => SidebarNode::simple(SidebarNodeKind::Divider),
            "VStack" | "LazyVStack" | "List" | "Group" | "Section" | "ScrollView" => {
                let mut child_environment = environment.child();
                let children = closure
                    .map(|closure| self.eval_statements(closure, &mut child_environment, depth + 1))
                    .unwrap_or_default();
                let mut node = SidebarNode::container(SidebarNodeKind::VStack, children);
                node.spacing = self.numeric_argument(&args, "spacing", environment, depth + 1);
                node.alignment = self.token_argument(&args, "alignment", environment, depth + 1);
                if name == "Section" {
                    node.title = args
                        .first()
                        .and_then(|arg| self.eval_expr(arg.value, environment, depth + 1))
                        .map(|value| value.display_string());
                }
                node
            }
            "HStack" | "LazyHStack" | "HSplitView" => {
                let mut child_environment = environment.child();
                let children = closure
                    .map(|closure| self.eval_statements(closure, &mut child_environment, depth + 1))
                    .unwrap_or_default();
                let mut node = SidebarNode::container(SidebarNodeKind::HStack, children);
                node.spacing = self.numeric_argument(&args, "spacing", environment, depth + 1);
                node.alignment = self.token_argument(&args, "alignment", environment, depth + 1);
                node
            }
            "ZStack" => {
                let mut child_environment = environment.child();
                let children = closure
                    .map(|closure| self.eval_statements(closure, &mut child_environment, depth + 1))
                    .unwrap_or_default();
                let mut node = SidebarNode::container(SidebarNodeKind::ZStack, children);
                node.alignment = self.token_argument(&args, "alignment", environment, depth + 1);
                node
            }
            "ProgressView" | "Gauge" => {
                let value = self
                    .argument(&args, "value")
                    .and_then(|value| self.eval_expr(value, environment, depth + 1))
                    .and_then(|value| value.as_f64());
                let total = self
                    .argument(&args, "total")
                    .and_then(|value| self.eval_expr(value, environment, depth + 1))
                    .and_then(|value| value.as_f64())
                    .unwrap_or(1.0);
                let mut node = SidebarNode::simple(SidebarNodeKind::Progress);
                node.value = value
                    .filter(|_| total != 0.0)
                    .map(|value| (value / total).clamp(0.0, 1.0));
                node
            }
            "Rectangle"
            | "RoundedRectangle"
            | "UnevenRoundedRectangle"
            | "Capsule"
            | "Circle"
            | "Ellipse" => {
                let mut node = SidebarNode::simple(SidebarNodeKind::Shape);
                node.corner_radius =
                    self.numeric_argument(&args, "cornerRadius", environment, depth + 1);
                node
            }
            "AnyView" => {
                let inner = args.first()?;
                return self.eval_view(inner.value, environment, depth + 1);
            }
            "EmptyView" => SidebarNode::container(SidebarNodeKind::VStack, Vec::new()),
            _ => {
                if self.views.contains_key(&name) {
                    return self.eval_custom_view(&name, node, environment, depth + 1);
                }
                return self.eval_user_view_function(&name, node, environment, depth + 1);
            }
        };
        self.record_node();
        if rendered.kind == SidebarNodeKind::VStack
            && rendered
                .title
                .as_deref()
                .is_some_and(|title| !title.is_empty())
        {
            rendered.children.insert(
                0,
                SidebarNode {
                    weight: Some("semibold".to_string()),
                    ..SidebarNode::text(rendered.title.take().unwrap())
                },
            );
        }
        Some(rendered)
    }

    fn eval_button(
        &mut self,
        call: Node<'tree>,
        args: &[CallArgument<'tree>],
        trailing_closure: Option<Node<'tree>>,
        environment: &Environment,
        depth: usize,
    ) -> SidebarNode {
        let action_closure = args
            .iter()
            .find(|arg| arg.label.as_deref() == Some("action"))
            .map(|arg| arg.value)
            .filter(|node| node.kind() == "lambda_literal");
        let mut children = Vec::new();
        let mut title = None;
        let action = if let Some(action_closure) = action_closure {
            if let Some(label_closure) = trailing_closure {
                let mut child_environment = environment.child();
                children = self.eval_statements(label_closure, &mut child_environment, depth + 1);
            }
            self.parse_action(action_closure, environment, depth + 1)
        } else {
            title = args
                .first()
                .and_then(|arg| self.eval_expr(arg.value, environment, depth + 1))
                .map(|value| value.display_string());
            trailing_closure
                .map(|closure| self.parse_action(closure, environment, depth + 1))
                .unwrap_or_default()
        };
        let mut node = SidebarNode::simple(SidebarNodeKind::Button);
        node.title = title;
        node.children = children;
        node.action = (!action.commands.is_empty()).then_some(action);
        if node.title.is_none() && node.children.is_empty() {
            node.title = self
                .call_arguments(call)
                .first()
                .and_then(|arg| self.eval_expr(arg.value, environment, depth + 1))
                .map(|value| value.display_string());
        }
        node
    }

    fn eval_toggle(
        &mut self,
        args: &[CallArgument<'tree>],
        trailing_closure: Option<Node<'tree>>,
        environment: &Environment,
        depth: usize,
    ) -> Option<SidebarNode> {
        let binding = self.binding_argument(args, "isOn", environment, depth + 1)?;
        if !binding.value.is_boolean() {
            return None;
        }
        let mut node = SidebarNode::simple(SidebarNodeKind::Toggle);
        node.title = args
            .first()
            .filter(|arg| arg.label.is_none())
            .and_then(|arg| self.eval_expr(arg.value, environment, depth + 1))
            .map(|value| value.display_string());
        if let Some(closure) = trailing_closure {
            let mut child_environment = environment.child();
            node.children = self.eval_statements(closure, &mut child_environment, depth + 1);
        }
        node.binding = Some(binding);
        Some(node)
    }

    fn eval_text_field(
        &mut self,
        args: &[CallArgument<'tree>],
        environment: &Environment,
        depth: usize,
    ) -> Option<SidebarNode> {
        let binding = self.binding_argument(args, "text", environment, depth + 1)?;
        if !binding.value.is_string() {
            return None;
        }
        let mut node = SidebarNode::simple(SidebarNodeKind::TextField);
        node.placeholder = args
            .first()
            .filter(|arg| arg.label.is_none())
            .and_then(|arg| self.eval_expr(arg.value, environment, depth + 1))
            .map(|value| value.display_string())
            .filter(|value| !value.is_empty());
        node.binding = Some(binding);
        Some(node)
    }

    fn eval_slider(
        &mut self,
        args: &[CallArgument<'tree>],
        trailing_closure: Option<Node<'tree>>,
        environment: &Environment,
        depth: usize,
    ) -> Option<SidebarNode> {
        let binding = self.binding_argument(args, "value", environment, depth + 1)?;
        if !binding.value.is_number() {
            return None;
        }
        let (minimum, maximum) = self
            .range_argument(args, "in", environment, depth + 1)
            .unwrap_or((0.0, 1.0));
        if minimum >= maximum {
            return None;
        }
        let step = self
            .numeric_argument(args, "step", environment, depth + 1)
            .filter(|step| *step > 0.0)
            .unwrap_or_else(|| ((maximum - minimum) / 100.0).max(0.000_001));
        let mut node = SidebarNode::simple(SidebarNodeKind::Slider);
        node.binding = Some(binding);
        node.minimum = Some(minimum);
        node.maximum = Some(maximum);
        node.step = Some(step);
        if let Some(closure) = trailing_closure {
            let mut child_environment = environment.child();
            node.children = self.eval_statements(closure, &mut child_environment, depth + 1);
        }
        Some(node)
    }

    fn eval_picker(
        &mut self,
        args: &[CallArgument<'tree>],
        trailing_closure: Option<Node<'tree>>,
        environment: &Environment,
        depth: usize,
    ) -> Option<SidebarNode> {
        let binding = self.binding_argument(args, "selection", environment, depth + 1)?;
        if !is_picker_json_value(&binding.value) {
            return None;
        }
        let mut node = SidebarNode::simple(SidebarNodeKind::Picker);
        node.title = args
            .first()
            .filter(|arg| arg.label.is_none())
            .and_then(|arg| self.eval_expr(arg.value, environment, depth + 1))
            .map(|value| value.display_string())
            .filter(|value| !value.is_empty());
        node.binding = Some(binding);
        if let Some(closure) = trailing_closure {
            let mut child_environment = environment.child();
            node.options = self
                .eval_statements(closure, &mut child_environment, depth + 1)
                .into_iter()
                .filter_map(|mut option| {
                    let value = option.tag.take()?;
                    let label = sidebar_option_label(&option)?;
                    Some(SidebarOption { label, value })
                })
                .collect();
        }
        Some(node)
    }

    fn eval_stepper(
        &mut self,
        args: &[CallArgument<'tree>],
        trailing_closure: Option<Node<'tree>>,
        environment: &Environment,
        depth: usize,
    ) -> Option<SidebarNode> {
        let binding = self.binding_argument(args, "value", environment, depth + 1)?;
        let current = binding.value.as_f64()?;
        let (minimum, maximum) = self
            .range_argument(args, "in", environment, depth + 1)
            .unwrap_or_else(|| (current.min(-1_000_000_000.0), current.max(1_000_000_000.0)));
        if minimum >= maximum {
            return None;
        }
        let step = self
            .numeric_argument(args, "step", environment, depth + 1)
            .filter(|step| *step > 0.0)
            .unwrap_or(1.0);
        let mut node = SidebarNode::simple(SidebarNodeKind::Stepper);
        node.title = args
            .first()
            .filter(|arg| arg.label.is_none())
            .and_then(|arg| self.eval_expr(arg.value, environment, depth + 1))
            .map(|value| value.display_string())
            .filter(|value| !value.is_empty());
        node.binding = Some(binding);
        node.minimum = Some(minimum);
        node.maximum = Some(maximum);
        node.step = Some(step);
        if let Some(closure) = trailing_closure {
            let mut child_environment = environment.child();
            node.children = self.eval_statements(closure, &mut child_environment, depth + 1);
        }
        Some(node)
    }

    fn binding_argument(
        &mut self,
        args: &[CallArgument<'tree>],
        label: &str,
        environment: &Environment,
        depth: usize,
    ) -> Option<SidebarBinding> {
        let EvalValue::Binding(key) = self
            .argument(args, label)
            .and_then(|value| self.eval_expr(value, environment, depth + 1))?
        else {
            return None;
        };
        let value = environment.state.borrow().get(&key).cloned()?;
        Some(SidebarBinding { key, value })
    }

    fn eval_user_view_function(
        &mut self,
        name: &str,
        call: Node<'tree>,
        environment: &Environment,
        depth: usize,
    ) -> Option<SidebarNode> {
        if depth > MAX_EXPRESSION_DEPTH || self.function_calls >= MAX_FUNCTION_CALLS {
            return None;
        }
        let definition = *self.functions.get(name)?;
        self.function_calls += 1;
        let args = self.call_arguments(call);
        let parameters = self.function_parameters(definition);
        let mut child_environment = environment.child();
        child_environment.push_call_identity(call.start_byte(), name);
        for (index, parameter) in parameters.iter().enumerate() {
            if let Some(value) = args
                .get(index)
                .and_then(|arg| self.eval_expr(arg.value, environment, depth + 1))
            {
                child_environment.define(parameter, value);
            }
        }
        let nodes = self.eval_statements(definition.body, &mut child_environment, depth + 1);
        self.function_calls = self.function_calls.saturating_sub(1);
        match nodes.len() {
            0 => None,
            1 => nodes.into_iter().next(),
            _ => Some(SidebarNode::container(SidebarNodeKind::VStack, nodes)),
        }
    }

    fn eval_custom_view(
        &mut self,
        name: &str,
        call: Node<'tree>,
        environment: &Environment,
        depth: usize,
    ) -> Option<SidebarNode> {
        if depth > MAX_EXPRESSION_DEPTH || self.function_calls >= MAX_FUNCTION_CALLS {
            return None;
        }
        let definition = *self.views.get(name)?;
        self.function_calls += 1;
        let args = self.call_arguments(call);
        let mut positional_args = args.iter().filter(|arg| arg.label.is_none());
        let mut child_environment = environment.child();
        child_environment.push_call_identity(call.start_byte(), name);
        let mut fields = HashMap::new();
        let mut valid = true;
        let mut members = definition.members.walk();
        for property in definition
            .members
            .named_children(&mut members)
            .filter(|member| member.kind() == "property_declaration")
        {
            let Some(property_name) = self.property_name(property) else {
                continue;
            };
            if property_name == "body" {
                continue;
            }
            if self.property_has_state_attribute(property) {
                self.apply_binding(property, &mut child_environment, depth + 1);
            } else {
                let argument = args
                    .iter()
                    .find(|argument| argument.label.as_deref() == Some(property_name.as_str()))
                    .or_else(|| positional_args.next());
                let value = argument
                    .and_then(|argument| self.eval_expr(argument.value, environment, depth + 1))
                    .or_else(|| {
                        property
                            .child_by_field_name("value")
                            .and_then(|value| self.eval_expr(value, &child_environment, depth + 1))
                    });
                if let Some(value) = value {
                    child_environment.define(property_name.clone(), value);
                } else {
                    valid = false;
                    break;
                }
            }
            if let Some(value) = child_environment.lookup(&property_name) {
                fields.insert(property_name, value);
            }
        }
        if !valid {
            self.function_calls = self.function_calls.saturating_sub(1);
            return None;
        }
        child_environment.define("self", EvalValue::Object(fields));
        let nodes = self.eval_statements(definition.body, &mut child_environment, depth + 1);
        self.function_calls = self.function_calls.saturating_sub(1);
        match nodes.len() {
            0 => None,
            1 => nodes.into_iter().next(),
            _ => Some(SidebarNode::container(SidebarNodeKind::VStack, nodes)),
        }
    }

    fn eval_for_each(
        &mut self,
        call: Node<'tree>,
        environment: &Environment,
        depth: usize,
    ) -> Vec<SidebarNode> {
        let args = self.call_arguments(call);
        let Some(sequence) = args
            .first()
            .and_then(|arg| self.eval_expr(arg.value, environment, depth + 1))
        else {
            return Vec::new();
        };
        let Some(closure) = self.call_closure(call) else {
            return Vec::new();
        };
        let parameters = self.lambda_parameters(closure);
        let mut nodes = Vec::new();
        for (index, value) in sequence.iteration_values().into_iter().enumerate() {
            if self.produced_nodes >= 4096 {
                break;
            }
            let mut child_environment = environment.child();
            let identity = self.iteration_identity_value(&args, &value, index);
            child_environment.push_identity("foreach", call.start_byte(), &identity);
            child_environment.define("$0", value.clone());
            if parameters.len() >= 2 {
                if let Some(parameter) = parameters.first() {
                    child_environment.define(
                        parameter,
                        value
                            .member("0")
                            .unwrap_or_else(|| EvalValue::Int(index as i64)),
                    );
                }
                if let Some(parameter) = parameters.get(1) {
                    child_environment.define(
                        parameter,
                        value.member("1").unwrap_or_else(|| value.clone()),
                    );
                }
            } else if let Some(parameter) = parameters.first() {
                child_environment.define(parameter, value);
            }
            nodes.extend(self.eval_statements(closure, &mut child_environment, depth + 1));
        }
        nodes
    }

    fn iteration_identity_value(
        &self,
        args: &[CallArgument<'tree>],
        value: &EvalValue,
        index: usize,
    ) -> EvalValue {
        if let Some(id) = self.argument(args, "id") {
            let key_path = self
                .text(id)
                .trim()
                .trim_start_matches('\\')
                .trim_start_matches('.')
                .trim();
            if key_path == "self" {
                return value.clone();
            }
            if !key_path.is_empty() {
                if let Some(identity) = value.member(key_path) {
                    return identity;
                }
            }
        }
        value.member("id").unwrap_or_else(|| match value {
            EvalValue::Null | EvalValue::Binding(_) => EvalValue::Int(index as i64),
            _ => value.clone(),
        })
    }

    fn eval_reorderable(
        &mut self,
        call: Node<'tree>,
        environment: &Environment,
        depth: usize,
    ) -> Option<SidebarNode> {
        let args = self.call_arguments(call);
        let sequence = args
            .first()
            .and_then(|arg| self.eval_expr(arg.value, environment, depth + 1))?;
        let closure = self.call_closure(call)?;
        let method = self
            .argument(&args, "move")
            .and_then(|value| self.eval_expr(value, environment, depth + 1))
            .map(|value| value.display_string())
            .filter(|value| !value.is_empty())?;
        let id_parameter = reorder_id_parameter(&method);
        let parameters = self.lambda_parameters(closure);
        let mut children = Vec::new();
        for (index, value) in sequence.iteration_values().into_iter().enumerate() {
            let item_id = value
                .member("id")
                .unwrap_or_else(|| value.clone())
                .display_string();
            if item_id.is_empty() {
                continue;
            }
            let mut child_environment = environment.child();
            child_environment.push_identity(
                "reorderable",
                call.start_byte(),
                &EvalValue::String(item_id.clone()),
            );
            child_environment.define("$0", value.clone());
            if let Some(parameter) = parameters.first() {
                child_environment.define(parameter, value);
            }
            let rendered = self.eval_statements(closure, &mut child_environment, depth + 1);
            let mut item = match rendered.len() {
                0 => continue,
                1 => rendered.into_iter().next().unwrap(),
                _ => {
                    self.record_node();
                    SidebarNode::container(SidebarNodeKind::VStack, rendered)
                }
            };
            item.reorder = Some(SidebarReorder {
                method: method.clone(),
                id_parameter: id_parameter.clone(),
                item_id,
                index,
            });
            children.push(item);
        }
        Some(SidebarNode::container(SidebarNodeKind::VStack, children))
    }

    fn eval_if_flow(
        &mut self,
        node: Node<'tree>,
        environment: &Environment,
        depth: usize,
    ) -> EvalFlow<Vec<SidebarNode>> {
        let condition_environment = self.eval_condition_environment(node, environment, depth + 1);
        let mut cursor = node.walk();
        let branches = node
            .named_children(&mut cursor)
            .filter(|child| child.kind() == "statements")
            .collect::<Vec<_>>();
        let selected = if condition_environment.is_some() {
            branches.first().copied()
        } else {
            branches.get(1).copied()
        };
        let mut child_environment = condition_environment.unwrap_or_else(|| environment.child());
        selected
            .map(|branch| self.eval_statements_flow(branch, &mut child_environment, depth + 1))
            .unwrap_or_else(|| EvalFlow::Normal(Vec::new()))
    }

    fn eval_condition_environment(
        &mut self,
        node: Node<'tree>,
        environment: &Environment,
        depth: usize,
    ) -> Option<Environment> {
        let conditions = node
            .named_children(&mut node.walk())
            .take_while(|child| child.kind() != "statements" && child.kind() != "else")
            .collect::<Vec<_>>();
        let mut condition_environment = environment.child();
        let mut index = 0;
        while index < conditions.len() {
            let condition = conditions[index];
            if condition.kind() == "value_binding_pattern" {
                let name = conditions.get(index + 1).copied()?;
                let expression = conditions.get(index + 2).copied()?;
                let value = self
                    .eval_expr(expression, &condition_environment, depth + 1)?
                    .optional_binding_value()?;
                condition_environment.define(self.text(name).trim(), value);
                index += 3;
                continue;
            }
            if !self
                .eval_expr(condition, &condition_environment, depth + 1)
                .is_some_and(|value| value.is_truthy())
            {
                return None;
            }
            index += 1;
        }
        Some(condition_environment)
    }

    fn eval_switch_flow(
        &mut self,
        node: Node<'tree>,
        environment: &Environment,
        depth: usize,
    ) -> EvalFlow<Vec<SidebarNode>> {
        let Some((statements, mut branch_environment)) =
            self.select_switch_branch(node, environment, depth + 1)
        else {
            return EvalFlow::Normal(Vec::new());
        };
        self.eval_statements_flow(statements, &mut branch_environment, depth + 1)
    }

    fn select_switch_branch(
        &mut self,
        node: Node<'tree>,
        environment: &Environment,
        depth: usize,
    ) -> Option<(Node<'tree>, Environment)> {
        let value = node
            .child_by_field_name("expr")
            .and_then(|expr| self.eval_expr(expr, environment, depth + 1))?;
        let mut default_entry = None;
        for entry in node
            .named_children(&mut node.walk())
            .filter(|child| child.kind() == "switch_entry")
        {
            let children = entry.named_children(&mut entry.walk()).collect::<Vec<_>>();
            if children
                .iter()
                .any(|child| child.kind() == "default_keyword")
            {
                default_entry = Some(entry);
                continue;
            }
            for pattern in children
                .iter()
                .copied()
                .filter(|child| child.kind() == "switch_pattern")
            {
                let mut branch_environment = environment.child();
                if !self.switch_pattern_matches(pattern, &value, &mut branch_environment, depth + 1)
                {
                    continue;
                }
                if let Some(where_index) = children
                    .iter()
                    .position(|child| child.kind() == "where_keyword")
                {
                    let Some(condition) = children.get(where_index + 1) else {
                        continue;
                    };
                    if !self
                        .eval_expr(*condition, &branch_environment, depth + 1)
                        .is_some_and(|value| value.is_truthy())
                    {
                        continue;
                    }
                }
                let statements = children
                    .iter()
                    .copied()
                    .find(|child| child.kind() == "statements")?;
                return Some((statements, branch_environment));
            }
        }
        let entry = default_entry?;
        let statements = entry
            .named_children(&mut entry.walk())
            .find(|child| child.kind() == "statements")?;
        Some((statements, environment.child()))
    }

    fn switch_pattern_matches(
        &mut self,
        pattern: Node<'tree>,
        value: &EvalValue,
        environment: &mut Environment,
        depth: usize,
    ) -> bool {
        let source = self.text(pattern).trim().to_string();
        let source = source.as_str();
        if source == "_" {
            return true;
        }
        if let EvalValue::EnumCase {
            type_name,
            case_name,
            values,
            ..
        } = value
        {
            let mut source = source;
            let bind_all = source.starts_with("let ") || source.starts_with("var ");
            if bind_all {
                source = source
                    .split_once(char::is_whitespace)
                    .map(|(_, value)| value.trim())
                    .unwrap_or(source);
            }
            let (head, payload) = source
                .split_once('(')
                .map(|(head, payload)| (head.trim(), Some(payload.trim_end_matches(')').trim())))
                .unwrap_or((source, None));
            let head = head.trim_start_matches('.').trim();
            let parts = head
                .split('.')
                .map(str::trim)
                .filter(|part| !part.is_empty())
                .collect::<Vec<_>>();
            let pattern_case = parts.last().copied().unwrap_or_default();
            let pattern_type = (parts.len() >= 2).then(|| parts[parts.len() - 2]);
            if pattern_case != case_name
                || pattern_type.is_some_and(|pattern_type| pattern_type != type_name)
            {
                return false;
            }
            let Some(payload) = payload else {
                return values.is_empty();
            };
            let components = split_top_level(payload, ',');
            if components.len() != values.len() {
                return false;
            }
            for (component, associated_value) in components.into_iter().zip(values) {
                let component = component
                    .split_once(':')
                    .map(|(_, value)| value)
                    .unwrap_or(component)
                    .trim();
                let binds =
                    bind_all || component.starts_with("let ") || component.starts_with("var ");
                let component = component
                    .strip_prefix("let ")
                    .or_else(|| component.strip_prefix("var "))
                    .unwrap_or(component)
                    .trim();
                if component == "_" {
                    continue;
                }
                if binds && is_identifier(component) {
                    environment.define(component, associated_value.clone());
                    continue;
                }
                if !self.switch_scalar_pattern_matches(
                    component,
                    associated_value,
                    environment,
                    depth + 1,
                ) {
                    return false;
                }
            }
            return true;
        }
        if let Some(binding) = source
            .strip_prefix("let ")
            .or_else(|| source.strip_prefix("var "))
            .map(str::trim)
            .filter(|binding| is_identifier(binding))
        {
            environment.define(binding, value.clone());
            return true;
        }
        self.switch_scalar_pattern_matches(source, value, environment, depth + 1)
    }

    fn switch_scalar_pattern_matches(
        &mut self,
        source: &str,
        value: &EvalValue,
        environment: &Environment,
        _depth: usize,
    ) -> bool {
        if let Some((lower, upper)) = source
            .split_once("..<")
            .or_else(|| source.split_once("..."))
        {
            let inclusive = source.contains("...");
            let lower = parse_pattern_scalar(lower.trim())
                .or_else(|| environment.lookup(lower.trim()))
                .and_then(|value| value.as_f64());
            let upper = parse_pattern_scalar(upper.trim())
                .or_else(|| environment.lookup(upper.trim()))
                .and_then(|value| value.as_f64());
            let current = value.as_f64();
            return matches!((lower, upper, current), (Some(lower), Some(upper), Some(current))
                if current >= lower && (current < upper || (inclusive && current <= upper)));
        }
        let expected = parse_pattern_scalar(source).or_else(|| environment.lookup(source));
        expected.as_ref() == Some(value)
    }

    fn eval_for_flow(
        &mut self,
        node: Node<'tree>,
        environment: &Environment,
        depth: usize,
    ) -> EvalFlow<Vec<SidebarNode>> {
        let Some(sequence) = node
            .child_by_field_name("collection")
            .or_else(|| node.child_by_field_name("sequence"))
            .and_then(|sequence| self.eval_expr(sequence, environment, depth + 1))
        else {
            return EvalFlow::Normal(Vec::new());
        };
        let name = node
            .child_by_field_name("item")
            .or_else(|| node.child_by_field_name("pattern"))
            .map(|name| self.text(name).trim().to_string())
            .unwrap_or_else(|| "item".to_string());
        let Some(body) = node
            .child_by_field_name("body")
            .or_else(|| self.first_named_child(node, "statements"))
        else {
            return EvalFlow::Normal(Vec::new());
        };
        let mut nodes = Vec::new();
        for (index, value) in sequence.iteration_values().into_iter().enumerate() {
            let mut child_environment = environment.child();
            let identity = value.member("id").unwrap_or_else(|| value.clone());
            child_environment.push_identity("for", node.start_byte(), &identity);
            child_environment.define("$index", EvalValue::Int(index as i64));
            child_environment.define(&name, value);
            match self.eval_statements_flow(body, &mut child_environment, depth + 1) {
                EvalFlow::Normal(values) => nodes.extend(values),
                EvalFlow::Continue(values) => nodes.extend(values),
                EvalFlow::Break(values) => {
                    nodes.extend(values);
                    break;
                }
                EvalFlow::Return(values) => {
                    nodes.extend(values);
                    return EvalFlow::Return(nodes);
                }
            }
        }
        EvalFlow::Normal(nodes)
    }

    fn apply_binding(&mut self, node: Node<'tree>, environment: &mut Environment, depth: usize) {
        let name = node
            .child_by_field_name("name")
            .and_then(|name| self.last_identifier(name))
            .map(|name| self.text(name).to_string());
        let value = node
            .child_by_field_name("value")
            .and_then(|value| self.eval_expr(value, environment, depth + 1));
        if let (Some(name), Some(initial_value)) = (name, value) {
            if self.property_has_state_attribute(node) {
                let Some(key) = environment.state_key(&name, node.start_byte()) else {
                    return;
                };
                let existing = environment
                    .state
                    .borrow()
                    .get(&key)
                    .map(EvalValue::from_json);
                let existing =
                    existing.filter(|existing| state_value_types_match(existing, &initial_value));
                let value = existing.clone().unwrap_or_else(|| initial_value.clone());
                if existing.is_none() {
                    if let Some(json_value) = initial_value.to_json() {
                        environment
                            .state
                            .borrow_mut()
                            .insert(key.clone(), json_value);
                    }
                }
                environment.define_state(name, key, value);
            } else {
                environment.define(name, initial_value);
            }
        }
    }

    fn property_has_state_attribute(&self, node: Node<'tree>) -> bool {
        let mut cursor = node.walk();
        let mut stack = node.named_children(&mut cursor).collect::<Vec<_>>();
        while let Some(child) = stack.pop() {
            if child.kind() == "attribute"
                && child
                    .named_children(&mut child.walk())
                    .any(|value| self.text(value).trim() == "State")
            {
                return true;
            }
            stack.extend(child.named_children(&mut child.walk()));
        }
        false
    }

    fn property_name(&self, node: Node<'tree>) -> Option<String> {
        node.child_by_field_name("name")
            .and_then(|name| self.last_identifier(name))
            .map(|name| self.text(name).to_string())
    }

    fn eval_expr(
        &mut self,
        node: Node<'tree>,
        environment: &Environment,
        depth: usize,
    ) -> Option<EvalValue> {
        if depth > MAX_EXPRESSION_DEPTH {
            return None;
        }
        match node.kind() {
            "simple_identifier" | "type_identifier" => environment.lookup(self.text(node)),
            "self_expression" => environment.lookup("self"),
            "integer_literal" => self
                .text(node)
                .replace('_', "")
                .parse::<i64>()
                .ok()
                .map(EvalValue::Int),
            "real_literal" => self
                .text(node)
                .replace('_', "")
                .parse::<f64>()
                .ok()
                .filter(|value| value.is_finite())
                .map(EvalValue::Double),
            "boolean_literal" => Some(EvalValue::Bool(self.text(node) == "true")),
            "nil_literal" => Some(EvalValue::Optional(None)),
            "line_string_literal" | "multi_line_string_literal" => Some(EvalValue::String(
                self.eval_string(node, environment, depth + 1),
            )),
            "prefix_expression" => self.eval_prefix(node, environment, depth + 1),
            "navigation_expression" => self.eval_navigation(node, environment, depth + 1),
            "call_expression" => self.eval_value_call(node, environment, depth + 1),
            "ternary_expression" => {
                let condition = node
                    .child_by_field_name("condition")
                    .and_then(|value| self.eval_expr(value, environment, depth + 1))
                    .is_some_and(|value| value.is_truthy());
                node.child_by_field_name(if condition { "if_true" } else { "if_false" })
                    .and_then(|value| self.eval_expr(value, environment, depth + 1))
            }
            "nil_coalescing_expression" => {
                let value = node
                    .child_by_field_name("value")
                    .and_then(|value| self.eval_expr(value, environment, depth + 1))?;
                match value.optional_binding_value() {
                    Some(value) => Some(value),
                    None => node
                        .child_by_field_name("if_nil")
                        .and_then(|value| self.eval_expr(value, environment, depth + 1)),
                }
            }
            "array_literal" => {
                let mut cursor = node.walk();
                Some(EvalValue::Array(
                    node.named_children(&mut cursor)
                        .filter_map(|child| {
                            let value = if child.kind() == "array_literal_item" {
                                child.named_child(0).unwrap_or(child)
                            } else {
                                child
                            };
                            self.eval_expr(value, environment, depth + 1)
                        })
                        .collect(),
                ))
            }
            "tuple_expression" => node
                .child_by_field_name("value")
                .or_else(|| node.named_child(0))
                .and_then(|value| self.eval_expr(value, environment, depth + 1)),
            "additive_expression"
            | "multiplicative_expression"
            | "comparison_expression"
            | "equality_expression"
            | "conjunction_expression"
            | "disjunction_expression"
            | "infix_expression"
            | "range_expression" => self.eval_binary(node, environment, depth + 1),
            _ => node
                .named_child(0)
                .filter(|child| child.end_byte() <= node.end_byte())
                .and_then(|child| self.eval_expr(child, environment, depth + 1)),
        }
    }

    fn eval_navigation(
        &mut self,
        node: Node<'tree>,
        environment: &Environment,
        depth: usize,
    ) -> Option<EvalValue> {
        let target = node.child_by_field_name("target")?;
        let mut cursor = node.walk();
        let suffixes = node
            .named_children(&mut cursor)
            .filter(|child| child.kind() == "navigation_suffix")
            .collect::<Vec<_>>();
        let mut suffixes = suffixes.into_iter();
        let target_name = self.text(target).to_string();
        let mut value = if self.enums.contains_key(&target_name) {
            let case = suffixes
                .next()
                .and_then(|suffix| self.navigation_suffix(&suffix))?;
            self.construct_enum_case(&target_name, &case, &[], environment, depth + 1)?
        } else {
            self.eval_expr(target, environment, depth + 1)?
        };
        for suffix in suffixes {
            let member = self.navigation_suffix(&suffix)?;
            if self.navigation_suffix_is_optional(&suffix) {
                let Some(unwrapped) = value.optional_binding_value() else {
                    return Some(EvalValue::Optional(None));
                };
                value = EvalValue::optional(unwrapped.member(&member));
            } else {
                value = value.member(&member)?;
            }
        }
        Some(value)
    }

    fn eval_prefix(
        &mut self,
        node: Node<'tree>,
        environment: &Environment,
        depth: usize,
    ) -> Option<EvalValue> {
        let source = self.text(node).trim().to_string();
        let target = node
            .child_by_field_name("target")
            .or_else(|| node.named_child(0));
        if source.starts_with('.') {
            return Some(EvalValue::String(
                source.trim_start_matches('.').to_string(),
            ));
        }
        let value = target.and_then(|target| self.eval_expr(target, environment, depth + 1))?;
        if source.starts_with('!') {
            return Some(EvalValue::Bool(!value.is_truthy()));
        }
        if source.starts_with('-') {
            return match value {
                EvalValue::Int(value) => Some(EvalValue::Int(-value)),
                EvalValue::Double(value) => Some(EvalValue::Double(-value)),
                _ => None,
            };
        }
        Some(value)
    }

    fn eval_binary(
        &mut self,
        node: Node<'tree>,
        environment: &Environment,
        depth: usize,
    ) -> Option<EvalValue> {
        let lhs_node = node
            .child_by_field_name("lhs")
            .or_else(|| node.named_child(0))?;
        let rhs_node = node
            .child_by_field_name("rhs")
            .or_else(|| node.named_child(node.named_child_count().saturating_sub(1) as u32))?;
        let lhs = self.eval_expr(lhs_node, environment, depth + 1)?;
        let operator = self.source[lhs_node.end_byte()..rhs_node.start_byte()]
            .trim()
            .trim_matches(|character: char| character.is_whitespace())
            .to_string();
        if operator == "&&" && !lhs.is_truthy() {
            return Some(EvalValue::Bool(false));
        }
        if operator == "||" && lhs.is_truthy() {
            return Some(EvalValue::Bool(true));
        }
        let rhs = self.eval_expr(rhs_node, environment, depth + 1)?;
        match operator.as_str() {
            "&&" => Some(EvalValue::Bool(lhs.is_truthy() && rhs.is_truthy())),
            "||" => Some(EvalValue::Bool(lhs.is_truthy() || rhs.is_truthy())),
            "==" => Some(EvalValue::Bool(lhs == rhs)),
            "!=" => Some(EvalValue::Bool(lhs != rhs)),
            "..<" | "..." => match (lhs, rhs) {
                (EvalValue::Int(lower), EvalValue::Int(upper)) => {
                    Some(EvalValue::Range(lower, upper, operator == "..."))
                }
                (lhs, rhs) => Some(EvalValue::NumericRange(
                    lhs.as_f64()?,
                    rhs.as_f64()?,
                    operator == "...",
                )),
            },
            "+" => match (lhs, rhs) {
                (EvalValue::String(lhs), EvalValue::String(rhs)) => {
                    Some(EvalValue::String(lhs + &rhs))
                }
                (EvalValue::Int(lhs), EvalValue::Int(rhs)) => Some(EvalValue::Int(lhs + rhs)),
                (lhs, rhs) => Some(EvalValue::Double(lhs.as_f64()? + rhs.as_f64()?)),
            },
            "-" => match (&lhs, &rhs) {
                (EvalValue::Int(lhs), EvalValue::Int(rhs)) => Some(EvalValue::Int(lhs - rhs)),
                _ => Some(EvalValue::Double(lhs.as_f64()? - rhs.as_f64()?)),
            },
            "*" => match (&lhs, &rhs) {
                (EvalValue::Int(lhs), EvalValue::Int(rhs)) => Some(EvalValue::Int(lhs * rhs)),
                _ => Some(EvalValue::Double(lhs.as_f64()? * rhs.as_f64()?)),
            },
            "/" => match (&lhs, &rhs) {
                (_, EvalValue::Int(0)) => None,
                (EvalValue::Int(lhs), EvalValue::Int(rhs)) => Some(EvalValue::Int(lhs / rhs)),
                _ => {
                    let rhs = rhs.as_f64()?;
                    (rhs != 0.0).then(|| EvalValue::Double(lhs.as_f64().unwrap() / rhs))
                }
            },
            "%" => match (lhs, rhs) {
                (EvalValue::Int(_), EvalValue::Int(0)) => None,
                (EvalValue::Int(lhs), EvalValue::Int(rhs)) => Some(EvalValue::Int(lhs % rhs)),
                _ => None,
            },
            "<" | ">" | "<=" | ">=" => {
                let lhs = lhs.as_f64()?;
                let rhs = rhs.as_f64()?;
                Some(EvalValue::Bool(match operator.as_str() {
                    "<" => lhs < rhs,
                    ">" => lhs > rhs,
                    "<=" => lhs <= rhs,
                    _ => lhs >= rhs,
                }))
            }
            _ => None,
        }
    }

    fn eval_value_call(
        &mut self,
        call: Node<'tree>,
        environment: &Environment,
        depth: usize,
    ) -> Option<EvalValue> {
        let callee = call.named_child(0)?;
        let args = self.call_arguments(call);
        if callee.kind() == "navigation_expression" {
            let target = callee.child_by_field_name("target")?;
            let target_name = self.text(target).to_string();
            if self.enums.contains_key(&target_name) {
                let case = self.navigation_suffix(&callee)?;
                return self.construct_enum_case(
                    &target_name,
                    &case,
                    &args,
                    environment,
                    depth + 1,
                );
            }
            let base = self.eval_expr(target, environment, depth + 1)?;
            let method = self.navigation_suffix(&callee)?;
            if self.navigation_suffix_is_optional(&callee) {
                let Some(base) = base.optional_binding_value() else {
                    return Some(EvalValue::Optional(None));
                };
                return self
                    .eval_method(
                        base,
                        &method,
                        &args,
                        self.call_closure(call),
                        environment,
                        depth + 1,
                    )
                    .map(|value| EvalValue::optional(Some(value)));
            }
            return self.eval_method(
                base,
                &method,
                &args,
                self.call_closure(call),
                environment,
                depth + 1,
            );
        }
        let name = self.call_name(call)?;
        match name.as_str() {
            "Color" => {
                if let Some(value) = args
                    .first()
                    .and_then(|arg| self.eval_expr(arg.value, environment, depth + 1))
                {
                    return Some(value);
                }
                let red = self.numeric_argument(&args, "red", environment, depth + 1)?;
                let green = self.numeric_argument(&args, "green", environment, depth + 1)?;
                let blue = self.numeric_argument(&args, "blue", environment, depth + 1)?;
                Some(EvalValue::String(format!(
                    "#{:02X}{:02X}{:02X}",
                    color_channel(red),
                    color_channel(green),
                    color_channel(blue)
                )))
            }
            "Int" => args
                .first()
                .and_then(|arg| self.eval_expr(arg.value, environment, depth + 1))
                .and_then(|value| match value {
                    EvalValue::Int(value) => Some(EvalValue::Int(value)),
                    EvalValue::Double(value) => Some(EvalValue::Int(value as i64)),
                    EvalValue::String(value) => value.parse::<i64>().ok().map(EvalValue::Int),
                    _ => None,
                }),
            "Double" => args
                .first()
                .and_then(|arg| self.eval_expr(arg.value, environment, depth + 1))
                .and_then(|value| value.as_f64().map(EvalValue::Double)),
            "String" | "Array" => args
                .first()
                .and_then(|arg| self.eval_expr(arg.value, environment, depth + 1))
                .map(|value| {
                    if name == "String" {
                        EvalValue::String(value.display_string())
                    } else {
                        value
                    }
                }),
            "Optional" => args
                .first()
                .and_then(|arg| self.eval_expr(arg.value, environment, depth + 1))
                .map(|value| EvalValue::optional(Some(value))),
            "min" | "max" => {
                let numbers = args
                    .iter()
                    .filter_map(|arg| self.eval_expr(arg.value, environment, depth + 1))
                    .filter_map(|value| value.as_f64())
                    .collect::<Vec<_>>();
                let selected = if name == "min" {
                    numbers.into_iter().reduce(f64::min)
                } else {
                    numbers.into_iter().reduce(f64::max)
                }?;
                Some(EvalValue::Double(selected))
            }
            "abs" => args
                .first()
                .and_then(|arg| self.eval_expr(arg.value, environment, depth + 1))
                .and_then(|value| value.as_f64())
                .map(|value| EvalValue::Double(value.abs())),
            _ => self.eval_user_value_function(&name, call, environment, depth + 1),
        }
    }

    fn construct_enum_case(
        &mut self,
        type_name: &str,
        case_name: &str,
        args: &[CallArgument<'tree>],
        environment: &Environment,
        depth: usize,
    ) -> Option<EvalValue> {
        let definition = self.enums.get(type_name)?.clone();
        let case = definition.cases.get(case_name)?.clone();
        if args.len() != case.associated_labels.len() {
            return None;
        }
        if args
            .iter()
            .zip(&case.associated_labels)
            .any(|(argument, expected)| {
                expected
                    .as_deref()
                    .is_some_and(|expected| argument.label.as_deref() != Some(expected))
            })
        {
            return None;
        }
        let values = args
            .iter()
            .map(|argument| self.eval_expr(argument.value, environment, depth + 1))
            .collect::<Option<Vec<_>>>()?;
        let raw_value = case
            .raw_value
            .and_then(|value| self.eval_expr(value, environment, depth + 1))
            .or_else(|| {
                (definition.raw_type.as_deref() == Some("String"))
                    .then(|| EvalValue::String(case_name.to_string()))
            })
            .map(Box::new);
        Some(EvalValue::EnumCase {
            type_name: type_name.to_string(),
            case_name: case_name.to_string(),
            values,
            labels: case.associated_labels,
            raw_value,
        })
    }

    fn eval_method(
        &mut self,
        base: EvalValue,
        method: &str,
        args: &[CallArgument<'tree>],
        closure: Option<Node<'tree>>,
        environment: &Environment,
        depth: usize,
    ) -> Option<EvalValue> {
        match base {
            EvalValue::Optional(None) | EvalValue::Null => Some(EvalValue::Optional(None)),
            EvalValue::Optional(Some(value)) => self
                .eval_method(*value, method, args, closure, environment, depth + 1)
                .map(|value| EvalValue::optional(Some(value))),
            EvalValue::Array(values) => match method {
                "prefix" => {
                    let count = args
                        .first()
                        .and_then(|arg| self.eval_expr(arg.value, environment, depth + 1))
                        .and_then(|value| match value {
                            EvalValue::Int(value) => Some(value.max(0) as usize),
                            _ => None,
                        })?;
                    Some(EvalValue::Array(values.into_iter().take(count).collect()))
                }
                "filter" | "map" | "sorted" => {
                    let closure = closure?;
                    let parameters = self.lambda_parameters(closure);
                    if method == "sorted" {
                        let mut result = values;
                        result.sort_by(|left, right| {
                            let mut child_environment = environment.child();
                            child_environment.define("$0", left.clone());
                            child_environment.define("$1", right.clone());
                            if let Some(name) = parameters.first() {
                                child_environment.define(name, left.clone());
                            }
                            if let Some(name) = parameters.get(1) {
                                child_environment.define(name, right.clone());
                            }
                            if self
                                .eval_lambda_value(closure, &child_environment, depth + 1)
                                .is_some_and(|value| value.is_truthy())
                            {
                                std::cmp::Ordering::Less
                            } else {
                                std::cmp::Ordering::Greater
                            }
                        });
                        return Some(EvalValue::Array(result));
                    }
                    let mut result = Vec::new();
                    for value in values {
                        let mut child_environment = environment.child();
                        child_environment.define("$0", value.clone());
                        if let Some(name) = parameters.first() {
                            child_environment.define(name, value.clone());
                        }
                        let mapped = self.eval_lambda_value(closure, &child_environment, depth + 1);
                        if method == "filter" {
                            if mapped.is_some_and(|value| value.is_truthy()) {
                                result.push(value);
                            }
                        } else if let Some(mapped) = mapped {
                            result.push(mapped);
                        }
                    }
                    Some(EvalValue::Array(result))
                }
                "contains" => {
                    let needle = args
                        .first()
                        .and_then(|arg| self.eval_expr(arg.value, environment, depth + 1))?;
                    Some(EvalValue::Bool(values.contains(&needle)))
                }
                "reversed" => Some(EvalValue::Array(values.into_iter().rev().collect())),
                "enumerated" => Some(EvalValue::Array(
                    values
                        .into_iter()
                        .enumerate()
                        .map(|(index, value)| {
                            EvalValue::Object(HashMap::from([
                                ("offset".to_string(), EvalValue::Int(index as i64)),
                                ("element".to_string(), value.clone()),
                                ("0".to_string(), EvalValue::Int(index as i64)),
                                ("1".to_string(), value),
                            ]))
                        })
                        .collect(),
                )),
                _ => EvalValue::Array(values).member(method),
            },
            EvalValue::String(value) => {
                let mut argument = || {
                    args.first()
                        .and_then(|arg| self.eval_expr(arg.value, environment, depth + 1))
                        .map(|value| value.display_string())
                };
                match method {
                    "hasPrefix" => Some(EvalValue::Bool(value.starts_with(&argument()?))),
                    "hasSuffix" => Some(EvalValue::Bool(value.ends_with(&argument()?))),
                    "contains" => Some(EvalValue::Bool(value.contains(&argument()?))),
                    "uppercased" => Some(EvalValue::String(value.to_uppercase())),
                    "lowercased" => Some(EvalValue::String(value.to_lowercase())),
                    "capitalized" => Some(EvalValue::String(capitalize(&value))),
                    _ => EvalValue::String(value).member(method),
                }
            }
            value => value.member(method),
        }
    }

    fn eval_user_value_function(
        &mut self,
        name: &str,
        call: Node<'tree>,
        environment: &Environment,
        depth: usize,
    ) -> Option<EvalValue> {
        if depth > MAX_EXPRESSION_DEPTH || self.function_calls >= MAX_FUNCTION_CALLS {
            return None;
        }
        let definition = *self.functions.get(name)?;
        self.function_calls += 1;
        let args = self.call_arguments(call);
        let parameters = self.function_parameters(definition);
        let mut child_environment = environment.child();
        for (index, parameter) in parameters.iter().enumerate() {
            if let Some(value) = args
                .get(index)
                .and_then(|arg| self.eval_expr(arg.value, environment, depth + 1))
            {
                child_environment.define(parameter, value);
            }
        }
        let value = self.eval_block_value(definition.body, &mut child_environment, depth + 1);
        self.function_calls = self.function_calls.saturating_sub(1);
        value
    }

    fn eval_block_value(
        &mut self,
        body: Node<'tree>,
        environment: &mut Environment,
        depth: usize,
    ) -> Option<EvalValue> {
        let statements = if body.kind() == "statements" {
            body
        } else {
            self.first_named_child(body, "statements").unwrap_or(body)
        };
        let mut cursor = statements.walk();
        let mut last = None;
        for child in statements.named_children(&mut cursor) {
            match child.kind() {
                "property_declaration" => self.apply_binding(child, environment, depth + 1),
                "control_transfer_statement" => {
                    return child
                        .child_by_field_name("result")
                        .and_then(|result| self.eval_expr(result, environment, depth + 1));
                }
                "if_statement" => {
                    let condition = child
                        .child_by_field_name("condition")
                        .and_then(|condition| self.eval_expr(condition, environment, depth + 1))
                        .is_some_and(|value| value.is_truthy());
                    let mut branch_cursor = child.walk();
                    let branches = child
                        .named_children(&mut branch_cursor)
                        .filter(|candidate| candidate.kind() == "statements")
                        .collect::<Vec<_>>();
                    if let Some(branch) = if condition {
                        branches.first()
                    } else {
                        branches.get(1)
                    } {
                        let mut branch_environment = environment.child();
                        if let Some(value) =
                            self.eval_block_value(*branch, &mut branch_environment, depth + 1)
                        {
                            return Some(value);
                        }
                    }
                }
                "switch_statement" => {
                    if let Some((branch, mut branch_environment)) =
                        self.select_switch_branch(child, environment, depth + 1)
                    {
                        if let Some(value) =
                            self.eval_block_value(branch, &mut branch_environment, depth + 1)
                        {
                            return Some(value);
                        }
                    }
                }
                _ => last = self.eval_expr(child, environment, depth + 1),
            }
        }
        last
    }

    fn eval_lambda_value(
        &mut self,
        closure: Node<'tree>,
        environment: &Environment,
        depth: usize,
    ) -> Option<EvalValue> {
        let mut child_environment = environment.child();
        self.eval_block_value(closure, &mut child_environment, depth + 1)
    }

    fn eval_string(
        &mut self,
        node: Node<'tree>,
        environment: &Environment,
        depth: usize,
    ) -> String {
        let mut cursor = node.walk();
        let mut output = String::new();
        for child in node.named_children(&mut cursor) {
            match child.kind() {
                "line_str_text" | "multi_line_str_text" => output.push_str(self.text(child)),
                "interpolated_expression" => {
                    if let Some(value) = child
                        .child_by_field_name("value")
                        .or_else(|| child.named_child(0))
                        .and_then(|value| self.eval_expr(value, environment, depth + 1))
                    {
                        output.push_str(&value.display_string());
                    }
                }
                _ => {}
            }
        }
        if output.is_empty() {
            self.text(node)
                .trim()
                .trim_start_matches('#')
                .trim_matches('"')
                .to_string()
        } else {
            output
        }
    }

    fn parse_action(
        &mut self,
        closure: Node<'tree>,
        environment: &Environment,
        depth: usize,
    ) -> SidebarAction {
        let mut commands = Vec::new();
        let statements = self
            .first_named_child(closure, "statements")
            .unwrap_or(closure);
        let mut cursor = statements.walk();
        for child in statements.named_children(&mut cursor) {
            if child.kind() == "switch_statement" {
                if let Some((branch, branch_environment)) =
                    self.select_switch_branch(child, environment, depth + 1)
                {
                    commands.extend(
                        self.parse_action(branch, &branch_environment, depth + 1)
                            .commands,
                    );
                }
                continue;
            }
            if child.kind() == "assignment" {
                if let Some(command) = self.parse_state_assignment(child, environment, depth + 1) {
                    commands.push(command);
                }
                continue;
            }
            if child.kind() != "call_expression" {
                continue;
            }
            if let Some(command) = self.parse_state_method(child, environment, depth + 1) {
                commands.push(command);
                continue;
            }
            let Some(name) = self.call_name(child) else {
                continue;
            };
            let args = self.call_arguments(child);
            match name.as_str() {
                "cmux" => {
                    let Some(method) = args
                        .first()
                        .and_then(|arg| self.eval_expr(arg.value, environment, depth + 1))
                        .map(|value| value.display_string())
                    else {
                        continue;
                    };
                    let params = args
                        .iter()
                        .skip(1)
                        .filter_map(|arg| {
                            Some((
                                arg.label.clone()?,
                                self.eval_expr(arg.value, environment, depth + 1)?
                                    .display_string(),
                            ))
                        })
                        .collect();
                    commands.push(SidebarActionCommand {
                        kind: "cmux".to_string(),
                        method: Some(method),
                        params,
                        message: None,
                        operation: None,
                        key: None,
                        value: None,
                    });
                }
                "log" | "openURL" => {
                    let message = args
                        .first()
                        .and_then(|arg| self.eval_expr(arg.value, environment, depth + 1))
                        .map(|value| value.display_string())
                        .unwrap_or_default();
                    commands.push(SidebarActionCommand {
                        kind: name,
                        method: None,
                        params: HashMap::new(),
                        message: Some(message),
                        operation: None,
                        key: None,
                        value: None,
                    });
                }
                _ => {}
            }
        }
        SidebarAction {
            kind: commands
                .first()
                .map(|command| command.kind.clone())
                .unwrap_or_default(),
            message: commands.first().and_then(|command| command.message.clone()),
            params: HashMap::new(),
            commands,
        }
    }

    fn parse_state_assignment(
        &mut self,
        assignment: Node<'tree>,
        environment: &Environment,
        depth: usize,
    ) -> Option<SidebarActionCommand> {
        let target = assignment.child_by_field_name("target")?;
        let key = self.state_key_for_target(target, environment)?;
        let result = assignment.child_by_field_name("result")?;
        let mut value = self.eval_expr(result, environment, depth + 1)?.to_json()?;
        let operator = self.source[target.end_byte()..result.start_byte()].trim();
        let operation = match operator {
            "=" => "set",
            "+=" => "add",
            "-=" => {
                value = match value {
                    Value::Number(number) => number
                        .as_i64()
                        .and_then(i64::checked_neg)
                        .map(Number::from)
                        .or_else(|| number.as_f64().and_then(|value| Number::from_f64(-value)))
                        .map(Value::Number)?,
                    _ => return None,
                };
                "add"
            }
            _ => return None,
        };
        Some(SidebarActionCommand {
            kind: "state".to_string(),
            operation: Some(operation.to_string()),
            key: Some(key),
            value: Some(value),
            ..SidebarActionCommand::default()
        })
    }

    fn parse_state_method(
        &mut self,
        call: Node<'tree>,
        environment: &Environment,
        depth: usize,
    ) -> Option<SidebarActionCommand> {
        let callee = call.named_child(0)?;
        if callee.kind() != "navigation_expression" {
            return None;
        }
        let target = callee.child_by_field_name("target")?;
        let key = self.state_key_for_target(target, environment)?;
        let operation = self.navigation_suffix(&callee)?;
        match operation.as_str() {
            "toggle" => Some(SidebarActionCommand {
                kind: "state".to_string(),
                operation: Some("toggle".to_string()),
                key: Some(key),
                ..SidebarActionCommand::default()
            }),
            "append" => {
                let value = self
                    .call_arguments(call)
                    .first()
                    .and_then(|arg| self.eval_expr(arg.value, environment, depth + 1))
                    .and_then(|value| value.to_json())?;
                Some(SidebarActionCommand {
                    kind: "state".to_string(),
                    operation: Some("append".to_string()),
                    key: Some(key),
                    value: Some(value),
                    ..SidebarActionCommand::default()
                })
            }
            _ => None,
        }
    }

    fn state_key_for_target(
        &self,
        target: Node<'tree>,
        environment: &Environment,
    ) -> Option<String> {
        let name = self
            .text(target)
            .trim()
            .trim_start_matches('$')
            .trim()
            .to_string();
        environment.state_bindings.get(&name).cloned()
    }

    fn apply_modifier(
        &mut self,
        node: &mut SidebarNode,
        name: &str,
        call: Node<'tree>,
        environment: &Environment,
        depth: usize,
    ) {
        let args = self.call_arguments(call);
        let first = args
            .first()
            .and_then(|arg| self.eval_expr(arg.value, environment, depth + 1));
        match name {
            "padding" => {
                node.padding = first.and_then(|value| value.as_f64()).or(Some(8.0));
            }
            "fontWeight" => node.weight = first.map(|value| value.display_string()),
            "bold" => node.weight = Some("bold".to_string()),
            "font" => {
                let source = args
                    .first()
                    .map(|arg| self.text(arg.value).trim().to_string())
                    .unwrap_or_default();
                if let Some(size) = named_number_from_source(&source, "size") {
                    node.size = Some(size);
                } else {
                    node.font = first.map(|value| value.display_string()).or_else(|| {
                        source
                            .trim_start_matches('.')
                            .split(['(', '.'])
                            .next()
                            .map(str::to_string)
                    });
                }
            }
            "foregroundColor" | "foregroundStyle" | "tint" | "fill" => {
                let color = first
                    .map(|value| value.display_string())
                    .or_else(|| args.first().map(|arg| token_text(self.text(arg.value))));
                if node.kind == SidebarNodeKind::Shape && name == "fill" {
                    node.background = color;
                } else {
                    node.color = color;
                }
            }
            "background" => {
                if let Some(color) = first.map(|value| value.display_string()) {
                    node.background = Some(color);
                } else if let Some(closure) = self.call_closure(call) {
                    let mut child_environment = environment.child();
                    if let Some(background) = self
                        .eval_statements(closure, &mut child_environment, depth + 1)
                        .into_iter()
                        .next()
                    {
                        node.background = background.color.or(background.background);
                        node.corner_radius = node.corner_radius.or(background.corner_radius);
                    }
                }
            }
            "opacity" => node.opacity = first.and_then(|value| value.as_f64()),
            "frame" => {
                node.width = self.numeric_argument(&args, "width", environment, depth + 1);
                node.height = self.numeric_argument(&args, "height", environment, depth + 1);
            }
            "cornerRadius" => node.corner_radius = first.and_then(|value| value.as_f64()),
            "tag" => node.tag = first.and_then(|value| value.to_json()),
            "onChange" => {
                let Some(closure) = self.call_closure(call) else {
                    return;
                };
                let Some(watched) = self
                    .argument(&args, "of")
                    .or_else(|| args.first().map(|arg| arg.value))
                else {
                    return;
                };
                let Some(key) = self.state_key_for_target(watched, environment) else {
                    return;
                };
                let current = self
                    .eval_expr(watched, environment, depth + 1)
                    .unwrap_or(EvalValue::Null);
                let id = self.event_id("change", call);
                let (old_value, new_value) = self
                    .event
                    .as_ref()
                    .filter(|event| event.id == id)
                    .map(|event| {
                        (
                            EvalValue::from_json(&event.old_value),
                            EvalValue::from_json(&event.new_value),
                        )
                    })
                    .unwrap_or_else(|| (current.clone(), current));
                let mut action_environment = environment.child();
                let parameters = self.lambda_parameters(closure);
                if parameters.len() == 1 {
                    action_environment.define(&parameters[0], new_value);
                } else if parameters.len() >= 2 {
                    action_environment.define(&parameters[0], old_value);
                    action_environment.define(&parameters[1], new_value);
                }
                let action = self.parse_action(closure, &action_environment, depth + 1);
                if !action.commands.is_empty() {
                    node.on_change.push(SidebarEvent {
                        id,
                        key: Some(key),
                        action,
                    });
                }
            }
            "onSubmit" => {
                let Some(closure) = self.call_closure(call) else {
                    return;
                };
                let action = self.parse_action(closure, environment, depth + 1);
                if !action.commands.is_empty() {
                    node.on_submit.push(SidebarEvent {
                        id: self.event_id("submit", call),
                        key: node.binding.as_ref().map(|binding| binding.key.clone()),
                        action,
                    });
                }
            }
            "onTapGesture" => {
                if let Some(closure) = self.call_closure(call) {
                    let action = self.parse_action(closure, environment, depth + 1);
                    if !action.commands.is_empty() {
                        node.action = Some(action);
                    }
                }
            }
            _ => {}
        }
    }

    fn call_name(&self, call: Node<'tree>) -> Option<String> {
        let callee = call.named_child(0)?;
        match callee.kind() {
            "simple_identifier" | "type_identifier" => Some(self.text(callee).to_string()),
            "prefix_expression" => {
                Some(self.text(callee).trim().trim_start_matches('.').to_string())
            }
            "navigation_expression" => self.navigation_suffix(&callee),
            _ => None,
        }
    }

    fn event_id(&self, kind: &str, call: Node<'tree>) -> String {
        format!("{kind}:{}:{}", call.start_byte(), call.end_byte())
    }

    fn call_arguments(&self, call: Node<'tree>) -> Vec<CallArgument<'tree>> {
        let Some(suffix) = call
            .named_children(&mut call.walk())
            .find(|child| child.kind() == "call_suffix")
        else {
            return Vec::new();
        };
        let Some(arguments) = suffix
            .named_children(&mut suffix.walk())
            .find(|child| child.kind() == "value_arguments")
        else {
            return Vec::new();
        };
        arguments
            .named_children(&mut arguments.walk())
            .filter(|child| child.kind() == "value_argument")
            .filter_map(|argument| {
                let value = argument.child_by_field_name("value").or_else(|| {
                    argument.named_child(argument.named_child_count().saturating_sub(1) as u32)
                })?;
                let label = argument
                    .child_by_field_name("name")
                    .and_then(|name| self.last_identifier(name))
                    .map(|name| self.text(name).to_string());
                Some(CallArgument { label, value })
            })
            .collect()
    }

    fn call_closure(&self, call: Node<'tree>) -> Option<Node<'tree>> {
        let suffix = call
            .named_children(&mut call.walk())
            .find(|child| child.kind() == "call_suffix")?;
        suffix
            .named_children(&mut suffix.walk())
            .find(|child| child.kind() == "lambda_literal")
    }

    fn navigation_suffix(&self, node: &Node<'tree>) -> Option<String> {
        let mut cursor = node.walk();
        let suffix = if node.kind() == "navigation_suffix" {
            *node
        } else {
            node.named_children(&mut cursor)
                .filter(|child| child.kind() == "navigation_suffix")
                .last()?
        };
        self.last_identifier(suffix)
            .map(|identifier| self.text(identifier).to_string())
    }

    fn navigation_suffix_is_optional(&self, node: &Node<'tree>) -> bool {
        let mut cursor = node.walk();
        let suffix = if node.kind() == "navigation_suffix" {
            Some(*node)
        } else {
            node.named_children(&mut cursor)
                .filter(|child| child.kind() == "navigation_suffix")
                .last()
        };
        suffix.is_some_and(|suffix| self.text(suffix).contains('?'))
    }

    fn lambda_parameters(&self, closure: Node<'tree>) -> Vec<String> {
        let mut cursor = closure.walk();
        let mut stack = vec![closure];
        let mut parameters = Vec::new();
        while let Some(node) = stack.pop() {
            if node.kind() == "statements" {
                continue;
            }
            if node.kind() == "lambda_parameter" {
                if let Some(name) = node
                    .child_by_field_name("name")
                    .or_else(|| self.last_identifier(node))
                {
                    parameters.push((name.start_byte(), self.text(name).to_string()));
                }
                continue;
            }
            stack.extend(node.named_children(&mut cursor));
        }
        parameters.sort_by_key(|(offset, _)| *offset);
        parameters
            .into_iter()
            .map(|(_, parameter)| parameter)
            .collect()
    }

    fn function_parameters(&self, definition: FunctionDefinition<'tree>) -> Vec<String> {
        let header = &self.source[definition.node.start_byte()..definition.body.start_byte()];
        let Some(open) = header.find('(') else {
            return Vec::new();
        };
        let Some(close) = header.rfind(')') else {
            return Vec::new();
        };
        split_top_level(&header[open + 1..close], ',')
            .into_iter()
            .filter_map(|parameter| {
                let left = parameter.split(':').next()?.trim();
                left.split_whitespace()
                    .rev()
                    .find(|token| *token != "_")
                    .map(|token| {
                        token.trim_matches(|character: char| !is_identifier_char(character))
                    })
                    .filter(|token| !token.is_empty())
                    .map(str::to_string)
            })
            .collect()
    }

    fn argument<'args>(
        &self,
        args: &'args [CallArgument<'tree>],
        label: &str,
    ) -> Option<Node<'tree>> {
        args.iter()
            .find(|arg| arg.label.as_deref() == Some(label))
            .map(|arg| arg.value)
    }

    fn numeric_argument(
        &mut self,
        args: &[CallArgument<'tree>],
        label: &str,
        environment: &Environment,
        depth: usize,
    ) -> Option<f64> {
        self.argument(args, label)
            .and_then(|value| self.eval_expr(value, environment, depth + 1))
            .and_then(|value| value.as_f64())
    }

    fn range_argument(
        &mut self,
        args: &[CallArgument<'tree>],
        label: &str,
        environment: &Environment,
        depth: usize,
    ) -> Option<(f64, f64)> {
        match self
            .argument(args, label)
            .and_then(|value| self.eval_expr(value, environment, depth + 1))?
        {
            EvalValue::Range(lower, upper, inclusive) => {
                let upper = if inclusive {
                    upper as f64
                } else {
                    (upper - 1) as f64
                };
                Some((lower as f64, upper))
            }
            EvalValue::NumericRange(lower, upper, _) => Some((lower, upper)),
            _ => None,
        }
    }

    fn token_argument(
        &mut self,
        args: &[CallArgument<'tree>],
        label: &str,
        environment: &Environment,
        depth: usize,
    ) -> Option<String> {
        self.argument(args, label)
            .and_then(|value| {
                self.eval_expr(value, environment, depth + 1)
                    .map(|value| value.display_string())
                    .or_else(|| Some(token_text(self.text(value))))
            })
            .filter(|value| !value.is_empty())
    }

    fn first_named_child(&self, node: Node<'tree>, kind: &str) -> Option<Node<'tree>> {
        node.named_children(&mut node.walk())
            .find(|child| child.kind() == kind)
    }

    fn last_identifier(&self, node: Node<'tree>) -> Option<Node<'tree>> {
        if matches!(node.kind(), "simple_identifier" | "type_identifier") {
            return Some(node);
        }
        let mut cursor = node.walk();
        node.named_children(&mut cursor)
            .filter_map(|child| self.last_identifier(child))
            .last()
    }

    fn record_node(&mut self) {
        self.produced_nodes = self.produced_nodes.saturating_add(1);
    }

    fn text(&self, node: Node<'tree>) -> &str {
        &self.source[node.start_byte()..node.end_byte()]
    }
}

#[derive(Clone)]
struct CallArgument<'tree> {
    label: Option<String>,
    value: Node<'tree>,
}

fn display_number(value: f64) -> String {
    if value.is_finite() && value.fract() == 0.0 {
        format!("{value:.0}")
    } else {
        value.to_string()
    }
}

fn identity_value_bytes(value: &EvalValue) -> Vec<u8> {
    value
        .to_json()
        .and_then(|value| serde_json::to_vec(&value).ok())
        .unwrap_or_else(|| value.display_string().into_bytes())
}

fn stable_identity_hash(bytes: &[u8]) -> u64 {
    let mut hash = 0xcbf29ce484222325u64;
    for byte in bytes {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x100000001b3);
    }
    hash
}

fn is_picker_json_value(value: &Value) -> bool {
    value.is_boolean()
        || value.is_number()
        || value.is_string()
        || value
            .as_object()
            .is_some_and(|value| value.contains_key(ENUM_TYPE_KEY))
}

fn sidebar_option_label(node: &SidebarNode) -> Option<String> {
    node.text
        .as_deref()
        .or(node.title.as_deref())
        .filter(|label| !label.trim().is_empty())
        .map(str::to_string)
        .or_else(|| node.children.iter().find_map(sidebar_option_label))
}

fn token_text(source: &str) -> String {
    source
        .trim()
        .trim_start_matches('.')
        .trim_matches(|character: char| matches!(character, '(' | ')' | ' '))
        .to_string()
}

fn named_number_from_source(source: &str, label: &str) -> Option<f64> {
    let marker = format!("{label}:");
    let start = source.find(&marker)? + marker.len();
    let tail = source[start..].trim_start();
    let length = tail
        .find(|character: char| !(character.is_ascii_digit() || matches!(character, '.' | '-')))
        .unwrap_or(tail.len());
    tail[..length]
        .parse::<f64>()
        .ok()
        .filter(|value| value.is_finite())
}

fn color_channel(value: f64) -> u8 {
    (value.clamp(0.0, 1.0) * 255.0).round() as u8
}

fn state_value_types_match(left: &EvalValue, right: &EvalValue) -> bool {
    match (left, right) {
        (EvalValue::Null, EvalValue::Null)
        | (EvalValue::Bool(_), EvalValue::Bool(_))
        | (EvalValue::Int(_) | EvalValue::Double(_), EvalValue::Int(_) | EvalValue::Double(_))
        | (EvalValue::String(_), EvalValue::String(_))
        | (EvalValue::Array(_), EvalValue::Array(_))
        | (EvalValue::Object(_), EvalValue::Object(_)) => true,
        (
            EvalValue::EnumCase {
                type_name: left, ..
            },
            EvalValue::EnumCase {
                type_name: right, ..
            },
        ) => left == right,
        _ => false,
    }
}

fn capitalize(value: &str) -> String {
    value
        .split_whitespace()
        .map(|part| {
            let mut chars = part.chars();
            chars
                .next()
                .map(|first| first.to_uppercase().collect::<String>() + chars.as_str())
                .unwrap_or_default()
        })
        .collect::<Vec<_>>()
        .join(" ")
}

fn split_top_level(source: &str, separator: char) -> Vec<&str> {
    let mut depth = 0i32;
    let mut in_string = false;
    let mut escaped = false;
    let mut start = 0usize;
    let mut parts = Vec::new();
    for (index, character) in source.char_indices() {
        if in_string {
            if escaped {
                escaped = false;
            } else if character == '\\' {
                escaped = true;
            } else if character == '"' {
                in_string = false;
            }
            continue;
        }
        match character {
            '"' => in_string = true,
            '(' | '[' | '{' => depth += 1,
            ')' | ']' | '}' => depth -= 1,
            value if value == separator && depth == 0 => {
                parts.push(source[start..index].trim());
                start = index + character.len_utf8();
            }
            _ => {}
        }
    }
    parts.push(source[start..].trim());
    parts
}

fn is_identifier_char(character: char) -> bool {
    character.is_alphanumeric() || character == '_'
}

fn is_identifier(value: &str) -> bool {
    let mut characters = value.chars();
    characters
        .next()
        .is_some_and(|character| character.is_alphabetic() || character == '_')
        && characters.all(is_identifier_char)
}

fn parse_pattern_scalar(source: &str) -> Option<EvalValue> {
    let source = source.trim();
    match source {
        "true" => return Some(EvalValue::Bool(true)),
        "false" => return Some(EvalValue::Bool(false)),
        "nil" => return Some(EvalValue::Null),
        _ => {}
    }
    if source.starts_with('"') && source.ends_with('"') {
        return serde_json::from_str::<String>(source)
            .ok()
            .map(EvalValue::String);
    }
    let normalized = source.replace('_', "");
    normalized
        .parse::<i64>()
        .ok()
        .map(EvalValue::Int)
        .or_else(|| {
            normalized
                .parse::<f64>()
                .ok()
                .filter(|value| value.is_finite())
                .map(EvalValue::Double)
        })
}

fn reorder_id_parameter(method: &str) -> String {
    match method.split('.').next().unwrap_or_default() {
        "workspace" => "workspace_id",
        "surface" => "surface_id",
        "pane" => "pane_id",
        "window" => "window_id",
        _ => "id",
    }
    .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn interprets_live_workspace_rows_and_parameterized_actions() {
        let document = evaluate(
            r#"
VStack(alignment: .leading, spacing: 6) {
  Text("Workspaces \(workspaceCount)").font(.headline)
  ForEach(workspaces) { w in
    if w.unread > 0 {
      Button(action: { cmux("workspace.select", workspace_id: w.id) }) {
        HStack {
          Text(w.title).fontWeight(w.selected ? .bold : .regular)
          Spacer()
          Text("\(w.unread)")
        }
      }
    }
  }
}
"#,
            &json!({
                "workspaceCount": 2,
                "workspaces": [
                    {"id": "workspace-a", "title": "Alpha", "selected": true, "unread": 3},
                    {"id": "workspace-b", "title": "Beta", "selected": false, "unread": 0}
                ]
            }),
        )
        .expect("evaluate sidebar");
        assert_eq!(document.root.kind, SidebarNodeKind::VStack);
        assert_eq!(document.root.children.len(), 2);
        assert_eq!(
            document.root.children[0].text.as_deref(),
            Some("Workspaces 2")
        );
        let button = &document.root.children[1];
        assert_eq!(button.kind, SidebarNodeKind::Button);
        assert_eq!(
            button.action.as_ref().unwrap().commands[0].params["workspace_id"],
            "workspace-a"
        );
        assert_eq!(
            button.children[0].children[0].weight.as_deref(),
            Some("bold")
        );
    }

    #[test]
    fn interprets_value_and_view_helpers() {
        let document = evaluate(
            r#"
func visible(_ w) -> Bool {
  return w.title.contains("A")
}
func row(_ w) -> some View {
  Text(w.title.lowercased())
}
VStack {
  ForEach(workspaces.filter { visible($0) }.prefix(2)) { w in
    row(w)
  }
}
"#,
            &json!({
                "workspaces": [
                    {"title": "Alpha"},
                    {"title": "Beta"},
                    {"title": "Atlas"}
                ]
            }),
        )
        .expect("evaluate helpers");
        assert_eq!(
            document
                .root
                .children
                .iter()
                .filter_map(|node| node.text.as_deref())
                .collect::<Vec<_>>(),
            vec!["alpha", "atlas"]
        );
    }

    #[test]
    fn interprets_reorderable_rows_with_persisted_move_metadata() {
        let document = evaluate(
            r#"
Reorderable(workspaces, move: "workspace.reorder") { w in
  Button(action: { cmux("workspace.select", workspace_id: w.id) }) {
    Text(w.title)
  }
}
"#,
            &json!({
                "workspaces": [
                    {"id": "workspace-a", "title": "Alpha"},
                    {"id": "workspace-b", "title": "Beta"}
                ]
            }),
        )
        .expect("evaluate reorderable");
        assert_eq!(document.root.kind, SidebarNodeKind::VStack);
        assert_eq!(document.root.children.len(), 2);
        let first = document.root.children[0]
            .reorder
            .as_ref()
            .expect("first reorder metadata");
        assert_eq!(first.method, "workspace.reorder");
        assert_eq!(first.id_parameter, "workspace_id");
        assert_eq!(first.item_id, "workspace-a");
        assert_eq!(first.index, 0);
        let second = document.root.children[1]
            .reorder
            .as_ref()
            .expect("second reorder metadata");
        assert_eq!(second.item_id, "workspace-b");
        assert_eq!(second.index, 1);
    }

    #[test]
    fn reorderable_row_state_tracks_item_identity_after_reorder() {
        let source = r#"
func row(_ workspace: Workspace) -> some View {
  @State private var expanded = false
  return Toggle(workspace.title, isOn: $expanded)
}

Reorderable(workspaces, move: "workspace.reorder") { workspace in
  row(workspace)
}
"#;
        let mut state = SidebarState::new();
        let initial = evaluate_with_state(
            source,
            &json!({
                "workspaces": [
                    {"id": "workspace-a", "title": "Alpha"},
                    {"id": "workspace-b", "title": "Beta"}
                ]
            }),
            &mut state,
        )
        .expect("initial reorderable state");
        let alpha_key = initial.root.children[0]
            .binding
            .as_ref()
            .expect("alpha binding")
            .key
            .clone();
        let beta_key = initial.root.children[1]
            .binding
            .as_ref()
            .expect("beta binding")
            .key
            .clone();
        assert_ne!(alpha_key, beta_key);
        state.insert(alpha_key.clone(), json!(true));

        let reordered = evaluate_with_state(
            source,
            &json!({
                "workspaces": [
                    {"id": "workspace-b", "title": "Beta"},
                    {"id": "workspace-a", "title": "Alpha"}
                ]
            }),
            &mut state,
        )
        .expect("reordered state");
        assert_eq!(
            reordered.root.children[0]
                .binding
                .as_ref()
                .expect("reordered beta binding"),
            &SidebarBinding {
                key: beta_key,
                value: json!(false),
            }
        );
        assert_eq!(
            reordered.root.children[1]
                .binding
                .as_ref()
                .expect("reordered alpha binding"),
            &SidebarBinding {
                key: alpha_key,
                value: json!(true),
            }
        );
    }

    #[test]
    fn stateful_controls_seed_bindings_and_capture_mutations() {
        let source = r#"
@State private var count = 0
@State private var enabled = true
@State private var name = "cmux"
VStack {
  Text("Count \(count)")
  Button("Increment") { count += 1 }
  Button("Disable") { enabled.toggle() }
  Toggle("Enabled", isOn: $enabled)
  TextField("Name", text: $name)
}
"#;
        let mut state = SidebarState::new();
        let document =
            evaluate_with_state(source, &json!({}), &mut state).expect("evaluate stateful sidebar");
        assert_eq!(state["count"], 0);
        assert_eq!(state["enabled"], true);
        assert_eq!(state["name"], "cmux");
        assert_eq!(document.root.children[0].text.as_deref(), Some("Count 0"));
        let increment = &document.root.children[1].action.as_ref().unwrap().commands[0];
        assert_eq!(increment.kind, "state");
        assert_eq!(increment.operation.as_deref(), Some("add"));
        assert_eq!(increment.key.as_deref(), Some("count"));
        assert_eq!(increment.value, Some(json!(1)));
        let toggle_action = &document.root.children[2].action.as_ref().unwrap().commands[0];
        assert_eq!(toggle_action.operation.as_deref(), Some("toggle"));
        assert_eq!(toggle_action.key.as_deref(), Some("enabled"));
        assert_eq!(
            document.root.children[3].binding,
            Some(SidebarBinding {
                key: "enabled".to_string(),
                value: json!(true)
            })
        );
        assert_eq!(
            document.root.children[4].binding,
            Some(SidebarBinding {
                key: "name".to_string(),
                value: json!("cmux")
            })
        );
        assert_eq!(
            document.root.children[4].placeholder.as_deref(),
            Some("Name")
        );

        state.insert("count".to_string(), json!(7));
        state.insert("enabled".to_string(), json!(false));
        state.insert("name".to_string(), json!("linux"));
        let rerendered =
            evaluate_with_state(source, &json!({}), &mut state).expect("rerender stateful sidebar");
        assert_eq!(rerendered.root.children[0].text.as_deref(), Some("Count 7"));
        assert_eq!(
            rerendered.root.children[3]
                .binding
                .as_ref()
                .map(|binding| &binding.value),
            Some(&json!(false))
        );
        assert_eq!(
            rerendered.root.children[4]
                .binding
                .as_ref()
                .map(|binding| &binding.value),
            Some(&json!("linux"))
        );
    }

    #[test]
    fn numeric_and_selection_controls_lower_typed_bindings() {
        let source = r#"
@State private var volume = 0.5
@State private var mode = "balanced"
@State private var count = 2
VStack {
  Slider(value: $volume, in: 0.0...1.0, step: 0.1) {
    Text("Volume")
  }
  Picker("Mode", selection: $mode) {
    Text("Fast").tag("fast")
    Text("Balanced").tag("balanced")
  }
  Stepper("Count \(count)", value: $count, in: 0...10, step: 2)
}
"#;
        let mut state = SidebarState::new();
        let document =
            evaluate_with_state(source, &json!({}), &mut state).expect("evaluate input controls");
        assert_eq!(state["volume"], 0.5);
        assert_eq!(state["mode"], "balanced");
        assert_eq!(state["count"], 2);

        let slider = &document.root.children[0];
        assert_eq!(slider.kind, SidebarNodeKind::Slider);
        assert_eq!(slider.minimum, Some(0.0));
        assert_eq!(slider.maximum, Some(1.0));
        assert_eq!(slider.step, Some(0.1));
        assert_eq!(slider.children[0].text.as_deref(), Some("Volume"));
        assert_eq!(
            slider.binding,
            Some(SidebarBinding {
                key: "volume".to_string(),
                value: json!(0.5)
            })
        );

        let picker = &document.root.children[1];
        assert_eq!(picker.kind, SidebarNodeKind::Picker);
        assert_eq!(picker.title.as_deref(), Some("Mode"));
        assert_eq!(
            picker.options,
            vec![
                SidebarOption {
                    label: "Fast".to_string(),
                    value: json!("fast")
                },
                SidebarOption {
                    label: "Balanced".to_string(),
                    value: json!("balanced")
                }
            ]
        );

        let stepper = &document.root.children[2];
        assert_eq!(stepper.kind, SidebarNodeKind::Stepper);
        assert_eq!(stepper.title.as_deref(), Some("Count 2"));
        assert_eq!(stepper.minimum, Some(0.0));
        assert_eq!(stepper.maximum, Some(10.0));
        assert_eq!(stepper.step, Some(2.0));
        assert_eq!(
            stepper.binding,
            Some(SidebarBinding {
                key: "count".to_string(),
                value: json!(2)
            })
        );
    }

    #[test]
    fn change_and_submit_hooks_evaluate_after_state_writes() {
        let source = r#"
@State private var name = "old"
@State private var changeCount = 0
@State private var transition = ""
@State private var submitted = ""
VStack {
  TextField("Name", text: $name)
    .onChange(of: name) { oldValue, newValue in
      transition = "\(oldValue)->\(newValue)"
      changeCount += 1
    }
    .onSubmit {
      submitted = name
    }
  Text("\(transition) \(changeCount) \(submitted)")
}
"#;
        let mut state = SidebarState::new();
        let initial =
            evaluate_with_state(source, &json!({}), &mut state).expect("initial event document");
        let control = &initial.root.children[0];
        assert_eq!(control.on_change.len(), 1);
        assert_eq!(control.on_submit.len(), 1);
        let change_id = control.on_change[0].id.clone();
        let submit_id = control.on_submit[0].id.clone();

        state.insert("name".to_string(), json!("new"));
        let changed = evaluate_with_state_and_event(
            source,
            &json!({}),
            &mut state,
            Some(&SidebarEvaluationEvent {
                id: change_id,
                old_value: json!("old"),
                new_value: json!("new"),
            }),
        )
        .expect("changed event document");
        let commands = &changed.root.children[0].on_change[0].action.commands;
        assert_eq!(commands[0].key.as_deref(), Some("transition"));
        assert_eq!(commands[0].value, Some(json!("old->new")));
        assert_eq!(commands[1].key.as_deref(), Some("changeCount"));
        assert_eq!(commands[1].operation.as_deref(), Some("add"));

        let submitted = evaluate_with_state_and_event(
            source,
            &json!({}),
            &mut state,
            Some(&SidebarEvaluationEvent {
                id: submit_id,
                old_value: json!("new"),
                new_value: json!("new"),
            }),
        )
        .expect("submit event document");
        let command = &submitted.root.children[0].on_submit[0].action.commands[0];
        assert_eq!(command.key.as_deref(), Some("submitted"));
        assert_eq!(command.value, Some(json!("new")));
    }

    #[test]
    fn custom_view_struct_renders_memberwise_fields_defaults_self_and_state() {
        let source = r#"
struct StatusRow: View {
  let title: String
  let detail: String = "Ready"
  @State private var expanded = false

  var body: some View {
    VStack {
      Text("\(self.title): \(detail)")
      Toggle("Expanded", isOn: $expanded)
    }
  }
}

StatusRow(title: "Alpha")
"#;
        let mut state = SidebarState::new();
        let initial = evaluate_with_state(source, &json!({}), &mut state)
            .expect("custom view initial document");
        assert_eq!(initial.root.kind, SidebarNodeKind::VStack);
        assert_eq!(
            initial.root.children[0].text.as_deref(),
            Some("Alpha: Ready")
        );
        let binding = initial.root.children[1]
            .binding
            .as_ref()
            .expect("custom view state binding");
        let key = binding.key.clone();
        assert!(key.starts_with(INSTANCE_STATE_PREFIX));
        assert_eq!(binding.value, json!(false));

        state.insert(key.clone(), json!(true));
        let updated = evaluate_with_state(source, &json!({}), &mut state)
            .expect("custom view updated document");
        assert_eq!(
            updated.root.children[1]
                .binding
                .as_ref()
                .expect("updated custom view binding"),
            &SidebarBinding {
                key,
                value: json!(true),
            }
        );
    }

    #[test]
    fn custom_view_struct_state_isolated_by_foreach_identity() {
        let source = r#"
struct StatusRow: View {
  let item: Item
  @State private var expanded = false

  var body: some View {
    Toggle(item.title, isOn: $expanded)
  }
}

ForEach(items, id: \.key) { item in
  StatusRow(item: item)
}
"#;
        let mut state = SidebarState::new();
        let original = json!({
            "items": [
                {"key": "alpha", "title": "Alpha"},
                {"key": "beta", "title": "Beta"}
            ]
        });
        let initial = evaluate_with_state(source, &original, &mut state)
            .expect("custom row initial document");
        let alpha_key = initial.root.children[0]
            .binding
            .as_ref()
            .expect("alpha custom row binding")
            .key
            .clone();
        let beta_key = initial.root.children[1]
            .binding
            .as_ref()
            .expect("beta custom row binding")
            .key
            .clone();
        assert_ne!(alpha_key, beta_key);
        state.insert(alpha_key.clone(), json!(true));

        let reordered = evaluate_with_state(
            source,
            &json!({
                "items": [
                    {"key": "beta", "title": "Beta"},
                    {"key": "alpha", "title": "Alpha"}
                ]
            }),
            &mut state,
        )
        .expect("custom row reordered document");
        assert_eq!(
            reordered.root.children[0]
                .binding
                .as_ref()
                .expect("reordered beta custom row"),
            &SidebarBinding {
                key: beta_key,
                value: json!(false),
            }
        );
        assert_eq!(
            reordered.root.children[1]
                .binding
                .as_ref()
                .expect("reordered alpha custom row"),
            &SidebarBinding {
                key: alpha_key,
                value: json!(true),
            }
        );
    }

    #[test]
    fn custom_view_struct_requires_non_defaulted_memberwise_arguments() {
        let error = evaluate(
            r#"
struct StatusRow: View {
  let title: String

  var body: some View {
    Text(title)
  }
}

StatusRow()
"#,
            &json!({}),
        )
        .expect_err("missing required custom view argument");
        assert_eq!(error, "No supported SwiftUI view found.");
    }

    #[test]
    fn enum_switch_renders_custom_view_cases_and_associated_values() {
        let document = evaluate(
            r#"
enum Status: String {
  case idle
  case running
  case failed(message: String)
}

struct StatusView: View {
  let status: Status

  var body: some View {
    switch status {
    case .idle:
      Text("Idle")
    case .running:
      Text("Running")
    case let .failed(message):
      Text("Failed: \(message)")
    }
  }
}

VStack {
  StatusView(status: Status.idle)
  StatusView(status: Status.running)
  StatusView(status: Status.failed(message: "network"))
}
"#,
            &json!({}),
        )
        .expect("enum switch document");
        assert_eq!(
            document
                .root
                .children
                .iter()
                .filter_map(|node| node.text.as_deref())
                .collect::<Vec<_>>(),
            vec!["Idle", "Running", "Failed: network"]
        );
    }

    #[test]
    fn switch_value_helpers_support_scalar_ranges_where_and_default() {
        let document = evaluate(
            r#"
enum Result {
  case score(Int)
}

func scoreLabel(_ result: Result) -> String {
  switch result {
  case let .score(value) where value >= 90:
    return "excellent"
  case let .score(value) where value >= 50:
    return "passing"
  default:
    return "retry"
  }
}

func bucket(_ value: Int) -> String {
  switch value {
  case 0:
    return "zero"
  case 1...3:
    return "small"
  default:
    return "large"
  }
}

VStack {
  Text(scoreLabel(Result.score(95)))
  Text(scoreLabel(Result.score(70)))
  Text(scoreLabel(Result.score(20)))
  Text(bucket(0))
  Text(bucket(2))
  Text(bucket(8))
}
"#,
            &json!({}),
        )
        .expect("switch helper document");
        assert_eq!(
            document
                .root
                .children
                .iter()
                .filter_map(|node| node.text.as_deref())
                .collect::<Vec<_>>(),
            vec!["excellent", "passing", "retry", "zero", "small", "large"]
        );
    }

    #[test]
    fn enum_state_round_trips_raw_values_actions_and_picker_tags() {
        let source = r#"
enum Mode: String {
  case compact
  case expanded = "wide"
}

@State private var mode = Mode.compact

VStack {
  Text(mode.rawValue)
  Button("Expand") { mode = Mode.expanded }
  Picker("Mode", selection: $mode) {
    Text("Compact").tag(Mode.compact)
    Text("Expanded").tag(Mode.expanded)
  }
}
"#;
        let mut state = SidebarState::new();
        let initial =
            evaluate_with_state(source, &json!({}), &mut state).expect("initial enum state");
        assert_eq!(initial.root.children[0].text.as_deref(), Some("compact"));
        let command = &initial.root.children[1]
            .action
            .as_ref()
            .expect("enum state action")
            .commands[0];
        let expanded = command.value.clone().expect("expanded enum value");
        assert_eq!(expanded[ENUM_TYPE_KEY], "Mode");
        assert_eq!(expanded[ENUM_CASE_KEY], "expanded");
        assert_eq!(expanded[ENUM_RAW_VALUE_KEY], "wide");
        let picker = &initial.root.children[2];
        assert_eq!(
            picker.binding.as_ref().expect("enum picker binding").value[ENUM_CASE_KEY],
            "compact"
        );
        assert_eq!(picker.options.len(), 2);
        assert_eq!(picker.options[1].value[ENUM_RAW_VALUE_KEY], "wide");

        state.insert("mode".to_string(), expanded);
        let updated =
            evaluate_with_state(source, &json!({}), &mut state).expect("updated enum state");
        assert_eq!(updated.root.children[0].text.as_deref(), Some("wide"));
        assert_eq!(
            updated.root.children[2]
                .binding
                .as_ref()
                .expect("updated enum picker binding")
                .value[ENUM_CASE_KEY],
            "expanded"
        );
    }

    #[test]
    fn enum_multiple_case_entries_and_qualified_patterns_are_supported() {
        let document = evaluate(
            r#"
enum Mode: String {
  case compact, expanded
  case focused = "focus"
}

VStack {
  Text(Mode.compact.rawValue)
  switch Mode.expanded {
  case Module.Mode.compact, .expanded:
    Text("matched")
  case .focused:
    Text("focused")
  }
}
"#,
            &json!({}),
        )
        .expect("multiple enum cases");
        assert_eq!(document.root.children[0].text.as_deref(), Some("compact"));
        assert_eq!(document.root.children[1].text.as_deref(), Some("matched"));
    }

    #[test]
    fn enum_state_resets_when_the_declared_enum_type_changes() {
        let mut state = SidebarState::new();
        evaluate_with_state(
            r#"
enum First {
  case selected
}
@State private var value = First.selected
Text("\(value)")
"#,
            &json!({}),
            &mut state,
        )
        .expect("first enum state");
        assert_eq!(state["value"][ENUM_TYPE_KEY], "First");

        let document = evaluate_with_state(
            r#"
enum Second {
  case selected
}
@State private var value = Second.selected
Text("\(value)")
"#,
            &json!({}),
            &mut state,
        )
        .expect("second enum state");
        assert_eq!(state["value"][ENUM_TYPE_KEY], "Second");
        assert_eq!(document.root.text.as_deref(), Some("selected"));
    }

    #[test]
    fn switch_in_action_selects_enum_state_transition_at_render_time() {
        let source = r#"
enum Mode {
  case compact
  case expanded
}

@State private var mode = Mode.compact

Button("Toggle mode") {
  switch mode {
  case .compact:
    mode = Mode.expanded
  case .expanded:
    mode = Mode.compact
  }
}
"#;
        let mut state = SidebarState::new();
        let compact =
            evaluate_with_state(source, &json!({}), &mut state).expect("compact enum action");
        let expanded_value = compact
            .root
            .action
            .as_ref()
            .expect("compact action")
            .commands[0]
            .value
            .clone()
            .expect("expanded transition");
        assert_eq!(expanded_value[ENUM_CASE_KEY], "expanded");

        state.insert("mode".to_string(), expanded_value);
        let expanded =
            evaluate_with_state(source, &json!({}), &mut state).expect("expanded enum action");
        assert_eq!(
            expanded
                .root
                .action
                .as_ref()
                .expect("expanded action")
                .commands[0]
                .value
                .as_ref()
                .expect("compact transition")[ENUM_CASE_KEY],
            "compact"
        );
    }

    #[test]
    fn row_state_identity_survives_reorder_and_prunes_removed_instances() {
        let source = r#"
func row(_ item: Item) -> some View {
  @State private var expanded = false
  return Toggle(item.title, isOn: $expanded)
}

ForEach(items, id: \.key) { item in
  row(item)
}
"#;
        let original = json!({
            "items": [
                {"key": "alpha", "title": "Alpha"},
                {"key": "beta", "title": "Beta"}
            ]
        });
        let mut state = SidebarState::new();
        let initial =
            evaluate_with_state(source, &original, &mut state).expect("initial row state document");
        let alpha = initial
            .root
            .children
            .iter()
            .find(|node| node.title.as_deref() == Some("Alpha"))
            .expect("alpha row");
        let beta = initial
            .root
            .children
            .iter()
            .find(|node| node.title.as_deref() == Some("Beta"))
            .expect("beta row");
        let alpha_key = alpha.binding.as_ref().expect("alpha binding").key.clone();
        let beta_key = beta.binding.as_ref().expect("beta binding").key.clone();
        assert_ne!(alpha_key, beta_key);
        assert!(alpha_key.starts_with(INSTANCE_STATE_PREFIX));
        assert_eq!(state[&alpha_key], false);
        assert_eq!(state[&beta_key], false);

        state.insert(alpha_key.clone(), json!(true));
        let reordered = json!({
            "items": [
                {"key": "beta", "title": "Beta"},
                {"key": "alpha", "title": "Alpha"}
            ]
        });
        let reordered = evaluate_with_state(source, &reordered, &mut state)
            .expect("reordered row state document");
        let alpha = reordered
            .root
            .children
            .iter()
            .find(|node| node.title.as_deref() == Some("Alpha"))
            .expect("reordered alpha row");
        let beta = reordered
            .root
            .children
            .iter()
            .find(|node| node.title.as_deref() == Some("Beta"))
            .expect("reordered beta row");
        assert_eq!(alpha.binding.as_ref().unwrap().key, alpha_key);
        assert_eq!(alpha.binding.as_ref().unwrap().value, json!(true));
        assert_eq!(beta.binding.as_ref().unwrap().key, beta_key);
        assert_eq!(beta.binding.as_ref().unwrap().value, json!(false));

        let beta_only = json!({
            "items": [
                {"key": "beta", "title": "Beta"}
            ]
        });
        evaluate_with_state(source, &beta_only, &mut state).expect("pruned row state document");
        assert!(!state.contains_key(&alpha_key));
        assert_eq!(state[&beta_key], false);
    }

    #[test]
    fn row_state_identity_limit_fails_without_mutating_existing_state() {
        let source = r#"
func row(_ item: Item) -> some View {
  @State private var selected = false
  return Toggle(item.title, isOn: $selected)
}

ForEach(items, id: \.id) { item in
  row(item)
}
"#;
        let context = json!({
            "items": (0..=MAX_STATE_IDENTITIES)
                .map(|index| json!({
                    "id": format!("item-{index}"),
                    "title": format!("Item {index}")
                }))
                .collect::<Vec<_>>()
        });
        let original = SidebarState::from([("existing".to_string(), json!("kept"))]);
        let mut state = original.clone();
        let error = evaluate_with_state(source, &context, &mut state)
            .expect_err("too many active state identities must fail");
        assert!(error.contains(&format!("limited to {MAX_STATE_IDENTITIES} active values")));
        assert_eq!(state, original);
    }

    #[test]
    fn stateful_declaration_type_changes_reset_only_that_value() {
        let mut state = SidebarState::from([
            ("value".to_string(), json!(true)),
            ("untouched".to_string(), json!(9)),
        ]);
        let document = evaluate_with_state(
            r#"
@State private var value = "reset"
@State private var untouched = 0
VStack {
  TextField("Value", text: $value)
  Text("Untouched \(untouched)")
}
"#,
            &json!({}),
            &mut state,
        )
        .expect("evaluate changed state declaration");
        assert_eq!(state["value"], "reset");
        assert_eq!(state["untouched"], 9);
        assert_eq!(
            document.root.children[0]
                .binding
                .as_ref()
                .map(|binding| binding.value.clone()),
            Some(json!("reset"))
        );
        assert_eq!(
            document.root.children[1].text.as_deref(),
            Some("Untouched 9")
        );
    }

    #[test]
    fn repository_custom_sidebar_examples_produce_live_trees() {
        let context = json!({
            "workspaceCount": 2,
            "selectedTitle": "Alpha",
            "selectedId": "workspace-a",
            "unreadTotal": 3,
            "clock": {"time": "12:34:56", "epoch": 1_750_000_000},
            "workspaces": [
                {
                    "id": "workspace-a",
                    "title": "Alpha crash fix",
                    "selected": true,
                    "pinned": true,
                    "index": 0,
                    "directory": "/tmp/alpha",
                    "ports": [3000],
                    "portCount": 1,
                    "unread": 3,
                    "tabs": [{
                        "id": "surface-a",
                        "title": "Terminal",
                        "focused": true,
                        "pinned": false,
                        "directory": "/tmp/alpha",
                        "branch": "fix/crash",
                        "dirty": true,
                        "ports": [3000]
                    }],
                    "tabCount": 1,
                    "branch": "fix/crash",
                    "dirty": true,
                    "progress": {"value": 0.5, "label": "Tests"},
                    "latestPrompt": "fix crash",
                    "latestMessage": "Working",
                    "latestAt": 1_749_999_900
                },
                {
                    "id": "workspace-b",
                    "title": "Research",
                    "selected": false,
                    "pinned": false,
                    "index": 1,
                    "directory": "/tmp/research",
                    "ports": [],
                    "portCount": 0,
                    "unread": 0,
                    "tabs": [],
                    "tabCount": 0,
                    "dirty": false
                }
            ]
        });
        let status = evaluate(
            include_str!("../../Examples/CustomSidebars/status-board.swift"),
            &context,
        )
        .expect("evaluate status board");
        assert!(sidebar_contains_text(&status.root, "Status board"));
        assert!(sidebar_contains_command(
            &status.root,
            "workspace.select",
            "workspace_id",
            "workspace-a"
        ));

        let finder = evaluate(
            include_str!("../../Examples/CustomSidebars/finder.swift"),
            &context,
        )
        .expect("evaluate finder");
        assert!(sidebar_contains_text(&finder.root, "Cmux"));
        assert!(sidebar_contains_command(
            &finder.root,
            "surface.focus",
            "surface_id",
            "surface-a"
        ));
    }

    fn sidebar_contains_text(node: &SidebarNode, needle: &str) -> bool {
        node.text.as_deref() == Some(needle)
            || node.title.as_deref() == Some(needle)
            || node
                .children
                .iter()
                .any(|child| sidebar_contains_text(child, needle))
    }

    fn sidebar_contains_command(
        node: &SidebarNode,
        method: &str,
        parameter: &str,
        value: &str,
    ) -> bool {
        node.action.as_ref().is_some_and(|action| {
            action.commands.iter().any(|command| {
                command.method.as_deref() == Some(method)
                    && command
                        .params
                        .get(parameter)
                        .is_some_and(|actual| actual == value)
            })
        }) || node
            .children
            .iter()
            .any(|child| sidebar_contains_command(child, method, parameter, value))
    }
}
