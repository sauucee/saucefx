"""Exercise the shipped binary against the private X server in smoke-test.sh."""
import pathlib
import re
import subprocess
import sys
import time

log, state = map(pathlib.Path, sys.argv[1:])


def x(*args):
    return subprocess.run(['xdotool', *map(str, args)], text=True,
                          capture_output=True, timeout=5, check=True).stdout.strip()


def visible():
    try:
        return x('search', '--onlyvisible', '--name', '^SauceFX').splitlines()
    except subprocess.CalledProcessError as e:
        if e.returncode != 1:
            raise
        return []


def samples():
    return log.read_text().splitlines().count('saucefx: spectrum sample')


def wait(description, condition):
    end = time.monotonic() + 10
    while time.monotonic() < end:
        if condition():
            return
        time.sleep(.05)
    raise AssertionError(description)


def resume(label, action):
    before = samples()
    action()
    wait(label + ': no visible window', visible)
    wait(label + ': no fresh samples', lambda: samples() >= before + 3)
    print(label + ': sampling resumed', flush=True)
    return visible()[0]


failures = []


def quiet(label):
    time.sleep(.2)  # Allow the final in-flight window event to finish.
    before = samples()
    time.sleep(.8)
    delta = samples() - before
    print(f'{label}: {delta} samples during quiet window', flush=True)
    if delta:
        failures.append(label)


assert 'saucefx: spectrum trace ready' in log.read_text(), 'host did not enable tracing'
assert not visible(), 'autostart unexpectedly mapped the window'
quiet('hidden at login')
show = state / 'show.request'
wid = resume('show after login', show.touch)
x('windowminimize', wid)
wait('window manager did not minimize', lambda: not visible())
quiet('minimized')
wid = resume('restore from taskbar', lambda: x('windowactivate', '--sync', wid))
geometry = x('getwindowgeometry', '--shell', wid)
match = re.search(r'^WIDTH=(\d+)$', geometry, re.M)
assert match, f'window width missing: {geometry}'
width = int(match[1])
# Click SauceFX's own close button; do not destroy its native window with XKillClient.
x('mousemove', '--window', wid, width - 26, 25, 'click', '1')
wait('close-to-background did not hide', lambda: not visible())
quiet('closed to background')
resume('reopen from background', show.touch)
assert not failures, 'background spectrum still sampled: ' + ', '.join(failures)
