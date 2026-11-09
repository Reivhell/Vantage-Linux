// Screen 9: Settings ("Pengaturan")
import { $, invoke, run, toast, showModal, setLocale, applyI18n, hasTauri } from "./core.js";

function setSwitch(id, value) {
  const el = $(id);
  if (el) el.checked = Boolean(value);
}

export async function loadSettings() {
  if (!hasTauri) {
    toast("Backend Vantage tidak tersedia. Pengaturan hanya dapat diubah di aplikasi desktop.", true);
    return;
  }
  try {
    const [config, dev, info] = await Promise.all([
      invoke("cmd_get_config"),
      invoke("cmd_get_device_info"),
      invoke("cmd_get_app_info"),
    ]);

    if (config) {
      setSwitch("switch-cfg-startup", config.restore_profile_on_login);
      setSwitch("switch-cfg-tray", config.start_minimized);
      setSwitch("switch-cfg-notifications", config.desktop_notifications);
      const interval = $("sel-refresh-interval");
      if (interval) {
        const value = String(config.refresh_interval_ms ?? 1000);
        const known = Array.from(interval.options).some((o) => o.value === value);
        if (!known) {
          const opt = document.createElement("option");
          opt.value = value;
          opt.textContent = `${(Number(value) / 1000).toLocaleString("id-ID")} detik`;
          interval.appendChild(opt);
        }
        interval.value = value;
      }
    }

    if (dev) {
      const model = [dev.vendor, dev.product].filter(Boolean).join(" ").trim();
      if ($("about-device-model")) $("about-device-model").textContent = model || "--";
    }

    if (info) {
      if ($("about-version")) $("about-version").textContent = `v${info.version} (Stable)`;
      if ($("about-os")) {
        $("about-os").textContent = [info.os_name, info.os_version].filter(Boolean).join(" ") || "--";
      }
      if ($("about-kernel")) $("about-kernel").textContent = info.kernel || "--";
      const helper = $("about-helper");
      if (helper) {
        if (!info.helper_installed) {
          helper.textContent = "Tidak terpasang";
          helper.style.color = "var(--warning, #f0a030)";
        } else if (info.helper_outdated) {
          helper.textContent = `v${info.helper_version ?? "?"} (kedaluwarsa, perlu dipasang ulang)`;
          helper.style.color = "var(--warning, #f0a030)";
        } else {
          helper.textContent = `v${info.helper_version ?? "?"} siap`;
          helper.style.color = "var(--success)";
        }
      }
    }
  } catch (err) {
    console.error("loadSettings error:", err);
    toast(String(err), true);
  }
}

async function updateConfig(update) {
  return run("cmd_update_config", { update });
}

export function initSettings() {
  document.querySelectorAll("#settings-pills button").forEach((btn) => {
    btn.addEventListener("click", () => {
      document.querySelectorAll("#settings-pills button").forEach((b) => b.classList.remove("active"));
      btn.classList.add("active");
      const subtab = btn.dataset.subtab;
      document.querySelectorAll(".settings-tab-content").forEach((panel) => {
        panel.style.display = panel.id === `subtab-${subtab}` ? "block" : "none";
      });
      if (subtab === "tentang") loadSettings();
    });
  });

  // Theme selection pills
  ["theme-light", "theme-dark", "theme-auto"].forEach((id) => {
    $(id)?.addEventListener("click", () => {
      ["theme-light", "theme-dark", "theme-auto"].forEach((i) => $(i)?.classList.remove("active"));
      $(id)?.classList.add("active");
      toast(`Tema diubah ke ${$(id)?.textContent}`);
    });
  });

  // Language select
  $("sel-language")?.addEventListener("change", (e) => {
    const val = e.target.value;
    setLocale(val);
    applyI18n();
    toast(val === "en" ? "Language switched to English" : "Bahasa aplikasi diubah ke Indonesia");
  });

  // Startup switches
  $("switch-cfg-startup")?.addEventListener("change", async (e) => {
    const res = await updateConfig({ restore_profile_on_login: e.target.checked });
    if (res) toast(`Pemulihan profil ${e.target.checked ? "diaktifkan" : "dinonaktifkan"}`);
  });

  $("switch-cfg-tray")?.addEventListener("change", async (e) => {
    const res = await updateConfig({ start_minimized: e.target.checked });
    if (res) toast(`Mulai diminimalkan ${e.target.checked ? "diaktifkan" : "dinonaktifkan"}`);
  });

  $("switch-cfg-notifications")?.addEventListener("change", async (e) => {
    const res = await updateConfig({ desktop_notifications: e.target.checked });
    if (res) toast(`Notifikasi desktop ${e.target.checked ? "diaktifkan" : "dinonaktifkan"}`);
  });

  $("sel-refresh-interval")?.addEventListener("change", async (e) => {
    const res = await updateConfig({ refresh_interval_ms: Number(e.target.value) });
    if (res) toast("Interval telemetri diperbarui.");
  });

  $("row-cfg-export")?.addEventListener("click", async () => {
    const suggested = `vantage-battery-${new Date().toISOString().slice(0, 10)}.csv`;
    const path = prompt("Simpan file ekspor ke (contoh: ~/vantage-battery.csv):", `~/${suggested}`);
    if (!path) return;
    try {
      const saved = await invoke("cmd_export_battery_csv", { path });
      toast(`Riwayat baterai diekspor ke ${saved}`);
    } catch (e) {
      toast(String(e), true);
    }
  });

  $("row-cfg-clear")?.addEventListener("click", () => {
    showModal(
      "Konfirmasi Hapus Riwayat",
      "<p>Hapus seluruh sampel baterai, event pengisian, dan audit otomatisasi? Aturan dan konfigurasi tetap disimpan.</p>",
      async () => {
        await run("cmd_clear_local_history", {}, "Riwayat lokal berhasil dihapus.");
      }
    );
  });
}
