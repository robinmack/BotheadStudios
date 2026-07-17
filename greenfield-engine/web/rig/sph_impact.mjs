// Rig-watch for the in-browser GPU SPH deformable-Earth impact (docs/33 stage 4c.4). Loads the space scene,
// triggers demo.start_gpu_impact() (the GPU stepper takes over), and screenshots the evolving particle field.
import { chromium } from 'playwright';
const out = process.env.OUT || '/tmp';
const PORT = process.env.PORT || '5173';
const b = await chromium.launch({ headless: false, args: ['--enable-unsafe-webgpu', '--enable-features=Vulkan', '--use-angle=vulkan', '--no-sandbox'] });
const p = await b.newPage({ viewport: { width: 1280, height: 800 } });
p.on('console', (m) => console.log('PAGE:', m.text()));
p.on('pageerror', (e) => console.log('PAGEERR:', e.message));
await p.goto(`http://127.0.0.1:${PORT}/orbit.html`, { waitUntil: 'load' });
await p.waitForTimeout(2500);
await p.screenshot({ path: `${out}/sph-before.png` });
const ok = await p.evaluate(() => {
  const d = window.__demo;
  if (!d) return 'no __demo';
  d.start_gpu_impact();
  return 'triggered';
});
console.log('start_gpu_impact:', ok);
const marks = [400, 600, 1000, 1500, 2000, 3000];
let t = 0;
for (const dt of marks) {
  await p.waitForTimeout(dt);
  t += dt;
  await p.screenshot({ path: `${out}/sph-${t}ms.png` });
  console.log(`shot t+${t}ms`);
}
await b.close();
console.log('done');
