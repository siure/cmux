use anyhow::Result;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};

pub const ENABLED_KEY: &str = "terminal.agentHibernation.enabled";
pub const DEFAULT_IDLE_SECONDS: u64 = 5;
pub const DEFAULT_MAX_LIVE_TERMINALS: u64 = 12;
pub const DEFAULT_CONFIRMATION_SECONDS: u64 = 60;
pub const MIN_IDLE_SECONDS: u64 = 5;
pub const MAX_IDLE_SECONDS: u64 = 604_800;
pub const MIN_LIVE_TERMINALS: u64 = 1;
pub const MAX_LIVE_TERMINALS: u64 = 256;
pub const MIN_CONFIRMATION_SECONDS: u64 = 1;
pub const MAX_CONFIRMATION_SECONDS: u64 = 600;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Settings {
    pub enabled: bool,
    pub idle_seconds: u64,
    pub max_live_terminals: u64,
    pub confirmation_seconds: u64,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            enabled: false,
            idle_seconds: DEFAULT_IDLE_SECONDS,
            max_live_terminals: DEFAULT_MAX_LIVE_TERMINALS,
            confirmation_seconds: DEFAULT_CONFIRMATION_SECONDS,
        }
    }
}

impl Settings {
    pub fn sanitized(self) -> Self {
        Self {
            enabled: self.enabled,
            idle_seconds: self.idle_seconds.clamp(MIN_IDLE_SECONDS, MAX_IDLE_SECONDS),
            max_live_terminals: self
                .max_live_terminals
                .clamp(MIN_LIVE_TERMINALS, MAX_LIVE_TERMINALS),
            confirmation_seconds: self
                .confirmation_seconds
                .clamp(MIN_CONFIRMATION_SECONDS, MAX_CONFIRMATION_SECONDS),
        }
    }

