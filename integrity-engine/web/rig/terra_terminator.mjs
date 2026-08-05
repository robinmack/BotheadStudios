// **The day/night boundary from space, and the haze along it** (docs/66).
//
// Robin: *"Bonus points if we see the haze of the atmosphere at day/night boundary from space."*
//
// This is the one thing the retired closed form could not draw at all, and the reason is structural, not
// a matter of degree. `rayleigh_veil` is a flat slab, so its terminator is wherever `µs` crosses zero —
// a knife edge — and it needed a DECLARED `sqrt(2H/R)` ramp bolted on to blur it. The march has the
// geometry instead: past the terminator the low air is inside the planet's own shadow while the air
// ABOVE it is still in sunlight, so the boundary is a gradient the height of the atmosphere, and the
// light that survives to reach it has crossed the whole column and is therefore RED.
//
// Nothing here is told to draw a band. It is the shadow test in `column_to_space`.
//
// Measured within ONE frame, so no exposure, epoch or camera difference can explain the result:
//   day side  →  the boundary  →  night side, across a scan that crosses all three.
//
// Run:  scripts/rigshot.sh terra_terminator.mjs
import { launch, VIEWPORT } from './_launch.mjs';
import { decodePng } from './_png.mjs';
import { BLUE_MARBLE, LOW_ORBIT, discHalfWidth, pose, wholeDiscAltitude } from './_poses.mjs';

const out = process.env.OUT || '/tmp/rigshot';
const PORT = process.env.PORT || '5173';
const EPOCH = 1718945000; // the June solstice, so the terminator leans

const LAT = 10;
const LON = 0;

const b = await launch();
const p = await b.newPage({ viewport: VIEWPORT });
p.on('pageerror', (e) => console.log('PAGEERR:', e.message));
await p.goto(`http://127.0.0.1:${PORT}/terra.html`, { waitUntil: 'load' });
await p.waitForFunction(() => !!window.__terra, null, { timeout: 60000 });
await p.waitForTimeout(3500);

/** One frame from a named pose. */
async function shot(name, where) {
  await pose(p, { ...where, lat: LAT, lon: LON }, { epoch: EPOCH, settleMs: 2400 });
  await p.screenshot({ path: `${out}/terminator-${name}.png` });
  return decodePng(await p.screenshot());
}

/** Mean RGB of a vertical strip, x as a fraction of width. */
const strip = (img, x0, x1) => {
  const c = img.channels;
  let r = 0, g = 0, bl = 0, n = 0;
  for (let y = Math.floor(0.25 * img.height); y < Math.floor(0.75 * img.height); y++) {
    for (let x = Math.floor(x0 * img.width); x < Math.floor(x1 * img.width); x++) {
      const i = (y * img.width + x) * c;
      r += img.data[i]; g += img.data[i + 1]; bl += img.data[i + 2]; n++;
    }
  }
  return n ? [r / n, g / n, bl / n] : [0, 0, 0];
};
const lum = (c) => (c[0] + c[1] + c[2]) / 3;
const f = (v) => v.map((x) => x.toFixed(1)).join('/');
const ok = (name, cond, detail) => console.log(`${cond ? 'PASS' : 'FAIL'}  ${name} — ${detail}`);

console.log(`viewport ${VIEWPORT.width}x${VIEWPORT.height}`);

// ★ THE POSE IS THE MEASUREMENT, AND THE ASSEMBLY OWNS THE BOUNDARY (docs/66 §10).
//
// Robin, on an earlier version of this rig that located the planet's edge by scanning pixel columns:
// *"This should be done in the engine as a boundary between the assembly (containing the atmosphere as
// a component of Earth) and space."* She is right, and the scan below is a STOPGAP kept only until a
// scene verb can state it (docs/46 row 44, half open). What the rig CAN do honestly is predict where
// the boundary should be from the assembly's own geometry and check the picture against it, which is
// what `discHalfWidth` does — and the prediction disagrees with the picture from high orbit, which is
// a finding rather than a failed test. See the note printed below.
console.log(`whole disc fits from ${(wholeDiscAltitude() / 1000).toFixed(0)} km`);
const marble = await shot('marble', { ...BLUE_MARBLE, sunLon: LON + 90 });
// ASSERTIONS run on the LOW-ORBIT frame, where the segment covers what is in view. From the whole-disc
// pose it does not (see below), so a scan there measures the mesh's edge, not the assembly's.
const img = await shot('low-orbit', { ...LOW_ORBIT, sunLon: LON + 90 });

