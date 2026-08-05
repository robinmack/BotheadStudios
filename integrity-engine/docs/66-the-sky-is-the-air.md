# 66 — The sky is the air

> **Robin, 2026-08-04:** *"Sky must be a component of Earth assembly, no"* — and, on what should draw it:
> *"making ray-tracing work in the engine (where models integrate with the universe and become
> viewable)… It doesn't have to be very sophisticated ray tracing… Sun is close to a point source."*

Status: **built and rig-verified 2026-08-05.** Supersedes the sky half of `docs/46` row 41.

---

## 1. What was wrong

`shaders/sky.wgsl` was compiled by **nothing**. Its header said *"for the terrain scene"*; that scene was
deleted in July (docs/50), its successor the Ground scene on 2026-08-03 (docs/46 row 37), and Terra never
had a sky of its own. So a daylight frame near the ground was vivid lit grass under a **black starfield**,
which is what Robin was describing when she wrote *"the engine seems to be rendering the color without
taking available light into account… Grass shouldn't be apparently brighter than everything else at
night, right?"* The ground was right. The sky was absent.

Every test about the atmosphere passed the whole time, because they all asked whether the OPTICS were
correct and none asked whether anything ran them. That is the docs/48 pattern — *the law is built and
proven, then wired into one place or none* — and it now has a gate:
`laws::compiled_shader_tests::every_shader_is_compiled_by_something`. On its first run it caught two
more orphans (`rayleigh.wgsl`, freshly orphaned by this change, and `particles.wgsl`, superseded in
**July**). Both deleted.

## 2. The one law

The closed form the engine already had — `atmosphere::rayleigh_veil` — is the analytic solution of **one
geometry**: a slab of air seen from OUTSIDE it, looking down. Along such a ray the sun path and the view
path shorten together, and that pairing is the only reason the integral collapses to
`1 − e^{−τ(1/μᵥ+1/μₛ)}`.

Stand on the ground and look UP and the pairing **inverts** — the view path grows with height while the
sun path shrinks. The closed form is not approximate there; it is the wrong integral. (The retired
`sky.wgsl` used it anyway, with `μᵥ = ray.y`, which is also why it could only ever be a sky for a flat
world.)

So the law is the integral, marched:

```
L = F · (P(Θ)/4) · ∫ β(h) · e^{−τ_sun(s)} · e^{−τ_view(s)} ds
      P(Θ) = ¾(1 + cos²Θ)          the Rayleigh phase
      β(h) = (τ/H) · e^{−h/H}      volume scattering, from the DECLARED air
```

`atmosphere::air_inscatter` in Rust, mirrored line-for-line by `shaders/atmos.wgsl`. Nothing in it is
chosen: `τ` comes from the emergent surface pressure (the weight of the body's declared air over its own
radius), `H` from that air's molar mass at that body's own gravity, and `β = τ/H` follows because
`∫₀^∞ β₀e^{−h/H} dh = β₀H = τ`. The `/4` rather than `/4π` is the engine's standing radiance convention
— the surface term is likewise `albedo·µ·F` and not `albedo·µ·F/π` — so the sky and the ground under it
share one exposure.

**`rayleigh_veil` is now its analytic special case, and is used as the reference the march is pinned to**
(`the_march_reproduces_the_closed_form_from_above`, agreement < 1% in the plane-parallel limit). It is no
longer in any render path; the Rust function survives as a test reference and as the shell-tint
approximation the space band's flat-lit bodies still spend.

### What falls out of it

Nothing below is drawn on. Each is the same integral with different geometry:

| what you see | why |
|---|---|
| blue overhead, pale at the horizon | path length, band by band; every band saturates at `1 − e^{−τ}` on a long path |
| **red at sunset** | the blue is removed from the SUNLIGHT before it can scatter |
| a soft terminator and **twilight** | the low air is in the planet's shadow while the air above it is not |
| aerial perspective over the ground | the same integral, stopped at the surface point instead of at space |
| a glowing limb from orbit | rays that miss the ground still cross lit air |

★★ **Twilight retires a declared number.** `twilight_half_angle` is an openly-flagged `sqrt(2H/R)` ramp,
applied to the closed form because a flat slab has no geometry that could produce twilight. The march
has that geometry, so the gradient emerges from the shadow test —
`twilight_emerges_from_the_planets_own_shadow` asserts only geometric facts (lit below the shadow bound,
monotone, exactly zero past it), never a remembered value. The ramp remains only where the closed form
does.

### Resolution, not a dial

`SKY_VIEW_STEPS = 32`, `SKY_SUN_STEPS = 8`, mirrored in the WGSL. These are **read off a convergence
measurement**, not chosen: `the_integral_converges_with_sample_count` walks the worst ray in the scene
(near-horizontal view, near-horizontal sun) against a 512×128 reference. At the shipped counts that ray
is within **1.6%**; halving them costs **6.6%**. The test also asserts the answer *improves* with more
samples, which is the property a fudge does not have.

