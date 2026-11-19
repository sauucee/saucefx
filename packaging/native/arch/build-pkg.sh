#!/usr/bin/env bash
# Build the Arch pacman package from the checked-out source. Runs in the
# archlinux:base-devel CI container (root); makepkg itself runs as an
# unprivileged user because it refuses root. Installs the result and checks
# that every dynamic library resolves.
set -euo pipefail

SCRIPT_DIR=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)
ROOT=$(cd -- "$SCRIPT_DIR/../../.." && pwd)
VERSION=$(sed -n 's/^version = "\([^"]*\)".*/\1/p' "$ROOT/Cargo.toml" | head -n 1)
if [ -z "$VERSION" ]; then
    echo "error: could not read the workspace version from Cargo.toml" >&2
    exit 1
fi

sed -i "s/^pkgver=.*/pkgver=$VERSION/" "$SCRIPT_DIR/PKGBUILD"
git config --global --add safe.directory "$ROOT" 2>/dev/null || true
git -C "$ROOT" archive --format=tar.gz --prefix="saucefx-$VERSION/" \
    -o "$SCRIPT_DIR/saucefx-$VERSION.tar.gz" HEAD

if [ "$(id -u)" = 0 ]; then
    id builder >/dev/null 2>&1 || useradd -m builder
    chown -R builder:builder "$SCRIPT_DIR"
    su builder -c "cd '$SCRIPT_DIR' && makepkg -sf --noconfirm --skipchecksums --nocheck"
else
    (cd "$SCRIPT_DIR" && makepkg -sf --noconfirm --skipchecksums --nocheck)
fi

mkdir -p "$ROOT/release"
cp "$SCRIPT_DIR"/*.pkg.tar.zst "$ROOT/release/"

echo "==> Install + link check"
pacman -U --noconfirm "$SCRIPT_DIR"/*.pkg.tar.zst
if ldd /usr/bin/SauceFX | grep 'not found'; then
    echo "error: unresolved dynamic libraries" >&2
    exit 1
fi
desktop-file-validate /usr/share/applications/io.github.sauucee.SauceFX.desktop
test -f /usr/share/saucefx/presets/Factsoft/Default.fac
echo "Arch package: $(ls "$ROOT/release"/*.pkg.tar.zst)"