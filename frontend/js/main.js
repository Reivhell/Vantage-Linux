// Master entrypoint for Lenovo Vantage Linux
import { $, escapeHtml, invoke, listen, showModal, toast } from "./core.js";
import { initTabs, onTabChange, switchTab } from "./tabs.js";
import { loadDashboard } from "./dashboard.js";
import { initPerformance, loadPerformance } from "./performance.js";
import { initBattery, loadBattery } from "./battery.js";
import { initThermal, loadThermal } from "./thermal.js";
import { initGpu, loadGpu } from "./gpu.js";
import { initAutomation, loadAutomation } from "./automation.js";
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
    const unavailable = () => toast("Kontrol jendela hanya tersedia di aplikasi desktop.", true);
    $("win-min")?.addEventListener("click", unavailable);
    $("win-max")?.addEventListener("click", unavailable);
    $("win-close")?.addEventListener("click", unavailable);
  }

  await refreshNotificationBadge();
  $("btn-notifications")?.addEventListener("click", async () => {
    try {
      const events = await invoke("cmd_get_automation_events", { limit: 10 });
      if (!events.length) {
        showModal("Audit Otomatisasi", "<p>Belum ada aksi otomatisasi yang tercatat.</p>");
      } else {
        const rows = events
          .map(
            (ev) => `<li><strong>${escapeHtml(ev.rule_name || ev.rule_id)}</strong>
              — ${escapeHtml(ev.action)} · ${escapeHtml(ev.status)}<br>
              <span class="mono">${escapeHtml(ev.detail || "")}</span></li>`
          )
          .join("");
        showModal("Audit Otomatisasi", `<ul class="audit-modal-list">${rows}</ul>`);
      }
      hideNotificationBadge();
    } catch (e) {
      toast(String(e), true);
    }
  });
}

async function refreshNotificationBadge() {
  const dot = document.querySelector("#btn-notifications .notif-dot");
  if (!dot) return;
  try {
    const events = await invoke("cmd_get_automation_events", { limit: 1 });
    dot.style.display = events.length ? "block" : "none";
  } catch {
    dot.style.display = "none";
  }
}

function hideNotificationBadge() {
  const dot = document.querySelector("#btn-notifications .notif-dot");
  if (dot) dot.style.display = "none";
}

async function boot() {
  // 1. Initialize navigation and components
  initTabs();
  initPerformance();
  initBattery();
  initThermal();
  initGpu();
  initAutomation();
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
  onTabChange("automation", loadAutomation);
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
