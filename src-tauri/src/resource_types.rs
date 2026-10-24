use std::sync::atomic::{AtomicU64, Ordering};
use std::time::SystemTime;

use serde::{Deserialize, Serialize};

/// Metrik yang dapat diwaspadai oleh aturan sumber daya.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash, Copy)]
pub enum Metric {
    CpuUtil,
    GpuUtil,
    CpuTemp,
    GpuTemp,
    GpuPowerW,
    GpuVramPct,
    FanRpm,
    BatteryPct,
}

/// Lock-free scalar gauge untuk pembacaan monitor loop.
pub struct AtomicF64(AtomicU64);

impl AtomicF64 {
    pub fn new(v: f64) -> Self {
        Self(AtomicU64::new(v.to_bits()))
    }
    pub fn store(&self, v: f64) {
        self.0.store(v.to_bits(), Ordering::Relaxed);
    }
    pub fn load(&self) -> f64 {
        f64::from_bits(self.0.load(Ordering::Relaxed))
    }
}

/// Snapshot CPU (lock-free untuk pembaca).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CpuSnapshot {
    pub overall_util: f64,
    pub per_core: Vec<f64>,
    pub freq_mhz: u32,
    pub temp_c: Option<f64>,
    pub throttle_events: u64,
}

/// Snapshot GPU (lock-free untuk pembaca).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GpuSnapshot {
    pub util: u32,
    pub vram_used_mb: u64,
    pub vram_total_mb: u64,
    pub temp_c: Option<u32>,
    pub power_w: Option<f64>,
    pub clock_mhz: Option<u32>,
    pub fan_pct: Option<u32>,
}

/// Ambang batas dengan hysteresis agar tidak oszilasi.
#[derive(Debug, Serialize, Deserialize)]
pub struct Threshold {
    pub metric: Metric,
    pub warn: f64,
    pub critical: f64,
    pub hysteresis_pct: f64,
    pub cooldown_secs: u64,
}

impl Clone for Threshold {
    fn clone(&self) -> Self {
        Threshold {
            metric: self.metric.clone(),
            warn: self.warn,
            critical: self.critical,
            hysteresis_pct: self.hysteresis_pct,
            cooldown_secs: self.cooldown_secs,
        }
    }
}

/// Kondisi aturan: ALL (semua terpenuhi) atau ANY (salah satu).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Condition {
    All(Vec<Threshold>),
    Any(Vec<Threshold>),
}

/// Aksi yang dijalankan RuleEngine ketika kondisi terpenuhi.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Action {
    SetThermalMode { mode: String },
    SetGpuPowerLimit { watts: u32 },
    SetGpuRuntimePm { control: String },
    SetFanCurve { fan_id: i32, points_json: String },
    SetFanManual { fan_id: i32, pwm_percent: i32 },
    SetFanAuto { fan_id: i32 },
    Renice { pid: i32, nice: i32 },
    Notify { title: String, body: String },
}

/// Satu atur otomatisasi sumber daya.
#[derive(Debug, Serialize, Deserialize)]
pub struct Rule {
    pub id: String,
    pub name: String,
    pub enabled: bool,
    pub condition: Condition,
    pub action: Action,
    pub persistent: bool,
}

impl Clone for Rule {
    fn clone(&self) -> Self {
        Rule {
            id: self.id.clone(),
            name: self.name.clone(),
            enabled: self.enabled,
            condition: self.condition.clone(),
            action: self.action.clone(),
            persistent: self.persistent,
        }
    }
}

/// Status governor yang diexpose ke frontend.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResourceStatus {
    pub cpu: CpuSnapshot,
    pub gpu: GpuSnapshot,
    pub rules: Vec<RuleSummary>,
}

/// Ringkasan aturan untuk frontend (tanpa atomics).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RuleSummary {
    pub id: String,
    pub name: String,
    pub enabled: bool,
}
