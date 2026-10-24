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

// Fallback mock responses when running in standard browser preview
function mockInvoke(cmd, args) {
  switch (cmd) {
    case "cmd_get_device_info":
      return {
        vendor: "Lenovo",
        product: "LOQ 15IAX9E",
        version: "83LK",
        board: "LNVNB161216",
        bios_label: "NECN38WW · 10/12/2024",
        kernel: "6.12.0-cachyos",
        cpu_model: "12th Gen Intel Core i5-12450HX",
        memory_total_gb: 16.0,
        memory_label: "16.0 GB",
        uptime_label: "4j 22m",
        battery_health_pct: 92.0,
        battery_cycles: 78,
        health_score: 95,
        health_level: "good",
      };
    case "cmd_get_presets":
      return [
        { key: "quiet", label: "Quiet", description: "Kebisingan rendah, daya hemat.", active: false, available: true },
        { key: "balanced", label: "Balanced", description: "Seimbang antara performa dan daya.", active: true, available: true },
        { key: "performance", label: "Performance", description: "Performa maksimal untuk game dan aplikasi berat.", active: false, available: true },
        { key: "custom", label: "Custom", description: "Atur sendiri sesuai kebutuhan.", active: false, available: true },
      ];
    case "cmd_get_stats_snapshot":
      return {
        timestamp: Date.now() / 1000,
        cpu_percent: 12.4,
        gpu_percent: 8.0,
        ram_percent: 34.0,
        battery_percent: 78,
        fan1_rpm: 1200,
        fan2_rpm: 1100,
        temp_cpu: 48,
        temp_gpu: 42,
        view: {
          cpu_percent: "12%",
          cpu_freq: "2.8 GHz",
          gpu_percent: "8%",
          ram_percent: "34%",
          ram_used: "5.4 / 16 GB",
          battery: "78%",
          fan_rpm: "1200 / 1100 RPM",
        },
      };
    case "cmd_get_battery_detail":
      return {
        available: true,
        percent: 78,
        status: "Discharging",
        health_pct: 92.0,
        cycles: 78,
        energy_full_wh: 56.4,
        energy_design_wh: 60.0,
        threshold_available: true,
        threshold_pct: 80,
      };
    case "cmd_get_gpu_mode":
      return {
        mode: "hybrid",
        mode_label: "Hybrid Mode (disarankan)",
        pending: null,
        backend: "supergfxctl",
        conflict: "",
        modes: [
          { key: "hybrid", label: "Hybrid Mode (disarankan)", hint: "Otomatis beralih antara iGPU dan dGPU." },
          { key: "nvidia", label: "Discrete GPU Mode", hint: "Performa grafis maksimal." },
          { key: "integrated", label: "Integrated GPU Mode", hint: "Hemat daya maksimal." },
        ],
      };
    case "cmd_get_device_settings":
      return {
        keyboard_backlight: true,
        touchpad: true,
        camera: true,
        fn_lock: false,
        win_key: true,
        usb_charging: false,
        vantage_key: true,
        overdrive: false,
      };
    case "cmd_set_device_setting":
      return { ok: true, message: "Setting saved" };
    case "cmd_apply_preset":
      return { ok: true, message: "Preset applied" };
    default:
      return { ok: true, message: "ok" };
  }
}
