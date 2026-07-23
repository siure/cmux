use anyhow::{anyhow, bail, Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};
use uuid::Uuid;

const MAX_UNTRACKED_SNAPSHOT_FILES: usize = 64;
const MAX_UNTRACKED_SNAPSHOT_FILE_BYTES: u64 = 1024 * 1024;
const MAX_UNTRACKED_SNAPSHOT_TOTAL_BYTES: u64 = 4 * 1024 * 1024;

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
#[allow(dead_code)]
struct AgentTurnDiffBaselineRecord {
    workspace_id: String,
    surface_id: String,
    session_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    turn_id: Option<String>,
    agent: String,
    repo_root: String,
    base_commit: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    untracked_paths: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    untracked_path_hashes: Option<HashMap<String, String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    untracked_snapshot_id: Option<String>,
    captured_at: f64,
}

#[derive(Debug, Deserialize, Serialize)]
struct AgentTurnDiffBaselineStore {
    #[serde(default = "default_store_version")]
    version: u64,
    #[serde(default)]
    records: Vec<AgentTurnDiffBaselineRecord>,
}

#[derive(Debug)]
pub struct LastTurnDiff {
    pub patch: String,
    pub source_label: String,
    pub repo_root: PathBuf,
}

#[derive(Debug)]
pub struct RecordedTurnBaseline {
    pub store_path: PathBuf,
    pub repo_root: PathBuf,
    pub base_commit: String,
    pub replaced: bool,
}

pub fn record_turn_baseline(
    agent: &str,
    session_id: &str,
    turn_id: Option<&str>,
    starting_dir: &Path,
    workspace_id: Option<&str>,
    surface_id: Option<&str>,
    preserve_existing: bool,
) -> Result<Option<RecordedTurnBaseline>> {
    let Some(workspace_id) = normalized_context_value(workspace_id) else {
        return Ok(None);
    };
    let Some(surface_id) = normalized_context_value(surface_id) else {
        return Ok(None);
    };
    let Some(session_id) = normalized_context_value(Some(session_id)) else {
        return Ok(None);
    };
    let agent = normalized_context_value(Some(agent)).unwrap_or_else(|| "agent".to_string());
    let turn_id = turn_id.and_then(|value| normalized_context_value(Some(value)));
    let repo_root = git_repo_root(starting_dir)?;
    let base_commit = agent_turn_diff_baseline_commit(&repo_root)?;
    let untracked_paths = git_untracked_paths(&repo_root)?;
    let store_path = agent_turn_diff_baseline_store_path();
    let untracked_snapshot = git_untracked_path_hashes(&untracked_paths, &repo_root, &store_path)?;
    let record = AgentTurnDiffBaselineRecord {
        workspace_id: workspace_id.clone(),
        surface_id: surface_id.clone(),
        session_id: session_id.clone(),
        turn_id: turn_id.clone(),
        agent,
        repo_root: repo_root.display().to_string(),
        base_commit: base_commit.clone(),
        untracked_paths: (!untracked_paths.is_empty()).then_some(untracked_paths),
        untracked_path_hashes: (!untracked_snapshot.hashes.is_empty())
            .then_some(untracked_snapshot.hashes),
        untracked_snapshot_id: untracked_snapshot.snapshot_id.clone(),
        captured_at: current_unix_seconds(),
    };

    let mut store = read_agent_turn_diff_baseline_store(&store_path)?;
    let matching_scope = |existing: &AgentTurnDiffBaselineRecord| {
        standardized_path(Path::new(&existing.repo_root)) == standardized_path(&repo_root)
            && scope_identifier_equals(&existing.workspace_id, &workspace_id)
            && scope_identifier_equals(&existing.surface_id, &surface_id)
            && existing.session_id == session_id
    };
    let matching_turn = |existing: &AgentTurnDiffBaselineRecord| {
        if let Some(turn_id) = &turn_id {
            existing.turn_id.as_deref() == Some(turn_id.as_str())
        } else {
            existing.turn_id.is_none()
        }
    };

    if preserve_existing
        && store
            .records
            .iter()
            .any(|existing| matching_scope(existing) && matching_turn(existing))
    {
        if let Some(snapshot_id) = untracked_snapshot.snapshot_id {
            remove_snapshot(&store_path, &snapshot_id);
        }
        return Ok(None);
    }

    let before_len = store.records.len();
    let mut removed_snapshots = Vec::new();
    store.records.retain(|existing| {
        let remove = matching_scope(existing) && matching_turn(existing);
        if remove {
            if let Some(snapshot_id) = &existing.untracked_snapshot_id {
                removed_snapshots.push(snapshot_id.clone());
            }
        }
        !remove
    });
    store.records.push(record);
    write_agent_turn_diff_baseline_store(&store_path, &store)?;
    for snapshot_id in removed_snapshots {
        remove_snapshot(&store_path, &snapshot_id);
    }

    Ok(Some(RecordedTurnBaseline {
        store_path,
        repo_root,
        base_commit,
        replaced: store.records.len() <= before_len,
    }))
}

