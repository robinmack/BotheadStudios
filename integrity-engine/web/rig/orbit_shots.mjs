import { launch } from './_launch.mjs';
const out = process.env.OUT || '/tmp'; const PORT = process.env.PORT || '5173';
const b = await launch();
const p = await b.newPage({ viewport: { width: 1280, height: 800 } });
await p.goto(`http://127.0.0.1:${PORT}/birth.html`, { waitUntil: 'load' });
await p.waitForTimeout(150000);
// pretty view + zoom OUT so a ~30,000 km moon is in frame
await p.evaluate(() => { window.__demo?.set_render_blend?.(0);
  const s=[...document.querySelectorAll('input[type=range]')].find(el=>el.parentElement.textContent.includes('Zoom'));
  if(s){ s.value='22'; s.dispatchEvent(new Event('input',{bubbles:true})); } });
let prev=150;
for (const t of [156,196,236,276]) {
  await p.waitForTimeout(t*1000 - prev*1000); prev=t;
  const m = await p.evaluate(() => window.__demo?.gpu_moon_track_json?.()??'null');
  await p.screenshot({ path: `${out}/orbit-${t}s.png` });
  console.log(`+${t}s ${m}`);
}
await b.close(); console.log('done');
