use crate::config;
use base64::engine::general_purpose::STANDARD;
use base64::Engine;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::fs;
use std::io::Read;
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;

const APPROVAL_VERSION: u32 = 1;
const SECRET_FILE_NAME: &str = ".surface-resume-approval-secret";

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub(crate) enum ResumeApprovalPolicy {
    Manual,
    Prompt,
    Auto,
}

impl ResumeApprovalPolicy {
    pub(crate) fn parse(value: &str) -> Option<Self> {
        match value.trim() {
            "manual" => Some(Self::Manual),
            "prompt" => Some(Self::Prompt),
            "auto" => Some(Self::Auto),
            _ => None,
        }
    }

    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Manual => "manual",
            Self::Prompt => "prompt",
            Self::Auto => "auto",
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ResumeApprovalRecord {
    version: u32,
    pub(crate) id: String,
    pub(crate) name: Option<String>,
    pub(crate) command_prefix: Vec<String>,
    pub(crate) cwd: Option<String>,
    pub(crate) environment: Option<HashMap<String, String>>,
    pub(crate) environment_keys: Vec<String>,
    pub(crate) source: Option<String>,
    pub(crate) policy: ResumeApprovalPolicy,
    pub(crate) created_at: f64,
    pub(crate) updated_at: f64,
    pub(crate) last_used_at: Option<f64>,
    signature: Option<String>,
}

#[derive(Clone, Debug)]
pub(crate) struct ResumeBindingDescriptor {
    pub(crate) name: Option<String>,
    pub(crate) command: String,
    pub(crate) cwd: Option<String>,
    pub(crate) environment: HashMap<String, String>,
    pub(crate) source: Option<String>,
}

#[derive(Clone, Debug)]
pub(crate) struct EffectiveApproval {
    pub(crate) policy: ResumeApprovalPolicy,
    pub(crate) record_id: String,
}

pub(crate) fn list_status() -> Value {
    let secret = load_secret().ok();
    let records = load_records();
    let rows = records
        .iter()
        .map(|record| {
            let valid = secret
                .as_deref()
                .is_some_and(|secret| record.has_valid_signature(secret));
            json!({
                "version": record.version,
                "id": record.id,
                "name": record.name,
                "commandPrefix": record.command_prefix,
                "commandPrefixText": shell_join(&record.command_prefix),
                "cwd": record.cwd,
                "environment": record.environment,
                "environmentKeys": record.environment_keys,
                "source": record.source,
                "policy": record.policy.as_str(),
                "createdAt": record.created_at,
                "updatedAt": record.updated_at,
                "lastUsedAt": record.last_used_at,
                "signature": record.signature,
                "validSignature": valid
            })
        })
        .collect::<Vec<_>>();
    json!({
        "records": rows,
        "configPath": config::primary_cmux_json_path_live().display().to_string(),
        "secretPath": secret_path().display().to_string()
    })
}

pub(crate) fn effective_approval(binding: &ResumeBindingDescriptor) -> Option<EffectiveApproval> {
    let secret = load_secret().ok()?;
    load_records()
        .into_iter()
        .find(|record| record.has_valid_signature(&secret) && record.matches(binding))
        .map(|record| EffectiveApproval {
            policy: record.policy,
            record_id: record.id,
        })
}

pub(crate) fn approve(
    binding: &ResumeBindingDescriptor,
    policy: ResumeApprovalPolicy,
    command_prefix: Option<Vec<String>>,
) -> Result<ResumeApprovalRecord, String> {
    let tokens = shell_tokens(&binding.command)?;
    let prefix = command_prefix.unwrap_or_else(|| tokens.clone());
    if prefix.is_empty() || tokens.len() < prefix.len() || tokens[..prefix.len()] != prefix[..] {
        return Err("command prefix must match the saved resume command".to_string());
    }
    let secret = load_or_create_secret()?;
    let mut records = load_records();
    let now = unix_timestamp_seconds();
    let existing = records
        .iter()
        .find(|record| record.has_valid_signature(&secret) && record.matches(binding))
        .cloned();
    let environment = (!binding.environment.is_empty()).then(|| binding.environment.clone());
    let mut environment_keys = binding.environment.keys().cloned().collect::<Vec<_>>();
    environment_keys.sort();
    let mut record = ResumeApprovalRecord {
        version: APPROVAL_VERSION,
        id: existing
            .as_ref()
            .map(|record| record.id.clone())
            .unwrap_or_else(|| uuid::Uuid::new_v4().to_string()),
        name: normalized(binding.name.clone()),
        command_prefix: prefix,
        cwd: normalized(binding.cwd.clone()).map(normalized_path),
        environment,
        environment_keys,
        source: normalized(binding.source.clone()),
        policy,
        created_at: existing
            .as_ref()
            .map(|record| record.created_at)
            .unwrap_or(now),
        updated_at: now,
        last_used_at: existing.and_then(|record| record.last_used_at),
        signature: None,
    };
    record.sign(&secret);
    records.retain(|candidate| candidate.id != record.id);
    records.push(record.clone());
    write_records(&records)?;
    Ok(record)
}

pub(crate) fn ensure_manual_record(
    binding: &ResumeBindingDescriptor,
) -> Result<ResumeApprovalRecord, String> {
    if let Some(existing) = effective_approval(binding) {
        let records = load_records();
        if let Some(record) = records
            .into_iter()
            .find(|record| record.id == existing.record_id)
        {
            return Ok(record);
        }
    }
    approve(binding, ResumeApprovalPolicy::Manual, None)
}

pub(crate) fn update(
    record_id: &str,
    policy: Option<ResumeApprovalPolicy>,
    command_prefix: Option<Vec<String>>,
) -> Result<ResumeApprovalRecord, String> {
    let secret = load_secret()?;
    let mut records = load_records();
    let record = records
        .iter_mut()
        .find(|record| record.id == record_id)
        .ok_or_else(|| "resume approval record not found".to_string())?;
    if !record.has_valid_signature(&secret) {
        return Err("resume approval record signature is invalid".to_string());
    }
    if let Some(policy) = policy {
        record.policy = policy;
    }
    if let Some(prefix) = command_prefix {
        if prefix.is_empty() {
            return Err("command prefix must not be empty".to_string());
        }
        record.command_prefix = prefix;
    }
    record.updated_at = unix_timestamp_seconds();
    record.sign(&secret);
    let updated = record.clone();
    write_records(&records)?;
    Ok(updated)
}

pub(crate) fn delete(record_id: &str) -> Result<bool, String> {
    let mut records = load_records();
    let previous_len = records.len();
    records.retain(|record| record.id != record_id);
    if records.len() == previous_len {
        return Ok(false);
    }
    write_records(&records)?;
    Ok(true)
}

pub(crate) fn clear() -> Result<usize, String> {
    let count = load_records().len();
    write_records(&[])?;
    Ok(count)
}

pub(crate) fn shell_tokens(command: &str) -> Result<Vec<String>, String> {
    let mut words = Vec::new();
    let mut current = String::new();
    let mut chars = command.chars().peekable();
    let mut quote: Option<char> = None;
    let mut in_word = false;
    while let Some(ch) = chars.next() {
        match (quote, ch) {
            (None, ch) if ch.is_whitespace() => {
                if in_word {
                    words.push(std::mem::take(&mut current));
                    in_word = false;
                }
            }
            (None, '\'' | '"') => {
                quote = Some(ch);
                in_word = true;
            }
            (Some(active), ch) if ch == active => {
                quote = None;
                in_word = true;
            }
            (None, '\\') | (Some('"'), '\\') => {
                let Some(next) = chars.next() else {
                    return Err("resume command ends with an incomplete escape".to_string());
                };
                current.push(next);
                in_word = true;
            }
            _ => {
                current.push(ch);
                in_word = true;
            }
        }
    }
    if quote.is_some() {
        return Err("resume command contains an unterminated quote".to_string());
    }
    if in_word {
        words.push(current);
    }
    if words.is_empty() {
        return Err("resume command is empty".to_string());
    }
    Ok(words)
}

fn load_records() -> Vec<ResumeApprovalRecord> {
    config::terminal_resume_commands()
        .unwrap_or_default()
        .into_iter()
        .filter_map(|value| serde_json::from_value(value).ok())
        .filter(|record: &ResumeApprovalRecord| {
            record.version == APPROVAL_VERSION && !record.command_prefix.is_empty()
        })
        .collect()
}

fn write_records(records: &[ResumeApprovalRecord]) -> Result<(), String> {
    let values = records
        .iter()
        .map(serde_json::to_value)
        .collect::<Result<Vec<_>, _>>()
        .map_err(|err| format!("failed to encode resume approvals: {err}"))?;
    config::set_terminal_resume_commands(&values).map(|_| ())
}

impl ResumeApprovalRecord {
    fn matches(&self, binding: &ResumeBindingDescriptor) -> bool {
        let Ok(tokens) = shell_tokens(&binding.command) else {
            return false;
        };
        if tokens.len() < self.command_prefix.len()
            || tokens[..self.command_prefix.len()] != self.command_prefix[..]
        {
            return false;
        }
        if let Some(cwd) = self.cwd.as_deref() {
            if normalized(binding.cwd.clone())
                .map(normalized_path)
                .as_deref()
                != Some(cwd)
            {
                return false;
            }
        }
        match self.environment.as_ref() {
            Some(environment) if !environment.is_empty() => environment == &binding.environment,
            _ => binding.environment.is_empty(),
        }
    }

