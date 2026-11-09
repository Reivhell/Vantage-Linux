use serde::Deserialize;
use std::fs;
use std::path::Path;
use std::process::Command;
use std::sync::atomic::Ordering;
use std::sync::{Mutex, OnceLock};
use std::time::SystemTime;
use sysinfo::{ProcessesToUpdate, System};

use crate::battery;
use crate::capabilities::{discover, Capabilities};
use crate::config::{home_dir, save_config, AppConfig, CONFIG_DIR};
use crate::db;
use crate::device;
use crate::diagnostics;
use crate::display;
use crate::error::{AppError, ErrorEnvelope};
use crate::gpu::{
    detect_gpu_mode, gpu_label, gpu_modes, pending_gpu_switch, read_gpu_processes, GpuModeInfo,
    GpuProcessInfo,
};
use crate::gpu_ext;
use crate::history;
use crate::memory::{read_dimms, MemoryInfo};
use crate::power::{
    find_attr_path, get_charge_mode, get_thermal_mode, read_sysfs_int, run_helper, set_charge_mode,
    set_thermal_mode, ChargeModeInfo, PowerLimitAttr, SetLimitCmd, ThermalModeInfo, ATTR_MAP,
};
use crate::presets;
use crate::profiles;
use crate::resource_governor::ResourceGovernor;
use crate::resource_types::{validate_rule, AutomationEvent, ResourceStatus, Rule};
use crate::stats::{collect_stats, read_nvidia, SystemStats};
use crate::types::{ProcessInfo, SimpleResult};
use crate::{AppState, REFRESH_MS};

static PROC_SYS: Mutex<Option<System>> = Mutex::new(None);

struct PowerLimitCache {
    entries: Option<(SystemTime, Vec<crate::power::PowerLimitAttr>)>,
    ttl_seconds: f64,
}

static POWER_LIMIT_CACHE: OnceLock<Mutex<PowerLimitCache>> = OnceLock::new();

fn power_limit_cache() -> &'static Mutex<PowerLimitCache> {
    POWER_LIMIT_CACHE.get_or_init(|| {
        Mutex::new(PowerLimitCache {
            entries: None,
            ttl_seconds: 5.0,
        })
    })
}

// ---------------------------------------------------------------------------
// Tauri commands
// ---------------------------------------------------------------------------

#[tauri::command]
pub fn cmd_discover() -> Capabilities {
    discover()
}

#[derive(Debug, serde::Serialize)]
pub struct AppInfo {
    pub version: String,
    pub os_name: String,
    pub os_version: String,
    pub kernel: String,
    pub helper_installed: bool,
    pub helper_version: Option<String>,
    pub helper_outdated: bool,
    pub config_dir: String,
}

fn os_release_value(key: &str) -> String {
    fs::read_to_string("/etc/os-release")
        .ok()
        .and_then(|text| {
            text.lines().find_map(|line| {
                let (k, v) = line.split_once('=')?;
                (k == key).then(|| v.trim_matches('"').to_string())
            })
        })
        .unwrap_or_default()
}

#[tauri::command]
pub fn cmd_get_app_info(state: tauri::State<'_, AppState>) -> AppInfo {
    let caps = state.caps.read().unwrap();
    AppInfo {
        version: env!("CARGO_PKG_VERSION").to_string(),
        os_name: {
            let name = os_release_value("PRETTY_NAME");
            if name.is_empty() {
                "Linux".into()
            } else {
                name
            }
        },
        os_version: os_release_value("VERSION_ID"),
        kernel: caps.kernel_version.clone(),
        helper_installed: caps.helper_installed,
        helper_version: caps.helper_version.clone(),
        helper_outdated: caps.helper_outdated,
        config_dir: home_dir().join(CONFIG_DIR).display().to_string(),
    }
}

#[tauri::command]
pub fn cmd_get_device_info(state: tauri::State<'_, AppState>) -> device::DeviceInfo {
    let caps = state.caps.read().unwrap();
    device::read_device_info(caps.helper_version.clone(), caps.helper_outdated)
}

