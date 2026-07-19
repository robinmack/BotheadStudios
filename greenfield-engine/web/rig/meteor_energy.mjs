import { chromium } from 'playwright';
const out = '/tmp/claude-1000/-home-ratwood/b8643c15-d933-437e-8ec8-236cf9ecf634/scratchpad';
const PORT = process.env.PORT || '5299';
const b = await chromium.launch({ headless: false, args: ['--enable-unsafe-webgpu','--enable-features=Vulkan','--use-angle=vulkan','--no-sandbox'] });
const p = await b.newPage({ viewport: { width: 1280, height: 800 } });
const stat = async () => (await p.locator('#stats').innerText().catch(()=> '')).replace(/\s+/g,' ').trim();
const debris = async () => { const m = (await stat()).match(/debris\s+(\d+)/); return m ? +m[1] : -1; };
await p.goto(`http://127.0.0.1:${PORT}/terrain.html`, { waitUntil: 'load' });
await p.waitForTimeout(3000);
await p.screenshot({ path: `${out}/me-0-start.png` });
console.log('start:', await stat());
await p.keyboard.press('m');   // ONE meteor, normal time
let peak = 0;
const marks = [1500, 1500, 3000, 4000, 6000, 8000, 10000, 10000]; // dt between screenshots
let elapsed = 0;
for (const dt of marks) {
  await p.waitForTimeout(dt);
  elapsed += dt;
  const d = await debris();
  peak = Math.max(peak, d);
  const s = (elapsed/1000).toFixed(1);
  console.log(`t+${s}s: debris=${d}`);
  await p.screenshot({ path: `${out}/me-${s}s.png` });
}
console.log('peak debris:', peak, 'final:', await debris());
await b.close();
