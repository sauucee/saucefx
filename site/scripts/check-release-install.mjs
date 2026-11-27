import assert from 'node:assert/strict';
import { chromium } from 'playwright';

const [base, version, out] = process.argv.slice(2);
assert(base && version && out, 'provide the site URL, Cargo version and screenshot directory');
const browser = await chromium.launch();
try {
  for (const [label, viewport] of [['desktop', { width: 1440, height: 900 }], ['mobile', { width: 390, height: 844 }]]) {
    const page = await browser.newPage({ viewport, reducedMotion: 'reduce' });
    const errors = [];
    page.on('pageerror', err => errors.push(err.message));
    page.on('response', res => { if (res.url().startsWith(base) && res.status() >= 400) errors.push(`${res.status()} ${res.url()}`); });
    const response = await page.goto(base, { waitUntil: 'networkidle' });
    assert.equal(response?.status(), 200);
    const install = page.locator('#install');
    await install.scrollIntoViewIfNeeded();
    await page.locator('.pre').waitFor({ state: 'detached', timeout: 12000 });
    assert((await install.innerText()).includes(version), `install panel missing expected version ${version}`);
    assert.equal(await install.locator('a[href="https://github.com/sauucee/saucefx/releases/latest"]').count(), 1);
    const panel = page.locator('#install-panel');
    const flatpak = await panel.innerText();
    assert(flatpak.includes('flatpak install') && flatpak.includes('https://sauucee.github.io/saucefx/flatpak/SauceFX.flatpakref'), 'missing signed Flatpak install link');
    await page.getByRole('tab', { name: 'Fedora RPM' }).click();
    const rpm = await panel.innerText();
    assert(rpm.includes('dnf install') && rpm.includes('saucefx-') && rpm.includes('.rpm') && !rpm.includes('SauceFX-x86_64.rpm'), 'invalid native RPM instruction');
    await page.getByRole('tab', { name: 'Flatpak' }).click();
    assert((await panel.innerText()).includes('flatpak install'));
    assert(!(await page.locator('body').innerText()).includes('0.2.4'));
    assert(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth + 1), 'viewport overflow');
    assert.deepEqual(errors, []);
    await install.screenshot({ path: `${out}/${label}.png` });
    await page.close();
    console.log(`${label}: version ${version}, release link, Flatpak and RPM tabs passed`);
  }
} finally { await browser.close(); }