    fn sign(&mut self, secret: &[u8]) {
        self.signature = None;
        self.signature = Some(STANDARD.encode(hmac_sha256(secret, &self.signing_payload())));
    }

    fn has_valid_signature(&self, secret: &[u8]) -> bool {
        let Some(signature) = self.signature.as_deref() else {
            return false;
        };
        let expected = STANDARD.encode(hmac_sha256(secret, &self.signing_payload()));
        constant_time_eq(signature.as_bytes(), expected.as_bytes())
    }

    fn signing_payload(&self) -> Vec<u8> {
        let encoded_prefix = self
            .command_prefix
            .iter()
            .map(|value| STANDARD.encode(value.as_bytes()))
            .collect::<Vec<_>>()
            .join(",");
        let encoded_environment_keys = self
            .environment_keys
            .iter()
            .map(|value| STANDARD.encode(value.as_bytes()))
            .collect::<Vec<_>>()
            .join(",");
        let mut environment = self
            .environment
            .clone()
            .unwrap_or_default()
            .into_iter()
            .collect::<Vec<_>>();
        environment.sort_by(|left, right| left.0.cmp(&right.0));
        let encoded_environment = environment
            .iter()
            .map(|(key, value)| {
                format!(
                    "{}={}",
                    STANDARD.encode(key.as_bytes()),
                    STANDARD.encode(value.as_bytes())
                )
            })
            .collect::<Vec<_>>()
            .join(",");
        [
            format!("version={}", self.version),
            format!("id={}", self.id),
            format!(
                "name={}",
                self.name
                    .as_deref()
                    .map(|value| STANDARD.encode(value.as_bytes()))
                    .unwrap_or_default()
            ),
            format!("commandPrefix={encoded_prefix}"),
            format!(
                "cwd={}",
                self.cwd
                    .as_deref()
                    .map(|value| STANDARD.encode(value.as_bytes()))
                    .unwrap_or_default()
            ),
            format!("environment={encoded_environment}"),
            format!("environmentKeys={encoded_environment_keys}"),
            format!(
                "source={}",
                self.source
                    .as_deref()
                    .map(|value| STANDARD.encode(value.as_bytes()))
                    .unwrap_or_default()
            ),
            format!("policy={}", self.policy.as_str()),
            format!(
                "createdAtMs={}",
                canonical_timestamp_millis(self.created_at)
            ),
            format!(
                "updatedAtMs={}",
                canonical_timestamp_millis(self.updated_at)
            ),
            format!(
                "lastUsedAtMs={}",
                self.last_used_at
                    .map(canonical_timestamp_millis)
                    .map(|value| value.to_string())
                    .unwrap_or_default()
            ),
        ]
        .join("\n")
        .into_bytes()
    }
}

fn load_or_create_secret() -> Result<Vec<u8>, String> {
    let path = secret_path();
    if let Ok(secret) = load_secret() {
        return Ok(secret);
    }
    let mut secret = vec![0u8; 32];
    fs::File::open("/dev/urandom")
        .and_then(|mut file| file.read_exact(&mut secret))
        .map_err(|err| format!("failed to generate resume approval secret: {err}"))?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .map_err(|err| format!("failed to create {}: {err}", parent.display()))?;
        let _ = fs::set_permissions(parent, fs::Permissions::from_mode(0o700));
    }
    fs::write(&path, &secret)
        .map_err(|err| format!("failed to write {}: {err}", path.display()))?;
    let _ = fs::set_permissions(&path, fs::Permissions::from_mode(0o600));
    Ok(secret)
}

