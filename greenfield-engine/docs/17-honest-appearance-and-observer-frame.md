# Honest appearance, the observer frame, and scale-adaptive importance

> Design note. Appearance must be as honest as the physics: **emergent from real matter and real
> light, summarized (never faked) as we zoom out, viewed from an explicit physical frame of
> reference.** This is the rendering counterpart to the representation invariant (`docs/15`) and the
> scale-relative north star (`docs/13`). Status: **in progress** — the operators and honest colours
> have landed; the ray-traced, real-Sun-lit, focus-switching view is staged for on-device work.
>
> **"Integrity"** is the working principle (and a candidate name): every value on screen must trace
> to something real, or be openly flagged as a placeholder. No fudge, all the way down.

## 1. Brightness is illumination × reflectance

The Moon looks bright not because it is a bright material — basalt's albedo is ~0.05, genuinely dark —
but because a very bright Sun reflects off it. So a body's colour is its **reflectance** (albedo, and
later a full BRDF), and its **brightness** comes from the light. Baking brightness into the material
(a "bright grey Moon") is a fudge that collapses the moment the lighting changes.

- `Material::albedo` is diffuse reflectance, and is itself flagged as a **summary placeholder** for the
  real spectral, microstructure-dependent optics we don't yet derive from first principles.
- The space shader now computes `reflectance × (ambient + n·l × SUN_GAIN)` and Reinhard tone-maps the
  result, so honest low-albedo bodies read correctly bright. `SUN_GAIN` is a single uniform
  lighting/exposure scalar (a camera property), identical for every body — it moves brightness, never
  hue or relative reflectance.
- **Goal: ray tracing.** Rasterized direct lighting is the stopgap. Honest light transport — the Sun's
  rays actually reflecting, shadowing, and scattering off real materials — is the target, because it is
  the appearance analogue of "consequences are real, not scripted."

## 2. Summaries are computed, never hand-picked

Zooming out *must* summarize — you cannot render every grain of a planet from orbit. The rule that
keeps a summary honest: it is **reduced from everything we know about the object's constituents**, by
one operator that works at every scale, for every object.

- `materials::aggregate_albedo(composition, materials)` — the fraction-weighted mean albedo of a
  **composition** (what the object is made of). The same reduction summarizes a shovel of mixed dirt or
  a planet's surface. Density and other properties reduce the same way.
- The space band uses it honestly: Earth = ~71% ocean water + ~24% continental (granitic) rock + ~5%
  polar ice; Moon = maria basalt. These are stated real fractions, not a paint job.
- **Flagged gaps (honesty, not hidden):** the Earth composition excludes the **atmosphere**, so there
  is deliberately no Rayleigh "blue-marble" blue — that blue is atmospheric scattering we don't model,
  and faking it would be a fudge. The Moon lacks highland **anorthosite** in the material DB, so it
  renders darker than the real Moon until that material is added.

## 3. A real Sun lights and holds the system

The illuminant should be a **real body**, not a hardcoded light direction: a Sun at the true mass,
distance, and size. Its gravity holds the Earth; its light lights everything.

- **Physics: verified.** `orbit::sun_earth_moon_system_is_bound` adds the Sun (1.989e30 kg) at 1 AU and
  gives the Earth its **appropriate heliocentric velocity** (~29.78 km/s); the Moon, carrying the
  Earth's velocity plus its own, stays bound to the moving Earth while the Earth orbits the Sun — the
  correct nesting, emergent from the one gravity law, not a placed tableau.
- **Staged:** wiring that Sun into the *live* view as the illuminant (light direction from the Sun's
  real position, intensity ∝ L/r²), and a stellar material (photosphere/plasma) with honest properties.
  The current shader still uses a placeholder light direction, flagged in-code.

## 4. The viewport is a physical frame of reference

The camera is an **observer with a frame of reference**, not a floating eye. What is rendered is stated
relative to a chosen frame.

- **Focus is selectable.** Start focused on the planet; switch focus to the Moon (or any body we add).
  The focused body defines the frame's origin (and, later, its rest velocity), so the system stays
  framed while everything else moves honestly around it.
- This is `docs/13` observer-relative fidelity made concrete: the view is a physical vantage, and
  fidelity is spent on what that vantage can perceive.

## 5. Scale-adaptive importance — the real research question

> Can the system understand what matters at a given scale?

At the Earth–Moon zoom, the Sun's *relevant* contribution is its **light and a slow gravitational
tug**, not a rendered disk (it is 390× farther than the Moon and would be off-frame). Zoom out to the
solar system and the Sun becomes a rendered body while the Earth collapses to a dot. The engine must
decide, per scale, which properties of an object are worth simulating and drawing — mass here, surface
composition there, individual grains only at the contact point (`docs/08`). Getting this decision right
— spending finite compute on what the observer can actually perceive — is the core of the whole
scale-relative programme.

## Status / next

- **Landed (verified natively):** `aggregate_albedo` operator + tests; honest composition-derived body
  colours (no painted tints); illumination × reflectance + tone-map shader; Sun–Earth–Moon bound-system
  physics test.
- **Needs your on-device eyes:** the new lit appearance of the space band (headless WebGPU can't render
  here).
- **Staged (larger, honest work):** real Sun wired as the live illuminant + heliocentric re-centering +
  focus switching; ray tracing; specular/BRDF from `roughness`/`metallic`; stellar & anorthosite
  materials; atmosphere (Rayleigh) for the earned blue; and the requested **orbital-decay** control
  (halving the Moon's velocity yields a bound *eccentric* ellipse in a conservative sim — real decay
  needs a dissipative mechanism, e.g. an atmosphere or tides, which we'd model, not fake).