Uniform steps were the first attempt and needed 3× as many: an exponential column varies by e-foldings
along a near-horizontal ray. `ray_sample` packs samples quadratically toward the ray's **closest approach
to the surface** — its densest point, wherever that falls — which is one rule covering a ray leaving the
ground, a ray grazing the limb from orbit, and a ray straight up.

## 3. The sky is a component of the body, not of the scene

`AirColumn::of_body(body, mats, temp)` is the only place a declared mass of air becomes optical depth and
scale height. Both scenes hold one, from the same body, so one planet cannot have two atmospheres
depending on who is looking at it. `render::SkyVeil` draws whatever it is handed and has no opinion: hand
it a body with no air and it draws nothing, with no branch. **The Moon's black sky needs no code.**

A scene gained no new verb for any of this (docs/65 ratchet unchanged, 57 debt entries).

## 4. f32 at planetary scale

Two places would have returned noise, and both are rearranged rather than tolerated:

- **Altitude on a ray.** `|eye + t·d| − R` subtracts two numbers that agree to seven digits at ground
  level. `ray_altitude` multiplies by the conjugate instead — `(r²−R²)/(r+R)` with `r0²−R² = h₀(2R+h₀)` —
  so every term is `O(h·R)` or `O(t·R)` and nothing cancels.
- **The eye's own altitude.** A camera 2 m up is 3·10⁻⁷ of Earth's radius. It is formed in f64 on the CPU
  and passed as a metre count (`Air::seen_from`), never reconstructed in the shader. For the same reason
  `ray_reaches` takes altitudes rather than radii: `target − r0` for the ground is exactly `−h₀`.

Lengths in the shader are **metres**, all of them, including the path to a ground fragment. The caller
converts once.

## 5. Stars go out at dawn, for the reason they really do

A first rig shot showed stars plainly visible through a blue daylight sky. The cause is not the air —
at zenith it passes ~90% of the blue — it is that **you cannot see a star in daylight because the sky is
brighter than it is**, and that is a statement about a SUM. This engine tone-maps each pass separately,
so the sum never happened: the sky could only dim the stars by its own transmittance.

The star pass now evaluates the sky along its own ray with the same `atmos.wgsl` chunk and outputs
`tonemap(L_sky + L_star·T)`, which *is* the sum. Per-band `T`, so a low star reddens as well as dims. A
scene with no air passes `τ = 0` and this is bit-identical to what it replaced.

★ **What is still wrong, and it is measured, not hidden:** the star pass carries exposure 80 while every
other view of this world carries `SUN_GAIN = 22`. Two exposures in one frame is one scene with two
answers, and the sum is what made it visible. At the daylight zenith the brightest pixel is **+128%**
above the sky mean where a real daytime star is ~0.01%. Recorded as `docs/46` row 43 with the derivation
the fix needs (a star's apparent brightness depends on the pixel's solid angle, which neither exposure
knows).

## 6. Also retired

- **`veil_column_fraction`** — the flagged stand-in that scaled a whole-column veil by `1 − e^{−h/H}`.
  At 1.7 m that number is `2·10⁻⁴`, identical for every pixel however distant, so the ground came back
  **bit-identical with and without an atmosphere**. Marching to the fragment is the computation it named.
- **The painted background.** Both scenes cleared to `0.01/0.01/0.03` — a declared dark blue with nothing
  emitting it, and, once the sky was derived, a floor under every night measurement (the first sky rig
  read 3.9/4.1/9.3 for a "black" sky; that was entirely the clear). Space is black.

## 7. Verified

`web/rig/terra_sky.mjs`, at 2560×1600 on the 5060 Ti. **Every claim is a difference against a frame that
must not show the effect** — the discipline docs/46 row 41 was written to enforce after a seasonal
measurement turned out to be the sun's own elevation.

The control is `worlds/earth-airless` (`terra.html?world=earth-airless`): the same Earth, the same
ground, the same camera, **zero kilograms of air** — a first-class configuration `world_def::Atmosphere`
already documented.

```
noon-horizon    sky  69.0/121.7/215.2   ground 120.5/177.2/83.7   B/R 3.12
sunset          sky   74.4/80.0/63.6    ground   3.8/5.4/4.6      B/R 0.85
night           sky      2.7/2.9/3.6                              (stars only)
airless noon    sky      2.7/2.9/3.6    ground 120.5/177.2/83.7   (stars only)
```

