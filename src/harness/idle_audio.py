"""Compare two real saucefxrt binaries on one hardware-free GitHub runner."""
import array
import contextlib
import hashlib
import json
import math
import os
from pathlib import Path
import statistics
import subprocess as sp
import sys
import tempfile
import time
import wave

RATE = 48000


def cmd(*args):
    return sp.check_output(args, text=True, stderr=sp.DEVNULL, timeout=8).strip()


def wait_for(description, fn):
    end = time.monotonic() + 12
    while time.monotonic() < end:
        try:
            if fn():
                return
        except (FileNotFoundError, KeyError, sp.CalledProcessError):
            pass
        time.sleep(.1)
    raise RuntimeError(description)


def feeders():
    graph = json.loads(cmd('pw-dump'))
    nodes = [o for o in graph if o['type'].endswith(':Node')]
    ids = {}
    for name in ('SauceFX', 'saucefx.capture', 'saucefx.render', 'dspout'):
        matches = [o['id'] for o in nodes if o['info']['props'].get('node.name') == name]
        if len(matches) != 1:
            raise KeyError(name)
        ids[name] = matches[0]
    active = [o['info'] for o in graph if o['type'].endswith(':Link') and o['info']['state'] == 'active']
    assert any(l['output-node-id'] == ids['SauceFX'] and l['input-node-id'] == ids['saucefx.capture'] for l in active), 'capture link absent'
    assert any(l['output-node-id'] == ids['saucefx.render'] and l['input-node-id'] == ids['dspout'] for l in active), 'output link absent'
    return sum(l['input-node-id'] == ids['SauceFX'] and l['output-node-id'] not in ids.values() for l in active)


def ticks(pid):
    fields = Path(f'/proc/{pid}/stat').read_text().rsplit(')', 1)[1].split()
    return int(fields[11]) + int(fields[12])


def idle_cpu(proc):
    measured = []
    for _ in range(3):
        assert proc.poll() is None and feeders() == 0
        start, cpu = time.monotonic(), ticks(proc.pid)
        time.sleep(2)
        measured.append(100 * (ticks(proc.pid) - cpu) / os.sysconf('SC_CLK_TCK') / (time.monotonic() - start))
    assert proc.poll() is None and feeders() == 0
    return measured  # Percent of one core, not percent of whole machine.


@contextlib.contextmanager
def child(args):
    p = sp.Popen(args, stdin=sp.DEVNULL)
    try:
        yield p
    finally:
        if p.poll() is None:
            p.terminate()
            try:
                p.wait(4)
            except sp.TimeoutExpired:
                p.kill()
                p.wait()


def stimulus(path):
    # Short opening note makes losing the beginning of a stream observable.
    a = array.array('h', (round(9000 * math.sin(2*math.pi*(880 if i < RATE*.16 else 440)*i/RATE)) for i in range(2*RATE)))
    with wave.open(str(path), 'wb') as f:
        f.setparams((1, 2, RATE, 0, 'NONE', ''))
        f.writeframes(a.tobytes())


def analyze(path):
    a = array.array('h'); a.frombytes(path.read_bytes())
    size = RATE // 100
    powers = [sum(v*v for v in a[i:i+size]) / (size*32768**2) for i in range(0, len(a)-size+1, size)]
    loud = [i for i, v in enumerate(powers) if v > .035**2]
    assert loud, 'no output reached the monitor'
    end = loud[-1]; tail = powers[end+10:end+35]
    assert len(tail) == 25, 'recording ended before the effect tail'
    tail_power = statistics.mean(tail)
    bin_size = RATE // 50
    basis = {f: [complex(math.cos(2*math.pi*f*i/RATE), math.sin(2*math.pi*f*i/RATE)) for i in range(bin_size)] for f in (440, 880)}
    notes = []
    for start in range(0, len(a)-bin_size+1, bin_size):
        block = a[start:start+bin_size]; energy = sum(x*x for x in block)
        if energy > bin_size*500**2:
            for f, weights in basis.items():
                projection = sum(x*b for x, b in zip(block, weights))
                if 2*abs(projection)**2 / (bin_size*energy) > .65:
                    notes.append(f); break
    assert notes and notes[0] == 880, 'opening note lost on wake'
    result = {'opening_seconds': notes.count(880)/50, 'body_seconds': notes.count(440)/50,
              'tail_dbfs': 10*math.log10(tail_power) if tail_power > 0 else None}
    assert result['opening_seconds'] >= .10 and result['body_seconds'] >= 1.70, result
    return result


