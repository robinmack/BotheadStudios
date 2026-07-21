// Exercise the GPU particle CONTAINER (`crate::gpu_particles`) end to end in the real scene: fire a
// meteor, watch grains be appended, stepped by particle_step.wgsl, expanded to render sub-cubes, read
// back and de-resolved. Written for the docs/33 lift of `GpuParticles` out of `#[cfg(wasm32)] mod app`
// — a static terrain shot barely touches the container, so it could not have caught a broken lift.
//
//   bash scripts/rigshot.sh debris_container.mjs
import { launch } from './_launch.mjs';
const PORT = process.env.PORT || '5173';
const OUT = process.env.OUT || '/tmp';
const b = await launch();
const p = await b.newPage({ viewport: { width: 1280, height: 800 } });
p.on('pageerror', (e) => console.log('[pageerror]', e.message));
p.on('console', (m) => { const t = m.text(); if (/error|panic|unreachable/i.test(t)) console.log('[page]', t); });

const hud = async () => (await p.locator('#stats').innerText().catch(() => '')).replace(/\s+/g, ' ').trim();
const debris = async () => { const m = (await hud()).match(/debris\s+([\d,]+)/); return m ? +m[1].replace(/,/g, '') : null; };

await p.goto(`http://127.0.0.1:${PORT}/terrain.html`, { waitUntil: 'load' });
await p.waitForTimeout(5000);
console.log('build:', ((await hud()).match(/build\s+(\S+)/) || [])[1]);
console.log('debris before:', await debris());
await p.screenshot({ path: `${OUT}/debris-0-before.png` });

await p.locator('#gpu-canvas').click({ position: { x: 640, y: 500 } }); // focus for key input
await p.keyboard.press('m');                                            // ☄ meteor
let peak = 0;
for (const s of [1, 2, 4, 8, 14, 22]) {
  await p.waitForTimeout(s * 1000 - (peak ? 0 : 0));
  const d = await debris();
  peak = Math.max(peak, d ?? 0);
  console.log(`t+${s}s debris=${d}`);
  await p.screenshot({ path: `${OUT}/debris-${s}s.png` });
}
console.log('PEAK debris:', peak);
console.log('final HUD:', await hud());
await b.close();
