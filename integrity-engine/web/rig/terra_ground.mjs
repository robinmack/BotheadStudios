// docs/43 Phase 5 — ground-LOD verification: fly orbit → ground over varied terrain, capturing the cap cross-fade.
import { launch } from './_launch.mjs';
const out = process.env.OUT || '/tmp'; const PORT = process.env.PORT || '5173';
const b = await launch();
const p = await b.newPage({ viewport: { width: 1000, height: 800 } });
p.on('pageerror', e => console.log('PAGEERR:', e.message));
await p.goto(`http://127.0.0.1:${PORT}/terra.html`, { waitUntil: 'load' });
await p.waitForTimeout(3500);

const setFly = (lat, lon, alt, yaw, pitch) => p.evaluate(([a,b,c,d,e]) => window.__terra?.set_fly(a,b,c,d,e), [lat,lon,alt,yaw,pitch]);
const read = () => p.evaluate(() => ({ lat:+window.__terra.latitude().toFixed(2), lon:+window.__terra.longitude().toFixed(2), alt:Math.round(window.__terra.altitude_m()) }));
const shot = async (tag) => { await p.waitForTimeout(300); await p.screenshot({ path: `${out}/gnd-${tag}.png` }); console.log('shot', tag, JSON.stringify(await read())); };

// Orbital → globe/cap transition (exag=1 real relief).
await setFly(28, 84, 8_000_000, 0, 0);  await shot('00-orbit');
await setFly(28, 84, 200_000, 0, 0.05); await shot('01-200km');
await setFly(28, 84, 35_000, 0, 0.05);  await shot('02-35km-fade');
// Ground over the Himalaya (real peaks at exag=1).
await setFly(28, 84, 6_000, 0, 0.06);   await shot('03-6km-himalaya');
await setFly(28, 84, 1_500, 0, 0.08);   await shot('04-1500m');
await setFly(28, 84, 300, 0, 0.10);     await shot('05-300m');
// Coast (lat 36, lon -6) and a flatter interior.
await setFly(36, -6, 800, 1.2, 0.05);   await shot('06-coast-800m');
await setFly(40, -100, 1_200, 0.5, 0.05); await shot('07-plains-1200m');

await b.close(); console.log('done');