pub fn last_turn_diff(
    starting_dir: &Path,
    workspace_id: Option<&str>,
    surface_id: Option<&str>,
) -> Result<LastTurnDiff> {
    let workspace_id = normalized_context_value(workspace_id).ok_or_else(|| {
        anyhow!(
            "cmux diff --last-turn requires a workspace and surface context. Run it from a cmux terminal or pass --workspace and --surface."
        )
    })?;
    let surface_id = normalized_context_value(surface_id).ok_or_else(|| {
        anyhow!(
            "cmux diff --last-turn requires a workspace and surface context. Run it from a cmux terminal or pass --workspace and --surface."
        )
    })?;
    let repo_root = git_repo_root(starting_dir)?;
    let store_path = agent_turn_diff_baseline_store_path();
    let record =
        latest_agent_turn_diff_baseline(&store_path, &repo_root, &workspace_id, &surface_id)?;
    let source_label = format!("git last-turn {workspace_id} {surface_id}");

    let Some(record) = record else {
        return Ok(LastTurnDiff {
            patch: empty_diff_text("last-turn"),
            source_label,
            repo_root,
        });
    };

    git_stdout(
        &repo_root,
        &[
            "cat-file",
            "-e",
            &format!("{}^{{tree}}", record.base_commit),
        ],
        &[0],
    )
    .with_context(|| {
        format!(
            "last-turn baseline commit is unavailable: {}",
            record.base_commit
        )
    })?;

    let tracked = git_stdout(
        &repo_root,
        &git_diff_patch_args(&[record.base_commit.as_str(), "--"]),
        &[0],
    )?;
    let untracked = git_untracked_patch_since_baseline(&record, &repo_root, &store_path)?;
    let patch = joined_git_diff_patches([tracked, untracked]);

    Ok(LastTurnDiff {
        patch: if patch.trim().is_empty() {
            empty_diff_text("last-turn")
        } else {
            patch
        },
        source_label,
        repo_root,
    })
}

fn default_store_version() -> u64 {
    1
}

fn agent_turn_diff_baseline_store_path() -> PathBuf {
    if let Some(dir) = normalized_env("CMUX_AGENT_HOOK_STATE_DIR") {
        return home_expanded_path(&dir).join("agent-turn-diff-baselines.json");
    }
    if let Some(home) = normalized_env("HOME") {
        return PathBuf::from(home).join(".cmuxterm/agent-turn-diff-baselines.json");
    }
    PathBuf::from(".cmuxterm/agent-turn-diff-baselines.json")
}

