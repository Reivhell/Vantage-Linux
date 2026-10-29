use crate::config::home_dir;
use crate::gpu_ext;
use crate::power;
use crate::resource_types::*;
use crate::stats::read_cpu_temp;
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};
use std::sync::{Arc, RwLock};
use std::time::{SystemTime, UNIX_EPOCH};
use tokio::sync::mpsc;

/// Titik deteksi pelanggaran ambang (untuk hysteresis).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum BreachState {
    Clear,
    Critical,
}

/// Controller CPU: baca /proc/stat + thermal zone, expose snapshot lock-free.
pub struct CpuController {
    overall_util: AtomicF64,
    per_core: Vec<AtomicF64>,
    temp_c: AtomicF64,
    freq_mhz: AtomicU32,
    prev: std::sync::Mutex<CpuPrev>,
}

struct CpuPrev {
    total: u64,
    idle: u64,
    core_total: Vec<u64>,
    core_idle: Vec<u64>,
}

impl CpuController {
    pub fn new() -> Self {
        let mut c = Self {
            overall_util: AtomicF64::new(0.0),
            per_core: Vec::new(),
            temp_c: AtomicF64::new(f64::NAN),
            freq_mhz: AtomicU32::new(0),
            prev: std::sync::Mutex::new(CpuPrev {
                total: 0,
                idle: 0,
                core_total: Vec::new(),
                core_idle: Vec::new(),
            }),
        };
        c.init_core_count();
        c
    }

    fn init_core_count(&mut self) {
        let n = Self::core_count();
        self.per_core = (0..n).map(|_| AtomicF64::new(0.0)).collect();
        self.prev.lock().unwrap().core_total = vec![0; n];
        self.prev.lock().unwrap().core_idle = vec![0; n];
    }

    fn core_count() -> usize {
        fs::read_dir("/sys/devices/system/cpu")
            .ok()
            .and_then(|entries| {
                Some(
                    entries
                        .flatten()
                        .filter(|e| {
                            e.file_name()
                                .to_string_lossy()
                                .starts_with("cpu")
                                && e.file_name().to_string_lossy()[3..]
                                    .chars()
                                    .all(|c| c.is_ascii_digit())
                        })
                        .count(),
                )
            })
            .unwrap_or(1)
            .max(1)
    }

    /// Tick dipanggil governor setiap detik.
    pub fn tick(&self) {
        if let Some((total, idle, cores)) = read_proc_stat() {
            let mut prev = self.prev.lock().unwrap();
            let util = if prev.total > 0 {
                let dt = total.saturating_sub(prev.total);
                let di = idle.saturating_sub(prev.idle);
                if dt > 0 {
                    100.0 * (1.0 - di as f64 / dt as f64)
                } else {
                    0.0
                }
            } else {
                0.0
            };
            self.overall_util.store(util.clamp(0.0, 100.0));
            prev.total = total;
            prev.idle = idle;

            for (i, (ct, ci)) in cores.iter().enumerate() {
                if i < self.per_core.len() {
                    let p = &mut prev;
                    let ptotal = p.core_total[i];
                    let pidle = p.core_idle[i];
                    let cu = if ptotal > 0 {
                        let dt = ct.saturating_sub(ptotal);
                        let di = ci.saturating_sub(pidle);
                        if dt > 0 {
                            100.0 * (1.0 - di as f64 / dt as f64)
                        } else {
                            0.0
                        }
                    } else {
                        0.0
                    };
                    self.per_core[i].store(cu.clamp(0.0, 100.0));
                    p.core_total[i] = *ct;
                    p.core_idle[i] = *ci;
                }
            }
        }
        if let Some(temp) = read_cpu_temp() {
            self.temp_c.store(temp as f64);
        }
        // Frekuensi dari sysinfo-ish /proc/cpuinfo
        if let Some(freq) = read_cpu_freq_mhz() {
            self.freq_mhz.store(freq, Ordering::Relaxed);
        }
    }

    pub fn snapshot(&self) -> CpuSnapshot {
        let rel = Ordering::Relaxed;
        CpuSnapshot {
            overall_util: self.overall_util.load(),
            per_core: self.per_core.iter().map(|a| a.load()).collect(),
            freq_mhz: self.freq_mhz.load(rel),
            temp_c: if self.temp_c.load().is_nan() {
                None
            } else {
                Some(self.temp_c.load())
            },
            throttle_events: 0,
        }
    }
}

