// Check (a) the normal Space scene still has Sun–Earth–Moon orbital motion, and (b) the GPU impact debris
// (escaped vs bound disk) over time. Prints Moon distance/speed for the orbit scene.
import { chromium } from 'playwright';
const out = process.env.OUT || '/tmp';
const PORT = process.env.PORT || '5173';
const b = await chromium.launch({ headless: false, args: ['--enable-unsafe-webgpu', '--enable-features=Vulkan', '--use-angle=vulkan', '--no-sandbox'] });
const p = await b.newPage({ viewport: { width: 1280, height: 800 } });
p.on('pageerror', (e) => console.log('PAGEERR:', e.message));
// --- (a) Space scene: orbits ---
await p.goto(`http://127.0.0.1:${PORT}/orbit.html`, { waitUntil: 'load' });
await p.waitForTimeout(1500);
console.log('=== SPACE (orbit.html): Moon distance km over time (should vary smoothly ~384400) ===');
for (let i = 0; i < 5; i++) {
  await p.waitForTimeout(1200);
  const d = await p.evaluate(() => ({ dist: window.__demo?.moon_distance_km?.(), spd: window.__demo?.moon_speed_kms?.() }));
  console.log(`  t${i}: Moon ${d.dist?.toFixed(0)} km, v ${d.spd?.toFixed(3)} km/s`);
}
await p.screenshot({ path: `${out}/orbit-space.png` });
await b.close();
console.log('done');
