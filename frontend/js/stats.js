// Live telemetry listener and dispatcher
import { updateDashboardStats } from "./dashboard.js";
import { updateThermalGauges } from "./thermal.js";
import { updateAutomationLive } from "./automation.js";

let latestStats = null;

export const getLatestStats = () => latestStats;

export function handleIncomingStats(stats) {
  if (!stats) return;
  latestStats = stats;

  // Broadcast to active views
  updateDashboardStats(stats);
  updateThermalGauges(stats);
  updateAutomationLive(stats);
}
