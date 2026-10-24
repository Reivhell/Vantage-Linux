use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, OnceLock, RwLock};
use std::thread;
use std::time::Duration;
use tauri::{Builder, Emitter, Manager};
use tokio::runtime::Runtime;
use tracing::{info, warn};

use crate::resource_governor::ResourceGovernor;

pub(crate) use commands::collect_processes;
pub(crate) use config::{home_dir, HELPER_PATH};
use power::{maybe_auto_preset, note_power_state, set_thermal_mode};
pub(crate) use power::{run_helper, run_helper_quiet};
pub(crate) use types::which;

pub use capabilities::{discover, Capabilities};
pub use commands::memory_info;
pub use config::{load_config, save_config, AppConfig};
pub(crate) use fan_control::FanCurvePoint;
pub use gpu::GpuModeInfo;
pub use memory::MemoryInfo;
pub use power::{
    get_available_profiles, get_battery_conservation, get_charge_mode, get_thermal_mode,
    ChargeModeInfo, PowerLimitAttr, SetLimitCmd, ThermalModeInfo,
};
pub use stats::{collect_stats, SystemStats};
pub(crate) use stats::{find_hwmon, read_cpu_temp, read_fan_rpms, read_gpu_temp_sysfs};
pub use types::{ProcessInfo, SimpleResult};

mod battery;
mod capabilities;
mod commands;
mod config;
mod db;
mod device;
mod diagnostics;
mod display;
mod error;
mod fan;
mod fan_control;
mod gpu;
mod gpu_ext;
mod history;
mod keyboard;
mod memory;
mod power;
mod presets;
mod profiles;
mod resource_governor;
mod resource_types;
mod stats;
mod types;

static APP_HANDLE: OnceLock<tauri::AppHandle> = OnceLock::new();
static LAST_BATTERY_SAMPLE: Mutex<f64> = Mutex::new(0.0);
pub(crate) static REFRESH_MS: AtomicU64 = AtomicU64::new(1000);

pub struct AppState {
    pub caps: RwLock<Capabilities>,
    pub config: RwLock<AppConfig>,
    pub fan_failsafe: FanFailSafe,
    pub governor: Arc<ResourceGovernor>,
    pub rt: Runtime,
}

/// Guard yang mengembalikan kontrol kipas ke firmware saat AppState dibuang
/// (saat Tauri berhenti / crash) sebagai fail-safe.
pub struct FanFailSafe;

impl Drop for FanFailSafe {
    fn drop(&mut self) {
        fan::fan_fail_safe();
    }
}

impl Default for FanFailSafe {
    fn default() -> Self {
        Self
    }
}

// ---------------------------------------------------------------------------
// Discovery
// ---------------------------------------------------------------------------

pub fn display_info() -> display::DisplayView {
    display::read_display_view()
}

pub fn device_info() -> device::DeviceInfo {
    device::read_device_info(None, false)
}

pub fn battery_detail() -> battery::BatteryDetail {
    battery::read_battery_detail()
}

pub fn preset_info() -> Vec<presets::PresetInfo> {
    presets::presets()
}

// ---------------------------------------------------------------------------
// Background monitor loop (emits stats events to frontend)
// ---------------------------------------------------------------------------

fn monitor_loop(handle: tauri::AppHandle) -> Result<(), Box<dyn std::error::Error>> {
    let _ = APP_HANDLE.set(handle.clone());
    loop {
        let stats = collect_stats();
        let _ = handle.emit("loq:stats", &stats);

        // Battery history is sampled every 60 s: full capacity, design capacity,
        // health, cycles, power and plug state all come from real sysfs data.
        let sample_due = {
            let mut last = LAST_BATTERY_SAMPLE.lock().unwrap();
            if stats.timestamp - *last >= 60.0 {
                *last = stats.timestamp;
                true
            } else {
                false
            }
        };
        if sample_due {
            let detail = battery::read_battery_detail();
            if let Some(pct) = detail.percent.or(stats.battery_percent.map(|v| v as f64)) {
                let status = if detail.status.is_empty() {
                    stats.battery_status.as_str()
                } else {
                    detail.status.as_str()
                };
                let _ = db::record_sample(
                    Some(pct),
                    detail
                        .health_pct
                        .or(stats.battery_health_pct.map(|v| v as f64)),
                    detail.cycles.or(stats.battery_cycles),
                    detail.energy_full_wh,
                    detail.energy_design_wh,
                    detail.power_w.or(stats.battery_power_w),
                    status,
                    stats.power_plugged,
                );
            }
        }

        // Deteksi reset firmware pada kipas setiap siklus monitoring.
        fan::fan_check_reset(1, stats.fan1_rpm);
        fan::fan_check_reset(2, stats.fan2_rpm);

        if let Some(plugged) = stats.power_plugged {
            if note_power_state(plugged) {
                std::thread::spawn(move || maybe_auto_preset(plugged));
            }
        }
        let interval = REFRESH_MS.load(Ordering::Relaxed).clamp(250, 10_000);
        thread::sleep(Duration::from_millis(interval));
    }
}

