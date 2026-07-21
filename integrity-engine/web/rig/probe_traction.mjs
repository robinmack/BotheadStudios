// Probe-on-terrain rest + traction watch, for the swap of `collide_probe_with_terrain` onto
// `granular::terrain_contact_resolve` (docs/23).
//
// WHY THIS RIG EXISTS. The replaced code carried a hand-rolled dead zone whose comment explained it was
// there because hard-snapping the per-substep penetration of a RESTING probe "pumps potential energy into
// its stiff bonds every substep — that was the probe's 'free energy' (it vibrated apart and its scattered
// particles fell forever)". The new path drops that hack and relies on the constraint's bounded,
// velocity-decoupled position projection instead. So the regression to watch for is precisely: does the
// probe still REST, with its bonds intact?
//
// Asserts, from the engine's own telemetry rather than from pixels:
//   1. integrity stays 1.0            — no bond broke, i.e. no energy was pumped into the lattice
//   2. altitude converges             — it settles instead of sinking through or falling forever
//   3. it settles ABOVE the ground    — it rests on the surface, not in it
//
//   PORT=5173 node web/rig/probe_traction.mjs      (dev server must be running)
//   xvfb-run -a node web/rig/probe_traction.mjs    (headed Chromium is required — rig/README.md)
import { launch } from './_launch.mjs';

const PORT = process.env.PORT || '5173';
const b = await launch();
const p = await b.newPage({ viewport: { width: 1280, height: 800 } });
p.on('pageerror', (e) => console.log('PAGEERR:', e.message));
await p.goto(`http://127.0.0.1:${PORT}/terrain.html`, { waitUntil: 'load' });
await p.waitForTimeout(4000); // wasm compile + first frames

const sample = () =>
  p.evaluate(() => {
    const t = document.getElementById('stats')?.textContent ?? '';
    const alt = t.match(/alt\s*([-\d.]+)\s*m/);
    const integ = t.match(/integrity\s*(\d+)%/);
    return { alt: alt ? parseFloat(alt[1]) : null, integrity: integ ? parseInt(integ[1], 10) : null };
  });

// The probe spawns high and falls, so the landing transient must be allowed to ring down before the
// "has it settled" question is even meaningful. SAMPLES is env-tunable so this rig can be pushed out
// when a change is suspected of slowing convergence rather than preventing it.
const SAMPLES = parseInt(process.env.SAMPLES || '24', 10);
const series = [];
for (let i = 0; i < SAMPLES; i++) {
  await p.waitForTimeout(500);
  series.push(await sample());
}

const usable = series.filter((s) => s.alt !== null && s.integrity !== null);
if (usable.length < 8) {
  console.log('could not read probe telemetry from #stats — is the terrain scene running?');
  console.log('samples:', JSON.stringify(series.slice(0, 4)));
  await b.close();
  process.exit(1);
}

const tail = usable.slice(-8);
const alts = tail.map((s) => s.alt);
const spread = Math.max(...alts) - Math.min(...alts);
const minIntegrity = Math.min(...usable.map((s) => s.integrity));
const settled = alts[alts.length - 1];

console.log(`  samples          ${usable.length}`);
console.log(`  altitude trace   ${usable.map((s) => s.alt.toFixed(1)).join(' → ')}`);
console.log(`  integrity min    ${minIntegrity}%`);
console.log(`  settled altitude ${settled.toFixed(2)} m  (last-8 spread ${spread.toFixed(3)} m)`);

let bad = 0;
// 1. Bonds intact — the energy-pumping regression the dead zone used to mask.
if (minIntegrity < 100) { console.log(`  FAIL integrity fell to ${minIntegrity}% — bonds broke (energy injected?)`); bad++; }
// 2. Converged — resting, not drifting.
if (spread > 0.25) { console.log(`  FAIL altitude still moving ${spread.toFixed(3)} m across the last 8 samples`); bad++; }
// 3. On the surface, not through it. Altitude is measured to the ground, so a large negative means sunk.
if (settled < -1.0) { console.log(`  FAIL settled ${settled.toFixed(2)} m — probe sank through the surface`); bad++; }

console.log(bad === 0 ? '  PASS — probe rests intact on the terrain constraint' : `  ${bad} CHECK(S) FAILED`);
await b.close();
process.exit(bad === 0 ? 0 : 1);
