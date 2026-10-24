use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

fn battery_paths() -> Vec<PathBuf> {
    let Ok(entries) = fs::read_dir("/sys/class/power_supply") else {
        return Vec::new();
    };
    entries
        .flatten()
        .map(|e| e.path())
        .filter(|p| {
            p.file_name()
                .map(|n| n.to_string_lossy().starts_with("BAT"))
                .unwrap_or(false)
        })
        .collect()
}

fn read_str(path: &str) -> Option<String> {
    fs::read_to_string(path).ok().map(|v| v.trim().to_string())
}

fn read_num(path: &str) -> Option<f64> {
    read_str(path)?.parse().ok()
}

fn first_file(name: &str) -> Option<PathBuf> {
    battery_paths()
        .into_iter()
        .map(|p| p.join(name))
        .find(|p| p.exists())
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BatteryDetail {
    pub available: bool,
    pub percent: Option<f64>,
    pub status: String,
    pub health_pct: Option<f64>,
    pub cycles: Option<u32>,
    pub temp_c: Option<f64>,
    pub energy_now_wh: Option<f64>,
    pub energy_full_wh: Option<f64>,
    pub energy_design_wh: Option<f64>,
    pub voltage_v: Option<f64>,
    pub power_w: Option<f64>,
    pub charge_types: Vec<String>,
    pub current_charge_type: String,
    pub threshold_available: bool,
    pub threshold_pct: Option<u32>,
    pub charge_behaviours: Vec<String>,
    pub current_charge_behaviour: String,
}

pub fn read_battery_detail() -> BatteryDetail {
    let mut detail = BatteryDetail {
        available: false,
        percent: None,
        status: String::new(),
        health_pct: None,
        cycles: None,
        temp_c: None,
        energy_now_wh: None,
        energy_full_wh: None,
        energy_design_wh: None,
        voltage_v: None,
        power_w: None,
        charge_types: Vec::new(),
        current_charge_type: String::new(),
        threshold_available: false,
        threshold_pct: None,
        charge_behaviours: Vec::new(),
        current_charge_behaviour: String::new(),
    };

    let Some(path) = battery_paths().into_iter().next() else {
        return detail;
    };
    detail.available = true;
    detail.percent = read_num(&format!("{}/capacity", path.display()));
    detail.status = read_str(&format!("{}/status", path.display())).unwrap_or_default();
    detail.cycles =
        read_str(&format!("{}/cycle_count", path.display())).and_then(|v| v.parse().ok());

    let unit_scale = 1_000_000.0;
    let voltage_uv = read_num(&format!("{}/voltage_now", path.display()));
    let to_wh = |energy_wh: Option<f64>, charge_uh: Option<f64>| -> Option<f64> {
        energy_wh
            .map(|v| v / unit_scale)
            .or_else(|| charge_uh.zip(voltage_uv).map(|(c, v)| c * v / 1e12))
    };
    let energy_full = to_wh(
        read_num(&format!("{}/energy_full", path.display())),
        read_num(&format!("{}/charge_full", path.display())),
    );
    let design = to_wh(
        read_num(&format!("{}/energy_full_design", path.display())),
        read_num(&format!("{}/charge_full_design", path.display())),
    );
    detail.energy_full_wh = energy_full;
    detail.energy_design_wh = design;
    detail.energy_now_wh = to_wh(
        read_num(&format!("{}/energy_now", path.display())),
        read_num(&format!("{}/charge_now", path.display())),
    );
    if let (Some(full), Some(design)) = (energy_full, design) {
        if design > 0.0 {
            detail.health_pct = Some((full / design * 1000.0).round() / 10.0);
        }
    }

    detail.temp_c = read_num(&format!("{}/temp", path.display())).map(|v| v / 10.0);
    detail.voltage_v = read_num(&format!("{}/voltage_now", path.display())).map(|v| v / unit_scale);
    detail.power_w = read_num(&format!("{}/power_now", path.display()))
        .map(|v| v / unit_scale)
        .or_else(|| {
            let v = read_num(&format!("{}/voltage_now", path.display()))?;
            let i = read_num(&format!("{}/current_now", path.display()))?;
            Some(v * i / 1_000_000_000_000.0)
        });

    if let Some(path) = first_file("charge_types") {
        if let Ok(raw) = fs::read_to_string(path) {
            for token in raw.split_whitespace() {
                let active = token.starts_with('[') && token.ends_with(']');
                let name = token.trim_matches(|c| c == '[' || c == ']').to_string();
                if active {
                    detail.current_charge_type = name.clone();
                }
                detail.charge_types.push(name);
            }
        }
    }
    if let Some(path) = first_file("charge_control_end_threshold") {
        detail.threshold_available = true;
        detail.threshold_pct = read_str(&path.to_string_lossy()).and_then(|v| v.parse().ok());
    }
    if let Some(path) = first_file("charge_behaviour") {
        if let Ok(raw) = fs::read_to_string(path) {
            for token in raw.split_whitespace() {
                let active = token.starts_with('[') && token.ends_with(']');
                let name = token.trim_matches(|c| c == '[' || c == ']').to_string();
                if active {
                    detail.current_charge_behaviour = name.clone();
                }
                detail.charge_behaviours.push(name);
            }
        }
    }
    detail
}

pub fn set_charge_threshold(percent: u32) -> crate::SimpleResult {
    if !(50..=100).contains(&percent) {
        return crate::SimpleResult {
            ok: false,
            message: "threshold must be 50-100".into(),
        };
    }
    if first_file("charge_control_end_threshold").is_none() {
        return crate::SimpleResult {
            ok: false,
            message: "firmware does not expose a charge threshold".into(),
        };
    }
    crate::run_helper("charge-threshold", &[&percent.to_string()])
}
