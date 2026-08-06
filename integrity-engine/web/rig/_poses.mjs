// **Camera poses worth returning to.** (Robin, 2026-08-05: *"Maybe create rigs that place your camera
// where they need to be; what is useful/interesting for you as camera poses will likely be useful to
// others too."*)
//
// Every rig here used to re-derive its own framing, and the derivations were where the errors lived: a
// "sunset" that had the sun 20° above the horizon (at 53°N in June the sun is up 16½ hours, so 90° of
// hour angle is nowhere near setting), and an "orbit limb" from an altitude where the planet fills the
// frame and there is no limb in it. Both were MY arithmetic, both cost a rig run, and both are fixed
// once here instead of once per rig.
//
// A pose is data: `{ lat, lon, alt_m, yaw, pitch, sunLon }`, ready for `place_camera` plus
// `set_epoch_sun_over_lon`. `sunLon` is where the SUN goes, which is how time of day is expressed
// without touching anything else in the frame.

const rad = Math.PI / 180;

/** Earth's mean radius (m) — the same number the engine derives its geometry from. */
export const EARTH_R = 6_371_000;

/**
 * **The hour angle at which the sun sits ON the horizon**, in degrees from local noon, for a latitude
 * and a solar declination: `cos H₀ = −tan φ · tan δ`. This is the day-length relation
 * (`solar::day_length_hours` in the engine) and it is the only honest way to ask for "sunset" — the
 * answer swings from 90° at the equinox to *no solution at all* inside a polar summer, and guessing
 * "90°" gives a frame with the sun a quarter of the sky up.
 *
 * Returns `null` where the sun does not set that day, so a caller has to face the midnight sun rather
 * than silently framing something else.
 */
export function sunsetHourAngle(latDeg, declDeg = 23.44) {
  const c = -Math.tan(latDeg * rad) * Math.tan(declDeg * rad);
  if (c < -1 || c > 1) return null; // midnight sun, or polar night
  return Math.acos(c) / rad;
}

/**
 * **The altitude from which the whole planet fits in frame**, with `margin` of the half-field to spare.
 * The disc subtends `asin(R/(R+h))`, so it fits when that is below the camera's half-FOV — which is why
 * a "blue marble" shot from 1,200 km is not one: the disc is 68° across and the frame is 51°. Terra's
 * default vertical FOV is 0.9 rad (`fly_camera::DEFAULT_FOV_Y`).
 */
export function wholeDiscAltitude(fovY = 0.9, margin = 0.72, radius = EARTH_R) {
  const half = Math.max(0.05, Math.min(1.5, (fovY / 2) * margin));
  return radius / Math.sin(half) - radius;
}

/** Distance to the horizon from altitude `h` on a sphere of radius `R` — `sqrt(h(h+2R))`. */
export function horizonDistance(h, radius = EARTH_R) {
  return Math.sqrt(h * (h + 2 * radius));
}

// ── Named poses ─────────────────────────────────────────────────────────────────────────────────────
// Places chosen because something about the engine is legible from them, not because they look nice.

/** Standing in Irish pasture at eye height, looking at the horizon. The ground-truth sky frame. */
export const GALWAY_GROUND = { lat: 53.1, lon: -9.45, alt_m: 1.7, yaw: 0, pitch: 0.05 };

/** The same spot with the sun ON the horizon and off to one side, so one frame holds both azimuths. */
export const GALWAY_SUNSET = {
  ...GALWAY_GROUND,
  yaw: 60 * rad,
  sunLon: -9.45 + (sunsetHourAngle(53.1) ?? 90),
};

/** The same spot, sun far below the shadow bound: the sky must be honestly black. */
export const GALWAY_NIGHT = { ...GALWAY_GROUND, sunLon: -9.45 + 175 };

/** Straight up from the same spot — the shortest air path, where the sky is bluest. */
export const GALWAY_ZENITH = { ...GALWAY_GROUND, pitch: 1.2 };

/**
 * **THE BLUE MARBLE**: high enough that the whole disc sits inside the frame with space around it, so
 * the limb has somewhere to glow. This is the pose an "atmosphere from orbit" claim needs; anything
 * lower photographs ground with a gradient across it and calls it a limb.
 */
export const BLUE_MARBLE = {
  lat: 10,
  lon: 0,
  alt_m: wholeDiscAltitude(),
  yaw: 0,
  pitch: -1.2,
};

/** The blue marble with the sun 90° away, so the terminator runs through the middle of the disc. */
export const TERMINATOR_FROM_SPACE = { ...BLUE_MARBLE, sunLon: 90 };

/** Low orbit over the same place — the disc overfills the frame; good for surface detail, not for limbs. */
export const LOW_ORBIT = { lat: 10, lon: 0, alt_m: 400_000, yaw: 0, pitch: -1.2 };

/**
 * Drive a Terra page to a pose. Pins the clock first and the sun second — `set_epoch_sun_over_lon`
 * solves for an instant near the epoch already pinned, so calling it BEFORE `set_epoch` silently throws
 * the date away (a trap that once made four seasonal frames identical).
 */
export async function pose(page, p, { epoch, settleMs = 2000 } = {}) {
  await page.evaluate(
    async (a) => {
      const w = window.__terra;
      w.set_alt_bounds(0.05, 8e10);
      if (a.epoch != null) w.set_epoch(a.epoch);
      if (a.sunLon != null) w.set_epoch_sun_over_lon(a.sunLon);
      w.place_camera(a.lat, a.lon, a.alt_m, a.yaw ?? 0, a.pitch ?? 0);
      await new Promise((r) => setTimeout(r, a.settleMs));
    },
    { ...p, epoch, settleMs },
  );
}

/**
 * **Where the assembly's edge falls in the frame**, as a fraction of half the frame WIDTH, looking
 * straight down from `alt_m`. The body's angular radius is `asin(extent/(R+h))` and a perspective
 * projection puts it at `tan(theta) / (aspect * tan(fov_y/2))`.
 *
 * ★ `extent` is the ASSEMBLY's boundary — rock plus air — not the solid radius, because the atmosphere
 * is a component of the body (docs/66 §10, `atmosphere::AirColumn::extent`). Earth's air adds ~97 km,
 * which is where the limb glow lives; a rig that predicts the rock's edge is 97 km short of the thing
 * it is trying to photograph. `airTop` is the one number here the engine should be asked for rather
 * than restated — it is 11.5 scale heights, and it is duplicated until a scene verb reports it
 * (docs/46 row 44).
 */
export function discHalfWidth(alt_m, aspect, { fovY = 0.9, radius = EARTH_R, airTop = 96_700 } = {}) {
  const theta = Math.asin(Math.min(1, (radius + airTop) / (radius + alt_m)));
  return Math.tan(theta) / (aspect * Math.tan(fovY / 2));
}
