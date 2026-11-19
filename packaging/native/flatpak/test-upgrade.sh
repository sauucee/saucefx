#!/usr/bin/env bash
# Exercise the installer with real bundles in a disposable installation.
# The application and its audio engine are never launched.
set -euo pipefail
if [ "$#" -ne 2 ]; then
    echo "Usage: bash test-upgrade.sh OLD.flatpak NEW.flatpak" >&2
    exit 1
fi
OLD_BUNDLE=$(realpath -- "$1")
NEW_BUNDLE=$(realpath -- "$2")
SCRIPT_DIR=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)
REAL_FLATPAK=$(command -v flatpak)
export REAL_FLATPAK
WORK=$(mktemp -d "${TMPDIR:-/tmp}/saucefx-upgrade.XXXXXX")
trap 'rm -rf -- "$WORK"' EXIT
export XDG_DATA_HOME="$WORK/data"
export XDG_CONFIG_HOME="$WORK/config"
export XDG_CACHE_HOME="$WORK/cache"
export FLATPAK_USER_DIR="$WORK/user-install"
export FLATPAK_SYSTEM_DIR="$WORK/system-install"
export SAUCEFX_TEST_CALLS="$WORK/flatpak-commands"
mkdir -p "$WORK/bin" "$XDG_CONFIG_HOME"

# Skip runtime downloads and interactive prompts only in this test. All
# bundle import/deploy and installed-scope queries still use real Flatpak.
cat > "$WORK/bin/flatpak" <<'EOF'
#!/usr/bin/env bash
set -euo pipefail
printf '%s\n' "${1:-}" >> "$SAUCEFX_TEST_CALLS"
if [ "${1:-}" = install ]; then
    shift
    exec "$REAL_FLATPAK" install --no-deps --no-related --assumeyes --noninteractive "$@"
fi
exec "$REAL_FLATPAK" "$@"
EOF
chmod +x "$WORK/bin/flatpak"
export PATH="$WORK/bin:$PATH"
APP_ID=io.github.sauucee.SauceFX
flatpak install --user --bundle "$OLD_BUNDLE"
OLD_COMMIT=$(flatpak info --user --show-commit "$APP_ID")
OLD_REF=$(flatpak info --user --show-ref "$APP_ID")
# Mirror the host's home-relative settings/session and custom-preset paths
# in a disposable fixture; the application itself is never launched.
TEST_HOME="$WORK/home"
STATE_DIR="$TEST_HOME/.local/state/saucefx"
PRESET_DIR="$TEST_HOME/.local/share/saucefx/presets"
mkdir -p "$STATE_DIR" "$PRESET_DIR"
printf 'theme=light\nautostart=off\noutput=system\n' > "$STATE_DIR/settings.txt"
printf 'version=1\nfactory-preset=Default.fac\npower=1\n' > "$STATE_DIR/preset-session.txt"
printf 'CLASS1 : Effect Type\n9: Version\nMy preset\n' > "$PRESET_DIR/My preset.fac"
sha256sum "$STATE_DIR/settings.txt" "$STATE_DIR/preset-session.txt" \
    "$PRESET_DIR/My preset.fac" > "$WORK/user-data.sha256"

bash "$SCRIPT_DIR/install-flatpak.sh" "$NEW_BUNDLE"
NEW_COMMIT=$(flatpak info --user --show-commit "$APP_ID")
NEW_REF=$(flatpak info --user --show-ref "$APP_ID")
test "$OLD_COMMIT" != "$NEW_COMMIT"
test "$OLD_REF" = "$NEW_REF"
sha256sum --check "$WORK/user-data.sha256"
LOCATION=$(flatpak info --user --show-location "$APP_ID")
test -r "$LOCATION/files/share/saucefx/presets/Factsoft/Default.fac"
# Flatpak reports an identical bundle as already installed. Regardless of
# that command's status, the deployed commit and saved data must stay intact.
bash "$SCRIPT_DIR/install-flatpak.sh" "$NEW_BUNDLE" >"$WORK/repeat.log" 2>&1 || true
test "$(flatpak info --user --show-commit "$APP_ID")" = "$NEW_COMMIT"
sha256sum --check "$WORK/user-data.sha256"
if grep -qx uninstall "$SAUCEFX_TEST_CALLS"; then
    echo "FAIL: an upgrade must not uninstall the existing application" >&2
    exit 1
fi
echo "PASS: bundle upgrades in place and preserves user data and factory Default"