fn latest_agent_turn_diff_baseline(
    store_path: &Path,
    repo_root: &Path,
    workspace_id: &str,
    surface_id: &str,
) -> Result<Option<AgentTurnDiffBaselineRecord>> {
    let text = match fs::read_to_string(store_path) {
        Ok(text) => text,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(err) => {
            return Err(err).with_context(|| format!("failed to read {}", store_path.display()));
        }
    };
    if text.trim().is_empty() {
        return Ok(None);
    }
    let store: AgentTurnDiffBaselineStore = serde_json::from_str(&text)
        .with_context(|| format!("failed to decode {}", store_path.display()))?;
    let repo_root = standardized_path(repo_root);
    Ok(store
        .records
        .into_iter()
        .filter(|record| {
            standardized_path(Path::new(&record.repo_root)) == repo_root
                && scope_identifier_equals(&record.workspace_id, workspace_id)
                && scope_identifier_equals(&record.surface_id, surface_id)
        })
        .max_by(|lhs, rhs| {
            lhs.captured_at
                .partial_cmp(&rhs.captured_at)
                .unwrap_or(std::cmp::Ordering::Equal)
        }))
}

fn read_agent_turn_diff_baseline_store(store_path: &Path) -> Result<AgentTurnDiffBaselineStore> {
    let text = match fs::read_to_string(store_path) {
        Ok(text) => text,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
            return Ok(AgentTurnDiffBaselineStore {
                version: 1,
                records: Vec::new(),
            });
        }
        Err(err) => {
            return Err(err).with_context(|| format!("failed to read {}", store_path.display()));
        }
    };
    if text.trim().is_empty() {
        return Ok(AgentTurnDiffBaselineStore {
            version: 1,
            records: Vec::new(),
        });
    }
    serde_json::from_str(&text)
        .with_context(|| format!("failed to decode {}", store_path.display()))
}

fn write_agent_turn_diff_baseline_store(
    store_path: &Path,
    store: &AgentTurnDiffBaselineStore,
) -> Result<()> {
    if let Some(parent) = store_path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("failed to create {}", parent.display()))?;
    }
    let text = serde_json::to_string_pretty(store)? + "\n";
    fs::write(store_path, text).with_context(|| format!("failed to write {}", store_path.display()))
}

fn agent_turn_diff_baseline_commit(repo_root: &Path) -> Result<String> {
    let stash = git_stdout(
        repo_root,
        &["stash", "create", "cmux last turn baseline"],
        &[0],
    )?;
    let stash = stash.trim();
    if !stash.is_empty() {
        let ref_name = format!("refs/cmux/last-turn/{stash}");
        git_stdout(repo_root, &["update-ref", &ref_name, stash], &[0])?;
        return Ok(stash.to_string());
    }
    if let Ok(head) = git_stdout(repo_root, &["rev-parse", "HEAD"], &[0]) {
        let head = head.trim();
        if !head.is_empty() {
            return Ok(head.to_string());
        }
    }
    Ok(
        git_stdout(repo_root, &["hash-object", "-t", "tree", "/dev/null"], &[0])?
            .trim()
            .to_string(),
    )
}

struct UntrackedSnapshot {
    snapshot_id: Option<String>,
    hashes: HashMap<String, String>,
}

