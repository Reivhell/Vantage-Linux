// Otomatisasi sumber daya: editor aturan, telemetri nyata, dan audit eksekusi.
import { $, invoke, run, toast, showModal, escapeHtml } from "./core.js";

const METRICS = [
  { key: "CpuUtil", label: "Utilisasi CPU", unit: "%", direction: "Above", warn: 85, critical: 95 },
  { key: "CpuTemp", label: "Suhu CPU", unit: "°C", direction: "Above", warn: 85, critical: 95 },
  { key: "GpuUtil", label: "Utilisasi GPU", unit: "%", direction: "Above", warn: 90, critical: 98 },
  { key: "GpuTemp", label: "Suhu GPU", unit: "°C", direction: "Above", warn: 85, critical: 100 },
  { key: "GpuPowerW", label: "Daya GPU", unit: "W", direction: "Above", warn: 80, critical: 100 },
  { key: "GpuVramPct", label: "VRAM GPU", unit: "%", direction: "Above", warn: 85, critical: 95 },
  { key: "FanRpm", label: "RPM kipas", unit: "RPM", direction: "Above", warn: 4000, critical: 4700 },
  { key: "BatteryPct", label: "Baterai", unit: "%", direction: "Below", warn: 22, critical: 15 },
  { key: "RamPct", label: "RAM", unit: "%", direction: "Above", warn: 85, critical: 92 },
];

const METRIC_BY_KEY = Object.fromEntries(METRICS.map((m) => [m.key, m]));

const ACTION_LABELS = {
  Notify: "Notifikasi desktop",
  SetThermalMode: "Ubah mode thermal",
  SetFanAuto: "Kembalikan kipas ke firmware",
  SetFanCurve: "Terapkan kurva kipas",
  SetFanManual: "Atur PWM kipas manual",
  SetGpuPowerLimit: "Batasi daya GPU",
  SetGpuRuntimePm: "Ubah runtime PM GPU",
  Renice: "Ubah prioritas proses",
};

let editing = null; // null = aturan baru, selain itu rule asli
let pollTimer = null;
let pendingAction = null;

const metricLabel = (key) => METRIC_BY_KEY[key]?.label || key;
const metricUnit = (key) => METRIC_BY_KEY[key]?.unit || "";
const pctLabel = (value) =>
  value === null || value === undefined ? "--" : `${Math.round(value)}%`;

export function updateAutomationLive(stats) {
  if (!stats) return;
  if ($("auto-cpu")) $("auto-cpu").textContent = pctLabel(stats.cpu_percent);
  if ($("auto-cpu-temp")) {
    $("auto-cpu-temp").textContent = stats.cpu_temp ? `${Math.round(stats.cpu_temp)} °C` : "-- °C";
  }
  if ($("auto-gpu")) $("auto-gpu").textContent = pctLabel(stats.gpu_util);
  if ($("auto-gpu-temp")) {
    $("auto-gpu-temp").textContent = stats.gpu_temp ? `${stats.gpu_temp} °C` : "-- °C";
  }
  if ($("auto-fan")) {
    const rpm = Math.max(stats.fan1_rpm || 0, stats.fan2_rpm || 0);
    $("auto-fan").textContent = rpm > 0 ? `${rpm}` : "--";
  }
  if ($("auto-battery")) $("auto-battery").textContent = pctLabel(stats.battery_percent);
  if ($("auto-ram")) $("auto-ram").textContent = pctLabel(stats.ram_percent);
  if ($("auto-power")) {
    const plugged = stats.power_plugged;
    $("auto-power").textContent =
      plugged === true ? "AC tersambung" : plugged === false ? "Mode baterai" : "--";
  }
}

export async function loadAutomation() {
  try {
    const [status, rules, events] = await Promise.all([
      invoke("cmd_get_resource_status"),
      invoke("cmd_list_rules"),
      invoke("cmd_get_automation_events", { limit: 80 }),
    ]);
    renderStatus(status);
    renderRules(Array.isArray(rules) ? rules : [], status);
    renderEvents(Array.isArray(events) ? events : []);
    if ($("automation-engine-state")) {
      $("automation-engine-state").textContent = status ? "Engine aktif" : "Engine idle";
    }
  } catch (error) {
    if ($("automation-rule-list")) {
      $("automation-rule-list").innerHTML =
        '<p class="automation-empty">Gagal memuat otomatisasi: backend tidak merespons.</p>';
    }
    console.error("loadAutomation error:", error);
  }
}

