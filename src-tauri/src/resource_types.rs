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
    RamPct,
}

impl Metric {
    pub const ALL: [Metric; 9] = [
        Metric::CpuUtil,
        Metric::GpuUtil,
        Metric::CpuTemp,
        Metric::GpuTemp,
        Metric::GpuPowerW,
        Metric::GpuVramPct,
        Metric::FanRpm,
        Metric::BatteryPct,
        Metric::RamPct,
    ];

    /// Nilai aman minimum yang boleh dipakai sebagai ambang.
    pub fn min_value(self) -> f64 {
        match self {
            Metric::CpuTemp | Metric::GpuTemp => -20.0,
            _ => 0.0,
        }
    }

    /// Nilai aman maksimum yang boleh dipakai sebagai ambang.
    pub fn max_value(self) -> f64 {
        match self {
            Metric::CpuTemp | Metric::GpuTemp => 125.0,
            Metric::GpuPowerW => 1000.0,
            Metric::FanRpm => 20_000.0,
            Metric::CpuUtil
            | Metric::GpuUtil
            | Metric::GpuVramPct
            | Metric::BatteryPct
            | Metric::RamPct => 100.0,
        }
    }

    pub fn unit(self) -> &'static str {
        match self {
            Metric::CpuTemp | Metric::GpuTemp => "°C",
            Metric::GpuPowerW => "W",
            Metric::FanRpm => "RPM",
            _ => "%",
        }
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

/// Snapshot sensor sistem yang dipakai aturan non-CPU/GPU.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SystemSnapshot {
    pub fan1_rpm: u32,
    pub fan2_rpm: u32,
    pub battery_pct: Option<f64>,
    pub ram_pct: f64,
    pub plugged: Option<bool>,
}

impl Default for SystemSnapshot {
    fn default() -> Self {
        Self {
            fan1_rpm: 0,
            fan2_rpm: 0,
            battery_pct: None,
            ram_pct: 0.0,
            plugged: None,
        }
    }
}

/// Arah pelanggaran ambang: nilai terlalu tinggi atau terlalu rendah.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
pub enum Direction {
    #[default]
    Above,
    Below,
}

/// Ambang batas dengan hysteresis agar tidak oszilasi.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Threshold {
    pub metric: Metric,
    #[serde(default)]
    pub direction: Direction,
    /// Ambang peringatan sekaligus titik pulih (reset) setelah aksi terpicu.
    pub warn: f64,
    /// Ambang yang memicu aksi.
    pub critical: f64,
    /// Margin tambahan (%) melewati `warn` untuk mencegah osilasi di sekitar ambang.
    #[serde(default = "default_hysteresis")]
    pub hysteresis_pct: f64,
    #[serde(default = "default_cooldown")]
    pub cooldown_secs: u64,
}

fn default_hysteresis() -> f64 {
    5.0
}

fn default_cooldown() -> u64 {
    120
}

impl Threshold {
    /// Batas nilai saat aturan dianggap pulih.
    pub fn reset_value(&self) -> f64 {
        let margin = self.warn.abs() * (self.hysteresis_pct.clamp(0.0, 50.0) / 100.0);
        match self.direction {
            Direction::Above => (self.warn - margin).max(self.metric.min_value()),
            Direction::Below => (self.warn + margin).min(self.metric.max_value()),
        }
    }
}

/// Kondisi aturan: ALL (semua terpenuhi) atau ANY (salah satu).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Condition {
    All(Vec<Threshold>),
    Any(Vec<Threshold>),
}

impl Condition {
    pub fn thresholds(&self) -> &[Threshold] {
        match self {
            Condition::All(ts) | Condition::Any(ts) => ts,
        }
    }
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

impl Action {
    pub fn kind(&self) -> &'static str {
        match self {
            Action::SetThermalMode { .. } => "thermal-mode",
            Action::SetGpuPowerLimit { .. } => "gpu-power-limit",
            Action::SetGpuRuntimePm { .. } => "gpu-runtime-pm",
            Action::SetFanCurve { .. } => "fan-curve",
            Action::SetFanManual { .. } => "fan-manual",
            Action::SetFanAuto { .. } => "fan-auto",
            Action::Renice { .. } => "renice",
            Action::Notify { .. } => "notify",
        }
    }

    pub fn describe(&self) -> String {
        match self {
            Action::SetThermalMode { mode } => format!("Set thermal mode → {mode}"),
            Action::SetGpuPowerLimit { watts } => format!("Set GPU power limit → {watts} W"),
            Action::SetGpuRuntimePm { control } => format!("Set GPU runtime PM → {control}"),
            Action::SetFanCurve { fan_id, .. } => format!("Apply fan curve → fan {fan_id}"),
            Action::SetFanManual {
                fan_id,
                pwm_percent,
            } => {
                format!("Set fan {fan_id} manual → {pwm_percent}%")
            }
            Action::SetFanAuto { fan_id } => format!("Return fan {fan_id} to firmware control"),
            Action::Renice { pid, nice } => format!("Renice PID {pid} → {nice}"),
            Action::Notify { title, .. } => format!("Notify → {title}"),
        }
    }
}

