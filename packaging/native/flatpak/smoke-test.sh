#!/usr/bin/env bash
# Launch-check the installed native Flatpak: private PipeWire session (ALSA
# monitor disabled) + Xvfb, then assert the app starts and creates its
# virtual sink inside the sandbox. Exact-PID cleanup only.
set -euo pipefail
if [ "${1:-}" != --isolated ]; then
    exec dbus-run-session -- xvfb-run -a bash "$0" --isolated
fi

WORK=$(mktemp -d "${TMPDIR:-/tmp}/saucefx-flatpak-test.XXXXXX")
PIDS=()
cleanup() {
    local result=$?
    if [ "$result" -ne 0 ]; then cat "$WORK"/*.log >&2 || true; fi
    for pid in "${PIDS[@]}"; do kill "$pid" 2>/dev/null || true; done
    for pid in "${PIDS[@]}"; do wait "$pid" 2>/dev/null || true; done
    rm -rf -- "$WORK"
}
trap cleanup EXIT

mkdir -p "$HOME/.config/wireplumber/wireplumber.conf.d"
cat > "$HOME/.config/wireplumber/wireplumber.conf.d/90-isolated.conf" <<'EOF'
wireplumber.profiles = { main = {
  monitor.alsa = disabled
  monitor.bluez = disabled
  monitor.v4l2 = disabled
  monitor.libcamera = disabled
} }
EOF
pipewire >"$WORK/pipewire.log" 2>&1 &
PIDS+=("$!")
for ((i=0; i<100; ++i)); do [ ! -S "${XDG_RUNTIME_DIR}/pipewire-0" ] || break; sleep 0.05; done
test -S "${XDG_RUNTIME_DIR}/pipewire-0"
pipewire-pulse >"$WORK/pulse.log" 2>&1 &
PIDS+=("$!")
wireplumber >"$WORK/wireplumber.log" 2>&1 &
PIDS+=("$!")
for ((i=0; i<100; ++i)); do if pactl info >/dev/null 2>&1; then break; fi; sleep 0.05; done
pactl info >/dev/null

app=io.github.sauucee.SauceFX
flatpak run --no-documents-portal "$app" --autostart >"$WORK/app.log" 2>&1 &
APP_PID=$!
PIDS+=("$APP_PID")
for ((i=0; i<300; ++i)); do
    if pactl list short sinks 2>/dev/null | grep -q SauceFX; then break; fi
    if ! kill -0 "$APP_PID" 2>/dev/null; then cat "$WORK/app.log" >&2; exit 1; fi
    sleep 0.1
done
pactl list short sinks | grep -q SauceFX
kill -0 "$APP_PID"
# First launch must discover and apply the installed factory Default. Check
# the saved app state rather than just the presence of a file in the package.
for ((i=0; i<100; ++i)); do
    if flatpak run --no-documents-portal --command=sh io.github.sauucee.SauceFX -c \
        'grep -qx "preset=/app/share/saucefx/presets/Factsoft/Default.fac" "$XDG_STATE_HOME/saucefx/preset-session.txt"' \
        2>/dev/null; then break; fi
    sleep 0.1
done
flatpak run --no-documents-portal --command=sh io.github.sauucee.SauceFX -c \
    'grep -qx "preset=/app/share/saucefx/presets/Factsoft/Default.fac" "$XDG_STATE_HOME/saucefx/preset-session.txt" && grep -qx "factory-preset=Default.fac" "$XDG_STATE_HOME/saucefx/preset-session.txt"'
# The login instance is processing audio, but should not yet have a mapped window.
if xdotool search --onlyvisible --name '^SauceFX' >/dev/null; then
    echo 'FAIL: the login window was already visible' >&2; exit 1
fi
# Run the action from the installed export, not from our source .desktop file.
# CI has no documents portal; otherwise use the exact Flatpak-exported command.
exported="$(flatpak info --user --show-location "$app")/export/share/applications/$app.desktop"
python3 - "$exported" "$app" <<'PY'
import configparser, pathlib, shlex, subprocess, sys
path, app = sys.argv[1:]
entry = configparser.ConfigParser(interpolation=None)
with open(path) as stream:
    entry.read_file(stream)
assert 'ShowWindow' in entry['Desktop Entry']['Actions'].split(';')
cmd = shlex.split(entry['Desktop Action ShowWindow']['Exec'])
assert pathlib.Path(cmd[0]).name == 'flatpak' and cmd[1] == 'run' and cmd[-1] == app, cmd
result = subprocess.run(cmd[:2] + ['--no-documents-portal'] + cmd[2:],
                        check=True, timeout=12, capture_output=True, text=True)
assert 'asked it to show its window' in result.stderr, result.stderr
PY
for ((i=0; i<100; ++i)); do
    if wid=$(xdotool search --onlyvisible --name '^SauceFX' 2>/dev/null); then break; fi
    sleep 0.1
done
[[ "$wid" =~ ^[0-9]+$ ]]
test "$(xdotool getwindowname "$wid")" = 'SauceFX'
kill -0 "$APP_PID"
pactl list short sinks | grep -q SauceFX
echo "PASS: installed Flatpak Show Window action restores the login instance with title SauceFX and its audio sink intact"