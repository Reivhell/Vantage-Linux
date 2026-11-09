use crate::config::home_dir;
use crate::error::{AppError, AppResult};
use crate::gpu_ext;
use crate::power;
use crate::resource_types::*;
use crate::stats::SystemStats;
use std::collections::HashMap;
use std::fs;
use std::io::Write;
use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
use std::path::PathBuf;
use std::sync::{Arc, RwLock};
use std::time::{SystemTime, UNIX_EPOCH};
use tokio::sync::mpsc;

/// Titik deteksi pelanggaran ambang (state machine histeresis).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum BreachState {
    Clear,
    Warn,
    Critical,
}

/// Paket aksi beserta konteks aturan untuk audit.
pub struct RuleAction {
    pub rule_id: String,
    pub rule_name: String,
    pub action: Action,
}

/// Controller CPU: menerima snapshot dari monitor loop (satu sumber telemetri).
pub struct CpuController {
    snapshot: RwLock<CpuSnapshot>,
}

impl CpuController {
    pub fn new() -> Self {
        Self {
            snapshot: RwLock::new(CpuSnapshot {
                overall_util: 0.0,
                per_core: Vec::new(),
                freq_mhz: 0,
                temp_c: None,
                throttle_events: 0,
            }),
        }
    }

    pub fn ingest(&self, stats: &SystemStats) {
        let mut snap = self.snapshot.write().unwrap();
        snap.overall_util = stats.cpu_percent as f64;
        snap.per_core = stats.cpu_per_core.iter().map(|v| *v as f64).collect();
        snap.freq_mhz = stats.cpu_freq_mhz as u32;
        snap.temp_c = stats.cpu_temp.map(|v| v as f64).filter(|v| *v > 0.0);
    }

    pub fn snapshot(&self) -> CpuSnapshot {
        self.snapshot.read().unwrap().clone()
    }
}

/// Controller GPU: menerima snapshot dari monitor loop yang sama.
pub struct GpuController {
    snapshot: RwLock<GpuSnapshot>,
}

impl GpuController {
    pub fn new() -> Self {
        Self {
            snapshot: RwLock::new(GpuSnapshot {
                util: 0,
                vram_used_mb: 0,
                vram_total_mb: 0,
                temp_c: None,
                power_w: None,
                clock_mhz: None,
                fan_pct: None,
            }),
        }
    }

    pub fn ingest(&self, stats: &SystemStats) {
        let mut snap = self.snapshot.write().unwrap();
        snap.util = stats.gpu_util.unwrap_or(0);
        snap.vram_used_mb = stats.gpu_vram_used_mb.unwrap_or(0);
        snap.vram_total_mb = stats.gpu_vram_total_mb.unwrap_or(0);
        snap.temp_c = stats.gpu_temp.filter(|v| *v > 0);
        snap.power_w = stats.gpu_power_w.filter(|v| *v > 0.0);
        snap.clock_mhz = stats.gpu_freq_mhz.filter(|v| *v > 0);
        snap.fan_pct = stats.gpu_fan_pct.filter(|v| *v > 0);
    }

    pub fn snapshot(&self) -> GpuSnapshot {
        self.snapshot.read().unwrap().clone()
    }
}

/// Controller sensor sistem (kipas, baterai, RAM).
pub struct SystemController {
    snapshot: RwLock<SystemSnapshot>,
}

impl SystemController {
    pub fn new() -> Self {
        Self {
            snapshot: RwLock::new(SystemSnapshot::default()),
        }
    }

    pub fn ingest(&self, stats: &SystemStats) {
        let mut snap = self.snapshot.write().unwrap();
        snap.fan1_rpm = stats.fan1_rpm;
        snap.fan2_rpm = stats.fan2_rpm;
        snap.battery_pct = stats.battery_percent.map(|v| v as f64);
        snap.ram_pct = stats.ram_percent as f64;
        snap.plugged = stats.power_plugged;
    }

    pub fn snapshot(&self) -> SystemSnapshot {
        self.snapshot.read().unwrap().clone()
    }
}

