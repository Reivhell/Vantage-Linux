import { $, escapeHtml, fmt, invoke, run, toast } from "./core.js";

let timer = null;

export async function refreshProcesses() {
  const filter = $("proc-filter").value.trim();
  const list = await invoke("cmd_get_processes", filter ? { filter, limit: 400 } : { limit: 400 });
  const body = $("proc-body");
  body.innerHTML = "";
  (list || []).forEach((process) => {
    const row = document.createElement("tr");
    row.innerHTML = `<td class="mono">${process.pid}</td><td>${escapeHtml(process.name)}</td>
      <td class="muted">${escapeHtml(process.user)}</td><td class="num">${fmt(process.cpu)}</td>
      <td class="num">${fmt(process.memory_mb)} MB</td>
      <td><button class="btn danger" data-kill="${process.pid}">Kill</button></td>`;
    body.appendChild(row);
  });
  if (!list?.length) {
    body.innerHTML = '<tr><td colspan="6" class="empty-row">No processes match this filter.</td></tr>';
  }
}

export function startProcessTimer() {
  stopProcessTimer();
  timer = setInterval(refreshProcesses, 3000);
}

export function stopProcessTimer() {
  if (timer) clearInterval(timer);
  timer = null;
}

$("proc-body").addEventListener("click", async (event) => {
  const pid = event.target.dataset.kill;
  if (!pid) return;
  const res = await run("cmd_kill_process", { pid: Number(pid), force: true });
  if (res) {
    toast(`Signalled PID ${pid}`);
    refreshProcesses();
  }
});
$("proc-filter").addEventListener("input", () => refreshProcesses());
$("proc-refresh").addEventListener("click", refreshProcesses);
