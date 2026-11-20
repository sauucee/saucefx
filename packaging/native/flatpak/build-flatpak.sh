#!/usr/bin/env bash
# Build the native UI Flatpak bundle. Version comes from the Rust workspace;
# no frontend/JUCE steps. Mirrors the released app's builder handling.
set -euo pipefail

SCRIPT_DIR=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)
ROOT=$(cd -- "$SCRIPT_DIR/../../.." && pwd)
MANIFEST="$SCRIPT_DIR/github-bundle.yml"
VERSION=$(sed -n 's/^version = "\([^"]*\)".*/\1/p' "$ROOT/Cargo.toml" | head -n 1)
ARCH=$(uname -m)
RELEASE_DIR="$ROOT/release"
BUILD_DIR="$RELEASE_DIR/flatpak-build"
REPO_DIR="$RELEASE_DIR/flatpak-repo"
BUNDLE="$RELEASE_DIR/SauceFX-$VERSION-$ARCH.flatpak"

if [ -z "${FLATPAK_BUILDER_N_JOBS:-}" ]; then
    FLATPAK_BUILDER_N_JOBS=$(nproc)
    if [ "$FLATPAK_BUILDER_N_JOBS" -gt 2 ]; then
        FLATPAK_BUILDER_N_JOBS=2
    fi
    export FLATPAK_BUILDER_N_JOBS
fi

if [ -z "$VERSION" ]; then
    echo "error: could not read the workspace version from Cargo.toml" >&2
    exit 1
fi

if ! flatpak remotes --user --columns=name 2>/dev/null | grep -Fqx flathub; then
    flatpak remote-add --user --if-not-exists flathub \
        https://dl.flathub.org/repo/flathub.flatpakrepo
fi

# Point host SDK lookups at a per-user installation when that is where the
# SDK lives (the common rootless setup).
USER_FLATPAK_DIR="${FLATPAK_USER_DIR:-${XDG_DATA_HOME:-$HOME/.local/share}/flatpak}"
if flatpak info --user org.gnome.Sdk//51 >/dev/null 2>&1 &&
   flatpak info --user org.gnome.Platform//51 >/dev/null 2>&1 &&
   ! flatpak info --system org.gnome.Sdk//51 >/dev/null 2>&1; then
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
    exit 1
fi

mkdir -p "$RELEASE_DIR"

echo "==> Building Flatpak $VERSION for $ARCH"
"${BUILDER[@]}" \
    --jobs="$FLATPAK_BUILDER_N_JOBS" \
    --force-clean \
    --install-deps-from=flathub \
    --repo="$REPO_DIR" \
    "$BUILD_DIR" \
    "$MANIFEST"

source "$SCRIPT_DIR/repository.env"
: "${SAUCEFX_SIGNING_KEY:?prepare the signing key before building a bundle}"
: "${SAUCEFX_PUBLIC_KEY:?prepare the public signing key before building a bundle}"
flatpak build-sign --arch="$ARCH" --gpg-sign="$SAUCEFX_SIGNING_KEY" \
    --gpg-homedir="$GNUPGHOME" "$REPO_DIR" "$APP_ID" "$BRANCH"
flatpak build-update-repo --gpg-sign="$SAUCEFX_SIGNING_KEY" \
    --gpg-homedir="$GNUPGHOME" "$REPO_DIR"
echo "==> Creating signed update-enabled bundle"
flatpak build-bundle --arch="$ARCH" --repo-url="$UPDATE_REPO_URL" \
    --gpg-keys="$SAUCEFX_PUBLIC_KEY" --runtime-repo="$RUNTIME_REPO_URL" \
    "$REPO_DIR" "$BUNDLE" "$APP_ID" "$BRANCH"
python3 -I "$SCRIPT_DIR/repository-metadata.py" "$REPO_DIR" \
    "$SAUCEFX_PUBLIC_KEY" "$RELEASE_DIR/SauceFX.flatpakref"
python3 -I "$SCRIPT_DIR/verify-repository.py" "$REPO_DIR" "$SAUCEFX_PUBLIC_KEY"
tar -C "$REPO_DIR" -czf "$RELEASE_DIR/flatpak-repository.tar.gz" .
echo "Flatpak bundle: $BUNDLE"