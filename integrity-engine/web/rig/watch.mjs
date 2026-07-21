import { launch } from './_launch.mjs';
import { writeFileSync } from 'node:fs';
const PORT = process.env.PORT || '5173';
const out = process.env.OUT || '/tmp';
const browser = await launch();
const page = await browser.newPage({ viewport: { width: 1280, height: 800 } });
const grab = async (name) => {
  await page.screenshot({ path: `${out}/${name}.png` });
  console.log('grabbed', name);
};
await page.goto(`http://127.0.0.1:${PORT}/orbit.html`, { waitUntil: 'load' });
await page.waitForTimeout(6000);
await grab('c-orbit');
await page.goto(`http://127.0.0.1:${PORT}/birth.html`, { waitUntil: 'load' });
await page.waitForTimeout(4000);
await grab('c-birth-pre');
await page.waitForTimeout(8000);
await grab('c-birth-post');
await browser.close();
