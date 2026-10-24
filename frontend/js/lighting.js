import { $, invoke, run, toast } from "./core.js";

let lightingInfo = null;

function zoneLabel(index) {
  return `Zone ${index + 1}`;
}

export async function loadLighting() {
  lightingInfo = await invoke("cmd_get_lighting_info");
  const effect = $("light-effect");
  effect.innerHTML = "";
  (lightingInfo.effects || []).forEach((name) => {
    const option = document.createElement("option");
    option.value = name;
    option.textContent = name;
    effect.appendChild(option);
  });
  renderZones();
}

function renderZones() {
  const host = $("zones");
  host.innerHTML = "";
  const count = lightingInfo?.zones || 0;
  const color = lightingInfo?.default_color || "#ffffff";
  for (let index = 0; index < count; index++) {
    const element = document.createElement("div");
    element.className = "zone";
    element.innerHTML = `<label>${zoneLabel(index)}</label>
      <input type="color" value="${color}" data-zone="${index}" />
      <button class="btn" data-zone-apply="${index}">Apply</button>`;
    host.appendChild(element);
  }
}

$("zones").addEventListener("click", async (event) => {
  const index = event.target.dataset.zoneApply;
  if (index === undefined) return;
  const input = document.querySelector(`[data-zone="${index}"]`);
  const res = await run("cmd_keyboard_zone", {
    cmd: { zone: Number(index), color: input.value },
  });
  if (res) toast(`Zone ${Number(index) + 1} colour applied`);
});

$("brightness").addEventListener("input", (event) => {
  $("bright-label").textContent = `${event.target.value}%`;
});

$("light-apply").addEventListener("click", async () => {
  const zones = [...document.querySelectorAll("[data-zone]")].map((input, index) => ({
    zone: index,
    color: input.value,
  }));
  const cmd = {
    zones,
    effect: $("light-effect").value,
    brightness_pct: Number($("brightness").value),
  };
  const res = await run("cmd_keyboard_all", { cmd });
  if (res) toast("Lighting applied");
});

$("light-off").addEventListener("click", async () => {
  const res = await run("cmd_keyboard_off");
  if (res) toast("Lighting off");
});