/// RuleEngine: mengevaluasi aturan setiap tick dan mengirimkan aksi.
pub struct RuleEngine {
    rules: Arc<RwLock<Vec<Rule>>>,
    action_tx: mpsc::UnboundedSender<RuleAction>,
    breach_state: RwLock<HashMap<(String, Metric), BreachState>>,
    /// Level gabungan untuk Condition::All; edge-trigger agar aksi tidak berulang tiap tick.
    all_active: RwLock<HashMap<String, bool>>,
    last_action: RwLock<HashMap<String, u64>>,
    runtime: Arc<RwLock<HashMap<String, RuleRuntime>>>,
}

impl RuleEngine {
    pub fn new(
        rules: Arc<RwLock<Vec<Rule>>>,
        action_tx: mpsc::UnboundedSender<RuleAction>,
        runtime: Arc<RwLock<HashMap<String, RuleRuntime>>>,
    ) -> Self {
        Self {
            rules,
            action_tx,
            breach_state: RwLock::new(HashMap::new()),
            all_active: RwLock::new(HashMap::new()),
            last_action: RwLock::new(HashMap::new()),
            runtime,
        }
    }

    pub fn evaluate(&self, cpu: &CpuSnapshot, gpu: &GpuSnapshot, system: &SystemSnapshot) {
        let rules = self.rules.read().unwrap();
        let now = now_secs();
        for rule in rules.iter() {
            if !rule.enabled {
                continue;
            }
            let thresholds = rule.condition.thresholds();
            // Evaluate every threshold before combining: short-circuiting would leave
            // stale hysteresis state for the thresholds that were skipped.
            let breaches: Vec<bool> = thresholds
                .iter()
                .map(|t| self.breached(rule.id.as_str(), t, cpu, gpu, system))
                .collect();
            let triggered = match &rule.condition {
                Condition::All(_) => {
                    if thresholds.is_empty() {
                        false
                    } else {
                        // Kombinasikan level per-ambang, lalu tepi-naikkan hasilnya.
                        // Meng-AND edge event per-ambang bisa tidak pernah true bila
                        // ambang melewati batas pada tick yang berbeda.
                        let active = thresholds
                            .iter()
                            .all(|t| self.current_level(rule.id.as_str(), t.metric));
                        let mut all = self.all_active.write().unwrap();
                        let was = all.insert(rule.id.clone(), active).unwrap_or(false);
                        active && !was
                    }
                }
                Condition::Any(_) => breaches.iter().any(|b| *b),
            };
            if !triggered {
                continue;
            }
            let cooldown = thresholds
                .iter()
                .map(|t| t.cooldown_secs)
                .max()
                .unwrap_or(120);
            let last = self
                .last_action
                .read()
                .unwrap()
                .get(rule.id.as_str())
                .copied()
                .unwrap_or(0);
            if now.saturating_sub(last) < cooldown {
                continue;
            }
            if self
                .action_tx
                .send(RuleAction {
                    rule_id: rule.id.clone(),
                    rule_name: rule.name.clone(),
                    action: rule.action.clone(),
                })
                .is_ok()
            {
                self.last_action
                    .write()
                    .unwrap()
                    .insert(rule.id.clone(), now);
                let mut runtime = self.runtime.write().unwrap();
                let entry = runtime.entry(rule.id.clone()).or_default();
                entry.trigger_count = entry.trigger_count.saturating_add(1);
                entry.last_triggered_at = Some(now);
            }
        }
    }

    /// Hapus state deteksi setelah aturan berubah agar ambang baru langsung dievaluasi.
    pub fn reset_detection_state(&self) {
        self.breach_state.write().unwrap().clear();
        self.all_active.write().unwrap().clear();
        self.last_action.write().unwrap().clear();
    }

    /// Ambang dianggap aktif bila state histeresisnya Warn atau Critical.
    fn current_level(&self, rule_id: &str, metric: Metric) -> bool {
        self.breach_state
            .read()
            .unwrap()
            .get(&(rule_id.to_string(), metric))
            .is_some_and(|s| *s != BreachState::Clear)
    }

