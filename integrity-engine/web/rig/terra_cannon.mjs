// **The acceptance test, as a picture** (docs/64): a working cannon on a working planet, emplaced on a
// coast, pointed at the sea, and fired by a button press — shot to splash.
//
// Robin: *"As long as we can build a working cannon and a working planet, and put a working cannon on a
// working planet and fire it, we know our assembly build is sound"*, and *"I want to be able to see the
// canon from the camera."*
//
// This rig does not assert that it works. It stands the camera at the gun, photographs what is there,
// presses the button, and photographs what happens — so the places it is wrong are found rather than
// assumed. A screenshot cannot see stutter (CLAUDE.md 4b), so the frames here answer a different
// question: is the cannon VISIBLE, and does firing it put something in the air.
import { launch } from './_launch.mjs';
const out = process.env.OUT || '/tmp/rigshot';
const PORT = process.env.PORT || '5173';
// The Pacific coast of Patagonia — the site `ballistics` found by scanning the shipped land mask for a
// headland whose shot lands in water. Not a chosen viewpoint: the physics picked it.
const LAT = +(process.env.LAT || -51);
const LON = +(process.env.LON || -75);
const BEARING = +(process.env.BEARING || 240);
const ALT = +(process.env.ALT || 8);

const b = await launch();
const p = await b.newPage({ viewport: { width: 1000, height: 800 } });
p.on('pageerror', e => console.log('PAGEERR:', e.message));
p.on('console', m => {
  const t = m.text();
  if (/cannon|error|panic|lost/i.test(t)) console.log('CONSOLE:', t);
});
await p.goto(`http://127.0.0.1:${PORT}/terra.html`, { waitUntil: 'load' });
await p.waitForFunction(() => !!window.__terra, null, { timeout: 60000 });
await p.waitForTimeout(3000);

// Stand on the shore, looking out along the firing bearing. The sun is PINNED so the run is
// reproducible — a free-running sky moves enough between runs to swamp a real change (docs/59).
await p.evaluate(({ lat, lon, alt, bearing }) => {
  const t = window.__terra;
  t.set_alt_bounds(0.05, 8e10);
  t.set_epoch_sun_over_lon(lon + 70);
  // Yaw is the bearing; a slight downward pitch so the gun at our feet is in frame.
  t.place_camera(lat, lon, alt, bearing * Math.PI / 180, -0.35);
}, { lat: LAT, lon: LON, alt: ALT, bearing: BEARING });
await p.waitForTimeout(2500);

const shot = async (name) => {
  await p.waitForTimeout(400);
  await p.screenshot({ path: `${out}/cannon-${name}.png` });
  console.log(`  ${name}`);
};

console.log(`--- cannon at ${LAT}, ${LON}, bearing ${BEARING} ---`);

// 1. Emplace it and LOOK. This is the frame that answers "can I see the cannon".
await p.evaluate(bearing => window.__terra.emplace_cannon(bearing), BEARING);
await p.waitForTimeout(1200);
await shot('1-emplaced');

// 2. Fire, via the same button a person would press — not the wasm method directly, because the button
// existing and being wired is half of what is being verified.
const before = await p.evaluate(() => window.__terra.cannon_shots());
const clicked = await p.evaluate(() => {
  const btn = [...document.querySelectorAll('button')].find(b => /fire cannon/i.test(b.textContent));
  if (!btn) return false;
  btn.click();
  return true;
});
if (!clicked) {
  console.log('  FAIL: no "Fire cannon" button in the HUD');
  await b.close();
  process.exit(1);
}
await p.waitForTimeout(300);
const after = await p.evaluate(() => window.__terra.cannon_shots());
console.log(`  shots ${before} -> ${after}`);
await shot('2-fired');

// 3. Watch the shot go. A 24-pounder is in the air for seconds, so sample across the flight.
for (const t of [600, 1400, 2600]) {
  await p.waitForTimeout(t);
  await shot(`3-flight-${t}ms`);
}

// What the engine says is in the air — the count is the engine's, not the rig's.
const airborne = await p.evaluate(() => window.__terra.swarm_count?.() ?? -1);
console.log(`  bodies in flight (engine's count): ${airborne}`);
console.log(`  shots fired: ${after}`);
if (after <= before) {
  console.log('  FAIL: the button did not fire the gun');
  await b.close();
  process.exit(1);
}
await b.close();
