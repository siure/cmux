use crate::custom_sidebar::{self, SidebarDocument};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::fs::{self, OpenOptions};
use std::io::{Read, Write};
use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};

pub const HOSTED_PROVIDER_ID: &str = "cmux.sidebar.extensions";
const PROTOCOL_VERSION: u32 = 1;
const STATE_VERSION: u32 = 1;
const MAX_MANIFEST_BYTES: u64 = 64 * 1024;
const MAX_REQUEST_BYTES: usize = 8 * 1024 * 1024;
const MAX_RESPONSE_BYTES: usize = 32 * 1024 * 1024;
const MAX_STDERR_BYTES: usize = 64 * 1024;
const DEFAULT_TIMEOUT: Duration = Duration::from_secs(2);

pub const READ_SCOPES: &[&str] = &[
    "workspaceList",
    "workspaceMetadata",
    "surfaceMetadata",
    "workspacePaths",
    "notifications",
    "networkPorts",
    "pullRequests",
];

pub const ACTION_SCOPES: &[&str] = &[
    "createWorkspace",
    "selectWorkspace",
    "closeWorkspace",
    "createSurface",
    "selectSurface",
    "closeSurface",
    "splitSurface",
    "zoomSurface",
    "navigateWorkspace",
    "navigateSurface",
    "openURL",
    "createWorkspaceWithPath",
];

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ApiVersion {
    pub major: u32,
    pub minor: u32,
}