fn git_untracked_path_hashes(
    paths: &[String],
    repo_root: &Path,
    store_path: &Path,
) -> Result<UntrackedSnapshot> {
    if paths.is_empty() {
        return Ok(UntrackedSnapshot {
            snapshot_id: None,
            hashes: HashMap::new(),
        });
    }
    let snapshot_id = Uuid::new_v4().to_string();
    let staging_dir = snapshot_staging_directory(store_path, &snapshot_id);
    let files_root = staging_dir.join("files");
    fs::create_dir_all(&files_root)
        .with_context(|| format!("failed to create {}", files_root.display()))?;
    let mut hashes = HashMap::new();
    let mut captured_bytes = 0_u64;

    for path in paths {
        if hashes.len() >= MAX_UNTRACKED_SNAPSHOT_FILES {
            break;
        }
        let Some(components) = safe_relative_path_components(path) else {
            continue;
        };
        let source = repo_root.join(path);
        let Ok(metadata) = fs::metadata(&source) else {
            continue;
        };
        if !metadata.is_file() {
            continue;
        }
        let file_size = metadata.len();
        if file_size > MAX_UNTRACKED_SNAPSHOT_FILE_BYTES
            || captured_bytes.saturating_add(file_size) > MAX_UNTRACKED_SNAPSHOT_TOTAL_BYTES
        {
            continue;
        }
        let mut destination = files_root.clone();
        for component in components {
            destination.push(component);
        }
        copy_file_into(&source, &destination)?;
        let hash = git_stdout(
            repo_root,
            &[
                "hash-object",
                "--no-filters",
                "--",
                destination.to_str().unwrap_or_default(),
            ],
            &[0],
        )?
        .trim()
        .to_string();
        if !hash.is_empty() {
            hashes.insert(path.clone(), hash);
            captured_bytes += file_size;
        }
    }

    if hashes.is_empty() {
        let _ = fs::remove_dir_all(&staging_dir);
        return Ok(UntrackedSnapshot {
            snapshot_id: None,
            hashes,
        });
    }

    let final_dir = snapshot_directory(store_path, &snapshot_id);
    if let Some(parent) = final_dir.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("failed to create {}", parent.display()))?;
    }
    if final_dir.exists() {
        let _ = fs::remove_dir_all(&final_dir);
    }
    fs::rename(&staging_dir, &final_dir).with_context(|| {
        format!(
            "failed to publish untracked snapshot {} to {}",
            staging_dir.display(),
            final_dir.display()
        )
    })?;

    Ok(UntrackedSnapshot {
        snapshot_id: Some(snapshot_id),
        hashes,
    })
}

fn git_untracked_patch_since_baseline(
    record: &AgentTurnDiffBaselineRecord,
    repo_root: &Path,
    store_path: &Path,
) -> Result<String> {
    let baseline_paths = record
        .untracked_paths
        .clone()
        .unwrap_or_default()
        .into_iter()
        .collect::<HashSet<_>>();
    let baseline_hashes = record.untracked_path_hashes.clone().unwrap_or_default();
    let current_paths = git_untracked_paths(repo_root)?;
    let current_path_set = current_paths.iter().cloned().collect::<HashSet<_>>();
    let mut patches = Vec::new();

    for path in current_paths {
        if !baseline_paths.contains(&path) {
            patches.push(git_added_untracked_patch(&path, repo_root)?);
            continue;
        }
        let Some(baseline_hash) = baseline_hashes.get(&path) else {
            continue;
        };
        if git_untracked_path_hash(&path, repo_root)? == *baseline_hash {
            continue;
        }
        if let Some(snapshot) = agent_turn_diff_baseline_snapshot_file(&path, record, store_path) {
            if snapshot.is_file() {
                if let Some(patch) = git_changed_untracked_patch(&path, &snapshot, repo_root)? {
                    patches.push(patch);
                    continue;
                }
            }
        }
        if let Some(patch) =
            git_changed_untracked_patch_from_git_object(&path, baseline_hash, repo_root)?
        {
            patches.push(patch);
        }
    }

    for path in baseline_paths
        .difference(&current_path_set)
        .cloned()
        .collect::<Vec<_>>()
    {
        if repo_path_exists(&path, repo_root) {
            continue;
        }
        let Some(baseline_hash) = baseline_hashes.get(&path) else {
            continue;
        };
        if let Some(snapshot) = agent_turn_diff_baseline_snapshot_file(&path, record, store_path) {
            if snapshot.is_file() {
                if let Some(patch) = git_deleted_untracked_patch(&path, &snapshot)? {
                    patches.push(patch);
                    continue;
                }
            }
        }
        if let Some(patch) =
            git_deleted_untracked_patch_from_git_object(&path, baseline_hash, repo_root)?
        {
            patches.push(patch);
        }
    }

    Ok(joined_git_diff_patches(patches))
}

