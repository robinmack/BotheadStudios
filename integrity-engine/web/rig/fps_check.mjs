import { launch } from './_launch.mjs';
const PORT = process.env.PORT || '5173';
const b = await launch();
const p = await b.newPage({ viewport: { width: 1100, height: 720 } });
await p.goto(`http://127.0.0.1:${PORT}/birth.html`, { waitUntil: 'load' });
for (const t of [5,10,15,20]) {
  await p.waitForTimeout(5000);
  const s = await p.evaluate(() => {
    const adv = (window.__adv||[]).slice(-60);
    const avg = adv.length ? adv.reduce((a,b)=>a+b,0)/adv.length : 0;
    const max = adv.length ? Math.max(...adv) : 0;
    return { substeps: window.__demo?.__nothing, avgAdvMs: +avg.toFixed(1), maxAdvMs: +max.toFixed(1) };
  });
  console.log(`t=${t}s avgAdvance=${s.avgAdvMs}ms maxAdvance=${s.maxAdvMs}ms`);
}
await b.close(); console.log('done');
