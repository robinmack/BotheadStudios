// **The "Yarrr!" scene** — the same Earth, one assembly standing on it, and somewhere to watch from.
//
// Robin: *"clone the scene and create a new one called Yarrr! where the scene starts looking down from
// behind the cannon so it can be seen, and then give me the fire button with the camera panning to the
// splash."* No engine code was added to make this scene exist: `yarr.html` loads the SAME
// `/worlds/earth/world.json` that `terra.html` does, and adds one attribute saying a gun stands here.
import { launch } from './_launch.mjs';
const out = process.env.OUT || '/tmp/rigshot';
const PORT = process.env.PORT || '5173';
const b = await launch();
const p = await b.newPage({ viewport: { width: 960, height: 640 } });
p.on('pageerror', e => console.log('PAGEERR:', e.message));
p.on('console', m => { const t = m.text(); if (/cannon|arrival|error|panic/i.test(t)) console.log('CONSOLE:', t); });
await p.goto(`http://127.0.0.1:${PORT}/yarr.html`, { waitUntil: 'load' });
await p.waitForFunction(() => !!window.__terra, null, { timeout: 60000 });
await p.waitForTimeout(4000);
// ★ The sun is NOT pinned. This scene runs on the real clock, so what this rig photographs is the
// light Ireland actually has at the moment it runs — which is the point of moving it there. A pinned
// sun would make the rig reproducible and the claim ("it is daylit") meaningless.
await p.waitForTimeout(1500);

await p.screenshot({ path: `${out}/yarr-0-start.png` });
console.log('  0-start (the view the scene opens on)');

const ok = await p.evaluate(() => {
  const btn = [...document.querySelectorAll('button')].find(b => /fire cannon/i.test(b.textContent));
  if (!btn) return false; btn.click(); return true;
});
if (!ok) { console.log('  FAIL: no Fire cannon button'); await b.close(); process.exit(1); }

for (const [wait, name] of [[120, '1-fired'], [900, '2-downrange'], [2200, '3-tracking'], [4000, '4-out'], [7000, '5-splash']]) {
  await p.waitForTimeout(wait);
  await p.screenshot({ path: `${out}/yarr-${name}.png` });
  const s = await p.evaluate(() => ({
    flying: window.__terra.heaviest_fragment().length > 0,
    parcels: window.__terra.trail_parcels?.() ?? -1,
    alt: window.__terra.fly_alt_m?.() ?? -1,
  }));
  console.log(`  ${name}: in flight ${s.flying}, ${s.parcels} smoke parcels`);
}
await b.close();