/// Satu aturan otomatisasi sumber daya.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Rule {
    pub id: String,
    pub name: String,
    pub enabled: bool,
    pub condition: Condition,
    pub action: Action,
    #[serde(default)]
    pub persistent: bool,
}

/// Status runtime satu aturan (tidak dipersistensi; audit ada di SQLite).
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct RuleRuntime {
    pub trigger_count: u64,
    pub last_triggered_at: Option<u64>,
    pub last_error: Option<String>,
}

/// Ringkasan aturan untuk frontend.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RuleSummary {
    pub id: String,
    pub name: String,
    pub enabled: bool,
    pub condition_mode: String,
    pub metric: String,
    pub action: String,
    pub trigger_count: u64,
    pub last_triggered_at: Option<u64>,
    pub last_error: Option<String>,
}

/// Status governor yang diexpose ke frontend.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResourceStatus {
    pub cpu: CpuSnapshot,
    pub gpu: GpuSnapshot,
    pub system: SystemSnapshot,
    pub rules: Vec<RuleSummary>,
}

/// Satu catatan audit eksekusi aksi otomatisasi.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AutomationEvent {
    pub id: i64,
    pub timestamp: f64,
    pub rule_id: String,
    pub rule_name: String,
    pub action: String,
    pub status: String,
    pub detail: String,
}

/// Validasi otoritatif sebelum aturan disimpan atau dijalankan.
/// Semua batas di sini adalah pengaman keras: frontend tidak boleh melewatinya.
pub fn validate_rule(rule: &Rule) -> Result<(), String> {
    if rule.id.is_empty() || rule.id.len() > 64 {
        return Err("id harus 1–64 karakter".into());
    }
    if !rule
        .id
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
    {
        return Err("id hanya boleh huruf, angka, '-' dan '_'".into());
    }
    let name = rule.name.trim();
    if name.is_empty() || name.chars().count() > 80 {
        return Err("nama aturan harus 1–80 karakter".into());
    }

    let thresholds = rule.condition.thresholds();
    if thresholds.is_empty() {
        return Err("kondisi harus memiliki minimal satu ambang".into());
    }
    if thresholds.len() > 8 {
        return Err("maksimal 8 ambang per aturan".into());
    }
    for (index, t) in thresholds.iter().enumerate() {
        let min = t.metric.min_value();
        let max = t.metric.max_value();
        if !t.warn.is_finite() || !t.critical.is_finite() {
            return Err(format!("ambang #{} bukan angka valid", index + 1));
        }
        if t.warn < min || t.warn > max || t.critical < min || t.critical > max {
            return Err(format!(
                "ambang #{} harus berada di rentang {min}–{max} {}",
                index + 1,
                t.metric.unit()
            ));
        }
        match t.direction {
            Direction::Above if t.warn >= t.critical => {
                return Err(format!(
                    "ambang #{}: untuk arah 'Above', warn harus lebih rendah dari critical",
                    index + 1
                ));
            }
            Direction::Below if t.warn <= t.critical => {
                return Err(format!(
                    "ambang #{}: untuk arah 'Below', warn harus lebih tinggi dari critical",
                    index + 1
                ));
            }
            _ => {}
        }
        if !(0.0..=50.0).contains(&t.hysteresis_pct) {
            return Err(format!("ambang #{}: hysteresis 0–50%", index + 1));
        }
        if !(5..=86_400).contains(&t.cooldown_secs) {
            return Err(format!("ambang #{}: cooldown 5–86400 detik", index + 1));
        }
    }

    match &rule.action {
        Action::SetThermalMode { mode } => {
            if !["low-power", "quiet", "balanced", "performance", "max-power"]
                .contains(&mode.as_str())
            {
                return Err("mode thermal tidak dikenal".into());
            }
        }
        Action::SetGpuPowerLimit { watts } => {
            if !(30..=600).contains(watts) {
                return Err("batas daya GPU harus 30–600 W".into());
            }
        }
        Action::SetGpuRuntimePm { control } => {
            if control != "auto" && control != "on" {
                return Err("GPU runtime PM hanya 'auto' atau 'on'".into());
            }
        }
        Action::SetFanCurve {
            fan_id,
            points_json,
        } => {
            if *fan_id != 1 && *fan_id != 2 {
                return Err("fan_id harus 1 atau 2".into());
            }
            validate_fan_curve(points_json)?;
        }
        Action::SetFanManual {
            fan_id,
            pwm_percent,
        } => {
            if *fan_id != 1 && *fan_id != 2 {
                return Err("fan_id harus 1 atau 2".into());
            }
            if !(0..=100).contains(pwm_percent) {
                return Err("PWM manual harus 0–100%".into());
            }
        }
        Action::SetFanAuto { fan_id } => {
            if *fan_id != 1 && *fan_id != 2 {
                return Err("fan_id harus 1 atau 2".into());
            }
        }
        Action::Renice { pid, nice } => {
            if *pid <= 1 {
                return Err("renice menolak PID 1 atau lebih kecil".into());
            }
            if !(-20..=19).contains(nice) {
                return Err("nice harus -20…19".into());
            }
        }
        Action::Notify { title, body } => {
            if title.trim().is_empty() || title.chars().count() > 80 {
                return Err("judul notifikasi 1–80 karakter".into());
            }
            if body.chars().count() > 500 {
                return Err("isi notifikasi maksimal 500 karakter".into());
            }
        }
    }
    Ok(())
}

