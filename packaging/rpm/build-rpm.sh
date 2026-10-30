#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)
ROOT=$(cd -- "$SCRIPT_DIR/../.." && pwd)
SPEC="$SCRIPT_DIR/saucefx.spec"
VERSION=$(sed -n 's/^project(SauceFX VERSION \([^ ]*\).*/\1/p' \
    "$ROOT/src/gui/CMakeLists.txt" | head -n 1)
ARCH=$(uname -m)
RELEASE_DIR="$ROOT/release"
BUILD_DIR="$RELEASE_DIR/rpm-build"
STAGE_DIR="$RELEASE_DIR/rpm-root"
RPM_TOP="$RELEASE_DIR/rpmbuild"
JUCE_URL="https://github.com/juce-framework/JUCE/archive/refs/tags/9.0.4.tar.gz"
JUCE_SHA256="e2bee25d5c776118aea550ad168a3dc9520c0669a79bd9f5a72b9f404ee1c40a"

# JUCE's large translation units can exhaust memory when every reported CPU
# is used. Keep the default reliable and allow builders to opt into more jobs.
if [ -z "${SAUCEFX_BUILD_JOBS:-}" ]; then
    SAUCEFX_BUILD_JOBS=$(nproc)
    if [ "$SAUCEFX_BUILD_JOBS" -gt 2 ]; then
        SAUCEFX_BUILD_JOBS=2
    fi
fi

if [ -z "$VERSION" ]; then
    echo "error: could not read the SauceFX version from CMakeLists.txt" >&2
    exit 1
fi
if ! command -v rpmbuild >/dev/null 2>&1; then
    echo "error: rpmbuild is not installed." >&2
    echo "On Fedora install rpm-build, then retry." >&2
    echo "A Fedora build container is also supported; see README.md." >&2
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

if [ -z "${JUCE_DIR:-}" ]; then
    for candidate in "$ROOT/../deps/JUCE" "$ROOT/.deps/JUCE"; do
        if [ -f "$candidate/CMakeLists.txt" ]; then
            JUCE_DIR="$candidate"
            break
        fi
    done
fi

if [ -z "${JUCE_DIR:-}" ] || [ ! -f "$JUCE_DIR/CMakeLists.txt" ]; then
    echo "==> Downloading JUCE 9.0.4"
    mkdir -p "$RELEASE_DIR/deps"
    if [ ! -f "$RELEASE_DIR/deps/juce-9.0.4.tar.gz" ]; then
        curl -fL --retry 3 -o "$RELEASE_DIR/deps/juce-9.0.4.tar.gz" \
            "$JUCE_URL"
    fi
    echo "$JUCE_SHA256  $RELEASE_DIR/deps/juce-9.0.4.tar.gz" \
        | sha256sum -c -
    rm -rf "$RELEASE_DIR/deps/JUCE"
    mkdir -p "$RELEASE_DIR/deps/JUCE"
    tar -xzf "$RELEASE_DIR/deps/juce-9.0.4.tar.gz" \
        --strip-components=1 -C "$RELEASE_DIR/deps/JUCE"
    JUCE_DIR="$RELEASE_DIR/deps/JUCE"
fi

echo "==> Building native app"
make -C "$ROOT/src"
rm -rf "$BUILD_DIR"
cmake -S "$ROOT/src/gui" -B "$BUILD_DIR" \
    -DCMAKE_BUILD_TYPE=Release \
    -DCMAKE_INSTALL_PREFIX=/usr \
    -DJUCE_DIR="$JUCE_DIR" \
    -DSAUCEFX_WEB_DIST_DIR="$ROOT/frontend/dist"
cmake --build "$BUILD_DIR" --parallel "$SAUCEFX_BUILD_JOBS"

echo "==> Staging install tree"
rm -rf "$STAGE_DIR"
DESTDIR="$STAGE_DIR" cmake --install "$BUILD_DIR" --component Runtime

echo "==> Building RPM"
rm -rf "$RPM_TOP"
mkdir -p "$RPM_TOP"/{BUILD,BUILDROOT,RPMS,SOURCES,SPECS,SRPMS}
rpmbuild -bb \
    --define "_topdir $RPM_TOP" \
    --define "_sourcedir $RPM_TOP/SOURCES" \
    --define "_specdir $RPM_TOP/SPECS" \
    --define "_builddir $RPM_TOP/BUILD" \
    --define "_rpmdir $RPM_TOP/RPMS" \
    --define "_srcrpmdir $RPM_TOP/SRPMS" \
    --define "_buildrootdir $RPM_TOP/BUILDROOT" \
    --define "_saucefx_root $STAGE_DIR" \
    "$SPEC"

mkdir -p "$RELEASE_DIR"
RPM_FILE=$(find "$RPM_TOP/RPMS" -type f -name '*.rpm' -print -quit)
if [ -z "$RPM_FILE" ]; then
    echo "error: rpmbuild did not produce an RPM" >&2
    exit 1
fi

# Publish a readable versioned filename plus a stable filename used by the
# GitHub /releases/latest download URL. The RPM's internal metadata still
# carries the exact Fedora release and package version.
VERSIONED_RPM="$RELEASE_DIR/SauceFX-$VERSION-$ARCH.rpm"
LATEST_RPM="$RELEASE_DIR/SauceFX-$ARCH.rpm"
install -m 0644 "$RPM_FILE" "$VERSIONED_RPM"
install -m 0644 "$RPM_FILE" "$LATEST_RPM"

echo "RPM package: $VERSIONED_RPM"
echo "Stable release asset: $LATEST_RPM"
