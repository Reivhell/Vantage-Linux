<p align="center">
  <picture>
    <source media="(prefers-color-scheme: dark)" srcset="src-tauri/icons/icon.png">
    <img src="src-tauri/icons/icon.png" alt="Vantage Linux Logo" width="112" height="112">
  </picture>
</p>

<h1 align="center">Vantage Linux</h1>

<p align="center">
  <strong>A modern, lightweight hardware control center for Lenovo laptops on Linux</strong><br>
  Built with <strong>Rust</strong> and <strong>Tauri 2</strong> • Zero Node.js runtime • Native embedded web UI • Non-root privileged security model
</p>

<p align="center">
  <a href="https://github.com/Reivhell/Vantage-Linux/releases"><img src="https://img.shields.io/badge/Release-v0.1.0-blue?style=flat-square&logo=github" alt="Release Version"></a>
  <a href="https://www.rust-lang.org/"><img src="https://img.shields.io/badge/Language-Rust_2021-DEA584?style=flat-square&logo=rust&logoColor=white" alt="Rust"></a>
  <a href="https://v2.tauri.app/"><img src="https://img.shields.io/badge/Framework-Tauri_2-24C8DB?style=flat-square&logo=tauri&logoColor=white" alt="Tauri 2"></a>
  <a href="https://kernel.org/"><img src="https://img.shields.io/badge/Platform-Linux_Kernel_5.15%2B-FCC624?style=flat-square&logo=linux&logoColor=black" alt="Linux"></a>
  <a href="LICENSE"><img src="https://img.shields.io/badge/License-MIT-green?style=flat-square" alt="MIT License"></a>
  <a href="https://github.com/Reivhell/Vantage-Linux/pulls"><img src="https://img.shields.io/badge/PRs-Welcome-brightgreen?style=flat-square" alt="PRs Welcome"></a>
</p>

<p align="center">
  <a href="#-quick-start">🚀 Quick Start</a> •
  <a href="#-what-is-vantage-linux">💡 Overview</a> •
  <a href="#-key-features">✨ Features</a> •
  <a href="#-architecture--security-model">🏗️ Architecture</a> •
  <a href="#-system-integration-paths">📁 File Locations</a> •
  <a href="#-supported-hardware--integrations">🔌 Compatibility</a> •
  <a href="#-cli--telemetry">💻 CLI Reference</a> •
  <a href="#-faq--troubleshooting">❓ FAQ</a>
</p>

---

## 🚀 Quick Start

### ⚡ One-Line Quick Install (Recommended)

Install Vantage Linux directly from GitHub using `curl`. The installer automatically detects your Linux distribution, resolves dependencies, compiles the optimized binary, and configures all systemd, polkit, and udev integrations:

```bash
curl -fsSL https://raw.githubusercontent.com/Reivhell/Vantage-Linux/main/scripts/install.sh | bash
```

> [!TIP]
> Prefer downloading and inspecting first? You can run:
> ```bash
> bash -c "$(curl -fsSL https://raw.githubusercontent.com/Reivhell/Vantage-Linux/main/scripts/install.sh)"
> ```

---

### 📦 Manual Git Clone & Interactive Installer

If you prefer inspecting source code or building offline:

```bash
# 1. Clone repository
git clone https://github.com/Reivhell/Vantage-Linux.git
cd Vantage-Linux

# 2. Launch the interactive installer
./scripts/install.sh
```

The installer re-launches itself through `sudo` once and displays a terminal menu:

```text
  ╭──────────────────────────────────────────────────────────╮
  │  VANTAGE LINUX                                           │
  │  Lenovo hardware control for Linux                       │
  │  version 0.1.0                                           │
  ╰──────────────────────────────────────────────────────────╯

  1) Update / Install   Pull latest source, compile & install integrations
  2) Reinstall          Rebuild current source tree and reinstall
  3) Repair             Refresh sudoers, polkit, udev & desktop files
  4) Status             Show installed versions and file health
  5) Uninstall          Full clean removal (binaries, services, data)
  6) Uninstall (safe)   Remove binaries & services, keep configs/data
```

