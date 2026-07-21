import { chromium } from 'playwright';
const PORT = process.env.PORT || '5173';
const out = process.env.OUT || '/tmp';
const b = await chromium.launch({ headless: false, args: ['--enable-unsafe-webgpu','--enable-features=Vulkan','--use-angle=vulkan','--no-sandbox'] });
const p = await b.newPage({ viewport: { width: 1280, height: 800 } });
const stat = async () => (await p.locator('#stats').innerText().catch(()=> '')).replace(/\s+/g,' ').trim();
await p.goto(`http://127.0.0.1:${PORT}/terrain.html`, { waitUntil: 'load' });
await p.waitForTimeout(2500); await p.screenshot({ path: `${out}/ma-0-before.png` }); console.log('before:', await stat());
for (let i=0;i<5;i++){ await p.keyboard.press('m'); await p.waitForTimeout(500); }
await p.waitForTimeout(1500); await p.screenshot({ path: `${out}/ma-1-strikes.png` }); console.log('after:', await stat());
await p.waitForTimeout(3000); await p.screenshot({ path: `${out}/ma-2-settle.png` }); console.log('settle:', await stat());
await b.close();