async function refreshLive() {
  try {
    const [status, events] = await Promise.all([
      invoke("cmd_get_resource_status"),
      invoke("cmd_get_automation_events", { limit: 80 }),
    ]);
    renderStatus(status);
    renderEvents(Array.isArray(events) ? events : []);
    if (status?.rules && !$("automation-editor").hidden) {
      // Jangan menimpa form yang sedang diedit; hanya runtime di daftar yang diperbarui.
      updateRuleRuntime(status.rules);
    } else if (status?.rules) {
      const rules = await invoke("cmd_list_rules");
      renderRules(Array.isArray(rules) ? rules : [], status);
    }
  } catch (error) {
    console.warn("automation live refresh failed:", error);
  }
}

function renderStatus(status) {
  if (!status) return;
  if ($("auto-cpu")) $("auto-cpu").textContent = pctLabel(status.cpu?.overall_util);
  if ($("auto-cpu-temp")) {
    $("auto-cpu-temp").textContent =
      status.cpu?.temp_c == null ? "-- °C" : `${Math.round(status.cpu.temp_c)} °C`;
  }
  if ($("auto-gpu")) $("auto-gpu").textContent = pctLabel(status.gpu?.util);
  if ($("auto-gpu-temp")) {
    $("auto-gpu-temp").textContent =
      status.gpu?.temp_c == null ? "-- °C" : `${status.gpu.temp_c} °C`;
  }
  if ($("auto-fan")) {
    const rpm = Math.max(status.system?.fan1_rpm || 0, status.system?.fan2_rpm || 0);
    $("auto-fan").textContent = rpm > 0 ? `${rpm}` : "--";
  }
  if ($("auto-battery")) $("auto-battery").textContent = pctLabel(status.system?.battery_pct);
  if ($("auto-ram")) $("auto-ram").textContent = pctLabel(status.system?.ram_pct);
  if ($("auto-power")) {
    const plugged = status.system?.plugged;
    $("auto-power").textContent =
      plugged === true ? "AC tersambung" : plugged === false ? "Mode baterai" : "--";
  }
}

function ruleMetricLabel(rule) {
  const thresholds = thresholdsOf(rule);
  if (thresholds.length === 0) return "Tanpa ambang";
  const labels = thresholds.map((t) => {
    const meta = METRIC_BY_KEY[t.metric];
    return `${meta?.label || t.metric} ${t.direction === "Below" ? "≤" : "≥"} ${t.critical}${meta?.unit || ""}`;
  });
  return labels.join(" · ");
}

function thresholdsOf(rule) {
  return rule?.condition?.All || rule?.condition?.Any || [];
}

function actionLabel(rule) {
  const entry = Object.entries(rule?.action || {})[0];
  if (!entry) return "--";
  return ACTION_LABELS[entry[0]] || entry[0];
}

function renderRules(rules, status) {
  const host = $("automation-rule-list");
  if (!host) return;
  host.innerHTML = "";
  if ($("automation-rule-count")) {
    const active = rules.filter((r) => r.enabled).length;
    $("automation-rule-count").textContent = `${active} aktif dari ${rules.length} aturan`;
  }
  if (rules.length === 0) {
    host.innerHTML = '<p class="automation-empty">Belum ada aturan. Buat aturan pertama untuk mengotomatisasi perangkat.</p>';
    return;
  }
  for (const rule of rules) {
    host.appendChild(renderRule(rule, status));
  }
}