<details>
<summary><strong>Non-interactive CLI Flags (for scripting & automation)</strong></summary>

```bash
./scripts/install.sh --install         # Run automated non-interactive install
./scripts/install.sh --update          # Pull latest git commit and rebuild
./scripts/install.sh --reinstall       # Force clean rebuild and install
./scripts/install.sh --repair          # Reinstall system integration files without rebuild
./scripts/install.sh --status          # Check health of installed components
./scripts/install.sh --enable-service  # Enable autostart systemd user service
./scripts/uninstall.sh                 # Complete cleanup of all files and data
./scripts/uninstall.sh --keep-data     # Keep config, profiles and battery history
```
Additional flags supported: `--yes` (skip confirmation), `--no-color`, `--no-build`.
</details>

---

## 💡 What Is Vantage Linux?

**Vantage Linux** delivers the full hardware control suite Lenovo users love on Windows (like Lenovo Vantage and Legion Zone), reimagined natively for Linux. 

Unlike heavy Electron apps, Vantage Linux is engineered with **Rust** and **Tauri 2**. The user interface is pure static HTML/CSS/JS compiled straight into the binary with **zero Node.js runtime and zero npm overhead**.

### ⚖️ Why It's Different

| Feature | Standard Linux Experience | Vantage Linux |
|---|---|---|
| **Memory Footprint** | ~350MB – 800MB (Electron apps) | **< 30MB RAM** (Tauri + Rust core) |
| **Security Model** | Often runs GUI as `root` (dangerous) | **Unprivileged GUI** + audited whitelist helper |
| **Battery Conservation** | Manual `echo` commands in terminal | **GUI 80% threshold, Rapid Charge & SQLite analytics** |
| **Display Control** | Different tools per desktop environment | **Unified controller** (X11, KDE, GNOME `gdctl`, Sway, Hyprland) |
| **Keyboard RGB** | Kernel patch or third-party daemon | **Direct 33-byte ITE HID output** (no root after udev rule) |
| **GPU Switching** | Complex CLI syntax across backends | **1-click EnvyControl, supergfxctl, or prime-select** |
| **Telemetry** | Polling multiple separate CLI tools | **Integrated kernel `/proc` telemetry & sparklines** |

---

## ✨ Key Features

```
┌────────────────────────────────────────────────────────────────────────┐
│                             VANTAGE LINUX                              │
├─────────────────┬─────────────────┬──────────────────┬─────────────────┤
│ ⚡ Performance   │ 🔋 Battery Care  │ 🎮 GPU Center    │ 🖥️ Display       │
│  • Quiet/Perf   │  • Conservation │  • Hybrid / dGPU │  • Refresh rate │
│  • Auto AC/Batt │  • Rapid Charge │  • Runtime PM    │  • Multi-monitor│
│  • Power limits │  • SQLite Wear  │  • Power limits  │  • sysfs backlt │
├─────────────────┼─────────────────┼──────────────────┼─────────────────┤
│ ❄️ Fan Control   │ 🌈 4-Zone RGB   │ 🩺 Diagnostics   │ 📊 Telemetry    │
│  • Custom curves│  • Static/Breath│  • SMART Storage │  • Per-core CPU │
│  • RPM sensor   │  • Wave/Smooth  │  • Throttling    │  • Disk I/O ops │
│  • Safety guard │  • Zero-root HID│  • Markdown exp. │  • Task manager │
└─────────────────┴─────────────────┴──────────────────┴─────────────────┘
```

### ⚡ Performance & Thermal Profiles
- **Platform Profile Integration**: Maps directly to hardware ACPI thermal profiles (`quiet`, `balanced`, `performance`).
- **Smart AC/Battery Switching**: Automatically switches to Performance when plugged into AC power and transitions to Quiet when running on battery.
- **Power Limits Adjustment**: Fine-tune PL1 / PL2 / sustained package power limits on supported firmware.

