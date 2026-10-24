import { $, invoke, run, toast } from "./core.js";

export function renderPresetChips(hostId, list, onApply) {
  const host = $(hostId);
  if (!host) return;
  host.innerHTML = "";
  (list || []).forEach((preset) => {
    const chip = document.createElement("button");
    chip.className = `chip${preset.active ? " active" : ""}`;
    chip.disabled = !preset.available;
    chip.title = preset.description || "";
    chip.textContent = preset.label;
    chip.onclick = () => onApply(preset);
    host.appendChild(chip);
  });
}

export async function applyPreset(preset, reload) {
  const res = await run("cmd_apply_preset", { name: preset.key });
  if (!res) return;
  toast(`Preset: ${preset.label}`);
  await reload?.();
}

export async function loadPresets() {
  return invoke("cmd_get_presets");
}
