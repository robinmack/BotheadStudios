// Anchored ground tiers (docs/59 Stage B): a tier's mesh is built about a FIXED world point and reused
// while the camera moves, with `anchor - eye` carried in the model matrix. The arithmetic is pinned
// vertex-for-vertex in `ground_cap::an_anchored_cap_draws_exactly_where_an_eye_relative_one_did`; what a
// rig has to catch is the failure that unit test cannot see — the ground DRIFTING as the eye walks away
// from the anchor it was built about, which would look fine in the first frame after every rebuild and
// wrong in between.
//
// So: walk the camera across a tier's lifetime without letting it rebuild, and shoot the whole walk. A
// correct anchor holds the horizon and the terrain steady in the frame; a wrong one slides them.
// Also prices the walk, because the point of anchoring is that the frames between rebuilds are cheap.
import { launch } from './_launch.mjs';
const out = process.env.OUT || '/tmp/rigshot';
const PORT = process.env.PORT || '5173';
const TIERS = +(process.env.TIERS || 4);

const b = await launch();
const p = await b.newPage({ viewport: { width: 1000, height: 800 } });
p.on('pageerror', e => console.log('PAGEERR:', e.message));
await p.goto(`http://127.0.0.1:${PORT}/terra.html`, { waitUntil: 'load' });
await p.waitForFunction(() => !!window.__terra, null, { timeout: 60000 });
await p.waitForTimeout(3000);

// Pace to ~60 fps and time `render` — an uncapped rig INVENTS stalls (CLAUDE.md 4b).
await p.evaluate(() => {
  const t = window.__terra, orig = t.render.bind(t);
  window.__r = []; let last = 0;
  t.render = () => {
    const n = performance.now(); if (n - last < 16.7) return; last = n;
    const a = performance.now(); orig(); window.__r.push(performance.now() - a);
  };
});

const shoot = async (tag) => { await p.screenshot({ path: `${out}/anchor-${tag}.png` }); };
const stats = async () => p.evaluate(() => {
  const a = window.__r.slice(); window.__r.length = 0;
  if (!a.length) return { p50: 0, max: 0, n: 0 };
  const s = a.slice().sort((x, y) => x - y);
  return { p50: +s[Math.floor(a.length / 2)].toFixed(2), max: +Math.max(...a).toFixed(1), n: a.length };
});

// A LATERAL WALK at fixed altitude — the case the cache is built for. The tier is rebuilt once at the
// start (set_cap_ladder clears it), then held while the sub-point drifts into the margin CAP_MARGIN buys.
// 0.02° of latitude is ~2.2 km on the ground, well inside the ~24 km of slack at 500 m.
console.log(`--- lateral walk, ${TIERS} tier(s), 500 m ---`);
await p.evaluate((t) => { window.__terra.set_octave_budget(4); }, TIERS);
for (const [i, dlat] of [0, 0.004, 0.008, 0.012, 0.02].entries()) {
  await p.evaluate(({ dlat }) => window.__terra.set_fly(28 + dlat, 86, 500, 0.6, -0.20), { dlat });
  await p.waitForTimeout(1200);
  const s = await stats();
  await shoot(`walk${i}`);
  console.log(`  +${(dlat * 111).toFixed(2)} km  p50 ${s.p50} ms  max ${s.max} ms  (${s.n} frames)`);
}

// A DESCENT — the case that must still rebuild, and the one Robin's ask is about ("better LOD as we
// descend"). Each halving of altitude crosses the octave, so this should show rebuild spikes in `max`
// while p50 stays cheap.
console.log(`--- descent, ${TIERS} tier(s) ---`);
for (const alt of [64000, 32000, 16000, 8000, 4000, 2000, 1000, 500]) {
  await p.evaluate(({ alt }) => window.__terra.set_fly(28, 86, alt, 0.6, -0.30), { alt });
  await p.waitForTimeout(1200);
  const s = await stats();
  await shoot(`alt${alt}`);
  console.log(`  ${String(alt).padStart(6)} m  p50 ${s.p50} ms  max ${s.max} ms  (${s.n} frames)`);
}

await b.close();
console.log('done');