### 🔋 Battery Health & Conservation Mode
- **Conservation Mode (80% Limit)**: Extends battery longevity by locking maximum charging at 80% when docked.
- **Rapid Charge Toggle**: Enables rapid charging when supported by the Embedded Controller (EC).
- **In-Depth Telemetry**: Track real-time wattage discharge, voltage, operating temperature, cycle count, and design capacity.
- **Historical Wear Tracking**: Embedded SQLite database (`battery.db`) logs battery health progression over time.

### 🎮 GPU Modes & NVIDIA Power Limits
- **Unified Switcher**: Seamlessly integrates with `envycontrol`, `supergfxctl`, or `prime-select` for switching between **Integrated**, **Hybrid**, and **Dedicated (NVIDIA)** modes.
- **Reboot Persistence**: Remembers scheduled GPU mode transitions across reboots.
- **NVIDIA Telemetry & Runtime PM**: Real-time VRAM allocation, GPU core clock, power draw, temperature, and runtime power management control.

### 🖥️ Display & Refresh Rate Management
- **Universal Wayland & X11 Support**: Works out-of-the-box on X11, KDE Plasma Wayland, GNOME Wayland (via native `gdctl` with `gnome-randr` fallback), sway/wlroots, and Hyprland.
- **Refresh Rate Ceiling**: Switch between 60Hz and high refresh rates (120Hz, 144Hz, 165Hz, 240Hz) with one click.
- **GNOME Layout Preservation**: Utilizes `gdctl` to change refresh rates without scrambling multi-monitor spatial arrangements.

### 🌈 4-Zone ITE RGB Keyboard
- **Direct HID Communication**: Sends raw 33-byte HID reports directly to `/dev/hidrawN` (VID `0x048d`).
- **No Background Daemon or Root**: Safe, instant color and effect adjustments permitted through non-root `udev` permissions.
- **Presets & Effects**: Static colors, breathing, waves, speed settings, and custom 4-zone palette mappings.

### ❄️ Fan Control & Custom Fan Curves
- **Live Fan Readout**: Real-time RPM tracking for dual-fan cooling setups.
- **PWM Overrides**: Manual speed sliders and custom temperature-to-PWM slope curves.
- **Safety Fallback**: Automatic thermal fail-safe restores firmware auto-fan mode if system temperatures exceed safety ceilings.

### 🩺 System Diagnostics & Smart Scan
- **Instant Health Audit**: Scans storage SMART status, CPU thermal throttling flags, systemd service failures, and orphan packages.
- **Exportable Markdown Reports**: Generates comprehensive diagnosis reports directly to `~/.local/state/vantage-linux/`.

### 📊 Telemetry & Process Governor
- **Lightweight System Monitor**: Live per-core CPU load, memory utilization, swap usage, disk I/O throughput (`io_ticks`), and network RX/TX.
- **Safe Process Manager**: Filter, sort, and terminate misbehaving tasks safely (refuses PID 1 and critical system tasks).

---

## 🏗️ Architecture & Security Model

Vantage Linux follows strict **least-privilege security design**. The graphical interface and Tauri application **never run as root**.

```
┌────────────────────────────────────────────────────────┐
│                   Vantage Linux UI                     │
│           (Vanilla HTML5 / CSS3 / JavaScript)          │
└───────────────────────────┬────────────────────────────┘
                            │  Tauri IPC (IPC Bridge)
┌───────────────────────────▼────────────────────────────┐
│                  Rust Backend Engine                   │
│   • Capabilities Probe  • Stats Reader  • TOML Config  │
│   • Display Adapters    • Process Guard • History DB   │
└─────────────┬────────────────────────────┬─────────────┘
              │ Unprivileged sysfs / /proc │
              │ (Sensors, Battery, Network)│
              ▼                            │
┌───────────────────────────┐              │ Sudoers Whitelist
│      Kernel Sysfs         │              │ (NOPASSWD for single binary)
│   /sys/class/power_supply │              ▼
│   /sys/devices/platform   │ ┌──────────────────────────┐
│   /dev/hidraw* (udev rule)│ │  vantage-helper (Setuid) │
└───────────────────────────┘ │  • Safe Fan Curves       │
                              │  • GPU Mode Switching    │
                              │  • Battery Thresholds    │
                              │  • SMART Disk Checks     │
                              └────────────┬─────────────┘
                                           │ Whitelisted Sysfs Writes
                                           ▼
                              ┌──────────────────────────┐
                              │ Lenovo WMI / ACPI Driver │
                              └──────────────────────────┘
```