impl Default for ApiVersion {
    fn default() -> Self {
        Self { major: 2, minor: 0 }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Manifest {
    pub id: String,
    pub display_name: String,
    #[serde(default)]
    pub minimum_api_version: ApiVersion,
    #[serde(default)]
    pub read_scopes: Vec<String>,
    #[serde(default)]
    pub action_scopes: Vec<String>,
    pub entrypoint: String,
    #[serde(default)]
    pub arguments: Vec<String>,
}

#[derive(Clone, Debug)]
pub struct Descriptor {
    pub manifest: Manifest,
    pub root: PathBuf,
    pub executable: PathBuf,
    pub fingerprint: String,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
struct Grant {
    manifest_fingerprint: String,
    #[serde(default)]
    read_scopes: BTreeSet<String>,
    #[serde(default)]
    action_scopes: BTreeSet<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct State {
    version: u32,
    #[serde(default)]
    selected_extension_id: Option<String>,
    #[serde(default)]
    grants: BTreeMap<String, Grant>,
}

impl Default for State {
    fn default() -> Self {
        Self {
            version: STATE_VERSION,
            selected_extension_id: None,
            grants: BTreeMap::new(),
        }
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct WorkerRequest<'a> {
    protocol_version: u32,
    request_type: &'static str,
    manifest: &'a Manifest,
    granted_read_scopes: &'a BTreeSet<String>,
    granted_action_scopes: &'a BTreeSet<String>,
    snapshot: &'a Value,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct WorkerResponse {
    protocol_version: u32,
    document: Option<SidebarDocument>,
    error: Option<String>,
}

pub fn extensions_dir() -> PathBuf {
    if let Some(path) = normalized_env("CMUX_SIDEBAR_EXTENSIONS_DIR") {
        return PathBuf::from(path);
    }
    if let Some(path) = normalized_env("XDG_CONFIG_HOME") {
        return PathBuf::from(path).join("cmux/extensions");
    }
    if let Some(home) = normalized_env("HOME") {
        return PathBuf::from(home).join(".config/cmux/extensions");
    }
    std::env::temp_dir().join("cmux-extensions")
}

pub fn state_path() -> PathBuf {
    if let Some(path) = normalized_env("CMUX_SIDEBAR_EXTENSION_STATE_PATH") {
        return PathBuf::from(path);
    }
    if let Some(path) = normalized_env("XDG_STATE_HOME") {
        return PathBuf::from(path).join("cmux/sidebar-extensions.json");
    }
    if let Some(home) = normalized_env("HOME") {
        return PathBuf::from(home).join(".local/state/cmux/sidebar-extensions.json");
    }
    std::env::temp_dir().join("cmux-sidebar-extensions.json")
}

pub fn discover() -> Vec<Result<Descriptor, String>> {
    let directory = extensions_dir();
    let Ok(entries) = fs::read_dir(&directory) else {
        return Vec::new();
    };
    let mut manifests = entries
        .flatten()
        .map(|entry| entry.path().join("manifest.json"))
        .filter(|path| path.is_file())
        .collect::<Vec<_>>();
    manifests.sort();
    manifests
        .into_iter()
        .map(|path| load_descriptor(&path))
        .collect()
}

pub fn valid_descriptors() -> Vec<Descriptor> {
    discover().into_iter().filter_map(Result::ok).collect()
}

pub fn selected_extension_id() -> Option<String> {
    selected_descriptor().map(|descriptor| descriptor.manifest.id)
}

pub fn selected_descriptor() -> Option<Descriptor> {
    let descriptors = valid_descriptors();
    let selected = load_state().selected_extension_id;
    selected
        .as_deref()
        .and_then(|selected| {
            descriptors
                .iter()
                .find(|descriptor| descriptor.manifest.id == selected)
                .cloned()
        })
        .or_else(|| descriptors.into_iter().next())
}

pub fn select(extension_id: &str) -> Result<(), String> {
    if !valid_descriptors()
        .iter()
        .any(|descriptor| descriptor.manifest.id == extension_id)
    {
        return Err("Sidebar extension was not found.".to_string());
    }
    let mut state = load_state();
    state.selected_extension_id = Some(extension_id.to_string());
    save_state(&state)
}

pub fn grant_requested(extension_id: &str) -> Result<(), String> {
    let descriptor = valid_descriptors()
        .into_iter()
        .find(|descriptor| descriptor.manifest.id == extension_id)
        .ok_or_else(|| "Sidebar extension was not found.".to_string())?;
    let mut state = load_state();
    state.grants.insert(
        extension_id.to_string(),
        Grant {
            manifest_fingerprint: descriptor.fingerprint,
            read_scopes: descriptor.manifest.read_scopes.iter().cloned().collect(),
            action_scopes: descriptor.manifest.action_scopes.iter().cloned().collect(),
        },
    );
    save_state(&state)
}

pub fn revoke(extension_id: &str) -> Result<(), String> {
    let mut state = load_state();
    state.grants.remove(extension_id);
    save_state(&state)
}

pub fn status_value() -> Value {
    let state = load_state();
    let effective_selected = selected_descriptor().map(|descriptor| descriptor.manifest.id);
    let entries = discover()
        .into_iter()
        .map(|result| match result {
            Ok(descriptor) => {
                let grant = effective_grant(&state, &descriptor);
                json!({
                    "id": descriptor.manifest.id,
                    "displayName": descriptor.manifest.display_name,
                    "minimumAPIVersion": descriptor.manifest.minimum_api_version,
                    "readScopes": descriptor.manifest.read_scopes,
                    "actionScopes": descriptor.manifest.action_scopes,
                    "grantedReadScopes": grant.read_scopes,
                    "grantedActionScopes": grant.action_scopes,
                    "approved": grant_is_complete(&descriptor, &grant),
                    "selected": effective_selected.as_deref() == Some(descriptor.manifest.id.as_str()),
                    "path": descriptor.root.display().to_string(),
                    "ok": true,
                    "error": Value::Null
                })
            }
            Err(error) => json!({
                "ok": false,
                "error": error
            }),
        })
        .collect::<Vec<_>>();
    json!({
        "supported": true,
        "directory": extensions_dir().display().to_string(),
        "statePath": state_path().display().to_string(),
        "selectedExtensionId": effective_selected,
        "extensions": entries,
        "sandbox": sandbox_name()
    })
}

pub fn render_selected(snapshot: &Value) -> Result<(Descriptor, SidebarDocument), String> {
    let descriptor =
        selected_descriptor().ok_or_else(|| "No sidebar extension is installed.".to_string())?;
    let state = load_state();
    let grant = effective_grant(&state, &descriptor);
    if !grant_is_complete(&descriptor, &grant) {
        return Err("Sidebar extension access has not been approved.".to_string());
    }
    let filtered = filter_snapshot(snapshot, &grant.read_scopes, &grant.action_scopes);
    let request = serde_json::to_vec(&WorkerRequest {
        protocol_version: PROTOCOL_VERSION,
        request_type: "render",
        manifest: &descriptor.manifest,
        granted_read_scopes: &grant.read_scopes,
        granted_action_scopes: &grant.action_scopes,
        snapshot: &filtered,
    })
    .map_err(|error| format!("Failed to encode sidebar extension request: {error}"))?;
    if request.len() > MAX_REQUEST_BYTES {
        return Err(format!(
            "Sidebar extension request exceeds the {MAX_REQUEST_BYTES} byte limit."
        ));
    }
    let response = run_worker(&descriptor, &request)?;
    if response.protocol_version != PROTOCOL_VERSION {
        return Err(format!(
            "Sidebar extension protocol version {} is unsupported; expected {PROTOCOL_VERSION}.",
            response.protocol_version
        ));
    }
    let mut document = response.document.ok_or_else(|| {
        response
            .error
            .unwrap_or_else(|| "Sidebar extension returned no document.".to_string())
    })?;
    custom_sidebar::validate_document(&document)?;
    validate_extension_actions(&document, &grant.action_scopes)?;
    bind_extension_id(&mut document, &descriptor.manifest.id);
    Ok((descriptor, document))
}

pub fn action_scope(
    kind: &str,
    params: &serde_json::Map<String, Value>,
) -> Option<BTreeSet<String>> {
    let mut scopes = BTreeSet::new();
    match kind {
        "createWorkspace" => {
            scopes.insert("createWorkspace".to_string());
            if params
                .get("working_directory")
                .or_else(|| params.get("workingDirectory"))
                .and_then(Value::as_str)
                .is_some_and(|value| !value.trim().is_empty())
            {
                scopes.insert("createWorkspaceWithPath".to_string());
            }
        }
        "selectWorkspace" => {
            scopes.insert("selectWorkspace".to_string());
        }
        "closeWorkspace" => {
            scopes.insert("closeWorkspace".to_string());
        }
        "selectNextWorkspace" | "selectPreviousWorkspace" => {
            scopes.insert("navigateWorkspace".to_string());
        }
        "createTerminalSurface" => {
            scopes.insert("createSurface".to_string());
        }
        "createBrowserSurface" => {
            scopes.insert("createSurface".to_string());
            if params
                .get("url")
                .and_then(Value::as_str)
                .is_some_and(|value| !value.trim().is_empty())
            {
                scopes.insert("openURL".to_string());
            }
        }
        "selectSurface" => {
            scopes.insert("selectSurface".to_string());
        }
        "selectNextSurface" | "selectPreviousSurface" => {
            scopes.insert("navigateSurface".to_string());
        }
        "closeSurface" => {
            scopes.insert("closeSurface".to_string());
        }
        "splitTerminal" => {
            scopes.insert("splitSurface".to_string());
        }
        "splitBrowser" => {
            scopes.insert("splitSurface".to_string());
            if params
                .get("url")
                .and_then(Value::as_str)
                .is_some_and(|value| !value.trim().is_empty())
            {
                scopes.insert("openURL".to_string());
            }
        }
        "toggleSurfaceZoom" => {
            scopes.insert("zoomSurface".to_string());
        }
        "openURL" => {
            scopes.insert("openURL".to_string());
        }
        _ => return None,
    }
    Some(scopes)
}

pub fn action_is_granted(extension_id: &str, kind: &str, params: &Value) -> bool {
    let Some(descriptor) = valid_descriptors()
        .into_iter()
        .find(|descriptor| descriptor.manifest.id == extension_id)
    else {
        return false;
    };
    let state = load_state();
    let grant = effective_grant(&state, &descriptor);
    let Some(params) = params.as_object() else {
        return false;
    };
    action_scope(kind, params).is_some_and(|required| required.is_subset(&grant.action_scopes))
}

fn load_descriptor(path: &Path) -> Result<Descriptor, String> {
    let metadata = fs::symlink_metadata(path)
        .map_err(|error| format!("Failed to inspect {}: {error}", path.display()))?;
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        return Err(format!(
            "{} must be a regular manifest file.",
            path.display()
        ));
    }
    if metadata.len() > MAX_MANIFEST_BYTES {
        return Err(format!(
            "{} exceeds the {MAX_MANIFEST_BYTES} byte manifest limit.",
            path.display()
        ));
    }
    let bytes =
        fs::read(path).map_err(|error| format!("Failed to read {}: {error}", path.display()))?;
    let manifest = serde_json::from_slice::<Manifest>(&bytes).map_err(|error| {
        format!(
            "Invalid sidebar extension manifest {}: {error}",
            path.display()
        )
    })?;
    validate_manifest(&manifest)?;
    let root = path
        .parent()
        .ok_or_else(|| "Sidebar extension manifest has no parent directory.".to_string())?
        .canonicalize()
        .map_err(|error| format!("Failed to resolve extension root: {error}"))?;
    let executable = root.join(&manifest.entrypoint);
    let executable_metadata = fs::symlink_metadata(&executable).map_err(|error| {
        format!(
            "Sidebar extension entrypoint {} is unavailable: {error}",
            executable.display()
        )
    })?;
    if executable_metadata.file_type().is_symlink()
        || !executable_metadata.is_file()
        || executable_metadata.permissions().mode() & 0o111 == 0
    {
        return Err(format!(
            "Sidebar extension entrypoint {} must be a regular executable file.",
            executable.display()
        ));
    }
    let executable = executable
        .canonicalize()
        .map_err(|error| format!("Failed to resolve extension entrypoint: {error}"))?;
    if !executable.starts_with(&root) {
        return Err(
            "Sidebar extension entrypoint must remain inside its extension directory.".to_string(),
        );
    }
    let fingerprint = Sha256::digest(
        serde_json::to_vec(&manifest)
            .map_err(|error| format!("Failed to fingerprint extension manifest: {error}"))?,
    )
    .iter()
    .map(|byte| format!("{byte:02x}"))
    .collect();
    Ok(Descriptor {
        manifest,
        root,
        executable,
        fingerprint,
    })
}

fn validate_manifest(manifest: &Manifest) -> Result<(), String> {
    if manifest.id.trim().is_empty()
        || manifest.id.len() > 255
        || !manifest.id.chars().all(|character| {
            character.is_ascii_alphanumeric() || matches!(character, '.' | '-' | '_')
        })
    {
        return Err(
            "Sidebar extension id must be a non-empty reverse-DNS style identifier.".to_string(),
        );
    }
    if manifest.display_name.trim().is_empty() || manifest.display_name.chars().count() > 256 {
        return Err("Sidebar extension displayName is invalid.".to_string());
    }
    if manifest.minimum_api_version.major != 2 || manifest.minimum_api_version.minor > 0 {
        return Err(format!(
            "Sidebar extension API {}.{} is unsupported; Linux supports 2.0.",
            manifest.minimum_api_version.major, manifest.minimum_api_version.minor
        ));
    }
    validate_scope_list(&manifest.read_scopes, READ_SCOPES, "read")?;
    validate_scope_list(&manifest.action_scopes, ACTION_SCOPES, "action")?;
    let entrypoint = Path::new(&manifest.entrypoint);
    if manifest.entrypoint.trim().is_empty()
        || entrypoint.is_absolute()
        || entrypoint
            .components()
            .any(|component| matches!(component, std::path::Component::ParentDir))
    {
        return Err(
            "Sidebar extension entrypoint must be a relative path inside the extension directory."
                .to_string(),
        );
    }
    if manifest.arguments.len() > 64
        || manifest
            .arguments
            .iter()
            .any(|argument| argument.len() > 4096 || argument.contains('\0'))
    {
        return Err("Sidebar extension arguments exceed the allowed limits.".to_string());
    }
    Ok(())
}

fn validate_scope_list(scopes: &[String], allowed: &[&str], kind: &str) -> Result<(), String> {
    if scopes.len() > allowed.len() {
        return Err(format!(
            "Sidebar extension requested too many {kind} scopes."
        ));
    }
    for scope in scopes {
        if !allowed.contains(&scope.as_str()) {
            return Err(format!("Unknown sidebar extension {kind} scope: {scope}"));
        }
    }
    Ok(())
}

fn effective_grant(state: &State, descriptor: &Descriptor) -> Grant {
    state
        .grants
        .get(&descriptor.manifest.id)
        .filter(|grant| grant.manifest_fingerprint == descriptor.fingerprint)
        .cloned()
        .unwrap_or_default()
}

fn grant_is_complete(descriptor: &Descriptor, grant: &Grant) -> bool {
    descriptor
        .manifest
        .read_scopes
        .iter()
        .all(|scope| grant.read_scopes.contains(scope))
        && descriptor
            .manifest
            .action_scopes
            .iter()
            .all(|scope| grant.action_scopes.contains(scope))
}

fn filter_snapshot(
    snapshot: &Value,
    read_scopes: &BTreeSet<String>,
    action_scopes: &BTreeSet<String>,
) -> Value {
    let metadata = read_scopes.contains("workspaceMetadata");
    let list = read_scopes.contains("workspaceList") || metadata;
    let workspaces = if list {
        snapshot
            .get("workspaces")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .map(|workspace| {
                if !metadata {
                    return json!({
                        "id": workspace.get("id"),
                        "title": ""
                    });
                }
                let mut row = json!({
                    "id": workspace.get("id"),
                    "title": workspace.get("title"),
                    "detail": workspace.get("description"),
                    "isPinned": workspace.get("pinned").and_then(Value::as_bool).unwrap_or(false),
                    "gitBranch": workspace.get("branch_summary"),
                    "unreadCount": workspace.get("unread_count").and_then(Value::as_u64).unwrap_or(0),
                });
                if read_scopes.contains("workspacePaths") {
                    row["rootPath"] = workspace.get("root_path").cloned().unwrap_or(Value::Null);
                    row["projectRootPath"] = workspace
                        .get("project_root_path")
                        .cloned()
                        .unwrap_or(Value::Null);
                }
                if read_scopes.contains("notifications") {
                    row["latestNotification"] = workspace
                        .get("latest_notification_text")
                        .cloned()
                        .unwrap_or(Value::Null);
                }
                if read_scopes.contains("networkPorts") {
                    row["listeningPorts"] = workspace
                        .get("listening_ports")
                        .cloned()
                        .unwrap_or_else(|| json!([]));
                }
                if read_scopes.contains("pullRequests") {
                    row["pullRequestURLs"] = workspace
                        .get("pull_request_urls")
                        .cloned()
                        .unwrap_or_else(|| json!([]));
                }
                if read_scopes.contains("surfaceMetadata") {
                    row["surfaces"] = workspace
                        .get("surfaces")
                        .cloned()
                        .unwrap_or_else(|| json!([]));
                }
                row
            })
            .collect::<Vec<_>>()
    } else {
        Vec::new()
    };
    json!({
        "apiVersion": {"major": 2, "minor": 0},
        "sequence": snapshot.get("sequence").or_else(|| snapshot.get("seq")).and_then(Value::as_u64).unwrap_or(0),
        "windowID": metadata.then(|| snapshot.get("window_id").cloned().unwrap_or(Value::Null)).unwrap_or(Value::Null),
        "selectedWorkspaceID": metadata.then(|| snapshot.get("selected_workspace_id").cloned().unwrap_or(Value::Null)).unwrap_or(Value::Null),
        "grantedReadScopes": read_scopes,
        "grantedActionScopes": action_scopes,
        "workspaces": workspaces
    })
}

fn validate_extension_actions(
    document: &SidebarDocument,
    granted_scopes: &BTreeSet<String>,
) -> Result<(), String> {
    fn visit(
        node: &crate::custom_sidebar::SidebarNode,
        granted: &BTreeSet<String>,
    ) -> Result<(), String> {
        if node.reorder.is_some() {
            return Err(
                "Sidebar extensions cannot emit raw reorder methods; use typed extension actions."
                    .to_string(),
            );
        }
        if let Some(action) = node.action.as_ref() {
            let actions = if action.commands.is_empty() {
                vec![(action.kind.as_str(), &action.params)]
            } else {
                action
                    .commands
                    .iter()
                    .map(|command| (command.kind.as_str(), &command.params))
                    .collect()
            };
            for (kind, params) in actions {
                if kind != "extension" {
                    return Err(
                        "Sidebar extensions may only emit typed `extension` actions.".to_string(),
                    );
                }
                let action_kind = params.get("action").map(String::as_str).ok_or_else(|| {
                    "Sidebar extension action is missing params.action.".to_string()
                })?;
                let value_params = params
                    .iter()
                    .filter(|(key, _)| key.as_str() != "action")
                    .map(|(key, value)| (key.clone(), Value::String(value.clone())))
                    .collect::<serde_json::Map<_, _>>();
                let required = action_scope(action_kind, &value_params)
                    .ok_or_else(|| format!("Unknown sidebar extension action: {action_kind}"))?;
                if !required.is_subset(granted) {
                    return Err(format!(
                        "Sidebar extension action {action_kind} requires ungranted scopes."
                    ));
                }
            }
        }
        for child in &node.children {
            visit(child, granted)?;
        }
        Ok(())
    }
    visit(&document.root, granted_scopes)
}

fn bind_extension_id(document: &mut SidebarDocument, extension_id: &str) {
    fn visit(node: &mut crate::custom_sidebar::SidebarNode, extension_id: &str) {
        if let Some(action) = node.action.as_mut() {
            if action.commands.is_empty() {
                action
                    .params
                    .insert("extension_id".to_string(), extension_id.to_string());
            } else {
                for command in &mut action.commands {
                    command
                        .params
                        .insert("extension_id".to_string(), extension_id.to_string());
                }
            }
        }
        for child in &mut node.children {
            visit(child, extension_id);
        }
    }
    visit(&mut document.root, extension_id);
}

fn run_worker(descriptor: &Descriptor, request: &[u8]) -> Result<WorkerResponse, String> {
    run_worker_with_options(
        descriptor,
        request,
        worker_timeout(),
        normalized_env("CMUX_SIDEBAR_EXTENSION_SANDBOX").as_deref() != Some("0"),
    )
}

fn run_worker_with_options(
    descriptor: &Descriptor,
    request: &[u8],
    timeout: Duration,
    sandbox: bool,
) -> Result<WorkerResponse, String> {
    let mut command = sandboxed_command(descriptor, sandbox)?;
    command.process_group(0);
    let mut child = command
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|error| format!("Failed to start sidebar extension: {error}"))?;
    let mut stdin = child
        .stdin
        .take()
        .ok_or_else(|| "Sidebar extension stdin was unavailable.".to_string())?;
    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| "Sidebar extension stdout was unavailable.".to_string())?;
    let stderr = child
        .stderr
        .take()
        .ok_or_else(|| "Sidebar extension stderr was unavailable.".to_string())?;
    let request = request.to_vec();
    let stdin_writer = thread::spawn(move || {
        stdin
            .write_all(&request)
            .map_err(|error| format!("Failed to write sidebar extension request: {error}"))
    });
    let stdout_reader = thread::spawn(move || read_bounded(stdout, MAX_RESPONSE_BYTES));
    let stderr_reader = thread::spawn(move || read_bounded(stderr, MAX_STDERR_BYTES));
    let deadline = Instant::now() + timeout;
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) if Instant::now() < deadline => thread::sleep(Duration::from_millis(10)),
            Ok(None) => {
                terminate_child_group(&mut child);
                let _ = child.wait();
                return Err(format!(
                    "Sidebar extension timed out after {} milliseconds.",
                    timeout.as_millis()
                ));
            }
            Err(error) => {
                terminate_child_group(&mut child);
                let _ = child.wait();
                return Err(format!("Failed to wait for sidebar extension: {error}"));
            }
        }
    };
    stdin_writer
        .join()
        .map_err(|_| "Sidebar extension stdin writer panicked.".to_string())??;
    let stdout = stdout_reader
        .join()
        .map_err(|_| "Sidebar extension stdout reader panicked.".to_string())??;
    let stderr = stderr_reader
        .join()
        .map_err(|_| "Sidebar extension stderr reader panicked.".to_string())??;
    if !status.success() {
        let detail = String::from_utf8_lossy(&stderr);
        return Err(format!(
            "Sidebar extension exited with {status}{}",
            if detail.trim().is_empty() {
                String::new()
            } else {
                format!(": {}", detail.trim())
            }
        ));
    }
    serde_json::from_slice::<WorkerResponse>(&stdout)
        .map_err(|error| format!("Sidebar extension returned invalid JSON: {error}"))
}

