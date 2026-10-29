use serde::{Deserialize, Serialize};
use std::fs;
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

use crate::run_helper_quiet;

static DIMM_CACHE: Mutex<Option<(f64, Vec<DimmInfo>)>> = Mutex::new(None);

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DimmInfo {
    pub locator: String,
    pub size: String,
    pub speed: String,
    pub mem_type: String,
    pub manufacturer: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct MemoryInfo {
    pub dimms: Vec<DimmInfo>,
    pub source: String,
    pub total_gb: f64,
    pub used_gb: f64,
    pub available_gb: f64,
    pub free_gb: f64,
    pub cached_gb: f64,
    pub swap_used_gb: f64,
    pub swap_total_gb: f64,
    pub pressure_some: Option<f64>,
    pub pressure_full: Option<f64>,
    pub vram_used_mb: Option<u64>,
    pub vram_total_mb: Option<u64>,
}

pub(crate) fn read_memory_overview() -> (f64, f64, f64, f64, f64, f64, f64, Option<f64>, Option<f64>)
{
    let meminfo = fs::read_to_string("/proc/meminfo").unwrap_or_default();
    let field = |key: &str| -> f64 {
        let wanted = format!("{key}:");
        meminfo
            .lines()
            .find_map(|line| {
                let mut parts = line.split_whitespace();
                if parts.next()? != wanted.as_str() {
                    return None;
                }
                Some(
                    parts
                        .next()
                        .and_then(|v| v.parse::<f64>().ok())
                        .unwrap_or(0.0),
                )
            })
            .unwrap_or(0.0)
    };
    let kb_to_gb = |kb: f64| kb * 1024.0 / 1e9;
    let total = field("MemTotal");
    let available = field("MemAvailable");
    let free = field("MemFree");
    let cached = field("Cached") + field("SReclaimable") + field("Buffers");
    let used = (total - available).max(0.0);
    let swap_total = field("SwapTotal");
    let swap_free = field("SwapFree");
    let swap_used = (swap_total - swap_free).max(0.0);
    let pressure = fs::read_to_string("/proc/pressure/memory").unwrap_or_default();
    let pressure_value = |kind: &str| -> Option<f64> {
        pressure
            .lines()
            .find(|line| line.starts_with(kind))
            .and_then(|line| {
                line.split_whitespace()
                    .find_map(|part| part.strip_prefix("avg10="))
                    .and_then(|v| v.parse::<f64>().ok())
            })
    };
    (
        kb_to_gb(total),
        kb_to_gb(used),
        kb_to_gb(available),
        kb_to_gb(free),
        kb_to_gb(cached),
        kb_to_gb(swap_used),
        kb_to_gb(swap_total),
        pressure_value("some"),
        pressure_value("full"),
    )
}

pub(crate) fn read_dimms() -> Vec<DimmInfo> {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs_f64();
    {
        let cache = DIMM_CACHE.lock().unwrap();
        if let Some((cached_at, dimms)) = cache.as_ref() {
            if now - cached_at < 60.0 {
                return dimms.clone();
            }
        }
    }
    let res = run_helper_quiet("memory-info", &[]);
    let dimms: Vec<DimmInfo> = if res.ok {
        serde_json::from_str(&res.message).unwrap_or_default()
    } else {
        Vec::new()
    };
    *DIMM_CACHE.lock().unwrap() = Some((now, dimms.clone()));
    dimms
}
