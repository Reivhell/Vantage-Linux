use serde::{Deserialize, Serialize};
use std::fs;
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

use crate::device::DeviceInfo;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Check {
    pub id: String,
    pub label: String,
    pub status: String,
    pub summary: String,
    pub detail: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiagnosticsReport {
    pub generated_at: f64,
    pub score: u8,
    pub checks: Vec<Check>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Finding {
    pub severity: String,
    pub title: String,
    pub detail: String,
    pub action: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SmartScan {
    pub score: u8,
    pub findings: Vec<Finding>,
}

fn command_text(program: &str, args: &[&str]) -> Option<String> {
    let bin = crate::which(program)?;
    let out = Command::new(bin).args(args).output().ok()?;
    Some(format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    ))
}

fn root_device() -> Option<String> {
    let source = command_text("findmnt", &["-n", "-o", "SOURCE", "/"])?;
    let source = source.trim().lines().next()?.to_string();
    if !source.starts_with("/dev/") || source.contains("mapper") || source.contains("md") {
        return None;
    }
    let base = source.split('[').next()?.to_string();
    if base.contains("nvme") {
        let idx = base.rfind('p')?;
        if base[idx + 1..].chars().all(|c| c.is_ascii_digit()) {
            return Some(base[..idx].to_string());
        }
    }
    let trimmed = base.trim_end_matches(|c: char| c.is_ascii_digit());
    Some(trimmed.to_string())
}

fn smart_check() -> Check {
    let Some(device) = root_device() else {
        return Check {
            id: "smart".into(),
            label: "Storage health".into(),
            status: "unknown".into(),
            summary: "Root disk is not a plain block device".into(),
            detail: String::new(),
        };
    };
    let result = crate::run_helper_quiet("smart-health", &[&device]);
    if !result.ok {
        return Check {
            id: "smart".into(),
            label: "Storage health".into(),
            status: "unknown".into(),
            summary: "SMART unavailable (helper, smartctl, or permission)".into(),
            detail: format!("{device}: {}", result.message),
        };
    }
    let text = result.message.to_lowercase();
    let (status, summary) = if text.contains("failed") || text.contains("failing") {
        ("fail", "SMART reports a failing disk")
    } else if text.contains("passed") || text.contains("ok") {
        ("ok", "SMART health passed")
    } else {
        ("unknown", "SMART result could not be classified")
    };
    Check {
        id: "smart".into(),
        label: "Storage health".into(),
        status: status.into(),
        summary: summary.into(),
        detail: format!("{device}\n{}", result.message.trim()),
    }
}

fn failed_units() -> Check {
    let Some(text) = command_text("systemctl", &["--failed", "--no-legend", "--no-pager"]) else {
        return Check {
            id: "services".into(),
            label: "System services".into(),
            status: "unknown".into(),
            summary: "systemctl unavailable".into(),
            detail: String::new(),
        };
    };
    let lines: Vec<&str> = text.lines().filter(|l| !l.trim().is_empty()).collect();
    Check {
        id: "services".into(),
        label: "System services".into(),
        status: if lines.is_empty() { "ok" } else { "warn" }.into(),
        summary: if lines.is_empty() {
            "No failed system services".into()
        } else {
            format!("{} failed service(s)", lines.len())
        },
        detail: lines.join("\n"),
    }
}

fn journal_check() -> Check {
    let Some(text) = command_text("journalctl", &["-p", "3", "-b", "--no-pager", "-q"]) else {
        return Check {
            id: "journal".into(),
            label: "Boot errors".into(),
            status: "unknown".into(),
            summary: "journalctl unavailable".into(),
            detail: String::new(),
        };
    };
    let count = text.lines().filter(|l| !l.trim().is_empty()).count();
    Check {
        id: "journal".into(),
        label: "Boot errors".into(),
        status: if count == 0 { "ok" } else { "warn" }.into(),
        summary: if count == 0 {
            "No error-level journal entries this boot".into()
        } else {
            format!("{count} error-level entry/entries this boot")
        },
        detail: text.lines().take(20).collect::<Vec<_>>().join("\n"),
    }
}

fn thermal_check() -> Check {
    let Some(text) = command_text("journalctl", &["-k", "-b", "--no-pager", "-q"]) else {
        return Check {
            id: "thermal".into(),
            label: "Thermal throttling".into(),
            status: "unknown".into(),
            summary: "Kernel log unavailable".into(),
            detail: String::new(),
        };
    };
    let hits: Vec<&str> = text
        .lines()
        .filter(|l| {
            let l = l.to_lowercase();
            l.contains("thermal throttl")
                || l.contains("package temperature above threshold")
                || l.contains("cpu clock throttled")
        })
        .collect();
    Check {
        id: "thermal".into(),
        label: "Thermal throttling".into(),
        status: if hits.is_empty() { "ok" } else { "warn" }.into(),
        summary: if hits.is_empty() {
            "No throttling events this boot".into()
        } else {
            format!("{} throttling event(s) this boot", hits.len())
        },
        detail: hits.into_iter().take(10).collect::<Vec<_>>().join("\n"),
    }
}

fn battery_check() -> Check {
    let battery = crate::battery::read_battery_detail();
    if !battery.available {
        return Check {
            id: "battery".into(),
            label: "Battery".into(),
            status: "unknown".into(),
            summary: "No battery detected".into(),
            detail: String::new(),
        };
    }
    let health = battery.health_pct.unwrap_or(100.0);
    let (status, summary) = if health < 60.0 {
        ("fail", format!("Battery health {health:.0}%"))
    } else if health < 80.0 {
        ("warn", format!("Battery health {health:.0}%"))
    } else {
        ("ok", format!("Battery health {health:.0}%"))
    };
    Check {
        id: "battery".into(),
        label: "Battery".into(),
        status: status.into(),
        summary,
        detail: format!(
            "cycles: {} · temp: {} °C · status: {}",
            battery
                .cycles
                .map(|v| v.to_string())
                .unwrap_or_else(|| "?".into()),
            battery
                .temp_c
                .map(|v| format!("{v:.1}"))
                .unwrap_or_else(|| "?".into()),
            battery.status
        ),
    }
}

fn disk_space_check() -> Check {
    let disks = sysinfo::Disks::new_with_refreshed_list();
    let mut worst: Option<(String, f64)> = None;
    for disk in &disks {
        let total = disk.total_space();
        if total == 0 {
            continue;
        }
        let used_pct = (total - disk.available_space()) as f64 / total as f64 * 100.0;
        if worst.as_ref().map(|(_, p)| used_pct > *p).unwrap_or(true) {
            worst = Some((disk.mount_point().display().to_string(), used_pct));
        }
    }
    match worst {
        Some((mount, pct)) if pct >= 90.0 => Check {
            id: "disk".into(),
            label: "Disk space".into(),
            status: "warn".into(),
            summary: format!("{mount} is {pct:.0}% full"),
            detail: String::new(),
        },
        Some((mount, pct)) => Check {
            id: "disk".into(),
            label: "Disk space".into(),
            status: "ok".into(),
            summary: format!("{mount} is {pct:.0}% full"),
            detail: String::new(),
        },
        None => Check {
            id: "disk".into(),
            label: "Disk space".into(),
            status: "unknown".into(),
            summary: "No disk data".into(),
            detail: String::new(),
        },
    }
}

pub fn run_diagnostics() -> DiagnosticsReport {
    let mut checks = vec![
        battery_check(),
        smart_check(),
        failed_units(),
        journal_check(),
        thermal_check(),
        disk_space_check(),
    ];
    if let Some(temp) = crate::read_cpu_temp() {
        checks.push(Check {
            id: "cpu_temp".into(),
            label: "CPU temperature".into(),
            status: if temp >= 95.0 { "warn" } else { "ok" }.into(),
            summary: format!("{temp:.0} °C"),
            detail: String::new(),
        });
    }
    let score = if checks.is_empty() {
        0
    } else {
        let points: i32 = checks
            .iter()
            .map(|c| match c.status.as_str() {
                "ok" => 100,
                "warn" => 55,
                "fail" => 0,
                _ => 40,
            })
            .sum();
        (points / checks.len() as i32) as u8
    };
    DiagnosticsReport {
        generated_at: SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs_f64(),
        score,
        checks,
    }
}

pub fn smart_scan() -> SmartScan {
    let mut findings = Vec::new();

    if let Some(text) = command_text("systemctl", &["--failed", "--no-legend", "--no-pager"]) {
        let failed: Vec<&str> = text.lines().filter(|l| !l.trim().is_empty()).collect();
        if !failed.is_empty() {
            findings.push(Finding {
                severity: "warn".into(),
                title: format!("{} failed system service(s)", failed.len()),
                detail: failed
                    .iter()
                    .take(8)
                    .copied()
                    .collect::<Vec<_>>()
                    .join("\n"),
                action: "Inspect with: systemctl --failed".into(),
            });
        }
    }

    if let Some(text) = command_text("pacman", &["-Qdtq"]) {
        let orphans: Vec<&str> = text.lines().filter(|l| !l.trim().is_empty()).collect();
        if !orphans.is_empty() {
            findings.push(Finding {
                severity: "info".into(),
                title: format!("{} orphan package(s)", orphans.len()),
                detail: orphans
                    .iter()
                    .take(12)
                    .copied()
                    .collect::<Vec<_>>()
                    .join(", "),
                action: "Review with: pacman -Qdt".into(),
            });
        }
    }

    let autostart = crate::home_dir().join(".config/autostart");
    if let Ok(entries) = fs::read_dir(autostart) {
        let count = entries
            .flatten()
            .filter(|e| e.path().extension().and_then(|v| v.to_str()) == Some("desktop"))
            .count();
        if count > 8 {
            findings.push(Finding {
                severity: "info".into(),
                title: format!("{count} desktop autostart entries"),
                detail: "Many login applications can slow down boot and use background RAM.".into(),
                action: "Review ~/.config/autostart".into(),
            });
        }
    }

    if let Some(text) = command_text("journalctl", &["-p", "3", "-b", "--no-pager", "-q"]) {
        let count = text.lines().filter(|l| !l.trim().is_empty()).count();
        if count > 0 {
            findings.push(Finding {
                severity: "info".into(),
                title: format!("{count} boot error(s) in the journal"),
                detail: text.lines().take(6).collect::<Vec<_>>().join("\n"),
                action: "Inspect with: journalctl -p 3 -b".into(),
            });
        }
    }

    let processes = crate::collect_processes(None, 3);
    if let Some(top) = processes.first() {
        if top.memory_mb > 4096.0 {
            findings.push(Finding {
                severity: "warn".into(),
                title: format!("{} uses {:.1} GB RAM", top.name, top.memory_mb / 1024.0),
                detail: format!("PID {}", top.pid),
                action: "Inspect the process before terminating it.".into(),
            });
        }
    }

    let score = if findings.is_empty() {
        100
    } else {
        100u8.saturating_sub((findings.len() as u8) * 10)
    };
    SmartScan { score, findings }
}

pub fn export_report(device: &DeviceInfo, report: &DiagnosticsReport) -> Result<String, String> {
    let dir = crate::home_dir().join(".local/state/loq-control");
    fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    let path = dir.join(format!("diagnostics-{stamp}.md"));
    let mut text = String::new();
    text.push_str("# LOQ Control Center diagnostics\n\n");
    text.push_str(&format!("Generated: {stamp}\n\n"));
    text.push_str("## Device\n\n");
    text.push_str(&format!(
        "- Model: {} {}\n- BIOS: {} ({})\n- Kernel: {}\n- Desktop: {} / {}\n- Score: {}/100\n\n",
        device.vendor,
        device.product,
        device.bios_version,
        device.bios_date,
        device.kernel,
        device.desktop,
        device.session_type,
        device.health_score
    ));
    text.push_str("## Checks\n\n");
    for check in &report.checks {
        text.push_str(&format!(
            "### {} — {}\n\n{}\n\n",
            check.label, check.status, check.summary
        ));
        if !check.detail.is_empty() {
            text.push_str("```\n");
            text.push_str(&check.detail);
            text.push_str("\n```\n\n");
        }
    }
    fs::write(&path, text).map_err(|e| e.to_string())?;
    Ok(path.display().to_string())
}
