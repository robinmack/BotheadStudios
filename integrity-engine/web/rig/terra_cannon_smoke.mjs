// **Smoke and flash, photographed** — and neither is an effect.
//
// Robin: *"will there be a cloud of smoke? There should be... This should also be a natural product of
// the engine"*, *"also likely flash and fire would be visible"*, and *"smoke and flash should emerge
// naturally from the detonation/shape of barrel/velocity/amount of material, not the scene."*
//
// What this photographs is `oxidation` deciding how much of the charge left as permanent gas and how
// much stayed condensed, `ballistics::Ejecta` deciding where the barrel ends and how fast the jet
// leaves, and `flight::shed_at` putting that matter into the air through the same door an ablating
// meteor's vapour uses. The glow is `emission::incandescence` on matter at the products' own flame
// temperature — the same law that lights a meteor. Nothing here draws a muzzle flash.
//
// The camera stands BEHIND and BESIDE the gun, because the interesting thing happens at the muzzle and
// a camera at the gun's own feet has it behind the HUD.
import { launch } from './_launch.mjs';
const out = process.env.OUT || '/tmp/rigshot';
const PORT = process.env.PORT || '5173';
const LAT = +(process.env.LAT || -51);
const LON = +(process.env.LON || -75);
const B = +(process.env.BEARING || 240);

// Step `d` metres along a compass bearing from the gun.
const along = (d, bearing) => {
  const r = bearing * Math.PI / 180;
  return [
    LAT + d * Math.cos(r) / 111320,
    LON + d * Math.sin(r) / (111320 * Math.cos(LAT * Math.PI / 180)),
  ];
};

const b = await launch();
const p = await b.newPage({ viewport: { width: 900, height: 620 } });
p.on('pageerror', e => console.log('PAGEERR:', e.message));
p.on('console', m => { const t = m.text(); if (/cannon|arrival/i.test(t)) console.log('CONSOLE:', t); });
await p.goto(`http://127.0.0.1:${PORT}/terra.html`, { waitUntil: 'load' });
await p.waitForFunction(() => !!window.__terra, null, { timeout: 60000 });
await p.waitForTimeout(3000);

// Emplace at the site, with the sun low enough to light the ground without washing out a glow.
await p.evaluate(({ lat, lon, B }) => {
  const t = window.__terra;
  t.set_alt_bounds(0.05, 8e10);
  t.set_epoch_sun_over_lon(lon + 55);
  t.set_fly(lat, lon, 3, B * Math.PI / 180, -0.1);
  t.emplace_cannon(B);
}, { lat: LAT, lon: LON, B });
await p.waitForTimeout(1200);

// Stand off the gun's quarter: back along the reverse bearing and out to one side, looking at the
// muzzle, so both the gun and the space in front of it are in frame.
// Stand off the gun's quarter and AIM AT THE MUZZLE. The first version pointed the camera 32 degrees
// off the gun's bearing, which put the muzzle — and therefore the whole event — just past the frame
// edge: 160 parcels drawn and nothing visible. The gun is 3 m long, so the camera has to look where it
// points, not where it stands.
const [la, lo] = along(9, B + 125);
await p.evaluate(({ la, lo, B }) => window.__terra.set_fly(la, lo, 2.6, (B - 62) * Math.PI / 180, -0.02),
  { la, lo, B });
await p.waitForTimeout(900);
await p.screenshot({ path: `${out}/smoke-0-before.png` });
console.log('  0-before');

// Fire through the HUD button a person would press.
const ok = await p.evaluate(() => {
  const btn = [...document.querySelectorAll('button')].find(b => /fire cannon/i.test(b.textContent));
  if (!btn) return false;
  btn.click();
  return true;
});
if (!ok) { console.log('  FAIL: no Fire cannon button'); await b.close(); process.exit(1); }

// The flash is brief and the smoke lingers, so sample fast then slow.
const report = async (name) => {
  const s = await p.evaluate(() => ({
    parcels: window.__terra.trail_parcels?.() ?? -1,
    kg: window.__terra.trail_mass_kg?.() ?? -1,
    drawn: window.__terra.drawn_count?.() ?? -1,
    flying: window.__terra.swarm_count?.() ?? -1,
  }));
  console.log(`  ${name}: ${s.parcels} parcels, ${s.kg.toFixed(2)} kg airborne, ${s.drawn} drawn`);
};

// **The muzzle.** Flash first, then the cloud it becomes.
// ★ The FIRST frame has to be immediate. The jet leaves at ~295 m/s, so at 70 ms it is already 20 m
// downrange and out of frame — a cloud photographed too late is a cloud you conclude is not there.
for (const [wait, name] of [[0, '0-muzzle'], [16, '1-flash'], [60, '2-jet'], [400, '3-cloud'], [1500, '4-drift']]) {
  await p.waitForTimeout(wait);
  await p.screenshot({ path: `${out}/smoke-${name}.png` });
  await report(name);
}

// **Then PAN to follow the shot out to sea.** The camera turns and lifts to keep the falling ball in
// view — a camera decision, made from where the engine says its matter IS. The engine has no notion of
// "following" and does not need one (docs/59): it is fed a pose.
console.log('--- panning downrange ---');
for (const [i, [wait, pitch, alt]] of [[400, -0.05, 6], [700, -0.02, 14], [900, 0.02, 26], [1200, 0.05, 40]].entries()) {
  const [w, pi, al] = [wait, pitch, alt];
  await p.waitForTimeout(w);
  await p.evaluate(({ la, lo, bearing, pi, al }) => window.__terra.set_fly(la, lo, al, bearing * Math.PI / 180, pi),
    { la, lo, bearing: B, pi, al });
  await p.waitForTimeout(250);
  await p.screenshot({ path: `${out}/pan-${i}-downrange.png` });
  await report(`pan-${i}`);
}

// **The splash.** Move the camera out over the water to where the shot comes down and watch it arrive.
const [sla, slo] = along(4800, B);
await p.evaluate(({ sla, slo, bearing }) => window.__terra.set_fly(sla, slo, 120, (bearing + 180) * Math.PI / 180, -0.35),
  { sla, slo, bearing: B });
for (const [wait, name] of [[600, '0-approach'], [1200, '1-impact'], [1500, '2-after']]) {
  await p.waitForTimeout(wait);
  await p.screenshot({ path: `${out}/splash-${name}.png` });
  await report(`splash-${name}`);
}
await b.close();
