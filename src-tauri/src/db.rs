use rusqlite::{params, Connection};
use serde::Serialize;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use crate::resource_types::AutomationEvent;

/// Lokasi database SQLite lokal (XDG-compliant: ~/.local/share/vantage-linux/battery.db).
fn db_path() -> PathBuf {
    let home = std::env::var("HOME").unwrap_or_default();
    Path::new(&home)
        .join(".local/share/vantage-linux/battery.db")
        .to_path_buf()
}

fn ensure_parent(path: &Path) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    Ok(())
}

/// Pooled SQLite connection shared across the application.
/// rusqlite::Connection is Send but not Sync, so we guard it with a Mutex.
pub struct DbPool {
    conn: Mutex<Connection>,
}

impl DbPool {
    pub fn open() -> Result<Arc<Self>, String> {
        let path = db_path();
        ensure_parent(&path)?;
        let conn = Connection::open(&path).map_err(|e| e.to_string())?;
        configure_conn(&conn)?;
        init_schema(&conn)?;
        Ok(Arc::new(Self {
            conn: Mutex::new(conn),
        }))
    }

    fn lock(&self) -> std::sync::LockResult<std::sync::MutexGuard<'_, Connection>> {
        self.conn.lock()
    }

    #[allow(clippy::too_many_arguments)]
    pub fn record_sample(
        &self,
        percent: Option<f64>,
        health_pct: Option<f64>,
        cycles: Option<u32>,
        energy_full_wh: Option<f64>,
        energy_design_wh: Option<f64>,
        power_w: Option<f64>,
        status: &str,
        plugged: Option<bool>,
    ) -> Result<(), String> {
        let conn = self.lock().map_err(|e| e.to_string())?;
        record_sample_inner(
            &conn,
            percent,
            health_pct,
            cycles,
            energy_full_wh,
            energy_design_wh,
            power_w,
            status,
            plugged,
        )
    }

    pub fn get_history(&self, seconds: f64) -> Result<Vec<BatterySample>, String> {
        let conn = self.lock().map_err(|e| e.to_string())?;
        get_history_inner(&conn, seconds)
    }

    pub fn get_latest_trend(&self) -> Result<Option<BatteryTrend>, String> {
        let conn = self.lock().map_err(|e| e.to_string())?;
        get_latest_trend_inner(&conn)
    }

    pub fn get_rate(&self, window_seconds: f64) -> Result<Option<f64>, String> {
        let conn = self.lock().map_err(|e| e.to_string())?;
        get_rate_inner(&conn, window_seconds)
    }

    pub fn check_capacity_drop(
        &self,
        threshold_pct: f64,
    ) -> Result<Option<(f64, f64, f64)>, String> {
        let conn = self.lock().map_err(|e| e.to_string())?;
        check_capacity_drop_inner(&conn, threshold_pct)
    }

    pub fn export_csv(&self, path: &Path) -> Result<String, String> {
        let path = expand_home(path);
        let conn = self.lock().map_err(|e| e.to_string())?;
        let csv = export_csv_inner(&conn, &path)?;
        fs::write(&path, csv).map_err(|e| e.to_string())?;
        Ok(path.to_string_lossy().to_string())
    }

    pub fn record_charge_event(
        &self,
        start_pct: f64,
        end_pct: f64,
        duration_min: f64,
        energy_wh: f64,
        rate_w: f64,
    ) -> Result<(), String> {
        let conn = self.lock().map_err(|e| e.to_string())?;
        record_charge_event_inner(&conn, start_pct, end_pct, duration_min, energy_wh, rate_w)
    }

    /// Buang sample lebih lama dari `retention_seconds` lalu VACUUM.
    pub fn prune_retention(&self, retention_seconds: f64) -> Result<(), String> {
        let conn = self.lock().map_err(|e| e.to_string())?;
        let now = now_secs();
        let cutoff = now - retention_seconds;
        conn.execute(
            "DELETE FROM battery_samples WHERE timestamp < ?1",
            params![cutoff],
        )
        .map_err(|e| e.to_string())?;
        conn.execute_batch("VACUUM").map_err(|e| e.to_string())?;
        Ok(())
    }

    /// Catat satu eksekusi aksi otomatisasi ke audit log lokal.
    pub fn record_automation_event(
        &self,
        rule_id: &str,
        rule_name: &str,
        action: &str,
        status: &str,
        detail: &str,
    ) -> Result<(), String> {
        let conn = self.lock().map_err(|e| e.to_string())?;
        record_automation_event_inner(&conn, rule_id, rule_name, action, status, detail)
    }

    /// Ambil audit terbaru (maksimal `limit`).
    pub fn recent_automation_events(&self, limit: usize) -> Result<Vec<AutomationEvent>, String> {
        let conn = self.lock().map_err(|e| e.to_string())?;
        recent_automation_events_inner(&conn, limit)
    }

    pub fn clear_automation_events(&self) -> Result<usize, String> {
        let conn = self.lock().map_err(|e| e.to_string())?;
        conn.execute("DELETE FROM automation_events", [])
            .map_err(|e| e.to_string())
    }

    /// Hapus seluruh riwayat lokal (baterai, event pengisian, audit otomatisasi).
    pub fn clear_local_history(&self) -> Result<(), String> {
        let conn = self.lock().map_err(|e| e.to_string())?;
        conn.execute_batch(
            "DELETE FROM battery_samples;
             DELETE FROM charge_events;
             DELETE FROM automation_events;
             VACUUM;",
        )
        .map_err(|e| e.to_string())
    }

    /// Hapus audit lebih lama dari `retention_seconds`.
    pub fn prune_automation_events(&self, retention_seconds: f64) -> Result<usize, String> {
        let conn = self.lock().map_err(|e| e.to_string())?;
        let cutoff = now_secs() - retention_seconds;
        conn.execute(
            "DELETE FROM automation_events WHERE timestamp < ?1",
            params![cutoff],
        )
        .map_err(|e| e.to_string())
    }
}