#[tauri::command]
pub fn cmd_get_battery_detail() -> battery::BatteryDetail {
    battery::read_battery_detail()
}

#[tauri::command]
pub fn cmd_set_charge_threshold(percent: u32) -> SimpleResult {
    battery::set_charge_threshold(percent)
}

#[tauri::command]
pub fn cmd_get_presets() -> Vec<presets::PresetInfo> {
    presets::presets()
}

#[tauri::command]
pub fn cmd_apply_preset(state: tauri::State<'_, AppState>, name: String) -> SimpleResult {
    let mode = presets::presets()
        .into_iter()
        .find(|p| p.key == name && p.available)
        .and_then(|p| p.thermal_mode);
    let result = presets::apply_preset(&name);
    if result.ok {
        if let Some(mode) = mode {
            let mut cfg = state.config.write().unwrap();
            cfg.last_thermal_mode = mode;
            let _ = save_config(&cfg);
        }
    }
    result
}

#[tauri::command]
pub fn cmd_get_display_info() -> display::DisplayView {
    display::read_display_view()
}

#[tauri::command]
pub fn cmd_set_display_mode(output: String, width: u32, height: u32, refresh: f64) -> SimpleResult {
    if width == 0
        || height == 0
        || width > 100_000
        || height > 100_000
        || !(1.0..=1000.0).contains(&refresh)
    {
        return SimpleResult {
            ok: false,
            message: "invalid display mode".into(),
        };
    }
    display::set_display_mode(&output, width, height, refresh)
}

#[tauri::command]
pub fn cmd_set_brightness(percent: u32) -> SimpleResult {
    display::set_brightness(percent)
}

#[tauri::command]
pub fn cmd_run_diagnostics() -> diagnostics::DiagnosticsReport {
    diagnostics::run_diagnostics()
}

#[tauri::command]
pub fn cmd_smart_scan() -> diagnostics::SmartScan {
    diagnostics::smart_scan()
}

#[tauri::command]
pub fn cmd_export_diagnostics(
    state: tauri::State<'_, AppState>,
    report: diagnostics::DiagnosticsReport,
) -> SimpleResult {
    let caps = state.caps.read().unwrap();
    let device = device::read_device_info(caps.helper_version.clone(), caps.helper_outdated);
    drop(caps);
    match diagnostics::export_report(&device, &report) {
        Ok(path) => SimpleResult {
            ok: true,
            message: path,
        },
        Err(e) => SimpleResult {
            ok: false,
            message: e,
        },
    }
}

#[tauri::command]
pub fn cmd_get_gpu_details() -> gpu_ext::GpuDetails {
    gpu_ext::read_gpu_details()
}

#[tauri::command]
pub fn cmd_set_gpu_power_limit(watts: u32) -> SimpleResult {
    gpu_ext::set_power_limit(watts)
}

#[tauri::command]
pub fn cmd_set_gpu_runtime_pm(control: String) -> SimpleResult {
    gpu_ext::set_runtime_pm(&control)
}

#[tauri::command]
pub fn cmd_get_config(state: tauri::State<'_, AppState>) -> AppConfig {
    state.config.read().unwrap().clone()
}

#[derive(Debug, Deserialize)]
pub struct ConfigUpdate {
    pub refresh_interval_ms: Option<u64>,
    pub restore_profile_on_login: Option<bool>,
    pub auto_power_mode: Option<bool>,
    pub start_minimized: Option<bool>,
    pub desktop_notifications: Option<bool>,
    pub ac_preset: Option<String>,
    pub battery_preset: Option<String>,
}

fn valid_preset(name: &str) -> bool {
    ["quiet", "balanced", "performance", "custom"].contains(&name)
}

