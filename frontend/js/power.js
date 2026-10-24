import { $, escapeHtml, fmt, invoke, run, toast } from "./core.js";
import { applyPreset, loadPresets, renderPresetChips } from "./presets.js";
import { loadConfig } from "./settings.js";

export async function loadPower() {
  const presets = await loadPresets();
  renderPresetChips("preset-modes", presets, (preset) => applyPreset(preset, loadPower));

  const battery = await invoke("cmd_get_battery_detail");
  $("bat-detail").innerHTML = battery?.available
    ? [
        ["Charge", battery.percent === null ? "--" : `${fmt(battery.percent, 0)} %`],
        ["Status", battery.status || "--"],
        ["Health", battery.health_pct === null ? "--" : `${fmt(battery.health_pct)} %`],
        ["Cycles", battery.cycles ?? "--"],
        ["Temperature", battery.temp_c === null ? "--" : `${fmt(battery.temp_c)} °C`],
        ["Energy", battery.energy_full_wh === null ? "--" : `${fmt(battery.energy_full_wh)} / ${fmt(battery.energy_design_wh)} Wh`],
        ["Power", battery.power_w === null ? "--" : `${fmt(battery.power_w)} W`],
      ]
        .map(([k, v]) => `<div><span class="muted">${k}</span><span class="mono">${escapeHtml(v)}</span></div>`)
        .join("")
    : '<div class="muted small">No battery detected.</div>';
  $("bat-threshold-field").hidden = !battery?.threshold_available;
  $("bat-threshold-apply").hidden = !battery?.threshold_available;
  if (battery?.threshold_available) {
    $("bat-threshold").value = battery.threshold_pct ?? 100;
    $("bat-threshold-label").textContent = `${battery.threshold_pct ?? 100}%`;
  }
  $("bat-threshold-note").textContent = battery?.threshold_available
    ? "Limits charging to extend battery lifespan. Set 100% to disable."
    : "Firmware does not expose a charge threshold; use the charging modes above.";

  const thermal = await invoke("cmd_get_thermal_mode");
  const thermalHost = $("thermal-modes");
  thermalHost.innerHTML = "";
  (thermal.choices || []).forEach((mode) => {
    const chip = document.createElement("button");
    chip.className = `chip${mode.key === thermal.current ? " active" : ""}`;
    chip.textContent = mode.label;
    chip.onclick = async () => {
      const res = await run("cmd_set_thermal_mode", { mode: mode.key });
      if (res) {
        toast(`Laptop mode: ${mode.label}`);
        await loadConfig();
        loadPower();
      }
    };
    thermalHost.appendChild(chip);
  });
  $("thermal-note").textContent = thermal.available
    ? `Active: ${thermal.current_label}. Firmware platform profile.`
    : "No thermal mode interface detected.";

  const charge = await invoke("cmd_get_charge_mode");
  const chargeHost = $("charge-modes");
  chargeHost.innerHTML = "";
  (charge.supported || []).forEach((mode) => {
    const chip = document.createElement("button");
    chip.className = `chip${mode.key === charge.current ? " active" : ""}`;
    chip.textContent = mode.label;
    chip.onclick = async () => {
      const res = await run("cmd_set_charge_mode", { mode: mode.key });
      if (res) {
        toast(`Charging: ${mode.label}`);
        loadPower();
      }
    };
    chargeHost.appendChild(chip);
  });
  $("charge-label").textContent = !charge.available
    ? "Not supported on this firmware."
    : charge.current
    ? `Active: ${charge.current_label}`
    : "Select a charging mode.";

  const limits = await invoke("cmd_get_power_limits");
  const host = $("limits");
  host.innerHTML = "";
  (limits || []).forEach((limit) => {
    const element = document.createElement("div");
    element.className = "limit";
    element.innerHTML = `<div class="lb">${escapeHtml(limit.label)}${limit.writable ? "" : " · read-only"}</div>
      <div class="val">${limit.current} ${escapeHtml(limit.unit)}</div>
      ${limit.writable ? "" : `<div class="tr"><i style="transform:scaleX(${limit.position_pct / 100})"></i></div>`}`;
    if (limit.writable) {
      const row = document.createElement("div");
      row.className = "apply-row";
      row.innerHTML = `<input type="range" min="${limit.min_val}" max="${limit.max_val}"
          value="${limit.current}" step="1" />
        <span class="val mono">${limit.current} ${escapeHtml(limit.unit)}</span>
        <button class="btn">Apply</button>`;
      const range = row.querySelector("input");
      const readout = row.querySelector(".val");
      range.addEventListener("input", () => {
        readout.textContent = `${range.value} ${limit.unit}`;
      });
      row.querySelector("button").addEventListener("click", async () => {
        const res = await run("cmd_set_power_limit", {
          cmd: { attr: limit.key, value: Number(range.value) },
        });
        if (res) {
          toast(`${limit.label}: ${range.value} ${limit.unit}`);
          loadPower();
        }
      });
      element.appendChild(row);
    }
    host.appendChild(element);
  });
  if (!limits?.length) {
    host.innerHTML = '<p class="muted small">No firmware power-limit attributes found.</p>';
  }
}

$("bat-threshold").addEventListener("input", (event) => {
  $("bat-threshold-label").textContent = `${event.target.value}%`;
});

$("bat-threshold-apply").addEventListener("click", async () => {
  const percent = Number($("bat-threshold").value);
  const res = await run("cmd_set_charge_threshold", { percent });
  if (res) {
    toast(`Charge limit: ${percent}%`);
    loadPower();
  }
});
