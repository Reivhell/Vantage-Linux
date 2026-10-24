use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Mutex;
use std::thread;
use std::time::Duration;

use crate::config::{load_config, save_config, HELPER_PATH};
use crate::presets;
use crate::types::{charge_label, thermal_label, which, Choice, SimpleResult};

static LAST_PLUGGED: Mutex<Option<bool>> = Mutex::new(None);

// ---------------------------------------------------------------------------
// Power profiles
// ---------------------------------------------------------------------------

#[derive(Debug, Serialize, Deserialize)]
pub struct ProfileList {
    pub profiles: Vec<String>,
}

pub(crate) fn run_helper(subcmd: &str, args: &[&str]) -> SimpleResult {
    run_helper_inner(subcmd, args, true)
}

/// Read-only helper calls must never pop a polkit password dialog.
pub(crate) fn run_helper_quiet(subcmd: &str, args: &[&str]) -> SimpleResult {
    run_helper_inner(subcmd, args, false)
}

fn run_helper_inner(subcmd: &str, args: &[&str], allow_pkexec: bool) -> SimpleResult {
    if !Path::new(HELPER_PATH).is_file() {
        return SimpleResult {
            ok: false,
            message: "loq-helper is not installed (run scripts/install.sh)".into(),
        };
    }

    let mut last_message = String::new();
    let mut attempts: Vec<(&str, bool)> = vec![("sudo", true)];
    if allow_pkexec {
        attempts.push(("pkexec", false));
    }

    const MAX_RETRIES: u32 = 3;
    let mut backoff = Duration::from_millis(100);

    for attempt in 0..MAX_RETRIES {
        for (program, passwordless) in &attempts {
            let mut cmd = Command::new(program);
            if *passwordless {
                cmd.arg("-n");
            }
            cmd.arg(HELPER_PATH).arg(subcmd);
            for a in args {
                cmd.arg(a);
            }
            match cmd.output() {
                Ok(o) if o.status.success() => {
                    return SimpleResult {
                        ok: true,
                        message: String::from_utf8_lossy(&o.stdout).trim().to_string(),
                    };
                }
                Ok(o) => {
                    last_message = String::from_utf8_lossy(&o.stderr).trim().to_string();
                    if last_message.is_empty() {
                        last_message = format!("{program} exited with {}", o.status);
                    }
                    if *passwordless && !last_message.contains("password") {
                        return SimpleResult {
                            ok: false,
                            message: last_message,
                        };
                    }
                }
                Err(e) => last_message = e.to_string(),
            }
        }
        if attempt + 1 < MAX_RETRIES {
            thread::sleep(backoff);
            backoff *= 2;
        }
    }
    SimpleResult {
        ok: false,
        message: last_message,
    }
}

fn powerprofilesctl(args: &[&str]) -> SimpleResult {
    match Command::new("powerprofilesctl").args(args).output() {
        Ok(o) => {
            if o.status.success() {
                SimpleResult {
                    ok: true,
                    message: String::from_utf8_lossy(&o.stdout).trim().to_string(),
                }
            } else {
                SimpleResult {
                    ok: false,
                    message: String::from_utf8_lossy(&o.stderr).trim().to_string(),
                }
            }
        }
        Err(e) => SimpleResult {
            ok: false,
            message: e.to_string(),
        },
    }
}

pub fn get_available_profiles() -> ProfileList {
    match powerprofilesctl(&["list"]) {
        SimpleResult {
            ok: true, message, ..
        } => {
            let mut profiles = Vec::new();
            for line in message.lines() {
                let line = line.trim();
                if line.ends_with(':') {
                    profiles.push(
                        line.trim_end_matches(':')
                            .trim_start_matches('*')
                            .trim()
                            .to_string(),
                    );
                }
            }
            if profiles.is_empty() {
                profiles = vec![
                    "power-saver".into(),
                    "balanced".into(),
                    "performance".into(),
                ];
            }
            ProfileList { profiles }
        }
        _ => ProfileList {
            profiles: vec![
                "power-saver".into(),
                "balanced".into(),
                "performance".into(),
            ],
        },
    }
}