fn terminate_child_group(child: &mut std::process::Child) {
    let group = format!("-{}", child.id());
    let _ = Command::new("/usr/bin/kill")
        .args(["-KILL", "--", group.as_str()])
        .status();
    let _ = child.kill();
}

fn sandboxed_command(descriptor: &Descriptor, sandbox: bool) -> Result<Command, String> {
    if !sandbox {
        let mut command = Command::new(&descriptor.executable);
        command.args(&descriptor.manifest.arguments);
        command.current_dir(&descriptor.root);
        command.env_clear();
        command.env("PATH", "/usr/bin:/bin");
        return Ok(command);
    }
    let bwrap = Path::new("/usr/bin/bwrap");
    if !bwrap.is_file() {
        return Err(
            "Bubblewrap is required to run Linux sidebar extensions securely. Install `bubblewrap`."
                .to_string(),
        );
    }
    let executable_relative = descriptor
        .executable
        .strip_prefix(&descriptor.root)
        .map_err(|_| "Sidebar extension entrypoint escaped its root.".to_string())?;
    let sandbox_executable = Path::new("/extension").join(executable_relative);
    let mut command = Command::new(bwrap);
    command.args([
        "--die-with-parent",
        "--new-session",
        "--unshare-all",
        "--proc",
        "/proc",
        "--dev",
        "/dev",
        "--tmpfs",
        "/tmp",
        "--ro-bind",
        descriptor.root.to_string_lossy().as_ref(),
        "/extension",
        "--chdir",
        "/extension",
        "--setenv",
        "HOME",
        "/tmp",
        "--setenv",
        "PATH",
        "/usr/bin:/bin",
    ]);
    for path in ["/usr", "/bin", "/lib", "/lib64", "/etc"] {
        if Path::new(path).exists() {
            command.args(["--ro-bind", path, path]);
        }
    }
    command.arg("--");
    command.arg(sandbox_executable);
    command.args(&descriptor.manifest.arguments);
    command.env_clear();
    Ok(command)
}

