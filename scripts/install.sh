#!/usr/bin/env bash
# =============================================================================
# Vantage Linux Installer
# =============================================================================
# Interactive:      sudo ./scripts/install.sh
# Install:          sudo ./scripts/install.sh --install
# Update:           sudo ./scripts/install.sh --update
# Reinstall:        sudo ./scripts/install.sh --reinstall
# Repair:           sudo ./scripts/install.sh --repair
# Status:           sudo ./scripts/install.sh --status
# Uninstall:        sudo ./scripts/uninstall.sh [--keep-data]
# =============================================================================

set -euo pipefail

for arg in "$@"; do
    [[ "$arg" == "--no-color" ]] && export NO_COLOR=1
done

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]:-$0}")" 2>/dev/null && pwd || pwd)"
PROJECT_DIR="$(dirname "$SCRIPT_DIR")"
# Support install.sh next to Cargo.toml as well as inside scripts/.
if [[ ! -f "$PROJECT_DIR/Cargo.toml" && -f "$SCRIPT_DIR/Cargo.toml" ]]; then
    PROJECT_DIR="$SCRIPT_DIR"
fi

# Fallback: if executed via curl pipe or outside source tree, clone the repository
if [[ ! -f "$PROJECT_DIR/Cargo.toml" ]]; then
    echo -e "\033[1;36m==>\033[0m Source tree not found locally. Cloning Vantage Linux repository..."
    TMP_CLONE_DIR="$(mktemp -d /tmp/vantage-linux-XXXXXX)"
    if ! command -v git >/dev/null 2>&1; then
        echo -e "\033[1;31mError:\033[0m 'git' command not found. Please install git first." >&2
        exit 1
    fi
    git clone https://github.com/Reivhell/Vantage-Linux.git "$TMP_CLONE_DIR"
    exec bash "$TMP_CLONE_DIR/scripts/install.sh" "$@"
fi

APP_ICON="$PROJECT_DIR/src-tauri/icons/Icon.avif"
# Tauri embeds a PNG window icon on Unix; the desktop entry uses the AVIF source.
TAURI_ICON="$PROJECT_DIR/src-tauri/icons/icon.png"

# ---------------------------------------------------------------------------
# Shared terminal UI and privilege helpers
# ---------------------------------------------------------------------------
if [[ -t 1 && -z "${NO_COLOR:-}" ]]; then
    BOLD=$'\033[1m'
    DIM=$'\033[2m'
    RED=$'\033[0;31m'
    GREEN=$'\033[0;32m'
    YELLOW=$'\033[1;33m'
    BLUE=$'\033[0;34m'
    CYAN=$'\033[0;36m'
    MAGENTA=$'\033[0;35m'
    RESET=$'\033[0m'
else
    BOLD='' DIM='' RED='' GREEN='' YELLOW='' BLUE='' CYAN='' MAGENTA='' RESET=''
fi

ui_rule() {
    printf '%s\n' "${DIM}────────────────────────────────────────────────────────────${RESET}"
}

ui_banner() {
    local version="${SOURCE_VERSION:-?}"
    printf '\n'
    printf '%s\n' "${CYAN}${BOLD}  ╭──────────────────────────────────────────────────────────╮${RESET}"
    printf '%s%-58s%s\n' "${CYAN}${BOLD}  │${RESET}" "  VANTAGE LINUX" "${CYAN}${BOLD}│${RESET}"
    printf '%s%-58s%s\n' "${CYAN}${BOLD}  │${RESET}" "  Lenovo LOQ hardware control for Linux" "${CYAN}${BOLD}│${RESET}"
    printf '%s%-58s%s\n' "${CYAN}${BOLD}  │${RESET}" "  version $version" "${CYAN}${BOLD}│${RESET}"
    printf '%s\n' "${CYAN}${BOLD}  ╰──────────────────────────────────────────────────────────╯${RESET}"
    printf '\n'
}

ui_section() {
    printf '\n%s\n' "${BOLD}${BLUE}▸ $*${RESET}"
    ui_rule
}

ui_step() {
    local current="$1" total="$2" label="$3"
    printf '%s\n' "${DIM}  [${current}/${total}]${RESET} ${label}"
}

ui_info() {
    printf '%s\n' "  ${CYAN}→${RESET} $*"
}

ui_ok() {
    printf '%s\n' "  ${GREEN}✔${RESET} $*"
}

ui_warn() {
    printf '%s\n' "  ${YELLOW}!${RESET} $*" >&2
}

ui_err() {
    printf '%s\n' "  ${RED}✖${RESET} $*" >&2
    exit 1
}

ui_kv() {
    printf '  %s%-12s%s %s\n' "$DIM" "$1" "$RESET" "$2"
}

ui_confirm() {
    local prompt="$1" default="${2:-no}" reply
    if [[ ${ASSUME_YES:-0} -eq 1 ]]; then
        return 0
    fi
    if [[ ! -t 0 ]]; then
        return 1
    fi
    if [[ "$default" == "yes" ]]; then
        prompt="$prompt [Y/n] "
    else
        prompt="$prompt [y/N] "
    fi
    read -r -p "  ${YELLOW}?${RESET} $prompt" reply
    reply="${reply:-$default}"
    [[ "$reply" =~ ^[Yy]$ ]]
}

