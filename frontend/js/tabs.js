// Tab and screen navigation management
import { $ } from "./core.js";

const screenTitles = {
  home: { title: "Selamat datang!", sub: "Atur performa laptop Anda sesuai kebutuhan." },
  performance: { title: "Mode Performa", sub: "Pilih mode yang sesuai dengan aktivitas Anda." },
  battery: { title: "Battery", sub: "Pantau kesehatan baterai dan pengisiannya." },
  thermal: { title: "System Temperature", sub: "Pantau suhu komponen laptop Anda secara real-time." },
  gpu: { title: "Pengaturan GPU", sub: "Kelola penggunaan GPU Intel dan NVIDIA." },
  fan: { title: "Kontrol Kipas", sub: "Atur kecepatan kipas sesuai kebutuhan Anda." },
  device: { title: "Pengaturan Perangkat", sub: "Kelola fitur perangkat keras laptop Anda." },
  diagnostics: { title: "Diagnostik & Pemindaian", sub: "Periksa kesehatan sistem dan perangkat Anda." },
  settings: { title: "Pengaturan", sub: "Sesuaikan preferensi aplikasi." },
};

let currentTab = "home";
const listeners = {};

export function onTabChange(tab, fn) {
  if (!listeners[tab]) listeners[tab] = [];
  listeners[tab].push(fn);
}

export function switchTab(tab) {
  currentTab = tab;

  // Update navigation items in sidebar
  document.querySelectorAll(".nav-item").forEach((btn) => {
    const isTarget = btn.dataset.tab === tab || (tab === "fan" && btn.dataset.tab === "thermal");
    btn.classList.toggle("active", isTarget);
    btn.setAttribute("aria-selected", isTarget ? "true" : "false");
  });

  // Update active panel group
  document.querySelectorAll(".panel-group").forEach((panel) => {
    panel.classList.toggle("active", panel.id === `tab-${tab}`);
  });

  // Call registered listeners for this tab
  if (listeners[tab]) {
    listeners[tab].forEach((fn) => fn());
  }
}

export function initTabs() {
  document.querySelectorAll(".nav-item").forEach((button) => {
    button.addEventListener("click", () => {
      const tab = button.dataset.tab;
      if (tab) switchTab(tab);
    });
  });

  // Quick navigation link from Home mode button
  $("btn-change-mode")?.addEventListener("click", () => switchTab("performance"));

  // Quick navigation from lower cards on Home
  $("card-quick-temp")?.addEventListener("click", () => switchTab("thermal"));
  $("card-quick-fan")?.addEventListener("click", () => switchTab("fan"));

  // Thermal header view switches (Suhu <-> Kipas)
  $("btn-thermal-temp")?.addEventListener("click", () => switchTab("thermal"));
  $("btn-thermal-fan")?.addEventListener("click", () => switchTab("fan"));

  // Check URL query param or hash on page load
  const params = new URLSearchParams(window.location.search);
  const initialTab = params.get("tab") || window.location.hash.replace("#", "");
  if (initialTab && (screenTitles[initialTab] || initialTab === "fan")) {
    switchTab(initialTab);
  }

  window.addEventListener("hashchange", () => {
    const h = window.location.hash.replace("#", "");
    if (h && (screenTitles[h] || h === "fan") && h !== currentTab) {
      switchTab(h);
    }
  });
}
