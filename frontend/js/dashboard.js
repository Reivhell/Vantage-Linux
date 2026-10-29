// Screen 1: Home Dashboard loader and updater
import { $, invoke } from "./core.js";

const spark = { cpu: [], gpu: [], ram: [] };

function pushSpark(key, value) {
  const v = Number(value);
  if (!Number.isFinite(v)) return;
  const arr = spark[key];
  arr.push(Math.max(0, Math.min(100, v)));
  if (arr.length > 60) arr.shift();
  const line = $(`home-${key}-spark`);
  if (!line || arr.length < 2) return;
  const last = arr.length - 1;
  line.setAttribute(
    "points",
    arr
      .map((n, i) => `${((i / Math.max(1, last)) * 100).toFixed(1)},${(28 - (n / 100) * 26).toFixed(1)}`)
      .join(" ")
  );
}

function tempText(value) {
  return value === null || value === undefined ? "--" : `${Math.round(value)}°C`;
}

function rpmText(rpm, available) {
  return available && rpm !== null && rpm !== undefined ? `${rpm} RPM` : "N/A";
}

export async function loadDashboard() {
  try {
    const [dev, presets, stats] = await Promise.all([
      invoke("cmd_get_device_info"),
      invoke("cmd_get_presets"),
      invoke("cmd_get_stats_snapshot"),
    ]);

    if (dev) {
      const model = [dev.vendor, dev.product].filter(Boolean).join(" ") || "--";
      if ($("sidebar-device-model")) $("sidebar-device-model").textContent = model;
    }

    if (Array.isArray(presets)) {
      const active = presets.find((p) => p.active) || presets.find((p) => p.key === "balanced") || presets[0];
      if (active) {
        if ($("dash-mode-title")) $("dash-mode-title").textContent = active.label || "--";
        if ($("dash-mode-desc")) $("dash-mode-desc").textContent = active.description || "--";
      }
    }

    if (stats) updateDashboardStats(stats);
  } catch (err) {
    console.error("loadDashboard error:", err);
  }
}

export function updateDashboardStats(stats) {
  if (!stats) return;
  const view = stats.view || {};

  const cpu = Number(stats.cpu_percent) || 0;
  const gpuValue = stats.gpu_util === null || stats.gpu_util === undefined ? null : Number(stats.gpu_util);
  const gpu = gpuValue ?? 0;
  const ram = Number(stats.ram_percent) || 0;

  if ($("home-cpu-pct")) $("home-cpu-pct").textContent = `${Math.round(cpu)}%`;
  if ($("home-cpu-freq")) $("home-cpu-freq").textContent = view.cpu_freq || "--";
  pushSpark("cpu", cpu);

  if ($("home-gpu-pct")) $("home-gpu-pct").textContent = gpuValue === null ? "--" : `${Math.round(gpu)}%`;
  if ($("home-gpu-clock")) $("home-gpu-clock").textContent = view.gpu_freq || "--";
  if (gpuValue !== null && Number.isFinite(gpuValue)) pushSpark("gpu", gpuValue);

  if ($("home-ram-pct")) $("home-ram-pct").textContent = `${Math.round(ram)}%`;
  if ($("home-ram-used")) {
    $("home-ram-used").textContent = view.ram_used
      ? `${view.ram_used} / ${view.ram_total || "--"}`
      : "--";
  }
  pushSpark("ram", ram);

  const batPct = stats.battery_percent;
  if ($("home-bat-pct")) $("home-bat-pct").textContent = batPct === null || batPct === undefined ? "--" : `${Math.round(batPct)}%`;
  if ($("home-bat-time")) $("home-bat-time").textContent = view.battery_time || "--";
  if ($("home-bat-fill") && batPct !== null && batPct !== undefined) {
    $("home-bat-fill").style.width = `${Math.min(100, Math.max(0, batPct))}%`;
  }

  if ($("home-temp-cpu")) $("home-temp-cpu").textContent = tempText(stats.cpu_temp);
  if ($("home-temp-gpu")) $("home-temp-gpu").textContent = tempText(stats.gpu_temp);
  if ($("home-temp-ssd")) $("home-temp-ssd").textContent = tempText(stats.ssd_temp);

  if ($("home-fan-cpu")) $("home-fan-cpu").textContent = rpmText(stats.fan1_rpm, stats.fan1_available);
  if ($("home-fan-gpu")) $("home-fan-gpu").textContent = rpmText(stats.fan2_rpm, stats.fan2_available);
  if ($("home-net-status")) $("home-net-status").textContent = view.net_status || "--";
}
