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

flatpak run --no-documents-portal io.github.sauucee.SauceFX >"$WORK/app.log" 2>&1 &
APP_PID=$!
PIDS+=("$APP_PID")
for ((i=0; i<300; ++i)); do
    if pactl list short sinks 2>/dev/null | grep -q SauceFX; then break; fi
    if ! kill -0 "$APP_PID" 2>/dev/null; then cat "$WORK/app.log" >&2; exit 1; fi
    sleep 0.1
done
pactl list short sinks | grep -q SauceFX
kill -0 "$APP_PID"
echo "PASS: native Flatpak launches in the sandbox and creates the SauceFX sink"