# LOQ Control Center for Linux

A hardware control center for **Lenovo LOQ / Legion** laptops, written in **Rust** with
**Tauri 2**. The UI is plain HTML/CSS/JS embedded in the binary — no Node.js toolchain and
no npm build step.


---

## Features

- **Dashboard** — device model/BIOS/kernel/session summary, battery health, health score with
  issues, and one-click performance presets.
- **Performance presets** — Quiet / Balanced / Performance / Custom mapped to the
  `platform_profile` choices this firmware actually exposes, with optional automatic
  AC/battery preset switching.
- **Battery health detail** — capacity, health, cycles, temperature, voltage, power draw,
  charge behaviour and a firmware charge-limit slider (when exposed).
- **Display control** — per-monitor resolution and refresh-rate controls, supported-rate
  detection and panel refresh ceiling across X11, KDE Wayland, GNOME Wayland (`gdctl`),
  sway/wlroots and Hyprland; panel brightness via sysfs with helper fallback.
- **Diagnostics** — read-only battery/SMART/service/journal/throttling/disk/CPU-temperature
  checks, a smart scan (failed units, orphan packages, autostart, boot errors), and Markdown
  report export to `~/.local/state/loq-control/`.
- **GPU details** — driver version, P-state, clocks, power/temperature, runtime-PM state,
  PRIME render-offload hint, power-limit control and runtime-PM switching via the helper.
- **Live telemetry** — CPU load/per-core/frequency/temperature, RAM and swap, per-mount disk
  usage and I/O throughput (`/proc/diskstats`), network RX/TX and IPv4
  (`/proc/net/dev`, `ip`), battery and AC state.
- **NVIDIA dGPU** — utilisation, VRAM, temperature, power draw and fan via `nvidia-smi`.
- **Intel iGPU** — utilisation derived from `gt_act_freq_mhz` / `gt_max_freq_mhz`.
- **Process manager** — searchable process list with CPU/memory columns and kill action
  (safety checks in Rust; PID 1 is refused).
- **Laptop mode** — firmware thermal modes (`low-power`, `balanced`, `performance`,
  `max-power`) through `platform_profile`; `power-profiles-daemon` is used when available,
  with a direct helper write as fallback. Last mode can be restored at login.
- **Battery charging** — `Fast`, `Standard` and `Long_Life` (80 % conservation) through
  `charge_types`, with fallbacks to `conservation_mode` and `charge_control_end_threshold`.
- **Power limits** — firmware attributes discovered via sysfs; editable when the helper is
  installed, read-only otherwise.
- **Fan control** — RPM readout, manual PWM override, automatic mode, and temperature→PWM
  fan curves through the privileged helper.
- **GPU mode switching** — `integrated` / `hybrid` / `nvidia` via `envycontrol`, then
  `supergfxctl`, then `prime-select`; shows the backend in use, warns when two backends
  disagree, and remembers the reboot marker across restarts.
- **Memory** — RAM/swap, top memory processes, SMBIOS DIMM details (dmidecode via helper),
  NVIDIA VRAM, and a confirmed cache-drop action.
- **4-zone ITE RGB keyboard** — direct 33-byte HID output report to `/dev/hidrawN`
  (VID `0x048d`), no root needed after the udev rule is installed.
- **Config** — TOML at `~/.config/loq-control/config.toml`.
- **Tray icon** — show/hide window and quit; closing the window hides it to the tray.
- **CLI discovery** — `loq-control --discover` prints capabilities and `--selftest` prints
  charging/thermal/memory/display/device/battery/preset state as JSON.

## Architecture

```
frontend/            static HTML/CSS/JS (Tauri IPC bridge from window.__TAURI__)
src-tauri/
  src/lib.rs         app bootstrap, shared state, monitor loop, tray, IPC registration
  src/commands.rs    thin Tauri command facade over the feature modules
  src/config.rs      TOML config load/save
  src/capabilities.rs hardware discovery and probes
  src/stats.rs       live telemetry, Rust-side view model, sparklines
  src/power.rs       power profiles, charging, thermal modes, power limits
  src/gpu.rs         GPU mode detection and switching backends
  src/memory.rs      DIMM / VRAM summary
  src/keyboard.rs    ITE RGB HID reports and keyboard commands
  src/fan.rs         fan safety, reset detection, profiles, curves
  src/fan_control.rs fan IPC adapters
  src/device.rs      device identity, battery health, dashboard score
  src/battery.rs     battery detail and charge threshold
  src/presets.rs     performance preset mapping
  src/display.rs     X11 / KDE / GNOME / sway / wlroots / Hyprland adapters
  src/diagnostics.rs health checks, smart scan, report export
  src/gpu_ext.rs     NVIDIA details, power limit, runtime PM
  src/main.rs        entry point, --discover / --selftest
  src/bin/loq-helper.rs  privileged helper (fan, GPU switch, power limits, charging,
                     thermal mode, brightness, SMART, GPU power/runtime PM,
                     memory info, cache drop)
  tauri.conf.json
  capabilities/      Tauri 2 permissions
scripts/             install.sh / uninstall.sh
polkit/ udev/  systemd/  desktop/
```

The frontend is intentionally thin: it renders state and forwards user intent. All hardware
decisions, validation, mode mapping, process filtering/limits, fan-curve checks, display
adapter selection, diagnostics, and config normalisation live in Rust (feature modules,
`commands.rs`, and `loq-helper`), so the JS never touches sysfs and cannot bypass a check.

