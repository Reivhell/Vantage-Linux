import { $, escapeHtml, fmt, invoke, run, toast } from "./core.js";

export async function loadMemory() {
  const info = await invoke("cmd_get_memory_info");
  $("vram-used").textContent = info.vram_used_mb !== null ? `${info.vram_used_mb} MB` : "--";
  $("vram-total").textContent = info.vram_total_mb !== null ? `${info.vram_total_mb} MB` : "--";

  const dimmHost = $("dimms");
  if (!info.dimms || !info.dimms.length) {
    dimmHost.innerHTML = '<div class="muted small">SMBIOS data unavailable (helper or dmidecode missing).</div>';
  } else {
    dimmHost.innerHTML = info.dimms
      .map(
        (dimm, index) => `<div><span class="muted">${escapeHtml(dimm.locator || `Slot ${index + 1}`)}</span>
          <span class="mono">${escapeHtml(dimm.size)} ${escapeHtml(dimm.mem_type)} ${escapeHtml(dimm.speed)}</span></div>`
      )
      .join("");
  }

  const list = info.top_processes || [];
  const body = $("mem-procs");
  body.innerHTML = "";
  list.forEach((process) => {
    const row = document.createElement("tr");
    row.innerHTML = `<td class="mono">${process.pid}</td><td>${escapeHtml(process.name)}</td>
      <td class="num">${fmt(process.memory_mb)} MB</td>`;
    body.appendChild(row);
  });
  if (!list.length) {
    body.innerHTML = '<tr><td colspan="3" class="empty-row">No process data available.</td></tr>';
  }
}

$("mem-drop").addEventListener("click", async () => {
  if (!confirm("Drop page cache, dentries and inodes?")) return;
  const res = await run("cmd_drop_caches");
  if (res) toast(res.message || "Caches dropped");
});
