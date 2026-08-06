// Shoot ALL THREE scenes in one run. Purpose-built for changes to the SHARED render scaffolding
// (`crate::render`): a refactor there can only be trusted if every scene that draws through it still
// draws. Terrain alone would miss the space band and the globe, which use the same `GpuMesh`/
// `UniformSlot`/`Camera`/uniform PODs via completely different pipelines.
//
//   bash scripts/rigshot.sh all_scenes.mjs
import { launch, VIEWPORT } from './_launch.mjs';
const PORT = process.env.PORT || '5173';
const OUT = process.env.OUT || '/tmp';
const SCENES = [
  ['birth',   'birth.html',   9000],
  ['terra',   'terra.html',  10000],
];
const b = await launch();
// Floor calibrated from measurement, not guessed: each run prints a blank-page control cropped to the
// same rectangle, so the margin between "composited" and "did not" is visible every time rather than
// asserted once. Real renders measured 64-137 kB; see JOURNAL.md.
// ★ AMENDED 2026-08-05 — the absolute floor was calibrated against a background that no longer exists.
// Both scenes used to clear to a declared dark blue (0.01/0.01/0.03); with the sky derived, that painted
// colour was retired and space is BLACK (docs/66). A starfield on true black compresses far smaller, so
// the birth scene came in at 5,879 B against a 20,000 B floor and this rig called a perfectly good
// render blank. The scene was fine; the yardstick had moved.
//
// So the floor is now RELATIVE to the control this rig already measures — a genuinely blank page,
// cropped identically, in the same run. That is self-calibrating: it cannot drift when the background
// does, which is exactly what just happened to the absolute one.
const sizes = [];
const BLANK_RATIO = Number(process.env.BLANK_RATIO || 2.0);
let bad = 0;
for (const [name, page, wait] of SCENES) {
  const p = await b.newPage({ viewport: VIEWPORT });
  const errs = [];
  p.on('pageerror', (e) => errs.push(e.message));
  p.on('console', (m) => { const t = m.text(); if (/error|panic|unreachable|DEVICE_LOST/i.test(t)) errs.push(t); });
  await p.goto(`http://127.0.0.1:${PORT}/${page}`, { waitUntil: 'load' });
  await p.waitForTimeout(wait);
  const hud = (await p.locator('#stats').innerText().catch(() => '(no HUD)')).replace(/\s+/g, ' ').trim();
  await p.screenshot({ path: `${OUT}/scene-${name}.png` });
  // A BLANK canvas under a live HUD is the classic false-green here (it is exactly what xvfb produced).
  // Measure a canvas-only crop that excludes the HUD and nav: PNG is lossless, so a flat region
  // compresses to almost nothing while any real render does not. Bytes, not pixel stats, because that
  // needs no image library. The floor is calibrated from the measured renders below, not guessed.
  const crop = await p.screenshot({ clip: { x: 300, y: 120, width: 680, height: 460 } });
  console.log(`\n=== ${name} ===\n  HUD: ${hud}\n  canvas-crop png bytes: ${crop.length}`);
  sizes.push([name, crop.length]);
  if (errs.length) { bad++; console.log('  ERRORS:', errs.slice(0, 3)); }
  await p.close();
}
// The guard's own CONTROL: an genuinely blank page, cropped identically. This is what "the canvas did
// not composite" actually looks like on disk. Measured rather than assumed — a first attempt used a
// corner of the terra scene as the flat reference and it came back 39,992 B (only 1.6x below the real
// terra render), because the crop overlapped the globe. A blank page is unambiguous.
{
  const p = await b.newPage({ viewport: VIEWPORT });
  await p.goto('about:blank', { waitUntil: 'load' });
  const flat = await p.screenshot({ clip: { x: 300, y: 120, width: 680, height: 460 } });
  const floor = flat.length * BLANK_RATIO;
  console.log(`\nblank-page control: ${flat.length} B  ->  floor ${floor.toFixed(0)} B (${BLANK_RATIO}x)`);
  for (const [name, bytes] of sizes) {
    if (bytes < floor) {
      bad++;
      console.log(`  BLANK-ish canvas: ${name} at ${bytes} B — render did not composite`);
    } else {
      console.log(`  ${name}: ${bytes} B (${(bytes / flat.length).toFixed(1)}x the blank control)`);
    }
  }
  await p.close();
}
console.log(`\nscenes with errors: ${bad}/${SCENES.length}`);
await b.close();