#[tauri::command]
pub fn cmd_update_config(state: tauri::State<'_, AppState>, update: ConfigUpdate) -> SimpleResult {
    // Validate into a copy first: a rejected field must not mutate live config.
    let mut snapshot = state.config.read().unwrap().clone();
    if let Some(ms) = update.refresh_interval_ms {
        snapshot.refresh_interval_ms = ms.clamp(250, 10_000);
    }
    if let Some(restore) = update.restore_profile_on_login {
        snapshot.restore_profile_on_login = restore;
    }
    if let Some(auto) = update.auto_power_mode {
        snapshot.auto_power_mode = auto;
    }
    if let Some(start_minimized) = update.start_minimized {
        snapshot.start_minimized = start_minimized;
    }
    if let Some(notifications) = update.desktop_notifications {
        snapshot.desktop_notifications = notifications;
    }
    if let Some(preset) = update.ac_preset {
        if !valid_preset(&preset) {
            return SimpleResult::from_app_error(AppError::InvalidInput {
                field: "ac_preset".into(),
                reason: format!("'{preset}' tidak didukung"),
            });
        }
        snapshot.ac_preset = preset;
    }
    if let Some(preset) = update.battery_preset {
        if !valid_preset(&preset) {
            return SimpleResult::from_app_error(AppError::InvalidInput {
                field: "battery_preset".into(),
                reason: format!("'{preset}' tidak didukung"),
            });
        }
        snapshot.battery_preset = preset;
    }
    if let Err(e) = save_config(&snapshot) {
        return SimpleResult {
            ok: false,
            message: e,
        };
    }
    REFRESH_MS.store(snapshot.refresh_interval_ms, Ordering::Relaxed);
    *state.config.write().unwrap() = snapshot;
    SimpleResult {
        ok: true,
        message: "saved".into(),
    }
}

#[tauri::command]
pub fn cmd_get_stats_snapshot() -> SystemStats {
    collect_stats()
}

#[tauri::command]
pub fn cmd_get_history(metric: String, window_seconds: f64) -> Option<history::MetricSeries> {
    history::history_get(&metric, window_seconds)
}

#[tauri::command]
pub fn cmd_get_processes(filter: Option<String>, limit: Option<usize>) -> Vec<ProcessInfo> {
    collect_processes(filter.as_deref(), limit.unwrap_or(400).clamp(1, 1000))
}