# Run a command while showing a small spinner. Output is captured and only
# printed when the command fails.
ui_run() {
    local label="$1"
    shift
    local log rc=0
    log="$(mktemp)"
    if [[ -t 1 ]]; then
        "$@" >"$log" 2>&1 &
        local pid=$!
        local frames=('⠋' '⠙' '⠹' '⠸' '⠼' '⠴' '⠦' '⠧' '⠇' '⠏') i=0
        while kill -0 "$pid" 2>/dev/null; do
            printf '\r  %s%s%s %s' "${CYAN}" "${frames[$((i % ${#frames[@]}))]}" "${RESET}" "$label"
            i=$((i + 1))
            sleep 0.08
        done
        wait "$pid" || rc=$?
        printf '\r\033[K'
    else
        "$@" >"$log" 2>&1 || rc=$?
    fi
    if [[ $rc -ne 0 ]]; then
        printf '\n'
        cat "$log" >&2
        rm -f "$log"
        return "$rc"
    fi
    rm -f "$log"
    ui_ok "$label"
}

require_root() {
    [[ "$(id -u)" -eq 0 ]] && return
    # Re-launch through sudo so the user is prompted once for their password.
    command -v sudo >/dev/null 2>&1 \
        || ui_err "sudo is required. Install sudo, then run: sudo $DISPLAY_INSTALL"
    [[ -t 0 ]] || ui_err "Run with sudo: sudo $DISPLAY_INSTALL"
    exec sudo -- "$0" "${ORIGINAL_ARGS[@]}"
}

# Run a privileged command. The installer normally runs as root after the
# re-launch above; the sudo fallback keeps this correct if that ever changes.
run_priv() {
    if [[ "$(id -u)" -eq 0 ]]; then
        "$@"
    else
        sudo "$@"
    fi
}

resolve_user() {
    if [[ "$(id -u)" -eq 0 ]]; then
        REAL_USER="${SUDO_USER:-}"
        [[ -n "$REAL_USER" && "$REAL_USER" != "root" ]] \
            || ui_err "Do not run this installer as root directly; use sudo from your user."
    else
        REAL_USER="${USER:-}"
    fi
    REAL_HOME="$(getent passwd "$REAL_USER" | cut -d: -f6)"
    REAL_UID="$(id -u "$REAL_USER")"
    [[ -n "$REAL_HOME" ]] || ui_err "Could not resolve home directory for $REAL_USER"
    if [[ "$(id -u)" -eq 0 ]]; then
        XDG_RUNTIME_DIR="/run/user/$REAL_UID"
    else
        XDG_RUNTIME_DIR="${XDG_RUNTIME_DIR:-/run/user/$REAL_UID}"
    fi
    export REAL_USER REAL_HOME REAL_UID XDG_RUNTIME_DIR
}

# Run a command as the desktop user, with enough session environment for
# systemctl --user, desktop database and git.
as_user() {
    if [[ "$(id -u)" -ne 0 ]]; then
        "$@"
        return
    fi
    local envs=(HOME="$REAL_HOME" USER="$REAL_USER" LOGNAME="$REAL_USER")
    if [[ -d "$XDG_RUNTIME_DIR" ]]; then
        envs+=(XDG_RUNTIME_DIR="$XDG_RUNTIME_DIR" DBUS_SESSION_BUS_ADDRESS="unix:path=$XDG_RUNTIME_DIR/bus")
    fi
    if [[ -d "$REAL_HOME/.cargo" ]]; then
        envs+=(CARGO_HOME="$REAL_HOME/.cargo")
    fi
    if [[ -d "$REAL_HOME/.rustup" ]]; then
        envs+=(RUSTUP_HOME="$REAL_HOME/.rustup")
    fi
    sudo -u "$REAL_USER" env "${envs[@]}" "$@"
}

systemctl_user() {
    if [[ -d "$XDG_RUNTIME_DIR" ]]; then
        as_user systemctl --user "$@"
    else
        return 1
    fi
}

service_active() {
    systemctl_user is-active --quiet vantage-linux.service 2>/dev/null
}

source_version() {
    sed -n 's/^version = "\(.*\)"/\1/p' "$PROJECT_DIR/src-tauri/Cargo.toml" | head -n 1
}

file_hash() {
    [[ -f "$1" ]] || return 1
    sha256sum "$1" 2>/dev/null | cut -d' ' -f1
}

detect_pkg_manager() {
    local pm
    for pm in pacman apt-get dnf zypper apk; do
        if command -v "$pm" >/dev/null 2>&1; then
            printf '%s' "$pm"
            return
        fi
    done
    printf 'unknown'
}

PKG_MANAGER="$(detect_pkg_manager)"

pc_present() {
    command -v pkg-config >/dev/null 2>&1 && pkg-config --exists "$1"
}

