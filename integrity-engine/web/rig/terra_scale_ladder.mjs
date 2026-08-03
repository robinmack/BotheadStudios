// **THE NORTH STAR, as an instrument** (docs/13, docs/23, docs/59): one continuous representation from
// Mars-Earth distance down to 10 cm above the ground — ~12 orders of magnitude — with detail resolving
// automatically and no frame-rate cliff.
//
// Robin: *"from mars-earth distance to 10 cm above the surface … so we can prove our frame of
// reference/increased detail system works, with visual details resolving automatically so they look
// reasonably accurate all along the path without noticeable frame-rate impact."*
//
// This rig does not assert that it works. It WALKS the ladder and reports what is actually on the screen
// and what each rung costs, so the places it stops being continuous are found rather than assumed. A
// screenshot cannot see stutter (CLAUDE.md 4b), so every rung is paced to ~60 fps and timed.
//
// The frame is also measured, not just photographed: `ink` is the fraction of the render area that is not
// background, which is how a rung that draws NOTHING is told apart from one that draws a dark planet;
// `detail` is the standard deviation of luminance over the ground region, which is what "detail resolving"
// has to move if the claim is true.
import { launch } from './_launch.mjs';
const out = process.env.OUT || '/tmp/rigshot';
const PORT = process.env.PORT || '5173';
// Mars at a typical opposition, ~0.52 AU. The top of the ladder is a DISTANCE, not an altitude the fly
// camera was built for, which is the point.
const TOP_M = +(process.env.TOP || 7.8e10);
const BOTTOM_M = +(process.env.BOTTOM || 0.1);
const PER_DECADE = +(process.env.PER_DECADE || 2); // rungs per decade
// A high-relief site. The sun is BROUGHT to it (`set_epoch_sun_over_lon`) rather than waited for, so this
// rig renders the same frames whatever time it is run — see the note by the epoch call below.
const LAT = +(process.env.LAT || 39);
const LON = +(process.env.LON || -106);

const b = await launch();
const p = await b.newPage({ viewport: { width: 1000, height: 800 } });
p.on('pageerror', e => console.log('PAGEERR:', e.message));
p.on('console', m => { const t = m.text(); if (/error|panic|lost/i.test(t)) console.log('CONSOLE:', t); });
await p.goto(`http://127.0.0.1:${PORT}/terra.html`, { waitUntil: 'load' });
await p.waitForFunction(() => !!window.__terra, null, { timeout: 60000 });
await p.waitForTimeout(3000);

// Open the observer's band to the whole ladder, so the LOD machinery is told the altitude the eye is
// actually at. Without this the rig measures the world file's declared 2 m / 40,000 km clamp.
// BOUNDS=0 runs the CONTROL: the world's own declared band, to attribute any failure to the knob or to
// the engine rather than guessing which.
const BOUNDS = process.env.BOUNDS !== '0';
if (BOUNDS) {
  await p.evaluate(({ TOP_M, BOTTOM_M }) => {
    window.__terra.set_alt_bounds(BOTTOM_M, TOP_M * 2);
  }, { TOP_M, BOTTOM_M });
}
console.log(BOUNDS ? '(observer band widened to the ladder)' : '(CONTROL: world-declared band, 2 m - 40,000 km)');

// ★ PIN THE SKY. The first run of this rig came back black below 4,300 km and looked exactly like a
// renderer collapse; it was lon 86 at 17:00 UTC, i.e. the middle of the night. A rig should command the
// clock rather than wait for the sun (Robin), so ask the engine for the instant that puts the daylight
// over the site. This also makes the run REPRODUCIBLE: with a free-running sky, two runs of identical
// code differ by mean 2.5-4.8/255 from the sun and stars moving alone, which is enough to swamp a real
// change. SUN=0 leaves the sky free-running.
// ★ And put it LOW, not overhead. Aiming the sun at the site's own longitude is local NOON, which is the
// worst possible light for showing relief: no shadows, so a mountain range and a billiard table look
// alike. Measured here — the same Himalaya rungs read detail 0.65 at noon against 4.12 for a site under
// slanting light. Offsetting the subsolar longitude east of the site gives morning light and real shading,
// which is what "details resolving so they look reasonably accurate" actually depends on.
const SUN_OFFSET = +(process.env.SUN_OFFSET || 70);
if (process.env.SUN !== '0') {
  const [t, ss] = await p.evaluate(({ LON, SUN_OFFSET }) => {
    const t = window.__terra.set_epoch_sun_over_lon(LON + SUN_OFFSET);
    return [t, window.__terra.sub_solar()];
  }, { LON, SUN_OFFSET });
  console.log(`(sky pinned to ${new Date(t * 1000).toISOString()} — subsolar ${ss[0].toFixed(1)}, ${ss[1].toFixed(1)})`);
}

