#!/usr/bin/env bash
# Run the real output-restore regression on a private audio server in CI.
set -euo pipefail
SCRIPT_DIR=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)
ROOT=$(cd -- "$SCRIPT_DIR/../.." && pwd)
if [ "${1:-}" != --isolated ]; then
    exec dbus-run-session -- xvfb-run -a bash "$0" --isolated
fi

WORK=$(mktemp -d "${TMPDIR:-/tmp}/saucefx-direct.XXXXXX")
PIDS=()
cleanup() {
    local result=$?
    if [ "$result" -ne 0 ]; then cat "$WORK"/*.log >&2 || true; fi
    for pid in "${PIDS[@]}"; do kill "$pid" 2>/dev/null || true; done
    for pid in "${PIDS[@]}"; do wait "$pid" 2>/dev/null || true; done
    rm -rf -- "$WORK"
}
trap cleanup EXIT
export XDG_RUNTIME_DIR="$WORK/run" PIPEWIRE_RUNTIME_DIR="$WORK/run"
export XDG_CONFIG_HOME="$WORK/config" XDG_STATE_HOME="$WORK/state"
export XDG_CACHE_HOME="$WORK/cache" FXSOUND_REG_FILE="$WORK/registry.txt"
export PULSE_SERVER="unix:$WORK/run/pulse/native" SAUCEFX_TEST_ISOLATED=1
unset PIPEWIRE_REMOTE PIPEWIRE_CONFIG_DIR WIREPLUMBER_CONFIG_DIR PULSE_RUNTIME_PATH
mkdir -p "$XDG_RUNTIME_DIR/pulse" "$XDG_CONFIG_HOME/wireplumber/wireplumber.conf.d"
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
PROBE=
for ((i=0; i<100; ++i)); do
    if PROBE=$(env PULSE_PROP=media.category=Manager pactl load-module \
        module-null-sink sink_name=RestoreProbe 2>"$WORK/setup.log"); then break; fi
    sleep 0.1
done
test -n "$PROBE"
env PULSE_PROP=media.category=Manager pactl unload-module "$PROBE"
cargo test --manifest-path "$ROOT/Cargo.toml" --locked --jobs 2 \
    -p saucefx-host --no-default-features --features renderer-skia \
    fxpw::tests::direct_playback_startup_restores_the_saved_engine_output \
    -- --exact --ignored --test-threads=1
