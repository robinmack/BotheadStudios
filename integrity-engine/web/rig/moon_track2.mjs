import { chromium } from 'playwright';
const PORT = process.env.PORT || '5173';
const b = await chromium.launch({ headless: false, args: ['--enable-unsafe-webgpu','--enable-features=Vulkan','--use-angle=vulkan','--no-sandbox'] });
const p = await b.newPage({ viewport: { width: 1000, height: 700 } });
await p.goto(`http://127.0.0.1:${PORT}/birth.html`, { waitUntil: 'load' });
await p.waitForTimeout(30000);
for (let i=0;i<40;i++){
  await p.waitForTimeout(3000);
  const [m,d] = await p.evaluate(() => [window.__demo?.gpu_moon_track_json?.()??'null', window.__demo?.gpu_disk_stats_json?.()??'null']);
  let D=null; try{D=JSON.parse(d)}catch{}
  const dk = D?`disk=${D.disk.toFixed(2)} esc=${D.escaped.toFixed(2)} rem=${D.remnant_km}`:'';
  console.log(`+${30+i*3}s moon=${m}  ${dk}`);
}
await b.close(); console.log('done');