missing_required() {
    local missing=()
    command -v cargo >/dev/null 2>&1 || missing+=("Rust toolchain (cargo)")
    command -v cc >/dev/null 2>&1 || missing+=("C compiler")
    command -v pkg-config >/dev/null 2>&1 || missing+=("pkg-config")
    pc_present webkit2gtk-4.1 || missing+=("webkit2gtk-4.1 development files")
    pc_present gtk+-3.0 || missing+=("GTK3 development files")
    pc_present openssl || missing+=("OpenSSL development files")
    pc_present librsvg-2.0 || missing+=("librsvg development files")
    if command -v ldconfig >/dev/null 2>&1 \
        && ! ldconfig -p 2>/dev/null | grep -E 'lib(appindicator3|ayatana-appindicator3)\.so' >/dev/null; then
        missing+=("AppIndicator runtime library")
    fi
    command -v sudo >/dev/null 2>&1 || missing+=("sudo")
    command -v udevadm >/dev/null 2>&1 || missing+=("udev")
    command -v systemctl >/dev/null 2>&1 || missing+=("systemd")
    command -v visudo >/dev/null 2>&1 || missing+=("visudo")
    command -v getent >/dev/null 2>&1 || missing+=("getent")
    command -v install >/dev/null 2>&1 || missing+=("coreutils")
    if [[ ${#missing[@]} -gt 0 ]]; then
        printf '%s\n' "${missing[@]}"
    fi
}

# Build/runtime packages per distribution. Optional hardware tools (nvidia-smi,
# smartctl, GPU switching, compositor helpers) are intentionally not installed.
required_packages() {
    case "$PKG_MANAGER" in
        pacman)
            printf '%s\n' "base-devel rust pkgconf webkit2gtk-4.1 libappindicator openssl gtk3 librsvg util-linux systemd sudo"
            ;;
        apt-get)
            printf '%s\n' "build-essential cargo rustc curl wget file pkg-config libssl-dev libgtk-3-dev libwebkit2gtk-4.1-dev libayatana-appindicator3-dev librsvg2-dev libxdo-dev util-linux systemd sudo"
            ;;
        dnf)
            printf '%s\n' "gcc gcc-c++ make rust cargo curl wget file pkgconf-pkg-config openssl-devel gtk3-devel webkit2gtk4.1-devel libappindicator-gtk3-devel librsvg2-devel libxdo-devel util-linux systemd sudo"
            ;;
        zypper)
            printf '%s\n' "gcc gcc-c++ make rust cargo curl wget file pkg-config libopenssl-devel gtk3-devel webkit2gtk3-devel libappindicator3-1 librsvg-devel util-linux systemd sudo"
            ;;
        apk)
            printf '%s\n' "build-base rust cargo pkgconf openssl-dev gtk+3.0-dev webkit2gtk-dev libappindicator-dev librsvg-dev util-linux sudo"
            ;;
        *)
            return 1
            ;;
    esac
}

install_required_deps() {
    local missing packages=()
    missing="$(missing_required)"
    ui_section "Dependencies"

    if [[ -z "$missing" ]]; then
        ui_ok "Required build and runtime dependencies present"
        return
    fi

    ui_warn "Missing required dependencies:"
    printf '%s\n' "$missing" | while IFS= read -r item; do
        printf '    - %s\n' "$item"
    done

    required_packages >/dev/null \
        || ui_err "No supported package manager found; install the missing packages manually."

    read -r -a packages <<< "$(required_packages)"
    ui_info "Package manager: $PKG_MANAGER"
    ui_confirm "Download and install the required packages now?" yes \
        || ui_err "Cannot continue without the required dependencies."

    case "$PKG_MANAGER" in
        pacman) run_priv pacman -S --needed --noconfirm "${packages[@]}" ;;
        apt-get)
            run_priv apt-get update
            run_priv apt-get install -y "${packages[@]}"
            ;;
        dnf) run_priv dnf install -y "${packages[@]}" ;;
        zypper) run_priv zypper --non-interactive install "${packages[@]}" ;;
        apk) run_priv apk add "${packages[@]}" ;;
    esac || ui_err "Package installation failed."

    ui_ok "Required dependencies installed"
}

if [[ "$SCRIPT_DIR" == "$PROJECT_DIR" ]]; then
    INSTALL_REL="install.sh"
else
    INSTALL_REL="scripts/install.sh"
fi
if [[ -f "$PROJECT_DIR/uninstall.sh" ]]; then
    UNINSTALL_REL="uninstall.sh"
else
    UNINSTALL_REL="scripts/uninstall.sh"
fi
DISPLAY_INSTALL="./$INSTALL_REL"
DISPLAY_UNINSTALL="./$UNINSTALL_REL"

SOURCE_VERSION="$(source_version)"
ORIGINAL_ARGS=("$@")
ACTION=""
PULL=0
NO_BUILD=0
ASSUME_YES="${VANTAGE_ASSUME_YES:-0}"
ENABLE_SERVICE=0
DISABLE_SERVICE=0
KEEP_DATA=0
PURGE_FLAG=0

usage() {
    cat <<EOF
Vantage Linux installer ${SOURCE_VERSION}

Usage:
  $DISPLAY_INSTALL                 Interactive menu
  $DISPLAY_INSTALL --install       Build and install
  $DISPLAY_INSTALL --update        Pull latest source, build and install
  $DISPLAY_INSTALL --reinstall     Build current source and reinstall
  $DISPLAY_INSTALL --repair        Refresh system integration without building
  $DISPLAY_INSTALL --status        Show installed versions and service state
  $DISPLAY_UNINSTALL               Remove the app and all program data

The installer re-launches itself with sudo, so you are prompted for your password
once. Missing build/runtime packages are installed with the distribution's package
manager after confirmation.

Options:
  --enable-service      Enable and start the user service after installing
  --disable-service     Disable and stop the user service
  --keep-data           Keep config, profiles and battery history when uninstalling
  --purge               Remove everything (the default; kept for compatibility)
  --no-build            Use binaries already in target/release
  -y, --yes             Assume yes for confirmation prompts
  --no-color            Disable ANSI colours
  -h, --help            Show this help
  -V, --version         Show the source version

Uninstall removes only files owned by Vantage Linux. It never removes
system packages, drivers, or files belonging to other applications. Use
--keep-data to preserve your config, profiles and battery history.
EOF
}

while [[ $# -gt 0 ]]; do
    case "$1" in
        -h|--help) usage; exit 0 ;;
        -V|--version) printf 'vantage-linux %s\n' "$SOURCE_VERSION"; exit 0 ;;
        --install) ACTION=install ;;
        --update) ACTION=install; PULL=1 ;;
        --reinstall) ACTION=install ;;
        --repair) ACTION=repair ;;
        --status) ACTION=status ;;
        --uninstall) ACTION=uninstall ;;
        --purge) PURGE_FLAG=1 ;;
        --keep-data) KEEP_DATA=1 ;;
        --enable-service) ENABLE_SERVICE=1 ;;
        --disable-service) DISABLE_SERVICE=1 ;;
        --no-build) NO_BUILD=1 ;;
        -y|--yes) ASSUME_YES=1 ;;
        --no-color) : ;;
        *) usage >&2; ui_err "Unknown option: $1" ;;
    esac
    shift
