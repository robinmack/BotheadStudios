import { launch, PORT } from './_launch.mjs';
const b = await launch();
const p = await b.newPage({ viewport: { width: 1280, height: 800 } });
const t0 = Date.now();
p.on('console', m => { const t = m.text(); if (/ready|Failed|error/i.test(t)) console.log(`  +${((Date.now()-t0)/1000).toFixed(1)}s [console]`, t.slice(0,120)); });
await p.goto(`http://127.0.0.1:${PORT}/birth.html`, { waitUntil: 'load' });
for (let i = 0; i < 12; i++) {
  await p.waitForTimeout(5000);
  const st = (await p.locator('#status').innerText().catch(()=>'')).replace(/\s+/g,' ').slice(0,60);
  const crop = await p.screenshot({ clip: { x: 300, y: 120, width: 680, height: 460 } });
  console.log(`  +${((Date.now()-t0)/1000).toFixed(0).padStart(3)}s  ${String(crop.length).padStart(7)} B  status="${st}"`);
  if (crop.length > 20000) { console.log('  -> RENDERS'); break; }
}
await b.close();