fn git_untracked_paths(repo_root: &Path) -> Result<Vec<String>> {
    let output = git_stdout(
        repo_root,
        &["ls-files", "--others", "--exclude-standard", "-z"],
        &[0],
    )?;
    Ok(output
        .split('\0')
        .filter(|path| !path.is_empty())
        .map(ToString::to_string)
        .collect())
}

fn git_added_untracked_patch(path: &str, repo_root: &Path) -> Result<String> {
    git_stdout(
        repo_root,
        &git_diff_patch_args(&["--no-index", "--", "/dev/null", path]),
        &[0, 1],
    )
}

fn git_changed_untracked_patch(
    path: &str,
    baseline_file: &Path,
    repo_root: &Path,
) -> Result<Option<String>> {
    let Some(temp) = temporary_git_path(path) else {
        return Ok(None);
    };
    let baseline = temp.root.join("baseline").join(path);
    let current = temp.root.join("current").join(path);
    copy_file_into(baseline_file, &baseline)?;
    copy_file_into(&repo_root.join(path), &current)?;
    let patch = git_stdout(
        &temp.root,
        &git_diff_patch_args(&[
            "--no-index",
            "--",
            &format!("baseline/{path}"),
            &format!("current/{path}"),
        ]),
        &[0, 1],
    )?;
    let _ = fs::remove_dir_all(&temp.root);
    Ok(Some(rewrite_changed_untracked_patch(&patch)))
}

fn git_changed_untracked_patch_from_git_object(
    path: &str,
    baseline_hash: &str,
    repo_root: &Path,
) -> Result<Option<String>> {
    if git_stdout(
        repo_root,
        &["cat-file", "-e", &format!("{baseline_hash}^{{blob}}")],
        &[0],
    )
    .is_err()
    {
        return Ok(None);
    }
    let Some(temp) = temporary_git_path(path) else {
        return Ok(None);
    };
    let baseline = temp.root.join("baseline").join(path);
    let current = temp.root.join("current").join(path);
    write_file_into(
        &baseline,
        git_stdout_bytes(repo_root, &["cat-file", "blob", baseline_hash])?,
    )?;
    copy_file_into(&repo_root.join(path), &current)?;
    let patch = git_stdout(
        &temp.root,
        &git_diff_patch_args(&[
            "--no-index",
            "--",
            &format!("baseline/{path}"),
            &format!("current/{path}"),
        ]),
        &[0, 1],
    )?;
    let _ = fs::remove_dir_all(&temp.root);
    Ok(Some(rewrite_changed_untracked_patch(&patch)))
}

fn git_deleted_untracked_patch(path: &str, baseline_file: &Path) -> Result<Option<String>> {
    let Some(temp) = temporary_git_path(path) else {
        return Ok(None);
    };
    copy_file_into(baseline_file, &temp.file)?;
    let patch = git_stdout(
        &temp.root,
        &git_diff_patch_args(&["--no-index", "--", path, "/dev/null"]),
        &[0, 1],
    )?;
    let _ = fs::remove_dir_all(&temp.root);
    Ok(Some(patch))
}

fn git_deleted_untracked_patch_from_git_object(
    path: &str,
    baseline_hash: &str,
    repo_root: &Path,
) -> Result<Option<String>> {
    if git_stdout(
        repo_root,
        &["cat-file", "-e", &format!("{baseline_hash}^{{blob}}")],
        &[0],
    )
    .is_err()
    {
        return Ok(None);
    }
    let Some(temp) = temporary_git_path(path) else {
        return Ok(None);
    };
    write_file_into(
        &temp.file,
        git_stdout_bytes(repo_root, &["cat-file", "blob", baseline_hash])?,
    )?;
    let patch = git_stdout(
        &temp.root,
        &git_diff_patch_args(&["--no-index", "--", path, "/dev/null"]),
        &[0, 1],
    )?;
    let _ = fs::remove_dir_all(&temp.root);
    Ok(Some(patch))
}

