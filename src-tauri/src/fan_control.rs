use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::process::Command;
use std::sync::{Mutex, OnceLock};

use crate::config::HELPER_PATH;
use crate::power::run_helper;
use crate::stats::{
    find_hwmon, read_cpu_temp, read_fan_rpms, read_gpu_temp_sysfs,
};
use crate::types::SimpleResult;

// ---------------------------------------------------------------------------
// Fan control
// ---------------------------------------------------------------------------

#[derive(Debug, Serialize, Deserialize)]
pub struct FanCurvePoint {
    pub temp: i32,
    pub pwm: i32,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct FanCurveCmd {
    pub fan_id: i32,
    pub points: Vec<FanCurvePoint>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct FanInfo {
    pub available: bool,
    pub fan_count: u32,
    pub max_points: usize,
    pub temp_max: i32,
    pub pwm_max: i32,
    pub default_curve: Vec<FanCurvePoint>,
}

/// Status kipas yang diexpose ke frontend.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FanStatus {
    pub available: bool,
    pub firmware_reset: bool,
    pub manual_active: bool,
    pub fan1_rpm: u32,
    pub fan2_rpm: u32,
    pub throttle_alert: bool,
}

/// Apa yang terakhir kali diterapkan ke kipas.
#[derive(Debug, Clone)]
struct FanApplied {
    mode: String, // "manual", "curve", "auto"
    pwm: i32,
    zero_rpm_count: u32,
}

static FAN_STATE: OnceLock<Mutex<HashMap<i32, FanApplied>>> = OnceLock::new();
static FIRMWARE_RESET: OnceLock<Mutex<bool>> = OnceLock::new();
static THROTTLE_ALERTED: OnceLock<Mutex<bool>> = OnceLock::new();

fn fan_state() -> &'static Mutex<HashMap<i32, FanApplied>> {
    FAN_STATE.get_or_init(|| Mutex::new(HashMap::new()))
}

fn fan_reset_test_state() {
    *firmware_reset_flag().lock().unwrap() = false;
    fan_state().lock().unwrap().clear();
}

fn firmware_reset_flag() -> &'static Mutex<bool> {
    FIRMWARE_RESET.get_or_init(|| Mutex::new(false))
}

fn throttle_alerted() -> &'static Mutex<bool> {
    THROTTLE_ALERTED.get_or_init(|| Mutex::new(false))
}

/// Kembalikan kontrol kipas ke firmware saat proses berhenti (fail-safe).
pub fn fan_fail_safe() {
    for id in [1i32, 2] {
        for program in ["sudo", "pkexec"] {
            let mut cmd = Command::new(program);
            if program == "sudo" {
                cmd.arg("-n");
            }
            cmd.arg(HELPER_PATH).arg("fan-auto").arg(id.to_string());
            let _ = cmd.output();
        }
    }
    *firmware_reset_flag().lock().unwrap() = false;
}

/// Tandai bahwa firmware telah mengembalikan kipas ke curve bawaan.
pub fn fan_mark_firmware_reset() {
    *firmware_reset_flag().lock().unwrap() = true;
}

pub fn fan_is_firmware_reset() -> bool {
    *firmware_reset_flag().lock().unwrap()
}

/// Catat penggunaan manual kipas untuk deteksi reset firmware.
pub fn fan_record_applied(fan_id: i32, mode: &str, pwm: i32) {
    let mut guard = fan_state().lock().unwrap();
    guard.insert(
        fan_id,
        FanApplied {
            mode: mode.to_string(),
            pwm,
            zero_rpm_count: 0,
        },
    );
}

/// Periksa apakah kipas yang diatur manual tidak berputar (firmeware reset).
/// Dipanggil setiap siklus monitoring. Jika manual PWM > 0 tapi RPM 0 beberapa kali,
/// anggap firmware telah mengambil alih kembali.
pub fn fan_check_reset(fan_id: i32, rpm: u32) {
    let mut guard = fan_state().lock().unwrap();
    let entry = match guard.get_mut(&fan_id) {
        Some(e) => e,
        None => return,
    };
    if entry.mode == "manual" && entry.pwm > 0 {
        if rpm == 0 {
            entry.zero_rpm_count += 1;
            if entry.zero_rpm_count >= 3 {
                fan_mark_firmware_reset();
                entry.zero_rpm_count = 0;
            }
        } else {
            entry.zero_rpm_count = 0;
        }
    }
}

#[tauri::command]
pub fn cmd_get_fan_info() -> FanInfo {
    FanInfo {
        available: find_hwmon().is_some(),
        fan_count: 2,
        max_points: 10,
        temp_max: 110,
        pwm_max: 100,
        default_curve: vec![
            FanCurvePoint { temp: 45, pwm: 25 },
            FanCurvePoint { temp: 60, pwm: 45 },
            FanCurvePoint { temp: 75, pwm: 70 },
            FanCurvePoint { temp: 88, pwm: 100 },
        ],
    }
}

