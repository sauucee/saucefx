/**
 * Interface check for the built site. Run against `vite preview`:
 *   node scripts/verify.mjs http://127.0.0.1:4173
 */
import { chromium } from 'playwright';

const URL = process.argv[2] ?? 'http://127.0.0.1:4173';
const OUT = process.argv[3] ?? '/tmp/shots';

const browser = await chromium.launch();
const results = [];
const fail = (name, detail) => results.push({ name, ok: false, detail });
const pass = (name, detail = '') => results.push({ name, ok: true, detail });

/* ---------------------------------------------------------------- desktop */
const context = await browser.newContext({
  viewport: { width: 1440, height: 900 },
  deviceScaleFactor: 1,
});
const page = await context.newPage();
await context.grantPermissions(['clipboard-read', 'clipboard-write']);

const consoleErrors = [];
page.on('console', (msg) => {
  if (msg.type() === 'error') consoleErrors.push(msg.text());
});
page.on('pageerror', (err) => consoleErrors.push(`pageerror: ${err.message}`));

await page.goto(URL, { waitUntil: 'networkidle' });
await page.waitForTimeout(3600); // preloader is ~2.5s

const preloaderGone = (await page.locator('.pre').count()) === 0;
preloaderGone ? pass('preloader unmounts') : fail('preloader unmounts', 'still in DOM');

const heroBox = await page.locator('#hero-title').boundingBox();
const winBox = await page.locator('.hero__window').boundingBox();
if (!heroBox || !winBox) fail('hero layers', 'missing box');
else {
  const heroWider = heroBox.width > winBox.width;
  heroWider
    ? pass('headline is wider than the app window', `${Math.round(heroBox.width)}px vs ${Math.round(winBox.width)}px`)
    : fail('headline is wider than the app window', `${heroBox.width} vs ${winBox.width}`);
  const overlaps = winBox.y < heroBox.y + heroBox.height && winBox.y + winBox.height > heroBox.y;
  overlaps ? pass('window overlays the headline') : fail('window overlays the headline', 'no overlap');
}

const overflow = await page.evaluate(() => {
  const doc = document.documentElement;
  const offenders = [];
  document.querySelectorAll('*').forEach((el) => {
    const r = el.getBoundingClientRect();
    if (r.width > 0 && r.right > doc.clientWidth + 2) {
      offenders.push(`${el.tagName}.${String(el.className).slice(0, 48)} right=${Math.round(r.right)}`);
    }
  });
  return {
    scrollWidth: doc.scrollWidth,
    clientWidth: doc.clientWidth,
    bodyScroll: document.body.scrollWidth,
    offenders: offenders.slice(0, 8),
  };
});
if (overflow.scrollWidth <= overflow.clientWidth + 1) pass('no horizontal overflow (desktop)');
else fail('no horizontal overflow (desktop)', JSON.stringify(overflow));

/* stats counter finished */
await page.locator('.stats').scrollIntoViewIfNeeded();
await page.waitForTimeout(2000);
const statValue = await page.locator('[data-count]').first().textContent();
statValue === '05' ? pass('stat counters land on real values', statValue) : fail('stat counters', `got ${statValue}`);

/* pinned rack steps */
await page.evaluate(() => window.scrollTo(0, 0));
await page.waitForTimeout(400);
const rack = page.locator('#rack');
const rackTop = await page.evaluate(() => {
  const el = document.querySelector('#rack');
  return el ? el.getBoundingClientRect().top + window.scrollY : -1;
});
const rackHeight = await page.evaluate(() => document.querySelector('#rack')?.offsetHeight ?? 0);
await page.evaluate((y) => window.scrollTo(0, y), rackTop + rackHeight * 0.55);
await page.waitForTimeout(900);
const rackTitle = await page.locator('#rack .rack__title').first().textContent();
rackTitle && rackTitle !== 'Clarity'
  ? pass('rack advances with scroll', rackTitle)
  : fail('rack advances with scroll', `title ${rackTitle}`);

/* EQ drag */
await page.locator('#playground').scrollIntoViewIfNeeded();
await page.waitForTimeout(500);
const fader = page.locator('.eq__knob').nth(20);
const before = await fader.getAttribute('aria-valuenow');
const box = await fader.boundingBox();
const plotBox = await page.locator('.eq__plot').boundingBox();
if (box && plotBox) {
  await page.mouse.move(box.x + box.width / 2, box.y + box.height / 2);
  await page.mouse.down();
  await page.mouse.move(plotBox.x + plotBox.width * 0.67, plotBox.y + 24, { steps: 12 });
  await page.mouse.up();
}
const after = await fader.getAttribute('aria-valuenow');
before !== after ? pass('EQ fader responds to drag', `${before} → ${after}`) : fail('EQ fader drag', `stuck at ${before}`);