pub fn open_conn() -> Result<Connection, String> {
    let path = db_path();
    ensure_parent(&path)?;
    let conn = Connection::open(&path).map_err(|e| e.to_string())?;
    configure_conn(&conn)?;
    Ok(conn)
}

/// PRAGMA produksi: WAL untuk pembacaan UI tanpa memblokir monitor loop,
/// busy_timeout agar penulisan bersamaan menunggu alih-alih gagal.
fn configure_conn(conn: &Connection) -> Result<(), String> {
    conn.busy_timeout(Duration::from_secs(5))
        .map_err(|e| e.to_string())?;
    conn.execute_batch(
        "PRAGMA journal_mode=WAL;
         PRAGMA synchronous=NORMAL;
         PRAGMA foreign_keys=ON;",
    )
    .map_err(|e| e.to_string())
}

fn now_secs() -> f64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs_f64()
}

pub fn init_schema(conn: &Connection) -> Result<(), String> {
    conn.execute_batch(
        r#"
        CREATE TABLE IF NOT EXISTS battery_samples (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            timestamp REAL NOT NULL,
            percent REAL,
            health_pct REAL,
            cycles INTEGER,
            energy_full_wh REAL,
            energy_design_wh REAL,
            power_w REAL,
            status TEXT,
            plugged INTEGER
        );
        CREATE INDEX IF NOT EXISTS idx_battery_samples_ts ON battery_samples(timestamp);

        CREATE TABLE IF NOT EXISTS charge_events (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            timestamp REAL NOT NULL,
            start_pct REAL,
            end_pct REAL,
            duration_min REAL,
            energy_wh REAL,
            rate_w REAL
        );
        CREATE INDEX IF NOT EXISTS idx_charge_events_ts ON charge_events(timestamp);

        CREATE TABLE IF NOT EXISTS automation_events (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            timestamp REAL NOT NULL,
            rule_id TEXT NOT NULL,
            rule_name TEXT NOT NULL,
            action TEXT NOT NULL,
            status TEXT NOT NULL,
            detail TEXT NOT NULL
        );
        CREATE INDEX IF NOT EXISTS idx_automation_events_ts
            ON automation_events(timestamp DESC);
        "#,
    )
    .map_err(|e| e.to_string())
}

