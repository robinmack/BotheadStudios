import { chromium } from 'playwright';
const out = '/tmp/claude-1000/-home-ratwood/b8643c15-d933-437e-8ec8-236cf9ecf634/scratchpad';
const b = await chromium.launch({ headless: false, args: ['--enable-unsafe-webgpu','--enable-features=Vulkan','--use-angle=vulkan','--no-sandbox'] });
const p = await b.newPage({ viewport: { width: 1280, height: 800 } });
await p.goto('http://127.0.0.1:5292/terrain.html', { waitUntil: 'load' });
await p.waitForTimeout(3000); await p.screenshot({ path: `${out}/FINAL-terrain.png` });
for (let i=0;i<3;i++){ await p.keyboard.press('m'); await p.waitForTimeout(500); }
await p.waitForTimeout(2500); await p.screenshot({ path: `${out}/FINAL-terrain-strike.png` });
await b.close();