done

[[ $ENABLE_SERVICE -eq 1 && $DISABLE_SERVICE -eq 1 ]] \
    && ui_err "--enable-service and --disable-service cannot be used together"
[[ $KEEP_DATA -eq 1 && $PURGE_FLAG -eq 1 ]] \
    && ui_err "--keep-data and --purge cannot be used together"
[[ ( $KEEP_DATA -eq 1 || $PURGE_FLAG -eq 1 ) && "$ACTION" != "uninstall" ]] \
    && ui_err "--keep-data and --purge are only valid with --uninstall or uninstall.sh"
[[ $NO_BUILD -eq 1 && $PULL -eq 1 ]] \
    && ui_err "--no-build cannot be combined with --update"

if [[ -z "$ACTION" ]]; then
    if [[ -t 0 && -t 1 && "$ASSUME_YES" -eq 0 ]]; then
        ACTION=menu
    else
        ACTION=install
    fi
fi

require_root
resolve_user

# ---------------------------------------------------------------------------
# Status helpers
# ---------------------------------------------------------------------------

service_state() {
    if ! command -v systemctl >/dev/null 2>&1; then
        printf 'unavailable'
        return
    fi
    local enabled active
    enabled="$(systemctl_user is-enabled vantage-linux.service 2>/dev/null || true)"
    active="$(systemctl_user is-active vantage-linux.service 2>/dev/null || true)"
    printf '%s / %s' "${enabled:-unknown}" "${active:-inactive}"
}

file_state() {
    if [[ -e "$1" ]]; then printf 'installed'; else printf 'missing'; fi
}

app_state() {
    local release="$PROJECT_DIR/target/release/vantage-linux"
    local installed='/usr/local/bin/vantage-linux'
    if [[ ! -x "$installed" ]]; then
        printf 'not installed'
        return
    fi
    if [[ ! -x "$release" ]]; then
        printf 'installed · no release build to compare'
        return
    fi
    local a b
    a="$(file_hash "$installed" || true)"
    b="$(file_hash "$release" || true)"
    if [[ -n "$a" && "$a" == "$b" ]]; then
        printf 'installed · up to date'
    else
        printf 'installed · differs from source build'
    fi
}

helper_state() {
    local installed='/usr/local/bin/vantage-helper'
    if [[ ! -x "$installed" ]]; then
        printf 'not installed'
        return
    fi
    local version
    version="$("$installed" version 2>/dev/null || printf 'unknown')"
    local release="$PROJECT_DIR/target/release/vantage-helper"
    if [[ ! -x "$release" ]]; then
        printf '%s · no release build to compare' "$version"
        return
    fi
    local a b
    a="$(file_hash "$installed" || true)"
    b="$(file_hash "$release" || true)"
    if [[ -n "$a" && "$a" == "$b" ]]; then
        printf '%s · up to date' "$version"
        return
    fi
    printf '%s · source build differs' "$version"
}

show_overview() {
    ui_kv "Project" "$PROJECT_DIR"
    ui_kv "Source" "$SOURCE_VERSION"
    ui_kv "User" "$REAL_USER"
    ui_kv "App" "$(app_state)"
    ui_kv "Helper" "$(helper_state)"
    ui_kv "Service" "$(service_state)"
}

# Remove integration owned by the pre-rebrand name. User data is migrated
# separately so existing profiles, config and battery history survive.
remove_legacy_integration() {
    ui_info "Removing legacy LOQ Control Center integration"
    systemctl_user disable --now loq-control.service 2>/dev/null || true
    rm -f /usr/local/bin/loq-control /usr/local/bin/loq-helper \
        /etc/sudoers.d/99-loq-control \
        /usr/share/polkit-1/actions/com.github.loq-control.policy \
        /usr/share/applications/loq-control.desktop
    rm -f "$REAL_HOME/.config/systemd/user/loq-control.service" \
        "$REAL_HOME/.config/systemd/user/graphical-session.target.wants/loq-control.service" \
        "$REAL_HOME/.config/systemd/user/default.target.wants/loq-control.service" \
        "$REAL_HOME/.config/autostart/loq-control.desktop" \
        "$REAL_HOME/.local/share/applications/loq-control.desktop" \
        "$REAL_HOME/.local/bin/loq-control" "$REAL_HOME/.local/bin/loq-helper"
    rm -f /usr/share/icons/hicolor/*/apps/loq-control.png
    rm -f /usr/share/icons/hicolor/*/apps/loq-control.avif
    rm -f "$REAL_HOME"/.local/share/icons/hicolor/*/apps/loq-control.png
    rm -f "$REAL_HOME"/.local/share/icons/hicolor/*/apps/loq-control.avif
    remove_owned_globs \
        "$REAL_HOME/.local/lib/python*/site-packages/loq_control" \
        "$REAL_HOME/.local/lib/python*/site-packages/loq_control-*" \
        "$REAL_HOME/.local/lib/python*/site-packages/loq_control.*.dist-info" \
        "$REAL_HOME/.local/lib/python*/site-packages/loq_control.egg-info" \
        "$REAL_HOME/.local/lib/python*/site-packages/loq_control-*.egg-info" \
        "$REAL_HOME/.local/lib/python*/site-packages/loq_helper.py"
    systemctl_user daemon-reload 2>/dev/null || true
}

