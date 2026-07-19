// Measure GPU-impact energy conservation + disk/escape over time (docs/35 diagnosis). Triggers the GPU impact
// on orbit.html and logs total energy drift, remnant radius, bound-disk mass, escaped mass, moon candidate.
import { chromium } from 'playwright';
const PORT = process.env.PORT || '5307';
const OUT = process.env.OUT || '/tmp';
const b = await chromium.launch({ headless: false, args: ['--enable-unsafe-webgpu', '--enable-features=Vulkan', '--use-angle=vulkan', '--no-sandbox'] });
const p = await b.newPage({ viewport: { width: 1280, height: 800 } });
p.on('pageerror', (e) => console.log('PAGEERR:', e.message));
await p.goto(`http://127.0.0.1:${PORT}/orbit.html`, { waitUntil: 'load' });
await p.waitForTimeout(1500);
await p.evaluate(() => window.__demo.start_gpu_impact());
let t = 0, e0 = null;
for (const dt of [5000, 5000, 5000, 5000, 6000, 6000, 8000, 8000, 8000, 8000]) {
  await p.waitForTimeout(dt);
  t += dt;
  const e = await p.evaluate(() => window.__demo?.gpu_energy_json?.() ?? 'null');
  const d = await p.evaluate(() => window.__demo?.gpu_disk_stats_json?.() ?? 'null');
  let tot = null; try { tot = JSON.parse(e).tot; } catch {}
  if (tot != null && e0 == null) e0 = tot;
  const dE = tot != null && e0 != null ? ((tot - e0) / Math.abs(e0) * 100).toFixed(2) + '%' : '-';
  let ds = {}; try { ds = JSON.parse(d) || {}; } catch {}
  console.log(`t+${t / 1000}s ΔE=${dE} remnant=${ds.remnant_km ?? '-'}km disk=${ds.disk ?? '-'}M☾ esc=${ds.escaped ?? '-'}M☾ moon=${ds.moon ?? '-'}M☾ earth%=${ds.earth_pct ?? '-'}`);
}
await p.screenshot({ path: `${OUT}/energy-final.png` });
await b.close();
console.log('done');
