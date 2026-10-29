// Screen 5: GPU Settings & controls
import { $, invoke, run, toast, showModal, escapeHtml } from "./core.js";

let gpuModeInfo = null;

export async function loadGpu() {
  try {
    const [modeInfo, details, stats] = await Promise.all([
      invoke("cmd_get_gpu_mode"),
      invoke("cmd_get_gpu_details"),
      invoke("cmd_get_stats_snapshot"),
    ]);

    gpuModeInfo = modeInfo;

    // Mode GPU Title & description
    if (modeInfo) {
      if ($("gpu-cur-mode-title")) $("gpu-cur-mode-title").textContent = modeInfo.mode_label || "--";
    }

    // Intel iGPU Stats
    if (stats) {
      if ($("gpu-i-util")) {
        $("gpu-i-util").textContent =
          stats.igpu_util === null || stats.igpu_util === undefined ? "--" : `${Math.round(stats.igpu_util)}%`;
      }
      if ($("gpu-i-temp")) $("gpu-i-temp").textContent = "--";
      if ($("gpu-i-vram")) $("gpu-i-vram").textContent = "--";

      // NVIDIA dGPU Stats
      if ($("gpu-d-util")) {
        $("gpu-d-util").textContent =
          stats.gpu_util === null || stats.gpu_util === undefined ? "--" : `${stats.gpu_util}%`;
      }
      if ($("gpu-d-temp")) $("gpu-d-temp").textContent = stats.view?.gpu_temp || "--";
      if ($("gpu-d-vram")) $("gpu-d-vram").textContent = stats.view?.gpu_vram || "--";
    }

    if (details) {
      if ($("gpu-nvidia-card-title")) {
        $("gpu-nvidia-card-title").textContent = details.name ? details.name.replace("NVIDIA ", "") : "GPU Diskret";
      }
      if ($("switch-gpu-pm")) {
        $("switch-gpu-pm").checked = details.runtime_control !== "on";
      }
    }
  } catch (err) {
    console.error("loadGpu error:", err);
  }
}

export function initGpu() {
  // Mode GPU Row click -> opens switcher modal
  $("btn-select-gpu-mode")?.addEventListener("click", () => {
    if (!gpuModeInfo) return;
    const modesHtml = (gpuModeInfo.modes || [])
      .map(
        (m) => `
        <div class="setting-row-card" style="margin-bottom: 8px; cursor: pointer;" data-switch-mode="${m.key}">
          <div>
            <strong style="color: var(--text-primary); display: block;">${escapeHtml(m.label)}</strong>
            <span style="font-size: 11px; color: var(--text-muted);">${escapeHtml(m.hint || "")}</span>
          </div>
          <button class="btn-primary" style="padding: 4px 12px; font-size: 11px;">Pilih</button>
        </div>`
      )
      .join("");

    showModal(
      "Pilih Mode Operasi GPU",
      `<p style="margin-bottom: 14px; font-size: 12px;">Peralihan mode GPU diskret/hibrida mungkin memerlukan restart sesi atau sistem.</p>
       <div>${modesHtml}</div>`,
      null
    );

    document.querySelectorAll("[data-switch-mode]").forEach((el) => {
      el.addEventListener("click", async () => {
        const mode = el.dataset.switchMode;
        $("app-modal").classList.remove("active");
        const res = await run("cmd_switch_gpu_mode", { mode });
        if (res) {
          toast(`Mode GPU diubah ke ${mode}. Silakan restart untuk menerapkan.`);
          await loadGpu();
        }
      });
    });
  });

  // GPU Runtime PM toggle
  $("switch-gpu-pm")?.addEventListener("change", async (e) => {
    const checked = e.target.checked;
    const control = checked ? "auto" : "on";
    const sub = $("gpu-pm-sub");
    if (sub) sub.textContent = checked ? "Aktif" : "Nonaktif";
    const res = await run("cmd_set_gpu_runtime_pm", { control });
    if (res) toast(`GPU Runtime PM ${checked ? "diaktifkan (auto power down)" : "dinonaktifkan (selalu aktif)"}`);
  });

  // Preferensi aplikasi per GPU -> Modal showing processes
  $("row-gpu-procs")?.addEventListener("click", async () => {
    try {
      const procs = (await invoke("cmd_get_gpu_processes")) || [];
      const rows = procs.length
        ? procs
            .map(
              (p) =>
                `<tr><td style="padding: 6px; font-family: var(--font-mono);">${p.pid}</td><td style="padding: 6px;">${escapeHtml(p.name)}</td><td style="padding: 6px; text-align: right; font-family: var(--font-mono);">${p.memory_mb || "--"} MB</td></tr>`
            )
            .join("")
        : '<tr><td colspan="3" style="padding: 12px; text-align: center; color: var(--text-muted);">Tidak ada aplikasi yang sedang menggunakan NVIDIA GPU.</td></tr>';

      showModal(
        "Aplikasi Menggunakan GPU NVIDIA",
        `<table style="width: 100%; border-collapse: collapse; font-size: 12px;">
           <thead><tr style="border-bottom: 1px solid var(--border-card); color: var(--text-muted); text-align: left;"><th style="padding: 6px;">PID</th><th style="padding: 6px;">Aplikasi</th><th style="padding: 6px; text-align: right;">VRAM</th></tr></thead>
           <tbody>${rows}</tbody>
         </table>`,
        null
      );
    } catch (e) {
      toast("Gagal membaca proses GPU", true);
    }
  });

  // Penggunaan daya NVIDIA -> Modal showing wattage details
  $("row-gpu-power")?.addEventListener("click", async () => {
    try {
      const details = await invoke("cmd_get_gpu_details");
      const watts = details?.power_w !== null && details?.power_w !== undefined ? `${details.power_w.toFixed(1)} W` : "--";
      const limit = details?.power_limit_w !== null && details?.power_limit_w !== undefined ? `${details.power_limit_w} W` : "--";

      showModal(
        "Penggunaan Daya NVIDIA",
        `<div style="display: flex; flex-direction: column; gap: 12px;">
           <div style="display: flex; justify-content: space-between;"><span>Konsumsi Daya Saat Ini</span><strong class="mono" style="color: var(--cyan);">${watts}</strong></div>
           <div style="display: flex; justify-content: space-between;"><span>Batas Daya (TGP Limit)</span><strong class="mono" style="color: var(--text-primary);">${limit}</strong></div>
           <div style="display: flex; justify-content: space-between;"><span>Suhu GPU</span><strong class="mono">${details?.temperature_c !== null && details?.temperature_c !== undefined ? `${details.temperature_c}°C` : "--"}</strong></div>
         </div>`,
        null
      );
    } catch (e) {
      toast("Detail daya GPU tidak tersedia", true);
    }
  });
}
