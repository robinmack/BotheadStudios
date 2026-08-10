// **Does Terra have a sky, and is it the air?** (docs/66)
//
// `shaders/sky.wgsl` was compiled by NOTHING until today: Terra lit the ground correctly and the sky
// region fell through to the star field, so a daylight frame near the ground was lit grass under a
// black starfield (docs/46 row 41). This measures whether that is fixed AND whether what replaced it
// is the declared atmosphere rather than a gradient that happens to look like one.
//
// ★★ THE CONTROL IS THE POINT. On 2026-08-04 this project reported "Ireland turns" from a measurement
// that was actually the sun's own elevation, because the run had no negative control. So every claim
// here is a DIFFERENCE against a frame that must not show the effect:
//
//   1. `?world=earth-airless` — the same Earth, the same ground, the same camera, ZERO kg of air.
//      A sky that is in-scattered sunlight must vanish; a painted one will not notice.
//   2. NIGHT — the same air, the sun far below the horizon. Real in-scatter has nothing to scatter.
//   3. WITHIN one sunset frame — toward the sun against away from it. No exposure change, no ground
//      difference and no epoch difference can produce a colour gradient that reverses across azimuth.
//
// Run:  scripts/rigshot.sh terra_sky.mjs
import { launch, VIEWPORT } from './_launch.mjs';
import { decodePng } from './_png.mjs';

const out = process.env.OUT || '/tmp/rigshot';
const PORT = process.env.PORT || '5173';

// Galway, at head height, looking at the horizon. The sun is put over a chosen longitude offset, which
// is how "noon", "sunset" and "night" are expressed without touching anything but the clock.
const LAT = 53.1;
const LON = -9.45;

/** Mean RGB of a band of the frame, as fractions of the image height (0 = top). */
const bandRgb = (img, y0, y1, x0 = 0, x1 = 1) => {
  const c = img.channels;
  let r = 0, g = 0, b = 0, n = 0;
  for (let y = Math.floor(y0 * img.height); y < Math.floor(y1 * img.height); y++) {
    for (let x = Math.floor(x0 * img.width); x < Math.floor(x1 * img.width); x++) {
      const i = (y * img.width + x) * c;
      r += img.data[i]; g += img.data[i + 1]; b += img.data[i + 2]; n++;
    }
  }
  return n ? [r / n, g / n, b / n] : [0, 0, 0];
};
const f = (v) => v.map((x) => x.toFixed(1)).join('/');
const lum = (c) => (c[0] + c[1] + c[2]) / 3;
const ok = (name, cond, detail) => console.log(`${cond ? 'PASS' : 'FAIL'}  ${name} — ${detail}`);

/** One frame: pin the clock, aim the camera, shoot, and report the sky band and the ground band. */
async function frame(p, name, { epoch, sunLon, alt, pitch, yaw = 0 }) {
  await p.evaluate(
    async ({ epoch, sunLon, alt, pitch, yaw, lat, lon }) => {
      const w = window.__terra;
      w.set_alt_bounds(0.05, 8e10);
      w.set_epoch(epoch);
      // Put the sun over a longitude WITHOUT losing the date — the same ordering trap the seasons rig
      // documents: `set_epoch_sun_over_lon` solves near the epoch already pinned.
      w.set_epoch_sun_over_lon(sunLon);
      w.place_camera(lat, lon, alt, yaw, pitch);
      await new Promise((r) => setTimeout(r, 1500));
    },
    { epoch, sunLon, alt, pitch, yaw, lat: LAT, lon: LON },
  );
  const path = `${out}/sky-${name}.png`;
  await p.screenshot({ path });
  const img = decodePng(await p.screenshot());
  // Upper fifth is sky whenever the camera looks at the horizon; lower fifth is ground.
  const sky = bandRgb(img, 0.02, 0.2);
  const ground = bandRgb(img, 0.8, 0.98);
  console.log(
    `${name.padEnd(22)} sky ${f(sky).padStart(18)}   ground ${f(ground).padStart(18)}   ` +
      `B/R ${(sky[2] / Math.max(sky[0], 0.01)).toFixed(2)}`,
  );
  return { sky, ground, img };
}

const b = await launch();
const p = await b.newPage({ viewport: VIEWPORT });
p.on('pageerror', (e) => console.log('PAGEERR:', e.message));

const open = async (world) => {
  const q = world ? `?world=${world}` : '';
  await p.goto(`http://127.0.0.1:${PORT}/terra.html${q}`, { waitUntil: 'load' });
  await p.waitForFunction(() => !!window.__terra, null, { timeout: 60000 });
  await p.waitForTimeout(3500);
};

