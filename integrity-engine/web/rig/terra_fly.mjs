// docs/43 Phase 4 — fly-camera verification: an orbit→ground zoom sequence + functional WASD/drag readback tests.
import { chromium } from 'playwright';
const out = process.env.OUT || '/tmp'; const PORT = process.env.PORT || '5173';
const b = await chromium.launch({ headless: false, args: ['--enable-unsafe-webgpu','--enable-features=Vulkan','--use-angle=vulkan','--no-sandbox'] });
const p = await b.newPage({ viewport: { width: 1000, height: 800 } });
p.on('pageerror', e => console.log('PAGEERR:', e.message));
await p.goto(`http://127.0.0.1:${PORT}/terra.html`, { waitUntil: 'load' });
await p.waitForTimeout(3500);

const setFly = (lat, lon, alt, yaw, pitch) => p.evaluate(([a,b,c,d,e]) => window.__terra?.set_fly(a,b,c,d,e), [lat,lon,alt,yaw,pitch]);
const read = () => p.evaluate(() => ({ lat: window.__terra.latitude(), lon: window.__terra.longitude(), alt: window.__terra.altitude_m() }));
const shot = async (tag) => { await p.waitForTimeout(350); await p.screenshot({ path: `${out}/fly-${tag}.png` }); console.log('shot', tag, JSON.stringify(await read())); };

// --- Orbit → ground zoom sequence (look straight down blends to horizon automatically as altitude drops) ---
await shot('00-initial');
await setFly(28, 84, 800_000, 0, 0.0);  await shot('01-800km-himalaya');
await setFly(28, 84, 80_000, 0, 0.0);   await shot('02-80km');
await setFly(28, 84, 8_000, 0, 0.0);    await shot('03-8km');
await setFly(28, 84, 1_500, 0, 0.05);   await shot('04-1500m-horizon');
await setFly(36, -6, 1_200, 1.2, 0.02); await shot('05-coast-horizon');

// --- Functional: WASD moves north (W) over the equator ---
await setFly(0, 0, 5_000, 0, 0.0);
const before = await read();
await p.keyboard.down('w'); await p.waitForTimeout(600); await p.keyboard.up('w');
const after = await read();
console.log('WASD W:', JSON.stringify(before), '->', JSON.stringify(after), 'dLat=', (after.lat - before.lat).toFixed(4));

// --- Functional: drag orbits at high altitude (lat/lon change) ---
await setFly(0, 0, 8_000_000, 0, 0.0);
const o0 = await read();
await p.mouse.move(500, 420); await p.mouse.down(); await p.mouse.move(300, 420, { steps: 6 }); await p.mouse.up();
const o1 = await read();
console.log('DRAG orbital: dLon=', (o1.lon - o0.lon).toFixed(3), 'dLat=', (o1.lat - o0.lat).toFixed(3));

// --- Functional: drag free-looks at ground (lat/lon barely change) ---
await setFly(0, 0, 800, 0, 0.0);
const g0 = await read();
await p.mouse.move(500, 420); await p.mouse.down(); await p.mouse.move(300, 420, { steps: 6 }); await p.mouse.up();
const g1 = await read();
console.log('DRAG ground: dLon=', (g1.lon - g0.lon).toFixed(3), 'dLat=', (g1.lat - g0.lat).toFixed(3), '(should be ~0)');

await b.close(); console.log('done');
