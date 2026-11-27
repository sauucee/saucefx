// Render the pinned pre-Rust frontend with independent factory inputs at the native viewport.
import assert from 'node:assert/strict';
import { readFileSync, writeFileSync } from 'node:fs';
import { createRequire } from 'node:module';
import path from 'node:path';
const [refDir, out, url] = process.argv.slice(2);
const { chromium } = createRequire(path.resolve(refDir, 'frontend/package.json'))('@playwright/test');
const browser = await chromium.launch();
try {
  const page = await browser.newPage({ viewport: { width: 1040, height: 568 }, deviceScaleFactor: 1 });
  await page.addInitScript(() => {
    window.saucefxHost = {
      onState(fn) { window.feedEQ = fn; return () => {}; },
      onSpectrum() { return () => {}; }, onToast() { return () => {}; },
      command(type, payload) { if (type === 'eq-band') window.lastBandEdit = payload; },
    };
  });
  await page.goto(url); await page.waitForFunction(() => typeof window.feedEQ === 'function');
  await page.evaluate(() => document.fonts.ready);
  for (const [label, file] of [['Music','2.fac'], ['Movies','10.fac'], ['Single-band','Default.fac']]) {
    const fac = readFileSync(path.join(refDir, 'presets/Factsoft', file), 'utf8');
    const anchors = [...fac.matchAll(/^\s*([-+\d.]+)\s*:\s*Boost\/Cut/gm)].map(x => Number(x[1]));
    assert(anchors.length > 0);
    const levels = Array.from({length:31}, (_,i) => {
      const pos=i*(anchors.length-1)/30, lo=Math.floor(pos);
      return anchors[lo]+(pos-lo)*(anchors[Math.min(lo+1,anchors.length-1)]-anchors[lo]);
    });
    await page.evaluate(({label,levels}) => window.feedEQ({
      power:true, presetId:label, presets:[{id:label, name:label, group:'factory'}],
      eq:levels, effects:[0,0,0,0,0], autostart:false, theme:'dark', customAccent:true,
      accent:'#ff2e57', output:'system', version:'0.2.4',
    }), {label,levels});
    const chart = page.locator('.eq-chart'); await chart.waitFor();
    if (label === 'Single-band') {
      assert(levels.every(x => x === 0));
      const box = await chart.boundingBox();
      const x=box.x+(17.5/31)*box.width;
      await page.mouse.move(x,box.y+box.height/2); await page.mouse.down();
      await page.mouse.move(x,box.y+1,{steps:8}); await page.mouse.up();
      await page.waitForFunction(() => window.lastBandEdit?.index===17 && window.lastBandEdit.value===12);
      levels[17]=12;
    }
    await page.mouse.move(3,3);
    // Read the actual path endpoints, not slider display text which may be rounded.
    await page.waitForFunction(expected => {
      const values = document.querySelector('.eq-curve').getAttribute('d').match(/-?[\d.]+/g).map(Number);
      return values.length === 182 && expected.every((v,i) => Math.abs(12-(values[i*6+1]-12)*24/176-v) < .001);
    }, levels);
    const native=JSON.parse(readFileSync(path.join(out,label+'-native.json'),'utf8'));
    assert.equal(native.length,31);assert(levels.every((v,i)=>Math.abs(v-native[i])<.00001),label+': native inputs differ');
    const rect=await chart.boundingBox(); assert(rect && rect.width>500 && rect.height>100);
    writeFileSync(path.join(out,label+'-chart.json'),JSON.stringify(rect));
    await page.screenshot({path:path.join(out,label+'-original.png')});
    console.log(`${label}: actual pre-Rust path, packaged native session and factory/drag inputs agree`);
  }
} finally { await browser.close(); }