function renderRule(rule, status) {
  const runtime = (status?.rules || []).find((r) => r.id === rule.id);
  const el = document.createElement("div");
  el.className = `rule-item${rule.enabled ? "" : " disabled"}`;
  el.dataset.ruleId = rule.id;
  const mode = rule.condition?.All ? "ALL" : "ANY";
  const last = runtime?.last_triggered_at
    ? new Date(runtime.last_triggered_at * 1000).toLocaleString()
    : "belum pernah";
  el.innerHTML = `
    <div class="rule-main">
      <div class="rule-title-row">
        <strong class="rule-name">${escapeHtml(rule.name)}</strong>
        <span class="badge-soft">${mode}</span>
        <span class="badge-soft${rule.enabled ? " good" : ""}">${rule.enabled ? "aktif" : "nonaktif"}</span>
      </div>
      <p class="rule-meta">${escapeHtml(ruleMetricLabel(rule))}</p>
      <p class="rule-meta">Aksi: ${escapeHtml(actionLabel(rule))} · eksekusi: ${runtime?.trigger_count || 0} · terakhir: ${escapeHtml(last)}</p>
      ${runtime?.last_error ? `<p class="rule-error">${escapeHtml(runtime.last_error)}</p>` : ""}
    </div>
    <div class="rule-actions">
      <label class="switch" title="Aktifkan / nonaktifkan">
        <input type="checkbox" data-rule-toggle="${escapeHtml(rule.id)}" ${rule.enabled ? "checked" : ""} />
        <span class="slider"></span>
      </label>
      <button class="btn-secondary btn-compact" type="button" data-rule-edit="${escapeHtml(rule.id)}">Edit</button>
      <button class="btn-secondary btn-compact danger" type="button" data-rule-delete="${escapeHtml(rule.id)}">Hapus</button>
    </div>`;
  return el;
}

function updateRuleRuntime(summaries) {
  document.querySelectorAll("[data-rule-id]").forEach((item) => {
    const summary = summaries.find((r) => r.id === item.dataset.ruleId);
    if (!summary) return;
    const meta = item.querySelectorAll(".rule-meta")[1];
    if (!meta) return;
    const last = summary.last_triggered_at
      ? new Date(summary.last_triggered_at * 1000).toLocaleString()
      : "belum pernah";
    meta.textContent = `Aksi: ${summary.action} · eksekusi: ${summary.trigger_count} · terakhir: ${last}`;
  });
}

function renderEvents(events) {
  const host = $("automation-event-list");
  if (!host) return;
  host.innerHTML = "";
  if (events.length === 0) {
    host.innerHTML = '<p class="automation-empty">Belum ada aksi yang dijalankan.</p>';
    return;
  }
  for (const event of events) {
    const el = document.createElement("div");
    el.className = `event-item ${event.status === "error" ? "error" : "ok"}`;
    const when = new Date(event.timestamp * 1000).toLocaleString();
    el.innerHTML = `
      <div class="event-top">
        <strong>${escapeHtml(event.rule_name)}</strong>
        <span class="badge-soft${event.status === "error" ? " bad" : " good"}">${event.status === "error" ? "gagal" : "ok"}</span>
      </div>
      <p class="event-action">${escapeHtml(event.action)}</p>
      <p class="event-meta">${escapeHtml(when)}${event.detail ? ` · ${escapeHtml(event.detail)}` : ""}</p>`;
    host.appendChild(el);
  }
}

// ---------------------------------------------------------------------------
// Editor aturan
// ---------------------------------------------------------------------------

function openEditor(rule) {
  editing = rule || null;
  const editor = $("automation-editor");
  if (!editor) return;
  editor.hidden = false;
  if ($("auto-editor-title")) $("auto-editor-title").textContent = rule ? "Edit aturan" : "Aturan baru";
  if ($("auto-rule-name")) $("auto-rule-name").value = rule?.name || "";
  const idInput = $("auto-rule-id");
  if (idInput) {
    idInput.value = rule?.id || suggestId();
    idInput.disabled = Boolean(rule);
  }
  if ($("auto-rule-enabled")) $("auto-rule-enabled").checked = rule ? rule.enabled : true;
  const mode = rule?.condition?.All ? "All" : "Any";
  if ($("auto-condition-mode")) $("auto-condition-mode").value = mode;

  const host = $("auto-conditions");
  host.innerHTML = "";
  const thresholds = thresholdsOf(rule);
  if (thresholds.length === 0) {
    addConditionRow();
  } else {
    thresholds.forEach((t) => addConditionRow(t));
  }

  const entry = Object.entries(rule?.action || {})[0] || ["Notify", { title: "", body: "" }];
  if ($("auto-action-kind")) $("auto-action-kind").value = entry[0];
  renderActionFields(entry[0], entry[1]);

  setEditorError("");
  editor.scrollIntoView({ behavior: "smooth", block: "start" });
}