fn load_secret() -> Result<Vec<u8>, String> {
    let path = secret_path();
    let secret =
        fs::read(&path).map_err(|err| format!("failed to read {}: {err}", path.display()))?;
    if secret.is_empty() {
        return Err(format!(
            "resume approval secret is empty: {}",
            path.display()
        ));
    }
    Ok(secret)
}

fn secret_path() -> PathBuf {
    std::env::var_os("CMUX_SURFACE_RESUME_APPROVAL_SECRET_PATH")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            config::primary_cmux_json_path_live()
                .parent()
                .map(|parent| parent.join(SECRET_FILE_NAME))
                .unwrap_or_else(|| PathBuf::from(SECRET_FILE_NAME))
        })
}

fn hmac_sha256(secret: &[u8], payload: &[u8]) -> [u8; 32] {
    const BLOCK_SIZE: usize = 64;
    let mut key = [0u8; BLOCK_SIZE];
    if secret.len() > BLOCK_SIZE {
        key[..32].copy_from_slice(&Sha256::digest(secret));
    } else {
        key[..secret.len()].copy_from_slice(secret);
    }
    let mut inner_pad = [0x36u8; BLOCK_SIZE];
    let mut outer_pad = [0x5cu8; BLOCK_SIZE];
    for index in 0..BLOCK_SIZE {
        inner_pad[index] ^= key[index];
        outer_pad[index] ^= key[index];
    }
    let mut inner = Sha256::new();
    inner.update(inner_pad);
    inner.update(payload);
    let inner_hash = inner.finalize();
    let mut outer = Sha256::new();
    outer.update(outer_pad);
    outer.update(inner_hash);
    outer.finalize().into()
}