    fn breached(
        &self,
        rule_id: &str,
        t: &Threshold,
        cpu: &CpuSnapshot,
        gpu: &GpuSnapshot,
        system: &SystemSnapshot,
    ) -> bool {
        let Some(value) = metric_value(&t.metric, cpu, gpu, system) else {
            // Sensor tidak tersedia: jangan pernah memicu aksi berdasarkan nilai fiktif.
            return false;
        };
        let key = (rule_id.to_string(), t.metric);
        let mut state = self.breach_state.write().unwrap();
        let currently = state.entry(key).or_insert(BreachState::Clear);
        let (exceeds_critical, at_warn, recovered) = match t.direction {
            Direction::Above => (
                value >= t.critical,
                value >= t.warn,
                value <= t.reset_value(),
            ),
            Direction::Below => (
                value <= t.critical,
                value <= t.warn,
                value >= t.reset_value(),
            ),
        };
        match *currently {
            BreachState::Clear => {
                if exceeds_critical {
                    *currently = BreachState::Critical;
                    true
                } else if at_warn {
                    *currently = BreachState::Warn;
                    false
                } else {
                    false
                }
            }
            BreachState::Warn => {
                if exceeds_critical {
                    *currently = BreachState::Critical;
                    true
                } else if recovered {
                    *currently = BreachState::Clear;
                    false
                } else {
                    false
                }
            }
            BreachState::Critical => {
                if recovered {
                    *currently = BreachState::Clear;
                }
                // Tetap Critical sampai pulih: jangan mengulang aksi setiap tick.
                false
            }
        }
    }
}

fn metric_value(
    m: &Metric,
    cpu: &CpuSnapshot,
    gpu: &GpuSnapshot,
    system: &SystemSnapshot,
) -> Option<f64> {
    match m {
        Metric::CpuUtil => Some(cpu.overall_util),
        Metric::GpuUtil => Some(gpu.util as f64),
        Metric::CpuTemp => cpu.temp_c,
        Metric::GpuTemp => gpu.temp_c.map(|v| v as f64),
        Metric::GpuPowerW => gpu.power_w,
        Metric::GpuVramPct => {
            if gpu.vram_total_mb > 0 {
                Some(gpu.vram_used_mb as f64 / gpu.vram_total_mb as f64 * 100.0)
            } else {
                None
            }
        }
        Metric::FanRpm => {
            let rpm = system.fan1_rpm.max(system.fan2_rpm);
            (rpm > 0).then_some(rpm as f64)
        }
        Metric::BatteryPct => system.battery_pct,
        Metric::RamPct => Some(system.ram_pct),
    }
}

/// ActionExecutor: menguras channel aksi, mengeksekusi, dan menulis audit.
pub struct ActionExecutor {
    action_rx: mpsc::UnboundedReceiver<RuleAction>,
    runtime: Arc<RwLock<HashMap<String, RuleRuntime>>>,
}

impl ActionExecutor {
    pub fn new(
        action_rx: mpsc::UnboundedReceiver<RuleAction>,
        runtime: Arc<RwLock<HashMap<String, RuleRuntime>>>,
    ) -> Self {
        Self { action_rx, runtime }
    }

    pub async fn run(mut self) {
        while let Some(message) = self.action_rx.recv().await {
            let description = message.action.describe();
            let action = message.action.clone();
            let result = match tokio::task::spawn_blocking(move || Self::dispatch(&action)).await {
                Ok(result) => result,
                Err(e) => Err(AppError::Io {
                    op: "automation action task".into(),
                    path: None,
                    source: e.to_string(),
                }),
            };
            let (status, detail) = match result {
                Ok(detail) => ("ok", detail),
                Err(e) => ("error", e.to_string()),
            };
            if status == "error" {
                if let Some(entry) = self.runtime.write().unwrap().get_mut(&message.rule_id) {
                    entry.last_error = Some(detail.clone());
                }
            } else if let Some(entry) = self.runtime.write().unwrap().get_mut(&message.rule_id) {
                entry.last_error = None;
            }
            if let Err(e) = crate::db::record_automation_event(
                &message.rule_id,
                &message.rule_name,
                &description,
                status,
                &detail,
            ) {
                tracing::warn!("failed to persist automation event: {e}");
            }
            if status == "error" {
                tracing::warn!(
                    rule = %message.rule_id,
                    action = %description,
                    error = %detail,
                    "automation action failed"
                );
            } else {
                tracing::info!(
                    rule = %message.rule_id,
                    action = %description,
                    "automation action executed"
                );
            }
        }
    }

