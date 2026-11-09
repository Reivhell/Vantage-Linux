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
      const model = [dev.vendor, dev.product].filter(Boolean).join(" ") || "--";
      const modelEl = $("sidebar-device-model");
      if (modelEl) modelEl.textContent = model;
      if ($("sidebar-conn-status")) $("sidebar-conn-status").textContent = "Backend aktif";
    }

    // Active Profile in Featured Banner
    if (presets && Array.isArray(presets)) {
      const active = presets.find((p) => p.active);
      if (active) {
        if ($("dash-mode-title")) $("dash-mode-title").textContent = active.label;
        if ($("dash-mode-desc")) $("dash-mode-desc").textContent = active.description || "";
      }
    }

    // Live Snapshot telemetry
    if (stats) {
      updateDashboardStats(stats);
    }
  } catch (err) {
    console.error("loadDashboard error:", err);
    if ($("sidebar-conn-status")) $("sidebar-conn-status").textContent = "Backend tidak tersedia";
  }
}

export function updateDashboardStats(stats) {
  if (!stats) return;

  const view = stats.view || {};
  const pct = (value) => (value === null || value === undefined ? "--" : `${Math.round(value)}%`);

  // CPU
  if ($("home-cpu-pct")) $("home-cpu-pct").textContent = pct(stats.cpu_percent);
  if ($("home-cpu-freq")) $("home-cpu-freq").textContent = view.cpu_freq || "--";
  if ($("home-cpu-spark") && view.sparks?.cpu) $("home-cpu-spark").setAttribute("points", view.sparks.cpu);

  // GPU
  if ($("home-gpu-pct")) $("home-gpu-pct").textContent = pct(stats.gpu_util);
  if ($("home-gpu-clock")) $("home-gpu-clock").textContent = stats.gpu_freq_mhz ? `${stats.gpu_freq_mhz} MHz` : "--";
  if ($("home-gpu-spark") && view.sparks?.gpu) $("home-gpu-spark").setAttribute("points", view.sparks.gpu);

  // RAM
  if ($("home-ram-pct")) $("home-ram-pct").textContent = pct(stats.ram_percent);
  if ($("home-ram-used")) {
    $("home-ram-used").textContent =
      stats.ram_used_gb != null && stats.ram_total_gb != null
        ? `${stats.ram_used_gb.toFixed(1)} / ${stats.ram_total_gb.toFixed(1)} GB`
        : "--";
  }
  if ($("home-ram-spark") && view.sparks?.ram) $("home-ram-spark").setAttribute("points", view.sparks.ram);

  // Battery
  const batPctNum = stats.battery_percent;
  if ($("home-bat-pct")) $("home-bat-pct").textContent = pct(batPctNum);
  if ($("home-bat-time")) $("home-bat-time").textContent = stats.battery_status || view.ac_state || "--";
  if ($("home-bat-fill")) {
    $("home-bat-fill").style.width = batPctNum == null ? "0%" : `${Math.min(100, Math.max(0, batPctNum))}%`;
  }

  // Lower cards
  if ($("home-temp-cpu")) $("home-temp-cpu").textContent = stats.cpu_temp ? `${Math.round(stats.cpu_temp)}°C` : "--";
  if ($("home-temp-gpu")) $("home-temp-gpu").textContent = stats.gpu_temp ? `${stats.gpu_temp}°C` : "--";
  if ($("home-temp-ssd")) $("home-temp-ssd").textContent = stats.ssd_temp ? `${Math.round(stats.ssd_temp)}°C` : "--";
  if ($("home-fan-cpu")) $("home-fan-cpu").textContent = stats.fan1_rpm ? `${stats.fan1_rpm} RPM` : "--";
  if ($("home-fan-gpu")) $("home-fan-gpu").textContent = stats.fan2_rpm ? `${stats.fan2_rpm} RPM` : "--";

  // Network
  if ($("home-net-status")) {
    const iface = view.net_interface || stats.net_interface || "";
    const ip = view.net_ip || stats.net_ipv4 || "";
    $("home-net-status").textContent = iface ? `${iface}${ip ? ` · ${ip}` : ""}` : "Tidak terhubung";
  }
}
