use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::process::Command;
use std::sync::{Mutex, OnceLock};

use crate::{SimpleResult, HELPER_PATH};

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
static THROTTLE_ALERTED: Mutex<bool> = Mutex::new(false);

fn fan_state() -> &'static Mutex<HashMap<i32, FanApplied>> {
    FAN_STATE.get_or_init(|| Mutex::new(HashMap::new()))
}

fn firmware_reset_flag() -> &'static Mutex<bool> {
    FIRMWARE_RESET.get_or_init(|| Mutex::new(false))
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

/// Periksa apakah kipas yang diatur manual tidak berputar (firmware reset).
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
            *firmware_reset_flag().lock().unwrap() = false;
        }
    }
}

pub fn cmd_fan_status() -> FanStatus {
    let (fan1, fan2) = crate::read_fan_rpms();
    let cpu_temp = crate::read_cpu_temp();
    // Throttle jika CPU >= 95 °C. GPU temp dibaca dari sysfs jika tersedia.
    let gpu_temp = crate::read_gpu_temp_sysfs();
    let throttle = cpu_temp.map_or(false, |t| t >= 95.0) || gpu_temp.map_or(false, |t| t >= 100.0);

    {
        let mut alerted = THROTTLE_ALERTED.lock().unwrap();
        *alerted = throttle;
    }

    fan_check_reset(1, fan1);
    fan_check_reset(2, fan2);

    let guard = fan_state().lock().unwrap();
    let manual_active = guard.values().any(|e| e.mode == "manual" && e.pwm > 0);
    drop(guard);

    FanStatus {
        available: crate::find_hwmon().is_some(),
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

pub fn cmd_apply_fan_profile(cmd: FanProfileCmd) -> SimpleResult {
    if cmd.fan_id != 1 && cmd.fan_id != 2 {
        return SimpleResult {
            ok: false,
            message: "fan_id must be 1 or 2".into(),
        };
    }
    let profile = cmd.profile.to_lowercase();
    match profile.as_str() {
        "auto" => {
            let r = crate::run_helper("fan-auto", &[cmd.fan_id.to_string().as_str()]);
            if r.ok {
                fan_record_applied(cmd.fan_id, "auto", 0);
            }
            r
        }
        "quiet" => {
            let points = vec![
                crate::FanCurvePoint { temp: 45, pwm: 25 },
                crate::FanCurvePoint { temp: 60, pwm: 35 },
                crate::FanCurvePoint { temp: 75, pwm: 50 },
                crate::FanCurvePoint { temp: 90, pwm: 75 },
            ];
            let payload = serde_json::to_string(&points).unwrap_or_default();
            let r = crate::run_helper("fan-curve", &[cmd.fan_id.to_string().as_str(), &payload]);
            if r.ok {
                fan_record_applied(cmd.fan_id, "curve", 0);
            }
            r
        }
        "full" => {
            let r = crate::run_helper("fan-manual", &[cmd.fan_id.to_string().as_str(), "100"]);
            if r.ok {
                fan_record_applied(cmd.fan_id, "manual", 100);
            }
            r
        }
        _ => SimpleResult {
            ok: false,
            message: format!("fan profile '{}' not supported (auto/quiet/full)", profile),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_fan_record_and_reset_detection() {
        fan_record_applied(1, "manual", 80);
        fan_check_reset(1, 0);
        fan_check_reset(1, 0);
        assert!(!fan_is_firmware_reset());
        fan_check_reset(1, 0);
        assert!(fan_is_firmware_reset());
    }

    #[test]
    fn test_rpm_recovery_clears_reset() {
        fan_record_applied(2, "manual", 60);
        fan_check_reset(2, 0);
        fan_check_reset(2, 0);
        fan_check_reset(2, 2000);
        assert!(!fan_is_firmware_reset());
    }
}
