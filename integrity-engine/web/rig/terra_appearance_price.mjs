// **What does the appearance integral actually COST?** (docs/63, docs/46 row 29.)
//
// The integral tripled Terra's mesh-rebuild hitch (44-53 ms -> 158-186 ms, measured against `main` on
// the same rig). Before optimising it, price it — and price it the way gpu-perf §5 requires: MOVE ONE
// THING and re-time. Deleting the stage and re-timing the whole build measures the deletion, not the
// stage, and would happily attribute the mesh build's own cost to the integral.
//
// The thing moved is the sample grid side, pinned via `set_appearance_probes`. Probe count per vertex
// is `(side+1)^2`, so if the cost lives in the probes it must go as the grid AREA. If instead there is
// a large constant that does not move with `side`, the cost is per-VERTEX overhead and an optimisation
// aimed at the probes is aimed at the wrong thing entirely.
//
// ★ A REBUILD MUST BE FORCED BEFORE EVERY TIMED FRAME, and the first version of this rig did not do
// that — it nudged the camera 0.004 degrees and trusted that to invalidate the cache. It does not: the
// segment is deliberately over-built by `CAP_MARGIN`, so at 100 m altitude it spans ~71 km and a 440 m
// move is comfortably inside the slack it was built with. Only the first rep of each side rebuilt (the
// knob nulls the cache), so the MEDIAN of five reps was a frame that did no work at all — 0.3 ms at
// every grid size, which reads as "the integral is free" and is really "the rig measured nothing".
// `set_appearance_probes` nulls `segment_built`, so calling it before each timed frame forces the
// rebuild deterministically.
import { launch } from './_launch.mjs';
const PORT = process.env.PORT || '5173';
const LAT = +(process.env.LAT || 39);
const LON = +(process.env.LON || -106);
const ALT = +(process.env.ALT || 100);
const SIDES = (process.env.SIDES || '1,2,4,6,8').split(',').map(Number);
const REPS = +(process.env.REPS || 5);

const b = await launch();
const p = await b.newPage({ viewport: { width: 1000, height: 800 } });
p.on('pageerror', e => console.log('PAGEERR:', e.message));
p.on('console', m => { const t = m.text(); if (/error|panic|lost/i.test(t)) console.log('CONSOLE:', t); });
await p.goto(`http://127.0.0.1:${PORT}/terra.html`, { waitUntil: 'load' });
await p.waitForFunction(() => !!window.__terra, null, { timeout: 60000 });
await p.waitForTimeout(3000);

// Open the observer band so the LOD machinery is told the altitude the eye is really at, and pin the
// sky so the run is reproducible (the sun moving is a confound in any timing OR pixel comparison).
await p.evaluate(({ lat, lon, alt }) => {
  const t = window.__terra;
  t.set_alt_bounds(0.05, 8e10);
  t.set_epoch_sun_over_lon(lon + 70);
  t.set_fly(lat, lon, alt, 0.6, -0.45);
}, { lat: LAT, lon: LON, alt: ALT });

// Let the tiles for this site arrive before timing anything: a rebuild with no tiles loaded probes a
// 19.5 km raster, where cell/step < 1 forces a 1x1 grid, and is cheap for a reason that has nothing to
// do with the budget. Timing that would price the wrong thing entirely.
{
  const t0 = Date.now();
  let last = -1, stable = 0;
  while (Date.now() - t0 < 12000) {
    const n = await p.evaluate(() => (window.__tiles ? window.__tiles() : 0));
    if (n === last) { if (++stable >= 4) break; } else { stable = 0; last = n; }
    await p.waitForTimeout(150);
  }
  console.log(`(tiles loaded: ${last})`);
}

console.log(`--- pricing the appearance integral at ${ALT} m, ${LAT},${LON} ---`);
console.log('  side   probes/vertex   rebuild ms (median of ' + REPS + ')');
const rows = [];
for (const side of SIDES) {
  const times = [];
  for (let i = 0; i < REPS; i++) {
    const t = await p.evaluate(({ side }) => {
      const T = window.__terra;
      // ★ Invalidate and time in ONE SYNCHRONOUS BLOCK. Awaiting a frame in between hands control to
      // the PAGE'S OWN render loop, which consumes the invalidation and rebuilds the mesh itself — so
      // the call we then timed had nothing left to do and reported the steady-state 0.3 ms. That is
      // the rig measuring the rig. No await between the knob and the render.
      T.set_appearance_probes(side);
      const t0 = performance.now();
      T.render();
      return performance.now() - t0;
    }, { side });
    times.push(t);
    await p.waitForTimeout(120);
  }
  times.sort((a, b) => a - b);
  const med = times[times.length >> 1];
  rows.push({ side, probes: (side + 1) * (side + 1), med });
  console.log(`  ${String(side).padStart(4)}   ${String((side + 1) ** 2).padStart(13)}   ${med.toFixed(1)}`);
}

// Is it area-scaling? Fit against the smallest side and report what a probe-bound cost would predict.
const base = rows[0];
console.log('\n  --- is the cost in the probes? (a probe-bound cost scales with grid AREA) ---');
for (const r of rows) {
  const predicted = base.med * (r.probes / base.probes);
  console.log(`  side ${String(r.side).padStart(2)}: measured ${r.med.toFixed(1)} ms · probe-bound would predict ${predicted.toFixed(1)} ms`);
}
await b.close();
