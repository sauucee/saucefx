import assert from 'node:assert/strict';
import { test } from 'node:test';
import { createSignalEngine } from '../src/lib/signal.ts';

test('31 bands, values in [0,1], moving over time', () => {
  const e = createSignalEngine(31);
  e.tick(0.016);
  e.tick(0.016);
  const before = Float32Array.from(e.bands);
  e.tick(0.2);
  assert.equal(e.bands.length, 31);
  for (const v of e.bands) assert.ok(v >= 0 && v <= 1, `band out of range: ${v}`);
  assert.ok(e.level >= 0 && e.level <= 1);
  assert.notDeepEqual([...e.bands], [...before], 'bands must evolve over time');
});

test('demo mode needs no source; injected source drives bands (mic path)', () => {
  const e = createSignalEngine(31);
  e.tick(0.016);
  assert.equal(e.mode, 'demo');
  e.setSource((target, t) => {
    for (let i = 0; i < target.length; i++) target[i] = t > 0 ? 0.5 : 0;
  });
  e.mode = 'mic';
  e.tick(0.1);
  assert.ok(Math.abs(e.bands[7] - 0.5) < 0.5, 'source target must reach bands via easing');
  e.mode = 'demo'; // switching back drops the injected source
  e.tick(0.016);
});
