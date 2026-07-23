use anyhow::Result;
use serde_json::{json, Value};
use std::fs;
use std::path::{Path, PathBuf};

pub const DISABLED_KEY: &str = "browserDisabledOverride";
pub const DOMAIN: &str = "com.cmuxterm.app";

pub fn settings_path() -> PathBuf {
    if let Some(path) = normalized_env("CMUX_BROWSER_SETTINGS_PATH") {
        return PathBuf::from(path);
    }
    state_dir().join("browser-availability.json")
}

pub fn load_enabled(path: &Path) -> bool {
    read_enabled(path).unwrap_or(true)
}

pub fn read_enabled(path: &Path) -> Option<bool> {
    let text = fs::read_to_string(path).ok()?;
    let value = serde_json::from_str::<Value>(&text).ok()?;
    if let Some(disabled) = value.get(DISABLED_KEY).and_then(Value::as_bool) {
        return Some(!disabled);
    }
    if let Some(disabled) = value.get("disabled").and_then(Value::as_bool) {
        return Some(!disabled);
    }
    value.get("enabled").and_then(Value::as_bool)
}

pub fn save_enabled(path: &Path, enabled: bool) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let contents = serde_json::to_string_pretty(&json!({
        DISABLED_KEY: !enabled,
        "disabled": !enabled,
        "enabled": enabled
    }))?;
    fs::write(path, format!("{contents}\n"))?;
    Ok(())
}

pub fn payload(path: &Path, enabled: bool) -> Value {
    json!({
        "enabled": enabled,
        "disabled": !enabled,
        "domain": DOMAIN,
        "key": DISABLED_KEY,
        "path": path.display().to_string()
    })
}

fn state_dir() -> PathBuf {
    if let Some(path) = normalized_env("XDG_STATE_HOME") {
        return PathBuf::from(path).join("cmux");
    }
    if let Some(home) = normalized_env("HOME") {
        return PathBuf::from(home).join(".local/state/cmux");
    }
    std::env::temp_dir().join("cmux")
}

fn normalized_env(key: &str) -> Option<String> {
    let value = std::env::var(key).ok()?;
    let trimmed = value.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
    }
}
