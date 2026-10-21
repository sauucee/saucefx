#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)
ROOT=$(cd -- "$SCRIPT_DIR/../.." && pwd)
MANIFEST="$SCRIPT_DIR/com.saucefx.SauceFX.yml"
VERSION=$(sed -n 's/^project(SauceFX VERSION \([^ ]*\).*/\1/p' \
    "$ROOT/src/gui/CMakeLists.txt" | head -n 1)
ARCH=$(uname -m)
RELEASE_DIR="$ROOT/release"
BUILD_DIR="$RELEASE_DIR/flatpak-build"
REPO_DIR="$RELEASE_DIR/flatpak-repo"
BUNDLE="$RELEASE_DIR/SauceFX-$VERSION-$ARCH.flatpak"

if [ -z "$VERSION" ]; then
    echo "error: could not read the SauceFX version from CMakeLists.txt" >&2
    exit 1
fi

if [ ! -f "$ROOT/frontend/dist/index.html" ]; then
    echo "==> Building frontend/dist"
    (
        cd "$ROOT/frontend"
        npm ci --no-audit --no-fund
        npm run build
    )
fi

if ! flatpak remotes --user --columns=name 2>/dev/null |
		grep -Fqx flathub; then
    flatpak remote-add --user --if-not-exists flathub \
        https://dl.flathub.org/repo/flathub.flatpakrepo
fi

# flatpak-builder asks the host flatpak for the SDK/runtime, which normally
# means the system installation. If the SDK/runtime were installed per-user
# (the common rootless setup), point the host lookups at the user installation.
USER_FLATPAK_DIR="${FLATPAK_USER_DIR:-${XDG_DATA_HOME:-$HOME/.local/share}/flatpak}"
if flatpak info --user org.gnome.Sdk//50 >/dev/null 2>&1 &&
   flatpak info --user org.gnome.Platform//50 >/dev/null 2>&1 &&
   ! flatpak info --system org.gnome.Sdk//50 >/dev/null 2>&1; then
    export FLATPAK_SYSTEM_DIR="$USER_FLATPAK_DIR"
fi

if command -v flatpak-builder >/dev/null 2>&1; then
    BUILDER=(flatpak-builder)
elif flatpak info org.flatpak.Builder >/dev/null 2>&1; then
    if [ -n "${FLATPAK_SYSTEM_DIR:-}" ]; then
        BUILDER=(flatpak run
            --env=FLATPAK_SYSTEM_DIR="$FLATPAK_SYSTEM_DIR"
            --command=flatpak-builder org.flatpak.Builder)
    else
        BUILDER=(flatpak run --command=flatpak-builder org.flatpak.Builder)
    fi
else
    echo "error: flatpak-builder is not installed." >&2
    echo "Install flatpak-builder or org.flatpak.Builder, then retry." >&2
    exit 1
fi

mkdir -p "$RELEASE_DIR"

echo "==> Building Flatpak $VERSION for $ARCH"
"${BUILDER[@]}" \
    --force-clean \
    --repo="$REPO_DIR" \
    "$BUILD_DIR" \
    "$MANIFEST"

echo "==> Creating bundle"
flatpak build-bundle --arch="$ARCH" \
    --runtime-repo=https://dl.flathub.org/repo/flathub.flatpakrepo \
    "$REPO_DIR" "$BUNDLE" com.saucefx.SauceFX

echo "Flatpak bundle: $BUNDLE"
