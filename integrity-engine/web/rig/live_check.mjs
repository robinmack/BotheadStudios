// Verify the DEPLOYED site renders — not that it returns 200. A deploy that serves a blank canvas answers
// every HTTP check perfectly.
import { launch } from './_launch.mjs';
const out = process.env.OUT || '/tmp/rigshot';
const URL_ = process.env.URL || 'https://integrity.bothead.net/terra.html';
const b = await launch();
const p = await b.newPage({ viewport: { width: 1000, height: 800 } });
p.on('pageerror', e => console.log('PAGEERR:', e.message));
await p.goto(URL_, { waitUntil: 'load' });
await p.waitForFunction(() => !!window.__terra, null, { timeout: 60000 });
await p.waitForTimeout(4000);
const lon = await p.evaluate(() => window.__terra.sub_solar()[1]);
console.log(`live subsolar lon ${lon.toFixed(1)} — flying somewhere lit`);
for (const [alt, tag] of [[8.0e6, 'orbit'], [3.0e3, '3km'], [3.0e2, '300m']]) {
  await p.evaluate(({ alt, lon }) => window.__terra.set_fly(39, lon + 40, alt, 0.6, -0.45), { alt, lon });
  await p.waitForTimeout(4500);
  const n = await p.evaluate(() => (window.__tiles ? window.__tiles() : -1));
  await p.screenshot({ path: `${out}/live-${tag}.png` });
  console.log(`  ${tag}: tiles=${n}`);
}
await b.close(); console.log('done');
