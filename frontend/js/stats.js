// Live telemetry listener and dispatcher
import { updateDashboardStats } from "./dashboard.js";
import { updateThermalGauges } from "./thermal.js";
import { updateFanLive } from "./fan.js";

let latestStats = null;

export const getLatestStats = () => latestStats;

export function handleIncomingStats(stats) {
  if (!stats) return;
  latestStats = stats;
  if (document.hidden) return;
  updateDashboardStats(stats);
  // Rebuilding the thermal sensor list is the expensive part; skip it off-tab.
  if (document.getElementById("tab-thermal")?.classList.contains("active")) {
    updateThermalGauges(stats);
  }
  if (document.getElementById("tab-fan")?.classList.contains("active")) {
    updateFanLive(stats);
  }
}