    fn dispatch(action: &Action) -> AppResult<String> {
        match action {
            Action::SetThermalMode { mode } => {
                let res = power::set_thermal_mode(mode);
                if !res.ok {
                    return Err(AppError::HelperFailed {
                        subcommand: "thermal-mode".into(),
                        exit: None,
                        stderr: res.message,
                    });
                }
                Ok(res.message)
            }
            Action::SetGpuPowerLimit { watts } => {
                let res = gpu_ext::set_power_limit(*watts);
                if !res.ok {
                    return Err(AppError::HelperFailed {
                        subcommand: "gpu-power-limit".into(),
                        exit: None,
                        stderr: res.message,
                    });
                }
                Ok(res.message)
            }
            Action::SetGpuRuntimePm { control } => {
                let res = gpu_ext::set_runtime_pm(control);
                if !res.ok {
                    return Err(AppError::HelperFailed {
                        subcommand: "gpu-runtime-pm".into(),
                        exit: None,
                        stderr: res.message,
                    });
                }
                Ok(res.message)
            }
            Action::SetFanCurve {
                fan_id,
                points_json,
            } => {
                let id = fan_id.to_string();
                let res = power::run_helper("fan-curve", &[id.as_str(), points_json.as_str()]);
                if !res.ok {
                    return Err(AppError::HelperFailed {
                        subcommand: "fan-curve".into(),
                        exit: None,
                        stderr: res.message,
                    });
                }
                Ok(res.message)
            }
            Action::SetFanManual {
                fan_id,
                pwm_percent,
            } => {
                let id = fan_id.to_string();
                let pwm = pwm_percent.to_string();
                let res = power::run_helper("fan-manual", &[id.as_str(), pwm.as_str()]);
                if !res.ok {
                    return Err(AppError::HelperFailed {
                        subcommand: "fan-manual".into(),
                        exit: None,
                        stderr: res.message,
                    });
                }
                Ok(res.message)
            }
            Action::SetFanAuto { fan_id } => {
                let id = fan_id.to_string();
                let res = power::run_helper("fan-auto", &[id.as_str()]);
                if !res.ok {
                    return Err(AppError::HelperFailed {
                        subcommand: "fan-auto".into(),
                        exit: None,
                        stderr: res.message,
                    });
                }
                Ok(res.message)
            }
            Action::Renice { pid, nice } => {
                let status = std::process::Command::new("renice")
                    .args([nice.to_string().as_str(), "-p", pid.to_string().as_str()])
                    .status()
                    .map_err(|e| AppError::Io {
                        op: "renice".into(),
                        path: None,
                        source: e.to_string(),
                    })?;
                if !status.success() {
                    return Err(AppError::HelperFailed {
                        subcommand: "renice".into(),
                        exit: status.code(),
                        stderr: String::new(),
                    });
                }
                Ok(format!("PID {pid} nice={nice}"))
            }
            Action::Notify { title, body } => send_desktop_notification(title, body),
        }
    }
}

/// Kirim notifikasi desktop nyata; catat audit jika `notify-send` tidak tersedia.
fn send_desktop_notification(title: &str, body: &str) -> AppResult<String> {
    if !crate::config::load_config().desktop_notifications {
        return Ok("desktop notifications disabled in settings".into());
    }
    match std::process::Command::new("notify-send")
        .args(["--app-name=Vantage", "--urgency=normal", title, body])
        .output()
    {
        Ok(output) if output.status.success() => Ok("notified".into()),
        Ok(output) => Err(AppError::HelperFailed {
            subcommand: "notify-send".into(),
            exit: output.status.code(),
            stderr: String::from_utf8_lossy(&output.stderr).trim().to_string(),
        }),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            // Tidak ada daemon notifikasi: audit tetap tersimpan dan UI menampilkannya.
            Ok("notification daemon unavailable; recorded in audit log".into())
        }
        Err(e) => Err(AppError::Io {
            op: "notify-send".into(),
            path: None,
            source: e.to_string(),
        }),
    }
}

/// Governor utama: controller, aturan, engine, dan executor.
pub struct ResourceGovernor {
    cpu: CpuController,
    gpu: GpuController,
    system: SystemController,
    rules: Arc<RwLock<Vec<Rule>>>,
    engine: RuleEngine,
    action_tx: mpsc::UnboundedSender<RuleAction>,
    runtime: Arc<RwLock<HashMap<String, RuleRuntime>>>,
}

