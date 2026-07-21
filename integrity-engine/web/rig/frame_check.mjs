import { launch } from './_launch.mjs';
const PORT = process.env.PORT || '5173';
const b = await launch();
const p = await b.newPage({ viewport: { width: 1100, height: 720 } });
await p.goto(`http://127.0.0.1:${PORT}/birth.html`, { waitUntil: 'load' });
for (const t of [6,12,18,24]) {
  await p.waitForTimeout(6000);
  const r = await p.evaluate(() => new Promise((res) => {
    const ts = []; let last = performance.now(); let n = 0;
    function tick(now){ ts.push(now-last); last=now; if(++n<40) requestAnimationFrame(tick); else {
      ts.sort((a,b)=>a-b); const avg=ts.reduce((a,b)=>a+b,0)/ts.length;
      res({ fps:+(1000/avg).toFixed(1), avgMs:+avg.toFixed(1), medMs:+ts[ts.length>>1].toFixed(1), maxMs:+ts[ts.length-1].toFixed(1) });
    }}
    requestAnimationFrame(tick);
  }));
  console.log(`t=${t}s fps=${r.fps} avgFrame=${r.avgMs}ms median=${r.medMs}ms worst=${r.maxMs}ms`);
}
await b.close(); console.log('done');