    pub fn payload(self, path: &Path) -> Value {
        json!({
            "ok": true,
            "enabled": self.enabled,
            "key": ENABLED_KEY,
            "path": path.display().to_string(),
            "idle_seconds": self.idle_seconds,
            "idleSeconds": self.idle_seconds,
            "max_live_terminals": self.max_live_terminals,
            "maxLiveTerminals": self.max_live_terminals,
            "confirmation_seconds": self.confirmation_seconds,
            "confirmationSeconds": self.confirmation_seconds
        })
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PlannerInput {
    pub surface_id: String,
    pub has_restorable_agent: bool,
    pub is_live: bool,
    pub is_protected: bool,
    pub lifecycle_idle: bool,
    pub has_unconfirmed_terminal_input: bool,
    pub last_activity_ms: u64,
}

pub fn selected_surface_ids(
    inputs: &[PlannerInput],
    settings: Settings,
    now_ms: u64,
) -> HashSet<String> {
    if !settings.enabled {
        return HashSet::new();
    }
    let live_restorable = inputs
        .iter()
        .filter(|input| input.has_restorable_agent && input.is_live)
        .collect::<Vec<_>>();
    let excess = live_restorable
        .len()
        .saturating_sub(settings.max_live_terminals as usize);
    if excess == 0 {
        return HashSet::new();
    }

    let idle_ms = settings.idle_seconds.saturating_mul(1_000);
    let mut eligible = live_restorable
        .into_iter()
        .filter(|input| {
            !input.is_protected
                && input.lifecycle_idle
                && !input.has_unconfirmed_terminal_input
                && now_ms.saturating_sub(input.last_activity_ms) >= idle_ms
        })
        .collect::<Vec<_>>();
    eligible.sort_by(|left, right| {
        left.last_activity_ms
            .cmp(&right.last_activity_ms)
            .then_with(|| left.surface_id.cmp(&right.surface_id))
    });
    eligible
        .into_iter()
        .take(excess)
        .map(|input| input.surface_id.clone())
        .collect()
}

pub fn settings_path_override() -> Option<PathBuf> {
    normalized_env("CMUX_AGENT_HIBERNATION_SETTINGS_PATH").map(PathBuf::from)
}

pub fn load(path: &Path) -> Settings {
    let Ok(text) = fs::read_to_string(path) else {
        return Settings::default();
    };
    let Ok(value) = serde_json::from_str::<Value>(&text) else {
        return Settings::default();
    };
    Settings {
        enabled: value
            .get(ENABLED_KEY)
            .or_else(|| value.get("enabled"))
            .and_then(Value::as_bool)
            .unwrap_or(false),
        idle_seconds: u64_value(&value, "idle_seconds", "idleSeconds")
            .unwrap_or(DEFAULT_IDLE_SECONDS),
        max_live_terminals: u64_value(&value, "max_live_terminals", "maxLiveTerminals")
            .unwrap_or(DEFAULT_MAX_LIVE_TERMINALS),
        confirmation_seconds: u64_value(&value, "confirmation_seconds", "confirmationSeconds")
            .unwrap_or(DEFAULT_CONFIRMATION_SECONDS),
    }
    .sanitized()
}

pub fn save(path: &Path, settings: Settings) -> Result<()> {
    let settings = settings.sanitized();
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let contents = serde_json::to_string_pretty(&json!({
        ENABLED_KEY: settings.enabled,
        "enabled": settings.enabled,
        "idle_seconds": settings.idle_seconds,
        "idleSeconds": settings.idle_seconds,
        "max_live_terminals": settings.max_live_terminals,
        "maxLiveTerminals": settings.max_live_terminals,
        "confirmation_seconds": settings.confirmation_seconds,
        "confirmationSeconds": settings.confirmation_seconds
    }))?;
    fs::write(path, format!("{contents}\n"))?;
    Ok(())
}

fn u64_value(value: &Value, snake: &str, camel: &str) -> Option<u64> {
    value
        .get(snake)
        .or_else(|| value.get(camel))
        .and_then(Value::as_u64)
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn settings_round_trip_and_clamp_all_runtime_values() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let path = tmp.path().join("agent-hibernation.json");
        save(
            &path,
            Settings {
                enabled: true,
                idle_seconds: 1,
                max_live_terminals: 999,
                confirmation_seconds: 0,
            },
        )
        .expect("save settings");
        assert_eq!(
            load(&path),
            Settings {
                enabled: true,
                idle_seconds: MIN_IDLE_SECONDS,
                max_live_terminals: MAX_LIVE_TERMINALS,
                confirmation_seconds: MIN_CONFIRMATION_SECONDS,
            }
        );
    }

    #[test]
    fn planner_hibernates_oldest_eligible_excess_only() {
        let settings = Settings {
            enabled: true,
            idle_seconds: 5,
            max_live_terminals: 2,
            confirmation_seconds: 60,
        };
        let input = |surface_id: &str, last_activity_ms: u64| PlannerInput {
            surface_id: surface_id.to_string(),
            has_restorable_agent: true,
            is_live: true,
            is_protected: false,
            lifecycle_idle: true,
            has_unconfirmed_terminal_input: false,
            last_activity_ms,
        };
        let selected = selected_surface_ids(
            &[
                input("surface-c", 4_000),
                input("surface-a", 1_000),
                input("surface-b", 2_000),
                input("surface-d", 3_000),
            ],
            settings,
            20_000,
        );
        assert_eq!(
            selected,
            HashSet::from(["surface-a".to_string(), "surface-b".to_string()])
        );
    }

    #[test]
    fn planner_counts_protected_live_surfaces_but_never_selects_them() {
        let settings = Settings {
            enabled: true,
            idle_seconds: 5,
            max_live_terminals: 1,
            confirmation_seconds: 60,
        };
        let selected = selected_surface_ids(
            &[
                PlannerInput {
                    surface_id: "visible".to_string(),
                    has_restorable_agent: true,
                    is_live: true,
                    is_protected: true,
                    lifecycle_idle: true,
                    has_unconfirmed_terminal_input: false,
                    last_activity_ms: 1_000,
                },
                PlannerInput {
                    surface_id: "background".to_string(),
                    has_restorable_agent: true,
                    is_live: true,
                    is_protected: false,
                    lifecycle_idle: true,
                    has_unconfirmed_terminal_input: false,
                    last_activity_ms: 2_000,
                },
            ],
            settings,
            20_000,
        );
        assert_eq!(selected, HashSet::from(["background".to_string()]));
    }

    #[test]
    fn planner_rejects_active_unconfirmed_and_non_restorable_surfaces() {
        let settings = Settings {
            enabled: true,
            idle_seconds: 5,
            max_live_terminals: 1,
            confirmation_seconds: 60,
        };
        let base = PlannerInput {
            surface_id: "eligible".to_string(),
            has_restorable_agent: true,
            is_live: true,
            is_protected: false,
            lifecycle_idle: true,
            has_unconfirmed_terminal_input: false,
            last_activity_ms: 1_000,
        };
        let selected = selected_surface_ids(
            &[
                base.clone(),
                PlannerInput {
                    surface_id: "running".to_string(),
                    lifecycle_idle: false,
                    ..base.clone()
                },
                PlannerInput {
                    surface_id: "unconfirmed".to_string(),
                    has_unconfirmed_terminal_input: true,
                    ..base.clone()
                },
                PlannerInput {
                    surface_id: "not-restorable".to_string(),
                    has_restorable_agent: false,
                    ..base
                },
            ],
            settings,
            20_000,
        );
        assert_eq!(selected, HashSet::from(["eligible".to_string()]));
    }
}