migrate_legacy_user_data() {
    local pair old new
    for pair in \
        ".config/loq-control:.config/vantage-linux" \
        ".local/share/loq-control:.local/share/vantage-linux" \
        ".local/state/loq-control:.local/state/vantage-linux" \
        ".cache/loq-control:.cache/vantage-linux"; do
        old="$REAL_HOME/${pair%%:*}"
        new="$REAL_HOME/${pair##*:}"
        [[ -e "$old" ]] || continue
        if [[ -e "$new" ]]; then
            # An empty directory can be left by a failed/early new launch; it
            # must not block the migration of the real legacy data.
            if [[ -d "$new" && ! -L "$new" ]] \
                && [[ -z "$(find "$new" -mindepth 1 -maxdepth 1 -print -quit)" ]]; then
                as_user rmdir "$new"
            else
                ui_warn "Legacy data left at $old (new path already exists)"
                continue
            fi
        fi
        as_user mkdir -p "$(dirname "$new")"
        if as_user mv "$old" "$new"; then
            ui_ok "Migrated ${old#"$REAL_HOME"/} to ${new#"$REAL_HOME"/}"
        else
            ui_warn "Could not migrate $old; leaving it in place"
        fi
    done
}

# ---------------------------------------------------------------------------
# Menu
# ---------------------------------------------------------------------------

choose_action() {
    while true; do
        ui_banner
        ui_section "Control center"
        show_overview
        printf '\n'
        printf '  %s1)%s Update / Install      pull latest, build, install\n' "$CYAN" "$RESET"
        printf '  %s2)%s Reinstall             rebuild current source and reinstall\n' "$CYAN" "$RESET"
        printf '  %s3)%s Repair                refresh sudoers, udev, service, desktop\n' "$CYAN" "$RESET"
        printf '  %s4)%s Status                installed versions and service state\n' "$CYAN" "$RESET"
        printf '  %s5)%s Uninstall             remove app and all program data\n' "$CYAN" "$RESET"
        printf '  %s6)%s Uninstall (keep data) remove app, keep config and battery history\n' "$CYAN" "$RESET"
        printf '  %s0)%s Exit\n\n' "$CYAN" "$RESET"
        local choice
        read -r -p "  ${BOLD}Select${RESET} [0-6]: " choice || exit 0
        case "$choice" in
            1) ACTION=install; PULL=1; return ;;
            2) ACTION=install; return ;;
            3) ACTION=repair; return ;;
            4) ACTION=status; return ;;
            5) ACTION=uninstall; KEEP_DATA=0; return ;;
            6) ACTION=uninstall; KEEP_DATA=1; return ;;
            0|q|Q) exit 0 ;;
            *) ui_warn "Invalid choice: $choice"; sleep 1 ;;
        esac
    done
}

# ---------------------------------------------------------------------------
# Install steps
# ---------------------------------------------------------------------------

check_source() {
    local files=(
        "$PROJECT_DIR/Cargo.toml"
        "$PROJECT_DIR/src-tauri/Cargo.toml"
        "$PROJECT_DIR/udev/99-ite-keyboard.rules"
        "$PROJECT_DIR/polkit/com.github.vantagelinux.policy"
        "$PROJECT_DIR/systemd/vantage-linux.service"
        "$PROJECT_DIR/desktop/vantage-linux.desktop"
        "$APP_ICON"
        "$TAURI_ICON"
    )
    command -v getent >/dev/null 2>&1 || ui_err "getent is required (install libc)"
    command -v install >/dev/null 2>&1 || ui_err "install is required (coreutils)"
    command -v visudo >/dev/null 2>&1 || ui_err "visudo is required (install sudo)"
    local f
    for f in "${files[@]}"; do
        [[ -f "$f" ]] || ui_err "Missing installer file: $f"
    done
}

install_binaries() {
    local release="$PROJECT_DIR/target/release"
    [[ -x "$release/vantage-linux" ]] || ui_err "vantage-linux binary not found in $release"
    [[ -x "$release/vantage-helper" ]] || ui_err "vantage-helper binary not found in $release"

    ui_step 1 7 "Installing binaries to /usr/local/bin"
    rm -f /usr/local/bin/vantage-linux /usr/local/bin/vantage-helper
    install -Dm755 "$release/vantage-linux" /usr/local/bin/vantage-linux
    install -Dm755 "$release/vantage-helper" /usr/local/bin/vantage-helper
    chown root:root /usr/local/bin/vantage-linux /usr/local/bin/vantage-helper
    ui_ok "Binaries installed"
}

install_sudoers() {
    ui_step 2 7 "Configuring the privileged helper"
    local rule=/etc/sudoers.d/99-vantage-linux
    cat > "$rule" <<EOF
$REAL_USER ALL=(ALL) NOPASSWD: /usr/local/bin/vantage-helper
EOF
    chmod 0440 "$rule"
    if ! visudo -cf "$rule" >/dev/null 2>&1; then
        rm -f "$rule"
        ui_err "Invalid sudoers rule; removed it"
    fi
    ui_ok "Sudoers rule valid"
}

install_polkit() {
    local dir=/usr/share/polkit-1/actions
    if [[ -d "$dir" ]]; then
        install -Dm644 "$PROJECT_DIR/polkit/com.github.vantagelinux.policy" \
            "$dir/com.github.vantagelinux.policy"
        ui_ok "Polkit policy installed"
    else
        ui_warn "Polkit actions directory not found; skipping"
    fi
}

