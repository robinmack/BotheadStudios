// Sample the GPU birth impact's energy + disk over time — is total energy conserved (bound disk) or rising
// (dt injection → dispersal)? docs/41 browser-GpuSph debug.
import { chromium } from 'playwright';
const PORT = process.env.PORT || '5307';
const b = await chromium.launch({ headless: false, args: ['--enable-unsafe-webgpu', '--enable-features=Vulkan', '--use-angle=vulkan', '--no-sandbox'] });
const p = await b.newPage({ viewport: { width: 1000, height: 700 } });
p.on('pageerror', (e) => console.log('PAGEERR:', e.message));
await p.goto(`http://127.0.0.1:${PORT}/birth.html`, { waitUntil: 'load' });
for (let i = 0; i < 30; i++) {
  await p.waitForTimeout(2500);
  const [e, d] = await p.evaluate(() => [window.__demo?.gpu_energy_json?.() ?? 'no', window.__demo?.gpu_disk_stats_json?.() ?? 'no']);
  console.log(`t+${((i + 1) * 2.5).toFixed(1)}s  disk=${d}`);
}
await b.close();
