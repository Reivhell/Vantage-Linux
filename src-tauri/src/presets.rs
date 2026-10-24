use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PresetInfo {
    pub key: String,
    pub label: String,
    pub description: String,
    pub available: bool,
    pub active: bool,
    pub thermal_mode: Option<String>,
}

fn first_choice(choices: &[String], candidates: &[&str]) -> Option<String> {
    candidates
        .iter()
        .find(|c| choices.iter().any(|v| v == *c))
        .map(|c| c.to_string())
}

pub fn presets() -> Vec<PresetInfo> {
    let thermal = crate::get_thermal_mode();
    let choices: Vec<String> = thermal.choices.iter().map(|c| c.key.clone()).collect();
    let definitions = [
        (
            "quiet",
            "Quiet",
            "Lowest noise and power draw",
            first_choice(&choices, &["quiet", "low-power", "power-saver"]),
        ),
        (
            "balanced",
            "Balanced",
            "Daily use and mixed workloads",
            first_choice(&choices, &["balanced", "normal"]),
        ),
        (
            "performance",
            "Performance",
            "Maximum sustained performance",
            first_choice(&choices, &["performance", "max-power"]),
        ),
        (
            "custom",
            "Custom",
            "Firmware custom platform profile",
            first_choice(&choices, &["custom"]),
        ),
    ];
    definitions
        .into_iter()
        .map(|(key, label, description, mode)| PresetInfo {
            key: key.to_string(),
            label: label.to_string(),
            description: description.to_string(),
            available: mode.is_some(),
            active: mode.as_deref() == Some(thermal.current.as_str()),
            thermal_mode: mode,
        })
        .collect()
}

pub fn apply_preset(name: &str) -> crate::SimpleResult {
    let Some(preset) = presets().into_iter().find(|p| p.key == name) else {
        return crate::SimpleResult {
            ok: false,
            message: format!("unknown preset: {name}"),
        };
    };
    let Some(mode) = preset.thermal_mode else {
        return crate::SimpleResult {
            ok: false,
            message: format!("preset '{name}' is not supported by this firmware"),
        };
    };
    crate::set_thermal_mode(&mode)
}