fn constant_time_eq(left: &[u8], right: &[u8]) -> bool {
    if left.len() != right.len() {
        return false;
    }
    left.iter()
        .zip(right)
        .fold(0u8, |difference, (left, right)| difference | (left ^ right))
        == 0
}

fn normalized(value: Option<String>) -> Option<String> {
    value
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

fn normalized_path(value: String) -> String {
    fs::canonicalize(&value)
        .unwrap_or_else(|_| PathBuf::from(&value))
        .to_string_lossy()
        .to_string()
}

fn shell_join(words: &[String]) -> String {
    words
        .iter()
        .map(|word| shell_quote(word))
        .collect::<Vec<_>>()
        .join(" ")
}

fn shell_quote(value: &str) -> String {
    if !value.is_empty()
        && value
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || "_+-=./:@%".contains(ch))
    {
        return value.to_string();
    }
    format!("'{}'", value.replace('\'', "'\\''"))
}

fn unix_timestamp_seconds() -> f64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_secs_f64())
        .unwrap_or_default()
}

fn canonical_timestamp_millis(value: f64) -> i64 {
    if !value.is_finite() {
        return 0;
    }
    (value * 1_000.0).round() as i64
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shell_tokens_match_quoted_prefixes_and_reject_incomplete_input() {
        assert_eq!(
            shell_tokens(r#"codex resume "session one" --flag"#).unwrap(),
            vec!["codex", "resume", "session one", "--flag"]
        );
        assert!(shell_tokens("'unterminated").is_err());
        assert!(shell_tokens("trailing\\").is_err());
    }

    #[test]
    fn hmac_changes_when_the_payload_changes() {
        let first = hmac_sha256(b"secret", b"payload");
        let second = hmac_sha256(b"secret", b"changed");
        assert_ne!(first, second);
        assert!(constant_time_eq(&first, &first));
        assert!(!constant_time_eq(&first, &second));
    }

    #[test]
    fn signed_timestamps_survive_json_float_round_trips() {
        let original = 1_784_198_299.522_015_3;
        let encoded = serde_json::to_string(&original).unwrap();
        let decoded: f64 = serde_json::from_str(&encoded).unwrap();
        assert_eq!(
            canonical_timestamp_millis(original),
            canonical_timestamp_millis(decoded)
        );
    }
}
