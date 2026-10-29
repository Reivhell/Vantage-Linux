use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};

use crate::power::{run_helper, run_helper_quiet};
use crate::stats::{find_hwmon, read_cpu_temp, read_fan_rpms, read_nvidia_cached};
use crate::types::SimpleResult;

const FAN_MODE_CANDIDATES: &[&str] = &[
    "/sys/devices/pci0000:00/0000:00:1f.0/PNP0C09:00/VPC2004:00/fan_mode",
    "/sys/bus/platform/drivers/ideapad_acpi/VPC2004:00/fan_mode",
];

const FAN_MODES: [(u8, &str, &str); 4] = [
    (0, "silent", "Super Silent"),
    (1, "standard", "Standard"),
    (2, "dust", "Dust Cleaning"),
    (4, "cooling", "Efficient Cooling"),
];

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FanCurvePoint {
    pub temp: i32,
    pub pwm: i32,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct FanCurveCmd {
    pub fan_id: i32,
    pub points: Vec<FanCurvePoint>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FanModeInfo {
    pub key: String,
    pub label: String,
    pub value: u8,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct FanInfo {
    pub available: bool,
    pub mode_control: bool,
    pub pwm_control: bool,
    pub fan_count: u32,
    pub max_points: usize,
    pub temp_max: i32,
    pub pwm_max: i32,
    pub default_curve: Vec<FanCurvePoint>,
    pub modes: Vec<FanModeInfo>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FanStatus {
    pub available: bool,
    pub mode_control: bool,
    pub pwm_control: bool,
    pub rpm_read: bool,
    pub firmware_reset: bool,
    pub manual_active: bool,
    pub fan1_rpm: Option<u32>,
    pub fan2_rpm: Option<u32>,
    pub mode: String,
    pub mode_label: String,
    pub throttle_alert: bool,
}

static LAST_APPLIED_MODE: OnceLock<Mutex<Option<u8>>> = OnceLock::new();
static ORIGINAL_MODE: OnceLock<Mutex<Option<u8>>> = OnceLock::new();

fn last_applied() -> &'static Mutex<Option<u8>> {
    LAST_APPLIED_MODE.get_or_init(|| Mutex::new(None))
}

fn original_mode() -> &'static Mutex<Option<u8>> {
    ORIGINAL_MODE.get_or_init(|| Mutex::new(None))
}

fn fan_mode_path() -> Option<PathBuf> {
    FAN_MODE_CANDIDATES
        .iter()
        .map(PathBuf::from)
        .find(|path| Path::new(path).is_file())
}

fn mode_control_available() -> bool {
    fan_mode_path().is_some()
}

fn pwm_control_available() -> bool {
    find_hwmon()
        .map(|path| path.join("pwm1").exists() || path.join("pwm2").exists())
        .unwrap_or(false)
}

fn read_mode_raw() -> Option<u8> {
    let value = fs::read_to_string(fan_mode_path()?).ok()?;
    value.trim().parse::<u8>().ok()
}

fn mode_value(key: &str) -> Option<u8> {
    let key = key.trim().to_lowercase().replace(['_', ' '], "-");
    match key.as_str() {
        "silent" | "quiet" | "super-silent" => Some(0),
        "standard" | "auto" | "balanced" | "normal" => Some(1),
        "dust" | "dust-cleaning" => Some(2),
        "cooling" | "performance" | "full" | "max" => Some(4),
        _ => None,
    }
}

fn mode_label(value: Option<u8>) -> (String, String) {
    match value {
        Some(0) => ("silent".into(), "Super Silent".into()),
        Some(1) => ("standard".into(), "Standard".into()),
        Some(2) => ("dust".into(), "Dust Cleaning".into()),
        Some(4) => ("cooling".into(), "Efficient Cooling".into()),
        Some(other) => ("unknown".into(), format!("Firmware ({other})")),
        None => ("unknown".into(), "Tidak tersedia".into()),
    }
}

fn modes() -> Vec<FanModeInfo> {
    FAN_MODES
        .iter()
        .map(|(value, key, label)| FanModeInfo {
            key: (*key).into(),
            label: (*label).into(),
            value: *value,
        })
        .collect()
}

#[tauri::command]
pub fn cmd_get_fan_info() -> FanInfo {
    let (fan1, fan2) = read_fan_rpms();
    FanInfo {
        available: mode_control_available() || pwm_control_available(),
        mode_control: mode_control_available(),
        pwm_control: pwm_control_available(),
        fan_count: fan1.is_some() as u32 + fan2.is_some() as u32,
        max_points: if pwm_control_available() { 10 } else { 0 },
        temp_max: 110,
        pwm_max: 100,
        default_curve: Vec::new(),
        modes: modes(),
    }
}

#[tauri::command]
pub fn cmd_fan_status() -> FanStatus {
    let (fan1, fan2) = read_fan_rpms();
    let raw_mode = read_mode_raw();
    let (mode, label) = mode_label(raw_mode);
    let cpu = read_cpu_temp();
    let gpu = read_nvidia_cached(2.0).3.map(|v| v as f32);
    let throttle = cpu.map_or(false, |t| t >= 95.0) || gpu.map_or(false, |t| t >= 100.0);
    let applied = *last_applied().lock().unwrap();

    FanStatus {
        available: mode_control_available() || pwm_control_available(),
        mode_control: mode_control_available(),
        pwm_control: pwm_control_available(),
        rpm_read: fan1.is_some() || fan2.is_some(),
        firmware_reset: false,
        manual_active: applied.is_some(),
        fan1_rpm: fan1,
        fan2_rpm: fan2,
        mode,
        mode_label: label,
        throttle_alert: throttle,
    }
}

#[tauri::command]
pub fn cmd_set_fan_mode(mode: String) -> SimpleResult {
    let Some(value) = mode_value(&mode) else {
        return SimpleResult {
            ok: false,
            message: format!("mode kipas '{mode}' tidak didukung"),
        };
    };
    if !mode_control_available() {
        return SimpleResult {
            ok: false,
            message: "kontrol mode kipas tidak tersedia di firmware ini".into(),
        };
    }

    let before = read_mode_raw().filter(|v| FAN_MODES.iter().any(|(m, _, _)| m == v));
    if before.is_some() {
        let mut original = original_mode().lock().unwrap();
        if original.is_none() {
            *original = before;
        }
    }

    let result = run_helper("fan-mode", &[&value.to_string()]);
    if result.ok {
        *last_applied().lock().unwrap() = Some(value);
    }
    result
}

#[tauri::command]
pub fn cmd_apply_fan_profile(cmd: FanProfileCmd) -> SimpleResult {
    cmd_set_fan_mode(cmd.profile)
}

#[derive(Debug, Deserialize)]
pub struct FanProfileCmd {
    pub profile: String,
}

#[tauri::command]
pub fn cmd_apply_fan_curve(cmd: FanCurveCmd) -> SimpleResult {
    if !pwm_control_available() {
        return SimpleResult {
            ok: false,
            message: "hardware ini tidak menyediakan kontrol PWM/kurva kipas".into(),
        };
    }
    if cmd.fan_id != 1 && cmd.fan_id != 2 {
        return SimpleResult {
            ok: false,
            message: "fan_id must be 1 or 2".into(),
        };
    }
    if cmd.points.len() < 2 || cmd.points.len() > 10 {
        return SimpleResult {
            ok: false,
            message: "expected 2–10 curve points".into(),
        };
    }
    if cmd
        .points
        .iter()
        .any(|p| !(0..=110).contains(&p.temp) || !(0..=100).contains(&p.pwm))
    {
        return SimpleResult {
            ok: false,
            message: "temp 0–110 °C, speed 0–100 % required".into(),
        };
    }
    let mut points = cmd.points;
    points.sort_by_key(|p| p.temp);
    let payload = serde_json::to_string(&points).unwrap_or_default();
    run_helper("fan-curve", &[cmd.fan_id.to_string().as_str(), &payload])
}

#[tauri::command]
pub fn cmd_set_fan_manual(fan_id: i32, pwm_percent: i32) -> SimpleResult {
    if !pwm_control_available() {
        return SimpleResult {
            ok: false,
            message: "hardware ini tidak menyediakan kontrol PWM kipas".into(),
        };
    }
    if fan_id != 1 && fan_id != 2 {
        return SimpleResult {
            ok: false,
            message: "fan_id must be 1 or 2".into(),
        };
    }
    run_helper(
        "fan-manual",
        &[
            fan_id.to_string().as_str(),
            pwm_percent.clamp(0, 100).to_string().as_str(),
        ],
    )
}

#[tauri::command]
pub fn cmd_fan_auto(fan_id: i32) -> SimpleResult {
    if mode_control_available() {
        return cmd_set_fan_mode("standard".into());
    }
    if !pwm_control_available() {
        return SimpleResult {
            ok: false,
            message: "kontrol kipas tidak tersedia".into(),
        };
    }
    if fan_id != 1 && fan_id != 2 {
        return SimpleResult {
            ok: false,
            message: "fan_id must be 1 or 2".into(),
        };
    }
    run_helper("fan-auto", &[fan_id.to_string().as_str()])
}

/// Kembalikan mode kipas ke kondisi awal saat aplikasi berhenti.
pub fn fan_fail_safe() {
    if last_applied().lock().unwrap().is_none() {
        return;
    }
    if !mode_control_available() {
        return;
    }
    let restore = original_mode()
        .lock()
        .unwrap()
        .filter(|v| FAN_MODES.iter().any(|(m, _, _)| m == v))
        .unwrap_or(1);
    let _ = run_helper_quiet("fan-mode", &[&restore.to_string()]);
    *last_applied().lock().unwrap() = None;
    *original_mode().lock().unwrap() = None;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fan_mode_aliases_are_validated() {
        assert_eq!(mode_value("silent"), Some(0));
        assert_eq!(mode_value("auto"), Some(1));
        assert_eq!(mode_value("performance"), Some(4));
        assert_eq!(mode_value("dust cleaning"), Some(2));
        assert_eq!(mode_value("custom"), None);
    }

    #[test]
    fn mode_labels_are_human_readable() {
        assert_eq!(mode_label(Some(0)).1, "Super Silent");
        assert_eq!(mode_label(Some(4)).0, "cooling");
        assert_eq!(mode_label(Some(133)).0, "unknown");
    }
}
