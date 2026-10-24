use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use crate::config::HELPER_PATH;
use crate::types::which;

const ITE_VID: u32 = 0x048d;
const ITE_PIDS: [u32; 7] = [0xc993, 0xc994, 0xc995, 0xc996, 0xc997, 0x6004, 0x6006];

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Capabilities {
    pub fan_module: Option<String>,
    pub fan_hwmon_path: Option<String>,
    pub fan_rpm_readable: bool,
    pub fan_curve_writable: bool,
    pub power_profiles_available: bool,
    pub power_profiles_profiles: Vec<String>,
    pub battery_conservation_available: bool,
    pub battery_conservation_enabled: bool,
    pub power_limits_available: bool,
    pub power_limit_attrs: HashMap<String, String>,
    pub nvidia_available: bool,
    pub nvidia_device_name: Option<String>,
    pub intel_gpu_available: bool,
    pub gpu_switcher: Option<String>,
    pub keyboard_rgb_available: bool,
    pub keyboard_hid_path: Option<String>,
    pub keyboard_vid: u32,
    pub keyboard_pid: u32,
    pub kernel_version: String,
    pub cpu_model: String,
    pub helper_installed: bool,
    pub helper_version: Option<String>,
    pub helper_outdated: bool,
}

pub fn discover() -> Capabilities {
    let mut caps = Capabilities {
        fan_module: None,
        fan_hwmon_path: None,
        fan_rpm_readable: false,
        fan_curve_writable: false,
        power_profiles_available: false,
        power_profiles_profiles: Vec::new(),
        battery_conservation_available: false,
        battery_conservation_enabled: false,
        power_limits_available: false,
        power_limit_attrs: HashMap::new(),
        nvidia_available: false,
        nvidia_device_name: None,
        intel_gpu_available: false,
        gpu_switcher: None,
        keyboard_rgb_available: false,
        keyboard_hid_path: None,
        keyboard_vid: ITE_VID,
        keyboard_pid: 0,
        kernel_version: String::new(),
        cpu_model: String::new(),
        helper_installed: Path::new(HELPER_PATH).is_file(),
        helper_version: None,
        helper_outdated: false,
    };

    probe_system(&mut caps);
    probe_helper(&mut caps);
    probe_fan(&mut caps);
    probe_power_profiles(&mut caps);
    probe_battery_conservation(&mut caps);
    probe_nvidia(&mut caps);
    probe_intel_gpu(&mut caps);
    probe_gpu_switcher(&mut caps);
    probe_keyboard(&mut caps);

    caps
}

fn probe_helper(caps: &mut Capabilities) {
    if !caps.helper_installed {
        return;
    }
    let version = crate::run_helper_quiet("version", &[]);
    if version.ok && !version.message.is_empty() {
        caps.helper_outdated = version.message.trim() != env!("CARGO_PKG_VERSION");
        caps.helper_version = Some(version.message.trim().to_string());
    } else {
        caps.helper_outdated = true;
    }
}

fn probe_system(caps: &mut Capabilities) {
    if let Ok(rel) = fs::read_to_string("/proc/sys/kernel/osrelease") {
        caps.kernel_version = rel.trim().to_string();
    }
    if let Ok(text) = fs::read_to_string("/proc/cpuinfo") {
        for line in text.lines() {
            if let Some(rest) = line.strip_prefix("model name") {
                caps.cpu_model = rest
                    .split(':')
                    .nth(1)
                    .unwrap_or("Unknown")
                    .trim()
                    .to_string();
                break;
            }
        }
    }
    if caps.cpu_model.is_empty() {
        caps.cpu_model = "Unknown".into();
    }
}

fn probe_fan(caps: &mut Capabilities) {
    if let Ok(output) = Command::new("lsmod").output() {
        let loaded = String::from_utf8_lossy(&output.stdout);
        for name in [
            "legion_laptop",
            "lenovo_wmi_fan",
            "lenovo_wmi_other",
            "ideapad_laptop",
        ] {
            if loaded.contains(name) {
                caps.fan_module = Some(name.to_string());
                break;
            }
        }
    }
    if let Ok(entries) = fs::read_dir("/sys/class/hwmon") {
        for entry in entries.flatten() {
            let path = entry.path();
            if let Ok(name) = fs::read_to_string(path.join("name")) {
                let name = name.trim();
                if matches!(
                    name,
                    "legion_laptop" | "lenovo_wmi_fan" | "lenovo_wmi_other" | "ideapad"
                ) {
                    caps.fan_hwmon_path = Some(path.to_string_lossy().to_string());
                    if fs::read_to_string(path.join("fan1_input"))
                        .map(|v| v.trim().parse::<i32>().is_ok())
                        .unwrap_or(false)
                    {
                        caps.fan_rpm_readable = true;
                    }
                    if fs::read_to_string(path.join("pwm1_auto_point1_pwm")).is_ok() {
                        caps.fan_curve_writable = true;
                    }
                    break;
                }
            }
        }
    }
}

