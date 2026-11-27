#!/usr/bin/env bash
# Exercise the extracted native image on a private audio server and X display.
# No FUSE, system installation, or changes to the desktop audio session are
# needed. Proves the full chain: relocated launch -> session restore ->
# SauceFX sink -> DSP -> render into the default output, with the bundled
# (version-matched) PipeWire library actually in use.
set -euo pipefail
SCRIPT_DIR=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)
IMAGE=$(realpath "${1:?pass the AppImage path}")
if [ "${2:-}" != --isolated ]; then
    if [ "${SAUCEFX_TEST_WAYLAND:-0}" = 1 ]; then
        exec dbus-run-session -- bash "$0" "$IMAGE" --isolated
    fi
    exec dbus-run-session -- xvfb-run -a bash "$0" "$IMAGE" --isolated
fi

WORK=$(mktemp -d "${TMPDIR:-/tmp}/saucefx-native-test.XXXXXX")
RUN_DIR=$(mktemp -d "${TMPDIR:-/tmp}/sfx-run.XXXXXX")
PIDS=()
cleanup() {
    local result=$?
    if [ "$result" -ne 0 ]; then cat "$WORK"/*.log >&2 || true; fi
    for pid in "${PIDS[@]}"; do kill "$pid" 2>/dev/null || true; done
    for pid in "${PIDS[@]}"; do wait "$pid" 2>/dev/null || true; done
    rm -rf -- "$WORK" "$RUN_DIR"
}
trap cleanup EXIT

export XDG_RUNTIME_DIR="$RUN_DIR" XDG_CONFIG_HOME="$WORK/config"
export XDG_STATE_HOME="$WORK/state" XDG_DATA_HOME="$WORK/data" XDG_CACHE_HOME="$WORK/cache"
export HOME="$WORK/home" GIO_USE_VFS=local
export PIPEWIRE_RUNTIME_DIR="$XDG_RUNTIME_DIR"
export PULSE_SERVER="unix:$XDG_RUNTIME_DIR/pulse/native"
unset APPDIR APPIMAGE APPIMAGE_EXTRACT_AND_RUN FXSOUND_REG_FILE PIPEWIRE_REMOTE PULSE_RUNTIME_PATH
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
# A listening Pulse socket is not enough: WirePlumber must first grant the
# manager access to factories/default metadata (slower on busy desktops).
MODULE=
for ((i=0; i<100; ++i)); do
    if MODULE=$(env PULSE_PROP=media.category=Manager pactl load-module \
        module-null-sink sink_name=AppImageTestOutput 2>"$WORK/setup.log"); then break; fi
    sleep 0.1
done
test -n "$MODULE"
for ((i=0; i<100; ++i)); do
    if env PULSE_PROP=media.category=Manager pactl set-default-sink AppImageTestOutput \
        2>"$WORK/setup.log"; then break; fi
    sleep 0.1
done
test "$(pactl get-default-sink)" = AppImageTestOutput

# Use a directory containing spaces, and launch from an unrelated cwd.
mkdir -p "$WORK/Portable App"
(cd "$WORK/Portable App" && "$IMAGE" --appimage-extract >/dev/null)
APPDIR_TEST="$WORK/Portable App/squashfs-root"
APP_STATE_DIR="$XDG_STATE_HOME/saucefx"
mkdir -p "$APP_STATE_DIR"
printf 'theme=dark\nautostart=on\noutput=saucefx\n' > "$APP_STATE_DIR/settings.txt"
python3 - "$APP_STATE_DIR/preset-session.txt" <<'PY'
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
if [ "${SAUCEFX_TEST_WAYLAND:-0}" = 1 ]; then
    # No XWayland fallback: prove the app runs on a native Wayland surface.
    unset DISPLAY WAYLAND_DISPLAY WAYLAND_SOCKET
    printf 'xwayland disable\noutput * resolution 1280x800\n' > "$WORK/sway.conf"
    WLR_BACKENDS=headless WLR_LIBINPUT_NO_DEVICES=1 WLR_RENDERER=pixman \
        sway -c "$WORK/sway.conf" >"$WORK/wm.log" 2>&1 &
    wm_pid=$!
    PIDS+=("$wm_pid")
    export SWAYSOCK="$XDG_RUNTIME_DIR/sway-ipc.$(id -u).$wm_pid.sock"
    for ((i=0; i<100; ++i)); do
        for socket in "$XDG_RUNTIME_DIR"/wayland-*; do
            if [ -S "$socket" ]; then export WAYLAND_DISPLAY="${socket##*/}"; break; fi
        done
        if [ -n "${WAYLAND_DISPLAY:-}" ] && swaymsg -t get_outputs >/dev/null 2>&1; then break; fi
        kill -0 "$wm_pid"
        sleep .1
    done
    test -S "$XDG_RUNTIME_DIR/${WAYLAND_DISPLAY:?missing Wayland display}"
    swaymsg -t get_outputs >/dev/null
else
    # Xvfb alone has no minimize/restore; use a real window manager.
    openbox >"$WORK/wm.log" 2>&1 &
    PIDS+=("$!")