await p.evaluate(() => {
  const t = window.__terra, orig = t.render.bind(t);
  window.__r = []; let last = 0;
  t.render = () => {
    const n = performance.now(); if (n - last < 16.7) return; last = n;
    const a = performance.now(); orig(); window.__r.push(performance.now() - a);
  };
});

const tilesAt = [];
const rungs = [];
const decades = Math.log10(TOP_M / BOTTOM_M);
const n = Math.round(decades * PER_DECADE);
for (let i = 0; i <= n; i++) rungs.push(TOP_M * Math.pow(BOTTOM_M / TOP_M, i / n));

const fmt = (m) => m >= 1e9 ? `${(m / 1e9).toFixed(2)} Gm` : m >= 1e6 ? `${(m / 1e6).toFixed(2)} Mm`
  : m >= 1e3 ? `${(m / 1e3).toFixed(2)} km` : `${m.toFixed(2)} m`;

console.log(`--- ${n + 1} rungs, ${decades.toFixed(1)} decades: ${fmt(TOP_M)} -> ${fmt(BOTTOM_M)} ---`);
console.log('  altitude        p50 ms   worst ms   fps  tiles');
for (const [i, alt] of rungs.entries()) {
  await p.evaluate(({ alt, LAT, LON }) => {
    window.__terra.set_fly(LAT, LON, alt, 0.6, -0.45);
    window.__r.length = 0;
  }, { alt, LAT, LON });
  await p.waitForTimeout(900);
  // Wait for the streamed elevation patch to settle before believing the frame: a screenshot taken while
  // tiles are still in flight is a picture of the network, not of the engine. Bounded, because at high
  // altitude or over ocean there may be nothing to fetch and that is a legitimate answer.
  {
    const t0 = Date.now();
    let last = -1, stable = 0;
    while (Date.now() - t0 < 4000) {
      const n = await p.evaluate(() => (window.__tiles ? window.__tiles() : 0));
      if (n === last) { if (++stable >= 3) break; } else { stable = 0; last = n; }
      await p.waitForTimeout(120);
    }
    tilesAt.push(last);
  }
  const s = await p.evaluate(() => {
    const a = window.__r; if (!a.length) return { p50: 0, max: 0, n: 0 };
    const q = a.slice().sort((x, y) => x - y);
    return { p50: +q[Math.floor(a.length / 2)].toFixed(2), max: +Math.max(...a).toFixed(1), n: a.length };
  });
  const shot = `${out}/scale-${String(i).padStart(2, '0')}-${alt.toExponential(1)}.png`;
  await p.screenshot({ path: shot });
  // ★ NO in-page pixel readback here. A WebGPU canvas is only readable while its drawing buffer is
  // current (CLAUDE.md rule 0), so `drawImage(canvas)` from a later tick silently yields BLANK — the
  // first version of this rig did exactly that and reported 0% ink for frames that plainly contained a
  // planet. The PNGs are the measurement; analyse them after the run.
  const alive = await p.evaluate(() => window.__terra.altitude_m());
  console.log(
    `  ${fmt(alt).padStart(10)}  ${String(s.p50).padStart(7)}  ${String(s.max).padStart(8)}  ` +
    `${String(s.n ? Math.round(s.n / 0.9) : 0).padStart(4)}  ${String(tilesAt[tilesAt.length - 1]).padStart(5)}` +
    `${(Math.abs(alive - alt) / alt > 0.01 ? `   alt clamped -> ${fmt(alive)}` : '')}`
  );
}
await b.close();
console.log('done');
