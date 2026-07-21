// Quick visual confirm that birth.html still lofts a proto-lunar disk after the space-band ejecta change.
import { chromium } from 'playwright';
const out = process.env.OUT || '/tmp';
const PORT = process.env.PORT || '5173';
const b = await chromium.launch({ headless: false, args: ['--enable-unsafe-webgpu','--enable-features=Vulkan','--use-angle=vulkan','--no-sandbox'] });
const p = await b.newPage({ viewport: { width: 1280, height: 800 } });
const stat = async () => (await p.locator('#stats').innerText().catch(()=> '')).replace(/\s+/g,' ').trim();
await p.goto(`http://127.0.0.1:${PORT}/birth.html`, { waitUntil: 'load' });
const marks = [3000, 4000, 5000, 6000, 8000];
let t = 0;
await p.waitForTimeout(2500); await p.screenshot({ path: `${out}/birth-0.png` }); console.log('start:', await stat());
for (const dt of marks) { await p.waitForTimeout(dt); t += dt; await p.screenshot({ path: `${out}/birth-${(t/1000).toFixed(0)}s.png` }); console.log(`t+${(t/1000).toFixed(0)}s:`, await stat()); }
await b.close();
