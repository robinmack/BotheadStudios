// docs/59 Stage B: ride a fragment from orbit to the ground and check the view actually descends and the
// terrain resolves finer as it does. Paced to ~60 fps, because an uncapped rig both invents stalls and
// (with the 1/30 s dt clamp) makes the sim run at roughly half speed — see CLAUDE.md rule 4b.
import { launch } from './_launch.mjs';
const out = process.env.OUT || '/tmp'; const PORT = process.env.PORT || '5173';
const b = await launch();
const p = await b.newPage({ viewport: { width: 1000, height: 800 } });
p.on('console', m => { const t = m.text(); if (t.startsWith('swarm') || t.startsWith('arrival')) console.log('PAGE:', t); });
p.on('pageerror', e => console.log('PAGEERR:', e.message));
await p.goto(`http://127.0.0.1:${PORT}/terra.html`, { waitUntil: 'load' });
await p.waitForFunction(() => !!window.__terra, null, { timeout: 60000 });
await p.waitForTimeout(3000);
await p.evaluate(({ LAT, LON }) => {
  const t = window.__terra;
  const orig = t.render.bind(t);
  window.__r = []; let lastRender = 0;
  t.render = () => {
    const now = performance.now();
    if (now - lastRender < 16.7) return;
    lastRender = now;
    const a = performance.now(); orig(); window.__r.push(performance.now() - a);
  };
  t.place_camera(LAT, LON, 700000, 0, -0.55);
  window.launchSwarm();
}, { LAT: Number(process.env.LAT ?? 10), LON: Number(process.env.LON ?? 0) });
await p.waitForTimeout(2000);
await p.evaluate(() => window.followFragment());
const alts = [];
const gatesLeft = new Set([400, 100, 40, 15, 5, 1]);
for (let i = 1; i <= 40; i++) {
  await p.evaluate(() => { window.__r.length = 0; });
  await p.waitForTimeout(2000);
  const r = await p.evaluate(() => {
    const t = window.__terra, a = window.__r;
    return {
      altKm: +(t.altitude_m() / 1000).toFixed(1), lat: +t.latitude().toFixed(2), lon: +t.longitude().toFixed(2),
      inFlight: t.flight_count(), drawn: t.drawn_count(), ablated: Math.round(t.trail_mass_kg()),
      renderMax: a.length ? +Math.max(...a).toFixed(1) : 0, renders: a.length,
    };
  });
  alts.push(r.altKm);
  // Shots on the way down, at roughly decade steps of altitude.
  for (const gate of [...gatesLeft].sort((a, c) => c - a)) {
    if (r.altKm <= gate) { await p.screenshot({ path: `${out}/follow-${gate}km.png` }); gatesLeft.delete(gate); break; }
  }
  console.log(`t+${i * 2}s`, JSON.stringify(r));
  if (r.inFlight === 0) break;
}
const desc = alts.filter((a, i) => i > 0 && a < alts[i - 1]).length;
console.log(`ALTITUDES ${alts[0]} -> ${alts[alts.length - 1]} km; ${desc}/${alts.length - 1} samples descending`);
await b.close(); console.log('done');