fn sandbox_name() -> &'static str {
    if normalized_env("CMUX_SIDEBAR_EXTENSION_SANDBOX").as_deref() == Some("0") {
        "disabled"
    } else {
        "bubblewrap"
    }
}

fn worker_timeout() -> Duration {
    normalized_env("CMUX_SIDEBAR_EXTENSION_TIMEOUT_MS")
        .and_then(|value| value.parse::<u64>().ok())
        .map(|milliseconds| Duration::from_millis(milliseconds.clamp(50, 10_000)))
        .unwrap_or(DEFAULT_TIMEOUT)
}

fn read_bounded(mut reader: impl Read, limit: usize) -> Result<Vec<u8>, String> {
    let mut output = Vec::new();
    reader
        .by_ref()
        .take(limit as u64 + 1)
        .read_to_end(&mut output)
        .map_err(|error| format!("Failed to read sidebar extension output: {error}"))?;
    if output.len() > limit {
        return Err(format!(
            "Sidebar extension output exceeds the {limit} byte limit."
        ));
    }
    Ok(output)
}

fn load_state() -> State {
    fs::read(state_path())
        .ok()
        .and_then(|bytes| serde_json::from_slice::<State>(&bytes).ok())
        .filter(|state| state.version == STATE_VERSION)
        .unwrap_or_default()
}

