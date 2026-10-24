// Screen 7: Device Settings ("Pengaturan Perangkat")
import { $, invoke, run, toast } from "./core.js";

export async function loadDevice() {
  try {
    const settings = await invoke("cmd_get_device_settings");
    if (!settings) return;

    if ($("switch-dev-backlight")) $("switch-dev-backlight").checked = settings.keyboard_backlight ?? true;
    if ($("switch-dev-touchpad")) $("switch-dev-touchpad").checked = settings.touchpad ?? true;
    if ($("switch-dev-camera")) $("switch-dev-camera").checked = settings.camera ?? true;
    if ($("switch-dev-fnlock")) $("switch-dev-fnlock").checked = settings.fn_lock ?? false;
    if ($("switch-dev-winkey")) $("switch-dev-winkey").checked = settings.win_key ?? true;
    if ($("switch-dev-usbcharge")) $("switch-dev-usbcharge").checked = settings.usb_charging ?? false;
    if ($("switch-dev-vantagekey")) $("switch-dev-vantagekey").checked = settings.vantage_key ?? true;
  } catch (err) {
    console.error("loadDevice error:", err);
  }
}

export function initDevice() {
  const setupToggle = (id, key, label) => {
    $(id)?.addEventListener("change", async (e) => {
      const value = e.target.checked;
      const res = await run("cmd_set_device_setting", { key, value });
      if (res) {
        toast(`${label} ${value ? "diaktifkan" : "dinonaktifkan"}`);
      }
    });
  };

  setupToggle("switch-dev-backlight", "keyboard_backlight", "Keyboard Backlight");
  setupToggle("switch-dev-touchpad", "touchpad", "Touchpad");
  setupToggle("switch-dev-camera", "camera", "Kamera");
  setupToggle("switch-dev-fnlock", "fn_lock", "Fn Lock");
  setupToggle("switch-dev-winkey", "win_key", "Tombol Windows");
  setupToggle("switch-dev-usbcharge", "usb_charging", "USB Charging saat Mati");
  setupToggle("switch-dev-vantagekey", "vantage_key", "Lenovo Vantage Key");
}
