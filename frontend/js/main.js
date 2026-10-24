// Master entrypoint for Lenovo Vantage Linux
import { $, invoke, listen, toast } from "./core.js";
import { initTabs, onTabChange, switchTab } from "./tabs.js";
import { loadDashboard } from "./dashboard.js";
import { initPerformance, loadPerformance } from "./performance.js";
import { initBattery, loadBattery } from "./battery.js";
import { initThermal, loadThermal } from "./thermal.js";
import { initGpu, loadGpu } from "./gpu.js";
import { initFan, loadFan } from "./fan.js";
import { initDevice, loadDevice } from "./device.js";
import { initDiagnostics, loadDiagnostics } from "./diagnostics.js";
import { initSettings, loadSettings } from "./settings.js";
import { handleIncomingStats } from "./stats.js";

async function initWindowControls() {
  const T = window.__TAURI__;
  if (T && T.window) {
    const appWindow = T.window.getCurrentWindow();
    $("win-min")?.addEventListener("click", () => appWindow.minimize());
    $("win-max")?.addEventListener("click", () => appWindow.toggleMaximize());
    $("win-close")?.addEventListener("click", () => appWindow.close());
  } else {
    $("win-min")?.addEventListener("click", () => toast("Jendela diminimalkan"));
    $("win-max")?.addEventListener("click", () => toast("Jendela dimaksimalkan"));
    $("win-close")?.addEventListener("click", () => toast("Aplikasi ditutup"));
  }

  $("btn-notifications")?.addEventListener("click", () => {
    toast("Semua status perangkat dan performa normal.");
  });
}

async function boot() {
  // 1. Initialize navigation and components
  initTabs();
  initPerformance();
  initBattery();
  initThermal();
  initGpu();
  initFan();
  initDevice();
  initDiagnostics();
  initSettings();
  initWindowControls();

  // 2. Register tab loaders
  onTabChange("home", loadDashboard);
  onTabChange("performance", loadPerformance);
  onTabChange("battery", loadBattery);
  onTabChange("thermal", loadThermal);
  onTabChange("gpu", loadGpu);
  onTabChange("fan", loadFan);
  onTabChange("device", loadDevice);
  onTabChange("diagnostics", loadDiagnostics);
  onTabChange("settings", loadSettings);

  // 3. Load initial data
  try {
    const params = new URLSearchParams(window.location.search);
    const targetTab = params.get("tab") || window.location.hash.replace("#", "") || "home";
    switchTab(targetTab);

    // 4. Listen to live hardware stats from Rust backend
    await listen("loq:stats", (event) => {
      handleIncomingStats(event.payload);
    });

    const snapshot = await invoke("cmd_get_stats_snapshot");
    if (snapshot) {
      handleIncomingStats(snapshot);
    }
  } catch (error) {
    console.warn("Bootstrap initial fetch warning:", error);
  }
}

// Start application
if (document.readyState === "loading") {
  document.addEventListener("DOMContentLoaded", boot);
} else {
  boot();
}
