// Screen 6: Fan Control ("Kontrol Kipas")
import { $, invoke, run, toast } from "./core.js";

let currentFanCurve = [];
let defaultFanCurve = [];

let activeFanMode = "auto";

export async function loadFan() {
  try {
    const [info, stats] = await Promise.all([
      invoke("cmd_get_fan_info"),
      invoke("cmd_get_stats_snapshot"),
    ]);

    if (info && info.default_curve && info.default_curve.length) {
      defaultFanCurve = info.default_curve.map((p) => ({ ...p }));
      currentFanCurve = defaultFanCurve.map((p) => ({ ...p }));
    }

    if (stats) {
      const rpm1 = stats.fan1_rpm ? `${stats.fan1_rpm} RPM` : "--";
      const rpm2 = stats.fan2_rpm ? `${stats.fan2_rpm} RPM` : "--";
      if ($("fan-cpu-rpm")) $("fan-cpu-rpm").textContent = rpm1;
      if ($("fan-gpu-rpm")) $("fan-gpu-rpm").textContent = rpm2;

      // Update circular rings
      const c = 264;
      const setRing = (id, rpm) => {
        const ring = $(id);
        if (!ring) return;
        const pct = Math.min(100, Math.max(0, (rpm / 4500) * 100));
        const offset = c - (pct / 100) * c;
        ring.style.strokeDasharray = `${c}`;
        ring.style.strokeDashoffset = `${offset}`;
      };
      if (stats.fan1_rpm) setRing("ring-fan-cpu", stats.fan1_rpm);
      if (stats.fan2_rpm) setRing("ring-fan-gpu", stats.fan2_rpm);
    }

    renderFanCurveChart();
  } catch (err) {
    console.error("loadFan error:", err);
    toast(String(err), true);
  }
}

function badgeLabel(mode) {
  return mode.charAt(0).toUpperCase() + mode.slice(1);
}

async function applyFanModeToBoth(command, profile) {
  for (const fanId of [1, 2]) {
    const args =
      command === "cmd_apply_fan_profile"
        ? { cmd: { profile, fan_id: fanId } }
        : { fanId };
    const res = await run(command, args);
    if (!res) return false;
  }
  return true;
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
  // Operating Mode Segmented Pills
  document.querySelectorAll("#fan-modes-pills button").forEach((btn) => {
    btn.addEventListener("click", async () => {
      const mode = btn.dataset.fanMode;
      let ok = true;
      if (mode === "auto") {
        ok = await applyFanModeToBoth("cmd_fan_auto");
      } else if (mode === "silent") {
        ok = await applyFanModeToBoth("cmd_apply_fan_profile", "quiet");
      } else if (mode === "performance") {
        ok = await applyFanModeToBoth("cmd_apply_fan_profile", "full");
      }

      if (!ok) return;

      document.querySelectorAll("#fan-modes-pills button").forEach((b) => b.classList.remove("active"));
      btn.classList.add("active");
      activeFanMode = mode;

      const label = badgeLabel(activeFanMode);
      if ($("badge-fan-cpu-mode")) $("badge-fan-cpu-mode").textContent = label;
      if ($("badge-fan-gpu-mode")) $("badge-fan-gpu-mode").textContent = label;

      if (mode === "custom") {
        toast("Mode Custom: tekan Terapkan untuk mengirim kurva.");
      } else {
        toast(`Mode kipas: ${label}`);
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
