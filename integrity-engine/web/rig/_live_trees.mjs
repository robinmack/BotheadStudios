// Ground-level plants, on PRODUCTION — and the thing the lattice fix was for: walk, and check the
// trees are where they were. A 200 is not a scene; a still frame is not "they stay put".
import { launch, VIEWPORT } from './_launch.mjs';
import { pose } from './_poses.mjs';
const base = process.env.BASE || 'https://integrity.bothead.net';
const out = process.env.OUT || '/tmp/rigshot';
const b = await launch();
const p = await b.newPage({ viewport: VIEWPORT });
p.on('pageerror', (e) => console.log('PAGEERR:', e.message.slice(0, 200)));
await p.goto(`${base}/terra.html`, { waitUntil: 'load' });
await p.waitForFunction(() => !!window.__terra, null, { timeout: 60000 });
await p.waitForTimeout(4000);

// Maine in October: mixed forest, and the season is on.
const SITES = [
  ['maine-forest', 45.3, -69.0, 1730000000],
  ['ireland-pasture', 53.1, -9.45, 1718945000],
];
for (const [name, lat, lon, epoch] of SITES) {
  for (const [tag, alt, pitch] of [['eye', 1.7, 0.02], ['crouch', 0.4, 0.05], ['above', 25, -0.35]]) {
    await pose(p, { lat, lon, alt_m: alt, yaw: 0.6, pitch, sunLon: lon + 35 }, { epoch, settleMs: 2600 });
    await p.screenshot({ path: `${out}/trees-${name}-${tag}.png` });
  }
  // ★ THE WALK. Step 8 m north and back; the same stand must be the same stand.
  const at = async (dlat) => {
    await pose(p, { lat: lat + dlat, lon, alt_m: 1.7, yaw: 0.6, pitch: 0.02, sunLon: lon + 35 },
      { epoch, settleMs: 2600 });
    return p.evaluate(() => window.__terra.flora_count?.() ?? -1);
  };
  await at(0);
  await p.screenshot({ path: `${out}/trees-${name}-walk-a.png` });
  await at(0.00007); // ~7.8 m north
  await p.screenshot({ path: `${out}/trees-${name}-walk-b.png` });
  await at(0);
  await p.screenshot({ path: `${out}/trees-${name}-walk-back.png` });
  console.log(`${name}: shot eye/crouch/above + a walk there and back`);
}
await b.close();
