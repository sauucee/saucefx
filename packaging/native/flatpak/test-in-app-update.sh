#!/usr/bin/env bash
# GitHub-only integration: use the application's real updater backend from
# inside a sandbox, with a signed local repository and no audio engine.
set -euo pipefail
if [ "${GITHUB_ACTIONS:-}" != true ] || [ "${CI:-}" != true ]; then
    printf '%s\n' 'Run the updater integration in GitHub Actions.' >&2
    exit 1
fi
PROBE=$(realpath -- "${1:?pass the GitHub-built updater_probe binary}")
if [ "${2:-}" != --isolated ]; then
    exec dbus-run-session -- bash "$0" "$PROBE" --isolated
fi
WORK=$(mktemp -d "${TMPDIR:-/tmp}/saucefx-in-app-update.XXXXXX")
STAGED_PID=
cleanup() {
    local result=$?
    if [ -n "$STAGED_PID" ]; then
        kill "$STAGED_PID" 2>/dev/null || true
        wait "$STAGED_PID" 2>/dev/null || true
    fi
    if [ "$result" -ne 0 ]; then
        for log in "$WORK"/*.log; do [ ! -f "$log" ] || cat "$log" >&2; done
    fi
    # The document portal can mount a FUSE filesystem under this private
    # runtime; unmount it before cleaning the generated test directory.
    if mountpoint -q "$WORK/run/doc"; then fusermount -u "$WORK/run/doc" || true; fi
    rm -rf -- "$WORK"
}
trap cleanup EXIT
export XDG_CONFIG_HOME="$WORK/config" XDG_DATA_HOME="$WORK/data"
export XDG_CACHE_HOME="$WORK/cache" XDG_STATE_HOME="$WORK/state"
export FLATPAK_USER_DIR="$WORK/user-install" FLATPAK_SYSTEM_DIR="$WORK/system"
export GNUPGHOME="$WORK/gnupg"
export XDG_RUNTIME_DIR="$WORK/run"
mkdir -p "$GNUPGHOME" "$XDG_CONFIG_HOME" "$XDG_DATA_HOME" "$XDG_STATE_HOME" "$XDG_CACHE_HOME" \
    "$FLATPAK_USER_DIR" "$FLATPAK_SYSTEM_DIR" "$XDG_RUNTIME_DIR"
chmod 700 "$GNUPGHOME" "$XDG_RUNTIME_DIR"
# D-Bus services are activated by the private bus. They must inherit the
# same installation paths that the test's flatpak commands use.
dbus-update-activation-environment FLATPAK_USER_DIR FLATPAK_SYSTEM_DIR \
    XDG_CONFIG_HOME XDG_DATA_HOME XDG_CACHE_HOME XDG_STATE_HOME XDG_RUNTIME_DIR
ostree init --repo="$FLATPAK_USER_DIR/repo" --mode=bare-user-only
ostree init --repo="$FLATPAK_SYSTEM_DIR/repo" --mode=bare-user-only
gpg --batch --pinentry-mode loopback --passphrase '' --quick-gen-key \
    'SauceFX CI update test <ci@saucefx.invalid>' rsa2048 sign 1d >"$WORK/key.log" 2>&1
KEY=$(gpg --batch --with-colons --list-secret-keys 2>/dev/null | python3 -I -c 'import sys; print(next(line.split(":")[9] for line in sys.stdin if line.startswith("fpr:")))')
gpg --batch --export "$KEY" >"$WORK/test-public.gpg"
APP_ID=io.github.sauucee.SauceFX
REF="app/$APP_ID/x86_64/master"
RUNTIME_ID=io.github.sauucee.UpdateTestRuntime
RUNTIME_REF="runtime/$RUNTIME_ID/x86_64/master"
REPO="$WORK/repo"
mkdir -p "$REPO"
ostree init --repo="$REPO" --mode=archive

export_runtime() {
    flatpak build-export --runtime --files=usr --gpg-sign="$KEY" --gpg-homedir="$GNUPGHOME" \
        "$REPO" "$RUNTIME" master >"$WORK/export-runtime-$1.log" 2>&1
}
export_app() {
    flatpak build-export --gpg-sign="$KEY" --gpg-homedir="$GNUPGHOME" \
        "$REPO" "$APP" master >"$WORK/export-app-$1.log" 2>&1
}
refresh_summary() {
    flatpak build-update-repo --gpg-sign="$KEY" --gpg-homedir="$GNUPGHOME" "$REPO"
}
installed_commit() {
    flatpak info --user --show-commit "$1"
}
published_commit() {
    ostree rev-parse --repo="$REPO" "$1"
}
run_probe() {
    flatpak run --no-documents-portal "$APP_ID" "$@"
}
wait_for_file() {
    local path=$1 seconds=$2
    local deadline=$((SECONDS + seconds))
    while [ ! -s "$path" ]; do
        if (( SECONDS >= deadline )); then
            printf 'Timed out waiting for %s\n' "$path" >&2
            return 1
        fi
        sleep 0.1
    done
}
activate_runtime() {
    export_runtime "$1"
    refresh_summary
    flatpak update --user --noninteractive --assumeyes "$RUNTIME_REF" >"$WORK/install-runtime-$1.log" 2>&1
    test "$(installed_commit "$RUNTIME_REF")" = "$(published_commit "$RUNTIME_REF")"
}

# Include the fixed deployment query and the shared objects it and the probe
# need. No graphics or audio libraries are required.
RUNTIME="$WORK/runtime"
mkdir -p "$RUNTIME/usr/lib/x86_64-linux-gnu" "$RUNTIME/usr/bin" "$RUNTIME/usr/lib64"
printf '[Runtime]\nname=%s\n' "$RUNTIME_ID" >"$RUNTIME/metadata"
cp -L /usr/bin/cat "$RUNTIME/usr/bin/cat"
cp -L /lib64/ld-linux-x86-64.so.2 "$RUNTIME/usr/lib64/"
ldd "$PROBE" /usr/bin/cat | python3 -I -c 'import re,sys; print("\n".join(dict.fromkeys(re.findall(r"(?:=>\s+)?(/[^\s]+)",sys.stdin.read()))))' >"$WORK/dependencies"
while IFS= read -r library; do
    [ -f "$library" ] || continue
    cp -L "$library" "$RUNTIME/usr/lib/x86_64-linux-gnu/"
done < "$WORK/dependencies"
ln -s lib64/ld-linux-x86-64.so.2 "$RUNTIME/usr/ld-linux-x86-64.so.2"
export_runtime initial
APP="$WORK/app"
mkdir -p "$APP/files/bin"
cp "$PROBE" "$APP/files/bin/SauceFX"
printf '[Application]\nname=%s\nruntime=%s/x86_64/master\ncommand=SauceFX\n' "$APP_ID" "$RUNTIME_ID" >"$APP/metadata"
printf old >"$APP/files/version-marker"
printf 0.3.2 >"$APP/files/release-version"
flatpak build-finish --command=SauceFX --filesystem="$XDG_STATE_HOME" "$APP"
export_app old
refresh_summary
flatpak remote-add --user --gpg-import="$WORK/test-public.gpg" update-test "file://$REPO"
flatpak install --user -y --noninteractive update-test "$RUNTIME_REF" "$REF"
# The fixture uses temporary XDG paths rather than a normal per-app home.
# Mount and bind the same state directory for launch and portal restart.
flatpak override --user --env=XDG_STATE_HOME="$XDG_STATE_HOME" "$APP_ID"
STATE=$(run_probe --state-path)
OLD=$(installed_commit "$REF")
RUNTIME_OLD=$(installed_commit "$RUNTIME_REF")

# Prove this transaction changes the runtime but not the application. A test
# that hit an empty transaction instead would miss the original false success.
printf updated >"$RUNTIME/usr/runtime-marker"
export_runtime updated
refresh_summary
RUNTIME_NEW=$(published_commit "$RUNTIME_REF")
test "$RUNTIME_NEW" != "$RUNTIME_OLD"
test "$(published_commit "$REF")" = "$OLD"
# Permission changes are scoped to the private session's permission store.
flatpak permission-set flatpak updates "$APP_ID" yes
run_probe --expect-up-to-date >"$WORK/runtime-only.log" 2>&1
test "$(installed_commit "$RUNTIME_REF")" = "$RUNTIME_NEW"
test "$(installed_commit "$REF")" = "$OLD"

# A new signed app commit on the same version must still require restart.
printf new >"$APP/files/version-marker"
export_app new
refresh_summary
NEW=$(published_commit "$REF")
test "$OLD" != "$NEW"
flatpak permission-set flatpak updates "$APP_ID" no
run_probe --expect-error >"$WORK/denied.log" 2>&1
test "$(installed_commit "$REF")" = "$OLD"
flatpak permission-set flatpak updates "$APP_ID" yes
run_probe --repeat-check --test-restart >"$WORK/update.log" 2>&1
test "$(installed_commit "$REF")" = "$NEW"
LOCATION=$(flatpak info --user --show-location "$REF")
test "$(<"$LOCATION/files/version-marker")" = new
test "$(<"$LOCATION/files/release-version")" = 0.3.2
wait_for_file "$STATE/restarted-version" 20
test "$(<"$STATE/restarted-version")" = new
test "$(<"$STATE/saved-before-restart")" = saved
run_probe --expect-up-to-date >"$WORK/current.log" 2>&1

# Hold an instance of B before its first check, then install C externally.
# The portal has nothing left to install, but B still needs a restart.
timeout --kill-after=5s 390s flatpak run --no-documents-portal "$APP_ID" \
    --wait-for-staged-update --repeat-check >"$WORK/staged.log" 2>&1 &
STAGED_PID=$!
wait_for_file "$STATE/stage-ready" 30
test "$(<"$STATE/stage-running-commit")" = "$NEW"
printf staged >"$APP/files/version-marker"
export_app staged
refresh_summary
STAGED=$(published_commit "$REF")
test "$STAGED" != "$NEW"
timeout --kill-after=5s 60s flatpak update --user --noninteractive --assumeyes "$REF" >"$WORK/stage-external.log" 2>&1
test "$(installed_commit "$REF")" = "$STAGED"
touch "$STATE/stage-release"
staged_result=0
wait "$STAGED_PID" || staged_result=$?
STAGED_PID=
test "$staged_result" = 0

# The app stays current. Signed fixture runtimes replace only the fixed query
# command. The path faults keep app-path valid while breaking original-app-path,
# so a parser that ignores the authoritative key cannot pass on any runner.
# A witness proves each query reached the helper.
cp "$PROBE" "$RUNTIME/usr/bin/cat"
printf '%s\n' "$STATE" >"$RUNTIME/usr/deployment-query-state"
for mode in exit malformed identity installation; do
    printf '%s\n' "$mode" >"$RUNTIME/usr/deployment-query-mode"
    activate_runtime "query-$mode"
    run_probe --expect-error >"$WORK/query-$mode.log" 2>&1
    test "$(<"$STATE/query-$mode-observed")" = queried
    test "$(installed_commit "$REF")" = "$STAGED"
done
cp -L /usr/bin/cat "$RUNTIME/usr/bin/cat"
rm -- "$RUNTIME/usr/deployment-query-mode" "$RUNTIME/usr/deployment-query-state"
activate_runtime restored
run_probe --expect-up-to-date >"$WORK/staged-current.log" 2>&1

# A signed summary must not make an unsigned application commit installable.
printf unsigned >"$APP/files/version-marker"
flatpak build-export "$REPO" "$APP" master >"$WORK/export-unsigned.log" 2>&1
refresh_summary
test "$(published_commit "$REF")" != "$STAGED"
run_probe --expect-error >"$WORK/unsigned.log" 2>&1
test "$(installed_commit "$REF")" = "$STAGED"
cat "$WORK/runtime-only.log" "$WORK/update.log" "$WORK/current.log" "$WORK/denied.log" \
    "$WORK/staged.log" "$WORK/staged-current.log" "$WORK"/query-*.log "$WORK/unsigned.log"
printf '%s\n' 'PASS: runtime-only update, same-version app update, staged/repeated checks, confirmed restart, failure handling and signature enforcement'
