import { launch, PORT, OUT } from './_launch.mjs';
const b = await launch();
const p = await b.newPage({ viewport: { width: 1280, height: 800 } });
await p.goto(`http://127.0.0.1:${PORT}/birth.html`, { waitUntil: 'load' });
for (const t of [5, 10, 20, 35]) {
  await p.waitForTimeout(t === 5 ? 5000 : (t - (t === 10 ? 5 : t === 20 ? 10 : 20)) * 1000);
  const crop = await p.screenshot({ clip: { x: 300, y: 120, width: 680, height: 460 } });
  const hud = (await p.locator('#stats').innerText().catch(()=>'')).replace(/\s+/g,' ').slice(0, 80);
  console.log(`t+${String(t).padStart(2)}s: ${String(crop.length).padStart(7)} B  ${crop.length > 20000 ? 'RENDERS' : 'blank  '} | ${hud}`);
}
await b.close();