The app never runs as root. `loq-helper` is installed root-owned and is invoked with
`sudo -n /usr/local/bin/loq-helper`; the installer adds a NOPASSWD sudoers rule for that
single binary only. All helper inputs are validated and writes are limited to whitelisted
sysfs paths.

## Build

The installer installs the required build/runtime packages for your distribution
(pacman, apt, dnf, zypper or apk) after asking for confirmation. To build manually
on Arch:

```bash
sudo pacman -S rust base-devel pkgconf webkit2gtk-4.1 libappindicator \
               openssl gtk3 librsvg
```

```bash
cargo build --release          # workspace build (both binaries)
./target/release/loq-control   # run
./target/release/loq-control --discover
```

## Install

```bash
./scripts/install.sh
```

The installer re-launches itself through `sudo`, so you are prompted for your
password once. It detects the distribution's package manager and offers to download
any missing required build/runtime packages before building.

With no arguments the installer opens an interactive menu:

1. **Update / Install** — pull the latest source (only when tracked files are clean),
   build, and install everything.
2. **Reinstall** — rebuild the current source and reinstall.
3. **Repair** — refresh sudoers, polkit, udev, the user service, and the desktop entry
   without rebuilding.
4. **Status** — show installed/source versions, helper version, service state, and
   whether each integration file is present.
5. **Uninstall** — remove the app and every file it owns: binaries, helper, sudoers,
   polkit, udev rule, service, desktop entry, icons, config, profiles, reports,
   cache and battery history.
6. **Uninstall (keep data)** — remove the app files but keep config, profiles and
   battery history.

Non-interactive commands:

```bash
./scripts/install.sh --install
./scripts/install.sh --update
./scripts/install.sh --reinstall
./scripts/install.sh --repair
./scripts/install.sh --status
./scripts/uninstall.sh                # full cleanup
./scripts/uninstall.sh --keep-data    # keep config + profiles + battery history
```

Useful flags: `--enable-service`, `--disable-service`, `--no-build`, `--yes`, and
`--no-color`. `--purge` is kept as an alias for the default full cleanup. The
interactive menu is skipped when stdin is not a terminal.

This builds the release binaries and installs:

| Item | Location |
|---|---|
| app | `/usr/local/bin/loq-control` |
| privileged helper | `/usr/local/bin/loq-helper` (root:root, 755) |
| sudoers rule | `/etc/sudoers.d/99-loq-control` |
| polkit policy | `/usr/share/polkit-1/actions/` |
| udev rule | `/etc/udev/rules.d/99-ite-keyboard.rules` |
| user service | `~/.config/systemd/user/loq-control.service` |
| desktop entry | `~/.local/share/applications/loq-control.desktop` |
| config | `~/.config/loq-control/config.toml` |
| profiles | `~/.config/loq-control/profiles.json` |
| battery history | `~/.local/share/loq-control/battery.db` |
| diagnostic reports | `~/.local/state/loq-control/` |

Autostart is disabled by default. Enable with:

```bash
systemctl --user enable --now loq-control
```

`--enable-service` does this during installation. Uninstalling removes only files
owned by LOQ Control Center; system packages, drivers and other applications are
never touched. Use `--keep-data` if you want to keep config, profiles and battery
history for a future reinstall.

## Optional integrations (not required)

These tools add features when present. The installer never requires or installs them;
the app degrades gracefully without them.

| Tool | Purpose |
|---|---|
| `nvidia-smi` | NVIDIA telemetry and power limit |
| `smartctl` (smartmontools) | storage health check via the helper |
| `powerprofilesctl` | power profile switching |
| `envycontrol`, `supergfxctl` or `prime-select` | GPU mode switching |
| `xrandr` | X11 output modes |
| `kscreen-doctor` | KDE Plasma Wayland output modes |
| `gnome-randr` | GNOME Wayland output modes |
| `gdctl` | GNOME Wayland output modes |
| `swaymsg`, `wlr-randr` | sway / wlroots output modes |
| `hyprctl` | Hyprland output modes |
| `dmidecode` | DIMM size/type/speed (helper runs it as root) |
| `lsblk` (util-linux), `ip` (iproute2) | storage and network details |

## Notes

- Fan control depends on the Lenovo WMI/hwmon driver exposing `pwmN` and `pwmN_auto_point*`.
  Many LOQ models do not expose it; the UI hides the Fan tab in that case.
- Display refresh control uses the active compositor: `xrandr`, `kscreen-doctor`,
  `gdctl`/`gnome-randr`, `swaymsg`, `wlr-randr`, or `hyprctl`. On GNOME Wayland, `gdctl`
  preserves the existing logical-monitor layout while changing one output's mode.
  `gnome-randr` is only a fallback when `gdctl` is unavailable.
- Diagnostics are read-only. SMART and privileged brightness/GPU writes go through the
  helper; read-only checks use `sudo -n` and never prompt.
- Rapid charge appears only when the EC advertises `Fast` in `charge_types`.
- Thermal modes follow `platform_profile_choices`; Windows-only Vantage features (cloud
  updates, warranty, Dolby, BIOS/MUX toggles) are out of scope.
- Disk I/O reports throughput and real busy % from `io_ticks`.
- Re-run `sudo ./scripts/install.sh --update` after updating so `/usr/local/bin/loq-helper`
  matches the app. The UI shows `helper: outdated` until the helper is refreshed.
