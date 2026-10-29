#!/usr/bin/env bash
# Regression test for stale virtual-sink reaping across hard restarts.
#
# A PulseAudio null-sink module outlives the process that created it. The
# bridge tags each sink with fxpw-<pid>-<starttime> so a restarted process
# can distinguish a stale sink from a live sibling. Flatpak makes this
# especially important: every launch runs in a fresh PID namespace, so the
# process is PID 2 again and a PID-only tag looks alive forever.
#
# This test runs saucefxrt inside a fresh user+PID namespace for each launch,
# SIGKILLs the first instance (leaving its sink behind), then starts a second
# instance and requires exactly one sink for the test name. The process count
# and PID reuse are deliberately the same as two Flatpak launches.
#
# Usage: src/harness/sinkrestarttest.sh [path/to/saucefxrt]
# Exit 77 means the environment cannot run the test (no pactl, no user+PID
# namespaces), following the automake "skip" convention.

set -euo pipefail

ROOT=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/../.." && pwd)
RT=${1:-"$ROOT/src/build/bin/saucefxrt"}
SINK="SauceFXRestartTest-$$"
LOG1=$(mktemp)
LOG2=$(mktemp)
ENGINE_PID=

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
    if [ -n "${ENGINE_PID:-}" ]; then
        kill -KILL "$ENGINE_PID" 2>/dev/null || true
        wait "$ENGINE_PID" 2>/dev/null || true
        ENGINE_PID=
    fi
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
if ! command -v unshare >/dev/null 2>&1 ||
        ! unshare --user --map-root-user --pid --fork --mount-proc \
            --kill-child=SIGKILL true >/dev/null 2>&1; then
    echo "SKIP: unprivileged user and PID namespaces are not available" >&2
    exit 77
fi

start_engine()
{
    local log=$1
    unshare --user --map-root-user --pid --fork --mount-proc \
        --kill-child=SIGKILL "$RT" --sink-name "$SINK" >"$log" 2>&1 &
    ENGINE_PID=$!
}

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

stop_engine()
{
    if [ -n "${ENGINE_PID:-}" ]; then
        kill -KILL "$ENGINE_PID" 2>/dev/null || true
        wait "$ENGINE_PID" 2>/dev/null || true
        ENGINE_PID=
    fi
}

start_engine "$LOG1"
wait_ready "$ENGINE_PID" "$LOG1"

old_rows=$(sink_rows)
if [ "$(count_sinks)" -ne 1 ]; then
    echo "FAIL: first launch did not create exactly one $SINK sink" >&2
    echo "sinks:" >&2
    sink_rows >&2
    exit 1
fi

# SIGKILL cannot run fxpw_stop(), so the module must still be present.
stop_engine
sleep 0.2
if [ "$(count_sinks)" -ne 1 ]; then
    echo "FAIL: sink did not survive SIGKILL, cannot test stale reaping" >&2
    exit 1
fi

start_engine "$LOG2"
wait_ready "$ENGINE_PID" "$LOG2"

new_rows=
for _ in $(seq 1 300); do
    rows=$(sink_rows)
    count=$(count_sinks)
    if [ "$count" -eq 1 ] && [ "$rows" != "$old_rows" ]; then
        new_rows=$rows
        break
    fi
    if ! kill -0 "$ENGINE_PID" 2>/dev/null; then
        echo "FAIL: second launch exited before reaping the stale sink:" >&2
        cat "$LOG2" >&2
        exit 1
    fi
    sleep 0.1
done

if [ -z "$new_rows" ]; then
    echo "FAIL: expected exactly one new $SINK sink after restart" >&2
    echo "old sink:" >&2
    printf '%s\n' "$old_rows" >&2
    echo "current sinks:" >&2
    sink_rows >&2
    exit 1
fi

stop_engine
echo "PASS: restart reaped the stale sink and left exactly one $SINK sink"
