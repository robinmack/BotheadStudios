// Does the trail DISSIPATE as it cools? (Robin, 2026-07-24.) Launch, let the entry happen, then watch the
// resolved parcels, their temperature, and the mass that has finished cooling into the air. A trail fading
// is those three numbers moving — measured, not asserted from a screenshot.
import { launch } from './_launch.mjs';
const out = process.env.OUT || '/tmp'; const PORT = process.env.PORT || '5173';
const b = await launch();
const p = await b.newPage({ viewport: { width: 1000, height: 800 } });
p.on('console', m => { const t = m.text(); if (t.startsWith('swarm')) console.log('PAGE:', t); });
p.on('pageerror', e => console.log('PAGEERR:', e.message));
await p.goto(`http://127.0.0.1:${PORT}/terra.html`, { waitUntil: 'load' });
await p.waitForFunction(() => !!window.__terra, null, { timeout: 60000 });
await p.waitForTimeout(3000);
await p.evaluate(() => window.__terra.set_fly(10, 0, 700000, 0, -0.55));
await p.waitForTimeout(1000);
await p.evaluate(() => window.launchSwarm());
const read = () => p.evaluate(() => {
  const t = window.__terra;
  return {
    parcels: t.trail_parcels(), hotK: Math.round(t.trail_hot_k()), meanK: Math.round(t.trail_mean_k()),
    aloftKg: +(t.trail_mass_kg() - t.trail_merged_kg()).toFixed(2),
    airKg: +t.trail_merged_kg().toFixed(2), totalKg: +t.trail_mass_kg().toFixed(2),
  };
});
// Sample every 2 s for 100 s: through the burn and well past it.
for (let i = 1; i <= 50; i++) {
  await p.waitForTimeout(2000);
  const s = await read();
  console.log(`t+${i * 2}s`, JSON.stringify(s));
  if (i % 5 === 0) await p.screenshot({ path: `${out}/fade-${i * 2}s.png` });
}
await b.close(); console.log('done');