/// Global connection pool initialized once at startup.
static DB_POOL: OnceLock<Arc<DbPool>> = OnceLock::new();

pub fn init_pool() -> Result<(), String> {
    let pool = DbPool::open()?;
    let _ = DB_POOL.set(pool);
    Ok(())
}

fn pool() -> Result<Arc<DbPool>, String> {
    DB_POOL
        .get()
        .cloned()
        .ok_or_else(|| "database pool not initialized".to_string())
}

/// Free-function wrappers (menggunakan pool global) untuk kompatibilitas dengan
/// command dan monitor loop yang tidak memegang AppState.
#[allow(clippy::too_many_arguments)]
pub fn record_sample(
    percent: Option<f64>,
    health_pct: Option<f64>,
    cycles: Option<u32>,
    energy_full_wh: Option<f64>,
    energy_design_wh: Option<f64>,
    power_w: Option<f64>,
    status: &str,
    plugged: Option<bool>,
) -> Result<(), String> {
    pool()?.record_sample(
        percent,
        health_pct,
        cycles,
        energy_full_wh,
        energy_design_wh,
        power_w,
        status,
        plugged,
    )
}

pub fn get_history(seconds: f64) -> Result<Vec<BatterySample>, String> {
    pool()?.get_history(seconds)
}

pub fn get_latest_trend() -> Result<Option<BatteryTrend>, String> {
    pool()?.get_latest_trend()
}

pub fn get_rate(window_seconds: f64) -> Result<Option<f64>, String> {
    pool()?.get_rate(window_seconds)
}

pub fn check_capacity_drop(threshold_pct: f64) -> Result<Option<(f64, f64, f64)>, String> {
    pool()?.check_capacity_drop(threshold_pct)
}

pub fn export_csv(path: &Path) -> Result<String, String> {
    pool()?.export_csv(path)
}

/// Catat satu sesi pengisian baterai memakai pool global.
pub fn record_charge_event(
    start_pct: f64,
    end_pct: f64,
    duration_min: f64,
    energy_wh: f64,
    rate_w: f64,
) -> Result<(), String> {
    pool()?.record_charge_event(start_pct, end_pct, duration_min, energy_wh, rate_w)
}

pub fn init_db() -> Result<(), String> {
    let conn = open_conn()?;
    init_schema(&conn)
}

/// Catat audit eksekusi aksi otomatisasi memakai pool global.
pub fn record_automation_event(
    rule_id: &str,
    rule_name: &str,
    action: &str,
    status: &str,
    detail: &str,
) -> Result<(), String> {
    pool()?.record_automation_event(rule_id, rule_name, action, status, detail)
}

pub fn recent_automation_events(limit: usize) -> Result<Vec<AutomationEvent>, String> {
    pool()?.recent_automation_events(limit)
}

pub fn clear_automation_events() -> Result<usize, String> {
    pool()?.clear_automation_events()
}

pub fn clear_local_history() -> Result<(), String> {
    pool()?.clear_local_history()
}

/// Pembersihan retensi dijalankan sekali saat startup.
pub fn prune_all(battery_retention: f64, event_retention: f64) -> Result<(), String> {
    let pool = pool()?;
    pool.prune_retention(battery_retention)?;
    pool.prune_automation_events(event_retention)?;
    Ok(())
}

// --- Internal helpers (beroperasi pada &Connection, mudah diuji dengan in-memory DB) ---

