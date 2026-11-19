#!/usr/bin/env bash
# Build the Rust/Slint native SauceFX AppImage. Run on Ubuntu x86_64 (the
# release CI environment); the glibc 2.35 base keeps the image runnable on
# every distro from Ubuntu 22.04 / Debian 12 / Fedora 36 onward.
#
# Unlike the JUCE/WebKit image there is no browser sandbox, GTK plugin or
# frontend build: the host is one Slint binary over the DfxDsp archives.
# PipeWire's client library and its version-coupled SPA/module plugins are
# bundled together (never mix bundled modules with a host library), and the
# host's GPU drivers and C library stay in use.
set -euo pipefail

SCRIPT_DIR=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)
ROOT=$(cd -- "$SCRIPT_DIR/../../.." && pwd)
VERSION=$(sed -n 's/^version = "\([^"]*\)".*/\1/p' "$ROOT/Cargo.toml" | head -n 1)
ARCH=$(uname -m)
RELEASE_DIR="$ROOT/release"
TOOLS="$RELEASE_DIR/appimage-tools"
APPDIR="$RELEASE_DIR/SauceFX-native.AppDir"
JOBS=${SAUCEFX_BUILD_JOBS:-2}

if [ "$ARCH" != x86_64 ] || ! command -v dpkg-architecture >/dev/null; then
    echo "Build AppImages in the Ubuntu x86_64 environment used by release CI." >&2
    exit 1
fi
case "$JOBS" in 1|2|4) ;; *) echo "SAUCEFX_BUILD_JOBS must be 1, 2 or 4" >&2; exit 1 ;; esac
if [ -z "$VERSION" ]; then
    echo "error: could not read the workspace version from Cargo.toml" >&2
    exit 1
fi
export APPIMAGE_EXTRACT_AND_RUN=1
export ARCH VERSION
mkdir -p "$TOOLS"

fetch() {
    local name=$1 url=$2 hash=$3
    if [ ! -f "$TOOLS/$name" ]; then
        curl -fL --retry 3 -o "$TOOLS/$name" "$url"
    fi
    printf '%s  %s\n' "$hash" "$TOOLS/$name" | sha256sum -c -
}

# Same pinned tools as the JUCE image (no GTK plugin: no GTK in the app).
fetch linuxdeploy-x86_64.AppImage \
    https://github.com/linuxdeploy/linuxdeploy/releases/download/1-alpha-20251107-1/linuxdeploy-x86_64.AppImage \
    c20cd71e3a4e3b80c3483cef793cda3f4e990aca14014d23c544ca3ce1270b4d
fetch appimagetool-x86_64.AppImage \
    https://github.com/AppImage/appimagetool/releases/download/1.9.1/appimagetool-x86_64.AppImage \
    ed4ce84f0d9caff66f50bcca6ff6f35aae54ce8135408b3fa33abfc3cb384eb0

# Pin the runtime asset, not the moving continuous release tag.
if [ ! -f "$TOOLS/runtime-x86_64" ]; then
    curl -fL --retry 3 -H 'Accept: application/octet-stream' \
        -o "$TOOLS/runtime-x86_64" \
        https://api.github.com/repos/AppImage/type2-runtime/releases/assets/657374641
fi
printf '%s  %s\n' 617aa77314d57c8b8e4fae9c3f0b35af7b30d5529cb69e98f64f92510ed98a46 \
    "$TOOLS/runtime-x86_64" | sha256sum -c -
curl -fL --retry 3 -o "$TOOLS/runtime-LICENSE" \
    https://raw.githubusercontent.com/AppImage/type2-runtime/fedfad51acc2860b2916a1eef32ba4a500ff31ef/LICENSE
