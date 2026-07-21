import { launch } from './_launch.mjs';
const PORT = process.env.PORT || '5173';
const out = process.env.OUT || '/tmp';
const browser = await launch();
const page = await browser.newPage({ viewport: { width: 1280, height: 800 } });
const readout = async () => (await page.locator('#stats').innerText().catch(() => '')).replace(/\s+/g, ' ').trim();
const grab = async (n) => { await page.screenshot({ path: `${out}/${n}.png` }); console.log('---', n, '\n', await readout()); };
await page.goto(`http://127.0.0.1:${PORT}/birth.html`, { waitUntil: 'load' });
await page.waitForTimeout(9000);   // just after impact — the ejecta curtain
await grab('p1-curtain');
await page.waitForTimeout(6000);   // disk settling
await grab('p2-disk');
await browser.close();