fn read_proc_stat() -> Option<(u64, u64, Vec<(u64, u64)>)> {
    let text = fs::read_to_string("/proc/stat").ok()?;
    let mut total = 0u64;
    let mut idle = 0u64;
    let mut cores = Vec::new();
    for line in text.lines() {
        let mut fields = line.split_whitespace();
        let label = fields.next()?;
        if !label.starts_with("cpu") {
            continue;
        }
        let vals: Vec<u64> = fields.filter_map(|v| v.parse().ok()).collect();
        if vals.len() < 4 {
            continue;
        }
        // fields: user nice system idle iowait irq softirq steal ...
        let u = vals.iter().sum::<u64>();
        let id = vals[3];
        if label == "cpu" {
            total = u;
            idle = id;
        } else {
            cores.push((u, id));
        }
    }
    Some((total, idle, cores))
}

fn read_cpu_freq_mhz() -> Option<u32> {
    let text = fs::read_to_string("/proc/cpuinfo").ok()?;
    for line in text.lines() {
        let (_, after) = line.split_once("cpu MHz")?;
        let v: f64 = after.split(':').nth(1)?.trim().parse().ok()?;
        return Some(v.round() as u32);
    }
    None
}

/// Controller GPU: baca nvidia-smi, expose snapshot lock-free.
pub struct GpuController {
    util: AtomicU32,
    vram_used_mb: AtomicU64,
    vram_total_mb: AtomicU64,
    temp_c: AtomicU32,
    power_w: AtomicF64,
    clock_mhz: AtomicU32,
    fan_pct: AtomicU32,
}

impl GpuController {
    pub fn new() -> Self {
        Self {
            util: AtomicU32::new(0),
            vram_used_mb: AtomicU64::new(0),
            vram_total_mb: AtomicU64::new(0),
            temp_c: AtomicU32::new(0),
            power_w: AtomicF64::new(0.0),
            clock_mhz: AtomicU32::new(0),
            fan_pct: AtomicU32::new(0),
        }
    }

    pub fn tick(&self) {
        let Ok(out) = std::process::Command::new("nvidia-smi")
            .args([
                "--query-gpu=utilization.gpu,memory.used,memory.total,temperature.gpu,power.draw,clocks.gr,fan.speed",
                "--format=csv,noheader,nounits",
            ])
            .output()
        else {
            return;
        };
        let text = String::from_utf8_lossy(&out.stdout);
        let Some(line) = text.lines().find(|l| !l.trim().is_empty()) else {
            return;
        };
        let parts: Vec<&str> = line.split(',').map(|s| s.trim()).collect();
        if parts.len() < 7 {
            return;
        }
        let num = |i: usize| parts.get(i).and_then(|v| v.parse::<f64>().ok());
        let util = num(0).unwrap_or(0.0) as u32;
        let vram_used = num(1).unwrap_or(0.0) as u64;
        let vram_total = num(2).unwrap_or(0.0) as u64;
        let temp = num(3).unwrap_or(0.0) as u32;
        let power = num(4).unwrap_or(0.0);
        let clock = num(5).unwrap_or(0.0) as u32;
        let fan = num(6).unwrap_or(0.0) as u32;

        self.util
            .store(util.clamp(0, 100), Ordering::Relaxed);
        self.vram_used_mb
            .store(vram_used, Ordering::Relaxed);
        self.vram_total_mb
            .store(vram_total, Ordering::Relaxed);
        self.temp_c.store(temp, Ordering::Relaxed);
        self.power_w.store(power);
        self.clock_mhz.store(clock, Ordering::Relaxed);
        self.fan_pct
            .store(fan.clamp(0, 100), Ordering::Relaxed);
    }

    pub fn snapshot(&self) -> GpuSnapshot {
        let rel = Ordering::Relaxed;
        GpuSnapshot {
            util: self.util.load(rel),
            vram_used_mb: self.vram_used_mb.load(rel),
            vram_total_mb: self.vram_total_mb.load(rel),
            temp_c: if self.temp_c.load(rel) == 0 {
                None
            } else {
                Some(self.temp_c.load(rel))
            },
            power_w: if self.power_w.load() == 0.0 {
                None
            } else {
                Some(self.power_w.load())
            },
            clock_mhz: if self.clock_mhz.load(rel) == 0 {
                None
            } else {
                Some(self.clock_mhz.load(rel))
            },
            fan_pct: if self.fan_pct.load(rel) == 0 {
                None
            } else {
                Some(self.fan_pct.load(rel))
            },
        }
    }
}

/// RuleEngine: mengevaluasi aturan setiap tick dan mengirimkan aksi.
pub struct RuleEngine {
    rules: Arc<RwLock<Vec<Rule>>>,
    action_tx: mpsc::UnboundedSender<Action>,
    breach_state: Arc<RwLock<HashMap<(String, Metric), BreachState>>>,
    last_action: Arc<RwLock<HashMap<String, u64>>>,
}