function closeEditor() {
  editing = null;
  pendingAction = null;
  if ($("automation-editor")) $("automation-editor").hidden = true;
  setEditorError("");
}

function suggestId() {
  return `rule-${Date.now().toString(36)}`;
}

function addConditionRow(threshold) {
  const host = $("auto-conditions");
  if (!host) return;
  if (host.children.length >= 8) {
    toast("Maksimal 8 ambang per aturan", true);
    return;
  }
  const metric = threshold?.metric || "CpuTemp";
  const meta = METRIC_BY_KEY[metric] || METRICS[1];
  const row = document.createElement("div");
  row.className = "condition-row";
  row.innerHTML = `
    <label class="automation-field">
      <span>Metrik</span>
      <select class="settings-select" data-c-metric>
        ${METRICS.map(
          (m) => `<option value="${m.key}"${m.key === metric ? " selected" : ""}>${m.label} (${m.unit})</option>`
        ).join("")}
      </select>
    </label>
    <label class="automation-field">
      <span>Arah</span>
      <select class="settings-select" data-c-direction>
        <option value="Above"${(threshold?.direction || meta.direction) === "Above" ? " selected" : ""}>Di atas ambang</option>
        <option value="Below"${(threshold?.direction || meta.direction) === "Below" ? " selected" : ""}>Di bawah ambang</option>
      </select>
    </label>
    <label class="automation-field">
      <span>Pulih (warn)</span>
      <input class="settings-select mono" type="number" step="any" data-c-warn value="${threshold?.warn ?? meta.warn}" />
    </label>
    <label class="automation-field">
      <span>Pemicu (critical)</span>
      <input class="settings-select mono" type="number" step="any" data-c-critical value="${threshold?.critical ?? meta.critical}" />
    </label>
    <label class="automation-field">
      <span>Cooldown (detik)</span>
      <input class="settings-select mono" type="number" min="5" max="86400" step="1" data-c-cooldown value="${threshold?.cooldown_secs ?? 120}" />
    </label>
    <button class="btn-secondary btn-compact danger condition-remove" type="button" aria-label="Hapus ambang">Hapus</button>`;
  row.querySelector("[data-c-metric]").addEventListener("change", (event) => {
    const next = METRIC_BY_KEY[event.target.value] || meta;
    row.querySelector("[data-c-direction]").value = next.direction;
    row.querySelector("[data-c-warn]").value = next.warn;
    row.querySelector("[data-c-critical]").value = next.critical;
  });
  row.querySelector(".condition-remove").addEventListener("click", () => {
    if (host.children.length <= 1) {
      toast("Aturan harus memiliki minimal satu ambang", true);
      return;
    }
    row.remove();
  });
  host.appendChild(row);
}

