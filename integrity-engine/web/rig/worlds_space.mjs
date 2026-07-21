// docs/43 — verify the Space + Two Moons scenes now load from DATA worlds and the deorbit physics still works.
import { launch } from './_launch.mjs';
const out = process.env.OUT || '/tmp'; const PORT = process.env.PORT || '5173';
const b = await launch();
const p = await b.newPage({ viewport: { width: 1000, height: 800 } });
p.on('pageerror', e => console.log('PAGEERR:', e.message));
p.on('console', m => { const t = m.text(); if (t.includes('system world') || t.includes('error') || t.includes('Error')) console.log('PAGE:', t); });

async function load(url) {
  await p.goto(`http://127.0.0.1:${PORT}/${url}`, { waitUntil: 'load' });
  await p.waitForTimeout(3500);
}
const rd = () => p.evaluate(() => ({
  focus: window.__demo?.focus_label?.(),
  moon_km: Math.round(window.__demo?.moon_distance_km?.() ?? -1),
  impacted: window.__demo?.has_impacted?.(),
  debris: window.__demo?.debris_count?.(),
  tscale: window.__demo?.time_scale_value?.(),
}));

const shot = (name) => p.screenshot({ path: `${out}/${name}.png`, timeout: 8000 }).catch((e) => console.log(`shot ${name} skipped:`, e.message.slice(0, 60)));

// --- Space (one-moon world) ---
await load('orbit.html');
console.log('SPACE loaded:', JSON.stringify(await rd()));
await shot('space-load');

// Deorbit: drop the moon (cancel orbital velocity → radial infall), let the fast sim run, expect an impact.
await p.evaluate(() => window.__demo.drop_moon());
for (let i = 0; i < 12; i++) {
  await p.waitForTimeout(1000);
  const s = await rd();
  console.log(`  t+${i + 1}s: moon ${s.moon_km} km · impacted ${s.impacted} · debris ${s.debris}`);
  if (s.impacted) break;
}
await shot("space-deorbit");

// --- Two Moons world ---
await load('twomoons.html');
console.log('TWO MOONS loaded:', JSON.stringify(await rd()));
await shot("twomoons-load");

await b.close(); console.log('done');
