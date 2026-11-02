#!/usr/bin/env bash
# Exercise the extracted image on a private audio server and X display. No
# FUSE, system installation, or changes to the desktop audio session are needed.
set -euo pipefail
SCRIPT_DIR=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)
IMAGE=$(realpath "${1:?pass the AppImage path}")
if [ "${2:-}" != --isolated ]; then
    exec dbus-run-session -- xvfb-run -a bash "$0" "$IMAGE" --isolated
fi

WORK=$(mktemp -d "${TMPDIR:-/tmp}/saucefx-appimage-test.XXXXXX")
PIDS=()
cleanup() {
    local result=$?
    if [ "$result" -ne 0 ]; then cat "$WORK"/*.log >&2 || true; fi
    for pid in "${PIDS[@]}"; do kill "$pid" 2>/dev/null || true; done
    for pid in "${PIDS[@]}"; do wait "$pid" 2>/dev/null || true; done
    rm -rf -- "$WORK"
}
trap cleanup EXIT

export XDG_RUNTIME_DIR="$WORK/run" XDG_CONFIG_HOME="$WORK/config"
export XDG_STATE_HOME="$WORK/state" XDG_DATA_HOME="$WORK/data" XDG_CACHE_HOME="$WORK/cache"
export HOME="$WORK/home" GIO_USE_VFS=local
export PIPEWIRE_RUNTIME_DIR="$XDG_RUNTIME_DIR"
export PULSE_RUNTIME_PATH="$XDG_RUNTIME_DIR/pulse"
export PULSE_SERVER="unix:$PULSE_RUNTIME_PATH/native"
unset APPDIR APPIMAGE APPIMAGE_EXTRACT_AND_RUN FXSOUND_REG_FILE PIPEWIRE_REMOTE
mkdir -p "$XDG_RUNTIME_DIR" "$HOME" "$XDG_CONFIG_HOME/wireplumber/wireplumber.conf.d"
chmod 700 "$XDG_RUNTIME_DIR"
cat > "$XDG_CONFIG_HOME/wireplumber/wireplumber.conf.d/90-isolated.conf" <<'EOF'
wireplumber.profiles = { main = {
  monitor.alsa = disabled
  monitor.bluez = disabled
  monitor.v4l2 = disabled
  monitor.libcamera = disabled
} }
EOF
pipewire >"$WORK/pipewire.log" 2>&1 &
PIDS+=("$!")
for ((i=0; i<100; ++i)); do [ ! -S "$XDG_RUNTIME_DIR/pipewire-0" ] || break; sleep 0.05; done
test -S "$XDG_RUNTIME_DIR/pipewire-0"
pipewire-pulse >"$WORK/pulse.log" 2>&1 &
PIDS+=("$!")
wireplumber >"$WORK/wireplumber.log" 2>&1 &
PIDS+=("$!")
for ((i=0; i<100; ++i)); do if pactl info >/dev/null 2>&1; then break; fi; sleep 0.05; done
pactl load-module module-null-sink sink_name=AppImageTestOutput >/dev/null
pactl set-default-sink AppImageTestOutput

# Use a directory containing spaces, and launch from unrelated cwd.
mkdir -p "$WORK/Portable App"
(cd "$WORK/Portable App" && "$IMAGE" --appimage-extract >/dev/null)
APPDIR_TEST="$WORK/Portable App/squashfs-root"
mkdir -p "$XDG_STATE_HOME/saucefx"
printf 'theme=dark\nautostart=on\noutput=saucefx\n' > "$XDG_STATE_HOME/saucefx/settings.txt"
python3 - "$XDG_STATE_HOME/saucefx/preset-session.txt" <<'PY'
import sys
from pathlib import Path
frequencies = [20, 25, 31.5, 40, 50, 63, 80, 100, 125, 160, 200, 250, 315, 400,
    500, 630, 800, 1000, 1250, 1600, 2000, 2500, 3150, 4000, 5000, 6300, 8000,
    10000, 12500, 16000, 20000]
lines = ['version=1', 'preset=/tmp/old-AppImage-mount/usr/share/saucefx/presets/Factsoft/Default.fac',
    'factory-preset=Default.fac', 'power=1', 'bands=31']
lines += [f'effect{i}=0' for i in range(5)]
for i, frequency in enumerate(frequencies):
    lines += [f'eq{i}freq={frequency}', f'eq{i}gain={3 if i == 17 else 0}']
Path(sys.argv[1]).write_text('\n'.join(lines) + '\n')
PY
cd "$WORK"
APPIMAGE="$IMAGE" "$APPDIR_TEST/AppRun" >"$WORK/app.log" 2>&1 &
APP_PID=$!
PIDS+=("$APP_PID")
for ((i=0; i<300; ++i)); do
    if grep -q 'web interface ready' "$WORK/app.log"; then break; fi
    if ! kill -0 "$APP_PID" 2>/dev/null; then cat "$WORK/app.log" >&2; exit 1; fi
    sleep 0.1
done
grep -q 'web interface ready' "$WORK/app.log"
grep -q "startup preset 'Default' \[ok\]" "$WORK/app.log"
grep -q 'session restored' "$WORK/app.log"
grep -q 'render target: AppImageTestOutput' "$WORK/app.log"
grep -q 'libwebkit2gtk-4.1' "/proc/$APP_PID/maps"
grep 'libwebkit2gtk-4.1' "/proc/$APP_PID/maps" | grep -F "$APPDIR_TEST/" >/dev/null
grep -F "Exec=\"$IMAGE\" --autostart" "$XDG_CONFIG_HOME/autostart/saucefx.desktop"
if grep -q 'WEBKIT_DISABLE_SANDBOX' "$APPDIR_TEST/AppRun"; then
    echo 'Browser sandbox must not be disabled' >&2; exit 1
fi
echo "PASS: AppImage starts from a relocated directory, restores its preset session, renders its UI, processes audio, and preserves its autostart path"
