// **The same Earth in every scene — Robin's rule, photographed.**
//
// Robin (2026-08-03), on the day the foliage materials landed: *"Because the scene just calls out
// which assemblies to include, we should be able to get enhanced renders of earth in all scenes from
// this work today. If not, we have a serious flaw in how we implement scene/assembly/engine."* And
// when that was called a prediction: *"Not a prediction, a confident assertion of the rules I've
// decreed (and a way to ensure they are being met)."*
//
// So this is the ensuring. A change to `data/materials.json` and to Earth's biome map — no scene code
// at all — must show up wherever Earth is drawn. Each scene here is pointed at the SAME spot on the
// planet from the same distance, with the sun put overhead so illumination is not a variable, and the
// frame is written for `tools/ground_colour.py` to decode. Land that comes back red/brown in one scene
// and green in another means two Earths, which is exactly what docs/63 exists to end.
import { launch } from './_launch.mjs';

const out = process.env.OUT || '/tmp/rigshot/one-earth';
const PORT = process.env.PORT || '5173';
// Over the Amazon, low enough that the surface fills the frame in every scene that can get there.
const LAT = -3.0;
const LON = -60.0;

// `page` is what the scene exposes on `window`, and `pose` is how that scene is asked to look at a
// coordinate. Different scene structs, one question.
const SCENES = [
  { page: 'terra.html', global: '__terra' },
  { page: 'yarr.html', global: '__terra' },
  { page: 'orbit.html', global: '__demo' },
  { page: 'groundzero.html', global: '__demo' },
  { page: 'twomoons.html', global: '__demo' },
];

const b = await launch();
for (const s of SCENES) {
  const p = await b.newPage({ viewport: { width: 640, height: 440 } });
  const errs = [];
  p.on('pageerror', (e) => errs.push(e.message));
  try {
    await p.goto(`http://127.0.0.1:${PORT}/${s.page}`, { waitUntil: 'load' });
    await p.waitForFunction((g) => !!window[g], s.global, { timeout: 60000 });
    await p.waitForTimeout(4000);
    // Two scene structs, two camera APIs: Terra flies to a coordinate, the space band orbits a focus.
    // Both are asked for the same thing — look at Earth from close enough to see its surface.
    const posed = await p.evaluate(
      ({ g, lat, lon }) => {
        const t = window[g];
        if (typeof t.set_epoch_sun_over_lon === 'function') t.set_epoch_sun_over_lon(lon);
        if (typeof t.set_fly === 'function') {
          t.set_alt_bounds?.(0.05, 8e10);
          t.set_fly(lat, lon, 40000, 0, -1.2);
          return 'set_fly';
        }
        if (typeof t.focus_earth === 'function') {
          // ★★ THE SPACE BAND'S CAMERA CANNOT BE SET FROM OUTSIDE, and calling `set_orbit` looks like
          // it works. `orbit.ts` re-drives `demo.set_orbit(cam.yaw, cam.pitch, cam.zoom)` from its OWN
          // `cam` object every single frame, so anything a rig pushes in is overwritten on the next
          // one — the first two versions of this rig did exactly that and reported 99% empty sky, which
          // reads as "this scene does not draw Earth" when the truth is that it was never asked to.
          // So drive it the way a PERSON does: press the Earth button, then pull the zoom slider in.
          if (typeof t.arc_stop === 'function' && t.arc_active?.()) t.arc_stop();
          const btn = [...document.querySelectorAll('button')].find((x) => /earth/i.test(x.textContent));
          if (btn) btn.click();
          const slider = document.querySelector('input[type=range]');
          if (slider) {
            slider.value = slider.min;
            slider.dispatchEvent(new Event('input', { bubbles: true }));
          }
          return `earth-btn=${!!btn} slider=${slider ? slider.value : 'none'}`;
        }
        return 'NO POSE CONTROL';
      },
      { g: s.global, lat: LAT, lon: LON },
    );
    await p.waitForTimeout(3000);
    await p.screenshot({ path: `${out}/${s.page.replace('.html', '')}.png` });
    // What the engine says the surface is made of there, where the scene can be asked.
    const mat = await p.evaluate(
      ({ g, lat, lon }) => {
        const t = window[g];
        return typeof t.surface_material_at === 'function' ? t.surface_material_at(lat, lon) : 'n/a';
      },
      { g: s.global, lat: LAT, lon: LON },
    );
    console.log(`  ${s.page.padEnd(16)} pose=${posed.padEnd(14)} material=${mat}${errs.length ? '  ERR:' + errs[0] : ''}`);
  } catch (e) {
    console.log(`  ${s.page.padEnd(16)} FAILED: ${e.message.split('\n')[0]}`);
  }
  await p.close();
}
await b.close();
