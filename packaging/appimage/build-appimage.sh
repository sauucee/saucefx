#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)
ROOT=$(cd -- "$SCRIPT_DIR/../.." && pwd)
VERSION=$(sed -n 's/^project(SauceFX VERSION \([^ ]*\).*/\1/p' "$ROOT/src/gui/CMakeLists.txt")
ARCH=$(uname -m)
RELEASE_DIR="$ROOT/release"
TOOLS="$RELEASE_DIR/appimage-tools"
BUILD_DIR="$RELEASE_DIR/appimage-build"
APPDIR="$RELEASE_DIR/SauceFX.AppDir"
JOBS=${SAUCEFX_BUILD_JOBS:-1}

if [ "$ARCH" != x86_64 ] || ! command -v dpkg-architecture >/dev/null; then
    echo "Build AppImages in the Ubuntu x86_64 environment used by release CI." >&2
    exit 1
fi
case "$JOBS" in 1|2) ;; *) echo "SAUCEFX_BUILD_JOBS must be 1 or 2" >&2; exit 1 ;; esac
export CMAKE_BUILD_PARALLEL_LEVEL="$JOBS"
export MAKEFLAGS="-j$JOBS"
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

fetch linuxdeploy-x86_64.AppImage \
    https://github.com/linuxdeploy/linuxdeploy/releases/download/1-alpha-20251107-1/linuxdeploy-x86_64.AppImage \
    c20cd71e3a4e3b80c3483cef793cda3f4e990aca14014d23c544ca3ce1270b4d
fetch linuxdeploy-plugin-gtk.sh \
    https://raw.githubusercontent.com/linuxdeploy/linuxdeploy-plugin-gtk/7a3fbc31a9e5075073ff8790f26effbac5f84453/linuxdeploy-plugin-gtk.sh \
    b0f4cbc684a0103a9651f0955b635eaea0096b3a66c0f5a2c2aa337960375171
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
curl -fL --retry 3 -o "$TOOLS/gtk-plugin-LICENSE" \
    https://raw.githubusercontent.com/linuxdeploy/linuxdeploy-plugin-gtk/7a3fbc31a9e5075073ff8790f26effbac5f84453/LICENSE.txt
chmod +x "$TOOLS"/*.AppImage "$TOOLS/linuxdeploy-plugin-gtk.sh"

if [ -z "${JUCE_DIR:-}" ]; then
    for candidate in "$ROOT/../deps/JUCE" "$ROOT/.deps/JUCE" "$TOOLS/JUCE"; do
        if [ -f "$candidate/CMakeLists.txt" ]; then JUCE_DIR=$candidate; break; fi
    done
fi
if [ -z "${JUCE_DIR:-}" ]; then
    fetch juce-9.0.4.tar.gz \
        https://github.com/juce-framework/JUCE/archive/refs/tags/9.0.4.tar.gz \
        e2bee25d5c776118aea550ad168a3dc9520c0669a79bd9f5a72b9f404ee1c40a
    mkdir -p "$TOOLS/JUCE"
    tar -xzf "$TOOLS/juce-9.0.4.tar.gz" --strip-components=1 -C "$TOOLS/JUCE"
    JUCE_DIR="$TOOLS/JUCE"
fi
if [ ! -f "$ROOT/frontend/dist/index.html" ]; then
    npm --prefix "$ROOT/frontend" ci --no-audit --no-fund
    npm --prefix "$ROOT/frontend" run build
fi

make -C "$ROOT/src" -j"$JOBS"
cmake -S "$ROOT/src/gui" -B "$BUILD_DIR" \
    -DCMAKE_BUILD_TYPE=Release -DCMAKE_INSTALL_PREFIX=/usr \
    -DJUCE_DIR="$JUCE_DIR" -DSAUCEFX_WEB_DIST_DIR="$ROOT/frontend/dist"
cmake --build "$BUILD_DIR" --parallel "$JOBS"

# This is exclusively generated output. Preserve any previous image until the
# new image is produced successfully.
rm -rf -- "$APPDIR"
DESTDIR="$APPDIR" cmake --install "$BUILD_DIR" --component Runtime
ln -s ../share/saucefx/web "$APPDIR/usr/bin/web"
install -Dm0644 "$JUCE_DIR/LICENSE.md" "$APPDIR/usr/share/licenses/saucefx/JUCE-LICENSE.md"
install -Dm0644 "$TOOLS/runtime-LICENSE" "$APPDIR/usr/share/licenses/saucefx/AppImage-runtime-LICENSE"
install -Dm0644 "$TOOLS/gtk-plugin-LICENSE" "$APPDIR/usr/share/licenses/saucefx/GTK-plugin-LICENSE"
install -Dm0755 /usr/bin/pactl "$APPDIR/usr/bin/pactl"

LIBDIR=$(pkg-config --variable=libdir webkit2gtk-4.1)
WEBKIT_DIR="$LIBDIR/webkit2gtk-4.1"
test -x "$WEBKIT_DIR/WebKitWebProcess"
mkdir -p "$APPDIR$WEBKIT_DIR" "$APPDIR/usr/lib" "$APPDIR/usr/share"
cp -a "$WEBKIT_DIR/." "$APPDIR$WEBKIT_DIR/"
cp -a /usr/share/pipewire "$APPDIR/usr/share/"
cp -a "$LIBDIR/pipewire-0.3" "$LIBDIR/spa-0.2" "$APPDIR/usr/lib/"
if [ -d "$LIBDIR/gio/modules" ]; then
    mkdir -p "$APPDIR/usr/lib/gio"
    cp -a "$LIBDIR/gio/modules" "$APPDIR/usr/lib/gio/"
fi

export DEPLOY_GTK_VERSION=3
"$TOOLS/linuxdeploy-x86_64.AppImage" --appdir "$APPDIR" \
    --desktop-file "$APPDIR/usr/share/applications/io.github.sauucee.SauceFX.desktop" \
    --icon-file "$APPDIR/usr/share/icons/hicolor/512x512/apps/io.github.sauucee.SauceFX.png" \
    --exclude-library 'libstdc++*' --exclude-library 'libgcc_s*' \
    --library "$LIBDIR/libwebkit2gtk-4.1.so.0" \
    --deploy-deps-only "$APPDIR/usr" --plugin gtk \
    --custom-apprun "$SCRIPT_DIR/AppRun"

# Plugins can deploy more libraries; enforce the host-runtime exclusions.
find "$APPDIR/usr/lib" -maxdepth 1 \( -name 'libstdc++*' -o -name 'libgcc_s*' \) -delete
mapfile -d '' WEBKIT_LIBS < <(find "$APPDIR/usr/lib" -type f -name 'libwebkit2gtk-4.1.so*' -print0)
test "${#WEBKIT_LIBS[@]}" -gt 0
python3 "$SCRIPT_DIR/relocate-webkit.py" "${WEBKIT_LIBS[@]}"
python3 "$SCRIPT_DIR/collect-licenses.py" "$APPDIR"

IMAGE="$RELEASE_DIR/SauceFX-$VERSION-$ARCH.AppImage"
TEMP_IMAGE="$IMAGE.part"
"$TOOLS/appimagetool-x86_64.AppImage" --runtime-file "$TOOLS/runtime-x86_64" \
    --no-appstream --comp zstd --mksquashfs-opt=-processors --mksquashfs-opt="$JOBS" \
    --mksquashfs-opt=-mem --mksquashfs-opt=256M "$APPDIR" "$TEMP_IMAGE"
mv -f -- "$TEMP_IMAGE" "$IMAGE"
chmod +x "$IMAGE"
echo "AppImage: $IMAGE"