// ---------------------------------------------------------------------------
// Battery conservation
// ---------------------------------------------------------------------------

pub fn get_battery_conservation() -> (bool, bool) {
    let candidates = [
        "/sys/bus/platform/drivers/ideapad_acpi/VPC2004:00/conservation_mode",
        "/sys/devices/platform/ideapad_laptop/conservation_mode",
        "/sys/bus/platform/devices/ideapad_laptop/conservation_mode",
        "/sys/class/power_supply/BAT0/charge_control_end_threshold",
        "/sys/class/power_supply/BAT1/charge_control_end_threshold",
    ];
    for c in candidates {
        let p = Path::new(c);
        if p.exists() {
            if let Ok(v) = fs::read_to_string(p) {
                let v = v.trim();
                let enabled = if c.contains("charge_control_end_threshold") {
                    v.parse::<u8>().map(|n| n > 0 && n < 100).unwrap_or(false)
                } else {
                    v == "1"
                };
                return (true, enabled);
            }
            return (true, false);
        }
    }
    (false, false)
}

// ---------------------------------------------------------------------------
// Charging control
// ---------------------------------------------------------------------------

#[derive(Debug, Serialize, Deserialize)]
pub struct ChargeModeInfo {
    pub available: bool,
    pub current: String,
    pub current_label: String,
    pub supported: Vec<Choice>,
}

fn battery_file(name: &str) -> Option<PathBuf> {
    let entries = fs::read_dir("/sys/class/power_supply").ok()?;
    for entry in entries.flatten() {
        let path = entry.path();
        let base = path
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_default();
        if !base.starts_with("BAT") {
            continue;
        }
        let candidate = path.join(name);
        if candidate.exists() {
            return Some(candidate);
        }
    }
    None
}

fn parse_charge_types(raw: &str) -> (Vec<String>, Option<String>) {
    let mut supported = Vec::new();
    let mut current = None;
    for token in raw.split_whitespace() {
        let active = token.starts_with('[') && token.ends_with(']');
        let name = token.trim_matches(|c| c == '[' || c == ']').to_string();
        if active {
            current = Some(name.clone());
        }
        if !supported.contains(&name) {
            supported.push(name);
        }
    }
    (supported, current)
}

pub fn get_charge_mode() -> ChargeModeInfo {
    if let Some(path) = battery_file("charge_types") {
        if let Ok(raw) = fs::read_to_string(&path) {
            let (supported, current) = parse_charge_types(&raw);
            if !supported.is_empty() {
                let current = current.unwrap_or_default();
                return ChargeModeInfo {
                    available: true,
                    current_label: charge_label(&current),
                    supported: supported
                        .iter()
                        .map(|m| Choice::new(m, &charge_label(m)))
                        .collect(),
                    current,
                };
            }
        }
    }
    let (available, enabled) = get_battery_conservation();
    if available {
        let current = if enabled { "Long_Life" } else { "Standard" };
        return ChargeModeInfo {
            available: true,
            current: current.into(),
            current_label: charge_label(current),
            supported: ["Standard", "Long_Life"]
                .iter()
                .map(|m| Choice::new(m, &charge_label(m)))
                .collect(),
        };
    }
    ChargeModeInfo {
        available: false,
        current: String::new(),
        current_label: String::new(),
        supported: Vec::new(),
    }
}

pub fn set_charge_mode(mode: &str) -> SimpleResult {
    let canonical = match mode.to_lowercase().replace(' ', "_").as_str() {
        "fast" | "rapid" | "rapid_charge" => "Fast",
        "standard" | "normal" => "Standard",
        "long_life" | "longlife" | "conservation" => "Long_Life",
        _ => {
            return SimpleResult {
                ok: false,
                message: format!("unsupported charge mode: {mode}"),
            }
        }
    };
    run_helper("charge-type", &[canonical])
}