#[tauri::command]
pub fn cmd_fan_status() -> FanStatus {
    let (fan1, fan2) = read_fan_rpms();
    let cpu_temp = read_cpu_temp();
    // Throttle jika CPU >= 95 °C. GPU temp dibaca dari sysfs jika tersedia.
    let gpu_temp = read_gpu_temp_sysfs();
    let throttle = cpu_temp.map_or(false, |t| t >= 95.0)
        || gpu_temp.map_or(false, |t| t >= 100.0);

    // Debounce throttle alert agar tidak spam.
    {
        let mut alerted = throttle_alerted().lock().unwrap();
        if throttle {
            *alerted = true;
        } else {
            *alerted = false;
        }
    }

    // Deteksi reset pada kedua kipas.
    fan_check_reset(1, fan1);
    fan_check_reset(2, fan2);

    let mut guard = fan_state().lock().unwrap();
    let manual_active = guard.values().any(|e| e.mode == "manual" && e.pwm > 0);
    drop(guard);

    FanStatus {
        available: find_hwmon().is_some(),
        firmware_reset: fan_is_firmware_reset(),
        manual_active,
        fan1_rpm: fan1,
        fan2_rpm: fan2,
        throttle_alert: throttle,
    }
}

#[derive(Debug, Deserialize)]
pub struct FanProfileCmd {
    pub profile: String, // "auto", "quiet", "full"
    pub fan_id: i32,
}

#[tauri::command]
pub fn cmd_apply_fan_profile(cmd: FanProfileCmd) -> SimpleResult {
    if cmd.fan_id != 1 && cmd.fan_id != 2 {
        return SimpleResult {
            ok: false,
            message: "fan_id must be 1 or 2".into(),
        };
    }
    let profile = cmd.profile.to_lowercase();
    let res = match profile.as_str() {
        "auto" => {
            let r = run_helper("fan-auto", &[cmd.fan_id.to_string().as_str()]);
            if r.ok {
                fan_record_applied(cmd.fan_id, "auto", 0);
            }
            r
        }
        "quiet" => {
            // Curve rendah: 45°C=25%, 60°C=35%, 75°C=50%, 90°C=75%
            let points = vec![
                FanCurvePoint { temp: 45, pwm: 25 },
                FanCurvePoint { temp: 60, pwm: 35 },
                FanCurvePoint { temp: 75, pwm: 50 },
                FanCurvePoint { temp: 90, pwm: 75 },
            ];
            let payload = serde_json::to_string(&points).unwrap_or_default();
            let r = run_helper(
                "fan-curve",
                &[cmd.fan_id.to_string().as_str(), &payload],
            );
            if r.ok {
                fan_record_applied(cmd.fan_id, "curve", 0);
            }
            r
        }
        "full" => {
            // Paksa 100% via manual (fail-safe masih aktif saat daemon mati).
            let r = run_helper(
                "fan-manual",
                &[cmd.fan_id.to_string().as_str(), "100"],
            );
            if r.ok {
                fan_record_applied(cmd.fan_id, "manual", 100);
            }
            r
        }
        _ => SimpleResult {
            ok: false,
            message: format!("fan profile '{}' not supported (auto/quiet/full)", profile),
        },
    };
    res
}

#[tauri::command]
pub fn cmd_apply_fan_curve(cmd: FanCurveCmd) -> SimpleResult {
    if cmd.fan_id != 1 && cmd.fan_id != 2 {
        return SimpleResult { ok: false, message: "fan_id must be 1 or 2".into() };
    }
    if cmd.points.len() < 2 || cmd.points.len() > 10 {
        return SimpleResult { ok: false, message: "expected 2–10 curve points".into() };
    }
    if cmd.points.iter().any(|p| !(0..=110).contains(&p.temp) || !(0..=100).contains(&p.pwm)) {
        return SimpleResult { ok: false, message: "temp 0–110 °C, speed 0–100 % required".into() };
    }
    let mut points = cmd.points;
    points.sort_by_key(|p| p.temp);
    let payload = serde_json::to_string(&points).unwrap_or_default();
    run_helper("fan-curve", &[cmd.fan_id.to_string().as_str(), &payload])
}

#[tauri::command]
pub fn cmd_set_fan_manual(fan_id: i32, pwm_percent: i32) -> SimpleResult {
    if fan_id != 1 && fan_id != 2 {
        return SimpleResult { ok: false, message: "fan_id must be 1 or 2".into() };
    }
    run_helper(
        "fan-manual",
        &[fan_id.to_string().as_str(), pwm_percent.clamp(0, 100).to_string().as_str()],
    )
}

#[tauri::command]
pub fn cmd_fan_auto(fan_id: i32) -> SimpleResult {
    if fan_id != 1 && fan_id != 2 {
        return SimpleResult { ok: false, message: "fan_id must be 1 or 2".into() };
    }
    run_helper("fan-auto", &[fan_id.to_string().as_str()])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_fan_record_and_reset_detection() {
        fan_reset_test_state();
        // RPM 0 dengan manual PWM > 0 tiga kali -> firmware reset.
        fan_record_applied(1, "manual", 80);
        fan_check_reset(1, 0);
        fan_check_reset(1, 0);
        assert!(!fan_is_firmware_reset());
        fan_check_reset(1, 0);
        assert!(fan_is_firmware_reset());
    }

    #[test]
    fn test_rpm_recovery_clears_reset() {
        fan_reset_test_state();
        fan_record_applied(2, "manual", 60);
        fan_check_reset(2, 0);
        fan_check_reset(2, 0);
        fan_check_reset(2, 2000);
        assert!(!fan_is_firmware_reset());
    }
}
