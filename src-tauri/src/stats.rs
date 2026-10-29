use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};
use sysinfo::System;

use crate::history;

static LAST_DISK: Mutex<Option<(u64, u64, u64)>> = Mutex::new(None);
static MONITOR_SYS: Mutex<Option<System>> = Mutex::new(None);
static LAST_DISK_TIME: Mutex<Option<f64>> = Mutex::new(None);
static LAST_NET: Mutex<Option<(String, u64, u64, f64)>> = Mutex::new(None);
static MOUNTS_CACHE: Mutex<Option<(f64, Vec<DiskMount>)>> = Mutex::new(None);
static LAST_HISTORY_PUSH: Mutex<f64> = Mutex::new(0.0);
static HWMON_CACHE: Mutex<Option<(f64, Vec<(String, PathBuf)>)>> = Mutex::new(None);
static IP_CACHE: Mutex<Option<(String, String, f64)>> = Mutex::new(None);
static NVIDIA_CACHE: Mutex<Option<(f64, NvidiaReadings)>> = Mutex::new(None);

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SystemStats {
    pub timestamp: f64,
    pub cpu_percent: f32,
    pub cpu_per_core: Vec<f32>,
    pub cpu_freq_mhz: f32,
    pub cpu_temp: Option<f32>,
    pub ram_used_gb: f64,
    pub ram_total_gb: f64,
    pub ram_available_gb: f64,
    pub ram_free_gb: f64,
    pub ram_cached_gb: f64,
    pub ram_percent: f32,
    pub swap_used_gb: f64,
    pub swap_total_gb: f64,
    pub disk_mounts: Vec<DiskMount>,
    pub disk_read_kbps: f64,
    pub disk_write_kbps: f64,
    pub disk_busy_percent: f64,
    pub gpu_util: Option<u32>,
    pub gpu_vram_used_mb: Option<u64>,
    pub gpu_vram_total_mb: Option<u64>,
    pub gpu_temp: Option<u32>,
    pub gpu_freq_mhz: Option<u32>,
    pub gpu_power_w: Option<f64>,
    pub gpu_fan_pct: Option<u32>,
    pub igpu_util: Option<f32>,
    pub fan1_rpm: u32,
    pub fan2_rpm: u32,
    pub fan1_available: bool,
    pub fan2_available: bool,
    pub ssd_temp: Option<f32>,
    pub ram_temp: Option<f32>,
    pub wifi_temp: Option<f32>,
    pub board_temp: Option<f32>,
    pub thermal: Vec<ThermalSensor>,
    pub net_rx_kbps: f64,
    pub net_tx_kbps: f64,
    pub net_interface: String,
    pub net_ipv4: String,
    pub net_is_wifi: bool,
    pub power_plugged: Option<bool>,
    pub battery_percent: Option<f32>,
    pub battery_health_pct: Option<f32>,
    pub battery_cycles: Option<u32>,
    pub battery_status: String,
    pub battery_power_w: Option<f64>,
    pub battery_energy_now_wh: Option<f64>,
    pub battery_energy_full_wh: Option<f64>,
    pub view: StatsView,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ThermalSensor {
    pub id: String,
    pub label: String,
    pub temp_c: Option<f32>,
    pub source: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct BarView {
    pub scale: f64,
    pub hot: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct StatsBars {
    pub cpu: BarView,
    pub ram: BarView,
    pub gpu: BarView,
    pub igpu: BarView,
    pub mem: BarView,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct LiveStats {
    pub cpu: String,
    pub gpu: String,
    pub temp: String,
    pub battery: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Sparklines {
    pub cpu: String,
    pub ram: String,
    pub gpu: String,
    pub net: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct CoreView {
    pub label: String,
    pub percent_label: String,
    pub scale: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct MountView {
    pub mount: String,
    pub usage: String,
    pub scale: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct StatsView {
    pub live: LiveStats,
    pub cpu_percent: String,
    pub cpu_freq: String,
    pub cpu_temp: String,
    pub ram_percent: String,
    pub ram_used: String,
    pub ram_total: String,
    pub swap: String,
    pub gpu_percent: String,
    pub gpu_temp: String,
    pub gpu_freq: String,
    pub gpu_power: String,
    pub gpu_vram: String,
    pub gpu_fan: String,
    pub igpu_percent: String,
    pub battery_health: String,
    pub battery_cycles: String,
    pub battery_status: String,
    pub battery_power: String,
    pub fan_rpm: String,
    pub battery: String,
    pub battery_time: String,
    pub ac_state: String,
    pub net_status: String,
    pub net_interface: String,
    pub net_ip: String,
    pub net_rx: String,
    pub net_tx: String,
    pub disk_read: String,
    pub disk_write: String,
    pub bars: StatsBars,
    pub sparks: Sparklines,
    pub cores: Vec<CoreView>,
    pub mounts: Vec<MountView>,
    pub mount_key: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiskMount {
    pub mount: String,
    pub used_gb: f64,
    pub total_gb: f64,
    pub percent: f64,
    pub device: String,
    pub fstype: String,
    pub label: String,
}

pub fn collect_stats() -> SystemStats {
    collect_stats_inner(true)
}

/// Tanpa `StatsView`: dipakai monitor saat window disembunyikan, supaya
/// string/array untuk UI tidak dibentuk tiap detik.
pub(crate) fn collect_stats_raw() -> SystemStats {
    collect_stats_inner(false)
}

fn collect_stats_inner(include_view: bool) -> SystemStats {
    let mut sys_guard = MONITOR_SYS.lock().unwrap();
    let sys = sys_guard.get_or_insert_with(System::new);
    sys.refresh_cpu_all();
    sys.refresh_memory();

    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs_f64();

    let cpus: Vec<f32> = sys.cpus().iter().map(|c| c.cpu_usage()).collect();
    let cpu_percent = if cpus.is_empty() {
        0.0
    } else {
        cpus.iter().sum::<f32>() / cpus.len() as f32
    };
    let cpu_freq = sys
        .cpus()
        .first()
        .map(|c| c.frequency() as f32)
        .unwrap_or(0.0);
    let cpu_temp = read_cpu_temp();

    let mem_used = sys.used_memory() as f64 / 1e9;
    let mem_total = sys.total_memory() as f64 / 1e9;
    let mem_available = sys.available_memory() as f64 / 1e9;
    let mem_free = sys.free_memory() as f64 / 1e9;
    let mem_cached = read_meminfo_kb(&["Cached", "SReclaimable", "Buffers"]) * 1024.0 / 1e9;
    let ram_percent = if mem_total > 0.0 {
        (mem_used / mem_total * 100.0) as f32
    } else {
        0.0
    };
    let swap_used = sys.used_swap() as f64 / 1e9;
    let swap_total = sys.total_swap() as f64 / 1e9;

    let disk_mounts = {
        let mut cache = MOUNTS_CACHE.lock().unwrap();
        let fresh = cache
            .as_ref()
            .map(|(cached_at, _)| timestamp - cached_at < 10.0)
            .unwrap_or(false);
        if !fresh {
            *cache = Some((timestamp, read_partitions()));
        }
        cache.as_ref().unwrap().1.clone()
    };
    let (disk_read, disk_write, disk_busy) = disk_io_delta();
    let nvidia = read_nvidia();
    let gpu_temp = nvidia
        .3
        .or_else(|| read_gpu_temp_sysfs().map(|v| v.round() as u32));
    let igpu_util = read_igpu_util();
    let (fan1_opt, fan2_opt) = read_fan_rpms();
    let fan1 = fan1_opt.unwrap_or(0);
    let fan2 = fan2_opt.unwrap_or(0);
    let (plugged, bat_pct) = read_power_supply();
    let (bat_health, bat_cycles, bat_status, bat_power, bat_energy_now, bat_energy_full) =
        read_battery_details();
    let (net_rx, net_tx, iface, ip, wifi) = read_net_dev();
    let ssd_temp = read_ssd_temp();
    let ram_temp = read_named_hwmon_temp("spd5118", None);
    let wifi_temp = read_named_hwmon_temp("iwlwifi", None);
    let board_temp = read_board_temp();
    let gpu_source = if nvidia.3.is_some() {
        "nvidia-smi"
    } else {
        "hwmon/nvidia"
    };
    let thermal = if include_view {
        read_thermal_sensors(
            cpu_temp, gpu_temp, ssd_temp, ram_temp, wifi_temp, board_temp, gpu_source,
        )
    } else {
        Vec::new()
    };

    // Push ke histori metrik untuk grafik 1m/15m/1h/24h. Cukup 1 titik/5 detik;
    // data 1 detik penuh tidak terlihat di grafik dan memakan RAM besar.
    let history_due = {
        let mut last = LAST_HISTORY_PUSH.lock().unwrap();
        if timestamp - *last >= 5.0 {
            *last = timestamp;
            true
        } else {
            false
        }
    };
    if history_due {
        history::history_push("cpu_percent", cpu_percent as f64);
        history::history_push("ram_percent", ram_percent as f64);
        if let Some(v) = cpu_temp {
            history::history_push("cpu_temp", v as f64);
        }
        if let Some(v) = gpu_temp {
            history::history_push("gpu_temp", v as f64);
        }
        if let Some(v) = ssd_temp {
            history::history_push("ssd_temp", v as f64);
        }
        if let Some(v) = ram_temp {
            history::history_push("ram_temp", v as f64);
        }
        if let Some(v) = wifi_temp {
            history::history_push("wifi_temp", v as f64);
        }
        if let Some(v) = fan1_opt {
            history::history_push("fan1_rpm", v as f64);
        }
        history::history_push("disk_read_kbps", disk_read);
        history::history_push("disk_write_kbps", disk_write);
        if let Some(v) = nvidia.0 {
            history::history_push("gpu_util", v as f64);
        }
        history::history_push("net_kbps", net_rx + net_tx);
        if let Some(p) = bat_pct {
            history::history_push("battery_percent", p as f64);
        }
        if let Some(pw) = nvidia.4 {
            history::history_push("gpu_power_w", pw);
        }
    }

    let mut stats = SystemStats {
        timestamp,
        cpu_percent,
        cpu_per_core: cpus,
        cpu_freq_mhz: cpu_freq,
        cpu_temp,
        ram_used_gb: mem_used,
        ram_total_gb: mem_total,
        ram_available_gb: mem_available,
        ram_free_gb: mem_free,
        ram_cached_gb: mem_cached,
        ram_percent,
        swap_used_gb: swap_used,
        swap_total_gb: swap_total,
        disk_mounts,
        disk_read_kbps: disk_read,
        disk_write_kbps: disk_write,
        disk_busy_percent: disk_busy,
        gpu_util: nvidia.0,
        gpu_vram_used_mb: nvidia.1,
        gpu_vram_total_mb: nvidia.2,
        gpu_temp,
        gpu_freq_mhz: nvidia.6,
        gpu_power_w: nvidia.4,
        gpu_fan_pct: nvidia.5,
        igpu_util,
        fan1_rpm: fan1,
        fan2_rpm: fan2,
        fan1_available: fan1_opt.is_some(),
        fan2_available: fan2_opt.is_some(),
        ssd_temp,
        ram_temp,
        wifi_temp,
        board_temp,
        thermal,
        net_rx_kbps: net_rx,
        net_tx_kbps: net_tx,
        net_interface: iface,
        net_ipv4: ip,
        net_is_wifi: wifi,
        power_plugged: plugged,
        battery_percent: bat_pct,
        battery_health_pct: bat_health,
        battery_cycles: bat_cycles,
        battery_status: bat_status,
        battery_power_w: bat_power,
        battery_energy_now_wh: bat_energy_now,
        battery_energy_full_wh: bat_energy_full,
        view: StatsView::default(),
    };
    if include_view {
        stats.view = build_stats_view(&stats);
    }
    stats
}

fn fmt_num(value: f64, digits: usize) -> String {
    if !value.is_finite() {
        return "--".into();
    }
    format!("{value:.digits$}")
}

fn fmt_opt_u32(value: Option<u32>) -> String {
    value.map(|v| v.to_string()).unwrap_or_else(|| "--".into())
}

fn bar_view(value: f64, hot_at: f64) -> BarView {
    let value = if value.is_finite() { value } else { 0.0 };
    BarView {
        scale: (value / 100.0).clamp(0.0, 1.0),
        hot: value >= hot_at,
    }
}

fn spark_points(values: &[f64], max: f64) -> String {
    let max = if max.is_finite() && max > 0.0 {
        max
    } else {
        1.0
    };
    values
        .iter()
        .enumerate()
        .map(|(index, value)| {
            let x = index as f64 * 100.0 / 59.0;
            let y = 28.0 - (value.clamp(0.0, max) / max) * 26.0;
            format!("{x:.2},{y:.2}")
        })
        .collect::<Vec<_>>()
        .join(" ")
}

fn spark_values(metric: &str, current: f64) -> Vec<f64> {
    let mut values = history::history_last(metric, 60);
    if values.is_empty() {
        values.push(current);
    }
    values
}

fn build_stats_view(stats: &SystemStats) -> StatsView {
    let cpu_temp = stats
        .cpu_temp
        .filter(|value| *value > 0.0)
        .map(|value| fmt_num(value as f64, 1))
        .map(|value| format!("{value} °C"))
        .unwrap_or_else(|| "--".into());
    let gpu_temp = stats
        .gpu_temp
        .map(|value| format!("{value} °C"))
        .unwrap_or_else(|| "--".into());
    let gpu_percent = stats
        .gpu_util
        .map(|value| value.to_string())
        .unwrap_or_else(|| "--".into());
    let gpu_freq = stats
        .gpu_freq_mhz
        .map(|value| format!("{value} MHz"))
        .unwrap_or_else(|| "--".into());
    let battery_time = battery_time_label(stats);
    let net_status = if stats.net_interface.is_empty() || stats.net_ipv4.is_empty() {
        "Tidak terhubung".to_string()
    } else if stats.net_is_wifi {
        format!("Wi-Fi · {}", stats.net_interface)
    } else {
        format!("Ethernet · {}", stats.net_interface)
    };
    let fan_rpm = match (stats.fan1_available, stats.fan2_available) {
        (false, false) => "N/A".to_string(),
        (true, true) => format!("{} / {} RPM", stats.fan1_rpm, stats.fan2_rpm),
        (true, false) => format!("{} RPM", stats.fan1_rpm),
        (false, true) => format!("{} RPM", stats.fan2_rpm),
    };
    let gpu_vram = match (stats.gpu_vram_used_mb, stats.gpu_vram_total_mb) {
        (Some(used), Some(total)) => format!("{used} / {total} MB"),
        _ => "--".into(),
    };
    let gpu_fan = stats
        .gpu_fan_pct
        .map(|value| format!("{value} %"))
        .unwrap_or_else(|| "--".into());
    let battery_health = stats
        .battery_health_pct
        .map(|value| format!("{} %", fmt_num(value as f64, 0)))
        .unwrap_or_else(|| "--".into());
    let battery = stats
        .battery_percent
        .map(|value| format!("{} %", fmt_num(value as f64, 0)))
        .unwrap_or_else(|| "--".into());
    let ac_state = match stats.power_plugged {
        Some(true) => "Plugged".into(),
        Some(false) => "On battery".into(),
        None => "--".into(),
    };
    let cpu_history = spark_values("cpu_percent", stats.cpu_percent as f64);
    let ram_history = spark_values("ram_percent", stats.ram_percent as f64);
    let gpu_history = spark_values("gpu_util", stats.gpu_util.unwrap_or(0) as f64);
    let net_total = stats.net_rx_kbps + stats.net_tx_kbps;
    let net_history = spark_values("net_kbps", net_total);
    let net_peak = net_history.iter().copied().fold(100.0, f64::max);
    let mounts: Vec<MountView> = stats
        .disk_mounts
        .iter()
        .map(|mount| MountView {
            mount: mount.mount.clone(),
            usage: format!(
                "{} / {} GB",
                fmt_num(mount.used_gb, 1),
                fmt_num(mount.total_gb, 1)
            ),
            scale: (mount.percent / 100.0).clamp(0.0, 1.0),
        })
        .collect();

    StatsView {
        live: LiveStats {
            cpu: format!("{}%", fmt_num(stats.cpu_percent as f64, 0)),
            gpu: stats
                .gpu_util
                .map(|value| format!("{value}%"))
                .unwrap_or_else(|| "--".into()),
            temp: stats
                .cpu_temp
                .filter(|value| *value > 0.0)
                .map(|value| format!("{}°C", fmt_num(value as f64, 0)))
                .unwrap_or_else(|| "--".into()),
            battery: stats
                .battery_percent
                .map(|value| format!("{}%", fmt_num(value as f64, 0)))
                .unwrap_or_else(|| "--".into()),
        },
        cpu_percent: fmt_num(stats.cpu_percent as f64, 0),
        cpu_freq: format!("{} MHz", fmt_num(stats.cpu_freq_mhz as f64, 0)),
        cpu_temp,
        ram_percent: fmt_num(stats.ram_percent as f64, 0),
        ram_used: format!("{} GB", fmt_num(stats.ram_used_gb, 1)),
        ram_total: format!("{} GB", fmt_num(stats.ram_total_gb, 1)),
        swap: format!(
            "{} / {} GB",
            fmt_num(stats.swap_used_gb, 1),
            fmt_num(stats.swap_total_gb, 1)
        ),
        gpu_percent,
        gpu_temp,
        gpu_freq,
        gpu_power: stats
            .gpu_power_w
            .map(|value| format!("{} W", fmt_num(value, 1)))
            .unwrap_or_else(|| "--".into()),
        gpu_vram,
        gpu_fan,
        igpu_percent: stats
            .igpu_util
            .map(|value| fmt_num(value as f64, 0))
            .unwrap_or_else(|| "--".into()),
        battery_health,
        battery_cycles: fmt_opt_u32(stats.battery_cycles),
        battery_status: if stats.battery_status.is_empty() {
            "--".into()
        } else {
            stats.battery_status.clone()
        },
        battery_power: stats
            .battery_power_w
            .map(|value| format!("{} W", fmt_num(value, 1)))
            .unwrap_or_else(|| "--".into()),
        fan_rpm,
        battery,
        battery_time,
        ac_state,
        net_status,
        net_interface: if stats.net_interface.is_empty() {
            "--".into()
        } else {
            stats.net_interface.clone()
        },
        net_ip: if stats.net_ipv4.is_empty() {
            "--".into()
        } else {
            stats.net_ipv4.clone()
        },
        net_rx: format!("{} KB/s", fmt_num(stats.net_rx_kbps, 0)),
        net_tx: format!("{} KB/s", fmt_num(stats.net_tx_kbps, 0)),
        disk_read: format!("{} KB/s", fmt_num(stats.disk_read_kbps, 0)),
        disk_write: format!("{} KB/s", fmt_num(stats.disk_write_kbps, 0)),
        bars: StatsBars {
            cpu: bar_view(stats.cpu_percent as f64, 88.0),
            ram: bar_view(stats.ram_percent as f64, 90.0),
            gpu: bar_view(stats.gpu_util.unwrap_or(0) as f64, 88.0),
            igpu: bar_view(stats.igpu_util.unwrap_or(0_f32) as f64, 88.0),
            mem: bar_view(stats.ram_percent as f64, 90.0),
        },
        sparks: Sparklines {
            cpu: spark_points(&cpu_history, 100.0),
            ram: spark_points(&ram_history, 100.0),
            gpu: spark_points(&gpu_history, 100.0),
            net: spark_points(&net_history, net_peak),
        },
        cores: stats
            .cpu_per_core
            .iter()
            .enumerate()
            .map(|(index, value)| CoreView {
                label: format!("CPU{index}"),
                percent_label: format!("{}%", fmt_num(*value as f64, 0)),
                scale: (*value as f64 / 100.0).clamp(0.0, 1.0),
            })
            .collect(),
        mounts,
        mount_key: stats
            .disk_mounts
            .iter()
            .map(|mount| mount.mount.as_str())
            .collect::<Vec<_>>()
            .join("|"),
    }
}

fn hwmon_index() -> Vec<(String, PathBuf)> {
    const TTL_SECONDS: f64 = 10.0;
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs_f64();
    {
        let cache = HWMON_CACHE.lock().unwrap();
        if let Some((cached_at, entries)) = cache.as_ref() {
            if now - *cached_at < TTL_SECONDS {
                return entries.clone();
            }
        }
    }

    let mut entries = Vec::new();
    if let Ok(read) = fs::read_dir("/sys/class/hwmon") {
        for entry in read.flatten() {
            let path = entry.path();
            let Ok(name) = fs::read_to_string(path.join("name")) else {
                continue;
            };
            let name = name.trim().to_lowercase();
            if !name.is_empty() {
                entries.push((name, path));
            }
        }
    }
    *HWMON_CACHE.lock().unwrap() = Some((now, entries.clone()));
    entries
}

fn hwmon_paths(name: &str) -> Vec<PathBuf> {
    let needle = name.to_lowercase();
    hwmon_index()
        .into_iter()
        .filter(|(entry_name, _)| entry_name.contains(&needle))
        .map(|(_, path)| path)
        .collect()
}

fn read_named_hwmon_temp(name: &str, label: Option<&str>) -> Option<f32> {
    for path in hwmon_paths(name) {
        let mut best: Option<f32> = None;
        for i in 1..=8 {
            let temp_f = path.join(format!("temp{i}_input"));
            let Ok(raw) = fs::read_to_string(&temp_f) else {
                continue;
            };
            let Ok(value) = raw.trim().parse::<f32>() else {
                continue;
            };
            if value <= 0.0 {
                continue;
            }
            if let Some(want) = label {
                let actual =
                    fs::read_to_string(path.join(format!("temp{i}_label"))).unwrap_or_default();
                if !actual.trim().to_lowercase().contains(&want.to_lowercase()) {
                    continue;
                }
            }
            let celsius = value / 1000.0;
            best = Some(best.map_or(celsius, |old| old.max(celsius)));
        }
        if best.is_some() {
            return best;
        }
    }
    None
}

fn read_thermal_zone(name: &str) -> Option<f32> {
    let entries = fs::read_dir("/sys/class/thermal").ok()?;
    for entry in entries.flatten() {
        let path = entry.path();
        let kind = fs::read_to_string(path.join("type")).unwrap_or_default();
        if kind.trim().eq_ignore_ascii_case(name) {
            return fs::read_to_string(path.join("temp"))
                .ok()
                .and_then(|v| v.trim().parse::<f32>().ok())
                .map(|v| v / 1000.0);
        }
    }
    None
}

pub(crate) fn read_cpu_temp() -> Option<f32> {
    read_named_hwmon_temp("coretemp", Some("Package id 0"))
        .or_else(|| read_named_hwmon_temp("coretemp", None))
        .or_else(|| read_thermal_zone("x86_pkg_temp"))
        .or_else(|| read_thermal_zone("TCPU"))
        .or_else(|| read_thermal_zone("cpu_thermal"))
}

fn read_ssd_temp() -> Option<f32> {
    read_named_hwmon_temp("nvme", Some("Composite")).or_else(|| read_named_hwmon_temp("nvme", None))
}

fn read_board_temp() -> Option<f32> {
    let entries = fs::read_dir("/sys/class/thermal").ok()?;
    let mut values = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        let kind = fs::read_to_string(path.join("type")).unwrap_or_default();
        let kind = kind.trim();
        if kind.starts_with("SEN") {
            if let Ok(raw) = fs::read_to_string(path.join("temp")) {
                if let Ok(v) = raw.trim().parse::<f32>() {
                    if v > 0.0 {
                        values.push(v / 1000.0);
                    }
                }
            }
        }
    }
    values.into_iter().reduce(f32::max)
}

fn read_thermal_sensors(
    cpu: Option<f32>,
    gpu: Option<u32>,
    ssd: Option<f32>,
    ram: Option<f32>,
    wifi: Option<f32>,
    board: Option<f32>,
    gpu_source: &str,
) -> Vec<ThermalSensor> {
    let sensor = |id: &str, label: &str, temp_c: Option<f32>, source: &str| ThermalSensor {
        id: id.into(),
        label: label.into(),
        temp_c,
        source: source.into(),
    };
    vec![
        sensor("cpu", "CPU Package", cpu, "hwmon/coretemp"),
        sensor("gpu", "GPU", gpu.map(|v| v as f32), gpu_source),
        sensor("ssd", "NVMe", ssd, "hwmon/nvme"),
        sensor("ram", "RAM", ram, "hwmon/spd5118"),
        sensor("wifi", "WiFi", wifi, "hwmon/iwlwifi"),
        sensor("board", "EC / Board", board, "thermal/EC"),
    ]
}

pub(crate) fn read_gpu_temp_sysfs() -> Option<f32> {
    read_named_hwmon_temp("nvidia", None)
}

fn read_meminfo_kb(keys: &[&str]) -> f64 {
    let Ok(text) = fs::read_to_string("/proc/meminfo") else {
        return 0.0;
    };
    let mut total = 0.0;
    for line in text.lines() {
        let mut parts = line.split_whitespace();
        let Some(key) = parts.next() else { continue };
        let key = key.trim_end_matches(':');
        if keys.contains(&key) {
            total += parts
                .next()
                .and_then(|v| v.parse::<f64>().ok())
                .unwrap_or(0.0);
        }
    }
    total
}

fn battery_time_label(stats: &SystemStats) -> String {
    let Some(power) = stats.battery_power_w.filter(|v| *v > 0.5) else {
        return "--".into();
    };
    let now = stats
        .battery_energy_now_wh
        .or_else(|| Some(stats.battery_energy_full_wh? * stats.battery_percent? as f64 / 100.0));
    let target = match stats.power_plugged {
        Some(true) => match (stats.battery_energy_full_wh, now) {
            (Some(full), Some(now)) => Some((full - now).max(0.0)),
            _ => stats.battery_energy_full_wh,
        },
        _ => now,
    };
    let Some(wh) = target else {
        return "--".into();
    };
    let hours = wh / power;
    if !hours.is_finite() || hours <= 0.0 {
        return "--".into();
    }
    let minutes = (hours * 60.0).round() as u64;
    if minutes >= 60 {
        format!("{} jam {} menit", minutes / 60, minutes % 60)
    } else {
        format!("{minutes} menit")
    }
}

fn read_df() -> HashMap<String, (f64, f64, f64)> {
    // target -> (used_gb, total_gb, percent)
    let mut out = HashMap::new();
    let output = match Command::new("df")
        .args(["--output=source,used,size,pcent,target", "-B1"])
        .output()
    {
        Ok(o) => String::from_utf8_lossy(&o.stdout).to_string(),
        Err(_) => return out,
    };
    for line in output.lines().skip(1) {
        let mut fields = line.split_whitespace();
        let source = fields.next().unwrap_or("");
        let used = fields
            .next()
            .and_then(|s| s.parse::<f64>().ok())
            .unwrap_or(0.0);
        let total = fields
            .next()
            .and_then(|s| s.parse::<f64>().ok())
            .unwrap_or(0.0);
        let pcent = fields
            .next()
            .and_then(|s| s.trim_end_matches('%').parse::<f64>().ok())
            .unwrap_or(0.0);
        let target = fields.next().unwrap_or("");
        if !target.is_empty() && !source.starts_with("tmpfs") && !source.starts_with("devtmpfs") {
            out.insert(target.to_string(), (used / 1e9, total / 1e9, pcent));
        }
    }
    out
}

fn read_partitions() -> Vec<DiskMount> {
    let df = read_df();
    let ok = Command::new("lsblk")
        .args([
            "-J",
            "-b",
            "-o",
            "NAME,SIZE,FSTYPE,MOUNTPOINTS,LABEL,MODEL,TYPE",
        ])
        .output()
        .ok()
        .and_then(|o| o.status.success().then_some(o.stdout));
    let stdout = match ok {
        Some(s) => String::from_utf8_lossy(&s).to_string(),
        None => return Vec::new(),
    };
    let data: serde_json::Value = match serde_json::from_str(&stdout) {
        Ok(v) => v,
        Err(_) => return Vec::new(),
    };
    let mut out = Vec::new();
    let mut seen = HashMap::new();
    let blockdevices: Vec<&serde_json::Value> = data
        .get("blockdevices")
        .and_then(|v| v.as_array())
        .map(|a| a.iter().collect())
        .unwrap_or_default();
    for dev in blockdevices {
        let children: Vec<&serde_json::Value> = dev
            .get("children")
            .and_then(|v| v.as_array())
            .map(|a| a.iter().collect())
            .unwrap_or_else(|| vec![dev]);
        for child in children {
            let name = child.get("name").and_then(|v| v.as_str()).unwrap_or("");
            let fstype = child.get("fstype").and_then(|v| v.as_str()).unwrap_or("");
            if name.is_empty()
                || matches!(
                    fstype,
                    "swap" | "squashfs" | "tmpfs" | "devtmpfs" | "overlay"
                )
            {
                continue;
            }
            let size_bytes = child.get("size").and_then(|v| v.as_u64()).unwrap_or(0);
            if size_bytes < 32 * 1024 * 1024 && fstype.is_empty() {
                continue;
            }
            let dev_path = format!("/dev/{name}");
            if seen.insert(dev_path.clone(), true).is_some() {
                continue;
            }
            let fstype_str = if fstype.is_empty() {
                "Partition".to_string()
            } else {
                fstype.to_string()
            };
            let label = child
                .get("label")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let mountpoints: Vec<String> = child
                .get("mountpoints")
                .and_then(|v| v.as_array())
                .map(|arr| {
                    arr.iter()
                        .filter_map(|v| v.as_str().map(|s| s.to_string()))
                        .collect()
                })
                .unwrap_or_default();
            let mount_path = if mountpoints.contains(&"/".to_string()) {
                "/".to_string()
            } else if let Some(m) = mountpoints.first() {
                m.clone()
            } else if !label.is_empty() {
                format!("Unmounted ({label})")
            } else {
                "Unmounted".to_string()
            };
            let (mut used_gb, mut total_gb, mut percent) = (0.0, size_bytes as f64 / 1e9, 0.0);
            for mp in &mountpoints {
                if let Some((u, t, p)) = df.get(mp) {
                    used_gb = *u;
                    total_gb = *t;
                    percent = *p;
                    break;
                }
            }
            out.push(DiskMount {
                mount: mount_path,
                used_gb,
                total_gb,
                percent,
                device: dev_path,
                fstype: fstype_str,
                label,
            });
        }
    }
    out
}

fn disk_io_delta() -> (f64, f64, f64) {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs_f64();
    let (read_b, write_b, io_ticks) = read_diskstats_bytes();
    let (r, w, b) = {
        let mut last = LAST_DISK.lock().unwrap();
        let mut last_time = LAST_DISK_TIME.lock().unwrap();
        let prev = *last;
        let dt = (now - *last_time.get_or_insert(now)).max(0.1);
        *last = Some((read_b, write_b, io_ticks));
        *last_time = Some(now);
        match prev {
            Some((pr, pw, pt)) => (
                read_b.saturating_sub(pr) as f64 / 1024.0 / dt,
                write_b.saturating_sub(pw) as f64 / 1024.0 / dt,
                (io_ticks.saturating_sub(pt) as f64 / (dt * 1000.0) * 100.0).clamp(0.0, 100.0),
            ),
            None => (0.0, 0.0, 0.0),
        }
    };
    (r, w, b)
}

fn read_diskstats_bytes() -> (u64, u64, u64) {
    let Ok(text) = fs::read_to_string("/proc/diskstats") else {
        return (0, 0, 0);
    };
    let mut read_sectors = 0u64;
    let mut write_sectors = 0u64;
    let mut io_ticks = 0u64;
    for line in text.lines() {
        let fields: Vec<&str> = line.split_whitespace().collect();
        if fields.len() < 10 {
            continue;
        }
        let name = fields[2];
        // Whole physical disks only: partitions would double count, and dm/loop/zram
        // mirror traffic that is already reported by the underlying device.
        if name.starts_with("loop")
            || name.starts_with("zram")
            || name.starts_with("ram")
            || name.starts_with("dm-")
            || Path::new("/sys/class/block")
                .join(name)
                .join("partition")
                .exists()
        {
            continue;
        }
        read_sectors += fields[5].parse::<u64>().unwrap_or(0);
        write_sectors += fields[9].parse::<u64>().unwrap_or(0);
        io_ticks += fields[12].parse::<u64>().unwrap_or(0);
    }
    (read_sectors * 512, write_sectors * 512, io_ticks)
}

type NvidiaReadings = (
    Option<u32>,
    Option<u64>,
    Option<u64>,
    Option<u32>,
    Option<f64>,
    Option<u32>,
    Option<u32>,
);

const NVIDIA_NONE: NvidiaReadings = (None, None, None, None, None, None, None);

pub(crate) fn nvidia_driver_present() -> bool {
    Path::new("/dev/nvidia0").exists()
        || Path::new("/dev/nvidiactl").exists()
        || Path::new("/proc/driver/nvidia/version").exists()
}

pub(crate) fn read_nvidia() -> NvidiaReadings {
    let readings = read_nvidia_uncached();
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs_f64();
    *NVIDIA_CACHE.lock().unwrap() = Some((now, readings));
    readings
}

/// Reuse reading terakhir dari monitor loop (mis. untuk tab RAM) supaya
/// `nvidia-smi` tidak dijalankan dua kali dalam beberapa detik.
pub(crate) fn read_nvidia_cached(max_age_seconds: f64) -> NvidiaReadings {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs_f64();
    if let Some((cached_at, readings)) = *NVIDIA_CACHE.lock().unwrap() {
        if now - cached_at <= max_age_seconds {
            return readings;
        }
    }
    read_nvidia()
}

fn read_nvidia_uncached() -> NvidiaReadings {
    // Driver tidak loaded / GPU tidak ada: jangan spawn nvidia-smi tiap detik.
    if !nvidia_driver_present() {
        return NVIDIA_NONE;
    }
    let out = match Command::new("nvidia-smi")
        .args([
            "--query-gpu=utilization.gpu,memory.used,memory.total,temperature.gpu,power.draw,fan.speed,clocks.gr",
            "--format=csv,noheader,nounits",
        ])
        .output()
    {
        Ok(o) if o.status.success() => String::from_utf8_lossy(&o.stdout).trim().to_string(),
        _ => return NVIDIA_NONE,
    };
    if out.is_empty() {
        return NVIDIA_NONE;
    }
    let parts: Vec<&str> = out.split(',').map(|s| s.trim()).collect();
    if parts.len() < 7 {
        return NVIDIA_NONE;
    }
    let util = parts[0].parse::<u32>().ok();
    let mem_used = parts[1].parse::<f64>().ok();
    let mem_total = parts[2].parse::<f64>().ok();
    let temp = parts[3].parse::<u32>().ok();
    let power = parts[4].parse::<f64>().ok();
    let fan = parts[5].parse::<u32>().ok();
    let freq = parts[6].parse::<u32>().ok();
    (
        util,
        mem_used.map(|m| m as u64),
        mem_total.map(|m| m as u64),
        temp,
        power,
        fan,
        freq,
    )
}

fn read_igpu_util() -> Option<f32> {
    let entries = fs::read_dir("/sys/class/drm").ok()?;
    for entry in entries.flatten() {
        let p = entry.path();
        let act = p.join("gt_act_freq_mhz");
        let act = if act.exists() {
            act
        } else {
            p.join("gt/gt0/rps_act_freq_mhz")
        };
        let act = if act.exists() {
            act
        } else {
            p.join("gt_cur_freq_mhz")
        };
        let max = p.join("gt_max_freq_mhz");
        let max = if max.exists() {
            max
        } else {
            p.join("gt/gt0/rps_max_freq_mhz")
        };
        let min = p.join("gt_min_freq_mhz");
        let min = if min.exists() {
            min
        } else {
            p.join("gt/gt0/rps_min_freq_mhz")
        };
        if act.exists() && max.exists() {
            if let (Ok(act), Ok(mx), Ok(mn)) = (
                fs::read_to_string(&act),
                fs::read_to_string(&max),
                fs::read_to_string(&min),
            ) {
                if let (Ok(act), Ok(mx), Ok(mn)) = (
                    act.trim().parse::<f32>(),
                    mx.trim().parse::<f32>(),
                    mn.trim().parse::<f32>(),
                ) {
                    if mx > mn {
                        return Some(((act - mn) / (mx - mn) * 100.0).clamp(0.0, 100.0));
                    }
                }
            }
        }
    }
    None
}

pub(crate) fn read_fan_rpms() -> (Option<u32>, Option<u32>) {
    let mut fan1 = None;
    let mut fan2 = None;
    for (name, path) in hwmon_index() {
        if !matches!(
            name.as_str(),
            "legion_laptop" | "lenovo_wmi_fan" | "lenovo_wmi_other" | "ideapad" | "yogafan"
        ) {
            continue;
        }
        for (slot, file) in [(&mut fan1, "fan1_input"), (&mut fan2, "fan2_input")] {
            if slot.is_some() {
                continue;
            }
            if let Ok(raw) = fs::read_to_string(path.join(file)) {
                if let Ok(value) = raw.trim().parse::<u32>() {
                    *slot = Some(value);
                }
            }
        }
    }
    (fan1, fan2)
}

pub(crate) fn find_hwmon() -> Option<PathBuf> {
    for name in [
        "legion_laptop",
        "lenovo_wmi_fan",
        "lenovo_wmi_other",
        "ideapad",
        "yogafan",
    ] {
        let paths = hwmon_paths(name);
        if let Some(path) = paths.into_iter().find(|p| p.join("pwm1").exists()) {
            return Some(path);
        }
        if name == "yogafan" {
            if let Some(path) = hwmon_paths(name).into_iter().next() {
                return Some(path);
            }
        }
    }
    None
}

fn read_power_supply() -> (Option<bool>, Option<f32>) {
    let mut plugged = None;
    let mut pct = None;
    if let Ok(entries) = fs::read_dir("/sys/class/power_supply") {
        for entry in entries.flatten() {
            let path = entry.path();
            let name = path
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_default();
            let type_f = path.join("type");
            let online_f = path.join("online");
            if type_f.exists() && online_f.exists() {
                if let (Ok(t), Ok(o)) = (fs::read_to_string(&type_f), fs::read_to_string(&online_f))
                {
                    if t.trim().to_lowercase() == "mains" || t.trim().to_lowercase() == "ac" {
                        if let Ok(v) = o.trim().parse::<i32>() {
                            plugged = Some(v == 1);
                        }
                    }
                }
            }
            if name.starts_with("BAT") {
                let cap = path.join("capacity");
                if let Ok(v) = fs::read_to_string(&cap) {
                    if let Ok(p) = v.trim().parse::<f32>() {
                        pct = Some(p);
                    }
                }
            }
        }
    }
    (plugged, pct)
}

fn read_battery_details() -> (
    Option<f32>,
    Option<u32>,
    String,
    Option<f64>,
    Option<f64>,
    Option<f64>,
) {
    let Ok(entries) = fs::read_dir("/sys/class/power_supply") else {
        return (None, None, String::new(), None, None, None);
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let name = path
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_default();
        if !name.starts_with("BAT") {
            continue;
        }
        let read_u64 = |file: &str| {
            fs::read_to_string(path.join(file))
                .ok()
                .and_then(|v| v.trim().parse::<u64>().ok())
        };
        let energy_full = read_u64("energy_full").or_else(|| read_u64("charge_full"));
        let energy_design =
            read_u64("energy_full_design").or_else(|| read_u64("charge_full_design"));
        let health = match (energy_full, energy_design) {
            (Some(full), Some(design)) if design > 0 => {
                Some((full as f32 / design as f32 * 100.0).clamp(0.0, 100.0))
            }
            _ => None,
        };
        let cycles = read_u64("cycle_count").map(|v| v as u32);
        let status = fs::read_to_string(path.join("status"))
            .map(|v| v.trim().to_string())
            .unwrap_or_default();
        let power = read_u64("power_now").map(|v| v as f64 / 1e6).or_else(|| {
            let current = read_u64("current_now")?;
            let voltage = read_u64("voltage_now")?;
            Some(current as f64 * voltage as f64 / 1e12)
        });
        let energy_now = read_u64("energy_now").map(|v| v as f64 / 1e6);
        let energy_full = energy_full.map(|v| v as f64 / 1e6);
        return (health, cycles, status, power, energy_now, energy_full);
    }
    (None, None, String::new(), None, None, None)
}

fn default_iface() -> Option<String> {
    let text = fs::read_to_string("/proc/net/route").ok()?;
    for line in text.lines().skip(1) {
        let fields: Vec<&str> = line.split_whitespace().collect();
        if fields.len() > 2 && fields[1] == "00000000" && fields[2] == "00000000" {
            return Some(fields[0].to_string());
        }
    }
    None
}

fn read_net_dev() -> (f64, f64, String, String, bool) {
    let default = default_iface();
    let mut best_iface = String::new();
    let mut best_rx = 0u64;
    let mut best_tx = 0u64;
    let mut is_wifi = false;
    // /proc/net/dev fields: iface, bytes_recv, ..., bytes_sent, ...
    if let Ok(text) = fs::read_to_string("/proc/net/dev") {
        for line in text.lines().skip(2) {
            let trimmed = line.trim_start();
            if let Some(colon) = trimmed.find(':') {
                let iface = trimmed[..colon].trim();
                if iface == "lo" {
                    continue;
                }
                let rest = trimmed[colon + 1..]
                    .split_whitespace()
                    .collect::<Vec<&str>>();
                if rest.len() < 10 {
                    continue;
                }
                let rx: u64 = rest[0].parse().unwrap_or(0);
                let tx: u64 = rest[8].parse().unwrap_or(0);
                let wifi =
                    iface.contains("wlan") || iface.contains("wlp") || iface.contains("wifi");
                let preferred = default.as_deref() == Some(iface);
                let current_preferred = default.as_deref() == Some(best_iface.as_str());
                if best_iface.is_empty()
                    || (preferred && !current_preferred)
                    || (wifi && !is_wifi && !current_preferred)
                {
                    best_iface = iface.to_string();
                    best_rx = rx;
                    best_tx = tx;
                    is_wifi = wifi;
                }
            }
        }
    }
    let ip = read_ipv4(&best_iface);
    let (rx, tx) = {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs_f64();
        let mut last = LAST_NET.lock().unwrap();
        if last.as_ref().map(|l| l.0.clone()) != Some(best_iface.clone()) {
            *last = Some((best_iface.clone(), best_rx, best_tx, now));
            (0.0, 0.0)
        } else {
            let l = last.as_mut().unwrap();
            let dt = (now - l.3).max(0.1);
            let dr = best_rx.saturating_sub(l.1) as f64 / 1024.0 / dt;
            let dw = best_tx.saturating_sub(l.2) as f64 / 1024.0 / dt;
            l.1 = best_rx;
            l.2 = best_tx;
            l.3 = now;
            (dr.max(0.0), dw.max(0.0))
        }
    };
    (rx, tx, best_iface, ip, is_wifi)
}

fn read_ipv4(iface: &str) -> String {
    if iface.is_empty() {
        return String::new();
    }
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs_f64();
    {
        let cache = IP_CACHE.lock().unwrap();
        if let Some((cached_iface, addr, cached_at)) = cache.as_ref() {
            if cached_iface == iface && now - *cached_at < 5.0 {
                return addr.clone();
            }
        }
    }
    let addr = read_ipv4_uncached(iface);
    *IP_CACHE.lock().unwrap() = Some((iface.to_string(), addr.clone(), now));
    addr
}

fn read_ipv4_uncached(iface: &str) -> String {
    let out = match Command::new("ip")
        .args(["-j", "-br", "-f", "inet", "addr", "show", iface])
        .output()
    {
        Ok(o) => String::from_utf8_lossy(&o.stdout).to_string(),
        Err(_) => return String::new(),
    };
    let arr: Vec<serde_json::Value> = match serde_json::from_str(&out) {
        Ok(v) => v,
        Err(_) => return String::new(),
    };
    for entry in arr {
        if let Some(infos) = entry.get("addr_info").and_then(|v| v.as_array()) {
            for info in infos {
                if let Some(local) = info.get("local").and_then(|v| v.as_str()) {
                    return local.to_string();
                }
            }
        }
    }
    String::new()
}
