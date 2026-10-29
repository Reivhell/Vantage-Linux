// Screen 2: Performance profiles and additional settings
import { $, invoke, run, toast, showModal } from "./core.js";
import { switchTab } from "./tabs.js";

let currentPresets = [];
let activeModeKey = "balanced";

function primaryOutput(view) {
  const outputs = view?.outputs || [];
  return outputs.find((output) => output.primary) || outputs[0] || null;
}

function refreshText(output) {
  const hz = output?.current_refresh_hz;
  return hz === null || hz === undefined ? "--" : `Otomatis (${Math.round(hz)} Hz)`;
}

export async function loadPerformance() {
  try {
    const [presets, gpu, disp, devSettings] = await Promise.all([
      invoke("cmd_get_presets"),
      invoke("cmd_get_gpu_mode"),
      invoke("cmd_get_display_info"),
      invoke("cmd_get_device_settings"),
    ]);

    currentPresets = presets || [];
    renderProfileCards(currentPresets);

    // Load GPU Mode label
    if (gpu && $("perf-gpu-val")) {
      $("perf-gpu-val").textContent = gpu.mode_label || "--";
    }

    // Load display refresh rate
    if ($("perf-refresh-val")) {
      $("perf-refresh-val").textContent = refreshText(primaryOutput(disp));
    }

    // Load overdrive state
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
      const output = primaryOutput(disp);
      const groups = output?.groups || [];
      const group =
        groups.find(
          (item) =>
            item.width === output.current_width &&
            item.height === output.current_height
        ) || groups[0];
      const modes = (group?.modes || []).slice().sort((a, b) => b.refresh_hz - a.refresh_hz);
      const unique = [];
      for (const mode of modes) {
        if (!unique.some((item) => Math.abs(item.refresh_hz - mode.refresh_hz) < 0.05)) {
          unique.push(mode);
        }
      }
      const high = unique.find((mode) => mode.refresh_hz > 90) || unique[0];
      const low = unique.slice().reverse().find((mode) => mode.refresh_hz <= 90) || unique[unique.length - 1];
      const choices = [];
      for (const mode of [high, low]) {
        if (mode && !choices.some((item) => Math.abs(item.refresh_hz - mode.refresh_hz) < 0.05)) {
          choices.push(mode);
        }
      }

      if (!output || !choices.length) {
        toast("Mode refresh rate tidak tersedia untuk layar ini", true);
        return;
      }

      const buttons = choices
        .map((mode, index) => {
          const label = mode.refresh_hz > 90 ? "Halus" : "Hemat Daya";
          return `<button class="${index === 0 ? "btn-primary" : "btn-secondary"}" data-refresh="${mode.refresh_hz}" data-width="${mode.width ?? output.current_width}" data-height="${mode.height ?? output.current_height}" style="flex: 1;">
            ${Math.round(mode.refresh_hz)} Hz<br><span style="font-size: 10px; opacity: .7;">${label}</span>
          </button>`;
        })
        .join("");

      showModal(
        "Pilih Refresh Rate Layar",
        `<p style="margin-bottom: 12px;">Pilih frekuensi refresh layar untuk menghemat baterai atau mendapatkan animasi ultra-halus.</p>
         <div style="display: flex; gap: 10px; margin-top: 10px;">${buttons}</div>`,
        () => {}
      );

      document.querySelectorAll("#modal-body [data-refresh]").forEach((btn) => {
        btn.addEventListener("click", async () => {
          const width = Number(btn.dataset.width);
          const height = Number(btn.dataset.height);
          const refresh = Number(btn.dataset.refresh);
          if (!output.name || !Number.isFinite(width) || !Number.isFinite(height) || !Number.isFinite(refresh)) {
            return;
          }
          const res = await run("cmd_set_display_mode", {
            output: output.name,
            width,
            height,
            refresh,
          });
          if (res) {
            $("perf-refresh-val").textContent = `Otomatis (${Math.round(refresh)} Hz)`;
            toast(`Refresh rate diatur ke ${Math.round(refresh)} Hz`);
            $("app-modal")?.classList.remove("active");
          }
        });
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
