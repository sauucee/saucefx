#!/usr/bin/env bash
# GitHub-only integration: use the application's real updater backend from
# inside a sandbox, with a signed local repository and no audio engine.
set -euo pipefail
if [ "${CI:-}" != true ]; then
    printf '%s\n' 'Run the updater integration in GitHub Actions.' >&2
    exit 1
fi
PROBE=$(realpath -- "${1:?pass the GitHub-built updater_probe binary}")
if [ "${2:-}" != --isolated ]; then
    exec dbus-run-session -- bash "$0" "$PROBE" --isolated
fi
WORK=$(mktemp -d "${TMPDIR:-/tmp}/saucefx-in-app-update.XXXXXX")
PIDS=()
cleanup() {
    local result=$?
    if [ "$result" -ne 0 ]; then
        for log in "$WORK"/*.log; do [ ! -f "$log" ] || cat "$log" >&2; done
    fi
    for pid in "${PIDS[@]}"; do kill "$pid" 2>/dev/null || true; done
    for pid in "${PIDS[@]}"; do wait "$pid" 2>/dev/null || true; done
    rm -rf -- "$WORK"
}
trap cleanup EXIT
export XDG_CONFIG_HOME="$WORK/config" XDG_DATA_HOME="$WORK/data"
export XDG_CACHE_HOME="$WORK/cache" XDG_STATE_HOME="$WORK/state"
export FLATPAK_USER_DIR="$WORK/user-install" FLATPAK_SYSTEM_DIR="$WORK/system"
export GNUPGHOME="$WORK/gnupg"
mkdir -p "$GNUPGHOME" "$XDG_CONFIG_HOME" "$FLATPAK_USER_DIR" "$FLATPAK_SYSTEM_DIR"
mkdir -p "$FLATPAK_SYSTEM_DIR/repo"
chmod 700 "$GNUPGHOME"
gpg --batch --pinentry-mode loopback --passphrase '' --quick-gen-key \
    'SauceFX CI update test <ci@saucefx.invalid>' rsa2048 sign 1d >"$WORK/key.log" 2>&1
KEY=$(gpg --batch --with-colons --list-secret-keys 2>/dev/null | python3 -I -c 'import sys; print(next(line.split(":")[9] for line in sys.stdin if line.startswith("fpr:")))')
gpg --batch --export "$KEY" > "$WORK/test-public.gpg"
APP_ID=io.github.sauucee.SauceFX
REF="app/$APP_ID/x86_64/master"
# This probe does not link graphics/audio libraries. The runtime is an empty
# sandbox fixture; required shared objects come from the same GitHub runner.
RUNTIME="$WORK/runtime"
mkdir -p "$RUNTIME/usr/lib/x86_64-linux-gnu" "$RUNTIME/usr/bin" "$RUNTIME/usr/lib64"
printf '[Runtime]\nname=io.github.sauucee.UpdateTestRuntime\n' > "$RUNTIME/metadata"
cp -L /lib64/ld-linux-x86-64.so.2 "$RUNTIME/usr/lib64/"
ldd "$PROBE" | python3 -I -c 'import re,sys; print("\n".join(dict.fromkeys(re.findall(r"(?:=>\s+)?(/[^\s]+)",sys.stdin.read()))))' > "$WORK/dependencies"
while IFS= read -r library; do
    [ -f "$library" ] || continue
    cp -L "$library" "$RUNTIME/usr/lib/x86_64-linux-gnu/"
done < "$WORK/dependencies"
ln -s lib64/ld-linux-x86-64.so.2 "$RUNTIME/usr/ld-linux-x86-64.so.2"
flatpak build-export --runtime --files=usr --gpg-sign="$KEY" --gpg-homedir="$GNUPGHOME" \
    "$WORK/repo" "$RUNTIME" master >"$WORK/export-runtime.log" 2>&1
APP="$WORK/app"
mkdir -p "$APP/files/bin"
cp "$PROBE" "$APP/files/bin/SauceFX"
printf '[Application]\nname=%s\nruntime=io.github.sauucee.UpdateTestRuntime/x86_64/master\ncommand=SauceFX\n' "$APP_ID" > "$APP/metadata"
printf old > "$APP/files/version-marker"
flatpak build-finish --command=SauceFX "$APP"
flatpak build-export --gpg-sign="$KEY" --gpg-homedir="$GNUPGHOME" \
    "$WORK/repo" "$APP" master >"$WORK/export-old.log" 2>&1
flatpak build-update-repo --gpg-sign="$KEY" --gpg-homedir="$GNUPGHOME" "$WORK/repo"
flatpak remote-add --user --gpg-import="$WORK/test-public.gpg" update-test "file://$WORK/repo"
flatpak install --user -y --noninteractive update-test io.github.sauucee.UpdateTestRuntime//master "$APP_ID"
OLD=$(flatpak info --user --show-commit "$APP_ID")
# The installed app remains A while the authenticated source advances to B.
printf new > "$APP/files/version-marker"
flatpak build-export --gpg-sign="$KEY" --gpg-homedir="$GNUPGHOME" \
    "$WORK/repo" "$APP" master >"$WORK/export-new.log" 2>&1
flatpak build-update-repo --gpg-sign="$KEY" --gpg-homedir="$GNUPGHOME" "$WORK/repo"
# Set permission only on this private D-Bus session to avoid interactive CI UI.
flatpak permission-set flatpak updates "$APP_ID" yes
flatpak run --no-documents-portal "$APP_ID" >"$WORK/update.log" 2>&1
NEW=$(flatpak info --user --show-commit "$APP_ID")
test "$OLD" != "$NEW"
LOCATION=$(flatpak info --user --show-location "$APP_ID")
test "$(< "$LOCATION/files/version-marker")" = new
flatpak run --no-documents-portal "$APP_ID" --expect-up-to-date >"$WORK/current.log" 2>&1
printf '%s\n' 'PASS: the native updater installs a signed update inside Flatpak and reports current afterward'