impl ResourceGovernor {
    pub fn new(rules: Vec<Rule>) -> (Self, ActionExecutor) {
        let (tx, rx) = mpsc::unbounded_channel();
        let rules = Arc::new(RwLock::new(rules));
        let runtime = Arc::new(RwLock::new(HashMap::new()));
        let engine = RuleEngine::new(rules.clone(), tx.clone(), runtime.clone());
        let executor = ActionExecutor::new(rx, runtime.clone());

        (
            Self {
                cpu: CpuController::new(),
                gpu: GpuController::new(),
                system: SystemController::new(),
                rules,
                engine,
                action_tx: tx,
                runtime,
            },
            executor,
        )
    }

    /// Aturan bawaan yang aman dan non-destruktif.
    pub fn default_rules() -> Vec<Rule> {
        fn threshold(metric: Metric, warn: f64, critical: f64, cooldown: u64) -> Threshold {
            Threshold {
                metric,
                direction: Direction::Above,
                warn,
                critical,
                hysteresis_pct: 5.0,
                cooldown_secs: cooldown,
            }
        }
        vec![
            Rule {
                id: "cpu-temp-critical".into(),
                name: "CPU thermal guard".into(),
                enabled: true,
                condition: Condition::Any(vec![threshold(Metric::CpuTemp, 85.0, 95.0, 300)]),
                // Thermal guard must increase cooling, not reduce it.
                action: Action::SetThermalMode {
                    mode: "performance".into(),
                },
                persistent: true,
            },
            Rule {
                id: "gpu-temp-critical".into(),
                name: "GPU thermal alert".into(),
                enabled: true,
                condition: Condition::Any(vec![threshold(Metric::GpuTemp, 85.0, 100.0, 300)]),
                action: Action::Notify {
                    title: "GPU temperature critical".into(),
                    body: "GPU exceeded 100 °C. Reduce load or switch to Quiet mode.".into(),
                },
                persistent: true,
            },
            Rule {
                id: "battery-low".into(),
                name: "Battery low warning".into(),
                enabled: true,
                condition: Condition::Any(vec![Threshold {
                    metric: Metric::BatteryPct,
                    direction: Direction::Below,
                    warn: 22.0,
                    critical: 15.0,
                    hysteresis_pct: 5.0,
                    cooldown_secs: 900,
                }]),
                action: Action::Notify {
                    title: "Battery low".into(),
                    body: "Battery is below 15%. Connect the charger to avoid an abrupt shutdown."
                        .into(),
                },
                persistent: true,
            },
        ]
    }

    fn rules_path() -> PathBuf {
        home_dir().join(".config/vantage-linux/resource_rules.json")
    }

    /// Muat aturan dari berkas; aturan tidak valid dibuang, bukan dijalankan.
    pub fn load_rules() -> Vec<Rule> {
        let path = Self::rules_path();
        let Ok(text) = fs::read_to_string(&path) else {
            return Self::default_rules();
        };
        let parsed: Vec<Rule> = match serde_json::from_str(&text) {
            Ok(rules) => rules,
            Err(e) => {
                tracing::warn!("ignoring invalid resource rules file: {e}");
                return Self::default_rules();
            }
        };
        let mut valid = Vec::with_capacity(parsed.len());
        for rule in parsed {
            match validate_rule(&rule) {
                Ok(()) if valid.len() < 48 => valid.push(rule),
                Ok(()) => tracing::warn!(rule = %rule.id, "rule limit reached, dropping"),
                Err(e) => tracing::warn!(rule = %rule.id, error = %e, "dropping invalid rule"),
            }
        }
        // A readable file — including an explicit empty array — is authoritative.
        // Only a missing or unparseable file falls back to safe defaults.
        valid
    }

    /// Simpan aturan secara atomik dengan permission 0600.
    pub fn save_rules(rules: &[Rule]) -> Result<(), String> {
        if rules.len() > 48 {
            return Err("maksimal 48 aturan".into());
        }
        for rule in rules {
            validate_rule(rule)?;
        }
        let path = Self::rules_path();
        let parent = path
            .parent()
            .ok_or_else(|| "invalid rules path".to_string())?;
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        let text = serde_json::to_vec_pretty(rules).map_err(|e| e.to_string())?;
        let tmp = parent.join(format!(".resource_rules.{}.tmp", std::process::id()));
        let mut file = fs::OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .mode(0o600)
            .open(&tmp)
            .map_err(|e| e.to_string())?;
        file.write_all(&text).map_err(|e| e.to_string())?;
        file.sync_all().map_err(|e| e.to_string())?;
        fs::set_permissions(&tmp, fs::Permissions::from_mode(0o600)).map_err(|e| e.to_string())?;
        fs::rename(&tmp, &path).map_err(|e| e.to_string())?;
        Ok(())
    }

