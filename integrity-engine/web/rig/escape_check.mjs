import { launch } from './_launch.mjs';
const PORT = process.env.PORT || '5173';
const b = await launch();
const p = await b.newPage({ viewport: { width: 1000, height: 700 } });
await p.goto(`http://127.0.0.1:${PORT}/birth.html`, { waitUntil: 'load' });
let prev = 0;
for (const t of [15,30,45,60,75,90,105]) {
  await p.waitForTimeout(t*1000 - prev*1000); prev = t;
  const [e,d] = await p.evaluate(() => [window.__demo?.gpu_energy_json?.()??'n', window.__demo?.gpu_disk_stats_json?.()??'n']);
  let E=null,D=null; try{E=JSON.parse(e)}catch{} try{D=JSON.parse(d)}catch{}
  console.log(`t=${t}s tot=${E?E.tot.toExponential(3):e}  disk=${D?D.disk.toFixed(3):'-'} moon=${D?D.moon.toFixed(3):'-'} escaped=${D?D.escaped.toFixed(3):'-'} remnant=${D?D.remnant_km:'-'}km`);
}
await b.close(); console.log('done');
