// Screen 1: Home Dashboard loader and updater
import { $, invoke } from "./core.js";

export async function loadDashboard() {
  try {
    const [dev, presets, stats] = await Promise.all([
      invoke("cmd_get_device_info"),
      invoke("cmd_get_presets"),
      invoke("cmd_get_stats_snapshot"),
    ]);

    // Device Model & Status in sidebar and header
    if (dev) {
      const model = [dev.vendor, dev.product].filter(Boolean).join(" ") || "Lenovo LOQ 15IAX9E";
      const modelEl = $("sidebar-device-model");
      if (modelEl) modelEl.textContent = model;
    }

    // Active Profile in Featured Banner
    if (presets && Array.isArray(presets)) {
      const active = presets.find((p) => p.active) || presets.find((p) => p.key === "balanced") || presets[0];
      if (active) {
        if ($("dash-mode-title")) $("dash-mode-title").textContent = active.label || "Balanced";
        if ($("dash-mode-desc")) $("dash-mode-desc").textContent = active.description || "Seimbang antara performa dan daya.";
      }
    }

    // Live Snapshot telemetry
    if (stats) {
      updateDashboardStats(stats);
    }
  } catch (err) {
    console.error("loadDashboard error:", err);
  }
}

export function updateDashboardStats(stats) {
  if (!stats) return;

  const view = stats.view || {};

  // CPU
  const cpuPct = view.cpu_percent || `${Math.round(stats.cpu_percent || 12)}%`;
  const cpuFreq = view.cpu_freq || "2.8 GHz";
  if ($("home-cpu-pct")) $("home-cpu-pct").textContent = cpuPct;
  if ($("home-cpu-freq")) $("home-cpu-freq").textContent = cpuFreq;

  // GPU
  const gpuPct = view.gpu_percent || `${Math.round(stats.gpu_percent || 8)}%`;
  const gpuFreq = stats.gpu_clock ? `${stats.gpu_clock} MHz` : "210 MHz";
  if ($("home-gpu-pct")) $("home-gpu-pct").textContent = gpuPct;
  if ($("home-gpu-clock")) $("home-gpu-clock").textContent = gpuFreq;

  // RAM
  const ramPct = view.ram_percent || `${Math.round(stats.ram_percent || 34)}%`;
  let ramUsed = "5.4 / 16 GB";
  if (view.ram_used) {
    ramUsed = view.ram_used.includes("/") ? view.ram_used : `${view.ram_used} / ${view.ram_total || "16 GB"}`;
  }
  if ($("home-ram-pct")) $("home-ram-pct").textContent = ramPct;
  if ($("home-ram-used")) $("home-ram-used").textContent = ramUsed;

  // Battery
  const batPctNum = stats.battery_percent ?? 78;
  const batPct = `${batPctNum}%`;
  const batTime = "2 jam 45 menit";
  if ($("home-bat-pct")) $("home-bat-pct").textContent = batPct;
  if ($("home-bat-time")) $("home-bat-time").textContent = batTime;
  if ($("home-bat-fill")) $("home-bat-fill").style.width = `${Math.min(100, Math.max(5, batPctNum))}%`;

  // Lower cards
  // Suhu
  const tempCpu = stats.temp_cpu ? `${stats.temp_cpu}°C` : (view.cpu_temp || "48°C");
  const tempGpu = stats.temp_gpu ? `${stats.temp_gpu}°C` : (view.gpu_temp || "42°C");
  const tempSsd = stats.temp_ssd ? `${stats.temp_ssd}°C` : "36°C";
  if ($("home-temp-cpu")) $("home-temp-cpu").textContent = tempCpu;
  if ($("home-temp-gpu")) $("home-temp-gpu").textContent = tempGpu;
  if ($("home-temp-ssd")) $("home-temp-ssd").textContent = tempSsd;

  // Kipas
  const fanCpu = stats.fan1_rpm ? `${stats.fan1_rpm} RPM` : "1200 RPM";
  const fanGpu = stats.fan2_rpm ? `${stats.fan2_rpm} RPM` : "1100 RPM";
  if ($("home-fan-cpu")) $("home-fan-cpu").textContent = fanCpu;
  if ($("home-fan-gpu")) $("home-fan-gpu").textContent = fanGpu;
}