fn agent_turn_diff_baseline_snapshot_file(
    path: &str,
    record: &AgentTurnDiffBaselineRecord,
    store_path: &Path,
) -> Option<PathBuf> {
    let snapshot_id = record.untracked_snapshot_id.as_ref()?;
    Uuid::parse_str(snapshot_id).ok()?;
    let components = safe_relative_path_components(path)?;
    let mut file = store_path
        .parent()?
        .join("agent-turn-diff-baseline-snapshots")
        .join(snapshot_id)
        .join("files");
    for component in components {
        file.push(component);
    }
    Some(file)
}

fn snapshot_directory(store_path: &Path, snapshot_id: &str) -> PathBuf {
    store_path
        .parent()
        .unwrap_or_else(|| Path::new("."))
        .join("agent-turn-diff-baseline-snapshots")
        .join(snapshot_id)
}

fn snapshot_staging_directory(store_path: &Path, snapshot_id: &str) -> PathBuf {
    store_path
        .parent()
        .unwrap_or_else(|| Path::new("."))
        .join("agent-turn-diff-baseline-snapshots-staging")
        .join(snapshot_id)
}

fn remove_snapshot(store_path: &Path, snapshot_id: &str) {
    if Uuid::parse_str(snapshot_id).is_err() {
        return;
    }
    let _ = fs::remove_dir_all(snapshot_directory(store_path, snapshot_id));
    let _ = fs::remove_dir_all(snapshot_staging_directory(store_path, snapshot_id));
}

fn git_untracked_path_hash(path: &str, repo_root: &Path) -> Result<String> {
    Ok(git_stdout(
        repo_root,
        &["hash-object", "--no-filters", "--", path],
        &[0],
    )?
    .trim()
    .to_string())
}

fn git_repo_root(starting_dir: &Path) -> Result<PathBuf> {
    let output = git_stdout(starting_dir, &["rev-parse", "--show-toplevel"], &[0])?;
    let raw = output.trim();
    if raw.is_empty() {
        bail!("git rev-parse --show-toplevel returned an empty path");
    }
    Ok(PathBuf::from(raw)
        .canonicalize()
        .unwrap_or_else(|_| PathBuf::from(raw)))
}

fn git_diff_patch_args(tail: &[&str]) -> Vec<String> {
    ["diff", "--no-ext-diff", "--no-color", "--binary"]
        .into_iter()
        .map(ToString::to_string)
        .chain(tail.iter().map(|value| value.to_string()))
        .collect()
}

