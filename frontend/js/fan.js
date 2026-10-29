// Screen 6: Fan Control ("Kontrol Kipas")
import { $, invoke, run, toast } from "./core.js";

let fanInfo = null;

function setRing(id, rpm, available) {
  const ring = $(id);
  if (!ring) return;
  const c = 264;
  const pct = available && Number.isFinite(rpm) ? Math.min(100, Math.max(0, (rpm / 5000) * 100)) : 0;
  ring.style.strokeDasharray = `${c}`;
  ring.style.strokeDashoffset = `${c - (pct / 100) * c}`;
}

function paintRpm(stats) {
  if (!stats) return;
  const f1 = stats.fan1_available ? stats.fan1_rpm : null;
  const f2 = stats.fan2_available ? stats.fan2_rpm : null;
  if ($("fan-cpu-rpm")) $("fan-cpu-rpm").textContent = f1 === null || f1 === undefined ? "N/A" : `${f1} RPM`;
  if ($("fan-gpu-rpm")) $("fan-gpu-rpm").textContent = f2 === null || f2 === undefined ? "N/A" : `${f2} RPM`;
  setRing("ring-fan-cpu", f1, stats.fan1_available);
  setRing("ring-fan-gpu", f2, stats.fan2_available);
  if ($("fan-card-cpu")) $("fan-card-cpu").style.display = stats.fan1_available ? "" : "none";
  if ($("fan-card-gpu")) $("fan-card-gpu").style.display = stats.fan2_available ? "" : "none";
}

function paintMode(status) {
  const label = status?.mode_label || "Tidak tersedia";
  if ($("badge-fan-cpu-mode")) $("badge-fan-cpu-mode").textContent = label;
  if ($("badge-fan-gpu-mode")) $("badge-fan-gpu-mode").textContent = label;

  document.querySelectorAll("#fan-modes-pills button").forEach((btn) => {
    btn.classList.toggle("active", btn.dataset.fanMode === status?.mode);
    btn.disabled = !status?.mode_control;
  });

  if ($("fan-unavailable-note")) {
    $("fan-unavailable-note").style.display = status?.available ? "none" : "block";
  }
}

export async function loadFan() {
  try {
    const [info, status, stats] = await Promise.all([
      invoke("cmd_get_fan_info"),
      invoke("cmd_fan_status"),
      invoke("cmd_get_stats_snapshot"),
    ]);
    fanInfo = info;
    paintRpm(stats);
    paintMode(status);
  } catch (err) {
    console.error("loadFan error:", err);
  }
}

export function updateFanLive(stats) {
  paintRpm(stats);
}

export function initFan() {
  document.querySelectorAll("#fan-modes-pills button").forEach((btn) => {
    btn.addEventListener("click", async () => {
      const mode = btn.dataset.fanMode;
      if (!mode || !fanInfo?.mode_control) {
        toast("Kontrol mode kipas tidak tersedia pada firmware ini.", true);
        return;
      }
      const res = await run("cmd_set_fan_mode", { mode });
      if (res) {
        toast(`Mode kipas: ${btn.textContent.trim()}`);
        await loadFan();
      }
    });
  });
}