fn probe_power_profiles(caps: &mut Capabilities) {
    if let Ok(raw) = fs::read_to_string("/sys/firmware/acpi/platform_profile_choices") {
        let mut mapped = Vec::new();
        for c in raw.split_whitespace() {
            let m = match c {
                "low-power" | "quiet" | "power-saver" => Some("power-saver"),
                "balanced" | "normal" => Some("balanced"),
                "performance" | "max-power" | "high-performance" => Some("performance"),
                _ => None,
            };
            if let Some(m) = m {
                if !mapped.contains(&m.to_string()) {
                    mapped.push(m.to_string());
                }
            }
        }
        if !mapped.is_empty() {
            caps.power_profiles_available = true;
            caps.power_profiles_profiles = mapped;
            return;
        }
    }
    if Command::new("powerprofilesctl")
        .arg("list")
        .output()
        .map_or(false, |o| o.status.success())
    {
        caps.power_profiles_available = true;
        caps.power_profiles_profiles = vec![
            "power-saver".into(),
            "balanced".into(),
            "performance".into(),
        ];
    }
}

fn probe_battery_conservation(caps: &mut Capabilities) {
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
            caps.battery_conservation_available = true;
            if let Ok(v) = fs::read_to_string(p) {
                let v = v.trim();
                caps.battery_conservation_enabled = if c.contains("charge_control_end_threshold") {
                    v == "80"
                } else {
                    v == "1"
                };
            }
            break;
        }
    }
}

fn probe_nvidia(caps: &mut Capabilities) {
    if !Command::new("nvidia-smi")
        .output()
        .map_or(false, |o| o.status.success())
    {
        return;
    }
    caps.nvidia_available = true;
    let name = Command::new("nvidia-smi")
        .args(["--query-gpu=name", "--format=csv,noheader"])
        .output()
        .ok()
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .filter(|n| !n.is_empty());
    caps.nvidia_device_name = name.or_else(|| Some("NVIDIA GPU".into()));
}

fn probe_intel_gpu(caps: &mut Capabilities) {
    if Command::new("intel_gpu_top")
        .arg("--version")
        .output()
        .map_or(false, |o| o.status.success())
    {
        caps.intel_gpu_available = true;
        return;
    }
    if let Ok(entries) = fs::read_dir("/sys/class/drm") {
        for entry in entries.flatten() {
            let p = entry.path();
            for act in [
                p.join("gt_act_freq_mhz"),
                p.join("gt/gt0/rps_act_freq_mhz"),
                p.join("gt_cur_freq_mhz"),
            ] {
                if act.exists() {
                    caps.intel_gpu_available = true;
                    return;
                }
            }
        }
    }
}

fn probe_gpu_switcher(caps: &mut Capabilities) {
    if which("envycontrol").is_some() {
        caps.gpu_switcher = Some("envycontrol".into());
    } else if which("supergfxctl").is_some() {
        caps.gpu_switcher = Some("supergfxctl".into());
    } else if which("prime-select").is_some() {
        caps.gpu_switcher = Some("prime-select".into());
    }
}

pub(crate) fn find_ite_hidraw() -> Option<(PathBuf, u32, u32)> {
    let entries = fs::read_dir("/sys/class/hidraw").ok()?;
    for entry in entries.flatten() {
        let dev = entry.path().join("device");
        let Ok(vid) = fs::read_to_string(dev.join("idVendor")) else {
            continue;
        };
        let Ok(pid) = fs::read_to_string(dev.join("idProduct")) else {
            continue;
        };
        let Ok(vid) = u32::from_str_radix(vid.trim(), 16) else {
            continue;
        };
        let Ok(pid) = u32::from_str_radix(pid.trim(), 16) else {
            continue;
        };
        if vid == ITE_VID && ITE_PIDS.contains(&pid) {
            let path = PathBuf::from(format!("/dev/{}", entry.file_name().to_string_lossy()));
            return Some((path, vid, pid));
        }
    }
    None
}

fn probe_keyboard(caps: &mut Capabilities) {
    if let Some((path, vid, pid)) = find_ite_hidraw() {
        caps.keyboard_rgb_available = true;
        caps.keyboard_vid = vid;
        caps.keyboard_pid = pid;
        caps.keyboard_hid_path = Some(path.to_string_lossy().to_string());
    }
}
