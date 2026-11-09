// Screen 8: Diagnostics & Scanning ("Diagnostik & Pemindaian")
import { $, invoke, run, toast, showModal, escapeHtml } from "./core.js";

let latestReport = null;

export async function loadDiagnostics() {
  // Restore last report if available
  if (latestReport) {
    renderDiagChecklist(latestReport);
  }
}

function renderDiagChecklist(report) {
  if (!report) return;

  const checks = report.checks || [];
  const statusLabel = (status) => {
    switch (status) {
      case "ok":
        return "Normal";
      case "warn":
        return "Peringatan";
      case "unknown":
        return "Tidak tersedia";
      default:
        return "Kesalahan";
    }
  };
  const findStatus = (keywords) => {
    const item = checks.find((c) => keywords.some((k) => c.label.toLowerCase().includes(k)));
    return item ? statusLabel(item.status) : "Tidak tersedia";
  };

  if ($("status-cpu-mem")) $("status-cpu-mem").textContent = findStatus(["cpu", "memory", "ram"]);
  if ($("status-storage")) $("status-storage").textContent = findStatus(["storage", "disk", "smart"]);
  if ($("status-battery")) $("status-battery").textContent = findStatus(["battery", "baterai"]);
  if ($("status-thermal")) $("status-thermal").textContent = findStatus(["thermal", "temp", "suhu"]);
  if ($("status-gpu")) $("status-gpu").textContent = findStatus(["gpu", "nvidia", "grafis"]);
  if ($("status-driver")) $("status-driver").textContent = findStatus(["driver", "helper", "system", "kernel"]);
}

export function initDiagnostics() {
  // Start scan button
  $("btn-start-scan")?.addEventListener("click", async () => {
    const btn = $("btn-start-scan");
    const barWrap = $("scan-bar-wrap");
    const barFill = $("scan-bar-fill");

    btn.disabled = true;
    btn.textContent = "Memindai...";
    if (barWrap) barWrap.style.display = "block";
    if (barFill) barFill.style.width = "20%";

    try {
      if (barFill) barFill.style.width = "65%";
      latestReport = await invoke("cmd_run_diagnostics");
      if (barFill) barFill.style.width = "100%";

      // Update timestamp
      const now = new Date();
      const months = ["Jan", "Feb", "Mar", "Apr", "Mei", "Jun", "Jul", "Agu", "Sep", "Okt", "Nov", "Des"];
      const ts = `${now.getDate()} ${months[now.getMonth()]} ${now.getFullYear()} ${String(now.getHours()).padStart(2, "0")}:${String(now.getMinutes()).padStart(2, "0")}`;
      if ($("scan-last-time")) $("scan-last-time").textContent = `Terakhir diperiksa: ${ts}`;

      renderDiagChecklist(latestReport);
      toast(`Pemindaian selesai. Skor kesehatan: ${latestReport.score ?? "--"}/100`);
    } catch (e) {
      toast(String(e), true);
    } finally {
      setTimeout(() => {
        btn.disabled = false;
        btn.textContent = "Mulai Pemindaian";
        if (barWrap) barWrap.style.display = "none";
        if (barFill) barFill.style.width = "0%";
      }, 600);
    }
  });

  // View full report button -> Modal
  $("btn-view-report")?.addEventListener("click", async () => {
    if (!latestReport) {
      // Run quick scan if not yet run
      try {
        latestReport = await invoke("cmd_run_diagnostics");
      } catch (e) {
        toast("Silakan jalankan pemindaian terlebih dahulu.", true);
        return;
      }
    }

    const statusLabel = (status) =>
      status === "ok" ? "Normal" : status === "warn" ? "Perhatian" : status === "unknown" ? "Tidak tersedia" : "Kesalahan";
    const statusColor = (status) =>
      status === "ok" ? "var(--success)" : status === "warn" ? "var(--warning)" : "var(--text-muted)";
    const checksHtml = (latestReport.checks || [])
      .map(
        (c) => `
        <div style="padding: 10px 0; border-bottom: 1px solid var(--border-card); display: flex; justify-content: space-between; align-items: center;">
          <div>
            <strong style="color: var(--text-primary);">${escapeHtml(c.label)}</strong>
            <div style="font-size: 11px; color: var(--text-muted); margin-top: 2px;">${escapeHtml(c.summary || "")}</div>
          </div>
          <span style="font-size: 11px; font-weight: 600; color: ${statusColor(c.status)};">${statusLabel(c.status)}</span>
        </div>`
      )
      .join("");

    showModal(
      `Laporan Diagnostik Sistem (${latestReport.score ?? "--"}/100)`,
      `<div style="max-height: 380px; overflow-y: auto;">
         ${checksHtml || '<p style="color: var(--text-muted);">Semua modul hardware beroperasi normal.</p>'}
       </div>
       <div style="margin-top: 16px; display: flex; justify-content: flex-end;">
         <button class="btn-primary" id="btn-export-diag-modal">Ekspor Laporan ke File</button>
       </div>`,
      null
    );

    $("btn-export-diag-modal")?.addEventListener("click", async () => {
      const res = await run("cmd_export_diagnostics", { report: latestReport });
      if (res) toast(`Laporan disimpan di ${res.message}`);
    });
  });
}