1. **Unprivileged Frontend & Core**: The GUI and main backend run entirely under your standard user account.
2. **Audited Helper (`vantage-helper`)**: Privileged operations (writing to thermal profiles, fan PWM, GPU switching) are delegated to a dedicated Rust binary invoked via `sudo -n /usr/local/bin/vantage-helper`.
3. **Restricted Sudoers**: The installer adds `/etc/sudoers.d/99-vantage-linux`, granting passwordless elevation exclusively to `vantage-helper`.
4. **Input Validation**: `vantage-helper` verifies all arguments against strict whitelists—arbitrary commands or unauthorized file writes are impossible.

---

## 📁 System Integration Paths

When installed, Vantage Linux places files strictly in standard Linux XDG and system locations:

| Component | Path | Description |
|---|---|---|
| **Main Application** | `/usr/local/bin/vantage-linux` | Compiled release application binary |
| **Privileged Helper** | `/usr/local/bin/vantage-helper` | Audited root helper (`root:root`, 0755) |
| **Sudoers Rule** | `/etc/sudoers.d/99-vantage-linux` | NOPASSWD authorization for helper |
| **Polkit Policy** | `/usr/share/polkit-1/actions/com.github.vantagelinux.policy` | Polkit privilege definition |
| **udev Rule** | `/etc/udev/rules.d/99-ite-keyboard.rules` | User-access rule for ITE RGB HID controller |
| **Desktop Entry** | `~/.local/share/applications/vantage-linux.desktop` | Application launcher shortcut |
| **Application Icon** | `~/.local/share/icons/hicolor/256x256/apps/vantage-linux.png` | Hi-res desktop icon |
| **Systemd Service** | `~/.config/systemd/user/vantage-linux.service` | Optional background service for auto-presets |
| **Configuration** | `~/.config/vantage-linux/config.toml` | User settings, fan curves, and preset rules |
| **Custom Profiles** | `~/.config/vantage-linux/profiles.json` | Performance profile configurations |
| **Battery History** | `~/.local/share/vantage-linux/battery.db` | SQLite telemetry and health logs |
| **Diagnostic Reports**| `~/.local/state/vantage-linux/` | Exported health inspection files |

---

## 🔌 Supported Hardware & Integrations

Vantage Linux is optimized for **Lenovo LOQ, Legion, IdeaPad Gaming, and ThinkPad** series laptops. Features degrade gracefully if specific firmware interfaces are not exposed by the kernel.

### Optional Integrations Matrix
The application automatically detects and interfaces with the following system utilities when installed:

| Tool | Category | Benefit |
|---|---|---|
| `nvidia-smi` | GPU | NVIDIA power limits, clocks, temperature, and fan telemetry |
| `envycontrol` / `supergfxctl` | GPU | 1-click Hybrid / Integrated / Dedicated GPU mode switching |
| `prime-select` | GPU | Ubuntu/Debian GPU switching fallback |
| `smartctl` | Diagnostics | In-depth NVMe / SSD drive health inspection |
| `powerprofilesctl` | Power | System power-profiles-daemon interoperability |
| `gdctl` | Display | GNOME Wayland resolution and refresh-rate control without layout distortion |
| `kscreen-doctor` | Display | KDE Plasma Wayland display modes and refresh rates |
| `wlr-randr` / `swaymsg` | Display | Sway and wlroots compositor display modes |
| `hyprctl` | Display | Hyprland dynamic display management |
| `xrandr` | Display | X11 display resolution and refresh rate controls |
| `dmidecode` | Memory | Physical RAM DIMM manufacturer, clock speed, and slot topology |