/* preset chips animate the curve */
const curvePath = page.locator('#playground [data-curve]');
const pathBefore = await curvePath.getAttribute('d');
await page.getByRole('button', { name: 'Bass', exact: true }).click();
await page.waitForTimeout(1000);
const pathAfter = await curvePath.getAttribute('d');
pathBefore !== pathAfter ? pass('preset changes the curve') : fail('preset changes the curve');

/* install tabs, copy and the feature rail */
await page.locator('#install').scrollIntoViewIfNeeded();
await page.waitForTimeout(500);
await page.getByRole('tab', { name: 'Fedora RPM' }).click();
await page.waitForTimeout(250);
const rpmSelected = (await page.getByRole('tab', { name: 'Fedora RPM' }).getAttribute('aria-selected')) === 'true';
const rpmText = (await page.locator('#install-panel').textContent()) ?? '';
const rpmTab = rpmSelected && rpmText.includes('dnf install') && !rpmText.includes('flatpak install');
rpmTab ? pass('install tabs switch command sets') : fail('install tabs', `selected=${rpmSelected}`);

await page.getByRole('tab', { name: 'Flatpak' }).click();
await page.getByRole('button', { name: 'Copy command 1' }).click();
await page.waitForTimeout(200);
const copiedVisible = await page.getByRole('button', { name: 'Copied' }).first().isVisible();
const clipboard = await page.evaluate(() => navigator.clipboard.readText().catch(() => ''));
const copyWorks = copiedVisible && clipboard.includes('flatpak install');
copyWorks
  ? pass('copy puts the install commands on the clipboard')
  : fail('copy', `confirmed=${copiedVisible} clipboard=${JSON.stringify(clipboard.slice(0, 40))}`);

const rail = page.locator('.rail__viewport');
const railBefore = await rail.evaluate((el) => el.scrollLeft);
await page.getByRole('button', { name: 'Next feature' }).click();
await page.waitForTimeout(700);
const railAfter = await rail.evaluate((el) => el.scrollLeft);
railAfter > railBefore ? pass('feature rail advances with its arrow', `${railBefore} → ${railAfter}`) : fail('feature rail arrow');

/* theme + accent */
await page.evaluate(() => window.scrollTo(0, 0));
await page.waitForTimeout(300);
const darkBg = await page.evaluate(() => getComputedStyle(document.body).backgroundColor);
await page.getByRole('button', { name: 'Soft light theme' }).click({ force: true });
await page.waitForTimeout(500);
const lightBg = await page.evaluate(() => getComputedStyle(document.body).backgroundColor);
const lightAttr = await page.evaluate(() => document.documentElement.dataset.theme);
darkBg !== lightBg && lightAttr === 'light'
  ? pass('theme toggle switches the whole site', `${darkBg} → ${lightBg}`)
  : fail('theme toggle', `${darkBg} → ${lightBg} (${lightAttr})`);

const accentBefore = await page.evaluate(() =>
  getComputedStyle(document.documentElement).getPropertyValue('--accent').trim(),
);
await page.getByRole('button', { name: 'Amber accent' }).click({ force: true });
await page.waitForTimeout(400);
const accentAfter = await page.evaluate(() =>
  getComputedStyle(document.documentElement).getPropertyValue('--accent').trim(),
);
accentBefore !== accentAfter ? pass('accent picker recolours', `${accentBefore} → ${accentAfter}`) : fail('accent picker');

await page.screenshot({ path: `${OUT}/desktop-light.png`, fullPage: false });
await page.evaluate(() => {
  document.documentElement.dataset.theme = 'dark';
  document.documentElement.dataset.accent = 'pulse';
});
await page.waitForTimeout(300);
await page.screenshot({ path: `${OUT}/desktop-hero.png` });
await page.screenshot({ path: `${OUT}/desktop-full.png`, fullPage: true });