fn build_tray(app: &tauri::App) -> tauri::Result<()> {
    use tauri::menu::{Menu, MenuItem};
    use tauri::tray::TrayIconBuilder;

    let show = MenuItem::with_id(app, "show", "Show window", true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", "Quit", true, None::<&str>)?;
    let menu = Menu::with_items(app, &[&show, &quit])?;

    let mut builder = TrayIconBuilder::new()
        .tooltip("LOQ Control Center")
        .menu(&menu)
        .on_menu_event(|app, event| match event.id.as_ref() {
            "show" => {
                if let Some(window) = app.get_webview_window("main") {
                    let _ = window.show();
                    let _ = window.set_focus();
                }
            }
            "quit" => app.exit(0),
            _ => {}
        });
    if let Some(icon) = app.default_window_icon() {
        builder = builder.icon(icon.clone());
    }
    builder.build(app)?;
    Ok(())
}

pub fn run() {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .with_target(false)
        .init();
    info!("starting LOQ Control Center");

    let rt = Runtime::new().expect("failed to create tokio runtime");
    let rules = ResourceGovernor::load_rules();
    let (governor, executor) = ResourceGovernor::new(rules);
    let governor = Arc::new(governor);

    let g = governor.clone();
    rt.spawn(async move {
        let mut interval = tokio::time::interval(Duration::from_secs(1));
        loop {
            interval.tick().await;
            g.tick();
            g.evaluate();
        }
    });
    rt.spawn(executor.run());

    Builder::default()
        .manage(AppState {
            caps: RwLock::new(discover()),
            config: RwLock::new(load_config()),
            fan_failsafe: FanFailSafe::default(),
            governor,
            rt,
        })
        .setup(|app| {
            // Inisialisasi schema SQLite lokal untuk histori baterai (tidak fatal jika gagal).
            if let Err(e) = db::init_db() {
                warn!("failed to init battery db: {e}");
            }
            // Inisialisasi connection pool global untuk baterai (tidak fatal jika gagal).
            if let Err(e) = db::init_pool() {
                warn!("failed to init db pool: {e}");
            }
            // Inisialisasi penyimpan histori metrik untuk grafik (simpan 24 jam = 86400 detik).
            history::init_history(86400.0);
            let handle = app.handle().clone();
            std::thread::spawn(move || {
            if let Err(e) = monitor_loop(handle) {
                warn!("monitor loop ended: {e}");
            }
            });
            build_tray(app)?;
            let cfg = load_config();
            REFRESH_MS.store(
                cfg.refresh_interval_ms.clamp(250, 10_000),
                Ordering::Relaxed,
            );
            if cfg.restore_profile_on_login && !cfg.last_thermal_mode.is_empty() {
                let thermal = cfg.last_thermal_mode.clone();
                std::thread::spawn(move || {
                    let _ = set_thermal_mode(&thermal);
                });
            }
            if cfg.start_minimized {
                if let Some(window) = app.get_webview_window("main") {
                    let _ = window.hide();
                }
            }
            Ok(())
        })
        .on_window_event(|window, event| {
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                api.prevent_close();
                let _ = window.hide();
            }
        })
        .invoke_handler(tauri::generate_handler![
            commands::cmd_discover,
            commands::cmd_get_device_info,
            commands::cmd_get_battery_detail,
            commands::cmd_set_charge_threshold,
            commands::cmd_get_presets,
            commands::cmd_apply_preset,
            commands::cmd_get_display_info,
            commands::cmd_set_display_mode,
            commands::cmd_set_brightness,
            commands::cmd_run_diagnostics,
            commands::cmd_smart_scan,
            commands::cmd_export_diagnostics,
            commands::cmd_get_gpu_details,
            commands::cmd_set_gpu_power_limit,
            commands::cmd_set_gpu_runtime_pm,
            commands::cmd_get_config,
            commands::cmd_update_config,
            commands::cmd_get_stats_snapshot,
            commands::cmd_get_history,
            commands::cmd_get_processes,
            commands::cmd_kill_process,
            commands::cmd_get_charge_mode,
            commands::cmd_set_charge_mode,
            commands::cmd_battery_history,
            commands::cmd_battery_latest_trend,
            commands::cmd_battery_rate,
            commands::cmd_battery_capacity_alert,
            commands::cmd_export_battery_csv,
            commands::cmd_list_profiles,
            commands::cmd_save_profile,
            commands::cmd_delete_profile,
            commands::cmd_apply_profile,
            commands::cmd_get_thermal_mode,
            commands::cmd_set_thermal_mode,
            commands::cmd_get_memory_info,
            commands::cmd_drop_caches,
            commands::cmd_get_gpu_mode,
            commands::cmd_switch_gpu_mode,
            commands::cmd_get_gpu_processes,
            fan_control::cmd_get_fan_info,
            fan_control::cmd_apply_fan_curve,
            fan_control::cmd_set_fan_manual,
            fan_control::cmd_fan_auto,
            fan_control::cmd_fan_status,
            fan_control::cmd_apply_fan_profile,
            commands::cmd_get_power_limits,
            commands::cmd_set_power_limit,
            keyboard::cmd_get_lighting_info,
            keyboard::cmd_keyboard_zone,
            keyboard::cmd_keyboard_all,
            keyboard::cmd_keyboard_off,
            commands::cmd_get_device_settings,
            commands::cmd_set_device_setting,
            commands::cmd_get_resource_status,
            commands::cmd_list_rules,
            commands::cmd_add_rule,
            commands::cmd_delete_rule,
            commands::cmd_toggle_rule,
        ])
        .run(tauri::generate_context!())
        .expect("error while running LOQ Control Center");
}
