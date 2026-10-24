#!/usr/bin/env bash
# LOQ Control Center (Rust + Tauri) — Uninstall wrapper
#
# Usage:
#   sudo ./scripts/uninstall.sh               remove the app and all program data
#   sudo ./scripts/uninstall.sh --keep-data   remove the app, keep config + history
#
# The actual uninstall logic lives in install.sh so the installer, updater and
# uninstaller share the same service, path and confirmation handling. Only files
# owned by LOQ Control Center are removed; system packages and drivers are not.

set -euo pipefail

for arg in "$@"; do
    case "$arg" in
        --uninstall|--keep-data|--purge|--yes|-y|--no-color|--help|-h|--version|-V) ;;
        *) printf 'uninstall.sh only supports --keep-data, --purge, --yes, --no-color, --help and --version\n' >&2; exit 2 ;;
    esac
done

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
exec "$SCRIPT_DIR/install.sh" --uninstall "$@"
