// The LIVE architecture page, not the local one — a deploy is only verified against what is served.
import { launch } from './_launch.mjs';
const out = process.env.OUT || '/tmp/rigshot';
const b = await launch();
const p = await b.newPage({ viewport: { width: 1100, height: 820 } });
p.on('pageerror', (e) => console.log('PAGEERR:', e.message));
await p.goto('https://integrity.bothead.net/architecture.html', { waitUntil: 'load' });
await p.waitForTimeout(1500);
await p.evaluate(() => window.scrollTo(0, 760));
await p.waitForTimeout(500);
await p.screenshot({ path: `${out}/arch-live.png` });
console.log('title:', await p.title());
await b.close();
