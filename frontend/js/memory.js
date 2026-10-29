// RAM overview and live task manager
import { $, escapeHtml, fmt, invoke, run, toast } from "./core.js";

let timer = null;
let filterTimer = null;

function gb(value) {
  return value === null || value === undefined ? "--" : `${Number(value).toFixed(2)} GB`;
}

function renderMemory(info) {
  if (!info) return;
  if ($("mem-total")) $("mem-total").textContent = gb(info.total_gb);
  if ($("mem-used")) $("mem-used").textContent = gb(info.used_gb);
  if ($("mem-available")) $("mem-available").textContent = gb(info.available_gb);
  if ($("mem-free")) $("mem-free").textContent = gb(info.free_gb);
  if ($("mem-cached")) $("mem-cached").textContent = gb(info.cached_gb);
  if ($("mem-swap")) $("mem-swap").textContent = `${gb(info.swap_used_gb)} / ${gb(info.swap_total_gb)}`;
  if ($("mem-pressure")) {
    $("mem-pressure").textContent =
      info.pressure_some === null || info.pressure_some === undefined ? "--" : `${Number(info.pressure_some).toFixed(2)}%`;
  }
  if ($("mem-vram")) {
    $("mem-vram").textContent =
      info.vram_used_mb === null || info.vram_used_mb === undefined
        ? "--"
        : `${info.vram_used_mb} / ${info.vram_total_mb ?? "--"} MB`;
  }

  const pct = info.total_gb ? (info.used_gb / info.total_gb) * 100 : 0;
  if ($("mem-bar-fill")) $("mem-bar-fill").style.width = `${Math.min(100, Math.max(0, pct))}%`;
  if ($("mem-bar-label")) $("mem-bar-label").textContent = `${Math.round(pct)}%`;

  const host = $("dimms");
  if (host) {
    if (!info.dimms || !info.dimms.length) {
      host.innerHTML = '<div class="muted small">Data DIMM tidak tersedia tanpa helper/root.</div>';
    } else {
      host.innerHTML = info.dimms
        .map(
          (dimm, index) => `<div class="dimm-row">
            <span class="muted">${escapeHtml(dimm.locator || `Slot ${index + 1}`)}</span>
            <span class="mono">${escapeHtml(dimm.size)} ${escapeHtml(dimm.mem_type)} ${escapeHtml(dimm.speed)}</span>
            <span class="muted">${escapeHtml(dimm.manufacturer || "")}</span>
          </div>`
        )
        .join("");
    }
  }
}

function renderProcesses(list) {
  const body = $("proc-body");
  if (!body) return;
  const rows = list || [];
  body.innerHTML = rows
    .map(
      (process) => `<tr>
        <td class="mono">${process.pid}</td>
        <td>${escapeHtml(process.name)}</td>
        <td class="muted">${escapeHtml(process.user)}</td>
        <td class="num">${fmt(process.cpu)}%</td>
        <td class="num">${fmt(process.memory_mb)} MB</td>
        <td class="num">${fmt(process.memory_pct)}%</td>
        <td class="num">${process.threads ?? "--"}</td>
        <td class="muted">${escapeHtml(process.status || "--")}</td>
        <td><button class="btn-secondary btn-sm" data-kill="${process.pid}">Kill</button></td>
      </tr>`
    )
    .join("");
  if (!rows.length) {
    body.innerHTML = '<tr><td colspan="9" class="empty-row">Tidak ada proses yang cocok.</td></tr>';
  }
  if ($("proc-count")) $("proc-count").textContent = `${rows.length} proses`;
}

async function refreshProcesses() {
  try {
    const filter = $("proc-filter")?.value.trim() || "";
    const list = await invoke("cmd_get_processes", filter ? { filter, limit: 300 } : { limit: 300 });
    renderProcesses(list);
  } catch (err) {
    console.error("refreshProcesses error:", err);
  }
}

export async function loadMemory() {
  try {
    const [info] = await Promise.all([invoke("cmd_get_memory_info"), refreshProcesses()]);
    renderMemory(info);
  } catch (err) {
    console.error("loadMemory error:", err);
  }
}

export function initMemory() {
  if (!timer) {
    timer = setInterval(() => {
      if ($("tab-memory")?.classList.contains("active") && !document.hidden) loadMemory();
    }, 3000);
  }

  $("proc-filter")?.addEventListener("input", () => {
    clearTimeout(filterTimer);
    filterTimer = setTimeout(refreshProcesses, 250);
  });
  $("proc-refresh")?.addEventListener("click", loadMemory);

  $("proc-body")?.addEventListener("click", async (event) => {
    const button = event.target.closest?.("button[data-kill]");
    const pid = button?.dataset.kill;
    if (!pid) return;
    if (!confirm(`Hentikan proses PID ${pid}?`)) return;
    const res = await run("cmd_kill_process", { pid: Number(pid), force: true });
    if (res) {
      toast(`Sinyal dikirim ke PID ${pid}`);
      refreshProcesses();
    }
  });

  $("mem-drop")?.addEventListener("click", async () => {
    if (!confirm("Bersihkan page cache, dentries, dan inodes?")) return;
    const res = await run("cmd_drop_caches");
    if (res) toast(res.message || "Cache dibersihkan");
  });
}
