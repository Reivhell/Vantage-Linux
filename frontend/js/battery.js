// Screen 3: Battery management and charging history
import { $, invoke, run, toast, fmt } from "./core.js";

let batteryWindow = 86400; // 24 hours default

export async function loadBattery() {
  try {
    const detail = await invoke("cmd_get_battery_detail");
    const chargeMode = await invoke("cmd_get_charge_mode");

    if (detail) {
      // Circular gauge
      const pct = detail.percent === null || detail.percent === undefined ? null : Math.round(detail.percent);
      if ($("bat-gauge-pct")) $("bat-gauge-pct").textContent = pct === null ? "--" : `${pct}%`;

      const circle = $("bat-circle-progress");
      if (circle) {
        const circumference = 264;
        const offset = pct === null ? circumference : circumference - (pct / 100) * circumference;
        circle.style.strokeDasharray = `${circumference}`;
        circle.style.strokeDashoffset = `${offset}`;
      }

      if ($("bat-stat-time")) {
        const power = detail.power_w;
        const energy = detail.status?.toLowerCase().includes("charg") ? detail.energy_full_wh - detail.energy_now_wh : detail.energy_now_wh;
        if (power > 0.5 && energy > 0) {
          const minutes = Math.round((energy / power) * 60);
          $("bat-stat-time").textContent = minutes >= 60 ? `${Math.floor(minutes / 60)} jam ${minutes % 60} menit` : `${minutes} menit`;
        } else {
          $("bat-stat-time").textContent = "--";
        }
      }

      if ($("bat-stat-health")) {
        if (detail.health_pct === null || detail.health_pct === undefined) {
          $("bat-stat-health").textContent = "--";
        } else {
          const label = detail.health_pct >= 80 ? "Baik" : "Turun";
          $("bat-stat-health").textContent = `${label} (${fmt(detail.health_pct, 0)}%)`;
        }
      }

      if ($("bat-stat-cycles")) $("bat-stat-cycles").textContent = detail.cycles ?? "--";

      if ($("bat-stat-cap")) {
        if (detail.energy_full_wh === null || detail.energy_full_wh === undefined) {
          $("bat-stat-cap").textContent = "--";
        } else {
          const full = `${fmt(detail.energy_full_wh, 1)} Wh`;
          const design = detail.energy_design_wh ? `${fmt(detail.energy_design_wh, 0)} Wh` : "--";
          $("bat-stat-cap").textContent = `${full} (desain ${design})`;
        }
      }
    }

    // Charging settings switches
    if (chargeMode) {
      const isConservation = chargeMode.current === "conservation" || chargeMode.current === "Long_Life";
      const isRapid = chargeMode.current === "rapid" || chargeMode.current === "Fast";
      if ($("switch-threshold")) $("switch-threshold").checked = isConservation;
      if ($("switch-rapid")) $("switch-rapid").checked = isRapid;
    }

    await renderBatteryHistoryChart();
  } catch (err) {
    console.error("loadBattery error:", err);
  }
}

async function renderBatteryHistoryChart() {
  const line = $("bat-chart-line");
  const area = $("bat-chart-area");
  if (!line || !area) return;

  try {
    const history = (await invoke("cmd_battery_history", { seconds: batteryWindow })) || [];
    // Map history samples to SVG coordinate range: X [45, 330], Y [123 (0%) to 15 (100%)]
    const now = Date.now() / 1000;
    const start = now - batteryWindow;
    const points = history
      .filter((s) => s.percent !== null && s.percent !== undefined)
      .map((s) => {
        const x = 45 + Math.max(0, Math.min(1, (s.timestamp - start) / batteryWindow)) * 285;
        const y = 123 - (s.percent / 100) * 108;
        return `${x.toFixed(1)},${y.toFixed(1)}`;
      });
    if (points.length >= 3) {
      line.setAttribute("points", points.join(" "));
      area.setAttribute("points", `45,123 ${points.join(" ")} 330,123`);
    } else {
      line.setAttribute("points", "");
      area.setAttribute("points", "");
    }
  } catch (e) {
    console.warn("battery history error:", e);
  }
}

export function initBattery() {
  // Period filter pills (24 Jam, 7 Hari, 30 Hari)
  document.querySelectorAll("#bat-pills-window button").forEach((btn) => {
    btn.addEventListener("click", () => {
      document.querySelectorAll("#bat-pills-window button").forEach((b) => b.classList.remove("active"));
      btn.classList.add("active");
      batteryWindow = Number(btn.dataset.bwin) || 86400;
      renderBatteryHistoryChart();
    });
  });

  // Threshold switch
  $("switch-threshold")?.addEventListener("change", async (e) => {
    const checked = e.target.checked;
    const sub = $("threshold-subtext");
    if (sub) sub.textContent = checked ? "80%" : "Nonaktif";
    const mode = checked ? "conservation" : "standard";
    const res = await run("cmd_set_charge_mode", { mode });
    if (res) toast(checked ? "Batas pengisian diaktifkan pada 80%" : "Batas pengisian dinonaktifkan (100%)");
  });

  // Rapid Charge switch
  $("switch-rapid")?.addEventListener("change", async (e) => {
    const checked = e.target.checked;
    const sub = $("rapid-subtext");
    if (sub) sub.textContent = checked ? "Aktif" : "Nonaktif";
    const mode = checked ? "rapid" : "standard";
    const res = await run("cmd_set_charge_mode", { mode });
    if (res) toast(checked ? "Rapid Charge diaktifkan" : "Rapid Charge dinonaktifkan");
  });
}
