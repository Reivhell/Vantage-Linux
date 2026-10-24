// Screen 6: Fan Control ("Kontrol Kipas")
import { $, invoke, run, toast } from "./core.js";

let currentFanCurve = [
  { temp: 0, pwm: 0 },
  { temp: 40, pwm: 0 },
  { temp: 60, pwm: 45 },
  { temp: 80, pwm: 75 },
  { temp: 100, pwm: 100 },
];

let activeFanMode = "auto";

export async function loadFan() {
  try {
    const [info, stats] = await Promise.all([
      invoke("cmd_get_fan_info"),
      invoke("cmd_get_stats_snapshot"),
    ]);

    if (info && info.default_curve && info.default_curve.length) {
      currentFanCurve = info.default_curve.map((p) => ({ ...p }));
    }

    if (stats) {
      const rpm1 = stats.fan1_rpm ? `${stats.fan1_rpm} RPM` : "1200 RPM";
      const rpm2 = stats.fan2_rpm ? `${stats.fan2_rpm} RPM` : "1100 RPM";
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
      setRing("ring-fan-cpu", stats.fan1_rpm || 1200);
      setRing("ring-fan-gpu", stats.fan2_rpm || 1100);
    }

    renderFanCurveChart();
  } catch (err) {
    console.error("loadFan error:", err);
  }
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
      document.querySelectorAll("#fan-modes-pills button").forEach((b) => b.classList.remove("active"));
      btn.classList.add("active");
      activeFanMode = btn.dataset.fanMode;

      const badgeCpu = $("badge-fan-cpu-mode");
      const badgeGpu = $("badge-fan-gpu-mode");
      const label = activeFanMode.charAt(0).toUpperCase() + activeFanMode.slice(1);
      if (badgeCpu) badgeCpu.textContent = label;
      if (badgeGpu) badgeGpu.textContent = label;

      if (activeFanMode === "auto") {
        await run("cmd_fan_auto", { fan_id: 0 });
        toast("Mode kipas: Auto");
      } else if (activeFanMode === "silent") {
        await run("cmd_apply_fan_profile", { profile: "quiet", fan_id: 1 });
        await run("cmd_apply_fan_profile", { profile: "quiet", fan_id: 2 });
        toast("Mode kipas: Silent");
      } else if (activeFanMode === "performance") {
        await run("cmd_apply_fan_profile", { profile: "full", fan_id: 1 });
        await run("cmd_apply_fan_profile", { profile: "full", fan_id: 2 });
        toast("Mode kipas: Performance");
      } else {
        toast("Mode kipas: Custom (kurva manual)");
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
    currentFanCurve = [
      { temp: 0, pwm: 0 },
      { temp: 40, pwm: 0 },
      { temp: 60, pwm: 45 },
      { temp: 80, pwm: 75 },
      { temp: 100, pwm: 100 },
    ];
    renderFanCurveChart();
    toast("Kurva kipas dikembalikan ke bawaan.");
  });

  // Apply curve
  $("btn-fan-apply")?.addEventListener("click", async () => {
    const cmd = {
      fan_id: 1,
      points: currentFanCurve,
    };
    const res = await run("cmd_apply_fan_curve", { cmd });
    if (res) toast("Kurva kipas kustom berhasil diterapkan.");
  });
}
