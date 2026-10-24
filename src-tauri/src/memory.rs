use serde::{Deserialize, Serialize};
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

use crate::run_helper_quiet;
use crate::types::ProcessInfo;

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
    pub vram_used_mb: Option<u64>,
    pub vram_total_mb: Option<u64>,
    pub top_processes: Vec<ProcessInfo>,
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