fn save_state(state: &State) -> Result<(), String> {
    let path = state_path();
    let parent = path
        .parent()
        .ok_or_else(|| "Sidebar extension state path has no parent.".to_string())?;
    fs::create_dir_all(parent)
        .map_err(|error| format!("Failed to create {}: {error}", parent.display()))?;
    fs::set_permissions(parent, fs::Permissions::from_mode(0o700))
        .map_err(|error| format!("Failed to protect {}: {error}", parent.display()))?;
    let temporary = path.with_extension(format!("tmp-{}", std::process::id()));
    let mut file = OpenOptions::new()
        .create(true)
        .truncate(true)
        .write(true)
        .mode(0o600)
        .open(&temporary)
        .map_err(|error| format!("Failed to write {}: {error}", temporary.display()))?;
    serde_json::to_writer_pretty(&mut file, state)
        .map_err(|error| format!("Failed to encode sidebar extension state: {error}"))?;
    file.write_all(b"\n")
        .map_err(|error| format!("Failed to finish sidebar extension state: {error}"))?;
    file.sync_all()
        .map_err(|error| format!("Failed to sync sidebar extension state: {error}"))?;
    fs::rename(&temporary, &path)
        .map_err(|error| format!("Failed to replace {}: {error}", path.display()))
}

fn normalized_env(key: &str) -> Option<String> {
    let value = std::env::var(key).ok()?;
    let trimmed = value.trim();
    (!trimmed.is_empty()).then(|| trimmed.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn snapshot_filtering_matches_read_scope_boundaries() {
        let snapshot = json!({
            "sequence": 9,
            "window_id": "window-a",
            "selected_workspace_id": "workspace-a",
            "workspaces": [{
                "id": "workspace-a",
                "title": "Private",
                "description": "Detail",
                "pinned": true,
                "root_path": "/secret",
                "project_root_path": "/secret",
                "branch_summary": "main",
                "unread_count": 3,
                "latest_notification_text": "secret",
                "listening_ports": [3000],
                "pull_request_urls": ["https://example.com/pr/1"],
                "surfaces": [{"id": "surface-a"}]
            }]
        });
        let list = filter_snapshot(
            &snapshot,
            &BTreeSet::from(["workspaceList".to_string()]),
            &BTreeSet::new(),
        );
        assert_eq!(list["windowID"], Value::Null);
        assert_eq!(list["workspaces"][0]["title"], "");
        assert_eq!(list["workspaces"][0]["rootPath"], Value::Null);

        let metadata = filter_snapshot(
            &snapshot,
            &BTreeSet::from([
                "workspaceMetadata".to_string(),
                "workspacePaths".to_string(),
                "notifications".to_string(),
                "networkPorts".to_string(),
                "pullRequests".to_string(),
                "surfaceMetadata".to_string(),
            ]),
            &BTreeSet::from(["selectWorkspace".to_string()]),
        );
        assert_eq!(metadata["windowID"], "window-a");
        assert_eq!(metadata["workspaces"][0]["title"], "Private");
        assert_eq!(metadata["workspaces"][0]["rootPath"], "/secret");
        assert_eq!(metadata["workspaces"][0]["latestNotification"], "secret");
        assert_eq!(metadata["workspaces"][0]["listeningPorts"], json!([3000]));
        assert_eq!(metadata["workspaces"][0]["surfaces"][0]["id"], "surface-a");
    }

    #[test]
    fn url_and_path_actions_require_combined_scopes() {
        let browser =
            serde_json::Map::from_iter([("url".to_string(), json!("https://example.com"))]);
        assert_eq!(
            action_scope("createBrowserSurface", &browser),
            Some(BTreeSet::from([
                "createSurface".to_string(),
                "openURL".to_string()
            ]))
        );
        let workspace =
            serde_json::Map::from_iter([("workingDirectory".to_string(), json!("/tmp/project"))]);
        assert_eq!(
            action_scope("createWorkspace", &workspace),
            Some(BTreeSet::from([
                "createWorkspace".to_string(),
                "createWorkspaceWithPath".to_string()
            ]))
        );
    }

    #[test]
    fn manifest_validation_rejects_unknown_scopes_and_newer_api() {
        let mut manifest = Manifest {
            id: "dev.example.sidebar".to_string(),
            display_name: "Example".to_string(),
            minimum_api_version: ApiVersion::default(),
            read_scopes: vec!["workspaceMetadata".to_string()],
            action_scopes: vec![],
            entrypoint: "extension".to_string(),
            arguments: vec![],
        };
        assert!(validate_manifest(&manifest).is_ok());
        manifest.read_scopes = vec!["filesystem".to_string()];
        assert!(validate_manifest(&manifest)
            .unwrap_err()
            .contains("Unknown sidebar extension read scope"));
        manifest.read_scopes.clear();
        manifest.minimum_api_version = ApiVersion { major: 2, minor: 1 };
        assert!(validate_manifest(&manifest)
            .unwrap_err()
            .contains("unsupported"));
    }

    #[test]
    fn extension_documents_reject_raw_cmux_and_reorder_escape_hatches() {
        let mut document = SidebarDocument {
            version: 1,
            root: crate::custom_sidebar::SidebarNode::simple(
                crate::custom_sidebar::SidebarNodeKind::Button,
            ),
        };
        document.root.action = Some(crate::custom_sidebar::SidebarAction {
            kind: "cmux".to_string(),
            message: None,
            params: std::collections::HashMap::from([(
                "action".to_string(),
                "workspace.close".to_string(),
            )]),
            commands: vec![],
        });
        assert!(validate_extension_actions(&document, &BTreeSet::new())
            .unwrap_err()
            .contains("typed `extension` actions"));

        document.root.action = None;
        document.root.reorder = Some(crate::custom_sidebar::SidebarReorder {
            method: "workspace.reorder".to_string(),
            id_parameter: "workspace_id".to_string(),
            item_id: "workspace-a".to_string(),
            index: 0,
        });
        assert!(validate_extension_actions(&document, &BTreeSet::new())
            .unwrap_err()
            .contains("cannot emit raw reorder methods"));
    }

    #[test]
    fn worker_timeout_terminates_a_stuck_extension() {
        let root = tempfile::tempdir().expect("extension tempdir");
        let executable = root.path().join("extension");
        fs::write(&executable, "#!/bin/sh\nsleep 5\n").expect("write extension");
        let mut permissions = fs::metadata(&executable)
            .expect("extension metadata")
            .permissions();
        permissions.set_mode(0o755);
        fs::set_permissions(&executable, permissions).expect("chmod extension");
        let descriptor = Descriptor {
            manifest: Manifest {
                id: "dev.example.timeout".to_string(),
                display_name: "Timeout".to_string(),
                minimum_api_version: ApiVersion::default(),
                read_scopes: vec![],
                action_scopes: vec![],
                entrypoint: "extension".to_string(),
                arguments: vec![],
            },
            root: root.path().to_path_buf(),
            executable,
            fingerprint: "test".to_string(),
        };
        let error = run_worker_with_options(
            &descriptor,
            br#"{"protocolVersion":1}"#,
            Duration::from_millis(50),
            false,
        )
        .unwrap_err();
        assert!(error.contains("timed out after 50 milliseconds"));
    }
}
