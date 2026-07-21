import { chromium } from 'playwright';
const PORT = process.env.PORT || '5173';
const out = process.env.OUT || '/tmp';
const b = await chromium.launch({ headless: false, args: ['--enable-unsafe-webgpu','--enable-features=Vulkan','--use-angle=vulkan','--no-sandbox'] });
const p = await b.newPage({ viewport: { width: 1280, height: 800 } });
await p.goto(`http://127.0.0.1:${PORT}/terrain.html`, { waitUntil: 'load' });
await p.waitForTimeout(2500); await p.screenshot({ path: `${out}/t1-terrain.png` });
// Fire several meteors to crater the surface and expose the strata below.
for (let i = 0; i < 6; i++) { await p.keyboard.press('m'); await p.waitForTimeout(600); }
await p.waitForTimeout(2500); await p.screenshot({ path: `${out}/t2-crater.png` });
await p.waitForTimeout(4000); await p.screenshot({ path: `${out}/t3-crater.png` });
await b.close();
