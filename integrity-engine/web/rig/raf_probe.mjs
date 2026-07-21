// Is the PAGE's rAF throttled, or is the ENGINE's callback slow? Runs an independent, empty rAF loop
// alongside the app and measures its own rate. Empty loop slow too => the page is being paced by the
// browser. Empty loop fast while HUD is slow => the app's own callback is the cost.
import { launch } from './_launch.mjs';
const PORT = process.env.PORT || '5173';
const b = await launch();
for (const scene of ['terra.html', 'birth.html']) {
  const p = await b.newPage({ viewport: { width: 1280, height: 800 } });
  await p.goto(`http://127.0.0.1:${PORT}/${scene}`, { waitUntil: 'load' });
  await p.waitForTimeout(6000);
  const r = await p.evaluate(() => new Promise((res) => {
    const t = []; let last = 0; const t0 = performance.now();
    const tick = (ts) => { if (last) t.push(ts - last); last = ts;
      if (performance.now() - t0 < 5000) requestAnimationFrame(tick);
      else { const s=[...t].sort((a,b)=>a-b);
             res({ n: t.length, med: s[Math.floor(s.length/2)] || 0, fps: 1000*t.length/(performance.now()-t0),
                   vis: document.visibilityState, hidden: document.hidden }); } };
    requestAnimationFrame(tick);
  }));
  const hud = (await p.locator('#stats').innerText().catch(()=>'')).replace(/\s+/g,' ');
  const hudFps = (hud.match(/·\s*([\d.]+)\s*fps/)||[])[1] ?? '?';
  console.log(`${scene.padEnd(13)} independent-rAF ${r.fps.toFixed(1).padStart(5)} fps (median ${r.med.toFixed(1)} ms) | app HUD ${String(hudFps).padStart(4)} fps | visibility=${r.vis}`);
  await p.close();
}
await b.close();