install_udev() {
    ui_step 3 7 "Installing keyboard udev rule"
    install -Dm644 "$PROJECT_DIR/udev/99-ite-keyboard.rules" \
        /etc/udev/rules.d/99-ite-keyboard.rules
    udevadm control --reload-rules 2>/dev/null || true
    udevadm trigger 2>/dev/null || true
    ui_ok "udev rule installed"
}

install_service() {
    ui_step 4 7 "Installing systemd user service"
    local dir="$REAL_HOME/.config/systemd/user"
    as_user mkdir -p "$dir"
    install -Dm644 "$PROJECT_DIR/systemd/vantage-linux.service" "$dir/vantage-linux.service"
    chown "$REAL_USER:" "$dir/vantage-linux.service"
    systemctl_user daemon-reload 2>/dev/null || true
    ui_ok "User service installed (autostart stays opt-in)"
}

install_desktop() {
    ui_step 5 7 "Installing desktop entry and icon"
    as_user mkdir -p "$REAL_HOME/.local/share/applications"
    as_user mkdir -p "$REAL_HOME/.local/share/icons/hicolor/256x256/apps"
    install -Dm644 "$PROJECT_DIR/desktop/vantage-linux.desktop" \
        "$REAL_HOME/.local/share/applications/vantage-linux.desktop"
    # Icon theme lookup wants a .png name; GdkPixbuf decodes the AVIF contents.
    install -Dm644 "$APP_ICON" \
        "$REAL_HOME/.local/share/icons/hicolor/256x256/apps/vantage-linux.png"
    install -Dm644 "$APP_ICON" \
        /usr/share/icons/hicolor/256x256/apps/vantage-linux.png
    chown "$REAL_USER:" \
        "$REAL_HOME/.local/share/applications/vantage-linux.desktop" \
        "$REAL_HOME/.local/share/icons/hicolor/256x256/apps/vantage-linux.png"
    as_user update-desktop-database "$REAL_HOME/.local/share/applications" 2>/dev/null || true
    as_user gtk-update-icon-cache -f "$REAL_HOME/.local/share/icons/hicolor" 2>/dev/null || true
    gtk-update-icon-cache -f /usr/share/icons/hicolor 2>/dev/null || true
    ui_ok "Desktop entry and icon installed"
}

install_config() {
    ui_step 6 7 "Preparing user configuration"
    as_user mkdir -p "$REAL_HOME/.config/vantage-linux"
    ui_ok "Config directory ready (existing config preserved)"
}

git_update() {
    ui_section "Source update"
    if [[ ! -d "$PROJECT_DIR/.git" ]]; then
        ui_warn "Not a git checkout; rebuilding the current source."
        return
    fi
    if ! command -v git >/dev/null 2>&1; then
        ui_warn "git not found; rebuilding the current source."
        return
    fi
    local tracked
    tracked="$(as_user git -C "$PROJECT_DIR" status --porcelain --untracked-files=no || true)"
    if [[ -n "$tracked" ]]; then
        ui_warn "Tracked local changes detected; skipping git pull."
        ui_info "Rebuilding the current checkout."
        return
    fi
    ui_info "Pulling latest changes from origin…"
    if as_user git -C "$PROJECT_DIR" pull --ff-only; then
        SOURCE_VERSION="$(source_version)"
        ui_ok "Source updated to $SOURCE_VERSION"
    else
        ui_warn "git pull failed; continuing with the current checkout."
    fi
}

build_release() {
    local cargo_bin
    if [[ -x "$REAL_HOME/.cargo/bin/cargo" ]]; then
        cargo_bin="$REAL_HOME/.cargo/bin/cargo"
    else
        cargo_bin="$(command -v cargo || true)"
    fi
    [[ -n "$cargo_bin" ]] || ui_err "cargo not found. Install the Rust toolchain (rustup) first."

    ui_section "Build"
    ui_info "This can take a while on the first build."
    ui_run "Building release binaries" \
        as_user "$cargo_bin" build --release \
        --manifest-path "$PROJECT_DIR/Cargo.toml"
}

restart_service_if_needed() {
    local was_active="$1"
    if [[ "$DISABLE_SERVICE" -eq 1 ]]; then
        ui_info "Disabling user service"
        systemctl_user disable --now vantage-linux.service 2>/dev/null \
            || ui_warn "Could not disable the user service (no graphical session?)"
    elif [[ "$was_active" -eq 1 || "$ENABLE_SERVICE" -eq 1 ]]; then
        ui_info "Starting user service"
        if systemctl_user enable --now vantage-linux.service 2>/dev/null; then
            ui_ok "User service enabled and running"
        else
            ui_warn "Could not start the user service; start it from your desktop session"
        fi
    fi
}

print_summary() {
    local title="$1"
    echo
    ui_rule
    printf '%s\n' "  ${GREEN}${BOLD}✔ ${title}${RESET}"
    ui_rule
    ui_kv "Start now" "vantage-linux"
    ui_kv "Discovery" "vantage-linux --discover"
    ui_kv "Start at login" "systemctl --user enable --now vantage-linux"
    ui_kv "Logs" "journalctl --user -u vantage-linux -f"
    ui_kv "Status" "sudo $DISPLAY_INSTALL --status"
    echo
}

