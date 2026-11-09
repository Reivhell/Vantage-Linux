// Screen 6: Fan Control ("Kontrol Kipas")
import { $, invoke, run, toast } from "./core.js";

let fanInfo = null;
let currentFanCurve = [];
let defaultFanCurve = [];

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

    if (info && info.default_curve && info.default_curve.length) {
      defaultFanCurve = info.default_curve.map((p) => ({ ...p }));
      currentFanCurve = defaultFanCurve.map((p) => ({ ...p }));
    }

    renderFanCurveChart();
  } catch (err) {
    console.error("loadFan error:", err);
    toast(String(err), true);
  }
}

export function updateFanLive(stats) {
  paintRpm(stats);
}

function renderFanCurveChart() {
  const line = $("fan-curve-line");
  const svg = $("fan-curve-editor-svg");
  if (!line || !svg) return;

  // Chart range: X [45 (0°C) to 450 (100°C)], Y [130 (0%) to 25 (100%)]
  const points = currentFanCurve.map((pt) => {
    const x = 45 + (Math.min(100, Math.max(0, pt.temp)) / 100) * 405;
    const y = 130 - (Math.min(100, Math.max(0, pt.pwm)) / 100) * 105;
    return `${x.toFixed(1)},${y.toFixed(1)}`;
  });

  line.setAttribute("points", points.join(" "));

  // Update control point circles
  const circles = svg.querySelectorAll("circle.curve-pt");
  currentFanCurve.forEach((pt, i) => {
    if (circles[i]) {
      const x = 45 + (Math.min(100, Math.max(0, pt.temp)) / 100) * 405;
      const y = 130 - (Math.min(100, Math.max(0, pt.pwm)) / 100) * 105;
      circles[i].setAttribute("cx", x);
      circles[i].setAttribute("cy", y);
    }
  });
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

  // Curve points interactivity (drag or click adjustments)
  const svg = $("fan-curve-editor-svg");
  if (svg) {
    let activeDragIdx = -1;

    svg.querySelectorAll("circle.curve-pt").forEach((circle, idx) => {
      circle.addEventListener("mousedown", (e) => {
        activeDragIdx = idx;
        e.preventDefault();
      });
    });

    window.addEventListener("mousemove", (e) => {
      if (activeDragIdx < 0) return;
      const rect = svg.getBoundingClientRect();
      const mouseY = e.clientY - rect.top;
      // Invert Y mapping to PWM percentage
      const pwmPct = Math.round(Math.max(0, Math.min(100, (1 - (mouseY - 25) / 105) * 100)));
      if (currentFanCurve[activeDragIdx]) {
        currentFanCurve[activeDragIdx].pwm = pwmPct;
        renderFanCurveChart();
      }
    });

    window.addEventListener("mouseup", () => {
      activeDragIdx = -1;
    });
  }

  // Reset to default
  $("btn-fan-reset")?.addEventListener("click", () => {
    if (!defaultFanCurve.length) {
      toast("Kurva bawaan tidak tersedia dari backend.", true);
      return;
    }
    currentFanCurve = defaultFanCurve.map((p) => ({ ...p }));
    renderFanCurveChart();
    toast("Kurva kipas dikembalikan ke bawaan.");
  });

  // Apply curve to both fans; a partial failure is reported and leaves the UI honest.
  $("btn-fan-apply")?.addEventListener("click", async () => {
    if (currentFanCurve.length < 2) {
      toast("Kurva belum dimuat dari backend.", true);
      return;
    }
    for (const fanId of [1, 2]) {
      const cmd = { fan_id: fanId, points: currentFanCurve };
      const res = await run("cmd_apply_fan_curve", { cmd });
      if (!res) return;
    }
    toast("Kurva kipas kustom diterapkan ke kedua kipas.");
  });
}
