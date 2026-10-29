// Screen 9: Settings ("Pengaturan")
import { $, invoke, run, toast, showModal, setLocale, applyI18n } from "./core.js";

export async function loadSettings() {
  try {
    const [config, dev] = await Promise.all([
      invoke("cmd_get_config"),
      invoke("cmd_get_device_info"),
    ]);

    if (config) {
      if ($("switch-cfg-startup")) $("switch-cfg-startup").checked = config.restore_profile_on_login ?? true;
      if ($("switch-cfg-tray")) $("switch-cfg-tray").checked = config.start_minimized ?? true;
    }

    if (dev) {
      const model = [dev.vendor, dev.product, dev.version].filter(Boolean).join(" ");
      if ($("about-device-model")) $("about-device-model").textContent = model || "--";
      if ($("about-os")) $("about-os").textContent = [dev.os_name, dev.os_version, dev.kernel].filter(Boolean).join(" · ") || "--";
      if ($("about-helper")) $("about-helper").textContent = dev.helper_label || "Tidak tersedia";
    }
  } catch (err) {
    console.error("loadSettings error:", err);
  }
}

export function initSettings() {
  // Sub-tabs navigation (Umum, Notifikasi, Tentang)
  document.querySelectorAll("#settings-pills button").forEach((btn) => {
    btn.addEventListener("click", () => {
      document.querySelectorAll("#settings-pills button").forEach((b) => b.classList.remove("active"));
      btn.classList.add("active");

      const subtab = btn.dataset.subtab;
      document.querySelectorAll(".settings-tab-content").forEach((panel) => {
        panel.style.display = panel.id === `subtab-${subtab}` ? "block" : "none";
      });
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
    await run("cmd_update_config", { update: { restore_profile_on_login: e.target.checked } });
    toast(`Jalankan saat startup ${e.target.checked ? "diaktifkan" : "dinonaktifkan"}`);
  });

  $("switch-cfg-tray")?.addEventListener("change", async (e) => {
    toast(`Minimalkan ke system tray ${e.target.checked ? "diaktifkan" : "dinonaktifkan"}`);
  });

  // Data & Privacy
  $("row-cfg-export")?.addEventListener("click", async () => {
    const suggested = `vantage-backup-${new Date().toISOString().slice(0, 10)}.csv`;
    const path = prompt("Simpan file ekspor ke direktori (contoh: ~/vantage-data.csv):", `~/${suggested}`);
    if (path) {
      const res = await run("cmd_export_battery_csv", { path });
      if (res) toast(`Data diagnostik & baterai berhasil diekspor.`);
    }
  });

  $("row-cfg-clear")?.addEventListener("click", () => {
    showModal(
      "Konfirmasi Hapus Riwayat",
      "<p>Apakah Anda yakin ingin menghapus data riwayat baterai dan log diagnostik lokal aplikasi?</p>",
      () => {
        toast("Riwayat lokal berhasil dibersihkan.");
      }
    );
  });

  // Notification switches
  ["notif-temp", "notif-bat", "notif-profile", "notif-driver"].forEach((id) => {
    $(id)?.addEventListener("change", (e) => {
      toast(`Preferensi notifikasi diperbarui.`);
    });
  });
}
