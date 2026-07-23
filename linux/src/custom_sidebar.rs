use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::{BTreeMap, HashMap};
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
use std::path::{Path, PathBuf};

pub const DEFAULT_PROVIDER_ID: &str = "cmux.sidebar.workspaces";
pub const CUSTOM_PROVIDER_PREFIX: &str = "cmux.sidebar.custom.";
const DOCUMENT_VERSION: u32 = 1;
const MAX_DOCUMENT_BYTES: u64 = 1024 * 1024;
const MAX_NODE_COUNT: usize = 4096;
const MAX_NODE_DEPTH: usize = 64;
const MAX_TEXT_CHARS: usize = 16_384;
const STATE_STORE_VERSION: u32 = 1;
const MAX_STATE_STORE_BYTES: u64 = 1024 * 1024;
const MAX_STATE_SIDEBARS: usize = 128;
const MAX_STATE_ENTRIES_PER_SIDEBAR: usize = 256;
const MAX_STATE_VALUE_DEPTH: usize = 8;
const MAX_STATE_VALUE_ITEMS: usize = 1024;
const MAX_STATE_KEY_CHARS: usize = 128;
const MAX_PICKER_OPTIONS: usize = 512;
const MAX_NODE_EVENTS: usize = 16;
const ENUM_TYPE_KEY: &str = "__cmux_enum_type";
const ENUM_CASE_KEY: &str = "__cmux_enum_case";
const ENUM_VALUES_KEY: &str = "__cmux_enum_values";
const ENUM_LABELS_KEY: &str = "__cmux_enum_labels";
const ENUM_RAW_VALUE_KEY: &str = "__cmux_enum_raw_value";

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct SidebarValidationEntry {
    pub name: String,
    pub path: String,
    pub kind: String,
    pub ok: bool,
    pub error: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SidebarDocument {
    pub version: u32,
    pub root: SidebarNode,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SidebarNode {
    #[serde(rename = "type")]
    pub kind: SidebarNodeKind,
    #[serde(default)]
    pub children: Vec<SidebarNode>,
    #[serde(default)]
    pub spacing: Option<f64>,
    #[serde(default)]
    pub alignment: Option<String>,
    #[serde(default)]
    pub padding: Option<f64>,
    #[serde(default)]
    pub text: Option<String>,
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub font: Option<String>,
    #[serde(default)]
    pub weight: Option<String>,
    #[serde(default)]
    pub color: Option<String>,
    #[serde(default)]
    pub background: Option<String>,
    #[serde(rename = "systemName", default)]
    pub system_name: Option<String>,
    #[serde(default)]
    pub size: Option<f64>,
    #[serde(default)]
    pub width: Option<f64>,
    #[serde(default)]
    pub height: Option<f64>,
    #[serde(default)]
    pub opacity: Option<f64>,
    #[serde(rename = "cornerRadius", default)]
    pub corner_radius: Option<f64>,
    #[serde(default)]
    pub value: Option<f64>,
    #[serde(default)]
    pub minimum: Option<f64>,
    #[serde(default)]
    pub maximum: Option<f64>,
    #[serde(default)]
    pub step: Option<f64>,
    #[serde(default)]
    pub binding: Option<SidebarBinding>,
    #[serde(default)]
    pub options: Vec<SidebarOption>,
    #[serde(default)]
    pub tag: Option<Value>,
    #[serde(default)]
    pub placeholder: Option<String>,
    #[serde(default)]
    pub action: Option<SidebarAction>,
    #[serde(rename = "onChange", default)]
    pub on_change: Vec<SidebarEvent>,
    #[serde(rename = "onSubmit", default)]
    pub on_submit: Vec<SidebarEvent>,
    #[serde(default)]
    pub reorder: Option<SidebarReorder>,
}

impl SidebarNode {
    pub fn simple(kind: SidebarNodeKind) -> Self {
        Self {
            kind,
            children: Vec::new(),
            spacing: None,
            alignment: None,
            padding: None,
            text: None,
            title: None,
            font: None,
            weight: None,
            color: None,
            background: None,
            system_name: None,
            size: None,
            width: None,
            height: None,
            opacity: None,
            corner_radius: None,
            value: None,
            minimum: None,
            maximum: None,
            step: None,
            binding: None,
            options: Vec::new(),
            tag: None,
            placeholder: None,
            action: None,
            on_change: Vec::new(),
            on_submit: Vec::new(),
            reorder: None,
        }
    }

    pub fn container(kind: SidebarNodeKind, children: Vec<Self>) -> Self {
        Self {
            children,
            ..Self::simple(kind)
        }
    }

    pub fn text(text: String) -> Self {
        Self {
            text: Some(text),
            ..Self::simple(SidebarNodeKind::Text)
        }
    }

    pub fn image(system_name: String) -> Self {
        Self {
            system_name: Some(system_name),
            ..Self::simple(SidebarNodeKind::Image)
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SidebarNodeKind {
    VStack,
    HStack,
    ZStack,
    Text,
    Button,
    Image,
    Spacer,
    Divider,
    Progress,
    Shape,
    Toggle,
    TextField,
    Slider,
    Picker,
    Stepper,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SidebarBinding {
    pub key: String,
    pub value: Value,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SidebarOption {
    pub label: String,
    pub value: Value,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct SidebarAction {
    #[serde(rename = "type")]
    #[serde(default)]
    pub kind: String,
    #[serde(default)]
    pub message: Option<String>,
    #[serde(default)]
    pub params: HashMap<String, String>,
    #[serde(default)]
    pub commands: Vec<SidebarActionCommand>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct SidebarActionCommand {
    #[serde(rename = "type")]
    pub kind: String,
    #[serde(default)]
    pub method: Option<String>,
    #[serde(default)]
    pub params: HashMap<String, String>,
    #[serde(default)]
    pub message: Option<String>,
    #[serde(default)]
    pub operation: Option<String>,
    #[serde(default)]
    pub key: Option<String>,
    #[serde(default)]
    pub value: Option<Value>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SidebarEvent {
    pub id: String,
    #[serde(default)]
    pub key: Option<String>,
    pub action: SidebarAction,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SidebarReorder {
    pub method: String,
    #[serde(rename = "idParameter")]
    pub id_parameter: String,
    #[serde(rename = "itemId")]
    pub item_id: String,
    pub index: usize,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct SidebarSelection {
    version: u32,
    provider_id: String,
}

pub type SidebarState = BTreeMap<String, Value>;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SidebarStateStore {
    version: u32,
    #[serde(default)]
    sidebars: BTreeMap<String, SidebarState>,
}

impl Default for SidebarStateStore {
    fn default() -> Self {
        Self {
            version: STATE_STORE_VERSION,
            sidebars: BTreeMap::new(),
        }
    }
}

impl SidebarStateStore {
    pub fn sidebar_state(&self, provider_id: &str) -> SidebarState {
        self.sidebars.get(provider_id).cloned().unwrap_or_default()
    }

    pub fn replace_sidebar_state(
        &mut self,
        provider_id: &str,
        state: SidebarState,
    ) -> Result<bool, String> {
        validate_provider_state(provider_id, &state)?;
        if self.sidebars.get(provider_id) == Some(&state) {
            return Ok(false);
        }
        if state.is_empty() {
            self.sidebars.remove(provider_id);
        } else {
            if !self.sidebars.contains_key(provider_id) && self.sidebars.len() >= MAX_STATE_SIDEBARS
            {
                return Err(format!(
                    "Custom sidebar state is limited to {MAX_STATE_SIDEBARS} sidebars."
                ));
            }
            self.sidebars.insert(provider_id.to_string(), state);
        }
        Ok(true)
    }

    pub fn set(&mut self, provider_id: &str, key: &str, value: Value) -> Result<bool, String> {
        validate_state_provider(provider_id)?;
        validate_state_key(key)?;
        validate_state_value(&value)?;
        let state = self.sidebars.entry(provider_id.to_string()).or_default();
        if !state.contains_key(key) && state.len() >= MAX_STATE_ENTRIES_PER_SIDEBAR {
            return Err(format!(
                "Custom sidebar state is limited to {MAX_STATE_ENTRIES_PER_SIDEBAR} values per sidebar."
            ));
        }
        if state.get(key) == Some(&value) {
            return Ok(false);
        }
        state.insert(key.to_string(), value);
        Ok(true)
    }

    pub fn clear(&mut self, provider_id: &str) -> Result<bool, String> {
        validate_state_provider(provider_id)?;
        Ok(self.sidebars.remove(provider_id).is_some())
    }
}

pub fn sidebars_dir() -> PathBuf {
    if let Some(path) = normalized_env("CMUX_CUSTOM_SIDEBARS_DIR") {
        return PathBuf::from(path);
    }
    if let Some(path) = normalized_env("XDG_CONFIG_HOME") {
        return PathBuf::from(path).join("cmux/sidebars");
    }
    if let Some(home) = normalized_env("HOME") {
        return PathBuf::from(home).join(".config/cmux/sidebars");
    }
    std::env::temp_dir().join("cmux-sidebars")
}

pub fn selection_path(state_dir: &Path) -> PathBuf {
    normalized_env("CMUX_CUSTOM_SIDEBAR_SELECTION_PATH")
        .map(PathBuf::from)
        .unwrap_or_else(|| state_dir.join("custom-sidebar-selection.json"))
}

pub fn state_store_path(state_dir: &Path) -> PathBuf {
    normalized_env("CMUX_CUSTOM_SIDEBAR_STATE_PATH")
        .map(PathBuf::from)
        .unwrap_or_else(|| state_dir.join("custom-sidebar-state.json"))
}

pub fn load_state_store(path: &Path) -> SidebarStateStore {
    let Ok(metadata) = fs::metadata(path) else {
        return SidebarStateStore::default();
    };
    if metadata.len() > MAX_STATE_STORE_BYTES {
        return SidebarStateStore::default();
    }
    let Ok(bytes) = fs::read(path) else {
        return SidebarStateStore::default();
    };
    let Ok(store) = serde_json::from_slice::<SidebarStateStore>(&bytes) else {
        return SidebarStateStore::default();
    };
    if store.version != STATE_STORE_VERSION
        || store.sidebars.len() > MAX_STATE_SIDEBARS
        || store
            .sidebars
            .iter()
            .any(|(provider_id, state)| validate_provider_state(provider_id, state).is_err())
    {
        return SidebarStateStore::default();
    }
    store
}

pub fn save_state_store(path: &Path, store: &SidebarStateStore) -> Result<(), String> {
    if store.version != STATE_STORE_VERSION || store.sidebars.len() > MAX_STATE_SIDEBARS {
        return Err("Custom sidebar state store is invalid.".to_string());
    }
    for (provider_id, state) in &store.sidebars {
        validate_provider_state(provider_id, state)?;
    }
    write_private_json(path, store, "custom sidebar state")
}

pub fn load_selected_provider(path: &Path) -> String {
    fs::read(path)
        .ok()
        .and_then(|bytes| serde_json::from_slice::<SidebarSelection>(&bytes).ok())
        .filter(|selection| selection.version == 1)
        .map(|selection| selection.provider_id)
        .filter(|provider| {
            provider == DEFAULT_PROVIDER_ID
                || provider == crate::sidebar_extension::HOSTED_PROVIDER_ID
                || provider.starts_with(CUSTOM_PROVIDER_PREFIX)
        })
        .unwrap_or_else(|| DEFAULT_PROVIDER_ID.to_string())
}

pub fn save_selected_provider(path: &Path, provider_id: &str) -> Result<(), String> {
    write_private_json(
        path,
        &SidebarSelection {
            version: 1,
            provider_id: provider_id.to_string(),
        },
        "custom sidebar selection",
    )
}

fn write_private_json<T: Serialize>(path: &Path, value: &T, label: &str) -> Result<(), String> {
    let parent = path
        .parent()
        .ok_or_else(|| format!("{label} path has no parent: {}", path.display()))?;
    fs::create_dir_all(parent).map_err(|err| {
        format!(
            "failed to create {label} directory {}: {err}",
            parent.display()
        )
    })?;
    fs::set_permissions(parent, fs::Permissions::from_mode(0o700)).map_err(|err| {
        format!(
            "failed to protect {label} directory {}: {err}",
            parent.display()
        )
    })?;
    let bytes = serde_json::to_vec_pretty(value)
        .map_err(|err| format!("failed to encode {label}: {err}"))?;
    if bytes.len() as u64 > MAX_STATE_STORE_BYTES {
        return Err(format!(
            "{label} exceeds the {MAX_STATE_STORE_BYTES} byte limit."
        ));
    }
    let temporary = path.with_extension(format!("tmp-{}", std::process::id()));
    let mut file = OpenOptions::new()
        .create(true)
        .truncate(true)
        .write(true)
        .mode(0o600)
        .open(&temporary)
        .map_err(|err| format!("failed to write {label} {}: {err}", temporary.display()))?;
    file.write_all(&bytes)
        .map_err(|err| format!("failed to write {label} {}: {err}", temporary.display()))?;
    file.write_all(b"\n")
        .map_err(|err| format!("failed to finish {label} {}: {err}", temporary.display()))?;
    file.sync_all()
        .map_err(|err| format!("failed to sync {label} {}: {err}", temporary.display()))?;
    fs::rename(&temporary, path).map_err(|err| {
        let _ = fs::remove_file(&temporary);
        format!("failed to replace {label} {}: {err}", path.display())
    })?;
    fs::set_permissions(path, fs::Permissions::from_mode(0o600))
        .map_err(|err| format!("failed to protect {label} {}: {err}", path.display()))
}

pub fn provider_id(name: &str) -> String {
    format!("{CUSTOM_PROVIDER_PREFIX}{name}")
}

pub fn provider_name(provider_id: &str) -> Option<&str> {
    provider_id
        .strip_prefix(CUSTOM_PROVIDER_PREFIX)
        .filter(|name| valid_name(name))
}

pub fn valid_name(name: &str) -> bool {
    let trimmed = name.trim();
    !trimmed.is_empty()
        && trimmed == name
        && trimmed != "."
        && trimmed != ".."
        && !trimmed.contains('/')
        && !trimmed.contains('\\')
        && trimmed
            .chars()
            .all(|ch| ch.is_alphanumeric() || matches!(ch, '-' | '_' | '.'))
}

pub fn discover(directory: &Path, requested_name: Option<&str>) -> Vec<PathBuf> {
    let Ok(entries) = fs::read_dir(directory) else {
        return Vec::new();
    };
    let mut by_name = BTreeMap::<String, PathBuf>::new();
    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_file() {
            continue;
        }
        let Some(extension) = path.extension().and_then(|value| value.to_str()) else {
            continue;
        };
        let extension = extension.to_ascii_lowercase();
        if extension != "swift" && extension != "json" {
            continue;
        }
        let Some(name) = path.file_stem().and_then(|value| value.to_str()) else {
            continue;
        };
        if !valid_name(name) || requested_name.is_some_and(|requested| requested != name) {
            continue;
        }
        if by_name
            .get(name)
            .and_then(|existing| existing.extension())
            .and_then(|value| value.to_str())
            .is_some_and(|existing| existing.eq_ignore_ascii_case("swift"))
        {
            continue;
        }
        by_name.insert(name.to_string(), path);
    }
    by_name.into_values().collect()
}

pub fn validate(directory: &Path, requested_name: Option<&str>) -> Vec<SidebarValidationEntry> {
    validate_with_context(directory, requested_name, &validation_context())
}

pub fn validate_with_context(
    directory: &Path,
    requested_name: Option<&str>,
    context: &Value,
) -> Vec<SidebarValidationEntry> {
    let paths = discover(directory, requested_name);
    if paths.is_empty() {
        return requested_name
            .map(|name| SidebarValidationEntry {
                name: name.to_string(),
                path: directory.join(format!("{name}.json")).display().to_string(),
                kind: "json".to_string(),
                ok: false,
                error: Some("Sidebar file is missing.".to_string()),
            })
            .into_iter()
            .collect();
    }
    paths
        .into_iter()
        .map(|path| validate_path_with_context(&path, context))
        .collect()
}

#[cfg(test)]
pub fn validate_path(path: &Path) -> SidebarValidationEntry {
    validate_path_with_context(path, &validation_context())
}

pub fn validate_path_with_context(path: &Path, context: &Value) -> SidebarValidationEntry {
    let name = path
        .file_stem()
        .and_then(|value| value.to_str())
        .unwrap_or_default()
        .to_string();
    let kind = path
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or("json")
        .to_ascii_lowercase();
    let result = read_document_with_context(path, context).map(|_| ());
    SidebarValidationEntry {
        name,
        path: path.display().to_string(),
        kind,
        ok: result.is_ok(),
        error: result.err(),
    }
}

pub fn read_document_with_context(path: &Path, context: &Value) -> Result<SidebarDocument, String> {
    read_document_with_context_and_state(path, context, &mut SidebarState::new())
}

pub fn read_document_with_context_and_state(
    path: &Path,
    context: &Value,
    state: &mut SidebarState,
) -> Result<SidebarDocument, String> {
    read_document_with_context_state_and_event(path, context, state, None)
}

pub fn read_document_with_context_state_and_event(
    path: &Path,
    context: &Value,
    state: &mut SidebarState,
    event: Option<&crate::swift_sidebar::SidebarEvaluationEvent>,
) -> Result<SidebarDocument, String> {
    let metadata =
        fs::metadata(path).map_err(|err| format!("Failed to read sidebar file metadata: {err}"))?;
    if metadata.len() > MAX_DOCUMENT_BYTES {
        return Err(format!(
            "Sidebar file exceeds the {} byte limit.",
            MAX_DOCUMENT_BYTES
        ));
    }
    let bytes = fs::read(path).map_err(|err| format!("Failed to read sidebar file: {err}"))?;
    let is_swift = path
        .extension()
        .and_then(|value| value.to_str())
        .is_some_and(|extension| extension.eq_ignore_ascii_case("swift"));
    let mut evaluated_state = state.clone();
    let mut document = if is_swift {
        let source = std::str::from_utf8(&bytes)
            .map_err(|_| "Interpreted Swift sidebar source must be valid UTF-8.".to_string())?;
        match crate::config::custom_sidebar_renderer_mode() {
            crate::config::CustomSidebarRendererMode::InProcess => {
                crate::swift_sidebar::evaluate_with_state_and_event(
                    source,
                    context,
                    &mut evaluated_state,
                    event,
                )?
            }
            crate::config::CustomSidebarRendererMode::Remote => {
                crate::swift_sidebar::evaluate_isolated_with_state_and_event(
                    source,
                    context,
                    &mut evaluated_state,
                    event,
                )?
            }
        }
    } else {
        serde_json::from_slice::<SidebarDocument>(&bytes)
            .map_err(|err| describe_json_error(&err))?
    };
    if !is_swift {
        hydrate_document_bindings(&mut document.root, &mut evaluated_state)?;
    }
    validate_document(&document)?;
    *state = evaluated_state;
    Ok(document)
}

pub fn selected_snapshot(
    directory: &Path,
    selected_provider_id: &str,
    reload_generation: u64,
    last_good: Option<&SidebarDocument>,
    context: &Value,
    state: &mut SidebarState,
) -> (Value, Option<SidebarDocument>) {
    let discovered = discover(directory, None);
    let providers = discovered
        .iter()
        .map(|path| {
            let name = path
                .file_stem()
                .and_then(|value| value.to_str())
                .unwrap_or_default();
            let kind = path
                .extension()
                .and_then(|value| value.to_str())
                .unwrap_or("json")
                .to_ascii_lowercase();
            json!({
                "id": provider_id(name),
                "title": name,
                "subtitle": "Custom sidebar",
                "path": path.display().to_string(),
                "kind": kind,
                "ok": true,
                "error": Value::Null
            })
        })
        .collect::<Vec<_>>();
    let base = json!({
        "enabled": true,
        "directory": directory.display().to_string(),
        "default_provider_id": DEFAULT_PROVIDER_ID,
        "selected_provider_id": selected_provider_id,
        "renderer": crate::config::custom_sidebar_renderer_mode().as_str(),
        "reload_generation": reload_generation,
        "providers": providers
    });
    let Some(name) = provider_name(selected_provider_id) else {
        let mut snapshot = base;
        snapshot["selected_name"] = Value::Null;
        snapshot["state"] = json!("default");
        snapshot["document"] = Value::Null;
        snapshot["error"] = Value::Null;
        snapshot["using_last_good"] = json!(false);
        return (snapshot, None);
    };
    let mut snapshot = base;
    snapshot["selected_name"] = json!(name);
    let Some(path) = discovered.iter().find(|path| {
        path.file_stem()
            .and_then(|value| value.to_str())
            .is_some_and(|candidate| candidate == name)
    }) else {
        snapshot["state"] = json!("missing");
        snapshot["document"] = Value::Null;
        snapshot["error"] = json!("Sidebar file is missing.");
        snapshot["using_last_good"] = json!(false);
        return (snapshot, None);
    };
    snapshot["path"] = json!(path.display().to_string());
    snapshot["kind"] = json!(path
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or("json")
        .to_ascii_lowercase());
    match read_document_with_context_and_state(path, context, state) {
        Ok(document) => {
            set_snapshot_provider_validation(&mut snapshot, selected_provider_id, true, None);
            snapshot["state"] = json!("ready");
            snapshot["document"] = serde_json::to_value(&document).unwrap_or(Value::Null);
            snapshot["error"] = Value::Null;
            snapshot["using_last_good"] = json!(false);
            (snapshot, Some(document))
        }
        Err(error) => {
            set_snapshot_provider_validation(
                &mut snapshot,
                selected_provider_id,
                false,
                Some(&error),
            );
            snapshot["state"] = json!(if last_good.is_some() {
                "stale"
            } else {
                "error"
            });
            snapshot["document"] = last_good
                .and_then(|document| serde_json::to_value(document).ok())
                .unwrap_or(Value::Null);
            snapshot["error"] = json!(error);
            snapshot["using_last_good"] = json!(last_good.is_some());
            (snapshot, None)
        }
    }
}

fn set_snapshot_provider_validation(
    snapshot: &mut Value,
    provider_id: &str,
    ok: bool,
    error: Option<&str>,
) {
    let Some(provider) = snapshot
        .get_mut("providers")
        .and_then(Value::as_array_mut)
        .and_then(|providers| {
            providers
                .iter_mut()
                .find(|provider| provider.get("id").and_then(Value::as_str) == Some(provider_id))
        })
    else {
        return;
    };
    provider["ok"] = json!(ok);
    provider["error"] = error.map_or(Value::Null, |error| json!(error));
}

fn validation_context() -> Value {
    json!({
        "workspaces": [{
            "id": "workspace-sample",
            "title": "Sample Workspace",
            "selected": true,
            "pinned": false,
            "index": 0,
            "directory": "~/project",
            "ports": [3000],
            "portCount": 1,
            "unread": 0,
            "tabs": [],
            "tabCount": 0,
            "branch": "main",
            "dirty": false
        }],
        "workspaceCount": 1,
        "selectedTitle": "Sample Workspace",
        "selectedId": "workspace-sample",
        "unreadTotal": 0,
        "clock": {
            "time": "12:00:00",
            "hour": 12,
            "minute": 0,
            "second": 0,
            "weekday": 1,
            "epoch": 0
        }
    })
}

pub(crate) fn validate_document(document: &SidebarDocument) -> Result<(), String> {
    if document.version != DOCUMENT_VERSION {
        return Err(format!(
            "Unsupported sidebar document version {}; expected {}.",
            document.version, DOCUMENT_VERSION
        ));
    }
    let mut node_count = 0;
    validate_node(&document.root, 1, &mut node_count)
}

fn validate_node(node: &SidebarNode, depth: usize, node_count: &mut usize) -> Result<(), String> {
    if depth > MAX_NODE_DEPTH {
        return Err(format!(
            "Sidebar tree exceeds the maximum depth of {MAX_NODE_DEPTH}."
        ));
    }
    *node_count += 1;
    if *node_count > MAX_NODE_COUNT {
        return Err(format!(
            "Sidebar tree exceeds the maximum node count of {MAX_NODE_COUNT}."
        ));
    }
    for value in [
        node.spacing,
        node.padding,
        node.size,
        node.width,
        node.height,
        node.opacity,
        node.corner_radius,
        node.value,
        node.minimum,
        node.maximum,
        node.step,
    ]
    .into_iter()
    .flatten()
    {
        if !value.is_finite() {
            return Err("Sidebar numeric style values must be finite.".to_string());
        }
    }
    for value in [
        node.text.as_deref(),
        node.title.as_deref(),
        node.alignment.as_deref(),
        node.font.as_deref(),
        node.weight.as_deref(),
        node.color.as_deref(),
        node.background.as_deref(),
        node.system_name.as_deref(),
        node.placeholder.as_deref(),
    ]
    .into_iter()
    .flatten()
    {
        if value.chars().count() > MAX_TEXT_CHARS {
            return Err(format!(
                "Sidebar text fields are limited to {MAX_TEXT_CHARS} characters."
            ));
        }
    }
    if let Some(binding) = &node.binding {
        validate_state_key(&binding.key)?;
        validate_state_value(&binding.value)?;
        match node.kind {
            SidebarNodeKind::Toggle if !binding.value.is_boolean() => {
                return Err("Sidebar Toggle bindings must contain a boolean value.".to_string());
            }
            SidebarNodeKind::TextField if !binding.value.is_string() => {
                return Err("Sidebar TextField bindings must contain a string value.".to_string());
            }
            SidebarNodeKind::Slider | SidebarNodeKind::Stepper if !binding.value.is_number() => {
                return Err("Sidebar numeric control bindings must contain a number.".to_string());
            }
            SidebarNodeKind::Picker if !is_picker_value(&binding.value) => {
                return Err(
                    "Sidebar Picker bindings must contain a boolean, number, string, or enum value."
                        .to_string(),
                );
            }
            _ => {}
        }
    } else if matches!(
        node.kind,
        SidebarNodeKind::Toggle
            | SidebarNodeKind::TextField
            | SidebarNodeKind::Slider
            | SidebarNodeKind::Picker
            | SidebarNodeKind::Stepper
    ) {
        return Err("Sidebar input controls require a binding.".to_string());
    }
    if matches!(
        node.kind,
        SidebarNodeKind::Slider | SidebarNodeKind::Stepper
    ) {
        let minimum = node
            .minimum
            .ok_or_else(|| "Sidebar numeric controls require a minimum value.".to_string())?;
        let maximum = node
            .maximum
            .ok_or_else(|| "Sidebar numeric controls require a maximum value.".to_string())?;
        let step = node
            .step
            .ok_or_else(|| "Sidebar numeric controls require a step value.".to_string())?;
        if minimum >= maximum {
            return Err(
                "Sidebar numeric control maximum must be greater than its minimum.".to_string(),
            );
        }
        if step <= 0.0 {
            return Err("Sidebar numeric control step must be greater than zero.".to_string());
        }
    }
    if node.options.len() > MAX_PICKER_OPTIONS {
        return Err(format!(
            "Sidebar Picker controls are limited to {MAX_PICKER_OPTIONS} options."
        ));
    }
    for option in &node.options {
        validate_text_field(&option.label)?;
        if option.label.trim().is_empty() {
            return Err("Sidebar Picker option labels must not be empty.".to_string());
        }
        validate_state_value(&option.value)?;
        if !is_picker_value(&option.value) {
            return Err(
                "Sidebar Picker options must contain boolean, number, string, or enum values."
                    .to_string(),
            );
        }
    }
    if node.kind == SidebarNodeKind::Picker {
        if node.options.is_empty() {
            return Err("Sidebar Picker controls require at least one option.".to_string());
        }
        for (index, option) in node.options.iter().enumerate() {
            if node.options[..index]
                .iter()
                .any(|candidate| candidate.value == option.value)
            {
                return Err("Sidebar Picker option values must be unique.".to_string());
            }
        }
    } else if !node.options.is_empty() {
        return Err("Sidebar options are only valid on Picker controls.".to_string());
    }
    if let Some(tag) = &node.tag {
        validate_state_value(tag)?;
        if !is_picker_value(tag) {
            return Err(
                "Sidebar tags must contain boolean, number, string, or enum values.".to_string(),
            );
        }
    }
    if let Some(action) = &node.action {
        validate_action(action)?;
    }
    if node.on_change.len() > MAX_NODE_EVENTS || node.on_submit.len() > MAX_NODE_EVENTS {
        return Err(format!(
            "Sidebar nodes are limited to {MAX_NODE_EVENTS} events of each kind."
        ));
    }
    for event in node.on_change.iter().chain(&node.on_submit) {
        validate_text_field(&event.id)?;
        if event.id.trim().is_empty() {
            return Err("Sidebar event IDs must not be empty.".to_string());
        }
        if let Some(key) = &event.key {
            validate_state_key(key)?;
        }
        validate_action(&event.action)?;
    }
    if let Some(reorder) = &node.reorder {
        validate_text_field(&reorder.method)?;
        validate_text_field(&reorder.id_parameter)?;
        validate_text_field(&reorder.item_id)?;
        if reorder.method.trim().is_empty()
            || reorder.id_parameter.trim().is_empty()
            || reorder.item_id.trim().is_empty()
        {
            return Err(
                "Sidebar reorder method, id parameter, and item id must not be empty.".to_string(),
            );
        }
    }
    if node.kind == SidebarNodeKind::Button
        && node
            .action
            .as_ref()
            .is_some_and(|action| action.kind.trim().is_empty() && action.commands.is_empty())
    {
        return Err("Sidebar button action type must not be empty.".to_string());
    }
    for child in &node.children {
        validate_node(child, depth + 1, node_count)?;
    }
    Ok(())
}

fn hydrate_document_bindings(
    node: &mut SidebarNode,
    state: &mut SidebarState,
) -> Result<(), String> {
    hydrate_document_bindings_inner(node, state, &mut BTreeMap::new())
}

fn validate_action(action: &SidebarAction) -> Result<(), String> {
    validate_text_field(&action.kind)?;
    if let Some(message) = &action.message {
        validate_text_field(message)?;
    }
    for (key, value) in &action.params {
        validate_text_field(key)?;
        validate_text_field(value)?;
    }
    for command in &action.commands {
        validate_text_field(&command.kind)?;
        if let Some(method) = &command.method {
            validate_text_field(method)?;
        }
        if let Some(message) = &command.message {
            validate_text_field(message)?;
        }
        for (key, value) in &command.params {
            validate_text_field(key)?;
            validate_text_field(value)?;
        }
        if let Some(operation) = &command.operation {
            validate_text_field(operation)?;
        }
        if let Some(key) = &command.key {
            validate_state_key(key)?;
        }
        if let Some(value) = &command.value {
            validate_state_value(value)?;
        }
        if command.kind == "state"
            && (!matches!(
                command.operation.as_deref(),
                Some("set" | "add" | "toggle" | "append")
            ) || command.key.is_none()
                || (matches!(command.operation.as_deref(), Some("set" | "add" | "append"))
                    && command.value.is_none()))
        {
            return Err(
                "Sidebar state actions require a valid operation and binding key.".to_string(),
            );
        }
    }
    Ok(())
}

fn hydrate_document_bindings_inner(
    node: &mut SidebarNode,
    state: &mut SidebarState,
    declarations: &mut BTreeMap<String, Value>,
) -> Result<(), String> {
    if let Some(binding) = node.binding.as_mut() {
        validate_state_key(&binding.key)?;
        validate_state_value(&binding.value)?;
        if let Some(declared) = declarations.get(&binding.key) {
            if !state_value_types_match(declared, &binding.value) {
                return Err(format!(
                    "Sidebar binding '{}' is declared with conflicting value types.",
                    binding.key
                ));
            }
        } else {
            declarations.insert(binding.key.clone(), binding.value.clone());
        }
        let value = state
            .get(&binding.key)
            .filter(|value| state_value_types_match(value, &binding.value))
            .cloned()
            .unwrap_or_else(|| binding.value.clone());
        state.insert(binding.key.clone(), value.clone());
        binding.value = value;
    }
    for child in &mut node.children {
        hydrate_document_bindings_inner(child, state, declarations)?;
    }
    Ok(())
}

fn state_value_types_match(left: &Value, right: &Value) -> bool {
    match (left, right) {
        (Value::Null, Value::Null)
        | (Value::Bool(_), Value::Bool(_))
        | (Value::String(_), Value::String(_))
        | (Value::Array(_), Value::Array(_)) => true,
        (Value::Object(left), Value::Object(right)) => {
            match (
                left.get(ENUM_TYPE_KEY).and_then(Value::as_str),
                right.get(ENUM_TYPE_KEY).and_then(Value::as_str),
            ) {
                (Some(left), Some(right)) => left == right,
                (None, None) => true,
                _ => false,
            }
        }
        (Value::Number(left), Value::Number(right)) => {
            (left.is_i64() && right.is_i64())
                || (left.is_u64() && right.is_u64())
                || (!left.is_i64() && !left.is_u64() && !right.is_i64() && !right.is_u64())
        }
        _ => false,
    }
}

fn validate_provider_state(provider_id: &str, state: &SidebarState) -> Result<(), String> {
    validate_state_provider(provider_id)?;
    if state.len() > MAX_STATE_ENTRIES_PER_SIDEBAR {
        return Err(format!(
            "Custom sidebar state is limited to {MAX_STATE_ENTRIES_PER_SIDEBAR} values per sidebar."
        ));
    }
    for (key, value) in state {
        validate_state_key(key)?;
        validate_state_value(value)?;
    }
    Ok(())
}

fn is_picker_value(value: &Value) -> bool {
    value.is_boolean() || value.is_number() || value.is_string() || is_enum_value(value)
}

fn is_enum_value(value: &Value) -> bool {
    let Some(value) = value.as_object() else {
        return false;
    };
    value.get(ENUM_TYPE_KEY).is_some_and(Value::is_string)
        && value.get(ENUM_CASE_KEY).is_some_and(Value::is_string)
        && value.get(ENUM_VALUES_KEY).is_some_and(Value::is_array)
        && value.get(ENUM_LABELS_KEY).is_some_and(Value::is_array)
        && value.contains_key(ENUM_RAW_VALUE_KEY)
}

fn validate_state_provider(provider_id: &str) -> Result<(), String> {
    if provider_name(provider_id).is_none() {
        return Err("Custom sidebar state requires a custom sidebar provider ID.".to_string());
    }
    Ok(())
}

fn validate_state_key(key: &str) -> Result<(), String> {
    if key.is_empty()
        || key.chars().count() > MAX_STATE_KEY_CHARS
        || key
            .chars()
            .next()
            .is_some_and(|character| character.is_ascii_digit())
        || !key
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || character == '_')
    {
        return Err("Custom sidebar state key is invalid.".to_string());
    }
    Ok(())
}

fn validate_state_value(value: &Value) -> Result<(), String> {
    let mut items = 0usize;
    validate_state_value_at_depth(value, 0, &mut items)
}

fn validate_state_value_at_depth(
    value: &Value,
    depth: usize,
    items: &mut usize,
) -> Result<(), String> {
    if depth > MAX_STATE_VALUE_DEPTH {
        return Err(format!(
            "Custom sidebar state exceeds the maximum depth of {MAX_STATE_VALUE_DEPTH}."
        ));
    }
    *items = items.saturating_add(1);
    if *items > MAX_STATE_VALUE_ITEMS {
        return Err(format!(
            "Custom sidebar state exceeds the maximum item count of {MAX_STATE_VALUE_ITEMS}."
        ));
    }
    match value {
        Value::Null | Value::Bool(_) => Ok(()),
        Value::Number(number) if number.as_f64().is_some_and(f64::is_finite) => Ok(()),
        Value::Number(_) => Err("Custom sidebar state numbers must be finite.".to_string()),
        Value::String(value) => validate_text_field(value),
        Value::Array(values) => {
            for value in values {
                validate_state_value_at_depth(value, depth + 1, items)?;
            }
            Ok(())
        }
        Value::Object(values) => {
            for (key, value) in values {
                validate_text_field(key)?;
                validate_state_value_at_depth(value, depth + 1, items)?;
            }
            Ok(())
        }
    }
}

fn validate_text_field(value: &str) -> Result<(), String> {
    if value.chars().count() > MAX_TEXT_CHARS {
        return Err(format!(
            "Sidebar text fields are limited to {MAX_TEXT_CHARS} characters."
        ));
    }
    Ok(())
}

fn describe_json_error(error: &serde_json::Error) -> String {
    if error.line() > 0 {
        format!(
            "Invalid sidebar JSON at line {}, column {}: {}",
            error.line(),
            error.column(),
            error
        )
    } else {
        format!("Invalid sidebar JSON: {error}")
    }
}

fn normalized_env(key: &str) -> Option<String> {
    let value = std::env::var(key).ok()?;
    let trimmed = value.trim();
    (!trimmed.is_empty()).then(|| trimmed.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write(path: &Path, contents: &str) {
        fs::write(path, contents).expect("write sidebar fixture");
    }

    #[test]
    fn discovery_prefers_swift_and_sorts_names() {
        let directory = tempfile::tempdir().expect("sidebar tempdir");
        write(
            &directory.path().join("finder.json"),
            r#"{"version":1,"root":{"type":"text","text":"JSON"}}"#,
        );
        write(&directory.path().join("finder.swift"), r#"Text("Swift")"#);
        write(
            &directory.path().join("alpha.json"),
            r#"{"version":1,"root":{"type":"text","text":"Alpha"}}"#,
        );
        assert_eq!(
            discover(directory.path(), None)
                .into_iter()
                .map(|path| path.file_name().unwrap().to_string_lossy().into_owned())
                .collect::<Vec<_>>(),
            vec!["alpha.json", "finder.swift"]
        );
    }

    #[test]
    fn validation_accepts_document_and_rejects_unknown_version() {
        let directory = tempfile::tempdir().expect("sidebar tempdir");
        let valid = directory.path().join("valid.json");
        write(
            &valid,
            r##"{"version":1,"root":{"type":"vstack","spacing":8,"children":[{"type":"text","text":"Status","color":"#ff8800"},{"type":"button","title":"Open","action":{"type":"workspace.next"}}]}}"##,
        );
        assert!(validate_path(&valid).ok);

        let invalid = directory.path().join("invalid.json");
        write(
            &invalid,
            r#"{"version":2,"root":{"type":"text","text":"No"}}"#,
        );
        assert_eq!(
            validate_path(&invalid).error.as_deref(),
            Some("Unsupported sidebar document version 2; expected 1.")
        );
    }

    #[test]
    fn validation_accepts_bounded_input_controls_and_rejects_ambiguous_picker_values() {
        let directory = tempfile::tempdir().expect("sidebar tempdir");
        let valid = directory.path().join("controls.json");
        write(
            &valid,
            r#"{
                "version": 1,
                "root": {
                    "type": "vstack",
                    "children": [
                        {
                            "type": "slider",
                            "minimum": 0,
                            "maximum": 1,
                            "step": 0.1,
                            "binding": {"key": "volume", "value": 0.5}
                        },
                        {
                            "type": "picker",
                            "title": "Mode",
                            "binding": {"key": "mode", "value": "balanced"},
                            "options": [
                                {"label": "Fast", "value": "fast"},
                                {"label": "Balanced", "value": "balanced"}
                            ]
                        },
                        {
                            "type": "stepper",
                            "minimum": 0,
                            "maximum": 10,
                            "step": 1,
                            "binding": {"key": "count", "value": 2}
                        }
                    ]
                }
            }"#,
        );
        assert!(validate_path(&valid).ok);

        let duplicate = directory.path().join("duplicate.json");
        write(
            &duplicate,
            r#"{
                "version": 1,
                "root": {
                    "type": "picker",
                    "binding": {"key": "mode", "value": "fast"},
                    "options": [
                        {"label": "Fast", "value": "fast"},
                        {"label": "Also fast", "value": "fast"}
                    ]
                }
            }"#,
        );
        assert_eq!(
            validate_path(&duplicate).error.as_deref(),
            Some("Sidebar Picker option values must be unique.")
        );

        let invalid_range = directory.path().join("invalid-range.json");
        write(
            &invalid_range,
            r#"{
                "version": 1,
                "root": {
                    "type": "slider",
                    "minimum": 1,
                    "maximum": 1,
                    "step": 0.1,
                    "binding": {"key": "volume", "value": 1}
                }
            }"#,
        );
        assert_eq!(
            validate_path(&invalid_range).error.as_deref(),
            Some("Sidebar numeric control maximum must be greater than its minimum.")
        );
    }

    #[test]
    fn validation_accepts_tagged_enum_picker_values_and_checks_enum_types() {
        let directory = tempfile::tempdir().expect("sidebar tempdir");
        let valid = directory.path().join("enum-picker.json");
        write(
            &valid,
            r#"{
                "version": 1,
                "root": {
                    "type": "picker",
                    "binding": {
                        "key": "mode",
                        "value": {
                            "__cmux_enum_type": "Mode",
                            "__cmux_enum_case": "compact",
                            "__cmux_enum_values": [],
                            "__cmux_enum_labels": [],
                            "__cmux_enum_raw_value": "compact"
                        }
                    },
                    "options": [
                        {
                            "label": "Compact",
                            "value": {
                                "__cmux_enum_type": "Mode",
                                "__cmux_enum_case": "compact",
                                "__cmux_enum_values": [],
                                "__cmux_enum_labels": [],
                                "__cmux_enum_raw_value": "compact"
                            }
                        },
                        {
                            "label": "Expanded",
                            "value": {
                                "__cmux_enum_type": "Mode",
                                "__cmux_enum_case": "expanded",
                                "__cmux_enum_values": [],
                                "__cmux_enum_labels": [],
                                "__cmux_enum_raw_value": "expanded"
                            }
                        }
                    ]
                }
            }"#,
        );
        assert!(validate_path(&valid).ok);

        let mode = json!({
            "__cmux_enum_type": "Mode",
            "__cmux_enum_case": "compact",
            "__cmux_enum_values": [],
            "__cmux_enum_labels": [],
            "__cmux_enum_raw_value": "compact"
        });
        let expanded = json!({
            "__cmux_enum_type": "Mode",
            "__cmux_enum_case": "expanded",
            "__cmux_enum_values": [],
            "__cmux_enum_labels": [],
            "__cmux_enum_raw_value": "expanded"
        });
        let other = json!({
            "__cmux_enum_type": "Other",
            "__cmux_enum_case": "compact",
            "__cmux_enum_values": [],
            "__cmux_enum_labels": [],
            "__cmux_enum_raw_value": "compact"
        });
        assert!(state_value_types_match(&mode, &expanded));
        assert!(!state_value_types_match(&mode, &other));
        assert!(!is_enum_value(&json!({
            "__cmux_enum_type": "Mode",
            "__cmux_enum_case": "compact"
        })));
    }

    #[test]
    fn json_control_bindings_seed_and_reuse_provider_state_atomically() {
        let directory = tempfile::tempdir().expect("sidebar tempdir");
        let path = directory.path().join("controls.json");
        write(
            &path,
            r#"{
                "version": 1,
                "root": {
                    "type": "vstack",
                    "children": [
                        {
                            "type": "toggle",
                            "binding": {"key": "enabled", "value": true}
                        },
                        {
                            "type": "slider",
                            "minimum": 0,
                            "maximum": 1,
                            "step": 0.1,
                            "binding": {"key": "volume", "value": 0.5}
                        },
                        {
                            "type": "picker",
                            "binding": {"key": "mode", "value": "balanced"},
                            "options": [
                                {"label": "Fast", "value": "fast"},
                                {"label": "Balanced", "value": "balanced"}
                            ]
                        }
                    ]
                }
            }"#,
        );
        let mut state = SidebarState::from([
            ("enabled".to_string(), json!(false)),
            ("volume".to_string(), json!("wrong type")),
            ("mode".to_string(), json!("fast")),
        ]);
        let document = read_document_with_context_and_state(&path, &json!({}), &mut state)
            .expect("read bound JSON sidebar");
        assert_eq!(state["enabled"], false);
        assert_eq!(state["volume"], 0.5);
        assert_eq!(state["mode"], "fast");
        assert_eq!(
            document.root.children[0].binding.as_ref().unwrap().value,
            json!(false)
        );
        assert_eq!(
            document.root.children[1].binding.as_ref().unwrap().value,
            json!(0.5)
        );
        assert_eq!(
            document.root.children[2].binding.as_ref().unwrap().value,
            json!("fast")
        );

        write(
            &path,
            r#"{
                "version": 1,
                "root": {
                    "type": "vstack",
                    "children": [
                        {"type": "toggle", "binding": {"key": "shared", "value": true}},
                        {"type": "textfield", "binding": {"key": "shared", "value": "text"}}
                    ]
                }
            }"#,
        );
        let before = state.clone();
        assert!(
            read_document_with_context_and_state(&path, &json!({}), &mut state)
                .expect_err("conflicting JSON bindings")
                .contains("conflicting value types")
        );
        assert_eq!(state, before);
    }

    #[test]
    fn selected_snapshot_keeps_last_good_document() {
        let directory = tempfile::tempdir().expect("sidebar tempdir");
        let path = directory.path().join("status.json");
        write(
            &path,
            r#"{"version":1,"root":{"type":"text","text":"Good"}}"#,
        );
        let provider = provider_id("status");
        let context = validation_context();
        let mut state = SidebarState::new();
        let (ready, last_good) =
            selected_snapshot(directory.path(), &provider, 1, None, &context, &mut state);
        assert_eq!(ready["state"], "ready");
        let last_good = last_good.expect("last good");

        write(&path, r#"{"version":1,"root":{"type":"text""#);
        let (stale, replacement) = selected_snapshot(
            directory.path(),
            &provider,
            2,
            Some(&last_good),
            &context,
            &mut state,
        );
        assert_eq!(stale["state"], "stale");
        assert_eq!(stale["using_last_good"], true);
        assert_eq!(stale["document"]["root"]["text"], "Good");
        assert!(replacement.is_none());
    }

    #[test]
    fn selection_round_trip_is_private() {
        let directory = tempfile::tempdir().expect("sidebar tempdir");
        let path = directory.path().join("selection.json");
        save_selected_provider(&path, &provider_id("finder")).expect("save selection");
        assert_eq!(load_selected_provider(&path), provider_id("finder"));
        assert_eq!(
            fs::metadata(path)
                .expect("selection metadata")
                .permissions()
                .mode()
                & 0o777,
            0o600
        );
    }

    #[test]
    fn state_store_round_trip_is_private_and_provider_scoped() {
        let directory = tempfile::tempdir().expect("sidebar tempdir");
        let path = directory.path().join("state.json");
        let alpha = provider_id("alpha");
        let beta = provider_id("beta");
        let mut store = SidebarStateStore::default();
        assert!(store
            .set(&alpha, "enabled", json!(true))
            .expect("set alpha state"));
        assert!(store
            .set(&beta, "enabled", json!(false))
            .expect("set beta state"));
        save_state_store(&path, &store).expect("save state");

        let loaded = load_state_store(&path);
        assert_eq!(loaded.sidebar_state(&alpha)["enabled"], true);
        assert_eq!(loaded.sidebar_state(&beta)["enabled"], false);
        assert_eq!(
            fs::metadata(path)
                .expect("state metadata")
                .permissions()
                .mode()
                & 0o777,
            0o600
        );
    }

    #[test]
    fn state_store_rejects_invalid_keys_and_oversized_values() {
        let mut store = SidebarStateStore::default();
        let provider = provider_id("status");
        assert!(store.set(&provider, "bad-key", json!(true)).is_err());
        assert!(store
            .set(&provider, "message", json!("x".repeat(MAX_TEXT_CHARS + 1)))
            .is_err());
        assert!(store
            .set(DEFAULT_PROVIDER_ID, "enabled", json!(true))
            .is_err());
    }
}