---

## 💻 CLI & Telemetry

You can use Vantage Linux directly from your terminal for headless discovery, script automation, or troubleshooting:

```bash
# Print discovered laptop hardware capabilities
vantage-linux --discover

# Perform self-test and output charging, thermal, memory, display, and battery state as JSON
vantage-linux --selftest

# Enable the background user service (for auto AC/battery preset switching)
systemctl --user enable --now vantage-linux

# View service logs in real time
journalctl --user -u vantage-linux -f
```

---

## 🛠️ Manual Build & Development

If you wish to build Vantage Linux manually from source:

<details>
<summary><strong>1. Install Build Dependencies</strong></summary>

Select your distribution:

**Arch Linux / Manjaro:**
```bash
sudo pacman -S --needed rust base-devel pkgconf webkit2gtk-4.1 libappindicator-gtk3 openssl gtk3 librsvg
```

**Ubuntu / Debian / Linux Mint:**
```bash
sudo apt update && sudo apt install -y build-essential curl pkg-config \
  libwebkit2gtk-4.1-dev libappindicator3-dev libssl-dev libgtk-3-dev librsvg2-dev
```

**Fedora / RHEL:**
```bash
sudo dnf install -y gcc-c++ pkgconf-pkg-config webkit2gtk4.1-devel \
  libappindicator-gtk3-devel openssl-devel gtk3-devel librsvg2-devel
```

**openSUSE:**
```bash
sudo zypper install -y gcc-c++ pkg-config webkit2gtk3-soup2-devel \
  libappindicator3-devel libopenssl-devel gtk3-devel librsvg-devel
```
</details>

<details>
<summary><strong>2. Compile & Run</strong></summary>

```bash
# Compile both binaries in release mode
cargo build --release

# Run the compiled binary
./target/release/vantage-linux

# Test hardware detection
./target/release/vantage-linux --discover
```
</details>

---

## ❓ FAQ & Troubleshooting

<details>
<summary><strong>Why is the Fan tab missing or disabled?</strong></summary>
Fan control depends on the Lenovo ACPI/WMI driver exposing <code>pwmN</code> and <code>pwmN_auto_point*</code> interfaces in sysfs (<code>/sys/class/hwmon/</code>). Many newer LOQ models do not yet expose direct fan control in the mainline kernel. In this case, Vantage Linux hides the Fan tab to prevent system instability.
</details>

<details>
<summary><strong>How does RGB keyboard lighting work without root?</strong></summary>
The installer registers <code>/etc/udev/rules.d/99-ite-keyboard.rules</code>, which grants read/write permissions to users in the active seat for the ITE 8295 / 8291 USB HID raw device (VID <code>0x048d</code>).
</details>

<details>
<summary><strong>I previously used LOQ Control Center. Do I need to uninstall it first?</strong></summary>
No. The Vantage Linux installer automatically detects old LOQ Control Center installations, removes legacy files and services, and seamlessly migrates your existing settings, custom profiles, and battery history database to the new XDG paths.
</details>

<details>
<summary><strong>How do I completely uninstall Vantage Linux?</strong></summary>
Run <code>sudo /usr/local/bin/scripts/uninstall.sh</code> or re-run the installer and select option <code>5</code>. To remove the software while preserving your battery history and profile configuration, use <code>./scripts/uninstall.sh --keep-data</code>.
</details>

---

## 🤝 Contributing

Contributions make the open-source community a wonderful place to build and collaborate!

1. Fork the Project (`https://github.com/Reivhell/Vantage-Linux/fork`)
2. Create your Feature Branch (`git checkout -b feature/AmazingFeature`)
3. Commit your Changes (`git commit -m 'Add some AmazingFeature'`)
4. Push to the Branch (`git push origin feature/AmazingFeature`)
5. Open a Pull Request

---

## 📄 License

Distributed under the **MIT License**. See [LICENSE](LICENSE) for more information.

<p align="center">
  <sub>Built with Rust for the Linux Lenovo community • Not affiliated with Lenovo Inc.</sub>
</p>