fn git_stdout(cwd: &Path, args: &[impl AsRef<str>], allowed_statuses: &[i32]) -> Result<String> {
    let output = Command::new("git")
        .args(args.iter().map(AsRef::as_ref))
        .current_dir(cwd)
        .output()
        .with_context(|| format!("failed to run git {}", command_label(args)))?;
    let status = output.status.code().unwrap_or(-1);
    if !allowed_statuses.contains(&status) {
        bail!(
            "git {} failed: {}",
            command_label(args),
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }
    String::from_utf8(output.stdout).context("git output was not UTF-8")
}

fn git_stdout_bytes(cwd: &Path, args: &[&str]) -> Result<Vec<u8>> {
    let output = Command::new("git")
        .args(args)
        .current_dir(cwd)
        .output()
        .with_context(|| format!("failed to run git {}", args.join(" ")))?;
    if !output.status.success() {
        bail!(
            "git {} failed: {}",
            args.join(" "),
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }
    Ok(output.stdout)
}

fn command_label(args: &[impl AsRef<str>]) -> String {
    args.iter().map(AsRef::as_ref).collect::<Vec<_>>().join(" ")
}

pub fn empty_diff_text(label: &str) -> String {
    format!("diff --git a/.cmux-empty-diff b/.cmux-empty-diff\n--- a/.cmux-empty-diff\n+++ b/.cmux-empty-diff\n@@ -1 +1 @@\n-No {label} changes\n+No {label} changes\n")
}

fn joined_git_diff_patches(patches: impl IntoIterator<Item = String>) -> String {
    let trimmed = patches
        .into_iter()
        .map(|patch| patch.trim_matches('\n').to_string())
        .filter(|patch| !patch.is_empty())
        .collect::<Vec<_>>();
    if trimmed.is_empty() {
        String::new()
    } else {
        trimmed.join("\n") + "\n"
    }
}

fn rewrite_changed_untracked_patch(patch: &str) -> String {
    patch
        .lines()
        .map(|line| {
            if line.starts_with("diff --git ") {
                line.replace("a/baseline/", "a/")
                    .replace("b/current/", "b/")
            } else if line.starts_with("--- ") {
                line.replace("a/baseline/", "a/")
            } else if line.starts_with("+++ ") {
                line.replace("b/current/", "b/")
            } else {
                line.to_string()
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
        + if patch.ends_with('\n') { "\n" } else { "" }
}

struct TemporaryGitPath {
    root: PathBuf,
    file: PathBuf,
}

fn temporary_git_path(relative_path: &str) -> Option<TemporaryGitPath> {
    let components = safe_relative_path_components(relative_path)?;
    let root = std::env::temp_dir().join(format!("cmux-diff-untracked-{}", Uuid::new_v4()));
    let mut file = root.clone();
    for component in components {
        file.push(component);
    }
    Some(TemporaryGitPath { root, file })
}

fn safe_relative_path_components(relative_path: &str) -> Option<Vec<String>> {
    if relative_path.starts_with('/') {
        return None;
    }
    let components = relative_path
        .split('/')
        .map(ToString::to_string)
        .collect::<Vec<_>>();
    if components.is_empty()
        || components
            .iter()
            .any(|part| part.is_empty() || part == "." || part == "..")
    {
        return None;
    }
    Some(components)
}

fn copy_file_into(source: &Path, destination: &Path) -> Result<()> {
    if let Some(parent) = destination.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::copy(source, destination).with_context(|| {
        format!(
            "failed to copy {} to {}",
            source.display(),
            destination.display()
        )
    })?;
    Ok(())
}

fn write_file_into(destination: &Path, bytes: Vec<u8>) -> Result<()> {
    if let Some(parent) = destination.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(destination, bytes)
        .with_context(|| format!("failed to write {}", destination.display()))
}

fn repo_path_exists(relative_path: &str, repo_root: &Path) -> bool {
    safe_relative_path_components(relative_path)
        .map(|components| {
            let mut path = repo_root.to_path_buf();
            for component in components {
                path.push(component);
            }
            path.exists()
        })
        .unwrap_or(true)
}

fn scope_identifier_equals(lhs: &str, rhs: &str) -> bool {
    match (Uuid::parse_str(lhs), Uuid::parse_str(rhs)) {
        (Ok(lhs), Ok(rhs)) => lhs == rhs,
        _ => lhs == rhs,
    }
}

fn normalized_context_value(value: Option<&str>) -> Option<String> {
    value
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToString::to_string)
}

fn normalized_env(key: &str) -> Option<String> {
    std::env::var(key)
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

fn home_expanded_path(raw: &str) -> PathBuf {
    let trimmed = raw.trim();
    if trimmed == "~" {
        if let Some(home) = normalized_env("HOME") {
            return PathBuf::from(home);
        }
    }
    if let Some(rest) = trimmed.strip_prefix("~/") {
        if let Some(home) = normalized_env("HOME") {
            return PathBuf::from(home).join(rest);
        }
    }
    PathBuf::from(trimmed)
}

fn standardized_path(path: &Path) -> PathBuf {
    let path = if let Some(raw) = path.to_str() {
        home_expanded_path(raw)
    } else {
        path.to_path_buf()
    };
    path.canonicalize().unwrap_or(path)
}

fn current_unix_seconds() -> f64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs_f64())
        .unwrap_or(0.0)
}
