// docs/28 crater-scaled ejecta: fire ONE terrain meteor and MEASURE whether the ejecta lands in a
// LOCAL blanket (a few crater radii, tens of m) or the old footprint-spanning storm (~96 m patch / sky).
// Reads the engine's live terrain_debris_spread_m / _height_m via the scene's exposed sim handle.
import { chromium } from 'playwright';
const out = '/tmp/claude-1000/-home-ratwood/b8643c15-d933-437e-8ec8-236cf9ecf634/scratchpad';
const PORT = process.env.PORT || '5307';
const b = await chromium.launch({ headless: false, args: ['--enable-unsafe-webgpu','--enable-features=Vulkan','--use-angle=vulkan','--no-sandbox'] });
const p = await b.newPage({ viewport: { width: 1280, height: 800 } });
p.on('console', m => { const t = m.text(); if (/spread|debris|blanket|EJECTA/.test(t)) console.log('  [page]', t); });
const stat = async () => (await p.locator('#stats').innerText().catch(()=> '')).replace(/\s+/g,' ').trim();
// Reach into the scene: the terrain page stashes its Engine on window for the rig (see main.ts). If not
// present, fall back to parsing #stats.
const probe = async () => p.evaluate(() => {
  const s = window.__sim || window.sim || window.engine;
  if (s && s.terrain_debris_spread_m) {
    return { spread: s.terrain_debris_spread_m(), height: s.terrain_debris_height_m(), n: s.particle_count() };
  }
  return null;
});
await p.goto(`http://127.0.0.1:${PORT}/terrain.html`, { waitUntil: 'load' });
await p.waitForTimeout(3500);
await p.screenshot({ path: `${out}/eb-0-start.png` });
console.log('start:', await stat(), '| probe:', JSON.stringify(await probe()));
await p.keyboard.press('m');   // ONE meteor, normal time
const marks = [1200, 1800, 3000, 4000, 6000, 8000];
let elapsed = 0, peakSpread = 0, peakHeight = 0;
for (const dt of marks) {
  await p.waitForTimeout(dt); elapsed += dt;
  const pr = await probe();
  if (pr) { peakSpread = Math.max(peakSpread, pr.spread); peakHeight = Math.max(peakHeight, pr.height); }
  const s = (elapsed/1000).toFixed(1);
  console.log(`t+${s}s: ${await stat()} | probe: ${JSON.stringify(pr)}`);
  await p.screenshot({ path: `${out}/eb-${s}s.png` });
}
console.log(`EJECTA peak spread ${peakSpread.toFixed(1)} m · peak height ${peakHeight.toFixed(1)} m (96 m patch)`);
await b.close();
