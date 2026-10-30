/**
 * Renders public/og-image.png (1200×630) with the site's own font and palette,
 * so the social card can never drift from the design.
 *
 *   npx vite preview --port 4173      # serve public/fonts
 *   node scripts/og.mjs
 */
import { chromium } from 'playwright';

const BASE = process.argv[2] ?? 'http://localhost:4173';

const html = `<!doctype html>
<html><head><meta charset="utf-8">
<style>
  @font-face {
    font-family: 'Figtree';
    src: url('${BASE}/fonts/figtree-latin-wght-normal.woff2') format('woff2-variations');
    font-weight: 300 900;
  }
  * { margin: 0; box-sizing: border-box; }
  body {
    width: 1200px; height: 630px; overflow: hidden;
    background: #07070a; color: #f5f3f6;
    font-family: 'Figtree', sans-serif;
    position: relative;
    display: flex; flex-direction: column; justify-content: space-between;
    padding: 56px 64px 48px;
  }
  .glow {
    position: absolute; width: 900px; height: 560px; left: -180px; top: -260px;
    background: radial-gradient(circle, rgba(255,46,99,.42) 0%, transparent 66%);
    filter: blur(40px);
  }
  .grid {
    position: absolute; inset: 0;
    background-image: linear-gradient(rgba(255,255,255,.055) 1px, transparent 1px),
                      linear-gradient(90deg, rgba(255,255,255,.055) 1px, transparent 1px);
    background-size: 60px 60px;
    mask-image: radial-gradient(ellipse at 50% 40%, #000 20%, transparent 78%);
  }
  .top { display: flex; align-items: center; gap: 14px; position: relative; }
  .mark { display: block; width: 34px; height: 34px; }
  .brand { font-size: 19px; font-weight: 650; letter-spacing: .01em; }
  .tag {
    margin-left: auto; font-family: ui-monospace, monospace; font-size: 12.5px;
    letter-spacing: .16em; color: #8d8998; text-transform: uppercase;
  }
  .mid { position: relative; }
  h1 {
    font-size: 168px; font-weight: 880; letter-spacing: -0.055em; line-height: .84;
    background: linear-gradient(96deg, #ffffff 0%, #f7e9ee 46%, #ff2e63 100%);
    -webkit-background-clip: text; background-clip: text; color: transparent;
  }
  p.sub { margin-top: 22px; font-size: 33px; font-weight: 560; letter-spacing: -0.02em; color: #ded9e2; }
  .bottom { display: flex; align-items: flex-end; justify-content: space-between; position: relative; }
  .chips { display: flex; gap: 9px; }
  .chip {
    border: 1px solid rgba(255,255,255,.16); border-radius: 999px;
    padding: 8px 15px; font-size: 14px; color: #c8c4d0;
  }
  .chip.hot { border-color: transparent; background: #ff2e63; color: #16040a; font-weight: 650; }
  .repo { font-family: ui-monospace, monospace; font-size: 12.5px; color: #8d8998; letter-spacing: .04em; }
</style></head>
<body>
  <div class="glow"></div><div class="grid"></div>
  <div class="top">
    <img class="mark" src="${BASE}/favicon.svg" alt="" width="34" height="34">
    <span class="brand">SauceFX</span>
    <span class="tag">Linux · PipeWire</span>
  </div>

  <div class="mid">
    <h1>SAUCEFX</h1>
    <p class="sub">Sound EQ for Linux.</p>
  </div>

  <div class="bottom">
    <div class="chips">
      <span class="chip hot">31-band EQ</span>
      <span class="chip">5 effects</span>
      <span class="chip">Flatpak + RPM</span>
      <span class="chip">AGPL-3.0-or-later</span>
    </div>
    <span class="repo">github.com/sauucee/saucefx</span>
  </div>
</body></html>`;

const browser = await chromium.launch();
const page = await browser.newPage({ viewport: { width: 1200, height: 630 }, deviceScaleFactor: 1 });
await page.setContent(html, { waitUntil: 'networkidle' });
await page.evaluate(() => document.fonts.ready);
await page.waitForTimeout(250);
await page.screenshot({ path: 'public/og-image.png' });
await browser.close();
console.log('wrote public/og-image.png');