impl RuleEngine {
    pub fn new(
        rules: Arc<RwLock<Vec<Rule>>>,
        action_tx: mpsc::UnboundedSender<Action>,
    ) -> Self {
        Self {
            rules,
            action_tx,
            breach_state: Arc::new(RwLock::new(HashMap::new())),
            last_action: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    pub fn evaluate(&self, cpu: &CpuSnapshot, gpu: &GpuSnapshot) {
        let rules = self.rules.read().unwrap();
        let now = now_secs();
        for rule in rules.iter() {
            if !rule.enabled {
                continue;
            }
            let triggered = match &rule.condition {
                Condition::All(ts) => ts
                    .iter()
                    .all(|t| self.breached(rule.id.as_str(), t, cpu, gpu)),
                Condition::Any(ts) => ts
                    .iter()
                    .any(|t| self.breached(rule.id.as_str(), t, cpu, gpu)),
            };
            if triggered {
                let last = self
                    .last_action
                    .read()
                    .unwrap()
                    .get(rule.id.as_str())
                    .copied()
                    .unwrap_or(0);
                if now.saturating_sub(last) >= rule_cooldown(rule) {
                    let _ = self.action_tx.send(rule.action.clone());
                    self.last_action.write().unwrap().insert(rule.id.clone(), now);
                }
            }
        }
    }

    fn breached(
        &self,
        rule_id: &str,
        t: &Threshold,
        cpu: &CpuSnapshot,
        gpu: &GpuSnapshot,
    ) -> bool {
        let value = metric_value(&t.metric, cpu, gpu);
        let key = (rule_id.to_string(), t.metric.clone());
        let mut state = self.breach_state.write().unwrap();
        let currently = state.entry(key).or_insert(BreachState::Clear);
        match *currently {
            BreachState::Clear => {
                if value >= t.critical {
                    *currently = BreachState::Critical;
                    true
                } else {
                    false
                }
            }
            BreachState::Critical => {
                if value <= t.warn {
                    *currently = BreachState::Clear;
                    false
                } else {
                    true
                }
            }
        }
    }
}

fn metric_value(m: &Metric, cpu: &CpuSnapshot, gpu: &GpuSnapshot) -> f64 {
    match m {
        Metric::CpuUtil => cpu.overall_util,
        Metric::GpuUtil => gpu.util as f64,
        Metric::CpuTemp => cpu.temp_c.unwrap_or(0.0),
        Metric::GpuTemp => gpu.temp_c.map(|v| v as f64).unwrap_or(0.0),
        Metric::GpuPowerW => gpu.power_w.unwrap_or(0.0),
        Metric::GpuVramPct => {
            if gpu.vram_total_mb > 0 {
                gpu.vram_used_mb as f64 / gpu.vram_total_mb as f64 * 100.0
            } else {
                0.0
            }
        }
        Metric::FanRpm => 0.0,
        Metric::BatteryPct => 0.0,
    }
}

fn rule_cooldown(rule: &Rule) -> u64 {
    match &rule.condition {
        Condition::All(ts) | Condition::Any(ts) => {
            ts.iter().map(|t| t.cooldown_secs).max().unwrap_or(60)
        }
    }
}

/// ActionExecutor: menguras channel aksi dan mengeksekusi via helper / sistem.
pub struct ActionExecutor {
    action_rx: mpsc::UnboundedReceiver<Action>,
}

impl ActionExecutor {
    pub fn new(action_rx: mpsc::UnboundedReceiver<Action>) -> Self {
        Self { action_rx }
    }

    pub async fn run(mut self) {
        while let Some(action) = self.action_rx.recv().await {
            if let Err(e) = self.dispatch(&action) {
                tracing::warn!("resource action failed: {e}");
            }
        }
    }

    fn dispatch(&self, action: &Action) -> Result<(), String> {
        match action {
            Action::SetThermalMode { mode } => {
                if !power::set_thermal_mode(mode).ok {
                    return Err("helper 'thermal-mode' failed".into());
                }
            }
            Action::SetGpuPowerLimit { watts } => {
                if !gpu_ext::set_power_limit(*watts).ok {
                    return Err("helper 'gpu-power-limit' failed".into());
                }
            }
            Action::SetGpuRuntimePm { control } => {
                if !gpu_ext::set_runtime_pm(control).ok {
                    return Err("helper 'gpu-runtime-pm' failed".into());
                }
            }
            Action::SetFanCurve { fan_id, points_json } => {
                let res = power::run_helper(
                    "fan-curve",
                    &[fan_id.to_string().as_str(), points_json.as_str()],
                );
                if !res.ok {
                    return Err(format!("helper 'fan-curve' failed: {}", res.message));
                }
            }
            Action::SetFanManual { fan_id, pwm_percent } => {
                let res = power::run_helper(
                    "fan-manual",
                    &[fan_id.to_string().as_str(), pwm_percent.to_string().as_str()],
                );
                if !res.ok {
                    return Err(format!("helper 'fan-manual' failed: {}", res.message));
                }
            }
            Action::SetFanAuto { fan_id } => {
                let res = power::run_helper("fan-auto", &[fan_id.to_string().as_str()]);
                if !res.ok {
                    return Err(format!("helper 'fan-auto' failed: {}", res.message));
                }
            }
            Action::Renice { pid, nice } => {
                if *pid <= 1 {
                    return Err("refusing to renice PID 1 or below".into());
                }
                let status = std::process::Command::new("renice")
                    .args([nice.to_string().as_str(), "-p", pid.to_string().as_str()])
                    .status()
                    .map_err(|e| format!("renice: {e}"))?;
                if !status.success() {
                    return Err(format!("helper 'renice' failed (exit {:?})", status.code()));
                }
            }
            Action::Notify { title, body } => {
                tracing::info!(resource_notify = %title, %body);
            }
        }
        Ok(())
    }
}

/// Governor utama: milki controller, aturan, dan executor; jalankan loop tokio.
pub struct ResourceGovernor {
    cpu: Arc<CpuController>,
    gpu: Arc<GpuController>,
    rules: Arc<RwLock<Vec<Rule>>>,
    engine: RuleEngine,
    action_tx: mpsc::UnboundedSender<Action>,
}

impl ResourceGovernor {
    pub fn new(rules: Vec<Rule>) -> (Self, ActionExecutor) {
        let (tx, rx) = mpsc::unbounded_channel();
        let rules = Arc::new(RwLock::new(rules));
        let cpu = Arc::new(CpuController::new());
        let gpu = Arc::new(GpuController::new());
        let engine = RuleEngine::new(rules.clone(), tx.clone());
        let executor = ActionExecutor::new(rx);

        (
            Self {
                cpu,
                gpu,
                rules,
                engine,
                action_tx: tx,
            },
            executor,
        )
    }

    /// Aturan bawaan yang aman dan non-destructif.
    pub fn default_rules() -> Vec<Rule> {
        fn threshold(metric: Metric, warn: f64, critical: f64) -> Threshold {
            Threshold {
                metric,
                warn,
                critical,
                hysteresis_pct: 5.0,
                cooldown_secs: 300,
            }
        }
        vec![
            Rule {
                id: "cpu-temp-critical".into(),
                name: "CPU suhu kritis".into(),
                enabled: true,
                condition: Condition::Any(vec![threshold(Metric::CpuTemp, 85.0, 95.0)]),
                action: Action::SetThermalMode {
                    mode: "quiet".into(),
                },
                persistent: true,
            },
            Rule {
                id: "gpu-temp-critical".into(),
                name: "GPU suhu kritis".into(),
                enabled: true,
                condition: Condition::Any(vec![threshold(Metric::GpuTemp, 85.0, 100.0)]),
                action: Action::Notify {
                    title: "GPU panas".into(),
                    body: "Suhu GPU melampati ambang aman; turunkan beban atau aktifkan mode Quiet."
                        .into(),
                },
                persistent: true,
            },
        ]
    }

    fn rules_path() -> PathBuf {
        home_dir().join(".config/vantage-linux/resource_rules.json")
    }

    /// Muat aturan dari berkas, atau bawaan jika berkas belum ada.
    pub fn load_rules() -> Vec<Rule> {
        let path = Self::rules_path();
        match fs::read_to_string(&path) {
            Ok(text) => serde_json::from_str(&text).unwrap_or_else(|_| Self::default_rules()),
            Err(_) => Self::default_rules(),
        }
    }

    /// Simpan aturan ke berkas.
    pub fn save_rules(rules: &[Rule]) -> Result<(), String> {
        let path = Self::rules_path();
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        let text = serde_json::to_string_pretty(rules).map_err(|e| e.to_string())?;
        fs::write(&path, text).map_err(|e| e.to_string())
    }

    /// Tick sinkron: update controller snapshot. Dipanggil oleh task monitor.
    pub fn tick(&self) {
        self.cpu.tick();
        self.gpu.tick();
    }

    /// Evaluasi aturan terhadap snapshot terkini.
    pub fn evaluate(&self) {
        let cpu = self.cpu.snapshot();
        let gpu = self.gpu.snapshot();
        self.engine.evaluate(&cpu, &gpu);
    }

    pub fn status(&self) -> ResourceStatus {
        ResourceStatus {
            cpu: self.cpu.snapshot(),
            gpu: self.gpu.snapshot(),
            rules: self
                .rules
                .read()
                .unwrap()
                .iter()
                .map(|r| RuleSummary {
                    id: r.id.clone(),
                    name: r.name.clone(),
                    enabled: r.enabled,
                })
                .collect(),
        }
    }

    pub fn rules(&self) -> Arc<RwLock<Vec<Rule>>> {
        self.rules.clone()
    }

    pub fn action_tx(&self) -> mpsc::UnboundedSender<Action> {
        self.action_tx.clone()
    }
}

fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}
