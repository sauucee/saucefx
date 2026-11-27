"""Check the packaged app under the private X11 or Wayland test compositor."""
import json
import os
import pathlib
import re
import subprocess
import sys
import time

log, state = map(pathlib.Path, sys.argv[1:])


def x(*args):
    return subprocess.run(['xdotool', *map(str, args)], text=True,
                          capture_output=True, timeout=5, check=True).stdout.strip()


def sway(*args):
    out = subprocess.check_output(['swaymsg', '-r', *args], text=True, timeout=5)
    return json.loads(out)


def visible_wayland():
    workspaces = {w['name'] for w in sway('-t', 'get_workspaces') if w['visible']}

    def windows(node, on_screen=False):
        if node['type'] == 'workspace':
            on_screen = node['name'] in workspaces
        if (on_screen and node.get('shell') == 'xdg_shell'
                and (node.get('name') or '').startswith('SauceFX')):
            yield node['id']
        for child in node['nodes'] + node['floating_nodes']:
            yield from windows(child, on_screen)

    return list(windows(sway('-t', 'get_tree')))


def visible():
    if os.environ.get('SAUCEFX_TEST_WAYLAND') == '1':
        return visible_wayland()
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
if os.environ.get('SAUCEFX_TEST_WAYLAND') == '1':
    assert sway('workspace visualizer-away')[0]['success']
    wait('workspace switch did not hide native window', lambda: not visible())
    quiet('Wayland hidden workspace')
    resume('return to Wayland workspace', lambda: sway(f'[con_id={wid}] focus'))
    assert not failures, 'Wayland background spectrum still sampled: ' + ', '.join(failures)
    sys.exit(0)

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