// ---------------------------------------------------------------------------
// Thermal mode (Vantage quiet / balanced / performance)
// ---------------------------------------------------------------------------

#[derive(Debug, Serialize, Deserialize)]
pub struct ThermalModeInfo {
    pub available: bool,
    pub current: String,
    pub current_label: String,
    pub choices: Vec<Choice>,
}

pub fn get_thermal_mode() -> ThermalModeInfo {
    let choices: Vec<String> = fs::read_to_string("/sys/firmware/acpi/platform_profile_choices")
        .map(|raw| raw.split_whitespace().map(|s| s.to_string()).collect())
        .unwrap_or_default();
    if !choices.is_empty() {
        let current = fs::read_to_string("/sys/firmware/acpi/platform_profile")
            .map(|v| v.trim().to_string())
            .unwrap_or_default();
        return ThermalModeInfo {
            available: true,
            current_label: thermal_label(&current),
            choices: choices
                .iter()
                .map(|m| Choice::new(m, &thermal_label(m)))
                .collect(),
            current,
        };
    }
    let mut profiles = get_available_profiles().profiles;
    profiles.retain(|p| ["power-saver", "balanced", "performance"].contains(&p.as_str()));
    let current = powerprofilesctl(&["get"]).message;
    ThermalModeInfo {
        available: !profiles.is_empty(),
        current_label: thermal_label(&current),
        choices: profiles
            .iter()
            .map(|m| Choice::new(m, &thermal_label(m)))
            .collect(),
        current,
    }
}

pub fn set_thermal_mode(mode: &str) -> SimpleResult {
    let info = get_thermal_mode();
    if !info.available {
        return SimpleResult {
            ok: false,
            message: "no thermal mode interface found".into(),
        };
    }
    if !info.choices.iter().any(|c| c.key == mode) {
        return SimpleResult {
            ok: false,
            message: format!("thermal mode '{mode}' is not supported here"),
        };
    }
    let ppd = match mode {
        "low-power" | "quiet" => Some("power-saver"),
        "balanced" | "normal" => Some("balanced"),
        "performance" => Some("performance"),
        _ => None,
    };
    if let Some(profile) = ppd {
        if which("powerprofilesctl").is_some() {
            let res = powerprofilesctl(&["set", profile]);
            if res.ok {
                return res;
            }
        }
    }
    run_helper("thermal-mode", &[mode])
}

