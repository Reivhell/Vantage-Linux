// Screen 2: Performance profiles and additional settings
import { $, invoke, run, toast, showModal } from "./core.js";
import { switchTab } from "./tabs.js";

let currentPresets = [];
let activeModeKey = "";

export async function loadPerformance() {
  try {
    currentPresets = await invoke("cmd_get_presets");
    renderProfileCards(currentPresets);

    // Load GPU Mode label
    const gpu = await invoke("cmd_get_gpu_mode");
    if (gpu && $("perf-gpu-val")) {
      $("perf-gpu-val").textContent = gpu.mode_label || "--";
    }

    // Load display refresh rate
    const disp = await invoke("cmd_get_display_info");
    const primary = disp?.outputs?.find((o) => o.primary) || disp?.outputs?.[0];
    if (primary && $("perf-refresh-val")) {
      $("perf-refresh-val").textContent = `${primary.current_resolution_label} · ${primary.current_refresh_label} Hz`;
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

      const res = await run("cmd_apply_preset", { name: key });
      if (!res) return;

      activeModeKey = key;
      document.querySelectorAll(".profile-card").forEach((c) => {
        c.classList.toggle("active", c.dataset.profile === key);
      });

      const matched = currentPresets.find((p) => p.key === key);
      if (matched) {
        if ($("dash-mode-title")) $("dash-mode-title").textContent = matched.label;
        if ($("dash-mode-desc")) $("dash-mode-desc").textContent = matched.description || "";
      }
      toast(`Mode performa diubah ke ${key.toUpperCase()}`);
    });
  });

  // Additional settings: GPU Mode row
  $("row-perf-gpu")?.addEventListener("click", () => {
    switchTab("gpu");
  });

  // Additional settings: Refresh Rate row -> mode nyata dari compositor.
  $("row-perf-refresh")?.addEventListener("click", async () => {
    try {
      const disp = await invoke("cmd_get_display_info");
      const primary = disp?.outputs?.find((output) => output.primary) || disp?.outputs?.[0];
      if (!primary) {
        toast("Compositor tidak melaporkan output aktif.", true);
        return;
      }
      const modes = [];
      for (const group of primary.groups || []) {
        for (const mode of group.modes || []) {
          modes.push({ width: group.width, height: group.height, refresh: mode.refresh_hz, label: group.resolution_label, current: mode.current });
        }
      }
      const options = modes
        .map(
          (mode) =>
            `<button class="btn-${mode.current ? "secondary" : "primary"}" style="display:flex;justify-content:space-between;width:100%;margin-bottom:6px;padding:8px 12px;" data-display-mode="${mode.width}x${mode.height}@${mode.refresh}" ${mode.current ? "disabled" : ""}>
               <span>${mode.label}</span><span class="mono">${mode.refresh} Hz${mode.current ? " · aktif" : ""}</span>
             </button>`
        )
        .join("");
      showModal(
        `Mode Layar ${primary.name}`,
        `<p style="margin-bottom:12px;">Pilih mode yang benar-benar dilaporkan compositor. Mode saat ini dinonaktifkan.</p>
         <div style="max-height:320px;overflow-y:auto;">${options || '<p class="muted">Tidak ada mode yang dilaporkan.</p>'}</div>`,
        null
      );
      document.querySelectorAll("[data-display-mode]").forEach((button) => {
        button.addEventListener("click", async () => {
          const [size, refresh] = button.dataset.displayMode.split("@");
          const [width, height] = size.split("x").map(Number);
          const res = await run(
            "cmd_set_display_mode",
            { output: primary.name, width, height, refresh: Number(refresh) },
            "Mode layar diterapkan."
          );
          if (res) {
            $("app-modal").classList.remove("active");
            await loadPerformance();
          }
        });
      });
    } catch (e) {
      toast(String(e), true);
    }
  });
}
