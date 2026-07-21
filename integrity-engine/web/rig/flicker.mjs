import { launch } from './_launch.mjs';
const PORT = process.env.PORT || '5173';
const out = process.env.OUT || '/tmp';
const browser = await launch();
const page = await browser.newPage({ viewport: { width: 1280, height: 800 } });
await page.goto(`http://127.0.0.1:${PORT}/birth.html`, { waitUntil: 'load' });
await page.waitForTimeout(14000); // into the aftermath
for (let i = 0; i < 12; i++) {
  await page.screenshot({ path: `${out}/fl-${String(i).padStart(2, '0')}.png` });
  await page.waitForTimeout(350);
}
await browser.close();
