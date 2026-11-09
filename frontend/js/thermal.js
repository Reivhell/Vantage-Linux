// Screen 4: System Temperature monitoring & multi-line chart
import { $, invoke } from "./core.js";

let thermalWindow = 60; // 1m default

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

  const cpu = stats.cpu_temp == null ? null : Math.round(stats.cpu_temp);
  const gpu = stats.gpu_temp == null ? null : Math.round(stats.gpu_temp);
  const ssd = stats.ssd_temp == null ? null : Math.round(stats.ssd_temp);

  if ($("gauge-temp-cpu")) $("gauge-temp-cpu").textContent = cpu == null ? "--" : `${cpu}°C`;
  if ($("gauge-temp-gpu")) $("gauge-temp-gpu").textContent = gpu == null ? "--" : `${gpu}°C`;
  if ($("gauge-temp-ssd")) $("gauge-temp-ssd").textContent = ssd == null ? "--" : `${ssd}°C`;

  // Circumference for r=40 is 2 * Math.PI * 40 ~= 251.3
  const c = 251;
  const setRing = (id, temp) => {
    const ring = $(id);
    if (!ring) return;
    const pct = Math.min(100, Math.max(0, (temp / 100) * 100));
    const offset = c - (pct / 100) * c;
    ring.style.strokeDasharray = `${c}`;
    ring.style.strokeDashoffset = `${offset}`;
  };

  if (cpu != null) setRing("ring-cpu-temp", cpu);
  if (gpu != null) setRing("ring-gpu-temp", gpu);
  if (ssd != null) setRing("ring-ssd-temp", ssd);

  const updateBadge = (id, temp) => {
    const el = $(id);
    if (!el) return;
    if (temp == null) {
      el.textContent = "--";
      el.style.color = "var(--text-muted)";
      el.style.background = "transparent";
    } else if (temp >= 85) {
      el.textContent = "Tinggi";
      el.style.color = "var(--error)";
      el.style.background = "rgba(255, 101, 122, 0.15)";
    } else if (temp >= 70) {
      el.textContent = "Hangat";
      el.style.color = "var(--warning)";
      el.style.background = "rgba(246, 189, 75, 0.15)";
    } else {
      el.textContent = "Normal";
      el.style.color = "var(--success)";
      el.style.background = "rgba(39, 217, 149, 0.15)";
    }
  };

  updateBadge("badge-temp-cpu", cpu);
  updateBadge("badge-temp-gpu", gpu);
  updateBadge("badge-temp-ssd", ssd);
}

export async function updateThermalChart() {
  try {
    const [cpuHist, gpuHist, ssdHist] = await Promise.all([
      invoke("cmd_get_history", { metric: "cpu_temp", windowSeconds: thermalWindow }),
      invoke("cmd_get_history", { metric: "gpu_temp", windowSeconds: thermalWindow }),
      invoke("cmd_get_history", { metric: "ssd_temp", windowSeconds: thermalWindow }),
    ]);

    const draw = (id, hist) => {
      const line = $(id);
      if (!line) return;
      if (!hist?.points || hist.points.length < 2) {
        line.setAttribute("points", "");
        return;
      }
      const now = Date.now() / 1000;
      const start = now - thermalWindow;
      const pts = hist.points.map((p) => {
        const x = 40 + Math.max(0, Math.min(1, (p.timestamp - start) / thermalWindow)) * 550;
        const y = 131 - (Math.min(100, p.value) / 100) * 116;
        return `${x.toFixed(1)},${y.toFixed(1)}`;
      });
      line.setAttribute("points", pts.join(" "));
    };

    // Coordinate space: X in [40, 590], Y in [131 (0°C) to 15 (100°C)]
    draw("thermal-line-cpu", cpuHist);
    draw("thermal-line-gpu", gpuHist);
    draw("thermal-line-ssd", ssdHist);
  } catch (e) {
    console.warn("thermal history unavailable:", e);
  }
}

export function initThermal() {
  // Time window selector pills (1m, 15m, 1h, 24h)
  document.querySelectorAll("#thermal-chart-pills button").forEach((btn) => {
    btn.addEventListener("click", () => {
      document.querySelectorAll("#thermal-chart-pills button").forEach((b) => b.classList.remove("active"));
      btn.classList.add("active");
      thermalWindow = Number(btn.dataset.twin) || 60;
      updateThermalChart();
    });
  });

  // Header Mode pills: Real-time vs Riwayat
  $("pill-realtime")?.addEventListener("click", () => {
    $("pill-realtime")?.classList.add("active");
    $("pill-history")?.classList.remove("active");
    $("thermal-gauges-wrap").style.display = "grid";
    $("thermal-history-card").style.display = "block";
  });

  $("pill-history")?.addEventListener("click", () => {
    $("pill-history")?.classList.add("active");
    $("pill-realtime")?.classList.remove("active");
    $("thermal-gauges-wrap").style.display = "none";
    $("thermal-history-card").style.display = "block";
  });
}