/* --------------------------------------------------------------- reduced */
const reducedContext = await browser.newContext({
  viewport: { width: 1440, height: 900 },
  reducedMotion: 'reduce',
});
const reduced = await reducedContext.newPage();
await reduced.goto(URL, { waitUntil: 'networkidle' });
await reduced.waitForTimeout(700);
const reducedPreloader = await reduced.locator('.pre').count();
const reducedHeroOpacity = await reduced.evaluate(
  () => getComputedStyle(document.querySelector('#hero-title')).opacity,
);
reducedPreloader === 0 && Number(reducedHeroOpacity) > 0.9
  ? pass('reduced motion: no curtain, hero visible')
  : fail('reduced motion', `preloader=${reducedPreloader} opacity=${reducedHeroOpacity}`);
await reduced.screenshot({ path: `${OUT}/reduced-hero.png` });
await reducedContext.close();

/* ----------------------------------------------------------------- mobile */
const mobileContext = await browser.newContext({
  viewport: { width: 390, height: 844 },
  deviceScaleFactor: 2,
  isMobile: true,
  hasTouch: true,
});
const mobile = await mobileContext.newPage();
await mobile.addInitScript(() => {
  window.__cls = 0;
  new PerformanceObserver((list) => {
    for (const entry of list.getEntries()) {
      if (!entry.hadRecentInput) window.__cls += entry.value;
    }
  }).observe({ type: 'layout-shift', buffered: true });
});
await mobile.goto(URL, { waitUntil: 'networkidle' });
await mobile.waitForTimeout(3600);
await mobile.getByRole('button', { name: 'Open menu' }).click();
await mobile.waitForTimeout(300);
const menuOpen = await mobile.locator('#drawer').isVisible();
await mobile.keyboard.press('Escape');
await mobile.waitForTimeout(300);
const menuClosed = (await mobile.locator('#drawer').count()) === 0;
const menuScrollRestored = await mobile.evaluate(
  () => document.documentElement.style.overflow === '',
);
menuOpen && menuClosed && menuScrollRestored
  ? pass('mobile menu opens, closes on Escape, restores scroll')
  : fail('mobile menu', `open=${menuOpen} closed=${menuClosed} scroll=${menuScrollRestored}`);
const mOverflow = await mobile.evaluate(() => ({
  scrollWidth: document.documentElement.scrollWidth,
  clientWidth: document.documentElement.clientWidth,
}));
mOverflow.scrollWidth <= mOverflow.clientWidth + 1
  ? pass('no horizontal overflow (mobile)')
  : fail('no horizontal overflow (mobile)', JSON.stringify(mOverflow));

const mHero = await mobile.evaluate(() => {
  const word = document.querySelector('#hero-title').getBoundingClientRect();
  const win = document.querySelector('.hero__window').getBoundingClientRect();
  return { wordBottom: word.bottom, winTop: win.top };
});
mHero.winTop >= mHero.wordBottom - 20
  ? pass('mobile hero stacks word above window')
  : fail('mobile hero stacking', JSON.stringify(mHero));

const cls = await mobile.evaluate(() => Number((window.__cls ?? 0).toFixed(4)));
cls < 0.02 ? pass('cumulative layout shift', String(cls)) : fail('cumulative layout shift', String(cls));
await mobile.screenshot({ path: `${OUT}/mobile-hero.png` });
await mobile.screenshot({ path: `${OUT}/mobile-full.png`, fullPage: true });
await mobileContext.close();

/* ------------------------------------------------------------------ a11y */
const a11yPage = await context.newPage();
await a11yPage.goto(URL, { waitUntil: 'networkidle' });
await a11yPage.waitForTimeout(3200);
const a11y = await a11yPage.evaluate(() => {
  const issues = [];
  if (document.querySelectorAll('h1').length !== 1) issues.push('h1 count != 1');
  const imgs = [...document.querySelectorAll('img')];
  imgs.forEach((img) => {
    if (!img.hasAttribute('alt')) issues.push(`img without alt: ${img.src.slice(-40)}`);
  });
  const sliders = [...document.querySelectorAll('[role="slider"]')];
  if (sliders.length !== 31) issues.push(`expected 31 sliders, got ${sliders.length}`);
  sliders.forEach((s) => {
    if (!s.hasAttribute('aria-valuenow') || !s.hasAttribute('aria-valuetext')) issues.push('slider missing value');
    if (s.getAttribute('tabindex') !== '0') issues.push('slider not focusable');
  });
  const buttons = [...document.querySelectorAll('button')];
  const unlabelled = buttons.filter((b) => !b.textContent.trim() && !b.getAttribute('aria-label'));
  if (unlabelled.length) issues.push(`${unlabelled.length} buttons without accessible name`);
  const links = [...document.querySelectorAll('a')];
  const emptyLinks = links.filter((a) => !a.textContent.trim() && !a.getAttribute('aria-label'));
  if (emptyLinks.length) issues.push(`${emptyLinks.length} links without accessible name`);
  return { issues, sliders: sliders.length, imgs: imgs.length };
});
a11y.issues.length === 0
  ? pass('a11y structural checks', `${a11y.sliders} sliders, ${a11y.imgs} images`)
  : fail('a11y structural checks', a11y.issues.join('; '));

