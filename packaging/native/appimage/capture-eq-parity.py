"""Capture the actual packaged UI in the smoke test's private X11 session."""
import json
import math
import pathlib
import re
import subprocess
import sys
import time

state, factory, out = map(pathlib.Path, sys.argv[1:])
out.mkdir(parents=True, exist_ok=True)
run = lambda *args: subprocess.check_output(list(map(str, args)), text=True, timeout=10).strip()
windows = run('xdotool', 'search', '--onlyvisible', '--name', '^SauceFX').splitlines()
assert len(windows) == 1, windows
wid = windows[0]
geom = run('xwininfo', '-id', wid)
def field(key):
    match = re.search(re.escape(key) + r':\s*(-?\d+)', geom)
    assert match, (key, geom)
    return int(match[1])

x, y = field('Absolute upper-left X'), field('Absolute upper-left Y')
width, height = field('Width'), field('Height')
assert (width, height) == (1040, 568), (width, height)
files = sorted(factory.glob('*.fac'), key=lambda p: p.name.lower())


def read_state():
    data = dict(l.split('=', 1) for l in state.read_text().splitlines() if '=' in l)
    return data['factory-preset'], [float(data[f'eq{i}gain']) for i in range(31)]


def wait_state(name, expected):
    end = time.monotonic() + 12
    actual = None
    while time.monotonic() < end:
        try:
            actual = read_state()
        except (KeyError, ValueError):  # May read during the periodic settings write.
            time.sleep(.1); continue
        if actual[0] == name and all(math.isclose(a, b, abs_tol=.00001) for a, b in zip(actual[1], expected)):
            return actual[1]
        time.sleep(.1)
    raise AssertionError(f'{name}: expected {expected}, saved {actual}')


def select(file):
    anchors = list(map(float, re.findall(r'^\s*([-+\d.]+)\s*:\s*Boost/Cut', (factory/file).read_text(), re.M)))
    assert anchors
    values = []
    for i in range(31):
        pos = i * (len(anchors)-1) / 30
        lo = int(pos)
        values.append(anchors[lo] + (pos-lo)*(anchors[min(lo+1, len(anchors)-1)]-anchors[lo]))
    run('xdotool', 'mousemove', '--window', wid, 140, 100, 'click', 1)
    time.sleep(.2)
    run('xdotool', 'mousemove', '--window', wid, 3, 3)
    index = [p.name for p in files].index(file)
    run('xdotool', 'key', '--delay', 55, 'Home', *(['Down']*index), 'Return')
    return wait_state(file, values)


def capture(label, gains):
    time.sleep(.3)
    run('import', '-silent', '-window', 'root', '-crop', f'{width}x{height}+{x}+{y}', '+repage', out/f'{label}-native.png')
    (out/f'{label}-native.json').write_text(json.dumps(gains))
    print(label, 'native persisted gains:', gains, flush=True)


for label, file in [('Music', '2.fac'), ('Movies', '10.fac')]:
    capture(label, select(file))
flat = select('Default.fac')
assert flat == [0.0]*31
# Drag the 1 kHz thumb from the centre to +12 dB; leave the plot before capture.
run('xdotool', 'mousemove', '--window', wid, 694, 434, 'mousedown', 1,
    'sleep', '.1', 'mousemove', '--window', wid, 694, 344, 'sleep', '.1',
    'mouseup', 1, 'mousemove', '--window', wid, 3, 3)
flat[17] = 12.0
capture('Single-band', wait_state('Default.fac', flat))