function renderActionFields(kind, action = {}) {
  const host = $("auto-action-fields");
  if (!host) return;
  pendingAction = { kind, action };
  host.innerHTML = "";
  const field = (label, input) => {
    const wrapper = document.createElement("label");
    wrapper.className = "automation-field";
    const span = document.createElement("span");
    span.textContent = label;
    wrapper.appendChild(span);
    wrapper.appendChild(input);
    host.appendChild(wrapper);
  };
  const text = (name, value, type = "text", attrs = {}) => {
    const input = document.createElement("input");
    input.className = "settings-select mono";
    input.type = type;
    input.dataset.aField = name;
    input.value = value ?? "";
    Object.entries(attrs).forEach(([k, v]) => input.setAttribute(k, v));
    return input;
  };
  const select = (name, options, value) => {
    const el = document.createElement("select");
    el.className = "settings-select";
    el.dataset.aField = name;
    for (const [val, label] of options) {
      const option = document.createElement("option");
      option.value = val;
      option.textContent = label;
      option.selected = String(val) === String(value);
      el.appendChild(option);
    }
    return el;
  };
  const area = (name, value) => {
    const el = document.createElement("textarea");
    el.className = "settings-select mono";
    el.dataset.aField = name;
    el.rows = 5;
    el.value = value ?? "";
    return el;
  };
  const fanOptions = [
    ["1", "Kipas 1 (CPU)"],
    ["2", "Kipas 2 (GPU)"],
  ];

  if (kind === "Notify") {
    field("Judul notifikasi", text("title", action.title, "text", { maxlength: "80" }));
    field("Isi notifikasi", text("body", action.body, "text", { maxlength: "500" }));
  } else if (kind === "SetThermalMode") {
    field(
      "Mode thermal",
      select(
        "mode",
        [
          ["low-power", "Low power / Quiet"],
          ["balanced", "Balanced"],
          ["performance", "Performance"],
          ["max-power", "Max power"],
        ],
        action.mode || "low-power"
      )
    );
  } else if (kind === "SetFanAuto") {
    field("Kipas", select("fan_id", fanOptions, action.fan_id || 1));
  } else if (kind === "SetFanCurve") {
    field("Kipas", select("fan_id", fanOptions, action.fan_id || 1));
    field(
      "Kurva JSON (temp °C, pwm %)",
      area(
        "points_json",
        action.points_json ||
          '[{"temp":45,"pwm":25},{"temp":65,"pwm":45},{"temp":85,"pwm":75}]'
      )
    );
  } else if (kind === "SetFanManual") {
    field("Kipas", select("fan_id", fanOptions, action.fan_id || 1));
    field("PWM (%)", text("pwm_percent", action.pwm_percent ?? 60, "number", { min: "0", max: "100", step: "1" }));
  } else if (kind === "SetGpuPowerLimit") {
    field("Daya (W)", text("watts", action.watts ?? 80, "number", { min: "30", max: "600", step: "1" }));
  } else if (kind === "SetGpuRuntimePm") {
    field(
      "Kontrol",
      select(
        "control",
        [
          ["auto", "Auto (hemat daya)"],
          ["on", "On (selalu aktif)"],
        ],
        action.control || "auto"
      )
    );
  } else if (kind === "Renice") {
    field("PID", text("pid", action.pid ?? "", "number", { min: "2", step: "1" }));
    field("Nice (-20…19)", text("nice", action.nice ?? 10, "number", { min: "-20", max: "19", step: "1" }));
  }
}

function collectRule() {
  const thresholds = [...document.querySelectorAll("#auto-conditions .condition-row")].map((row) => {
    const metric = row.querySelector("[data-c-metric]").value;
    return {
      metric,
      direction: row.querySelector("[data-c-direction]").value,
      warn: Number(row.querySelector("[data-c-warn]").value),
      critical: Number(row.querySelector("[data-c-critical]").value),
      hysteresis_pct: 5,
      cooldown_secs: Number(row.querySelector("[data-c-cooldown]").value),
    };
  });
  const mode = $("auto-condition-mode").value;
  const actionKind = $("auto-action-kind").value;
  const fields = {};
  document.querySelectorAll("#auto-action-fields [data-a-field]").forEach((input) => {
    fields[input.dataset.aField] = input.type === "number" ? Number(input.value) : input.value;
  });
  let action;
  switch (actionKind) {
    case "SetThermalMode":
      action = { SetThermalMode: { mode: fields.mode } };
      break;
    case "SetFanAuto":
      action = { SetFanAuto: { fan_id: Number(fields.fan_id) } };
      break;
    case "SetFanCurve":
      action = {
        SetFanCurve: {
          fan_id: Number(fields.fan_id),
          points_json: String(fields.points_json || "").trim(),
        },
      };
      break;
    case "SetFanManual":
      action = { SetFanManual: { fan_id: Number(fields.fan_id), pwm_percent: Number(fields.pwm_percent) } };
      break;
    case "SetGpuPowerLimit":
      action = { SetGpuPowerLimit: { watts: Number(fields.watts) } };
      break;
    case "SetGpuRuntimePm":
      action = { SetGpuRuntimePm: { control: fields.control } };
      break;
    case "Renice":
      action = { Renice: { pid: Number(fields.pid), nice: Number(fields.nice) } };
      break;
    default:
      action = { Notify: { title: fields.title, body: fields.body } };
  }
  return {
    id: $("auto-rule-id").value.trim(),
    name: $("auto-rule-name").value.trim(),
    enabled: $("auto-rule-enabled").checked,
    condition: mode === "All" ? { All: thresholds } : { Any: thresholds },
    action,
    persistent: true,
  };
}

