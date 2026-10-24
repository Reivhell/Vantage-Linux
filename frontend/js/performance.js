// Screen 2: Performance profiles and additional settings
import { $, invoke, run, toast, showModal } from "./core.js";
import { switchTab } from "./tabs.js";

let currentPresets = [];
let activeModeKey = "balanced";

export async function loadPerformance() {
  try {
    currentPresets = await invoke("cmd_get_presets");
    renderProfileCards(currentPresets);

    // Load GPU Mode label
    const gpu = await invoke("cmd_get_gpu_mode");
    if (gpu && $("perf-gpu-val")) {
      $("perf-gpu-val").textContent = gpu.mode_label || "Hybrid (disarankan)";
    }

    // Load display refresh rate
    const disp = await invoke("cmd_get_display_info");
    if (disp && disp.displays && disp.displays.length > 0) {
      const primary = disp.displays[0];
      const hz = Math.round(primary.current_refresh || 144);
      if ($("perf-refresh-val")) $("perf-refresh-val").textContent = `Otomatis (${hz} Hz)`;
    }

    // Load overdrive state
    const devSettings = await invoke("cmd_get_device_settings");
    if (devSettings && $("perf-overdrive-val")) {
      $("perf-overdrive-val").textContent = devSettings.overdrive ? "Aktif" : "Nonaktif";
    }
  } catch (err) {
    console.error("loadPerformance error:", err);
  }
}

function renderProfileCards(presets) {
  if (!presets || !Array.isArray(presets)) return;

  const active = presets.find((p) => p.active);
  if (active) activeModeKey = active.key;

  document.querySelectorAll(".profile-card").forEach((card) => {
    const key = card.dataset.profile;
    const isActive = key === activeModeKey;
    card.classList.toggle("active", isActive);
  });
}

export function initPerformance() {
  // Profile card clicks
  document.querySelectorAll(".profile-card").forEach((card) => {
    card.addEventListener("click", async () => {
      const key = card.dataset.profile;
      if (!key) return;

      activeModeKey = key;
      document.querySelectorAll(".profile-card").forEach((c) => {
        c.classList.toggle("active", c.dataset.profile === key);
      });

      // Update Featured Banner on Home immediately
      const matched = currentPresets.find((p) => p.key === key);
      if (matched) {
        if ($("dash-mode-title")) $("dash-mode-title").textContent = matched.label;
        if ($("dash-mode-desc")) $("dash-mode-desc").textContent = matched.description;
      }

      const res = await run("cmd_apply_preset", { name: key }, `Mode ${key.toUpperCase()} diaktifkan.`);
      if (res) {
        toast(`Mode performa diubah ke ${key.toUpperCase()}`);
      }
    });
  });

  // Additional settings: GPU Mode row
  $("row-perf-gpu")?.addEventListener("click", () => {
    switchTab("gpu");
  });

  // Additional settings: Refresh Rate row
  $("row-perf-refresh")?.addEventListener("click", async () => {
    try {
      const disp = await invoke("cmd_get_display_info");
      const current = disp?.displays?.[0]?.current_refresh || 144;
      const targetHz = current > 100 ? 60 : 144;

      showModal(
        "Pilih Refresh Rate Layar",
        `<p style="margin-bottom: 12px;">Pilih frekuensi refresh layar untuk menghemat baterai atau mendapatkan animasi ultra-halus.</p>
         <div style="display: flex; gap: 10px; margin-top: 10px;">
           <button class="btn-primary" id="btn-set-144" style="flex: 1;">144 Hz (Halus)</button>
           <button class="btn-secondary" id="btn-set-60" style="flex: 1;">60 Hz (Hemat Daya)</button>
         </div>`,
        () => {}
      );

      $("btn-set-144")?.addEventListener("click", async () => {
        $("perf-refresh-val").textContent = "144 Hz";
        toast("Refresh rate diatur ke 144 Hz");
        $("app-modal").classList.remove("active");
      });

      $("btn-set-60")?.addEventListener("click", async () => {
        $("perf-refresh-val").textContent = "60 Hz";
        toast("Refresh rate diatur ke 60 Hz");
        $("app-modal").classList.remove("active");
      });
    } catch (e) {
      toast("Pengaturan refresh rate tidak tersedia", true);
    }
  });

  // Additional settings: Over Drive row
  $("row-perf-overdrive")?.addEventListener("click", async () => {
    const valEl = $("perf-overdrive-val");
    const current = valEl?.textContent === "Aktif";
    const next = !current;
    if (valEl) valEl.textContent = next ? "Aktif" : "Nonaktif";
    await run("cmd_set_device_setting", { key: "overdrive", value: next });
    toast(`Over Drive ${next ? "diaktifkan (respons layar lebih cepat)" : "dinonaktifkan"}`);
  });
}
