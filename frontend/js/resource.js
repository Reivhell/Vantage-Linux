// Screen: Advanced Resource Management (CPU/GPU threshold rules & automation)
import { $, invoke, run, toast, escapeHtml } from "./core.js";

let rules = [];
let status = null;

export async function loadResource() {
  try {
    const [st, rs] = await Promise.all([
      invoke("cmd_get_resource_status"),
      invoke("cmd_list_rules"),
    ]);
    status = st;
    rules = Array.isArray(rs) ? rs : [];
    render();
  } catch (err) {
    console.error("loadResource error:", err);
  }
}

export function initResource() {
  $("btn-add-rule")?.addEventListener("click", () => {
    const name = $("rule-name")?.value.trim();
    const metric = $("rule-metric")?.value;
    const critical = Number($("rule-critical")?.value);
    const warn = Number($("rule-warn")?.value);
    const action = $("rule-action")?.value;

    if (!name || !metric || !action) {
      toast("Nama, metrik, dan aksi wajib diisi.", true);
      return;
    }

    const rule = {
      id: `rule-${Date.now()}`,
      name,
      enabled: true,
      condition: { Any: [{ metric, warn, critical, hysteresis_pct: 5, cooldown_secs: 60 }] },
      action: parseAction(action),
      persistent: true,
    };
    invoke("cmd_add_rule", rule).then((res) => {
      if (res && res.ok) {
        toast("Aturan ditambahkan.");
        loadResource();
      } else {
        toast(res?.message || "Gagal menambahkan aturan", true);
      }
    });
  });

  $("btn-refresh-resource")?.addEventListener("click", loadResource);
}

function parseAction(action) {
  switch (action) {
    case "quiet":
      return { SetThermalMode: { mode: "quiet" } };
    case "balanced":
      return { SetThermalMode: { mode: "balanced" } };
    case "performance":
      return { SetThermalMode: { mode: "performance" } };
    case "notify":
      return {
        Notify: {
          title: "Batas sumber daya terlampaui",
          body: "Salah satu metrik sumber daya telah melampati ambang batas.",
        },
      };
    default:
      return { SetThermalMode: { mode: "balanced" } };
  }
}

function metricLabel(m) {
  const labels = {
    CpuUtil: "CPU Utilisasi",
    GpuUtil: "GPU Utilisasi",
    CpuTemp: "Suhu CPU",
    GpuTemp: "Suhu GPU",
    GpuPowerW: "Daya GPU (W)",
    GpuVramPct: "VRAM GPU (%)",
  };
  return labels[m] || m;
}

function actionLabel(a) {
  if (a.SetThermalMode) return `Mode termal: ${a.SetThermalMode.mode}`;
  if (a.Notify) return `Notifikasi: ${a.Notify.title}`;
  return "Aksi";
}

function render() {
  const container = $("resource-rules-list");
  const cpuEl = $("resource-cpu-util");
  const gpuEl = $("resource-gpu-util");
  const ctempEl = $("resource-cpu-temp");
  const gtempEl = $("resource-gpu-temp");

  if (status) {
    if (cpuEl) cpuEl.textContent = `${Math.round(status.cpu?.overall_util ?? 0)}%`;
    if (gpuEl) gpuEl.textContent = `${status.gpu?.util ?? 0}%`;
    if (ctempEl)
      ctempEl.textContent =
        status.cpu?.temp_c != null ? `${status.cpu.temp_c.toFixed(1)}°C` : "--";
    if (gtempEl)
      gtempEl.textContent =
        status.gpu?.temp_c != null ? `${status.gpu.temp_c}°C` : "--";
  }

  if (!container) return;
  container.innerHTML = "";
  if (!rules || rules.length === 0) {
    container.innerHTML =
      '<p class="muted">Belum ada aturan sumber daya. Tambahkan aturan di bawah.</p>';
    return;
  }
  for (const r of rules) {
    const el = document.createElement("div");
    el.className = "card resource-rule-card";
    el.innerHTML = `
      <div class="resource-rule-header">
        <div>
          <strong>${escapeHtml(r.name)}</strong>
          <span class="rule-badge ${r.enabled ? "rule-active" : "rule-inactive"}">
            ${r.enabled ? "Aktif" : "Nonaktif"}
          </span>
        </div>
        <div class="resource-rule-actions">
          <button class="btn-icon btn-toggle-rule" data-id="${r.id}" title="${r.enabled ? "Nonaktifkan" : "Aktifkan"}">
            ${r.enabled ? "\u2716" : "\u2713"}
          </button>
          <button class="btn-icon btn-del-rule" data-id="${r.id}" title="Hapus">\u274c</button>
        </div>
      </div>
      <div class="resource-rule-body">
        <span class="muted small">Jika</span> <strong>${escapeHtml(metricLabel(r.condition?.Any?.[0]?.metric || ""))}</span>
        <span class="muted small">melebihi</span>
        <strong>${r.condition?.Any?.[0]?.critical ?? ""}</span>
        <span class="muted small">(warn ${r.condition?.Any?.[0]?.warn ?? ""})</span>
        <span class="muted small">maka</span>
        <strong>${escapeHtml(actionLabel(r.action))}</strong>
      </div>
    `;
    container.appendChild(el);
  }

  // Wire delete/toggle
  container.querySelectorAll(".btn-del-rule").forEach((btn) => {
    btn.addEventListener("click", () => {
      const id = btn.dataset.id;
      invoke("cmd_delete_rule", { id }).then((res) => {
        if (res && res.ok) {
          toast("Aturan dihapus.");
          loadResource();
        } else {
          toast(res?.message || "Gagal menghapus", true);
        }
      });
    });
  });
    container.querySelectorAll(".btn-toggle-rule").forEach((btn) => {
      btn.addEventListener("click", () => {
        const id = btn.dataset.id;
        invoke("cmd_toggle_rule", { id }).then((res) => {
          if (res && res.ok) {
            toast(res.message);
            loadResource();
          } else {
            toast(res?.message || "Gagal mengubah aturan", true);
          }
        });
      });
    });
}
