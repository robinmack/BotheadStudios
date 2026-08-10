import { launch, VIEWPORT } from './_launch.mjs';
const url = process.argv[2];
const browser = await launch();
const page = await browser.newPage({ viewport: VIEWPORT });
await page.goto(url, { waitUntil: 'load' });
await page.waitForTimeout(14000); // through impact + settle
const stats = await page.evaluate(() => document.getElementById('stats')?.textContent ?? '');
console.log(url.split('/').pop(), '→', (stats.match(/(\d+)\s*fps/) ?? ['', '?'])[1], 'fps');
await browser.close();
