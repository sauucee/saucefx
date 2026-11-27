"""Compare aligned output for every factory preset, including quiet wake after idle."""
import array
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile


engine, presets, output = map(lambda arg: Path(arg).resolve(), sys.argv[1:])
files = sorted(presets.glob('*.fac'))
assert files and (presets / '2.fac') in files
results = {}
for preset in files:
    with tempfile.TemporaryDirectory() as tmp:
        audio = []
        for mode in ('reference', 'candidate'):
            raw = Path(tmp) / f'{mode}.f32'
            env = dict(os.environ, HOME=tmp, FXSOUND_REG_FILE=str(Path(tmp) / f'{mode}.registry'))
            subprocess.run([engine, preset, mode, raw], cwd=tmp, env=env, check=True, timeout=60)
            samples = array.array('f')
            with raw.open('rb') as stream:
                samples.fromfile(stream, raw.stat().st_size // samples.itemsize)
            audio.append(samples)
        assert len(audio[0]) == len(audio[1]) and len(audio[0]) > 48000 * 2 * 10
        def difference(reference, actual):
            assert len(reference) == len(actual)
            phases = ('cold', 'first_tone', 'decay', 'quiet_wake')
            offset, peaks = 0, {}
            for phase, seconds in zip(phases, (8, 2, 8, 2)):
                count = (seconds * 48000 // 256) * 256 * 2
                peak = max(abs(a-b) for a, b in zip(reference[offset:offset+count], actual[offset:offset+count]))
                peaks[phase] = peak; offset += count
            assert offset == len(reference)
            return peaks

        peaks = difference(*audio)
        results[preset.name] = max(peaks.values())
        if results[preset.name] >= 1 / 32768:
            print('PRESET_DIAG', preset.name, 'production', peaks, flush=True)
            for setting in ('repeat-reference', '1000', '2000', '4000'):
                # Compare unmodified against itself first. A timing-dependent
                # reference is not proof of a regression caused by the gate.
                raw = Path(tmp) / f'{setting}.f32'
                args = ['reference', raw] if setting == 'repeat-reference' else ['candidate', raw, setting]
                env = dict(os.environ, HOME=tmp, FXSOUND_REG_FILE=str(Path(tmp) / f'{setting}.registry'))
                subprocess.run([engine, preset, *args], cwd=tmp, env=env, check=True, timeout=60)
                other = array.array('f')
                with raw.open('rb') as stream: other.fromfile(stream, raw.stat().st_size // other.itemsize)
                print('PRESET_DIAG', preset.name, setting, difference(audio[0], other), flush=True)
output.parent.mkdir(parents=True, exist_ok=True)
output.write_text(json.dumps(results, indent=2))
print('FACTORY_WAVEFORMS', json.dumps(results), flush=True)
# Absolute error across every stereo sample, not just average tone level.
# A difference larger than one 16-bit PCM step is a regression to investigate.
assert all(error < 1 / 32768 for error in results.values()), results