    /// Perbarui snapshot dari monitor loop; tidak ada polling hardware ganda.
    pub fn ingest(&self, stats: &SystemStats) {
        self.cpu.ingest(stats);
        self.gpu.ingest(stats);
        self.system.ingest(stats);
    }

    /// Evaluasi aturan terhadap snapshot terkini.
    pub fn evaluate(&self) {
        let cpu = self.cpu.snapshot();
        let gpu = self.gpu.snapshot();
        let system = self.system.snapshot();
        self.engine.evaluate(&cpu, &gpu, &system);
    }

    pub fn status(&self) -> ResourceStatus {
        let runtime = self.runtime.read().unwrap();
        ResourceStatus {
            cpu: self.cpu.snapshot(),
            gpu: self.gpu.snapshot(),
            system: self.system.snapshot(),
            rules: self
                .rules
                .read()
                .unwrap()
                .iter()
                .map(|r| {
                    let rt = runtime.get(&r.id).cloned().unwrap_or_default();
                    let mode = match &r.condition {
                        Condition::All(_) => "ALL",
                        Condition::Any(_) => "ANY",
                    };
                    let metric = r
                        .condition
                        .thresholds()
                        .first()
                        .map(|t| format!("{:?}", t.metric))
                        .unwrap_or_default();
                    RuleSummary {
                        id: r.id.clone(),
                        name: r.name.clone(),
                        enabled: r.enabled,
                        condition_mode: mode.into(),
                        metric,
                        action: r.action.describe(),
                        trigger_count: rt.trigger_count,
                        last_triggered_at: rt.last_triggered_at,
                        last_error: rt.last_error,
                    }
                })
                .collect(),
        }
    }

    pub fn rules(&self) -> Arc<RwLock<Vec<Rule>>> {
        self.rules.clone()
    }

    /// Reset histeresis/cooldown setelah aturan ditambah, diubah, atau dihapus.
    pub fn reset_detection_state(&self) {
        self.engine.reset_detection_state();
    }

    pub fn action_tx(&self) -> mpsc::UnboundedSender<RuleAction> {
        self.action_tx.clone()
    }
}

fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cpu(temp: f64) -> CpuSnapshot {
        CpuSnapshot {
            overall_util: 10.0,
            per_core: vec![10.0],
            freq_mhz: 2000,
            temp_c: Some(temp),
            throttle_events: 0,
        }
    }

    fn empty_gpu() -> GpuSnapshot {
        GpuSnapshot {
            util: 0,
            vram_used_mb: 0,
            vram_total_mb: 0,
            temp_c: None,
            power_w: None,
            clock_mhz: None,
            fan_pct: None,
        }
    }

    fn engine_and_rx(_temp: f64) -> (RuleEngine, mpsc::UnboundedReceiver<RuleAction>) {
        let rules = vec![Rule {
            id: "t".into(),
            name: "t".into(),
            enabled: true,
            condition: Condition::Any(vec![Threshold {
                metric: Metric::CpuTemp,
                direction: Direction::Above,
                warn: 80.0,
                critical: 90.0,
                hysteresis_pct: 5.0,
                cooldown_secs: 0,
            }]),
            action: Action::Notify {
                title: "hot".into(),
                body: "hot".into(),
            },
            persistent: false,
        }];
        let (tx, rx) = mpsc::unbounded_channel();
        let engine = RuleEngine::new(
            Arc::new(RwLock::new(rules)),
            tx,
            Arc::new(RwLock::new(HashMap::new())),
        );
        (engine, rx)
    }

    #[test]
    fn triggers_only_at_critical_and_rearms_below_hysteresis() {
        let (engine, mut rx) = engine_and_rx(0.0);
        let system = SystemSnapshot::default();

        engine.evaluate(&cpu(85.0), &empty_gpu(), &system);
        assert!(rx.try_recv().is_err());

        engine.evaluate(&cpu(91.0), &empty_gpu(), &system);
        assert!(rx.try_recv().is_ok());

        // Masih di atas reset: tidak ada aksi berulang.
        engine.evaluate(&cpu(91.0), &empty_gpu(), &system);
        assert!(rx.try_recv().is_err());

        // Turun di bawah reset (80 - 5% = 76), lalu naik lagi memicu ulang.
        engine.evaluate(&cpu(70.0), &empty_gpu(), &system);
        engine.evaluate(&cpu(92.0), &empty_gpu(), &system);
        assert!(rx.try_recv().is_ok());
    }

    #[test]
    fn reset_detection_state_rearms_after_rule_change() {
        let (engine, mut rx) = engine_and_rx(0.0);
        let system = SystemSnapshot::default();

        engine.evaluate(&cpu(91.0), &empty_gpu(), &system);
        assert!(rx.try_recv().is_ok());
        engine.evaluate(&cpu(91.0), &empty_gpu(), &system);
        assert!(rx.try_recv().is_err());

        engine.reset_detection_state();
        engine.evaluate(&cpu(91.0), &empty_gpu(), &system);
        assert!(rx.try_recv().is_ok());
    }

    #[test]
    fn all_condition_triggers_when_thresholds_cross_at_different_times() {
        let rules = vec![Rule {
            id: "all".into(),
            name: "all".into(),
            enabled: true,
            condition: Condition::All(vec![
                Threshold {
                    metric: Metric::CpuTemp,
                    direction: Direction::Above,
                    warn: 80.0,
                    critical: 90.0,
                    hysteresis_pct: 5.0,
                    cooldown_secs: 0,
                },
                Threshold {
                    metric: Metric::GpuTemp,
                    direction: Direction::Above,
                    warn: 80.0,
                    critical: 90.0,
                    hysteresis_pct: 5.0,
                    cooldown_secs: 0,
                },
            ]),
            action: Action::Notify {
                title: "hot".into(),
                body: "hot".into(),
            },
            persistent: false,
        }];
        let (tx, mut rx) = mpsc::unbounded_channel();
        let engine = RuleEngine::new(
            Arc::new(RwLock::new(rules)),
            tx,
            Arc::new(RwLock::new(HashMap::new())),
        );
        let system = SystemSnapshot::default();
        let mut gpu = empty_gpu();

        // CPU crosses first; GPU is still below critical.
        engine.evaluate(&cpu(91.0), &gpu, &system);
        assert!(rx.try_recv().is_err());

        // GPU crosses on a later tick: combined condition must now fire.
        gpu.temp_c = Some(91);
        engine.evaluate(&cpu(91.0), &gpu, &system);
        assert!(rx.try_recv().is_ok());

        // No repeat while both stay critical.
        engine.evaluate(&cpu(91.0), &gpu, &system);
        assert!(rx.try_recv().is_err());
    }

    #[test]
    fn missing_sensor_never_triggers() {
        let (engine, mut rx) = engine_and_rx(0.0);
        let mut missing = cpu(0.0);
        missing.temp_c = None;
        engine.evaluate(&missing, &empty_gpu(), &SystemSnapshot::default());
        assert!(rx.try_recv().is_err());
    }

    #[test]
    fn battery_metric_reads_system_snapshot() {
        let rules = vec![Rule {
            id: "bat".into(),
            name: "bat".into(),
            enabled: true,
            condition: Condition::Any(vec![Threshold {
                metric: Metric::BatteryPct,
                direction: Direction::Below,
                warn: 20.0,
                critical: 15.0,
                hysteresis_pct: 5.0,
                cooldown_secs: 5,
            }]),
            action: Action::Notify {
                title: "low".into(),
                body: "low".into(),
            },
            persistent: false,
        }];
        let (tx, mut rx) = mpsc::unbounded_channel();
        let engine = RuleEngine::new(
            Arc::new(RwLock::new(rules)),
            tx,
            Arc::new(RwLock::new(HashMap::new())),
        );
        let system = SystemSnapshot {
            battery_pct: Some(10.0),
            ..SystemSnapshot::default()
        };
        engine.evaluate(&cpu(20.0), &empty_gpu(), &system);
        assert!(rx.try_recv().is_ok());
    }
}