fi
SAUCEFX_TRACE_SPECTRUM=1 APPIMAGE="$IMAGE" "$APPDIR_TEST/AppRun" --autostart >"$WORK/app.log" 2>&1 &
APP_PID=$!
PIDS+=("$APP_PID")

# The host creates its virtual sink once the engine is up (Skia's first
# frame can take a few seconds on the software GL driver).
for ((i=0; i<300; ++i)); do
    if pactl list short sinks 2>/dev/null | grep -q SauceFX; then break; fi
    if ! kill -0 "$APP_PID" 2>/dev/null; then cat "$WORK/app.log" >&2; exit 1; fi
    sleep 0.1
done
pactl list short sinks | grep -q SauceFX
kill -0 "$APP_PID"

# The bundled, version-matched PipeWire client library must be the one in use.
grep 'libpipewire-0.3.so' "/proc/$APP_PID/maps" | grep -F "$APPDIR_TEST/" >/dev/null

# Full audio chain: play a 440 Hz tone into the SauceFX sink and record the
# default output the engine renders into. The restored Default preset must
# pass the tone through (no doubling, no stall).
python3 - "$WORK/tone440.wav" <<'PY'
import math, struct, sys, wave
rate = 44100
with wave.open(sys.argv[1], 'wb') as w:
    w.setnchannels(1); w.setsampwidth(2); w.setframerate(rate)
    w.writeframes(b''.join(struct.pack('<h', int(20000 * math.sin(2 * math.pi * 440 * t / rate)))
                           for t in range(rate * 3)))
PY
audio_check() {
    parecord --device=AppImageTestOutput.monitor --file-format=wav "$WORK/flow.wav" >"$WORK/rec.log" 2>&1 &
    REC_PID=$!
    PIDS+=("$REC_PID")
    sleep 0.5
    paplay --device=SauceFX "$WORK/tone440.wav" >"$WORK/play.log" 2>&1
    sleep 1
    kill "$REC_PID" 2>/dev/null || true
    wait "$REC_PID" 2>/dev/null || true
    python3 - "$WORK/flow.wav" <<'PY'
import math, struct, sys, wave
w = wave.open(sys.argv[1], 'rb')
rate, ch = w.getframerate(), w.getnchannels()
data = w.readframes(w.getnframes())
vals = struct.unpack(f'<{len(data)//2}h', data)
xs = [v / 32768.0 for v in vals[::ch]]
seg = xs[rate:2 * rate]
if len(seg) <= rate // 2:
    sys.exit(f'capture too short: {len(xs)/rate:.2f}s')
rms = math.sqrt(sum(x * x for x in seg) / len(seg))
def goertzel(f):
    coeff = 2 * math.cos(2 * math.pi * f / rate)
    s1 = s2 = 0.0
    for x in seg:
        s0 = x + coeff * s1 - s2
        s2, s1 = s1, s0
    return math.sqrt(s1 * s1 + s2 * s2 - coeff * s1 * s2) * 2 / len(seg)
g440, g880 = goertzel(440), goertzel(880)
if rms <= 0.05:
    sys.exit(f'no audio reached the output (rms={rms:.4f})')
if g440 <= 10 * g880:
    sys.exit(f'440 Hz tone distorted (440={g440:.4f}, 880={g880:.4f})')
print(f'audio chain ok: rms={rms:.3f} |440|={g440:.3f} |880|={g880:.5f}')
PY
}
# The sink can exist before the engine's render path is fully settled; a
# capture taken in that window is mostly startup transient (observed once
# on a loaded runner). Retry with a settle delay before declaring failure.
for attempt in 1 2 3; do
    if audio_check; then break; fi
    if [ "$attempt" = 3 ]; then
        echo "audio chain check failed after 3 attempts" >&2
        exit 1
    fi
    sleep 2
done

# The tone above traversed the real DSP while the login window was hidden.
python3 "$SCRIPT_DIR/test-visualizer.py" "$WORK/app.log" "$APP_STATE_DIR"

# Require the save timer to replace the old mount path and retain the
# non-default band value, proving the seeded session was actually restored.
for ((i=0; i<100; ++i)); do
    if grep -Fqx "preset=$APPDIR_TEST/usr/share/saucefx/presets/Factsoft/Default.fac" \
        "$APP_STATE_DIR/preset-session.txt"; then break; fi
    sleep 0.1
done
grep -Fqx "preset=$APPDIR_TEST/usr/share/saucefx/presets/Factsoft/Default.fac" \
    "$APP_STATE_DIR/preset-session.txt"
grep -qx 'factory-preset=Default.fac' "$APP_STATE_DIR/preset-session.txt"
grep -qx 'eq17gain=3.000000' "$APP_STATE_DIR/preset-session.txt"
# Start at login is on by default and re-applied at startup: the portable
# launch command is registered under the private config dir.
grep -F "Exec=\"$IMAGE\" --autostart" "$XDG_CONFIG_HOME/autostart/saucefx.desktop"
kill -0 "$APP_PID"
echo "PASS: native AppImage starts from a relocated directory, restores its preset session, uses the bundled PipeWire, and processes audio end to end"