#!/usr/bin/env bash
# Build the RPM from the checked-out source in a Fedora CI container, then
# install it and verify linkage, the desktop entry and the factory presets.
set -euo pipefail

SCRIPT_DIR=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)
ROOT=$(cd -- "$SCRIPT_DIR/../../.." && pwd)
VERSION=$(sed -n 's/^version = "\([^"]*\)".*/\1/p' "$ROOT/Cargo.toml" | head -n 1)
if [ -z "$VERSION" ]; then
    echo "error: could not read the workspace version from Cargo.toml" >&2
    exit 1
fi

sed -i "s/^Version:.*/Version:        $VERSION/" "$SCRIPT_DIR/saucefx-native.spec"
git config --global --add safe.directory "$ROOT" 2>/dev/null || true
git -C "$ROOT" archive --format=tar.gz --prefix="saucefx-$VERSION/" \
    -o "/tmp/saucefx-$VERSION.tar.gz" HEAD

TOP=$(mktemp -d)
mkdir -p "$TOP/BUILD" "$TOP/RPMS" "$TOP/SOURCES" "$TOP/SPECS" "$TOP/SRPMS"
cp "/tmp/saucefx-$VERSION.tar.gz" "$TOP/SOURCES/"
cp "$SCRIPT_DIR/saucefx-native.spec" "$TOP/SPECS/"
rpmbuild --define "_topdir $TOP" -bb "$TOP/SPECS/saucefx-native.spec"

mkdir -p "$ROOT/release"
find "$TOP/RPMS" -name '*.rpm' -exec cp {} "$ROOT/release/" \;

echo "==> Install + link check"
dnf install -y "$ROOT/release"/saucefx-*.rpm
if ldd /usr/bin/SauceFX | grep 'not found'; then
    echo "error: unresolved dynamic libraries" >&2
    exit 1
fi
desktop-file-validate /usr/share/applications/io.github.sauucee.SauceFX.desktop
test -f /usr/share/saucefx/presets/Factsoft/Default.fac
echo "RPM: $(ls "$ROOT/release"/*.rpm)"