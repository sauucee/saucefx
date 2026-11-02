#!/usr/bin/env bash
# Run routing changes on a private audio server, never on the desktop session.
set -euo pipefail
ROOT=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)
TEST=${1:-"$ROOT/build/bin/outputroutingtest"}

if [ "${2:-}" != --isolated ]; then
    for tool in pipewire pipewire-pulse wireplumber pactl pw-dump dbus-run-session python3; do
        if ! command -v "$tool" >/dev/null 2>&1; then
            echo "SKIP: output routing tests need $tool" >&2
            exit 77
        fi
    done
    exec dbus-run-session -- bash "$0" "$TEST" --isolated
fi

WORK=$(mktemp -d "${TMPDIR:-/tmp}/saucefx-outputs.XXXXXX")
PIDS=()
cleanup() {
    local result=$?
    if [ "$result" -ne 0 ]; then
        cat "$WORK"/*.log >&2 || true
    fi
    for pid in "${PIDS[@]}"; do kill "$pid" 2>/dev/null || true; done
    for pid in "${PIDS[@]}"; do wait "$pid" 2>/dev/null || true; done
    rm -rf "$WORK"
}
trap cleanup EXIT

export XDG_RUNTIME_DIR="$WORK/run"
export PIPEWIRE_RUNTIME_DIR="$XDG_RUNTIME_DIR"
export XDG_CONFIG_HOME="$WORK/config"
export XDG_STATE_HOME="$WORK/state"
export XDG_CACHE_HOME="$WORK/cache"
export PULSE_RUNTIME_PATH="$XDG_RUNTIME_DIR/pulse"
export PULSE_SERVER="unix:$PULSE_RUNTIME_PATH/native"
export FXSOUND_REG_FILE="$WORK/registry.txt"
export SAUCEFX_TEST_ISOLATED=1
export GIO_USE_VFS=local
unset PIPEWIRE_REMOTE PIPEWIRE_CONFIG_DIR WIREPLUMBER_CONFIG_DIR
mkdir -p "$XDG_RUNTIME_DIR" "$XDG_CONFIG_HOME/wireplumber/wireplumber.conf.d"
chmod 700 "$XDG_RUNTIME_DIR"
cat > "$XDG_CONFIG_HOME/wireplumber/wireplumber.conf.d/90-isolated.conf" <<'EOF'
wireplumber.profiles = {
  main = {
    monitor.alsa = disabled
    monitor.bluez = disabled
    monitor.v4l2 = disabled
    monitor.libcamera = disabled
  }
}
EOF

pipewire >"$WORK/pipewire.log" 2>&1 &
PIDS+=("$!")
for ((i=0; i<100; ++i)); do
    if [ -S "$XDG_RUNTIME_DIR/pipewire-0" ]; then break; fi
    sleep 0.05
done
test -S "$XDG_RUNTIME_DIR/pipewire-0"
pipewire-pulse >"$WORK/pulse.log" 2>&1 &
PIDS+=("$!")
wireplumber >"$WORK/wireplumber.log" 2>&1 &
PIDS+=("$!")
for ((i=0; i<100; ++i)); do
    if pactl info >/dev/null 2>&1; then break; fi
    sleep 0.05
done
pactl info >/dev/null
cd "$ROOT"
"$TEST"
