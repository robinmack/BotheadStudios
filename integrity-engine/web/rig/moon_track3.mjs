import { launch } from './_launch.mjs';
const PORT = process.env.PORT || '5173';
const b = await launch();
const p = await b.newPage({ viewport: { width: 1000, height: 700 } });
await p.goto(`http://127.0.0.1:${PORT}/birth.html`, { waitUntil: 'load' });
await p.waitForTimeout(40000);
for (let i=0;i<50;i++){
  await p.waitForTimeout(4000);
  const m = await p.evaluate(() => window.__demo?.gpu_moon_track_json?.()??'null');
  if (m!=='null') console.log(`+${40+i*4}s ${m}`);
}
await b.close(); console.log('done');
