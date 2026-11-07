// What the appearance pyramids cost in the BROWSER (docs/73 step 2): load terra.html, capture the
// engine's own log lines ("appearance: globe pyramid … built in N ms", "appearance: patch pyramid …"),
// and the time from navigation to the first of each. Native builds measured 0.8 s for the globe and
// 210–330 ms for the patch on the 5060 Ti; the wasm build (opt-level "s") is the one the live site pays.
// Usage: scripts/rigshot.sh appearance_cost.mjs   (vite on $PORT, GPU Xorg up)
import { launch } from './_launch.mjs';
const PORT = process.env.PORT || '5173';
const PAGE = process.env.PAGE || 'terra.html';
const b = await launch();
const p = await b.newPage({ viewport: { width: 1100, height: 720 } });
const t0 = Date.now();
const lines = [];
p.on('console', (m) => {
  const text = m.text();
  if (process.env.ALL) console.log(`  [console] ${text.slice(0, 200)}`);
  if (/appearance:|Terra: loaded|Terra: surface loaded/.test(text)) {
    lines.push({ at_ms: Date.now() - t0, text });
    console.log(`+${((Date.now() - t0) / 1000).toFixed(2)}s  ${text}`);
  }
});
p.on('pageerror', (e) => console.log(`  [pageerror] ${String(e).slice(0, 300)}`));
await p.goto(`http://127.0.0.1:${PORT}/${PAGE}`, { waitUntil: 'load' });
// The globe is built on the first frame after the world loads; give the page time to stream a
// tile patch too, so the patch's cost is seen with real tiles.
await p.waitForTimeout(Number(process.env.WAIT_MS || 25000));
const globe = lines.find((l) => /globe pyramid/.test(l.text));
const patches = lines.filter((l) => /patch pyramid/.test(l.text));
console.log(
  `appearance cost: globe ${globe ? globe.text.match(/built in (\d+) ms/)?.[1] + ' ms' : 'NOT SEEN'} · ` +
    `patch builds ${patches.length}` +
    (patches.length ? ` (${patches.map((l) => l.text.match(/built in (\d+) ms/)?.[1]).join(', ')} ms)` : ''),
);
await b.close();
console.log('done');
