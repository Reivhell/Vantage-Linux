// Core utilities, Tauri IPC bridge, and UI notifications
const T = window.__TAURI__;

export const hasTauri = Boolean(T && T.core && T.core.invoke);

export const invoke = async (cmd, args = {}) => {
  if (hasTauri) {
    try {
      return await T.core.invoke(cmd, args);
    } catch (e) {
      console.warn(`[Tauri] ${cmd} error:`, e);
      throw e;
    }
  }
  // Mock data fallback only for browser preview mode
  return mockInvoke(cmd, args);
};

export const listen = (event, callback) => {
  if (hasTauri && T.event && T.event.listen) {
    return T.event.listen(event, callback);
  }
  return Promise.resolve(() => {});
};

export const $ = (id) => document.getElementById(id);

const STRINGS_ID = {
  "nav.home": "Home",
  "nav.performance": "Performance",
  "nav.battery": "Battery",
  "nav.thermal": "Thermal",
  "nav.gpu": "GPU",
  "nav.device": "Device",
  "nav.diagnostics": "Diagnostik",
  "nav.settings": "Pengaturan",
  "nav.resource": "Sumber Daya",
  "nav.memory": "RAM & Tasks",
  "screen.home.title": "Selamat datang!",
  "screen.home.sub": "Atur performa laptop Anda sesuai kebutuhan.",
  "screen.performance.title": "Mode Performa",
  "screen.performance.sub": "Pilih mode yang sesuai dengan aktivitas Anda.",
  "screen.battery.title": "Battery",
  "screen.battery.sub": "Pantau kesehatan baterai dan pengisiannya.",
  "screen.thermal.title": "System Temperature",
  "screen.thermal.sub": "Pantau suhu komponen laptop Anda secara real-time.",
  "screen.gpu.title": "Pengaturan GPU",
  "screen.gpu.sub": "Kelola penggunaan GPU Intel dan NVIDIA.",
  "screen.resource.title": "Manajemen Sumber Daya",
  "screen.resource.sub": "Pantau dan atur batas CPU/GPU secara otomatis.",
  "screen.memory.title": "RAM & Tasks",
  "screen.memory.sub": "Pantau memori fisik dan proses yang berjalan.",
  "screen.fan.title": "Kontrol Kipas",
  "screen.fan.sub": "Atur kecepatan kipas sesuai kebutuhan Anda.",
  "screen.device.title": "Pengaturan Perangkat",
  "screen.device.sub": "Kelola fitur perangkat keras laptop Anda.",
  "screen.diagnostics.title": "Diagnostik & Pemindaian",
  "screen.diagnostics.sub": "Periksa kesehatan sistem dan perangkat Anda.",
  "screen.settings.title": "Pengaturan",
  "screen.settings.sub": "Sesuaikan preferensi aplikasi.",
};

const STRINGS_EN = {
  "nav.home": "Home",
  "nav.performance": "Performance",
  "nav.battery": "Battery",
  "nav.thermal": "Thermal",
  "nav.gpu": "GPU",
  "nav.device": "Device",
  "nav.diagnostics": "Diagnostics",
  "nav.settings": "Settings",
  "nav.resource": "Resource Management",
  "nav.memory": "RAM & Tasks",
  "screen.home.title": "Welcome",
  "screen.home.sub": "Tune your laptop performance to your needs.",
  "screen.performance.title": "Performance Mode",
  "screen.performance.sub": "Choose the mode that fits your activity.",
  "screen.battery.title": "Battery",
  "screen.battery.sub": "Monitor battery health and charging.",
  "screen.thermal.title": "System Temperature",
  "screen.thermal.sub": "Monitor component temperatures in real time.",
  "screen.gpu.title": "GPU Settings",
  "screen.gpu.sub": "Manage Intel and NVIDIA GPU usage.",
  "screen.resource.title": "Resource Management",
  "screen.resource.sub": "Monitor and automate CPU/GPU thresholds.",
  "screen.memory.title": "RAM & Tasks",
  "screen.memory.sub": "Monitor physical memory and running processes.",
  "screen.fan.title": "Fan Control",
  "screen.fan.sub": "Set fan speed to your needs.",
  "screen.device.title": "Device Settings",
  "screen.device.sub": "Manage your laptop hardware features.",
  "screen.diagnostics.title": "Diagnostics & Scan",
  "screen.diagnostics.sub": "Check system and device health.",
  "screen.settings.title": "Settings",
  "screen.settings.sub": "Adjust app preferences.",
};

let locale = "id";

export function setLocale(loc) {
  if (loc === "id" || loc === "en") locale = loc;
}

export function t(key) {
  const table = locale === "en" ? STRINGS_EN : STRINGS_ID;
  return table[key] || key;
}

export function applyI18n() {
  document.querySelectorAll("[data-i18n]").forEach((el) => {
    const key = el.getAttribute("data-i18n");
    const val = t(key);
    if (!val) return;
    const label = el.matches("button") ? el.querySelector("span") : null;
    if (label) label.textContent = val;
    else el.textContent = val;
  });
}

export const fmt = (n, digits = 1) =>
  n === null || n === undefined || Number.isNaN(n) ? "--" : Number(n).toFixed(digits);

export const clampPct = (n) => Math.max(0, Math.min(100, n));

export function escapeHtml(text) {
  return String(text).replace(/[&<>"']/g, (c) =>
    ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;", "'": "&#39;" }[c])
  );
}

export function toast(message, isError = false) {
  const el = $("toast-notice");
  const txt = $("toast-text");
  if (!el || !txt) return;
  txt.textContent = message;
  el.classList.toggle("err", isError);
  el.classList.add("show");
  clearTimeout(el._timer);
  el._timer = setTimeout(() => el.classList.remove("show"), 3200);
}

export async function run(cmd, args, okMessage) {
  try {
    const res = await invoke(cmd, args);
    if (res && res.ok === false) {
      toast(res.message || "Aksi gagal", true);
      return null;
    }
    if (okMessage) toast(okMessage);
    return res;
  } catch (e) {
    toast(String(e), true);
    return null;
  }
}

// Modal dialog helper
export function showModal(title, contentHtml, onOk) {
  const modal = $("app-modal");
  const titleEl = $("modal-title");
  const bodyEl = $("modal-body");
  const btnOk = $("modal-btn-ok");
  const btnCancel = $("modal-btn-cancel");
  if (!modal) return;

  titleEl.textContent = title;
  bodyEl.innerHTML = contentHtml;
  modal.classList.add("active");

  const close = () => {
    modal.classList.remove("active");
  };

  btnCancel.onclick = close;
  btnOk.onclick = async () => {
    if (onOk) await onOk();
    close();
  };
}

// Browser preview only. Hardware commands return no data; run the Tauri app for real values.
function mockInvoke(cmd) {
  switch (cmd) {
    case "cmd_get_presets":
      return [
        { key: "quiet", label: "Quiet", description: "Kebisingan rendah, daya hemat.", active: false, available: true },
        { key: "balanced", label: "Balanced", description: "Seimbang antara performa dan daya.", active: true, available: true },
        { key: "performance", label: "Performance", description: "Performa maksimal untuk game dan aplikasi berat.", active: false, available: true },
        { key: "custom", label: "Custom", description: "Atur sendiri sesuai kebutuhan.", active: false, available: true },
      ];
    case "cmd_battery_history":
      return [];
    default:
      return null;
  }
}