pub(crate) fn collect_processes(filter: Option<&str>, limit: usize) -> Vec<ProcessInfo> {
    let mut guard = PROC_SYS.lock().unwrap();
    let sys = guard.get_or_insert_with(System::new_all);
    sys.refresh_cpu_all();
    sys.refresh_processes(ProcessesToUpdate::All, true);
    let mut procs: Vec<ProcessInfo> = sys
        .processes()
        .iter()
        .map(|(pid, p)| {
            let mem_mb = p.memory() as f64 / 1024.0 / 1024.0;
            ProcessInfo {
                pid: pid.as_u32() as i32,
                name: p.name().to_string_lossy().to_string(),
                user: p.user_id().map(|u| u.to_string()).unwrap_or_default(),
                cpu: p.cpu_usage(),
                memory_mb: mem_mb,
            }
        })
        .collect();
    if let Some(f) = filter {
        let f = f.to_lowercase();
        procs.retain(|p| p.name.to_lowercase().contains(&f) || p.pid.to_string().contains(&f));
    }
    procs.sort_by(|a, b| {
        b.memory_mb
            .partial_cmp(&a.memory_mb)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    procs.truncate(limit);
    procs
}

#[tauri::command]
pub fn cmd_kill_process(pid: i32, force: bool) -> SimpleResult {
    if pid <= 1 {
        return SimpleResult {
            ok: false,
            message: "refusing to signal PID 1 or invalid PID".into(),
        };
    }
    let signal = if force { "-KILL" } else { "-TERM" };
    match Command::new("kill")
        .arg(signal)
        .arg(pid.to_string())
        .status()
    {
        Ok(status) if status.success() => SimpleResult {
            ok: true,
            message: format!("sent SIG{}", &signal[1..]),
        },
        Ok(_) => SimpleResult {
            ok: false,
            message: "process not found or permission denied".into(),
        },
        Err(e) => SimpleResult {
            ok: false,
            message: e.to_string(),
        },
    }
}

#[tauri::command]
pub fn cmd_get_charge_mode() -> ChargeModeInfo {
    get_charge_mode()
}

#[tauri::command]
pub fn cmd_set_charge_mode(mode: String) -> SimpleResult {
    set_charge_mode(&mode)
}

#[tauri::command]
pub fn cmd_battery_history(seconds: f64) -> Result<Vec<db::BatterySample>, String> {
    db::get_history(seconds)
}

#[tauri::command]
pub fn cmd_battery_latest_trend() -> Result<Option<db::BatteryTrend>, String> {
    db::get_latest_trend()
}

#[tauri::command]
pub fn cmd_battery_rate(window_seconds: f64) -> Result<Option<f64>, String> {
    db::get_rate(window_seconds)
}

#[tauri::command]
pub fn cmd_battery_capacity_alert(threshold_pct: f64) -> Result<Option<(f64, f64, f64)>, String> {
    db::check_capacity_drop(threshold_pct)
}

#[tauri::command]
pub fn cmd_export_battery_csv(path: String) -> Result<String, ErrorEnvelope> {
    db::export_csv(Path::new(&path)).map_err(|e| {
        ErrorEnvelope::from(AppError::Io {
            op: "export battery CSV".into(),
            path: Some(std::path::PathBuf::from(&path)),
            source: e,
        })
    })
}

#[tauri::command]
pub fn cmd_list_profiles() -> Result<profiles::ProfileList, String> {
    profiles::list_profiles()
}

#[tauri::command]
pub fn cmd_save_profile(profile: profiles::Profile) -> Result<profiles::ProfileList, String> {
    profiles::save_profile(profile)
}

#[tauri::command]
pub fn cmd_delete_profile(key: String) -> Result<profiles::ProfileList, String> {
    profiles::delete_profile(&key)
}

#[tauri::command]
pub fn cmd_apply_profile(state: tauri::State<'_, AppState>, key: String) -> SimpleResult {
    // Ambil profil, lalu terapkan setiap setting melalui command yang sudah ada.
    let profile = match profiles::get_profile(&key) {
        Ok(Some(p)) => p,
        Ok(None) => {
            return SimpleResult::from_app_error(AppError::NotFound {
                kind: "profile".into(),
                key,
            })
        }
        Err(e) => return SimpleResult::from_app_error(AppError::Other { message: e }),
    };

    let mut pending_reboot = Vec::new();
    let mut applied = 0;

    for (setting, value) in &profile.settings {
        match setting.as_str() {
            "thermal_mode" => {
                if set_thermal_mode(value).ok {
                    applied += 1;
                    let mut cfg = state.config.write().unwrap();
                    cfg.last_thermal_mode = value.clone();
                    let _ = save_config(&cfg);
                }
            }
            "charge_mode" => {
                if set_charge_mode(value).ok {
                    applied += 1;
                }
            }
            "charge_threshold" => {
                if let Ok(pct) = value.parse::<u32>() {
                    if crate::battery::set_charge_threshold(pct).ok {
                        applied += 1;
                    }
                }
            }
            "brightness" => {
                if let Ok(pct) = value.parse::<u32>() {
                    if crate::display::set_brightness(pct).ok {
                        applied += 1;
                    }
                }
            }
            "gpu_mode" => {
                if gpu_modes().iter().any(|m| m.key.as_str() == value.as_str())
                    && cmd_switch_gpu_mode(value.clone()).ok
                {
                    applied += 1;
                    pending_reboot.push(format!("GPU mode -> {value} (reboot required)"));
                }
            }
            "gpu_runtime_pm" => {
                if crate::gpu_ext::set_runtime_pm(value).ok {
                    applied += 1;
                }
            }
            "fan_preset" => {
                // Apply to both fans: `||` would silently skip fan 2.
                let profile = value.to_lowercase();
                if matches!(profile.as_str(), "auto" | "quiet" | "full") {
                    let mut all_ok = true;
                    for fan_id in [1, 2] {
                        let res = crate::fan_control::cmd_apply_fan_profile(
                            crate::fan_control::FanProfileCmd {
                                profile: profile.clone(),
                                fan_id,
                            },
                        );
                        all_ok &= res.ok;
                    }
                    if all_ok {
                        applied += 1;
                    }
                }
            }
            _ => {}
        }
    }

    let mut message = format!(
        "Profile '{}' applied: {} setting(s).",
        profile.name, applied
    );
    if !pending_reboot.is_empty() {
        message.push(' ');
        message.push_str(&pending_reboot.join("; "));
    }
    SimpleResult { ok: true, message }
}

#[tauri::command]
pub fn cmd_get_thermal_mode() -> ThermalModeInfo {
    get_thermal_mode()
}

#[tauri::command]
pub fn cmd_set_thermal_mode(state: tauri::State<AppState>, mode: String) -> SimpleResult {
    let res = set_thermal_mode(&mode);
    if res.ok {
        let mut cfg = state.config.write().unwrap();
        cfg.last_thermal_mode = mode;
        let snapshot = cfg.clone();
        drop(cfg);
        let _ = save_config(&snapshot);
    }
    res
}

pub fn memory_info() -> MemoryInfo {
    let (_, used, total, _, _, _, _) = read_nvidia();
    let dimms = read_dimms();
    MemoryInfo {
        source: if dimms.is_empty() {
            "unavailable".into()
        } else {
            "dmidecode".into()
        },
        dimms,
        vram_used_mb: used,
        vram_total_mb: total,
        top_processes: collect_processes(None, 8),
    }
}

#[tauri::command]
pub fn cmd_get_memory_info() -> MemoryInfo {
    memory_info()
}

#[tauri::command]
pub fn cmd_drop_caches() -> SimpleResult {
    run_helper("drop-caches", &[])
}

#[tauri::command]
pub fn cmd_get_gpu_mode() -> GpuModeInfo {
    let (mode, backend, conflict) = detect_gpu_mode();
    let pending = pending_gpu_switch();
    if pending.is_some() && pending == mode {
        let _ = fs::remove_file(home_dir().join(CONFIG_DIR).join("pending_gpu_switch"));
    }
    let pending = if pending.is_some() && pending == mode {
        None
    } else {
        pending
    };
    GpuModeInfo {
        mode_label: mode.as_deref().map(gpu_label).unwrap_or_default(),
        mode,
        pending,
        backend,
        conflict,
        modes: gpu_modes(),
    }
}

#[tauri::command]
pub fn cmd_switch_gpu_mode(mode: String) -> SimpleResult {
    if !gpu_modes().iter().any(|m| m.key == mode) {
        return SimpleResult {
            ok: false,
            message: format!("unsupported GPU mode: {mode}"),
        };
    }
    let res = run_helper("gpu-switch", &[&mode]);
    if res.ok {
        let marker = home_dir().join(CONFIG_DIR).join("pending_gpu_switch");
        if let Some(parent) = marker.parent() {
            let _ = fs::create_dir_all(parent);
        }
        let _ = fs::write(&marker, &mode);
    }
    res
}

#[tauri::command]
pub fn cmd_get_gpu_processes() -> Vec<GpuProcessInfo> {
    read_gpu_processes()
}

#[tauri::command]
pub fn cmd_get_power_limits() -> Vec<PowerLimitAttr> {
    let now = SystemTime::now();
    let guard = power_limit_cache().lock().unwrap();
    if let Some((cached_at, entries)) = &guard.entries {
        if now
            .duration_since(*cached_at)
            .unwrap_or_default()
            .as_secs_f64()
            < guard.ttl_seconds
        {
            return entries.clone();
        }
    }
    drop(guard);

    let mut out = Vec::new();
    for (key, candidates) in crate::power::ATTR_MAP {
        let path = candidates.iter().find_map(|c| {
            if crate::power::find_attr_path(c).is_some() {
                Some(c.to_string())
            } else {
                None
            }
        });
        let path = match path {
            Some(c) => match crate::power::find_attr_path(&c) {
                Some(p) => p,
                None => continue,
            },
            None => continue,
        };
        let current = match crate::power::read_sysfs_int(&path) {
            Some(v) => v,
            None => continue,
        };
        let min_f = path.parent().unwrap().join("min_value");
        let max_f = path.parent().unwrap().join("max_value");
        let min_val = read_sysfs_int(&min_f).unwrap_or(match *key {
            "cpu_pl1" => 50,
            "cpu_pl2" => 60,
            "cpu_peak" => 0,
            "cpu_crossload" => 30,
            "cpu_temp" => 85,
            "gpu_ctgp" => 55,
            "gpu_ppab" => 10,
            "gpu_ac_offset" => 10,
            "gpu_temp" => 75,
            _ => 0,
        });
        let max_val = read_sysfs_int(&max_f).unwrap_or(match *key {
            "cpu_pl1" => 95,
            "cpu_pl2" => 167,
            "cpu_peak" => 56,
            "cpu_crossload" => 55,
            "cpu_temp" => 100,
            "gpu_ctgp" => 95,
            "gpu_ppab" => 25,
            "gpu_ac_offset" => 80,
            "gpu_temp" => 87,
            _ => 255,
        });
        let label = crate::power::ATTR_LABELS
            .iter()
            .find(|(k, _)| *k == *key)
            .map(|(_, l)| *l)
            .unwrap_or(*key);
        let unit = crate::power::ATTR_UNITS
            .iter()
            .find(|(k, _)| *k == *key)
            .map(|(_, u)| *u)
            .unwrap_or("");
        let position_pct = if max_val > min_val {
            ((current - min_val) as f64 / (max_val - min_val) as f64 * 100.0).clamp(0.0, 100.0)
        } else {
            0.0
        };
        out.push(PowerLimitAttr {
            key: key.to_string(),
            label: label.to_string(),
            unit: unit.to_string(),
            current,
            min_val,
            max_val,
            position_pct,
            writable: Path::new(crate::config::HELPER_PATH).is_file()
                && !fs::metadata(&path)
                    .map(|m| m.permissions().readonly())
                    .unwrap_or(true),
        });
    }
    let mut guard = power_limit_cache().lock().unwrap();
    guard.entries = Some((now, out.clone()));
    out
}

#[tauri::command]
pub fn cmd_set_power_limit(cmd: SetLimitCmd) -> SimpleResult {
    let candidates = ATTR_MAP
        .iter()
        .find(|(k, _)| *k == cmd.attr)
        .map(|(_, c)| *c)
        .unwrap_or(&[]);
    let sysfs_name = candidates
        .iter()
        .find(|c| find_attr_path(c).is_some())
        .copied()
        .unwrap_or("");
    if sysfs_name.is_empty() {
        return SimpleResult::from_app_error(AppError::HardwareUnsupported {
            feature: format!("power limit attribute {}", cmd.attr),
        });
    }
    let res = run_helper("power-limit", &[sysfs_name, &cmd.value.to_string()]);
    if res.ok {
        let mut guard = power_limit_cache().lock().unwrap();
        guard.entries = None;
    }
    res
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct DeviceSettingsState {
    pub keyboard_backlight: bool,
    pub touchpad: bool,
    pub camera: bool,
    pub fn_lock: bool,
    pub win_key: bool,
    pub usb_charging: bool,
    pub vantage_key: bool,
    pub overdrive: bool,
}

#[tauri::command]
pub fn cmd_get_device_settings(state: tauri::State<'_, AppState>) -> DeviceSettingsState {
    let fn_lock = fs::read_to_string("/sys/bus/platform/drivers/ideapad_acpi/VPC2004:00/fn_lock")
        .map(|s| s.trim() == "1")
        .unwrap_or(false);
    let camera =
        fs::read_to_string("/sys/bus/platform/drivers/ideapad_acpi/VPC2004:00/camera_power")
            .map(|s| s.trim() == "1")
            .unwrap_or(true);
    let usb_charging =
        fs::read_to_string("/sys/bus/platform/drivers/ideapad_acpi/VPC2004:00/usb_charging")
            .map(|s| s.trim() == "1")
            .unwrap_or(false);

    let cfg = state.config.read().unwrap();
    DeviceSettingsState {
        keyboard_backlight: cfg.keyboard_backlight.unwrap_or(true),
        touchpad: cfg.touchpad.unwrap_or(true),
        camera,
        fn_lock,
        win_key: cfg.win_key.unwrap_or(true),
        usb_charging,
        vantage_key: cfg.vantage_key.unwrap_or(true),
        overdrive: cfg.overdrive.unwrap_or(false),
    }
}

#[tauri::command]
pub fn cmd_set_device_setting(
    state: tauri::State<'_, AppState>,
    key: String,
    value: bool,
) -> SimpleResult {
    let val_str = if value { "1" } else { "0" };
    match key.as_str() {
        "keyboard_backlight" => {
            let res = if value {
                crate::keyboard::cmd_keyboard_all(crate::keyboard::KeyboardAllCmd {
                    zones: vec![
                        crate::keyboard::KeyboardZoneCmd {
                            zone: 0,
                            color: "#19C9E8".into(),
                        },
                        crate::keyboard::KeyboardZoneCmd {
                            zone: 1,
                            color: "#168CFF".into(),
                        },
                        crate::keyboard::KeyboardZoneCmd {
                            zone: 2,
                            color: "#168CFF".into(),
                        },
                        crate::keyboard::KeyboardZoneCmd {
                            zone: 3,
                            color: "#19C9E8".into(),
                        },
                    ],
                    effect: "Static".into(),
                    brightness_pct: 100,
                })
            } else {
                crate::keyboard::cmd_keyboard_off()
            };
            if !res.ok {
                return res;
            }
            state.config.write().unwrap().keyboard_backlight = Some(value);
        }
        // Firmware-backed toggles: route through the privileged helper and only
        // record the preference when the hardware accepted the write.
        "camera" | "fn_lock" | "usb_charging" | "touchpad" => {
            let res = run_helper("device-setting", &[key.as_str(), val_str]);
            if !res.ok {
                return res;
            }
            let mut cfg = state.config.write().unwrap();
            match key.as_str() {
                "camera" => cfg.camera = Some(value),
                "fn_lock" => cfg.fn_lock = Some(value),
                "usb_charging" => cfg.usb_charging = Some(value),
                "touchpad" => cfg.touchpad = Some(value),
                _ => unreachable!(),
            }
        }
        // App preferences with no firmware interface exposed on this platform.
        "win_key" => state.config.write().unwrap().win_key = Some(value),
        "vantage_key" => state.config.write().unwrap().vantage_key = Some(value),
        "overdrive" => state.config.write().unwrap().overdrive = Some(value),
        _ => {
            return SimpleResult {
                ok: false,
                message: format!("Unknown setting: {key}"),
            };
        }
    }
    let snapshot = state.config.read().unwrap().clone();
    let _ = save_config(&snapshot);
    SimpleResult {
        ok: true,
        message: format!("Setting '{key}' set to {value}."),
    }
}

#[tauri::command]
pub fn cmd_get_resource_status(state: tauri::State<'_, AppState>) -> ResourceStatus {
    state.governor.status()
}

#[tauri::command]
pub fn cmd_list_rules(state: tauri::State<'_, AppState>) -> Vec<Rule> {
    state.governor.rules().read().unwrap().clone()
}

#[tauri::command]
pub fn cmd_validate_rule(rule: Rule) -> SimpleResult {
    match validate_rule(&rule) {
        Ok(()) => SimpleResult {
            ok: true,
            message: "valid".into(),
        },
        Err(e) => SimpleResult {
            ok: false,
            message: e,
        },
    }
}

#[tauri::command]
pub fn cmd_add_rule(state: tauri::State<'_, AppState>, rule: Rule) -> SimpleResult {
    if let Err(e) = validate_rule(&rule) {
        return SimpleResult {
            ok: false,
            message: e,
        };
    }
    let arc = state.governor.rules();
    let mut guard = arc.write().unwrap();
    let mut next = guard.clone();
    if next.iter().any(|r| r.id == rule.id) {
        return SimpleResult {
            ok: false,
            message: format!("rule '{}' already exists", rule.id),
        };
    }
    if next.len() >= 48 {
        return SimpleResult {
            ok: false,
            message: "maximum of 48 rules reached".into(),
        };
    }
    next.push(rule.clone());
    if let Err(e) = ResourceGovernor::save_rules(&next) {
        return SimpleResult {
            ok: false,
            message: format!("failed to persist rules: {e}"),
        };
    }
    *guard = next;
    drop(guard);
    state.governor.reset_detection_state();
    SimpleResult {
        ok: true,
        message: format!("rule '{}' added", rule.id),
    }
}

#[tauri::command]
pub fn cmd_update_rule(state: tauri::State<'_, AppState>, rule: Rule) -> SimpleResult {
    if let Err(e) = validate_rule(&rule) {
        return SimpleResult {
            ok: false,
            message: e,
        };
    }
    let arc = state.governor.rules();
    let mut guard = arc.write().unwrap();
    let mut next = guard.clone();
    let Some(existing) = next.iter_mut().find(|r| r.id == rule.id) else {
        return SimpleResult {
            ok: false,
            message: format!("rule '{}' not found", rule.id),
        };
    };
    *existing = rule.clone();
    if let Err(e) = ResourceGovernor::save_rules(&next) {
        return SimpleResult {
            ok: false,
            message: format!("failed to persist rules: {e}"),
        };
    }
    *guard = next;
    drop(guard);
    state.governor.reset_detection_state();
    SimpleResult {
        ok: true,
        message: format!("rule '{}' updated", rule.id),
    }
}

#[tauri::command]
pub fn cmd_delete_rule(state: tauri::State<'_, AppState>, id: String) -> SimpleResult {
    let arc = state.governor.rules();
    let mut guard = arc.write().unwrap();
    let mut next = guard.clone();
    if !next.iter().any(|r| r.id == id) {
        return SimpleResult {
            ok: false,
            message: format!("rule '{}' not found", id),
        };
    }
    next.retain(|r| r.id != id);
    if let Err(e) = ResourceGovernor::save_rules(&next) {
        return SimpleResult {
            ok: false,
            message: format!("failed to persist rules: {e}"),
        };
    }
    *guard = next;
    drop(guard);
    state.governor.reset_detection_state();
    SimpleResult {
        ok: true,
        message: format!("rule '{}' deleted", id),
    }
}

#[tauri::command]
pub fn cmd_toggle_rule(state: tauri::State<'_, AppState>, id: String) -> SimpleResult {
    let arc = state.governor.rules();
    let mut guard = arc.write().unwrap();
    let mut next = guard.clone();
    let Some(rule) = next.iter_mut().find(|r| r.id == id) else {
        return SimpleResult {
            ok: false,
            message: format!("rule '{}' not found", id),
        };
    };
    rule.enabled = !rule.enabled;
    let enabled = rule.enabled;
    if let Err(e) = ResourceGovernor::save_rules(&next) {
        return SimpleResult {
            ok: false,
            message: format!("failed to persist rules: {e}"),
        };
    }
    *guard = next;
    drop(guard);
    state.governor.reset_detection_state();
    SimpleResult {
        ok: true,
        message: format!(
            "rule '{}' {}",
            id,
            if enabled { "enabled" } else { "disabled" }
        ),
    }
}

#[tauri::command]
pub fn cmd_get_automation_events(limit: Option<usize>) -> Result<Vec<AutomationEvent>, String> {
    db::recent_automation_events(limit.unwrap_or(100).clamp(1, 1000))
}

#[tauri::command]
pub fn cmd_clear_local_history() -> SimpleResult {
    match db::clear_local_history() {
        Ok(()) => SimpleResult {
            ok: true,
            message: "local history cleared".into(),
        },
        Err(e) => SimpleResult {
            ok: false,
            message: e,
        },
    }
}

#[tauri::command]
pub fn cmd_clear_automation_events() -> SimpleResult {
    match db::clear_automation_events() {
        Ok(count) => SimpleResult {
            ok: true,
            message: format!("{count} automation event(s) cleared"),
        },
        Err(e) => SimpleResult {
            ok: false,
            message: e,
        },
    }
}
