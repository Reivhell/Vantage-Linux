use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;
use std::process::Command;

use crate::stats::nvidia_driver_present;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GpuDetails {
    pub nvidia_available: bool,
    pub name: String,
    pub driver_version: String,
    pub pstate: String,
    pub power_w: Option<f64>,
    pub power_limit_w: Option<f64>,
    pub power_min_w: Option<f64>,
    pub power_max_w: Option<f64>,
    pub temperature_c: Option<u32>,
    pub clock_core_mhz: Option<u32>,
    pub clock_memory_mhz: Option<u32>,
    pub runtime_status: String,
    pub runtime_control: String,
    pub prime_offload: String,
}

fn nvidia_pci() -> Option<PathBuf> {
    for entry in fs::read_dir("/sys/bus/pci/devices").ok()?.flatten() {
        let path = entry.path();
        let vendor = fs::read_to_string(path.join("vendor")).unwrap_or_default();
        let class = fs::read_to_string(path.join("class")).unwrap_or_default();
        if vendor.trim() == "0x10de" && class.trim().starts_with("0x030") {
            return Some(path);
        }
    }
    None
}

pub fn read_gpu_details() -> GpuDetails {
    let mut details = GpuDetails {
        nvidia_available: false,
        name: String::new(),
        driver_version: String::new(),
        pstate: String::new(),
        power_w: None,
        power_limit_w: None,
        power_min_w: None,
        power_max_w: None,
        temperature_c: None,
        clock_core_mhz: None,
        clock_memory_mhz: None,
        runtime_status: String::new(),
        runtime_control: String::new(),
        prime_offload: "__NV_PRIME_RENDER_OFFLOAD=1 __GLX_VENDOR_LIBRARY_NAME=nvidia <command>"
            .into(),
    };

    if let Some(pci) = nvidia_pci() {
        details.runtime_status = fs::read_to_string(pci.join("power/runtime_status"))
            .map(|v| v.trim().to_string())
            .unwrap_or_default();
        details.runtime_control = fs::read_to_string(pci.join("power/control"))
            .map(|v| v.trim().to_string())
            .unwrap_or_default();
    }

    if !nvidia_driver_present() {
        return details;
    }

    let output = Command::new("nvidia-smi")
        .args([
            "--query-gpu=name,driver_version,pstate,power.draw,power.limit,power.min_limit,power.max_limit,temperature.gpu,clocks.gr,clocks.mem",
            "--format=csv,noheader,nounits",
        ])
        .output();
    let Ok(output) = output else {
        return details;
    };
    if !output.status.success() {
        return details;
    }
    let text = String::from_utf8_lossy(&output.stdout);
    let Some(line) = text.lines().find(|l| !l.trim().is_empty()) else {
        return details;
    };
    let fields: Vec<&str> = line.split(',').map(|v| v.trim()).collect();
    let num = |idx: usize| -> Option<f64> {
        fields
            .get(idx)
            .filter(|v| !v.is_empty() && **v != "[N/A]")
            .and_then(|v| v.parse().ok())
    };
    details.nvidia_available = true;
    details.name = fields.first().copied().unwrap_or("").to_string();
    details.driver_version = fields.get(1).copied().unwrap_or("").to_string();
    details.pstate = fields.get(2).copied().unwrap_or("").to_string();
    details.power_w = num(3);
    details.power_limit_w = num(4);
    details.power_min_w = num(5);
    details.power_max_w = num(6);
    details.temperature_c = num(7).map(|v| v as u32);
    details.clock_core_mhz = num(8).map(|v| v as u32);
    details.clock_memory_mhz = num(9).map(|v| v as u32);
    details
}

pub fn set_power_limit(watts: u32) -> crate::SimpleResult {
    let details = read_gpu_details();
    if !details.nvidia_available {
        return crate::SimpleResult {
            ok: false,
            message: "NVIDIA GPU is not available".into(),
        };
    }
    let min = details.power_min_w.unwrap_or(0.0).ceil() as u32;
    let max = details.power_max_w.unwrap_or(0.0).floor() as u32;
    if watts < min || (max > 0 && watts > max) {
        return crate::SimpleResult {
            ok: false,
            message: format!("power limit must be between {min} and {max} W"),
        };
    }
    crate::run_helper("gpu-power-limit", &[&watts.to_string()])
}

pub fn set_runtime_pm(control: &str) -> crate::SimpleResult {
    if control != "auto" && control != "on" {
        return crate::SimpleResult {
            ok: false,
            message: "runtime PM must be auto or on".into(),
        };
    }
    crate::run_helper("gpu-runtime-pm", &[control])
}
