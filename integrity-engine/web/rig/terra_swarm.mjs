// docs/59 — the meteor swarm, seen from orbit. Presses the button the way a person would, then watches
// the entry: a frame before, and frames through the burn, with the engine's own counts read out so the
// picture is checked against what the physics says is there (a screenshot cannot see an empty buffer).
import { launch } from './_launch.mjs';
const out = process.env.OUT || '/tmp'; const PORT = process.env.PORT || '5173';
const ALT = Number(process.env.ALT || 700000);   // camera altitude, metres
const LAT = Number(process.env.LAT ?? 10);
const LON = Number(process.env.LON ?? 0);
const YAW = Number(process.env.YAW ?? 0);
const PITCH = Number(process.env.PITCH ?? -0.55);
const b = await launch();
const p = await b.newPage({ viewport: { width: 1000, height: 800 } });
p.on('console', m => { const t = m.text(); if (!t.includes('[vite]')) console.log('PAGE:', t); });
p.on('pageerror', e => console.log('PAGEERR:', e.message));
await p.goto(`http://127.0.0.1:${PORT}/terra.html`, { waitUntil: 'load' });
await p.waitForFunction(() => !!window.__terra, null, { timeout: 60000 });
await p.waitForTimeout(3000);

// Put the camera on the NIGHT side looking down: an entry trail is a faint emitter, and against a sunlit
// ocean it is invisible for the same reason a real meteor is invisible at noon.
const stat = await p.evaluate(async ({alt, LAT, LON, YAW, PITCH}) => {
  const t = window.__terra;
  if (!t) return { err: 'no terra' };
  // Sweep longitude for the darkest view (the globe is oriented to real time, so "night" moves).
  // Frame the LIMB from orbit: the classic view of an entry from the ISS — the sunlit edge of the world
  // along the top, the dark side below it, and the streak crossing the dark. Looking straight down at the
  // night side is honest and completely black, which verifies nothing.
  t.set_fly(LAT, LON, alt, YAW, PITCH);
  return { world: t.world_name(), alt: t.altitude_m(), lat: t.latitude(), lon: t.longitude() };
}, { alt: ALT, LAT, LON, YAW, PITCH });
console.log('camera:', JSON.stringify(stat));
await p.waitForTimeout(1200);
await p.screenshot({ path: `${out}/swarm-0-before.png` });

await p.evaluate(() => window.launchSwarm());
const shots = [];
const STEP = Number(process.env.STEP || 5000);
const N = Number(process.env.N || 18);
for (let i = 1; i <= N; i++) {
  await p.waitForTimeout(STEP);
  const s = await p.evaluate(() => {
    const t = window.__terra;
    return {
      inFlight: t.flight_count(), drawn: t.drawn_count(),
      ablated: +t.trail_mass_kg().toFixed(2),
      minAltKm: +t.swarm_min_alt_km().toFixed(1), vKms: +t.swarm_speed_kms().toFixed(2),
    };
  });
  shots.push(s);
  console.log(`t+${((i * STEP) / 1000).toFixed(1)}s`, JSON.stringify(s));
  await p.screenshot({ path: `${out}/swarm-${i}.png` });
}
console.log('SUMMARY', JSON.stringify(shots));
await b.close(); console.log('done');
