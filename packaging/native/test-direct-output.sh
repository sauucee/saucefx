#!/usr/bin/env bash
# Run the host's real output-restore regression on the existing private-audio
# harness. Never launch this test against the desktop PipeWire session.
set -euo pipefail
SCRIPT_DIR=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)
ROOT=$(cd -- "$SCRIPT_DIR/../.." && pwd)
if [ "${SAUCEFX_TEST_ISOLATED:-}" != 1 ]; then
    exec bash "$ROOT/src/harness/outputroutingtest.sh" "$SCRIPT_DIR/test-direct-output.sh"
fi
exec cargo test --manifest-path "$ROOT/Cargo.toml" --locked --jobs 2 \
    -p saucefx-host --no-default-features --features renderer-skia \
    fxpw::tests::direct_playback_startup_restores_the_saved_engine_output \
    -- --exact --ignored --test-threads=1