// ---------------------------------------------------------------------------
// Power limits
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PowerLimitAttr {
    pub key: String,
    pub label: String,
    pub unit: String,
    pub current: i64,
    pub min_val: i64,
    pub max_val: i64,
    pub position_pct: f64,
    pub writable: bool,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct SetLimitCmd {
    pub attr: String,
    pub value: i64,
}

pub(crate) const ATTR_MAP: &[(&str, &[&str])] = &[
    (
        "cpu_pl1",
        &[
            "ppt_pl1_spl",
            "cpu_longterm_powerlimit",
            "CPULongTermPowerLimit",
        ],
    ),
    (
        "cpu_pl2",
        &[
            "ppt_pl2_sppt",
            "cpu_shortterm_powerlimit",
            "CPUShortTermPowerLimit",
        ],
    ),
    (
        "cpu_peak",
        &["ppt_pl1_tau", "cpu_peak_powerlimit", "CPUPeakPowerLimit"],
    ),
    (
        "cpu_crossload",
        &[
            "ppt_cpu_cl",
            "cpu_cross_loading_powerlimit",
            "CPUCrossLoadingPowerLimit",
        ],
    ),
    ("cpu_temp", &["cpu_temp"]),
    ("gpu_ctgp", &["gpu_nv_ctgp", "gpu_ctgp_powerlimit", "cTGP"]),
    ("gpu_ppab", &["gpu_nv_ppab", "gpu_ppab_powerlimit", "PPAB"]),
    (
        "gpu_ac_offset",
        &["gpu_nv_ac_offset", "gpu_ac_offset_powerlimit", "ACOffset"],
    ),
    ("gpu_temp", &["gpu_temp"]),
];

pub(crate) const ATTR_LABELS: &[(&str, &str)] = &[
    ("cpu_pl1", "CPU PL1 (Sustained)"),
    ("cpu_pl2", "CPU PL2 (Burst)"),
    ("cpu_peak", "CPU PL1 Tau (Duration)"),
    ("cpu_crossload", "CPU Cross-Load"),
    ("cpu_temp", "CPU Temp Limit"),
    ("gpu_ctgp", "GPU cTGP"),
    ("gpu_ppab", "GPU Dynamic Boost (PPAB)"),
    ("gpu_ac_offset", "GPU AC Offset"),
    ("gpu_temp", "GPU Temp Limit"),
];

pub(crate) const ATTR_UNITS: &[(&str, &str)] = &[
    ("cpu_pl1", "W"),
    ("cpu_pl2", "W"),
    ("cpu_peak", "s"),
    ("cpu_crossload", "W"),
    ("cpu_temp", "°C"),
    ("gpu_ctgp", "W"),
    ("gpu_ppab", "W"),
    ("gpu_ac_offset", "W"),
    ("gpu_temp", "°C"),
];

pub(crate) fn read_sysfs_int(path: &Path) -> Option<i64> {
    fs::read_to_string(path)
        .ok()
        .and_then(|v| v.trim().parse::<i64>().ok())
}

pub(crate) fn find_attr_path(attr_name: &str) -> Option<PathBuf> {
    let base_patterns = [
        "/sys/class/firmware-attributes/*/attributes",
        "/sys/module/legion_laptop/drivers/platform:legion/legion",
        "/sys/module/legion_laptop/drivers/platform:legion/PNP0C09:00",
        "/sys/bus/platform/drivers/ideapad_acpi/VPC2004:00",
        "/sys/bus/wmi/drivers/lenovo-wmi-other/*/*",
        "/sys/bus/wmi/drivers/lenovo-wmi-gamezone/*/*",
    ];
    for pattern in base_patterns {
        if let Ok(bases) = glob_simple(pattern) {
            for base in bases {
                let cand = base.join(attr_name);
                let target = if (cand.join("current_value")).is_file() {
                    Some(cand.join("current_value"))
                } else if cand.is_file() {
                    Some(cand)
                } else {
                    None
                };
                if let Some(t) = target {
                    if read_sysfs_int(&t).is_some() {
                        return Some(t);
                    }
                }
            }
        }
    }
    None
}

fn glob_simple(pattern: &str) -> Result<Vec<PathBuf>, ()> {
    if let Some((pre, post)) = pattern.split_once('*') {
        let pre_path = Path::new(pre.trim_end_matches('/'));
        let post = post.trim_start_matches('/');
        let mut out = Vec::new();
        if let Ok(entries) = fs::read_dir(pre_path) {
            for entry in entries.flatten() {
                out.push(entry.path().join(post));
            }
        }
        Ok(out)
    } else {
        Ok(vec![PathBuf::from(pattern)])
    }
}

pub(crate) fn note_power_state(plugged: bool) -> bool {
    let mut last = LAST_PLUGGED.lock().unwrap();
    let changed = last.is_some() && *last != Some(plugged);
    *last = Some(plugged);
    changed
}

pub(crate) fn maybe_auto_preset(plugged: bool) {
    let cfg = load_config();
    if !cfg.auto_power_mode {
        return;
    }
    let name = if plugged {
        cfg.ac_preset.clone()
    } else {
        cfg.battery_preset.clone()
    };
    let mode = presets::presets()
        .into_iter()
        .find(|p| p.key == name && p.available)
        .and_then(|p| p.thermal_mode);
    let Some(mode) = mode else {
        return;
    };
    if set_thermal_mode(&mode).ok {
        let mut latest = load_config();
        latest.last_thermal_mode = mode;
        let _ = save_config(&latest);
    }
}