def run_engine(engine, preset, output):
    home = Path(os.environ['HOME']); run = Path(os.environ['XDG_RUNTIME_DIR'])
    assert not Path('/dev/snd').exists() and os.environ['PULSE_SERVER'] == f'unix:{run}/pulse/native'
    cfg = Path(os.environ['XDG_CONFIG_HOME'])/'wireplumber/wireplumber.conf.d'
    cfg.mkdir(parents=True)
    (cfg/'90-isolated.conf').write_text('wireplumber.profiles = { main = { monitor.alsa = disabled monitor.bluez = disabled monitor.v4l2 = disabled monitor.libcamera = disabled } }\n')
    tone = home/'input.wav'; stimulus(tone)
    with contextlib.ExitStack() as stack:
        spawn = lambda args: stack.enter_context(child(args))
        spawn(['pipewire']); wait_for('private PipeWire did not start', lambda: (run/'pipewire-0').is_socket())
        spawn(['pipewire-pulse']); spawn(['wireplumber'])
        wait_for('private output unavailable', lambda: cmd('env', 'PULSE_PROP=media.category=Manager', 'pactl', 'load-module', 'module-null-sink', 'sink_name=dspout').isdigit())
        wait_for('private sink not default', lambda: cmd('pactl','set-default-sink','dspout') == '' and cmd('pactl','get-default-sink') == 'dspout')
        process = spawn([engine, '--out','dspout','--preset',preset]); wait_for('bridge did not connect', lambda: feeders() == 0)
        time.sleep(2)
        cold = idle_cpu(process); tones = []
        for i in range(2):
            raw = home/f'out{i}.raw'
            with child(['parecord','--raw','--rate=48000','--format=s16le','--channels=1','--latency-msec=40','--process-time-msec=20','--device=dspout.monitor',raw]):
                wait_for('monitor not recording',lambda: raw.is_file() and raw.stat().st_size > 8000)
                with child(['paplay','--device=SauceFX',tone]) as player:
                    wait_for('stimulus did not feed SauceFX',lambda: feeders() > 0)
                    assert player.wait(8) == 0
                time.sleep(1.1)
            tones.append(analyze(raw)); time.sleep(2)
        post = idle_cpu(process)
    result = {'cold_cpu':cold, 'post_cpu':post, 'tones':tones,'engine_sha256':hashlib.sha256(Path(engine).read_bytes()).hexdigest()}
    Path(output).write_text(json.dumps(result,indent=2)); print(output,result,flush=True)


def compare(control, candidate, preset, out):
    out.mkdir(parents=True,exist_ok=True); data = {'control':[], 'candidate':[]}
    for index, (label, binary) in enumerate([('control',control),('candidate',candidate),('candidate',candidate),('control',control)]):
        with tempfile.TemporaryDirectory(prefix='sfx-') as temp:
            root = Path(temp); run=root/'run';run.mkdir(mode=0o700);home=root/'home';home.mkdir()
            env = dict(os.environ, HOME=str(home), XDG_CONFIG_HOME=str(home/'.config'), XDG_STATE_HOME=str(home/'.state'), XDG_DATA_HOME=str(home/'.data'), XDG_CACHE_HOME=str(home/'.cache'), XDG_RUNTIME_DIR=str(run), PIPEWIRE_RUNTIME_DIR=str(run), PIPEWIRE_REMOTE='pipewire-0', PULSE_SERVER=f'unix:{run}/pulse/native', FXSOUND_REG_FILE=str(home/'registry'), GIO_USE_VFS='local')
            path=out/f'{index}-{label}.json'
            sp.run(['dbus-run-session','--',sys.executable,Path(__file__).resolve(),'--engine',binary,preset,path],env=env,check=True,timeout=90)
            data[label].append(json.loads(path.read_text()))
    summary = {}
    for name, rows in data.items():
        summary[name] = {k:statistics.median([v for row in rows for v in row[k]]) for k in ('cold_cpu','post_cpu')}
        tones=[t for row in rows for t in row['tones']];assert all(t['tail_dbfs'] is not None for t in tones), (name,tones)
        summary[name]['tail_dbfs'] = statistics.median(t['tail_dbfs'] for t in tones)
        summary[name]['wakeups_checked'] = len(tones)
    print('IDLE COMPARISON',json.dumps(summary),flush=True); (out/'summary.json').write_text(json.dumps(summary,indent=2))
    a,b=summary['control'],summary['candidate']; assert -88 < a['tail_dbfs'] < -30, ('control decay not measurable',a)
    for row in data['candidate']:
        assert all(abs(t['tail_dbfs'] - a['tail_dbfs']) <= 3 for t in row['tones']), ('effect decay changed',row)
    for stage in ('cold_cpu','post_cpu'):
        assert b[stage] < .75*a[stage], ('less than 25% idle CPU improvement',stage,a,b)


if __name__ == '__main__':
    if os.environ.get('GITHUB_ACTIONS') != 'true' or Path('/dev/snd').exists():
        sys.exit('This test requires a GitHub runner with no sound hardware.')
    if sys.argv[1] == '--engine':
        run_engine(*sys.argv[2:])
    else:
        compare(*(Path(arg).resolve() for arg in sys.argv[1:]))