chmod +x "$TOOLS"/*.AppImage

echo "==> Building the DfxDsp + fxpw archives"
make -C "$ROOT/src" rt -j"$JOBS"

echo "==> Building the native host (Skia renderer)"
(
    cd "$ROOT"
    CARGO_BUILD_JOBS="$JOBS" cargo build --release -p saucefx-host \
        --no-default-features --features renderer-skia
)

# This is exclusively generated output. Preserve any previous image until the
# new image is produced successfully.
rm -rf -- "$APPDIR"
mkdir -p "$APPDIR/usr/bin"
install -Dm0755 "$ROOT/target/release/saucefx" "$APPDIR/usr/bin/SauceFX"
install -Dm0755 /usr/bin/pactl "$APPDIR/usr/bin/pactl"
install -d "$APPDIR/usr/share/saucefx"
cp -a "$ROOT/presets" "$APPDIR/usr/share/saucefx/presets"
install -Dm0644 "$ROOT/packaging/linux/io.github.sauucee.SauceFX.desktop" \
    "$APPDIR/usr/share/applications/io.github.sauucee.SauceFX.desktop"
install -Dm0644 "$ROOT/packaging/linux/io.github.sauucee.SauceFX.metainfo.xml" \
    "$APPDIR/usr/share/metainfo/io.github.sauucee.SauceFX.metainfo.xml"
install -Dm0644 "$ROOT/assets/Images/saucefx-logo-512.png" \
    "$APPDIR/usr/share/icons/hicolor/512x512/apps/io.github.sauucee.SauceFX.png"
if command -v convert >/dev/null; then
    install -d "$APPDIR/usr/share/icons/hicolor/128x128/apps"
    convert "$ROOT/assets/Images/saucefx-logo-512.png" -resize 128x128 \
        "$APPDIR/usr/share/icons/hicolor/128x128/apps/io.github.sauucee.SauceFX.png"
fi
# resolve_launch looks for the autostart icon beside the executable.
install -Dm0644 "$ROOT/assets/Images/saucefx-logo-512.png" \
    "$APPDIR/usr/bin/images/saucefx-logo-512.png"
install -Dm0644 "$ROOT/LICENSE" "$APPDIR/usr/share/licenses/saucefx/LICENSE"
install -Dm0644 "$TOOLS/runtime-LICENSE" "$APPDIR/usr/share/licenses/saucefx/AppImage-runtime-LICENSE"

# PipeWire client library + its version-coupled plugins and client config.
LIBDIR=$(pkg-config --variable=libdir libpipewire-0.3)
mkdir -p "$APPDIR/usr/lib" "$APPDIR/usr/share"
cp -a /usr/share/pipewire "$APPDIR/usr/share/"
cp -a "$LIBDIR/pipewire-0.3" "$LIBDIR/spa-0.2" "$APPDIR/usr/lib/"

echo "==> Deploying dependencies"
"$TOOLS/linuxdeploy-x86_64.AppImage" --appdir "$APPDIR" \
    --desktop-file "$APPDIR/usr/share/applications/io.github.sauucee.SauceFX.desktop" \
    --icon-file "$APPDIR/usr/share/icons/hicolor/512x512/apps/io.github.sauucee.SauceFX.png" \
    --exclude-library 'libstdc++*' --exclude-library 'libgcc_s*' \
    --exclude-library 'libGL*' --exclude-library 'libEGL*' \
    --exclude-library 'libGLES*' --exclude-library 'libgbm*' \
    --exclude-library 'libdrm*' --exclude-library 'libwayland-egl*' \
    --deploy-deps-only "$APPDIR/usr" --custom-apprun "$SCRIPT_DIR/AppRun"

# Plugins can deploy more libraries; enforce the host-runtime exclusions.
find "$APPDIR/usr/lib" -maxdepth 1 \
    \( -name 'libstdc++*' -o -name 'libgcc_s*' -o -name 'libGL*' \
       -o -name 'libEGL*' -o -name 'libGLES*' -o -name 'libgbm*' \
       -o -name 'libdrm*' -o -name 'libwayland-egl*' \) -delete
# linuxdeploy's default exclusion list omits PipeWire itself. Its private
# modules are version-coupled, so never mix bundled modules with a host library.
PW_LIBDIR=$(pkg-config --variable=libdir libpipewire-0.3)
install -Dm0644 "$(readlink -f "$PW_LIBDIR/libpipewire-0.3.so.0")" \
    "$APPDIR/usr/lib/libpipewire-0.3.so.0"
python3 "$SCRIPT_DIR/../../appimage/collect-licenses.py" "$APPDIR"

IMAGE="$RELEASE_DIR/SauceFX-$VERSION-$ARCH.AppImage"
TEMP_IMAGE="$IMAGE.part"
echo "==> Packing $IMAGE"
"$TOOLS/appimagetool-x86_64.AppImage" --runtime-file "$TOOLS/runtime-x86_64" \
    --no-appstream --comp zstd --mksquashfs-opt=-processors --mksquashfs-opt="$JOBS" \
    --mksquashfs-opt=-mem --mksquashfs-opt=256M "$APPDIR" "$TEMP_IMAGE"
mv -f -- "$TEMP_IMAGE" "$IMAGE"
chmod +x "$IMAGE"
echo "AppImage: $IMAGE"