import { $, escapeHtml, invoke, run, toast } from "./core.js";

function renderDisplayOutput(output) {
  const groups = output.groups || [];
  let selectedGroup = output.selected_group ?? 0;
  let selectedMode = output.selected_mode ?? 0;

  const card = document.createElement("article");
  card.className = "display-card";
  card.innerHTML = `<div class="display-card-head">
      <div class="display-identity">
        <div class="display-name-line">
          <h3>${escapeHtml(output.name)}</h3>
          ${output.primary ? '<span class="pill ok">Primary</span>' : ""}
        </div>
      </div>
      <div class="display-ceiling">
        <span class="display-ceiling-value mono">${escapeHtml(output.max_refresh_label)}</span>
        <span class="display-ceiling-unit">Hz max</span>
      </div>
    </div>
    <div class="display-current">
      <span class="display-current-label">Current</span>
      <span class="mono">${escapeHtml(output.current_resolution_label)}</span>
      <span class="pill neutral">${escapeHtml(output.current_refresh_label)} Hz</span>
    </div>
    <div class="display-controls">
      <label class="field display-resolution">
        <span>Resolution</span>
        <select class="display-res-select"${groups.length ? "" : " disabled"}>
          ${
            groups.length
              ? groups
                  .map(
                    (group, index) =>
                      `<option value="${index}"${index === selectedGroup ? " selected" : ""}>${escapeHtml(group.resolution_label)}</option>`
                  )
                  .join("")
              : "<option>No modes reported</option>"
          }
        </select>
      </label>
      <div class="field display-refresh">
        <span>Refresh rate</span>
        <div class="rate-chips" role="group" aria-label="Supported refresh rates"></div>
      </div>
    </div>
    <div class="display-card-foot">
      <p class="display-support"></p>
      <button class="btn primary display-apply" type="button"></button>
    </div>`;

  const select = card.querySelector(".display-res-select");
  const rateHost = card.querySelector(".rate-chips");
  const support = card.querySelector(".display-support");
  const apply = card.querySelector(".display-apply");

  function renderRates() {
    const group = groups[selectedGroup];
    if (!group) {
      rateHost.innerHTML = '<span class="muted small">No modes reported.</span>';
      support.textContent = "The active adapter did not report any modes for this output.";
      apply.disabled = true;
      apply.classList.remove("current");
      apply.textContent = "Unavailable";
      return;
    }
    selectedMode = Math.min(selectedMode, group.modes.length - 1);
    rateHost.innerHTML = group.modes
      .map(
        (mode, index) =>
          `<button type="button" class="rate-chip${index === selectedMode ? " active" : ""}" data-rate="${index}" aria-pressed="${index === selectedMode}">${escapeHtml(mode.label)}<span>Hz</span></button>`
      )
      .join("");
    support.innerHTML = `Supported <span class="mono">${group.modes
      .map((mode) => escapeHtml(mode.label))
      .join(" · ")} Hz</span> <span class="muted">· ceiling ${escapeHtml(group.max_refresh_label)} Hz</span>`;

    const mode = group.modes[selectedMode];
    const isCurrent = Boolean(mode?.current);
    apply.disabled = !mode || isCurrent;
    apply.classList.toggle("current", isCurrent);
    apply.textContent = isCurrent ? "Current mode" : "Apply mode";
  }

  select.addEventListener("change", () => {
    selectedGroup = Number(select.value);
    const group = groups[selectedGroup];
    selectedMode = group ? group.modes.findIndex((mode) => mode.current) : -1;
    if (selectedMode < 0) selectedMode = 0;
    renderRates();
  });

  rateHost.addEventListener("click", (event) => {
    const button = event.target.closest("[data-rate]");
    const group = groups[selectedGroup];
    if (!button || !group) return;
    selectedMode = Number(button.dataset.rate);
    renderRates();
  });

  apply.addEventListener("click", async () => {
    const mode = groups[selectedGroup]?.modes[selectedMode];
    if (!mode) return;
    apply.disabled = true;
    apply.textContent = "Applying…";
    const res = await run("cmd_set_display_mode", {
      output: output.name,
      width: mode.width,
      height: mode.height,
      refresh: mode.refresh_hz,
    });
    if (res) {
      toast(`${output.name}: ${mode.width} × ${mode.height} @ ${mode.label} Hz`);
      await loadDisplay();
    } else {
      renderRates();
    }
  });

  renderRates();
  return card;
}

export async function loadDisplay() {
  const info = await invoke("cmd_get_display_info");
  $("display-meta").innerHTML = (info.summary || [])
    .map(
      (row) =>
        `<div><span class="display-meta-label">${escapeHtml(row.label)}</span><span class="mono">${escapeHtml(row.value)}</span></div>`
    )
    .join("");
  $("display-note").textContent = info.error
    ? `Adapter error: ${info.error}`
    : "Modes come from the active compositor. Rust validates the selection against reported modes before applying it.";

  const host = $("display-outputs");
  host.innerHTML = "";
  (info.outputs || []).forEach((output) => host.appendChild(renderDisplayOutput(output)));
  if (!info.outputs?.length) {
    host.innerHTML = '<p class="muted small">No outputs reported by the active adapter.</p>';
  }

  const brightnessAvailable = Boolean(info.brightness_device);
  $("panel-bright").value = info.brightness_pct ?? 50;
  $("panel-bright-label").textContent = info.brightness_label;
  $("panel-bright").disabled = !brightnessAvailable;
  $("panel-bright-apply").disabled = !brightnessAvailable;
  document.querySelectorAll("[data-bright]").forEach((button) => {
    button.disabled = !brightnessAvailable;
  });
}

$("panel-bright").addEventListener("input", (event) => {
  $("panel-bright-label").textContent = `${event.target.value}%`;
});
document.querySelectorAll("[data-bright]").forEach((button) => {
  button.addEventListener("click", () => {
    $("panel-bright").value = button.dataset.bright;
    $("panel-bright-label").textContent = `${button.dataset.bright}%`;
  });
});
$("panel-bright-apply").addEventListener("click", async () => {
  const res = await run("cmd_set_brightness", { percent: Number($("panel-bright").value) });
  if (res) {
    toast(`Brightness: ${$("panel-bright").value}%`);
    loadDisplay();
  }
});