{
  // Where the assembly's edge SHOULD fall, from its own extent — no pixels involved.
  const predicted = discHalfWidth(BLUE_MARBLE.alt_m, VIEWPORT.width / VIEWPORT.height);
  const c = marble.channels;
  let lo = 1, hi = 0;
  for (let x = 0; x < marble.width; x++) {
    const y = Math.floor(marble.height / 2);
    const i = (y * marble.width + x) * c;
    if (marble.data[i] + marble.data[i + 1] + marble.data[i + 2] > 3) {
      lo = Math.min(lo, x / marble.width);
      hi = Math.max(hi, x / marble.width);
    }
  }
  const drawn = (hi - lo) / 2;
  console.log(
    `\nASSEMBLY BOUNDARY from ${(BLUE_MARBLE.alt_m / 1000).toFixed(0)} km: the Earth assembly (rock + ` +
      `~97 km of air) should span ${(2 * predicted * 100).toFixed(1)}% of the frame width; ` +
      `${(2 * drawn * 100).toFixed(1)}% is drawn.`,
  );
  if (drawn < 0.8 * predicted) {
    console.log(
      `  FINDING: the surface segment does not reach the assembly's own edge from here — the picture ` +
        `is smaller than the body is. Not a sky defect; a coverage one (docs/63's segment extent).`,
    );
  }
}

// Scan across the frame in twenty strips and find where the brightness falls off — that IS the
// terminator, located by measurement rather than by assuming where it should be.
const N = 20;
const bands = [];
for (let i = 0; i < N; i++) bands.push(strip(img, i / N, (i + 1) / N));
const bright = Math.max(...bands.map(lum));
console.log('scan across the disc (R/G/B, luminance):');
bands.forEach((c, i) => {
  const bar = '#'.repeat(Math.round((32 * lum(c)) / Math.max(bright, 1)));
  console.log(`  ${String(i).padStart(2)} ${f(c).padStart(18)}  ${lum(c).toFixed(1).padStart(6)}  ${bar}`);
});

// ★ EXCLUDE SPACE AND THE LIMB. The first version of this analysis scanned the whole frame and found
// its "steepest fall" at the edge of the DISC — the limb against black sky, which is a real edge and a
// different feature entirely. It then reported a knife-edge terminator that was not the terminator. The
// disc is where the frame is not exactly black; the outermost band of it is the limb; the terminator is
// what is left.
const inDisc = bands.map((c) => lum(c) > 0.5);
const first = inDisc.indexOf(true);
const last = inDisc.lastIndexOf(true);
const lo = first + 1, hi = last - 1; // drop one band at each edge: that is the limb
const disc = bands.slice(lo, hi + 1);
console.log(`disc spans bands ${first}..${last}; limb at ${first} and ${last}; scanning ${lo}..${hi}`);
console.log(`  limb band ${last}: ${f(bands[last])} — R/B ${(bands[last][0] / Math.max(bands[last][2], 0.01)).toFixed(2)}`);

const day = disc.reduce((a, c) => (lum(c) > lum(a) ? c : a));
const night = disc.reduce((a, c) => (lum(c) < lum(a) ? c : a));
let edge = 0, drop = 0;
for (let i = 1; i < disc.length; i++) {
  const d = Math.abs(lum(disc[i]) - lum(disc[i - 1]));
  if (d > drop) { drop = d; edge = i; }
}
// The terminator band: halfway down the fall from day to night, found by value rather than by index.
const mid = (lum(day) + lum(night)) / 2;
const band = disc.reduce((a, c) => (Math.abs(lum(c) - mid) < Math.abs(lum(a) - mid) ? c : a));
const red = (c) => c[0] / Math.max(c[2], 0.01);

console.log(`\nday ${f(day)}   terminator ${f(band)}   night ${f(night)}   (steepest step at disc band ${edge})`);
ok(
  'the day/night boundary is a gradient, not an edge',
  drop < 0.7 * (lum(day) - lum(night)),
  `the steepest single step across the disc is ${drop.toFixed(1)} of a ` +
    `${(lum(day) - lum(night)).toFixed(1)} total fall — spread over ` +
    `${Math.round((lum(day) - lum(night)) / Math.max(drop, 0.01))} bands, not one`,
);
ok(
  'and the light along it is REDDER than the daylight it fades from',
  red(band) > 1.1 * red(day),
  `R/B ${red(band).toFixed(2)} at the boundary vs ${red(day).toFixed(2)} in full day — the blue is ` +
    `removed on the long slant path before it can scatter`,
);
ok(
  'the night side is genuinely dark',
  lum(night) < 0.25 * lum(day),
  `${f(night)} against ${f(day)}`,
);

await b.close();
