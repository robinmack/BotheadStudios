// Reversed-Z check over the SUB-SOLAR point (lat21 lon31 — fully day-lit nadir), so a real void is unmistakable
// (a lit surface must NOT be black). Sweep the void-altitude band.
import { chromium } from 'playwright';
const out = process.env.OUT || '/tmp'; const PORT = process.env.PORT || '5173';
const b = await chromium.launch({ headless: false, args: ['--enable-unsafe-webgpu','--enable-features=Vulkan','--use-angle=vulkan','--no-sandbox'] });
const p = await b.newPage({ viewport: { width: 1000, height: 800 } });
p.on('pageerror', e => console.log('PAGEERR:', e.message));
await p.goto(`http://127.0.0.1:${PORT}/terra.html`, { waitUntil: 'load' });
await p.waitForTimeout(3500);
const setFly = (lat, lon, alt, yaw, pitch) => p.evaluate(([a,b,c,d,e]) => window.__terra?.set_fly(a,b,c,d,e), [lat,lon,alt,yaw,pitch]);
const shot = async (tag) => { await p.waitForTimeout(250); await p.screenshot({ path: `${out}/depth-${tag}.png` }); console.log('shot', tag); };

const LAT = 21, LON = 31; // sub-solar: nadir fully lit
await setFly(LAT, LON, 6_000_000, 0, -1.4); await shot('orbit-day');
await setFly(LAT, LON, 500_000, 0, -1.4);   await shot('500km');
await setFly(LAT, LON, 259_000, 0, -1.4);   await shot('259km');
await setFly(LAT, LON, 250_000, 0, -1.4);   await shot('250km');
await setFly(LAT, LON, 100_000, 0, -1.4);   await shot('100km');
await setFly(LAT, LON, 45_000, 0, -1.0);    await shot('45km');
await setFly(LAT, LON, 20_000, 0, -0.5);    await shot('20km');
await setFly(LAT, LON, 1_500, 0, 0.05);     await shot('1500m');
await b.close(); console.log('done');
