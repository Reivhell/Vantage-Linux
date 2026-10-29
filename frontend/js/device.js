// Screen 7: Device Settings and real hardware information
import { $, invoke, run, toast } from "./core.js";

function text(id, value) {
  const el = $(id);
  if (el) el.textContent = value === null || value === undefined || value === "" ? "--" : String(value);
}

function renderDeviceInfo(dev) {
  if (!dev) return;
  const model = [dev.vendor, dev.product].filter(Boolean).join(" ");
  text("dev-info-model", model);
  text("dev-info-family", dev.product_family);
  text("dev-info-version", dev.version);
  text("dev-info-serial", dev.serial);
  text("dev-info-board", dev.board);
  text("dev-info-bios", `${dev.bios_vendor || ""} ${dev.bios_label || ""}`.trim());
  text("dev-info-os", [dev.os_name, dev.os_version].filter(Boolean).join(" "));
  text("dev-info-kernel", dev.kernel);
  text("dev-info-session", dev.session_label);
  text("dev-info-host", dev.hostname);
  text("dev-info-cpu", dev.cpu_model);
  text("dev-info-cpu-cores", `${dev.cpu_cores || "--"} core / ${dev.cpu_threads || "--"} thread`);
  text("dev-info-gpu", dev.gpu_name);
  text("dev-info-gpu-driver", dev.gpu_driver ? `Driver ${dev.gpu_driver}` : "");
  text("dev-info-storage", dev.storage);
  text("dev-info-memory", dev.memory_label);
  text("dev-info-uptime", dev.uptime_label);
  text(
    "dev-info-health",
    dev.battery_health_pct === null || dev.battery_health_pct === undefined
      ? "--"
      : `${Math.round(dev.battery_health_pct)}%`
  );
  text("dev-info-cycles", dev.battery_cycles ?? "--");
  text("dev-info-health-score", `${dev.health_score ?? "--"}/100 · ${dev.health_level || "--"}`);
}

async function loadDeviceInfo() {
  try {
    const dev = await invoke("cmd_get_device_info");
    renderDeviceInfo(dev);
  } catch (err) {
    console.error("loadDeviceInfo error:", err);
  }
}

export async function loadDevice() {
  try {
    const [settings, dev] = await Promise.all([
      invoke("cmd_get_device_settings").catch(() => null),
      invoke("cmd_get_device_info").catch(() => null),
    ]);

    if (settings) {
      if ($("switch-dev-backlight")) $("switch-dev-backlight").checked = settings.keyboard_backlight ?? true;
      if ($("switch-dev-touchpad")) $("switch-dev-touchpad").checked = settings.touchpad ?? true;
      if ($("switch-dev-camera")) $("switch-dev-camera").checked = settings.camera ?? true;
      if ($("switch-dev-fnlock")) $("switch-dev-fnlock").checked = settings.fn_lock ?? false;
      if ($("switch-dev-winkey")) $("switch-dev-winkey").checked = settings.win_key ?? true;
      if ($("switch-dev-usbcharge")) $("switch-dev-usbcharge").checked = settings.usb_charging ?? false;
      if ($("switch-dev-vantagekey")) $("switch-dev-vantagekey").checked = settings.vantage_key ?? true;
    }
    renderDeviceInfo(dev);
  } catch (err) {
    console.error("loadDevice error:", err);
  }
}

export function initDevice() {
  const setupToggle = (id, key, label) => {
    $(id)?.addEventListener("change", async (e) => {
      const value = e.target.checked;
      const res = await run("cmd_set_device_setting", { key, value });
      if (res) toast(`${label} ${value ? "diaktifkan" : "dinonaktifkan"}`);
    });
  };

  setupToggle("switch-dev-backlight", "keyboard_backlight", "Keyboard Backlight");
  setupToggle("switch-dev-touchpad", "touchpad", "Touchpad");
  setupToggle("switch-dev-camera", "camera", "Kamera");
  setupToggle("switch-dev-fnlock", "fn_lock", "Fn Lock");
  setupToggle("switch-dev-winkey", "win_key", "Tombol Windows");
  setupToggle("switch-dev-usbcharge", "usb_charging", "USB Charging saat Mati");
  setupToggle("switch-dev-vantagekey", "vantage_key", "Lenovo Vantage Key");

  $("btn-refresh-device-info")?.addEventListener("click", async () => {
    await loadDeviceInfo();
    toast("Informasi perangkat dipindai ulang.");
  });
}
