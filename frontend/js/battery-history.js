// Battery Intelligence: riwayat kapasitas, health trend, charge/discharge rate,
// ekspor CSV, dan peringatan penurunan kapasitas.
import { $, invoke, run, toast, fmt } from "./core.js";

let batteryWindow = 86400;
let batteryView = "health";

export async function loadBatteryHistory() {
  const line = $("battery-line");
  const note = $("battery-note");
  const alert = $("battery-alert");
  if (!line || !note) return;

  let points;
  try {
    points = (await invoke("cmd_battery_history", { seconds: batteryWindow })) || [];
  } catch (error) {
    note.textContent = `Gagal memuat histori baterai: ${error}`;
    return;
  }
  if (points.length === 0) {
    line.setAttribute("points", "");
    note.textContent = "Belum ada data baterai. Biarkan aplikasi berjalan untuk mengumpulkan riwayat.";
    return;
  }

  // Pilih nilai yang diformat sesuai view.
  let values;
  let label;
  if (batteryView === "health") {
    values = points.map((p) => p.health_pct ?? null);
    label = "health (%)";
  } else if (batteryView === "capacity") {
    values = points.map((p) => p.energy_full_wh ?? null);
    label = "full capacity (Wh)";
  } else {
    values = points.map((p) => p.power_w ?? null);
    label = "power (W)";
  }

  const valid = values.map((v, i) => ({ v, i })).filter((d) => d.v !== null && d.v !== undefined);
  if (valid.length === 0) {
    line.setAttribute("points", "");
    note.textContent = `Tidak ada data ${label} dalam window ini.`;
    return;
  }

  const now = Date.now() / 1000;
  const start = now - batteryWindow;
  const nums = valid.map((d) => d.v);
  const vmin = Math.min(...nums);
  const vmax = Math.max(...nums);
  const vrange = vmax === vmin ? 1 : vmax - vmin;
  const padY = 12;

  const pts = valid.map((d) => {
    const p = points[d.i];
    const x = ((p.timestamp - start) / batteryWindow) * 100;
    const y = 120 - padY - ((d.v - vmin) / vrange) * (120 - 2 * padY);
    return `${x.toFixed(1)},${y.toFixed(1)}`;
  });
  line.setAttribute("points", pts.join(" "));

  const latest = valid[valid.length - 1].v;
  note.textContent = `${valid.length} samples · latest ${label}: ${fmt(latest, 1)}`;

  // Cek penurunan kapasitas (threshold 20%).
  let drop = null;
  try {
    drop = await invoke("cmd_battery_capacity_alert", { threshold_pct: 20 });
  } catch (error) {
    console.error(error);
  }
  if (drop && alert) {
    const [baseline, latestHealth, dropPct] = drop;
    alert.textContent = `Peringatan: health turun dari ${fmt(baseline, 1)}% ke ${fmt(latestHealth, 1)}% (${fmt(dropPct, 1)}% penurunan).`;
    alert.classList.add("warn");
  } else if (alert) {
    alert.textContent = "";
    alert.classList.remove("warn");
  }
}

export function setupBatteryHistoryControls() {
  const sel = $("battery-view");
  if (!sel) return;
  sel.value = batteryView;
  sel.addEventListener("change", (event) => {
    batteryView = event.target.value;
    loadBatteryHistory();
  });

  document.querySelectorAll("[data-bwin]").forEach((button) => {
    button.classList.toggle("active", Number(button.dataset.bwin) === batteryWindow);
    button.addEventListener("click", () => {
      batteryWindow = Number(button.dataset.bwin);
      document.querySelectorAll("[data-bwin]").forEach((b) => {
        b.classList.toggle("active", b === button);
      });
      loadBatteryHistory();
    });
  });

  $("battery-export")?.addEventListener("click", async () => {
    const suggested = `battery-history-${new Date().toISOString().slice(0, 10)}.csv`;
    const path = prompt("Simpan CSV ke path (contoh ~/battery.csv):", `~/${suggested}`);
    if (!path) return;
    const res = await run("cmd_export_battery_csv", { path });
    if (res) toast(`Diekspor ke ${res}`);
  });

  $("battery-alert-check")?.addEventListener("click", loadBatteryHistory);
}
