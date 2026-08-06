// **Does a followed fragment become VISIBLE on the night side if you ride it long enough?**
//
// The engine glows a body at its OWN temperature (`emission::incandescence`, ramping from ~800 K), and a
// meteor is only visible against a dark sky — so the night side is where it SHOULD show. An earlier
// screenshot found black, but at 481 km with 0 kg ablated: still above the atmosphere, nothing to glow
// yet. This rides the descent and photographs it AT the thresholds rather than guessing when.
import { launch, PORT, VIEWPORT } from './_launch.mjs';

const b = await launch();
const p = await b.newPage({ viewport: VIEWPORT });
p.on('pageerror', (e) => console.log('ERR', e.message));
await p.goto(`http://127.0.0.1:${PORT}/terra.html`, { waitUntil: 'load' });
await p.waitForFunction(() => !!window.__terra, null, { timeout: 60000 });
await p.waitForTimeout(3000);
await p.evaluate(() => window.launchSwarm());
await p.waitForTimeout(3000);
// Remember WHICH fragment we ride: `heaviest_fragment` re-picks as the swarm ablates, and following the
// temperature of a different body each sample would be a graph of the swarm, not of this descent.
const followed = await p.evaluate(() => {
  const f = window.__terra.heaviest_fragment();
  window.__fid = f.length ? f[0] : null;
  window.followFragment();
  return window.__fid;
});
console.log(`following fragment #${followed}`);

const read = () =>
  p.evaluate(() => {
    const t = window.__terra;
    const f = window.__fid != null ? t.fragment(window.__fid) : [];
    return {
      altKm: +(t.altitude_m() / 1000).toFixed(1),
      tempK: f.length ? Math.round(f[8]) : null,
      trailHotK: Math.round(t.trail_hot_k()),
      parcels: t.trail_parcels(),
      ablatedKg: +t.trail_mass_kg().toFixed(1),
    };
  });

const marks = [800, 1500, 2500];
// **Does the trail obscure the shot?** The camera sits behind the fragment and the ablation trail streams
// BACKWARDS along -v — i.e. straight at the lens. Photograph at rising parcel counts, while still
// following, so the question is answered by looking rather than by argument.
const parcelMarks = [1000, 4000, 8000];
let peak = 0;
let filmed = false;
let lastAlt = null;
for (let i = 0; i < 420; i++) {
  await p.waitForTimeout(1200);
  const s = await read().catch(() => null);
  if (!s) continue;
  if ((s.tempK ?? 0) > peak) peak = s.tempK ?? 0;
  if (i % 10 === 0 || (s.tempK ?? 0) > 700) console.log(`t+${((i + 1) * 1.2).toFixed(0)}s ${JSON.stringify(s)}`);
  // FILMSTRIP: a rapid sequence of SCREENSHOTS, nothing to do with the engine. (It was called a "burst"
  // and that was a bad name in this codebase — `damage::disrupt` bursting a body into the swarm is a real
  // physical event here, and the word was already taken. The rig computes no physics; it only decides
  // when to photograph.)
  //
  // Needed because the entry is the last ~2 SECONDS of a four-minute ride, so a threshold-triggered shot
  // always lands after it. Filming across it is the only way to see whether the fragment's own trail,
  // which streams backwards into the camera, obscures the shot.
  if (!filmed && (s.tempK ?? 0) > 900) {
    filmed = true;
    for (let k = 0; k < 8; k++) {
      const q = await read().catch(() => null);
      await p.screenshot({ path: `/tmp/frame-${k}.png` });
      console.log(`   frame ${k}: ${JSON.stringify(q)}`);
      await p.waitForTimeout(140);
    }
  }
  while (parcelMarks.length && (s.parcels ?? 0) >= parcelMarks[0]) {
    const m = parcelMarks.shift();
    await p.screenshot({ path: `/tmp/trail-${m}p.png` });
    console.log(`   >>> ${m} trail parcels at ${s.altKm} km, fragment ${s.tempK} K — shot written`);
  }
  while (marks.length && peak >= marks[0]) {
    const m = marks.shift();
    await p.screenshot({ path: `/tmp/nightglow-${m}K.png` });
    console.log(`   >>> fragment crossed ${m} K at ${s.altKm} km — shot written`);
  }
  lastAlt = s.altKm;
  if (s.tempK == null && peak > 0) { console.log('fragment gone (landed or consumed)'); break; }
  if (s.altKm < 2) break;
}
await p.screenshot({ path: '/tmp/nightglow-final.png' });
console.log(`PEAK fragment temp ${peak} K; last altitude ${lastAlt} km`);
await b.close();