#[allow(clippy::too_many_arguments)]
fn record_sample_inner(
    conn: &Connection,
    percent: Option<f64>,
    health_pct: Option<f64>,
    cycles: Option<u32>,
    energy_full_wh: Option<f64>,
    energy_design_wh: Option<f64>,
    power_w: Option<f64>,
    status: &str,
    plugged: Option<bool>,
) -> Result<(), String> {
    let ts = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs_f64();
    conn.execute(
        "INSERT INTO battery_samples (timestamp, percent, health_pct, cycles, energy_full_wh,
         energy_design_wh, power_w, status, plugged)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
        params![
            ts,
            percent,
            health_pct,
            cycles.map(|c| c as i64),
            energy_full_wh,
            energy_design_wh,
            power_w,
            status,
            plugged.map(|p| p as i64),
        ],
    )
    .map_err(|e| e.to_string())?;
    Ok(())
}

#[derive(Debug, Clone, Serialize)]
pub struct BatterySample {
    pub timestamp: f64,
    pub percent: Option<f64>,
    pub health_pct: Option<f64>,
    pub cycles: Option<u32>,
    pub energy_full_wh: Option<f64>,
    pub energy_design_wh: Option<f64>,
    pub power_w: Option<f64>,
    pub status: Option<String>,
    pub plugged: Option<bool>,
}