- the daylit sky is blue — B>G>R
- **and it is the AIR**: zenith 52/95/185 with air, 2.7/3.0/3.7 with none
- the ground is still lit without air — the control removes the SKY, not the sun
- night has no sky
- **sunset reddens** — B/R 3.12 at noon against 0.85 at sunset, nothing changed but the clock
- brighter toward the sun, and **flat in hue across azimuth** — which is single scatter being right, not
  wrong (see §8)
- distant ground is veiled by the air in front of it — B/R 0.649 with air, 0.643 without, over 4.6 km

## 8. Honesty flags — four stand-ins, none a dial

1. **Single scatter.** The sky's own light does not light the sky, so deep twilight is darker than life.
2. **No Mie/aerosol term** — no haze, no white horizon band.
3. **No ozone** — the Chappuis band is what keeps a real twilight blue.
4. **A point Sun** (Robin's constraint), so no penumbra at the terminator.

(1) and (2) together are why the sunset's hue is flat across azimuth: in single scatter the reddening is
set by the SUN's path length, which for a sun on the horizon is the same however you turn your head. An
assertion demanding an azimuthal hue gradient was written into the rig, failed, and was **the assertion
that was wrong** — corrected in place with the reasoning, because a test that fails for a physical reason
is evidence, not a bug.

## 10. ★★ What Robin corrected, and it is bigger than the sky

Mid-session, watching a rig locate the planet's edge by scanning pixel columns for a brightness fall:

> *"Are you sure? This should be done in the engine as a boundary between the assembly (containing the
> atmosphere as a component of Earth) and space, which should be a collection of assemblies of type
> 'star' at coordinates."* … *"And of course Luna should be its own assembly."*

The rig was wrong, and so is some of what it was measuring against. Three statements, all of them
architecture rather than rendering:

1. **AN ASSEMBLY ENDS AT THE OUTERMOST BOUNDARY OF ITS OUTERMOST COMPONENT.** My first wording of this
   was *"a body ends where its air ends"*, and Robin corrected it on the spot: *"'A body ends where its
   AIR ends' is not accurate, as bodies are assemblies. An assembly ends at the outermost boundary of
   the assembly."* She is right, and the difference matters — name the rule after the air and the
   engine learns a special case, when the identical question is asked by a tree's canopy, a ship's mast
   and a cannon's muzzle. Air is merely the outermost component Earth happens to have.
   `AirColumn::outer_reach()` is therefore that COMPONENT's contribution (~97 km for Earth, exactly the
   surface for a body declaring none, no branch), and `angular_reach_from` gives it as seen from an
   altitude — `an_assembly_reaches_past_its_core_by_its_outermost_component`.
   ★ **Nothing downstream should ever infer this from a picture.** That is what the rig was doing, and
   it found the limb where the rock was, not where the assembly was. ★ **Still owed:** the general
   `Assembly::outer_reach` — a max over components — does not exist; `assembly.rs` has no extent method
   at all, so today the rule is stated only where the air states it.

2. **Space is not a background — it is a collection of assemblies of type `star` at coordinates.**
   `sky::Star` already carries exactly that (a real position in parsecs, a real luminosity, a real
   colour from a real temperature), but it is drawn by a `StarField` *background pass*, and the thing
   between the stars was until today a painted dark blue. Half of that is fixed — space is black now,
   because what is between assemblies is nothing — and half is not: a star is not yet an assembly, so
   the engine cannot ask one anything, and "the sky" is still a render concept rather than "the
   assemblies that happen to be very far away". Recorded as **docs/46 row 44**.

3. **Luna is its own assembly.** Today the Moon is a `LayeredBody` the space band places, and it is
   correct in the one respect this change touches — it declares no atmosphere, so it gets no sky, no
   twilight and a knife-edge terminator out of exactly the same code path Earth uses. That is the
   model working. What it is not yet is an assembly with components the way `docs/64` means it.

The through-line is docs/65: **the scene names the actors, the assemblies know themselves, the engine
holds the universe.** A boundary between two assemblies is the engine's fact to state, and every time
something else derives it — a rig from pixels, a renderer from a hardcoded radius — that is the same
class of error, wearing different clothes.

## 9. Owed

- **A GPU pin.** `atmos.wgsl` is a hand-mirror of the Rust. It should be run on the real device and
  compared, the way `tools/sph-verify` does for the SPH kernel. Until then the mirror is checked by
  reading, which is exactly the confidence level this project has learned not to trust.
- **One exposure** (§5, docs/46 row 43).
- **A limb photograph.** Terra's camera turns to look straight down above ~60 km, so it cannot be aimed
  at the horizon from orbit; the space band can. The integral is tested
  (`sphericity_is_small_overhead_and_decisive_at_the_limb`); no rig has photographed it.
- **Tone-map once.** The deeper version of §5: every pass tone-maps its own output, so radiances that
  should sum cannot. An HDR intermediate target and one final tonemap fixes §5 properly and changes how
  every scene composites — its own change, with its own rig evidence.
