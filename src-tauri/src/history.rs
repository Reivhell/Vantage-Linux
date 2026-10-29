use serde::Serialize;
use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

/// Satu titik data histori.
#[derive(Debug, Clone, Serialize)]
pub struct HistoryPoint {
    pub timestamp: f64,
    pub value: f64,
}

/// Seri metrik untuk grafik.
#[derive(Debug, Clone, Serialize)]
pub struct MetricSeries {
    pub points: Vec<HistoryPoint>,
}

/// Penyimpan histori metrik dengan roll-down otomatis.
/// Menyimpan semua poin dalam max_age_seconds; filter per window dilakukan saat query.
pub struct HistoryStore {
    points: Mutex<HashMap<String, Vec<HistoryPoint>>>,
    max_age_seconds: f64,
}

impl HistoryStore {
    pub fn new(max_age_seconds: f64) -> Self {
        Self {
            points: Mutex::new(HashMap::new()),
            max_age_seconds,
        }
    }

    /// Tambah satu poin untuk metrik.
    pub fn push(&self, metric: &str, value: f64) {
        let ts = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs_f64();
        let mut guard = self.points.lock().unwrap();
        let series = guard.entry(metric.to_string()).or_default();
        series.push(HistoryPoint {
            timestamp: ts,
            value,
        });
        self.prune(series, ts);
    }

    /// Hapus poin yang lebih tua dari max_age_seconds.
    fn prune(&self, series: &mut Vec<HistoryPoint>, now: f64) {
        let cutoff = now - self.max_age_seconds;
        // Drain the stale prefix in one shot instead of O(n) remove(0) loops.
        let keep = series
            .iter()
            .position(|p| p.timestamp >= cutoff)
            .unwrap_or(series.len());
        if keep > 0 {
            series.drain(..keep);
        }
    }

    /// Ambil sejumlah poin terakhir untuk sparkline.
    pub fn last(&self, metric: &str, limit: usize) -> Vec<f64> {
        let guard = self.points.lock().unwrap();
        let Some(series) = guard.get(metric) else {
            return Vec::new();
        };
        let start = series.len().saturating_sub(limit);
        series[start..].iter().map(|point| point.value).collect()
    }

    /// Ambil poin dalam window waktu (detik).
    pub fn get_window(&self, metric: &str, window_seconds: f64) -> MetricSeries {
        const MAX_WINDOW_POINTS: usize = 600;
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs_f64();
        let cutoff = now - window_seconds;
        let guard = self.points.lock().unwrap();
        let series = match guard.get(metric) {
            Some(s) => s,
            None => return MetricSeries { points: Vec::new() },
        };
        let mut points: Vec<HistoryPoint> = series
            .iter()
            .filter(|p| p.timestamp >= cutoff)
            .cloned()
            .collect();

        // Grafik hanya butuh ~600 titik. Downsample supaya window 24h tidak
        // mengirim puluhan ribu titik ke frontend setiap kali tab dibuka.
        let len = points.len();
        let step = (len + MAX_WINDOW_POINTS - 1) / MAX_WINDOW_POINTS;
        if step > 1 {
            let mut sampled = Vec::with_capacity(len / step + 1);
            let mut iter = points.into_iter().enumerate().peekable();
            while let Some((index, point)) = iter.next() {
                if index % step == 0 || iter.peek().is_none() {
                    sampled.push(point);
                }
            }
            points = sampled;
        }
        MetricSeries { points }
    }
}

static HISTORY: Mutex<Option<HistoryStore>> = Mutex::new(None);

/// Inisialisasi penyimpan histori (panggil sekali saat setup).
pub fn init_history(max_age_seconds: f64) {
    *HISTORY.lock().unwrap() = Some(HistoryStore::new(max_age_seconds));
}

/// Push nilai ke histori metrik.
pub fn history_push(metric: &str, value: f64) {
    if let Some(store) = HISTORY.lock().unwrap().as_ref() {
        store.push(metric, value);
    }
}

/// Ambil sejumlah poin terakhir untuk grafik.
pub fn history_last(metric: &str, limit: usize) -> Vec<f64> {
    HISTORY
        .lock()
        .unwrap()
        .as_ref()
        .map(|store| store.last(metric, limit))
        .unwrap_or_default()
}

/// Ambil histori metrik untuk window waktu (detik).
pub fn history_get(metric: &str, window_seconds: f64) -> Option<MetricSeries> {
    HISTORY
        .lock()
        .unwrap()
        .as_ref()
        .map(|s| s.get_window(metric, window_seconds))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_push_and_get_window() {
        let store = HistoryStore::new(3600.0);

        // Paksa timestamp lewat manipulasi internal: push 3 poin dengan jarak waktu.
        // Karena push menggunakan waktu sekarang, kita cukup verifikasi bahwa window
        // yang lebih kecil menghasilkan subset dari window yang lebih besar.
        store.push("cpu_temp", 50.0);
        store.push("cpu_temp", 55.0);
        store.push("cpu_temp", 60.0);

        let all = store.get_window("cpu_temp", 3600.0);
        assert!(all.points.len() >= 3);

        let small = store.get_window("cpu_temp", 1.0);
        assert!(small.points.len() <= all.points.len());
    }

    #[test]
    fn test_last_returns_tail() {
        let store = HistoryStore::new(3600.0);
        for value in [1.0, 2.0, 3.0, 4.0] {
            store.push("cpu", value);
        }
        assert_eq!(store.last("cpu", 2), vec![3.0, 4.0]);
    }

    #[test]
    fn test_unknown_metric_returns_empty() {
        let store = HistoryStore::new(3600.0);
        let series = store.get_window("nonexistent", 60.0);
        assert!(series.points.is_empty());
    }

    #[test]
    fn test_window_downsamples_large_series() {
        let store = HistoryStore::new(3600.0);
        for value in 0..2000 {
            store.push("temp", value as f64);
        }
        let series = store.get_window("temp", 3600.0);
        assert!(series.points.len() <= 601);
        assert!(series.points.len() > 400);
    }
}
