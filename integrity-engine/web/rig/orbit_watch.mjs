import { chromium } from 'playwright';
const PORT = process.env.PORT || '5173';
const b = await chromium.launch({ headless: false, args: ['--enable-unsafe-webgpu','--enable-features=Vulkan','--use-angle=vulkan','--no-sandbox'] });
const p = await b.newPage({ viewport: { width: 1000, height: 700 } });
await p.goto(`http://127.0.0.1:${PORT}/birth.html`, { waitUntil: 'load' });
await p.waitForTimeout(120000); // let the outside-Roche Moon establish
for (let i=0;i<45;i++){
  await p.waitForTimeout(4000);
  const m = await p.evaluate(() => window.__demo?.gpu_moon_track_json?.()??'null');
  if (m!=='null'){ const o=JSON.parse(m);
    console.log(`+${120+i*4}s dist=${o.dist_km} a=${o.a_km} ecc=${o.ecc} theta=${o.theta_deg}° mass=${o.mass_moon}`); }
  else console.log(`+${120+i*4}s (no moon)`);
}
await b.close(); console.log('done');
