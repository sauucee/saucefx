#!/usr/bin/env bash
# Install a downloaded bundle, reusing the installed application's scope.
set -euo pipefail

APP_ID=io.github.sauucee.SauceFX
SCOPE=
if [ "${1:-}" = --user ] || [ "${1:-}" = --system ]; then
    SCOPE=$1
    shift
fi
if [ "$#" -ne 1 ] || [ ! -f "$1" ]; then
    echo "Usage: bash install-flatpak.sh [--user|--system] /path/to/SauceFX.flatpak" >&2
    exit 1
fi
BUNDLE=$(realpath -- "$1")

if [ -z "$SCOPE" ]; then
    USER_INSTALLED=false
    SYSTEM_INSTALLED=false
    if flatpak info --user "$APP_ID" >/dev/null 2>&1; then USER_INSTALLED=true; fi
    if flatpak info --system "$APP_ID" >/dev/null 2>&1; then SYSTEM_INSTALLED=true; fi
    if "$USER_INSTALLED" && "$SYSTEM_INSTALLED"; then
        echo "SauceFX is installed for both your user and the system. Choose --user or --system." >&2
        exit 1
    elif "$SYSTEM_INSTALLED"; then
        SCOPE=--system
    else
        SCOPE=--user
    fi
fi

exec flatpak install "$SCOPE" --bundle --or-update "$BUNDLE"