/// Validasi kurva kipas: 2–10 titik, suhu naik, PWM naik, semua dalam batas.
fn validate_fan_curve(points_json: &str) -> Result<(), String> {
    let points: Vec<serde_json::Value> = serde_json::from_str(points_json)
        .map_err(|_| "kurva kipas bukan JSON valid".to_string())?;
    if !(2..=10).contains(&points.len()) {
        return Err("kurva kipas harus 2–10 titik".into());
    }
    let mut last_temp = -1i64;
    let mut last_pwm = -1i64;
    for (index, point) in points.iter().enumerate() {
        let temp = point
            .get("temp")
            .and_then(|v| v.as_i64())
            .ok_or_else(|| format!("titik #{}: 'temp' tidak valid", index + 1))?;
        let pwm = point
            .get("pwm")
            .and_then(|v| v.as_i64())
            .ok_or_else(|| format!("titik #{}: 'pwm' tidak valid", index + 1))?;
        if !(0..=110).contains(&temp) {
            return Err(format!("titik #{}: suhu 0–110 °C", index + 1));
        }
        if !(0..=100).contains(&pwm) {
            return Err(format!("titik #{}: PWM 0–100%", index + 1));
        }
        if temp <= last_temp {
            return Err("suhu kurva harus naik secara ketat".into());
        }
        if pwm < last_pwm {
            return Err("PWM kurva tidak boleh turun".into());
        }
        last_temp = temp;
        last_pwm = pwm;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn threshold(metric: Metric, warn: f64, critical: f64) -> Threshold {
        Threshold {
            metric,
            direction: Direction::Above,
            warn,
            critical,
            hysteresis_pct: 5.0,
            cooldown_secs: 60,
        }
    }

    fn rule(action: Action) -> Rule {
        Rule {
            id: "test-rule".into(),
            name: "Test".into(),
            enabled: true,
            condition: Condition::Any(vec![threshold(Metric::CpuTemp, 85.0, 95.0)]),
            action,
            persistent: true,
        }
    }

    #[test]
    fn valid_rule_passes() {
        validate_rule(&rule(Action::SetThermalMode {
            mode: "quiet".into(),
        }))
        .unwrap();
    }

    #[test]
    fn rejects_inverted_threshold() {
        let mut r = rule(Action::SetFanAuto { fan_id: 1 });
        r.condition = Condition::Any(vec![threshold(Metric::CpuTemp, 95.0, 85.0)]);
        assert!(validate_rule(&r).is_err());
    }

    #[test]
    fn rejects_unknown_thermal_mode() {
        let r = rule(Action::SetThermalMode {
            mode: "; rm -rf /".into(),
        });
        assert!(validate_rule(&r).is_err());
    }

    #[test]
    fn rejects_unsafe_renice() {
        let r = rule(Action::Renice { pid: 1, nice: 19 });
        assert!(validate_rule(&r).is_err());
    }

    #[test]
    fn validates_fan_curve_ordering() {
        let r = rule(Action::SetFanCurve {
            fan_id: 1,
            points_json: r#"[{"temp":40,"pwm":20},{"temp":60,"pwm":40}]"#.into(),
        });
        assert!(validate_rule(&r).is_ok());

        let bad = rule(Action::SetFanCurve {
            fan_id: 1,
            points_json: r#"[{"temp":60,"pwm":20},{"temp":40,"pwm":40}]"#.into(),
        });
        assert!(validate_rule(&bad).is_err());
    }

    #[test]
    fn reset_value_applies_hysteresis() {
        let t = threshold(Metric::CpuTemp, 85.0, 95.0);
        assert!((t.reset_value() - 80.75).abs() < 0.001);
    }

    #[test]
    fn below_direction_requires_inverted_thresholds() {
        let mut r = rule(Action::Notify {
            title: "low battery".into(),
            body: "charge".into(),
        });
        r.condition = Condition::Any(vec![Threshold {
            metric: Metric::BatteryPct,
            direction: Direction::Below,
            warn: 22.0,
            critical: 15.0,
            hysteresis_pct: 5.0,
            cooldown_secs: 60,
        }]);
        validate_rule(&r).unwrap();
        if let Condition::Any(ts) = &mut r.condition {
            ts[0].warn = 10.0;
        }
        assert!(validate_rule(&r).is_err());
        assert!((r.condition.thresholds()[0].reset_value() - 22.0).abs() > 0.0);
    }
}