do_install() {
    ui_banner
    ui_section "Update / Install"
    show_overview

    if [[ "$PULL" -eq 1 ]]; then
        git_update
    fi

    if [[ "$NO_BUILD" -eq 1 ]]; then
        ui_section "Build"
        ui_info "Skipping build (--no-build)"
    else
        install_required_deps
    fi

    check_source

    if [[ "$NO_BUILD" -eq 0 ]]; then
        build_release
    fi

    local release="$PROJECT_DIR/target/release"
    [[ -x "$release/vantage-linux" ]] || ui_err "vantage-linux binary not found in $release (run --reinstall)"
    [[ -x "$release/vantage-helper" ]] || ui_err "vantage-helper binary not found in $release (run --reinstall)"

    ui_section "Legacy migration"
    remove_legacy_integration
    migrate_legacy_user_data

    local service_was_active=0
    if service_active; then
        service_was_active=1
        ui_info "Stopping the running user service"
        systemctl_user stop vantage-linux.service 2>/dev/null || true
    fi

    ui_section "System integration"
    install_binaries
    install_sudoers
    install_polkit
    install_udev
    install_service
    install_desktop
    install_config
    ui_step 7 7 "Finalising"
    restart_service_if_needed "$service_was_active"
    ui_ok "System integration complete"

    print_summary "Vantage Linux $SOURCE_VERSION installed"
}

do_repair() {
    require_root
    resolve_user
    check_source
    ui_banner
    ui_section "Repair"
    show_overview

    local service_was_active=0
    if service_active; then
        service_was_active=1
        ui_info "Stopping the running user service"
        systemctl_user stop vantage-linux.service 2>/dev/null || true
    fi

    ui_section "Refreshing system integration"
    local release="$PROJECT_DIR/target/release"
    if [[ -x "$release/vantage-linux" && -x "$release/vantage-helper" ]]; then
        install_binaries
    elif [[ -x /usr/local/bin/vantage-linux && -x /usr/local/bin/vantage-helper ]]; then
        ui_warn "No release binaries found; keeping the installed binaries."
    else
        ui_err "No binaries found; run --reinstall to build them first"
    fi

    ui_section "Legacy migration"
    remove_legacy_integration
    migrate_legacy_user_data
    install_sudoers
    install_polkit
    install_udev
    install_service
    install_desktop
    install_config
    restart_service_if_needed "$service_was_active"

    print_summary "Repair complete"
}

do_status() {
    require_root
    resolve_user
    ui_banner
    ui_section "Installation status"
    ui_kv "Project" "$PROJECT_DIR"
    ui_kv "Source" "$SOURCE_VERSION"
    ui_kv "User" "$REAL_USER"
    ui_kv "App" "$(app_state)"
    ui_kv "Helper" "$(helper_state)"
    ui_kv "Service" "$(service_state)"
    ui_kv "Sudoers" "$(file_state /etc/sudoers.d/99-vantage-linux)"
    ui_kv "Polkit" "$(file_state /usr/share/polkit-1/actions/com.github.vantagelinux.policy)"
    ui_kv "udev" "$(file_state /etc/udev/rules.d/99-ite-keyboard.rules)"
    ui_kv "User unit" "$(file_state "$REAL_HOME/.config/systemd/user/vantage-linux.service")"
    ui_kv "Desktop" "$(file_state "$REAL_HOME/.local/share/applications/vantage-linux.desktop")"
    ui_kv "Icon" "$(file_state "$REAL_HOME/.local/share/icons/hicolor/256x256/apps/vantage-linux.png")"
    ui_kv "Config" "$(file_state "$REAL_HOME/.config/vantage-linux/config.toml")"
    ui_kv "Profiles" "$(file_state "$REAL_HOME/.config/vantage-linux/profiles.json")"
    ui_kv "Reports" "$(file_state "$REAL_HOME/.local/state/vantage-linux")"
    if [[ -f "$REAL_HOME/.local/share/vantage-linux/battery.db" ]]; then
        ui_kv "Battery DB" "$(du -h "$REAL_HOME/.local/share/vantage-linux/battery.db" | cut -f1) · $REAL_HOME/.local/share/vantage-linux/battery.db"
    else
        ui_kv "Battery DB" "no samples yet"
    fi
    if [[ -e /usr/local/bin/loq-control || -e /usr/local/bin/loq-helper \
        || -e "$REAL_HOME/.config/loq-control" || -e "$REAL_HOME/.local/share/loq-control" ]]; then
        ui_kv "Legacy" "found; run --install to migrate"
    fi
    echo
}

REMOVED_COUNT=0

# Remove exact paths that belong to Vantage Linux.
remove_owned() {
    local path
    for path in "$@"; do
        if [[ -e "$path" || -h "$path" ]]; then
            rm -rf -- "$path"
            REMOVED_COUNT=$((REMOVED_COUNT + 1))
        fi
    done
}

# Remove glob patterns, e.g. ~/.local/lib/python*/site-packages/vantage_linux.
# compgen only returns existing matches, so unrelated files are never touched.
remove_owned_globs() {
    local pattern p
    for pattern in "$@"; do
        while IFS= read -r p; do
            [[ -n "$p" ]] || continue
            rm -rf -- "$p"
            REMOVED_COUNT=$((REMOVED_COUNT + 1))
        done < <(compgen -G "$pattern" || true)
    done
}