fn get_history_inner(conn: &Connection, seconds: f64) -> Result<Vec<BatterySample>, String> {
    const MAX_POINTS: usize = 600;
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs_f64();
    let since = now - seconds;
    let mut stmt = conn
        .prepare(
            "SELECT timestamp, percent, health_pct, cycles, energy_full_wh, energy_design_wh,
                power_w, status, plugged
         FROM battery_samples
         WHERE timestamp >= ?1
         ORDER BY timestamp ASC",
        )
        .map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map(params![since], |row| {
            Ok(BatterySample {
                timestamp: row.get(0)?,
                percent: row.get(1)?,
                health_pct: row.get(2)?,
                cycles: row.get::<_, Option<i64>>(3)?.map(|v| v as u32),
                energy_full_wh: row.get(4)?,
                energy_design_wh: row.get(5)?,
                power_w: row.get(6)?,
                status: row.get(7)?,
                plugged: row.get::<_, Option<i64>>(8)?.map(|v| v == 1),
            })
        })
        .map_err(|e| e.to_string())?;
    let mut out = Vec::new();
    for r in rows {
        out.push(r.map_err(|e| e.to_string())?);
    }

    // Grafik baterai tidak butuh puluhan ribu titik untuk window 30 hari.
    let len = out.len();
    let step = (len + MAX_POINTS - 1) / MAX_POINTS;
    if step > 1 {
        let mut sampled = Vec::with_capacity(len / step + 1);
        let mut iter = out.into_iter().enumerate().peekable();
        while let Some((index, sample)) = iter.next() {
            if index % step == 0 || iter.peek().is_none() {
                sampled.push(sample);
            }
        }
        out = sampled;
    }
    Ok(out)
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct BatteryTrend {
    pub design_wh: Option<f64>,
    pub full_wh: Option<f64>,
    pub health_pct: Option<f64>,
    pub cycles: Option<u32>,
    pub timestamp: f64,
}

fn get_latest_trend_inner(conn: &Connection) -> Result<Option<BatteryTrend>, String> {
    let mut stmt = conn
        .prepare(
            "SELECT energy_design_wh, energy_full_wh, health_pct, cycles, timestamp
         FROM battery_samples
         WHERE health_pct IS NOT NULL OR energy_full_wh IS NOT NULL
         ORDER BY timestamp DESC LIMIT 1",
        )
        .map_err(|e| e.to_string())?;
    let mut rows = stmt
        .query_map([], |row| {
            Ok(BatteryTrend {
                design_wh: row.get(0)?,
                full_wh: row.get(1)?,
                health_pct: row.get(2)?,
                cycles: row.get::<_, Option<i64>>(3)?.map(|v| v as u32),
                timestamp: row.get(4)?,
            })
        })
        .map_err(|e| e.to_string())?;
    match rows.next() {
        Some(r) => Ok(Some(r.map_err(|e| e.to_string())?)),
        None => Ok(None),
    }
}

fn get_rate_inner(conn: &Connection, window_seconds: f64) -> Result<Option<f64>, String> {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs_f64();
    let since = now - window_seconds;
    let mut stmt = conn
        .prepare(
            "SELECT timestamp, power_w FROM battery_samples
         WHERE timestamp >= ?1 AND power_w IS NOT NULL
         ORDER BY timestamp ASC",
        )
        .map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map(params![since], |row| {
            Ok((row.get::<_, f64>(0)?, row.get::<_, Option<f64>>(1)?))
        })
        .map_err(|e| e.to_string())?;
    let samples: Vec<(f64, Option<f64>)> = rows
        .map(|r| r.map_err(|e| e.to_string()))
        .collect::<Result<Vec<_>, String>>()?;
    if samples.len() < 2 {
        return Ok(None);
    }
    let last = samples.last().unwrap();
    Ok(last.1)
}

fn record_charge_event_inner(
    conn: &Connection,
    start_pct: f64,
    end_pct: f64,
    duration_min: f64,
    energy_wh: f64,
    rate_w: f64,
) -> Result<(), String> {
    let ts = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs_f64();
    conn.execute(
        "INSERT INTO charge_events (timestamp, start_pct, end_pct, duration_min, energy_wh, rate_w)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        params![ts, start_pct, end_pct, duration_min, energy_wh, rate_w],
    )
    .map_err(|e| e.to_string())?;
    Ok(())
}

pub fn export_csv_inner(conn: &Connection, _path: &Path) -> Result<String, String> {
    let mut stmt = conn
        .prepare(
            "SELECT timestamp, percent, health_pct, cycles, energy_full_wh, energy_design_wh,
                power_w, status, plugged
         FROM battery_samples
         ORDER BY timestamp ASC",
        )
        .map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map([], |row| {
            Ok((
                row.get::<_, f64>(0)?,
                row.get::<_, Option<f64>>(1)?,
                row.get::<_, Option<f64>>(2)?,
                row.get::<_, Option<i64>>(3)?,
                row.get::<_, Option<f64>>(4)?,
                row.get::<_, Option<f64>>(5)?,
                row.get::<_, Option<f64>>(6)?,
                row.get::<_, Option<String>>(7)?,
                row.get::<_, Option<i64>>(8)?,
            ))
        })
        .map_err(|e| e.to_string())?;
    let mut out = String::from(
        "timestamp,percent,health_pct,cycles,energy_full_wh,energy_design_wh,power_w,status,plugged\n",
    );
    for r in rows {
        let (ts, pct, health, cycles, full, design, power, status, plugged) =
            r.map_err(|e| e.to_string())?;
        out.push_str(&format!(
            "{},{},{},{},{},{},{},{},{}\n",
            ts,
            pct.map(|v| v.to_string()).unwrap_or_default(),
            health.map(|v| v.to_string()).unwrap_or_default(),
            cycles.map(|c| c.to_string()).unwrap_or_default(),
            full.map(|v| v.to_string()).unwrap_or_default(),
            design.map(|v| v.to_string()).unwrap_or_default(),
            power.map(|v| v.to_string()).unwrap_or_default(),
            status.unwrap_or_default(),
            plugged.map(|p| p.to_string()).unwrap_or_default(),
        ));
    }

    // Sesi pengisian disimpan di tabel terpisah; sertakan sebagai bagian kedua
    // dengan header sendiri agar tetap terbaca tanpa mencampur kolom sampel.
    let mut charge_stmt = conn
        .prepare(
            "SELECT timestamp, start_pct, end_pct, duration_min, energy_wh, rate_w
             FROM charge_events ORDER BY timestamp ASC",
        )
        .map_err(|e| e.to_string())?;
    let charge_rows = charge_stmt
        .query_map([], |row| {
            Ok((
                row.get::<_, f64>(0)?,
                row.get::<_, Option<f64>>(1)?,
                row.get::<_, Option<f64>>(2)?,
                row.get::<_, Option<f64>>(3)?,
                row.get::<_, Option<f64>>(4)?,
                row.get::<_, Option<f64>>(5)?,
            ))
        })
        .map_err(|e| e.to_string())?;
    out.push_str("\ncharge_timestamp,start_pct,end_pct,duration_min,energy_wh,rate_w\n");
    for r in charge_rows {
        let (ts, start, end, duration, energy, rate) = r.map_err(|e| e.to_string())?;
        out.push_str(&format!(
            "{},{},{},{},{},{}\n",
            ts,
            start.map(|v| v.to_string()).unwrap_or_default(),
            end.map(|v| v.to_string()).unwrap_or_default(),
            duration.map(|v| v.to_string()).unwrap_or_default(),
            energy.map(|v| v.to_string()).unwrap_or_default(),
            rate.map(|v| v.to_string()).unwrap_or_default(),
        ));
    }
    Ok(out)
}

fn record_automation_event_inner(
    conn: &Connection,
    rule_id: &str,
    rule_name: &str,
    action: &str,
    status: &str,
    detail: &str,
) -> Result<(), String> {
    conn.execute(
        "INSERT INTO automation_events (timestamp, rule_id, rule_name, action, status, detail)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        params![now_secs(), rule_id, rule_name, action, status, detail],
    )
    .map_err(|e| e.to_string())?;
    Ok(())
}

fn recent_automation_events_inner(
    conn: &Connection,
    limit: usize,
) -> Result<Vec<AutomationEvent>, String> {
    let limit = limit.clamp(1, 1000) as i64;
    let mut stmt = conn
        .prepare(
            "SELECT id, timestamp, rule_id, rule_name, action, status, detail
             FROM automation_events ORDER BY timestamp DESC, id DESC LIMIT ?1",
        )
        .map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map(params![limit], |row| {
            Ok(AutomationEvent {
                id: row.get(0)?,
                timestamp: row.get(1)?,
                rule_id: row.get(2)?,
                rule_name: row.get(3)?,
                action: row.get(4)?,
                status: row.get(5)?,
                detail: row.get(6)?,
            })
        })
        .map_err(|e| e.to_string())?;
    let mut out = Vec::new();
    for row in rows {
        out.push(row.map_err(|e| e.to_string())?);
    }
    Ok(out)
}

fn expand_home(path: &Path) -> PathBuf {
    let text = path.to_string_lossy();
    if text == "~" {
        return PathBuf::from(std::env::var("HOME").unwrap_or_default());
    }
    match text.strip_prefix("~/") {
        Some(rest) => Path::new(&std::env::var("HOME").unwrap_or_default()).join(rest),
        None => path.to_path_buf(),
    }
}

/// Cek apakah health mengalami penurunan dari baseline (sample paling awal vs terbaru).
fn check_capacity_drop_inner(
    conn: &Connection,
    threshold_pct: f64,
) -> Result<Option<(f64, f64, f64)>, String> {
    let baseline: Option<f64> = conn
        .query_row(
            "SELECT health_pct FROM battery_samples WHERE health_pct IS NOT NULL
             ORDER BY timestamp ASC LIMIT 1",
            [],
            |row| row.get::<_, Option<f64>>(0),
        )
        .ok()
        .flatten();
    let latest: Option<f64> = conn
        .query_row(
            "SELECT health_pct FROM battery_samples WHERE health_pct IS NOT NULL
             ORDER BY timestamp DESC LIMIT 1",
            [],
            |row| row.get::<_, Option<f64>>(0),
        )
        .ok()
        .flatten();
    match (baseline, latest) {
        (Some(b), Some(l)) if b > 0.0 => {
            let drop = b - l;
            if drop >= threshold_pct {
                Ok(Some((b, l, drop)))
            } else {
                Ok(None)
            }
        }
        _ => Ok(None),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_conn() -> Connection {
        Connection::open_in_memory().unwrap()
    }

    fn init_test_conn(conn: &Connection) {
        init_schema(conn).unwrap();
    }

    #[test]
    fn test_init_schema_creates_tables() {
        let conn = test_conn();
        init_test_conn(&conn);
        let mut stmt = conn
            .prepare("SELECT name FROM sqlite_master WHERE type='table' ORDER BY name")
            .unwrap();
        let tables: Vec<String> = stmt
            .query_map([], |row| row.get::<_, String>(0))
            .unwrap()
            .map(|r| r.unwrap())
            .collect();
        assert!(tables.contains(&"battery_samples".to_string()));
        assert!(tables.contains(&"charge_events".to_string()));
        assert!(tables.contains(&"automation_events".to_string()));
    }

    #[test]
    fn test_record_and_get_history_roundtrip() {
        let conn = test_conn();
        init_test_conn(&conn);

        let ts0 = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs_f64();
        record_sample_inner(
            &conn,
            Some(80.0),
            Some(90.0),
            Some(5),
            Some(50.0),
            Some(55.0),
            Some(10.0),
            "Discharging",
            Some(false),
        )
        .unwrap();
        record_sample_inner(
            &conn,
            Some(75.0),
            Some(90.0),
            Some(5),
            Some(50.0),
            Some(55.0),
            Some(12.0),
            "Discharging",
            Some(false),
        )
        .unwrap();

        let hist = get_history_inner(&conn, 3600.0).unwrap();
        assert!(hist.len() >= 2);
        assert_eq!(hist[0].percent, Some(80.0));
        assert_eq!(hist[0].health_pct, Some(90.0));
        assert_eq!(hist[0].cycles, Some(5));
        assert_eq!(hist[0].status.as_deref(), Some("Discharging"));
        assert_eq!(hist[0].plugged, Some(false));
        assert!(hist[0].timestamp <= ts0 + 1.0);
    }

    #[test]
    fn test_battery_history_downsamples_large_window() {
        let conn = test_conn();
        init_test_conn(&conn);
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs_f64();
        for i in 0..2000 {
            conn.execute(
                "INSERT INTO battery_samples (timestamp, percent) VALUES (?1, ?2)",
                params![now - 2000.0 + i as f64, (i % 100) as f64],
            )
            .unwrap();
        }
        let hist = get_history_inner(&conn, 3600.0).unwrap();
        assert!(hist.len() <= 601);
        assert!(hist.len() > 400);
        assert_eq!(hist.last().and_then(|s| s.percent), Some(99.0));
    }

    #[test]
    fn test_get_latest_trend() {
        let conn = test_conn();
        init_test_conn(&conn);
        assert!(get_latest_trend_inner(&conn).unwrap().is_none());

        record_sample_inner(
            &conn,
            Some(50.0),
            Some(85.0),
            Some(10),
            Some(45.0),
            Some(50.0),
            None,
            "Charging",
            Some(true),
        )
        .unwrap();
        let trend = get_latest_trend_inner(&conn).unwrap().unwrap();
        assert_eq!(trend.health_pct, Some(85.0));
        assert_eq!(trend.cycles, Some(10));
        assert_eq!(trend.design_wh, Some(50.0));
        assert_eq!(trend.full_wh, Some(45.0));
    }

    #[test]
    fn test_export_csv_contains_header_and_rows() {
        let conn = test_conn();
        init_test_conn(&conn);
        record_sample_inner(
            &conn,
            Some(90.0),
            Some(95.0),
            None,
            None,
            None,
            None,
            "Charging",
            Some(true),
        )
        .unwrap();
        let csv = export_csv_inner(&conn, Path::new("/tmp/vantage_test.csv")).unwrap();
        assert!(csv.starts_with("timestamp,percent,health_pct,cycles,"));
        assert!(csv.contains("90"));
        assert!(csv.contains("95"));
        assert!(csv.contains("Charging"));
        assert!(csv.contains("charge_timestamp,start_pct,end_pct"));
    }

    #[test]
    fn test_export_csv_includes_charge_events() {
        let conn = test_conn();
        init_test_conn(&conn);
        record_charge_event_inner(&conn, 30.0, 80.0, 60.0, 25.0, 25.0).unwrap();
        let csv = export_csv_inner(&conn, Path::new("/tmp/loq_test.csv")).unwrap();
        let section = csv.split("charge_timestamp").nth(1).unwrap();
        assert!(section.contains("30"));
        assert!(section.contains("80"));
        assert!(section.contains("25"));
    }

    #[test]
    fn test_check_capacity_drop_detects_degradation() {
        let conn = test_conn();
        init_test_conn(&conn);
        record_sample_inner(
            &conn,
            Some(100.0),
            Some(100.0),
            None,
            None,
            None,
            None,
            "Charging",
            Some(true),
        )
        .unwrap();
        record_sample_inner(
            &conn,
            Some(80.0),
            Some(75.0),
            None,
            None,
            None,
            None,
            "Discharging",
            Some(false),
        )
        .unwrap();
        let result = check_capacity_drop_inner(&conn, 20.0).unwrap();
        assert!(result.is_some());
        let (base, latest, drop) = result.unwrap();
        assert_eq!(base, 100.0);
        assert_eq!(latest, 75.0);
        assert_eq!(drop, 25.0);

        let result2 = check_capacity_drop_inner(&conn, 30.0).unwrap();
        assert_eq!(result2, None);
    }

    #[test]
    fn test_automation_events_roundtrip_and_clear() {
        let conn = test_conn();
        init_test_conn(&conn);
        let pool = DbPool {
            conn: Mutex::new(conn),
        };

        pool.record_automation_event(
            "rule-1",
            "GPU Hot Guard",
            "set_fan_auto",
            "ok",
            "fan mode auto",
        )
        .unwrap();
        pool.record_automation_event(
            "rule-2",
            "Battery Guard",
            "notify",
            "error",
            "notify-send missing",
        )
        .unwrap();

        let events = pool.recent_automation_events(10).unwrap();
        assert_eq!(events.len(), 2);
        assert_eq!(events[0].rule_id, "rule-2");
        assert_eq!(events[0].status, "error");
        assert!(events[0].timestamp > 0.0);

        assert_eq!(pool.prune_automation_events(86_400.0).unwrap(), 0);
        assert_eq!(pool.clear_automation_events().unwrap(), 2);
        assert!(pool.recent_automation_events(10).unwrap().is_empty());

        record_sample_inner(
            &pool.conn.lock().unwrap(),
            Some(50.0),
            None,
            None,
            None,
            None,
            None,
            "Discharging",
            Some(false),
        )
        .unwrap();
        pool.clear_local_history().unwrap();
        assert!(pool.get_history(86_400.0).unwrap().is_empty());
    }

    #[test]
    fn test_record_charge_event() {
        let conn = test_conn();
        init_test_conn(&conn);
        record_charge_event_inner(&conn, 30.0, 80.0, 60.0, 25.0, 25.0).unwrap();
        let mut stmt = conn.prepare("SELECT COUNT(*) FROM charge_events").unwrap();
        let count: u64 = stmt.query_row([], |row| row.get(0)).unwrap();
        assert_eq!(count, 1);
    }

    #[test]
    fn test_export_path_expands_home() {
        let home = std::env::var("HOME").unwrap_or_default();
        let expanded = expand_home(Path::new("~/battery.csv"));
        assert!(expanded.starts_with(&home));
        assert!(expanded.ends_with("battery.csv"));
    }
}
