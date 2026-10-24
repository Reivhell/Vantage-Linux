use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SimpleResult {
    pub ok: bool,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProcessInfo {
    pub pid: i32,
    pub name: String,
    pub user: String,
    pub cpu: f32,
    pub memory_mb: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Choice {
    pub key: String,
    pub label: String,
    pub hint: Option<String>,
}

impl Choice {
    pub(crate) fn new(key: &str, label: &str) -> Self {
        Self {
            key: key.to_string(),
            label: label.to_string(),
            hint: None,
        }
    }

    pub(crate) fn with_hint(key: &str, label: &str, hint: &str) -> Self {
        Self {
            key: key.to_string(),
            label: label.to_string(),
            hint: Some(hint.to_string()),
        }
    }
}

pub(crate) fn thermal_label(key: &str) -> String {
    match key {
        "low-power" => "Power saving".into(),
        "power-saver" => "Power saving".into(),
        "quiet" => "Quiet".into(),
        "balanced" => "Balanced".into(),
        "normal" => "Balanced".into(),
        "performance" => "Performance".into(),
        "max-power" => "Max performance".into(),
        "custom" => "Custom".into(),
        other => other.to_string(),
    }
}

pub(crate) fn charge_label(key: &str) -> String {
    match key {
        "Fast" => "Rapid charge".into(),
        "Standard" => "Standard".into(),
        "Long_Life" => "Conservation (80%)".into(),
        other => other.to_string(),
    }
}

pub(crate) fn which(cmd: &str) -> Option<PathBuf> {
    if let Ok(p) = std::env::var("PATH") {
        for dir in p.split(':') {
            let full = Path::new(dir).join(cmd);
            if full.is_file() {
                return Some(full);
            }
        }
    }
    None
}