/* heading outline */
const headings = await a11yPage.evaluate(() =>
  [...document.querySelectorAll('h1,h2,h3')].map((h) => `${h.tagName}:${h.textContent.trim().slice(0, 34)}`),
);
pass('heading outline', headings.join(' | ').slice(0, 220));

/* contrast: text tokens and every accent against the page background */
const contrast = await a11yPage.evaluate(() => {
  const parse = (c) => c.match(/[\d.]+/g).map(Number);
  const lum = ([r, g, b]) => {
    const f = (v) => {
      const s = v / 255;
      return s <= 0.03928 ? s / 12.92 : ((s + 0.055) / 1.055) ** 2.4;
    };
    return 0.2126 * f(r) + 0.7152 * f(g) + 0.0722 * f(b);
  };
  const ratio = (a, b) => {
    const la = lum(parse(a));
    const lb = lum(parse(b));
    return (Math.max(la, lb) + 0.05) / (Math.min(la, lb) + 0.05);
  };
  // Resolve any token through a real element so color-mix()/var() resolve.
  const probe = document.createElement('span');
  probe.style.position = 'absolute';
  probe.style.opacity = '0';
  document.body.appendChild(probe);
  const canvas = document.createElement('canvas');
  canvas.width = canvas.height = 1;
  const ctx = canvas.getContext('2d', { willReadFrequently: true });
  const read = (token) => {
    probe.style.color = `var(${token})`;
    // Chrome may report color-mix() results in oklab(); paint one pixel to get sRGB.
    ctx.clearRect(0, 0, 1, 1);
    ctx.fillStyle = getComputedStyle(probe).color;
    ctx.fillRect(0, 0, 1, 1);
    const [r, g, b] = ctx.getImageData(0, 0, 1, 1).data;
    return `rgb(${r}, ${g}, ${b})`;
  };

  const out = {};
  const accents = ['pulse', 'amber', 'lime', 'mint', 'cyan', 'violet'];
  for (const theme of ['dark', 'light']) {
    document.documentElement.dataset.theme = theme;
    const bg = read('--bg');
    const accentMin = {};
    for (const accent of accents) {
      document.documentElement.dataset.accent = accent;
      accentMin[accent] = Number(ratio(read('--accent-text'), bg).toFixed(2));
    }
    document.documentElement.dataset.accent = 'pulse';
    out[theme] = {
      ink: Number(ratio(read('--ink'), bg).toFixed(2)),
      inkSoft: Number(ratio(read('--ink-soft'), bg).toFixed(2)),
      muted: Number(ratio(read('--muted'), bg).toFixed(2)),
      accentText: accentMin,
    };
  }
  probe.remove();
  document.documentElement.dataset.theme = 'dark';
  return out;
});
const contrastFails = [];
for (const [theme, values] of Object.entries(contrast)) {
  if (values.ink < 7) contrastFails.push(`${theme} ink ${values.ink}`);
  if (values.inkSoft < 4.5) contrastFails.push(`${theme} ink-soft ${values.inkSoft}`);
  if (values.muted < 4.5) contrastFails.push(`${theme} muted ${values.muted}`);
  for (const [accent, value] of Object.entries(values.accentText)) {
    if (value < 4.5) contrastFails.push(`${theme}/${accent} accent-text ${value}`);
  }
}
contrastFails.length === 0
  ? pass('text contrast AA in both themes', JSON.stringify(contrast))
  : fail('text contrast', contrastFails.join('; '));

consoleErrors.length === 0
  ? pass('no console errors')
  : fail('console errors', consoleErrors.slice(0, 4).join(' || '));

await context.close();
await browser.close();

const failures = results.filter((r) => !r.ok);
for (const r of results) {
  console.log(`${r.ok ? 'PASS' : 'FAIL'}  ${r.name}${r.detail ? `  — ${r.detail}` : ''}`);
}
console.log(`\n${results.length - failures.length}/${results.length} passed`);
process.exit(failures.length ? 1 : 0);
