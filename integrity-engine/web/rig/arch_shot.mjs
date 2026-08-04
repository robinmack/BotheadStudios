// Photograph the architecture page — a page is a picture, and inline SVG diagrams are exactly the
// kind of thing that looks fine in source and lands wrong on screen.
import { launch } from './_launch.mjs';
const out = process.env.OUT || '/tmp/rigshot';
const b = await launch();
const p = await b.newPage({ viewport: { width: 1100, height: 900 } });
p.on('pageerror', (e) => console.log('PAGEERR:', e.message));
await p.goto('http://127.0.0.1:5173/architecture.html', { waitUntil: 'load' });
await p.waitForTimeout(1200);
await p.screenshot({ path: `${out}/arch-1-top.png` });
for (const [i, y] of [700, 1500, 2400, 3300].entries()) {
  await p.evaluate((y) => window.scrollTo(0, y), y);
  await p.waitForTimeout(400);
  await p.screenshot({ path: `${out}/arch-${i + 2}-y${y}.png` });
}
const h = await p.evaluate(() => document.body.scrollHeight);
console.log(`page height ${h}px`);
await b.close();
