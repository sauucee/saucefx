#!/usr/bin/env bash
# Build the DEB from the already-built release host (CI builds it with the
# same toolchain as the AppImage), then install it and verify linkage, the
# desktop entry and the factory presets.
set -euo pipefail

SCRIPT_DIR=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)
ROOT=$(cd -- "$SCRIPT_DIR/../../.." && pwd)
VERSION=$(sed -n 's/^version = "\([^"]*\)".*/\1/p' "$ROOT/Cargo.toml" | head -n 1)
if [ -z "$VERSION" ]; then
    echo "error: could not read the workspace version from Cargo.toml" >&2
    exit 1
fi
ARCH=amd64
STAGE="$ROOT/release/deb-root"
rm -rf "$STAGE"
mkdir -p "$STAGE/DEBIAN"

install -Dm0755 "$ROOT/target/release/saucefx" "$STAGE/usr/bin/SauceFX"
install -d "$STAGE/usr/share/saucefx"
cp -a "$ROOT/presets" "$STAGE/usr/share/saucefx/presets"
install -Dm0644 "$ROOT/packaging/linux/io.github.sauucee.SauceFX.desktop" \
    "$STAGE/usr/share/applications/io.github.sauucee.SauceFX.desktop"
install -Dm0644 "$ROOT/packaging/linux/io.github.sauucee.SauceFX.metainfo.xml" \
    "$STAGE/usr/share/metainfo/io.github.sauucee.SauceFX.metainfo.xml"
install -Dm0644 "$ROOT/assets/Images/saucefx-logo-512.png" \
    "$STAGE/usr/share/icons/hicolor/512x512/apps/io.github.sauucee.SauceFX.png"
install -Dm0644 "$ROOT/LICENSE" "$STAGE/usr/share/doc/saucefx/copyright"
install -Dm0644 "$ROOT/assets/Fonts/OFL.txt" "$STAGE/usr/share/doc/saucefx/Figtree-OFL.txt"

cat > "$STAGE/DEBIAN/control" <<EOF
Package: saucefx
Version: $VERSION
Section: sound
Priority: optional
Architecture: $ARCH
Depends: pipewire, pipewire-pulse, pulseaudio-utils, libpulse0, zenity, fontconfig, libfreetype6, libxkbcommon0, libgl1, libegl1, hicolor-icon-theme
Maintainer: SauceFX <sauucee@users.noreply.github.com>
Description: Sound EQ and audio enhancement for Linux (native UI)
 System-wide equalizer and audio enhancements over PipeWire, with a
 native Rust/Slint interface.
EOF

cat > "$STAGE/DEBIAN/postinst" <<'EOF'
#!/bin/sh
set -e
update-desktop-database -q /usr/share/applications || true
gtk-update-icon-cache -q /usr/share/icons/hicolor || true
EOF
chmod 0755 "$STAGE/DEBIAN/postinst"

mkdir -p "$ROOT/release"
dpkg-deb --build --root-owner-group "$STAGE" \
    "$ROOT/release/saucefx_${VERSION}_${ARCH}.deb"

echo "==> Install + link check"
sudo apt-get install -y "$ROOT/release/saucefx_${VERSION}_${ARCH}.deb"
if ldd /usr/bin/SauceFX | grep 'not found'; then
    echo "error: unresolved dynamic libraries" >&2
    exit 1
fi
desktop-file-validate /usr/share/applications/io.github.sauucee.SauceFX.desktop
test -f /usr/share/saucefx/presets/Factsoft/Default.fac
echo "DEB: $ROOT/release/saucefx_${VERSION}_${ARCH}.deb"