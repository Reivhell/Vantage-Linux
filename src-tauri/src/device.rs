use serde::{Deserialize, Serialize};
use std::fs;
use std::process::Command;

use crate::stats::nvidia_driver_present;

fn read_trim(path: &str) -> String {
    fs::read_to_string(path)
        .map(|v| v.trim().to_string())
        .unwrap_or_default()
}

fn read_u64(path: &str) -> Option<u64> {
    read_trim(path).parse().ok()
}

fn memory_total_gb() -> f64 {
    read_trim("/proc/meminfo")
        .lines()
        .find_map(|line| {
            let mut parts = line.split_whitespace();
            if parts.next() != Some("MemTotal:") {
                return None;
            }
            parts
                .next()?
                .parse::<f64>()
                .ok()
                .map(|kb| kb * 1024.0 / 1e9)
        })
        .unwrap_or(0.0)
}

fn uptime_seconds() -> u64 {
    read_trim("/proc/uptime")
        .split_whitespace()
        .next()
        .and_then(|v| v.parse::<f64>().ok())
        .unwrap_or(0.0) as u64
}

fn battery_values() -> (Option<f64>, Option<f64>, Option<u32>, Option<f64>) {
    let entries = match fs::read_dir("/sys/class/power_supply") {
        Ok(e) => e,
        Err(_) => return (None, None, None, None),
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if !entry.file_name().to_string_lossy().starts_with("BAT") {
            continue;
        }
        let p = |name: &str| read_u64(&format!("{}/{name}", path.display()));
        let full = p("energy_full")
            .or_else(|| p("charge_full"))
            .map(|v| v as f64);
        let design = p("energy_full_design")
            .or_else(|| p("charge_full_design"))
            .map(|v| v as f64);
        let cycles = p("cycle_count").map(|v| v as u32);
        let temp = p("temp").map(|v| v as f64 / 10.0);
        return (full, design, cycles, temp);
    }
    (None, None, None, None)
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeviceInfo {
    pub vendor: String,
    pub product: String,
    pub product_family: String,
    pub version: String,
    pub board: String,
    pub bios_vendor: String,
    pub bios_version: String,
    pub bios_date: String,
    pub serial: String,
    pub hostname: String,
    pub kernel: String,
    pub os_name: String,
    pub os_version: String,
    pub cpu_model: String,
    pub cpu_cores: usize,
    pub cpu_threads: usize,
    pub gpu_name: String,
    pub gpu_driver: String,
    pub storage: String,
    pub memory_total_gb: f64,
    pub uptime_seconds: u64,
    pub desktop: String,
    pub session_type: String,
    pub battery_health_pct: Option<f64>,
    pub battery_cycles: Option<u32>,
    pub battery_temp_c: Option<f64>,
    pub battery_health_label: String,
    pub battery_temp_label: String,
    pub health_score: u8,
    pub health_level: String,
    pub health_issues: Vec<String>,
    pub uptime_label: String,
    pub bios_label: String,
    pub memory_label: String,
    pub session_label: String,
    pub helper_label: String,
    pub helper_version: Option<String>,
    pub helper_outdated: bool,
}

fn os_release() -> (String, String) {
    let text = read_trim("/etc/os-release");
    let value = |key: &str| {
        text.lines()
            .find_map(|line| line.strip_prefix(&format!("{key}=")))
            .map(|v| v.trim_matches('"').to_string())
            .unwrap_or_default()
    };
    let name = value("PRETTY_NAME");
    let version = value("VERSION_ID");
    (
        if name.is_empty() {
            "Linux".into()
        } else {
            name
        },
        version,
    )
}

fn cpu_counts() -> (usize, usize) {
    let text = read_trim("/proc/cpuinfo");
    let threads = text
        .lines()
        .filter(|line| line.starts_with("processor"))
        .count();
    let cores = text
        .lines()
        .find_map(|line| line.strip_prefix("cpu cores"))
        .and_then(|v| v.split(':').nth(1))
        .and_then(|v| v.trim().parse::<usize>().ok())
        .unwrap_or(threads.max(1));
    (cores, threads.max(1))
}

fn nvidia_info() -> (String, String) {
    if !nvidia_driver_present() {
        return (String::new(), String::new());
    }
    let Ok(out) = Command::new("nvidia-smi")
        .args(["--query-gpu=name,driver_version", "--format=csv,noheader"])
        .output()
    else {
        return (String::new(), String::new());
    };
    if !out.status.success() {
        return (String::new(), String::new());
    }
    let text = String::from_utf8_lossy(&out.stdout);
    let line = text.lines().next().unwrap_or("");
    let mut parts = line.split(',').map(|v| v.trim());
    (
        parts.next().unwrap_or("").to_string(),
        parts.next().unwrap_or("").to_string(),
    )
}

fn lspci_gpu_info() -> (String, String) {
    let Ok(out) = Command::new("lspci").arg("-nnk").output() else {
        return (String::new(), String::new());
    };
    let text = String::from_utf8_lossy(&out.stdout);
    let mut gpus: Vec<(String, String)> = Vec::new();
    let mut current_gpu = false;
    for line in text.lines() {
        if !line.starts_with(' ') && !line.starts_with('\t') {
            current_gpu = line.contains("VGA compatible controller")
                || line.contains("3D controller")
                || line.contains("Display controller");
            if current_gpu {
                let mut name = line
                    .split_once(": ")
                    .map(|(_, rest)| rest)
                    .unwrap_or(line)
                    .trim()
                    .to_string();
                if let Some(pos) = name.rfind(" (rev ") {
                    name.truncate(pos);
                }
                if let Some(pos) = name.rfind(" [") {
                    let suffix = &name[pos + 1..];
                    if suffix.ends_with(']') && suffix.contains(':') {
                        name.truncate(pos);
                    }
                }
                gpus.push((name, String::new()));
            }
        } else if current_gpu {
            if let Some(driver) = line.trim().strip_prefix("Kernel driver in use:") {
                if let Some((_, value)) = gpus.last_mut() {
                    *value = driver.trim().to_string();
                }
            }
        }
    }
    let mut fallback = None;
    for gpu in gpus {
        if gpu.0.to_lowercase().contains("nvidia") {
            return gpu;
        }
        if fallback.is_none() {
            fallback = Some(gpu);
        }
    }
    fallback.unwrap_or_default()
}

fn gpu_info() -> (String, String) {
    let (name, driver) = nvidia_info();
    if name.is_empty() {
        return lspci_gpu_info();
    }
    if driver.is_empty() {
        return (name, lspci_gpu_info().1);
    }
    (name, driver)
}

fn storage_info() -> String {
    let Ok(out) = Command::new("lsblk")
        .args(["-d", "-n", "-o", "NAME,MODEL,SIZE"])
        .output()
    else {
        return String::new();
    };
    let text = String::from_utf8_lossy(&out.stdout);
    text.lines()
        .find(|line| {
            line.split_whitespace()
                .next()
                .is_some_and(|n| n.starts_with("nvme"))
        })
        .map(|line| line.trim().to_string())
        .unwrap_or_default()
}

pub fn read_device_info(helper_version: Option<String>, helper_outdated: bool) -> DeviceInfo {
    let (full, design, cycles, temp) = battery_values();
    let memory_total = memory_total_gb();
    let battery_health = match (full, design) {
        (Some(full), Some(design)) if design > 0.0 => Some(full / design * 100.0),
        _ => None,
    };
    let cpu_model = fs::read_to_string("/proc/cpuinfo")
        .ok()
        .and_then(|text| {
            text.lines()
                .find(|l| l.starts_with("model name"))
                .and_then(|l| l.split(':').nth(1))
                .map(|v| v.trim().to_string())
        })
        .unwrap_or_default();

    let mut issues = Vec::new();
    let mut score: i32 = 100;
    if let Some(h) = battery_health {
        if h < 60.0 {
            score -= 35;
            issues.push(format!("Battery health low ({h:.0}%)"));
        } else if h < 80.0 {
            score -= 20;
            issues.push(format!("Battery health degraded ({h:.0}%)"));
        }
    }
    if helper_outdated {
        score -= 10;
        issues.push("Privileged helper is outdated; reinstall".into());
    }
    if memory_total < 8.0 {
        score -= 5;
        issues.push("Less than 8 GB RAM detected".into());
    }
    if temp.is_some_and(|t| t >= 45.0) {
        score -= 5;
        issues.push(format!(
            "Battery temperature elevated ({:.0} °C)",
            temp.unwrap_or(0.0)
        ));
    }

    let score = score.clamp(0, 100) as u8;
    let memory_total = (memory_total * 10.0).round() / 10.0;
    let uptime = uptime_seconds();
    let desktop = std::env::var("XDG_CURRENT_DESKTOP").unwrap_or_else(|_| "unknown".into());
    let session_type = std::env::var("XDG_SESSION_TYPE").unwrap_or_else(|_| "unknown".into());
    let bios_version = read_trim("/sys/class/dmi/id/bios_version");
    let bios_date = read_trim("/sys/class/dmi/id/bios_date");
    let (os_name, os_version) = os_release();
    let (cpu_cores, cpu_threads) = cpu_counts();
    let (gpu_name, gpu_driver) = gpu_info();
    let storage = storage_info();
    let health_level = if score >= 85 {
        "good"
    } else if score >= 65 {
        "warn"
    } else {
        "bad"
    };

    DeviceInfo {
        vendor: read_trim("/sys/class/dmi/id/sys_vendor"),
        product: read_trim("/sys/class/dmi/id/product_name"),
        product_family: read_trim("/sys/class/dmi/id/product_family"),
        version: read_trim("/sys/class/dmi/id/product_version"),
        board: read_trim("/sys/class/dmi/id/board_name"),
        bios_vendor: read_trim("/sys/class/dmi/id/bios_vendor"),
        bios_label: if bios_version.is_empty() {
            "--".into()
        } else {
            format!("{bios_version} · {bios_date}")
        },
        bios_version,
        bios_date,
        serial: read_trim("/sys/class/dmi/id/product_serial"),
        hostname: read_trim("/proc/sys/kernel/hostname"),
        kernel: read_trim("/proc/sys/kernel/osrelease"),
        os_name,
        os_version,
        cpu_model,
        cpu_cores,
        cpu_threads,
        gpu_name,
        gpu_driver,
        storage,
        memory_total_gb: memory_total,
        memory_label: format!("{memory_total:.1} GB"),
        uptime_seconds: uptime,
        uptime_label: uptime_label(uptime),
        desktop: desktop.clone(),
        session_type: session_type.clone(),
        session_label: format!("{desktop} · {session_type}"),
        battery_health_pct: battery_health.map(|v| (v * 10.0).round() / 10.0),
        battery_health_label: battery_health
            .map(|value| format!("{value:.1} %"))
            .unwrap_or_else(|| "--".into()),
        battery_cycles: cycles,
        battery_temp_c: temp,
        battery_temp_label: temp
            .map(|value| format!("{value:.1} °C"))
            .unwrap_or_else(|| "--".into()),
        health_score: score,
        health_level: health_level.into(),
        health_issues: issues,
        helper_label: if helper_outdated {
            "outdated".into()
        } else {
            helper_version
                .clone()
                .unwrap_or_else(|| "not installed".into())
        },
        helper_version,
        helper_outdated,
    }
}

fn uptime_label(seconds: u64) -> String {
    let days = seconds / 86400;
    let hours = (seconds % 86400) / 3600;
    let minutes = (seconds % 3600) / 60;
    if days > 0 {
        format!("{days}d {hours}h")
    } else if hours > 0 {
        format!("{hours}h {minutes}m")
    } else {
        format!("{minutes}m")
    }
}
