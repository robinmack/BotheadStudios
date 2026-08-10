// **Watch the shot, half a second at a time.** Robin: *"watch the scene, shots every half second...
// cannon seems to shoot to the side?!"*
//
// A single screenshot cannot show a trajectory going wrong; a sequence at a fixed cadence can. This
// fires ONCE and photographs every 500 ms through the whole flight, so where the shot goes relative to
// where the gun points is visible frame by frame rather than inferred.
import { launch } from './_launch.mjs';
const out = process.env.OUT || '/tmp/rigshot';
const FRAMES = +(process.env.FRAMES || 16);
const b = await launch();
const p = await b.newPage({ viewport: { width: 900, height: 600 } });
p.on('pageerror', e => console.log('PAGEERR:', e.message));
p.on('console', m => { const t = m.text(); if (/cannon|arrival|error|panic/i.test(t)) console.log('CONSOLE:', t); });
await p.goto('http://127.0.0.1:5173/yarr.html', { waitUntil: 'load' });
await p.waitForFunction(() => !!window.__terra, null, { timeout: 60000 });
await p.waitForTimeout(4500);

await p.screenshot({ path: `${out}/t00-rest.png` });
console.log('  t00-rest');
await p.evaluate(() => {
  const btn = [...document.querySelectorAll('button')].find(b => /fire cannon/i.test(b.textContent));
  btn.click();
});
for (let i = 1; i <= FRAMES; i++) {
  await p.waitForTimeout(500);
  const s = await p.evaluate(() => {
    const f = window.__terra.heaviest_fragment();
    return { flying: f.length > 0, r: f.length ? Math.hypot(f[1], f[2], f[3]) : 0, parcels: window.__terra.trail_parcels?.() ?? -1 };
  });
  const tag = String(i).padStart(2, '0');
  await p.screenshot({ path: `${out}/t${tag}-${(i * 0.5).toFixed(1)}s.png` });
  console.log(`  t${tag} (${(i * 0.5).toFixed(1)}s): in flight ${s.flying}${s.flying ? `, ${(s.r - 6371000).toFixed(0)} m above datum` : ''}`);
}
await b.close();