// A fixed instant, so nothing drifts between the run's frames.
const EPOCH = 1718945000; // 2024-06-21, the June solstice

console.log(`viewport ${VIEWPORT.width}x${VIEWPORT.height}`);
console.log('frame                          sky R/G/B            ground R/G/B');

// ★ WHERE SUNSET ACTUALLY IS. The first run of this rig used "+90° of longitude" for sunset and the
// frame came back barely redder than noon — because at 53°N in JUNE the sun is up for 16½ hours, so at
// 90° of hour angle it is still ~20° above the horizon. That is a measurement error, not an engine one.
// The sun reaches the horizon at the hour angle the day-length formula gives: cos H₀ = −tan φ · tan δ,
// the same relation `solar::day_length_hours` uses. Computed, not recalled.
const DECL = 23.44 * (Math.PI / 180); // June solstice declination
const rad = Math.PI / 180;
const H0 = (Math.acos(Math.max(-1, Math.min(1, -Math.tan(LAT * rad) * Math.tan(DECL)))) / rad);
console.log(`sunset at Galway on the June solstice: hour angle ${H0.toFixed(1)}° from noon`);

await open();
const noon = await frame(p, 'noon-horizon', { epoch: EPOCH, sunLon: LON, alt: 1.7, pitch: 0.05 });
const up = await frame(p, 'noon-zenith', { epoch: EPOCH, sunLon: LON, alt: 1.7, pitch: 1.2 });
// The sun ON the horizon, and then far past the shadow bound where the whole column is dark.
// Yaw 60° puts the sun (due east, at hour angle H0 before noon) about 30° to one side of centre, so
// ONE frame holds both a sunward sky and an anti-sunward one. That is the strongest control available:
// no exposure, epoch, camera or ground difference can produce a colour gradient across azimuth.
const sunset = await frame(p, 'sunset', {
  epoch: EPOCH, sunLon: LON + H0, alt: 1.7, pitch: 0.05, yaw: 60 * rad,
});
const night = await frame(p, 'night', { epoch: EPOCH, sunLon: LON + 175, alt: 1.7, pitch: 0.05 });
// ★ NOT a limb shot, and it is named for what it is. Terra's fly camera turns to look straight DOWN
// above ~60 km (`view_basis` blends `f_orbit = -up` in), so it cannot be aimed at the horizon from
// orbit at all — a true limb, where a ray misses the ground and still crosses lit air, is a space-band
// frame. `sphericity_is_small_overhead_and_decisive_at_the_limb` proves the integral does it; no rig
// here photographs it. Owed.
const orbit = await frame(p, 'orbit-down', { epoch: EPOCH, sunLon: LON, alt: 1.2e6, pitch: -0.35 });

// The within-frame sunset control: the sun is to one side, so compare the two halves of the sky band.
const sunward = bandRgb(sunset.img, 0.02, 0.2, 0.5, 1.0);
const away = bandRgb(sunset.img, 0.02, 0.2, 0.0, 0.5);
console.log(`  sunset sunward ${f(sunward)}  vs away ${f(away)}`);

// ── THE CONTROL ────────────────────────────────────────────────────────────────────────────────────
await open('earth-airless');
const airless = await frame(p, 'airless-noon-horizon', {
  epoch: EPOCH, sunLon: LON, alt: 1.7, pitch: 0.05,
});
const airlessUp = await frame(p, 'airless-noon-zenith', {
  epoch: EPOCH, sunLon: LON, alt: 1.7, pitch: 1.2,
});

// ★ AERIAL PERSPECTIVE — what the retired stand-in could never do, measured against the control so the
// comparison is not confounded. The first version of this check compared the ground NEAR the camera
// with the ground at the horizon in one frame and called the difference haze; it was not. The far band
// is simply brighter ground (different slopes, different shading), and it came out LESS blue, which is
// the confound talking. Same pixels, same sun, same terrain, air or no air is the only honest form.
//
// The band just below the horizon is ~4.6 km away at 1.7 m eye height (the horizon is sqrt(2Rh)), so
// the air in between is only ~0.03 optical depths in blue — a few percent. Small, real, and previously
// exactly ZERO: the stand-in scaled a whole-column veil by one altitude-derived number, 2e-4 here, the
// same for every pixel however far away it was.
{
  const withAir = bandRgb(noon.img, 0.55, 0.60);
  const without = bandRgb(airless.img, 0.55, 0.60);
  const blueness = (c) => c[2] / Math.max(c[0], 0.01);
  console.log(
    `  ground at the horizon: ${f(withAir)} through air vs ${f(without)} through none ` +
      `(B/R ${blueness(withAir).toFixed(3)} vs ${blueness(without).toFixed(3)})`,
  );
  ok(
    'distant ground is veiled by the air in front of it',
    blueness(withAir) > 1.01 * blueness(without),
    `B/R ${blueness(withAir).toFixed(3)} with air vs ${blueness(without).toFixed(3)} without, ` +
      `over ~4.6 km of it`,
  );
}