do_uninstall() {
    require_root
    resolve_user
    ui_banner
    ui_section "Uninstall"

    if [[ "$KEEP_DATA" -eq 1 ]]; then
        ui_confirm "Remove Vantage Linux and its system integration? Config, profiles and battery history stay." no \
            || { ui_info "Cancelled."; return; }
    else
        ui_warn "This removes the app and all of its own files, including config, profiles, reports and battery history."
        ui_confirm "Remove everything owned by Vantage Linux?" no \
            || { ui_info "Cancelled."; return; }
    fi

    ui_info "Stopping and disabling user services"
    systemctl_user disable --now vantage-linux.service 2>/dev/null || true
    systemctl_user disable --now loq-control.service 2>/dev/null || true

    ui_section "Removing program files"
    REMOVED_COUNT=0

    # System-wide integration owned by Vantage Linux.
    remove_owned \
        /usr/local/bin/vantage-linux \
        /usr/local/bin/vantage-helper \
        /usr/local/bin/loq-control \
        /usr/local/bin/loq-helper \
        /etc/sudoers.d/99-vantage-linux \
        /etc/sudoers.d/99-loq-control \
        /usr/share/polkit-1/actions/com.github.vantagelinux.policy \
        /usr/share/polkit-1/actions/com.github.loq-control.policy \
        /etc/udev/rules.d/99-ite-keyboard.rules \
        /usr/share/applications/vantage-linux.desktop \
        /usr/share/applications/loq-control.desktop

    # Per-user integration, icons and old Python entry points.
    remove_owned \
        "$REAL_HOME/.config/systemd/user/vantage-linux.service" \
        "$REAL_HOME/.config/systemd/user/graphical-session.target.wants/vantage-linux.service" \
        "$REAL_HOME/.config/systemd/user/default.target.wants/vantage-linux.service" \
        "$REAL_HOME/.config/systemd/user/loq-control.service" \
        "$REAL_HOME/.config/systemd/user/graphical-session.target.wants/loq-control.service" \
        "$REAL_HOME/.config/systemd/user/default.target.wants/loq-control.service" \
        "$REAL_HOME/.config/autostart/vantage-linux.desktop" \
        "$REAL_HOME/.config/autostart/loq-control.desktop" \
        "$REAL_HOME/.local/share/applications/vantage-linux.desktop" \
        "$REAL_HOME/.local/share/applications/loq-control.desktop" \
        "$REAL_HOME/.local/bin/vantage-linux" \
        "$REAL_HOME/.local/bin/vantage-helper" \
        "$REAL_HOME/.local/bin/loq-control" \
        "$REAL_HOME/.local/bin/loq-helper"

    remove_owned_globs \
        "/usr/share/icons/hicolor/*/apps/vantage-linux.png" \
        "/usr/share/icons/hicolor/*/apps/vantage-linux.avif" \
        "/usr/share/icons/hicolor/*/apps/loq-control.png" \
        "/usr/share/icons/hicolor/*/apps/loq-control.avif" \
        "$REAL_HOME/.local/share/icons/hicolor/*/apps/vantage-linux.png" \
        "$REAL_HOME/.local/share/icons/hicolor/*/apps/vantage-linux.avif" \
        "$REAL_HOME/.local/share/icons/hicolor/*/apps/loq-control.png" \
        "$REAL_HOME/.local/share/icons/hicolor/*/apps/loq-control.avif" \
        "$REAL_HOME/.local/lib/python*/site-packages/vantage_linux" \
        "$REAL_HOME/.local/lib/python*/site-packages/vantage_linux-*" \
        "$REAL_HOME/.local/lib/python*/site-packages/vantage_linux.*.dist-info" \
        "$REAL_HOME/.local/lib/python*/site-packages/vantage_linux.egg-info" \
        "$REAL_HOME/.local/lib/python*/site-packages/vantage_linux-*.egg-info" \
        "$REAL_HOME/.local/lib/python*/site-packages/loq_control" \
        "$REAL_HOME/.local/lib/python*/site-packages/loq_control-*" \
        "$REAL_HOME/.local/lib/python*/site-packages/loq_control.*.dist-info" \
        "$REAL_HOME/.local/lib/python*/site-packages/loq_control.egg-info" \
        "$REAL_HOME/.local/lib/python*/site-packages/loq_control-*.egg-info" \
        "$REAL_HOME/.local/lib/python*/site-packages/loq_helper.py"

    if [[ "$KEEP_DATA" -eq 1 ]]; then
        remove_owned "$REAL_HOME/.cache/vantage-linux" "$REAL_HOME/.cache/loq-control"
    else
        remove_owned \
            "$REAL_HOME/.config/vantage-linux" \
            "$REAL_HOME/.config/loq-control" \
            "$REAL_HOME/.local/share/vantage-linux" \
            "$REAL_HOME/.local/share/loq-control" \
            "$REAL_HOME/.local/state/vantage-linux" \
            "$REAL_HOME/.local/state/loq-control" \
            "$REAL_HOME/.cache/vantage-linux" \
            "$REAL_HOME/.cache/loq-control"
    fi

    systemctl_user daemon-reload 2>/dev/null || true
    udevadm control --reload-rules 2>/dev/null || true
    as_user update-desktop-database "$REAL_HOME/.local/share/applications" 2>/dev/null || true
    as_user gtk-update-icon-cache -f "$REAL_HOME/.local/share/icons/hicolor" 2>/dev/null || true
    gtk-update-icon-cache -f /usr/share/icons/hicolor 2>/dev/null || true
    ui_ok "$REMOVED_COUNT program file(s) removed"

    if [[ "$KEEP_DATA" -eq 1 ]]; then
        ui_ok "Config kept at $REAL_HOME/.config/vantage-linux"
        ui_ok "Battery history kept at $REAL_HOME/.local/share/vantage-linux/battery.db"
    else
        ui_ok "Config, profiles, reports and battery history removed"
    fi

    echo
    ui_rule
    printf '%s\n' "  ${GREEN}${BOLD}✔ Uninstall complete${RESET}"
    printf '%s\n' "  ${DIM}System packages, drivers and other applications were not touched.${RESET}"
    ui_rule
    echo
}

# ---------------------------------------------------------------------------
# Dispatch
# ---------------------------------------------------------------------------

if [[ "$ACTION" == "menu" ]]; then
    choose_action
fi

case "$ACTION" in
    install) do_install ;;
    repair) do_repair ;;
    status) do_status ;;
    uninstall) do_uninstall ;;
    *) ui_err "No action selected" ;;
esac
