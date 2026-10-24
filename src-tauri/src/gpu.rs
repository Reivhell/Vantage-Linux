use serde::{Deserialize, Serialize};
use std::fs;
use std::process::Command;

use crate::config::{home_dir, CONFIG_DIR};
use crate::types::{which, Choice};

#[derive(Debug, Serialize, Deserialize)]
pub struct GpuModeInfo {
    pub mode: Option<String>,
    pub mode_label: String,
    pub pending: Option<String>,
    pub backend: Option<String>,
    pub conflict: Option<String>,
    pub modes: Vec<Choice>,
}

pub(crate) fn gpu_modes() -> Vec<Choice> {
    vec![
        Choice::with_hint("integrated", "Integrated", "iGPU only"),
        Choice::with_hint("hybrid", "Hybrid", "Optimus"),
        Choice::with_hint("nvidia", "NVIDIA", "Dedicated"),
    ]
}

pub(crate) fn gpu_label(mode: &str) -> String {
    gpu_modes()
        .into_iter()
        .find(|m| m.key == mode)
        .map(|m| m.label)
        .unwrap_or_else(|| mode.to_string())
}

pub(crate) fn query_envycontrol_only() -> Option<String> {
    let bin = which("envycontrol")?;
    let o = Command::new(bin).arg("--query").output().ok()?;
    if !o.status.success() {
        return None;
    }
    let out = String::from_utf8_lossy(&o.stdout).to_lowercase();
    if out.contains("integrated") {
        Some("integrated".into())
    } else if out.contains("hybrid") {
        Some("hybrid".into())
    } else if out.contains("nvidia") {
        Some("nvidia".into())
    } else {
        None
    }
}

pub(crate) fn query_supergfxctl() -> Option<String> {
    let bin = which("supergfxctl")?;
    let o = Command::new(bin).arg("-g").output().ok()?;
    if !o.status.success() {
        return None;
    }
    let out = String::from_utf8_lossy(&o.stdout).to_lowercase();
    if out.contains("integrated") {
        Some("integrated".into())
    } else if out.contains("hybrid") {
        Some("hybrid".into())
    } else if out.contains("dgpu") || out.contains("nvidia") {
        Some("nvidia".into())
    } else {
        None
    }
}

pub(crate) fn query_prime_select() -> Option<String> {
    let bin = which("prime-select")?;
    let o = Command::new(bin).arg("query").output().ok()?;
    if !o.status.success() {
        return None;
    }
    let out = String::from_utf8_lossy(&o.stdout).to_lowercase();
    if out.contains("intel") {
        Some("integrated".into())
    } else if out.contains("nvidia") {
        Some("nvidia".into())
    } else if out.contains("on-demand") {
        Some("hybrid".into())
    } else {
        None
    }
}

pub(crate) fn detect_gpu_mode() -> (Option<String>, Option<String>, Option<String>) {
    let envy = query_envycontrol_only();
    let supergfx = query_supergfxctl();
    let conflict = match (&envy, &supergfx) {
        (Some(a), Some(b)) if a != b => Some(format!(
            "envycontrol reports {a}, supergfxctl reports {b}; both are installed. Uninstall one to avoid conflicts."
        )),
        _ => None,
    };
    let (mode, backend) = if let Some(m) = envy {
        (Some(m), Some("envycontrol".to_string()))
    } else if let Some(m) = supergfx {
        (Some(m), Some("supergfxctl".to_string()))
    } else if let Some(m) = query_prime_select() {
        (Some(m), Some("prime-select".to_string()))
    } else {
        (None, None)
    };
    (mode, backend, conflict)
}

pub(crate) fn pending_gpu_switch() -> Option<String> {
    let marker = home_dir().join(CONFIG_DIR).join("pending_gpu_switch");
    fs::read_to_string(&marker).ok().filter(|s| !s.is_empty())
}

#[derive(Debug, Serialize, Deserialize)]
pub struct GpuProcessInfo {
    pub pid: u32,
    pub name: String,
    pub gpu_id: String,
    pub memory_mb: Option<u32>,
}

pub(crate) fn read_gpu_processes() -> Vec<GpuProcessInfo> {
    // Coba nvidia-smi pmon terlebih dahulu (lebih akurat untuk proses GPU).
    if let Ok(o) = Command::new("nvidia-smi")
        .args(["pmon", "-s", "m", "-o", "short", "-c", "1"])
        .output()
    {
        let out = String::from_utf8_lossy(&o.stdout);
        let mut procs = Vec::new();
        for line in out.lines().skip(2) {
            // Format: # gpu pid sm  mem  enc dec  command
            let fields: Vec<&str> = line.split_whitespace().collect();
            if fields.len() >= 7 && fields[0].starts_with('#') {
                let pid: Option<u32> = fields[2].parse().ok();
                let mem: Option<u32> = fields[4].parse().ok();
                if let Some(pid) = pid {
                    procs.push(GpuProcessInfo {
                        pid,
                        name: fields[6..].join(" ").trim().to_string(),
                        gpu_id: fields[1].to_string(),
                        memory_mb: mem,
                    });
                }
            }
        }
        if !procs.is_empty() {
            return procs;
        }
    }

    // Fallback: parsing nvidia-smi query-compute-apps.
    if let Ok(o) = Command::new("nvidia-smi")
        .args([
            "--query-compute-apps=pid,process_name,used_memory",
            "--format=csv,noheader,nounits",
        ])
        .output()
    {
        let out = String::from_utf8_lossy(&o.stdout);
        let mut procs = Vec::new();
        for line in out.lines() {
            let fields: Vec<&str> = line.split(',').map(|s| s.trim()).collect();
            if fields.len() >= 3 {
                let pid: Option<u32> = fields[0].parse().ok();
                let mem: Option<u32> = fields[2].parse().ok();
                if let Some(pid) = pid {
                    procs.push(GpuProcessInfo {
                        pid,
                        name: fields[1].to_string(),
                        gpu_id: "0".to_string(),
                        memory_mb: mem,
                    });
                }
            }
        }
        return procs;
    }

    Vec::new()
}