// ── VERDICTS. Each is a difference against a frame that must not show the effect. ───────────────────

ok(
  'the daylit sky is blue',
  noon.sky[2] > noon.sky[1] && noon.sky[1] > noon.sky[0] && lum(noon.sky) > 20,
  `B>G>R at ${f(noon.sky)}`,
);
ok(
  'and it is the AIR, not a backdrop',
  lum(airlessUp.sky) < 0.15 * lum(up.sky),
  `zenith ${f(up.sky)} with air vs ${f(airlessUp.sky)} with none`,
);
ok(
  'the ground is still lit without air',
  lum(airless.ground) > 0.4 * lum(noon.ground),
  `${f(airless.ground)} vs ${f(noon.ground)} — the control removes the SKY, not the sun`,
);
ok(
  'night has no sky',
  lum(night.sky) < 0.1 * lum(noon.sky),
  `${f(night.sky)} vs ${f(noon.sky)}`,
);
// ★ THE ASSERTION THIS REPLACED WAS MINE, AND IT WAS PHYSICALLY NAIVE. It demanded that the sunset sky
// be redder TOWARD the sun than away from it within one frame, and the run came back 1.17 against 1.17.
// That is the model being right, not wrong: in SINGLE scatter the reddening is set by the SUN's path
// length, which for a sun on the horizon is the same however you turn your head — only the brightness
// varies across azimuth, through the phase function. The azimuthal hue swing of a real sunset comes
// from multiple scattering and aerosols, both of which `atmos.wgsl` flags as absent. So the honest
// control here is the CLOCK: same place, same camera, same ground, different hour.
ok(
  'sunset reddens — the same sky, hours later',
  noon.sky[2] / Math.max(noon.sky[0], 0.01) > 3.0 * (sunset.sky[2] / Math.max(sunset.sky[0], 0.01)),
  `B/R ${(noon.sky[2] / Math.max(noon.sky[0], 0.01)).toFixed(2)} at noon vs ` +
    `${(sunset.sky[2] / Math.max(sunset.sky[0], 0.01)).toFixed(2)} at sunset`,
);
ok(
  'and it is brighter toward the sun (the phase function, and only that)',
  Math.abs(lum(sunward) - lum(away)) / Math.max(lum(away), 0.01) > 0.03,
  `${(lum(sunward)).toFixed(1)} vs ${(lum(away)).toFixed(1)} across azimuth; ` +
    `hue R/B ${(sunward[0] / Math.max(sunward[2], 0.01)).toFixed(2)} vs ` +
    `${(away[0] / Math.max(away[2], 0.01)).toFixed(2)} — flat, as single scatter requires`,
);

// ★ WHAT IS STILL WRONG, MEASURED rather than asserted: stars in daylight. They now SUM with the sky
// instead of punching through it, which is why they are faint here and were not before — but a few
// bright ones still show, because the star pass carries exposure 80 while every other view of this
// world carries SUN_GAIN = 22. Two exposures in one frame is one scene with two answers, and the sum
// makes the mismatch visible for the first time. The derived fix is named in docs/46.
{
  const img = up.img, c = img.channels;
  let peak = 0, sum = 0, n = 0;
  for (let y = Math.floor(0.05 * img.height); y < Math.floor(0.25 * img.height); y++) {
    for (let x = 0; x < img.width; x++) {
      const i = (y * img.width + x) * c;
      const v = (img.data[i] + img.data[i + 1] + img.data[i + 2]) / 3;
      peak = Math.max(peak, v); sum += v; n++;
    }
  }
  const mean = sum / n;
  console.log(
    `      daylight zenith: brightest pixel ${peak.toFixed(1)} against a sky mean of ${mean.toFixed(1)} ` +
      `(+${(100 * (peak / mean - 1)).toFixed(1)}%) — a real daytime star is ~0.01% of the sky`,
  );
}
ok(
  'the air is still there from 1,200 km',
  lum(orbit.sky) > 1.5 * lum(night.sky),
  `${f(orbit.sky)} against a night sky of ${f(night.sky)}`,
);

await b.close();
