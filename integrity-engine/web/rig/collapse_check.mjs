import { chromium } from 'playwright';
const out = '/tmp/claude-1000/-home-ratwood/b8643c15-d933-437e-8ec8-236cf9ecf634/scratchpad';
const b = await chromium.launch({ headless: false, args: ['--enable-unsafe-webgpu','--enable-features=Vulkan','--use-angle=vulkan','--no-sandbox'] });
const p = await b.newPage({ viewport: { width: 1280, height: 800 } });
const stat = async () => (await p.locator('#stats').innerText().catch(()=> '')).replace(/\s+/g,' ').trim();
await p.goto('http://127.0.0.1:5296/terrain.html', { waitUntil: 'load' });
await p.waitForTimeout(3000); await p.screenshot({ path: `${out}/cc-0-start.png` }); console.log('start:', await stat());
await p.keyboard.press('m');   // ONE meteor, normal time
await p.waitForTimeout(6000); await p.screenshot({ path: `${out}/cc-1-postimpact.png` }); console.log('post:', await stat());
await p.waitForTimeout(8000); await p.screenshot({ path: `${out}/cc-2-settled.png` }); console.log('settled:', await stat());
await b.close();
