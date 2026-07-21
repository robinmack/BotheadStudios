import { launch } from './_launch.mjs';
const PORT = process.env.PORT || '5173';
const browser = await launch();
const page = await browser.newPage({ viewport: { width: 1280, height: 800 } });
await page.goto(`http://127.0.0.1:${PORT}/birth.html`, { waitUntil: 'load' });
await page.waitForTimeout(15000);
const r = await page.evaluate(() => {
  const w = window;
  const avg = (a) => a.slice(-60).reduce((x, y) => x + y, 0) / Math.min(60, a.length);
  return { advance_ms: avg(w.__adv ?? [0]), render_ms: avg(w.__ren ?? [0]) };
});
console.log(JSON.stringify(r));
await browser.close();
