#!/usr/bin/env bash
# Regression test: a live sibling engine's sink must not be reaped.
#
# Two engines can briefly share a sink name while one is starting (for
# example a second instance racing the single-instance handoff). The reaper
# must only unload sinks whose owning process is gone, never a live peer.
#
# This test starts two saucefxrt instances with the same isolated sink name in
# the same PID namespace. When the second starts it scans the existing sink
# modules; the first module must still be present and the first process must
# still be alive after the second reaches ready.
#
# Usage: src/harness/sinklivesiblingtest.sh [path/to/saucefxrt]
# Exit 77 means the environment cannot run the test (no pactl/PipeWire).

set -euo pipefail

ROOT=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/../.." && pwd)
RT=${1:-"$ROOT/src/build/bin/saucefxrt"}
SINK="SauceFXLiveSiblingTest-$$"
LOG1=$(mktemp)
LOG2=$(mktemp)
FIRST_PID=
SECOND_PID=

sink_rows()
{
    pactl list short modules 2>/dev/null |
        awk -v name="sink_name=$SINK " \
            'index($0, name) && $2 == "module-null-sink" { print }'
}

count_sinks()
{
    sink_rows | wc -l | tr -d '[:space:]'
}

cleanup()
{
    for pid in "$SECOND_PID" "$FIRST_PID"; do
        if [ -n "$pid" ]; then
            kill -KILL "$pid" 2>/dev/null || true
            wait "$pid" 2>/dev/null || true
        fi
    done
    SECOND_PID=
    FIRST_PID=
    if command -v pactl >/dev/null 2>&1; then
        for idx in $(pactl list short modules 2>/dev/null |
                awk -v name="sink_name=$SINK " \
                    'index($0, name) && $2 == "module-null-sink" { print $1 }'); do
            pactl unload-module "$idx" 2>/dev/null || true
        done
    fi
    rm -f "$LOG1" "$LOG2"
}
trap cleanup EXIT

if [ ! -x "$RT" ]; then
    echo "error: $RT is missing; run 'make -C src rt' first" >&2
    exit 1
fi
if ! command -v pactl >/dev/null 2>&1 ||
        ! pactl info >/dev/null 2>&1; then
    echo "SKIP: PulseAudio/PipeWire is not available" >&2
    exit 77
fi

wait_ready()
{
    local pid=$1
    local log=$2
    for _ in $(seq 1 300); do
        if grep -q 'fxpw: ready' "$log"; then
            return 0
        fi
        if ! kill -0 "$pid" 2>/dev/null; then
            echo "engine exited before becoming ready:" >&2
            cat "$log" >&2
            return 1
        fi
        sleep 0.1
    done
    echo "timeout waiting for engine readiness:" >&2
    cat "$log" >&2
    return 1
}

"$RT" --sink-name "$SINK" >"$LOG1" 2>&1 &
FIRST_PID=$!
wait_ready "$FIRST_PID" "$LOG1"

first_row=$(sink_rows)
if [ "$(count_sinks)" -ne 1 ]; then
    echo "FAIL: first engine did not create exactly one $SINK sink" >&2
    echo "sinks:" >&2
    sink_rows >&2
    exit 1
fi

"$RT" --sink-name "$SINK" >"$LOG2" 2>&1 &
SECOND_PID=$!
wait_ready "$SECOND_PID" "$LOG2"

if ! kill -0 "$FIRST_PID" 2>/dev/null; then
    echo "FAIL: second engine killed the live first engine" >&2
    echo "first engine log:" >&2
    cat "$LOG1" >&2
    exit 1
fi

if ! sink_rows | grep -Fqx -- "$first_row"; then
    echo "FAIL: second engine reaped the live first engine's sink" >&2
    echo "first sink:" >&2
    printf '%s\n' "$first_row" >&2
    echo "current sinks:" >&2
    sink_rows >&2
    exit 1
fi

if [ "$(count_sinks)" -ne 2 ]; then
    echo "FAIL: expected both live sinks to remain after the second start" >&2
    echo "sinks:" >&2
    sink_rows >&2
    exit 1
fi

echo "PASS: second engine left the live sibling's sink and process intact"