async function saveRule() {
  const rule = collectRule();
  if (!rule.name) return setEditorError("Nama aturan wajib diisi.");
  if (!/^[A-Za-z0-9_-]{1,64}$/.test(rule.id)) {
    return setEditorError("ID hanya boleh huruf, angka, '-' dan '_' (maksimal 64 karakter).");
  }
  setEditorError("");
  const validation = await run("cmd_validate_rule", { rule });
  if (!validation) return setEditorError("Validasi backend menolak aturan. Periksa ambang dan aksi.");
  const command = editing ? "cmd_update_rule" : "cmd_add_rule";
  const result = await run(command, { rule }, editing ? "Aturan diperbarui." : "Aturan dibuat.");
  if (!result) return;
  closeEditor();
  await loadAutomation();
}

function setEditorError(message) {
  const el = $("auto-editor-error");
  if (el) el.textContent = message;
  if (message) toast(message, true);
  return null;
}

function handleRuleListClick(event) {
  const editId = event.target.dataset.ruleEdit;
  const deleteId = event.target.dataset.ruleDelete;
  if (editId) {
    invoke("cmd_list_rules").then((rules) => {
      const rule = (rules || []).find((r) => r.id === editId);
      if (rule) openEditor(rule);
    });
  }
  if (deleteId) {
    showModal(
      "Hapus aturan",
      `<p>Hapus aturan <strong>${escapeHtml(deleteId)}</strong>? Aksi yang sudah berjalan tidak dibatalkan.</p>`,
      async () => {
        const result = await run("cmd_delete_rule", { id: deleteId }, "Aturan dihapus.");
        if (result) await loadAutomation();
      }
    );
  }
}

function handleRuleToggle(event) {
  const id = event.target.dataset.ruleToggle;
  if (!id) return;
  run("cmd_toggle_rule", { id }).then(async (result) => {
    if (result) await loadAutomation();
    else event.target.checked = !event.target.checked;
  });
}

export function initAutomation() {
  $("btn-automation-refresh")?.addEventListener("click", loadAutomation);
  $("btn-automation-new")?.addEventListener("click", () => openEditor(null));
  $("btn-auto-add-condition")?.addEventListener("click", () => addConditionRow());
  $("btn-auto-cancel")?.addEventListener("click", closeEditor);
  $("btn-auto-cancel-2")?.addEventListener("click", closeEditor);
  $("btn-auto-save")?.addEventListener("click", saveRule);
  $("auto-action-kind")?.addEventListener("change", (event) => {
    renderActionFields(event.target.value, {});
  });
  $("automation-rule-list")?.addEventListener("click", handleRuleListClick);
  $("automation-rule-list")?.addEventListener("change", handleRuleToggle);
  $("btn-automation-clear")?.addEventListener("click", () => {
    showModal(
      "Bersihkan audit",
      "<p>Hapus semua catatan audit eksekusi otomatisasi? Aturan dan konfigurasi tetap utuh.</p>",
      async () => {
        const result = await run("cmd_clear_automation_events", {}, "Audit dibersihkan.");
        if (result) await loadAutomation();
      }
    );
  });

  // Polling hanya saat tab aktif, interval longgar agar tidak membebani monitor loop.
  pollTimer = setInterval(() => {
    if ($("tab-automation")?.classList.contains("active")) refreshLive();
  }, 5000);
  window.addEventListener("beforeunload", () => clearInterval(pollTimer), { once: true });
}
