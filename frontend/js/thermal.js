// Screen 4: System Temperature monitoring & multi-line chart
import { $, escapeHtml, invoke } from "./core.js";

let thermalWindow = 60;

const fallbackSensor = (id, label) => ({ id, label, temp_c: null });

function sensorsFor(stats) {
  if (Array.isArray(stats.thermal) && stats.thermal.length) return stats.thermal;
  return [
    { id: "cpu", label: "CPU Package", temp_c: stats.cpu_temp },
    { id: "gpu", label: "GPU", temp_c: stats.gpu_temp },
    { id: "ssd", label: "NVMe", temp_c: stats.ssd_temp },
    { id: "ram", label: "RAM", temp_c: stats.ram_temp },
    { id: "wifi", label: "WiFi", temp_c: stats.wifi_temp },
    { id: "board", label: "EC / Board", temp_c: stats.board_temp },
  ].filter((s) => s.id !== undefined);
}

function sensorById(stats, id) {
  return sensorsFor(stats).find((sensor) => sensor.id === id) || fallbackSensor(id, id);
}

function badgeState(temp) {
  if (temp === null || temp === undefined) return { text: "N/A", color: "var(--text-muted)", bg: "rgba(255,255,255,0.06)" };
  if (temp >= 85) return { text: "Tinggi", color: "var(--error)", bg: "rgba(255, 101, 122, 0.15)" };
  if (temp >= 70) return { text: "Hangat", color: "var(--warning)", bg: "rgba(246, 189, 75, 0.15)" };
  return { text: "Normal", color: "var(--success)", bg: "rgba(39, 217, 149, 0.15)" };
}

function setGauge(ringId, valueId, badgeId, temp) {
  const ring = $(ringId);
  if (ring) {
    const c = 251;
    const pct = temp === null || temp === undefined ? 0 : Math.min(100, Math.max(0, temp));
    ring.style.strokeDasharray = `${c}`;
    ring.style.strokeDashoffset = `${c - (pct / 100) * c}`;
  }
  if ($(valueId)) $(valueId).textContent = temp === null || temp === undefined ? "--" : `${Math.round(temp)}°C`;
  const badge = $(badgeId);
  if (badge) {
    const state = badgeState(temp);
    badge.textContent = state.text;
    badge.style.color = state.color;
    badge.style.background = state.bg;
  }
}

export async function loadThermal() {
  try {
    const stats = await invoke("cmd_get_stats_snapshot");
    if (stats) updateThermalGauges(stats);
    await updateThermalChart();
  } catch (err) {
    console.error("loadThermal error:", err);
  }
}

export function updateThermalGauges(stats) {
  if (!stats) return;

  const cpu = sensorById(stats, "cpu").temp_c ?? null;
  const gpu = sensorById(stats, "gpu").temp_c ?? null;
  const ssd = sensorById(stats, "ssd").temp_c ?? null;

  setGauge("ring-cpu-temp", "gauge-temp-cpu", "badge-temp-cpu", cpu);
  setGauge("ring-gpu-temp", "gauge-temp-gpu", "badge-temp-gpu", gpu);
  setGauge("ring-ssd-temp", "gauge-temp-ssd", "badge-temp-ssd", ssd);

  const row = $("thermal-sensor-row");
  if (row) {
    row.innerHTML = sensorsFor(stats)
      .filter((sensor) => !["cpu", "gpu", "ssd"].includes(sensor.id))
      .map((sensor) => {
        const temp = sensor.temp_c;
        const state = badgeState(temp);
        return `<div class="thermal-sensor-item">
          <div><span class="thermal-sensor-label">${escapeHtml(sensor.label)}</span>
          <span class="thermal-sensor-source">${escapeHtml(sensor.source || "")}</span></div>
          <span class="thermal-sensor-temp mono">${temp === null || temp === undefined ? "--" : `${Math.round(temp)}°C`}</span>
          <span class="status-badge-pill" style="color:${state.color};background:${state.bg}">${state.text}</span>
        </div>`;
      })
      .join("");
  }
}


function setThermalXLabels() {
  for (let i = 0; i < 7; i += 1) {
    const el = $(`thermal-x-${i}`);
    if (!el) continue;
    const seconds = (thermalWindow * i) / 6;
    if (thermalWindow >= 4 * 3600) el.textContent = `${Math.round(seconds / 3600)}h`;
    else if (thermalWindow >= 120) el.textContent = `${Math.round(seconds / 60)}m`;
    else el.textContent = `${Math.round(seconds)}s`;
  }
}

function chartPoints(series) {
  if (!series || !series.points || series.points.length < 2) return "";
  const now = Date.now() / 1000;
  const start = now - thermalWindow;
  return series.points
    .map((point) => {
      const x = 40 + Math.max(0, Math.min(1, (point.timestamp - start) / thermalWindow)) * 550;
      const y = 131 - (Math.min(100, Math.max(0, point.value)) / 100) * 116;
      return `${x.toFixed(1)},${y.toFixed(1)}`;
    })
    .join(" ");
}

export async function updateThermalChart() {
  try {
    const [cpuHist, gpuHist, ssdHist] = await Promise.all([
      invoke("cmd_get_history", { metric: "cpu_temp", window_seconds: thermalWindow }),
      invoke("cmd_get_history", { metric: "gpu_temp", window_seconds: thermalWindow }),
      invoke("cmd_get_history", { metric: "ssd_temp", window_seconds: thermalWindow }),
    ]);
    const set = (id, series) => $(id)?.setAttribute("points", chartPoints(series));
    set("thermal-line-cpu", cpuHist);
    set("thermal-line-gpu", gpuHist);
    set("thermal-line-ssd", ssdHist);
    setThermalXLabels();
  } catch (err) {
    console.error("updateThermalChart error:", err);
  }
}

export function initThermal() {
  document.querySelectorAll("#thermal-chart-pills button").forEach((btn) => {
    btn.addEventListener("click", () => {
      document.querySelectorAll("#thermal-chart-pills button").forEach((b) => b.classList.remove("active"));
      btn.classList.add("active");
      thermalWindow = Number(btn.dataset.twin) || 60;
      updateThermalChart();
    });
  });

  $("pill-realtime")?.addEventListener("click", () => {
    $("pill-realtime")?.classList.add("active");
    $("pill-history")?.classList.remove("active");
    const gauges = $("thermal-gauges-wrap");
    if (gauges) gauges.style.display = "grid";
    const history = $("thermal-history-card");
    if (history) history.style.display = "block";
  });

  $("pill-history")?.addEventListener("click", () => {
    $("pill-history")?.classList.add("active");
    $("pill-realtime")?.classList.remove("active");
    const gauges = $("thermal-gauges-wrap");
    if (gauges) gauges.style.display = "none";
    const history = $("thermal-history-card");
    if (history) history.style.display = "block";
  });
}
